//! Treemap diagram model.
//!
//! Mirrors upstream `packages/mermaid/src/diagrams/treemap/types.ts`
//! plus the subset of `db.ts` state that survives the parse stage.
//!
//! Every rendered node lives in a flat `Vec<TreemapNode>` indexed by
//! `NodeId`. The hierarchy is expressed as parent / children indices so
//! we can hand it off to the layout stage without re-walking a string
//! graph. `children` on a non-leaf node is always present (possibly
//! empty). Leaf nodes have `children = None`, consistent with upstream
//! `buildHierarchy` in `utils.ts`.

use crate::model::DiagramMeta;

pub type NodeId = usize;

/// Distinguishes a leaf (has a numeric value) from a section (has
/// children, no own value but sums its children).
#[derive(Debug, Clone, PartialEq)]
pub enum TreemapNodeKind {
    Section,
    Leaf,
}

#[derive(Debug, Clone)]
pub struct TreemapNode {
    pub id: NodeId,
    pub name: String,
    pub kind: TreemapNodeKind,
    /// Leaf-only: raw value parsed from the source.
    pub value: Option<f64>,
    /// Parent id, or `None` for the synthetic root.
    pub parent: Option<NodeId>,
    /// Section-only: child node ids in declaration order. `None` for leaves.
    pub children: Option<Vec<NodeId>>,
    /// Optional class selector from `:::className` syntax.
    pub class_selector: Option<String>,
    /// Compiled styles from classDef (same order as `classDef` directive).
    pub css_compiled_styles: Vec<String>,
    /// Source-declared nesting depth (d3 `node.depth`), root = 0.
    pub depth: usize,
}

/// One `classDef <name> <style>;` statement. Styles are semicolon-
/// separated declarations; the original comma / semicolon escape dance
/// from upstream (`\,` → `§§§` → `,` etc.) is applied at parse time.
#[derive(Debug, Clone, Default)]
pub struct TreemapClassDef {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TreemapConfig {
    /// D3-format string applied to leaf values. Default `","` (grouped
    /// thousands, no decimals).
    pub value_format: Option<String>,
    /// When `false`, value text elements are omitted entirely. Defaults
    /// to `true` (upstream treats `undefined` as `true`).
    pub show_values: Option<bool>,
    /// Padding inside each section (upstream `treemap.padding`).
    pub padding: Option<f64>,
    /// Width override as multiples of section inner padding. Unused in
    /// fixtures but preserved for parity.
    pub node_width: Option<f64>,
    pub node_height: Option<f64>,
    /// Whether the SVG should be rendered at its container's max width
    /// (affects the `width="100%"` vs explicit pixel width attribute).
    pub use_max_width: Option<bool>,
    /// Outer diagram padding — upstream default 8.
    pub diagram_padding: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct TreemapDiagram {
    pub meta: DiagramMeta,
    pub nodes: Vec<TreemapNode>,
    /// Ids of the nodes that sit at source-level 0. In upstream these
    /// become the `children` of the synthetic `{ name: '', ... }` root.
    pub outer_nodes: Vec<NodeId>,
    /// Named classDef blocks in declaration order.
    pub classes: Vec<TreemapClassDef>,
    /// Per-diagram config merged from frontmatter `config:` + init directives.
    pub config: TreemapConfig,
    /// Theme name lifted from `config:` / `%%{init}` frontmatter. `None`
    /// means use whatever `preprocess` resolved at the top level.
    pub theme_override: Option<String>,
}
