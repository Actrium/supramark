//! YAML-frontmatter extraction.
//!
//! Port of upstream `diagram-api/frontmatter.ts` (60 LoC). Mermaid
//! uses `js-yaml` with the **JSON schema**, not the full YAML 1.2
//! schema — that limits the surface to scalars / sequences / mappings
//! with JSON-style typing. `serde_yml` happens to be loose enough by
//! default that parsing via `serde_json::Value` gives us the same shape
//! mermaid ends up with after `yaml.load(..., { schema: JSON_SCHEMA })`.
//!
//! Only three keys are surfaced structurally: `title`, `displayMode`,
//! `config`. Everything else the parser would accept is dropped —
//! matching upstream's behaviour of only lifting `parsed.title` /
//! `parsed.displayMode` / `parsed.config` into the metadata struct.
//!
//! Portions adapted from mmdflux (<https://github.com/kookyleo/mmdflux>,
//! MIT license). Specifically: the intuition around hand-scanning
//! `---\n...\n---` blocks when the wrapping regex misbehaves on CRLF /
//! indented-`---` edge cases — though this file uses a regex that
//! mirrors upstream exactly rather than mmdflux's hand-rolled scanner.

use crate::config::Config;
use regex::Regex;
use std::sync::OnceLock;

/// Mermaid frontmatter payload — the subset of YAML keys upstream
/// explicitly surfaces in [`FrontMatterMetadata`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frontmatter {
    pub title: Option<String>,
    /// `displayMode` is legacy gantt-only. Upstream stuffs it into
    /// `config.gantt.displayMode` during preprocessing — we keep the
    /// raw string here and let [`crate::preprocess`] do the hoisting.
    pub display_mode: Option<String>,
    /// The `config:` block — parsed as a [`Config`] overlay.
    pub config: Option<Config>,
}

/// Compiled copy of upstream `frontMatterRegex`
/// (`diagram-api/regexes.ts`).
///
/// ```text
/// /^-{3}\s*[\n\r](.*?)[\n\r]-{3}\s*[\n\r]+/s
/// ```
///
/// Notes:
/// - `^` anchored to start-of-input.
/// - `s` flag: `.` matches newlines (we enable via `(?s)`).
/// - No `m` flag upstream, so `^` is not multiline — only the very
///   first characters count.
fn frontmatter_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)^-{3}\s*[\n\r](.*?)[\n\r]-{3}\s*[\n\r]+").unwrap())
}

/// Extract + parse frontmatter, returning the leftover source.
///
/// Mirrors upstream `extractFrontMatter(text)` — on a miss we hand the
/// original source back unchanged; on a hit we strip exactly the
/// matched bytes (including the trailing newline-run).
pub fn parse_frontmatter(source: &str) -> (Option<Frontmatter>, &str) {
    let Some(m) = frontmatter_regex().captures(source) else {
        return (None, source);
    };
    let whole = m.get(0).unwrap();
    let body = m.get(1).unwrap().as_str();

    let rest = &source[whole.end()..];

    // Parse the YAML body. On parse failure we still strip the block
    // (upstream would throw, but we're trying to be more tolerant) and
    // return an empty Frontmatter.
    let fm = match serde_yml::from_str::<serde_yml::Value>(body) {
        Ok(v) => lift_metadata(&v),
        Err(_) => Frontmatter::default(),
    };

    (Some(fm), rest)
}

/// Upstream only surfaces `title`, `displayMode` and `config`. Anything
/// else from the YAML body is intentionally dropped so that a rogue
/// frontmatter can't inject arbitrary config values.
fn lift_metadata(value: &serde_yml::Value) -> Frontmatter {
    let serde_yml::Value::Mapping(map) = value else {
        return Frontmatter::default();
    };
    let mut out = Frontmatter::default();

    if let Some(title) = map.get(serde_yml::Value::String("title".into())) {
        out.title = yaml_to_string(title);
    }
    if let Some(dm) = map.get(serde_yml::Value::String("displayMode".into())) {
        out.display_mode = yaml_to_string(dm);
    }
    if let Some(cfg) = map.get(serde_yml::Value::String("config".into())) {
        // Re-encode as JSON and let serde_json parse a `Config`. This
        // path tolerates both JSON-schema YAML (which is all upstream
        // supports) and the JSON the JS side would have produced —
        // anything richer falls into `extras`.
        if let Ok(json) = serde_json::to_value(cfg) {
            if let Ok(parsed) = serde_json::from_value::<Config>(json) {
                out.config = Some(parsed);
            }
        }
    }
    out
}

fn yaml_to_string(v: &serde_yml::Value) -> Option<String> {
    match v {
        serde_yml::Value::String(s) => Some(s.clone()),
        serde_yml::Value::Number(n) => Some(n.to_string()),
        serde_yml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_frontmatter_returns_source_verbatim() {
        let src = "flowchart TD\nA-->B\n";
        let (fm, rest) = parse_frontmatter(src);
        assert!(fm.is_none());
        assert_eq!(rest, src);
    }

    #[test]
    fn extracts_title_and_strips_block() {
        let src = "---\ntitle: hello\n---\nflowchart TD\nA-->B\n";
        let (fm, rest) = parse_frontmatter(src);
        let fm = fm.unwrap();
        assert_eq!(fm.title.as_deref(), Some("hello"));
        assert_eq!(rest, "flowchart TD\nA-->B\n");
    }

    #[test]
    fn extracts_nested_config_block() {
        let src = "---\nconfig:\n  theme: dark\n  flowchart:\n    curve: linear\n---\nflowchart TD\nA-->B\n";
        let (fm, rest) = parse_frontmatter(src);
        let fm = fm.unwrap();
        let cfg = fm.config.expect("config present");
        assert_eq!(cfg.theme.as_deref(), Some("dark"));
        let fc = cfg.flowchart.as_ref().unwrap();
        assert_eq!(fc.curve.as_deref(), Some("linear"));
        assert!(rest.starts_with("flowchart TD"));
    }

    #[test]
    fn extracts_display_mode_for_gantt_legacy_path() {
        let src = "---\ndisplayMode: compact\n---\ngantt\n";
        let (fm, rest) = parse_frontmatter(src);
        let fm = fm.unwrap();
        assert_eq!(fm.display_mode.as_deref(), Some("compact"));
        assert_eq!(rest, "gantt\n");
    }

    #[test]
    fn tolerates_malformed_yaml_by_dropping_metadata() {
        // `:` with no space and a stray tab — serde_yml rejects it.
        // Upstream JS throws, we keep going with empty metadata.
        let src = "---\n\t: broken\n---\ngraph TD\n";
        let (fm, rest) = parse_frontmatter(src);
        assert_eq!(fm, Some(Frontmatter::default()));
        assert_eq!(rest, "graph TD\n");
    }

    #[test]
    fn frontmatter_requires_leading_dashes() {
        // Upstream `^-{3}` is anchored — a blank line before `---`
        // kills detection. Verify we match that quirk.
        let src = "\n---\ntitle: x\n---\ngraph TD\n";
        let (fm, rest) = parse_frontmatter(src);
        assert!(fm.is_none());
        assert_eq!(rest, src);
    }
}
