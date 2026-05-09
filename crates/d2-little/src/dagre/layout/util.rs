//! Layout utility functions ported from dagre.js util.ts

use super::types::*;
use crate::dagre::graph::{Graph, GraphOptions};
#[cfg(test)]
use std::collections::HashMap;

/// Add a dummy node to the graph and return its ID.
pub(crate) fn add_dummy_node(
    g: &mut Graph<NodeLabel, EdgeLabel>,
    dummy_type: &str,
    mut attrs: NodeLabel,
    prefix: &str,
) -> String {
    let mut v = prefix.to_string();
    while g.has_node(&v) {
        v = crate::dagre::util::unique_id(prefix);
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
    simplified.set_default_node_label(|_| NodeLabel::default());
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

/// Find rectangle-line intersection point.
pub(crate) fn intersect_rect(rect: &NodeLabel, point: &Point) -> Point {
    let x = rect.x.unwrap_or(0.0);
    let y = rect.y.unwrap_or(0.0);
    let dx = point.x - x;
    let dy = point.y - y;
    let mut w = rect.width / 2.0;
    let mut h = rect.height / 2.0;

    if dx == 0.0 && dy == 0.0 {
        return Point { x, y };
    }

    let (sx, sy);
    if dy.abs() * w > dx.abs() * h {
        if dy < 0.0 {
            h = -h;
        }
        sx = if dy != 0.0 { h * dx / dy } else { 0.0 };
        sy = h;
    } else {
        if dx < 0.0 {
            w = -w;
        }
        sx = w;
        sy = if dx != 0.0 { w * dy / dx } else { 0.0 };
    }

    Point {
        x: x + sx,
        y: y + sy,
    }
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
) -> HashMap<String, HashMap<String, i32>> {
    let mut result = HashMap::new();
    for v in g.nodes() {
        let mut sucs = HashMap::new();
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
) -> HashMap<String, HashMap<String, i32>> {
    let mut result = HashMap::new();
    for v in g.nodes() {
        let mut preds = HashMap::new();
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
