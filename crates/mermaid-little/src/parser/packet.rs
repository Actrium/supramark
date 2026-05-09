//! Packet diagram parser.
//!
//! Hand-rolled, line-oriented port of upstream's Langium grammar
//! (`/ext/mermaid-official-stable-v11.14.0/packages/parser/src/language/packet/packet.langium`,
//! 21 LoC) + the population step in `diagrams/packet/parser.ts`.
//!
//! Accepted lines inside the diagram body, after the `packet` /
//! `packet-beta` header:
//!
//! - `title <text>` — diagram title (same semantics as other diagrams).
//! - `accTitle: <text>` / `accDescr: <text>` — accessibility metadata.
//! - `<start>-<end>: "<label>"` — range field.
//! - `<start>: "<label>"`        — single-bit field.
//! - `+<bits>: "<label>"`        — relative field, width only; start
//!   is implicit (previous-end + 1).
//!
//! The parser also detects a YAML frontmatter block at the very top
//! of the source so the render/layout stages can observe
//! `config.packet.{showBits,bitsPerRow,…}`. This is a pragmatic
//! shortcut: the global `Config` type does not carry a typed `packet`
//! block (keys land in `extras`), so we extract what we need right here.

use crate::error::{MermaidError, Result};
use crate::model::packet::{PacketConfig, PacketDiagram, PacketField};
use crate::model::DiagramMeta;

/// Parse a packet diagram source string into a [`PacketDiagram`].
///
/// `source` may include a YAML frontmatter block; we strip it before
/// parsing the body (matching upstream's preprocess → parser order)
/// and surface `title` / `config.packet.*` into the model.
pub fn parse(source: &str) -> Result<PacketDiagram> {
    // 1. Normalise newlines & pull off the frontmatter if present.
    let normalized = normalize_newlines(source);
    let (fm_title, fm_config, body) = strip_frontmatter(&normalized);

    let mut diagram = PacketDiagram {
        meta: DiagramMeta::default(),
        fields: Vec::new(),
        config: fm_config.unwrap_or_default(),
    };
    if let Some(t) = fm_title {
        diagram.meta.title = Some(t);
    }

    // 2. Scan the body line by line.
    let mut saw_header = false;
    let mut last_end: i64 = -1;
    for (idx, raw_line) in body.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if strip_comment_line(line) {
            continue;
        }

        if !saw_header {
            // The header is `packet` or `packet-beta`, optionally
            // followed by whitespace + trailing garbage (upstream
            // accepts extra whitespace but nothing else).
            if line == "packet" || line == "packet-beta" {
                saw_header = true;
                continue;
            }
            return Err(MermaidError::Parse {
                line: idx + 1,
                col: 1,
                message: format!("expected 'packet' or 'packet-beta' header, got {line:?}"),
            });
        }

        // Recognise title / accessibility directives.
        if let Some(rest) = strip_keyword(line, "title") {
            diagram.meta.title = Some(rest.to_owned());
            continue;
        }
        if let Some(rest) = strip_colon_keyword(line, "accTitle") {
            diagram.meta.acc_title = Some(rest.to_owned());
            continue;
        }
        if let Some(rest) = strip_colon_keyword(line, "accDescr") {
            diagram.meta.acc_descr = Some(rest.to_owned());
            continue;
        }

        // Otherwise, it must be a bit-field block.
        let field = parse_block_line(line, last_end).map_err(|msg| MermaidError::Parse {
            line: idx + 1,
            col: 1,
            message: msg,
        })?;
        last_end = i64::from(field.end);
        diagram.fields.push(field);
    }

    if !saw_header {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: "empty packet diagram source (no header)".into(),
        });
    }

    Ok(diagram)
}

/// Normalise `\r\n` / lone `\r` to `\n`. The preprocessor would have
/// done this already in the real pipeline, but exposing the parser as
/// a standalone entry point (tests, ad-hoc callers) means we should
/// still cope with raw Windows-style input.
fn normalize_newlines(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
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

/// Detect a YAML frontmatter block at the top of the source, returning
/// the extracted title, packet config (if any) and the remaining body.
///
/// We deliberately reimplement a minimal YAML subset here (matching the
/// fixtures we have) instead of pulling in the shared frontmatter
/// machinery — the global `Config` type does not expose a typed
/// `packet` block, so piggybacking on it would still require a raw
/// extras lookup. Doing the extraction locally keeps the parser
/// self-contained.
fn strip_frontmatter(source: &str) -> (Option<String>, Option<PacketConfig>, String) {
    if !source.starts_with("---") {
        return (None, None, source.to_owned());
    }
    let after_open = match source.find('\n') {
        Some(p) => &source[p + 1..],
        None => return (None, None, source.to_owned()),
    };
    // Find the closing `---` line.
    let mut body_end: Option<usize> = None;
    let mut rest_start: Option<usize> = None;
    let mut offset = 0usize;
    for line in after_open.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n').trim_end();
        if trimmed == "---" {
            body_end = Some(offset);
            rest_start = Some(offset + line.len());
            break;
        }
        offset += line.len();
    }
    let Some(body_end) = body_end else {
        return (None, None, source.to_owned());
    };
    let body = &after_open[..body_end];
    let rest = rest_start
        .map(|s| after_open[s..].to_owned())
        .unwrap_or_default();

    // Parse the body. We rely on serde_yml to handle nesting / quotes
    // / booleans / bare strings. Match the same tolerant-on-failure
    // stance as the shared frontmatter parser.
    let mut title: Option<String> = None;
    let mut packet_config: Option<PacketConfig> = None;

    if let Ok(serde_yml::Value::Mapping(map)) = serde_yml::from_str::<serde_yml::Value>(body) {
        if let Some(t) = map.get(serde_yml::Value::String("title".into())) {
            title = yaml_to_string(t);
        }
        if let Some(serde_yml::Value::Mapping(cfg_map)) =
            map.get(serde_yml::Value::String("config".into()))
        {
            if let Some(pkt) = cfg_map.get(serde_yml::Value::String("packet".into())) {
                packet_config = Some(extract_packet_config(pkt));
            }
        }
    }

    (title, packet_config, rest)
}

fn yaml_to_string(v: &serde_yml::Value) -> Option<String> {
    match v {
        serde_yml::Value::String(s) => Some(s.clone()),
        serde_yml::Value::Number(n) => Some(n.to_string()),
        serde_yml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn extract_packet_config(v: &serde_yml::Value) -> PacketConfig {
    let mut cfg = PacketConfig::default();
    let serde_yml::Value::Mapping(map) = v else {
        return cfg;
    };
    let get = |key: &str| map.get(serde_yml::Value::String(key.into()));

    if let Some(n) = get("rowHeight").and_then(yaml_to_f64) {
        cfg.row_height = n;
    }
    if let Some(n) = get("bitWidth").and_then(yaml_to_f64) {
        cfg.bit_width = n;
    }
    if let Some(n) = get("bitsPerRow").and_then(yaml_to_u32) {
        cfg.bits_per_row = n;
    }
    if let Some(b) = get("showBits").and_then(yaml_to_bool) {
        cfg.show_bits = b;
    }
    if let Some(n) = get("paddingX").and_then(yaml_to_f64) {
        cfg.padding_x = n;
    }
    if let Some(n) = get("paddingY").and_then(yaml_to_f64) {
        cfg.padding_y = n;
    }
    cfg
}

fn yaml_to_f64(v: &serde_yml::Value) -> Option<f64> {
    match v {
        serde_yml::Value::Number(n) => n.as_f64(),
        serde_yml::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn yaml_to_u32(v: &serde_yml::Value) -> Option<u32> {
    match v {
        serde_yml::Value::Number(n) => n.as_u64().and_then(|x| u32::try_from(x).ok()),
        serde_yml::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn yaml_to_bool(v: &serde_yml::Value) -> Option<bool> {
    match v {
        serde_yml::Value::Bool(b) => Some(*b),
        _ => None,
    }
}

/// `true` when the line is a whole-line comment (`%% ...`). The
/// preprocessor normally removes these; we keep the defensive check.
fn strip_comment_line(line: &str) -> bool {
    line.starts_with("%%")
}

/// If `line` starts with `keyword` followed by whitespace, return the
/// remainder (trimmed). Used for bare-keyword forms such as `title X`.
fn strip_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?;
    let first = rest.chars().next()?;
    if !first.is_whitespace() {
        return None;
    }
    Some(rest.trim_start())
}

/// If `line` matches `keyword:` (optional whitespace after the colon),
/// return the trimmed tail. Used for accessibility directives.
fn strip_colon_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    Some(rest.trim_start())
}

/// Parse a single block line: `<start>[-<end>]: "<label>"` or
/// `+<bits>: "<label>"`. `last_end` is the previous block's end bit
/// (or -1 before any block) — used by the `+bits` form.
fn parse_block_line(line: &str, last_end: i64) -> std::result::Result<PacketField, String> {
    let colon = line
        .find(':')
        .ok_or_else(|| format!("missing ':' in block line {line:?}"))?;
    let (head, tail) = line.split_at(colon);
    let head = head.trim();
    let tail = tail[1..].trim(); // drop ':'

    // The label must be a quoted STRING literal — upstream langium
    // grammar enforces that; we extract the text between the first and
    // last double-quote in the tail and run the same escape handling
    // as the shared string helper in common.rs (not yet populated).
    let label = parse_quoted_label(tail)
        .ok_or_else(|| format!("missing quoted label in block line {line:?}"))?;

    let (start, end) = if let Some(bits_str) = head.strip_prefix('+') {
        let bits: u32 = bits_str
            .trim()
            .parse()
            .map_err(|_| format!("invalid bit-count in {line:?}"))?;
        if bits == 0 {
            return Err(format!(
                "Packet block {head} is invalid. Cannot have a zero bit field."
            ));
        }
        let start = (last_end + 1) as u32;
        let end = start + bits - 1;
        (start, end)
    } else if let Some(dash_pos) = head.find('-') {
        let (a, b) = head.split_at(dash_pos);
        let start: u32 = a
            .trim()
            .parse()
            .map_err(|_| format!("invalid start-bit in {line:?}"))?;
        let end: u32 = b[1..]
            .trim()
            .parse()
            .map_err(|_| format!("invalid end-bit in {line:?}"))?;
        if end < start {
            return Err(format!(
                "Packet block {start} - {end} is invalid. End must be greater than start."
            ));
        }
        (start, end)
    } else {
        let bit: u32 = head
            .trim()
            .parse()
            .map_err(|_| format!("invalid bit in {line:?}"))?;
        (bit, bit)
    };

    // Contiguity check (upstream errors out otherwise).
    if i64::from(start) != last_end + 1 {
        return Err(format!(
            "Packet block {start} - {end} is not contiguous. It should start from {}.",
            last_end + 1
        ));
    }

    Ok(PacketField {
        start,
        end,
        label: label.to_owned(),
    })
}

/// Extract the label from a tail like `"Source Port"`. We take the
/// substring between the first `"` and the last `"` so embedded
/// whitespace / backslashes round-trip verbatim — upstream's Langium
/// STRING terminal behaves identically for the subset exercised by
/// these fixtures.
fn parse_quoted_label(tail: &str) -> Option<&str> {
    let first = tail.find('"')?;
    let last = tail.rfind('"')?;
    if last <= first {
        return None;
    }
    Some(&tail[first + 1..last])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_packet_beta() {
        let src = "packet-beta\n  title Hello world\n  0-10: \"hello\"\n";
        let d = parse(src).unwrap();
        assert_eq!(d.meta.title.as_deref(), Some("Hello world"));
        assert_eq!(d.fields.len(), 1);
        assert_eq!(d.fields[0].start, 0);
        assert_eq!(d.fields[0].end, 10);
        assert_eq!(d.fields[0].label, "hello");
    }

    #[test]
    fn parses_single_bit_blocks() {
        let src = "packet\n  0: \"h\"\n  1: \"i\"\n";
        let d = parse(src).unwrap();
        assert_eq!(d.fields.len(), 2);
        assert_eq!(d.fields[0].start, 0);
        assert_eq!(d.fields[0].end, 0);
        assert_eq!(d.fields[1].start, 1);
        assert_eq!(d.fields[1].end, 1);
    }

    #[test]
    fn extracts_frontmatter_packet_showbits() {
        let src = "---\ntitle: \"Packet Diagram\"\nconfig:\n  packet:\n    showBits: false\n---\npacket\n  0-15: \"x\"\n";
        let d = parse(src).unwrap();
        assert_eq!(d.meta.title.as_deref(), Some("Packet Diagram"));
        assert!(!d.config.show_bits);
    }

    #[test]
    fn errors_on_noncontiguous_blocks() {
        let src = "packet\n  0-3: \"a\"\n  5-7: \"b\"\n";
        assert!(parse(src).is_err());
    }

    #[test]
    fn errors_on_end_before_start() {
        let src = "packet\n  5-3: \"oops\"\n";
        assert!(parse(src).is_err());
    }

    #[test]
    fn relative_bits_form_is_contiguous() {
        let src = "packet\n  0-3: \"a\"\n  +4: \"b\"\n";
        let d = parse(src).unwrap();
        assert_eq!(d.fields[1].start, 4);
        assert_eq!(d.fields[1].end, 7);
    }
}
