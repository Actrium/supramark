//! `%%{init: {...}}%%` directive extraction.
//!
//! Port of the upstream extract / parse pipeline in `utils.ts`
//! (`detectInit` / `detectDirective` / `removeDirectives`), reduced
//! to the two shapes we actually see in `.mmd` files:
//!
//! ```text
//!   %%{init: {...JSON5...}}%%
//!   %%{initialize: {...JSON5...}}%%
//!   %%{wrap}%%            (shorthand, no body)
//! ```
//!
//! The directive body is JSON-ish — unquoted keys, single or double
//! quotes, trailing commas all show up in the wild. Upstream leans on
//! a `replace(/'/gm, '"')` + `JSON.parse` dance; we use the `json5`
//! crate, which covers the same tolerance surface without ad-hoc
//! rewrites.
//!
//! We can't use upstream's directive regex verbatim because the Rust
//! `regex` crate disallows look-around, and the upstream pattern uses
//! `(?!\}%{2})` to stop the body at `}%%`. So we hand-roll a scanner
//! that finds each `%%{ ... }%%` block directly — while still being
//! quote- and brace-depth aware so a directive body containing an
//! embedded `}%%` inside a string literal isn't severed mid-block.
//!
//! Portions adapted from mmdflux (<https://github.com/Actrium/mmdflux>,
//! MIT license). Specifically: the realisation that handling quoted
//! strings vs. brace-depth by hand — instead of via regex backrefs — is
//! the only way to stay sane when a directive body contains nested
//! objects with quoted colons. The scanner below follows the pattern
//! established in mmdflux's `theme_hint.rs::split_top_level_members`.

use crate::config::Config;
use serde_json::Value;

/// Parse every `%%{init:...}%%` / `%%{initialize:...}%%` /
/// `%%{wrap}%%` block in `source`, in the order they appear, and
/// return each one as a [`Config`] overlay.
///
/// Returns an empty `Vec` if the source contains no directives. The
/// caller folds these on top of the frontmatter-derived config (see
/// [`crate::preprocess::preprocess`]).
pub fn parse_directives(source: &str) -> Vec<Config> {
    let mut out = Vec::new();
    for span in find_directive_spans(source) {
        let inner = &source[span.body_start..span.body_end];
        let Some(parsed) = parse_inner(inner) else {
            continue;
        };
        match parsed {
            ParsedDirective::Init(cfg) => out.push(cfg),
            ParsedDirective::Wrap => out.push(Config {
                wrap: Some(true),
                ..Config::default()
            }),
            ParsedDirective::Other => {}
        }
    }
    out
}

/// Remove every directive block from `source`, mirroring upstream
/// `removeDirectives(text)`. Kept separate from
/// [`parse_directives`] so preprocessing can do removal in the same
/// pass that produces the config overlays (by calling both in order).
pub fn remove_directives(source: &str) -> String {
    let spans = find_directive_spans(source);
    if spans.is_empty() {
        return source.to_owned();
    }
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0;
    for span in &spans {
        out.push_str(&source[cursor..span.start]);
        cursor = span.end;
    }
    out.push_str(&source[cursor..]);
    out
}

#[derive(Debug, Clone, Copy)]
struct DirectiveSpan {
    /// Byte offset of the opening `%%`.
    start: usize,
    /// Byte offset *past* the closing `%%` (i.e. exclusive).
    end: usize,
    /// Byte offset of the first character after `%%{`.
    body_start: usize,
    /// Byte offset of the last character before `}%%`.
    body_end: usize,
}

/// Scan `source` top-to-bottom and locate every `%%{ ... }%%` block.
///
/// The scanner is brace- and quote-aware so an embedded `}%%` inside a
/// JSON string doesn't prematurely close the directive.
fn find_directive_spans(source: &str) -> Vec<DirectiveSpan> {
    let bytes = source.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        if !(bytes[i] == b'%' && bytes[i + 1] == b'%' && bytes[i + 2] == b'{') {
            i += 1;
            continue;
        }

        let start = i;
        let body_start = i + 3;
        // Walk forward looking for the matching `}%%`, tracking string
        // literal context and brace depth so we don't bail early.
        let mut j = body_start;
        let mut depth: i32 = 1; // we've already consumed the outer `{`
        let mut quote: Option<u8> = None;
        let mut escaped = false;
        while j < bytes.len() {
            let c = bytes[j];
            if let Some(q) = quote {
                if escaped {
                    escaped = false;
                } else if c == b'\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
                j += 1;
                continue;
            }
            match c {
                b'"' | b'\'' => quote = Some(c),
                b'{' | b'[' => depth += 1,
                b']' => depth -= 1,
                b'}' => {
                    depth -= 1;
                    // Candidate close: `}%%` at depth 0?
                    if depth == 0 && j + 3 <= bytes.len() && &bytes[j + 1..j + 3] == b"%%" {
                        spans.push(DirectiveSpan {
                            start,
                            end: j + 3,
                            body_start,
                            body_end: j,
                        });
                        j += 3;
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        if j >= bytes.len() {
            // Unterminated directive — bail out of the scan. The
            // `%%` prefix stays in the source, which mirrors
            // upstream's silent failure on malformed directives.
            break;
        }
        i = j;
    }
    spans
}

enum ParsedDirective {
    Init(Config),
    Wrap,
    Other,
}

/// Parse the `... ` between `%%{` and `}%%`. Upstream supports two
/// shapes:
///   - `init: {...}` / `initialize: {...}` — JSON-ish body
///   - `wrap` — bare keyword, no body
fn parse_inner(inner: &str) -> Option<ParsedDirective> {
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Split into `key` and optional `rest` on the first `:`.
    let (key, rest) = match trimmed.find(':') {
        Some(pos) => (trimmed[..pos].trim(), Some(trimmed[pos + 1..].trim())),
        None => (trimmed, None),
    };

    if rest.is_none() {
        // Body-less directive — the only one upstream supports is `wrap`.
        if key.eq_ignore_ascii_case("wrap") {
            return Some(ParsedDirective::Wrap);
        }
        return Some(ParsedDirective::Other);
    }

    if !(key.eq_ignore_ascii_case("init") || key.eq_ignore_ascii_case("initialize")) {
        return Some(ParsedDirective::Other);
    }

    let body = rest.unwrap();
    parse_body(body).map(ParsedDirective::Init)
}

/// Parse a raw directive body into a [`Config`]. The body must be a
/// JSON5-parseable object. Unknown top-level keys land in
/// [`Config::extras`] for free.
fn parse_body(body: &str) -> Option<Config> {
    let value: Value = json5::from_str(body).ok()?;
    serde_json::from_value(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_single_init_directive_with_theme() {
        let src = "%%{init: {'theme': 'forest'}}%%\nflowchart TD\nA-->B\n";
        let dirs = parse_directives(src);
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].theme.as_deref(), Some("forest"));
    }

    #[test]
    fn accepts_unquoted_keys_json5_flavour() {
        let src = r#"%%{init: {theme: "dark", flowchart: {curve: "linear"}}}%%
flowchart TD
A-->B
"#;
        let dirs = parse_directives(src);
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].theme.as_deref(), Some("dark"));
        let fc = dirs[0].flowchart.as_ref().unwrap();
        assert_eq!(fc.curve.as_deref(), Some("linear"));
    }

    #[test]
    fn accepts_initialize_alias() {
        let src = r#"%%{initialize: {theme: "neutral"}}%%
graph TD
"#;
        let dirs = parse_directives(src);
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].theme.as_deref(), Some("neutral"));
    }

    #[test]
    fn wrap_shorthand_directive_yields_wrap_true() {
        let src = "%%{wrap}%%\nsequenceDiagram\nA->>B: hi\n";
        let dirs = parse_directives(src);
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].wrap, Some(true));
    }

    #[test]
    fn multiple_init_directives_preserve_order() {
        let src = r#"%%{init: {theme: "forest"}}%%
%%{init: {theme: "dark"}}%%
flowchart TD
"#;
        let dirs = parse_directives(src);
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0].theme.as_deref(), Some("forest"));
        assert_eq!(dirs[1].theme.as_deref(), Some("dark"));
    }

    #[test]
    fn remove_directives_strips_every_block() {
        let src = "%%{init: {theme: 'dark'}}%%\nflowchart TD\nA-->B\n";
        let cleaned = remove_directives(src);
        assert!(!cleaned.contains("%%{"));
        assert!(cleaned.contains("flowchart TD"));
    }

    #[test]
    fn malformed_body_is_ignored_not_fatal() {
        // `not-valid-even-for-json5` — unquoted identifier with a dash,
        // illegal in both JSON and JSON5 object keys.
        let src = "%%{init: {not-valid-even-for-json5}}%%\nflowchart TD\nA-->B\n";
        let dirs = parse_directives(src);
        assert_eq!(dirs.len(), 0);
    }

    #[test]
    fn directive_body_with_nested_braces_scans_correctly() {
        // A JSON body that itself contains a `}` mid-string shouldn't
        // truncate the directive span — hand-rolled scanner must be
        // quote-aware.
        let src = r#"%%{init: {theme: "da}rk"}}%%
flowchart TD
"#;
        let dirs = parse_directives(src);
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].theme.as_deref(), Some("da}rk"));
    }
}
