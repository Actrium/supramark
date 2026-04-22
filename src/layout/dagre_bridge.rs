//! Dagre adapter — glue between our `unified::LayoutData` and the
//! `dagre` crate (`/ext/dagre`).
//!
//! Upstream references:
//! * `rendering-util/layout-algorithms/dagre/index.ts`        (379 LoC)
//! * `rendering-util/layout-algorithms/dagre/mermaid-graphlib.ts` (413 LoC)
//!
//! Responsibilities:
//! 1. Populate a `dagre::graph::Graph<NodeLabel, EdgeLabel>` from our
//!    `LayoutData` — compound when any node has `parent_id`, simple
//!    otherwise.
//! 2. Self-edges get expanded into two helper nodes + three stitched
//!    edges, matching `index.ts`'s handling (lines 308-364 upstream).
//!    For Wave 3 P0 we keep the expansion simple — the rendered self-loop
//!    geometry refinement happens in `routing.rs`.
//! 3. Run `dagre::layout(&mut g, opts)`.
//! 4. Copy post-layout coordinates back to a fresh `LayoutResult`.
//!
//! Non-goals for this wave: cluster-DB bookkeeping
//! (`adjustClustersAndEdges`), subgraph-title margin offsets,
//! `sortNodesByHierarchy` — these land with the first Stratum 3 client
//! (flowchart), as their correctness only matters once a diagram
//! actually exercises compound graphs end-to-end.

use crate::error::Result;
use crate::layout::routing;
use crate::layout::unified::{Bounds, Cluster, Edge, LayoutData, LayoutResult, Node, Point};
use crate::theme::ThemeVariables;

use dagre::graph::{Graph, GraphOptions};
use dagre::layout::types::{EdgeLabel, LayoutOptions, NodeLabel, RankDir};

/// Default node box size when a diagram failed to size-measure its label
/// before handing us a `LayoutData`. Matches upstream's fallback where
/// `node.width / node.height` default to 0 and dagre treats them as
/// point-sized — which degenerates to coincident coords and is rarely
/// what a renderer wants, so we nudge to something sensible.
const DEFAULT_NODE_WIDTH: f64 = 80.0;
const DEFAULT_NODE_HEIGHT: f64 = 40.0;

/// Parse upstream's `rankdir` strings — "TB" / "BT" / "LR" / "RL".
/// Upstream also accepts the flowchart aliases "TD" (= "TB") and the
/// lowercase spellings; we cover those too.
fn parse_rankdir(s: Option<&str>) -> RankDir {
    match s.map(str::trim).map(str::to_ascii_uppercase).as_deref() {
        Some("BT") => RankDir::BT,
        Some("LR") => RankDir::LR,
        Some("RL") => RankDir::RL,
        // "TB" and "TD" and the absent case all map to top-bottom.
        _ => RankDir::TB,
    }
}

/// Determine whether the graph has any parent-child relationships — if
/// yes dagre must run in compound mode.
fn is_compound(data: &LayoutData) -> bool {
    data.nodes.iter().any(|n| n.parent_id.is_some())
}

/// Build the layout options from `LayoutData` + defaults. Mirrors
/// upstream `index.ts` lines 272-291's `.setGraph({...})` call.
fn build_layout_options(data: &LayoutData) -> LayoutOptions {
    LayoutOptions {
        rankdir: parse_rankdir(data.direction.as_deref()),
        nodesep: data.node_spacing.unwrap_or(50.0),
        ranksep: data.rank_spacing.unwrap_or(50.0),
        // Upstream hard-codes these to 8 at the top-level graph.
        marginx: 8.0,
        marginy: 8.0,
        ..LayoutOptions::default()
    }
}

/// Build a dagre NodeLabel populated with just the fields dagre cares
/// about (width/height/labelpos/padding). Shape/label rendering fields
/// are carried outside dagre — we re-attach them from `LayoutData` when
/// building the `LayoutResult`.
fn make_node_label(node: &Node) -> NodeLabel {
    NodeLabel {
        width: node.width.unwrap_or(DEFAULT_NODE_WIDTH),
        height: node.height.unwrap_or(DEFAULT_NODE_HEIGHT),
        label: node.label.clone(),
        padding: node.padding.unwrap_or(0.0),
        padding_x: node.label_padding_x,
        padding_y: node.label_padding_y,
        rx: node.rx,
        ry: node.ry,
        shape: node.shape.clone(),
        class: node.css_classes.clone(),
        ..NodeLabel::default()
    }
}

/// Build a dagre EdgeLabel. Only a handful of fields feed into dagre's
/// layout proper (`minlen`, `weight`, `width`, `height`, `labelpos`);
/// everything else rides back on the user-facing `Edge`.
fn make_edge_label(edge: &Edge) -> EdgeLabel {
    EdgeLabel {
        minlen: edge.minlen.unwrap_or(1),
        weight: 1,
        width: 0.0,
        height: 0.0,
        ..EdgeLabel::default()
    }
}

/// Resolve an edge's source node id. Upstream uses `edge.start` for
/// flowchart and `edge.source` for newer diagrams — we accept whichever
/// is populated, preferring `start` to match the dagre/index.ts call
/// site (`graph.setEdge(edge.start, edge.end, ...)`).
fn edge_source<'a>(e: &'a Edge) -> Option<&'a str> {
    e.start.as_deref().or(e.source.as_deref())
}

/// Symmetric to [`edge_source`].
fn edge_target<'a>(e: &'a Edge) -> Option<&'a str> {
    e.end.as_deref().or(e.target.as_deref())
}

/// Populate a dagre graph from a `LayoutData`. Self-edges are expanded
/// using the upstream pattern (two label-rect helper nodes + three
/// stitched edges).
fn build_graph(data: &LayoutData) -> Graph<NodeLabel, EdgeLabel> {
    let opts = GraphOptions {
        directed: true,
        multigraph: true,
        compound: is_compound(data),
    };
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(opts);

    for node in &data.nodes {
        g.set_node(node.id.clone(), Some(make_node_label(node)));
    }
    if g.is_compound() {
        for node in &data.nodes {
            if let Some(parent) = node.parent_id.as_deref() {
                g.set_parent(&node.id, Some(parent));
            }
        }
    }

    for edge in &data.edges {
        let (Some(src), Some(dst)) = (edge_source(edge), edge_target(edge)) else {
            log::warn!(
                "dagre_bridge: edge '{}' missing start/end (source/target); skipped",
                edge.id
            );
            continue;
        };

        if src == dst {
            // Self-edge expansion — see upstream index.ts:308-364.
            expand_self_edge(&mut g, edge, src);
        } else {
            let name = if edge.id.is_empty() {
                None
            } else {
                Some(edge.id.as_str())
            };
            g.set_edge(src, dst, Some(make_edge_label(edge)), name);
        }
    }

    g
}

/// Insert two helper nodes and three edges so dagre has something to
/// rank for a self-edge. Port of upstream `index.ts:308-364`, trimmed
/// to the ranking essentials — visual self-loop smoothing is the job
/// of `routing::smooth_self_loop` later.
fn expand_self_edge(g: &mut Graph<NodeLabel, EdgeLabel>, edge: &Edge, node_id: &str) {
    let sid1 = format!("{node_id}---{node_id}---1");
    let sid2 = format!("{node_id}---{node_id}---2");

    let helper = || NodeLabel {
        width: 10.0,
        height: 10.0,
        label: Some(String::new()),
        padding: 0.0,
        shape: Some("labelRect".to_string()),
        class: None,
        ..NodeLabel::default()
    };
    g.set_node(sid1.clone(), Some(helper()));
    g.set_node(sid2.clone(), Some(helper()));

    // Mirror parent-id when inside a cluster.
    if g.is_compound() {
        if let Some(parent) = g.parent(node_id).map(|s| s.to_string()) {
            g.set_parent(&sid1, Some(&parent));
            g.set_parent(&sid2, Some(&parent));
        }
    }

    let base_label = make_edge_label(edge);
    g.set_edge(
        node_id,
        &sid1,
        Some(base_label.clone()),
        Some(&format!("{node_id}-cyclic-special-0")),
    );
    g.set_edge(
        &sid1,
        &sid2,
        Some(base_label.clone()),
        Some(&format!("{node_id}-cyclic-special-1")),
    );
    g.set_edge(
        &sid2,
        node_id,
        Some(base_label),
        Some(&format!("{node_id}-cyclic-special-2")),
    );
}

/// Pull post-layout coordinates out of `g` and paint them back onto a
/// fresh copy of `data.nodes`, preserving original index order.
fn collect_nodes(data: &LayoutData, g: &Graph<NodeLabel, EdgeLabel>) -> Vec<Node> {
    data.nodes
        .iter()
        .map(|orig| {
            let mut out = orig.clone();
            if let Some(lbl) = g.node(&orig.id) {
                out.x = lbl.x;
                out.y = lbl.y;
                // Dagre may have widened a compound node while packing
                // children — honour the updated width/height.
                out.width = Some(lbl.width);
                out.height = Some(lbl.height);
            }
            out
        })
        .collect()
}

/// Pull post-layout edge spline points + label centres.
fn collect_edges(data: &LayoutData, g: &Graph<NodeLabel, EdgeLabel>) -> Vec<Edge> {
    data.edges
        .iter()
        .map(|orig| {
            let mut out = orig.clone();
            let (Some(src), Some(dst)) = (edge_source(orig), edge_target(orig)) else {
                return out;
            };
            if src == dst {
                // Self-edges were expanded; leave routing to
                // `routing::smooth_self_loop` which regenerates them
                // from the node bounds rather than from the helper
                // chain.
                return out;
            }
            let name = if orig.id.is_empty() {
                None
            } else {
                Some(orig.id.as_str())
            };
            if let Some(lbl) = g.edge(src, dst, name) {
                out.points = Some(
                    lbl.points
                        .iter()
                        .map(|p| Point { x: p.x, y: p.y })
                        .collect(),
                );
                out.label_x = lbl.x;
                out.label_y = lbl.y;
            }
            out
        })
        .collect()
}

/// Derive cluster metadata from compound-node bounds.
fn collect_clusters(nodes: &[Node]) -> Vec<Cluster> {
    nodes
        .iter()
        .filter(|n| n.is_group)
        .map(|n| Cluster {
            id: n.id.clone(),
            representative: None,
            bounds: match (n.x, n.y, n.width, n.height) {
                (Some(x), Some(y), Some(w), Some(h)) => Some(Bounds {
                    x: x - w / 2.0,
                    y: y - h / 2.0,
                    width: w,
                    height: h,
                }),
                _ => None,
            },
        })
        .collect()
}

/// Compute a tight AABB over all post-layout nodes + edge spline points.
fn compute_bounds(nodes: &[Node], edges: &[Edge]) -> Bounds {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for n in nodes {
        let (Some(x), Some(y)) = (n.x, n.y) else {
            continue;
        };
        let w = n.width.unwrap_or(0.0);
        let h = n.height.unwrap_or(0.0);
        min_x = min_x.min(x - w / 2.0);
        min_y = min_y.min(y - h / 2.0);
        max_x = max_x.max(x + w / 2.0);
        max_y = max_y.max(y + h / 2.0);
    }
    for e in edges {
        let Some(points) = e.points.as_ref() else {
            continue;
        };
        for p in points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
    }

    if !min_x.is_finite() {
        return Bounds::default();
    }
    Bounds {
        x: min_x,
        y: min_y,
        width: (max_x - min_x).max(0.0),
        height: (max_y - min_y).max(0.0),
    }
}

/// Public entry — run the dagre layout on a `LayoutData`, return the
/// geometry. Upstream analogue: `render.ts::render` + `dagre/index.ts::render`.
pub fn layout(data: &LayoutData, _theme: &ThemeVariables) -> Result<LayoutResult> {
    // Degenerate shortcut: empty graph — dagre handles it, but bypass to
    // save the pipeline overhead and keep the tests snappy.
    if data.nodes.is_empty() {
        return Ok(LayoutResult::default());
    }

    log::debug!(
        "dagre_bridge: laying out {} node(s), {} edge(s), compound={}",
        data.nodes.len(),
        data.edges.len(),
        is_compound(data)
    );

    let mut g = build_graph(data);
    let opts = build_layout_options(data);
    dagre::layout(&mut g, Some(opts));

    let nodes = collect_nodes(data, &g);
    let edges_pre = collect_edges(data, &g);
    // Give `routing` a chance to refine splines / place labels along paths.
    let edges = routing::refine_edges(&nodes, &edges_pre);
    let clusters = collect_clusters(&nodes);
    let bounds = compute_bounds(&nodes, &edges);

    Ok(LayoutResult {
        nodes,
        edges,
        clusters,
        bounds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::unified::{Edge, LayoutData, Node};
    use crate::theme::ThemeVariables;

    fn two_node_graph() -> LayoutData {
        let mut a = Node::default();
        a.id = "a".into();
        a.label = Some("A".into());
        a.width = Some(60.0);
        a.height = Some(30.0);

        let mut b = Node::default();
        b.id = "b".into();
        b.label = Some("B".into());
        b.width = Some(60.0);
        b.height = Some(30.0);

        let mut e = Edge::default();
        e.id = "e1".into();
        e.start = Some("a".into());
        e.end = Some("b".into());

        LayoutData {
            nodes: vec![a, b],
            edges: vec![e],
            direction: Some("TB".into()),
            ..LayoutData::default()
        }
    }

    #[test]
    fn two_node_pipeline_assigns_coordinates() {
        let data = two_node_graph();
        let theme = ThemeVariables::default();
        let out = layout(&data, &theme).expect("layout");

        assert_eq!(out.nodes.len(), 2);
        let a = &out.nodes[0];
        let b = &out.nodes[1];
        assert!(a.x.is_some() && a.y.is_some(), "a coords populated");
        assert!(b.x.is_some() && b.y.is_some(), "b coords populated");

        // TB layout: `b` is below `a` (larger y).
        assert!(
            b.y.unwrap() > a.y.unwrap(),
            "TB means target below source: a.y={:?} b.y={:?}",
            a.y,
            b.y
        );
        // And roughly centre-aligned on x (same width, no siblings).
        assert!(
            (a.x.unwrap() - b.x.unwrap()).abs() < 1e-6,
            "TB centres x: a.x={:?} b.x={:?}",
            a.x,
            b.x
        );

        // The edge should have waypoints connecting the two centres.
        let edge = &out.edges[0];
        let points = edge.points.as_ref().expect("edge points set");
        assert!(points.len() >= 2, "at least endpoints on the spline");
        let first = points.first().unwrap();
        let last = points.last().unwrap();
        assert!(first.y < last.y, "edge points go from A toward B downward");
    }

    #[test]
    fn empty_graph_returns_empty_result() {
        let data = LayoutData::default();
        let theme = ThemeVariables::default();
        let out = layout(&data, &theme).expect("empty");
        assert!(out.nodes.is_empty());
        assert!(out.edges.is_empty());
        assert!(out.clusters.is_empty());
    }

    #[test]
    fn lr_direction_orients_horizontally() {
        let mut data = two_node_graph();
        data.direction = Some("LR".into());
        let theme = ThemeVariables::default();
        let out = layout(&data, &theme).expect("layout");

        let a = &out.nodes[0];
        let b = &out.nodes[1];
        assert!(
            b.x.unwrap() > a.x.unwrap(),
            "LR means target right of source: a.x={:?} b.x={:?}",
            a.x,
            b.x
        );
        assert!(
            (a.y.unwrap() - b.y.unwrap()).abs() < 1e-6,
            "LR centres y: a.y={:?} b.y={:?}",
            a.y,
            b.y
        );
    }

    #[test]
    fn missing_endpoints_skip_gracefully() {
        let mut a = Node::default();
        a.id = "a".into();
        a.width = Some(40.0);
        a.height = Some(20.0);

        let bogus = Edge {
            id: "bad".into(),
            ..Edge::default()
        };

        let data = LayoutData {
            nodes: vec![a],
            edges: vec![bogus],
            ..LayoutData::default()
        };
        let theme = ThemeVariables::default();
        // Must not panic; the unmapped edge should just be carried
        // through without points.
        let out = layout(&data, &theme).expect("layout");
        assert_eq!(out.edges.len(), 1);
        assert!(out.edges[0].points.is_none());
    }
}
