//! Venn diagram parsed model.
//!
//! Upstream: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/venn/
//! Grammar: venn.jison (terminals SET / UNION / TEXT / STYLE / TITLE)
//!
//! The parser preserves the insertion order of `addSubsetData` calls
//! (one per `set ...` / `union ...` line). Set ids and union member
//! lists are alphabetically sorted at insertion time, matching the
//! upstream `vennDB.addSubsetData` which calls `.sort()` before storing.

use crate::model::DiagramMeta;

/// Parsed venn diagram.
#[derive(Debug, Clone, Default)]
pub struct VennDiagram {
    pub meta: DiagramMeta,
    /// Insertion-ordered subsets (sets + unions, mixed by source order).
    pub subsets: Vec<VennSubset>,
    /// Insertion-ordered text nodes (`text "label"` under a set/union).
    pub text_nodes: Vec<VennTextNode>,
    /// Style overrides: `style A,B fill:#fff` etc.
    pub styles: Vec<VennStyle>,
    /// Frontmatter / `%%{init: { 'theme': ... } }%%` theme override.
    pub theme_name: Option<String>,
    /// `%%{init: { 'look': 'handDrawn' } }%%`
    pub hand_drawn: bool,
    /// `%%{init: { 'handDrawnSeed': 1 } }%%`
    pub hand_drawn_seed: Option<i64>,
    /// `%%{init: { 'venn': { 'useDebugLayout': true } } }%%`
    pub use_debug_layout: bool,
}

/// One subset — either `set A` (1 element in `sets`) or `union A,B,...`
/// (>=2 elements). Lists are alphabetically sorted at insertion time.
#[derive(Debug, Clone)]
pub struct VennSubset {
    pub sets: Vec<String>,
    pub size: f64,
    pub label: Option<String>,
}

/// One `text "label"` line under a set/union (plus optional id +
/// bracket label, e.g. `text foo["Long label"]`).
#[derive(Debug, Clone)]
pub struct VennTextNode {
    pub sets: Vec<String>,
    pub id: String,
    pub label: Option<String>,
}

/// `style A,B fill:..., color:...`
#[derive(Debug, Clone)]
pub struct VennStyle {
    pub targets: Vec<String>,
    pub styles: Vec<(String, String)>,
}
