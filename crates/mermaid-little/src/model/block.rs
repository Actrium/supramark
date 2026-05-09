//! Block diagram AST.
//!
//! Upstream reference:
//! `packages/mermaid/src/diagrams/block/blockTypes.ts` and
//! `packages/mermaid/src/diagrams/block/blockDB.ts`.
//!
//! The parser produces a tree of [`BlockNode`]s rooted at a synthetic
//! composite (`id = "root"`). Leaves carry a [`BlockShape`] (square /
//! round / circle / …) and an optional label; composites carry a
//! vector of children and an optional `columns` override.

use crate::model::DiagramMeta;

/// A block diagram — frontmatter/accessibility metadata + parsed
/// hierarchy + edge list + classDef/style tables.
#[derive(Debug, Clone, Default)]
pub struct BlockDiagram {
    pub meta: DiagramMeta,
    /// Root composite. Always present; its `children` are the top-level
    /// statements.
    pub root: BlockNode,
    /// Flat edge list in declaration order.
    pub edges: Vec<BlockEdge>,
    /// `classDef` definitions: id → css attribute string.
    pub class_defs: Vec<ClassDef>,
}

/// A single block (leaf or composite). Maps to upstream `Block` in
/// `blockTypes.ts`.
#[derive(Debug, Clone, Default)]
pub struct BlockNode {
    pub id: String,
    pub label: Option<String>,
    pub shape: BlockShape,
    pub width_in_columns: i64,
    /// Explicit `columns N` directive on a composite; 0 means unset
    /// (falls back to auto / child count).
    pub columns: Option<i64>,
    pub children: Vec<BlockNode>,
    /// `space:N` widths — number of cells to span with empty content.
    /// Only meaningful when `shape == BlockShape::Space`.
    pub space_width: i64,
    /// Classes applied via `class X foo` statements.
    pub classes: Vec<String>,
    /// Inline style attrs from `style X fill:...`.
    pub styles: Vec<String>,
    /// `<["label"]>(dir, ...)` arrow cells. `None` for non-arrow blocks.
    pub arrow_dirs: Option<Vec<ArrowDir>>,
}

/// Upstream shape enum — mirrors `typeStr2Type` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockShape {
    /// `A` with no brackets — default square rect with `rx=0`.
    Na,
    /// `A[text]` — square rect `rx=0`.
    Square,
    /// `A(text)` — rounded rect `rx=5`.
    Round,
    /// `A((text))` — circle.
    Circle,
    /// `A(((text)))` — double circle.
    DoubleCircle,
    /// `A[(text)]` — cylinder.
    Cylinder,
    /// `A([text])` — stadium.
    Stadium,
    /// `A[[text]]` — subroutine.
    Subroutine,
    /// `A{text}` — diamond / question.
    Diamond,
    /// `A{{text}}` — hexagon.
    Hexagon,
    /// `A[/text/]` — parallelogram leaning right.
    LeanRight,
    /// `A[\text\]` — parallelogram leaning left.
    LeanLeft,
    /// `A[/text\]` — trapezoid.
    Trapezoid,
    /// `A[\text/]` — inverted trapezoid.
    InvTrapezoid,
    /// `A>text]` — rect with inverted left arrow (odd).
    RectLeftInvArrow,
    /// `A<[text]>(dir)` — block arrow.
    BlockArrow,
    /// Composite wrapper (`block ... end`, nested or root).
    Composite,
    /// `space` / `space:N` — empty cell(s).
    Space,
}

impl Default for BlockShape {
    fn default() -> Self {
        BlockShape::Na
    }
}

/// An edge between two blocks. Maps to upstream `Block { type: 'edge' }`.
#[derive(Debug, Clone, Default)]
pub struct BlockEdge {
    pub id: String,
    pub start: String,
    pub end: String,
    pub label: Option<String>,
    /// `'arrow_point'` / `'arrow_cross'` / `'arrow_circle'` / `''`.
    pub arrow_type_end: String,
    /// Always `'arrow_open'` in current upstream.
    pub arrow_type_start: String,
}

/// Direction token inside a `<[...]>(dir)` block arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowDir {
    Up,
    Down,
    Left,
    Right,
    X,
    Y,
}

/// `classDef name css-attributes`.
#[derive(Debug, Clone, Default)]
pub struct ClassDef {
    pub id: String,
    pub styles: String,
}
