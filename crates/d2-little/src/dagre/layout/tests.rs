//! Layout tests ported from dagre.js test suite.

use super::*;
use crate::dagre::graph::{Graph, GraphOptions};

fn make_graph() -> Graph<NodeLabel, EdgeLabel> {
    Graph::with_options(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    })
}

fn set_node(g: &mut Graph<NodeLabel, EdgeLabel>, v: &str, w: f64, h: f64) {
    let label = NodeLabel {
        width: w,

        height: h,

        ..Default::default()
    };
    g.set_node(v.to_string(), Some(label));
}

fn set_edge(g: &mut Graph<NodeLabel, EdgeLabel>, v: &str, w: &str) {
    g.set_edge(
        v.to_string(),
        w.to_string(),
        Some(EdgeLabel::default()),
        None,
    );
}

// ============================================================
// Acyclic module tests
// ============================================================

#[test]
fn acyclic_does_not_change_acyclic_graph() {
    let mut g = make_graph();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_edge(&mut g, "a", "b");

    acyclic::run(&mut g, None);
    assert!(g.has_edge("a", "b", None));
}

#[test]
fn acyclic_run_and_undo_restores_graph() {
    let mut g = make_graph();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "b", "a");

    let orig_edge_count = g.edge_count();
    acyclic::run(&mut g, None);
    acyclic::undo(&mut g);
    assert_eq!(g.edge_count(), orig_edge_count);
}

// ============================================================
// Rank module tests
// ============================================================

#[test]
fn rank_assigns_rank_to_single_node() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    rank::rank(&mut g, Ranker::LongestPath);
    assert!(g.node("a").unwrap().rank.is_some());
}

#[test]
fn rank_assigns_ranks_to_chain() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_node(&mut g, "c", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "b", "c");

    rank::rank(&mut g, Ranker::LongestPath);
    let ra = g.node("a").unwrap().rank.unwrap();
    let rb = g.node("b").unwrap().rank.unwrap();
    let rc = g.node("c").unwrap().rank.unwrap();

    assert!(ra < rb);
    assert!(rb < rc);
}

#[test]
fn rank_respects_minlen() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    let el = EdgeLabel {
        minlen: 3,

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el), None);

    rank::rank(&mut g, Ranker::LongestPath);
    let ra = g.node("a").unwrap().rank.unwrap();
    let rb = g.node("b").unwrap().rank.unwrap();
    assert!(rb - ra >= 3);
}

// ============================================================
// Normalize module tests
// ============================================================

#[test]
fn normalize_does_not_change_short_edges() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(1);

    let mut chains = Vec::new();
    normalize::run(&mut g, &mut chains);
    assert!(g.has_edge("a", "b", None));
    assert!(chains.is_empty());
}

#[test]
fn normalize_splits_long_edge() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(3);

    let mut chains = Vec::new();
    normalize::run(&mut g, &mut chains);

    // Original edge should be gone
    assert!(!g.has_edge("a", "b", None));
    // Should have dummy chains
    assert_eq!(chains.len(), 1);
    // Should have more nodes now (2 dummies for span of 3)
    assert!(g.node_count() > 2);
}

// ============================================================
// Order module tests
// ============================================================

#[test]
fn order_assigns_order_to_nodes() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_node(&mut g, "c", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "a", "c");

    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(1);
    g.node_mut("c").unwrap().rank = Some(1);

    order::order(&mut g);

    assert!(g.node("a").unwrap().order.is_some());
    assert!(g.node("b").unwrap().order.is_some());
    assert!(g.node("c").unwrap().order.is_some());
}

// ============================================================
// Cross count tests
// ============================================================

#[test]
fn cross_count_zero_for_no_crossings() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_node(&mut g, "c", 10.0, 10.0);
    set_node(&mut g, "d", 10.0, 10.0);
    set_edge(&mut g, "a", "c");
    set_edge(&mut g, "b", "d");

    g.node_mut("a").unwrap().order = Some(0);
    g.node_mut("b").unwrap().order = Some(1);
    g.node_mut("c").unwrap().order = Some(0);
    g.node_mut("d").unwrap().order = Some(1);

    let layering = vec![
        vec!["a".to_string(), "b".to_string()],
        vec!["c".to_string(), "d".to_string()],
    ];

    let cc = order::cross_count::cross_count(&g, &layering);
    assert_eq!(cc, 0);
}

#[test]
fn cross_count_one_crossing() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_node(&mut g, "c", 10.0, 10.0);
    set_node(&mut g, "d", 10.0, 10.0);
    // a->d and b->c creates one crossing
    set_edge(&mut g, "a", "d");
    set_edge(&mut g, "b", "c");

    g.node_mut("a").unwrap().order = Some(0);
    g.node_mut("b").unwrap().order = Some(1);
    g.node_mut("c").unwrap().order = Some(0);
    g.node_mut("d").unwrap().order = Some(1);

    let layering = vec![
        vec!["a".to_string(), "b".to_string()],
        vec!["c".to_string(), "d".to_string()],
    ];

    let cc = order::cross_count::cross_count(&g, &layering);
    assert_eq!(cc, 1);
}

// ============================================================
// Position module tests
// ============================================================

#[test]
fn position_assigns_coordinates() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 50.0, 20.0);
    set_node(&mut g, "b", 50.0, 20.0);
    set_edge(&mut g, "a", "b");

    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("a").unwrap().order = Some(0);
    g.node_mut("b").unwrap().rank = Some(1);
    g.node_mut("b").unwrap().order = Some(0);

    position::position(&mut g);

    assert!(g.node("a").unwrap().x.is_some());
    assert!(g.node("a").unwrap().y.is_some());
    assert!(g.node("b").unwrap().x.is_some());
    assert!(g.node("b").unwrap().y.is_some());
}

#[test]
fn position_y_increases_with_rank() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 50.0, 20.0);
    set_node(&mut g, "b", 50.0, 20.0);
    set_edge(&mut g, "a", "b");

    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("a").unwrap().order = Some(0);
    g.node_mut("b").unwrap().rank = Some(1);
    g.node_mut("b").unwrap().order = Some(0);

    position::position(&mut g);

    let ya = g.node("a").unwrap().y.unwrap();
    let yb = g.node("b").unwrap().y.unwrap();
    assert!(
        yb > ya,
        "y of rank 1 ({}) should be > y of rank 0 ({})",
        yb,
        ya
    );
}

// ============================================================
// Full layout integration tests
// ============================================================

#[test]
fn layout_single_node() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 50.0, 100.0);
    layout(&mut g, None);

    let a = g.node("a").unwrap();
    assert!(a.x.is_some(), "node should have x coordinate");
    assert!(a.y.is_some(), "node should have y coordinate");
}

#[test]
fn layout_two_connected_nodes() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 50.0, 100.0);
    set_node(&mut g, "b", 75.0, 200.0);
    set_edge(&mut g, "a", "b");
    layout(&mut g, None);

    let a = g.node("a").unwrap();
    let b = g.node("b").unwrap();
    assert!(a.x.is_some());
    assert!(a.y.is_some());
    assert!(b.x.is_some());
    assert!(b.y.is_some());

    // a should be above b (smaller y in TB layout)
    assert!(a.y.unwrap() < b.y.unwrap());
}

#[test]
fn layout_diamond_graph() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 50.0, 50.0);
    set_node(&mut g, "b", 50.0, 50.0);
    set_node(&mut g, "c", 50.0, 50.0);
    set_node(&mut g, "d", 50.0, 50.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "a", "c");
    set_edge(&mut g, "b", "d");
    set_edge(&mut g, "c", "d");
    layout(&mut g, None);

    // All nodes should have coordinates
    for v in &["a", "b", "c", "d"] {
        let n = g.node(v).unwrap();
        assert!(n.x.is_some(), "{} should have x", v);
        assert!(n.y.is_some(), "{} should have y", v);
    }

    // a should be above b, c
    let ya = g.node("a").unwrap().y.unwrap();
    let yb = g.node("b").unwrap().y.unwrap();
    let yc = g.node("c").unwrap().y.unwrap();
    let yd = g.node("d").unwrap().y.unwrap();
    assert!(ya < yb);
    assert!(ya < yc);
    assert!(yb < yd);
    assert!(yc < yd);
}

#[test]
fn layout_chain_graph() {
    let mut g = Graph::new();
    for v in &["a", "b", "c", "d", "e"] {
        set_node(&mut g, v, 30.0, 20.0);
    }
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "b", "c");
    set_edge(&mut g, "c", "d");
    set_edge(&mut g, "d", "e");
    layout(&mut g, None);

    // All y values should be strictly increasing
    let ys: Vec<f64> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|v| g.node(v).unwrap().y.unwrap())
        .collect();

    for i in 1..ys.len() {
        assert!(
            ys[i] > ys[i - 1],
            "y[{}]={} should be > y[{}]={}",
            i,
            ys[i],
            i - 1,
            ys[i - 1]
        );
    }
}

#[test]
fn layout_handles_cycle() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 30.0, 20.0);
    set_node(&mut g, "b", 30.0, 20.0);
    set_node(&mut g, "c", 30.0, 20.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "b", "c");
    set_edge(&mut g, "c", "a");
    layout(&mut g, None);

    // All nodes should have coordinates despite the cycle
    for v in &["a", "b", "c"] {
        let n = g.node(v).unwrap();
        assert!(n.x.is_some(), "{} should have x", v);
        assert!(n.y.is_some(), "{} should have y", v);
    }
}

#[test]
fn layout_handles_disconnected_components() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 30.0, 20.0);
    set_node(&mut g, "b", 30.0, 20.0);
    set_node(&mut g, "c", 30.0, 20.0);
    set_node(&mut g, "d", 30.0, 20.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "c", "d");
    layout(&mut g, None);

    for v in &["a", "b", "c", "d"] {
        let n = g.node(v).unwrap();
        assert!(n.x.is_some(), "{} should have x", v);
        assert!(n.y.is_some(), "{} should have y", v);
    }
}

// ============================================================
// Utility tests
// ============================================================

#[test]
fn intersect_rect_top() {
    let rect = NodeLabel {
        x: Some(0.0),

        y: Some(0.0),

        width: 100.0,

        height: 100.0,

        ..Default::default()
    };
    let point = Point::new(0.0, -200.0);
    let result = util::intersect_rect(&rect, &point);
    assert!((result.x - 0.0).abs() < 0.01);
    assert!((result.y - (-50.0)).abs() < 0.01);
}

#[test]
fn intersect_rect_right() {
    let rect = NodeLabel {
        x: Some(0.0),

        y: Some(0.0),

        width: 100.0,

        height: 100.0,

        ..Default::default()
    };
    let point = Point::new(200.0, 0.0);
    let result = util::intersect_rect(&rect, &point);
    assert!((result.x - 50.0).abs() < 0.01);
    assert!((result.y - 0.0).abs() < 0.01);
}

#[test]
fn build_layer_matrix_produces_correct_layers() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    for (v, rank, order) in &[("a", 0, 0), ("b", 0, 1), ("c", 1, 0)] {
        let label = NodeLabel {
            rank: Some(*rank),

            order: Some(*order),

            ..Default::default()
        };
        g.set_node(v.to_string(), Some(label));
    }

    let layers = util::build_layer_matrix(&g);
    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0], vec!["a", "b"]);
    assert_eq!(layers[1], vec!["c"]);
}

#[test]
fn normalize_ranks_shifts_to_zero() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    for (v, rank) in &[("a", 5), ("b", 7), ("c", 9)] {
        let label = NodeLabel {
            rank: Some(*rank),

            ..Default::default()
        };
        g.set_node(v.to_string(), Some(label));
    }

    util::normalize_ranks(&mut g);
    assert_eq!(g.node("a").unwrap().rank, Some(0));
    assert_eq!(g.node("b").unwrap().rank, Some(2));
    assert_eq!(g.node("c").unwrap().rank, Some(4));
}

// ============================================================
// Additional acyclic tests
// ============================================================

#[test]
fn acyclic_does_not_change_diamond_acyclic_graph() {
    let mut g = make_graph();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_node(&mut g, "c", 10.0, 10.0);
    set_node(&mut g, "d", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "a", "c");
    set_edge(&mut g, "b", "d");
    set_edge(&mut g, "c", "d");

    acyclic::run(&mut g, None);

    assert!(g.has_edge("a", "b", None));
    assert!(g.has_edge("a", "c", None));
    assert!(g.has_edge("b", "d", None));
    assert!(g.has_edge("c", "d", None));
}

#[test]
fn acyclic_breaks_cycles() {
    let mut g = make_graph();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_node(&mut g, "c", 10.0, 10.0);
    set_node(&mut g, "d", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "b", "c");
    set_edge(&mut g, "c", "d");
    set_edge(&mut g, "d", "a");

    acyclic::run(&mut g, None);

    // After acyclic run, graph should have no cycles
    let cycles = crate::dagre::graph::alg::find_cycles(&g);
    assert!(
        cycles.is_empty(),
        "graph should be acyclic after run, found cycles: {:?}",
        cycles
    );
}

#[test]
fn acyclic_undo_preserves_edge_labels() {
    let mut g = make_graph();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    let el = EdgeLabel {
        minlen: 2,

        weight: 3,

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el), None);

    acyclic::run(&mut g, None);
    acyclic::undo(&mut g);

    let label = g.edge("a", "b", None).unwrap();
    assert_eq!(label.minlen, 2);
    assert_eq!(label.weight, 3);
}

#[test]
fn acyclic_run_and_undo_restores_reversed_edges() {
    let mut g = make_graph();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    let el_ab = EdgeLabel {
        minlen: 2,

        weight: 3,

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el_ab), None);
    let el_ba = EdgeLabel {
        minlen: 3,

        weight: 4,

        ..Default::default()
    };
    g.set_edge("b".to_string(), "a".to_string(), Some(el_ba), None);

    acyclic::run(&mut g, None);
    acyclic::undo(&mut g);

    let ab = g.edge("a", "b", None).unwrap();
    assert_eq!(ab.minlen, 2);
    assert_eq!(ab.weight, 3);
    let ba = g.edge("b", "a", None).unwrap();
    assert_eq!(ba.minlen, 3);
    assert_eq!(ba.weight, 4);
    assert_eq!(g.edge_count(), 2);
}

// ============================================================
// Additional normalize tests
// ============================================================

#[test]
fn normalize_splits_two_layer_edge_into_segments() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(2);

    let mut chains = Vec::new();
    normalize::run(&mut g, &mut chains);

    // a should have exactly 1 successor (a dummy node)
    let succs = g.successors("a").unwrap();
    assert_eq!(succs.len(), 1);
    let dummy = &succs[0];
    let dummy_node = g.node(dummy).unwrap();
    assert_eq!(dummy_node.dummy.as_deref(), Some("edge"));
    assert_eq!(dummy_node.rank, Some(1));

    // The dummy should point to b
    let dummy_succs = g.successors(dummy).unwrap();
    assert_eq!(dummy_succs, vec!["b"]);

    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0], *dummy);
}

#[test]
fn normalize_assigns_zero_dims_to_dummy_nodes() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    let el = EdgeLabel {
        width: 10.0,

        height: 10.0,

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el), None);
    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(2);

    let mut chains = Vec::new();
    normalize::run(&mut g, &mut chains);

    let succs = g.successors("a").unwrap();
    assert_eq!(succs.len(), 1);
    let dummy_node = g.node(&succs[0]).unwrap();
    assert_eq!(dummy_node.width, 0.0);
    assert_eq!(dummy_node.height, 0.0);
}

#[test]
fn normalize_preserves_edge_weight() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    let el = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el), None);
    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(2);

    let mut chains = Vec::new();
    normalize::run(&mut g, &mut chains);

    let succs = g.successors("a").unwrap();
    assert_eq!(succs.len(), 1);
    let edge_label = g.edge("a", &succs[0], None).unwrap();
    assert_eq!(edge_label.weight, 2);
}

#[test]
fn normalize_undo_reverses_run() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(2);

    let mut chains = Vec::new();
    normalize::run(&mut g, &mut chains);
    normalize::undo(&mut g, &chains);

    // Original edge should be restored
    assert!(g.has_edge("a", "b", None));
    assert_eq!(g.node("a").unwrap().rank, Some(0));
    assert_eq!(g.node("b").unwrap().rank, Some(2));
}

#[test]
fn normalize_undo_collects_points() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(2);

    let mut chains = Vec::new();
    normalize::run(&mut g, &mut chains);

    // Assign coordinates to the dummy node
    let dummy_id = {
        let succs = g.successors("a").unwrap();
        succs[0].clone()
    };
    g.node_mut(&dummy_id).unwrap().x = Some(5.0);
    g.node_mut(&dummy_id).unwrap().y = Some(10.0);

    normalize::undo(&mut g, &chains);

    let edge_label = g.edge("a", "b", None).unwrap();
    assert_eq!(edge_label.points.len(), 1);
    assert_eq!(edge_label.points[0].x, 5.0);
    assert_eq!(edge_label.points[0].y, 10.0);
}

#[test]
fn normalize_undo_merges_multiple_points() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(4);

    let mut chains = Vec::new();
    normalize::run(&mut g, &mut chains);

    // Assign coordinates to the 3 dummy nodes
    let succs_a = g.successors("a").unwrap();
    assert_eq!(succs_a.len(), 1);
    let d1 = succs_a[0].clone();
    g.node_mut(&d1).unwrap().x = Some(5.0);
    g.node_mut(&d1).unwrap().y = Some(10.0);

    let succs_d1 = g.successors(&d1).unwrap();
    assert_eq!(succs_d1.len(), 1);
    let d2 = succs_d1[0].clone();
    g.node_mut(&d2).unwrap().x = Some(20.0);
    g.node_mut(&d2).unwrap().y = Some(25.0);

    let succs_d2 = g.successors(&d2).unwrap();
    assert_eq!(succs_d2.len(), 1);
    let d3 = succs_d2[0].clone();
    g.node_mut(&d3).unwrap().x = Some(100.0);
    g.node_mut(&d3).unwrap().y = Some(200.0);

    normalize::undo(&mut g, &chains);

    let edge_label = g.edge("a", "b", None).unwrap();
    assert_eq!(edge_label.points.len(), 3);
    assert_eq!(edge_label.points[0].x, 5.0);
    assert_eq!(edge_label.points[0].y, 10.0);
    assert_eq!(edge_label.points[1].x, 20.0);
    assert_eq!(edge_label.points[1].y, 25.0);
    assert_eq!(edge_label.points[2].x, 100.0);
    assert_eq!(edge_label.points[2].y, 200.0);
}

// ============================================================
// Additional rank tests
// ============================================================

#[test]
fn rank_network_simplex_single_node() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    rank::rank(&mut g, Ranker::NetworkSimplex);
    util::normalize_ranks(&mut g);
    assert_eq!(g.node("a").unwrap().rank, Some(0));
}

#[test]
fn rank_network_simplex_two_nodes() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    rank::rank(&mut g, Ranker::NetworkSimplex);
    util::normalize_ranks(&mut g);
    assert_eq!(g.node("a").unwrap().rank, Some(0));
    assert_eq!(g.node("b").unwrap().rank, Some(1));
}

#[test]
fn rank_network_simplex_diamond() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_node(&mut g, "c", 10.0, 10.0);
    set_node(&mut g, "d", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "a", "c");
    set_edge(&mut g, "b", "d");
    set_edge(&mut g, "c", "d");

    rank::rank(&mut g, Ranker::NetworkSimplex);
    util::normalize_ranks(&mut g);

    let ra = g.node("a").unwrap().rank.unwrap();
    let rb = g.node("b").unwrap().rank.unwrap();
    let rc = g.node("c").unwrap().rank.unwrap();
    let rd = g.node("d").unwrap().rank.unwrap();
    assert_eq!(ra, 0);
    assert_eq!(rb, 1);
    assert_eq!(rc, 1);
    assert_eq!(rd, 2);
}

#[test]
fn rank_network_simplex_uses_minlen() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_node(&mut g, "c", 10.0, 10.0);
    set_node(&mut g, "d", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    let el = EdgeLabel {
        minlen: 2,

        ..Default::default()
    };
    g.set_edge(
        "a".to_string(),
        "c".to_string(),
        Some(EdgeLabel::default()),
        None,
    );
    g.set_edge("c".to_string(), "d".to_string(), Some(el), None);
    set_edge(&mut g, "b", "d");

    rank::rank(&mut g, Ranker::NetworkSimplex);
    util::normalize_ranks(&mut g);

    let ra = g.node("a").unwrap().rank.unwrap();
    let rc = g.node("c").unwrap().rank.unwrap();
    let rd = g.node("d").unwrap().rank.unwrap();
    assert!(
        rd - rc >= 2,
        "d.rank - c.rank should be >= 2, got {} - {} = {}",
        rd,
        rc,
        rd - rc
    );
    assert_eq!(ra, 0);
}

#[test]
fn rank_network_simplex_gansner_graph() {
    let mut g = Graph::new();
    for v in &["a", "b", "c", "d", "e", "f", "g", "h"] {
        set_node(&mut g, v, 10.0, 10.0);
    }
    // a -> b -> c -> d -> h
    // a -> e -> g -> h
    // a -> f -> g
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "b", "c");
    set_edge(&mut g, "c", "d");
    set_edge(&mut g, "d", "h");
    set_edge(&mut g, "a", "e");
    set_edge(&mut g, "e", "g");
    set_edge(&mut g, "g", "h");
    set_edge(&mut g, "a", "f");
    set_edge(&mut g, "f", "g");

    rank::rank(&mut g, Ranker::NetworkSimplex);
    util::normalize_ranks(&mut g);

    assert_eq!(g.node("a").unwrap().rank, Some(0));
    assert_eq!(g.node("b").unwrap().rank, Some(1));
    assert_eq!(g.node("c").unwrap().rank, Some(2));
    assert_eq!(g.node("d").unwrap().rank, Some(3));
    assert_eq!(g.node("h").unwrap().rank, Some(4));
    assert_eq!(g.node("e").unwrap().rank, Some(1));
    assert_eq!(g.node("f").unwrap().rank, Some(1));
    assert_eq!(g.node("g").unwrap().rank, Some(2));
}

// ============================================================
// Additional cross count tests
// ============================================================

#[test]
fn cross_count_zero_for_empty_layering() {
    let g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let cc = order::cross_count::cross_count(&g, &[]);
    assert_eq!(cc, 0);
}

#[test]
fn cross_count_weighted_crossing() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    set_node(&mut g, "a1", 10.0, 10.0);
    set_node(&mut g, "a2", 10.0, 10.0);
    set_node(&mut g, "b1", 10.0, 10.0);
    set_node(&mut g, "b2", 10.0, 10.0);
    let el1 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("a1".to_string(), "b1".to_string(), Some(el1), None);
    let el2 = EdgeLabel {
        weight: 3,

        ..Default::default()
    };
    g.set_edge("a2".to_string(), "b2".to_string(), Some(el2), None);

    g.node_mut("a1").unwrap().order = Some(0);
    g.node_mut("a2").unwrap().order = Some(1);
    g.node_mut("b1").unwrap().order = Some(1);
    g.node_mut("b2").unwrap().order = Some(0);

    let layering = vec![
        vec!["a1".to_string(), "a2".to_string()],
        vec!["b2".to_string(), "b1".to_string()],
    ];

    let cc = order::cross_count::cross_count(&g, &layering);
    assert_eq!(cc, 6);
}

#[test]
fn cross_count_across_multiple_layers() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    for v in &["a1", "a2", "b1", "b2", "c1", "c2"] {
        set_node(&mut g, v, 10.0, 10.0);
    }
    set_edge(&mut g, "a1", "b1");
    set_edge(&mut g, "b1", "c1");
    set_edge(&mut g, "a2", "b2");
    set_edge(&mut g, "b2", "c2");

    g.node_mut("a1").unwrap().order = Some(0);
    g.node_mut("a2").unwrap().order = Some(1);
    g.node_mut("b1").unwrap().order = Some(1);
    g.node_mut("b2").unwrap().order = Some(0);
    g.node_mut("c1").unwrap().order = Some(0);
    g.node_mut("c2").unwrap().order = Some(1);

    let layering = vec![
        vec!["a1".to_string(), "a2".to_string()],
        vec!["b2".to_string(), "b1".to_string()],
        vec!["c1".to_string(), "c2".to_string()],
    ];

    let cc = order::cross_count::cross_count(&g, &layering);
    // 1 crossing in layer 0->1, 1 crossing in layer 1->2
    assert_eq!(cc, 2);
}

#[test]
fn cross_count_graph_one() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    for v in &["a", "b", "c", "d", "e", "f", "i"] {
        set_node(&mut g, v, 10.0, 10.0);
    }
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "b", "c");
    set_edge(&mut g, "d", "e");
    set_edge(&mut g, "e", "c");
    set_edge(&mut g, "a", "f");
    set_edge(&mut g, "f", "i");
    set_edge(&mut g, "a", "e");

    // First ordering: [a, d], [b, e, f], [c, i]
    g.node_mut("a").unwrap().order = Some(0);
    g.node_mut("d").unwrap().order = Some(1);
    g.node_mut("b").unwrap().order = Some(0);
    g.node_mut("e").unwrap().order = Some(1);
    g.node_mut("f").unwrap().order = Some(2);
    g.node_mut("c").unwrap().order = Some(0);
    g.node_mut("i").unwrap().order = Some(1);

    let layering = vec![
        vec!["a".to_string(), "d".to_string()],
        vec!["b".to_string(), "e".to_string(), "f".to_string()],
        vec!["c".to_string(), "i".to_string()],
    ];

    let cc = order::cross_count::cross_count(&g, &layering);
    assert_eq!(cc, 1);

    // Second ordering: [d, a], [e, b, f], [c, i] - should have 0 crossings
    g.node_mut("a").unwrap().order = Some(1);
    g.node_mut("d").unwrap().order = Some(0);
    g.node_mut("b").unwrap().order = Some(1);
    g.node_mut("e").unwrap().order = Some(0);
    g.node_mut("f").unwrap().order = Some(2);
    g.node_mut("c").unwrap().order = Some(0);
    g.node_mut("i").unwrap().order = Some(1);

    let layering2 = vec![
        vec!["d".to_string(), "a".to_string()],
        vec!["e".to_string(), "b".to_string(), "f".to_string()],
        vec!["c".to_string(), "i".to_string()],
    ];

    let cc2 = order::cross_count::cross_count(&g, &layering2);
    assert_eq!(cc2, 0);
}

// ============================================================
// Additional utility tests - simplify
// ============================================================

#[test]
fn simplify_copies_no_multiedge() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    let el = EdgeLabel {
        weight: 1,

        minlen: 1,

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el), None);

    let g2 = util::simplify(&g);
    let label = g2.edge("a", "b", None).unwrap();
    assert_eq!(label.weight, 1);
    assert_eq!(label.minlen, 1);
    assert_eq!(g2.edge_count(), 1);
}

#[test]
fn simplify_collapses_multiedges() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    let el1 = EdgeLabel {
        weight: 1,

        minlen: 1,

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el1), None);
    let el2 = EdgeLabel {
        weight: 2,

        minlen: 2,

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el2), Some("multi"));

    let g2 = util::simplify(&g);
    assert!(!g2.is_multigraph());
    let label = g2.edge("a", "b", None).unwrap();
    assert_eq!(label.weight, 3);
    assert_eq!(label.minlen, 2);
    assert_eq!(g2.edge_count(), 1);
}

// ============================================================
// Additional utility tests - asNonCompoundGraph
// ============================================================

#[test]
fn as_non_compound_copies_all_nodes() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(GraphOptions {
        compound: true,
        multigraph: true,
        ..Default::default()
    });
    set_node(&mut g, "a", 50.0, 20.0);
    set_node(&mut g, "b", 30.0, 10.0);

    let g2 = util::as_non_compound_graph(&g);
    assert!(g2.has_node("a"));
    assert!(g2.has_node("b"));
}

#[test]
fn as_non_compound_copies_all_edges() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(GraphOptions {
        compound: true,
        multigraph: true,
        ..Default::default()
    });
    set_node(&mut g, "a", 50.0, 20.0);
    set_node(&mut g, "b", 30.0, 10.0);
    set_edge(&mut g, "a", "b");

    let g2 = util::as_non_compound_graph(&g);
    assert!(g2.has_edge("a", "b", None));
}

#[test]
fn as_non_compound_does_not_copy_compound_nodes() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(GraphOptions {
        compound: true,
        multigraph: true,
        ..Default::default()
    });
    set_node(&mut g, "a", 50.0, 20.0);
    g.set_parent("a", Some("sg1"));

    let g2 = util::as_non_compound_graph(&g);
    assert!(!g2.is_compound());
    assert!(g2.has_node("a"));
    // sg1 has children, so it should NOT be in the non-compound graph
    // (as_non_compound_graph skips nodes that have children)
    assert!(!g2.has_node("sg1"));
}

// ============================================================
// Additional utility tests - successorWeights / predecessorWeights
// ============================================================

#[test]
fn successor_weights_maps_correctly() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_node(&mut g, "c", 10.0, 10.0);
    set_node(&mut g, "d", 10.0, 10.0);
    let el1 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el1), None);
    let el2 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("b".to_string(), "c".to_string(), Some(el2), None);
    let el3 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("b".to_string(), "c".to_string(), Some(el3), Some("multi"));
    let el4 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("b".to_string(), "d".to_string(), Some(el4), Some("multi"));

    let sw = util::successor_weights(&g);
    assert_eq!(sw["a"]["b"], 2);
    assert_eq!(sw["b"]["c"], 3);
    assert_eq!(sw["b"]["d"], 1);
    assert!(sw["c"].is_empty());
    assert!(sw["d"].is_empty());
}

#[test]
fn predecessor_weights_maps_correctly() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_node(&mut g, "c", 10.0, 10.0);
    set_node(&mut g, "d", 10.0, 10.0);
    let el1 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el1), None);
    let el2 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("b".to_string(), "c".to_string(), Some(el2), None);
    let el3 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("b".to_string(), "c".to_string(), Some(el3), Some("multi"));
    let el4 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("b".to_string(), "d".to_string(), Some(el4), Some("multi"));

    let pw = util::predecessor_weights(&g);
    assert!(pw["a"].is_empty());
    assert_eq!(pw["b"]["a"], 2);
    assert_eq!(pw["c"]["b"], 3);
    assert_eq!(pw["d"]["b"], 1);
}

// ============================================================
// Additional utility tests - intersectRect
// ============================================================

#[test]
fn intersect_rect_bottom() {
    let rect = NodeLabel {
        x: Some(0.0),

        y: Some(0.0),

        width: 100.0,

        height: 100.0,

        ..Default::default()
    };
    let point = Point::new(0.0, 200.0);
    let result = util::intersect_rect(&rect, &point);
    assert!((result.x - 0.0).abs() < 0.01);
    assert!((result.y - 50.0).abs() < 0.01);
}

#[test]
fn intersect_rect_left() {
    let rect = NodeLabel {
        x: Some(0.0),

        y: Some(0.0),

        width: 100.0,

        height: 100.0,

        ..Default::default()
    };
    let point = Point::new(-200.0, 0.0);
    let result = util::intersect_rect(&rect, &point);
    assert!((result.x - (-50.0)).abs() < 0.01);
    assert!((result.y - 0.0).abs() < 0.01);
}

#[test]
fn intersect_rect_touches_border() {
    let rect = NodeLabel {
        x: Some(0.0),

        y: Some(0.0),

        width: 1.0,

        height: 1.0,

        ..Default::default()
    };
    // Test various points and verify the result touches the border
    for &(px, py) in &[
        (2.0, 6.0),
        (2.0, -6.0),
        (6.0, 2.0),
        (-6.0, 2.0),
        (5.0, 0.0),
        (0.0, 5.0),
    ] {
        let point = Point::new(px, py);
        let cross = util::intersect_rect(&rect, &point);
        // Either x or y should be at the border
        let at_x_border = (cross.x.abs() - 0.5).abs() < 0.01;
        let at_y_border = (cross.y.abs() - 0.5).abs() < 0.01;
        assert!(
            at_x_border || at_y_border,
            "Point ({}, {}) => ({}, {}) should touch border",
            px,
            py,
            cross.x,
            cross.y
        );
    }
}

// ============================================================
// Additional utility tests - buildLayerMatrix
// ============================================================

#[test]
fn build_layer_matrix_with_three_layers() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    for (v, rank, order) in &[
        ("a", 0, 0),
        ("b", 0, 1),
        ("c", 1, 0),
        ("d", 1, 1),
        ("e", 2, 0),
    ] {
        let label = NodeLabel {
            rank: Some(*rank),

            order: Some(*order),

            ..Default::default()
        };
        g.set_node(v.to_string(), Some(label));
    }

    let layers = util::build_layer_matrix(&g);
    assert_eq!(layers.len(), 3);
    assert_eq!(layers[0], vec!["a", "b"]);
    assert_eq!(layers[1], vec!["c", "d"]);
    assert_eq!(layers[2], vec!["e"]);
}

// ============================================================
// Additional utility tests - normalizeRanks
// ============================================================

#[test]
fn normalize_ranks_works_for_negative_ranks() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    for (v, rank) in &[("a", -3), ("b", -2)] {
        let label = NodeLabel {
            rank: Some(*rank),

            ..Default::default()
        };
        g.set_node(v.to_string(), Some(label));
    }

    util::normalize_ranks(&mut g);
    assert_eq!(g.node("a").unwrap().rank, Some(0));
    assert_eq!(g.node("b").unwrap().rank, Some(1));
}

// ============================================================
// Additional utility tests - removeEmptyRanks
// ============================================================

#[test]
fn remove_empty_ranks_removes_border_ranks() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    // Set nodeRankFactor > 1 so empty ranks get removed
    let gl = GraphLabel {
        node_rank_factor: Some(2.0),

        ..Default::default()
    };
    g.set_graph_label(gl);

    let a = NodeLabel {
        rank: Some(0),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        rank: Some(4),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));

    util::remove_empty_ranks(&mut g);
    assert_eq!(g.node("a").unwrap().rank, Some(0));
    // With nodeRankFactor=2, ranks 0,2,4 are kept (multiples of 2),
    // ranks 1,3 are removed. Rank 4 shifts down by 2 (two removed ranks).
    assert_eq!(g.node("b").unwrap().rank, Some(2));
}

// ============================================================
// Additional full layout integration tests
// ============================================================

#[test]
fn layout_self_loop() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 100.0, 100.0);
    set_edge(&mut g, "a", "a");
    layout(&mut g, None);

    let a = g.node("a").unwrap();
    assert!(a.x.is_some(), "node should have x coordinate");
    assert!(a.y.is_some(), "node should have y coordinate");
}

#[test]
fn layout_wide_node() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 1000.0, 50.0);
    set_node(&mut g, "b", 50.0, 50.0);
    set_edge(&mut g, "a", "b");
    layout(&mut g, None);

    let a = g.node("a").unwrap();
    let b = g.node("b").unwrap();
    assert!(a.x.is_some());
    assert!(a.y.is_some());
    assert!(b.x.is_some());
    assert!(b.y.is_some());
    assert!(a.y.unwrap() < b.y.unwrap());
}

#[test]
fn layout_parallel_edges() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 50.0, 50.0);
    set_node(&mut g, "b", 50.0, 50.0);
    set_node(&mut g, "c", 50.0, 50.0);
    set_node(&mut g, "d", 50.0, 50.0);
    set_edge(&mut g, "a", "c");
    set_edge(&mut g, "a", "d");
    set_edge(&mut g, "b", "c");
    set_edge(&mut g, "b", "d");
    layout(&mut g, None);

    for v in &["a", "b", "c", "d"] {
        let n = g.node(v).unwrap();
        assert!(n.x.is_some(), "{} should have x", v);
        assert!(n.y.is_some(), "{} should have y", v);
    }
}

#[test]
fn layout_longer_chain() {
    let mut g = Graph::new();
    for v in &["a", "b", "c", "d", "e", "f", "g"] {
        set_node(&mut g, v, 30.0, 20.0);
    }
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "b", "c");
    set_edge(&mut g, "c", "d");
    set_edge(&mut g, "d", "e");
    set_edge(&mut g, "e", "f");
    set_edge(&mut g, "f", "g");
    layout(&mut g, None);

    let ys: Vec<f64> = ["a", "b", "c", "d", "e", "f", "g"]
        .iter()
        .map(|v| g.node(v).unwrap().y.unwrap())
        .collect();

    for i in 1..ys.len() {
        assert!(
            ys[i] > ys[i - 1],
            "y[{}]={} should be > y[{}]={}",
            i,
            ys[i],
            i - 1,
            ys[i - 1]
        );
    }
}

#[test]
fn layout_multiple_disconnected_chains() {
    let mut g = Graph::new();
    for v in &["a", "b", "c", "d", "e", "f"] {
        set_node(&mut g, v, 30.0, 20.0);
    }
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "b", "c");
    set_edge(&mut g, "d", "e");
    set_edge(&mut g, "e", "f");
    layout(&mut g, None);

    for v in &["a", "b", "c", "d", "e", "f"] {
        let n = g.node(v).unwrap();
        assert!(n.x.is_some(), "{} should have x", v);
        assert!(n.y.is_some(), "{} should have y", v);
    }
    // Both chains have rank ordering
    assert!(g.node("a").unwrap().y.unwrap() < g.node("b").unwrap().y.unwrap());
    assert!(g.node("d").unwrap().y.unwrap() < g.node("e").unwrap().y.unwrap());
}

#[test]
fn layout_assigns_edge_points() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 50.0, 50.0);
    set_node(&mut g, "b", 50.0, 50.0);
    set_edge(&mut g, "a", "b");
    layout(&mut g, None);

    let edge_label = g.edge("a", "b", None).unwrap();
    assert!(!edge_label.points.is_empty(), "edge should have points");
}

#[test]
fn layout_all_coords_positive() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 50.0, 50.0);
    set_node(&mut g, "b", 50.0, 50.0);
    set_node(&mut g, "c", 50.0, 50.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "a", "c");
    set_edge(&mut g, "b", "c");
    layout(&mut g, None);

    for v in &["a", "b", "c"] {
        let n = g.node(v).unwrap();
        assert!(n.x.unwrap() >= 0.0, "{}.x should be >= 0", v);
        assert!(n.y.unwrap() >= 0.0, "{}.y should be >= 0", v);
    }
}

#[test]
fn layout_nodes_same_rank_different_x() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 50.0, 50.0);
    set_node(&mut g, "b", 50.0, 50.0);
    set_node(&mut g, "c", 50.0, 50.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "a", "c");
    layout(&mut g, None);

    // b and c are on the same rank; they should have different x coordinates
    let xb = g.node("b").unwrap().x.unwrap();
    let xc = g.node("c").unwrap().x.unwrap();
    assert!(
        (xb - xc).abs() > 1.0,
        "b.x ({}) and c.x ({}) should differ",
        xb,
        xc
    );
}

#[test]
fn layout_long_edge_with_label() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 50.0, 100.0);
    set_node(&mut g, "b", 75.0, 200.0);
    let el = EdgeLabel {
        width: 60.0,

        height: 70.0,

        minlen: 2,

        labelpos: LabelPos::Center,

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el), None);
    layout(&mut g, None);

    let ya = g.node("a").unwrap().y.unwrap();
    let yb = g.node("b").unwrap().y.unwrap();
    let edge_label = g.edge("a", "b", None).unwrap();
    // Edge label should be between a and b
    if let (Some(ey), _) = (edge_label.y, edge_label.x) {
        assert!(ey > ya, "edge label y ({}) should be > a.y ({})", ey, ya);
        assert!(ey < yb, "edge label y ({}) should be < b.y ({})", ey, yb);
    }
}

// ============================================================
// Additional order module tests
// ============================================================

#[test]
fn order_assigns_unique_orders_per_rank() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    set_node(&mut g, "c", 10.0, 10.0);
    set_node(&mut g, "d", 10.0, 10.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "a", "c");
    set_edge(&mut g, "b", "d");
    set_edge(&mut g, "c", "d");

    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(1);
    g.node_mut("c").unwrap().rank = Some(1);
    g.node_mut("d").unwrap().rank = Some(2);

    order::order(&mut g);

    // b and c should have different orders
    let ob = g.node("b").unwrap().order.unwrap();
    let oc = g.node("c").unwrap().order.unwrap();
    assert_ne!(ob, oc, "b and c should have different orders");
}

// ============================================================
// Additional position module tests
// ============================================================

#[test]
fn position_two_nodes_same_rank_have_different_x() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 50.0, 20.0);
    set_node(&mut g, "b", 50.0, 20.0);

    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("a").unwrap().order = Some(0);
    g.node_mut("b").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().order = Some(1);

    position::position(&mut g);

    let xa = g.node("a").unwrap().x.unwrap();
    let xb = g.node("b").unwrap().x.unwrap();
    assert!(xb > xa, "b.x ({}) should be > a.x ({})", xb, xa);
}

#[test]
fn position_three_ranks_y_ordering() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 50.0, 20.0);
    set_node(&mut g, "b", 50.0, 20.0);
    set_node(&mut g, "c", 50.0, 20.0);
    set_edge(&mut g, "a", "b");
    set_edge(&mut g, "b", "c");

    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("a").unwrap().order = Some(0);
    g.node_mut("b").unwrap().rank = Some(1);
    g.node_mut("b").unwrap().order = Some(0);
    g.node_mut("c").unwrap().rank = Some(2);
    g.node_mut("c").unwrap().order = Some(0);

    position::position(&mut g);

    let ya = g.node("a").unwrap().y.unwrap();
    let yb = g.node("b").unwrap().y.unwrap();
    let yc = g.node("c").unwrap().y.unwrap();
    assert!(ya < yb);
    assert!(yb < yc);
}

// ============================================================
// Coordinate system tests
// ============================================================

use super::coordinate_system;

fn make_coord_graph(rankdir: RankDir) -> Graph<NodeLabel, EdgeLabel> {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let label = NodeLabel {
        width: 100.0,

        height: 200.0,

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(label));
    g.set_graph_label(GraphLabel {
        rankdir,
        ..Default::default()
    });
    g
}

#[test]
fn coord_adjust_tb_does_nothing() {
    let mut g = make_coord_graph(RankDir::TB);
    coordinate_system::adjust(&mut g);
    let a = g.node("a").unwrap();
    assert_eq!(a.width, 100.0);
    assert_eq!(a.height, 200.0);
}

#[test]
fn coord_adjust_bt_does_nothing() {
    let mut g = make_coord_graph(RankDir::BT);
    coordinate_system::adjust(&mut g);
    let a = g.node("a").unwrap();
    assert_eq!(a.width, 100.0);
    assert_eq!(a.height, 200.0);
}

#[test]
fn coord_adjust_lr_swaps_width_height() {
    let mut g = make_coord_graph(RankDir::LR);
    coordinate_system::adjust(&mut g);
    let a = g.node("a").unwrap();
    assert_eq!(a.width, 200.0);
    assert_eq!(a.height, 100.0);
}

#[test]
fn coord_adjust_rl_swaps_width_height() {
    let mut g = make_coord_graph(RankDir::RL);
    coordinate_system::adjust(&mut g);
    let a = g.node("a").unwrap();
    assert_eq!(a.width, 200.0);
    assert_eq!(a.height, 100.0);
}

fn make_coord_graph_with_pos(rankdir: RankDir) -> Graph<NodeLabel, EdgeLabel> {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let label = NodeLabel {
        width: 100.0,

        height: 200.0,

        x: Some(20.0),

        y: Some(40.0),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(label));
    g.set_graph_label(GraphLabel {
        rankdir,
        ..Default::default()
    });
    g
}

#[test]
fn coord_undo_tb_does_nothing() {
    let mut g = make_coord_graph_with_pos(RankDir::TB);
    coordinate_system::undo(&mut g);
    let a = g.node("a").unwrap();
    assert_eq!(a.x, Some(20.0));
    assert_eq!(a.y, Some(40.0));
    assert_eq!(a.width, 100.0);
    assert_eq!(a.height, 200.0);
}

#[test]
fn coord_undo_bt_negates_y() {
    let mut g = make_coord_graph_with_pos(RankDir::BT);
    coordinate_system::undo(&mut g);
    let a = g.node("a").unwrap();
    assert_eq!(a.x, Some(20.0));
    assert_eq!(a.y, Some(-40.0));
    assert_eq!(a.width, 100.0);
    assert_eq!(a.height, 200.0);
}

#[test]
fn coord_undo_lr_swaps_dims_and_coords() {
    let mut g = make_coord_graph_with_pos(RankDir::LR);
    coordinate_system::undo(&mut g);
    let a = g.node("a").unwrap();
    assert_eq!(a.x, Some(40.0));
    assert_eq!(a.y, Some(20.0));
    assert_eq!(a.width, 200.0);
    assert_eq!(a.height, 100.0);
}

#[test]
fn coord_undo_rl_swaps_and_negates() {
    let mut g = make_coord_graph_with_pos(RankDir::RL);
    coordinate_system::undo(&mut g);
    let a = g.node("a").unwrap();
    assert_eq!(a.x, Some(-40.0));
    assert_eq!(a.y, Some(20.0));
    assert_eq!(a.width, 200.0);
    assert_eq!(a.height, 100.0);
}

// ============================================================
// Ported from dagre-js: nesting-graph-test.ts
// ============================================================

fn make_compound_graph() -> Graph<NodeLabel, EdgeLabel> {
    Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    })
}

#[test]
fn nesting_run_connects_disconnected_graph() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_node("b".to_string(), Some(NodeLabel::default()));
    let comps = crate::dagre::graph::alg::components(&g);
    assert_eq!(comps.len(), 2);
    nesting_graph::run(&mut g);
    let comps2 = crate::dagre::graph::alg::components(&g);
    assert_eq!(comps2.len(), 1);
    assert!(g.has_node("a"));
    assert!(g.has_node("b"));
}

#[test]
fn nesting_run_adds_border_nodes_to_top_and_bottom_of_subgraph() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_parent("a", Some("sg1"));
    nesting_graph::run(&mut g);

    let sg1 = g.node("sg1").unwrap();
    let border_top = sg1.border_top.clone().expect("border_top should be set");
    let border_bottom = sg1
        .border_bottom
        .clone()
        .expect("border_bottom should be set");
    assert_eq!(g.parent(&border_top), Some("sg1"));
    assert_eq!(g.parent(&border_bottom), Some("sg1"));

    // border_top -> a edge exists
    let out_top_a = g.out_edges(&border_top, Some("a")).unwrap_or_default();
    assert_eq!(out_top_a.len(), 1);
    let e = &out_top_a[0];
    assert_eq!(g.edge(&e.v, &e.w, e.name.as_deref()).unwrap().minlen, 1);

    // a -> border_bottom edge exists
    let out_a_bot = g.out_edges("a", Some(&border_bottom)).unwrap_or_default();
    assert_eq!(out_a_bot.len(), 1);
    let e2 = &out_a_bot[0];
    assert_eq!(g.edge(&e2.v, &e2.w, e2.name.as_deref()).unwrap().minlen, 1);

    // border nodes have dummy="border"
    let bt_node = g.node(&border_top).unwrap();
    assert_eq!(bt_node.dummy.as_deref(), Some("border"));
    assert_eq!(bt_node.width, 0.0);
    assert_eq!(bt_node.height, 0.0);
    let bb_node = g.node(&border_bottom).unwrap();
    assert_eq!(bb_node.dummy.as_deref(), Some("border"));
    assert_eq!(bb_node.width, 0.0);
    assert_eq!(bb_node.height, 0.0);
}

#[test]
fn nesting_run_adds_edges_between_borders_of_nested_subgraphs() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_parent("sg2", Some("sg1"));
    g.set_parent("a", Some("sg2"));
    nesting_graph::run(&mut g);

    let sg1_top = g.node("sg1").unwrap().border_top.clone().unwrap();
    let sg1_bot = g.node("sg1").unwrap().border_bottom.clone().unwrap();
    let sg2_top = g.node("sg2").unwrap().border_top.clone().unwrap();
    let sg2_bot = g.node("sg2").unwrap().border_bottom.clone().unwrap();

    let out_sg1_top_sg2_top = g.out_edges(&sg1_top, Some(&sg2_top)).unwrap_or_default();
    assert_eq!(out_sg1_top_sg2_top.len(), 1);
    let e = &out_sg1_top_sg2_top[0];
    assert_eq!(g.edge(&e.v, &e.w, e.name.as_deref()).unwrap().minlen, 1);

    let out_sg2_bot_sg1_bot = g.out_edges(&sg2_bot, Some(&sg1_bot)).unwrap_or_default();
    assert_eq!(out_sg2_bot_sg1_bot.len(), 1);
    let e2 = &out_sg2_bot_sg1_bot[0];
    assert_eq!(g.edge(&e2.v, &e2.w, e2.name.as_deref()).unwrap().minlen, 1);
}

#[test]
fn nesting_run_adds_sufficient_weight_to_border_to_node_edges() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_node("b".to_string(), Some(NodeLabel::default()));
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_parent("x", Some("sg"));
    let el_ax = EdgeLabel {
        weight: 100,

        ..Default::default()
    };
    g.set_edge("a", "x", Some(el_ax), None);
    let el_xb = EdgeLabel {
        weight: 200,

        ..Default::default()
    };
    g.set_edge("x", "b", Some(el_xb), None);
    nesting_graph::run(&mut g);

    let top = g.node("sg").unwrap().border_top.clone().unwrap();
    let bot = g.node("sg").unwrap().border_bottom.clone().unwrap();
    assert!(g.edge(&top, "x", None).unwrap().weight > 300);
    assert!(g.edge("x", &bot, None).unwrap().weight > 300);
}

#[test]
fn nesting_run_adds_edge_from_root_to_top_of_top_level_subgraphs() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_parent("a", Some("sg1"));
    nesting_graph::run(&mut g);

    let root = g
        .graph_label::<GraphLabel>()
        .unwrap()
        .nesting_root
        .clone()
        .unwrap();
    let border_top = g.node("sg1").unwrap().border_top.clone().unwrap();
    let out = g.out_edges(&root, Some(&border_top)).unwrap_or_default();
    assert_eq!(out.len(), 1);
    assert!(g.has_edge(&root, &border_top, None));
}

#[test]
fn nesting_run_adds_edge_from_root_to_each_node_correct_minlen_1() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    nesting_graph::run(&mut g);

    let root = g
        .graph_label::<GraphLabel>()
        .unwrap()
        .nesting_root
        .clone()
        .unwrap();
    let out = g.out_edges(&root, Some("a")).unwrap_or_default();
    assert_eq!(out.len(), 1);
    let e = &out[0];
    let label = g.edge(&e.v, &e.w, e.name.as_deref()).unwrap();
    assert_eq!(label.weight, 0);
    assert_eq!(label.minlen, 1);
}

#[test]
fn nesting_run_adds_edge_from_root_to_each_node_correct_minlen_2() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_parent("a", Some("sg1"));
    nesting_graph::run(&mut g);

    let root = g
        .graph_label::<GraphLabel>()
        .unwrap()
        .nesting_root
        .clone()
        .unwrap();
    let out = g.out_edges(&root, Some("a")).unwrap_or_default();
    assert_eq!(out.len(), 1);
    let e = &out[0];
    let label = g.edge(&e.v, &e.w, e.name.as_deref()).unwrap();
    assert_eq!(label.weight, 0);
    assert_eq!(label.minlen, 3);
}

#[test]
fn nesting_run_adds_edge_from_root_to_each_node_correct_minlen_3() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_parent("sg2", Some("sg1"));
    g.set_parent("a", Some("sg2"));
    nesting_graph::run(&mut g);

    let root = g
        .graph_label::<GraphLabel>()
        .unwrap()
        .nesting_root
        .clone()
        .unwrap();
    let out = g.out_edges(&root, Some("a")).unwrap_or_default();
    assert_eq!(out.len(), 1);
    let e = &out[0];
    let label = g.edge(&e.v, &e.w, e.name.as_deref()).unwrap();
    assert_eq!(label.weight, 0);
    assert_eq!(label.minlen, 5);
}

#[test]
fn nesting_run_does_not_add_edge_from_root_to_itself() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    nesting_graph::run(&mut g);

    let root = g
        .graph_label::<GraphLabel>()
        .unwrap()
        .nesting_root
        .clone()
        .unwrap();
    let out = g.out_edges(&root, Some(&root)).unwrap_or_default();
    assert!(out.is_empty());
}

#[test]
fn nesting_run_expands_inter_node_edges_1() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    let el = EdgeLabel {
        minlen: 1,

        ..Default::default()
    };
    g.set_edge("a", "b", Some(el), None);
    nesting_graph::run(&mut g);
    assert_eq!(g.edge("a", "b", None).unwrap().minlen, 1);
}

#[test]
fn nesting_run_expands_inter_node_edges_2() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_parent("a", Some("sg1"));
    let el = EdgeLabel {
        minlen: 1,

        ..Default::default()
    };
    g.set_edge("a", "b", Some(el), None);
    nesting_graph::run(&mut g);
    assert_eq!(g.edge("a", "b", None).unwrap().minlen, 3);
}

#[test]
fn nesting_run_expands_inter_node_edges_3() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_parent("sg2", Some("sg1"));
    g.set_parent("a", Some("sg2"));
    let el = EdgeLabel {
        minlen: 1,

        ..Default::default()
    };
    g.set_edge("a", "b", Some(el), None);
    nesting_graph::run(&mut g);
    assert_eq!(g.edge("a", "b", None).unwrap().minlen, 5);
}

#[test]
fn nesting_run_sets_minlen_correctly_for_nested_sg_border_to_children() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_node("b".to_string(), Some(NodeLabel::default()));
    g.set_parent("a", Some("sg1"));
    g.set_parent("sg2", Some("sg1"));
    g.set_parent("b", Some("sg2"));
    nesting_graph::run(&mut g);

    let root = g
        .graph_label::<GraphLabel>()
        .unwrap()
        .nesting_root
        .clone()
        .unwrap();
    let sg1_top = g.node("sg1").unwrap().border_top.clone().unwrap();
    let sg1_bot = g.node("sg1").unwrap().border_bottom.clone().unwrap();
    let sg2_top = g.node("sg2").unwrap().border_top.clone().unwrap();
    let sg2_bot = g.node("sg2").unwrap().border_bottom.clone().unwrap();

    assert_eq!(g.edge(&root, &sg1_top, None).unwrap().minlen, 3);
    assert_eq!(g.edge(&sg1_top, &sg2_top, None).unwrap().minlen, 1);
    assert_eq!(g.edge(&sg1_top, "a", None).unwrap().minlen, 2);
    assert_eq!(g.edge("a", &sg1_bot, None).unwrap().minlen, 2);
    assert_eq!(g.edge(&sg2_top, "b", None).unwrap().minlen, 1);
    assert_eq!(g.edge("b", &sg2_bot, None).unwrap().minlen, 1);
    assert_eq!(g.edge(&sg2_bot, &sg1_bot, None).unwrap().minlen, 1);
}

#[test]
fn nesting_cleanup_removes_nesting_edges() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_parent("a", Some("sg1"));
    let el = EdgeLabel {
        minlen: 1,

        ..Default::default()
    };
    g.set_edge("a", "b", Some(el), None);
    nesting_graph::run(&mut g);
    let root = g
        .graph_label::<GraphLabel>()
        .unwrap()
        .nesting_root
        .clone()
        .unwrap();
    nesting_graph::cleanup(&mut g, &root);
    let succs = g.successors("a").unwrap_or_default();
    assert_eq!(succs, vec!["b"]);
}

#[test]
fn nesting_cleanup_removes_root_node() {
    let mut g = make_compound_graph();
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_parent("a", Some("sg1"));
    nesting_graph::run(&mut g);
    let root = g
        .graph_label::<GraphLabel>()
        .unwrap()
        .nesting_root
        .clone()
        .unwrap();
    nesting_graph::cleanup(&mut g, &root);
    // sg1 + sg1_border_top + sg1_border_bottom + "a" = 4
    assert_eq!(g.node_count(), 4);
}

// ============================================================
// Ported from dagre-js: normalize-test.ts
// (additional tests not already present)
// ============================================================

#[test]
fn normalize_run_assigns_width_height_from_edge_on_label_rank() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    let el = EdgeLabel {
        width: 20.0,

        height: 10.0,

        label_rank: Some(2.0),

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el), None);
    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(4);

    let mut chains = Vec::new();
    normalize::run(&mut g, &mut chains);

    // The node at rank 2 should be the edge-label dummy with full dimensions
    let succs_a = g.successors("a").unwrap();
    assert_eq!(succs_a.len(), 1);
    let d1 = &succs_a[0];
    let succs_d1 = g.successors(d1).unwrap();
    assert_eq!(succs_d1.len(), 1);
    let label_v = &succs_d1[0];
    let label_node = g.node(label_v).unwrap();
    assert_eq!(label_node.width, 20.0);
    assert_eq!(label_node.height, 10.0);
}

#[test]
fn normalize_undo_sets_coords_and_dims_for_edge_label() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    let el = EdgeLabel {
        width: 10.0,

        height: 20.0,

        label_rank: Some(1.0),

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el), None);
    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(2);

    let mut chains = Vec::new();
    normalize::run(&mut g, &mut chains);

    // Set coords on the label dummy
    let succs_a = g.successors("a").unwrap();
    let label_v = succs_a[0].clone();
    g.node_mut(&label_v).unwrap().x = Some(50.0);
    g.node_mut(&label_v).unwrap().y = Some(60.0);
    g.node_mut(&label_v).unwrap().width = 20.0;
    g.node_mut(&label_v).unwrap().height = 10.0;

    normalize::undo(&mut g, &chains);

    let el = g.edge("a", "b", None).unwrap();
    assert_eq!(el.x, Some(50.0));
    assert_eq!(el.y, Some(60.0));
    assert_eq!(el.width, 20.0);
    assert_eq!(el.height, 10.0);
}

#[test]
fn normalize_undo_sets_coords_and_dims_for_long_edge_label() {
    let mut g = Graph::new();
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    let el = EdgeLabel {
        width: 10.0,

        height: 20.0,

        label_rank: Some(2.0),

        ..Default::default()
    };
    g.set_edge("a".to_string(), "b".to_string(), Some(el), None);
    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(4);

    let mut chains = Vec::new();
    normalize::run(&mut g, &mut chains);

    // Navigate to the label node (at rank 2)
    let d1 = g.successors("a").unwrap()[0].clone();
    let label_v = g.successors(&d1).unwrap()[0].clone();
    g.node_mut(&label_v).unwrap().x = Some(50.0);
    g.node_mut(&label_v).unwrap().y = Some(60.0);
    g.node_mut(&label_v).unwrap().width = 20.0;
    g.node_mut(&label_v).unwrap().height = 10.0;

    normalize::undo(&mut g, &chains);

    let el = g.edge("a", "b", None).unwrap();
    assert_eq!(el.x, Some(50.0));
    assert_eq!(el.y, Some(60.0));
    assert_eq!(el.width, 20.0);
    assert_eq!(el.height, 10.0);
}

#[test]
fn normalize_undo_restores_multi_edges() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });
    set_node(&mut g, "a", 10.0, 10.0);
    set_node(&mut g, "b", 10.0, 10.0);
    g.node_mut("a").unwrap().rank = Some(0);
    g.node_mut("b").unwrap().rank = Some(2);
    g.set_edge(
        "a".to_string(),
        "b".to_string(),
        Some(EdgeLabel::default()),
        Some("bar"),
    );
    g.set_edge(
        "a".to_string(),
        "b".to_string(),
        Some(EdgeLabel::default()),
        Some("foo"),
    );

    let mut chains = Vec::new();
    normalize::run(&mut g, &mut chains);

    // Find the two dummy nodes from "a"
    let mut out_edges = g.out_edges("a", None).unwrap_or_default();
    out_edges.sort_by(|a, b| {
        let an = a.name.as_deref().unwrap_or("");
        let bn = b.name.as_deref().unwrap_or("");
        an.cmp(bn)
    });
    assert_eq!(out_edges.len(), 2);

    let bar_dummy = out_edges[0].w.clone();
    g.node_mut(&bar_dummy).unwrap().x = Some(5.0);
    g.node_mut(&bar_dummy).unwrap().y = Some(10.0);

    let foo_dummy = out_edges[1].w.clone();
    g.node_mut(&foo_dummy).unwrap().x = Some(15.0);
    g.node_mut(&foo_dummy).unwrap().y = Some(20.0);

    normalize::undo(&mut g, &chains);

    // After undo, multi-edges should be restored
    assert!(!g.has_edge("a", "b", None));
    let bar_label = g.edge("a", "b", Some("bar")).unwrap();
    assert_eq!(bar_label.points.len(), 1);
    assert_eq!(bar_label.points[0].x, 5.0);
    assert_eq!(bar_label.points[0].y, 10.0);
    let foo_label = g.edge("a", "b", Some("foo")).unwrap();
    assert_eq!(foo_label.points.len(), 1);
    assert_eq!(foo_label.points[0].x, 15.0);
    assert_eq!(foo_label.points[0].y, 20.0);
}

// ============================================================
// Ported from dagre-js: parent-dummy-chains-test.ts
// ============================================================

#[test]
fn parent_dummy_chains_no_parent_if_both_have_no_parent() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    g.set_graph_label(GraphLabel::default());
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_node("b".to_string(), Some(NodeLabel::default()));
    let d1 = NodeLabel {
        edge_obj: Some(crate::dagre::graph::Edge::new("a", "b")),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d1".to_string(), Some(d1));
    if let Some(gl) = g.graph_label_mut::<GraphLabel>() {
        gl.dummy_chains = vec!["d1".to_string()];
    }
    g.set_path(&["a", "d1", "b"], Some(EdgeLabel::default()));

    parent_dummy_chains::parent_dummy_chains(&mut g);
    assert!(g.parent("d1").is_none());
}

#[test]
fn parent_dummy_chains_uses_tails_parent_for_first_node() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    g.set_graph_label(GraphLabel::default());
    g.set_parent("a", Some("sg1"));
    let sg1 = NodeLabel {
        min_rank: Some(0),

        max_rank: Some(2),

        ..Default::default()
    };
    g.set_node("sg1".to_string(), Some(sg1));
    let d1 = NodeLabel {
        edge_obj: Some(crate::dagre::graph::Edge::new("a", "b")),

        rank: Some(2),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d1".to_string(), Some(d1));
    if let Some(gl) = g.graph_label_mut::<GraphLabel>() {
        gl.dummy_chains = vec!["d1".to_string()];
    }
    g.set_path(&["a", "d1", "b"], Some(EdgeLabel::default()));

    parent_dummy_chains::parent_dummy_chains(&mut g);
    assert_eq!(g.parent("d1"), Some("sg1"));
}

#[test]
fn parent_dummy_chains_uses_heads_parent_if_tails_is_root() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    g.set_graph_label(GraphLabel::default());
    g.set_parent("b", Some("sg1"));
    let sg1 = NodeLabel {
        min_rank: Some(1),

        max_rank: Some(3),

        ..Default::default()
    };
    g.set_node("sg1".to_string(), Some(sg1));
    let d1 = NodeLabel {
        edge_obj: Some(crate::dagre::graph::Edge::new("a", "b")),

        rank: Some(1),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d1".to_string(), Some(d1));
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    if let Some(gl) = g.graph_label_mut::<GraphLabel>() {
        gl.dummy_chains = vec!["d1".to_string()];
    }
    g.set_path(&["a", "d1", "b"], Some(EdgeLabel::default()));

    parent_dummy_chains::parent_dummy_chains(&mut g);
    assert_eq!(g.parent("d1"), Some("sg1"));
}

#[test]
fn parent_dummy_chains_handles_long_chain_starting_in_subgraph() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    g.set_graph_label(GraphLabel::default());
    g.set_parent("a", Some("sg1"));
    let sg1 = NodeLabel {
        min_rank: Some(0),

        max_rank: Some(2),

        ..Default::default()
    };
    g.set_node("sg1".to_string(), Some(sg1));

    let d1 = NodeLabel {
        edge_obj: Some(crate::dagre::graph::Edge::new("a", "b")),

        rank: Some(2),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d1".to_string(), Some(d1));

    let d2 = NodeLabel {
        rank: Some(3),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d2".to_string(), Some(d2));

    let d3 = NodeLabel {
        rank: Some(4),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d3".to_string(), Some(d3));

    g.set_node("b".to_string(), Some(NodeLabel::default()));

    if let Some(gl) = g.graph_label_mut::<GraphLabel>() {
        gl.dummy_chains = vec!["d1".to_string()];
    }
    g.set_path(&["a", "d1", "d2", "d3", "b"], Some(EdgeLabel::default()));

    parent_dummy_chains::parent_dummy_chains(&mut g);
    assert_eq!(g.parent("d1"), Some("sg1"));
    assert!(g.parent("d2").is_none());
    assert!(g.parent("d3").is_none());
}

#[test]
fn parent_dummy_chains_handles_long_chain_ending_in_subgraph() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    g.set_graph_label(GraphLabel::default());
    g.set_parent("b", Some("sg1"));
    let sg1 = NodeLabel {
        min_rank: Some(3),

        max_rank: Some(5),

        ..Default::default()
    };
    g.set_node("sg1".to_string(), Some(sg1));

    let d1 = NodeLabel {
        edge_obj: Some(crate::dagre::graph::Edge::new("a", "b")),

        rank: Some(1),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d1".to_string(), Some(d1));

    let d2 = NodeLabel {
        rank: Some(2),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d2".to_string(), Some(d2));

    let d3 = NodeLabel {
        rank: Some(3),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d3".to_string(), Some(d3));

    g.set_node("a".to_string(), Some(NodeLabel::default()));

    if let Some(gl) = g.graph_label_mut::<GraphLabel>() {
        gl.dummy_chains = vec!["d1".to_string()];
    }
    g.set_path(&["a", "d1", "d2", "d3", "b"], Some(EdgeLabel::default()));

    parent_dummy_chains::parent_dummy_chains(&mut g);
    assert!(g.parent("d1").is_none());
    assert!(g.parent("d2").is_none());
    assert_eq!(g.parent("d3"), Some("sg1"));
}

#[test]
fn parent_dummy_chains_handles_nested_subgraphs() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    g.set_graph_label(GraphLabel::default());
    g.set_parent("a", Some("sg2"));
    g.set_parent("sg2", Some("sg1"));
    let sg1 = NodeLabel {
        min_rank: Some(0),

        max_rank: Some(4),

        ..Default::default()
    };
    g.set_node("sg1".to_string(), Some(sg1));
    let sg2 = NodeLabel {
        min_rank: Some(1),

        max_rank: Some(3),

        ..Default::default()
    };
    g.set_node("sg2".to_string(), Some(sg2));

    g.set_parent("b", Some("sg4"));
    g.set_parent("sg4", Some("sg3"));
    let sg3 = NodeLabel {
        min_rank: Some(6),

        max_rank: Some(10),

        ..Default::default()
    };
    g.set_node("sg3".to_string(), Some(sg3));
    let sg4 = NodeLabel {
        min_rank: Some(7),

        max_rank: Some(9),

        ..Default::default()
    };
    g.set_node("sg4".to_string(), Some(sg4));

    for i in 0..5 {
        let name = format!("d{}", i + 1);
        let mut d = NodeLabel {
            rank: Some(i + 3),

            dummy: Some("edge".to_string()),

            ..Default::default()
        };
        if i == 0 {
            d.edge_obj = Some(crate::dagre::graph::Edge::new("a", "b"));
        }
        g.set_node(name, Some(d));
    }

    if let Some(gl) = g.graph_label_mut::<GraphLabel>() {
        gl.dummy_chains = vec!["d1".to_string()];
    }
    g.set_path(
        &["a", "d1", "d2", "d3", "d4", "d5", "b"],
        Some(EdgeLabel::default()),
    );

    parent_dummy_chains::parent_dummy_chains(&mut g);
    assert_eq!(g.parent("d1"), Some("sg2"));
    assert_eq!(g.parent("d2"), Some("sg1"));
    assert!(g.parent("d3").is_none());
    assert_eq!(g.parent("d4"), Some("sg3"));
    assert_eq!(g.parent("d5"), Some("sg4"));
}

#[test]
fn parent_dummy_chains_handles_overlapping_rank_ranges() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    g.set_graph_label(GraphLabel::default());
    g.set_parent("a", Some("sg1"));
    let sg1 = NodeLabel {
        min_rank: Some(0),

        max_rank: Some(3),

        ..Default::default()
    };
    g.set_node("sg1".to_string(), Some(sg1));

    g.set_parent("b", Some("sg2"));
    let sg2 = NodeLabel {
        min_rank: Some(2),

        max_rank: Some(6),

        ..Default::default()
    };
    g.set_node("sg2".to_string(), Some(sg2));

    let d1 = NodeLabel {
        edge_obj: Some(crate::dagre::graph::Edge::new("a", "b")),

        rank: Some(2),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d1".to_string(), Some(d1));

    let d2 = NodeLabel {
        rank: Some(3),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d2".to_string(), Some(d2));

    let d3 = NodeLabel {
        rank: Some(4),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d3".to_string(), Some(d3));

    g.set_node("a".to_string(), Some(NodeLabel::default()));

    if let Some(gl) = g.graph_label_mut::<GraphLabel>() {
        gl.dummy_chains = vec!["d1".to_string()];
    }
    g.set_path(&["a", "d1", "d2", "d3", "b"], Some(EdgeLabel::default()));

    parent_dummy_chains::parent_dummy_chains(&mut g);
    assert_eq!(g.parent("d1"), Some("sg1"));
    assert_eq!(g.parent("d2"), Some("sg1"));
    assert_eq!(g.parent("d3"), Some("sg2"));
}

#[test]
fn parent_dummy_chains_handles_lca_not_root_1() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    g.set_graph_label(GraphLabel::default());
    g.set_parent("a", Some("sg1"));
    g.set_parent("sg2", Some("sg1"));
    let sg1 = NodeLabel {
        min_rank: Some(0),

        max_rank: Some(6),

        ..Default::default()
    };
    g.set_node("sg1".to_string(), Some(sg1));
    g.set_parent("b", Some("sg2"));
    let sg2 = NodeLabel {
        min_rank: Some(3),

        max_rank: Some(5),

        ..Default::default()
    };
    g.set_node("sg2".to_string(), Some(sg2));

    let d1 = NodeLabel {
        edge_obj: Some(crate::dagre::graph::Edge::new("a", "b")),

        rank: Some(2),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d1".to_string(), Some(d1));

    let d2 = NodeLabel {
        rank: Some(3),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d2".to_string(), Some(d2));

    g.set_node("a".to_string(), Some(NodeLabel::default()));

    if let Some(gl) = g.graph_label_mut::<GraphLabel>() {
        gl.dummy_chains = vec!["d1".to_string()];
    }
    g.set_path(&["a", "d1", "d2", "b"], Some(EdgeLabel::default()));

    parent_dummy_chains::parent_dummy_chains(&mut g);
    assert_eq!(g.parent("d1"), Some("sg1"));
    assert_eq!(g.parent("d2"), Some("sg2"));
}

#[test]
fn parent_dummy_chains_handles_lca_not_root_2() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    g.set_graph_label(GraphLabel::default());
    g.set_parent("a", Some("sg2"));
    g.set_parent("sg2", Some("sg1"));
    let sg1 = NodeLabel {
        min_rank: Some(0),

        max_rank: Some(6),

        ..Default::default()
    };
    g.set_node("sg1".to_string(), Some(sg1));
    g.set_parent("b", Some("sg1"));
    let sg2 = NodeLabel {
        min_rank: Some(1),

        max_rank: Some(3),

        ..Default::default()
    };
    g.set_node("sg2".to_string(), Some(sg2));

    let d1 = NodeLabel {
        edge_obj: Some(crate::dagre::graph::Edge::new("a", "b")),

        rank: Some(3),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d1".to_string(), Some(d1));

    let d2 = NodeLabel {
        rank: Some(4),

        dummy: Some("edge".to_string()),

        ..Default::default()
    };
    g.set_node("d2".to_string(), Some(d2));

    g.set_node("a".to_string(), Some(NodeLabel::default()));

    if let Some(gl) = g.graph_label_mut::<GraphLabel>() {
        gl.dummy_chains = vec!["d1".to_string()];
    }
    g.set_path(&["a", "d1", "d2", "b"], Some(EdgeLabel::default()));

    parent_dummy_chains::parent_dummy_chains(&mut g);
    assert_eq!(g.parent("d1"), Some("sg2"));
    assert_eq!(g.parent("d2"), Some("sg1"));
}

// ============================================================
// Ported from dagre-js: greedy-fas-test.ts
// ============================================================

#[test]
fn greedy_fas_returns_empty_for_empty_graph() {
    let g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let weight_fn = |e: &crate::dagre::graph::Edge| -> i32 {
        g.edge(&e.v, &e.w, e.name.as_deref())
            .map_or(1, |l| l.weight)
    };
    let fas = acyclic::greedy_fas(&g, &weight_fn);
    assert!(fas.is_empty());
}

#[test]
fn greedy_fas_returns_empty_for_single_node() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    let weight_fn = |e: &crate::dagre::graph::Edge| -> i32 {
        g.edge(&e.v, &e.w, e.name.as_deref())
            .map_or(1, |l| l.weight)
    };
    let fas = acyclic::greedy_fas(&g, &weight_fn);
    assert!(fas.is_empty());
}

#[test]
fn greedy_fas_returns_empty_for_acyclic_graph() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    g.set_edge("a", "b", Some(EdgeLabel::default()), None);
    g.set_edge("b", "c", Some(EdgeLabel::default()), None);
    g.set_edge("b", "d", Some(EdgeLabel::default()), None);
    g.set_edge("a", "e", Some(EdgeLabel::default()), None);
    let weight_fn = |e: &crate::dagre::graph::Edge| -> i32 {
        g.edge(&e.v, &e.w, e.name.as_deref())
            .map_or(1, |l| l.weight)
    };
    let fas = acyclic::greedy_fas(&g, &weight_fn);
    assert!(fas.is_empty());
}

#[test]
fn greedy_fas_returns_single_edge_for_simple_cycle() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    g.set_edge("a", "b", Some(EdgeLabel::default()), None);
    g.set_edge("b", "a", Some(EdgeLabel::default()), None);
    let fas = {
        let weight_fn = |e: &crate::dagre::graph::Edge| -> i32 {
            g.edge(&e.v, &e.w, e.name.as_deref())
                .map_or(1, |l| l.weight)
        };
        acyclic::greedy_fas(&g, &weight_fn)
    };
    // After removing FAS edges, graph should be acyclic
    for e in &fas {
        g.remove_edge(&e.v, &e.w, e.name.as_deref());
    }
    assert!(crate::dagre::graph::alg::find_cycles(&g).is_empty());
}

#[test]
fn greedy_fas_returns_single_edge_in_4_node_cycle() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    g.set_edge("n1", "n2", Some(EdgeLabel::default()), None);
    g.set_path(&["n2", "n3", "n4", "n5", "n2"], Some(EdgeLabel::default()));
    g.set_edge("n3", "n5", Some(EdgeLabel::default()), None);
    g.set_edge("n4", "n2", Some(EdgeLabel::default()), None);
    g.set_edge("n4", "n6", Some(EdgeLabel::default()), None);

    let n = g.node_count();
    let m = g.edge_count();
    let fas = {
        let weight_fn = |e: &crate::dagre::graph::Edge| -> i32 {
            g.edge(&e.v, &e.w, e.name.as_deref())
                .map_or(1, |l| l.weight)
        };
        acyclic::greedy_fas(&g, &weight_fn)
    };
    for e in &fas {
        g.remove_edge(&e.v, &e.w, e.name.as_deref());
    }
    assert!(crate::dagre::graph::alg::find_cycles(&g).is_empty());
    assert!(fas.len() <= m / 2 - n / 6);
}

#[test]
fn greedy_fas_returns_two_edges_for_two_4_node_cycles() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    g.set_edge("n1", "n2", Some(EdgeLabel::default()), None);
    g.set_path(&["n2", "n3", "n4", "n5", "n2"], Some(EdgeLabel::default()));
    g.set_edge("n3", "n5", Some(EdgeLabel::default()), None);
    g.set_edge("n4", "n2", Some(EdgeLabel::default()), None);
    g.set_edge("n4", "n6", Some(EdgeLabel::default()), None);
    g.set_path(&["n6", "n7", "n8", "n9", "n6"], Some(EdgeLabel::default()));
    g.set_edge("n7", "n9", Some(EdgeLabel::default()), None);
    g.set_edge("n8", "n6", Some(EdgeLabel::default()), None);
    g.set_edge("n8", "n10", Some(EdgeLabel::default()), None);

    let n = g.node_count();
    let m = g.edge_count();
    let fas = {
        let weight_fn = |e: &crate::dagre::graph::Edge| -> i32 {
            g.edge(&e.v, &e.w, e.name.as_deref())
                .map_or(1, |l| l.weight)
        };
        acyclic::greedy_fas(&g, &weight_fn)
    };
    for e in &fas {
        g.remove_edge(&e.v, &e.w, e.name.as_deref());
    }
    assert!(crate::dagre::graph::alg::find_cycles(&g).is_empty());
    assert!(fas.len() <= m / 2 - n / 6);
}

#[test]
fn greedy_fas_works_with_weighted_edges() {
    // g1: n1->n2 weight 2, n2->n1 weight 1 => should reverse n2->n1
    let mut g1: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let el1 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g1.set_edge("n1", "n2", Some(el1), None);
    let el2 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g1.set_edge("n2", "n1", Some(el2), None);
    let fas1 = {
        let weight_fn = |e: &crate::dagre::graph::Edge| -> i32 {
            g1.edge(&e.v, &e.w, e.name.as_deref())
                .map_or(1, |l| l.weight)
        };
        acyclic::greedy_fas(&g1, &weight_fn)
    };
    assert_eq!(fas1.len(), 1);
    assert_eq!(fas1[0].v, "n2");
    assert_eq!(fas1[0].w, "n1");

    // g2: n1->n2 weight 1, n2->n1 weight 2 => should reverse n1->n2
    let mut g2: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let el3 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g2.set_edge("n1", "n2", Some(el3), None);
    let el4 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g2.set_edge("n2", "n1", Some(el4), None);
    let fas2 = {
        let weight_fn = |e: &crate::dagre::graph::Edge| -> i32 {
            g2.edge(&e.v, &e.w, e.name.as_deref())
                .map_or(1, |l| l.weight)
        };
        acyclic::greedy_fas(&g2, &weight_fn)
    };
    assert_eq!(fas2.len(), 1);
    assert_eq!(fas2[0].v, "n1");
    assert_eq!(fas2[0].w, "n2");
}

#[test]
fn greedy_fas_works_for_multigraphs() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: true,
        compound: false,
    });
    let el1 = EdgeLabel {
        weight: 5,

        ..Default::default()
    };
    g.set_edge("a", "b", Some(el1), Some("foo"));
    let el2 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("b", "a", Some(el2), Some("bar"));
    let el3 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("b", "a", Some(el3), Some("baz"));
    let fas = {
        let weight_fn = |e: &crate::dagre::graph::Edge| -> i32 {
            g.edge(&e.v, &e.w, e.name.as_deref())
                .map_or(1, |l| l.weight)
        };
        acyclic::greedy_fas(&g, &weight_fn)
    };
    let mut fas_sorted = fas.clone();
    fas_sorted.sort_by(|a, b| {
        a.name
            .as_deref()
            .unwrap_or("")
            .cmp(b.name.as_deref().unwrap_or(""))
    });
    assert_eq!(fas_sorted.len(), 2);
    assert_eq!(fas_sorted[0].v, "b");
    assert_eq!(fas_sorted[0].w, "a");
    assert_eq!(fas_sorted[0].name.as_deref(), Some("bar"));
    assert_eq!(fas_sorted[1].v, "b");
    assert_eq!(fas_sorted[1].w, "a");
    assert_eq!(fas_sorted[1].name.as_deref(), Some("baz"));
}

// ============================================================
// Ported from dagre-js: order/resolve-conflicts-test.ts
// ============================================================

use order::barycenter::BarycenterEntry;
use order::resolve_conflicts::{ResolvedEntry, resolve_conflicts};

fn sort_resolved(mut entries: Vec<ResolvedEntry>) -> Vec<ResolvedEntry> {
    entries.sort_by(|a, b| a.vs[0].cmp(&b.vs[0]));
    entries
}

#[test]
fn resolve_conflicts_returns_nodes_unchanged_no_constraints() {
    let cg: Graph<(), ()> = Graph::new();
    let input = vec![
        BarycenterEntry {
            v: "a".to_string(),
            barycenter: Some(2.0),
            weight: Some(3),
        },
        BarycenterEntry {
            v: "b".to_string(),
            barycenter: Some(1.0),
            weight: Some(2),
        },
    ];
    let result = sort_resolved(resolve_conflicts(&input, &cg));
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].vs, vec!["a"]);
    assert_eq!(result[0].barycenter, Some(2.0));
    assert_eq!(result[0].weight, Some(3));
    assert_eq!(result[1].vs, vec!["b"]);
    assert_eq!(result[1].barycenter, Some(1.0));
    assert_eq!(result[1].weight, Some(2));
}

#[test]
fn resolve_conflicts_returns_nodes_unchanged_no_conflicts() {
    let mut cg: Graph<(), ()> = Graph::new();
    cg.set_edge("b", "a", None, None);
    let input = vec![
        BarycenterEntry {
            v: "a".to_string(),
            barycenter: Some(2.0),
            weight: Some(3),
        },
        BarycenterEntry {
            v: "b".to_string(),
            barycenter: Some(1.0),
            weight: Some(2),
        },
    ];
    let result = sort_resolved(resolve_conflicts(&input, &cg));
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].vs, vec!["a"]);
    assert_eq!(result[0].barycenter, Some(2.0));
    assert_eq!(result[0].weight, Some(3));
    assert_eq!(result[1].vs, vec!["b"]);
    assert_eq!(result[1].barycenter, Some(1.0));
    assert_eq!(result[1].weight, Some(2));
}

#[test]
fn resolve_conflicts_coalesces_nodes_on_conflict() {
    let mut cg: Graph<(), ()> = Graph::new();
    cg.set_edge("a", "b", None, None);
    let input = vec![
        BarycenterEntry {
            v: "a".to_string(),
            barycenter: Some(2.0),
            weight: Some(3),
        },
        BarycenterEntry {
            v: "b".to_string(),
            barycenter: Some(1.0),
            weight: Some(2),
        },
    ];
    let result = resolve_conflicts(&input, &cg);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].vs, vec!["a", "b"]);
    let expected_bc = (3.0 * 2.0 + 2.0 * 1.0) / (3.0 + 2.0);
    assert!((result[0].barycenter.unwrap() - expected_bc).abs() < 1e-9);
    assert_eq!(result[0].weight, Some(5));
}

#[test]
fn resolve_conflicts_coalesces_nodes_on_conflict_2() {
    let mut cg: Graph<(), ()> = Graph::new();
    cg.set_path(&["a", "b", "c", "d"], None);
    let input = vec![
        BarycenterEntry {
            v: "a".to_string(),
            barycenter: Some(4.0),
            weight: Some(1),
        },
        BarycenterEntry {
            v: "b".to_string(),
            barycenter: Some(3.0),
            weight: Some(1),
        },
        BarycenterEntry {
            v: "c".to_string(),
            barycenter: Some(2.0),
            weight: Some(1),
        },
        BarycenterEntry {
            v: "d".to_string(),
            barycenter: Some(1.0),
            weight: Some(1),
        },
    ];
    let result = resolve_conflicts(&input, &cg);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].vs, vec!["a", "b", "c", "d"]);
    let expected_bc = (4.0 + 3.0 + 2.0 + 1.0) / 4.0;
    assert!((result[0].barycenter.unwrap() - expected_bc).abs() < 1e-9);
    assert_eq!(result[0].weight, Some(4));
}

#[test]
fn resolve_conflicts_multiple_constraints_same_target_1() {
    let mut cg: Graph<(), ()> = Graph::new();
    cg.set_edge("a", "c", None, None);
    cg.set_edge("b", "c", None, None);
    let input = vec![
        BarycenterEntry {
            v: "a".to_string(),
            barycenter: Some(4.0),
            weight: Some(1),
        },
        BarycenterEntry {
            v: "b".to_string(),
            barycenter: Some(3.0),
            weight: Some(1),
        },
        BarycenterEntry {
            v: "c".to_string(),
            barycenter: Some(2.0),
            weight: Some(1),
        },
    ];
    let result = resolve_conflicts(&input, &cg);
    assert_eq!(result.len(), 1);
    let r = &result[0];
    let c_idx = r.vs.iter().position(|v| v == "c").unwrap();
    let a_idx = r.vs.iter().position(|v| v == "a").unwrap();
    let b_idx = r.vs.iter().position(|v| v == "b").unwrap();
    assert!(c_idx > a_idx);
    assert!(c_idx > b_idx);
    let expected_bc = (4.0 + 3.0 + 2.0) / 3.0;
    assert!((r.barycenter.unwrap() - expected_bc).abs() < 1e-9);
    assert_eq!(r.weight, Some(3));
}

#[test]
fn resolve_conflicts_multiple_constraints_same_target_2() {
    let mut cg: Graph<(), ()> = Graph::new();
    cg.set_edge("a", "c", None, None);
    cg.set_edge("a", "d", None, None);
    cg.set_edge("b", "c", None, None);
    cg.set_edge("c", "d", None, None);
    let input = vec![
        BarycenterEntry {
            v: "a".to_string(),
            barycenter: Some(4.0),
            weight: Some(1),
        },
        BarycenterEntry {
            v: "b".to_string(),
            barycenter: Some(3.0),
            weight: Some(1),
        },
        BarycenterEntry {
            v: "c".to_string(),
            barycenter: Some(2.0),
            weight: Some(1),
        },
        BarycenterEntry {
            v: "d".to_string(),
            barycenter: Some(1.0),
            weight: Some(1),
        },
    ];
    let result = resolve_conflicts(&input, &cg);
    assert_eq!(result.len(), 1);
    let r = &result[0];
    let a_idx = r.vs.iter().position(|v| v == "a").unwrap();
    let b_idx = r.vs.iter().position(|v| v == "b").unwrap();
    let c_idx = r.vs.iter().position(|v| v == "c").unwrap();
    let d_idx = r.vs.iter().position(|v| v == "d").unwrap();
    assert!(c_idx > a_idx);
    assert!(c_idx > b_idx);
    assert!(d_idx > c_idx);
    let expected_bc = (4.0 + 3.0 + 2.0 + 1.0) / 4.0;
    assert!((r.barycenter.unwrap() - expected_bc).abs() < 1e-9);
    assert_eq!(r.weight, Some(4));
}

#[test]
fn resolve_conflicts_does_nothing_to_node_without_barycenter_or_constraint() {
    let cg: Graph<(), ()> = Graph::new();
    let input = vec![
        BarycenterEntry {
            v: "a".to_string(),
            barycenter: None,
            weight: None,
        },
        BarycenterEntry {
            v: "b".to_string(),
            barycenter: Some(1.0),
            weight: Some(2),
        },
    ];
    let result = sort_resolved(resolve_conflicts(&input, &cg));
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].vs, vec!["a"]);
    assert!(result[0].barycenter.is_none());
    assert!(result[0].weight.is_none());
    assert_eq!(result[1].vs, vec!["b"]);
    assert_eq!(result[1].barycenter, Some(1.0));
    assert_eq!(result[1].weight, Some(2));
}

#[test]
fn resolve_conflicts_treats_no_barycenter_as_always_violating_1() {
    let mut cg: Graph<(), ()> = Graph::new();
    cg.set_edge("a", "b", None, None);
    let input = vec![
        BarycenterEntry {
            v: "a".to_string(),
            barycenter: None,
            weight: None,
        },
        BarycenterEntry {
            v: "b".to_string(),
            barycenter: Some(1.0),
            weight: Some(2),
        },
    ];
    let result = resolve_conflicts(&input, &cg);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].vs, vec!["a", "b"]);
    assert_eq!(result[0].barycenter, Some(1.0));
    assert_eq!(result[0].weight, Some(2));
}

#[test]
fn resolve_conflicts_treats_no_barycenter_as_always_violating_2() {
    let mut cg: Graph<(), ()> = Graph::new();
    cg.set_edge("b", "a", None, None);
    let input = vec![
        BarycenterEntry {
            v: "a".to_string(),
            barycenter: None,
            weight: None,
        },
        BarycenterEntry {
            v: "b".to_string(),
            barycenter: Some(1.0),
            weight: Some(2),
        },
    ];
    let result = resolve_conflicts(&input, &cg);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].vs, vec!["b", "a"]);
    assert_eq!(result[0].barycenter, Some(1.0));
    assert_eq!(result[0].weight, Some(2));
}

#[test]
fn resolve_conflicts_ignores_edges_not_related_to_entries() {
    let mut cg: Graph<(), ()> = Graph::new();
    cg.set_edge("c", "d", None, None);
    let input = vec![
        BarycenterEntry {
            v: "a".to_string(),
            barycenter: Some(2.0),
            weight: Some(3),
        },
        BarycenterEntry {
            v: "b".to_string(),
            barycenter: Some(1.0),
            weight: Some(2),
        },
    ];
    let result = sort_resolved(resolve_conflicts(&input, &cg));
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].vs, vec!["a"]);
    assert_eq!(result[0].barycenter, Some(2.0));
    assert_eq!(result[0].weight, Some(3));
    assert_eq!(result[1].vs, vec!["b"]);
    assert_eq!(result[1].barycenter, Some(1.0));
    assert_eq!(result[1].weight, Some(2));
}

// ============================================================
// Ported from dagre-js: order/sort-test.ts
// ============================================================

use order::sort::sort as order_sort;

#[test]
fn sort_sorts_nodes_by_barycenter() {
    let input = vec![
        ResolvedEntry {
            vs: vec!["a".to_string()],
            i: 0,
            barycenter: Some(2.0),
            weight: Some(3),
        },
        ResolvedEntry {
            vs: vec!["b".to_string()],
            i: 1,
            barycenter: Some(1.0),
            weight: Some(2),
        },
    ];
    let result = order_sort(&input, false);
    assert_eq!(result.vs, vec!["b", "a"]);
    let expected_bc = (2.0 * 3.0 + 1.0 * 2.0) / (3.0 + 2.0);
    assert!((result.barycenter.unwrap() - expected_bc).abs() < 1e-9);
    assert_eq!(result.weight, Some(5));
}

#[test]
fn sort_can_sort_super_nodes() {
    let input = vec![
        ResolvedEntry {
            vs: vec!["a".to_string(), "c".to_string(), "d".to_string()],
            i: 0,
            barycenter: Some(2.0),
            weight: Some(3),
        },
        ResolvedEntry {
            vs: vec!["b".to_string()],
            i: 1,
            barycenter: Some(1.0),
            weight: Some(2),
        },
    ];
    let result = order_sort(&input, false);
    assert_eq!(result.vs, vec!["b", "a", "c", "d"]);
    let expected_bc = (2.0 * 3.0 + 1.0 * 2.0) / (3.0 + 2.0);
    assert!((result.barycenter.unwrap() - expected_bc).abs() < 1e-9);
    assert_eq!(result.weight, Some(5));
}

#[test]
fn sort_biases_left_by_default() {
    let input = vec![
        ResolvedEntry {
            vs: vec!["a".to_string()],
            i: 0,
            barycenter: Some(1.0),
            weight: Some(1),
        },
        ResolvedEntry {
            vs: vec!["b".to_string()],
            i: 1,
            barycenter: Some(1.0),
            weight: Some(1),
        },
    ];
    let result = order_sort(&input, false);
    assert_eq!(result.vs, vec!["a", "b"]);
    assert_eq!(result.barycenter, Some(1.0));
    assert_eq!(result.weight, Some(2));
}

#[test]
fn sort_biases_right_if_bias_right_true() {
    let input = vec![
        ResolvedEntry {
            vs: vec!["a".to_string()],
            i: 0,
            barycenter: Some(1.0),
            weight: Some(1),
        },
        ResolvedEntry {
            vs: vec!["b".to_string()],
            i: 1,
            barycenter: Some(1.0),
            weight: Some(1),
        },
    ];
    let result = order_sort(&input, true);
    assert_eq!(result.vs, vec!["b", "a"]);
    assert_eq!(result.barycenter, Some(1.0));
    assert_eq!(result.weight, Some(2));
}

#[test]
fn sort_can_sort_nodes_without_barycenter() {
    let input = vec![
        ResolvedEntry {
            vs: vec!["a".to_string()],
            i: 0,
            barycenter: Some(2.0),
            weight: Some(1),
        },
        ResolvedEntry {
            vs: vec!["b".to_string()],
            i: 1,
            barycenter: Some(6.0),
            weight: Some(1),
        },
        ResolvedEntry {
            vs: vec!["c".to_string()],
            i: 2,
            barycenter: None,
            weight: None,
        },
        ResolvedEntry {
            vs: vec!["d".to_string()],
            i: 3,
            barycenter: Some(3.0),
            weight: Some(1),
        },
    ];
    let result = order_sort(&input, false);
    assert_eq!(result.vs, vec!["a", "d", "c", "b"]);
    let expected_bc = (2.0 + 6.0 + 3.0) / 3.0;
    assert!((result.barycenter.unwrap() - expected_bc).abs() < 1e-9);
    assert_eq!(result.weight, Some(3));
}

#[test]
fn sort_handles_no_barycenters_for_any_nodes() {
    let input = vec![
        ResolvedEntry {
            vs: vec!["a".to_string()],
            i: 0,
            barycenter: None,
            weight: None,
        },
        ResolvedEntry {
            vs: vec!["b".to_string()],
            i: 3,
            barycenter: None,
            weight: None,
        },
        ResolvedEntry {
            vs: vec!["c".to_string()],
            i: 2,
            barycenter: None,
            weight: None,
        },
        ResolvedEntry {
            vs: vec!["d".to_string()],
            i: 1,
            barycenter: None,
            weight: None,
        },
    ];
    let result = order_sort(&input, false);
    assert_eq!(result.vs, vec!["a", "d", "c", "b"]);
    assert!(result.barycenter.is_none());
    assert!(result.weight.is_none());
}

#[test]
fn sort_handles_barycenter_of_0() {
    let input = vec![
        ResolvedEntry {
            vs: vec!["a".to_string()],
            i: 0,
            barycenter: Some(0.0),
            weight: Some(1),
        },
        ResolvedEntry {
            vs: vec!["b".to_string()],
            i: 3,
            barycenter: None,
            weight: None,
        },
        ResolvedEntry {
            vs: vec!["c".to_string()],
            i: 2,
            barycenter: None,
            weight: None,
        },
        ResolvedEntry {
            vs: vec!["d".to_string()],
            i: 1,
            barycenter: None,
            weight: None,
        },
    ];
    let result = order_sort(&input, false);
    assert_eq!(result.vs, vec!["a", "d", "c", "b"]);
    assert_eq!(result.barycenter, Some(0.0));
    assert_eq!(result.weight, Some(1));
}

// ============================================================
// Ported from dagre-js: order/build-layer-graph-test.ts
// ============================================================

use order::build_layer_graph::{Relationship, build_layer_graph, get_root};

#[test]
fn build_layer_graph_places_movable_nodes_under_root() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });
    let a = NodeLabel {
        rank: Some(1),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        rank: Some(1),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    let c = NodeLabel {
        rank: Some(2),

        ..Default::default()
    };
    g.set_node("c".to_string(), Some(c));
    let d = NodeLabel {
        rank: Some(3),

        ..Default::default()
    };
    g.set_node("d".to_string(), Some(d));

    let nodes: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
    let lg = build_layer_graph(&g, 1, Relationship::InEdges, &nodes);
    let root = get_root(&lg);
    assert!(lg.has_node(&root));
    let children = lg.children(Some(&root));
    assert!(children.contains(&"a".to_string()));
    assert!(children.contains(&"b".to_string()));
}

#[test]
fn build_layer_graph_copies_flat_nodes_from_layer() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });
    let a = NodeLabel {
        rank: Some(1),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        rank: Some(1),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    let c = NodeLabel {
        rank: Some(2),

        ..Default::default()
    };
    g.set_node("c".to_string(), Some(c));
    let d = NodeLabel {
        rank: Some(3),

        ..Default::default()
    };
    g.set_node("d".to_string(), Some(d));

    let nodes1: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
    let lg1 = build_layer_graph(&g, 1, Relationship::InEdges, &nodes1);
    assert!(lg1.has_node("a"));
    assert!(lg1.has_node("b"));

    let nodes2: Vec<String> = ["c"].iter().map(|s| s.to_string()).collect();
    let lg2 = build_layer_graph(&g, 2, Relationship::InEdges, &nodes2);
    assert!(lg2.has_node("c"));

    let nodes3: Vec<String> = ["d"].iter().map(|s| s.to_string()).collect();
    let lg3 = build_layer_graph(&g, 3, Relationship::InEdges, &nodes3);
    assert!(lg3.has_node("d"));
}

#[test]
fn build_layer_graph_copies_in_edges_incident_on_rank_nodes() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });
    let a = NodeLabel {
        rank: Some(1),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        rank: Some(1),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    let c = NodeLabel {
        rank: Some(2),

        ..Default::default()
    };
    g.set_node("c".to_string(), Some(c));
    let d = NodeLabel {
        rank: Some(3),

        ..Default::default()
    };
    g.set_node("d".to_string(), Some(d));

    let el_ac = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("a", "c", Some(el_ac), None);
    let el_bc = EdgeLabel {
        weight: 3,

        ..Default::default()
    };
    g.set_edge("b", "c", Some(el_bc), None);
    let el_cd = EdgeLabel {
        weight: 4,

        ..Default::default()
    };
    g.set_edge("c", "d", Some(el_cd), None);

    let nodes1: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
    let lg1 = build_layer_graph(&g, 1, Relationship::InEdges, &nodes1);
    assert_eq!(lg1.edge_count(), 0);

    let nodes2: Vec<String> = ["c"].iter().map(|s| s.to_string()).collect();
    let lg2 = build_layer_graph(&g, 2, Relationship::InEdges, &nodes2);
    assert_eq!(lg2.edge_count(), 2);
    assert_eq!(lg2.edge("a", "c", None).unwrap().weight, 2);
    assert_eq!(lg2.edge("b", "c", None).unwrap().weight, 3);

    let nodes3: Vec<String> = ["d"].iter().map(|s| s.to_string()).collect();
    let lg3 = build_layer_graph(&g, 3, Relationship::InEdges, &nodes3);
    assert_eq!(lg3.edge_count(), 1);
    assert_eq!(lg3.edge("c", "d", None).unwrap().weight, 4);
}

#[test]
fn build_layer_graph_copies_out_edges_incident_on_rank_nodes() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });
    let a = NodeLabel {
        rank: Some(1),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        rank: Some(1),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    let c = NodeLabel {
        rank: Some(2),

        ..Default::default()
    };
    g.set_node("c".to_string(), Some(c));
    let d = NodeLabel {
        rank: Some(3),

        ..Default::default()
    };
    g.set_node("d".to_string(), Some(d));

    let el_ac = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("a", "c", Some(el_ac), None);
    let el_bc = EdgeLabel {
        weight: 3,

        ..Default::default()
    };
    g.set_edge("b", "c", Some(el_bc), None);
    let el_cd = EdgeLabel {
        weight: 4,

        ..Default::default()
    };
    g.set_edge("c", "d", Some(el_cd), None);

    let nodes1: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
    let lg1 = build_layer_graph(&g, 1, Relationship::OutEdges, &nodes1);
    assert_eq!(lg1.edge_count(), 2);
    assert_eq!(lg1.edge("c", "a", None).unwrap().weight, 2);
    assert_eq!(lg1.edge("c", "b", None).unwrap().weight, 3);

    let nodes2: Vec<String> = ["c"].iter().map(|s| s.to_string()).collect();
    let lg2 = build_layer_graph(&g, 2, Relationship::OutEdges, &nodes2);
    assert_eq!(lg2.edge_count(), 1);
    assert_eq!(lg2.edge("d", "c", None).unwrap().weight, 4);

    let nodes3: Vec<String> = ["d"].iter().map(|s| s.to_string()).collect();
    let lg3 = build_layer_graph(&g, 3, Relationship::OutEdges, &nodes3);
    assert_eq!(lg3.edge_count(), 0);
}

#[test]
fn build_layer_graph_collapses_multi_edges() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });
    let a = NodeLabel {
        rank: Some(1),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        rank: Some(2),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));

    let el1 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("a", "b", Some(el1), None);
    let el2 = EdgeLabel {
        weight: 3,

        ..Default::default()
    };
    g.set_edge("a", "b", Some(el2), Some("multi"));

    let nodes: Vec<String> = vec!["b".to_string()];
    let lg = build_layer_graph(&g, 2, Relationship::InEdges, &nodes);
    assert_eq!(lg.edge("a", "b", None).unwrap().weight, 5);
}

#[test]
fn build_layer_graph_preserves_hierarchy_for_movable_layer() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });
    let a = NodeLabel {
        rank: Some(0),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        rank: Some(0),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    let c = NodeLabel {
        rank: Some(0),

        ..Default::default()
    };
    g.set_node("c".to_string(), Some(c));
    let sg = NodeLabel {
        min_rank: Some(0),

        max_rank: Some(0),

        border_left: vec!["bl".to_string()],

        border_right: vec!["br".to_string()],

        ..Default::default()
    };
    g.set_node("sg".to_string(), Some(sg));
    g.set_parent("a", Some("sg"));
    g.set_parent("b", Some("sg"));

    let nodes: Vec<String> = ["a", "b", "c", "sg"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let lg = build_layer_graph(&g, 0, Relationship::InEdges, &nodes);
    let root = get_root(&lg);
    let root_children = lg.children(Some(&root));
    assert!(root_children.contains(&"c".to_string()));
    assert!(root_children.contains(&"sg".to_string()));
    assert_eq!(lg.parent("a"), Some("sg"));
    assert_eq!(lg.parent("b"), Some("sg"));
}

// ============================================================
// Ported from dagre-js: order/barycenter-test.ts
// ============================================================

use order::barycenter::barycenter;

#[test]
fn barycenter_assigns_undefined_for_node_with_no_predecessors() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    g.set_node("x".to_string(), Some(NodeLabel::default()));

    let results = barycenter(&g, &["x".to_string()]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].v, "x");
    assert!(results[0].barycenter.is_none());
    assert!(results[0].weight.is_none());
}

#[test]
fn barycenter_assigns_position_of_sole_predecessor() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let a = NodeLabel {
        order: Some(2),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_edge("a", "x", Some(EdgeLabel::default()), None);

    let results = barycenter(&g, &["x".to_string()]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].v, "x");
    assert_eq!(results[0].barycenter, Some(2.0));
    assert_eq!(results[0].weight, Some(1));
}

#[test]
fn barycenter_assigns_average_of_multiple_predecessors() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let a = NodeLabel {
        order: Some(2),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        order: Some(4),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_edge("a", "x", Some(EdgeLabel::default()), None);
    g.set_edge("b", "x", Some(EdgeLabel::default()), None);

    let results = barycenter(&g, &["x".to_string()]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].v, "x");
    assert_eq!(results[0].barycenter, Some(3.0));
    assert_eq!(results[0].weight, Some(2));
}

#[test]
fn barycenter_takes_into_account_edge_weight() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let a = NodeLabel {
        order: Some(2),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        order: Some(4),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    let el1 = EdgeLabel {
        weight: 3,

        ..Default::default()
    };
    g.set_edge("a", "x", Some(el1), None);
    g.set_edge("b", "x", Some(EdgeLabel::default()), None);

    let results = barycenter(&g, &["x".to_string()]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].v, "x");
    assert_eq!(results[0].barycenter, Some(2.5));
    assert_eq!(results[0].weight, Some(4));
}

#[test]
fn barycenter_calculates_for_all_nodes_in_movable_layer() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let a = NodeLabel {
        order: Some(1),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        order: Some(2),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    let c = NodeLabel {
        order: Some(4),

        ..Default::default()
    };
    g.set_node("c".to_string(), Some(c));
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_node("y".to_string(), Some(NodeLabel::default()));
    g.set_node("z".to_string(), Some(NodeLabel::default()));
    g.set_edge("a", "x", Some(EdgeLabel::default()), None);
    g.set_edge("b", "x", Some(EdgeLabel::default()), None);
    let el_az = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("a", "z", Some(el_az), None);
    g.set_edge("c", "z", Some(EdgeLabel::default()), None);

    let results = barycenter(&g, &["x".to_string(), "y".to_string(), "z".to_string()]);
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].v, "x");
    assert_eq!(results[0].barycenter, Some(1.5));
    assert_eq!(results[0].weight, Some(2));
    assert_eq!(results[1].v, "y");
    assert!(results[1].barycenter.is_none());
    assert!(results[1].weight.is_none());
    assert_eq!(results[2].v, "z");
    assert_eq!(results[2].barycenter, Some(2.0));
    assert_eq!(results[2].weight, Some(3));
}

// ============================================================
// Ported from dagre-js: order/add-subgraph-constraints-test.ts
// ============================================================

use order::add_subgraph_constraints::add_subgraph_constraints;

#[test]
fn add_subgraph_constraints_flat_nodes_no_change() {
    let mut graph = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    let mut cg: Graph<(), ()> = Graph::new();
    let vs: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
    for v in &vs {
        graph.set_node(v.clone(), Some(NodeLabel::default()));
    }
    add_subgraph_constraints(&graph, &mut cg, &vs);
    assert_eq!(cg.node_count(), 0);
    assert_eq!(cg.edge_count(), 0);
}

#[test]
fn add_subgraph_constraints_contiguous_subgraph_no_constraint() {
    let mut graph = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    let mut cg: Graph<(), ()> = Graph::new();
    let vs: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    for v in &vs {
        graph.set_parent(v, Some("sg"));
    }
    add_subgraph_constraints(&graph, &mut cg, &vs);
    assert_eq!(cg.node_count(), 0);
    assert_eq!(cg.edge_count(), 0);
}

#[test]
fn add_subgraph_constraints_different_parents_adds_constraint() {
    let mut graph = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    let mut cg: Graph<(), ()> = Graph::new();
    let vs: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
    graph.set_parent("a", Some("sg1"));
    graph.set_parent("b", Some("sg2"));
    add_subgraph_constraints(&graph, &mut cg, &vs);
    let edges = cg.edges();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].v, "sg1");
    assert_eq!(edges[0].w, "sg2");
}

#[test]
fn add_subgraph_constraints_works_for_multiple_levels() {
    let mut graph = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    let mut cg: Graph<(), ()> = Graph::new();
    let vs: Vec<String> = ["a", "b", "c", "d", "e", "f", "g", "h"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for v in &vs {
        graph.set_node(v.clone(), Some(NodeLabel::default()));
    }
    graph.set_parent("b", Some("sg2"));
    graph.set_parent("sg2", Some("sg1"));
    graph.set_parent("c", Some("sg1"));
    graph.set_parent("d", Some("sg3"));
    graph.set_parent("sg3", Some("sg1"));
    graph.set_parent("f", Some("sg4"));
    graph.set_parent("g", Some("sg5"));
    graph.set_parent("sg5", Some("sg4"));
    add_subgraph_constraints(&graph, &mut cg, &vs);
    let mut edges = cg.edges();
    edges.sort_by(|a, b| a.v.cmp(&b.v));
    assert_eq!(edges.len(), 2);
    assert_eq!(edges[0].v, "sg1");
    assert_eq!(edges[0].w, "sg4");
    assert_eq!(edges[1].v, "sg2");
    assert_eq!(edges[1].w, "sg3");
}

// ============================================================
// Ported from dagre-js: order/sort-subgraph-test.ts
// ============================================================

use order::sort_subgraph::sort_subgraph;

fn make_sort_subgraph_graph() -> Graph<NodeLabel, EdgeLabel> {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    for (v, ord) in &[("0", 0), ("1", 1), ("2", 2), ("3", 3), ("4", 4)] {
        let label = NodeLabel {
            order: Some(*ord),

            ..Default::default()
        };
        g.set_node(v.to_string(), Some(label));
    }
    g
}

#[test]
fn sort_subgraph_sorts_flat_by_barycenter() {
    let mut g = make_sort_subgraph_graph();
    let cg: Graph<(), ()> = Graph::new();
    let el = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("3", "x", Some(el), None);
    let el2 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("1", "y", Some(el2), None);
    let el3 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("4", "y", Some(el3), None);
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_node("y".to_string(), Some(NodeLabel::default()));
    g.set_parent("x", Some("movable"));
    g.set_parent("y", Some("movable"));

    let result = sort_subgraph(&g, "movable", &cg, false);
    assert_eq!(result.vs, vec!["y", "x"]);
}

#[test]
fn sort_subgraph_preserves_pos_of_node_without_neighbors() {
    let mut g = make_sort_subgraph_graph();
    let cg: Graph<(), ()> = Graph::new();
    let el = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("3", "x", Some(el), None);
    g.set_node("y".to_string(), Some(NodeLabel::default()));
    let el2 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("1", "z", Some(el2), None);
    let el3 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("4", "z", Some(el3), None);
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_node("z".to_string(), Some(NodeLabel::default()));
    g.set_parent("x", Some("movable"));
    g.set_parent("y", Some("movable"));
    g.set_parent("z", Some("movable"));

    let result = sort_subgraph(&g, "movable", &cg, false);
    assert_eq!(result.vs, vec!["z", "y", "x"]);
}

#[test]
fn sort_subgraph_biases_left_without_reverse_bias() {
    let mut g = make_sort_subgraph_graph();
    let cg: Graph<(), ()> = Graph::new();
    let el1 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("1", "x", Some(el1), None);
    let el2 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("1", "y", Some(el2), None);
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_node("y".to_string(), Some(NodeLabel::default()));
    g.set_parent("x", Some("movable"));
    g.set_parent("y", Some("movable"));

    let result = sort_subgraph(&g, "movable", &cg, false);
    assert_eq!(result.vs, vec!["x", "y"]);
}

#[test]
fn sort_subgraph_biases_right_with_reverse_bias() {
    let mut g = make_sort_subgraph_graph();
    let cg: Graph<(), ()> = Graph::new();
    let el1 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("1", "x", Some(el1), None);
    let el2 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("1", "y", Some(el2), None);
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_node("y".to_string(), Some(NodeLabel::default()));
    g.set_parent("x", Some("movable"));
    g.set_parent("y", Some("movable"));

    let result = sort_subgraph(&g, "movable", &cg, true);
    assert_eq!(result.vs, vec!["y", "x"]);
}

#[test]
fn sort_subgraph_aggregates_stats() {
    let mut g = make_sort_subgraph_graph();
    let cg: Graph<(), ()> = Graph::new();
    let el1 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("3", "x", Some(el1), None);
    let el2 = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("1", "y", Some(el2), None);
    let el3 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("4", "y", Some(el3), None);
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_node("y".to_string(), Some(NodeLabel::default()));
    g.set_parent("x", Some("movable"));
    g.set_parent("y", Some("movable"));

    let result = sort_subgraph(&g, "movable", &cg, false);
    assert_eq!(result.barycenter, Some(2.25));
    assert_eq!(result.weight, Some(4));
}

#[test]
fn sort_subgraph_can_sort_nested_subgraph_no_barycenter() {
    let mut g = make_sort_subgraph_graph();
    let cg: Graph<(), ()> = Graph::new();
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_node("b".to_string(), Some(NodeLabel::default()));
    g.set_node("c".to_string(), Some(NodeLabel::default()));
    g.set_parent("a", Some("y"));
    g.set_parent("b", Some("y"));
    g.set_parent("c", Some("y"));
    let el1 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("0", "x", Some(el1), None);
    let el2 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("1", "z", Some(el2), None);
    let el3 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("2", "y", Some(el3), None);
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_node("y".to_string(), Some(NodeLabel::default()));
    g.set_node("z".to_string(), Some(NodeLabel::default()));
    g.set_parent("x", Some("movable"));
    g.set_parent("y", Some("movable"));
    g.set_parent("z", Some("movable"));

    let result = sort_subgraph(&g, "movable", &cg, false);
    assert_eq!(result.vs, vec!["x", "z", "a", "b", "c"]);
}

#[test]
fn sort_subgraph_can_sort_nested_subgraph_with_barycenter() {
    let mut g = make_sort_subgraph_graph();
    let cg: Graph<(), ()> = Graph::new();
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_node("b".to_string(), Some(NodeLabel::default()));
    g.set_node("c".to_string(), Some(NodeLabel::default()));
    g.set_parent("a", Some("y"));
    g.set_parent("b", Some("y"));
    g.set_parent("c", Some("y"));
    let el_0a = EdgeLabel {
        weight: 3,

        ..Default::default()
    };
    g.set_edge("0", "a", Some(el_0a), None);
    let el_0x = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("0", "x", Some(el_0x), None);
    let el_1z = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("1", "z", Some(el_1z), None);
    let el_2y = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("2", "y", Some(el_2y), None);
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_node("y".to_string(), Some(NodeLabel::default()));
    g.set_node("z".to_string(), Some(NodeLabel::default()));
    g.set_parent("x", Some("movable"));
    g.set_parent("y", Some("movable"));
    g.set_parent("z", Some("movable"));

    let result = sort_subgraph(&g, "movable", &cg, false);
    assert_eq!(result.vs, vec!["x", "a", "b", "c", "z"]);
}

#[test]
fn sort_subgraph_can_sort_nested_subgraph_no_in_edges() {
    let mut g = make_sort_subgraph_graph();
    let cg: Graph<(), ()> = Graph::new();
    g.set_node("a".to_string(), Some(NodeLabel::default()));
    g.set_node("b".to_string(), Some(NodeLabel::default()));
    g.set_node("c".to_string(), Some(NodeLabel::default()));
    g.set_parent("a", Some("y"));
    g.set_parent("b", Some("y"));
    g.set_parent("c", Some("y"));
    let el1 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("0", "a", Some(el1), None);
    let el2 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("1", "b", Some(el2), None);
    let el3 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("0", "x", Some(el3), None);
    let el4 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("1", "z", Some(el4), None);
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_node("y".to_string(), Some(NodeLabel::default()));
    g.set_node("z".to_string(), Some(NodeLabel::default()));
    g.set_parent("x", Some("movable"));
    g.set_parent("y", Some("movable"));
    g.set_parent("z", Some("movable"));

    let result = sort_subgraph(&g, "movable", &cg, false);
    assert_eq!(result.vs, vec!["x", "a", "b", "c", "z"]);
}

#[test]
fn sort_subgraph_sorts_border_nodes_to_extremes() {
    let mut g = make_sort_subgraph_graph();
    let cg: Graph<(), ()> = Graph::new();
    let el1 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("0", "x", Some(el1), None);
    let el2 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("1", "y", Some(el2), None);
    let el3 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("2", "z", Some(el3), None);
    let sg_label = NodeLabel {
        border_left: vec!["bl".to_string()],

        border_right: vec!["br".to_string()],

        ..Default::default()
    };
    g.set_node("sg1".to_string(), Some(sg_label));
    g.set_node("x".to_string(), Some(NodeLabel::default()));
    g.set_node("y".to_string(), Some(NodeLabel::default()));
    g.set_node("z".to_string(), Some(NodeLabel::default()));
    g.set_node("bl".to_string(), Some(NodeLabel::default()));
    g.set_node("br".to_string(), Some(NodeLabel::default()));
    for v in &["x", "y", "z", "bl", "br"] {
        g.set_parent(v, Some("sg1"));
    }

    let result = sort_subgraph(&g, "sg1", &cg, false);
    assert_eq!(result.vs, vec!["bl", "x", "y", "z", "br"]);
}

#[test]
fn sort_subgraph_assigns_barycenter_based_on_previous_border_nodes() {
    let mut g = make_sort_subgraph_graph();
    let cg: Graph<(), ()> = Graph::new();
    let bl1 = NodeLabel {
        order: Some(0),

        ..Default::default()
    };
    g.set_node("bl1".to_string(), Some(bl1));
    let br1 = NodeLabel {
        order: Some(1),

        ..Default::default()
    };
    g.set_node("br1".to_string(), Some(br1));
    let el1 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("bl1", "bl2", Some(el1), None);
    let el2 = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("br1", "br2", Some(el2), None);
    g.set_node("bl2".to_string(), Some(NodeLabel::default()));
    g.set_node("br2".to_string(), Some(NodeLabel::default()));
    g.set_parent("bl2", Some("sg"));
    g.set_parent("br2", Some("sg"));
    let sg_label = NodeLabel {
        border_left: vec!["bl2".to_string()],

        border_right: vec!["br2".to_string()],

        ..Default::default()
    };
    g.set_node("sg".to_string(), Some(sg_label));

    let result = sort_subgraph(&g, "sg", &cg, false);
    assert_eq!(result.barycenter, Some(0.5));
    assert_eq!(result.weight, Some(2));
    assert_eq!(result.vs, vec!["bl2", "br2"]);
}

// ============================================================
// Ported from dagre-js: add-border-segments-test.ts
// ============================================================

#[test]
fn add_border_segments_does_not_add_for_non_compound_graph() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let a = NodeLabel {
        rank: Some(0),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    add_border_segments::add_border_segments(&mut g);
    assert_eq!(g.node_count(), 1);
    assert_eq!(g.node("a").unwrap().rank, Some(0));
}

#[test]
fn add_border_segments_does_not_add_for_graph_with_no_clusters() {
    let mut g = make_compound_graph();
    let a = NodeLabel {
        rank: Some(0),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    add_border_segments::add_border_segments(&mut g);
    assert_eq!(g.node_count(), 1);
    assert_eq!(g.node("a").unwrap().rank, Some(0));
}

#[test]
fn add_border_segments_adds_border_for_single_rank_subgraph() {
    let mut g = make_compound_graph();
    let sg = NodeLabel {
        min_rank: Some(1),

        max_rank: Some(1),

        ..Default::default()
    };
    g.set_node("sg".to_string(), Some(sg));
    // Need a child so that add_border_segments recognizes sg as subgraph
    g.set_node("child".to_string(), Some(NodeLabel::default()));
    g.set_parent("child", Some("sg"));
    add_border_segments::add_border_segments(&mut g);

    let sg_node = g.node("sg").unwrap();
    // border_left/border_right are sparse vectors indexed by rank — the
    // single border for rank=1 lives at index 1 (with index 0 left as an
    // empty placeholder).
    assert_eq!(sg_node.border_left.len(), 2);
    assert_eq!(sg_node.border_right.len(), 2);
    assert!(sg_node.border_left[0].is_empty());
    assert!(sg_node.border_right[0].is_empty());

    let bl = &sg_node.border_left[1];
    let br = &sg_node.border_right[1];
    let bl_node = g.node(bl).unwrap();
    assert_eq!(bl_node.dummy.as_deref(), Some("border"));
    assert_eq!(bl_node.border_type, Some(BorderType::Left));
    assert_eq!(bl_node.rank, Some(1));
    assert_eq!(bl_node.width, 0.0);
    assert_eq!(bl_node.height, 0.0);
    assert_eq!(g.parent(bl), Some("sg"));

    let br_node = g.node(br).unwrap();
    assert_eq!(br_node.dummy.as_deref(), Some("border"));
    assert_eq!(br_node.border_type, Some(BorderType::Right));
    assert_eq!(br_node.rank, Some(1));
    assert_eq!(br_node.width, 0.0);
    assert_eq!(br_node.height, 0.0);
    assert_eq!(g.parent(br), Some("sg"));
}

#[test]
fn add_border_segments_adds_border_for_multi_rank_subgraph() {
    let mut g = make_compound_graph();
    let sg = NodeLabel {
        min_rank: Some(1),

        max_rank: Some(2),

        ..Default::default()
    };
    g.set_node("sg".to_string(), Some(sg));
    g.set_node("child".to_string(), Some(NodeLabel::default()));
    g.set_parent("child", Some("sg"));
    add_border_segments::add_border_segments(&mut g);

    let sg_node = g.node("sg").unwrap();
    // Sparse storage: borders for ranks 1 and 2 live at indices 1 and 2,
    // index 0 is an empty placeholder.
    assert_eq!(sg_node.border_left.len(), 3);
    assert_eq!(sg_node.border_right.len(), 3);
    assert!(sg_node.border_left[0].is_empty());
    assert!(sg_node.border_right[0].is_empty());

    // First border (rank 1)
    let bl1 = &sg_node.border_left[1];
    let br1 = &sg_node.border_right[1];
    assert_eq!(g.node(bl1).unwrap().rank, Some(1));
    assert_eq!(g.node(br1).unwrap().rank, Some(1));

    // Second border (rank 2)
    let bl2 = &sg_node.border_left[2];
    let br2 = &sg_node.border_right[2];
    assert_eq!(g.node(bl2).unwrap().rank, Some(2));
    assert_eq!(g.node(br2).unwrap().rank, Some(2));

    // Edges between consecutive border nodes
    assert!(g.has_edge(bl1, bl2, None));
    assert!(g.has_edge(br1, br2, None));
}

#[test]
fn add_border_segments_adds_borders_for_nested_subgraphs() {
    let mut g = make_compound_graph();
    let sg1 = NodeLabel {
        min_rank: Some(1),

        max_rank: Some(1),

        ..Default::default()
    };
    g.set_node("sg1".to_string(), Some(sg1));
    let sg2 = NodeLabel {
        min_rank: Some(1),

        max_rank: Some(1),

        ..Default::default()
    };
    g.set_node("sg2".to_string(), Some(sg2));
    g.set_parent("sg2", Some("sg1"));
    // Add children so they're recognized as subgraphs
    g.set_node("child1".to_string(), Some(NodeLabel::default()));
    g.set_parent("child1", Some("sg1"));
    g.set_node("child2".to_string(), Some(NodeLabel::default()));
    g.set_parent("child2", Some("sg2"));
    add_border_segments::add_border_segments(&mut g);

    // Both subgraphs have min_rank=max_rank=1 → border lives at index 1.
    let sg1_node = g.node("sg1").unwrap();
    let bl1 = &sg1_node.border_left[1];
    let br1 = &sg1_node.border_right[1];
    assert_eq!(g.node(bl1).unwrap().dummy.as_deref(), Some("border"));
    assert_eq!(g.node(bl1).unwrap().border_type, Some(BorderType::Left));
    assert_eq!(g.parent(bl1), Some("sg1"));
    assert_eq!(g.node(br1).unwrap().dummy.as_deref(), Some("border"));
    assert_eq!(g.node(br1).unwrap().border_type, Some(BorderType::Right));
    assert_eq!(g.parent(br1), Some("sg1"));

    let sg2_node = g.node("sg2").unwrap();
    let bl2 = &sg2_node.border_left[1];
    let br2 = &sg2_node.border_right[1];
    assert_eq!(g.node(bl2).unwrap().dummy.as_deref(), Some("border"));
    assert_eq!(g.node(bl2).unwrap().border_type, Some(BorderType::Left));
    assert_eq!(g.parent(bl2), Some("sg2"));
    assert_eq!(g.node(br2).unwrap().dummy.as_deref(), Some("border"));
    assert_eq!(g.node(br2).unwrap().border_type, Some(BorderType::Right));
    assert_eq!(g.parent(br2), Some("sg2"));
}

// ============================================================
// Ported from dagre-js: acyclic-test.ts
// (Additional tests beyond what's already ported)
// ============================================================

#[test]
fn acyclic_dfs_does_not_change_acyclic_diamond() {
    let mut g = make_graph();
    g.set_path(&["a", "b", "d"], Some(EdgeLabel::default()));
    g.set_path(&["a", "c", "d"], Some(EdgeLabel::default()));
    acyclic::run(&mut g, None);
    let mut edges: Vec<(String, String)> = g
        .edges()
        .iter()
        .map(|e| (e.v.clone(), e.w.clone()))
        .collect();
    edges.sort();
    assert_eq!(
        edges,
        vec![
            ("a".to_string(), "b".to_string()),
            ("a".to_string(), "c".to_string()),
            ("b".to_string(), "d".to_string()),
            ("c".to_string(), "d".to_string()),
        ]
    );
}

#[test]
fn acyclic_dfs_creates_multi_edge_where_necessary() {
    let mut g = make_graph();
    g.set_path(&["a", "b", "a"], Some(EdgeLabel::default()));
    acyclic::run(&mut g, None);
    assert!(crate::dagre::graph::alg::find_cycles(&g).is_empty());
    if g.has_edge("a", "b", None) {
        let out = g.out_edges("a", Some("b")).unwrap_or_default();
        assert_eq!(out.len(), 2);
    } else {
        let out = g.out_edges("b", Some("a")).unwrap_or_default();
        assert_eq!(out.len(), 2);
    }
    assert_eq!(g.edge_count(), 2);
}

#[test]
fn acyclic_greedy_prefers_to_break_at_low_weight_edges() {
    let mut g = make_graph();
    let el_default = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_path(&["a", "b", "c", "d", "a"], Some(el_default));
    // Override c->d with weight 1
    let el_low = EdgeLabel {
        weight: 1,

        ..Default::default()
    };
    g.set_edge("c", "d", Some(el_low), None);
    acyclic::run(&mut g, Some(Acyclicer::Greedy));
    assert!(crate::dagre::graph::alg::find_cycles(&g).is_empty());
    assert!(!g.has_edge("c", "d", None));
}

// ============================================================
// Ported from dagre-js: util-test.ts
// (Additional tests beyond what's already ported)
// ============================================================

#[test]
fn util_normalize_ranks_adjust_ranks_to_zero() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let a = NodeLabel {
        rank: Some(3),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        rank: Some(2),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    let c = NodeLabel {
        rank: Some(4),

        ..Default::default()
    };
    g.set_node("c".to_string(), Some(c));

    util::normalize_ranks(&mut g);
    assert_eq!(g.node("a").unwrap().rank, Some(1));
    assert_eq!(g.node("b").unwrap().rank, Some(0));
    assert_eq!(g.node("c").unwrap().rank, Some(2));
}

#[test]
fn util_normalize_ranks_does_not_assign_rank_to_subgraphs() {
    // In our Rust port, subgraph nodes (with children) that have no rank
    // should remain without rank.
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    let a = NodeLabel {
        rank: Some(0),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    g.set_node("sg".to_string(), Some(NodeLabel::default()));
    g.set_parent("a", Some("sg"));

    util::normalize_ranks(&mut g);
    assert!(g.node("sg").unwrap().rank.is_none());
    assert_eq!(g.node("a").unwrap().rank, Some(0));
}

#[test]
fn util_remove_empty_ranks_removes_border_ranks() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let gl = GraphLabel {
        node_rank_factor: Some(4.0),

        ..Default::default()
    };
    g.set_graph_label(gl);
    let a = NodeLabel {
        rank: Some(0),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        rank: Some(4),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));

    util::remove_empty_ranks(&mut g);
    assert_eq!(g.node("a").unwrap().rank, Some(0));
    assert_eq!(g.node("b").unwrap().rank, Some(1));
}

#[test]
fn util_remove_empty_ranks_does_not_remove_non_border_ranks() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    let gl = GraphLabel {
        node_rank_factor: Some(4.0),

        ..Default::default()
    };
    g.set_graph_label(gl);
    let a = NodeLabel {
        rank: Some(0),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        rank: Some(8),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));

    util::remove_empty_ranks(&mut g);
    assert_eq!(g.node("a").unwrap().rank, Some(0));
    assert_eq!(g.node("b").unwrap().rank, Some(2));
}

#[test]
fn util_remove_empty_ranks_handles_parents_with_undefined_ranks() {
    let mut g = Graph::with_options(GraphOptions {
        directed: true,
        multigraph: false,
        compound: true,
    });
    let gl = GraphLabel {
        node_rank_factor: Some(3.0),

        ..Default::default()
    };
    g.set_graph_label(gl);
    let a = NodeLabel {
        rank: Some(0),

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        rank: Some(6),

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    g.set_node("sg".to_string(), Some(NodeLabel::default()));
    g.set_parent("a", Some("sg"));

    util::remove_empty_ranks(&mut g);
    assert_eq!(g.node("a").unwrap().rank, Some(0));
    assert_eq!(g.node("b").unwrap().rank, Some(2));
    assert!(g.node("sg").unwrap().rank.is_none());
}

#[test]
fn util_intersect_rect_creates_slope_intersecting_center() {
    let rect = NodeLabel {
        x: Some(0.0),

        y: Some(0.0),

        width: 1.0,

        height: 1.0,

        ..Default::default()
    };
    for &(px, py) in &[
        (2.0, 6.0),
        (2.0, -6.0),
        (6.0, 2.0),
        (-6.0, 2.0),
        (5.0, 0.0),
        (0.0, 5.0),
    ] {
        let point = Point::new(px, py);
        let cross = util::intersect_rect(&rect, &point);
        if cross.x != px {
            let m = (cross.y - py) / (cross.x - px);
            let expected = m * (cross.x - 0.0);
            assert!(
                (cross.y - 0.0 - expected).abs() < 0.01,
                "slope check failed for ({}, {})",
                px,
                py
            );
        }
    }
}

#[test]
fn util_build_layer_matrix_creates_correct_matrix() {
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::new();
    for (v, rank, order) in &[
        ("a", 0, 0),
        ("b", 0, 1),
        ("c", 1, 0),
        ("d", 1, 1),
        ("e", 2, 0),
    ] {
        let label = NodeLabel {
            rank: Some(*rank),

            order: Some(*order),

            ..Default::default()
        };
        g.set_node(v.to_string(), Some(label));
    }
    let layers = util::build_layer_matrix(&g);
    assert_eq!(layers, vec![vec!["a", "b"], vec!["c", "d"], vec!["e"],]);
}

// ============================================================
// Ported from dagre-js: layout-test.ts
// (Additional tests beyond what's already ported)
// ============================================================

#[test]
fn layout_can_layout_single_node_exact() {
    let mut g = make_graph();
    let a = NodeLabel {
        width: 50.0,

        height: 100.0,

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    layout(&mut g, None);
    assert_eq!(g.node("a").unwrap().x, Some(25.0));
    assert_eq!(g.node("a").unwrap().y, Some(50.0));
}

#[test]
fn layout_can_layout_two_nodes_same_rank() {
    let mut g = make_graph();
    let a = NodeLabel {
        width: 50.0,

        height: 100.0,

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        width: 75.0,

        height: 200.0,

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    let opts = LayoutOptions {
        nodesep: 200.0,

        ..Default::default()
    };
    layout(&mut g, Some(opts));

    let ax = g.node("a").unwrap().x.unwrap();
    let ay = g.node("a").unwrap().y.unwrap();
    let bx = g.node("b").unwrap().x.unwrap();
    let by = g.node("b").unwrap().y.unwrap();
    assert_eq!(ax, 25.0);
    assert_eq!(ay, 100.0);
    assert_eq!(bx, 50.0 + 200.0 + 75.0 / 2.0);
    assert_eq!(by, 100.0);
}

#[test]
fn layout_can_layout_two_nodes_connected() {
    let mut g = make_graph();
    let a = NodeLabel {
        width: 50.0,

        height: 100.0,

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        width: 75.0,

        height: 200.0,

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    g.set_edge("a", "b", Some(EdgeLabel::default()), None);
    let opts = LayoutOptions {
        ranksep: 300.0,

        ..Default::default()
    };
    layout(&mut g, Some(opts));

    let ax = g.node("a").unwrap().x.unwrap();
    let ay = g.node("a").unwrap().y.unwrap();
    let bx = g.node("b").unwrap().x.unwrap();
    let by = g.node("b").unwrap().y.unwrap();
    assert_eq!(ax, 75.0 / 2.0);
    assert_eq!(ay, 100.0 / 2.0);
    assert_eq!(bx, 75.0 / 2.0);
    assert_eq!(by, 100.0 + 300.0 + 200.0 / 2.0);

    // Edge should not have x, y if no label dimensions
    let edge = g.edge("a", "b", None).unwrap();
    assert!(edge.x.is_none());
    assert!(edge.y.is_none());
}

#[test]
fn layout_can_layout_edge_with_label() {
    let mut g = make_graph();
    let a = NodeLabel {
        width: 50.0,

        height: 100.0,

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        width: 75.0,

        height: 200.0,

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    let el = EdgeLabel {
        width: 60.0,

        height: 70.0,

        labelpos: LabelPos::Center,

        ..Default::default()
    };
    g.set_edge("a", "b", Some(el), None);
    let opts = LayoutOptions {
        ranksep: 300.0,

        ..Default::default()
    };
    layout(&mut g, Some(opts));

    let ax = g.node("a").unwrap().x.unwrap();
    let ay = g.node("a").unwrap().y.unwrap();
    let bx = g.node("b").unwrap().x.unwrap();
    let by = g.node("b").unwrap().y.unwrap();
    assert_eq!(ax, 75.0 / 2.0);
    assert_eq!(ay, 100.0 / 2.0);
    assert_eq!(bx, 75.0 / 2.0);
    // y of b = 100 + 150 + 70 + 150 + 200/2 = 570
    assert_eq!(by, 100.0 + 150.0 + 70.0 + 150.0 + 200.0 / 2.0);

    let edge = g.edge("a", "b", None).unwrap();
    assert_eq!(edge.x, Some(75.0 / 2.0));
    assert_eq!(edge.y, Some(100.0 + 150.0 + 70.0 / 2.0));
}

#[test]
fn layout_short_cycle() {
    let mut g = make_graph();
    let a = NodeLabel {
        width: 100.0,

        height: 100.0,

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        width: 100.0,

        height: 100.0,

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    let el_ab = EdgeLabel {
        weight: 2,

        ..Default::default()
    };
    g.set_edge("a", "b", Some(el_ab), None);
    g.set_edge("b", "a", Some(EdgeLabel::default()), None);
    let opts = LayoutOptions {
        ranksep: 200.0,

        ..Default::default()
    };
    layout(&mut g, Some(opts));

    let ax = g.node("a").unwrap().x.unwrap();
    let ay = g.node("a").unwrap().y.unwrap();
    let bx = g.node("b").unwrap().x.unwrap();
    let by = g.node("b").unwrap().y.unwrap();
    assert_eq!(ax, 50.0);
    assert_eq!(ay, 50.0);
    assert_eq!(bx, 50.0);
    assert_eq!(by, 100.0 + 200.0 + 50.0);
}

#[test]
fn layout_adds_rectangle_intersects_for_edges() {
    let mut g = make_graph();
    let a = NodeLabel {
        width: 100.0,

        height: 100.0,

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        width: 100.0,

        height: 100.0,

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    g.set_edge("a", "b", Some(EdgeLabel::default()), None);
    let opts = LayoutOptions {
        ranksep: 200.0,

        ..Default::default()
    };
    layout(&mut g, Some(opts));

    let points = &g.edge("a", "b", None).unwrap().points;
    assert_eq!(points.len(), 3);
    assert_eq!(points[0].x, 50.0);
    assert_eq!(points[0].y, 100.0); // bottom of a
    assert_eq!(points[1].x, 50.0);
    assert_eq!(points[1].y, 100.0 + 100.0); // midpoint
    assert_eq!(points[2].x, 50.0);
    assert_eq!(points[2].y, 100.0 + 200.0); // top of b
}

#[test]
fn layout_adds_rectangle_intersects_for_multi_rank_edges() {
    let mut g = make_graph();
    let a = NodeLabel {
        width: 100.0,

        height: 100.0,

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let b = NodeLabel {
        width: 100.0,

        height: 100.0,

        ..Default::default()
    };
    g.set_node("b".to_string(), Some(b));
    let el = EdgeLabel {
        minlen: 2,

        ..Default::default()
    };
    g.set_edge("a", "b", Some(el), None);
    let opts = LayoutOptions {
        ranksep: 200.0,

        ..Default::default()
    };
    layout(&mut g, Some(opts));

    let points = &g.edge("a", "b", None).unwrap().points;
    assert_eq!(points.len(), 5);
    assert_eq!(points[0].x, 50.0);
    assert_eq!(points[0].y, 100.0); // bottom of a
    assert_eq!(points[1].x, 50.0);
    assert_eq!(points[1].y, 200.0); // bend #1
    assert_eq!(points[2].x, 50.0);
    assert_eq!(points[2].y, 300.0); // label point
    assert_eq!(points[3].x, 50.0);
    assert_eq!(points[3].y, 400.0); // bend #2
    assert_eq!(points[4].x, 50.0);
    assert_eq!(points[4].y, 500.0); // top of b
}

#[test]
fn layout_can_layout_graph_with_subgraphs() {
    let mut g = make_graph();
    let a = NodeLabel {
        width: 50.0,

        height: 50.0,

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    g.set_parent("a", Some("sg1"));
    layout(&mut g, None);
    // This test primarily ensures nothing panics
    assert!(g.node("a").unwrap().x.is_some());
    assert!(g.node("a").unwrap().y.is_some());
}

#[test]
fn layout_adds_dimensions_to_graph() {
    let mut g = make_graph();
    let a = NodeLabel {
        width: 100.0,

        height: 50.0,

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    layout(&mut g, None);
    let gl = g.graph_label::<GraphLabel>().unwrap();
    assert_eq!(gl.width, 100.0);
    assert_eq!(gl.height, 50.0);
}

#[test]
fn layout_ensures_coords_in_bounding_box_tb() {
    let mut g = make_graph();
    let a = NodeLabel {
        width: 100.0,

        height: 200.0,

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    layout(&mut g, None);
    assert_eq!(g.node("a").unwrap().x, Some(50.0));
    assert_eq!(g.node("a").unwrap().y, Some(100.0));
}

#[test]
fn layout_ensures_coords_in_bounding_box_lr() {
    let mut g = make_graph();
    let a = NodeLabel {
        width: 100.0,

        height: 200.0,

        ..Default::default()
    };
    g.set_node("a".to_string(), Some(a));
    let opts = LayoutOptions {
        rankdir: RankDir::LR,
        ..Default::default()
    };
    layout(&mut g, Some(opts));
    assert_eq!(g.node("a").unwrap().x, Some(50.0));
    assert_eq!(g.node("a").unwrap().y, Some(100.0));
}

#[test]
fn layout_minimizes_height_of_subgraphs() {
    let mut g = make_graph();
    for v in &["a", "b", "c", "d", "x", "y"] {
        let label = NodeLabel {
            width: 50.0,

            height: 50.0,

            ..Default::default()
        };
        g.set_node(v.to_string(), Some(label));
    }
    g.set_path(&["a", "b", "c", "d"], Some(EdgeLabel::default()));
    let el_ax = EdgeLabel {
        weight: 100,

        ..Default::default()
    };
    g.set_edge("a", "x", Some(el_ax), None);
    let el_yd = EdgeLabel {
        weight: 100,

        ..Default::default()
    };
    g.set_edge("y", "d", Some(el_yd), None);
    g.set_parent("x", Some("sg"));
    g.set_parent("y", Some("sg"));

    layout(&mut g, None);
    assert_eq!(g.node("x").unwrap().y, g.node("y").unwrap().y);
}

#[test]
fn layout_can_layout_subgraphs_with_different_rankdirs() {
    for rankdir in &[RankDir::TB, RankDir::BT, RankDir::LR, RankDir::RL] {
        let mut g = make_graph();
        let a = NodeLabel {
            width: 50.0,

            height: 50.0,

            ..Default::default()
        };
        g.set_node("a".to_string(), Some(a));
        g.set_node("sg".to_string(), Some(NodeLabel::default()));
        g.set_parent("a", Some("sg"));

        let opts = LayoutOptions {
            rankdir: *rankdir,
            ..Default::default()
        };
        layout(&mut g, Some(opts));

        let sg = g.node("sg").unwrap();
        assert!(sg.width > 50.0, "sg width should be > 50 for {:?}", rankdir);
        assert!(
            sg.height > 50.0,
            "sg height should be > 50 for {:?}",
            rankdir
        );
        assert!(
            sg.x.unwrap() > 25.0,
            "sg x should be > 25 for {:?}",
            rankdir
        );
        assert!(
            sg.y.unwrap() > 25.0,
            "sg y should be > 25 for {:?}",
            rankdir
        );
    }
}
