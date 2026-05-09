//! Sankey diagram data model.
//!
//! Upstream reference: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/sankey/sankeyDB.ts
//!
//! The grammar is an abridged CSV (source, target, value). Every line
//! grows the `nodes` vec (first occurrence wins) and appends a `SankeyLink`.

use crate::model::DiagramMeta;

/// Sankey nodeAlignment values. Mirrors upstream `d3-sankey` / config
/// `SankeyNodeAlignment`. Byte parity requires the JS `justify` default
/// when the frontmatter omits the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeAlignment {
    Left,
    Right,
    Center,
    #[default]
    Justify,
}

/// `linkColor` config — either one of four named schemes or a raw CSS
/// colour string. Upstream default is `Gradient`.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum LinkColor {
    #[default]
    Gradient,
    Source,
    Target,
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct SankeyLink {
    pub source: String,
    pub target: String,
    pub value: f64,
}

/// Config extracted from the fixture's frontmatter `config.sankey:` block
/// (none of the cypress/demo fixtures pass a config any other way).
///
/// Fields reflect upstream `SankeyDiagramConfig` defaults:
/// * `width`: 600
/// * `height`: fallback is `width` (upstream bug: `conf?.height ?? defaultSankeyConfig.width`)
/// * `useMaxWidth`: false
/// * `showValues`: true
/// * `prefix`: ""
/// * `suffix`: ""
/// * `nodeAlignment`: Justify
/// * `linkColor`: Gradient
#[derive(Debug, Clone)]
pub struct SankeyConfig {
    pub width: f64,
    /// Height. `None` means the fixture didn't set it; the layout
    /// falls back to the config's default (400).
    pub height: Option<f64>,
    pub use_max_width: bool,
    pub show_values: bool,
    pub prefix: String,
    pub suffix: String,
    pub node_alignment: NodeAlignment,
    pub link_color: LinkColor,
}

impl Default for SankeyConfig {
    fn default() -> Self {
        SankeyConfig {
            width: 600.0,
            height: None,
            // The schema declares `useMaxWidth: false` but the compiled
            // `defaultConfigJson` picks up BaseDiagramConfig's `true`
            // default for every diagram that doesn't override it at
            // runtime — every reference fixture rendered with mermaid
            // bakes `width="100%"` in, so true is the observed default.
            use_max_width: true,
            show_values: true,
            prefix: String::new(),
            suffix: String::new(),
            node_alignment: NodeAlignment::Justify,
            link_color: LinkColor::Gradient,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SankeyDiagram {
    pub meta: DiagramMeta,
    /// Nodes in insertion order (first-occurrence wins).
    pub nodes: Vec<String>,
    /// Links in textual order.
    pub links: Vec<SankeyLink>,
    /// Effective config (defaults merged with frontmatter).
    pub config: SankeyConfig,
}
