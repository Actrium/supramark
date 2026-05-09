//! Layout utility functions ported from dagre.js util.ts

use super::types::*;
use crate::graph::{Graph, GraphOptions};
#[cfg(test)]
use std::collections::BTreeMap;

/// Add a dummy node to the graph and return its ID.
pub(crate) fn add_dummy_node(
    g: &mut Graph<NodeLabel, EdgeLabel>,
    dummy_type: &str,
    mut attrs: NodeLabel,
    prefix: &str,
) -> String {
    let mut v = prefix.to_string();
    while g.has_node(&v) {
        v = crate::util::unique_id(prefix);
    }
    attrs.dummy = Some(dummy_type.to_string());
    g.set_node(v.clone(), Some(attrs));
    v
}

/// Create a simplified (non-multigraph) version of the graph,
/// aggregating multi-edge weights and taking max minlen.
pub(crate) fn simplify(g: &Graph<NodeLabel, EdgeLabel>) -> Graph<NodeLabel, EdgeLabel> {
    let mut simplified = Graph::new();
    for v in g.nodes() {
        if let Some(label) = g.node(&v) {
            simplified.set_node(v, Some(label.clone()));
        } else {
            simplified.set_node(v, None);
        }
    }
    for e in g.edges() {
        let existing = simplified.edge(&e.v, &e.w, None);
        let mut weight = existing.map_or(0, |l: &EdgeLabel| l.weight);
        let mut minlen = existing.map_or(1, |l: &EdgeLabel| l.minlen);

        if let Some(label) = g.edge(&e.v, &e.w, e.name.as_deref()) {
            weight += label.weight;
            minlen = minlen.max(label.minlen);
        }

        let label = EdgeLabel {
            weight,
            minlen,
            ..EdgeLabel::default()
        };
        simplified.set_edge(e.v.clone(), e.w.clone(), Some(label), None);
    }
    simplified
}

/// Create a non-compound version, keeping only leaf nodes.
pub(crate) fn as_non_compound_graph(
    g: &Graph<NodeLabel, EdgeLabel>,
) -> Graph<NodeLabel, EdgeLabel> {
    let mut simplified = Graph::with_options(GraphOptions {
        multigraph: g.is_multigraph(),
        ..Default::default()
    });
    for v in g.nodes() {
        if g.is_compound() && !g.children(Some(&v)).is_empty() {
            continue;
        }
        if let Some(label) = g.node(&v) {
            simplified.set_node(v, Some(label.clone()));
        } else {
            simplified.set_node(v, None);
        }
    }
    for e in g.edges() {
        // In dagre-d3-es (the upstream JS reference), all edges are included
        // in the simplified graph via `simplified.setEdge(e, g.edge(e))`.
        // graphlib's `setEdge` implicitly creates the endpoint nodes with an
        // empty label `{}` if they do not already exist, so compound parents
        // (which were skipped above) become implicit nodes. The downstream
        // `longest_path::dfs` then sets `g.node(v).rank = rank` on these
        // implicit `{}` objects without error.
        //
        // We replicate this by ensuring that if an edge endpoint is a
        // compound parent (excluded from the explicit node set), we insert it
        // as an implicit node with a default label before adding the edge.
        for endpoint in [&e.v, &e.w] {
            if !simplified.has_node(endpoint) {
                // Implicit node — compound parent excluded above. Add with
                // default label so that rank-assignment can reference it.
                simplified.set_node(endpoint.clone(), Some(NodeLabel::default()));
            }
        }
        if let Some(label) = g.edge(&e.v, &e.w, e.name.as_deref()) {
            simplified.set_edge(
                e.v.clone(),
                e.w.clone(),
                Some(label.clone()),
                e.name.as_deref(),
            );
        }
    }
    simplified
}

/// Find the intersection of the ray from the node center toward `point`
/// with the node's boundary polygon. The polygon is computed from the
/// node's shape, size, and center position.
///
/// * Diamond nodes use a rotated-square polygon.
/// * stateStart / stateEnd use ellipse/circle intersection (upstream sets
///   `node.intersect = intersectCircle(node, width/2, point)`).
/// * All other shapes fall back to `intersect_rect`.
///
/// This is a thin dispatcher over the public helpers in
/// [`crate::layout::intersect`] — downstream callers that need to clip
/// edges to non-built-in shapes (hexagon, trapezoid, …) should call
/// those helpers directly with their own polygon vertices.
pub(crate) fn intersect_node(rect: &NodeLabel, point: &Point) -> Point {
    let cx = rect.x.unwrap_or(0.0);
    let cy = rect.y.unwrap_or(0.0);
    match rect.shape.as_deref() {
        Some("diamond") => {
            let w2 = rect.width / 2.0;
            let h2 = rect.height / 2.0;
            let verts = [
                Point { x: cx, y: cy - h2 },
                Point { x: cx + w2, y: cy },
                Point { x: cx, y: cy + h2 },
                Point { x: cx - w2, y: cy },
            ];
            super::intersect::intersect_polygon(&verts, &Point { x: cx, y: cy }, point)
        }
        Some("stateStart" | "stateEnd" | "state_start" | "state_end" | "start" | "end") => {
            super::intersect::intersect_ellipse(cx, cy, rect.width / 2.0, rect.height / 2.0, point)
        }
        _ => super::intersect::intersect_rect(cx, cy, rect.width, rect.height, point),
    }
}

/// Test-only `&NodeLabel` adaptor over [`crate::layout::intersect::intersect_rect`].
/// The layout pipeline itself goes through [`intersect_node`]; this exists
/// solely so existing tests can keep their `intersect_rect(&rect, &point)`
/// call shape without rewriting.
#[cfg(test)]
pub(crate) fn intersect_rect(rect: &NodeLabel, point: &Point) -> Point {
    super::intersect::intersect_rect(
        rect.x.unwrap_or(0.0),
        rect.y.unwrap_or(0.0),
        rect.width,
        rect.height,
        point,
    )
}

/// Build a 2D matrix of node IDs indexed by [rank][order].
pub(crate) fn build_layer_matrix(g: &Graph<NodeLabel, EdgeLabel>) -> Vec<Vec<String>> {
    let max_r = max_rank(g);
    if max_r < 0 {
        return vec![];
    }
    let mut layers: Vec<Vec<String>> = vec![vec![]; (max_r + 1) as usize];
    for v in g.nodes() {
        if let Some(node) = g.node(&v)
            && let Some(rank) = node.rank
        {
            let r = rank as usize;
            if r < layers.len() {
                let order = node.order.unwrap_or(0);
                if order >= layers[r].len() {
                    layers[r].resize(order + 1, String::new());
                }
                layers[r][order] = v;
            }
        }
    }
    layers
}

/// Find the maximum rank value in the graph.
pub(crate) fn max_rank(g: &Graph<NodeLabel, EdgeLabel>) -> i32 {
    g.nodes()
        .iter()
        .filter_map(|v| g.node(v).and_then(|n| n.rank))
        .max()
        .unwrap_or(-1)
}

/// Normalize ranks so minimum rank is 0.
pub(crate) fn normalize_ranks(g: &mut Graph<NodeLabel, EdgeLabel>) {
    let min_rank = g
        .nodes()
        .iter()
        .filter_map(|v| g.node(v).and_then(|n| n.rank))
        .min();

    if let Some(min) = min_rank
        && min != 0
    {
        for v in g.nodes() {
            if let Some(node) = g.node_mut(&v)
                && let Some(ref mut rank) = node.rank
            {
                *rank -= min;
            }
        }
    }
}

/// Remove empty ranks from the layering.
pub(crate) fn remove_empty_ranks(g: &mut Graph<NodeLabel, EdgeLabel>) {
    let ranks: Vec<i32> = g
        .nodes()
        .iter()
        .filter_map(|v| g.node(v).and_then(|n| n.rank))
        .collect();

    if ranks.is_empty() {
        return;
    }
    let offset = *ranks.iter().min().unwrap();
    let max_r = *ranks.iter().max().unwrap();
    let len = (max_r - offset + 1) as usize;
    let mut layers: Vec<Vec<String>> = vec![vec![]; len];
    for v in g.nodes() {
        if let Some(node) = g.node(&v)
            && let Some(rank) = node.rank
        {
            layers[(rank - offset) as usize].push(v);
        }
    }

    // Get nodeRankFactor from graph label (set by nesting_graph).
    // In dagre.js, nestingGraph.run is always called, setting nodeRankFactor
    // to at least 1. With nodeRankFactor=1, `i % 1 === 0` for all i, so
    // no empty ranks are ever removed for non-compound graphs.
    // For compound graphs, nodeRankFactor = 2*height+1 (>= 1).
    // Only ranks at non-factor positions are removed.
    let node_rank_factor = g
        .graph_label::<GraphLabel>()
        .and_then(|gl| gl.node_rank_factor)
        .map(|f| f as i32)
        .unwrap_or(1); // Default to 1 matching JS nestingGraph behavior

    let mut delta: i32 = 0;
    for (i, layer) in layers.iter().enumerate() {
        if layer.is_empty() {
            // Remove empty rank only if it's NOT at a nodeRankFactor boundary
            if node_rank_factor > 0 && (i as i32) % node_rank_factor != 0 {
                delta -= 1;
            }
        } else if delta != 0 {
            for v in layer {
                if let Some(node) = g.node_mut(v)
                    && let Some(ref mut rank) = node.rank
                {
                    *rank += delta;
                }
            }
        }
    }
}

/// Add a border node to the graph.
pub(crate) fn add_border_node(
    g: &mut Graph<NodeLabel, EdgeLabel>,
    prefix: &str,
    rank: Option<i32>,
    order: Option<usize>,
) -> String {
    let node = NodeLabel {
        rank,
        order,
        ..NodeLabel::default()
    };
    add_dummy_node(g, "border", node, prefix)
}

/// Compute successor weight maps: node -> { neighbor -> total_weight }.
#[cfg(test)]
pub(crate) fn successor_weights(
    g: &Graph<NodeLabel, EdgeLabel>,
) -> BTreeMap<String, BTreeMap<String, i32>> {
    let mut result = BTreeMap::new();
    for v in g.nodes() {
        let mut sucs = BTreeMap::new();
        if let Some(edges) = g.out_edges(&v, None) {
            for e in edges {
                let w = g
                    .edge(&e.v, &e.w, e.name.as_deref())
                    .map_or(1, |l| l.weight);
                *sucs.entry(e.w.clone()).or_insert(0) += w;
            }
        }
        result.insert(v, sucs);
    }
    result
}

/// Compute predecessor weight maps: node -> { neighbor -> total_weight }.
#[cfg(test)]
pub(crate) fn predecessor_weights(
    g: &Graph<NodeLabel, EdgeLabel>,
) -> BTreeMap<String, BTreeMap<String, i32>> {
    let mut result = BTreeMap::new();
    for v in g.nodes() {
        let mut preds = BTreeMap::new();
        if let Some(edges) = g.in_edges(&v, None) {
            for e in edges {
                let w = g
                    .edge(&e.v, &e.w, e.name.as_deref())
                    .map_or(1, |l| l.weight);
                *preds.entry(e.v.clone()).or_insert(0) += w;
            }
        }
        result.insert(v, preds);
    }
    result
}
