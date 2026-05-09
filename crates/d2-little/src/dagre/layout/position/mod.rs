//! Coordinate assignment module.
//!
//! Assigns (x, y) coordinates to every node. The y-coordinates are derived
//! from rank information, and x-coordinates come from the Brandes-Köpf
//! algorithm (4-direction sweep + balance).

pub(crate) mod bk;

use crate::dagre::graph::Graph;
use crate::dagre::layout::types::{EdgeLabel, GraphLabel, NodeLabel, RankAlign};
use crate::dagre::layout::util::build_layer_matrix;

/// Assign final (x, y) coordinates to every node in the graph.
///
/// Assumes that each node already has `rank` and `order` set.
pub(crate) fn position(g: &mut Graph<NodeLabel, EdgeLabel>) {
    position_y(g);
    position_x(g);
}

/// Assign y-coordinates based on ranks.
///
/// Walks through the layer matrix rank by rank, accumulating vertical offset
/// from the tallest node in each preceding rank plus the configured ranksep.
/// Supports top / center / bottom alignment within each rank.
fn position_y(g: &mut Graph<NodeLabel, EdgeLabel>) {
    let layering = build_layer_matrix(g);

    let graph_label = g.graph_label::<GraphLabel>();
    let ranksep = graph_label.map_or(50.0, |gl| gl.ranksep);
    let rank_align = graph_label.map_or(RankAlign::Center, |gl| gl.rank_align);

    let mut prev_y: f64 = 0.0;

    for layer in &layering {
        // Find the maximum height among all nodes in this layer.
        let max_height = layer
            .iter()
            .filter(|v| !v.is_empty())
            .filter_map(|v| g.node(v).map(|n| n.height))
            .fold(0.0_f64, f64::max);

        // Collect node ids and their individual heights first, since we need
        // to mutably borrow the graph afterwards.
        let assignments: Vec<(String, f64)> = layer
            .iter()
            .filter(|v| !v.is_empty())
            .filter_map(|v| g.node(v).map(|n| (v.clone(), n.height)))
            .collect();

        for (v, node_height) in assignments {
            let y = match rank_align {
                RankAlign::Top => prev_y + node_height / 2.0,
                RankAlign::Bottom => prev_y + max_height - node_height / 2.0,
                RankAlign::Center => prev_y + max_height / 2.0,
            };
            if let Some(node) = g.node_mut(&v) {
                node.y = Some(y);
            }
        }

        prev_y += max_height + ranksep;
    }
}

/// Assign x coordinates using the Brandes-Köpf algorithm.
fn position_x(g: &mut Graph<NodeLabel, EdgeLabel>) {
    let xs = bk::position_x(g);
    for (v, x) in xs {
        if let Some(node) = g.node_mut(&v) {
            node.x = Some(x);
        }
    }
}
