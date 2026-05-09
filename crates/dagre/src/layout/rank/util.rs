//! Rank utilities: longest path initial ranking and slack computation.

use crate::graph::{Edge, Graph};
use crate::layout::types::{EdgeLabel, NodeLabel};
use std::collections::BTreeSet;

/// Initializes ranks for the input graph using the longest path algorithm.
///
/// Nodes are pushed to the lowest layer possible, leaving the bottom
/// ranks wide and edges longer than necessary. However, due to its speed,
/// this algorithm is good for getting an initial ranking that can be fed
/// into other algorithms.
///
/// Pre-conditions:
///   1. Input graph is a DAG.
///   2. Input graph node labels can be assigned properties.
///
/// Post-conditions:
///   1. Each node will be assigned an (unnormalized) "rank" property.
pub(crate) fn longest_path(g: &mut Graph<NodeLabel, EdgeLabel>) {
    let mut visited = BTreeSet::new();

    fn dfs(g: &mut Graph<NodeLabel, EdgeLabel>, v: &str, visited: &mut BTreeSet<String>) -> i32 {
        if visited.contains(v) {
            // We've already visited and assigned a rank to v on a previous
            // path; just return it. Defaulting to 0 if either lookup fails
            // (exotic call topology) keeps the recursion total.
            return g.node(v).and_then(|n| n.rank).unwrap_or(0);
        }
        visited.insert(v.to_string());

        let out_edges = g.out_edges(v, None).unwrap_or_default();
        let min_rank: Option<i32> = out_edges
            .iter()
            .map(|e| {
                let minlen = g
                    .edge(&e.v, &e.w, e.name.as_deref())
                    .map(|l| l.minlen)
                    .unwrap_or(1);
                dfs(g, &e.w, visited) - minlen
            })
            .min();

        let rank = min_rank.unwrap_or(0);
        if let Some(node) = g.node_mut(v) {
            node.rank = Some(rank);
        }
        rank
    }

    let sources = g.sources();
    for v in &sources {
        dfs(g, v, &mut visited);
    }
}

/// Returns the amount of slack for the given edge.
///
/// Slack is defined as the difference between the length of the edge
/// and its minimum length:
///   slack(g, e) = rank(e.w) - rank(e.v) - minlen
///
/// If either endpoint is missing or has no rank assigned (only possible
/// when an upstream caller invokes this on a partially-ranked graph),
/// this returns `0` rather than panicking — consistent with treating
/// the edge as already taut.
pub(crate) fn slack(g: &Graph<NodeLabel, EdgeLabel>, e: &Edge) -> i32 {
    let Some(w_rank) = g.node(&e.w).and_then(|n| n.rank) else {
        return 0;
    };
    let Some(v_rank) = g.node(&e.v).and_then(|n| n.rank) else {
        return 0;
    };
    let minlen = g
        .edge(&e.v, &e.w, e.name.as_deref())
        .map(|l| l.minlen)
        .unwrap_or(1);
    w_rank - v_rank - minlen
}
