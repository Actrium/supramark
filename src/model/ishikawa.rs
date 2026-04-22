//! Ishikawa (fishbone) diagram parsed model.
//!
//! Upstream reference: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/ishikawa/
//! Grammar: parser/ishikawa.jison — recognises `ishikawa[-beta]` + indented
//! text lines that form a tree via `yy.addNode(level, text)`.
//!
//! The parser reduces the source to a root node (the "effect") and a
//! recursively nested list of `IshikawaNode` children (causes and
//! sub-causes). The model preserves input order for every sibling
//! group — the renderer alternates even/odd causes above and below
//! the spine, which means any permutation would change the output.

use crate::model::DiagramMeta;

/// One tree node — either the root effect or a (sub-)cause.
/// Mirrors upstream `IshikawaNode` one-to-one.
#[derive(Debug, Clone, Default)]
pub struct IshikawaNode {
    pub text: String,
    pub children: Vec<IshikawaNode>,
}

/// Top-level ishikawa model. `root` may be `None` for the degenerate
/// "ishikawa-beta" header alone; in that case the renderer emits an
/// empty diagram (no spine, no head).
#[derive(Debug, Clone, Default)]
pub struct IshikawaDiagram {
    pub meta: DiagramMeta,
    pub root: Option<IshikawaNode>,
    /// Diagram padding (from frontmatter `config.ishikawa.diagramPadding`).
    /// Upstream default is 20.
    pub diagram_padding: f64,
}
