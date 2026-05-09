//! Build a bipartite layer graph for a single rank.
//!
//! Constructs a compound graph containing all nodes from the requested rank
//! in their original hierarchy, plus edges incident on those nodes
//! (either inEdges or outEdges depending on sweep direction).
//!
//! Non-movable neighbor nodes from the adjacent layer are also included
//! (without hierarchy) so that barycenter calculations have access to their
//! order values.

use crate::graph::{Graph, GraphOptions};
use crate::layout::types::{EdgeLabel, NodeLabel};

/// Relationship direction for building the layer graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Relationship {
    InEdges,
    OutEdges,
}

/// Build a layer graph for the given rank.
///
/// - `g`: the full layout graph (compound)
/// - `rank`: the rank to build the layer for
/// - `relationship`: `InEdges` for down-sweep, `OutEdges` for up-sweep
/// - `nodes_with_rank`: pre-filtered list of nodes belonging to this rank
///
/// Returns a compound graph where:
/// - Movable nodes from `rank` are present with their hierarchy preserved
/// - A synthetic root node parents any top-level movable nodes
/// - Adjacent-layer nodes incident on movable nodes are included (flat)
/// - Edge weights are aggregated for multi-edges
pub(crate) fn build_layer_graph(
    g: &Graph<NodeLabel, EdgeLabel>,
    rank: i32,
    relationship: Relationship,
    nodes_with_rank: &[String],
) -> Graph<NodeLabel, EdgeLabel> {
    let root = create_root_node(g);

    let mut result = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });

    // Store the root name as the graph-level label
    result.set_graph_label(root.clone());

    for v in nodes_with_rank {
        let node = match g.node(v) {
            Some(n) => n,
            None => continue,
        };

        let node_rank = node.rank;
        let node_min_rank = node.min_rank;
        let node_max_rank = node.max_rank;

        // Check if this node belongs to the requested rank
        let in_rank = node_rank == Some(rank)
            || (node_min_rank.is_some()
                && node_max_rank.is_some()
                && node_min_rank.unwrap() <= rank
                && rank <= node_max_rank.unwrap());

        if !in_rank {
            continue;
        }

        result.set_node(v.clone(), Some(node.clone()));

        // Set parent: use original parent if available, otherwise root
        let parent = g.parent(v).unwrap_or(&root).to_string();
        result.set_parent(v, Some(&parent));

        // Gather incident edges based on the relationship direction
        let edges = match relationship {
            Relationship::InEdges => g.in_edges(v, None).unwrap_or_default(),
            Relationship::OutEdges => g.out_edges(v, None).unwrap_or_default(),
        };

        for e in &edges {
            // u is the "other" node (the one in the adjacent layer).
            // For both inEdges and outEdges in the JS code, the edge is
            // always stored as (u, v) in the layer graph.
            let u = if e.v == *v { &e.w } else { &e.v };

            // Make sure the adjacent-layer node exists in the layer graph
            if !result.has_node(u) {
                if let Some(u_label) = g.node(u) {
                    result.set_node(u.clone(), Some(u_label.clone()));
                } else {
                    result.set_node(u.clone(), None);
                }
            }

            // Aggregate edge weight (the layer graph is simple, not multi).
            // Edge direction in the layer graph is always u -> v.
            let existing_weight = result.edge(u, v, None).map_or(0, |l: &EdgeLabel| l.weight);
            let edge_weight = g.edge_by_obj(e).map_or(1, |l| l.weight);

            let el = EdgeLabel {
                weight: edge_weight + existing_weight,
                ..EdgeLabel::default()
            };
            result.set_edge(u.clone(), v.clone(), Some(el), None);
        }

        // If this is a subgraph node (has minRank), set border info for this rank.
        // The border_left/border_right vectors are sparse and indexed by rank
        // (matching dagre.js's `borderLeft[rank]` lookup); empty-string slots
        // are placeholders and must be skipped.
        if node.min_rank.is_some() {
            let r = rank as usize;
            let bl = node.border_left.get(r).filter(|s| !s.is_empty()).cloned();
            let br = node.border_right.get(r).filter(|s| !s.is_empty()).cloned();

            let mut updated = NodeLabel::default();
            if let Some(bl_val) = bl {
                updated.border_left = vec![bl_val];
            }
            if let Some(br_val) = br {
                updated.border_right = vec![br_val];
            }
            result.set_node(v.clone(), Some(updated));
        }
    }

    result
}

/// Get the root pseudo-node ID for a layer graph.
pub(crate) fn get_root(lg: &Graph<NodeLabel, EdgeLabel>) -> String {
    lg.graph_label::<String>().cloned().unwrap_or_default()
}

/// Create a unique root node name that doesn't collide with existing nodes.
fn create_root_node(g: &Graph<NodeLabel, EdgeLabel>) -> String {
    loop {
        let v = crate::util::unique_id("_root");
        if !g.has_node(&v) {
            return v;
        }
    }
}
