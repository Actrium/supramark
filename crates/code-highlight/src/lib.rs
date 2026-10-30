use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, FontStyle, Style, Theme};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;
use two_face::theme::LazyThemeSet;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);
static THEME_SET: LazyLock<LazyThemeSet> =
    LazyLock::new(|| LazyThemeSet::from(two_face::theme::extra()));

const DEFAULT_THEME: &str = "GitHub";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightManifest {
    pub runtime: bool,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub language_aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub themes: Vec<String>,
    #[serde(default)]
    pub default_themes: DefaultThemes,
    #[serde(default)]
    pub full_languages: bool,
    #[serde(default)]
    pub full_themes: bool,
    #[serde(default)]
    pub feature_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultThemes {
    pub light: Option<String>,
    pub dark: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct HighlightOptions<'a> {
    pub language: Option<&'a str>,
    pub theme: Option<&'a str>,
    pub manifest: Option<&'a HighlightManifest>,
    pub unknown_language: UnknownLanguage,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UnknownLanguage {
    #[default]
    Plain,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightedCode {
    pub language: Option<String>,
    pub theme: String,
    pub lines: Vec<HighlightedLine>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightedLine {
    pub tokens: Vec<HighlightedToken>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightedToken {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub font_style: Vec<String>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum HighlightError {
    UnknownLanguage(String),
    UnknownTheme(String),
    Highlight(String),
}

impl fmt::Display for HighlightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownLanguage(language) => {
                write!(f, "unknown code highlight language: {language}")
            }
            Self::UnknownTheme(theme) => write!(f, "unknown code highlight theme: {theme}"),
            Self::Highlight(message) => write!(f, "code highlight failed: {message}"),
        }
    }
}

impl std::error::Error for HighlightError {}

pub fn highlight(
    code: &str,
    options: HighlightOptions<'_>,
) -> Result<HighlightedCode, HighlightError> {
    let syntax = resolve_syntax(options.language, options.manifest, options.unknown_language)?;
    let theme_name = resolve_theme_name(options.theme, options.manifest);
    let theme = resolve_theme(theme_name, options.manifest)?;
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut lines = Vec::new();

    for line in LinesWithEndings::from(code) {
        let ranges = highlighter
            .highlight_line(line, &SYNTAX_SET)
            .map_err(|err| HighlightError::Highlight(err.to_string()))?;
        lines.push(HighlightedLine {
            tokens: style_ranges_to_tokens(ranges),
        });
    }

    if code.is_empty() {
        lines.push(HighlightedLine { tokens: Vec::new() });
    }

    Ok(HighlightedCode {
        language: if syntax.name == "Plain Text" {
            None
        } else {
            Some(syntax.name.clone())
        },
        theme: theme_name.to_owned(),
        lines,
    })
}

pub fn list_languages() -> Vec<String> {
    SYNTAX_SET
        .syntaxes()
        .iter()
        .map(|syntax| syntax.name.clone())
        .collect()
}

pub fn list_themes() -> Vec<String> {
    THEME_SET.theme_names().map(ToOwned::to_owned).collect()
}

fn resolve_syntax<'a>(
    language: Option<&str>,
    manifest: Option<&HighlightManifest>,
    unknown_language: UnknownLanguage,
) -> Result<&'a SyntaxReference, HighlightError> {
    let Some(language) = language.map(str::trim).filter(|lang| !lang.is_empty()) else {
        return Ok(SYNTAX_SET.find_syntax_plain_text());
    };
    let resolved = manifest
        .and_then(|manifest| manifest.alias(language))
        .unwrap_or(language);

    let syntax = SYNTAX_SET
        .find_syntax_by_name(resolved)
        .or_else(|| SYNTAX_SET.find_syntax_by_token(resolved))
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(resolved));

    let Some(syntax) = syntax else {
        return match unknown_language {
            UnknownLanguage::Plain => Ok(SYNTAX_SET.find_syntax_plain_text()),
            UnknownLanguage::Error => Err(HighlightError::UnknownLanguage(language.to_owned())),
        };
    };

    if manifest
        .map(|manifest| manifest.allows_language(&syntax.name))
        .unwrap_or(true)
    {
        Ok(syntax)
    } else {
        match unknown_language {
            UnknownLanguage::Plain => Ok(SYNTAX_SET.find_syntax_plain_text()),
            UnknownLanguage::Error => Err(HighlightError::UnknownLanguage(language.to_owned())),
        }
    }
}

fn resolve_theme_name<'a>(
    theme: Option<&'a str>,
    manifest: Option<&'a HighlightManifest>,
) -> &'a str {
    theme
        .filter(|theme| !theme.trim().is_empty())
        .or_else(|| manifest.and_then(|manifest| manifest.default_themes.light.as_deref()))
        .unwrap_or(DEFAULT_THEME)
}

fn resolve_theme<'a>(
    theme: &'a str,
    manifest: Option<&HighlightManifest>,
) -> Result<&'a Theme, HighlightError> {
    if manifest
        .map(|manifest| manifest.allows_theme(theme))
        .unwrap_or(true)
    {
        THEME_SET
            .get(theme)
            .or_else(|| THEME_SET.get(DEFAULT_THEME))
            .ok_or_else(|| HighlightError::UnknownTheme(theme.to_owned()))
    } else {
        Err(HighlightError::UnknownTheme(theme.to_owned()))
    }
}

fn style_ranges_to_tokens(ranges: Vec<(Style, &str)>) -> Vec<HighlightedToken> {
    let mut tokens: Vec<HighlightedToken> = ranges
        .into_iter()
        .map(|(style, text)| HighlightedToken {
            text: text.to_owned(),
            color: Some(color_to_hex(style.foreground)),
            background_color: transparent_to_none(style.background),
            font_style: font_style_to_vec(style.font_style),
        })
        .collect();
    strip_line_ending(&mut tokens);
    tokens
}

fn strip_line_ending(tokens: &mut Vec<HighlightedToken>) {
    while let Some(last) = tokens.last_mut() {
        if last.text.ends_with("\r\n") {
            let len = last.text.len() - 2;
            last.text.truncate(len);
            break;
        }
        if last.text.ends_with('\n') || last.text.ends_with('\r') {
            last.text.pop();
            break;
        }
        if last.text.is_empty() {
            tokens.pop();
            continue;
        }
        break;
    }
}

fn color_to_hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

fn transparent_to_none(color: Color) -> Option<String> {
    (color.a != 0).then(|| color_to_hex(color))
}

fn font_style_to_vec(style: FontStyle) -> Vec<String> {
    let mut out = Vec::new();
    if style.contains(FontStyle::BOLD) {
        out.push("bold".to_owned());
    }
    if style.contains(FontStyle::ITALIC) {
        out.push("italic".to_owned());
    }
    if style.contains(FontStyle::UNDERLINE) {
        out.push("underline".to_owned());
    }
    out
}

impl HighlightManifest {
    fn alias<'a>(&'a self, language: &'a str) -> Option<&'a str> {
        let normalized = language.to_ascii_lowercase();
        self.language_aliases
            .get(&normalized)
            .or_else(|| self.language_aliases.get(language))
            .map(String::as_str)
    }

    fn allows_language(&self, language: &str) -> bool {
        if self.full_languages || self.languages.iter().any(|lang| lang == "*") {
            return true;
        }

        if self.languages.is_empty() {
            return true;
        }

        let allowed: BTreeSet<&str> = self.languages.iter().map(String::as_str).collect();
        allowed.contains(language)
    }

    fn allows_theme(&self, theme: &str) -> bool {
        if self.full_themes || self.themes.iter().any(|item| item == "*") {
            return true;
        }

        if self.themes.is_empty() {
            return true;
        }

        self.themes.iter().any(|item| item == theme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_typescript_with_alias() {
        let mut manifest = HighlightManifest {
            languages: vec!["TypeScript".to_owned()],
            themes: vec!["GitHub".to_owned()],
            ..Default::default()
        };
        manifest
            .language_aliases
            .insert("ts".to_owned(), "TypeScript".to_owned());

        let result = highlight(
            "const answer: number = 42;\n",
            HighlightOptions {
                language: Some("ts"),
                theme: Some("GitHub"),
                manifest: Some(&manifest),
                unknown_language: UnknownLanguage::Error,
            },
        )
        .unwrap();

        assert_eq!(result.language.as_deref(), Some("TypeScript"));
        assert!(!result.lines[0].tokens.is_empty());
    }

    #[test]
    fn falls_back_to_plain_text_for_uncompiled_language() {
        let manifest = HighlightManifest {
            languages: vec!["JSON".to_owned()],
            ..Default::default()
        };

        let result = highlight(
            "const answer = 42;",
            HighlightOptions {
                language: Some("ts"),
                manifest: Some(&manifest),
                ..HighlightOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.language, None);
    }

    #[test]
    fn lists_two_face_assets() {
        assert!(list_languages()
            .iter()
            .any(|language| language == "TypeScript"));
        assert!(list_themes().iter().any(|theme| theme == "GitHub"));
    }
}
