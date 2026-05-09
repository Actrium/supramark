//! A graph data structure supporting directed/undirected, simple/multi, and compound graphs.
//!
//! This is a Rust port of the graphlib library used by dagre.js.
//! Nodes are identified by string keys. Node and edge labels are generic.

pub mod alg;
pub mod json;

use std::collections::{HashMap, HashSet};
use std::fmt;

/// Default node label factory type.
type NodeLabelFactory<N> = Option<Box<dyn Fn(&str) -> N>>;

/// Default edge label factory type.
type EdgeLabelFactory<E> = Option<Box<dyn Fn(&Edge) -> E>>;

/// Edge descriptor: source, target, and optional name (for multigraphs).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Edge {
    pub v: String,
    pub w: String,
    pub name: Option<String>,
}

impl Edge {
    pub fn new(v: impl Into<String>, w: impl Into<String>) -> Self {
        Self {
            v: v.into(),
            w: w.into(),
            name: None,
        }
    }

    pub fn with_name(v: impl Into<String>, w: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            v: v.into(),
            w: w.into(),
            name: Some(name.into()),
        }
    }
}

impl fmt::Display for Edge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(name) => write!(f, "{}->{}:{}", self.v, self.w, name),
            None => write!(f, "{}->{}", self.v, self.w),
        }
    }
}

/// Options for constructing a graph.
#[derive(Debug, Clone)]
pub struct GraphOptions {
    pub directed: bool,
    pub multigraph: bool,
    pub compound: bool,
}

impl Default for GraphOptions {
    fn default() -> Self {
        Self {
            directed: true,
            multigraph: false,
            compound: false,
        }
    }
}

/// Canonical edge ID used as internal key.
fn edge_id(v: &str, w: &str, directed: bool, name: Option<&str>) -> String {
    let (v, w) = if !directed && v > w { (w, v) } else { (v, w) };
    match name {
        Some(n) => format!("{}\x01{}\x01{}", v, w, n),
        None => format!("{}\x01{}\x01", v, w),
    }
}

/// Construct an Edge from an edge_id and the edge descriptor.
fn edge_obj_for(directed: bool, v: &str, w: &str, name: Option<&str>) -> Edge {
    let (v, w) = if !directed && v > w { (w, v) } else { (v, w) };
    Edge {
        v: v.to_string(),
        w: w.to_string(),
        name: name.map(|s| s.to_string()),
    }
}

const GRAPH_NODE: &str = "\x00";

/// A graph supporting directed/undirected, simple/multigraph, and compound (hierarchical) modes.
///
/// Nodes are identified by `String` keys. Labels are generic:
/// - `N`: node label type
/// - `E`: edge label type
pub struct Graph<N = (), E = ()> {
    is_directed: bool,
    is_multigraph: bool,
    is_compound: bool,

    // Graph-level label
    label: Option<Box<dyn std::any::Any>>,

    // Node storage: node_id -> label
    nodes: HashMap<String, Option<N>>,
    // Insertion-order tracking for deterministic iteration (matches JS property order)
    node_order: Vec<String>,
    node_count: usize,

    // Compound graph: parent-child relationships
    parent: HashMap<String, String>,
    children: HashMap<String, Vec<String>>,

    // Adjacency: node_id -> { edge_id -> Edge }
    in_edges: HashMap<String, HashMap<String, Edge>>,
    out_edges: HashMap<String, HashMap<String, Edge>>,
    // Insertion-order tracking for in/out edges per node
    in_edge_order: HashMap<String, Vec<String>>,
    out_edge_order: HashMap<String, Vec<String>>,

    // Predecessor/successor counts: node -> { neighbor -> count }
    preds: HashMap<String, HashMap<String, usize>>,
    sucs: HashMap<String, HashMap<String, usize>>,
    // Insertion-order tracking for predecessors/successors per node
    preds_order: HashMap<String, Vec<String>>,
    sucs_order: HashMap<String, Vec<String>>,

    // Edge storage
    edge_objs: HashMap<String, Edge>,
    edge_labels: HashMap<String, Option<E>>,
    // Insertion-order tracking for edges
    edge_order: Vec<String>,
    edge_count: usize,

    // Default label factories
    default_node_label: NodeLabelFactory<N>,
    default_edge_label: EdgeLabelFactory<E>,
}

impl<N, E> Graph<N, E> {
    /// Create a new graph with default options (directed, simple, non-compound).
    pub fn new() -> Self {
        Self::with_options(GraphOptions::default())
    }

    /// Create a new graph with the given options.
    pub fn with_options(opts: GraphOptions) -> Self {
        let mut g = Self {
            is_directed: opts.directed,
            is_multigraph: opts.multigraph,
            is_compound: opts.compound,
            label: None,
            nodes: HashMap::new(),
            node_order: Vec::new(),
            node_count: 0,
            parent: HashMap::new(),
            children: HashMap::new(),
            in_edges: HashMap::new(),
            out_edges: HashMap::new(),
            in_edge_order: HashMap::new(),
            out_edge_order: HashMap::new(),
            preds: HashMap::new(),
            sucs: HashMap::new(),
            preds_order: HashMap::new(),
            sucs_order: HashMap::new(),
            edge_objs: HashMap::new(),
            edge_labels: HashMap::new(),
            edge_order: Vec::new(),
            edge_count: 0,
            default_node_label: None,
            default_edge_label: None,
        };

        if opts.compound {
            // Root pseudo-node has all top-level nodes as children
            g.children.insert(GRAPH_NODE.to_string(), Vec::new());
        }

        g
    }

    // --- Graph-level ---

    pub fn is_directed(&self) -> bool {
        self.is_directed
    }

    pub fn is_multigraph(&self) -> bool {
        self.is_multigraph
    }

    pub fn is_compound(&self) -> bool {
        self.is_compound
    }

    pub fn set_default_node_label(&mut self, f: impl Fn(&str) -> N + 'static) {
        self.default_node_label = Some(Box::new(f));
    }

    pub fn set_default_edge_label(&mut self, f: impl Fn(&Edge) -> E + 'static) {
        self.default_edge_label = Some(Box::new(f));
    }

    /// Set the graph-level label.
    pub fn set_graph_label<L: 'static>(&mut self, label: L) {
        self.label = Some(Box::new(label));
    }

    /// Get a reference to the graph-level label, downcasted to the expected type.
    pub fn graph_label<L: 'static>(&self) -> Option<&L> {
        self.label.as_ref().and_then(|l| l.downcast_ref::<L>())
    }

    /// Get a mutable reference to the graph-level label, downcasted to the expected type.
    pub fn graph_label_mut<L: 'static>(&mut self) -> Option<&mut L> {
        self.label.as_mut().and_then(|l| l.downcast_mut::<L>())
    }

    /// Look up an edge label by an Edge descriptor.
    pub fn edge_by_obj(&self, e: &Edge) -> Option<&E> {
        self.edge(&e.v, &e.w, e.name.as_deref())
    }

    /// Look up a mutable edge label by an Edge descriptor.
    pub fn edge_by_obj_mut(&mut self, e: &Edge) -> Option<&mut E> {
        self.edge_mut(&e.v, &e.w, e.name.as_deref())
    }

    // --- Node operations ---

    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// Returns all node IDs in insertion order.
    pub fn nodes(&self) -> Vec<String> {
        self.node_order.clone()
    }

    /// Returns nodes with no in-edges (in insertion order).
    pub fn sources(&self) -> Vec<String> {
        self.node_order
            .iter()
            .filter(|v| {
                self.in_edges
                    .get(v.as_str())
                    .is_none_or(|edges| edges.is_empty())
            })
            .cloned()
            .collect()
    }

    /// Returns nodes with no out-edges (in insertion order).
    pub fn sinks(&self) -> Vec<String> {
        self.node_order
            .iter()
            .filter(|v| {
                self.out_edges
                    .get(v.as_str())
                    .is_none_or(|edges| edges.is_empty())
            })
            .cloned()
            .collect()
    }

    /// Set a node with an optional label. Creates the node if it doesn't exist.
    pub fn set_node(&mut self, v: impl Into<String>, label: Option<N>) -> &mut Self {
        let v = v.into();
        if let std::collections::hash_map::Entry::Occupied(mut entry) = self.nodes.entry(v.clone())
        {
            if label.is_some() {
                entry.insert(label);
            }
            return self;
        }

        let label = if label.is_some() {
            label
        } else {
            self.default_node_label.as_ref().map(|f| f(&v))
        };

        self.nodes.insert(v.clone(), label);
        self.node_order.push(v.clone());
        self.node_count += 1;

        if self.is_compound {
            self.parent.insert(v.clone(), GRAPH_NODE.to_string());
            let siblings = self.children.entry(GRAPH_NODE.to_string()).or_default();
            if !siblings.contains(&v) {
                siblings.push(v.clone());
            }
            self.children.entry(v.clone()).or_default();
        }

        self.in_edges.entry(v.clone()).or_default();
        self.out_edges.entry(v.clone()).or_default();
        self.preds.entry(v.clone()).or_default();
        self.sucs.entry(v).or_default();

        self
    }

    /// Get a reference to a node's label.
    pub fn node(&self, v: &str) -> Option<&N> {
        self.nodes.get(v).and_then(|l| l.as_ref())
    }

    /// Get a mutable reference to a node's label.
    pub fn node_mut(&mut self, v: &str) -> Option<&mut N> {
        self.nodes.get_mut(v).and_then(|l| l.as_mut())
    }

    /// Check if a node exists.
    pub fn has_node(&self, v: &str) -> bool {
        self.nodes.contains_key(v)
    }

    /// Remove a node and all incident edges.
    pub fn remove_node(&mut self, v: &str) -> Option<N> {
        if !self.has_node(v) {
            return None;
        }

        // Remove incident edges
        if let Some(in_e) = self.in_edges.remove(v) {
            self.in_edge_order.remove(v);
            for edge in in_e.values() {
                self.remove_edge_by_obj(edge);
            }
        }
        if let Some(out_e) = self.out_edges.remove(v) {
            self.out_edge_order.remove(v);
            for edge in out_e.values() {
                self.remove_edge_by_obj(edge);
            }
        }

        // Remove from compound hierarchy
        if self.is_compound {
            // Re-parent children to this node's parent
            if let Some(parent_id) = self.parent.get(v).cloned() {
                if let Some(my_children) = self.children.remove(v) {
                    for child in &my_children {
                        self.parent.insert(child.clone(), parent_id.clone());
                        let parent_children = self.children.entry(parent_id.clone()).or_default();
                        if !parent_children.contains(child) {
                            parent_children.push(child.clone());
                        }
                    }
                }
                // Remove from parent's children
                if let Some(siblings) = self.children.get_mut(&parent_id) {
                    siblings.retain(|n| n != v);
                }
            }
            self.parent.remove(v);
        }

        self.preds.remove(v);
        self.sucs.remove(v);
        self.preds_order.remove(v);
        self.sucs_order.remove(v);
        self.node_order.retain(|n| n != v);
        self.node_count -= 1;
        self.nodes.remove(v).flatten()
    }

    // --- Compound graph operations ---

    /// Set the parent of a node. Both nodes must exist. Pass `None` to set parent to root.
    /// Panics if setting the parent would create a cycle in the parent hierarchy
    /// (i.e. if `parent` is a descendant of `v`).
    pub fn set_parent(&mut self, v: &str, parent: Option<&str>) -> &mut Self {
        assert!(
            self.is_compound,
            "Cannot set parent in a non-compound graph"
        );

        let parent = parent.unwrap_or(GRAPH_NODE);

        // Ensure parent node exists (unless it's the root)
        if parent != GRAPH_NODE && !self.has_node(parent) {
            self.set_node(parent.to_string(), None);
        }
        if !self.has_node(v) {
            self.set_node(v.to_string(), None);
        }

        // Check for cycle: walk up from `parent` and make sure we don't reach `v`
        if parent != GRAPH_NODE {
            let mut ancestor = Some(parent);
            while let Some(a) = ancestor {
                if a == v {
                    panic!("Setting {} as parent of {} would create a cycle", parent, v);
                }
                ancestor = self.parent.get(a).and_then(|p| {
                    if p == GRAPH_NODE {
                        None
                    } else {
                        Some(p.as_str())
                    }
                });
            }
        }

        // Remove from old parent
        if let Some(old_parent) = self.parent.get(v).cloned()
            && let Some(siblings) = self.children.get_mut(&old_parent)
        {
            siblings.retain(|n| n != v);
        }

        // Set new parent
        self.parent.insert(v.to_string(), parent.to_string());
        let new_siblings = self.children.entry(parent.to_string()).or_default();
        if !new_siblings.contains(&v.to_string()) {
            new_siblings.push(v.to_string());
        }

        self
    }

    /// Get the parent of a node. Returns None for top-level nodes or if not compound.
    pub fn parent(&self, v: &str) -> Option<&str> {
        if !self.is_compound {
            return None;
        }
        self.parent.get(v).and_then(|p| {
            if p == GRAPH_NODE {
                None
            } else {
                Some(p.as_str())
            }
        })
    }

    /// Get children of a node. Pass None to get top-level nodes.
    pub fn children(&self, v: Option<&str>) -> Vec<String> {
        if !self.is_compound {
            if v.is_none() {
                return self.nodes();
            }
            return vec![];
        }

        let key = v.unwrap_or(GRAPH_NODE);
        self.children.get(key).cloned().unwrap_or_default()
    }

    // --- Adjacency ---

    /// Get predecessors of a node (nodes with edges pointing to v), in insertion order.
    pub fn predecessors(&self, v: &str) -> Option<Vec<String>> {
        if !self.has_node(v) {
            return None;
        }
        Some(self.preds_order.get(v).cloned().unwrap_or_default())
    }

    /// Get successors of a node (nodes that v points to), in insertion order.
    pub fn successors(&self, v: &str) -> Option<Vec<String>> {
        if !self.has_node(v) {
            return None;
        }
        Some(self.sucs_order.get(v).cloned().unwrap_or_default())
    }

    /// Get all neighbors of a node (union of predecessors and successors), in insertion order.
    pub fn neighbors(&self, v: &str) -> Option<Vec<String>> {
        let preds = self.predecessors(v)?;
        let sucs = self.successors(v)?;
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for n in preds.into_iter().chain(sucs) {
            if seen.insert(n.clone()) {
                result.push(n);
            }
        }
        Some(result)
    }

    /// Check if a node has no outgoing edges.
    pub fn is_leaf(&self, v: &str) -> bool {
        match if self.is_directed {
            self.successors(v)
        } else {
            self.neighbors(v)
        } {
            Some(ns) => ns.is_empty(),
            None => false,
        }
    }

    // --- Edge operations ---

    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// Returns all edge descriptors in insertion order.
    pub fn edges(&self) -> Vec<Edge> {
        self.edge_order
            .iter()
            .filter_map(|eid| self.edge_objs.get(eid).cloned())
            .collect()
    }

    /// Set an edge with an optional label. Creates endpoint nodes if they don't exist.
    pub fn set_edge(
        &mut self,
        v: impl Into<String>,
        w: impl Into<String>,
        label: Option<E>,
        name: Option<&str>,
    ) -> &mut Self {
        let v = v.into();
        let w = w.into();
        let eid = edge_id(&v, &w, self.is_directed, name);
        let e = edge_obj_for(self.is_directed, &v, &w, name);

        if let std::collections::hash_map::Entry::Occupied(mut entry) =
            self.edge_labels.entry(eid.clone())
        {
            if label.is_some() {
                entry.insert(label);
            }
            return self;
        }

        if !self.is_multigraph && name.is_some() {
            panic!("Cannot set a named edge on a non-multigraph");
        }

        // Ensure nodes exist
        self.set_node(v.clone(), None);
        self.set_node(w.clone(), None);

        let label = if label.is_some() {
            label
        } else {
            self.default_edge_label.as_ref().map(|f| f(&e))
        };

        self.edge_labels.insert(eid.clone(), label);
        self.edge_objs.insert(eid.clone(), e.clone());
        self.edge_order.push(eid.clone());
        self.edge_count += 1;

        // Update adjacency
        self.in_edges
            .entry(e.w.clone())
            .or_default()
            .insert(eid.clone(), e.clone());
        self.in_edge_order
            .entry(e.w.clone())
            .or_default()
            .push(eid.clone());
        self.out_edges
            .entry(e.v.clone())
            .or_default()
            .insert(eid.clone(), e.clone());
        self.out_edge_order
            .entry(e.v.clone())
            .or_default()
            .push(eid.clone());

        {
            let preds_map = self.preds.entry(e.w.clone()).or_default();
            let is_new = !preds_map.contains_key(&e.v);
            *preds_map.entry(e.v.clone()).or_insert(0) += 1;
            if is_new {
                self.preds_order
                    .entry(e.w.clone())
                    .or_default()
                    .push(e.v.clone());
            }
        }
        {
            let sucs_map = self.sucs.entry(e.v.clone()).or_default();
            let is_new = !sucs_map.contains_key(&e.w);
            *sucs_map.entry(e.w.clone()).or_insert(0) += 1;
            if is_new {
                self.sucs_order
                    .entry(e.v.clone())
                    .or_default()
                    .push(e.w.clone());
            }
        }

        // For undirected graphs, add reverse adjacency as well
        if !self.is_directed {
            self.in_edges
                .entry(e.v.clone())
                .or_default()
                .insert(eid.clone(), e.clone());
            self.in_edge_order
                .entry(e.v.clone())
                .or_default()
                .push(eid.clone());
            self.out_edges
                .entry(e.w.clone())
                .or_default()
                .insert(eid.clone(), e.clone());
            self.out_edge_order
                .entry(e.w.clone())
                .or_default()
                .push(eid.clone());

            {
                let preds_map = self.preds.entry(e.v.clone()).or_default();
                let is_new = !preds_map.contains_key(&e.w);
                *preds_map.entry(e.w.clone()).or_insert(0) += 1;
                if is_new {
                    self.preds_order
                        .entry(e.v.clone())
                        .or_default()
                        .push(e.w.clone());
                }
            }
            {
                let sucs_map = self.sucs.entry(e.w.clone()).or_default();
                let is_new = !sucs_map.contains_key(&e.v);
                *sucs_map.entry(e.v.clone()).or_insert(0) += 1;
                if is_new {
                    self.sucs_order
                        .entry(e.w.clone())
                        .or_default()
                        .push(e.v.clone());
                }
            }
        }

        self
    }

    /// Get a reference to an edge's label.
    pub fn edge(&self, v: &str, w: &str, name: Option<&str>) -> Option<&E> {
        let eid = edge_id(v, w, self.is_directed, name);
        self.edge_labels.get(&eid).and_then(|l| l.as_ref())
    }

    /// Get a mutable reference to an edge's label.
    pub fn edge_mut(&mut self, v: &str, w: &str, name: Option<&str>) -> Option<&mut E> {
        let eid = edge_id(v, w, self.is_directed, name);
        self.edge_labels.get_mut(&eid).and_then(|l| l.as_mut())
    }

    /// Check if an edge exists.
    pub fn has_edge(&self, v: &str, w: &str, name: Option<&str>) -> bool {
        let eid = edge_id(v, w, self.is_directed, name);
        self.edge_labels.contains_key(&eid)
    }

    /// Remove an edge.
    pub fn remove_edge(&mut self, v: &str, w: &str, name: Option<&str>) -> Option<E> {
        let eid = edge_id(v, w, self.is_directed, name);
        self.remove_edge_by_id(&eid)
    }

    fn remove_edge_by_obj(&mut self, edge: &Edge) {
        let eid = edge_id(&edge.v, &edge.w, self.is_directed, edge.name.as_deref());
        self.remove_edge_by_id(&eid);
    }

    fn remove_edge_by_id(&mut self, eid: &str) -> Option<E> {
        let e = self.edge_objs.remove(eid)?;
        let label = self.edge_labels.remove(eid).flatten();
        self.edge_order.retain(|id| id != eid);
        self.edge_count -= 1;

        // Update adjacency
        if let Some(in_e) = self.in_edges.get_mut(&e.w) {
            in_e.remove(eid);
        }
        if let Some(order) = self.in_edge_order.get_mut(&e.w) {
            order.retain(|id| id != eid);
        }
        if let Some(out_e) = self.out_edges.get_mut(&e.v) {
            out_e.remove(eid);
        }
        if let Some(order) = self.out_edge_order.get_mut(&e.v) {
            order.retain(|id| id != eid);
        }

        // Decrement pred/suc counts and update order tracking
        if let Some(preds) = self.preds.get_mut(&e.w)
            && let Some(count) = preds.get_mut(&e.v)
        {
            *count -= 1;
            if *count == 0 {
                preds.remove(&e.v);
                if let Some(order) = self.preds_order.get_mut(&e.w) {
                    order.retain(|n| n != &e.v);
                }
            }
        }
        if let Some(sucs) = self.sucs.get_mut(&e.v)
            && let Some(count) = sucs.get_mut(&e.w)
        {
            *count -= 1;
            if *count == 0 {
                sucs.remove(&e.w);
                if let Some(order) = self.sucs_order.get_mut(&e.v) {
                    order.retain(|n| n != &e.w);
                }
            }
        }

        // For undirected graphs, also remove the reverse adjacency entries
        if !self.is_directed {
            if let Some(in_e) = self.in_edges.get_mut(&e.v) {
                in_e.remove(eid);
            }
            if let Some(order) = self.in_edge_order.get_mut(&e.v) {
                order.retain(|id| id != eid);
            }
            if let Some(out_e) = self.out_edges.get_mut(&e.w) {
                out_e.remove(eid);
            }
            if let Some(order) = self.out_edge_order.get_mut(&e.w) {
                order.retain(|id| id != eid);
            }
            if let Some(preds) = self.preds.get_mut(&e.v)
                && let Some(count) = preds.get_mut(&e.w)
            {
                *count -= 1;
                if *count == 0 {
                    preds.remove(&e.w);
                    if let Some(order) = self.preds_order.get_mut(&e.v) {
                        order.retain(|n| n != &e.w);
                    }
                }
            }
            if let Some(sucs) = self.sucs.get_mut(&e.w)
                && let Some(count) = sucs.get_mut(&e.v)
            {
                *count -= 1;
                if *count == 0 {
                    sucs.remove(&e.v);
                    if let Some(order) = self.sucs_order.get_mut(&e.w) {
                        order.retain(|n| n != &e.v);
                    }
                }
            }
        }

        label
    }

    /// Get incoming edges to a node, optionally filtered by source (in insertion order).
    pub fn in_edges(&self, v: &str, u: Option<&str>) -> Option<Vec<Edge>> {
        if !self.has_node(v) {
            return None;
        }
        let edges_map = self.in_edges.get(v)?;
        let order = self.in_edge_order.get(v);
        let result: Vec<Edge> = match order {
            Some(ord) => ord
                .iter()
                .filter_map(|eid| edges_map.get(eid))
                .filter(|e| u.is_none() || e.v == u.unwrap())
                .cloned()
                .collect(),
            None => edges_map
                .values()
                .filter(|e| u.is_none() || e.v == u.unwrap())
                .cloned()
                .collect(),
        };
        Some(result)
    }

    /// Get outgoing edges from a node, optionally filtered by target (in insertion order).
    pub fn out_edges(&self, v: &str, w: Option<&str>) -> Option<Vec<Edge>> {
        if !self.has_node(v) {
            return None;
        }
        let edges_map = self.out_edges.get(v)?;
        let order = self.out_edge_order.get(v);
        let result: Vec<Edge> = match order {
            Some(ord) => ord
                .iter()
                .filter_map(|eid| edges_map.get(eid))
                .filter(|e| w.is_none() || e.w == w.unwrap())
                .cloned()
                .collect(),
            None => edges_map
                .values()
                .filter(|e| w.is_none() || e.w == w.unwrap())
                .cloned()
                .collect(),
        };
        Some(result)
    }

    /// Get all edges incident to a node, optionally filtered by the other endpoint.
    pub fn node_edges(&self, v: &str, w: Option<&str>) -> Option<Vec<Edge>> {
        let mut result = self.in_edges(v, w)?;
        result.extend(self.out_edges(v, w)?);
        Some(result)
    }

    /// Set a chain of edges from a sequence of nodes.
    pub fn set_path(&mut self, nodes: &[&str], label: Option<E>) -> &mut Self
    where
        E: Clone,
    {
        for window in nodes.windows(2) {
            self.set_edge(window[0], window[1], label.clone(), None);
        }
        self
    }

    /// Create a new graph with only nodes that satisfy the predicate.
    /// Edges are included if both endpoints pass the filter.
    /// Compound relationships are preserved where possible.
    pub fn filter_nodes(&self, predicate: impl Fn(&str) -> bool) -> Self
    where
        N: Clone,
        E: Clone,
    {
        let mut g = Graph::with_options(GraphOptions {
            directed: self.is_directed,
            multigraph: self.is_multigraph,
            compound: self.is_compound,
        });

        for v in &self.node_order {
            if predicate(v) {
                let label = self.nodes.get(v).and_then(|l| l.clone());
                g.set_node(v.clone(), label);
            }
        }

        for (eid, e) in &self.edge_objs {
            if g.has_node(&e.v) && g.has_node(&e.w) {
                let label = self.edge_labels.get(eid).and_then(|l| l.clone());
                g.set_edge(e.v.clone(), e.w.clone(), label, e.name.as_deref());
            }
        }

        if self.is_compound {
            for v in g.nodes() {
                // Walk up the parent chain to find the nearest ancestor that's in the filtered graph
                let mut ancestor = self.parent(v.as_str());
                while let Some(a) = ancestor {
                    if g.has_node(a) || a == GRAPH_NODE {
                        break;
                    }
                    ancestor = self.parent(a);
                }
                g.set_parent(
                    &v,
                    ancestor.and_then(|a| if a == GRAPH_NODE { None } else { Some(a) }),
                );
            }
        }

        g
    }
}

impl<N, E> Default for Graph<N, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N: fmt::Debug, E: fmt::Debug> fmt::Debug for Graph<N, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Graph")
            .field("directed", &self.is_directed)
            .field("multigraph", &self.is_multigraph)
            .field("compound", &self.is_compound)
            .field("node_count", &self.node_count)
            .field("edge_count", &self.edge_count)
            .finish()
    }
}

#[cfg(test)]
mod tests;
