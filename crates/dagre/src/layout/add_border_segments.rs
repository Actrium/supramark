//! Add border segments for compound graph subgraphs.
//!
//! For compound nodes with min_rank/max_rank set, adds border_left and
//! border_right dummy nodes at each rank level and chains them with edges.
//!
//! Ported from dagre.js add-border-segments.ts

use super::types::*;
use super::util::add_border_node;
use crate::graph::Graph;

/// Add border segments (left/right border node chains) for each compound node.
pub(crate) fn add_border_segments(g: &mut Graph<NodeLabel, EdgeLabel>) {
    // Collect compound nodes that have min_rank set (i.e., subgraph nodes)
    let subgraph_nodes: Vec<(String, i32, i32)> = g
        .nodes()
        .into_iter()
        .filter_map(|v| {
            let node = g.node(&v)?;
            let min_rank = node.min_rank?;
            let max_rank = node.max_rank?;
            // Only process nodes that are subgraphs (have children)
            if g.children(Some(&v)).is_empty() {
                return None;
            }
            Some((v, min_rank, max_rank))
        })
        .collect();

    for (v, min_rank, max_rank) in subgraph_nodes {
        for rank in min_rank..=max_rank {
            add_border_node_for(g, &v, BorderType::Left, "_bl", rank);
            add_border_node_for(g, &v, BorderType::Right, "_br", rank);
        }
    }
}

fn add_border_node_for(
    g: &mut Graph<NodeLabel, EdgeLabel>,
    v: &str,
    border_type: BorderType,
    prefix: &str,
    rank: i32,
) {
    let node_id = add_border_node(g, prefix, Some(rank), None);

    // Set the border node's border_type
    if let Some(node) = g.node_mut(&node_id) {
        node.border_type = Some(border_type);
    }

    // Set parent to v
    g.set_parent(&node_id, Some(v));

    // dagre.js v0.8.5 stores borders in a sparse array indexed by *rank*:
    //
    //   var prev = sgNode[prop][rank - 1];
    //   sgNode[prop][rank] = curr;
    //
    // build_layer_graph then looks them up via `borderLeft[rank]`. We
    // mirror that storage shape with a Vec<String> that we grow to length
    // `rank + 1`, leaving any unused slots as empty strings — those slots
    // are skipped by the readers.
    let rank_idx = rank as usize;

    // Get previous border node for this side at rank-1 to chain edges.
    // Mirror dagre.js: only the slot at index `rank-1` is checked.
    let prev_border = {
        let node = g.node(v);
        node.and_then(|n| {
            let list = match border_type {
                BorderType::Left => &n.border_left,
                BorderType::Right => &n.border_right,
            };
            if rank_idx == 0 {
                None
            } else {
                list.get(rank_idx - 1).filter(|s| !s.is_empty()).cloned()
            }
        })
    };

    // Insert at index `rank_idx`, growing the Vec with empty placeholders
    // as needed.
    if let Some(node) = g.node_mut(v) {
        let list = match border_type {
            BorderType::Left => &mut node.border_left,
            BorderType::Right => &mut node.border_right,
        };
        while list.len() <= rank_idx {
            list.push(String::new());
        }
        list[rank_idx] = node_id.clone();
    }

    // Chain edge from previous border node to this one
    if let Some(prev) = prev_border {
        g.set_edge(prev, node_id, Some(EdgeLabel::default()), None);
    }
}
