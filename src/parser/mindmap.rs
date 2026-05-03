//! Mindmap parser.
//!
//! Port of upstream `packages/mermaid/src/diagrams/mindmap/parser/
//! mindmap.jison`. The grammar is whitespace-sensitive: leading-space
//! count drives the parent / child hierarchy, mirroring `addNode`'s
//! `getParent(level)` walk in `mindmapDb.ts`.
//!
//! Shape syntax:
//!   * `[text]`            - rectangle
//!   * `(text)`            - rounded rectangle
//!   * `((text))`          - circle
//!   * `)text(`            - cloud
//!   * `))text((`          - bang
//!   * `{{text}}`          - hexagon
//!   * `text`              - default (no border)
//!
//! Only the subset needed by the model layer is implemented; layout /
//! render are stubs pending cose-bilkent layout port. See
//! `tests/known_ignored.txt` for fixture status.

use crate::config::frontmatter;
use crate::error::{MermaidError, Result};
use crate::model::mindmap::{MindmapDiagram, MindmapNode, MindmapNodeType, NodeId};

const DEFAULT_PADDING: f64 = 10.0;
const DEFAULT_MAX_NODE_WIDTH: f64 = 200.0;

pub fn parse(source: &str) -> Result<MindmapDiagram> {
    let mut diag = MindmapDiagram::default();

    // Strip frontmatter and capture relevant config fields.
    let (fm, body) = frontmatter::parse_frontmatter(source);
    if let Some(meta) = fm {
        diag.meta.title = meta.title;
        if let Some(cfg) = meta.config.as_ref() {
            diag.theme_override = cfg.theme.clone();
            diag.layout_name = cfg.layout.clone();
        }
    }

    // Locate the `mindmap` header.
    let mut lines = body.lines().peekable();
    let mut found_header = false;
    while let Some(line) = lines.peek() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            lines.next();
            continue;
        }
        if trimmed == "mindmap" || trimmed.starts_with("mindmap ") {
            lines.next();
            found_header = true;
            break;
        }
        break;
    }
    if !found_header {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: "missing mindmap header".into(),
        });
    }

    // Track per-line indentation to recover the hierarchy. Upstream
    // `mindmapDb.addNode` re-bases the very first node's indent to
    // level 0, then subtracts that base from every subsequent node.
    let mut base_level: Option<usize> = None;
    let mut buffer = String::new();
    let mut buffer_indent: Option<usize> = None;
    let mut continuation_open: Option<char> = None;

    for raw in lines {
        // Multi-line node bodies open with `(`, `[`, or `((` and stay
        // open until the matching close token. We greedily concatenate
        // continuation lines until balanced.
        if continuation_open.is_some() {
            buffer.push('\n');
            buffer.push_str(raw);
            if line_closes(raw, continuation_open.unwrap()) {
                process_logical_line(&buffer, buffer_indent.unwrap(), &mut diag, &mut base_level)?;
                buffer.clear();
                buffer_indent = None;
                continuation_open = None;
            }
            continue;
        }

        let trimmed = raw.trim_end();
        if trimmed.trim().is_empty() || trimmed.trim_start().starts_with("%%") {
            continue;
        }
        let indent = leading_ws(raw);
        if let Some(open) = unbalanced_open(trimmed) {
            buffer.clear();
            buffer.push_str(raw);
            buffer_indent = Some(indent);
            continuation_open = Some(open);
            continue;
        }
        process_logical_line(trimmed, indent, &mut diag, &mut base_level)?;
    }

    // Drain any unfinished buffer (mismatched bracket — best effort).
    if !buffer.is_empty() {
        if let Some(ind) = buffer_indent {
            process_logical_line(&buffer, ind, &mut diag, &mut base_level)?;
        }
    }

    Ok(diag)
}

fn leading_ws(s: &str) -> usize {
    s.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count()
}

/// Find the *opening* bracket that starts a multi-line node body, or
/// `None` if the line is balanced. We return the close character we're
/// waiting for (`)`, `]`, etc.).
fn unbalanced_open(line: &str) -> Option<char> {
    // Look only for the *last* unclosed opener in the line.
    let s = line.trim_end();
    // Quick scan: count bracket pairs.
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut depth_brace = 0i32;
    for c in s.chars() {
        match c {
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            '[' => depth_brack += 1,
            ']' => depth_brack -= 1,
            '{' => depth_brace += 1,
            '}' => depth_brace -= 1,
            _ => {}
        }
    }
    if depth_brack > 0 {
        return Some(']');
    }
    if depth_brace > 0 {
        return Some('}');
    }
    if depth_paren > 0 {
        return Some(')');
    }
    None
}

fn line_closes(line: &str, target: char) -> bool {
    line.contains(target)
}

/// Decide which token kind the trimmed line represents and dispatch to
/// the right handler.
fn process_logical_line(
    raw: &str,
    indent: usize,
    diag: &mut MindmapDiagram,
    base_level: &mut Option<usize>,
) -> Result<()> {
    let trimmed = raw.trim_start();
    if trimmed.is_empty() {
        return Ok(());
    }

    // `:::class` decoration on its own line attaches to the most recent
    // node. Same for `::icon(...)`.
    if let Some(rest) = trimmed.strip_prefix(":::") {
        if let Some(last) = diag.nodes.last_mut() {
            last.class = Some(rest.trim().to_string());
        }
        return Ok(());
    }
    if let Some(rest) = trimmed.strip_prefix("::icon(") {
        if let Some(end) = rest.find(')') {
            let icon = &rest[..end];
            if let Some(last) = diag.nodes.last_mut() {
                last.icon = Some(icon.to_string());
            }
        }
        return Ok(());
    }

    let (id, descr, raw_descr, ty) = parse_node_token(trimmed);
    add_node(diag, indent, id, descr, raw_descr, ty, base_level);
    Ok(())
}

/// Parse one node token, returning `(id, description, raw_descr, type)`.
fn parse_node_token(s: &str) -> (String, String, String, MindmapNodeType) {
    for (open, close, ty) in [
        ("((", "))", MindmapNodeType::Circle),
        ("))", "((", MindmapNodeType::Bang),
        ("{{", "}}", MindmapNodeType::Hexagon),
        (")", "(", MindmapNodeType::Cloud),
    ] {
        if let Some(rest) = s.find(open) {
            if let Some(end) = s.rfind(close) {
                if end > rest + open.len() {
                    let id = s[..rest].trim().to_string();
                    let inner = &s[rest + open.len()..end];
                    let descr = strip_markdown_quotes(&inner.trim());
                    let raw_descr = inner.trim_start_matches('\n').to_string();
                    let id = if id.is_empty() { descr.clone() } else { id };
                    return (id, descr, raw_descr, ty);
                }
            }
        }
    }
    if let Some(open_idx) = s.find('[') {
        if let Some(close_idx) = s.rfind(']') {
            if close_idx > open_idx {
                let id = s[..open_idx].trim().to_string();
                let descr = strip_markdown_quotes(&s[open_idx + 1..close_idx].trim());
                let raw_descr = descr.clone();
                let id = if id.is_empty() { descr.clone() } else { id };
                return (id, descr, raw_descr, MindmapNodeType::Rect);
            }
        }
    }
    if let Some(open_idx) = s.find('(') {
        if let Some(close_idx) = s.rfind(')') {
            if close_idx > open_idx {
                let id = s[..open_idx].trim().to_string();
                let inner = &s[open_idx + 1..close_idx];
                let descr = strip_markdown_quotes(&inner.trim());
                let raw_descr = inner.trim_start_matches('\n').to_string();
                let id = if id.is_empty() { descr.clone() } else { id };
                return (id, descr, raw_descr, MindmapNodeType::RoundedRect);
            }
        }
    }
    let id = s.trim().to_string();
    (id.clone(), id.clone(), id, MindmapNodeType::Default)
}

/// Strip backtick-fenced markdown wrappers (`` `text` ``). Upstream's
/// jison NSTR2 state captures the inner text only, which is what we
/// reproduce here for byte-parity of `descr`.
fn strip_markdown_quotes(s: &str) -> String {
    let trimmed = s.trim();
    if let Some(inner) = trimmed.strip_prefix('`').and_then(|t| t.strip_suffix('`')) {
        inner.to_string()
    } else if let Some(inner) = trimmed.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        inner.to_string()
    } else {
        trimmed.to_string()
    }
}

fn add_node(
    diag: &mut MindmapDiagram,
    raw_indent: usize,
    id: String,
    descr: String,
    raw_descr: String,
    ty: MindmapNodeType,
    base_level: &mut Option<usize>,
) {
    let level = if diag.nodes.is_empty() {
        *base_level = Some(raw_indent);
        0
    } else {
        let base = base_level.unwrap_or(0);
        raw_indent.saturating_sub(base)
    };

    let mut padding = DEFAULT_PADDING;
    if matches!(
        ty,
        MindmapNodeType::Rect | MindmapNodeType::RoundedRect | MindmapNodeType::Hexagon
    ) {
        padding *= 2.0;
    }

    let parent = find_parent(&diag.nodes, level);
    let new_id: NodeId = diag.nodes.len();
    let node = MindmapNode {
        id: new_id,
        node_id: id,
        level,
        descr,
        raw_descr,
        node_type: ty,
        children: Vec::new(),
        parent,
        padding,
        width: DEFAULT_MAX_NODE_WIDTH,
        is_root: parent.is_none() && diag.nodes.is_empty(),
        icon: None,
        class: None,
    };
    if let Some(p) = parent {
        diag.nodes[p].children.push(new_id);
    }
    diag.nodes.push(node);
}

fn find_parent(nodes: &[MindmapNode], level: usize) -> Option<NodeId> {
    nodes.iter().rev().find(|n| n.level < level).map(|n| n.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_root() {
        let d = parse("mindmap\n  root\n").unwrap();
        assert_eq!(d.nodes.len(), 1);
        assert!(d.nodes[0].is_root);
        assert_eq!(d.nodes[0].descr, "root");
    }

    #[test]
    fn parses_root_with_two_children() {
        let d = parse("mindmap\nroot((mindmap))\n  A\n  B\n").unwrap();
        assert_eq!(d.nodes.len(), 3);
        assert_eq!(d.nodes[0].descr, "mindmap");
        assert_eq!(d.nodes[0].node_type, MindmapNodeType::Circle);
        assert_eq!(d.nodes[0].children.len(), 2);
    }

    #[test]
    fn parses_rect_root() {
        let d = parse("mindmap\nroot[root]\n").unwrap();
        assert_eq!(d.nodes.len(), 1);
        assert_eq!(d.nodes[0].node_type, MindmapNodeType::Rect);
    }
}
