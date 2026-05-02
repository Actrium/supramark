//! Sequence-diagram layout (scaffold).
//!
//! Upstream reference: `packages/mermaid/src/diagrams/sequence/sequenceRenderer.ts`
//!
//! Computes a coarse skeleton: per-actor X column, message Y rows, and
//! a height/width bounds rectangle. This is intentionally NOT byte
//! exact with upstream — it lets the renderer emit a non-empty SVG so
//! sequence dispatch is wired and the model can be exercised.

use crate::error::Result;
use crate::model::sequence::{DiagramItem, SequenceDiagram};
use crate::theme::ThemeVariables;

type Theme = ThemeVariables;

#[derive(Debug, Clone, Default)]
pub struct ActorCol {
    pub id: String,
    pub description: String,
    pub x: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default)]
pub struct MessageRow {
    pub from: String,
    pub to: String,
    pub text: String,
    pub y: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SequenceLayout {
    pub width: f64,
    pub height: f64,
    pub actors: Vec<ActorCol>,
    pub messages: Vec<MessageRow>,
    pub view_box_x: f64,
    pub view_box_y: f64,
}

pub fn layout(d: &SequenceDiagram, _theme: &Theme) -> Result<SequenceLayout> {
    let cfg = &d.config;
    let mut actors = Vec::with_capacity(d.actors.len());
    let mut x = 0.0_f64;
    for a in &d.actors {
        actors.push(ActorCol {
            id: a.id.clone(),
            description: a.description.clone(),
            x,
            width: cfg.width,
            height: cfg.height,
        });
        x += cfg.width + cfg.actor_margin;
    }
    let total_width = if actors.is_empty() {
        cfg.width
    } else {
        x - cfg.actor_margin
    };

    // Walk top-level items and produce one MessageRow per message.
    let mut messages = Vec::new();
    let mut y = cfg.height + cfg.message_margin;
    walk_items(&d.items, &mut messages, &mut y, cfg.message_margin);

    let total_height = y + cfg.height + cfg.message_margin;
    Ok(SequenceLayout {
        width: total_width,
        height: total_height,
        actors,
        messages,
        view_box_x: -cfg.diagram_margin_x,
        view_box_y: -cfg.diagram_margin_y,
    })
}

fn walk_items(items: &[DiagramItem], out: &mut Vec<MessageRow>, y: &mut f64, step: f64) {
    for it in items {
        match it {
            DiagramItem::Message(m) => {
                out.push(MessageRow {
                    from: m.from.clone(),
                    to: m.to.clone(),
                    text: m.text.clone(),
                    y: *y,
                });
                *y += step;
            }
            DiagramItem::Loop { items, .. }
            | DiagramItem::Opt { items, .. }
            | DiagramItem::Break { items, .. }
            | DiagramItem::Rect { items, .. } => {
                walk_items(items, out, y, step);
            }
            DiagramItem::Alt { branches } | DiagramItem::Critical { branches } => {
                for b in branches {
                    walk_items(&b.items, out, y, step);
                }
            }
            DiagramItem::Par { branches } => {
                for b in branches {
                    walk_items(&b.items, out, y, step);
                }
            }
            DiagramItem::Note(_)
            | DiagramItem::Activate(_)
            | DiagramItem::Deactivate(_)
            | DiagramItem::Create(_)
            | DiagramItem::Destroy(_) => {
                *y += step;
            }
            // Autonumber is a state-update statement — no own visual row.
            DiagramItem::Autonumber { .. } => {}
        }
    }
}
