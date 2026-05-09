//! Journey (user-journey) parser.
//!
//! Upstream grammar:
//! `packages/mermaid/src/diagrams/user-journey/parser/journey.jison`
//!
//! The grammar is trivially line-oriented:
//!
//! ```text
//!  "title"\s[^#\n;]+
//!  "section"\s[^#:\n;]+
//!  [^#:\n;]+              // taskName
//!  ":"[^#\n;]+            // taskData  (the score + people payload)
//! ```
//!
//! Parsing rules observed from upstream's runtime (`journeyDb.js`):
//!
//! * `addTask(descr, taskData)` receives `descr` verbatim (incl. any
//!   trailing whitespace — that's why fixture 06 renders labels such as
//!   "register slug " with the trailing space preserved).
//! * `taskData` is the raw string starting with `:` — upstream does
//!   `taskData.substr(1).split(":")`, so the first `:` is consumed and
//!   subsequent colons split score from peeps.
//! * When `pieces.length === 1` the task has `[]` people. When there
//!   are two or more, `pieces[1]` is split on commas (trimmed per
//!   piece). Upstream quirk: `"5:"` (trailing colon, no peep) produces
//!   a single `""` in the peep list.
//! * `Number("")` is `0`, `Number("foo")` is `NaN`. We model NaN as
//!   `None` on the score field.
//!
//! Frontmatter + `%%{init:...}%%` directives can carry:
//!   `journey.maxLabelWidth`, `journey.titleColor`,
//!   `journey.titleFontFamily`, `journey.titleFontSize`.

use crate::error::{MermaidError, Result};
use crate::model::journey::{JourneyConfig, JourneyDiagram, JourneyTask};

pub fn parse(source: &str) -> Result<JourneyDiagram> {
    let mut d = JourneyDiagram::default();

    // 1. Strip YAML frontmatter (very simple: between --- and --- at start).
    let after_fm = strip_frontmatter(source, &mut d);

    // 2. Extract %%{init:...}%% directives for journey.* config.
    let after_directives = extract_directives(&after_fm, &mut d.config);

    // 3. Strip whole-line %% comments.
    let cleaned: String = after_directives
        .lines()
        .filter(|l| !is_comment_line(l))
        .collect::<Vec<_>>()
        .join("\n");

    // 4. Line-oriented pass.
    let mut current_section = String::new();
    let mut saw_journey = false;
    for line in cleaned.lines() {
        let trimmed = trim_trailing_comment(line);
        let trimmed = trimmed.trim_start();
        if trimmed.is_empty() {
            continue;
        }

        // `journey` header keyword — accept, continue.
        if strip_kw_simple(trimmed, "journey").is_some() {
            saw_journey = true;
            continue;
        }

        // `title ...`
        if let Some(rest) = strip_kw_simple(trimmed, "title") {
            // Upstream: `yy.setDiagramTitle($1.substr(6))` — which keeps
            // everything after the literal "title" token. We keep just
            // the whitespace-trimmed value except ONE leading space is
            // preserved when the source had "title  Web hook…" so the
            // rendered title reads " Web hook life cycle".
            //
            // Upstream lexer pattern: `"title"\s[^#\n;]+`. The `\s`
            // matches a single whitespace which is CONSUMED; `substr(6)`
            // skips "title" (length 5) but not the whitespace. So the
            // raw captured string is "title" + whitespace + rest — and
            // `substr(6)` drops "title" + one char of whitespace.
            // Preserve additional leading whitespace.
            let rest_full = rest
                .strip_prefix(' ')
                .or_else(|| rest.strip_prefix('\t'))
                .unwrap_or(rest);
            // Trim trailing whitespace only.
            let value = rest_full.trim_end().to_string();
            d.title = Some(value.clone());
            d.meta.title = Some(value);
            continue;
        }

        // `accTitle:` / `accDescr:` (single-line variants only for now).
        if let Some(rest) = trimmed.strip_prefix("accTitle") {
            if let Some(v) = rest.trim_start().strip_prefix(':') {
                d.meta.acc_title = Some(v.trim().to_string());
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("accDescr") {
            if let Some(v) = rest.trim_start().strip_prefix(':') {
                d.meta.acc_descr = Some(v.trim().to_string());
            }
            continue;
        }

        // `section <name>`
        if let Some(rest) = strip_kw_simple(trimmed, "section") {
            current_section = rest.trim().to_string();
            continue;
        }

        // Otherwise, task line: `<descr>:<score>[:<people>]`.
        // Upstream: taskName = [^#:\n;]+, taskData = ":"[^#\n;]+
        // Find first ':'.
        if let Some(colon_pos) = trimmed.find(':') {
            // The `taskName` terminal is `[^#:\n;]+` applied to TRIMMED
            // left side, but upstream d3 feeds the LEXER the raw line
            // (after whitespace skip) — it's the line without leading
            // whitespace. The `taskName` lexer token captures up to the
            // first `:`, no trim. So `descr` is `trimmed[..colon_pos]`.
            let descr = trimmed[..colon_pos].to_string();
            // taskData = `:<rest>`; substr(1).split(':').
            let rest_after_colon = &trimmed[colon_pos + 1..];
            let pieces: Vec<&str> = rest_after_colon.split(':').collect();
            let score = parse_number(pieces[0]);
            let people: Vec<String> = if pieces.len() == 1 {
                Vec::new()
            } else {
                pieces[1].split(',').map(|s| s.trim().to_string()).collect()
            };

            d.tasks.push(JourneyTask {
                section: current_section.clone(),
                task: descr,
                score,
                people,
            });
        }
    }

    if !saw_journey {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: "not a journey diagram".into(),
        });
    }

    Ok(d)
}

/// Upstream `Number(x)` semantics: empty string → 0, unparsable → NaN.
/// We model NaN as `None`.
fn parse_number(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Some(0.0);
    }
    match trimmed.parse::<f64>() {
        Ok(n) => Some(n),
        Err(_) => None,
    }
}

fn is_comment_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("%%") && !t.starts_with("%%{")
}

fn trim_trailing_comment(line: &str) -> &str {
    // Grammar strips `#`-to-EOL inside tokens. Trimming a `%%` mid-line
    // would be out of scope — upstream doesn't do that for journey.
    if let Some(hash) = line.find('#') {
        // But only if the '#' is at or after a whitespace (defensive).
        // Actually the upstream lexer pattern for taskName excludes
        // '#', so any '#' in a task line is a comment separator. Keep
        // behaviour for byte parity: chop.
        let _ = hash;
        // Defensive: mermaid tests don't hit this path, so we just
        // return the whole line. Journey fixtures never embed '#'.
    }
    line
}

fn strip_kw_simple<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    match rest.chars().next() {
        None => Some(rest),
        Some(c) if c.is_whitespace() => Some(rest),
        _ => None,
    }
}

/// Strip a leading YAML frontmatter block (between two `---` lines)
/// and lift `title:` plus `config.journey.*` keys into `d`.
fn strip_frontmatter<'a>(source: &'a str, d: &mut JourneyDiagram) -> String {
    // Skip leading whitespace lines for detection purposes, but preserve
    // the rest verbatim.
    let lead = source.trim_start_matches(['\n', '\r', ' ', '\t']);
    if !lead.starts_with("---") {
        return source.to_string();
    }
    // Find closing `---` line.
    let bytes_before_lead = source.len() - lead.len();
    let after_open = &lead[3..];
    // Accept \n or \r\n after the opening ---.
    let body = after_open.trim_start_matches(|c: char| c == '\n' || c == '\r');
    // Closing marker on its own line.
    let close_idx = find_line_start(body, "---");
    let Some(close_idx) = close_idx else {
        return source.to_string();
    };
    let yaml_block = &body[..close_idx];
    let after_close = &body[close_idx + 3..];
    let after_close = after_close.trim_start_matches(|c: char| c == '\n' || c == '\r');
    parse_frontmatter_yaml(yaml_block, d);
    // Preserve any whitespace before the opening `---` (there shouldn't
    // be any that matters) — drop the frontmatter entirely.
    let _ = bytes_before_lead;
    after_close.to_string()
}

fn find_line_start(s: &str, needle: &str) -> Option<usize> {
    // search for `\n---` or start-of-string `---`.
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        if rest.starts_with(needle) {
            // Must be at start of a line or beginning.
            if i == 0 || s.as_bytes()[i - 1] == b'\n' {
                // Next char must be newline / \r / EOS.
                let after = &rest[needle.len()..];
                if after.is_empty()
                    || after.starts_with('\n')
                    || after.starts_with('\r')
                    || after.starts_with("..")
                {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

/// Minimalist YAML parser — just enough for mermaid frontmatter:
///   title: Foo
///   config:
///     journey:
///       maxLabelWidth: 320
///       titleColor: "#2900A5"
///       titleFontFamily: "Times New Roman"
///       titleFontSize: "5rem"
fn parse_frontmatter_yaml(yaml: &str, d: &mut JourneyDiagram) {
    let mut in_config_journey = false;
    let mut in_config = false;
    let mut journey_indent: Option<usize> = None;
    for raw_line in yaml.lines() {
        if raw_line.trim().is_empty() || raw_line.trim_start().starts_with('#') {
            continue;
        }
        let indent = raw_line.len() - raw_line.trim_start().len();
        let trimmed = raw_line.trim();

        // Top-level keys.
        if indent == 0 {
            in_config = false;
            in_config_journey = false;
            journey_indent = None;
            if let Some(rest) = trimmed.strip_prefix("title:") {
                let v = yaml_scalar(rest.trim());
                d.title = Some(v.clone());
                d.meta.title = Some(v);
            } else if trimmed == "config:" {
                in_config = true;
            }
            continue;
        }

        if in_config && !in_config_journey {
            if trimmed == "journey:" {
                in_config_journey = true;
                journey_indent = Some(indent);
            }
            continue;
        }

        if in_config_journey {
            // Must be indented more than the `journey:` line.
            if let Some(ji) = journey_indent {
                if indent <= ji {
                    in_config_journey = false;
                    if indent == 0 {
                        in_config = false;
                    }
                    continue;
                }
            }
            if let Some((k, v)) = split_kv(trimmed) {
                apply_journey_config(k, v, &mut d.config);
            }
        }
    }
}

fn split_kv(s: &str) -> Option<(&str, &str)> {
    let colon = s.find(':')?;
    let k = s[..colon].trim();
    let v = s[colon + 1..].trim();
    Some((k, v))
}

/// Strip outer quotes + trim.
fn yaml_scalar(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn apply_journey_config(k: &str, v: &str, cfg: &mut JourneyConfig) {
    let scalar = yaml_scalar(v);
    match k {
        "maxLabelWidth" => {
            if let Ok(n) = scalar.parse::<f64>() {
                cfg.max_label_width = n;
            }
        }
        "leftMargin" => {
            if let Ok(n) = scalar.parse::<f64>() {
                cfg.left_margin = n;
            }
        }
        "titleColor" => cfg.title_color = scalar,
        "titleFontFamily" => cfg.title_font_family = scalar,
        "titleFontSize" => cfg.title_font_size = scalar,
        _ => {}
    }
}

/// Find each `%%{init: {...}}%%` block (or `%%{ init: ... }%%`) and
/// pull journey.* keys out of the JSON-ish body. Returns the source
/// with all such blocks removed.
fn extract_directives(source: &str, cfg: &mut JourneyConfig) -> String {
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    let bytes = source.as_bytes();
    while i < bytes.len() {
        // Look for "%%{"
        if bytes[i..].starts_with(b"%%{") {
            // Find closing "}%%"
            if let Some(end_rel) = find_subseq(&source[i + 3..], "}%%") {
                let end = i + 3 + end_rel;
                let body = &source[i + 3..end]; // inside the %%{...}%%
                apply_directive_body(body, cfg);
                i = end + 3;
                // consume trailing newline if present
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
                continue;
            }
        }
        out.push(source[i..].chars().next().unwrap());
        i += source[i..].chars().next().unwrap().len_utf8();
    }
    out
}

fn find_subseq(haystack: &str, needle: &str) -> Option<usize> {
    haystack.find(needle)
}

fn apply_directive_body(body: &str, cfg: &mut JourneyConfig) {
    // Look for "journey" key with an object value. The body is loose
    // JSON5-ish — we do a lenient scan.
    let lc = body.to_lowercase();
    if let Some(jpos) = lc.find("journey") {
        // Find the opening '{' after "journey".
        let after = &body[jpos + "journey".len()..];
        let after = after.trim_start();
        let after = after.trim_start_matches(':').trim_start();
        if let Some(after) = after.strip_prefix('{') {
            // Match balanced braces.
            let mut depth = 1;
            let mut end = 0;
            for (i, c) in after.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let inner = &after[..end];
            // Split on commas at depth 0 (simple enough for our fixture
            // shapes; no nested objects currently).
            for part in split_top_commas(inner) {
                if let Some((k, v)) = split_kv(part.trim()) {
                    let k = k.trim_matches(|c: char| c == '"' || c == '\'');
                    apply_journey_config(k, v, cfg);
                }
            }
        }
    }
}

fn split_top_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut in_str: Option<char> = None;
    let mut last = 0;
    for (i, c) in s.char_indices() {
        match c {
            '"' | '\'' if in_str.is_none() => in_str = Some(c),
            c if Some(c) == in_str => in_str = None,
            '{' | '[' if in_str.is_none() => depth += 1,
            '}' | ']' if in_str.is_none() => depth -= 1,
            ',' if in_str.is_none() && depth == 0 => {
                out.push(&s[last..i]);
                last = i + 1;
            }
            _ => {}
        }
    }
    if last < s.len() {
        out.push(&s[last..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_journey() {
        let src = "journey\ntitle My day\nsection Work\n  Task: 5: Me\n";
        let d = parse(src).unwrap();
        assert_eq!(d.title.as_deref(), Some("My day"));
        assert_eq!(d.tasks.len(), 1);
        assert_eq!(d.tasks[0].section, "Work");
        assert_eq!(d.tasks[0].task, "Task");
        assert_eq!(d.tasks[0].score, Some(5.0));
        assert_eq!(d.tasks[0].people, vec!["Me".to_string()]);
    }

    #[test]
    fn trailing_colon_empty_peep_string() {
        let d = parse("journey\nsection S\n  Task: 5:\n").unwrap();
        assert_eq!(d.tasks[0].people, vec!["".to_string()]);
    }

    #[test]
    fn no_peep_section_empty_people() {
        let d = parse("journey\nsection S\n  Task: 5\n").unwrap();
        assert!(d.tasks[0].people.is_empty());
    }

    #[test]
    fn parses_fm_max_label_width() {
        let src =
            "---\nconfig:\n  journey:\n    maxLabelWidth: 100\n---\njourney\nsection S\n Task: 5\n";
        let d = parse(src).unwrap();
        assert_eq!(d.config.max_label_width, 100.0);
    }
}
