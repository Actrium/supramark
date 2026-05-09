//! Brandes-Köpf coordinate assignment algorithm.
//!
//! "Fast and Simple Horizontal Coordinate Assignment" — the algorithm runs
//! 4 times (up/down × left/right), producing one x-coordinate mapping each
//! time, then balances the results by taking the average of the two median
//! values for every node.

use std::collections::HashMap;

use log::trace;

use crate::dagre::graph::Graph;
use crate::dagre::layout::types::{Align, BorderType, EdgeLabel, GraphLabel, LabelPos, NodeLabel};
use crate::dagre::layout::util::build_layer_matrix;

/// Merged conflict set – keyed by the pair (min(v,w), max(v,w)).
type Conflicts = HashMap<String, HashMap<String, bool>>;

/// Node-id -> x-coordinate.
type PositionMap = HashMap<String, f64>;

/// A boxed function that returns neighbors of a node.
type NeighborFn<'a> = Box<dyn Fn(&str) -> Vec<String> + 'a>;

/// Function type for setting x-coordinates in a block graph.
type SetXsFn<'a> = dyn Fn(&Graph<(), f64>, &mut PositionMap, &str) + 'a;

/// Function type for getting next nodes in a block graph.
type NextNodesFn = dyn Fn(&Graph<(), f64>, &str) -> Vec<String>;

/// Alignment result from `vertical_alignment`.
pub(crate) struct AlignmentResult {
    pub(crate) root: HashMap<String, String>,
    pub(crate) align: HashMap<String, String>,
}

/// 4-direction keyed x-coordinate maps: "ul", "ur", "dl", "dr".
type XssMap = HashMap<String, PositionMap>;

// ---------------------------------------------------------------------------
// Public entry
// ---------------------------------------------------------------------------

/// Main entry: compute x-coordinates for every node using the BK algorithm.
pub(crate) fn position_x(g: &Graph<NodeLabel, EdgeLabel>) -> PositionMap {
    let layering = build_layer_matrix(g);
    let mut conflicts = find_type1_conflicts(g, &layering);
    // Merge type-2 conflicts into the same map.
    let type2 = find_type2_conflicts(g, &layering);
    for (k, inner) in type2 {
        let entry = conflicts.entry(k).or_default();
        for (k2, v2) in inner {
            entry.insert(k2, v2);
        }
    }

    let mut xss: XssMap = HashMap::new();

    for vert in &["u", "d"] {
        let adjusted_base: Vec<Vec<String>> = if *vert == "u" {
            layering.clone()
        } else {
            layering.iter().rev().cloned().collect()
        };

        for horiz in &["l", "r"] {
            let adjusted: Vec<Vec<String>> = if *horiz == "r" {
                adjusted_base
                    .iter()
                    .map(|layer| layer.iter().rev().cloned().collect())
                    .collect()
            } else {
                adjusted_base.clone()
            };

            let neighbor_fn: NeighborFn<'_> = if *vert == "u" {
                Box::new(|v: &str| g.predecessors(v).unwrap_or_default())
            } else {
                Box::new(|v: &str| g.successors(v).unwrap_or_default())
            };

            let alignment = vertical_alignment(g, &adjusted, &conflicts, &*neighbor_fn);
            let mut xs = horizontal_compaction(
                g,
                &adjusted,
                &alignment.root,
                &alignment.align,
                *horiz == "r",
            );

            if *horiz == "r" {
                for val in xs.values_mut() {
                    *val = -*val;
                }
            }

            let key = format!("{}{}", vert, horiz);
            trace!("BK alignment {}: {} nodes", key, xs.len());
            xss.insert(key, xs);
        }
    }

    let (smallest_key, smallest) = find_smallest_width_alignment(g, &xss);
    align_coordinates(&mut xss, &smallest_key, &smallest);

    let graph_label = graph_label(g);
    let align_opt = graph_label.and_then(|gl| gl.align);
    balance(&xss, align_opt)
}

// ---------------------------------------------------------------------------
// Type-1 conflicts
// ---------------------------------------------------------------------------

/// Find type-1 conflicts: non-inner segment edges that cross inner segments.
/// An inner segment is an edge where both endpoints are dummy nodes.
pub(crate) fn find_type1_conflicts(
    g: &Graph<NodeLabel, EdgeLabel>,
    layering: &[Vec<String>],
) -> Conflicts {
    let mut conflicts: Conflicts = HashMap::new();

    if layering.is_empty() {
        return conflicts;
    }

    let mut prev_layer: &Vec<String> = &layering[0];
    for layer in layering.iter().skip(1) {
        let mut k0: usize = 0;
        let mut scan_pos: usize = 0;
        let prev_layer_length = prev_layer.len();
        let last_node = layer.last().cloned().unwrap_or_default();

        for (i, v) in layer.iter().enumerate() {
            let w = find_other_inner_segment_node(g, v);
            let k1: usize = if let Some(ref w_id) = w {
                if let Some(nl) = g.node(w_id) {
                    nl.order.unwrap_or(prev_layer_length)
                } else {
                    prev_layer_length
                }
            } else {
                prev_layer_length
            };

            if w.is_some() || *v == last_node {
                for scan_node in layer.iter().skip(scan_pos).take(i + 1 - scan_pos) {
                    if let Some(preds) = g.predecessors(scan_node) {
                        for u in &preds {
                            if let Some(u_label) = g.node(u) {
                                let u_pos = u_label.order.unwrap_or(0);
                                if (u_pos < k0 || k1 < u_pos)
                                    && !(u_label.dummy.is_some()
                                        && g.node(scan_node).is_some_and(|sn| sn.dummy.is_some()))
                                {
                                    add_conflict(&mut conflicts, u, scan_node);
                                }
                            }
                        }
                    }
                }
                scan_pos = i + 1;
                k0 = k1;
            }
        }

        prev_layer = layer;
    }

    conflicts
}

// ---------------------------------------------------------------------------
// Type-2 conflicts
// ---------------------------------------------------------------------------

/// Find type-2 conflicts: dummy-to-dummy edges that cross border segments.
pub(crate) fn find_type2_conflicts(
    g: &Graph<NodeLabel, EdgeLabel>,
    layering: &[Vec<String>],
) -> Conflicts {
    let mut conflicts: Conflicts = HashMap::new();

    if layering.is_empty() {
        return conflicts;
    }

    let mut prev_layer: &Vec<String> = &layering[0];
    for layer in layering.iter().skip(1) {
        let north = prev_layer;
        let south = layer;
        let mut prev_north_pos: isize = -1;
        let mut next_north_pos: isize = -1;
        let mut south_pos: usize = 0;

        for (south_lookahead, v) in south.iter().enumerate() {
            if let Some(nl) = g.node(v)
                && nl.dummy.as_deref() == Some("border")
                && let Some(preds) = g.predecessors(v)
                && let Some(first_pred) = preds.first()
                && let Some(pred_label) = g.node(first_pred)
            {
                next_north_pos = pred_label.order.unwrap_or(0) as isize;
                scan_type2(
                    g,
                    &mut conflicts,
                    south,
                    south_pos,
                    south_lookahead,
                    prev_north_pos,
                    next_north_pos,
                );
                south_pos = south_lookahead;
                prev_north_pos = next_north_pos;
            }
            // Final scan after every node (matches the JS forEach structure:
            // the scan call is inside the forEach but outside the if-border block).
            scan_type2(
                g,
                &mut conflicts,
                south,
                south_pos,
                south.len(),
                next_north_pos,
                north.len() as isize,
            );
        }

        prev_layer = layer;
    }

    conflicts
}

fn scan_type2(
    g: &Graph<NodeLabel, EdgeLabel>,
    conflicts: &mut Conflicts,
    south: &[String],
    south_pos: usize,
    south_end: usize,
    prev_north_border: isize,
    next_north_border: isize,
) {
    for i in south_pos..south_end {
        if let Some(v) = south.get(i)
            && let Some(nl) = g.node(v)
            && nl.dummy.is_some()
            && let Some(preds) = g.predecessors(v)
        {
            for u in &preds {
                if let Some(u_node) = g.node(u)
                    && u_node.dummy.is_some()
                {
                    let u_order = u_node.order.unwrap_or(0) as isize;
                    if u_order < prev_north_border || u_order > next_north_border {
                        add_conflict(conflicts, u, v);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Conflict helpers
// ---------------------------------------------------------------------------

/// If v is a dummy node with a dummy predecessor, return that predecessor.
fn find_other_inner_segment_node(g: &Graph<NodeLabel, EdgeLabel>, v: &str) -> Option<String> {
    if let Some(nl) = g.node(v)
        && nl.dummy.is_some()
        && let Some(preds) = g.predecessors(v)
    {
        return preds
            .into_iter()
            .find(|u| g.node(u).is_some_and(|ul| ul.dummy.is_some()));
    }
    None
}

/// Record a conflict between nodes v and w (order-independent).
pub(crate) fn add_conflict(conflicts: &mut Conflicts, v: &str, w: &str) {
    let (v, w) = if v > w { (w, v) } else { (v, w) };
    conflicts
        .entry(v.to_string())
        .or_default()
        .insert(w.to_string(), true);
}

/// Check if a conflict exists between nodes v and w.
pub(crate) fn has_conflict(conflicts: &Conflicts, v: &str, w: &str) -> bool {
    let (v, w) = if v > w { (w, v) } else { (v, w) };
    conflicts.get(v).is_some_and(|inner| inner.contains_key(w))
}

// ---------------------------------------------------------------------------
// Vertical alignment
// ---------------------------------------------------------------------------

/// Attempt to align nodes into vertical "blocks" by connecting each node to
/// one of its median neighbors, skipping conflict edges and already-blocked
/// positions.
pub(crate) fn vertical_alignment(
    _g: &Graph<NodeLabel, EdgeLabel>,
    layering: &[Vec<String>],
    conflicts: &Conflicts,
    neighbor_fn: &dyn Fn(&str) -> Vec<String>,
) -> AlignmentResult {
    let mut root: HashMap<String, String> = HashMap::new();
    let mut align: HashMap<String, String> = HashMap::new();
    let mut pos: HashMap<String, usize> = HashMap::new();

    // Cache positions from layering order (may differ from node.order when
    // the layering has been reversed for a particular sweep direction).
    for layer in layering {
        for (order, v) in layer.iter().enumerate() {
            root.insert(v.clone(), v.clone());
            align.insert(v.clone(), v.clone());
            pos.insert(v.clone(), order);
        }
    }

    for layer in layering {
        let mut prev_idx: isize = -1;
        for v in layer {
            let ws_raw = neighbor_fn(v);
            if ws_raw.is_empty() {
                continue;
            }
            let mut ws = ws_raw;
            ws.sort_by(|a, b| {
                let pa = pos.get(a.as_str()).copied().unwrap_or(0);
                let pb = pos.get(b.as_str()).copied().unwrap_or(0);
                pa.cmp(&pb)
            });

            let mp: f64 = (ws.len() as f64 - 1.0) / 2.0;
            let lo = mp.floor() as usize;
            let hi = mp.ceil() as usize;

            for i in lo..=hi {
                if let Some(w) = ws.get(i) {
                    let pos_w = match pos.get(w.as_str()) {
                        Some(&p) => p,
                        None => continue,
                    };

                    if align.get(v.as_str()).map(|a| a.as_str()) == Some(v.as_str())
                        && prev_idx < pos_w as isize
                        && !has_conflict(conflicts, v, w)
                        && let Some(root_w) = root.get(w.as_str()).cloned()
                    {
                        align.insert(w.clone(), v.clone());
                        let r = root_w.clone();
                        root.insert(v.clone(), r.clone());
                        align.insert(v.clone(), r);
                        prev_idx = pos_w as isize;
                    }
                }
            }
        }
    }

    AlignmentResult { root, align }
}

// ---------------------------------------------------------------------------
// Horizontal compaction
// ---------------------------------------------------------------------------

/// Compute final x-coordinates by building a "block graph" and performing
/// two sweeps: first forward (smallest coordinates) then backward (tighten
/// unused space).
pub(crate) fn horizontal_compaction(
    g: &Graph<NodeLabel, EdgeLabel>,
    layering: &[Vec<String>],
    root: &HashMap<String, String>,
    align: &HashMap<String, String>,
    reverse_sep: bool,
) -> PositionMap {
    let mut xs: PositionMap = HashMap::new();
    let block_g = build_block_graph(g, layering, root, reverse_sep);
    let border_type = if reverse_sep {
        BorderType::Left
    } else {
        BorderType::Right
    };

    // Iterative post-order traversal via an explicit stack.
    fn iterate(
        block_g: &Graph<(), f64>,
        xs: &mut PositionMap,
        set_xs_func: &SetXsFn<'_>,
        next_nodes_func: &NextNodesFn,
    ) {
        let mut stack: Vec<String> = block_g.nodes();
        let mut visited: HashMap<String, bool> = HashMap::new();

        while let Some(elem) = stack.pop() {
            if visited.contains_key(&elem) {
                set_xs_func(block_g, xs, &elem);
            } else {
                visited.insert(elem.clone(), true);
                stack.push(elem.clone());
                for next_elem in next_nodes_func(block_g, &elem) {
                    stack.push(next_elem);
                }
            }
        }
    }

    // Pass 1: assign smallest coordinates (forward from predecessors).
    let pass1 = |block_g: &Graph<(), f64>, xs: &mut PositionMap, elem: &str| {
        if let Some(in_edges) = block_g.in_edges(elem, None) {
            if in_edges.is_empty() {
                xs.insert(elem.to_string(), 0.0);
            } else {
                let val = in_edges.iter().fold(0.0_f64, |acc, e| {
                    let xs_v = xs.get(&e.v).copied().unwrap_or(0.0);
                    let edge_weight = block_g
                        .edge(&e.v, &e.w, e.name.as_deref())
                        .copied()
                        .unwrap_or(0.0);
                    acc.max(xs_v + edge_weight)
                });
                xs.insert(elem.to_string(), val);
            }
        } else {
            xs.insert(elem.to_string(), 0.0);
        }
    };

    let predecessors_fn = |block_g: &Graph<(), f64>, elem: &str| -> Vec<String> {
        block_g.predecessors(elem).unwrap_or_default()
    };

    iterate(&block_g, &mut xs, &pass1, &predecessors_fn);

    // Pass 2: assign greatest coordinates (backward from successors).
    let pass2 = |block_g: &Graph<(), f64>, xs: &mut PositionMap, elem: &str| {
        let mut min = f64::INFINITY;
        if let Some(out_edges) = block_g.out_edges(elem, None)
            && !out_edges.is_empty()
        {
            min = out_edges.iter().fold(f64::INFINITY, |acc, e| {
                let xs_w = xs.get(&e.w).copied().unwrap_or(0.0);
                let edge_weight = block_g
                    .edge(&e.v, &e.w, e.name.as_deref())
                    .copied()
                    .unwrap_or(0.0);
                acc.min(xs_w - edge_weight)
            });
        }

        if let Some(node) = g.node(elem)
            && min != f64::INFINITY
            && node.border_type != Some(border_type)
        {
            let cur = xs.get(elem).copied().unwrap_or(0.0);
            xs.insert(elem.to_string(), cur.max(min));
        }
    };

    let successors_fn = |block_g: &Graph<(), f64>, elem: &str| -> Vec<String> {
        block_g.successors(elem).unwrap_or_default()
    };

    iterate(&block_g, &mut xs, &pass2, &successors_fn);

    // Assign x coordinates to all nodes (non-root nodes get their root's x).
    for v in align.keys() {
        if let Some(root_v) = root.get(v) {
            let root_x = xs.get(root_v).copied().unwrap_or(0.0);
            xs.insert(v.clone(), root_x);
        }
    }

    xs
}

// ---------------------------------------------------------------------------
// Block graph construction
// ---------------------------------------------------------------------------

/// Build a graph where each block root is a node, and edges represent the
/// minimum required separation between adjacent blocks in the same layer.
fn build_block_graph(
    g: &Graph<NodeLabel, EdgeLabel>,
    layering: &[Vec<String>],
    root: &HashMap<String, String>,
    reverse_sep: bool,
) -> Graph<(), f64> {
    let mut block_g: Graph<(), f64> = Graph::new();
    let gl = graph_label(g);
    let nodesep = gl.map_or(50.0, |l| l.nodesep);
    let edgesep = gl.map_or(20.0, |l| l.edgesep);

    for layer in layering {
        let mut u: Option<&String> = None;
        for v in layer {
            if let Some(v_root) = root.get(v) {
                block_g.set_node(v_root.clone(), None);
                if let Some(u_id) = u
                    && let Some(u_root) = root.get(u_id)
                {
                    let sep_val = sep(g, nodesep, edgesep, reverse_sep, v, u_id);
                    let prev_max = block_g.edge(u_root, v_root, None).copied().unwrap_or(0.0);
                    let new_weight = sep_val.max(prev_max);
                    block_g.set_edge(u_root.clone(), v_root.clone(), Some(new_weight), None);
                }
                u = Some(v);
            }
        }
    }

    block_g
}

// ---------------------------------------------------------------------------
// Separation function
// ---------------------------------------------------------------------------

/// Compute the required horizontal separation between nodes v and w,
/// considering their widths, label positions, and dummy/non-dummy status.
fn sep(
    g: &Graph<NodeLabel, EdgeLabel>,
    nodesep: f64,
    edgesep: f64,
    reverse_sep: bool,
    v: &str,
    w: &str,
) -> f64 {
    let v_label = match g.node(v) {
        Some(l) => l,
        None => return 0.0,
    };
    let w_label = match g.node(w) {
        Some(l) => l,
        None => return 0.0,
    };

    let mut sum = 0.0;

    sum += v_label.width / 2.0;

    // labelpos adjustment for v
    let mut delta: Option<f64> = match v_label.labelpos {
        LabelPos::Left => Some(-v_label.width / 2.0),
        LabelPos::Right => Some(v_label.width / 2.0),
        LabelPos::Center => None,
    };
    if let Some(d) = delta {
        sum += if reverse_sep { d } else { -d };
    }

    sum += (if v_label.dummy.is_some() {
        edgesep
    } else {
        nodesep
    }) / 2.0;
    sum += (if w_label.dummy.is_some() {
        edgesep
    } else {
        nodesep
    }) / 2.0;

    sum += w_label.width / 2.0;

    // labelpos adjustment for w
    delta = match w_label.labelpos {
        LabelPos::Left => Some(w_label.width / 2.0),
        LabelPos::Right => Some(-w_label.width / 2.0),
        LabelPos::Center => None,
    };
    if let Some(d) = delta {
        sum += if reverse_sep { d } else { -d };
    }

    sum
}

// ---------------------------------------------------------------------------
// Alignment & balancing
// ---------------------------------------------------------------------------

/// Return the key and alignment (PositionMap) that results in the smallest
/// total width. Iterates in the same order as dagre.js (ul, ur, dl, dr)
/// so that when two alignments have the same width, the first one wins.
pub(crate) fn find_smallest_width_alignment(
    g: &Graph<NodeLabel, EdgeLabel>,
    xss: &XssMap,
) -> (String, PositionMap) {
    let mut best_width = f64::INFINITY;
    let mut best_key: Option<&str> = None;
    let mut best_xs: Option<&PositionMap> = None;

    // Iterate in the same order as dagre.js Object.values(xss)
    for key in &["ul", "ur", "dl", "dr"] {
        let xs = match xss.get(*key) {
            Some(xs) => xs,
            None => continue,
        };

        let mut max = f64::NEG_INFINITY;
        let mut min = f64::INFINITY;

        for (v, &x) in xs {
            let half_w = node_width(g, v) / 2.0;
            let right = x + half_w;
            let left = x - half_w;
            if right > max {
                max = right;
            }
            if left < min {
                min = left;
            }
        }

        let width = max - min;
        if width < best_width {
            best_width = width;
            best_key = Some(key);
            best_xs = Some(xs);
        }
    }

    (
        best_key.unwrap_or("ul").to_string(),
        best_xs.cloned().unwrap_or_default(),
    )
}

/// Shift all alignments so that left-biased ones share their minimum x with
/// the smallest-width alignment's minimum, and right-biased ones share their
/// maximum x with the smallest-width alignment's maximum.
pub(crate) fn align_coordinates(xss: &mut XssMap, align_to_key: &str, align_to: &PositionMap) {
    if align_to.is_empty() {
        return;
    }

    let align_to_min = align_to.values().copied().fold(f64::INFINITY, f64::min);
    let align_to_max = align_to.values().copied().fold(f64::NEG_INFINITY, f64::max);

    for vert in &["u", "d"] {
        for horiz in &["l", "r"] {
            let key = format!("{}{}", vert, horiz);
            // Skip the alignment we're aligning to (JS uses reference equality)
            if key == align_to_key {
                continue;
            }
            let xs = match xss.get(&key) {
                Some(xs) => xs.clone(),
                None => continue,
            };

            if xs.is_empty() {
                continue;
            }

            let xs_min = xs.values().copied().fold(f64::INFINITY, f64::min);
            let xs_max = xs.values().copied().fold(f64::NEG_INFINITY, f64::max);

            let delta = if *horiz == "l" {
                align_to_min - xs_min
            } else {
                align_to_max - xs_max
            };

            if delta.abs() > f64::EPSILON {
                let shifted: PositionMap =
                    xs.iter().map(|(k, &v)| (k.clone(), v + delta)).collect();
                xss.insert(key, shifted);
            }
        }
    }
}

/// Produce the final x-coordinate for each node. If an explicit alignment is
/// specified, use that single alignment's values; otherwise take the average
/// of the two median values from the 4 alignments.
pub(crate) fn balance(xss: &XssMap, align: Option<Align>) -> PositionMap {
    let ul_map = match xss.get("ul") {
        Some(m) => m,
        None => return HashMap::new(),
    };

    ul_map
        .keys()
        .map(|v| {
            if let Some(a) = align {
                let key = match a {
                    Align::UL => "ul",
                    Align::UR => "ur",
                    Align::DL => "dl",
                    Align::DR => "dr",
                };
                if let Some(alignment) = xss.get(key)
                    && let Some(&val) = alignment.get(v)
                {
                    return (v.clone(), val);
                }
            }

            // Collect values from all 4 alignments, sort, and take the average
            // of the two middle values (indices 1 and 2).
            let mut vals: Vec<f64> = xss
                .values()
                .map(|xs| xs.get(v).copied().unwrap_or(0.0))
                .collect();
            vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median =
                (vals.get(1).copied().unwrap_or(0.0) + vals.get(2).copied().unwrap_or(0.0)) / 2.0;
            (v.clone(), median)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn node_width(g: &Graph<NodeLabel, EdgeLabel>, v: &str) -> f64 {
    g.node(v).map_or(0.0, |n| n.width)
}

/// Retrieve the graph-level label, downcasting from Any.
fn graph_label(g: &Graph<NodeLabel, EdgeLabel>) -> Option<&GraphLabel> {
    g.graph_label::<GraphLabel>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dagre::graph::Graph;
    use crate::dagre::layout::types::{EdgeLabel, GraphLabel, LabelPos, NodeLabel};
    use crate::dagre::layout::util::build_layer_matrix;
    use std::collections::HashMap;

    fn new_graph() -> Graph<NodeLabel, EdgeLabel> {
        let mut g = Graph::new();
        g.set_graph_label(GraphLabel::default());
        g
    }

    fn node(rank: i32, order: usize) -> NodeLabel {
        NodeLabel {
            rank: Some(rank),
            order: Some(order),
            ..Default::default()
        }
    }

    fn node_w(rank: i32, order: usize, width: f64) -> NodeLabel {
        NodeLabel {
            rank: Some(rank),
            order: Some(order),
            width,
            ..Default::default()
        }
    }

    fn node_wd(rank: i32, order: usize, width: f64, dummy: &str) -> NodeLabel {
        NodeLabel {
            rank: Some(rank),
            order: Some(order),
            width,
            dummy: Some(dummy.to_string()),
            ..Default::default()
        }
    }

    fn node_wdl(rank: i32, order: usize, width: f64, dummy: &str, labelpos: LabelPos) -> NodeLabel {
        NodeLabel {
            rank: Some(rank),
            order: Some(order),
            width,
            dummy: Some(dummy.to_string()),
            labelpos,
            ..Default::default()
        }
    }

    // -----------------------------------------------------------------------
    // findType1Conflicts
    // -----------------------------------------------------------------------

    fn setup_type1_graph() -> Graph<NodeLabel, EdgeLabel> {
        let mut g = new_graph();
        g.set_default_edge_label(|_| EdgeLabel::default());
        g.set_node("a".to_string(), Some(node(0, 0)));
        g.set_node("b".to_string(), Some(node(0, 1)));
        g.set_node("c".to_string(), Some(node(1, 0)));
        g.set_node("d".to_string(), Some(node(1, 1)));
        // Set up crossing
        g.set_edge("a", "d", None, None);
        g.set_edge("b", "c", None, None);
        g
    }

    #[test]
    fn type1_does_not_mark_edges_that_have_no_conflict() {
        let mut g = setup_type1_graph();
        g.remove_edge("a", "d", None);
        g.remove_edge("b", "c", None);
        g.set_edge("a", "c", None, None);
        g.set_edge("b", "d", None, None);

        let layering = build_layer_matrix(&g);
        let conflicts = find_type1_conflicts(&g, &layering);
        assert!(!has_conflict(&conflicts, "a", "c"));
        assert!(!has_conflict(&conflicts, "b", "d"));
    }

    #[test]
    fn type1_does_not_mark_type0_conflicts_no_dummies() {
        let g = setup_type1_graph();
        let layering = build_layer_matrix(&g);
        let conflicts = find_type1_conflicts(&g, &layering);
        assert!(!has_conflict(&conflicts, "a", "d"));
        assert!(!has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type1_does_not_mark_type0_conflicts_a_is_dummy() {
        let mut g = setup_type1_graph();
        g.node_mut("a").unwrap().dummy = Some("true".to_string());
        let layering = build_layer_matrix(&g);
        let conflicts = find_type1_conflicts(&g, &layering);
        assert!(!has_conflict(&conflicts, "a", "d"));
        assert!(!has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type1_does_not_mark_type0_conflicts_b_is_dummy() {
        let mut g = setup_type1_graph();
        g.node_mut("b").unwrap().dummy = Some("true".to_string());
        let layering = build_layer_matrix(&g);
        let conflicts = find_type1_conflicts(&g, &layering);
        assert!(!has_conflict(&conflicts, "a", "d"));
        assert!(!has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type1_does_not_mark_type0_conflicts_c_is_dummy() {
        let mut g = setup_type1_graph();
        g.node_mut("c").unwrap().dummy = Some("true".to_string());
        let layering = build_layer_matrix(&g);
        let conflicts = find_type1_conflicts(&g, &layering);
        assert!(!has_conflict(&conflicts, "a", "d"));
        assert!(!has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type1_does_not_mark_type0_conflicts_d_is_dummy() {
        let mut g = setup_type1_graph();
        g.node_mut("d").unwrap().dummy = Some("true".to_string());
        let layering = build_layer_matrix(&g);
        let conflicts = find_type1_conflicts(&g, &layering);
        assert!(!has_conflict(&conflicts, "a", "d"));
        assert!(!has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type1_does_mark_conflict_a_is_non_dummy() {
        // a is non-dummy, b,c,d are dummies
        let mut g = setup_type1_graph();
        for v in &["b", "c", "d"] {
            g.node_mut(v).unwrap().dummy = Some("true".to_string());
        }
        let layering = build_layer_matrix(&g);
        let conflicts = find_type1_conflicts(&g, &layering);
        // v=a or v=d => (a,d) is marked, (b,c) is not
        assert!(has_conflict(&conflicts, "a", "d"));
        assert!(!has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type1_does_mark_conflict_b_is_non_dummy() {
        let mut g = setup_type1_graph();
        for v in &["a", "c", "d"] {
            g.node_mut(v).unwrap().dummy = Some("true".to_string());
        }
        let layering = build_layer_matrix(&g);
        let conflicts = find_type1_conflicts(&g, &layering);
        // v=b or v=c => (b,c) is marked, (a,d) is not
        assert!(!has_conflict(&conflicts, "a", "d"));
        assert!(has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type1_does_mark_conflict_c_is_non_dummy() {
        let mut g = setup_type1_graph();
        for v in &["a", "b", "d"] {
            g.node_mut(v).unwrap().dummy = Some("true".to_string());
        }
        let layering = build_layer_matrix(&g);
        let conflicts = find_type1_conflicts(&g, &layering);
        // v=c => (b,c) is marked, (a,d) is not
        assert!(!has_conflict(&conflicts, "a", "d"));
        assert!(has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type1_does_mark_conflict_d_is_non_dummy() {
        let mut g = setup_type1_graph();
        for v in &["a", "b", "c"] {
            g.node_mut(v).unwrap().dummy = Some("true".to_string());
        }
        let layering = build_layer_matrix(&g);
        let conflicts = find_type1_conflicts(&g, &layering);
        // v=d => (a,d) is marked, (b,c) is not
        assert!(has_conflict(&conflicts, "a", "d"));
        assert!(!has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type1_does_not_mark_type2_conflicts_all_dummies() {
        let mut g = setup_type1_graph();
        for v in &["a", "b", "c", "d"] {
            g.node_mut(v).unwrap().dummy = Some("true".to_string());
        }
        let layering = build_layer_matrix(&g);
        let conflicts = find_type1_conflicts(&g, &layering);
        assert!(!has_conflict(&conflicts, "a", "d"));
        assert!(!has_conflict(&conflicts, "b", "c"));
    }

    // -----------------------------------------------------------------------
    // findType2Conflicts
    // -----------------------------------------------------------------------

    #[test]
    fn type2_marks_conflicts_favoring_border_segments_1() {
        let mut g = setup_type1_graph();
        // a,d are normal dummies; b,c are border dummies
        g.node_mut("a").unwrap().dummy = Some("true".to_string());
        g.node_mut("d").unwrap().dummy = Some("true".to_string());
        g.node_mut("b").unwrap().dummy = Some("border".to_string());
        g.node_mut("c").unwrap().dummy = Some("border".to_string());

        let layering = build_layer_matrix(&g);
        let conflicts = find_type2_conflicts(&g, &layering);
        assert!(has_conflict(&conflicts, "a", "d"));
        assert!(!has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type2_marks_conflicts_favoring_border_segments_2() {
        let mut g = setup_type1_graph();
        // b,c are normal dummies; a,d are border dummies
        g.node_mut("b").unwrap().dummy = Some("true".to_string());
        g.node_mut("c").unwrap().dummy = Some("true".to_string());
        g.node_mut("a").unwrap().dummy = Some("border".to_string());
        g.node_mut("d").unwrap().dummy = Some("border".to_string());

        let layering = build_layer_matrix(&g);
        let conflicts = find_type2_conflicts(&g, &layering);
        assert!(!has_conflict(&conflicts, "a", "d"));
        assert!(has_conflict(&conflicts, "b", "c"));
    }

    // -----------------------------------------------------------------------
    // hasConflict / addConflict
    // -----------------------------------------------------------------------

    #[test]
    fn has_conflict_regardless_of_edge_orientation() {
        let mut conflicts: Conflicts = HashMap::new();
        add_conflict(&mut conflicts, "b", "a");
        assert!(has_conflict(&conflicts, "a", "b"));
        assert!(has_conflict(&conflicts, "b", "a"));
    }

    #[test]
    fn has_conflict_works_for_multiple_conflicts_with_same_node() {
        let mut conflicts: Conflicts = HashMap::new();
        add_conflict(&mut conflicts, "a", "b");
        add_conflict(&mut conflicts, "a", "c");
        assert!(has_conflict(&conflicts, "a", "b"));
        assert!(has_conflict(&conflicts, "a", "c"));
    }

    // -----------------------------------------------------------------------
    // verticalAlignment
    // -----------------------------------------------------------------------

    #[test]
    fn align_with_itself_if_no_adjacencies() {
        let mut g = new_graph();
        g.set_node("a".to_string(), Some(node(0, 0)));
        g.set_node("b".to_string(), Some(node(1, 0)));

        let layering = build_layer_matrix(&g);
        let conflicts: Conflicts = HashMap::new();

        let result = vertical_alignment(&g, &layering, &conflicts, &|v: &str| {
            g.predecessors(v).unwrap_or_default()
        });
        assert_eq!(result.root.get("a").unwrap(), "a");
        assert_eq!(result.root.get("b").unwrap(), "b");
        assert_eq!(result.align.get("a").unwrap(), "a");
        assert_eq!(result.align.get("b").unwrap(), "b");
    }

    #[test]
    fn align_with_sole_adjacency() {
        let mut g = new_graph();
        g.set_node("a".to_string(), Some(node(0, 0)));
        g.set_node("b".to_string(), Some(node(1, 0)));
        g.set_edge("a", "b", None, None);

        let layering = build_layer_matrix(&g);
        let conflicts: Conflicts = HashMap::new();

        let result = vertical_alignment(&g, &layering, &conflicts, &|v: &str| {
            g.predecessors(v).unwrap_or_default()
        });
        assert_eq!(result.root.get("a").unwrap(), "a");
        assert_eq!(result.root.get("b").unwrap(), "a");
        assert_eq!(result.align.get("a").unwrap(), "b");
        assert_eq!(result.align.get("b").unwrap(), "a");
    }

    #[test]
    fn align_with_left_median_when_possible() {
        let mut g = new_graph();
        g.set_node("a".to_string(), Some(node(0, 0)));
        g.set_node("b".to_string(), Some(node(0, 1)));
        g.set_node("c".to_string(), Some(node(1, 0)));
        g.set_edge("a", "c", None, None);
        g.set_edge("b", "c", None, None);

        let layering = build_layer_matrix(&g);
        let conflicts: Conflicts = HashMap::new();

        let result = vertical_alignment(&g, &layering, &conflicts, &|v: &str| {
            g.predecessors(v).unwrap_or_default()
        });
        assert_eq!(result.root.get("a").unwrap(), "a");
        assert_eq!(result.root.get("b").unwrap(), "b");
        assert_eq!(result.root.get("c").unwrap(), "a");
        assert_eq!(result.align.get("a").unwrap(), "c");
        assert_eq!(result.align.get("b").unwrap(), "b");
        assert_eq!(result.align.get("c").unwrap(), "a");
    }

    #[test]
    fn align_correctly_regardless_of_node_name_insertion_order() {
        let mut g = new_graph();
        // Insert in non-alphabetical order
        g.set_node("b".to_string(), Some(node(0, 1)));
        g.set_node("c".to_string(), Some(node(1, 0)));
        g.set_node("z".to_string(), Some(node(0, 0)));
        g.set_edge("z", "c", None, None);
        g.set_edge("b", "c", None, None);

        let layering = build_layer_matrix(&g);
        let conflicts: Conflicts = HashMap::new();

        let result = vertical_alignment(&g, &layering, &conflicts, &|v: &str| {
            g.predecessors(v).unwrap_or_default()
        });
        assert_eq!(result.root.get("z").unwrap(), "z");
        assert_eq!(result.root.get("b").unwrap(), "b");
        assert_eq!(result.root.get("c").unwrap(), "z");
        assert_eq!(result.align.get("z").unwrap(), "c");
        assert_eq!(result.align.get("b").unwrap(), "b");
        assert_eq!(result.align.get("c").unwrap(), "z");
    }

    #[test]
    fn align_with_right_median_when_left_is_unavailable() {
        let mut g = new_graph();
        g.set_node("a".to_string(), Some(node(0, 0)));
        g.set_node("b".to_string(), Some(node(0, 1)));
        g.set_node("c".to_string(), Some(node(1, 0)));
        g.set_edge("a", "c", None, None);
        g.set_edge("b", "c", None, None);

        let layering = build_layer_matrix(&g);
        let mut conflicts: Conflicts = HashMap::new();
        add_conflict(&mut conflicts, "a", "c");

        let result = vertical_alignment(&g, &layering, &conflicts, &|v: &str| {
            g.predecessors(v).unwrap_or_default()
        });
        assert_eq!(result.root.get("a").unwrap(), "a");
        assert_eq!(result.root.get("b").unwrap(), "b");
        assert_eq!(result.root.get("c").unwrap(), "b");
        assert_eq!(result.align.get("a").unwrap(), "a");
        assert_eq!(result.align.get("b").unwrap(), "c");
        assert_eq!(result.align.get("c").unwrap(), "b");
    }

    #[test]
    fn align_with_neither_median_if_both_unavailable() {
        let mut g = new_graph();
        g.set_node("a".to_string(), Some(node(0, 0)));
        g.set_node("b".to_string(), Some(node(0, 1)));
        g.set_node("c".to_string(), Some(node(1, 0)));
        g.set_node("d".to_string(), Some(node(1, 1)));
        g.set_edge("a", "d", None, None);
        g.set_edge("b", "c", None, None);
        g.set_edge("b", "d", None, None);

        let layering = build_layer_matrix(&g);
        let conflicts: Conflicts = HashMap::new();

        let result = vertical_alignment(&g, &layering, &conflicts, &|v: &str| {
            g.predecessors(v).unwrap_or_default()
        });
        assert_eq!(result.root.get("a").unwrap(), "a");
        assert_eq!(result.root.get("b").unwrap(), "b");
        assert_eq!(result.root.get("c").unwrap(), "b");
        assert_eq!(result.root.get("d").unwrap(), "d");
        assert_eq!(result.align.get("a").unwrap(), "a");
        assert_eq!(result.align.get("b").unwrap(), "c");
        assert_eq!(result.align.get("c").unwrap(), "b");
        assert_eq!(result.align.get("d").unwrap(), "d");
    }

    #[test]
    fn align_single_median_for_odd_adjacencies() {
        let mut g = new_graph();
        g.set_node("a".to_string(), Some(node(0, 0)));
        g.set_node("b".to_string(), Some(node(0, 1)));
        g.set_node("c".to_string(), Some(node(0, 2)));
        g.set_node("d".to_string(), Some(node(1, 0)));
        g.set_edge("a", "d", None, None);
        g.set_edge("b", "d", None, None);
        g.set_edge("c", "d", None, None);

        let layering = build_layer_matrix(&g);
        let conflicts: Conflicts = HashMap::new();

        let result = vertical_alignment(&g, &layering, &conflicts, &|v: &str| {
            g.predecessors(v).unwrap_or_default()
        });
        assert_eq!(result.root.get("a").unwrap(), "a");
        assert_eq!(result.root.get("b").unwrap(), "b");
        assert_eq!(result.root.get("c").unwrap(), "c");
        assert_eq!(result.root.get("d").unwrap(), "b");
        assert_eq!(result.align.get("a").unwrap(), "a");
        assert_eq!(result.align.get("b").unwrap(), "d");
        assert_eq!(result.align.get("c").unwrap(), "c");
        assert_eq!(result.align.get("d").unwrap(), "b");
    }

    #[test]
    fn align_blocks_across_multiple_layers() {
        let mut g = new_graph();
        g.set_default_edge_label(|_| EdgeLabel::default());
        g.set_node("a".to_string(), Some(node(0, 0)));
        g.set_node("b".to_string(), Some(node(1, 0)));
        g.set_node("c".to_string(), Some(node(1, 1)));
        g.set_node("d".to_string(), Some(node(2, 0)));
        g.set_path(&["a", "b", "d"], None);
        g.set_path(&["a", "c", "d"], None);

        let layering = build_layer_matrix(&g);
        let conflicts: Conflicts = HashMap::new();

        let result = vertical_alignment(&g, &layering, &conflicts, &|v: &str| {
            g.predecessors(v).unwrap_or_default()
        });
        assert_eq!(result.root.get("a").unwrap(), "a");
        assert_eq!(result.root.get("b").unwrap(), "a");
        assert_eq!(result.root.get("c").unwrap(), "c");
        assert_eq!(result.root.get("d").unwrap(), "a");
        assert_eq!(result.align.get("a").unwrap(), "b");
        assert_eq!(result.align.get("b").unwrap(), "d");
        assert_eq!(result.align.get("c").unwrap(), "c");
        assert_eq!(result.align.get("d").unwrap(), "a");
    }

    // -----------------------------------------------------------------------
    // horizontalCompaction
    // -----------------------------------------------------------------------

    #[test]
    fn hc_single_node_at_origin() {
        let mut g = new_graph();
        g.set_node("a".to_string(), Some(node(0, 0)));
        let root = HashMap::from([("a".to_string(), "a".to_string())]);
        let align = HashMap::from([("a".to_string(), "a".to_string())]);
        let layering = build_layer_matrix(&g);
        let xs = horizontal_compaction(&g, &layering, &root, &align, false);
        assert_eq!(xs["a"], 0.0);
    }

    #[test]
    fn hc_separates_adjacent_nodes_by_nodesep() {
        let mut g = new_graph();
        g.graph_label_mut::<GraphLabel>().unwrap().nodesep = 100.0;
        g.set_node("a".to_string(), Some(node_w(0, 0, 100.0)));
        g.set_node("b".to_string(), Some(node_w(0, 1, 200.0)));
        let root = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
        ]);
        let align = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
        ]);
        let layering = build_layer_matrix(&g);
        let xs = horizontal_compaction(&g, &layering, &root, &align, false);
        assert_eq!(xs["a"], 0.0);
        assert_eq!(xs["b"], 100.0 / 2.0 + 100.0 + 200.0 / 2.0);
    }

    #[test]
    fn hc_separates_adjacent_edges_by_edgesep() {
        let mut g = new_graph();
        g.graph_label_mut::<GraphLabel>().unwrap().edgesep = 20.0;
        g.set_node("a".to_string(), Some(node_wd(0, 0, 100.0, "true")));
        g.set_node("b".to_string(), Some(node_wd(0, 1, 200.0, "true")));
        let root = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
        ]);
        let align = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
        ]);
        let layering = build_layer_matrix(&g);
        let xs = horizontal_compaction(&g, &layering, &root, &align, false);
        assert_eq!(xs["a"], 0.0);
        assert_eq!(xs["b"], 100.0 / 2.0 + 20.0 + 200.0 / 2.0);
    }

    #[test]
    fn hc_aligns_centers_of_nodes_in_same_block() {
        let mut g = new_graph();
        g.set_node("a".to_string(), Some(node_w(0, 0, 100.0)));
        g.set_node("b".to_string(), Some(node_w(1, 0, 200.0)));
        let root = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "a".to_string()),
        ]);
        let align = HashMap::from([
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "a".to_string()),
        ]);
        let layering = build_layer_matrix(&g);
        let xs = horizontal_compaction(&g, &layering, &root, &align, false);
        assert_eq!(xs["a"], 0.0);
        assert_eq!(xs["b"], 0.0);
    }

    #[test]
    fn hc_separates_blocks_with_appropriate_separation() {
        let mut g = new_graph();
        g.graph_label_mut::<GraphLabel>().unwrap().nodesep = 75.0;
        g.set_node("a".to_string(), Some(node_w(0, 0, 100.0)));
        g.set_node("b".to_string(), Some(node_w(1, 1, 200.0)));
        g.set_node("c".to_string(), Some(node_w(1, 0, 50.0)));
        let root = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "a".to_string()),
            ("c".to_string(), "c".to_string()),
        ]);
        let align = HashMap::from([
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "a".to_string()),
            ("c".to_string(), "c".to_string()),
        ]);
        let layering = build_layer_matrix(&g);
        let xs = horizontal_compaction(&g, &layering, &root, &align, false);
        assert_eq!(xs["a"], 50.0 / 2.0 + 75.0 + 200.0 / 2.0);
        assert_eq!(xs["b"], 50.0 / 2.0 + 75.0 + 200.0 / 2.0);
        assert_eq!(xs["c"], 0.0);
    }

    #[test]
    fn hc_separates_classes_with_appropriate_separation() {
        let mut g = new_graph();
        g.graph_label_mut::<GraphLabel>().unwrap().nodesep = 75.0;
        g.set_node("a".to_string(), Some(node_w(0, 0, 100.0)));
        g.set_node("b".to_string(), Some(node_w(0, 1, 200.0)));
        g.set_node("c".to_string(), Some(node_w(1, 0, 50.0)));
        g.set_node("d".to_string(), Some(node_w(1, 1, 80.0)));
        let root = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
            ("c".to_string(), "c".to_string()),
            ("d".to_string(), "b".to_string()),
        ]);
        let align = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "d".to_string()),
            ("c".to_string(), "c".to_string()),
            ("d".to_string(), "b".to_string()),
        ]);
        let layering = build_layer_matrix(&g);
        let xs = horizontal_compaction(&g, &layering, &root, &align, false);
        assert_eq!(xs["a"], 0.0);
        assert_eq!(xs["b"], 100.0 / 2.0 + 75.0 + 200.0 / 2.0);
        assert_eq!(
            xs["c"],
            100.0 / 2.0 + 75.0 + 200.0 / 2.0 - 80.0 / 2.0 - 75.0 - 50.0 / 2.0
        );
        assert_eq!(xs["d"], 100.0 / 2.0 + 75.0 + 200.0 / 2.0);
    }

    #[test]
    fn hc_shifts_classes_by_max_sep_from_adjacent_block_1() {
        let mut g = new_graph();
        g.graph_label_mut::<GraphLabel>().unwrap().nodesep = 75.0;
        g.set_node("a".to_string(), Some(node_w(0, 0, 50.0)));
        g.set_node("b".to_string(), Some(node_w(0, 1, 150.0)));
        g.set_node("c".to_string(), Some(node_w(1, 0, 60.0)));
        g.set_node("d".to_string(), Some(node_w(1, 1, 70.0)));
        let root = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
            ("c".to_string(), "a".to_string()),
            ("d".to_string(), "b".to_string()),
        ]);
        let align = HashMap::from([
            ("a".to_string(), "c".to_string()),
            ("b".to_string(), "d".to_string()),
            ("c".to_string(), "a".to_string()),
            ("d".to_string(), "b".to_string()),
        ]);
        let layering = build_layer_matrix(&g);
        let xs = horizontal_compaction(&g, &layering, &root, &align, false);
        assert_eq!(xs["a"], 0.0);
        assert_eq!(xs["b"], 50.0 / 2.0 + 75.0 + 150.0 / 2.0);
        assert_eq!(xs["c"], 0.0);
        assert_eq!(xs["d"], 50.0 / 2.0 + 75.0 + 150.0 / 2.0);
    }

    #[test]
    fn hc_shifts_classes_by_max_sep_from_adjacent_block_2() {
        let mut g = new_graph();
        g.graph_label_mut::<GraphLabel>().unwrap().nodesep = 75.0;
        g.set_node("a".to_string(), Some(node_w(0, 0, 50.0)));
        g.set_node("b".to_string(), Some(node_w(0, 1, 70.0)));
        g.set_node("c".to_string(), Some(node_w(1, 0, 60.0)));
        g.set_node("d".to_string(), Some(node_w(1, 1, 150.0)));
        let root = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
            ("c".to_string(), "a".to_string()),
            ("d".to_string(), "b".to_string()),
        ]);
        let align = HashMap::from([
            ("a".to_string(), "c".to_string()),
            ("b".to_string(), "d".to_string()),
            ("c".to_string(), "a".to_string()),
            ("d".to_string(), "b".to_string()),
        ]);
        let layering = build_layer_matrix(&g);
        let xs = horizontal_compaction(&g, &layering, &root, &align, false);
        assert_eq!(xs["a"], 0.0);
        assert_eq!(xs["b"], 60.0 / 2.0 + 75.0 + 150.0 / 2.0);
        assert_eq!(xs["c"], 0.0);
        assert_eq!(xs["d"], 60.0 / 2.0 + 75.0 + 150.0 / 2.0);
    }

    #[test]
    fn hc_cascades_class_shift() {
        let mut g = new_graph();
        g.graph_label_mut::<GraphLabel>().unwrap().nodesep = 75.0;
        g.set_node("a".to_string(), Some(node_w(0, 0, 50.0)));
        g.set_node("b".to_string(), Some(node_w(0, 1, 50.0)));
        g.set_node("c".to_string(), Some(node_w(1, 0, 50.0)));
        g.set_node("d".to_string(), Some(node_w(1, 1, 50.0)));
        g.set_node("e".to_string(), Some(node_w(1, 2, 50.0)));
        g.set_node("f".to_string(), Some(node_w(2, 0, 50.0)));
        g.set_node("g".to_string(), Some(node_w(2, 1, 50.0)));
        let root = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
            ("c".to_string(), "c".to_string()),
            ("d".to_string(), "d".to_string()),
            ("e".to_string(), "b".to_string()),
            ("f".to_string(), "f".to_string()),
            ("g".to_string(), "d".to_string()),
        ]);
        let align = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "e".to_string()),
            ("c".to_string(), "c".to_string()),
            ("d".to_string(), "g".to_string()),
            ("e".to_string(), "b".to_string()),
            ("f".to_string(), "f".to_string()),
            ("g".to_string(), "d".to_string()),
        ]);
        let layering = build_layer_matrix(&g);
        let xs = horizontal_compaction(&g, &layering, &root, &align, false);
        // Use f as reference, everything is relative
        assert_eq!(xs["a"], xs["b"] - 50.0 / 2.0 - 75.0 - 50.0 / 2.0);
        assert_eq!(xs["b"], xs["e"]);
        assert_eq!(xs["c"], xs["f"]);
        assert_eq!(xs["d"], xs["c"] + 50.0 / 2.0 + 75.0 + 50.0 / 2.0);
        assert_eq!(xs["e"], xs["d"] + 50.0 / 2.0 + 75.0 + 50.0 / 2.0);
        assert_eq!(xs["g"], xs["f"] + 50.0 / 2.0 + 75.0 + 50.0 / 2.0);
    }

    #[test]
    fn hc_handles_labelpos_l() {
        let mut g = new_graph();
        g.graph_label_mut::<GraphLabel>().unwrap().edgesep = 50.0;
        g.set_node("a".to_string(), Some(node_wd(0, 0, 100.0, "edge")));
        g.set_node(
            "b".to_string(),
            Some(node_wdl(0, 1, 200.0, "edge-label", LabelPos::Left)),
        );
        g.set_node("c".to_string(), Some(node_wd(0, 2, 300.0, "edge")));
        let root = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
            ("c".to_string(), "c".to_string()),
        ]);
        let align = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
            ("c".to_string(), "c".to_string()),
        ]);
        let layering = build_layer_matrix(&g);
        let xs = horizontal_compaction(&g, &layering, &root, &align, false);
        assert_eq!(xs["a"], 0.0);
        assert_eq!(xs["b"], xs["a"] + 100.0 / 2.0 + 50.0 + 200.0);
        assert_eq!(xs["c"], xs["b"] + 0.0 + 50.0 + 300.0 / 2.0);
    }

    #[test]
    fn hc_handles_labelpos_c() {
        let mut g = new_graph();
        g.graph_label_mut::<GraphLabel>().unwrap().edgesep = 50.0;
        g.set_node("a".to_string(), Some(node_wd(0, 0, 100.0, "edge")));
        g.set_node(
            "b".to_string(),
            Some(node_wdl(0, 1, 200.0, "edge-label", LabelPos::Center)),
        );
        g.set_node("c".to_string(), Some(node_wd(0, 2, 300.0, "edge")));
        let root = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
            ("c".to_string(), "c".to_string()),
        ]);
        let align = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
            ("c".to_string(), "c".to_string()),
        ]);
        let layering = build_layer_matrix(&g);
        let xs = horizontal_compaction(&g, &layering, &root, &align, false);
        assert_eq!(xs["a"], 0.0);
        assert_eq!(xs["b"], xs["a"] + 100.0 / 2.0 + 50.0 + 200.0 / 2.0);
        assert_eq!(xs["c"], xs["b"] + 200.0 / 2.0 + 50.0 + 300.0 / 2.0);
    }

    #[test]
    fn hc_handles_labelpos_r() {
        let mut g = new_graph();
        g.graph_label_mut::<GraphLabel>().unwrap().edgesep = 50.0;
        g.set_node("a".to_string(), Some(node_wd(0, 0, 100.0, "edge")));
        g.set_node(
            "b".to_string(),
            Some(node_wdl(0, 1, 200.0, "edge-label", LabelPos::Right)),
        );
        g.set_node("c".to_string(), Some(node_wd(0, 2, 300.0, "edge")));
        let root = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
            ("c".to_string(), "c".to_string()),
        ]);
        let align = HashMap::from([
            ("a".to_string(), "a".to_string()),
            ("b".to_string(), "b".to_string()),
            ("c".to_string(), "c".to_string()),
        ]);
        let layering = build_layer_matrix(&g);
        let xs = horizontal_compaction(&g, &layering, &root, &align, false);
        assert_eq!(xs["a"], 0.0);
        assert_eq!(xs["b"], xs["a"] + 100.0 / 2.0 + 50.0 + 0.0);
        assert_eq!(xs["c"], xs["b"] + 200.0 + 50.0 + 300.0 / 2.0);
    }

    // -----------------------------------------------------------------------
    // alignCoordinates
    // -----------------------------------------------------------------------

    #[test]
    fn align_coordinates_single_node() {
        let mut xss: XssMap = HashMap::new();
        xss.insert("ul".to_string(), HashMap::from([("a".to_string(), 50.0)]));
        xss.insert("ur".to_string(), HashMap::from([("a".to_string(), 100.0)]));
        xss.insert("dl".to_string(), HashMap::from([("a".to_string(), 50.0)]));
        xss.insert("dr".to_string(), HashMap::from([("a".to_string(), 200.0)]));

        let align_to = xss["ul"].clone();
        align_coordinates(&mut xss, "ul", &align_to);

        assert_eq!(xss["ul"]["a"], 50.0);
        assert_eq!(xss["ur"]["a"], 50.0);
        assert_eq!(xss["dl"]["a"], 50.0);
        assert_eq!(xss["dr"]["a"], 50.0);
    }

    #[test]
    fn align_coordinates_multiple_nodes() {
        let mut xss: XssMap = HashMap::new();
        xss.insert(
            "ul".to_string(),
            HashMap::from([("a".to_string(), 50.0), ("b".to_string(), 1000.0)]),
        );
        xss.insert(
            "ur".to_string(),
            HashMap::from([("a".to_string(), 100.0), ("b".to_string(), 900.0)]),
        );
        xss.insert(
            "dl".to_string(),
            HashMap::from([("a".to_string(), 150.0), ("b".to_string(), 800.0)]),
        );
        xss.insert(
            "dr".to_string(),
            HashMap::from([("a".to_string(), 200.0), ("b".to_string(), 700.0)]),
        );

        let align_to = xss["ul"].clone();
        align_coordinates(&mut xss, "ul", &align_to);

        assert_eq!(xss["ul"]["a"], 50.0);
        assert_eq!(xss["ul"]["b"], 1000.0);
        assert_eq!(xss["ur"]["a"], 200.0);
        assert_eq!(xss["ur"]["b"], 1000.0);
        assert_eq!(xss["dl"]["a"], 50.0);
        assert_eq!(xss["dl"]["b"], 700.0);
        assert_eq!(xss["dr"]["a"], 500.0);
        assert_eq!(xss["dr"]["b"], 1000.0);
    }

    // -----------------------------------------------------------------------
    // findSmallestWidthAlignment
    // -----------------------------------------------------------------------

    #[test]
    fn smallest_width_alignment_finds_correct() {
        let mut g = new_graph();
        g.set_node(
            "a".to_string(),
            Some(NodeLabel {
                width: 50.0,
                ..Default::default()
            }),
        );
        g.set_node(
            "b".to_string(),
            Some(NodeLabel {
                width: 50.0,
                ..Default::default()
            }),
        );

        let mut xss: XssMap = HashMap::new();
        xss.insert(
            "ul".to_string(),
            HashMap::from([("a".to_string(), 0.0), ("b".to_string(), 1000.0)]),
        );
        xss.insert(
            "ur".to_string(),
            HashMap::from([("a".to_string(), -5.0), ("b".to_string(), 1000.0)]),
        );
        xss.insert(
            "dl".to_string(),
            HashMap::from([("a".to_string(), 5.0), ("b".to_string(), 2000.0)]),
        );
        xss.insert(
            "dr".to_string(),
            HashMap::from([("a".to_string(), 0.0), ("b".to_string(), 200.0)]),
        );

        let (key, _xs) = find_smallest_width_alignment(&g, &xss);
        assert_eq!(key, "dr");
    }

    #[test]
    fn smallest_width_takes_node_width_into_account() {
        let mut g = new_graph();
        g.set_node(
            "a".to_string(),
            Some(NodeLabel {
                width: 50.0,
                ..Default::default()
            }),
        );
        g.set_node(
            "b".to_string(),
            Some(NodeLabel {
                width: 50.0,
                ..Default::default()
            }),
        );
        g.set_node(
            "c".to_string(),
            Some(NodeLabel {
                width: 200.0,
                ..Default::default()
            }),
        );

        let mut xss: XssMap = HashMap::new();
        xss.insert(
            "ul".to_string(),
            HashMap::from([
                ("a".to_string(), 0.0),
                ("b".to_string(), 100.0),
                ("c".to_string(), 75.0),
            ]),
        );
        xss.insert(
            "ur".to_string(),
            HashMap::from([
                ("a".to_string(), 0.0),
                ("b".to_string(), 100.0),
                ("c".to_string(), 80.0),
            ]),
        );
        xss.insert(
            "dl".to_string(),
            HashMap::from([
                ("a".to_string(), 0.0),
                ("b".to_string(), 100.0),
                ("c".to_string(), 85.0),
            ]),
        );
        xss.insert(
            "dr".to_string(),
            HashMap::from([
                ("a".to_string(), 0.0),
                ("b".to_string(), 100.0),
                ("c".to_string(), 90.0),
            ]),
        );

        let (key, _xs) = find_smallest_width_alignment(&g, &xss);
        assert_eq!(key, "ul");
    }

    // -----------------------------------------------------------------------
    // balance
    // -----------------------------------------------------------------------

    #[test]
    fn balance_single_node_shared_median() {
        let mut xss: XssMap = HashMap::new();
        xss.insert("ul".to_string(), HashMap::from([("a".to_string(), 0.0)]));
        xss.insert("ur".to_string(), HashMap::from([("a".to_string(), 100.0)]));
        xss.insert("dl".to_string(), HashMap::from([("a".to_string(), 100.0)]));
        xss.insert("dr".to_string(), HashMap::from([("a".to_string(), 200.0)]));

        let result = balance(&xss, None);
        assert_eq!(result["a"], 100.0);
    }

    #[test]
    fn balance_single_node_average_of_different_medians() {
        let mut xss: XssMap = HashMap::new();
        xss.insert("ul".to_string(), HashMap::from([("a".to_string(), 0.0)]));
        xss.insert("ur".to_string(), HashMap::from([("a".to_string(), 75.0)]));
        xss.insert("dl".to_string(), HashMap::from([("a".to_string(), 125.0)]));
        xss.insert("dr".to_string(), HashMap::from([("a".to_string(), 200.0)]));

        let result = balance(&xss, None);
        assert_eq!(result["a"], 100.0);
    }

    #[test]
    fn balance_multiple_nodes() {
        let mut xss: XssMap = HashMap::new();
        xss.insert(
            "ul".to_string(),
            HashMap::from([("a".to_string(), 0.0), ("b".to_string(), 50.0)]),
        );
        xss.insert(
            "ur".to_string(),
            HashMap::from([("a".to_string(), 75.0), ("b".to_string(), 0.0)]),
        );
        xss.insert(
            "dl".to_string(),
            HashMap::from([("a".to_string(), 125.0), ("b".to_string(), 60.0)]),
        );
        xss.insert(
            "dr".to_string(),
            HashMap::from([("a".to_string(), 200.0), ("b".to_string(), 75.0)]),
        );

        let result = balance(&xss, None);
        assert_eq!(result["a"], 100.0);
        assert_eq!(result["b"], 55.0);
    }

    // -----------------------------------------------------------------------
    // positionX (integration)
    // -----------------------------------------------------------------------

    #[test]
    fn px_single_node_at_origin() {
        let mut g = new_graph();
        g.set_node("a".to_string(), Some(node_w(0, 0, 100.0)));
        let result = position_x(&g);
        assert_eq!(result["a"], 0.0);
    }

    #[test]
    fn px_single_node_block_at_origin() {
        let mut g = new_graph();
        g.set_default_edge_label(|_| EdgeLabel::default());
        g.set_node("a".to_string(), Some(node_w(0, 0, 100.0)));
        g.set_node("b".to_string(), Some(node_w(1, 0, 100.0)));
        g.set_edge("a", "b", None, None);
        let result = position_x(&g);
        assert_eq!(result["a"], 0.0);
        assert_eq!(result["b"], 0.0);
    }

    #[test]
    fn px_single_node_block_at_origin_different_sizes() {
        let mut g = new_graph();
        g.set_default_edge_label(|_| EdgeLabel::default());
        g.set_node("a".to_string(), Some(node_w(0, 0, 40.0)));
        g.set_node("b".to_string(), Some(node_w(1, 0, 500.0)));
        g.set_node("c".to_string(), Some(node_w(2, 0, 20.0)));
        g.set_path(&["a", "b", "c"], None);
        let result = position_x(&g);
        assert_eq!(result["a"], 0.0);
        assert_eq!(result["b"], 0.0);
        assert_eq!(result["c"], 0.0);
    }

    #[test]
    fn px_centers_node_if_predecessor_of_two_same_sized() {
        let mut g = new_graph();
        g.set_default_edge_label(|_| EdgeLabel::default());
        g.graph_label_mut::<GraphLabel>().unwrap().nodesep = 10.0;
        g.set_node("a".to_string(), Some(node_w(0, 0, 20.0)));
        g.set_node("b".to_string(), Some(node_w(1, 0, 50.0)));
        g.set_node("c".to_string(), Some(node_w(1, 1, 50.0)));
        g.set_edge("a", "b", None, None);
        g.set_edge("a", "c", None, None);

        let pos = position_x(&g);
        let a = pos["a"];
        assert_eq!(pos["b"], a - (25.0 + 5.0));
        assert_eq!(pos["c"], a + (25.0 + 5.0));
    }

    #[test]
    fn px_shifts_blocks_on_both_sides_of_aligned_block() {
        let mut g = new_graph();
        g.set_default_edge_label(|_| EdgeLabel::default());
        g.graph_label_mut::<GraphLabel>().unwrap().nodesep = 10.0;
        g.set_node("a".to_string(), Some(node_w(0, 0, 50.0)));
        g.set_node("b".to_string(), Some(node_w(0, 1, 60.0)));
        g.set_node("c".to_string(), Some(node_w(1, 0, 70.0)));
        g.set_node("d".to_string(), Some(node_w(1, 1, 80.0)));
        g.set_edge("b", "c", None, None);

        let pos = position_x(&g);
        let b = pos["b"];
        let c = b;
        assert_eq!(pos["a"], b - 60.0 / 2.0 - 10.0 - 50.0 / 2.0);
        assert_eq!(pos["b"], b);
        assert_eq!(pos["c"], c);
        assert_eq!(pos["d"], c + 70.0 / 2.0 + 10.0 + 80.0 / 2.0);
    }

    #[test]
    fn px_aligns_inner_segments() {
        let mut g = new_graph();
        g.set_default_edge_label(|_| EdgeLabel::default());
        g.graph_label_mut::<GraphLabel>().unwrap().nodesep = 10.0;
        g.graph_label_mut::<GraphLabel>().unwrap().edgesep = 10.0;
        g.set_node("a".to_string(), Some(node_wd(0, 0, 50.0, "true")));
        g.set_node("b".to_string(), Some(node_w(0, 1, 60.0)));
        g.set_node("c".to_string(), Some(node_w(1, 0, 70.0)));
        g.set_node("d".to_string(), Some(node_wd(1, 1, 80.0, "true")));
        g.set_edge("b", "c", None, None);
        g.set_edge("a", "d", None, None);

        let pos = position_x(&g);
        let a = pos["a"];
        let d = a;
        assert_eq!(pos["a"], a);
        assert_eq!(pos["b"], a + 50.0 / 2.0 + 10.0 + 60.0 / 2.0);
        assert_eq!(pos["c"], d - 70.0 / 2.0 - 10.0 - 80.0 / 2.0);
        assert_eq!(pos["d"], d);
    }
}
