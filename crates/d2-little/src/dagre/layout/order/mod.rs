//! Ordering phase: minimize edge crossings via the Sugiyama heuristic.
//!
//! Applies barycenter-based heuristics with alternating up/down sweeps
//! to minimize edge crossings in a layered graph layout.
//!
//! Pre-conditions:
//!   1. Graph must be a DAG
//!   2. Graph nodes must have a `rank` attribute
//!   3. Graph edges must have a `weight` attribute
//!
//! Post-conditions:
//!   1. Graph nodes will have an `order` attribute based on the results.

pub(crate) mod add_subgraph_constraints;
pub(crate) mod barycenter;
pub(crate) mod build_layer_graph;
pub(crate) mod cross_count;
pub(crate) mod init_order;
pub(crate) mod resolve_conflicts;
pub(crate) mod sort;
pub(crate) mod sort_subgraph;

use std::collections::HashMap;

use crate::dagre::graph::Graph;
use crate::dagre::layout::types::{EdgeLabel, GraphLabel, NodeLabel};
use crate::dagre::layout::util::{build_layer_matrix, max_rank};

use self::add_subgraph_constraints::add_subgraph_constraints;
use self::build_layer_graph::{Relationship, build_layer_graph, get_root};
use self::cross_count::cross_count;
use self::init_order::init_order;
use self::sort_subgraph::sort_subgraph;

/// Assign `order` to each node, minimizing edge crossings.
///
/// This is the main entry point for the ordering phase.
pub(crate) fn order(g: &mut Graph<NodeLabel, EdgeLabel>) {
    let mr = max_rank(g);
    if mr < 0 {
        return;
    }
    let tie_keep_first = g
        .graph_label::<GraphLabel>()
        .is_some_and(|gl| gl.tie_keep_first);

    // Rank lists for down-sweeps (ranks 1..=maxRank) and
    // up-sweeps (ranks maxRank-1..=0).
    let down_ranks: Vec<i32> = (1..=mr).collect();
    let up_ranks: Vec<i32> = (0..mr).rev().collect();

    // Build an index: rank -> Vec<node_id> (used to rebuild layer graphs each sweep)
    let mut nodes_by_rank: HashMap<i32, Vec<String>> = HashMap::new();
    for v in g.nodes() {
        if let Some(node) = g.node(&v) {
            if let Some(rank) = node.rank {
                nodes_by_rank.entry(rank).or_default().push(v.clone());
            }
            if let (Some(min_r), Some(max_r)) = (node.min_rank, node.max_rank) {
                for r in min_r..=max_r {
                    if Some(r) != node.rank {
                        nodes_by_rank.entry(r).or_default().push(v.clone());
                    }
                }
            }
        }
    }

    // Initial ordering via DFS
    let layering = init_order(g);
    assign_order(g, &layering);

    let mut best_cc = usize::MAX;
    let mut best: Option<Vec<Vec<String>>> = None;

    // Match dagre.js: `for (let i = 0, lastBest = 0; lastBest < 4; ++i, ++lastBest)`
    // lastBest is incremented every iteration (including after improvement reset to 0).
    let mut i = 0_usize;
    let mut last_best = 0_usize;
    while last_best < 4 {
        let (ranks, relationship) = if !i.is_multiple_of(2) {
            (&down_ranks, Relationship::InEdges)
        } else {
            (&up_ranks, Relationship::OutEdges)
        };
        let bias_right = i % 4 >= 2;

        sweep_layer_graphs(g, ranks, &nodes_by_rank, relationship, bias_right);

        let layering = build_layer_matrix(g);
        let cc = cross_count(g, &layering);

        if cc < best_cc {
            last_best = 0;
            best = Some(layering.clone());
            best_cc = cc;
        } else if cc == best_cc && !tie_keep_first {
            // dagre.js v3.0.1-pre: when tied, replace best with current layering.
            // dagre.js v0.8.5 (used by Go d2) instead keeps the earliest tied
            // layering — opt into that behavior via `tie_keep_first` on
            // LayoutOptions.
            best = Some(layering);
        }

        i += 1;
        last_best += 1;
    }

    if let Some(ref best_layering) = best {
        assign_order(g, best_layering);
    }
}

/// Execute one sweep across the layer graphs, sorting each layer and
/// accumulating subgraph constraints.
///
/// In dagre.js, layer graphs share node objects by reference with the main
/// graph. When order values are updated on one, they're visible through the
/// other. In Rust, node labels are cloned, so we rebuild each layer graph
/// from the current main graph to ensure up-to-date order values propagate
/// between layers within a single sweep.
fn sweep_layer_graphs(
    g: &mut Graph<NodeLabel, EdgeLabel>,
    ranks: &[i32],
    nodes_by_rank: &std::collections::HashMap<i32, Vec<String>>,
    relationship: Relationship,
    bias_right: bool,
) {
    let mut cg: Graph<(), ()> = Graph::new();

    for &rank in ranks {
        let empty = Vec::new();
        let nodes = nodes_by_rank.get(&rank).unwrap_or(&empty);
        let lg = build_layer_graph(g, rank, relationship, nodes);

        let root = get_root(&lg);
        let sorted = sort_subgraph(&lg, &root, &cg, bias_right);

        // Assign order values back to the main graph
        for (i, v) in sorted.vs.iter().enumerate() {
            if let Some(node) = g.node_mut(v) {
                node.order = Some(i);
            }
        }

        add_subgraph_constraints(&lg, &mut cg, &sorted.vs);
    }
}

/// Write `order` values into the graph from a layering matrix.
fn assign_order(g: &mut Graph<NodeLabel, EdgeLabel>, layering: &[Vec<String>]) {
    for layer in layering {
        for (i, v) in layer.iter().enumerate() {
            if !v.is_empty()
                && let Some(node) = g.node_mut(v)
            {
                node.order = Some(i);
            }
        }
    }
}
