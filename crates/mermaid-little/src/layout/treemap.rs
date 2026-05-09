//! Treemap layout — straight port of the d3 pipeline.
//!
//! Upstream renderer wires:
//!
//! ```text
//! hierarchy(root).sum(v => v).sort((a,b) => b.value - a.value)
//!   .treemap()
//!     .size([W, H])
//!     .paddingTop(d => d.children?.length ? HEADER+PAD : 0)
//!     .paddingLeft/Right/Bottom(d => d.children?.length ? PAD : 0)
//!     .paddingInner(PAD)
//!     .round(true)
//! ```
//!
//! We therefore need three d3 components:
//!   * `hierarchy.sum` + `.sort` — recursive accumulation + in-place
//!     descending-value sort.
//!   * `treemap.positionNode` — padding stack walker.
//!   * `squarify` + `slice` / `dice` — the actual squarified layout.
//!
//! The algorithm is transliterated from
//! `tests/support/node_modules/d3-hierarchy/src/treemap/*.js`, so our
//! output matches d3's `rects`/round coordinates exactly.
//!
//! Portions adapted from mermaid-rs-renderer
//! (https://github.com/1jehuang/mermaid-rs-renderer), MIT license —
//! referenced for the squarified-treemap shape, but the actual d3
//! squarify algorithm is re-ported here because mmdr's version uses a
//! simpler alternate-axis slice heuristic.

use crate::error::Result;
use crate::model::treemap::{NodeId, TreemapDiagram, TreemapNodeKind};
use crate::theme::ThemeVariables;

/// Upstream: `DEFAULT_INNER_PADDING` / `SECTION_INNER_PADDING` = 10.
pub const SECTION_INNER_PADDING: f64 = 10.0;
/// Upstream: `SECTION_HEADER_HEIGHT` = 25.
pub const SECTION_HEADER_HEIGHT: f64 = 25.0;
/// Upstream default canvas — `defaultConfig.treemap.nodeWidth` is 100
/// and `nodeHeight` is 40, each scaled by `SECTION_INNER_PADDING`. The
/// `config.nodeWidth ? ... : 960` ternary in renderer.ts is therefore
/// dead code under the default config; every fixture shipped by
/// upstream renders at 1000 × 400.
pub const DEFAULT_WIDTH: f64 = 1000.0;
pub const DEFAULT_HEIGHT: f64 = 400.0;
/// D3's squarify target aspect ratio — the golden ratio.
const PHI: f64 = 1.618_033_988_749_895_f64;

/// A flattened hierarchy node with coordinates assigned by treemap.
#[derive(Debug, Clone)]
pub struct LaidNode {
    pub id: NodeId,
    /// Depth in the synthetic-root hierarchy (root = 0, top-level
    /// diagram entries = 1, ...).
    pub depth: usize,
    pub parent: Option<NodeId>,
    /// Final (rounded) rectangle.
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    /// Summed value for branches, raw value for leaves.
    pub value: f64,
    /// In-order index among leaves (for branches: `None`).
    pub leaf_index: Option<usize>,
    /// In-order index among branches, including the synthetic root
    /// (which always gets index 0).
    pub section_index: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct TreemapLayout {
    /// Laid out nodes in d3 pre-order (eachBefore), starting with the
    /// synthetic root.
    pub nodes: Vec<LaidNode>,
    /// Total sum across all roots.
    pub total_value: f64,
    /// Canvas dimensions used. Matches the `treemap.size([dx, dy])` call.
    pub width: f64,
    pub height: f64,
}

pub fn layout(d: &TreemapDiagram, _theme: &ThemeVariables) -> Result<TreemapLayout> {
    let width = d
        .config
        .node_width
        .map(|n| n * SECTION_INNER_PADDING)
        .unwrap_or(DEFAULT_WIDTH);
    let height = d
        .config
        .node_height
        .map(|n| n * SECTION_INNER_PADDING)
        .unwrap_or(DEFAULT_HEIGHT);
    let padding = d.config.padding.unwrap_or(SECTION_INNER_PADDING);

    // Build a mutable `d3.hierarchy` mirror — an arena of `HNode`s with
    // rectangles + summed values. The synthetic root (name="") wraps
    // the outer nodes exactly the way upstream's `getRoot()` does.
    let mut arena: Vec<HNode> = Vec::with_capacity(d.nodes.len() + 1);
    let root_idx = 0usize;
    arena.push(HNode {
        src_id: None,
        parent: None,
        children: Vec::new(),
        value: 0.0,
        kind: TreemapNodeKind::Section,
        depth: 0,
        x0: 0.0,
        y0: 0.0,
        x1: width,
        y1: height,
    });

    // Recursively materialise: for each source outer node, walk its
    // subtree and append `HNode`s.
    fn recurse(
        src_nodes: &[crate::model::treemap::TreemapNode],
        src_ids: &[NodeId],
        arena: &mut Vec<HNode>,
        parent_arena_id: usize,
        depth: usize,
    ) {
        for &sid in src_ids {
            let src = &src_nodes[sid];
            let h_idx = arena.len();
            arena.push(HNode {
                src_id: Some(sid),
                parent: Some(parent_arena_id),
                children: Vec::new(),
                value: src.value.unwrap_or(0.0),
                kind: src.kind.clone(),
                depth,
                x0: 0.0,
                y0: 0.0,
                x1: 0.0,
                y1: 0.0,
            });
            arena[parent_arena_id].children.push(h_idx);
            if let Some(children) = src.children.as_ref() {
                recurse(src_nodes, children, arena, h_idx, depth + 1);
            }
        }
    }
    recurse(&d.nodes, &d.outer_nodes, &mut arena, root_idx, 1);

    // `sum`: leaves keep their value, branches sum children.
    fn sum(arena: &mut [HNode], idx: usize) -> f64 {
        if arena[idx].children.is_empty() {
            return arena[idx].value;
        }
        let mut total = 0.0;
        let children = arena[idx].children.clone();
        for c in children {
            total += sum(arena, c);
        }
        arena[idx].value = total;
        total
    }
    sum(&mut arena, root_idx);

    // `sort((a,b) => b.value - a.value)` — descending, stable. d3
    // applies sort bottom-up (children first) so that the pre-order
    // walk produces descending siblings at every level.
    fn sort_recursive(arena: &mut Vec<HNode>, idx: usize) {
        let children = arena[idx].children.clone();
        for &c in &children {
            sort_recursive(arena, c);
        }
        // Stable descending sort by value.
        let mut sorted = children.clone();
        sorted.sort_by(|a, b| {
            let av = arena[*a].value;
            let bv = arena[*b].value;
            bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
        });
        arena[idx].children = sorted;
    }
    sort_recursive(&mut arena, root_idx);

    // Now: the d3 treemap pipeline. root.x0=0,y0=0,x1=W,y1=H, then
    // `root.eachBefore(positionNode)`, then `root.eachBefore(roundNode)`.
    position_tree(&mut arena, root_idx, padding);
    round_tree(&mut arena, root_idx);

    // Enumeration — d3 quirk:
    //   * `treemapData.descendants()` is breadth-first (the iterator in
    //     `node_modules/d3-hierarchy/src/hierarchy/iterator.js` uses a
    //     FIFO). Upstream `branchNodes = descendants().filter(…)` therefore
    //     assigns `section{N}` indices in BFS order.
    //   * `treemapData.leaves()` is pre-order depth-first (it filters via
    //     `eachBefore`). Upstream `leafNodes = treemapData.leaves()` thus
    //     assigns `leaf{N}` indices in pre-order.
    //   * Upstream emits ALL sections first (BFS), then ALL leaves
    //     (pre-order). We mirror that emission order here.
    let mut out_nodes: Vec<LaidNode> = Vec::new();

    // BFS over the arena once → section index table.
    let mut section_index_by_arena: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    {
        let mut queue: std::collections::VecDeque<usize> = Default::default();
        queue.push_back(root_idx);
        let mut counter = 0usize;
        while let Some(idx) = queue.pop_front() {
            if !arena[idx].children.is_empty() {
                section_index_by_arena.insert(idx, counter);
                counter += 1;
            }
            for &c in &arena[idx].children {
                queue.push_back(c);
            }
        }
    }

    // Pre-order leaf index table.
    let mut leaf_index_by_arena: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    {
        let mut counter = 0usize;
        fn each_before_leaves(
            arena: &[HNode],
            idx: usize,
            counter: &mut usize,
            map: &mut std::collections::HashMap<usize, usize>,
        ) {
            if arena[idx].children.is_empty() {
                map.insert(idx, *counter);
                *counter += 1;
            } else {
                for &c in &arena[idx].children {
                    each_before_leaves(arena, c, counter, map);
                }
            }
        }
        each_before_leaves(&arena, root_idx, &mut counter, &mut leaf_index_by_arena);
    }

    // Emit sections first in BFS order, then leaves in pre-order —
    // matching upstream's render sequence (sections-append before
    // leaves-append).
    {
        let mut queue: std::collections::VecDeque<usize> = Default::default();
        queue.push_back(root_idx);
        while let Some(idx) = queue.pop_front() {
            if !arena[idx].children.is_empty() {
                let n = &arena[idx];
                out_nodes.push(LaidNode {
                    id: n.src_id.unwrap_or(usize::MAX),
                    depth: n.depth,
                    parent: n.parent.and_then(|p| arena[p].src_id),
                    x0: n.x0,
                    y0: n.y0,
                    x1: n.x1,
                    y1: n.y1,
                    value: n.value,
                    leaf_index: None,
                    section_index: section_index_by_arena.get(&idx).copied(),
                });
            }
            for &c in &arena[idx].children {
                queue.push_back(c);
            }
        }
    }

    {
        fn each_before_emit(
            arena: &[HNode],
            idx: usize,
            out: &mut Vec<LaidNode>,
            leaf_map: &std::collections::HashMap<usize, usize>,
        ) {
            if arena[idx].children.is_empty() {
                let n = &arena[idx];
                out.push(LaidNode {
                    id: n.src_id.unwrap_or(usize::MAX),
                    depth: n.depth,
                    parent: n.parent.and_then(|p| arena[p].src_id),
                    x0: n.x0,
                    y0: n.y0,
                    x1: n.x1,
                    y1: n.y1,
                    value: n.value,
                    leaf_index: leaf_map.get(&idx).copied(),
                    section_index: None,
                });
            } else {
                for &c in &arena[idx].children {
                    each_before_emit(arena, c, out, leaf_map);
                }
            }
        }
        each_before_emit(&arena, root_idx, &mut out_nodes, &leaf_index_by_arena);
    }

    Ok(TreemapLayout {
        nodes: out_nodes,
        total_value: arena[root_idx].value,
        width,
        height,
    })
}

// ---------------------------------------------------------------------------------------------
// Arena-based hierarchy mirror.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct HNode {
    src_id: Option<NodeId>,
    parent: Option<usize>,
    children: Vec<usize>,
    value: f64,
    #[allow(dead_code)]
    kind: TreemapNodeKind,
    depth: usize,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

fn padding_top(arena: &[HNode], idx: usize) -> f64 {
    if arena[idx].children.is_empty() {
        0.0
    } else {
        SECTION_HEADER_HEIGHT + SECTION_INNER_PADDING
    }
}
fn padding_side(arena: &[HNode], idx: usize) -> f64 {
    if arena[idx].children.is_empty() {
        0.0
    } else {
        SECTION_INNER_PADDING
    }
}

/// d3 `positionNode` recursively — writing rectangles back into arena.
/// `padding_stack[depth]` holds `paddingInner(parent)/2` to match the
/// stack d3 builds in its `eachBefore(positionNode)` walk.
fn position_tree(arena: &mut [HNode], idx: usize, padding_inner: f64) {
    let mut stack = vec![0.0_f64];
    position_node(arena, idx, &mut stack, padding_inner);
}

fn position_node(arena: &mut [HNode], idx: usize, stack: &mut Vec<f64>, padding_inner: f64) {
    let depth = arena[idx].depth;
    let p = stack.get(depth).copied().unwrap_or(0.0);
    let mut x0 = arena[idx].x0 + p;
    let mut y0 = arena[idx].y0 + p;
    let mut x1 = arena[idx].x1 - p;
    let mut y1 = arena[idx].y1 - p;
    if x1 < x0 {
        x0 = (x0 + x1) / 2.0;
        x1 = x0;
    }
    if y1 < y0 {
        y0 = (y0 + y1) / 2.0;
        y1 = y0;
    }
    arena[idx].x0 = x0;
    arena[idx].y0 = y0;
    arena[idx].x1 = x1;
    arena[idx].y1 = y1;

    if !arena[idx].children.is_empty() {
        let p_new = padding_inner / 2.0;
        while stack.len() <= depth + 1 {
            stack.push(0.0);
        }
        stack[depth + 1] = p_new;
        let mut tx0 = x0 + padding_side(arena, idx) - p_new;
        let mut ty0 = y0 + padding_top(arena, idx) - p_new;
        let mut tx1 = x1 - padding_side(arena, idx) + p_new;
        let mut ty1 = y1 - padding_side(arena, idx) + p_new;
        if tx1 < tx0 {
            tx0 = (tx0 + tx1) / 2.0;
            tx1 = tx0;
        }
        if ty1 < ty0 {
            ty0 = (ty0 + ty1) / 2.0;
            ty1 = ty0;
        }
        squarify(arena, idx, tx0, ty0, tx1, ty1);
        // Recurse into children pre-order.
        let children = arena[idx].children.clone();
        for c in children {
            position_node(arena, c, stack, padding_inner);
        }
    }
}

fn round_tree(arena: &mut [HNode], idx: usize) {
    arena[idx].x0 = arena[idx].x0.round();
    arena[idx].y0 = arena[idx].y0.round();
    arena[idx].x1 = arena[idx].x1.round();
    arena[idx].y1 = arena[idx].y1.round();
    let children = arena[idx].children.clone();
    for c in children {
        round_tree(arena, c);
    }
}

// ---------------------------------------------------------------------------------------------
// squarify / dice / slice — transliterated from d3-hierarchy/src/treemap/*.js.
// ---------------------------------------------------------------------------------------------

fn squarify(arena: &mut [HNode], parent: usize, mut x0: f64, mut y0: f64, x1: f64, y1: f64) {
    let nodes = arena[parent].children.clone();
    let n = nodes.len();
    if n == 0 {
        return;
    }
    let mut i0 = 0usize;
    let mut i1;
    // d3 mutates `parent.value` inside the tile loop (subtracting
    // `sumValue` each pass). We mirror that with a local so the arena
    // parent node keeps its accumulated value for downstream stages.
    let mut value = arena[parent].value;
    let ratio = PHI;

    while i0 < n {
        let dx = x1 - x0;
        let dy = y1 - y0;

        // Find the next non-empty node.
        i1 = i0;
        let mut sum_value;
        loop {
            sum_value = arena[nodes[i1]].value;
            i1 += 1;
            if sum_value != 0.0 || i1 >= n {
                break;
            }
        }
        let mut min_value = sum_value;
        let mut max_value = sum_value;
        let alpha = {
            let m = if dx == 0.0 && dy == 0.0 {
                1.0
            } else {
                (dy / dx).max(dx / dy)
            };
            m / (value * ratio)
        };
        let mut beta = sum_value * sum_value * alpha;
        let mut min_ratio = (max_value / beta).max(beta / min_value);

        // Keep adding nodes while the aspect ratio maintains or improves.
        while i1 < n {
            let node_value = arena[nodes[i1]].value;
            sum_value += node_value;
            if node_value < min_value {
                min_value = node_value;
            }
            if node_value > max_value {
                max_value = node_value;
            }
            beta = sum_value * sum_value * alpha;
            let new_ratio = (max_value / beta).max(beta / min_value);
            if new_ratio > min_ratio {
                sum_value -= node_value;
                break;
            }
            min_ratio = new_ratio;
            i1 += 1;
        }

        let dice = dx < dy;
        let row_children: Vec<usize> = nodes[i0..i1].to_vec();
        if dice {
            // D3 mutates `y0` inside the function call via an argument
            // expression; we compute the new y0 first, then pass it.
            let new_y0 = if value != 0.0 {
                y0 + dy * sum_value / value
            } else {
                y1
            };
            tile_dice(arena, &row_children, sum_value, x0, y0, x1, new_y0);
            if value != 0.0 {
                y0 = new_y0;
            }
        } else {
            let new_x0 = if value != 0.0 {
                x0 + dx * sum_value / value
            } else {
                x1
            };
            tile_slice(arena, &row_children, sum_value, x0, y0, new_x0, y1);
            if value != 0.0 {
                x0 = new_x0;
            }
        }

        value -= sum_value;
        i0 = i1;
    }
}

/// `treemapDice`: lay nodes out side-by-side across `x0..x1`, each
/// taking the full `y0..y1` height.
fn tile_dice(arena: &mut [HNode], nodes: &[usize], value: f64, x0: f64, y0: f64, x1: f64, y1: f64) {
    let k = if value != 0.0 { (x1 - x0) / value } else { 0.0 };
    let mut cursor = x0;
    for &idx in nodes {
        let v = arena[idx].value;
        arena[idx].y0 = y0;
        arena[idx].y1 = y1;
        arena[idx].x0 = cursor;
        cursor += v * k;
        arena[idx].x1 = cursor;
    }
}

/// `treemapSlice`: lay nodes out stacked top-to-bottom across
/// `y0..y1`, each taking the full `x0..x1` width.
fn tile_slice(
    arena: &mut [HNode],
    nodes: &[usize],
    value: f64,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) {
    let k = if value != 0.0 { (y1 - y0) / value } else { 0.0 };
    let mut cursor = y0;
    for &idx in nodes {
        let v = arena[idx].value;
        arena[idx].x0 = x0;
        arena[idx].x1 = x1;
        arena[idx].y0 = cursor;
        cursor += v * k;
        arena[idx].y1 = cursor;
    }
}
