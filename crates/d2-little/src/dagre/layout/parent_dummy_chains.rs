//! Parent dummy chains: assigns correct parent subgraph to dummy nodes
//! along edges that cross subgraph boundaries.
//!
//! Ported from dagre.js parent-dummy-chains.ts

use super::types::*;
use crate::dagre::graph::Graph;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct PostorderNum {
    low: usize,
    lim: usize,
}

struct PathData {
    path: Vec<Option<String>>,
    lca: Option<String>,
}

/// Walk dummy chains and set their parent to the appropriate subgraph
/// using LCA-based path finding with postorder numbering.
pub(crate) fn parent_dummy_chains(g: &mut Graph<NodeLabel, EdgeLabel>) {
    let dummy_chains: Vec<String> = g
        .graph_label::<GraphLabel>()
        .map(|gl| gl.dummy_chains.clone())
        .unwrap_or_default();

    if dummy_chains.is_empty() {
        return;
    }

    // Build postorder numbering for the compound hierarchy
    let postorder_nums = postorder(g);

    for chain_start in &dummy_chains {
        let mut v = chain_start.clone();

        // Get the edge endpoints for the original edge this dummy chain represents
        let node = match g.node(&v) {
            Some(n) => n.clone(),
            None => continue,
        };
        let edge_obj = match &node.edge_obj {
            Some(e) => e.clone(),
            None => continue,
        };

        // Find LCA path between the source and target
        let path_data = find_path(g, &postorder_nums, &edge_obj.v, &edge_obj.w);
        let path = path_data.path;
        let lca = path_data.lca;
        let mut path_idx: usize = 0;
        let mut ascending = true;

        // Walk the dummy chain: v starts at the first dummy, ends when we reach edgeObj.w
        while v != edge_obj.w {
            let node = match g.node(&v) {
                Some(n) => n.clone(),
                None => break,
            };

            if ascending {
                // Walk up the ascending portion of the path
                while path_idx < path.len() {
                    let path_v = &path[path_idx];
                    if path_v == &lca {
                        // Reached the LCA, switch to descending
                        ascending = false;
                        break;
                    }
                    if let Some(pv) = path_v
                        && let Some(pn) = g.node(pv)
                        && pn.max_rank.unwrap_or(0) >= node.rank.unwrap_or(0)
                    {
                        break;
                    }
                    path_idx += 1;
                }

                if path[path_idx] == lca {
                    ascending = false;
                }
            }

            if !ascending {
                // Walk down the descending portion of the path
                while path_idx < path.len() - 1 {
                    let next = &path[path_idx + 1];
                    if let Some(nv) = next {
                        if let Some(nn) = g.node(nv) {
                            if nn.min_rank.unwrap_or(0) <= node.rank.unwrap_or(0) {
                                path_idx += 1;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }

            // Set parent for the current dummy node
            if let Some(Some(pv)) = path.get(path_idx) {
                g.set_parent(&v, Some(pv));
            }

            // Move to the next node in the chain
            let succs = g.successors(&v).unwrap_or_default();
            if succs.is_empty() {
                break;
            }
            v = succs[0].clone();
        }
    }
}

/// Build postorder numbering of the compound hierarchy.
fn postorder(g: &Graph<NodeLabel, EdgeLabel>) -> HashMap<String, PostorderNum> {
    let mut result: HashMap<String, PostorderNum> = HashMap::new();
    let mut lim: usize = 0;

    fn dfs(
        g: &Graph<NodeLabel, EdgeLabel>,
        v: &str,
        lim: &mut usize,
        result: &mut HashMap<String, PostorderNum>,
    ) {
        let low = *lim;
        let children = g.children(Some(v));
        for child in &children {
            dfs(g, child, lim, result);
        }
        result.insert(v.to_string(), PostorderNum { low, lim: *lim });
        *lim += 1;
    }

    let roots = g.children(None);
    for root in &roots {
        dfs(g, root, &mut lim, &mut result);
    }

    result
}

/// Find the path from v to w through their LCA in the compound hierarchy.
fn find_path(
    g: &Graph<NodeLabel, EdgeLabel>,
    postorder_nums: &HashMap<String, PostorderNum>,
    v: &str,
    w: &str,
) -> PathData {
    let mut v_path: Vec<Option<String>> = Vec::new();
    let mut w_path: Vec<Option<String>> = Vec::new();

    let v_nums = postorder_nums.get(v);
    let w_nums = postorder_nums.get(w);
    let low = std::cmp::min(
        v_nums.map(|n| n.low).unwrap_or(0),
        w_nums.map(|n| n.low).unwrap_or(0),
    );
    let lim = std::cmp::max(
        v_nums.map(|n| n.lim).unwrap_or(0),
        w_nums.map(|n| n.lim).unwrap_or(0),
    );

    // Traverse up from v to find the LCA
    let mut parent: Option<String> = Some(v.to_string());
    loop {
        parent = g
            .parent(parent.as_deref().unwrap_or(""))
            .map(|s| s.to_string());
        v_path.push(parent.clone());
        if let Some(ref p) = parent {
            if let Some(nums) = postorder_nums.get(p) {
                if nums.low <= low && lim <= nums.lim {
                    break;
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }
    let lca = parent;

    // Traverse from w to LCA
    let mut w_parent = w.to_string();
    loop {
        let p = g.parent(&w_parent).map(|s| s.to_string());
        match p {
            Some(ref pv) if Some(pv.clone()) != lca.as_ref().cloned().or(None) => {
                w_path.push(Some(pv.clone()));
                w_parent = pv.clone();
            }
            _ => break,
        }
    }

    // Combine: vPath + reversed wPath
    w_path.reverse();
    let mut combined = v_path;
    combined.extend(w_path);

    PathData {
        path: combined,
        lca,
    }
}
