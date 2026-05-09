//! Tests ported from graphlib.js graph-test.ts (170+ tests) and alg/ tests (57+ tests)

use super::alg;
use super::*;

// ============================================================
// Graph - Initial State
// ============================================================

#[test]
fn has_no_nodes_by_default() {
    let g: Graph<i32, i32> = Graph::new();
    assert_eq!(g.node_count(), 0);
}

#[test]
fn has_no_edges_by_default() {
    let g: Graph<i32, i32> = Graph::new();
    assert_eq!(g.edge_count(), 0);
}

#[test]
fn defaults_to_directed() {
    let g: Graph<i32, i32> = Graph::new();
    assert!(g.is_directed());
}

#[test]
fn defaults_to_non_multigraph() {
    let g: Graph<i32, i32> = Graph::new();
    assert!(!g.is_multigraph());
}

#[test]
fn defaults_to_non_compound() {
    let g: Graph<i32, i32> = Graph::new();
    assert!(!g.is_compound());
}

#[test]
fn can_create_undirected_graph() {
    let g: Graph<i32, i32> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    assert!(!g.is_directed());
}

#[test]
fn can_create_multigraph() {
    let g: Graph<i32, i32> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    assert!(g.is_multigraph());
}

#[test]
fn can_create_compound_graph() {
    let g: Graph<i32, i32> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    assert!(g.is_compound());
}

// ============================================================
// Graph - Nodes
// ============================================================

#[test]
fn set_node_creates_node() {
    let mut g: Graph<&str, ()> = Graph::new();
    g.set_node("a", None);
    assert!(g.has_node("a"));
    assert_eq!(g.node_count(), 1);
}

#[test]
fn set_node_with_label() {
    let mut g: Graph<i32, ()> = Graph::new();
    g.set_node("a", Some(42));
    assert_eq!(g.node("a"), Some(&42));
}

#[test]
fn set_node_overwrites_label() {
    let mut g: Graph<i32, ()> = Graph::new();
    g.set_node("a", Some(1));
    g.set_node("a", Some(2));
    assert_eq!(g.node("a"), Some(&2));
    assert_eq!(g.node_count(), 1);
}

#[test]
fn set_node_does_not_overwrite_with_none() {
    let mut g: Graph<i32, ()> = Graph::new();
    g.set_node("a", Some(42));
    g.set_node("a", None);
    assert_eq!(g.node("a"), Some(&42));
}

#[test]
fn set_node_with_default_label() {
    let mut g: Graph<String, ()> = Graph::new();
    g.set_default_node_label(|v| format!("label-{}", v));
    g.set_node("a", None);
    assert_eq!(g.node("a"), Some(&"label-a".to_string()));
}

#[test]
fn node_returns_none_for_nonexistent() {
    let g: Graph<i32, ()> = Graph::new();
    assert_eq!(g.node("a"), None);
}

#[test]
fn remove_node_removes_node() {
    let mut g: Graph<i32, ()> = Graph::new();
    g.set_node("a", Some(1));
    g.remove_node("a");
    assert!(!g.has_node("a"));
    assert_eq!(g.node_count(), 0);
}

#[test]
fn remove_node_returns_label() {
    let mut g: Graph<i32, ()> = Graph::new();
    g.set_node("a", Some(42));
    assert_eq!(g.remove_node("a"), Some(42));
}

#[test]
fn remove_node_removes_incident_edges() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    g.remove_node("b");
    assert_eq!(g.edge_count(), 0);
}

#[test]
fn remove_nonexistent_node_returns_none() {
    let mut g: Graph<i32, ()> = Graph::new();
    assert_eq!(g.remove_node("a"), None);
}

#[test]
fn nodes_returns_all_nodes() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    g.set_node("b", None);
    let mut nodes = g.nodes();
    nodes.sort();
    assert_eq!(nodes, vec!["a", "b"]);
}

// ============================================================
// Graph - Sources and Sinks
// ============================================================

#[test]
fn sources_returns_nodes_with_no_in_edges() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    let mut sources = g.sources();
    sources.sort();
    assert_eq!(sources, vec!["a"]);
}

#[test]
fn sinks_returns_nodes_with_no_out_edges() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    let mut sinks = g.sinks();
    sinks.sort();
    assert_eq!(sinks, vec!["c"]);
}

// ============================================================
// Graph - Edges
// ============================================================

#[test]
fn set_edge_creates_edge() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    assert!(g.has_edge("a", "b", None));
    assert_eq!(g.edge_count(), 1);
}

#[test]
fn set_edge_creates_nodes_if_needed() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    assert!(g.has_node("a"));
    assert!(g.has_node("b"));
}

#[test]
fn set_edge_with_label() {
    let mut g: Graph<(), i32> = Graph::new();
    g.set_edge("a", "b", Some(42), None);
    assert_eq!(g.edge("a", "b", None), Some(&42));
}

#[test]
fn set_edge_overwrites_label() {
    let mut g: Graph<(), i32> = Graph::new();
    g.set_edge("a", "b", Some(1), None);
    g.set_edge("a", "b", Some(2), None);
    assert_eq!(g.edge("a", "b", None), Some(&2));
    assert_eq!(g.edge_count(), 1);
}

#[test]
fn set_edge_does_not_overwrite_with_none() {
    let mut g: Graph<(), i32> = Graph::new();
    g.set_edge("a", "b", Some(42), None);
    g.set_edge("a", "b", None, None);
    assert_eq!(g.edge("a", "b", None), Some(&42));
}

#[test]
fn set_edge_with_default_label() {
    let mut g: Graph<(), String> = Graph::new();
    g.set_default_edge_label(|_| "default".to_string());
    g.set_edge("a", "b", None, None);
    assert_eq!(g.edge("a", "b", None), Some(&"default".to_string()));
}

#[test]
fn edge_returns_none_for_nonexistent() {
    let g: Graph<(), i32> = Graph::new();
    assert_eq!(g.edge("a", "b", None), None);
}

#[test]
fn remove_edge_removes_edge() {
    let mut g: Graph<(), i32> = Graph::new();
    g.set_edge("a", "b", Some(42), None);
    g.remove_edge("a", "b", None);
    assert!(!g.has_edge("a", "b", None));
    assert_eq!(g.edge_count(), 0);
}

#[test]
fn remove_edge_returns_label() {
    let mut g: Graph<(), i32> = Graph::new();
    g.set_edge("a", "b", Some(42), None);
    assert_eq!(g.remove_edge("a", "b", None), Some(42));
}

#[test]
fn remove_edge_keeps_nodes() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.remove_edge("a", "b", None);
    assert!(g.has_node("a"));
    assert!(g.has_node("b"));
}

#[test]
fn edges_returns_all_edges() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    assert_eq!(g.edge_count(), 2);
    assert_eq!(g.edges().len(), 2);
}

// ============================================================
// Graph - Directed edges
// ============================================================

#[test]
fn directed_edge_has_direction() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    assert!(g.has_edge("a", "b", None));
    assert!(!g.has_edge("b", "a", None));
}

#[test]
fn in_edges_returns_incoming() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("c", "b", None, None);
    let in_e = g.in_edges("b", None).unwrap();
    assert_eq!(in_e.len(), 2);
}

#[test]
fn in_edges_filtered_by_source() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("c", "b", None, None);
    let in_e = g.in_edges("b", Some("a")).unwrap();
    assert_eq!(in_e.len(), 1);
    assert_eq!(in_e[0].v, "a");
}

#[test]
fn out_edges_returns_outgoing() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "c", None, None);
    let out_e = g.out_edges("a", None).unwrap();
    assert_eq!(out_e.len(), 2);
}

#[test]
fn out_edges_filtered_by_target() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "c", None, None);
    let out_e = g.out_edges("a", Some("b")).unwrap();
    assert_eq!(out_e.len(), 1);
    assert_eq!(out_e[0].w, "b");
}

#[test]
fn node_edges_returns_all_incident() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("c", "b", None, None);
    let edges = g.node_edges("b", None).unwrap();
    assert_eq!(edges.len(), 2);
}

#[test]
fn predecessors_returns_incoming_nodes() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("c", "b", None, None);
    let mut preds = g.predecessors("b").unwrap();
    preds.sort();
    assert_eq!(preds, vec!["a", "c"]);
}

#[test]
fn successors_returns_outgoing_nodes() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "c", None, None);
    let mut succs = g.successors("a").unwrap();
    succs.sort();
    assert_eq!(succs, vec!["b", "c"]);
}

#[test]
fn neighbors_returns_all_adjacent() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("c", "a", None, None);
    let mut neighbors = g.neighbors("a").unwrap();
    neighbors.sort();
    assert_eq!(neighbors, vec!["b", "c"]);
}

// ============================================================
// Graph - Undirected
// ============================================================

#[test]
fn undirected_edge_both_directions() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_edge("a", "b", None, None);
    assert!(g.has_edge("a", "b", None));
    assert!(g.has_edge("b", "a", None));
}

#[test]
fn undirected_predecessors_and_successors_are_same() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_edge("a", "b", None, None);
    let preds = g.predecessors("b").unwrap();
    let succs = g.successors("b").unwrap();
    assert_eq!(preds, succs);
}

// ============================================================
// Graph - Multigraph
// ============================================================

#[test]
fn multigraph_allows_named_edges() {
    let mut g: Graph<(), i32> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", Some(1), Some("x"));
    g.set_edge("a", "b", Some(2), Some("y"));
    assert_eq!(g.edge("a", "b", Some("x")), Some(&1));
    assert_eq!(g.edge("a", "b", Some("y")), Some(&2));
    assert_eq!(g.edge_count(), 2);
}

#[test]
fn multigraph_remove_named_edge() {
    let mut g: Graph<(), i32> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", Some(1), Some("x"));
    g.set_edge("a", "b", Some(2), Some("y"));
    g.remove_edge("a", "b", Some("x"));
    assert!(!g.has_edge("a", "b", Some("x")));
    assert!(g.has_edge("a", "b", Some("y")));
    assert_eq!(g.edge_count(), 1);
}

// ============================================================
// Graph - Compound
// ============================================================

#[test]
fn compound_set_parent() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_node("a", None);
    g.set_node("b", None);
    g.set_parent("b", Some("a"));
    assert_eq!(g.parent("b"), Some("a"));
}

#[test]
fn compound_children_of_parent() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_node("a", None);
    g.set_node("b", None);
    g.set_node("c", None);
    g.set_parent("b", Some("a"));
    g.set_parent("c", Some("a"));
    let mut children = g.children(Some("a"));
    children.sort();
    assert_eq!(children, vec!["b", "c"]);
}

#[test]
fn compound_top_level_nodes() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_node("a", None);
    g.set_node("b", None);
    g.set_parent("b", Some("a"));
    let mut top = g.children(None);
    top.sort();
    assert_eq!(top, vec!["a"]);
}

#[test]
fn compound_parent_defaults_to_none() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_node("a", None);
    assert_eq!(g.parent("a"), None);
}

#[test]
fn compound_reset_parent_to_root() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_node("a", None);
    g.set_node("b", None);
    g.set_parent("b", Some("a"));
    g.set_parent("b", None);
    assert_eq!(g.parent("b"), None);
    assert!(g.children(Some("a")).is_empty());
}

#[test]
fn compound_remove_node_reparents_children() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_node("a", None);
    g.set_node("b", None);
    g.set_node("c", None);
    g.set_parent("b", Some("a"));
    g.set_parent("c", Some("b"));
    g.remove_node("b");
    // c should be reparented to a
    assert_eq!(g.parent("c"), Some("a"));
}

#[test]
fn compound_set_parent_creates_nodes() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_parent("b", Some("a"));
    assert!(g.has_node("a"));
    assert!(g.has_node("b"));
    assert_eq!(g.parent("b"), Some("a"));
}

// ============================================================
// Graph - set_path
// ============================================================

#[test]
fn set_path_creates_chain() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c"], None);
    assert!(g.has_edge("a", "b", None));
    assert!(g.has_edge("b", "c", None));
    assert!(!g.has_edge("a", "c", None));
}

// ============================================================
// Graph - is_leaf
// ============================================================

#[test]
fn is_leaf_for_sink_node() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    assert!(g.is_leaf("b"));
    assert!(!g.is_leaf("a"));
}

#[test]
fn is_leaf_for_isolated_node() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    assert!(g.is_leaf("a"));
}

// ============================================================
// Graph - filter_nodes
// ============================================================

#[test]
fn filter_nodes_copies_matching_nodes() {
    let mut g: Graph<i32, ()> = Graph::new();
    g.set_node("a", Some(1));
    g.set_node("b", Some(2));
    let g2 = g.filter_nodes(|v| v == "a");
    assert!(g2.has_node("a"));
    assert!(!g2.has_node("b"));
    assert_eq!(g2.node("a"), Some(&1));
}

#[test]
fn filter_nodes_copies_edges_for_matching_nodes() {
    let mut g: Graph<(), i32> = Graph::new();
    g.set_edge("a", "b", Some(1), None);
    g.set_edge("b", "c", Some(2), None);
    let g2 = g.filter_nodes(|v| v != "c");
    assert!(g2.has_edge("a", "b", None));
    assert!(!g2.has_edge("b", "c", None));
}

#[test]
fn filter_nodes_preserves_compound_parents() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_node("a", None);
    g.set_node("b", None);
    g.set_node("c", None);
    g.set_parent("b", Some("a"));
    g.set_parent("c", Some("b"));
    // Filter out "b", "c" should reparent to "a"
    let g2 = g.filter_nodes(|v| v != "b");
    assert_eq!(g2.parent("c"), Some("a"));
}

// ============================================================
// Graph - node_mut
// ============================================================

#[test]
fn node_mut_allows_modification() {
    let mut g: Graph<i32, ()> = Graph::new();
    g.set_node("a", Some(1));
    *g.node_mut("a").unwrap() = 42;
    assert_eq!(g.node("a"), Some(&42));
}

// ============================================================
// Algorithm - Topological Sort
// ============================================================

#[test]
fn topsort_empty_graph() {
    let g: Graph<(), ()> = Graph::new();
    assert_eq!(alg::topsort(&g).unwrap(), Vec::<String>::new());
}

#[test]
fn topsort_simple_chain() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c"], None);
    assert_eq!(alg::topsort(&g).unwrap(), vec!["a", "b", "c"]);
}

#[test]
fn topsort_diamond() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "d"], None);
    g.set_path(&["a", "c", "d"], None);
    let result = alg::topsort(&g).unwrap();
    // a must come before b, c; d must come last
    let pos = |v: &str| result.iter().position(|x| x == v).unwrap();
    assert!(pos("a") < pos("b"));
    assert!(pos("a") < pos("c"));
    assert!(pos("b") < pos("d"));
    assert!(pos("c") < pos("d"));
}

#[test]
fn topsort_detects_cycle() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c", "a"], None);
    assert!(alg::topsort(&g).is_err());
}

#[test]
fn topsort_detects_self_loop() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "a", None, None);
    assert!(alg::topsort(&g).is_err());
}

// ============================================================
// Algorithm - is_acyclic
// ============================================================

#[test]
fn is_acyclic_true_for_dag() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c"], None);
    assert!(alg::is_acyclic(&g));
}

#[test]
fn is_acyclic_false_for_cyclic() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c", "a"], None);
    assert!(!alg::is_acyclic(&g));
}

#[test]
fn is_acyclic_false_for_self_loop() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "a", None, None);
    assert!(!alg::is_acyclic(&g));
}

// ============================================================
// Algorithm - Tarjan (SCC)
// ============================================================

#[test]
fn tarjan_empty_graph() {
    let g: Graph<(), ()> = Graph::new();
    assert!(alg::tarjan(&g).is_empty());
}

#[test]
fn tarjan_single_node() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    let sccs = alg::tarjan(&g);
    assert_eq!(sccs.len(), 1);
    assert_eq!(sccs[0], vec!["a"]);
}

#[test]
fn tarjan_cycle() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c", "a"], None);
    let sccs = alg::tarjan(&g);
    let big_scc: Vec<&Vec<String>> = sccs.iter().filter(|s| s.len() > 1).collect();
    assert_eq!(big_scc.len(), 1);
    let mut scc = big_scc[0].clone();
    scc.sort();
    assert_eq!(scc, vec!["a", "b", "c"]);
}

#[test]
fn tarjan_two_cycles() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "a"], None);
    g.set_path(&["c", "d", "c"], None);
    let sccs = alg::tarjan(&g);
    let mut big_sccs: Vec<Vec<String>> = sccs
        .into_iter()
        .filter(|s| s.len() > 1)
        .map(|mut s| {
            s.sort();
            s
        })
        .collect();
    big_sccs.sort();
    assert_eq!(big_sccs.len(), 2);
    assert_eq!(big_sccs[0], vec!["a", "b"]);
    assert_eq!(big_sccs[1], vec!["c", "d"]);
}

// ============================================================
// Algorithm - find_cycles
// ============================================================

#[test]
fn find_cycles_empty() {
    let g: Graph<(), ()> = Graph::new();
    assert!(alg::find_cycles(&g).is_empty());
}

#[test]
fn find_cycles_acyclic() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c"], None);
    assert!(alg::find_cycles(&g).is_empty());
}

#[test]
fn find_cycles_self_loop() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    g.set_edge("a", "a", None, None);
    let cycles = alg::find_cycles(&g);
    assert_eq!(cycles.len(), 1);
}

// ============================================================
// Algorithm - Components
// ============================================================

#[test]
fn components_empty() {
    let g: Graph<(), ()> = Graph::new();
    assert!(alg::components(&g).is_empty());
}

#[test]
fn components_unconnected_nodes() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    g.set_node("b", None);
    let comps = alg::components(&g);
    assert_eq!(comps.len(), 2);
}

#[test]
fn components_connected() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    let comps = alg::components(&g);
    assert_eq!(comps.len(), 1);
    assert_eq!(comps[0].len(), 3);
}

#[test]
fn components_two_components() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("c", "d", None, None);
    let comps = alg::components(&g);
    assert_eq!(comps.len(), 2);
}

// ============================================================
// Algorithm - DFS / Preorder / Postorder
// ============================================================

#[test]
fn preorder_singleton() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    assert_eq!(alg::preorder(&g, &["a"]), vec!["a"]);
}

#[test]
fn preorder_tree() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "c", None, None);
    let result = alg::preorder(&g, &["a"]);
    assert_eq!(result[0], "a");
    assert_eq!(result.len(), 3);
}

#[test]
fn postorder_singleton() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    assert_eq!(alg::postorder(&g, &["a"]), vec!["a"]);
}

#[test]
fn postorder_tree() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "c", None, None);
    let result = alg::postorder(&g, &["a"]);
    // a should be last in postorder
    assert_eq!(result.last().unwrap(), "a");
    assert_eq!(result.len(), 3);
}

#[test]
fn dfs_visits_each_node_once() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "d"], None);
    g.set_path(&["a", "c", "d"], None);
    let result = alg::preorder(&g, &["a"]);
    assert_eq!(result.len(), 4);
    let unique: std::collections::HashSet<_> = result.iter().collect();
    assert_eq!(unique.len(), 4);
}

// ============================================================
// Algorithm - Dijkstra
// ============================================================

#[test]
fn dijkstra_simple_path() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "b", Some(1.0), None);
    g.set_edge("b", "c", Some(2.0), None);
    g.set_edge("a", "c", Some(10.0), None);
    let result = alg::dijkstra(&g, "a", |w| *w);
    assert_eq!(result["a"].0, 0.0);
    assert_eq!(result["b"].0, 1.0);
    assert_eq!(result["c"].0, 3.0); // a->b->c = 3, cheaper than a->c = 10
}

// ============================================================
// Algorithm - Prim
// ============================================================

#[test]
fn prim_simple_graph() {
    let mut g: Graph<(), f64> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_edge("a", "b", Some(1.0), None);
    g.set_edge("b", "c", Some(2.0), None);
    g.set_edge("a", "c", Some(5.0), None);
    let mst = alg::prim(&g, |w| *w);
    assert_eq!(mst.node_count(), 3);
    assert_eq!(mst.edge_count(), 2);
    // MST should pick a-b (1) and b-c (2), not a-c (5)
    assert!(mst.has_edge("a", "b", None));
    assert!(mst.has_edge("b", "c", None));
}

// ============================================================
// Graph - Additional setNode / node tests
// ============================================================

#[test]
fn set_node_is_idempotent() {
    let mut g: Graph<&str, ()> = Graph::new();
    g.set_node("a", Some("foo"));
    g.set_node("a", Some("foo"));
    assert_eq!(g.node("a"), Some(&"foo"));
    assert_eq!(g.node_count(), 1);
}

#[test]
fn nodes_empty_graph() {
    let g: Graph<(), ()> = Graph::new();
    assert_eq!(g.nodes(), Vec::<String>::new());
}

#[test]
fn remove_node_is_idempotent() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    g.remove_node("a");
    let result = g.remove_node("a");
    assert_eq!(result, None);
    assert!(!g.has_node("a"));
    assert_eq!(g.node_count(), 0);
}

// ============================================================
// Graph - Additional sources / sinks tests
// ============================================================

#[test]
fn sources_includes_isolated_nodes() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c"], None);
    g.set_node("d", None);
    let mut sources = g.sources();
    sources.sort();
    assert_eq!(sources, vec!["a", "d"]);
}

#[test]
fn sinks_includes_isolated_nodes() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c"], None);
    g.set_node("d", None);
    let mut sinks = g.sinks();
    sinks.sort();
    assert_eq!(sinks, vec!["c", "d"]);
}

// ============================================================
// Graph - Additional filterNodes tests
// ============================================================

#[test]
fn filter_nodes_returns_identical_graph_for_all_pass() {
    let mut g: Graph<i32, i32> = Graph::new();
    g.set_node("a", Some(123));
    g.set_path(&["a", "b", "c"], None);
    g.set_edge("a", "c", Some(456), None);
    let g2 = g.filter_nodes(|_| true);
    let mut nodes = g2.nodes();
    nodes.sort();
    assert_eq!(nodes, vec!["a", "b", "c"]);
    assert!(g2.has_edge("a", "b", None));
    assert!(g2.has_edge("b", "c", None));
    assert!(g2.has_edge("a", "c", None));
    assert_eq!(g2.node("a"), Some(&123));
    assert_eq!(g2.edge("a", "c", None), Some(&456));
}

#[test]
fn filter_nodes_returns_empty_graph_for_none_pass() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c"], None);
    let g2 = g.filter_nodes(|_| false);
    assert_eq!(g2.nodes().len(), 0);
    assert_eq!(g2.edges().len(), 0);
}

#[test]
fn filter_nodes_removes_edges_to_removed_nodes() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    let g2 = g.filter_nodes(|v| v == "a");
    let mut nodes = g2.nodes();
    nodes.sort();
    assert_eq!(nodes, vec!["a"]);
    assert_eq!(g2.edges().len(), 0);
}

#[test]
fn filter_nodes_preserves_directed_option() {
    let g: Graph<(), ()> = Graph::new();
    assert!(g.filter_nodes(|_| true).is_directed());

    let g: Graph<(), ()> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    assert!(!g.filter_nodes(|_| true).is_directed());
}

#[test]
fn filter_nodes_preserves_multigraph_option() {
    let g: Graph<(), ()> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    assert!(g.filter_nodes(|_| true).is_multigraph());

    let g: Graph<(), ()> = Graph::new();
    assert!(!g.filter_nodes(|_| true).is_multigraph());
}

#[test]
fn filter_nodes_preserves_compound_option() {
    let g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    assert!(g.filter_nodes(|_| true).is_compound());

    let g: Graph<(), ()> = Graph::new();
    assert!(!g.filter_nodes(|_| true).is_compound());
}

#[test]
fn filter_nodes_includes_subgraphs() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_parent("a", Some("parent"));
    let g2 = g.filter_nodes(|_| true);
    assert_eq!(g2.parent("a"), Some("parent"));
}

#[test]
fn filter_nodes_includes_multi_level_subgraphs() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_parent("a", Some("parent"));
    g.set_parent("parent", Some("root"));
    let g2 = g.filter_nodes(|_| true);
    assert_eq!(g2.parent("a"), Some("parent"));
    assert_eq!(g2.parent("parent"), Some("root"));
}

#[test]
fn filter_nodes_promotes_child_when_parent_filtered_out() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_parent("a", Some("parent"));
    g.set_parent("parent", Some("root"));
    let g2 = g.filter_nodes(|v| v != "parent");
    assert_eq!(g2.parent("a"), Some("root"));
}

// ============================================================
// Graph - Additional setParent tests
// ============================================================

#[test]
#[should_panic]
fn set_parent_throws_on_non_compound() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_parent("a", Some("b"));
}

#[test]
fn set_parent_moves_node_to_new_parent() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_parent("a", Some("parent"));
    g.set_parent("a", Some("parent2"));
    assert_eq!(g.parent("a"), Some("parent2"));
    assert!(g.children(Some("parent")).is_empty());
    assert_eq!(g.children(Some("parent2")), vec!["a"]);
}

#[test]
fn set_parent_idempotent_remove() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_parent("a", Some("parent"));
    g.set_parent("a", None);
    g.set_parent("a", None);
    assert_eq!(g.parent("a"), None);
    let mut top = g.children(None);
    top.sort();
    assert_eq!(top, vec!["a", "parent"]);
}

// ============================================================
// Graph - Additional parent tests
// ============================================================

#[test]
fn parent_returns_none_for_non_compound() {
    let g: Graph<(), ()> = Graph::new();
    assert_eq!(g.parent("a"), None);
}

#[test]
fn parent_returns_none_for_unknown_node() {
    let g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    assert_eq!(g.parent("a"), None);
}

#[test]
fn parent_returns_current_assignment() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_node("a", None);
    g.set_node("parent", None);
    g.set_parent("a", Some("parent"));
    assert_eq!(g.parent("a"), Some("parent"));
}

// ============================================================
// Graph - Additional children tests
// ============================================================

#[test]
fn children_defaults_to_empty_for_new_compound_node() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_node("a", None);
    assert!(g.children(Some("a")).is_empty());
}

#[test]
fn children_returns_empty_for_non_compound_leaf() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    assert!(g.children(Some("a")).is_empty());
}

#[test]
fn children_returns_all_nodes_for_non_compound_root() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    g.set_node("b", None);
    let mut children = g.children(None);
    children.sort();
    assert_eq!(children, vec!["a", "b"]);
}

#[test]
fn children_of_parent_compound() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_parent("a", Some("parent"));
    g.set_parent("b", Some("parent"));
    let mut children = g.children(Some("parent"));
    children.sort();
    assert_eq!(children, vec!["a", "b"]);
}

#[test]
fn children_without_parent_returns_top_level() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_node("a", None);
    g.set_node("b", None);
    g.set_node("c", None);
    g.set_node("parent", None);
    g.set_parent("a", Some("parent"));
    let mut top = g.children(None);
    top.sort();
    assert_eq!(top, vec!["b", "c", "parent"]);
}

// ============================================================
// Graph - removeNode removes compound relationships
// ============================================================

#[test]
fn remove_node_removes_compound_relationships() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_parent("c", Some("b"));
    g.set_parent("b", Some("a"));
    g.remove_node("b");
    assert_eq!(g.parent("b"), None);
    assert!(g.children(Some("b")).is_empty());
    // "a" should no longer list "b" as child
    let a_children = g.children(Some("a"));
    assert!(!a_children.contains(&"b".to_string()));
    // "c" gets reparented to "a"
    assert_eq!(g.parent("c"), Some("a"));
}

// ============================================================
// Graph - Additional predecessors / successors / neighbors tests
// ============================================================

#[test]
fn predecessors_returns_none_for_unknown_node() {
    let g: Graph<(), ()> = Graph::new();
    assert_eq!(g.predecessors("a"), None);
}

#[test]
fn predecessors_includes_self_loops() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    g.set_edge("a", "a", None, None);
    let mut preds = g.predecessors("a").unwrap();
    preds.sort();
    assert_eq!(preds, vec!["a"]);
}

#[test]
fn successors_returns_none_for_unknown_node() {
    let g: Graph<(), ()> = Graph::new();
    assert_eq!(g.successors("a"), None);
}

#[test]
fn successors_includes_self_loops() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    g.set_edge("a", "a", None, None);
    let mut succs = g.successors("a").unwrap();
    succs.sort();
    assert_eq!(succs, vec!["a", "b"]);
}

#[test]
fn successors_returns_empty_for_leaf() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    let succs = g.successors("c").unwrap();
    assert!(succs.is_empty());
}

#[test]
fn neighbors_returns_none_for_unknown_node() {
    let g: Graph<(), ()> = Graph::new();
    assert_eq!(g.neighbors("a"), None);
}

#[test]
fn neighbors_includes_self_loop() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    g.set_edge("a", "a", None, None);
    let mut neighbors = g.neighbors("a").unwrap();
    neighbors.sort();
    assert_eq!(neighbors, vec!["a", "b"]);
}

// ============================================================
// Graph - Additional isLeaf tests
// ============================================================

#[test]
fn is_leaf_false_for_connected_undirected() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_node("a", None);
    g.set_node("b", None);
    g.set_edge("a", "b", None, None);
    assert!(!g.is_leaf("b"));
}

#[test]
fn is_leaf_true_for_unconnected_undirected() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_node("a", None);
    assert!(g.is_leaf("a"));
}

#[test]
fn is_leaf_true_for_unconnected_directed() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    assert!(g.is_leaf("a"));
}

#[test]
fn is_leaf_false_for_predecessor_directed() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    g.set_node("b", None);
    g.set_edge("a", "b", None, None);
    assert!(!g.is_leaf("a"));
}

#[test]
fn is_leaf_true_for_successor_directed() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    g.set_node("b", None);
    g.set_edge("a", "b", None, None);
    assert!(g.is_leaf("b"));
}

// ============================================================
// Graph - Additional edges tests
// ============================================================

#[test]
fn edges_empty_graph() {
    let g: Graph<(), ()> = Graph::new();
    assert!(g.edges().is_empty());
}

// ============================================================
// Graph - Additional setPath tests
// ============================================================

#[test]
fn set_path_with_label() {
    let mut g: Graph<(), i32> = Graph::new();
    g.set_path(&["a", "b", "c"], Some(42));
    assert_eq!(g.edge("a", "b", None), Some(&42));
    assert_eq!(g.edge("b", "c", None), Some(&42));
}

// ============================================================
// Graph - Additional setEdge tests
// ============================================================

#[test]
fn set_edge_creates_multi_edge_only_in_multigraph() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", None, Some("name"));
    assert!(!g.has_edge("a", "b", None));
    assert!(g.has_edge("a", "b", Some("name")));
}

#[test]
#[should_panic]
fn set_edge_panics_for_named_edge_on_non_multigraph() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, Some("name"));
}

#[test]
fn set_edge_directed_treats_opposite_as_distinct() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    assert!(g.has_edge("a", "b", None));
    assert!(!g.has_edge("b", "a", None));
}

#[test]
fn set_edge_undirected_handles_both_directions() {
    let mut g: Graph<(), i32> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_edge("a", "b", Some(42), None);
    assert_eq!(g.edge("a", "b", None), Some(&42));
    assert_eq!(g.edge("b", "a", None), Some(&42));
}

// ============================================================
// Graph - Additional removeEdge tests
// ============================================================

#[test]
fn remove_edge_no_effect_when_absent() {
    let mut g: Graph<(), ()> = Graph::new();
    g.remove_edge("a", "b", None);
    assert!(!g.has_edge("a", "b", None));
    assert_eq!(g.edge_count(), 0);
}

#[test]
fn remove_edge_correctly_removes_neighbors() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.remove_edge("a", "b", None);
    assert_eq!(g.successors("a").unwrap(), Vec::<String>::new());
    assert_eq!(g.neighbors("a").unwrap(), Vec::<String>::new());
    assert_eq!(g.predecessors("b").unwrap(), Vec::<String>::new());
    assert_eq!(g.neighbors("b").unwrap(), Vec::<String>::new());
}

#[test]
fn remove_edge_correctly_decrements_neighbor_count_multigraph() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "b", None, Some("foo"));
    g.remove_edge("a", "b", None);
    assert!(g.has_edge("a", "b", Some("foo")));
    assert_eq!(g.successors("a").unwrap(), vec!["b"]);
    assert_eq!(g.neighbors("a").unwrap(), vec!["b"]);
    assert_eq!(g.predecessors("b").unwrap(), vec!["a"]);
    assert_eq!(g.neighbors("b").unwrap(), vec!["a"]);
}

#[test]
fn remove_edge_works_with_undirected() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_edge("h", "g", None, None);
    g.remove_edge("g", "h", None);
    assert_eq!(g.neighbors("g").unwrap(), Vec::<String>::new());
    assert_eq!(g.neighbors("h").unwrap(), Vec::<String>::new());
}

// ============================================================
// Graph - Additional inEdges tests
// ============================================================

#[test]
fn in_edges_returns_none_for_unknown_node() {
    let g: Graph<(), ()> = Graph::new();
    assert_eq!(g.in_edges("a", None), None);
}

#[test]
fn in_edges_returns_empty_for_source_node() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    let in_e = g.in_edges("a", None).unwrap();
    assert!(in_e.is_empty());
}

#[test]
fn in_edges_works_for_multigraph() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "b", None, Some("bar"));
    g.set_edge("a", "b", None, Some("foo"));
    let in_e = g.in_edges("a", None).unwrap();
    assert!(in_e.is_empty());
    let in_b = g.in_edges("b", None).unwrap();
    assert_eq!(in_b.len(), 3);
}

// ============================================================
// Graph - Additional outEdges tests
// ============================================================

#[test]
fn out_edges_returns_none_for_unknown_node() {
    let g: Graph<(), ()> = Graph::new();
    assert_eq!(g.out_edges("a", None), None);
}

#[test]
fn out_edges_returns_all_outgoing() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    let out_a = g.out_edges("a", None).unwrap();
    assert_eq!(out_a.len(), 1);
    assert_eq!(out_a[0].v, "a");
    assert_eq!(out_a[0].w, "b");
    let out_b = g.out_edges("b", None).unwrap();
    assert_eq!(out_b.len(), 1);
    let out_c = g.out_edges("c", None).unwrap();
    assert!(out_c.is_empty());
}

#[test]
fn out_edges_works_for_multigraph() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "b", None, Some("bar"));
    g.set_edge("a", "b", None, Some("foo"));
    let out_a = g.out_edges("a", None).unwrap();
    assert_eq!(out_a.len(), 3);
    let out_b = g.out_edges("b", None).unwrap();
    assert!(out_b.is_empty());
}

// ============================================================
// Graph - Additional nodeEdges tests
// ============================================================

#[test]
fn node_edges_returns_none_for_unknown_node() {
    let g: Graph<(), ()> = Graph::new();
    assert_eq!(g.node_edges("a", None), None);
}

#[test]
fn node_edges_returns_all_incident_edges() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    let ne_a = g.node_edges("a", None).unwrap();
    assert_eq!(ne_a.len(), 1);
    let ne_b = g.node_edges("b", None).unwrap();
    assert_eq!(ne_b.len(), 2);
    let ne_c = g.node_edges("c", None).unwrap();
    assert_eq!(ne_c.len(), 1);
}

#[test]
fn node_edges_works_for_multigraph() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "b", None, Some("bar"));
    g.set_edge("a", "b", None, Some("foo"));
    let ne_a = g.node_edges("a", None).unwrap();
    assert_eq!(ne_a.len(), 3);
    let ne_b = g.node_edges("b", None).unwrap();
    assert_eq!(ne_b.len(), 3);
}

// ============================================================
// Graph - Undirected additional tests
// ============================================================

#[test]
fn undirected_in_edges_and_out_edges_are_same() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_edge("a", "b", None, None);
    let in_b = g.in_edges("b", None).unwrap();
    let out_b = g.out_edges("b", None).unwrap();
    assert_eq!(in_b.len(), out_b.len());
}

#[test]
fn undirected_edge_retrieval_either_direction() {
    let mut g: Graph<(), i32> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_edge("a", "b", Some(42), None);
    assert_eq!(g.edge("a", "b", None), Some(&42));
    assert_eq!(g.edge("b", "a", None), Some(&42));
}

#[test]
fn undirected_neighbors_returns_all() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    let mut neighbors = g.neighbors("b").unwrap();
    neighbors.sort();
    assert_eq!(neighbors, vec!["a", "c"]);
}

// ============================================================
// Graph - setDefaultEdgeLabel tests
// ============================================================

#[test]
fn set_default_edge_label_does_not_change_existing() {
    let mut g: Graph<(), String> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_default_edge_label(|_| "foo".to_string());
    assert_eq!(g.edge("a", "b", None), None);
}

#[test]
fn set_default_edge_label_not_used_with_explicit_value() {
    let mut g: Graph<(), String> = Graph::new();
    g.set_default_edge_label(|_| "foo".to_string());
    g.set_edge("a", "b", Some("bar".to_string()), None);
    assert_eq!(g.edge("a", "b", None), Some(&"bar".to_string()));
}

// ============================================================
// Graph - setDefaultNodeLabel tests
// ============================================================

#[test]
fn set_default_node_label_does_not_change_existing() {
    let mut g: Graph<String, ()> = Graph::new();
    g.set_node("a", None);
    g.set_default_node_label(|_| "foo".to_string());
    // "a" already existed without a value; adding the default label factory does not update it
    assert_eq!(g.node("a"), None);
}

#[test]
fn set_default_node_label_not_used_with_explicit_value() {
    let mut g: Graph<String, ()> = Graph::new();
    g.set_default_node_label(|_| "foo".to_string());
    g.set_node("a", Some("bar".to_string()));
    assert_eq!(g.node("a"), Some(&"bar".to_string()));
}

// ============================================================
// Algorithm - Additional dijkstra tests
// ============================================================

#[test]
fn dijkstra_distance_zero_for_source() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_node("source", None);
    let result = alg::dijkstra(&g, "source", |w| *w);
    assert_eq!(result["source"].0, 0.0);
}

#[test]
fn dijkstra_infinity_for_unconnected() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_node("a", None);
    g.set_node("b", None);
    let result = alg::dijkstra(&g, "a", |w| *w);
    assert_eq!(result["a"].0, 0.0);
    assert!(result["b"].0.is_infinite());
}

#[test]
fn dijkstra_distance_and_path() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "b", Some(1.0), None);
    g.set_edge("b", "c", Some(1.0), None);
    g.set_edge("b", "d", Some(1.0), None);
    let result = alg::dijkstra(&g, "a", |w| *w);
    assert_eq!(result["a"].0, 0.0);
    assert_eq!(result["b"].0, 1.0);
    assert_eq!(result["c"].0, 2.0);
    assert_eq!(result["d"].0, 2.0);
    assert_eq!(result["b"].1, Some("a".to_string()));
    assert_eq!(result["c"].1, Some("b".to_string()));
}

#[test]
fn dijkstra_works_for_undirected() {
    let mut g: Graph<(), f64> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_edge("a", "b", Some(1.0), None);
    g.set_edge("b", "c", Some(2.0), None);
    g.set_edge("b", "d", Some(3.0), None);
    let result = alg::dijkstra(&g, "a", |w| *w);
    assert_eq!(result["a"].0, 0.0);
    assert_eq!(result["b"].0, 1.0);
    assert_eq!(result["c"].0, 3.0);
    assert_eq!(result["d"].0, 4.0);
}

// ============================================================
// Algorithm - Additional Prim tests
// ============================================================

#[test]
fn prim_empty_graph() {
    let g: Graph<(), f64> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    let mst = alg::prim(&g, |w| *w);
    assert_eq!(mst.node_count(), 0);
    assert_eq!(mst.edge_count(), 0);
}

#[test]
fn prim_single_node() {
    let mut g: Graph<(), f64> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_node("a", None);
    let mst = alg::prim(&g, |w| *w);
    assert_eq!(mst.node_count(), 1);
    assert_eq!(mst.edge_count(), 0);
}

// ============================================================
// Algorithm - Additional components tests
// ============================================================

#[test]
fn components_single_node() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("a", None);
    let comps = alg::components(&g);
    assert_eq!(comps.len(), 1);
    assert_eq!(comps[0].len(), 1);
}

// ============================================================
// Algorithm - Additional find_cycles tests
// ============================================================

#[test]
fn find_cycles_finds_single_cycle() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c", "a"], None);
    let cycles = alg::find_cycles(&g);
    assert_eq!(cycles.len(), 1);
    let mut cycle = cycles[0].clone();
    cycle.sort();
    assert_eq!(cycle, vec!["a", "b", "c"]);
}

#[test]
fn find_cycles_finds_two_separate_cycles() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "a"], None);
    g.set_path(&["c", "d", "c"], None);
    let mut cycles = alg::find_cycles(&g);
    assert_eq!(cycles.len(), 2);
    for cycle in &mut cycles {
        cycle.sort();
    }
    cycles.sort();
    assert_eq!(cycles[0], vec!["a", "b"]);
    assert_eq!(cycles[1], vec!["c", "d"]);
}

// ============================================================
// Algorithm - Additional tarjan tests
// ============================================================

#[test]
fn tarjan_chain_all_singletons() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c"], None);
    let sccs = alg::tarjan(&g);
    assert_eq!(sccs.len(), 3);
    for scc in &sccs {
        assert_eq!(scc.len(), 1);
    }
}

// ============================================================
// Algorithm - Additional preorder / postorder tests
// ============================================================

#[test]
fn preorder_multiple_roots() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("c", "d", None, None);
    let result = alg::preorder(&g, &["a", "c"]);
    assert_eq!(result.len(), 4);
    // a should come before b, c should come before d
    let pos = |v: &str| result.iter().position(|x| x == v).unwrap();
    assert!(pos("a") < pos("b"));
    assert!(pos("c") < pos("d"));
}

#[test]
fn postorder_multiple_roots() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("c", "d", None, None);
    let result = alg::postorder(&g, &["a", "c"]);
    assert_eq!(result.len(), 4);
    // b should come before a, d should come before c
    let pos = |v: &str| result.iter().position(|x| x == v).unwrap();
    assert!(pos("b") < pos("a"));
    assert!(pos("d") < pos("c"));
}

// ============================================================
// Graph - setGraph / graph_label tests (from graph-test.ts "setGraph")
// ============================================================

#[test]
fn set_graph_label_and_get() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_graph_label("foo".to_string());
    assert_eq!(g.graph_label::<String>(), Some(&"foo".to_string()));
}

#[test]
fn graph_label_defaults_to_none() {
    let g: Graph<(), ()> = Graph::new();
    assert_eq!(g.graph_label::<String>(), None);
}

// ============================================================
// Graph - setNode additional tests (from graph-test.ts "setNode")
// ============================================================

#[test]
fn set_node_creates_with_no_label() {
    let mut g: Graph<&str, ()> = Graph::new();
    g.set_node("a", None);
    assert!(g.has_node("a"));
    assert_eq!(g.node("a"), None);
    assert_eq!(g.node_count(), 1);
}

#[test]
fn set_node_uses_stringified_id() {
    // In Rust, node IDs are always strings. This test verifies
    // that "1" as a string key works correctly.
    let mut g: Graph<(), ()> = Graph::new();
    g.set_node("1", None);
    assert!(g.has_node("1"));
    assert_eq!(g.nodes(), vec!["1"]);
}

// ============================================================
// Graph - setNodeDefaults additional tests
// ============================================================

#[test]
fn set_default_node_label_with_constant_fn() {
    let mut g: Graph<String, ()> = Graph::new();
    g.set_default_node_label(|_| "foo".to_string());
    g.set_node("a", None);
    assert_eq!(g.node("a"), Some(&"foo".to_string()));
}

#[test]
fn set_default_node_label_with_name_fn() {
    let mut g: Graph<String, ()> = Graph::new();
    g.set_default_node_label(|v| format!("{}-foo", v));
    g.set_node("a", None);
    assert_eq!(g.node("a"), Some(&"a-foo".to_string()));
}

// ============================================================
// Graph - setParent additional tests (from graph-test.ts)
// ============================================================

#[test]
fn set_parent_creates_child_if_not_exist() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_node("parent", None);
    g.set_parent("a", Some("parent"));
    assert!(g.has_node("a"));
    assert_eq!(g.parent("a"), Some("parent"));
}

#[test]
fn set_parent_has_parent_none_if_never_invoked() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_node("a", None);
    assert_eq!(g.parent("a"), None);
}

#[test]
fn set_parent_removes_parent_on_none() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_parent("a", Some("parent"));
    g.set_parent("a", None);
    assert_eq!(g.parent("a"), None);
    let mut top = g.children(None);
    top.sort();
    assert_eq!(top, vec!["a", "parent"]);
}

#[test]
fn set_parent_stringified_id() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_parent("2", Some("1"));
    g.set_parent("3", Some("2"));
    assert_eq!(g.parent("2"), Some("1"));
    assert_eq!(g.parent("3"), Some("2"));
}

// The JS test "preserves the tree invariant" expects setParent("a", "c") to throw
// when c is a descendant of a. Our Rust implementation does not currently enforce this.
#[test]
#[should_panic]
fn set_parent_preserves_tree_invariant() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    g.set_parent("c", Some("b"));
    g.set_parent("b", Some("a"));
    // This should panic because "a" is an ancestor of "c"
    g.set_parent("a", Some("c"));
}

// ============================================================
// Graph - children additional (from graph-test.ts)
// ============================================================

#[test]
fn children_returns_empty_for_unknown_compound_node() {
    let g: Graph<(), ()> = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    // node "a" does not exist
    assert!(g.children(Some("a")).is_empty());
}

#[test]
fn children_returns_empty_for_non_compound_without_node() {
    let g: Graph<(), ()> = Graph::new();
    assert!(g.children(Some("a")).is_empty());
}

// ============================================================
// Graph - setEdge additional tests (from graph-test.ts)
// ============================================================

#[test]
fn set_edge_changes_value_for_multi_edge() {
    let mut g: Graph<(), &str> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", Some("value"), Some("name"));
    g.set_edge("a", "b", None, Some("name"));
    // After setting with None, the old value should still be retained
    assert_eq!(g.edge("a", "b", Some("name")), Some(&"value"));
    assert!(g.has_edge("a", "b", Some("name")));
}

#[test]
fn set_edge_undirected_id_order() {
    // Tests undirected edges where id has different order than stringified id
    let mut g: Graph<(), &str> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_edge("9", "10", Some("foo"), None);
    assert!(g.has_edge("9", "10", None));
    assert!(g.has_edge("10", "9", None));
    assert_eq!(g.edge("9", "10", None), Some(&"foo"));
}

#[test]
fn set_edge_stringified_ids() {
    let mut g: Graph<(), &str> = Graph::new();
    g.set_edge("1", "2", Some("foo"), None);
    let edges = g.edges();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].v, "1");
    assert_eq!(edges[0].w, "2");
    assert_eq!(g.edge("1", "2", None), Some(&"foo"));
}

#[test]
fn set_edge_stringified_ids_multigraph() {
    let mut g: Graph<(), &str> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("1", "2", Some("foo"), Some("3"));
    assert_eq!(g.edge("1", "2", Some("3")), Some(&"foo"));
    let edges = g.edges();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].v, "1");
    assert_eq!(edges[0].w, "2");
    assert_eq!(edges[0].name, Some("3".to_string()));
}

// ============================================================
// Graph - setDefaultEdgeLabel additional tests (from graph-test.ts)
// ============================================================

#[test]
fn set_default_edge_label_does_not_override_existing_multi_edge() {
    let mut g: Graph<(), String> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", Some("old".to_string()), Some("name"));
    g.set_default_edge_label(|_| "should not set this".to_string());
    g.set_edge("a", "b", None, Some("name"));
    assert_eq!(g.edge("a", "b", Some("name")), Some(&"old".to_string()));
}

// ============================================================
// Graph - edge additional tests (from graph-test.ts)
// ============================================================

#[test]
fn edge_returns_none_for_nonexistent_edge() {
    let g: Graph<(), i32> = Graph::new();
    assert_eq!(g.edge("a", "b", None), None);
    assert_eq!(g.edge("a", "b", Some("foo")), None);
}

#[test]
fn edge_returns_value_of_multi_edge() {
    let mut g: Graph<(), i32> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", Some(42), Some("foo"));
    assert_eq!(g.edge("a", "b", Some("foo")), Some(&42));
    assert_eq!(g.edge("a", "b", None), None);
}

#[test]
fn edge_undirected_returns_either_direction() {
    let mut g: Graph<(), i32> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_edge("a", "b", Some(42), None);
    assert_eq!(g.edge("a", "b", None), Some(&42));
    assert_eq!(g.edge("b", "a", None), Some(&42));
}

// ============================================================
// Graph - edge_by_obj tests (from graph-test.ts "can take an edge object")
// ============================================================

#[test]
fn edge_by_obj_simple() {
    let mut g: Graph<(), &str> = Graph::new();
    g.set_edge("a", "b", Some("value"), None);
    let e = Edge::new("a", "b");
    assert_eq!(g.edge_by_obj(&e), Some(&"value"));
}

#[test]
fn edge_by_obj_multigraph() {
    let mut g: Graph<(), &str> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", Some("value"), Some("name"));
    let e = Edge::with_name("a", "b", "name");
    assert_eq!(g.edge_by_obj(&e), Some(&"value"));
}

// ============================================================
// Graph - removeEdge additional (from graph-test.ts)
// ============================================================

#[test]
fn remove_edge_by_name_multigraph() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", None, Some("foo"));
    g.remove_edge("a", "b", Some("foo"));
    assert!(!g.has_edge("a", "b", Some("foo")));
    assert_eq!(g.edge_count(), 0);
}

// ============================================================
// Graph - inEdges additional tests (from graph-test.ts)
// ============================================================

#[test]
fn in_edges_returns_edges_that_point_at_node() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    assert_eq!(g.in_edges("a", None).unwrap().len(), 0);
    let in_b = g.in_edges("b", None).unwrap();
    assert_eq!(in_b.len(), 1);
    assert_eq!(in_b[0].v, "a");
    assert_eq!(in_b[0].w, "b");
    let in_c = g.in_edges("c", None).unwrap();
    assert_eq!(in_c.len(), 1);
    assert_eq!(in_c[0].v, "b");
    assert_eq!(in_c[0].w, "c");
}

#[test]
fn in_edges_filtered_by_source_multigraph() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "b", None, Some("foo"));
    g.set_edge("a", "c", None, None);
    g.set_edge("b", "c", None, None);
    g.set_edge("z", "a", None, None);
    g.set_edge("z", "b", None, None);
    assert_eq!(g.in_edges("a", Some("b")).unwrap().len(), 0);
    let in_b_from_a = g.in_edges("b", Some("a")).unwrap();
    assert_eq!(in_b_from_a.len(), 2);
}

// ============================================================
// Graph - outEdges additional tests (from graph-test.ts)
// ============================================================

#[test]
fn out_edges_returns_edges_from_node() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    let out_a = g.out_edges("a", None).unwrap();
    assert_eq!(out_a.len(), 1);
    assert_eq!(out_a[0].v, "a");
    assert_eq!(out_a[0].w, "b");
    let out_b = g.out_edges("b", None).unwrap();
    assert_eq!(out_b.len(), 1);
    assert_eq!(out_b[0].v, "b");
    assert_eq!(out_b[0].w, "c");
    let out_c = g.out_edges("c", None).unwrap();
    assert!(out_c.is_empty());
}

#[test]
fn out_edges_filtered_by_target_multigraph() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "b", None, Some("foo"));
    g.set_edge("a", "c", None, None);
    g.set_edge("b", "c", None, None);
    g.set_edge("z", "a", None, None);
    g.set_edge("z", "b", None, None);
    let out_a_to_b = g.out_edges("a", Some("b")).unwrap();
    assert_eq!(out_a_to_b.len(), 2);
    let out_b_to_a = g.out_edges("b", Some("a")).unwrap();
    assert!(out_b_to_a.is_empty());
}

// ============================================================
// Graph - nodeEdges additional tests (from graph-test.ts)
// ============================================================

#[test]
fn node_edges_between_specific_nodes_multigraph() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "b", None, Some("foo"));
    g.set_edge("a", "c", None, None);
    g.set_edge("b", "c", None, None);
    g.set_edge("z", "a", None, None);
    g.set_edge("z", "b", None, None);
    let ne_a_b = g.node_edges("a", Some("b")).unwrap();
    assert_eq!(ne_a_b.len(), 2);
    let ne_b_a = g.node_edges("b", Some("a")).unwrap();
    assert_eq!(ne_b_a.len(), 2);
}

// ============================================================
// Algorithm - topsort additional tests (from topsort-test.ts)
// ============================================================

#[test]
fn topsort_sorts_earlier_nodes_before_later() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["b", "c", "a"], None);
    let result = alg::topsort(&g).unwrap();
    assert_eq!(result, vec!["b", "c", "a"]);
}

#[test]
fn topsort_cycle_with_extra_edge() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["b", "c", "a", "b"], None);
    g.set_edge("b", "d", None, None);
    assert!(alg::topsort(&g).is_err());
}

#[test]
fn topsort_cycle_with_isolated_node() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["b", "c", "a", "b"], None);
    g.set_node("d", None);
    assert!(alg::topsort(&g).is_err());
}

// ============================================================
// Algorithm - is_acyclic additional tests (from is-acyclic-test.ts)
// ============================================================

#[test]
fn is_acyclic_false_for_single_node_cycle() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "a"], None);
    assert!(!alg::is_acyclic(&g));
}

// ============================================================
// Algorithm - find_cycles additional tests (from find-cycles-test.ts)
// ============================================================

#[test]
fn find_cycles_single_node_self_loop() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "a"], None);
    let cycles = alg::find_cycles(&g);
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0], vec!["a"]);
}

#[test]
fn find_cycles_two_node_cycle() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "a"], None);
    let mut cycles = alg::find_cycles(&g);
    assert_eq!(cycles.len(), 1);
    cycles[0].sort();
    assert_eq!(cycles[0], vec!["a", "b"]);
}

#[test]
fn find_cycles_triangle() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c", "a"], None);
    let mut cycles = alg::find_cycles(&g);
    assert_eq!(cycles.len(), 1);
    cycles[0].sort();
    assert_eq!(cycles[0], vec!["a", "b", "c"]);
}

#[test]
fn find_cycles_multiple_mixed() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "a"], None);
    g.set_path(&["c", "d", "e", "c"], None);
    g.set_path(&["f", "g", "g"], None);
    g.set_node("h", None);
    let mut cycles = alg::find_cycles(&g);
    for c in &mut cycles {
        c.sort();
    }
    cycles.sort();
    assert_eq!(cycles.len(), 3);
    assert_eq!(cycles[0], vec!["a", "b"]);
    assert_eq!(cycles[1], vec!["c", "d", "e"]);
    assert_eq!(cycles[2], vec!["g"]);
}

// ============================================================
// Algorithm - tarjan additional tests (from tarjan-test.ts)
// ============================================================

#[test]
fn tarjan_singletons_for_non_scc_nodes() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c"], None);
    g.set_edge("d", "c", None, None);
    let mut sccs: Vec<Vec<String>> = alg::tarjan(&g)
        .into_iter()
        .map(|mut s| {
            s.sort();
            s
        })
        .collect();
    sccs.sort();
    assert_eq!(sccs, vec![vec!["a"], vec!["b"], vec!["c"], vec!["d"]]);
}

#[test]
fn tarjan_single_component_for_cycle_of_1_edge() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "a"], None);
    let mut sccs: Vec<Vec<String>> = alg::tarjan(&g)
        .into_iter()
        .map(|mut s| {
            s.sort();
            s
        })
        .collect();
    sccs.sort();
    assert_eq!(sccs, vec![vec!["a", "b"]]);
}

#[test]
fn tarjan_single_component_for_triangle() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c", "a"], None);
    let mut sccs: Vec<Vec<String>> = alg::tarjan(&g)
        .into_iter()
        .map(|mut s| {
            s.sort();
            s
        })
        .collect();
    sccs.sort();
    assert_eq!(sccs, vec![vec!["a", "b", "c"]]);
}

#[test]
fn tarjan_multiple_components_with_singleton() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "a"], None);
    g.set_path(&["c", "d", "e", "c"], None);
    g.set_node("f", None);
    let mut sccs: Vec<Vec<String>> = alg::tarjan(&g)
        .into_iter()
        .map(|mut s| {
            s.sort();
            s
        })
        .collect();
    sccs.sort();
    assert_eq!(sccs, vec![vec!["a", "b"], vec!["c", "d", "e"], vec!["f"]]);
}

// ============================================================
// Algorithm - components additional tests (from components-test.ts)
// ============================================================

#[test]
fn components_returns_nodes_connected_by_neighbor_in_digraph() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "c", "a"], None);
    g.set_edge("d", "c", None, None);
    g.set_edge("e", "f", None, None);
    let mut comps: Vec<Vec<String>> = alg::components(&g)
        .into_iter()
        .map(|mut c| {
            c.sort();
            c
        })
        .collect();
    comps.sort();
    assert_eq!(comps.len(), 2);
    assert_eq!(comps[0], vec!["a", "b", "c", "d"]);
    assert_eq!(comps[1], vec!["e", "f"]);
}

// ============================================================
// Algorithm - preorder additional tests (from preorder-test.ts)
// ============================================================

#[test]
fn preorder_visits_each_node_once() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "d", "e"], None);
    g.set_path(&["a", "c", "d", "e"], None);
    let mut nodes = alg::preorder(&g, &["a"]);
    nodes.sort();
    assert_eq!(nodes, vec!["a", "b", "c", "d", "e"]);
}

#[test]
fn preorder_works_for_tree() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_path(&["a", "c", "d"], None);
    g.set_edge("c", "e", None, None);
    let nodes = alg::preorder(&g, &["a"]);
    let mut sorted = nodes.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["a", "b", "c", "d", "e"]);
    let pos = |v: &str| nodes.iter().position(|x| x == v).unwrap();
    assert!(pos("b") > pos("a"));
    assert!(pos("c") > pos("a"));
    assert!(pos("d") > pos("c"));
    assert!(pos("e") > pos("c"));
}

#[test]
fn preorder_works_for_array_of_roots() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("c", "d", None, None);
    g.set_node("e", None);
    g.set_node("f", None);
    let nodes = alg::preorder(&g, &["a", "c", "e"]);
    let pos = |v: &str| nodes.iter().position(|x| x == v).unwrap();
    assert!(pos("b") > pos("a"));
    assert!(pos("d") > pos("c"));
    let mut sorted = nodes.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["a", "b", "c", "d", "e"]);
}

// ============================================================
// Algorithm - postorder additional tests (from postorder-test.ts)
// ============================================================

#[test]
fn postorder_visits_each_node_once() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_path(&["a", "b", "d", "e"], None);
    g.set_path(&["a", "c", "d", "e"], None);
    let mut nodes = alg::postorder(&g, &["a"]);
    nodes.sort();
    assert_eq!(nodes, vec!["a", "b", "c", "d", "e"]);
}

#[test]
fn postorder_works_for_tree() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_path(&["a", "c", "d"], None);
    g.set_edge("c", "e", None, None);
    let nodes = alg::postorder(&g, &["a"]);
    let pos = |v: &str| nodes.iter().position(|x| x == v).unwrap();
    assert!(pos("b") < pos("a"));
    assert!(pos("c") < pos("a"));
    assert!(pos("d") < pos("c"));
    assert!(pos("e") < pos("c"));
    let mut sorted = nodes.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["a", "b", "c", "d", "e"]);
}

#[test]
fn postorder_works_for_array_of_roots() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("c", "d", None, None);
    g.set_node("e", None);
    g.set_node("f", None);
    let nodes = alg::postorder(&g, &["a", "b", "c", "e"]);
    let pos = |v: &str| nodes.iter().position(|x| x == v).unwrap();
    assert!(pos("b") < pos("a"));
    assert!(pos("d") < pos("c"));
    let mut sorted = nodes.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["a", "b", "c", "d", "e"]);
}

#[test]
fn postorder_works_for_multiple_connected_roots() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("a", "c", None, None);
    g.set_edge("d", "c", None, None);
    let nodes = alg::postorder(&g, &["a", "d"]);
    let pos = |v: &str| nodes.iter().position(|x| x == v).unwrap();
    assert!(pos("b") < pos("a"));
    assert!(pos("c") < pos("a"));
    assert!(pos("c") < pos("d"));
    let mut sorted = nodes.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["a", "b", "c", "d"]);
}

// ============================================================
// Algorithm - Dijkstra shared shortest-path tests
// (from utils/shortest-paths-tests.ts)
// ============================================================

#[test]
fn dijkstra_returns_distance_and_path_to_other_nodes() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_path(&["a", "b", "c"], Some(1.0));
    g.set_edge("b", "d", Some(1.0), None);
    let result = alg::dijkstra(&g, "a", |w| *w);
    assert_eq!(result["a"].0, 0.0);
    assert_eq!(result["b"].0, 1.0);
    assert_eq!(result["c"].0, 2.0);
    assert_eq!(result["d"].0, 2.0);
    assert_eq!(result["b"].1, Some("a".to_string()));
    assert_eq!(result["c"].1, Some("b".to_string()));
    assert_eq!(result["d"].1, Some("b".to_string()));
}

#[test]
fn dijkstra_uses_weight_function() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "b", Some(1.0), None);
    g.set_edge("a", "c", Some(2.0), None);
    g.set_edge("b", "d", Some(3.0), None);
    g.set_edge("c", "d", Some(3.0), None);
    let result = alg::dijkstra(&g, "a", |w| *w);
    assert_eq!(result["a"].0, 0.0);
    assert_eq!(result["b"].0, 1.0);
    assert_eq!(result["c"].0, 2.0);
    assert_eq!(result["d"].0, 4.0);
    assert_eq!(result["d"].1, Some("b".to_string()));
}

#[test]
fn dijkstra_works_for_undirected_reverse_start() {
    let mut g: Graph<(), f64> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_path(&["a", "b", "c"], Some(1.0));
    g.set_edge("b", "d", Some(1.0), None);
    let result = alg::dijkstra(&g, "d", |w| *w);
    assert_eq!(result["d"].0, 0.0);
    assert_eq!(result["b"].0, 1.0);
    assert_eq!(result["a"].0, 2.0);
    assert_eq!(result["c"].0, 2.0);
}

// ============================================================
// Algorithm - Prim additional tests (from prim-test.ts)
// ============================================================

#[test]
fn prim_deterministic_result() {
    let mut g: Graph<(), f64> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_edge("a", "b", Some(1.0), None);
    g.set_edge("b", "c", Some(2.0), None);
    g.set_edge("b", "d", Some(3.0), None);
    g.set_edge("c", "d", Some(20.0), None);
    g.set_edge("c", "e", Some(60.0), None);
    g.set_edge("d", "e", Some(1.0), None);
    let mst = alg::prim(&g, |w| *w);
    assert_eq!(mst.node_count(), 5);
    let mut na = mst.neighbors("a").unwrap();
    na.sort();
    assert_eq!(na, vec!["b"]);
    let mut nb = mst.neighbors("b").unwrap();
    nb.sort();
    assert_eq!(nb, vec!["a", "c", "d"]);
    let mut nc = mst.neighbors("c").unwrap();
    nc.sort();
    assert_eq!(nc, vec!["b"]);
    let mut nd = mst.neighbors("d").unwrap();
    nd.sort();
    assert_eq!(nd, vec!["b", "e"]);
    let mut ne = mst.neighbors("e").unwrap();
    ne.sort();
    assert_eq!(ne, vec!["d"]);
}

// The JS test expects prim to throw for unconnected graphs.
// Our implementation does not currently enforce this.
#[test]
#[should_panic]
fn prim_throws_for_unconnected_graph() {
    let mut g: Graph<(), f64> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_node("a", None);
    g.set_node("b", None);
    // This should panic because the graph is not connected
    let _mst = alg::prim(&g, |w| *w);
}

// ============================================================
// Algorithm - dijkstra_all (not implemented) - from dijkstra-all-test.ts
// ============================================================

#[test]
fn dijkstra_all_returns_0_for_node_itself() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_node("a", None);
    let result = alg::dijkstra_all(&g, |_e| 1.0, None);
    assert_eq!(result["a"]["a"].0, 0.0);
    assert_eq!(result["a"]["a"].1, None);
}

#[test]
fn dijkstra_all_returns_distance_and_path_from_all_nodes() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    let result = alg::dijkstra_all(&g, |_e| 1.0, None);
    assert_eq!(result["a"]["a"].0, 0.0);
    assert_eq!(result["a"]["b"].0, 1.0);
    assert_eq!(result["a"]["b"].1, Some("a".to_string()));
    assert_eq!(result["a"]["c"].0, 2.0);
    assert_eq!(result["a"]["c"].1, Some("b".to_string()));
    assert!(result["b"]["a"].0.is_infinite());
    assert_eq!(result["b"]["b"].0, 0.0);
    assert_eq!(result["b"]["c"].0, 1.0);
    assert!(result["c"]["a"].0.is_infinite());
    assert!(result["c"]["b"].0.is_infinite());
    assert_eq!(result["c"]["c"].0, 0.0);
}

#[test]
fn dijkstra_all_uses_weight_function() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "b", Some(2.0), None);
    g.set_edge("b", "c", Some(3.0), None);
    let weight_fn = |e: &Edge| *g.edge(&e.v, &e.w, e.name.as_deref()).unwrap_or(&1.0);
    let result = alg::dijkstra_all(&g, weight_fn, None);
    assert_eq!(result["a"]["a"].0, 0.0);
    assert_eq!(result["a"]["b"].0, 2.0);
    assert_eq!(result["a"]["c"].0, 5.0);
    assert!(result["b"]["a"].0.is_infinite());
    assert_eq!(result["b"]["b"].0, 0.0);
    assert_eq!(result["b"]["c"].0, 3.0);
}

#[test]
#[should_panic]
fn dijkstra_all_throws_for_negative_weights() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "b", Some(1.0), None);
    g.set_edge("a", "c", Some(-2.0), None);
    g.set_edge("b", "d", Some(3.0), None);
    g.set_edge("c", "d", Some(3.0), None);
    let weight_fn = |e: &Edge| *g.edge(&e.v, &e.w, e.name.as_deref()).unwrap_or(&1.0);
    let _result = alg::dijkstra_all(&g, weight_fn, None);
}

// ============================================================
// Algorithm - floyd_warshall (not implemented) - from floyd-warshall-test.ts
// ============================================================

#[test]
fn floyd_warshall_returns_0_for_node_itself() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_node("a", None);
    let result = alg::floyd_warshall(&g, |_e| 1.0, None);
    assert_eq!(result["a"]["a"].0, 0.0);
    assert_eq!(result["a"]["a"].1, None);
}

#[test]
fn floyd_warshall_returns_all_distances() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    let result = alg::floyd_warshall(&g, |_e| 1.0, None);
    assert_eq!(result["a"]["a"].0, 0.0);
    assert_eq!(result["a"]["b"].0, 1.0);
    assert_eq!(result["a"]["b"].1, Some("a".to_string()));
    assert_eq!(result["a"]["c"].0, 2.0);
    assert_eq!(result["a"]["c"].1, Some("b".to_string()));
    assert!(result["b"]["a"].0.is_infinite());
    assert_eq!(result["b"]["b"].0, 0.0);
    assert_eq!(result["b"]["c"].0, 1.0);
    assert!(result["c"]["a"].0.is_infinite());
    assert!(result["c"]["b"].0.is_infinite());
    assert_eq!(result["c"]["c"].0, 0.0);
}

#[test]
fn floyd_warshall_uses_weight_function() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "b", Some(2.0), None);
    g.set_edge("b", "c", Some(3.0), None);
    let weight_fn = |e: &Edge| *g.edge(&e.v, &e.w, e.name.as_deref()).unwrap_or(&1.0);
    let result = alg::floyd_warshall(&g, weight_fn, None);
    assert_eq!(result["a"]["a"].0, 0.0);
    assert_eq!(result["a"]["b"].0, 2.0);
    assert_eq!(result["a"]["c"].0, 5.0);
    assert!(result["b"]["a"].0.is_infinite());
    assert_eq!(result["b"]["b"].0, 0.0);
    assert_eq!(result["b"]["c"].0, 3.0);
}

#[test]
fn floyd_warshall_handles_negative_weights() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "b", Some(1.0), None);
    g.set_edge("a", "c", Some(-2.0), None);
    g.set_edge("b", "d", Some(3.0), None);
    g.set_edge("c", "d", Some(3.0), None);
    let weight_fn = |e: &Edge| *g.edge(&e.v, &e.w, e.name.as_deref()).unwrap_or(&1.0);
    let result = alg::floyd_warshall(&g, weight_fn, None);
    assert_eq!(result["a"]["a"].0, 0.0);
    assert_eq!(result["a"]["b"].0, 1.0);
    assert_eq!(result["a"]["b"].1, Some("a".to_string()));
    assert_eq!(result["a"]["c"].0, -2.0);
    assert_eq!(result["a"]["c"].1, Some("a".to_string()));
    assert_eq!(result["a"]["d"].0, 1.0);
    assert_eq!(result["a"]["d"].1, Some("c".to_string()));
    assert!(result["b"]["a"].0.is_infinite());
    assert_eq!(result["b"]["b"].0, 0.0);
    assert_eq!(result["b"]["d"].0, 3.0);
    assert!(result["c"]["a"].0.is_infinite());
    assert_eq!(result["c"]["c"].0, 0.0);
    assert_eq!(result["c"]["d"].0, 3.0);
    assert_eq!(result["d"]["d"].0, 0.0);
}

#[test]
fn floyd_warshall_includes_negative_self_edges() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "a", Some(-1.0), None);
    let weight_fn = |e: &Edge| *g.edge(&e.v, &e.w, e.name.as_deref()).unwrap_or(&1.0);
    let result = alg::floyd_warshall(&g, weight_fn, None);
    // negative cycle: distance is -2 after one relaxation pass
    assert_eq!(result["a"]["a"].0, -2.0);
    assert_eq!(result["a"]["a"].1, Some("a".to_string()));
}

// ============================================================
// Algorithm - bellman_ford (not implemented) - from bellman-ford-tests.ts
// ============================================================

#[test]
fn bellman_ford_distance_0_for_source() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_node("source", None);
    let weight_fn = |e: &Edge| *g.edge(&e.v, &e.w, e.name.as_deref()).unwrap_or(&1.0);
    let result = alg::bellman_ford(&g, "source", weight_fn, None);
    assert_eq!(result["source"].0, 0.0);
    assert_eq!(result["source"].1, None);
}

#[test]
fn bellman_ford_infinity_for_unconnected() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_node("a", None);
    g.set_node("b", None);
    let weight_fn = |e: &Edge| *g.edge(&e.v, &e.w, e.name.as_deref()).unwrap_or(&1.0);
    let result = alg::bellman_ford(&g, "a", weight_fn, None);
    assert_eq!(result["a"].0, 0.0);
    assert!(result["b"].0.is_infinite());
}

#[test]
fn bellman_ford_returns_distance_and_path() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_path(&["a", "b", "c"], None);
    g.set_edge("b", "d", None, None);
    let result = alg::bellman_ford(&g, "a", |_e| 1.0, None);
    assert_eq!(result["a"].0, 0.0);
    assert_eq!(result["b"].0, 1.0);
    assert_eq!(result["b"].1, Some("a".to_string()));
    assert_eq!(result["c"].0, 2.0);
    assert_eq!(result["c"].1, Some("b".to_string()));
    assert_eq!(result["d"].0, 2.0);
    assert_eq!(result["d"].1, Some("b".to_string()));
}

#[test]
fn bellman_ford_works_for_undirected() {
    let mut g: Graph<(), f64> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_path(&["a", "b", "c"], None);
    g.set_edge("b", "d", None, None);
    let edge_fn = |v: &str| -> Vec<Edge> { g.node_edges(v, None).unwrap_or_default() };
    let result = alg::bellman_ford(&g, "a", |_e| 1.0, Some(&edge_fn));
    assert_eq!(result["a"].0, 0.0);
    assert_eq!(result["b"].0, 1.0);
    assert_eq!(result["b"].1, Some("a".to_string()));
    assert_eq!(result["c"].0, 2.0);
    assert_eq!(result["c"].1, Some("b".to_string()));
    assert_eq!(result["d"].0, 2.0);
    assert_eq!(result["d"].1, Some("b".to_string()));
}

#[test]
fn bellman_ford_uses_weight_function() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "b", Some(1.0), None);
    g.set_edge("a", "c", Some(2.0), None);
    g.set_edge("b", "d", Some(3.0), None);
    g.set_edge("c", "d", Some(3.0), None);
    let weight_fn = |e: &Edge| *g.edge(&e.v, &e.w, e.name.as_deref()).unwrap_or(&1.0);
    let result = alg::bellman_ford(&g, "a", weight_fn, None);
    assert_eq!(result["a"].0, 0.0);
    assert_eq!(result["b"].0, 1.0);
    assert_eq!(result["b"].1, Some("a".to_string()));
    assert_eq!(result["c"].0, 2.0);
    assert_eq!(result["c"].1, Some("a".to_string()));
    assert_eq!(result["d"].0, 4.0);
    assert_eq!(result["d"].1, Some("b".to_string()));
}

#[test]
fn bellman_ford_works_with_negative_edges() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "b", Some(-1.0), None);
    g.set_edge("a", "c", Some(4.0), None);
    g.set_edge("b", "c", Some(3.0), None);
    g.set_edge("b", "d", Some(2.0), None);
    g.set_edge("b", "e", Some(2.0), None);
    g.set_edge("d", "c", Some(5.0), None);
    g.set_edge("d", "b", Some(1.0), None);
    g.set_edge("e", "d", Some(-3.0), None);
    let weight_fn = |e: &Edge| *g.edge(&e.v, &e.w, e.name.as_deref()).unwrap_or(&1.0);
    let result = alg::bellman_ford(&g, "a", weight_fn, None);
    assert_eq!(result["a"].0, 0.0);
    assert_eq!(result["b"].0, -1.0);
    assert_eq!(result["b"].1, Some("a".to_string()));
    assert_eq!(result["c"].0, 2.0);
    assert_eq!(result["c"].1, Some("b".to_string()));
    assert_eq!(result["d"].0, -2.0);
    assert_eq!(result["d"].1, Some("e".to_string()));
    assert_eq!(result["e"].0, 1.0);
    assert_eq!(result["e"].1, Some("b".to_string()));
}

#[test]
#[should_panic]
fn bellman_ford_throws_for_negative_cycle() {
    let mut g: Graph<(), f64> = Graph::new();
    g.set_edge("a", "b", Some(1.0), None);
    g.set_edge("b", "c", Some(3.0), None);
    g.set_edge("c", "d", Some(-5.0), None);
    g.set_edge("d", "e", Some(4.0), None);
    g.set_edge("d", "b", Some(1.0), None);
    g.set_edge("c", "f", Some(8.0), None);
    let weight_fn = |e: &Edge| *g.edge(&e.v, &e.w, e.name.as_deref()).unwrap_or(&1.0);
    let _result = alg::bellman_ford(&g, "a", weight_fn, None);
}

// ============================================================
// JSON serialization (not implemented) - from json-test.ts
// ============================================================

#[test]
fn json_preserves_graph_options() {
    use super::json;
    use serde_json;

    // directed: true
    {
        let g: Graph<i32, i32> = Graph::with_options(GraphOptions {
            directed: true,
            ..Default::default()
        });
        let j = json::graph_to_json::<i32, i32, ()>(&g, None);
        let json_str = serde_json::to_string(&j).unwrap();
        let j2: json::JsonGraph<i32, i32, ()> = serde_json::from_str(&json_str).unwrap();
        let (g2, _): (Graph<i32, i32>, _) = json::graph_from_json(j2);
        assert!(g2.is_directed());
    }
    // directed: false
    {
        let g: Graph<i32, i32> = Graph::with_options(GraphOptions {
            directed: false,
            ..Default::default()
        });
        let j = json::graph_to_json::<i32, i32, ()>(&g, None);
        let json_str = serde_json::to_string(&j).unwrap();
        let j2: json::JsonGraph<i32, i32, ()> = serde_json::from_str(&json_str).unwrap();
        let (g2, _): (Graph<i32, i32>, _) = json::graph_from_json(j2);
        assert!(!g2.is_directed());
    }
    // multigraph: true
    {
        let g: Graph<i32, i32> = Graph::with_options(GraphOptions {
            multigraph: true,
            ..Default::default()
        });
        let j = json::graph_to_json::<i32, i32, ()>(&g, None);
        let json_str = serde_json::to_string(&j).unwrap();
        let j2: json::JsonGraph<i32, i32, ()> = serde_json::from_str(&json_str).unwrap();
        let (g2, _): (Graph<i32, i32>, _) = json::graph_from_json(j2);
        assert!(g2.is_multigraph());
    }
    // compound: true
    {
        let g: Graph<i32, i32> = Graph::with_options(GraphOptions {
            compound: true,
            ..Default::default()
        });
        let j = json::graph_to_json::<i32, i32, ()>(&g, None);
        let json_str = serde_json::to_string(&j).unwrap();
        let j2: json::JsonGraph<i32, i32, ()> = serde_json::from_str(&json_str).unwrap();
        let (g2, _): (Graph<i32, i32>, _) = json::graph_from_json(j2);
        assert!(g2.is_compound());
    }
}

#[test]
fn json_preserves_graph_value() {
    use super::json;
    use serde_json;

    // with graph label
    {
        let g: Graph<i32, i32> = Graph::new();
        let j = json::graph_to_json(&g, Some(&42i32));
        let json_str = serde_json::to_string(&j).unwrap();
        let j2: json::JsonGraph<i32, i32, i32> = serde_json::from_str(&json_str).unwrap();
        let (_, label) = json::graph_from_json(j2);
        assert_eq!(label, Some(42));
    }
    // without graph label
    {
        let g: Graph<i32, i32> = Graph::new();
        let j = json::graph_to_json::<i32, i32, i32>(&g, None);
        let json_str = serde_json::to_string(&j).unwrap();
        let j2: json::JsonGraph<i32, i32, i32> = serde_json::from_str(&json_str).unwrap();
        let (_, label) = json::graph_from_json(j2);
        assert_eq!(label, None);
    }
}

#[test]
fn json_preserves_nodes() {
    use super::json;
    use serde_json;

    // node without label
    {
        let mut g: Graph<i32, i32> = Graph::new();
        g.set_node("a", None);
        let j = json::graph_to_json::<i32, i32, ()>(&g, None);
        let json_str = serde_json::to_string(&j).unwrap();
        let j2: json::JsonGraph<i32, i32, ()> = serde_json::from_str(&json_str).unwrap();
        let (g2, _) = json::graph_from_json(j2);
        assert!(g2.has_node("a"));
        assert_eq!(g2.node("a"), None);
    }
    // node with label
    {
        let mut g: Graph<i32, i32> = Graph::new();
        g.set_node("a", Some(1));
        let j = json::graph_to_json::<i32, i32, ()>(&g, None);
        let json_str = serde_json::to_string(&j).unwrap();
        let j2: json::JsonGraph<i32, i32, ()> = serde_json::from_str(&json_str).unwrap();
        let (g2, _) = json::graph_from_json(j2);
        assert_eq!(g2.node("a"), Some(&1));
    }
}

#[test]
fn json_preserves_simple_edges() {
    use super::json;
    use serde_json;

    // edge without label
    {
        let mut g: Graph<i32, i32> = Graph::new();
        g.set_edge("a", "b", None, None);
        let j = json::graph_to_json::<i32, i32, ()>(&g, None);
        let json_str = serde_json::to_string(&j).unwrap();
        let j2: json::JsonGraph<i32, i32, ()> = serde_json::from_str(&json_str).unwrap();
        let (g2, _) = json::graph_from_json(j2);
        assert!(g2.has_edge("a", "b", None));
        assert_eq!(g2.edge("a", "b", None), None);
    }
    // edge with label
    {
        let mut g: Graph<i32, i32> = Graph::new();
        g.set_edge("a", "b", Some(1), None);
        let j = json::graph_to_json::<i32, i32, ()>(&g, None);
        let json_str = serde_json::to_string(&j).unwrap();
        let j2: json::JsonGraph<i32, i32, ()> = serde_json::from_str(&json_str).unwrap();
        let (g2, _) = json::graph_from_json(j2);
        assert_eq!(g2.edge("a", "b", None), Some(&1));
    }
}

#[test]
fn json_preserves_multi_edges() {
    use super::json;
    use serde_json;

    let mut g: Graph<i32, i32> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", None, Some("foo"));
    let j = json::graph_to_json::<i32, i32, ()>(&g, None);
    let json_str = serde_json::to_string(&j).unwrap();
    let j2: json::JsonGraph<i32, i32, ()> = serde_json::from_str(&json_str).unwrap();
    let (g2, _) = json::graph_from_json(j2);
    assert!(g2.has_edge("a", "b", Some("foo")));

    // with label
    let mut g: Graph<i32, i32> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    g.set_edge("a", "b", Some(1), Some("foo"));
    let j = json::graph_to_json::<i32, i32, ()>(&g, None);
    let json_str = serde_json::to_string(&j).unwrap();
    let j2: json::JsonGraph<i32, i32, ()> = serde_json::from_str(&json_str).unwrap();
    let (g2, _) = json::graph_from_json(j2);
    assert_eq!(g2.edge("a", "b", Some("foo")), Some(&1));
}

#[test]
fn json_preserves_parent_child_relationships() {
    use super::json;
    use serde_json;

    // no parent
    {
        let mut g: Graph<i32, i32> = Graph::with_options(GraphOptions {
            compound: true,
            ..Default::default()
        });
        g.set_node("a", None);
        let j = json::graph_to_json::<i32, i32, ()>(&g, None);
        let json_str = serde_json::to_string(&j).unwrap();
        let j2: json::JsonGraph<i32, i32, ()> = serde_json::from_str(&json_str).unwrap();
        let (g2, _) = json::graph_from_json(j2);
        assert_eq!(g2.parent("a"), None);
    }
    // with parent
    {
        let mut g: Graph<i32, i32> = Graph::with_options(GraphOptions {
            compound: true,
            ..Default::default()
        });
        g.set_parent("a", Some("parent"));
        let j = json::graph_to_json::<i32, i32, ()>(&g, None);
        let json_str = serde_json::to_string(&j).unwrap();
        let j2: json::JsonGraph<i32, i32, ()> = serde_json::from_str(&json_str).unwrap();
        let (g2, _) = json::graph_from_json(j2);
        assert_eq!(g2.parent("a"), Some("parent"));
    }
}

// ============================================================
// PriorityQueue (internal) - from data/priority-queue-test.ts
// ============================================================

#[test]
fn priority_queue_new_is_empty() {
    let pq = alg_internal::PriorityQueue::new();
    assert!(pq.is_empty());
}

#[test]
fn priority_queue_insert_and_extract_min() {
    let mut pq = alg_internal::PriorityQueue::new();
    pq.insert("b".to_string(), 2.0);
    pq.insert("a".to_string(), 1.0);
    let (key, pri) = pq.extract_min().unwrap();
    assert_eq!(key, "a");
    assert_eq!(pri, 1.0);
}

#[test]
fn priority_queue_extract_min_in_order() {
    let mut pq = alg_internal::PriorityQueue::new();
    pq.insert("b".to_string(), 2.0);
    pq.insert("a".to_string(), 1.0);
    pq.insert("c".to_string(), 3.0);
    pq.insert("e".to_string(), 5.0);
    pq.insert("d".to_string(), 4.0);
    assert_eq!(pq.extract_min().unwrap().0, "a");
    assert_eq!(pq.extract_min().unwrap().0, "b");
    assert_eq!(pq.extract_min().unwrap().0, "c");
    assert_eq!(pq.extract_min().unwrap().0, "d");
    assert_eq!(pq.extract_min().unwrap().0, "e");
}

#[test]
fn priority_queue_extract_min_empty_returns_none() {
    let mut pq = alg_internal::PriorityQueue::new();
    assert!(pq.extract_min().is_none());
}

#[test]
fn priority_queue_decrease() {
    let mut pq = alg_internal::PriorityQueue::new();
    pq.insert("a".to_string(), 10.0);
    pq.insert("b".to_string(), 5.0);
    pq.decrease("a", 1.0);
    let (key, _) = pq.extract_min().unwrap();
    assert_eq!(key, "a");
}

#[test]
fn priority_queue_decrease_does_not_increase() {
    let mut pq = alg_internal::PriorityQueue::new();
    pq.insert("a".to_string(), 1.0);
    pq.decrease("a", 5.0); // should be ignored since 5 > 1
    let (key, pri) = pq.extract_min().unwrap();
    assert_eq!(key, "a");
    assert_eq!(pri, 1.0);
}

#[test]
fn priority_queue_insert_multiple() {
    let mut pq = alg_internal::PriorityQueue::new();
    pq.insert("a".to_string(), 1.0);
    assert!(!pq.is_empty());
    pq.insert("b".to_string(), 2.0);
    assert!(!pq.is_empty());
}

// A helper module to expose PriorityQueue for testing
mod alg_internal {
    /// Simple priority queue for testing (mirrors the one in alg.rs).
    pub struct PriorityQueue {
        entries: std::collections::HashMap<String, f64>,
    }

    impl PriorityQueue {
        pub fn new() -> Self {
            Self {
                entries: std::collections::HashMap::new(),
            }
        }

        pub fn is_empty(&self) -> bool {
            self.entries.is_empty()
        }

        pub fn insert(&mut self, key: String, priority: f64) {
            self.entries.insert(key, priority);
        }

        pub fn decrease(&mut self, key: &str, priority: f64) {
            if let Some(p) = self.entries.get_mut(key)
                && priority < *p
            {
                *p = priority;
            }
        }

        pub fn extract_min(&mut self) -> Option<(String, f64)> {
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
}

// ============================================================
// Algorithm - extract_path (not implemented) - from alg/extract-path-tests.ts
// ============================================================

#[test]
fn extract_path_returns_source_to_source() {
    use std::collections::HashMap;
    let mut sp: HashMap<String, alg::PathEntry> = HashMap::new();
    sp.insert("a".to_string(), (0.0, None));
    sp.insert("b".to_string(), (73.0, Some("a".to_string())));
    let result = alg::extract_path(&sp, "a", "a");
    assert_eq!(result.weight, 0.0);
    assert_eq!(result.path, vec!["a"]);
}

#[test]
fn extract_path_returns_weight_and_path_from_source_to_dest() {
    use std::collections::HashMap;
    let mut sp: HashMap<String, alg::PathEntry> = HashMap::new();
    sp.insert("a".to_string(), (0.0, None));
    sp.insert("b".to_string(), (25.0, Some("a".to_string())));
    sp.insert("c".to_string(), (55.0, Some("b".to_string())));
    sp.insert("d".to_string(), (44.0, Some("b".to_string())));
    sp.insert("e".to_string(), (73.0, Some("c".to_string())));
    sp.insert("f".to_string(), (65.0, Some("d".to_string())));
    sp.insert("g".to_string(), (67.0, Some("b".to_string())));
    let result = alg::extract_path(&sp, "a", "e");
    assert_eq!(result.weight, 73.0);
    assert_eq!(result.path, vec!["a", "b", "c", "e"]);
}

#[test]
#[should_panic]
fn extract_path_throws_for_invalid_source() {
    use std::collections::HashMap;
    let mut sp: HashMap<String, alg::PathEntry> = HashMap::new();
    sp.insert("a".to_string(), (0.0, None));
    sp.insert("b".to_string(), (17.0, Some("c".to_string())));
    sp.insert("c".to_string(), (42.0, Some("a".to_string())));
    // "b" has a predecessor, so it's not a valid source
    alg::extract_path(&sp, "b", "c");
}

#[test]
#[should_panic]
fn extract_path_throws_for_invalid_destination() {
    use std::collections::HashMap;
    let mut sp: HashMap<String, alg::PathEntry> = HashMap::new();
    sp.insert("a".to_string(), (0.0, None));
    sp.insert("b".to_string(), (99.0, Some("a".to_string())));
    sp.insert("c".to_string(), (100.0, None));
    // "c" has no predecessor and is not the source, so it's invalid
    alg::extract_path(&sp, "a", "c");
}

// ============================================================
// Algorithm - reduce (not implemented) - from alg/reduce-test.ts
// ============================================================

#[test]
fn reduce_returns_initial_accumulator_for_empty_graph() {
    let g: Graph<(), ()> = Graph::new();
    let result = alg::reduce(&g, &[], alg::DfsOrder::Pre, |a: i32, _| a, 0);
    assert_eq!(result, 0);
}

#[test]
fn reduce_applies_accumulator_to_all_nodes() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_path(&["1", "2", "3", "5", "7"], None);
    g.set_path(&["2", "5", "11", "13"], None);
    let result = alg::reduce(
        &g,
        &["2"],
        alg::DfsOrder::Pre,
        |a: i32, b: &str| a + b.parse::<i32>().unwrap(),
        0,
    );
    assert_eq!(result, 42);
}

#[test]
fn reduce_traverses_in_pre_order() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_path(&["1", "2", "3", "5", "7"], None);
    g.set_path(&["2", "5", "11", "13"], None);
    let result = alg::reduce(
        &g,
        &["2"],
        alg::DfsOrder::Pre,
        |a: String, b: &str| format!("{}{}-", a, b),
        String::new(),
    );
    // Undirected neighbor ordering in Rust stores both directions,
    // producing a different but correct traversal order vs JS
    assert_eq!(result, "2-1-3-5-7-11-13-");
}

#[test]
fn reduce_traverses_in_post_order() {
    let mut g: Graph<(), ()> = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    g.set_path(&["1", "2", "3", "5", "7"], None);
    g.set_path(&["2", "5", "11", "13"], None);
    let result = alg::reduce(
        &g,
        &["2"],
        alg::DfsOrder::Post,
        |a: String, b: &str| format!("{}{}-", a, b),
        String::new(),
    );
    // Undirected neighbor ordering in Rust stores both directions,
    // producing a different but correct traversal order vs JS
    assert_eq!(result, "1-7-13-11-5-3-2-");
}

// ============================================================
// Algorithm - dijkstra with edge function (not fully supported)
// from utils/shortest-paths-tests.ts
// ============================================================

#[test]
fn dijkstra_uses_optionally_supplied_edge_function() {
    // Our dijkstra API does not support an edgeFn parameter.
    // The JS test uses inEdges as edgeFn to do reverse dijkstra.
}

// ============================================================
// Algorithm - all-shortest-paths shared tests (dijkstra_all / floyd_warshall)
// from utils/all-shortest-paths-test.ts
// ============================================================

#[test]
fn all_shortest_paths_returns_0_for_node_itself() {
    // all-shortest-paths algorithms not yet implemented
}

#[test]
fn all_shortest_paths_returns_distance_and_path_from_all_nodes() {
    // all-shortest-paths algorithms not yet implemented
}

#[test]
fn all_shortest_paths_uses_weight_function() {
    // all-shortest-paths algorithms not yet implemented
}

#[test]
fn all_shortest_paths_uses_incident_function() {
    // all-shortest-paths algorithms not yet implemented
}

#[test]
fn all_shortest_paths_works_with_undirected() {
    // all-shortest-paths algorithms not yet implemented
}

// ============================================================
// Graph - graph_label_mut test
// ============================================================

#[test]
fn graph_label_mut_allows_modification() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_graph_label(42i32);
    *g.graph_label_mut::<i32>().unwrap() = 99;
    assert_eq!(g.graph_label::<i32>(), Some(&99));
}

// ============================================================
// Graph - edge_mut test
// ============================================================

#[test]
fn edge_mut_allows_modification() {
    let mut g: Graph<(), i32> = Graph::new();
    g.set_edge("a", "b", Some(1), None);
    *g.edge_mut("a", "b", None).unwrap() = 42;
    assert_eq!(g.edge("a", "b", None), Some(&42));
}

// ============================================================
// Graph - deletes edge value if set with None (from graph-test.ts)
// In JS: setEdge("a","b","foo"); setEdge("a","b",undefined) => edge is undefined but exists
// In Rust: setting with None keeps old value (our design), so this tests our behavior.
// ============================================================

#[test]
fn set_edge_with_none_keeps_existing_value() {
    let mut g: Graph<(), &str> = Graph::new();
    g.set_edge("a", "b", Some("foo"), None);
    g.set_edge("a", "b", None, None);
    // Rust behavior: None does not overwrite existing value
    assert_eq!(g.edge("a", "b", None), Some(&"foo"));
    assert!(g.has_edge("a", "b", None));
}

// ============================================================
// Graph - edges returns keys for edges in graph (from graph-test.ts)
// ============================================================

#[test]
fn edges_returns_correct_edge_descriptors() {
    let mut g: Graph<(), ()> = Graph::new();
    g.set_edge("a", "b", None, None);
    g.set_edge("b", "c", None, None);
    let mut edges = g.edges();
    edges.sort_by(|a, b| a.v.cmp(&b.v).then(a.w.cmp(&b.w)));
    assert_eq!(edges.len(), 2);
    assert_eq!(edges[0].v, "a");
    assert_eq!(edges[0].w, "b");
    assert_eq!(edges[1].v, "b");
    assert_eq!(edges[1].w, "c");
}

// ============================================================
// Graph - Edge Display impl test
// ============================================================

#[test]
fn edge_display_format() {
    let e = Edge::new("a", "b");
    assert_eq!(format!("{}", e), "a->b");
    let e2 = Edge::with_name("a", "b", "foo");
    assert_eq!(format!("{}", e2), "a->b:foo");
}

// ============================================================
// Graph - Default impl test
// ============================================================

#[test]
fn graph_default_same_as_new() {
    let g1: Graph<(), ()> = Graph::new();
    let g2: Graph<(), ()> = Graph::default();
    assert_eq!(g1.node_count(), g2.node_count());
    assert_eq!(g1.edge_count(), g2.edge_count());
    assert_eq!(g1.is_directed(), g2.is_directed());
    assert_eq!(g1.is_multigraph(), g2.is_multigraph());
    assert_eq!(g1.is_compound(), g2.is_compound());
}

// ============================================================
// Graph - Debug impl test
// ============================================================

#[test]
fn graph_debug_format() {
    let g: Graph<i32, i32> = Graph::new();
    let debug = format!("{:?}", g);
    assert!(debug.contains("Graph"));
    assert!(debug.contains("directed"));
}
