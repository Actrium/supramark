//! Mindmap diagram model.
//!
//! Mirrors upstream `packages/mermaid/src/diagrams/mindmap/mindmapDb.ts`.
//! A mindmap is a tree rooted at a single node with one of seven shape
//! types. The hierarchy is encoded by indentation; `addNode` re-bases
//! every level so the root sits at `level == 0`.
//!
//! Note on byte-exact parity: upstream renders mindmaps with the
//! `cose-bilkent` force-directed layout (cytoscape extension). The
//! produced node positions depend on the physics simulation, so
//! byte-exact reproduction requires porting that engine. Fixtures are
//! deferred via `tests/known_ignored.txt`.

use crate::model::DiagramMeta;

pub type NodeId = usize;

/// Node shape variants. Numeric values match upstream
/// `mindmapDb.nodeType` so the parser can store them directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MindmapNodeType {
    /// `text` — borderless default.
    Default = 0,
    /// `(text)` — rounded rectangle (also pill / stadium look).
    RoundedRect = 1,
    /// `[text]` — plain rectangle.
    Rect = 2,
    /// `((text))` — circle.
    Circle = 3,
    /// `)text(` — cloud / scallop.
    Cloud = 4,
    /// `))text((` — bang / explosion.
    Bang = 5,
    /// `{{text}}` — hexagon.
    Hexagon = 6,
}

#[derive(Debug, Clone)]
pub struct MindmapNode {
    pub id: NodeId,
    /// Source-declared identifier (before any shape brackets).
    pub node_id: String,
    /// Indentation level, re-based so root is 0.
    pub level: usize,
    /// User-visible label (the text inside the shape brackets, or the
    /// identifier when no brackets were given). Trimmed of leading/trailing
    /// whitespace — used for Default/Rect shapes that wrap text in `<p>`.
    pub descr: String,
    /// Raw inner text for Circle/RoundedRect shapes. Preserves
    /// newlines and indentation so jsdom's multi-line measurement
    /// and span innerHTML match upstream byte-for-byte.
    pub raw_descr: String,
    pub node_type: MindmapNodeType,
    pub children: Vec<NodeId>,
    pub parent: Option<NodeId>,
    pub padding: f64,
    pub width: f64,
    pub is_root: bool,
    /// Optional `::icon(name)` decoration applied via the `**::**` op.
    pub icon: Option<String>,
    /// Optional `:::className` decoration.
    pub class: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct MindmapDiagram {
    pub meta: DiagramMeta,
    /// Flat array of nodes; index 0 is always the root if any node was
    /// declared.
    pub nodes: Vec<MindmapNode>,
    /// Optional layout name lifted from frontmatter `config.layout`.
    /// Upstream defaults to `cose-bilkent`; the `tidy-tree` value seen
    /// in fixture 01 is recognised but currently unimplemented.
    pub layout_name: Option<String>,
    /// Theme name lifted from frontmatter `config.theme`.
    pub theme_override: Option<String>,
}
