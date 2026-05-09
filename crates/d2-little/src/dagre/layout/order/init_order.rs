//! Initial node ordering via DFS.
//!
//! Assigns an initial order value for each node by performing a DFS search
//! starting from nodes in the first rank. Nodes are assigned an order in their
//! rank as they are first visited.
//!
//! This approach comes from Gansner, et al., "A Technique for Drawing Directed Graphs."

use std::collections::HashSet;

use crate::dagre::graph::Graph;
use crate::dagre::layout::types::{EdgeLabel, NodeLabel};

/// Produce an initial layering matrix by DFS from lowest-rank nodes.
///
/// Returns a Vec of layers (indexed by rank), each layer containing node IDs
/// in the order they were first visited.
pub(crate) fn init_order(g: &Graph<NodeLabel, EdgeLabel>) -> Vec<Vec<String>> {
    let mut visited = HashSet::new();

    // Collect "simple" nodes (those with no compound children)
    let mut simple_nodes: Vec<String> = g
        .nodes()
        .into_iter()
        .filter(|v| g.children(Some(v)).is_empty())
        .collect();

    // Determine max rank among simple nodes
    let max_rank = simple_nodes
        .iter()
        .filter_map(|v| g.node(v).and_then(|n| n.rank))
        .max()
        .unwrap_or(0);

    let mut layers: Vec<Vec<String>> = vec![vec![]; (max_rank + 1) as usize];

    // Sort simple nodes by rank (ascending) so DFS starts from top ranks
    simple_nodes.sort_by_key(|v| g.node(v).and_then(|n| n.rank).unwrap_or(0));

    fn dfs(
        g: &Graph<NodeLabel, EdgeLabel>,
        v: &str,
        visited: &mut HashSet<String>,
        layers: &mut Vec<Vec<String>>,
    ) {
        if visited.contains(v) {
            return;
        }
        visited.insert(v.to_string());

        if let Some(node) = g.node(v)
            && let Some(rank) = node.rank
        {
            let r = rank as usize;
            if r < layers.len() {
                layers[r].push(v.to_string());
            }
        }

        if let Some(successors) = g.successors(v) {
            for w in successors {
                dfs(g, &w, visited, layers);
            }
        }
    }

    for v in &simple_nodes {
        dfs(g, v, &mut visited, &mut layers);
    }

    layers
}
