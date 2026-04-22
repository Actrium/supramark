//! Source preprocessing — port of upstream `preprocess.ts`.
//!
//! Responsibilities, in order:
//!   1. Normalise `\r\n` / `\r` to `\n` (parser problems on CRLF).
//!   2. Strip YAML frontmatter `---\n...\n---` and lift its metadata.
//!   3. Collect every `%%{init:...}%%` / `%%{wrap}%%` directive, emit
//!      the corresponding [`Config`] overlays, then drop the directive
//!      blocks from the source.
//!   4. Strip whole-line `%%` comments (but **not** the `%%{` dirs —
//!      those are already gone by step 3).
//!
//! The resulting [`PreprocessOutput`] carries:
//!   - `cleaned_source`: text the diagram-type detector + parser see,
//!   - `config`: the fully merged config (default ← frontmatter ← init),
//!   - `meta`: title + (future) accessibility metadata from the
//!     frontmatter.

use crate::config::{directive, frontmatter, Config, GanttConfig};
use crate::error::Result;
use crate::model::DiagramMeta;
use regex::Regex;
use std::sync::OnceLock;

/// Output of the preprocess stage.
#[derive(Debug, Clone)]
pub struct PreprocessOutput {
    /// Cleaned source — CRLF normalised, comments / directives /
    /// frontmatter stripped. Safe to hand to [`crate::detect::detect`]
    /// and to a diagram parser.
    pub cleaned_source: String,
    /// Fully merged config (default ← site ← frontmatter ← init).
    pub config: Config,
    /// Metadata lifted out of the frontmatter: title etc.
    pub meta: DiagramMeta,
}

/// Whole-line `%%` comment regex — mirrors upstream's `cleanupComments`
/// regex (`/^\s*%%(?!\{)[^\n]+\n?/gm`). The negative lookahead skips
/// `%%{...}%%` directives, but those are already removed by step 3 in
/// [`preprocess`] — we keep the `(?!\{)` guard as a defensive measure.
fn whole_line_comment_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Rust's `regex` crate has no lookaround, so we enumerate the
    // forbidden first-character with a character class instead:
    //   ^\s*%%[^{\n][^\n]*\n?    — ordinary comment
    //   ^\s*%%\n?                — bare `%%` on a line
    RE.get_or_init(|| Regex::new(r"(?m)^\s*%%(?:[^\{\n][^\n]*)?\n?").unwrap())
}

/// Run the full preprocess pipeline on `source`. Returns
/// [`PreprocessOutput`] or a [`crate::error::MermaidError`] on an
/// unrecoverable failure (current pipeline can't actually fail — the
/// `Result` is there so future additions like size-limit checks slot
/// in without breaking callers).
pub fn preprocess(source: &str) -> Result<PreprocessOutput> {
    // 1. Normalise CRLF -> LF.
    let text = normalize_newlines(source);

    // 2. Frontmatter.
    let (fm, after_fm) = frontmatter::parse_frontmatter(&text);
    let mut after_fm = after_fm.to_owned();

    // Build the config stack. Upstream order:
    //   default ← site ← frontmatter ← %%{init}%%
    // Wave 0 has no site config yet, so we fold default ← fm ← init.
    let mut title: Option<String> = None;
    let mut fm_config: Config = Config::default();
    if let Some(fm) = fm {
        title = fm.title;

        if let Some(cfg) = fm.config {
            fm_config = cfg;
        }
        // Upstream hoists `displayMode` into `config.gantt.displayMode`
        // for legacy gantt support. Mirror that here so the downstream
        // config always sees it in the same slot.
        if let Some(dm) = fm.display_mode {
            let gantt = fm_config.gantt.get_or_insert_with(GanttConfig::default);
            gantt.display_mode = Some(dm);
        }
    }

    // 3. Extract directives from the post-frontmatter source, then
    //    strip them out.
    let directive_configs = directive::parse_directives(&after_fm);
    after_fm = directive::remove_directives(&after_fm);

    // 4. Whole-line `%%` comment strip.
    let cleaned = whole_line_comment_regex().replace_all(&after_fm, "");
    let cleaned = cleaned.trim_start_matches(['\n', '\r']).to_owned();

    // Fold the config stack.
    let mut layers: Vec<Config> = Vec::with_capacity(1 + directive_configs.len());
    if fm_config != Config::default() {
        layers.push(fm_config);
    }
    layers.extend(directive_configs);
    let config = Config::fold(Config::builtin_defaults(), layers);

    let meta = DiagramMeta {
        title: title.or_else(|| config.title.clone()),
        ..DiagramMeta::default()
    };

    Ok(PreprocessOutput {
        cleaned_source: cleaned,
        config,
        meta,
    })
}

/// Normalise `\r\n` and lone `\r` to `\n` — mirrors upstream's
/// `code.replace(/\r\n?/g, '\n')`.
fn normalize_newlines(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_crlf_to_lf() {
        let out = preprocess("flowchart TD\r\nA-->B\r\n").unwrap();
        assert_eq!(out.cleaned_source, "flowchart TD\nA-->B\n");
    }

    #[test]
    fn normalises_lone_cr_to_lf() {
        let out = preprocess("flowchart TD\rA-->B\r").unwrap();
        assert_eq!(out.cleaned_source, "flowchart TD\nA-->B\n");
    }

    #[test]
    fn strips_whole_line_percent_comments() {
        let out = preprocess("%% a comment\nflowchart TD\n%% another\nA-->B\n").unwrap();
        assert!(!out.cleaned_source.contains("%%"));
        assert!(out.cleaned_source.contains("flowchart TD"));
        assert!(out.cleaned_source.contains("A-->B"));
    }

    #[test]
    fn frontmatter_title_populates_meta() {
        let out = preprocess("---\ntitle: Hello\n---\nflowchart TD\nA-->B\n").unwrap();
        assert_eq!(out.meta.title.as_deref(), Some("Hello"));
        assert_eq!(out.cleaned_source, "flowchart TD\nA-->B\n");
    }

    #[test]
    fn frontmatter_config_feeds_into_merged_config() {
        let src = "---\nconfig:\n  theme: dark\n---\nflowchart TD\n";
        let out = preprocess(src).unwrap();
        assert_eq!(out.config.theme.as_deref(), Some("dark"));
    }

    #[test]
    fn init_directive_overrides_frontmatter() {
        // Merge order: default ← frontmatter ← init. Init must win.
        let src = "---\nconfig:\n  theme: forest\n---\n%%{init: {theme: \"dark\"}}%%\nflowchart TD\n";
        let out = preprocess(src).unwrap();
        assert_eq!(out.config.theme.as_deref(), Some("dark"));
        assert!(!out.cleaned_source.contains("%%{"));
        assert!(!out.cleaned_source.contains("---"));
    }

    #[test]
    fn display_mode_hoists_into_gantt_block() {
        let src = "---\ndisplayMode: compact\n---\ngantt\n";
        let out = preprocess(src).unwrap();
        let gantt = out.config.gantt.expect("gantt config populated");
        assert_eq!(gantt.display_mode.as_deref(), Some("compact"));
    }

    #[test]
    fn defaults_survive_when_no_overrides_present() {
        let out = preprocess("flowchart TD\nA-->B\n").unwrap();
        assert_eq!(out.config.theme.as_deref(), Some("default"));
        assert_eq!(out.config.security_level.as_deref(), Some("strict"));
    }

    #[test]
    fn directive_without_init_body_does_not_break_pipeline() {
        // `%%{wrap}%%` is a valid shorthand directive.
        let out = preprocess("%%{wrap}%%\nsequenceDiagram\nA->>B: hi\n").unwrap();
        assert_eq!(out.config.wrap, Some(true));
        assert!(out.cleaned_source.starts_with("sequenceDiagram"));
    }
}
