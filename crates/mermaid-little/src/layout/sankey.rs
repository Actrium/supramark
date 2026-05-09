//! Sankey layout — calls the d3-sankey algorithm port.
//!
//! Upstream renderer: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/sankey/sankeyRenderer.ts
//!
//! ## Fixed upstream quirks
//! * `height` defaults to `width` if unset (upstream literally writes
//!   `conf?.height ?? defaultSankeyConfig.width!`).
//! * `nodePadding` is `10 + (showValues ? 15 : 0)`.
//! * `nodeWidth` is hard-coded at 10.
//! * 6 relaxation iterations, `0.99^i` alpha schedule.

#[path = "sankey_algo.rs"]
pub mod sankey_algo;

use crate::error::Result;
use crate::layout::sankey::sankey_algo::{AlgoGraph, Sankey};
use crate::model::sankey::SankeyDiagram;
use crate::theme::ThemeVariables;

/// Hard-coded from upstream `sankeyRenderer.ts` (Line 88).
pub const NODE_WIDTH: f64 = 10.0;

#[derive(Debug, Clone, Default)]
pub struct SankeyLayout {
    /// Total SVG width — taken from config (`width`).
    pub width: f64,
    /// Total SVG height — taken from config (`height` or fallback to `width`).
    pub height: f64,
    /// Per-node color from `schemeTableau10`, looked up by ordinal on
    /// the node id.
    pub node_colors: Vec<String>,
    /// Output of the d3-sankey port.
    pub graph: AlgoGraph,
}

pub fn layout(d: &SankeyDiagram, _theme: &ThemeVariables) -> Result<SankeyLayout> {
    let cfg = &d.config;
    let width = cfg.width;
    // Upstream uses `conf?.height ?? defaultSankeyConfig.width!`
    // which looks like a bug — the fallback is `width` (600) not the
    // schema-stated `height` default (400). In practice `conf.height`
    // is never `undefined` because `getConfig()` fills it with the
    // schema default; the buggy fallback is unreachable. So for an
    // unset height we use the real default 400, matching every
    // observed fixture.
    let height = cfg.height.unwrap_or(400.0);

    let node_padding = 10.0 + if cfg.show_values { 15.0 } else { 0.0 };

    let links: Vec<(String, String, f64)> = d
        .links
        .iter()
        .map(|l| (l.source.clone(), l.target.clone(), l.value))
        .collect();

    let sankey = Sankey {
        x0: 0.0,
        y0: 0.0,
        x1: width,
        y1: height,
        dx: NODE_WIDTH,
        dy: node_padding,
        align: cfg.node_alignment,
        iterations: 6,
    };

    let graph = sankey.layout(&d.nodes, &links);

    let node_colors: Vec<String> = (0..d.nodes.len())
        .map(|i| tableau10(i).to_string())
        .collect();

    Ok(SankeyLayout {
        width,
        height,
        node_colors,
        graph,
    })
}

/// d3 `schemeTableau10`. Ordinal scale cycles through this array keyed
/// by insertion order on `colorScheme(d.id)`.
pub fn tableau10(i: usize) -> &'static str {
    const PALETTE: [&str; 10] = [
        "#4e79a7", "#f28e2c", "#e15759", "#76b7b2", "#59a14f", "#edc949", "#af7aa1", "#ff9da7",
        "#9c755f", "#bab0ab",
    ];
    PALETTE[i % 10]
}
