//! Sankey diagram parser.
//!
//! Upstream grammar: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/sankey/parser/sankey.jison
//!
//! * A leading `sankey-beta` or `sankey` keyword consumes its line.
//! * Each subsequent non-blank line is a CSV record `source,target,value`.
//! * CSV escapes: a field wrapped in double-quotes can contain any
//!   character including `\n`, `"` doubles as `""`.
//! * Frontmatter (`---\nconfig:\n  sankey:\n    ...\n---`) carries all
//!   user-level configuration. We parse it inline here because the
//!   Wave 0 preprocess pipeline normalises it but doesn't expose the
//!   `sankey` sub-block as a typed struct.

use crate::error::{MermaidError, Result};
use crate::model::sankey::{LinkColor, NodeAlignment, SankeyConfig, SankeyDiagram, SankeyLink};

pub fn parse(source: &str) -> Result<SankeyDiagram> {
    // Split off the YAML frontmatter, keeping it raw so we can
    // cherry-pick `config.sankey` without caring about the rest.
    let (config, after_fm) = extract_frontmatter_config(source);

    let mut d = SankeyDiagram {
        config,
        ..SankeyDiagram::default()
    };

    // Tokenise the body. The jison grammar is effectively: `SANKEY
    // NEWLINE (record NEWLINE)* EOF`. We reuse a light-weight CSV
    // tokeniser that handles escaped fields (we don't see any in the
    // current fixtures, but they are cheap to support).
    let body = after_fm.trim_start_matches(['\n', '\r']);
    let mut lexer = Lexer::new(body);

    // Header: `sankey-beta` or `sankey`, then newline.
    let header = lexer.take_line_raw();
    let header_trimmed = header.trim();
    if header_trimmed != "sankey-beta" && header_trimmed != "sankey" {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: format!("expected 'sankey-beta' or 'sankey' header, got {header_trimmed:?}"),
        });
    }

    let mut line_no = 2usize;
    while !lexer.eof() {
        let raw = lexer.take_line_raw();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            line_no += 1;
            continue;
        }
        // Parse the CSV record using the raw line (which may be
        // multi-line if an escaped field contained a newline — not
        // currently exercised by fixtures, but the grammar allows it).
        let (source_field, target_field, value_field) =
            parse_csv_record(trimmed).ok_or_else(|| MermaidError::Parse {
                line: line_no,
                col: 1,
                message: format!("expected CSV record with 3 fields, got {trimmed:?}"),
            })?;

        let value: f64 = value_field
            .trim()
            .parse()
            .map_err(|_| MermaidError::Parse {
                line: line_no,
                col: 1,
                message: format!("invalid sankey value {value_field:?}"),
            })?;

        // Upstream `findOrCreateNode` tracks first-occurrence in a map
        // AND pushes into `nodes`. We preserve the same ordering.
        if !d.nodes.iter().any(|n| n == &source_field) {
            d.nodes.push(source_field.clone());
        }
        if !d.nodes.iter().any(|n| n == &target_field) {
            d.nodes.push(target_field.clone());
        }

        d.links.push(SankeyLink {
            source: source_field,
            target: target_field,
            value,
        });

        line_no += 1;
    }

    Ok(d)
}

// -------------------------------------------------------------------------------------------------
// Frontmatter config extraction.
// -------------------------------------------------------------------------------------------------

/// Pull the `config.sankey:` block (if any) out of a YAML frontmatter
/// header and return the merged config + the source text that follows
/// the frontmatter.
fn extract_frontmatter_config(source: &str) -> (SankeyConfig, &str) {
    let mut cfg = SankeyConfig::default();

    // The frontmatter is delimited by lines consisting of exactly
    // `---` (upstream regex `^-{3}\s*[\n\r](.*?)[\n\r]-{3}\s*[\n\r]+`).
    // Accept leading whitespace on the opening/closing line so sloppy
    // fixtures (e.g. ` ---` typed with an accidental space) still parse.
    let src = source.trim_start_matches('\u{feff}');
    let rest = src;
    let after_opening = match skip_frontmatter_marker(rest) {
        Some(r) => r,
        None => return (cfg, source),
    };

    let (body, after_body) = match find_frontmatter_end(after_opening) {
        Some(v) => v,
        None => return (cfg, source),
    };

    apply_sankey_config(body, &mut cfg);

    (cfg, after_body)
}

/// Accept an optional-leading-whitespace `---` line. Returns the slice
/// after the terminating newline on success.
fn skip_frontmatter_marker(s: &str) -> Option<&str> {
    let trimmed = s.trim_start_matches([' ', '\t']);
    if !trimmed.starts_with("---") {
        return None;
    }
    // Must be followed by whitespace + newline (no other chars).
    let after_dashes = &trimmed[3..];
    let mut idx = 0;
    for c in after_dashes.chars() {
        if c == '\n' {
            idx += c.len_utf8();
            break;
        }
        if c == ' ' || c == '\t' || c == '\r' {
            idx += c.len_utf8();
            continue;
        }
        return None;
    }
    Some(&after_dashes[idx..])
}

/// Locate the closing `---` line, honouring leading whitespace per
/// observed fixtures (demo/sankey/01.mmd has ` config:` indented).
fn find_frontmatter_end(body: &str) -> Option<(&str, &str)> {
    let mut line_start = 0usize;
    let bytes = body.as_bytes();
    let mut i = 0;
    while i <= bytes.len() {
        if i == bytes.len() || bytes[i] == b'\n' {
            let line = &body[line_start..i];
            let trimmed = line.trim_start_matches([' ', '\t']);
            if trimmed == "---"
                || trimmed
                    .strip_prefix("---")
                    .map(|r| r.chars().all(|c| c == ' ' || c == '\t' || c == '\r'))
                    .unwrap_or(false)
            {
                let body_slice = &body[..line_start];
                let after = if i < bytes.len() { &body[i + 1..] } else { "" };
                // Consume any additional leading newlines after the
                // closing marker (upstream regex eats `[\n\r]+`).
                let after = after.trim_start_matches(['\n', '\r']);
                return Some((body_slice, after));
            }
            line_start = i + 1;
        }
        i += 1;
    }
    None
}

/// Walk the YAML body line-by-line looking for `config:` → `sankey:`
/// and pull out the handful of keys we care about. We deliberately
/// avoid `serde_yml` here to sidestep the dependency on the Wave-0
/// frontmatter module — it already consumed the top-level `title` and
/// `config` but produced only a generic `Config` value.
fn apply_sankey_config(body: &str, cfg: &mut SankeyConfig) {
    // Find the indentation of the `sankey:` sub-block, then collect
    // every child line whose indent is strictly greater.
    let mut lines: Vec<(usize, &str)> = Vec::new();
    for raw in body.lines() {
        let indent = raw.chars().take_while(|c| *c == ' ').count();
        let content = &raw[indent..];
        if content.is_empty() {
            continue;
        }
        lines.push((indent, content));
    }

    // Locate `config:` at some indent, then `sankey:` at a greater
    // indent, then child keys at a yet-greater indent.
    let mut sankey_indent: Option<usize> = None;
    let mut inside_sankey = false;
    let mut inside_config = false;
    let mut config_indent: Option<usize> = None;
    for (ind, content) in &lines {
        if !inside_config {
            if content.trim_end_matches(|c: char| c.is_whitespace()) == "config:" {
                inside_config = true;
                config_indent = Some(*ind);
            }
            continue;
        }
        let cind = config_indent.unwrap();
        if *ind <= cind {
            // Left the `config:` block. Stop scanning.
            break;
        }
        if !inside_sankey {
            if content.trim_end_matches(|c: char| c.is_whitespace()) == "sankey:" {
                inside_sankey = true;
                sankey_indent = Some(*ind);
            }
            continue;
        }
        let sind = sankey_indent.unwrap();
        if *ind <= sind {
            // Left the `sankey:` block.
            break;
        }
        // Child key: `key: value`.
        if let Some(colon) = content.find(':') {
            let key = content[..colon].trim();
            let value = content[colon + 1..].trim();
            apply_sankey_kv(key, value, cfg);
        }
    }
}

fn apply_sankey_kv(key: &str, value: &str, cfg: &mut SankeyConfig) {
    // Strip surrounding quotes if present — YAML allows either.
    let val = value
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(value);
    match key {
        "width" => {
            if let Ok(n) = val.parse::<f64>() {
                cfg.width = n;
            }
        }
        "height" => {
            if let Ok(n) = val.parse::<f64>() {
                cfg.height = Some(n);
            }
        }
        "useMaxWidth" => {
            cfg.use_max_width = matches!(val, "true" | "True" | "TRUE");
        }
        "showValues" => {
            cfg.show_values = !matches!(val, "false" | "False" | "FALSE");
        }
        "prefix" => {
            cfg.prefix = val.to_string();
        }
        "suffix" => {
            cfg.suffix = val.to_string();
        }
        "nodeAlignment" => {
            cfg.node_alignment = match val {
                "left" => NodeAlignment::Left,
                "right" => NodeAlignment::Right,
                "center" => NodeAlignment::Center,
                _ => NodeAlignment::Justify,
            };
        }
        "linkColor" => {
            cfg.link_color = match val {
                "gradient" => LinkColor::Gradient,
                "source" => LinkColor::Source,
                "target" => LinkColor::Target,
                other => LinkColor::Custom(other.to_string()),
            };
        }
        _ => {}
    }
}

// -------------------------------------------------------------------------------------------------
// CSV tokeniser (kept trivial — fixtures only use unescaped fields).
// -------------------------------------------------------------------------------------------------

struct Lexer<'s> {
    src: &'s str,
    pos: usize,
}

impl<'s> Lexer<'s> {
    fn new(src: &'s str) -> Self {
        Lexer { src, pos: 0 }
    }

    fn eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn take_line_raw(&mut self) -> &'s str {
        let rest = &self.src[self.pos..];
        match rest.find('\n') {
            Some(nl) => {
                let line = &rest[..nl];
                self.pos += nl + 1;
                line.trim_end_matches('\r')
            }
            None => {
                let line = rest;
                self.pos = self.src.len();
                line.trim_end_matches('\r')
            }
        }
    }
}

/// Split a single-line CSV record into three (source, target, value)
/// fields. Honours upstream CSV rules: unquoted fields may not contain
/// `,`, quoted fields may contain anything and `""` unescapes to `"`.
fn parse_csv_record(line: &str) -> Option<(String, String, String)> {
    let mut fields: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    let mut quoted_field = false;
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(c);
            }
        } else if c == ',' {
            let val = if quoted_field {
                std::mem::take(&mut current)
            } else {
                current.trim().to_string()
            };
            current.clear();
            fields.push(val);
            quoted_field = false;
        } else if c == '"' && current.trim().is_empty() {
            current.clear();
            in_quotes = true;
            quoted_field = true;
        } else {
            current.push(c);
        }
    }
    let last = if quoted_field {
        current
    } else {
        current.trim().to_string()
    };
    fields.push(last);
    if fields.len() != 3 {
        return None;
    }
    let value = fields.pop().unwrap();
    let target = fields.pop().unwrap();
    let source = fields.pop().unwrap();
    Some((source, target, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_sankey_beta() {
        let src = "sankey-beta\n\nA,B,10\n";
        let d = parse(src).unwrap();
        assert_eq!(d.nodes, vec!["A", "B"]);
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].value, 10.0);
    }

    #[test]
    fn frontmatter_config_sankey() {
        let src = "---\n config:\n   sankey:\n     showValues: true\n     prefix: $\n     suffix: B\n     width: 800\n     nodeAlignment: left\n---\nsankey\n   a,b,8\n";
        let d = parse(src).unwrap();
        assert!(d.config.show_values);
        assert_eq!(d.config.prefix, "$");
        assert_eq!(d.config.suffix, "B");
        assert_eq!(d.config.width, 800.0);
        assert_eq!(d.config.node_alignment, NodeAlignment::Left);
    }
}
