//! Rank assignment for nodes in a directed graph.
//!
//! Assigns a rank to each node in the input graph that respects the "minlen"
//! constraint specified on edges between nodes.
//!
//! Derived from Gansner, et al., "A Technique for Drawing Directed Graphs."
//!
//! Pre-conditions:
//!   1. Graph must be a connected DAG.
//!   2. Graph nodes must be objects.
//!   3. Graph edges must have "weight" and "minlen" attributes.
//!
//! Post-conditions:
//!   1. Graph nodes will have a "rank" attribute based on the results of the
//!      algorithm. Ranks can start at any index (including negative), we'll
//!      fix them up later.

pub(crate) mod feasible_tree;
pub(crate) mod network_simplex;
pub(crate) mod util;

use crate::dagre::graph::Graph;
use crate::dagre::layout::types::{EdgeLabel, NodeLabel, Ranker};

/// Assigns ranks to all nodes in the graph using the configured ranker algorithm.
pub(crate) fn rank(g: &mut Graph<NodeLabel, EdgeLabel>, ranker: Ranker) {
    match ranker {
        Ranker::NetworkSimplex => network_simplex_ranker(g),
        Ranker::TightTree => tight_tree_ranker(g),
        Ranker::LongestPath => longest_path_ranker(g),
    }
}

/// A fast and simple ranker, but results are far from optimal.
fn longest_path_ranker(g: &mut Graph<NodeLabel, EdgeLabel>) {
    util::longest_path(g);
}

/// Builds a tight tree after longest path initialization.
fn tight_tree_ranker(g: &mut Graph<NodeLabel, EdgeLabel>) {
    util::longest_path(g);
    feasible_tree::feasible_tree(g);
}

/// Full network simplex for optimal ranking.
fn network_simplex_ranker(g: &mut Graph<NodeLabel, EdgeLabel>) {
    network_simplex::network_simplex(g);
}
