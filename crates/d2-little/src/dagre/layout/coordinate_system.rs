//! Coordinate system transformations for non-TB rank directions.
//!
//! Ported from dagre.js coordinate-system.ts

use super::types::*;
use crate::dagre::graph::Graph;

/// Before positioning: if rankdir is LR or RL, swap width <-> height
/// for all nodes and edges so the algorithm can work in TB mode.
pub(crate) fn adjust(g: &mut Graph<NodeLabel, EdgeLabel>) {
    let rankdir = g
        .graph_label::<GraphLabel>()
        .map_or(RankDir::TB, |gl| gl.rankdir);

    if rankdir == RankDir::LR || rankdir == RankDir::RL {
        swap_width_height_nodes(g);
        swap_width_height_edges(g);
    }
}

/// After positioning: undo coordinate transformations based on rankdir.
///
/// - BT: negate y coords
/// - RL: negate y coords, then swap x<->y and width<->height
/// - LR: swap x<->y and width<->height
pub(crate) fn undo(g: &mut Graph<NodeLabel, EdgeLabel>) {
    let rankdir = g
        .graph_label::<GraphLabel>()
        .map_or(RankDir::TB, |gl| gl.rankdir);

    if rankdir == RankDir::BT || rankdir == RankDir::RL {
        negate_y_nodes(g);
        negate_y_edges(g);
    }

    if rankdir == RankDir::LR || rankdir == RankDir::RL {
        swap_width_height_nodes(g);
        swap_width_height_edges(g);
        swap_xy_nodes(g);
        swap_xy_edges(g);
    }
}

fn swap_width_height_nodes(g: &mut Graph<NodeLabel, EdgeLabel>) {
    for v in g.nodes() {
        if let Some(node) = g.node_mut(&v) {
            std::mem::swap(&mut node.width, &mut node.height);
        }
    }
}

fn swap_width_height_edges(g: &mut Graph<NodeLabel, EdgeLabel>) {
    for e in g.edges() {
        if let Some(label) = g.edge_mut(&e.v, &e.w, e.name.as_deref()) {
            std::mem::swap(&mut label.width, &mut label.height);
        }
    }
}

fn negate_y_nodes(g: &mut Graph<NodeLabel, EdgeLabel>) {
    for v in g.nodes() {
        if let Some(node) = g.node_mut(&v)
            && let Some(ref mut y) = node.y
        {
            *y = -*y;
        }
    }
}

fn negate_y_edges(g: &mut Graph<NodeLabel, EdgeLabel>) {
    for e in g.edges() {
        if let Some(label) = g.edge_mut(&e.v, &e.w, e.name.as_deref()) {
            if let Some(ref mut y) = label.y {
                *y = -*y;
            }
            for pt in &mut label.points {
                pt.y = -pt.y;
            }
        }
    }
}

fn swap_xy_nodes(g: &mut Graph<NodeLabel, EdgeLabel>) {
    for v in g.nodes() {
        if let Some(node) = g.node_mut(&v) {
            std::mem::swap(&mut node.x, &mut node.y);
        }
    }
}

fn swap_xy_edges(g: &mut Graph<NodeLabel, EdgeLabel>) {
    for e in g.edges() {
        if let Some(label) = g.edge_mut(&e.v, &e.w, e.name.as_deref()) {
            std::mem::swap(&mut label.x, &mut label.y);
            for pt in &mut label.points {
                std::mem::swap(&mut pt.x, &mut pt.y);
            }
        }
    }
}
