//! Kanban data model — ported from
//! /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/kanban/kanbanDb.ts
//!
//! A kanban diagram is a flat list of sections (columns), each holding a
//! list of items. Items carry optional `ticket`, `priority`, `assigned`
//! metadata — everything else that upstream `kanbanDb.addNode` supports
//! (icon, cssClasses, shape override, custom label) shows up in no
//! byte-exact fixture, so we only keep the trio we actually render.

use crate::model::DiagramMeta;

/// Priority levels rendered as a coloured stripe on the left edge of an
/// item. Variant order matches upstream's `colorFromPriority` switch —
/// `Medium` explicitly maps to "no stroke" so we skip the stripe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    VeryHigh, // red
    High,     // orange
    Medium,   // no stroke
    Low,      // blue
    VeryLow,  // lightblue
}

impl Priority {
    /// Stroke colour or `None` for `Medium`.
    pub fn stroke(self) -> Option<&'static str> {
        match self {
            Priority::VeryHigh => Some("red"),
            Priority::High => Some("orange"),
            Priority::Medium => None,
            Priority::Low => Some("blue"),
            Priority::VeryLow => Some("lightblue"),
        }
    }
}

/// One item inside a section — the leaf-level node in the kanban tree.
#[derive(Debug, Clone, Default)]
pub struct KanbanItem {
    pub id: String,
    pub label: String,
    pub ticket: Option<String>,
    pub priority: Option<Priority>,
    pub assigned: Option<String>,
}

/// One column in the kanban board.
#[derive(Debug, Clone, Default)]
pub struct KanbanSection {
    pub id: String,
    pub label: String,
    pub items: Vec<KanbanItem>,
}

/// Top-level diagram. `ticket_base_url` comes from the frontmatter
/// (`config.kanban.ticketBaseUrl`) — when present and an item has a
/// `ticket:`, we wrap its ticket label in an `<a>` pointing at
/// `url.replace("#TICKET#", ticket)`.
#[derive(Debug, Clone, Default)]
pub struct KanbanDiagram {
    pub meta: DiagramMeta,
    pub sections: Vec<KanbanSection>,
    pub ticket_base_url: Option<String>,
}
