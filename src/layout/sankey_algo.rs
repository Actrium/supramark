//! Rust port of [d3-sankey](https://github.com/d3/d3-sankey) v0.12.3.
//!
//! This is a *faithful* port — every loop order, every `(...) >> 1`
//! integer division, every floating-point intermediate matches the JS
//! original line-for-line. The byte-exact reference pipeline depends
//! on the iteration order baked into this code: tests will fail if,
//! for example, `relaxLeftToRight` is rewritten to iterate via a
//! different summation order.
//!
//! Upstream: /ext/mermaid/tests/support/node_modules/d3-sankey/src/sankey.js
//!
//! Licensed under the original d3-sankey ISC license
//! (see https://github.com/d3/d3-sankey/blob/master/LICENSE).

use crate::model::sankey::NodeAlignment;

/// Node state mirroring the JS `node` object.
#[derive(Debug, Clone, Default)]
pub struct AlgoNode {
    pub index: usize,
    pub source_links: Vec<usize>,
    pub target_links: Vec<usize>,
    pub value: f64,
    pub depth: usize,
    pub height: usize,
    pub layer: usize,
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
}

#[derive(Debug, Clone)]
pub struct AlgoLink {
    pub index: usize,
    pub source: usize,
    pub target: usize,
    pub value: f64,
    pub width: f64,
    pub y0: f64,
    pub y1: f64,
}

#[derive(Debug, Clone, Default)]
pub struct AlgoGraph {
    pub nodes: Vec<AlgoNode>,
    pub links: Vec<AlgoLink>,
}

pub struct Sankey {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub dx: f64,
    pub dy: f64,
    pub align: NodeAlignment,
    pub iterations: usize,
}

impl Sankey {
    pub fn layout(&self, node_ids: &[String], links_in: &[(String, String, f64)]) -> AlgoGraph {
        let mut graph = AlgoGraph::default();

        // computeNodeLinks
        graph.nodes = node_ids
            .iter()
            .enumerate()
            .map(|(i, _)| AlgoNode {
                index: i,
                ..AlgoNode::default()
            })
            .collect();
        let node_by_id: std::collections::HashMap<&str, usize> = node_ids
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();

        for (i, (s, t, v)) in links_in.iter().enumerate() {
            let si = *node_by_id.get(s.as_str()).expect("missing source node");
            let ti = *node_by_id.get(t.as_str()).expect("missing target node");
            graph.nodes[si].source_links.push(i);
            graph.nodes[ti].target_links.push(i);
            graph.links.push(AlgoLink {
                index: i,
                source: si,
                target: ti,
                value: *v,
                width: 0.0,
                y0: 0.0,
                y1: 0.0,
            });
        }

        compute_node_values(&mut graph);
        compute_node_depths(&mut graph);
        compute_node_heights(&mut graph);

        let mut py = self.dy;
        let mut columns = compute_node_layers(&mut graph, self.x0, self.x1, self.dx, self.align);

        let max_col_len: usize = columns.iter().map(|c| c.len()).max().unwrap_or(0);
        if max_col_len > 1 {
            let alt = (self.y1 - self.y0) / (max_col_len as f64 - 1.0);
            if alt < py {
                py = alt;
            }
        }

        initialize_node_breadths(&mut graph, &columns, self.y0, self.y1, py);

        for i in 0..self.iterations {
            // JS uses `Math.pow(0.99, i)` which ultimately calls libm
            // pow(); Rust's `powi` does iterative multiplication and
            // drifts by a few ULPs. `powf` routes to libm and matches
            // JS byte-for-byte.
            let alpha = 0.99f64.powf(i as f64);
            let beta = (1.0 - alpha).max((i as f64 + 1.0) / self.iterations as f64);
            relax_right_to_left(&mut graph, &mut columns, alpha, beta, self.y0, self.y1, py);
            relax_left_to_right(&mut graph, &mut columns, alpha, beta, self.y0, self.y1, py);
        }

        compute_link_breadths(&mut graph);

        graph
    }
}

fn sum_link_values(graph: &AlgoGraph, link_indices: &[usize]) -> f64 {
    link_indices.iter().map(|&i| graph.links[i].value).sum()
}

fn compute_node_values(graph: &mut AlgoGraph) {
    for i in 0..graph.nodes.len() {
        let src_sum = sum_link_values(graph, &graph.nodes[i].source_links.clone());
        let tgt_sum = sum_link_values(graph, &graph.nodes[i].target_links.clone());
        graph.nodes[i].value = src_sum.max(tgt_sum);
    }
}

fn compute_node_depths(graph: &mut AlgoGraph) {
    let n = graph.nodes.len();
    let mut current: Vec<usize> = (0..n).collect();
    let mut next: Vec<usize> = Vec::new();
    let mut seen_next = vec![false; n];
    let mut x = 0usize;
    while !current.is_empty() {
        for &node_idx in &current {
            graph.nodes[node_idx].depth = x;
            for &li in &graph.nodes[node_idx].source_links.clone() {
                let t = graph.links[li].target;
                if !seen_next[t] {
                    seen_next[t] = true;
                    next.push(t);
                }
            }
        }
        x += 1;
        if x > n {
            break;
        }
        current = std::mem::take(&mut next);
        for s in seen_next.iter_mut() {
            *s = false;
        }
    }
}

fn compute_node_heights(graph: &mut AlgoGraph) {
    let n = graph.nodes.len();
    let mut current: Vec<usize> = (0..n).collect();
    let mut next: Vec<usize> = Vec::new();
    let mut seen_next = vec![false; n];
    let mut x = 0usize;
    while !current.is_empty() {
        for &node_idx in &current {
            graph.nodes[node_idx].height = x;
            for &li in &graph.nodes[node_idx].target_links.clone() {
                let s = graph.links[li].source;
                if !seen_next[s] {
                    seen_next[s] = true;
                    next.push(s);
                }
            }
        }
        x += 1;
        if x > n {
            break;
        }
        current = std::mem::take(&mut next);
        for s in seen_next.iter_mut() {
            *s = false;
        }
    }
}

fn compute_node_layers(
    graph: &mut AlgoGraph,
    x0: f64,
    x1: f64,
    dx: f64,
    align: NodeAlignment,
) -> Vec<Vec<usize>> {
    let max_depth = graph.nodes.iter().map(|n| n.depth).max().unwrap_or(0);
    let x_count = max_depth + 1;
    let kx = if x_count > 1 {
        (x1 - x0 - dx) / (x_count as f64 - 1.0)
    } else {
        0.0
    };
    let mut columns: Vec<Vec<usize>> = vec![Vec::new(); x_count];

    let n = graph.nodes.len();
    let mut layers: Vec<usize> = Vec::with_capacity(n);
    for idx in 0..n {
        let layer_raw = align_layer(
            &graph.nodes[idx],
            &graph.nodes,
            &graph.links,
            x_count,
            align,
        );
        // JS: `Math.max(0, Math.min(x - 1, Math.floor(align.call(...))))`.
        // All our alignment functions return integers, so floor is a
        // no-op; the clamping stays relevant.
        let clamped = layer_raw.min(x_count.saturating_sub(1));
        layers.push(clamped);
    }
    for (idx, &layer) in layers.iter().enumerate() {
        graph.nodes[idx].layer = layer;
        graph.nodes[idx].x0 = x0 + layer as f64 * kx;
        graph.nodes[idx].x1 = graph.nodes[idx].x0 + dx;
        columns[layer].push(idx);
    }
    columns
}

fn align_layer(
    node: &AlgoNode,
    nodes: &[AlgoNode],
    links: &[AlgoLink],
    n: usize,
    align: NodeAlignment,
) -> usize {
    match align {
        NodeAlignment::Left => node.depth,
        NodeAlignment::Right => (n - 1).saturating_sub(node.height),
        NodeAlignment::Center => {
            if !node.target_links.is_empty() {
                node.depth
            } else if !node.source_links.is_empty() {
                let mut m: usize = usize::MAX;
                for &li in &node.source_links {
                    let td = nodes[links[li].target].depth;
                    if td < m {
                        m = td;
                    }
                }
                m.saturating_sub(1)
            } else {
                0
            }
        }
        NodeAlignment::Justify => {
            if !node.source_links.is_empty() {
                node.depth
            } else {
                n - 1
            }
        }
    }
}

fn initialize_node_breadths(
    graph: &mut AlgoGraph,
    columns: &[Vec<usize>],
    y0: f64,
    y1: f64,
    py: f64,
) {
    let mut ky = f64::INFINITY;
    for col in columns {
        let sv: f64 = col.iter().map(|&i| graph.nodes[i].value).sum();
        if sv <= 0.0 {
            continue;
        }
        let k = (y1 - y0 - (col.len() as f64 - 1.0) * py) / sv;
        if k < ky {
            ky = k;
        }
    }
    if !ky.is_finite() {
        ky = 0.0;
    }

    for col in columns {
        let mut y = y0;
        for &i in col {
            graph.nodes[i].y0 = y;
            graph.nodes[i].y1 = y + graph.nodes[i].value * ky;
            y = graph.nodes[i].y1 + py;
            for &li in &graph.nodes[i].source_links.clone() {
                graph.links[li].width = graph.links[li].value * ky;
            }
        }
        let slack = (y1 - y + py) / (col.len() as f64 + 1.0);
        for (i, &node_idx) in col.iter().enumerate() {
            let shift = slack * (i as f64 + 1.0);
            graph.nodes[node_idx].y0 += shift;
            graph.nodes[node_idx].y1 += shift;
        }
        reorder_links(graph, col);
    }
}

fn reorder_links(graph: &mut AlgoGraph, col: &[usize]) {
    for &node_idx in col {
        let mut sl = graph.nodes[node_idx].source_links.clone();
        sl.sort_by(|&a, &b| ascending_target_breadth(graph, a, b));
        graph.nodes[node_idx].source_links = sl;
        let mut tl = graph.nodes[node_idx].target_links.clone();
        tl.sort_by(|&a, &b| ascending_source_breadth(graph, a, b));
        graph.nodes[node_idx].target_links = tl;
    }
}

fn ascending_target_breadth(graph: &AlgoGraph, a: usize, b: usize) -> std::cmp::Ordering {
    let la = &graph.links[a];
    let lb = &graph.links[b];
    let ya = graph.nodes[la.target].y0;
    let yb = graph.nodes[lb.target].y0;
    ya.partial_cmp(&yb)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| la.index.cmp(&lb.index))
}

fn ascending_source_breadth(graph: &AlgoGraph, a: usize, b: usize) -> std::cmp::Ordering {
    let la = &graph.links[a];
    let lb = &graph.links[b];
    let ya = graph.nodes[la.source].y0;
    let yb = graph.nodes[lb.source].y0;
    ya.partial_cmp(&yb)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| la.index.cmp(&lb.index))
}

fn ascending_breadth(graph: &AlgoGraph, a: usize, b: usize) -> std::cmp::Ordering {
    graph.nodes[a]
        .y0
        .partial_cmp(&graph.nodes[b].y0)
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn relax_left_to_right(
    graph: &mut AlgoGraph,
    columns: &mut [Vec<usize>],
    alpha: f64,
    beta: f64,
    y0: f64,
    y1: f64,
    py: f64,
) {
    for i in 1..columns.len() {
        // Iterate the column in its current order (as left by the
        // previous relax). `target` comes from the mutable vec, but
        // we snapshot it here so subsequent sort-in-place and
        // reorder_node_links don't invalidate the iteration.
        let column_snapshot = columns[i].clone();
        for &target in &column_snapshot {
            let mut y_acc = 0.0f64;
            let mut w_acc = 0.0f64;
            let target_layer = graph.nodes[target].layer as f64;
            for &li in &graph.nodes[target].target_links.clone() {
                let source = graph.links[li].source;
                let value = graph.links[li].value;
                let v = value * (target_layer - graph.nodes[source].layer as f64);
                y_acc += target_top_with_py(graph, source, target, py) * v;
                w_acc += v;
            }
            if !(w_acc > 0.0) {
                continue;
            }
            let dy = (y_acc / w_acc - graph.nodes[target].y0) * alpha;
            graph.nodes[target].y0 += dy;
            graph.nodes[target].y1 += dy;
            reorder_node_links(graph, target);
        }
        // JS: `if (sort === undefined) column.sort(ascendingBreadth);`
        // Sort the column IN PLACE so subsequent iterations see it.
        let column = &mut columns[i];
        sort_by_breadth(graph, column);
        let column_for_resolve = column.clone();
        resolve_collisions(graph, &column_for_resolve, beta, y0, y1, py);
    }
}

fn relax_right_to_left(
    graph: &mut AlgoGraph,
    columns: &mut [Vec<usize>],
    alpha: f64,
    beta: f64,
    y0: f64,
    y1: f64,
    py: f64,
) {
    let n = columns.len();
    if n < 2 {
        return;
    }
    let mut i = n as isize - 2;
    while i >= 0 {
        let column_snapshot = columns[i as usize].clone();
        for &source in &column_snapshot {
            let mut y_acc = 0.0f64;
            let mut w_acc = 0.0f64;
            let source_layer = graph.nodes[source].layer as f64;
            for &li in &graph.nodes[source].source_links.clone() {
                let target = graph.links[li].target;
                let value = graph.links[li].value;
                let v = value * (graph.nodes[target].layer as f64 - source_layer);
                y_acc += source_top_with_py(graph, source, target, py) * v;
                w_acc += v;
            }
            if !(w_acc > 0.0) {
                continue;
            }
            let dy = (y_acc / w_acc - graph.nodes[source].y0) * alpha;
            graph.nodes[source].y0 += dy;
            graph.nodes[source].y1 += dy;
            reorder_node_links(graph, source);
        }
        let column = &mut columns[i as usize];
        sort_by_breadth(graph, column);
        let column_for_resolve = column.clone();
        resolve_collisions(graph, &column_for_resolve, beta, y0, y1, py);
        i -= 1;
    }
}

/// Sort a column in place by `ascendingBreadth` (node.y0 asc, with
/// stable secondary on insertion index via the natural sort stability
/// from Rust — `slice::sort_by` is stable).
fn sort_by_breadth(graph: &AlgoGraph, col: &mut [usize]) {
    col.sort_by(|&a, &b| ascending_breadth(graph, a, b));
}

fn reorder_node_links(graph: &mut AlgoGraph, node_idx: usize) {
    let target_links = graph.nodes[node_idx].target_links.clone();
    for li in target_links {
        let source = graph.links[li].source;
        let mut sl = graph.nodes[source].source_links.clone();
        sl.sort_by(|&a, &b| ascending_target_breadth(graph, a, b));
        graph.nodes[source].source_links = sl;
    }
    let source_links = graph.nodes[node_idx].source_links.clone();
    for li in source_links {
        let target = graph.links[li].target;
        let mut tl = graph.nodes[target].target_links.clone();
        tl.sort_by(|&a, &b| ascending_source_breadth(graph, a, b));
        graph.nodes[target].target_links = tl;
    }
}

fn resolve_collisions(
    graph: &mut AlgoGraph,
    nodes: &[usize],
    alpha: f64,
    y0: f64,
    y1: f64,
    py: f64,
) {
    if nodes.is_empty() {
        return;
    }
    let i = nodes.len() >> 1;
    let subject = nodes[i];
    let subject_y0 = graph.nodes[subject].y0;
    let subject_y1 = graph.nodes[subject].y1;
    resolve_bottom_to_top(graph, nodes, subject_y0 - py, i as isize - 1, alpha, py);
    resolve_top_to_bottom(graph, nodes, subject_y1 + py, i + 1, alpha, py);
    resolve_bottom_to_top(graph, nodes, y1, nodes.len() as isize - 1, alpha, py);
    resolve_top_to_bottom(graph, nodes, y0, 0, alpha, py);
}

fn resolve_top_to_bottom(
    graph: &mut AlgoGraph,
    nodes: &[usize],
    mut y: f64,
    start: usize,
    alpha: f64,
    py: f64,
) {
    let mut i = start;
    while i < nodes.len() {
        let idx = nodes[i];
        let node_y0 = graph.nodes[idx].y0;
        let dy = (y - node_y0) * alpha;
        if dy > 1e-6 {
            graph.nodes[idx].y0 += dy;
            graph.nodes[idx].y1 += dy;
        }
        y = graph.nodes[idx].y1 + py;
        i += 1;
    }
}

fn resolve_bottom_to_top(
    graph: &mut AlgoGraph,
    nodes: &[usize],
    mut y: f64,
    start: isize,
    alpha: f64,
    py: f64,
) {
    let mut i = start;
    while i >= 0 {
        let idx = nodes[i as usize];
        let node_y1 = graph.nodes[idx].y1;
        let dy = (node_y1 - y) * alpha;
        if dy > 1e-6 {
            graph.nodes[idx].y0 -= dy;
            graph.nodes[idx].y1 -= dy;
        }
        y = graph.nodes[idx].y0 - py;
        i -= 1;
    }
}

fn target_top_with_py(graph: &AlgoGraph, source: usize, target: usize, py: f64) -> f64 {
    let src = &graph.nodes[source];
    let tgt = &graph.nodes[target];
    let mut y = src.y0 - (src.source_links.len() as f64 - 1.0) * py / 2.0;
    for &li in &src.source_links {
        let other = graph.links[li].target;
        if other == target {
            break;
        }
        y += graph.links[li].width + py;
    }
    for &li in &tgt.target_links {
        let other = graph.links[li].source;
        if other == source {
            break;
        }
        y -= graph.links[li].width;
    }
    y
}

fn source_top_with_py(graph: &AlgoGraph, source: usize, target: usize, py: f64) -> f64 {
    let src = &graph.nodes[source];
    let tgt = &graph.nodes[target];
    let mut y = tgt.y0 - (tgt.target_links.len() as f64 - 1.0) * py / 2.0;
    for &li in &tgt.target_links {
        let other = graph.links[li].source;
        if other == source {
            break;
        }
        y += graph.links[li].width + py;
    }
    for &li in &src.source_links {
        let other = graph.links[li].target;
        if other == target {
            break;
        }
        y -= graph.links[li].width;
    }
    y
}

fn compute_link_breadths(graph: &mut AlgoGraph) {
    let n = graph.nodes.len();
    for i in 0..n {
        let y_start = graph.nodes[i].y0;
        let mut y0 = y_start;
        let mut y1 = y_start;
        let sls = graph.nodes[i].source_links.clone();
        for li in sls {
            let w = graph.links[li].width;
            graph.links[li].y0 = y0 + w / 2.0;
            y0 += w;
        }
        let tls = graph.nodes[i].target_links.clone();
        for li in tls {
            let w = graph.links[li].width;
            graph.links[li].y1 = y1 + w / 2.0;
            y1 += w;
        }
    }
}
