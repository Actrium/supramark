//! Edge normalization: breaks long edges into unit-length segments.
//!
//! Ported from dagre.js normalize.ts

use super::types::*;
use super::util::add_dummy_node;
use crate::dagre::graph::Graph;

/// Break long edges into unit-length segments with dummy nodes.
///
/// Pre-conditions:
///   1. Input graph is a DAG
///   2. Each node has a `rank` property
///
/// Post-conditions:
///   1. All edges span exactly 1 rank
///   2. Dummy nodes fill in gaps for longer edges
pub(crate) fn run(g: &mut Graph<NodeLabel, EdgeLabel>, dummy_chains: &mut Vec<String>) {
    let edges: Vec<_> = g.edges();
    for e in edges {
        normalize_edge(
            g,
            &e.v.clone(),
            &e.w.clone(),
            e.name.as_deref(),
            dummy_chains,
        );
    }
}

fn normalize_edge(
    g: &mut Graph<NodeLabel, EdgeLabel>,
    v: &str,
    w: &str,
    name: Option<&str>,
    dummy_chains: &mut Vec<String>,
) {
    let v_rank = g.node(v).and_then(|n| n.rank).unwrap_or(0);
    let w_rank = g.node(w).and_then(|n| n.rank).unwrap_or(0);

    if w_rank == v_rank + 1 {
        return;
    }

    // Get edge label before removing
    let edge_label = g.edge(v, w, name).cloned().unwrap_or_default();
    let edge_weight = edge_label.weight;
    let label_rank = edge_label.label_rank;

    // Remove the original edge
    g.remove_edge(v, w, name);

    let mut prev = v.to_string();
    let mut current_rank = v_rank + 1;
    let mut i = 0;

    while current_rank < w_rank {
        // If this dummy is at the label rank, give it the label's dimensions
        let is_label_rank = label_rank.map(|r| r as i32) == Some(current_rank);
        let (dummy_type, lbl_width, lbl_height, labelpos) = if is_label_rank {
            (
                "edge-label",
                edge_label.width,
                edge_label.height,
                edge_label.labelpos,
            )
        } else {
            ("edge", 0.0, 0.0, LabelPos::default())
        };

        let attrs = NodeLabel {
            width: lbl_width,
            height: lbl_height,
            edge_label: Some(Box::new(edge_label.clone())),
            edge_obj: if let Some(n) = name {
                Some(crate::dagre::graph::Edge::with_name(v, w, n))
            } else {
                Some(crate::dagre::graph::Edge::new(v, w))
            },
            rank: Some(current_rank),
            labelpos,
            ..NodeLabel::default()
        };

        let dummy = add_dummy_node(g, dummy_type, attrs, "_d");

        // Set edge from prev to dummy
        let el = EdgeLabel {
            weight: edge_weight,
            ..EdgeLabel::default()
        };
        g.set_edge(prev.clone(), dummy.clone(), Some(el), name);

        if i == 0 {
            dummy_chains.push(dummy.clone());
        }

        prev = dummy;
        current_rank += 1;
        i += 1;
    }

    // Final edge from last dummy to w
    let el = EdgeLabel {
        weight: edge_weight,
        ..EdgeLabel::default()
    };
    g.set_edge(prev, w.to_string(), Some(el), name);
}

/// Restore original edges, removing dummy nodes and collecting edge points.
pub(crate) fn undo(g: &mut Graph<NodeLabel, EdgeLabel>, dummy_chains: &[String]) {
    for chain_start in dummy_chains {
        let mut v = chain_start.clone();

        let node = match g.node(&v) {
            Some(n) => n.clone(),
            None => continue,
        };

        let edge_obj = match &node.edge_obj {
            Some(e) => e.clone(),
            None => continue,
        };

        let mut orig_label = node
            .edge_label
            .as_ref()
            .map(|l| (**l).clone())
            .unwrap_or_default();

        // Restore the original edge (using the edge name from edge_obj for multi-edges)
        g.set_edge(
            edge_obj.v.clone(),
            edge_obj.w.clone(),
            Some(orig_label.clone()),
            edge_obj.name.as_deref(),
        );

        // Walk the dummy chain, collecting points
        while let Some(current) = g.node(&v).cloned() {
            if current.dummy.is_none() {
                break;
            }

            let succs = g.successors(&v).unwrap_or_default();
            let w = match succs.first() {
                Some(w) => w.clone(),
                None => break,
            };

            g.remove_node(&v);

            if let (Some(x), Some(y)) = (current.x, current.y) {
                orig_label.points.push(Point { x, y });
            }

            if current.dummy.as_deref() == Some("edge-label") {
                orig_label.x = current.x;
                orig_label.y = current.y;
                orig_label.width = current.width;
                orig_label.height = current.height;
            }

            v = w;
        }

        // Update the restored edge with all collected data
        if let Some(label) = g.edge_mut(&edge_obj.v, &edge_obj.w, edge_obj.name.as_deref()) {
            label.points = orig_label.points;
            label.x = orig_label.x;
            label.y = orig_label.y;
            label.width = orig_label.width;
            label.height = orig_label.height;
        }
    }
}
