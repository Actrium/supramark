//! Graph algorithms: topological sort, DFS traversal, connected components, etc.

use super::{Edge, Graph};
use std::collections::{HashMap, HashSet, VecDeque};

/// Type alias for an optional edge-traversal function used in shortest-path algorithms.
pub type EdgeFn<'a> = Option<&'a dyn Fn(&str) -> Vec<Edge>>;

/// Error returned when a cycle is detected during topological sort.
#[derive(Debug)]
pub struct CycleError;

impl std::fmt::Display for CycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Graph contains a cycle")
    }
}

impl std::error::Error for CycleError {}

/// Topological sort of a directed graph. Returns an error if the graph has a cycle.
pub fn topsort<N, E>(g: &Graph<N, E>) -> Result<Vec<String>, CycleError> {
    let mut visited = HashSet::new();
    let mut stack = HashSet::new();
    let mut result = Vec::new();

    fn visit<N, E>(
        g: &Graph<N, E>,
        v: &str,
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
        result: &mut Vec<String>,
    ) -> Result<(), CycleError> {
        if stack.contains(v) {
            return Err(CycleError);
        }
        if visited.contains(v) {
            return Ok(());
        }
        stack.insert(v.to_string());
        visited.insert(v.to_string());

        if let Some(preds) = g.predecessors(v) {
            for pred in preds {
                visit(g, &pred, visited, stack, result)?;
            }
        }

        stack.remove(v);
        result.push(v.to_string());
        Ok(())
    }

    let nodes = g.nodes();
    for v in &nodes {
        visit(g, v, &mut visited, &mut stack, &mut result)?;
    }

    Ok(result)
}

/// Check if a directed graph is acyclic.
pub fn is_acyclic<N, E>(g: &Graph<N, E>) -> bool {
    topsort(g).is_ok()
}

/// Find all cycles in the graph. Returns a list of cycles, each being a list of node IDs.
pub fn find_cycles<N, E>(g: &Graph<N, E>) -> Vec<Vec<String>> {
    let sccs = tarjan(g);
    sccs.into_iter()
        .filter(|scc| {
            scc.len() > 1 || {
                let v = &scc[0];
                g.has_edge(v, v, None)
            }
        })
        .collect()
}

/// Tarjan's strongly connected components algorithm.
pub fn tarjan<N, E>(g: &Graph<N, E>) -> Vec<Vec<String>> {
    struct TarjanState {
        index: u32,
        stack: Vec<String>,
        on_stack: HashSet<String>,
        indices: HashMap<String, u32>,
        lowlinks: HashMap<String, u32>,
        result: Vec<Vec<String>>,
    }

    fn strongconnect<N, E>(g: &Graph<N, E>, v: &str, state: &mut TarjanState) {
        state.indices.insert(v.to_string(), state.index);
        state.lowlinks.insert(v.to_string(), state.index);
        state.index += 1;
        state.stack.push(v.to_string());
        state.on_stack.insert(v.to_string());

        if let Some(succs) = g.successors(v) {
            for w in succs {
                if !state.indices.contains_key(&w) {
                    strongconnect(g, &w, state);
                    let lw = state.lowlinks[&w];
                    let lv = state.lowlinks.get_mut(v).unwrap();
                    *lv = (*lv).min(lw);
                } else if state.on_stack.contains(&w) {
                    let iw = state.indices[&w];
                    let lv = state.lowlinks.get_mut(v).unwrap();
                    *lv = (*lv).min(iw);
                }
            }
        }

        if state.lowlinks[v] == state.indices[v] {
            let mut scc = Vec::new();
            loop {
                let w = state.stack.pop().unwrap();
                state.on_stack.remove(&w);
                scc.push(w.clone());
                if w == v {
                    break;
                }
            }
            state.result.push(scc);
        }
    }

    let mut state = TarjanState {
        index: 0,
        stack: Vec::new(),
        on_stack: HashSet::new(),
        indices: HashMap::new(),
        lowlinks: HashMap::new(),
        result: Vec::new(),
    };

    for v in g.nodes() {
        if !state.indices.contains_key(&v) {
            strongconnect(g, &v, &mut state);
        }
    }

    state.result
}

/// Depth-first search traversal.
pub fn dfs<N, E>(g: &Graph<N, E>, roots: &[&str], order: DfsOrder) -> Vec<String> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();

    fn do_dfs<N, E>(
        g: &Graph<N, E>,
        v: &str,
        order: DfsOrder,
        visited: &mut HashSet<String>,
        result: &mut Vec<String>,
    ) {
        if visited.contains(v) {
            return;
        }
        visited.insert(v.to_string());

        if order == DfsOrder::Pre {
            result.push(v.to_string());
        }

        if let Some(neighbors) = g.successors(v) {
            for w in neighbors {
                do_dfs(g, &w, order, visited, result);
            }
        }

        if order == DfsOrder::Post {
            result.push(v.to_string());
        }
    }

    for root in roots {
        do_dfs(g, root, order, &mut visited, &mut result);
    }

    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DfsOrder {
    Pre,
    Post,
}

/// Preorder DFS traversal.
pub fn preorder<N, E>(g: &Graph<N, E>, roots: &[&str]) -> Vec<String> {
    dfs(g, roots, DfsOrder::Pre)
}

/// Postorder DFS traversal.
pub fn postorder<N, E>(g: &Graph<N, E>, roots: &[&str]) -> Vec<String> {
    dfs(g, roots, DfsOrder::Post)
}

/// Find weakly connected components. Returns a list of components,
/// each being a list of node IDs.
pub fn components<N, E>(g: &Graph<N, E>) -> Vec<Vec<String>> {
    let mut visited = HashSet::new();
    let mut result = Vec::new();

    for v in g.nodes() {
        if visited.contains(&v) {
            continue;
        }
        let mut component = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(v.clone());
        visited.insert(v);

        while let Some(node) = queue.pop_front() {
            component.push(node.clone());
            if let Some(neighbors) = g.neighbors(&node) {
                for w in neighbors {
                    if !visited.contains(&w) {
                        visited.insert(w.clone());
                        queue.push_back(w);
                    }
                }
            }
        }
        result.push(component);
    }

    result
}

// ============================================================
// Shortest-path result type
// ============================================================

/// Single-source shortest path result entry: (distance, predecessor).
/// Predecessor is `None` for source or unreachable nodes.
pub type PathEntry = (f64, Option<String>);

// ============================================================
// Dijkstra
// ============================================================

/// Dijkstra's single-source shortest paths (simple API).
/// `weight_fn` takes an edge label reference and returns its weight.
/// Returns a map from node to (distance, predecessor).
pub fn dijkstra<N, E>(
    g: &Graph<N, E>,
    source: &str,
    weight_fn: impl Fn(&E) -> f64,
) -> HashMap<String, PathEntry> {
    dijkstra_with_edge_fn(
        g,
        source,
        |e: &Edge| {
            g.edge(&e.v, &e.w, e.name.as_deref())
                .map(&weight_fn)
                .unwrap_or(1.0)
        },
        None,
    )
}

/// Dijkstra's single-source shortest paths (full API with edge function).
///
/// `weight_fn` takes an `Edge` descriptor and returns its weight.
/// `edge_fn` optionally customizes which edges to traverse from each node.
pub fn dijkstra_with_edge_fn<N, E>(
    g: &Graph<N, E>,
    source: &str,
    weight_fn: impl Fn(&Edge) -> f64,
    edge_fn: EdgeFn<'_>,
) -> HashMap<String, PathEntry> {
    let mut dist: HashMap<String, PathEntry> = HashMap::new();
    let mut pq = PriorityQueue::new();

    for v in g.nodes() {
        let d = if v == source { 0.0 } else { f64::INFINITY };
        dist.insert(v.clone(), (d, None));
        pq.insert(v, d);
    }

    let default_edge_fn = |v: &str| -> Vec<Edge> { g.out_edges(v, None).unwrap_or_default() };

    while let Some((u, d)) = pq.extract_min() {
        if d == f64::INFINITY {
            break;
        }
        let edges = match edge_fn {
            Some(ef) => ef(&u),
            None => default_edge_fn(&u),
        };
        for e in edges {
            let w = if e.v != u { &e.v } else { &e.w };
            let weight = weight_fn(&e);
            if weight < 0.0 {
                panic!(
                    "dijkstra does not allow negative edge weights. Bad edge: {} Weight: {}",
                    e, weight
                );
            }
            let alt = d + weight;
            if alt < dist.get(w).map_or(f64::INFINITY, |d| d.0) {
                dist.insert(w.clone(), (alt, Some(u.clone())));
                pq.decrease(w, alt);
            }
        }
    }

    dist
}

/// Run Dijkstra from every node. Returns source -> { target -> PathEntry }.
///
/// `weight_fn` takes an `Edge` descriptor and returns its weight.
/// `edge_fn` optionally customizes which edges to traverse.
pub fn dijkstra_all<N, E>(
    g: &Graph<N, E>,
    weight_fn: impl Fn(&Edge) -> f64,
    edge_fn: EdgeFn<'_>,
) -> HashMap<String, HashMap<String, PathEntry>> {
    let mut result = HashMap::new();
    for v in g.nodes() {
        result.insert(v.clone(), dijkstra_with_edge_fn(g, &v, &weight_fn, edge_fn));
    }
    result
}

// ============================================================
// Bellman-Ford
// ============================================================

/// Bellman-Ford single-source shortest paths. Handles negative weights.
/// Returns a map from node to (distance, predecessor).
/// Panics if the graph contains a negative weight cycle.
///
/// `weight_fn` takes an `Edge` descriptor and returns its weight.
/// `edge_fn` optionally customizes which edges to traverse.
pub fn bellman_ford<N, E>(
    g: &Graph<N, E>,
    source: &str,
    weight_fn: impl Fn(&Edge) -> f64,
    edge_fn: EdgeFn<'_>,
) -> HashMap<String, PathEntry> {
    let nodes = g.nodes();
    let mut results: HashMap<String, PathEntry> = HashMap::new();

    let default_edge_fn = |v: &str| -> Vec<Edge> { g.out_edges(v, None).unwrap_or_default() };

    // Initialization
    for v in &nodes {
        let d = if v == source { 0.0 } else { f64::INFINITY };
        results.insert(v.clone(), (d, None));
    }

    let relax_all_edges = |results: &mut HashMap<String, PathEntry>| -> bool {
        let mut did_upgrade = false;
        for vertex in &nodes {
            let edges = match edge_fn {
                Some(ef) => ef(vertex),
                None => default_edge_fn(vertex),
            };
            for edge in edges {
                // If the vertex on which the edgeFn is called is
                // the edge.w, then we treat the edge as if it was reversed
                let in_vertex = if edge.v == *vertex { &edge.v } else { &edge.w };
                let out_vertex = if *in_vertex == edge.v {
                    &edge.w
                } else {
                    &edge.v
                };
                let relaxed_edge = Edge::new(in_vertex.clone(), out_vertex.clone());
                let edge_weight = weight_fn(&relaxed_edge);
                let in_dist = results
                    .get(in_vertex.as_str())
                    .map_or(f64::INFINITY, |e| e.0);
                let out_dist = results
                    .get(out_vertex.as_str())
                    .map_or(f64::INFINITY, |e| e.0);
                if in_dist + edge_weight < out_dist {
                    results.insert(
                        out_vertex.clone(),
                        (in_dist + edge_weight, Some(in_vertex.clone())),
                    );
                    did_upgrade = true;
                }
            }
        }
        did_upgrade
    };

    let num_nodes = nodes.len();
    let mut iterations = 0usize;
    let mut did_upgrade;

    // Relax all edges |V|-1 times
    for _ in 1..num_nodes {
        did_upgrade = relax_all_edges(&mut results);
        iterations += 1;
        if !did_upgrade {
            break;
        }
    }

    // Detect negative weight cycle
    if num_nodes > 1 && iterations == num_nodes - 1 {
        did_upgrade = relax_all_edges(&mut results);
        if did_upgrade {
            panic!("The graph contains a negative weight cycle");
        }
    }

    results
}

// ============================================================
// Floyd-Warshall
// ============================================================

/// Floyd-Warshall all-pairs shortest paths.
/// Returns source -> { target -> PathEntry }.
///
/// `weight_fn` takes an `Edge` descriptor and returns its weight.
/// `edge_fn` optionally customizes which edges to traverse.
pub fn floyd_warshall<N, E>(
    g: &Graph<N, E>,
    weight_fn: impl Fn(&Edge) -> f64,
    edge_fn: EdgeFn<'_>,
) -> HashMap<String, HashMap<String, PathEntry>> {
    let nodes = g.nodes();
    let mut results: HashMap<String, HashMap<String, PathEntry>> = HashMap::new();

    let default_edge_fn = |v: &str| -> Vec<Edge> { g.out_edges(v, None).unwrap_or_default() };

    // Initialize
    for v in &nodes {
        let mut row: HashMap<String, PathEntry> = HashMap::new();
        row.insert(v.clone(), (0.0, None));
        for w in &nodes {
            if v != w {
                row.insert(w.clone(), (f64::INFINITY, None));
            }
        }
        let edges = match edge_fn {
            Some(ef) => ef(v),
            None => default_edge_fn(v),
        };
        for edge in edges {
            let w = if edge.v == *v { &edge.w } else { &edge.v };
            let d = weight_fn(&edge);
            row.insert(w.clone(), (d, Some(v.clone())));
        }
        results.insert(v.clone(), row);
    }

    // Floyd-Warshall relaxation
    for k in &nodes {
        for i in &nodes {
            for j in &nodes {
                let ik = results[i][k].0;
                let kj = results[k][j].0;
                let ij = results[i][j].0;
                let alt = ik + kj;
                if alt < ij {
                    let pred = results[k][j].1.clone();
                    results.get_mut(i).unwrap().insert(j.clone(), (alt, pred));
                }
            }
        }
    }

    results
}

// ============================================================
// Extract path
// ============================================================

/// Extracted path result: weight and ordered list of nodes.
pub struct ExtractedPath {
    pub weight: f64,
    pub path: Vec<String>,
}

/// Extract a path from shortest-path results.
/// Panics if the source or destination is invalid.
pub fn extract_path(
    shortest_paths: &HashMap<String, PathEntry>,
    source: &str,
    destination: &str,
) -> ExtractedPath {
    // Validate source: predecessor must be None
    match shortest_paths.get(source) {
        Some((_, None)) => {}
        _ => panic!("Invalid source vertex"),
    }

    // Validate destination: predecessor must be Some (unless dest == source)
    if destination != source {
        match shortest_paths.get(destination) {
            Some((_, Some(_))) => {}
            _ => panic!("Invalid destination vertex"),
        }
    }

    let weight = shortest_paths[destination].0;
    let mut path = Vec::new();
    let mut current = destination.to_string();

    while current != source {
        path.push(current.clone());
        current = shortest_paths[&current]
            .1
            .clone()
            .expect("broken path chain");
    }
    path.push(source.to_string());
    path.reverse();

    ExtractedPath { weight, path }
}

// ============================================================
// Reduce (graph fold)
// ============================================================

/// Graph reduction traversal: accumulate a value over a DFS traversal.
/// `roots` are the starting nodes; `order` is Pre or Post.
pub fn reduce<N, E, T>(
    g: &Graph<N, E>,
    roots: &[&str],
    order: DfsOrder,
    f: impl Fn(T, &str) -> T,
    initial: T,
) -> T {
    let mut visited = HashSet::new();
    let mut acc = initial;

    for root in roots {
        assert!(g.has_node(root), "Graph does not have node: {}", root);
        acc = do_reduce(g, root, order, &mut visited, &f, acc);
    }
    acc
}

fn do_reduce<N, E, T>(
    g: &Graph<N, E>,
    v: &str,
    order: DfsOrder,
    visited: &mut HashSet<String>,
    f: &impl Fn(T, &str) -> T,
    mut acc: T,
) -> T {
    if visited.contains(v) {
        return acc;
    }
    visited.insert(v.to_string());

    if order == DfsOrder::Pre {
        acc = f(acc, v);
    }

    let neighbors = if g.is_directed() {
        g.successors(v).unwrap_or_default()
    } else {
        g.neighbors(v).unwrap_or_default()
    };
    for w in neighbors {
        acc = do_reduce(g, &w, order, visited, f, acc);
    }

    if order == DfsOrder::Post {
        acc = f(acc, v);
    }

    acc
}

// ============================================================
// Prim
// ============================================================

/// Prim's minimum spanning tree for undirected graphs.
/// Returns a new undirected graph representing the MST.
/// Panics if the graph is not connected.
pub fn prim<N, E>(g: &Graph<N, E>, weight_fn: impl Fn(&E) -> f64) -> Graph<(), f64>
where
    N: Clone,
{
    let mut result = Graph::<(), f64>::with_options(super::GraphOptions {
        directed: false,
        multigraph: false,
        compound: false,
    });

    let nodes = g.nodes();
    if nodes.is_empty() {
        return result;
    }

    let start = &nodes[0];
    let mut in_mst = HashSet::new();
    let mut pq = PriorityQueue::new();

    for v in &nodes {
        result.set_node(v.clone(), None);
        pq.insert(v.clone(), f64::INFINITY);
    }
    pq.decrease(start, 0.0);

    let mut parents: HashMap<String, String> = HashMap::new();
    let mut weights: HashMap<String, f64> = HashMap::new();
    let mut init = false;

    while let Some((u, _)) = pq.extract_min() {
        in_mst.insert(u.clone());

        if let Some(parent) = parents.get(&u) {
            let w = weights.get(&u).copied().unwrap_or(0.0);
            result.set_edge(parent.clone(), u.clone(), Some(w), None);
        } else if init {
            panic!("Input graph is not connected");
        } else {
            init = true;
        }

        let edges = g.node_edges(&u, None);
        if let Some(edges) = edges {
            for e in edges {
                let w = if e.v == u { &e.w } else { &e.v };
                if in_mst.contains(w) {
                    continue;
                }
                if let Some(label) = g.edge(&e.v, &e.w, e.name.as_deref()) {
                    let edge_weight = weight_fn(label);
                    let current = weights.get(w).copied().unwrap_or(f64::INFINITY);
                    if edge_weight < current {
                        parents.insert(w.clone(), u.clone());
                        weights.insert(w.clone(), edge_weight);
                        pq.decrease(w, edge_weight);
                    }
                }
            }
        }
    }

    result
}

/// Simple priority queue for Dijkstra/Prim.
struct PriorityQueue {
    entries: HashMap<String, f64>,
}

impl PriorityQueue {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn insert(&mut self, key: String, priority: f64) {
        self.entries.insert(key, priority);
    }

    fn decrease(&mut self, key: &str, priority: f64) {
        if let Some(p) = self.entries.get_mut(key)
            && priority < *p
        {
            *p = priority;
        }
    }

    fn extract_min(&mut self) -> Option<(String, f64)> {
        if self.entries.is_empty() {
            return None;
        }
        let (key, priority) = self
            .entries
            .iter()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(k, v)| (k.clone(), *v))?;
        self.entries.remove(&key);
        Some((key, priority))
    }
}
