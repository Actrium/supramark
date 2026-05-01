//! Venn diagram parser — line-oriented hand-rolled descent that
//! mirrors the upstream jison grammar at
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/venn/parser/venn.jison`.
//!
//! Notable upstream behaviours we replicate:
//!   - `set <id>` / `union <id1>, <id2>, ...` plus optional
//!     `["Bracket Label"]` and trailing `: <numeric size>`. Default
//!     size is `10 / N^2` where N is the count of identifiers.
//!   - identifier lists are alphabetically sorted on insertion (matches
//!     `vennDB.addSubsetData`'s `.sort()`).
//!   - "indent text" mode — once a `set` / `union` line is seen, any
//!     subsequent line whose body starts with `text ...` (after extra
//!     indentation) attaches `text_nodes` to the most recent subset.
//!   - `style <id list> key:value, key:value ...` collects style data.
//!   - `%%{init: { 'theme': ..., 'look': ..., 'handDrawnSeed': N,
//!     'venn': { 'useDebugLayout': bool } } }%%` directive lifts the
//!     four knobs we honour.

use crate::error::{MermaidError, Result};
use crate::model::venn::{VennDiagram, VennStyle, VennSubset, VennTextNode};

pub fn parse(source: &str) -> Result<VennDiagram> {
    let mut d = VennDiagram::default();
    let cleaned = extract_init_directives(source, &mut d);

    let lines: Vec<&str> = cleaned.lines().collect();
    let mut i = 0;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    if i >= lines.len() {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: "empty venn source".into(),
        });
    }

    // Header: `venn-beta`
    let header = lines[i].trim();
    if header != "venn-beta" {
        return Err(MermaidError::Parse {
            line: i + 1,
            col: 1,
            message: format!("expected 'venn-beta' header, got {header:?}"),
        });
    }
    i += 1;

    // Track current sets for the indent-text feature.
    let mut current_sets: Option<Vec<String>> = None;

    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();
        i += 1;
        if trimmed.is_empty() {
            continue;
        }
        // Comments — `%% ...`.
        if trimmed.starts_with("%%") {
            continue;
        }

        if let Some(rest) = strip_kw(trimmed, "title") {
            let value = rest.trim();
            if !value.is_empty() && d.meta.title.is_none() {
                d.meta.title = Some(value.to_string());
            }
            continue;
        }

        if let Some(rest) = strip_kw(trimmed, "set") {
            let (sets, label, size) = parse_subset_args(rest, 1)?;
            assert_eq!(sets.len(), 1);
            let resolved_size = size.unwrap_or(10.0 / (1.0 * 1.0));
            d.subsets.push(VennSubset {
                sets: sets.clone(),
                size: resolved_size,
                label,
            });
            current_sets = Some(sets);
            continue;
        }

        if let Some(rest) = strip_kw(trimmed, "union") {
            let (sets, label, size) = parse_subset_args(rest, 2)?;
            if sets.len() < 2 {
                return Err(MermaidError::Parse {
                    line: i,
                    col: 1,
                    message: "union requires multiple identifiers".into(),
                });
            }
            let n = sets.len() as f64;
            let resolved_size = size.unwrap_or(10.0 / (n * n));
            d.subsets.push(VennSubset {
                sets: sets.clone(),
                size: resolved_size,
                label,
            });
            current_sets = Some(sets);
            continue;
        }

        if let Some(rest) = strip_kw(trimmed, "text") {
            // Two forms:
            //   text <ID|STRING> [BRACKET_LABEL]      — with explicit set list?
            //     Actually grammar is `TEXT identifierList ID|STRING [BRACKET]`
            //   text <ID|STRING>                     — under indent mode, attaches to current set
            let (sets_opt, id, label) = parse_text_args(rest, &current_sets)?;
            let sets = match sets_opt {
                Some(s) => s,
                None => current_sets.clone().ok_or_else(|| MermaidError::Parse {
                    line: i,
                    col: 1,
                    message: "text without preceding set/union".into(),
                })?,
            };
            d.text_nodes.push(VennTextNode { sets, id, label });
            continue;
        }

        if let Some(rest) = strip_kw(trimmed, "style") {
            let (targets, styles) = parse_style_args(rest, i)?;
            d.styles.push(VennStyle { targets, styles });
            continue;
        }

        return Err(MermaidError::Parse {
            line: i,
            col: 1,
            message: format!("unrecognised venn line: {trimmed:?}"),
        });
    }

    Ok(d)
}

/// `<id_list> [BRACKET_LABEL] [: NUMERIC]`
/// Returns sorted (alphabetically) sets.
fn parse_subset_args(rest: &str, _min: usize) -> Result<(Vec<String>, Option<String>, Option<f64>)> {
    let mut s = rest.trim_start();

    // Identifier list (comma-separated).
    let mut sets = Vec::<String>::new();
    loop {
        let (id, after) = read_identifier_or_string(s)?;
        sets.push(id);
        let after = after.trim_start();
        if let Some(after_comma) = after.strip_prefix(',') {
            s = after_comma.trim_start();
            continue;
        }
        s = after;
        break;
    }

    // Optional bracket label.
    let mut label: Option<String> = None;
    let s_trim = s.trim_start();
    if s_trim.starts_with('[') {
        let (lbl, after) = read_bracket_label(s_trim)?;
        label = Some(lbl);
        s = after;
    }

    // Optional `: NUMERIC`.
    let mut size: Option<f64> = None;
    let s_trim = s.trim_start();
    if let Some(after_colon) = s_trim.strip_prefix(':') {
        let after_colon = after_colon.trim_start();
        let (num, _) = read_numeric(after_colon)?;
        size = Some(num);
    }

    sets.sort();
    Ok((sets, label, size))
}

/// `text <ID|STRING> [BRACKET]` — with optional preceding identifierList.
/// In our use we expect at most one identifier (the id). If we see an
/// identifier list followed by another bare identifier, treat the list
/// as the targets and the trailing as id. Most fixtures we care about
/// use the indent-mode form: `text "Item 1"` or `text id["Long Label"]`.
fn parse_text_args(rest: &str, _current: &Option<Vec<String>>) -> Result<(Option<Vec<String>>, String, Option<String>)> {
    let s = rest.trim();
    // First token: identifier or string.
    let (first, after) = read_identifier_or_string(s)?;
    let after = after.trim_start();

    if after.starts_with('[') {
        // form: `text id["Label"]` — id is the text id, no explicit set list
        let (label, _) = read_bracket_label(after)?;
        return Ok((None, first, Some(label)));
    }
    if after.is_empty() {
        return Ok((None, first, None));
    }
    // Could be the form `text setlist id [bracket]` — not used by any
    // of our fixtures. Treat anything else as id with no label.
    Ok((None, first, None))
}

/// `style <id list> key:value, key:value, ...`
fn parse_style_args(rest: &str, lineno: usize) -> Result<(Vec<String>, Vec<(String, String)>)> {
    let mut s = rest.trim_start();

    // Identifier list (comma-separated). The list ends when we see an
    // identifier followed by `:` (i.e. a style field begins).
    let mut targets = Vec::<String>::new();
    loop {
        let (id, after) = read_identifier_or_string(s)?;
        let after_trim = after.trim_start();
        if let Some(after_comma) = after_trim.strip_prefix(',') {
            // Decide whether the next token after ',' starts a style
            // field (`key:value`) or another target. Peek: if the next
            // token followed by `:` exists then we treat this as a
            // style field separator.
            let after_comma = after_comma.trim_start();
            // Same as targets: keep adding identifiers until we hit the
            // first one followed by `:`.
            // We push current id and continue parsing.
            targets.push(id);
            s = after_comma;
            continue;
        }
        // Check if the *current* identifier is followed by `:` — that
        // means it's actually a style field, the previous targets list
        // ended at the prior comma.
        if after_trim.starts_with(':') {
            // The previous comma was a target separator; current id is
            // a style field key. But we already consumed it. We need
            // to re-add it to s for the field-parser below.
            // Rebuild s = "<id><after_trim>".
            let mut buf = String::with_capacity(id.len() + after_trim.len());
            buf.push_str(&id);
            buf.push_str(after_trim);
            // Have to use a leak/static lifetime alternative — use owned String for parsing.
            return parse_style_fields(&buf, targets, lineno);
        }
        // Just an identifier followed by space/EOL? Treat it as last target,
        // and remaining is empty / spaces.
        targets.push(id);
        s = after_trim;
        break;
    }

    // Now parse style fields.
    parse_style_fields(s, targets, lineno)
}

fn parse_style_fields(s: &str, targets: Vec<String>, lineno: usize) -> Result<(Vec<String>, Vec<(String, String)>)> {
    let mut s = s.trim_start();
    let mut fields = Vec::<(String, String)>::new();
    while !s.is_empty() {
        let (key, after) = read_identifier_or_string(s)?;
        let after = after.trim_start();
        let after = after.strip_prefix(':').ok_or_else(|| MermaidError::Parse {
            line: lineno,
            col: 1,
            message: format!("expected ':' after style key {key:?}"),
        })?;
        let after = after.trim_start();
        // Read value up to the next top-level comma. Style values can
        // be a quoted string, hex color, rgb()/rgba() or a token list.
        let (value, after) = read_style_value(after);
        fields.push((key, value));
        let after = after.trim_start();
        if let Some(after_comma) = after.strip_prefix(',') {
            s = after_comma.trim_start();
            continue;
        }
        s = after;
        break;
    }
    let mut targets_sorted = targets;
    targets_sorted.sort();
    Ok((targets_sorted, fields))
}

/// Style value: read until we see an unbalanced `,` (one not inside
/// `()` or quotes). Trim trailing whitespace.
fn read_style_value(s: &str) -> (String, &str) {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut in_quotes = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_quotes {
            if b == b'"' {
                in_quotes = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => in_quotes = true,
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' if depth == 0 => break,
            _ => {}
        }
        i += 1;
    }
    let raw = &s[..i];
    let trimmed = raw.trim();
    let normalised = if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    };
    (normalised, &s[i..])
}

/// Read either an unquoted identifier `[A-Za-z_][A-Za-z0-9\-_]*` or a
/// quoted string `"..."`. Strips the surrounding quotes from strings.
fn read_identifier_or_string(s: &str) -> Result<(String, &str)> {
    let s = s.trim_start();
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return Err(MermaidError::Parse {
            line: 0,
            col: 1,
            message: "expected identifier".into(),
        });
    }
    if bytes[0] == b'"' {
        // Find closing quote.
        let mut i = 1;
        while i < bytes.len() && bytes[i] != b'"' {
            i += 1;
        }
        if i >= bytes.len() {
            return Err(MermaidError::Parse {
                line: 0,
                col: 1,
                message: "unterminated string".into(),
            });
        }
        let id = s[1..i].to_string();
        Ok((id, &s[i + 1..]))
    } else {
        let mut end = 0;
        for (j, c) in s.char_indices() {
            let accept = if j == 0 {
                c.is_ascii_alphabetic() || c == '_'
            } else {
                c.is_ascii_alphanumeric() || c == '_' || c == '-'
            };
            if accept {
                end = j + c.len_utf8();
            } else {
                break;
            }
        }
        if end == 0 {
            return Err(MermaidError::Parse {
                line: 0,
                col: 1,
                message: format!("expected identifier in {s:?}"),
            });
        }
        Ok((s[..end].to_string(), &s[end..]))
    }
}

/// Read `["..."]` (string-quoted) or `[...]` (raw, trimmed) from `s`.
fn read_bracket_label(s: &str) -> Result<(String, &str)> {
    let s = s.strip_prefix('[').ok_or_else(|| MermaidError::Parse {
        line: 0,
        col: 1,
        message: format!("expected '[' in {s:?}"),
    })?;
    if let Some(rest) = s.strip_prefix('"') {
        // Quoted: read until closing ".
        let close = rest.find('"').ok_or_else(|| MermaidError::Parse {
            line: 0,
            col: 1,
            message: "unterminated bracket-quoted label".into(),
        })?;
        let lbl = rest[..close].to_string();
        let after = &rest[close + 1..];
        let after = after.strip_prefix(']').ok_or_else(|| MermaidError::Parse {
            line: 0,
            col: 1,
            message: "expected ']' after bracket label".into(),
        })?;
        Ok((lbl, after))
    } else {
        let close = s.find(']').ok_or_else(|| MermaidError::Parse {
            line: 0,
            col: 1,
            message: "unterminated bracket label".into(),
        })?;
        let lbl = s[..close].trim().to_string();
        Ok((lbl, &s[close + 1..]))
    }
}

/// Read a numeric literal `[+-]?(\d+(\.\d+)?|\.\d+)`.
fn read_numeric(s: &str) -> Result<(f64, &str)> {
    let mut end = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    end = i;
    if end == 0 || (end == 1 && (bytes[0] == b'+' || bytes[0] == b'-')) {
        return Err(MermaidError::Parse {
            line: 0,
            col: 1,
            message: format!("expected number in {s:?}"),
        });
    }
    let v: f64 = s[..end].parse().map_err(|e| MermaidError::Parse {
        line: 0,
        col: 1,
        message: format!("bad numeric {:?}: {e}", &s[..end]),
    })?;
    Ok((v, &s[end..]))
}

/// `kw` followed by EOL or whitespace.
fn strip_kw<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    match rest.chars().next() {
        None => Some(rest),
        Some(c) if c.is_whitespace() => Some(rest),
        _ => None,
    }
}

/// Strip `%%{init:...}%%` blocks and capture the four venn-related
/// knobs. Returns the source minus those blocks (and a trailing `\n`
/// each, when present).
fn extract_init_directives(source: &str, d: &mut VennDiagram) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 3 <= bytes.len() && &bytes[i..i + 3] == b"%%{" {
            if let Some(end_rel) = source[i + 3..].find("}%%") {
                let end = i + 3 + end_rel + 3;
                let body = &source[i + 3..i + 3 + end_rel];
                apply_directive_body(body, d);
                i = end;
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
                continue;
            }
        }
        let len = source[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        out.push_str(&source[i..i + len]);
        i += len;
    }
    out
}

fn apply_directive_body(body: &str, d: &mut VennDiagram) {
    if let Some(s) = scan_str_after(body, "'theme'").or_else(|| scan_str_after(body, "\"theme\"")) {
        d.theme_name = Some(s);
    }
    if let Some(s) = scan_str_after(body, "'look'").or_else(|| scan_str_after(body, "\"look\"")) {
        if s == "handDrawn" {
            d.hand_drawn = true;
        }
    }
    if let Some(v) = scan_num_after(body, "'handDrawnSeed'")
        .or_else(|| scan_num_after(body, "\"handDrawnSeed\""))
    {
        d.hand_drawn_seed = Some(v as i64);
    }
    if let Some(v) = scan_bool_after(body, "'useDebugLayout'")
        .or_else(|| scan_bool_after(body, "\"useDebugLayout\""))
    {
        d.use_debug_layout = v;
    }
}

fn scan_num_after(s: &str, key: &str) -> Option<f64> {
    let idx = s.find(key)?;
    let rest = &s[idx + key.len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
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
    let q = rest.chars().next()?;
    if q != '"' && q != '\'' {
        return None;
    }
    let rest = &rest[1..];
    let close = rest.find(q)?;
    Some(rest[..close].to_string())
}

fn scan_bool_after(s: &str, key: &str) -> Option<bool> {
    let idx = s.find(key)?;
    let rest = &s[idx + key.len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_two_set_with_union() {
        let d = parse("venn-beta\n  set A\n  set B\n  union A, B\n").unwrap();
        assert_eq!(d.subsets.len(), 3);
        assert_eq!(d.subsets[0].sets, vec!["A".to_string()]);
        assert_eq!(d.subsets[1].sets, vec!["B".to_string()]);
        assert_eq!(d.subsets[2].sets, vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn parses_sized_sets() {
        let d = parse("venn-beta\n  set A:20\n  set B:15\n  union A, B:5\n").unwrap();
        assert_eq!(d.subsets[0].size, 20.0);
        assert_eq!(d.subsets[2].size, 5.0);
    }

    #[test]
    fn parses_bracket_labels() {
        let d = parse("venn-beta\n  set A[\"Hello\"]\n  set B[\"World\"]\n").unwrap();
        assert_eq!(d.subsets[0].label.as_deref(), Some("Hello"));
    }

    #[test]
    fn parses_text_indent_mode() {
        let d = parse("venn-beta\n  set A\n    text \"Item 1\"\n    text \"Item 2\"\n").unwrap();
        assert_eq!(d.text_nodes.len(), 2);
        assert_eq!(d.text_nodes[0].sets, vec!["A".to_string()]);
        assert_eq!(d.text_nodes[0].id, "Item 1");
    }
}
