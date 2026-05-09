//! Recursive subgraph sorting for crossing reduction.
//!
//! Sorts the children of a compound node by computing barycenters,
//! resolving constraint conflicts, and recursively handling nested subgraphs.

use crate::dagre::graph::Graph;
use crate::dagre::layout::types::{EdgeLabel, NodeLabel};

use super::barycenter::{self, BarycenterEntry};
use super::resolve_conflicts;
use super::sort;

/// Result of sorting a subgraph: the ordered node list and optional
/// aggregate barycenter/weight.
#[derive(Debug, Clone)]
pub(crate) struct SubgraphResult {
    pub vs: Vec<String>,
    pub barycenter: Option<f64>,
    pub weight: Option<i32>,
}

/// Recursively sort the children of node `v` in the layer graph `g`.
///
/// - `g`: the layer graph (compound)
/// - `v`: the subgraph/root node whose children we're sorting
/// - `cg`: the constraint graph (edges encode "must come before" relationships)
/// - `bias_right`: if true, break ties by favoring right placement
pub(crate) fn sort_subgraph(
    g: &Graph<NodeLabel, EdgeLabel>,
    v: &str,
    cg: &Graph<(), ()>,
    bias_right: bool,
) -> SubgraphResult {
    let mut movable: Vec<String> = g.children(Some(v));
    let node = g.node(v);
    let bl: Option<String> = node.and_then(|n| n.border_left.first().cloned());
    let br: Option<String> = node.and_then(|n| n.border_right.first().cloned());

    // Exclude border nodes from the movable set
    if bl.is_some() {
        movable.retain(|w| Some(w.as_str()) != bl.as_deref() && Some(w.as_str()) != br.as_deref());
    }

    // Compute barycenters for the movable nodes
    let mut barycenters = barycenter::barycenter(g, &movable);

    // For each entry that is itself a subgraph, recursively sort it and merge
    let mut subgraph_results: std::collections::HashMap<String, SubgraphResult> =
        std::collections::HashMap::new();

    for entry in &mut barycenters {
        let children = g.children(Some(&entry.v));
        if !children.is_empty() {
            // This node is a subgraph; recursively sort it
            let sub_result = sort_subgraph(g, &entry.v, cg, bias_right);
            subgraph_results.insert(entry.v.clone(), sub_result.clone());

            // Merge the sub-result's barycenter into this entry
            if sub_result.barycenter.is_some() {
                merge_barycenters(entry, &sub_result);
            }
        }
    }

    // Resolve conflicts between barycenters and constraint graph
    let mut entries = resolve_conflicts::resolve_conflicts(&barycenters, cg);

    // Expand any subgraph entries: replace the subgraph node with its sorted children
    for entry in &mut entries {
        entry.vs = entry
            .vs
            .iter()
            .flat_map(|node_v| {
                if let Some(sub) = subgraph_results.get(node_v) {
                    sub.vs.clone()
                } else {
                    vec![node_v.clone()]
                }
            })
            .collect();
    }

    // Final sort by barycenter
    let mut result = sort::sort(&entries, bias_right);

    // If there are border nodes, sandwich them around the result
    if let (Some(bl_v), Some(br_v)) = (&bl, &br) {
        // Insert border left at the beginning, border right at the end
        let mut final_vs = vec![bl_v.clone()];
        final_vs.extend(result.vs);
        final_vs.push(br_v.clone());
        result.vs = final_vs;

        // Adjust barycenter to include border predecessors
        let bl_preds = g.predecessors(bl_v).unwrap_or_default();
        if !bl_preds.is_empty() {
            let bl_pred_order = g.node(&bl_preds[0]).and_then(|n| n.order).unwrap_or(0) as f64;
            let br_preds = g.predecessors(br_v).unwrap_or_default();
            let br_pred_order = if !br_preds.is_empty() {
                g.node(&br_preds[0]).and_then(|n| n.order).unwrap_or(0) as f64
            } else {
                0.0
            };

            if result.barycenter.is_none() {
                result.barycenter = Some(0.0);
                result.weight = Some(0);
            }

            let bc = result.barycenter.unwrap();
            let w = result.weight.unwrap();
            result.barycenter =
                Some((bc * w as f64 + bl_pred_order + br_pred_order) / (w + 2) as f64);
            result.weight = Some(w + 2);
        }
    }

    SubgraphResult {
        vs: result.vs,
        barycenter: result.barycenter,
        weight: result.weight,
    }
}

/// Merge a subgraph result's barycenter into a parent barycenter entry.
fn merge_barycenters(target: &mut BarycenterEntry, other: &SubgraphResult) {
    if let (Some(t_bc), Some(t_w)) = (target.barycenter, target.weight) {
        let o_bc = other.barycenter.unwrap_or(0.0);
        let o_w = other.weight.unwrap_or(0);
        let total_w = t_w + o_w;
        if total_w > 0 {
            target.barycenter = Some((t_bc * t_w as f64 + o_bc * o_w as f64) / total_w as f64);
            target.weight = Some(total_w);
        }
    } else {
        target.barycenter = other.barycenter;
        target.weight = other.weight;
    }
}
