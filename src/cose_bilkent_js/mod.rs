//! cose-bilkent layout integration — byte-exact mindmap layout via embedded
//! cytoscape.js + cose-bilkent inside rquickjs.
//!
//! Mermaid drives the JavaScript `cose-bilkent` extension to lay out mindmap
//! diagrams. A pure Rust port of the multi-level coarsening / spring embedder
//! is unlikely to be byte-exact (FP accumulation order, multi-stage tiling,
//! randomisation), so we embed the upstream JS bundle and run the layout
//! inside an embedded QuickJS runtime — same approach as the `katex` feature.
//!
//! Spike at `/tmp/spike-cytoscape` confirmed cytoscape headless +
//! cose-bilkent runs in rquickjs with only a 15-LOC stub (no-op `console`,
//! `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`). No DOM
//! shim is required when we set `headless: true` directly.
//!
//! Vendored sources (all MIT):
//!   * `vendor/cytoscape.umd.js`        — 1.15 MB
//!   * `vendor/layout-base.js`          — 115 KB
//!   * `vendor/cose-base.js`            —  45 KB
//!   * `vendor/cytoscape-cose-bilkent.js` — 16 KB

pub mod render;

pub use render::{layout, Edge, Graph, LayoutError, Node, PositionedEdge, PositionedNode};

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test — runs the spike's 3-node graph through the harness twice
    /// and checks that the output is deterministic across runs.
    #[test]
    fn smoke_3_nodes_deterministic() {
        let g = Graph {
            nodes: vec![
                Node {
                    id: "a".into(),
                    label: String::new(),
                    width: 50.0,
                    height: 30.0,
                    padding: 0.0,
                },
                Node {
                    id: "b".into(),
                    label: String::new(),
                    width: 50.0,
                    height: 30.0,
                    padding: 0.0,
                },
                Node {
                    id: "c".into(),
                    label: String::new(),
                    width: 50.0,
                    height: 30.0,
                    padding: 0.0,
                },
            ],
            edges: vec![
                Edge {
                    id: "ab".into(),
                    source: "a".into(),
                    target: "b".into(),
                },
                Edge {
                    id: "ac".into(),
                    source: "a".into(),
                    target: "c".into(),
                },
            ],
        };
        let out1 = layout(&g).expect("first layout run");
        let out2 = layout(&g).expect("second layout run");
        assert_eq!(out1.nodes.len(), 3);
        assert_eq!(out1.edges.len(), 2);
        for (a, b) in out1.nodes.iter().zip(out2.nodes.iter()) {
            assert_eq!(a.id, b.id);
            assert!(
                (a.x - b.x).abs() < 1e-9,
                "node {} x drift: {} vs {}",
                a.id,
                a.x,
                b.x
            );
            assert!(
                (a.y - b.y).abs() < 1e-9,
                "node {} y drift: {} vs {}",
                a.id,
                a.y,
                b.y
            );
        }
    }
}
