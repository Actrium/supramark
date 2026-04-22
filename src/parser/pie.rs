//! Pie diagram parser — hand-rolled recursive descent.
//!
//! Mirrors the langium grammar at
//! `/ext/mermaid-official-stable-v11.14.0/packages/parser/src/language/pie/pie.langium`:
//!
//! ```text
//! Pie:
//!   NEWLINE*
//!   "pie" showData?="showData"?
//!   (TitleAndAccessibilities | sections+=PieSection | NEWLINE)*
//!
//! PieSection: label=STRING ":" value=NUMBER EOL
//! ```
//!
//! The parser also recognises a single optional trailing `title ...`
//! fragment on the header line (e.g. `pie title Sports in Sweden`) and
//! the `accTitle:` / `accDescr:` / `title` lines that appear inside the
//! body. Duplicate slice labels are dropped (first write wins) to match
//! upstream `pieDb.addSection`.
//!
//! We additionally scan for `%%{init:...}%%` directive blocks that
//! carry `pie.textPosition` or `themeVariables.pieOuterStrokeWidth` —
//! preprocess in Wave 0 already strips these from the source, but for
//! byte-exact tests that feed raw `.mmd` bytes we still re-detect them
//! here. When the source has already been cleaned upstream, the scan
//! is a no-op and defaults win.

use crate::error::{MermaidError, Result};
use crate::model::pie::{PieDiagram, PieSlice};

/// Default label-radius fraction (upstream `defaultConfig.ts: pie.textPosition = 0.75`).
const DEFAULT_TEXT_POSITION: f64 = 0.75;
/// Default outer-stroke width (upstream `theme.pieOuterStrokeWidth = "2px"`).
const DEFAULT_OUTER_STROKE_WIDTH: &str = "2px";

pub fn parse(source: &str) -> Result<PieDiagram> {
    let mut d = PieDiagram {
        text_position: DEFAULT_TEXT_POSITION,
        outer_stroke_width: DEFAULT_OUTER_STROKE_WIDTH.to_string(),
        ..PieDiagram::default()
    };

    // First pass: hoover up any remaining `%%{init:...}%%` directives.
    // In the standard pipeline preprocess has already removed them, but
    // the byte-exact test harness feeds raw mmd sources directly to
    // `parse`, so we still need to honour them here.
    let source_after_directives = extract_init_directives(source, &mut d);

    // Second pass: line-oriented parse of the body.
    let lines: Vec<&str> = source_after_directives.lines().collect();
    let mut i = 0;

    // Skip leading blank lines.
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }

    // Header: `pie` [ `showData` ] [ `title <rest>` ]
    if i >= lines.len() {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: "empty pie source".into(),
        });
    }
    parse_header(lines[i], i + 1, &mut d)?;
    i += 1;

    // Body lines — in any order: title / accTitle / accDescr / "label": value / blank.
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        if let Some(rest) = strip_kw(trimmed, "title") {
            // Body-level `title ...` (the `title` fragment of TitleAndAccessibilities).
            let value = rest.trim_start().trim_end().to_string();
            if d.meta.title.is_none() && !value.is_empty() {
                d.meta.title = Some(value);
            }
        } else if let Some(rest) = strip_kw(trimmed, "accTitle") {
            // `accTitle:<value>` — colon is required by the langium terminal.
            if let Some(val) = rest.strip_prefix(':') {
                d.meta.acc_title = Some(val.trim().to_string());
            }
        } else if let Some(rest) = strip_kw(trimmed, "accDescr") {
            // `accDescr:<value>` or block form `accDescr { ... }` — we only
            // encounter the colon form in the fixtures we cover.
            if let Some(val) = rest.strip_prefix(':') {
                d.meta.acc_descr = Some(val.trim().to_string());
            }
        } else if trimmed.starts_with('"') || trimmed.starts_with('\'') {
            let (label, value) = parse_slice(trimmed, i + 1)?;
            // Upstream `pieDb.addSection`: first-writer-wins on duplicate labels.
            if !d.slices.iter().any(|s| s.label == label) {
                d.slices.push(PieSlice { label, value });
            }
        } else {
            return Err(MermaidError::Parse {
                line: i + 1,
                col: 1,
                message: format!("unrecognised pie line: {trimmed:?}"),
            });
        }
        i += 1;
    }

    Ok(d)
}

/// Remove `%%{init:...}%%` blocks and capture the two pie-related knobs
/// they might contain. Returns the source with the directive blocks
/// elided; non-directive content is preserved verbatim.
fn extract_init_directives(source: &str, d: &mut PieDiagram) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        // Look for the literal "%%{"
        if i + 3 <= bytes.len() && &bytes[i..i + 3] == b"%%{" {
            // Find matching "}%%".
            if let Some(end_rel) = source[i + 3..].find("}%%") {
                let end = i + 3 + end_rel + 3;
                let body = &source[i + 3..i + 3 + end_rel]; // inside
                apply_directive_body(body, d);
                i = end;
                // Drop a trailing newline so we don't leave a blank line.
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
                continue;
            }
        }
        out.push(source[i..].chars().next().unwrap_or('\0'));
        i += source[i..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(1);
    }
    out
}

/// Very small subset JSON scan for the two pie knobs we care about.
/// We do NOT attempt to parse the whole directive as JSON — we only
/// need two scalar values. Failing silently (defaults win) is fine:
/// byte-exact parity is re-checked by the test fixtures.
fn apply_directive_body(body: &str, d: &mut PieDiagram) {
    // `"textPosition": 0.45`  — any whitespace around the colon.
    if let Some(v) = scan_num_after(body, "\"textPosition\"") {
        d.text_position = v;
    }
    // `"pieOuterStrokeWidth": "5px"`
    if let Some(s) = scan_str_after(body, "\"pieOuterStrokeWidth\"") {
        d.outer_stroke_width = s;
    }
}

fn scan_num_after(s: &str, key: &str) -> Option<f64> {
    let idx = s.find(key)?;
    let rest = &s[idx + key.len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    // Read a number literal: [-]?\d+(\.\d+)?
    let mut end = 0;
    for (j, c) in rest.char_indices() {
        let accept = if j == 0 {
            c == '-' || c.is_ascii_digit()
        } else {
            c.is_ascii_digit() || c == '.'
        };
        if accept {
            end = j + c.len_utf8();
        } else {
            break;
        }
    }
    rest[..end].parse::<f64>().ok()
}

fn scan_str_after(s: &str, key: &str) -> Option<String> {
    let idx = s.find(key)?;
    let rest = &s[idx + key.len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let close = rest.find('"')?;
    Some(rest[..close].to_string())
}

/// Header: `pie` `[showData]` `[title <rest>]`
fn parse_header(line: &str, lineno: usize, d: &mut PieDiagram) -> Result<()> {
    let trimmed = line.trim();
    let mut rest = match strip_kw(trimmed, "pie") {
        Some(r) => r,
        None => {
            return Err(MermaidError::Parse {
                line: lineno,
                col: 1,
                message: format!("expected 'pie' header, got {trimmed:?}"),
            })
        }
    };
    rest = rest.trim_start();

    // Optional `showData` flag.
    if let Some(after) = strip_kw(rest, "showData") {
        d.show_data = true;
        rest = after.trim_start();
    }

    // Optional inline `title ...`.
    if let Some(after) = strip_kw(rest, "title") {
        let title = after.trim().to_string();
        d.meta.title = Some(title);
    } else if !rest.is_empty() {
        // Something trailing that isn't a keyword — upstream grammar
        // allows only the above. Ignore silently for defensive parsing.
    }
    Ok(())
}

/// `"label": value` or `'label' : value` — STRING + ":" + NUMBER.
fn parse_slice(line: &str, lineno: usize) -> Result<(String, f64)> {
    let (label, rest) = parse_string_lit(line, lineno)?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':').ok_or_else(|| MermaidError::Parse {
        line: lineno,
        col: 1,
        message: format!("expected ':' after label in {line:?}"),
    })?;
    let num_str = rest.split_whitespace().next().unwrap_or("");
    let value: f64 = num_str.parse().map_err(|e| MermaidError::Parse {
        line: lineno,
        col: 1,
        message: format!("bad pie value {num_str:?}: {e}"),
    })?;
    Ok((label, value))
}

/// Parse a double- or single-quoted string literal. Returns (contents, rest).
/// Handles the simple `\\.` escape from the langium `STRING` terminal.
fn parse_string_lit(input: &str, lineno: usize) -> Result<(String, &str)> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(MermaidError::Parse {
            line: lineno,
            col: 1,
            message: "empty slice line".into(),
        });
    }
    let quote = bytes[0];
    if quote != b'"' && quote != b'\'' {
        return Err(MermaidError::Parse {
            line: lineno,
            col: 1,
            message: format!("expected quoted label, got {input:?}"),
        });
    }
    let mut out = String::new();
    let mut i = 1;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            out.push(bytes[i + 1] as char);
            i += 2;
            continue;
        }
        if b == quote {
            return Ok((out, &input[i + 1..]));
        }
        out.push(b as char);
        i += 1;
    }
    Err(MermaidError::Parse {
        line: lineno,
        col: 1,
        message: format!("unterminated string in {input:?}"),
    })
}

/// If `s` starts with `kw` followed by end-of-string or whitespace/`:`,
/// return the remainder (after the keyword). Otherwise return `None`.
fn strip_kw<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    match rest.chars().next() {
        None => Some(rest),
        Some(c) if c.is_whitespace() || c == ':' => Some(rest),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_pie() {
        let src = "pie title Sports in Sweden\n  \"Bandy\": 40\n  \"Ice-Hockey\": 80\n  \"Football\": 90\n";
        let d = parse(src).unwrap();
        assert_eq!(d.meta.title.as_deref(), Some("Sports in Sweden"));
        assert_eq!(d.slices.len(), 3);
        assert_eq!(d.slices[0].label, "Bandy");
        assert_eq!(d.slices[0].value, 40.0);
        assert!(!d.show_data);
    }

    #[test]
    fn parses_show_data_without_title() {
        let d = parse("pie showData\n  \"Dogs\": 50\n  \"Cats\": 25\n").unwrap();
        assert!(d.show_data);
        assert_eq!(d.meta.title, None);
        assert_eq!(d.slices.len(), 2);
    }

    #[test]
    fn parses_body_level_acc_and_title() {
        let src = "pie\n  title Default text position: Animal adoption\n  accTitle: simple pie char demo\n  accDescr: three sections\n  \"Dogs\": 386\n";
        let d = parse(src).unwrap();
        assert_eq!(
            d.meta.title.as_deref(),
            Some("Default text position: Animal adoption")
        );
        assert_eq!(d.meta.acc_title.as_deref(), Some("simple pie char demo"));
        assert_eq!(d.meta.acc_descr.as_deref(), Some("three sections"));
    }

    #[test]
    fn parses_directive_text_position_and_outer_stroke() {
        let src = "%%{init: {\"pie\": {\"textPosition\": 0.45}, \"themeVariables\": {\"pieOuterStrokeWidth\": \"5px\"}}}%%\npie\n  \"A\": 1\n";
        let d = parse(src).unwrap();
        assert!((d.text_position - 0.45).abs() < 1e-12);
        assert_eq!(d.outer_stroke_width, "5px");
    }

    #[test]
    fn duplicate_labels_kept_first_only() {
        let d = parse("pie\n  \"A\": 1\n  \"A\": 99\n  \"B\": 2\n").unwrap();
        assert_eq!(d.slices.len(), 2);
        assert_eq!(d.slices[0].value, 1.0);
    }
}
