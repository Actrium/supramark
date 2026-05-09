//! Kanban parser — ported from
//! /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/kanban/parser/kanban.jison
//! and the companion `kanbanDb.addNode` logic.
//!
//! Grammar we support — enough for every fixture in
//! tests/ext_fixtures/cypress/kanban:
//!
//! ```text
//! ---
//! config:
//!   kanban:
//!     ticketBaseUrl: 'https://.../#TICKET#'
//! ---
//! kanban
//!   <id>[<label>]                    -- section (indent level = 2 spaces)
//!     <id>[<label>]                  -- item   (indent level = 4+ spaces)
//!     <id>[<label>]@{ ticket: X, priority: 'High', assigned: 'Y' }
//! ```
//!
//! Indentation is significant. Upstream's jison grammar tracks level
//! through `SPACELIST.length` — the **first** non-`kanban` line sets the
//! section indent, and items are anything with a **deeper** indent. All
//! fixture sources use exactly `  ` (2 sp) for sections and `    ` (4 sp)
//! for items, so we hardcode that distinction.

use crate::error::{MermaidError, Result};
use crate::model::kanban::{KanbanDiagram, KanbanItem, KanbanSection, Priority};
use crate::model::DiagramMeta;

/// Parse a kanban source (including any leading YAML frontmatter) into
/// a [`KanbanDiagram`].
pub fn parse(source: &str) -> Result<KanbanDiagram> {
    // 1. Pull frontmatter (for `config.kanban.ticketBaseUrl`). We reuse
    //    the pipeline's YAML frontmatter stripper so the body we parse
    //    lines up with what `detect` sees.
    let src = source.replace("\r\n", "\n").replace('\r', "\n");
    let (ticket_base_url, body) = extract_ticket_base_url(&src);

    let mut diagram = KanbanDiagram {
        meta: DiagramMeta::default(),
        sections: Vec::new(),
        ticket_base_url,
    };

    // 2. Consume the `kanban` header line.
    let mut lines = body.lines();
    let mut saw_header = false;
    for raw in lines.by_ref() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "kanban" || trimmed.starts_with("kanban ") {
            saw_header = true;
            break;
        }
        return Err(MermaidError::Unsupported(format!(
            "kanban parser: expected `kanban` header, got {trimmed:?}"
        )));
    }
    if !saw_header {
        return Err(MermaidError::Unsupported(
            "kanban parser: no `kanban` keyword in source".into(),
        ));
    }

    // 3. Consume the body. First non-header line's leading-space count is
    //    the section indent; anything deeper is an item under the most
    //    recent section.
    let mut section_indent: Option<usize> = None;
    for raw in lines {
        if raw.trim().is_empty() {
            continue;
        }
        let indent = leading_spaces(raw);
        let rest = &raw[indent..];

        let sec_i = *section_indent.get_or_insert(indent);
        if indent == sec_i {
            let (id, label, meta) = parse_node(rest)?;
            // `addNode` in upstream ignores `@{...}` metadata for
            // sections (only items get ticket/priority/assigned), so we
            // silently drop it here too.
            let _ = meta;
            diagram.sections.push(KanbanSection {
                id,
                label,
                items: Vec::new(),
            });
        } else if indent > sec_i {
            let section = diagram.sections.last_mut().ok_or_else(|| {
                MermaidError::Unsupported("kanban parser: item without a preceding section".into())
            })?;
            let (id, label, meta) = parse_node(rest)?;
            let (ticket, priority, assigned) = meta;
            section.items.push(KanbanItem {
                id,
                label,
                ticket,
                priority,
                assigned,
            });
        } else {
            return Err(MermaidError::Unsupported(format!(
                "kanban parser: unexpected outdent on line {raw:?}"
            )));
        }
    }

    Ok(diagram)
}

/// Count leading ASCII spaces (no tabs — upstream rejects tabs too).
fn leading_spaces(s: &str) -> usize {
    s.bytes().take_while(|&b| b == b' ').count()
}

/// Parse a single node line: `id[label]@{ meta }` or `id[label]` or bare
/// `id` (fixture 02 uses `id2` with no label). Returns
/// `(id, label, (ticket, priority, assigned))`.
#[allow(clippy::type_complexity)]
fn parse_node(
    s: &str,
) -> Result<(
    String,
    String,
    (Option<String>, Option<Priority>, Option<String>),
)> {
    // Split off `@{ ... }` shape-data suffix first so the bracket parse
    // below can't mistake a `]` inside the YAML for the label delimiter.
    let (core, shape_data) = match s.find("@{") {
        Some(i) => {
            let end = s[i..].find('}').map(|e| i + e + 1).ok_or_else(|| {
                MermaidError::Unsupported(format!("kanban parser: unterminated @{{ in {s:?}"))
            })?;
            (s[..i].trim_end(), Some(&s[i + 2..end - 1]))
        }
        None => (s, None),
    };

    let (id, label) = match core.find('[') {
        Some(i) => {
            let j = core.rfind(']').ok_or_else(|| {
                MermaidError::Unsupported(format!("kanban parser: unterminated [ in {core:?}"))
            })?;
            let id = core[..i].trim().to_string();
            let label = core[i + 1..j].to_string();
            (id, label)
        }
        None => (core.trim().to_string(), core.trim().to_string()),
    };

    let meta = match shape_data {
        Some(sd) => parse_shape_data(sd),
        None => (None, None, None),
    };

    Ok((id, label, meta))
}

/// Parse the YAML-ish shape-data body: `key: value, key: 'value', ...`.
/// Only three keys are meaningful for rendering.
fn parse_shape_data(body: &str) -> (Option<String>, Option<Priority>, Option<String>) {
    let mut ticket = None;
    let mut priority = None;
    let mut assigned = None;

    for raw in split_top_level_commas(body) {
        let Some(colon) = raw.find(':') else { continue };
        let key = raw[..colon].trim();
        let val = strip_quotes(raw[colon + 1..].trim());
        match key {
            "ticket" => ticket = Some(val.to_string()),
            "priority" => priority = parse_priority(val),
            "assigned" => assigned = Some(val.to_string()),
            _ => {}
        }
    }

    (ticket, priority, assigned)
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_s = false;
    let mut in_d = false;
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_d => in_s = !in_s,
            b'"' if !in_s => in_d = !in_d,
            b'{' | b'[' if !in_s && !in_d => depth += 1,
            b'}' | b']' if !in_s && !in_d => depth -= 1,
            b',' if depth == 0 && !in_s && !in_d => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')))
    {
        return &s[1..s.len() - 1];
    }
    s
}

fn parse_priority(s: &str) -> Option<Priority> {
    match s {
        "Very High" => Some(Priority::VeryHigh),
        "High" => Some(Priority::High),
        "Medium" => Some(Priority::Medium),
        "Low" => Some(Priority::Low),
        "Very Low" => Some(Priority::VeryLow),
        _ => None,
    }
}

/// Minimal frontmatter extractor — just enough to fish out
/// `config.kanban.ticketBaseUrl`. Anything richer goes through the
/// global preprocess pipeline.
fn extract_ticket_base_url(source: &str) -> (Option<String>, &str) {
    let rest = source.trim_start_matches('\n');
    if !rest.starts_with("---") {
        return (None, source);
    }
    // Find closing `---\n`.
    let after_open = &rest[3..];
    let after_open = after_open.trim_start_matches(['\r', '\n', ' ']);
    // Find the next `\n---` followed by newline or EOF.
    let close_rel = match after_open.find("\n---") {
        Some(i) => i,
        None => return (None, source),
    };
    let yaml_body = &after_open[..close_rel];
    let after_close = &after_open[close_rel + 4..];
    let after_close = after_close.trim_start_matches(['\r', '\n', ' ']);

    // Simple line-based scan — upstream fixture indents ticketBaseUrl
    // under `config: / kanban: /`. We don't need full YAML here.
    let mut in_config = false;
    let mut in_kanban = false;
    let mut base_url: Option<String> = None;
    for line in yaml_body.lines() {
        let indent = leading_spaces(line);
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if indent == 0 {
            in_config = trimmed.trim_end_matches(':') == "config";
            in_kanban = false;
            continue;
        }
        if in_config && indent == 2 {
            in_kanban = trimmed.trim_end_matches(':') == "kanban";
            continue;
        }
        if in_config && in_kanban && indent >= 4 {
            if let Some(rest) = trimmed.strip_prefix("ticketBaseUrl:") {
                base_url = Some(strip_quotes(rest.trim()).to_string());
            }
        }
    }

    (base_url, after_close)
}
