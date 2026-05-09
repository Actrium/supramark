//! Constructs a spanning tree with tight edges and adjusts the input node's
//! ranks to achieve this. A tight edge is one that has a length that matches
//! its "minlen" attribute.
//!
//! The basic structure for this function is derived from Gansner, et al.,
//! "A Technique for Drawing Directed Graphs."

use crate::dagre::graph::{Edge, Graph, GraphOptions};
use crate::dagre::layout::rank::util::slack;
use crate::dagre::layout::types::{EdgeLabel, NodeLabel};

/// Node label in the spanning tree, carrying DFS numbering for
/// the network simplex algorithm.
#[derive(Debug, Clone, Default)]
pub(crate) struct TreeNodeLabel {
    pub low: Option<i32>,
    pub lim: Option<i32>,
    pub parent: Option<String>,
}

/// Edge label in the spanning tree, carrying the cut value.
#[derive(Debug, Clone, Default)]
pub(crate) struct TreeEdgeLabel {
    pub cutvalue: Option<i32>,
}

/// Constructs a feasible tight spanning tree for the given graph.
///
/// Pre-conditions:
///   1. Graph must be a DAG.
///   2. Graph must be connected.
///   3. Graph must have at least one node.
///   4. Graph nodes must have been previously assigned a "rank" property
///      that respects the "minlen" property of incident edges.
///   5. Graph edges must have a "minlen" property.
///
/// Post-conditions:
///   - Graph nodes will have their rank adjusted to ensure that all tree
///     edges are tight.
///
/// Returns an undirected tree (Graph) constructed using only "tight" edges.
pub(crate) fn feasible_tree(
    g: &mut Graph<NodeLabel, EdgeLabel>,
) -> Graph<TreeNodeLabel, TreeEdgeLabel> {
    let mut tree = Graph::<TreeNodeLabel, TreeEdgeLabel>::with_options(GraphOptions {
        directed: false,
        multigraph: false,
        compound: false,
    });

    let nodes = g.nodes();
    if nodes.is_empty() {
        return tree;
    }

    // Choose arbitrary start node
    let start = &nodes[0];
    let size = g.node_count();
    tree.set_node(start.clone(), Some(TreeNodeLabel::default()));

    while tight_tree(&mut tree, g) < size {
        let edge = match find_min_slack_edge(&tree, g) {
            Some(e) => e,
            None => break,
        };
        let delta = if tree.has_node(&edge.v) {
            slack(g, &edge)
        } else {
            -slack(g, &edge)
        };
        shift_ranks(&tree, g, delta);
    }

    tree
}

/// Finds a maximal tree of tight edges and returns the number of nodes in the tree.
fn tight_tree(
    tree: &mut Graph<TreeNodeLabel, TreeEdgeLabel>,
    g: &Graph<NodeLabel, EdgeLabel>,
) -> usize {
    // Collect current tree nodes so we can iterate without borrow conflicts
    let tree_nodes = tree.nodes();

    fn dfs(
        tree: &mut Graph<TreeNodeLabel, TreeEdgeLabel>,
        g: &Graph<NodeLabel, EdgeLabel>,
        v: &str,
    ) {
        let node_edges = g.node_edges(v, None).unwrap_or_default();
        for e in &node_edges {
            let w = if v == e.v { &e.w } else { &e.v };
            if !tree.has_node(w) && slack(g, e) == 0 {
                tree.set_node(w.clone(), Some(TreeNodeLabel::default()));
                tree.set_edge(v, w.as_str(), Some(TreeEdgeLabel::default()), None);
                dfs(tree, g, w);
            }
        }
    }

    for v in &tree_nodes {
        dfs(tree, g, v);
    }

    tree.node_count()
}

/// Finds the edge with the smallest slack that is incident on the tree
/// (exactly one endpoint in tree, one outside).
fn find_min_slack_edge(
    tree: &Graph<TreeNodeLabel, TreeEdgeLabel>,
    g: &Graph<NodeLabel, EdgeLabel>,
) -> Option<Edge> {
    let mut best_slack = i32::MAX;
    let mut best_edge: Option<Edge> = None;

    for edge in g.edges() {
        // Exactly one endpoint must be in the tree
        if tree.has_node(&edge.v) != tree.has_node(&edge.w) {
            let s = slack(g, &edge);
            if s < best_slack {
                best_slack = s;
                best_edge = Some(edge);
            }
        }
    }

    best_edge
}

/// Shifts ranks of all tree nodes by delta.
fn shift_ranks(
    tree: &Graph<TreeNodeLabel, TreeEdgeLabel>,
    g: &mut Graph<NodeLabel, EdgeLabel>,
    delta: i32,
) {
    for v in tree.nodes() {
        if let Some(label) = g.node_mut(&v) {
            label.rank = Some(label.rank.unwrap() + delta);
        }
    }
}
