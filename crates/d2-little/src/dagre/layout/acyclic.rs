use std::collections::{HashMap, VecDeque};

use crate::dagre::graph::{Edge, Graph};
use crate::dagre::util::unique_id;

use super::types::*;

/// Make the graph acyclic by reversing feedback arc set edges.
///
/// If `acyclicer` is `Some(Greedy)`, uses the greedy heuristic to find the
/// feedback arc set. Otherwise, uses a DFS-based approach.
pub fn run(g: &mut Graph<NodeLabel, EdgeLabel>, acyclicer: Option<Acyclicer>) {
    let fas = match acyclicer {
        Some(Acyclicer::Greedy) => {
            let weight_fn = |e: &Edge| -> i32 {
                g.edge(&e.v, &e.w, e.name.as_deref())
                    .map_or(1, |label| label.weight)
            };
            greedy_fas(g, &weight_fn)
        }
        None => dfs_fas(g),
    };

    for e in fas {
        let label = g
            .remove_edge(&e.v, &e.w, e.name.as_deref())
            .unwrap_or_default();
        let mut label = label;
        label.forward_name = e.name.clone();
        label.reversed = true;
        if g.is_multigraph() {
            let rev_name = unique_id("rev");
            g.set_edge(&e.w, &e.v, Some(label), Some(rev_name.as_str()));
        } else {
            g.set_edge(&e.w, &e.v, Some(label), None);
        }
    }
}

/// Restore reversed edges to their original direction.
pub fn undo(g: &mut Graph<NodeLabel, EdgeLabel>) {
    let reversed_edges: Vec<Edge> = g
        .edges()
        .into_iter()
        .filter(|e| {
            g.edge(&e.v, &e.w, e.name.as_deref())
                .is_some_and(|label| label.reversed)
        })
        .collect();

    for e in reversed_edges {
        let mut label = g
            .remove_edge(&e.v, &e.w, e.name.as_deref())
            .unwrap_or_default();
        let forward_name = label.forward_name.take();
        label.reversed = false;
        g.set_edge(&e.w, &e.v, Some(label), forward_name.as_deref());
    }
}

/// Find feedback arc set using DFS.
fn dfs_fas(g: &Graph<NodeLabel, EdgeLabel>) -> Vec<Edge> {
    let mut fas = Vec::new();
    let mut stack: HashMap<String, bool> = HashMap::new();
    let mut visited: HashMap<String, bool> = HashMap::new();

    fn dfs(
        v: &str,
        g: &Graph<NodeLabel, EdgeLabel>,
        fas: &mut Vec<Edge>,
        stack: &mut HashMap<String, bool>,
        visited: &mut HashMap<String, bool>,
    ) {
        if visited.contains_key(v) {
            return;
        }
        visited.insert(v.to_string(), true);
        stack.insert(v.to_string(), true);

        if let Some(out_edges) = g.out_edges(v, None) {
            for e in out_edges {
                if stack.contains_key(&e.w) {
                    fas.push(e);
                } else {
                    dfs(&e.w, g, fas, stack, visited);
                }
            }
        }

        stack.remove(v);
    }

    for v in g.nodes() {
        dfs(&v, g, &mut fas, &mut stack, &mut visited);
    }

    fas
}

/// Entry for a node in the greedy FAS algorithm, tracking weighted in/out degree.
#[derive(Debug, Clone)]
struct FasEntry {
    in_weight: i32,
    out_weight: i32,
}

/// Find feedback arc set using the greedy heuristic.
///
/// Based on: P. Eades, X. Lin, and W. F. Smyth, "A fast and effective
/// heuristic for the feedback arc set problem." Extended to handle weighted edges.
pub fn greedy_fas(g: &Graph<NodeLabel, EdgeLabel>, weight_fn: &dyn Fn(&Edge) -> i32) -> Vec<Edge> {
    if g.node_count() <= 1 {
        return Vec::new();
    }

    let state = build_state(g, weight_fn);
    let results = do_greedy_fas(&state);

    // Expand multi-edges: map simplified edges back to the original multi-edges
    results
        .into_iter()
        .flat_map(|edge| g.out_edges(&edge.v, Some(&edge.w)).unwrap_or_default())
        .collect()
}

/// Bucket-based doubly-linked list for the greedy FAS algorithm.
///
/// Each bucket is a deque of node indices. Nodes are assigned to buckets based
/// on their out_weight - in_weight differential. Sinks go to bucket 0, sources
/// go to the last bucket, and others are placed by their weight differential.
struct FasState {
    /// The simplified (single-edge) graph: node -> (neighbor -> aggregated weight)
    out_adj: HashMap<String, HashMap<String, i32>>,
    in_adj: HashMap<String, HashMap<String, i32>>,
    /// Per-node FAS entry tracking in/out weight
    entries: HashMap<String, FasEntry>,
    /// Bucket array indexed by (out - in + zero_idx)
    buckets: Vec<VecDeque<String>>,
    /// Offset so that index 0 corresponds to the most negative differential
    zero_idx: usize,
}

fn build_state(g: &Graph<NodeLabel, EdgeLabel>, weight_fn: &dyn Fn(&Edge) -> i32) -> FasState {
    let mut out_adj: HashMap<String, HashMap<String, i32>> = HashMap::new();
    let mut in_adj: HashMap<String, HashMap<String, i32>> = HashMap::new();
    let mut entries: HashMap<String, FasEntry> = HashMap::new();
    let mut max_in: i32 = 0;
    let mut max_out: i32 = 0;

    // Initialize entries for all nodes
    for v in g.nodes() {
        entries.insert(
            v.clone(),
            FasEntry {
                in_weight: 0,
                out_weight: 0,
            },
        );
    }

    // Aggregate weights across multi-edges into single edges on the simplified graph
    for edge in g.edges() {
        let w = weight_fn(&edge);
        *out_adj
            .entry(edge.v.clone())
            .or_default()
            .entry(edge.w.clone())
            .or_insert(0) += w;
        *in_adj
            .entry(edge.w.clone())
            .or_default()
            .entry(edge.v.clone())
            .or_insert(0) += w;

        if let Some(v_entry) = entries.get_mut(&edge.v) {
            v_entry.out_weight += w;
            max_out = max_out.max(v_entry.out_weight);
        }
        if let Some(w_entry) = entries.get_mut(&edge.w) {
            w_entry.in_weight += w;
            max_in = max_in.max(w_entry.in_weight);
        }
    }

    let bucket_count = (max_out + max_in + 3) as usize;
    let zero_idx = (max_in + 1) as usize;
    let mut buckets: Vec<VecDeque<String>> = (0..bucket_count).map(|_| VecDeque::new()).collect();

    // Assign each node to its initial bucket
    for (v, entry) in &entries {
        assign_bucket(&mut buckets, zero_idx, v, entry);
    }

    FasState {
        out_adj,
        in_adj,
        entries,
        buckets,
        zero_idx,
    }
}

fn assign_bucket(buckets: &mut [VecDeque<String>], zero_idx: usize, v: &str, entry: &FasEntry) {
    let idx = if entry.out_weight == 0 {
        // Sink: bucket 0
        0
    } else if entry.in_weight == 0 {
        // Source: last bucket
        buckets.len() - 1
    } else {
        // Place by differential
        ((entry.out_weight - entry.in_weight) as isize + zero_idx as isize) as usize
    };

    if idx < buckets.len() {
        buckets[idx].push_front(v.to_string());
    }
}

fn do_greedy_fas(initial_state: &FasState) -> Vec<Edge> {
    // Clone the mutable state we need
    let mut out_adj = initial_state.out_adj.clone();
    let mut in_adj = initial_state.in_adj.clone();
    let mut entries = initial_state.entries.clone();
    let mut buckets = initial_state.buckets.clone();
    let zero_idx = initial_state.zero_idx;
    let mut remaining: HashMap<String, bool> = entries.keys().map(|k| (k.clone(), true)).collect();

    let mut results: Vec<Edge> = Vec::new();

    while !remaining.is_empty() {
        // Process all sinks (bucket 0)
        while let Some(v) = buckets[0].pop_back() {
            if remaining.remove(&v).is_some() {
                remove_node(
                    &v,
                    &mut out_adj,
                    &mut in_adj,
                    &mut entries,
                    &mut buckets,
                    zero_idx,
                    false,
                    &mut results,
                    &mut remaining,
                );
            }
        }

        // Process all sources (last bucket)
        let last = buckets.len() - 1;
        while let Some(v) = buckets[last].pop_back() {
            if remaining.remove(&v).is_some() {
                remove_node(
                    &v,
                    &mut out_adj,
                    &mut in_adj,
                    &mut entries,
                    &mut buckets,
                    zero_idx,
                    false,
                    &mut results,
                    &mut remaining,
                );
            }
        }

        if !remaining.is_empty() {
            // Find the highest differential bucket with an entry
            for i in (1..buckets.len() - 1).rev() {
                if let Some(v) = pop_valid_entry(&mut buckets[i], &remaining) {
                    remaining.remove(&v);
                    remove_node(
                        &v,
                        &mut out_adj,
                        &mut in_adj,
                        &mut entries,
                        &mut buckets,
                        zero_idx,
                        true,
                        &mut results,
                        &mut remaining,
                    );
                    break;
                }
            }
        }
    }

    results
}

/// Pop the next valid (still remaining) entry from a bucket.
fn pop_valid_entry(
    bucket: &mut VecDeque<String>,
    remaining: &HashMap<String, bool>,
) -> Option<String> {
    while let Some(v) = bucket.pop_back() {
        if remaining.contains_key(&v) {
            return Some(v);
        }
    }
    None
}

/// Remove a node from the FAS graph, updating neighbors' weights and bucket assignments.
/// If `collect_predecessors` is true, record incoming edges as feedback arcs.
#[allow(clippy::too_many_arguments)]
fn remove_node(
    v: &str,
    out_adj: &mut HashMap<String, HashMap<String, i32>>,
    in_adj: &mut HashMap<String, HashMap<String, i32>>,
    entries: &mut HashMap<String, FasEntry>,
    buckets: &mut [VecDeque<String>],
    zero_idx: usize,
    collect_predecessors: bool,
    results: &mut Vec<Edge>,
    remaining: &mut HashMap<String, bool>,
) {
    // Process in-edges: for each predecessor u, reduce u's out_weight
    if let Some(predecessors) = in_adj.remove(v) {
        for (u, weight) in &predecessors {
            if !remaining.contains_key(u) {
                continue;
            }
            if collect_predecessors {
                results.push(Edge::new(u.clone(), v));
            }
            if let Some(u_entry) = entries.get_mut(u) {
                u_entry.out_weight -= weight;
                assign_bucket(buckets, zero_idx, u, u_entry);
            }
        }
    }

    // Process out-edges: for each successor w, reduce w's in_weight
    if let Some(successors) = out_adj.remove(v) {
        for (w, weight) in &successors {
            if !remaining.contains_key(w) {
                continue;
            }
            if let Some(w_entry) = entries.get_mut(w) {
                w_entry.in_weight -= weight;
                assign_bucket(buckets, zero_idx, w, w_entry);
            }
        }
    }

    // Also clean up reverse adjacency references
    if entries.get(v).is_some() {
        in_adj.values_mut().for_each(|m| {
            m.remove(v);
        });
        out_adj.values_mut().for_each(|m| {
            m.remove(v);
        });
    }

    entries.remove(v);
}
