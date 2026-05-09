//! Bilayer edge crossing counter.
//!
//! Given a layering (an array of layers, each with an array of ordered nodes)
//! and a graph, returns a weighted crossing count.
//!
//! This algorithm is derived from Barth, et al., "Bilayer Cross Counting."

use std::collections::HashMap;

use crate::dagre::graph::Graph;
use crate::dagre::layout::types::{EdgeLabel, NodeLabel};

/// Count the total number of weighted edge crossings across all adjacent layers.
pub(crate) fn cross_count(g: &Graph<NodeLabel, EdgeLabel>, layering: &[Vec<String>]) -> usize {
    let mut cc = 0;
    for i in 1..layering.len() {
        cc += two_layer_cross_count(g, &layering[i - 1], &layering[i]);
    }
    cc
}

/// Count weighted crossings between a north layer and a south layer.
///
/// Uses the accumulator tree technique from Barth et al.
fn two_layer_cross_count(
    g: &Graph<NodeLabel, EdgeLabel>,
    north_layer: &[String],
    south_layer: &[String],
) -> usize {
    // Build position index for the south layer
    let south_pos: HashMap<&str, usize> = south_layer
        .iter()
        .enumerate()
        .map(|(i, v)| (v.as_str(), i))
        .collect();

    // Collect south entries: for each north node (in order), gather outgoing edges
    // sorted by their south position, paired with edge weight.
    let mut south_entries: Vec<(usize, i32)> = Vec::new();
    for v in north_layer {
        if let Some(edges) = g.out_edges(v, None) {
            let mut entries: Vec<(usize, i32)> = edges
                .iter()
                .filter_map(|e| {
                    let pos = south_pos.get(e.w.as_str())?;
                    let weight = g.edge_by_obj(e).map_or(1, |l| l.weight);
                    Some((*pos, weight))
                })
                .collect();
            entries.sort_by_key(|&(pos, _)| pos);
            south_entries.extend(entries);
        }
    }

    if south_layer.is_empty() {
        return 0;
    }

    // Build the accumulator tree
    let mut first_index: usize = 1;
    while first_index < south_layer.len() {
        first_index <<= 1;
    }
    let tree_size = 2 * first_index - 1;
    first_index -= 1;
    let mut tree = vec![0i64; tree_size];

    // Calculate the weighted crossings
    let mut cc: usize = 0;
    for (pos, weight) in &south_entries {
        let weight = *weight as i64;
        let mut index = pos + first_index;
        tree[index] += weight;
        let mut weight_sum: i64 = 0;
        while index > 0 {
            if !index.is_multiple_of(2) {
                // Left child: add the right sibling's value
                weight_sum += tree[index + 1];
            }
            index = (index - 1) >> 1;
            tree[index] += weight;
        }
        cc += (weight * weight_sum) as usize;
    }

    cc
}
