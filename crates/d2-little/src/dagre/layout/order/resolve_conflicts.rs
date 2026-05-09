//! Constraint conflict resolution for ordering.
//!
//! Given a list of barycenter entries and a constraint graph, resolves conflicts
//! by coalescing constrained nodes that would violate their ordering.
//!
//! Based on Forster, "A Fast and Simple Heuristic for Constrained Two-Level
//! Crossing Reduction."

use std::collections::HashMap;

use super::barycenter::BarycenterEntry;
use crate::dagre::graph::Graph;

/// A resolved entry: a group of nodes (possibly merged) with an index and
/// optional barycenter/weight.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedEntry {
    pub vs: Vec<String>,
    pub i: usize,
    pub barycenter: Option<f64>,
    pub weight: Option<i32>,
}

/// Internal mutable entry used during conflict resolution.
struct MappedEntry {
    indegree: usize,
    #[allow(clippy::vec_box)]
    in_entries: Vec<usize>, // indices into entries vec
    out_entries: Vec<usize>, // indices into entries vec
    vs: Vec<String>,
    i: usize,
    barycenter: Option<f64>,
    weight: Option<i32>,
    merged: bool,
}

/// Resolve conflicts between barycenter ordering and constraint edges.
///
/// Nodes connected by constraint edges that would violate barycenter ordering
/// are coalesced into single entries that respect the constraints.
pub(crate) fn resolve_conflicts(
    entries: &[BarycenterEntry],
    cg: &Graph<(), ()>,
) -> Vec<ResolvedEntry> {
    // Build mapped entries keyed by node name
    let mut mapped: HashMap<String, usize> = HashMap::new();
    let mut all_entries: Vec<MappedEntry> = Vec::with_capacity(entries.len());

    for (i, entry) in entries.iter().enumerate() {
        let me = MappedEntry {
            indegree: 0,
            in_entries: Vec::new(),
            out_entries: Vec::new(),
            vs: vec![entry.v.clone()],
            i,
            barycenter: entry.barycenter,
            weight: entry.weight,
            merged: false,
        };
        all_entries.push(me);
        mapped.insert(entry.v.clone(), i);
    }

    // Wire up constraint edges
    for e in cg.edges() {
        if let (Some(&v_idx), Some(&w_idx)) = (mapped.get(&e.v), mapped.get(&e.w)) {
            all_entries[w_idx].indegree += 1;
            all_entries[v_idx].out_entries.push(w_idx);
            // We store the back-reference so handle_in can find predecessors
            all_entries[w_idx].in_entries.push(v_idx);
        }
    }

    // Collect source set (entries with zero indegree)
    let mut source_set: Vec<usize> = all_entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.indegree == 0)
        .map(|(i, _)| i)
        .collect();

    do_resolve_conflicts(&mut all_entries, &mut source_set)
}

fn do_resolve_conflicts(
    entries: &mut [MappedEntry],
    source_set: &mut Vec<usize>,
) -> Vec<ResolvedEntry> {
    let mut result_order: Vec<usize> = Vec::new();

    while let Some(v_idx) = source_set.pop() {
        result_order.push(v_idx);

        // handle_in: try to merge predecessors into this entry
        let in_entries: Vec<usize> = entries[v_idx].in_entries.clone();
        for &u_idx in in_entries.iter().rev() {
            if entries[u_idx].merged {
                continue;
            }
            let u_bc = entries[u_idx].barycenter;
            let v_bc = entries[v_idx].barycenter;
            if u_bc.is_none() || v_bc.is_none() || u_bc.unwrap() >= v_bc.unwrap() {
                // Merge u into v
                merge_entries(entries, v_idx, u_idx);
            }
        }

        // handle_out: decrement indegree of successors, add to source_set if zero
        let out_entries: Vec<usize> = entries[v_idx].out_entries.clone();
        for &w_idx in &out_entries {
            // Add back-reference from w to v
            entries[w_idx].in_entries.push(v_idx);
            entries[w_idx].indegree -= 1;
            if entries[w_idx].indegree == 0 {
                source_set.push(w_idx);
            }
        }
    }

    result_order
        .iter()
        .filter(|&&idx| !entries[idx].merged)
        .map(|&idx| {
            let e = &entries[idx];
            ResolvedEntry {
                vs: e.vs.clone(),
                i: e.i,
                barycenter: e.barycenter,
                weight: e.weight,
            }
        })
        .collect()
}

/// Merge source entry into target entry, combining barycenters and weights.
fn merge_entries(entries: &mut [MappedEntry], target: usize, source: usize) {
    let mut sum = 0.0_f64;
    let mut weight = 0_i32;

    if let Some(tw) = entries[target].weight
        && tw > 0
    {
        sum += entries[target].barycenter.unwrap_or(0.0) * tw as f64;
        weight += tw;
    }

    if let Some(sw) = entries[source].weight
        && sw > 0
    {
        sum += entries[source].barycenter.unwrap_or(0.0) * sw as f64;
        weight += sw;
    }

    // Prepend source's vs to target's vs (source.vs.concat(target.vs) in JS)
    let source_vs = entries[source].vs.clone();
    let mut new_vs = source_vs;
    new_vs.append(&mut entries[target].vs);
    entries[target].vs = new_vs;

    if weight > 0 {
        entries[target].barycenter = Some(sum / weight as f64);
        entries[target].weight = Some(weight);
    }

    entries[target].i = entries[target].i.min(entries[source].i);
    entries[source].merged = true;
}
