//! Block diagram layout — own grid-based algorithm.
//!
//! Ports `packages/mermaid/src/diagrams/block/layout.ts` faithfully,
//! reproducing the two-pass sizing (bottom-up) + positioning (top-down)
//! that upstream performs inside the dagre-wrapper's non-dagre mode.
//!
//! ### Flat-bbox quirk
//!
//! jsdom's `SVGElement.getBBox` ignores transform and returns the union
//! of descendant geometry. The upstream sizing pass therefore reads a
//! **transform-less** bbox of the node's outer `<g>`, which combines:
//!
//!   * the inner `<rect>` (x = -(text_w + padding)/2, width = text_w + padding)
//!   * the `<foreignObject>` (x = 0, width = text_w)
//!
//! Union width = text_w + (text_w + padding)/2 = (3 * text_w + padding) / 2.
//! Same for height. This is implemented in [`sized_dims`].

use crate::error::Result;
use crate::font_metrics::text_width;
use crate::model::block::{BlockDiagram, BlockNode, BlockShape};
use crate::theme::ThemeVariables;

/// Upstream default (`config.block.padding`).
pub const PADDING: f64 = 8.0;
/// Height of a `<foreignObject>` holding a single line of 14 px sans-serif.
pub const LABEL_HEIGHT: f64 = 16.296875;

/// Per-node geometry after layout. `id` uniquely identifies a node
/// (matches `BlockNode::id`). Composites are included — their presence
/// in the final SVG depends on `shape`.
#[derive(Debug, Clone)]
pub struct NodeGeom {
    pub id: String,
    pub label: Option<String>,
    pub shape: BlockShape,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub text_width: f64,
    pub text_height: f64,
    pub width_in_columns: i64,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
    pub arrow_dirs: Vec<crate::model::block::ArrowDir>,
}

#[derive(Debug, Clone, Default)]
pub struct BlockLayout {
    pub nodes: Vec<NodeGeom>,
    /// `(x, y, width, height)` for the SVG viewBox (pre-margin).
    pub bounds: (f64, f64, f64, f64),
}

/// Layout entry point.
pub fn layout(d: &BlockDiagram, _theme: &ThemeVariables) -> Result<BlockLayout> {
    // Clone the AST into a mutable in-place tree we can decorate with
    // size/position. We carry the index of each sibling to preserve
    // declaration order.
    let mut tree = Tree::from_ast(&d.root);

    // Sizing pass: bottom-up. Leaves get `(sized_w, sized_h)` from the
    // flat-bbox formula; composites recurse first then compute their own.
    size_tree(&mut tree);

    // Position pass: top-down. Root starts at `(x=0, y=0)` with the
    // block.size.width/height already set, and children are placed
    // along rows using `calculateBlockPosition`.
    position_tree(&mut tree, 0.0, 0.0);

    // Walk bounds — skip the root wrapper. Seed with (0, 0, 0, 0)
    // (not "first child") to mirror upstream findBounds().
    let mut bx = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut init = true; // always "initialised" — seed is live.
    walk_bounds(&tree, &mut bx, &mut init, true);

    let mut nodes = Vec::new();
    collect_nodes(&tree, &mut nodes, true);
    Ok(BlockLayout {
        nodes,
        bounds: (bx.0, bx.1, bx.2 - bx.0, bx.3 - bx.1),
    })
}

// ─── Internal tree ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Tree {
    id: String,
    label: Option<String>,
    shape: BlockShape,
    width_in_columns: i64,
    columns: i64, // -1 = auto
    children: Vec<Tree>,
    classes: Vec<String>,
    styles: Vec<String>,
    arrow_dirs: Vec<crate::model::block::ArrowDir>,
    /// Computed size / position.
    width: f64,
    height: f64,
    x: f64,
    y: f64,
    text_w: f64,
    text_h: f64,
}

impl Tree {
    fn from_ast(node: &BlockNode) -> Self {
        Self {
            id: node.id.clone(),
            label: node.label.clone(),
            shape: node.shape,
            width_in_columns: node.width_in_columns.max(1),
            columns: node.columns.unwrap_or(-1),
            children: node.children.iter().map(Tree::from_ast).collect(),
            classes: node.classes.clone(),
            styles: node.styles.clone(),
            arrow_dirs: node.arrow_dirs.clone().unwrap_or_default(),
            width: 0.0,
            height: 0.0,
            x: 0.0,
            y: 0.0,
            text_w: 0.0,
            text_h: 0.0,
        }
    }

    fn is_leaf(&self) -> bool {
        !matches!(self.shape, BlockShape::Composite)
    }

    fn is_space(&self) -> bool {
        matches!(self.shape, BlockShape::Space)
    }
}

/// Compute `(width, height, text_w, text_h)` for a leaf's flat bbox and label
/// dimensions. Shape-specific `updateNodeBounds` behaviour varies:
///
/// * **Rect-family** (square, round, stadium, subroutine, cylinder, diamond,
///   hexagon, odd, lean_*, trapezoid, inv_trapezoid, rect_left_inv_arrow)
///   use the `<rect>`-union formula: `sized = (3 * text + padding) / 2`.
/// * **Circle / doubleCircle / ellipse** take the circle element's own
///   bbox which is `(text_w + padding, text_w + padding)` for circles.
/// * **BlockArrow** is handled separately.
fn sized_dims(
    label: &str,
    shape: BlockShape,
    arrow_dirs: &[crate::model::block::ArrowDir],
) -> (f64, f64, f64, f64) {
    // Empty labels still produce a label bbox with `height = LABEL_HEIGHT`
    // (jsdom's `measureTextBlock('')` returns `{width:0, height:lineHeight}`).
    // Width stays 0.
    let text_w = if label.is_empty() {
        0.0
    } else {
        text_width(label, "sans-serif", 14.0, false, false)
    };
    let text_h = LABEL_HEIGHT;
    // Upstream insertNode → shapeSvg.getBBox() returns the UNION of
    // the drawn element (e.g. `<rect>`, `<polygon>`, `<circle>`) and
    // the `<foreignObject>` sitting inside the `<g class="label">`.
    // Under jsdom's transform-blind shim, that union is:
    //   union.left = -drawn_w/2 (the drawn element is centered)
    //   union.right = max(drawn_w/2, text_w)     // foreignObject at x=0
    //   → width = text_w + drawn_w/2             (for text_w > drawn_w/2)
    // Likewise for height. Shape-specific `drawn_w` / `drawn_h`:
    let (sized_w, sized_h) = match shape {
        BlockShape::Circle | BlockShape::DoubleCircle => {
            // jsdom elementBBox on the node <g> returns the UNION of
            // the <circle> element and the <foreignObject> (transforms
            // are ignored by the shim).
            //   r = text_w/2 + PADDING/2
            //   circle_bbox: x ∈ [-r, r], y ∈ [-r, r]
            //   foreignObject_bbox: x ∈ [0, text_w], y ∈ [0, text_h]
            //   (label g transform "-bbox.width/2, -bbox.height/2" is ignored by jsdom)
            //   union width  = text_w + r = (3*text_w + PADDING) / 2
            //   union height = max(r, text_h) + r
            // For typical labels, text_h (16.296875) > r (e.g. 14.84 for short labels),
            // giving: union height = text_h + r = text_h + text_w/2 + PADDING/2
            let r = text_w / 2.0 + PADDING / 2.0;
            let w = (3.0 * text_w + PADDING) / 2.0;
            let h = r + r.max(text_h);
            (w, h)
        }
        BlockShape::Stadium => {
            // Stadium rect: w = text_w + (text_h+p)/4 + p; h = text_h + p.
            let drawn_w = text_w + (text_h + PADDING) / 4.0 + PADDING;
            let drawn_h = text_h + PADDING;
            (text_w + drawn_w / 2.0, text_h + drawn_h / 2.0)
        }
        // Polygon shapes using `insertPolygonShape2` with `translate(-w4/2, h3/2)`.
        // h3 = text_h + PADDING, w4 = text_w + PADDING.
        // jsdom getBBox of the parent <g> = union of polygon and label foreignObject:
        //   polygon y: [-h3, 0], foreignObject y: [0, text_h]
        //   → union height = text_h + h3 = 2*text_h + PADDING
        // x-range depends on shape-specific polygon x-offsets.
        BlockShape::LeanRight | BlockShape::Trapezoid | BlockShape::InvTrapezoid => {
            // lean_right2, trapezoid2, inv_trapezoid2 all have polygon x range
            // from -2*h3/6 to w4+2*h3/6  →  width = w4 + 2*h3/3.
            // Union with label [0, text_w]: x-min = -h3/3, x-max = w4+h3/3.
            let h3 = text_h + PADDING;
            let w4 = text_w + PADDING;
            let w = w4 + 2.0 * h3 / 3.0;
            let h = 2.0 * text_h + PADDING;
            (w, h)
        }
        BlockShape::LeanLeft => {
            // lean_left2 polygon x range: -h3/6 to w4+h3/6  →  width = w4 + h3/3.
            let h3 = text_h + PADDING;
            let w4 = text_w + PADDING;
            let w = w4 + h3 / 3.0;
            let h = 2.0 * text_h + PADDING;
            (w, h)
        }
        BlockShape::Diamond => {
            // question2: s2 = w4 + h3 = text_w + text_h + 2*PADDING.
            // Polygon x: [0, s2], y: [-s2, 0]. Label x: [0, text_w], y: [0, text_h].
            // Union: x [0, s2], y [-s2, text_h].
            // width = s2, height = s2 + text_h.
            let h3 = text_h + PADDING;
            let w4 = text_w + PADDING;
            let s2 = w4 + h3;
            let w = s2;
            let h = s2 + text_h;
            (w, h)
        }
        BlockShape::Hexagon => {
            // hexagon2: h3=text_h+P, m3=h3/4, w4=text_w+2*m3+P = text_w+h3/2+P.
            // Polygon x: [0, w4], y: [-h3, 0]. Label x: [0, text_w], y: [0, text_h].
            // Union: x [0, w4], y [-h3, text_h]. Width = w4, height = text_h + h3.
            let h3 = text_h + PADDING;
            let m3 = h3 / 4.0;
            let w4 = text_w + 2.0 * m3 + PADDING;
            let h = 2.0 * text_h + PADDING;
            (w4, h)
        }
        BlockShape::RectLeftInvArrow => {
            // rect_left_inv_arrow2: h3=text_h+P, w4=text_w+P.
            // Polygon x: [-h3/2, w4], y: [-h3, 0]. Label x: [0, text_w], y: [0, text_h].
            // Union: x [-h3/2, w4], y [-h3, text_h]. Width = w4+h3/2, height = text_h+h3.
            let h3 = text_h + PADDING;
            let w4 = text_w + PADDING;
            let w = w4 + h3 / 2.0;
            let h = 2.0 * text_h + PADDING;
            (w, h)
        }
        BlockShape::BlockArrow => {
            // block_arrow: height2 = text_h + 2*P, midpoint3 = height2/2.
            // width3 = text_w + 2*midpoint3 + P = text_w + height2 + P.
            // padding2 = P/2.
            // For most directions, polygon x: [0, width3], y: [-height2, 0].
            // For "x,y" (all 4 directions), x: [-P, width3+P], y: [-height2, P].
            // Label: [0, text_w] × [0, text_h].
            // Union height always = text_h + height2 = 2*text_h + 2*P (text_h > P).
            // Union width varies by direction:
            //   "x,y" → width3 + 2*P (wider by 2*P on each side → net +2*P total... wait)
            //   Actually: union x for "x,y": min(-P, 0)=-P to max(width3+P, text_w)=width3+P
            //   → width = width3 + 2*P
            //   Other: min(0, 0)=0 to max(width3, text_w)=width3 → width = width3
            use crate::model::block::ArrowDir;
            // "x,y" means both X and Y dirs present → expands to right+left+up+down.
            // X alone = right+left, Y alone = up+down. X+Y = all 4.
            let has_all_four = {
                let has_right =
                    arrow_dirs.contains(&ArrowDir::Right) || arrow_dirs.contains(&ArrowDir::X);
                let has_left =
                    arrow_dirs.contains(&ArrowDir::Left) || arrow_dirs.contains(&ArrowDir::X);
                let has_up =
                    arrow_dirs.contains(&ArrowDir::Up) || arrow_dirs.contains(&ArrowDir::Y);
                let has_down =
                    arrow_dirs.contains(&ArrowDir::Down) || arrow_dirs.contains(&ArrowDir::Y);
                has_right && has_left && has_up && has_down
            };
            // For "all 4 directions" (x+y), the polygon also extends in y:
            //   y range: -(height2 + 2*padding2) to 2*padding2 = -(height2+P) to P.
            //   union y: min(-(height2+P), 0)=-(height2+P) to max(P, text_h)=text_h.
            //   height = text_h + height2 + P = 2*text_h + 3*P.
            // For all other directions: y range -height2 to 0.
            //   union y: -height2 to text_h. height = text_h + height2 = 2*text_h + 2*P.
            let height2 = text_h + 2.0 * PADDING;
            let width3 = text_w + height2 + PADDING;
            let w = if has_all_four {
                width3 + 2.0 * PADDING
            } else {
                width3
            };
            let h = if has_all_four {
                2.0 * text_h + 3.0 * PADDING
            } else {
                2.0 * text_h + 2.0 * PADDING
            };
            (w, h)
        }
        _ => {
            // Plain rect (rect2): drawn_w = text_w + p, drawn_h = text_h + p.
            // rect: x = -text_w/2-P/2, y = -text_h/2-P/2, w = text_w+P, h = text_h+P.
            // Union with foreignObject at (0, 0, text_w, text_h):
            //   width = text_w - (-text_w/2-P/2) = 1.5*text_w + P/2 = (3*text_w + P) / 2
            //   height = text_h - (-text_h/2-P/2) = 1.5*text_h + P/2 = (3*text_h + P) / 2
            // Even when text_w=0 (empty label), text_h is still LABEL_HEIGHT.
            let w = (3.0 * text_w + PADDING) / 2.0;
            let h = (3.0 * text_h + PADDING) / 2.0;
            (w, h)
        }
    };
    (sized_w, sized_h, text_w, text_h)
}

/// Wrapper that mirrors upstream `setBlockSizes(block, db, siblingWidth, siblingHeight)`.
///
/// `sibling_w/h` represent the row-dimensions the parent has available
/// for this block after its own size-stretching pass. When the block's
/// own computed bounding box is smaller than the sibling envelope, both
/// the block and ALL its direct children are scaled to fill the row —
/// this is what produces the "D / E are 124.12px wide" effect in
/// fixture 15 where the inner composite inherits its siblings' width.
fn size_tree(node: &mut Tree) {
    size_tree_with_sibling(node, 0.0, 0.0);
}

fn size_tree_with_sibling(node: &mut Tree, sibling_w: f64, sibling_h: f64) {
    // 1. If leaf — compute own size (only on the initial pass where
    //    width hasn't been set yet). The "re-recurse with sibling
    //    dims" pass must NOT clobber widths that the parent already
    //    overrode to include widthInColumns multipliers.
    if node.is_leaf() {
        // First-pass initialisation — only runs while `width` is still
        // untouched. On the parent's re-recurse (with sibling envelope
        // dimensions) we preserve whatever width the parent already set
        // so widthInColumns multipliers and sibling-stretching survive.
        if node.width == 0.0 && node.height == 0.0 && node.text_h == 0.0 {
            if node.is_space() {
                // Space stays at (0, 0) — getMaxChildSize ignores it.
            } else {
                let label = node.label.as_deref().unwrap_or("");
                let (w, h, tw, th) = sized_dims(label, node.shape, &node.arrow_dirs);
                node.width = w;
                node.height = h;
                node.text_w = tw;
                node.text_h = th;
            }
        }
        let _ = sibling_w;
        let _ = sibling_h;
        return;
    }
    // 2. Composite: default size based on sibling envelope (if our own
    //    size hasn't been set yet). Recurse into children with
    //    siblingWidth/Height unset for the bottom-up pass.
    let cl = node.label.as_deref().unwrap_or("");
    node.text_w = if cl.is_empty() {
        0.0
    } else {
        text_width(cl, "sans-serif", 14.0, false, false)
    };
    node.text_h = LABEL_HEIGHT;
    if node.width == 0.0 {
        node.width = sibling_w;
        node.height = sibling_h;
    }
    for child in &mut node.children {
        size_tree_with_sibling(child, 0.0, 0.0);
    }
    // 3. maxChildSize: excludes spaces.
    let (mut max_w, mut max_h) = (0.0f64, 0.0f64);
    for child in &node.children {
        if child.is_space() {
            continue;
        }
        let wic = child.width_in_columns.max(1) as f64;
        if child.width > max_w {
            max_w = child.width / wic;
        }
        if child.height > max_h {
            max_h = child.height;
        }
    }
    // 4. Propagate to children: all children (including space) get the
    //    same row-height; widths multiply by widthInColumns + padding.
    //    Upstream's loop DOES write to space children, giving them the
    //    same cell-slot geometry as rendered nodes.
    for child in &mut node.children {
        let wic = child.width_in_columns.max(1) as f64;
        child.width = max_w * wic + PADDING * (wic - 1.0);
        child.height = max_h;
    }
    // 4b. Second recursion — pass maxWidth / maxHeight as sibling
    //     dimensions, letting composites stretch to fill their slot.
    for child in &mut node.children {
        size_tree_with_sibling(child, max_w, max_h);
    }
    // 5. Composite's own bounding box.
    //    Upstream quirk: `xSize` is `children.length` unless explicit
    //    `columns` is smaller than `numItems` (sum of widthInColumns).
    //    `ySize` derives from numItems / xSize — meaning widthInColumns>1
    //    slot overflow can inflate the row count even on a single
    //    visual row.
    let columns = node.columns;
    let num_items: i64 = node
        .children
        .iter()
        .map(|c| c.width_in_columns.max(1))
        .sum();
    let mut x_size = node.children.len() as i64;
    if columns > 0 && columns < num_items {
        x_size = columns;
    }
    if x_size == 0 {
        x_size = 1;
    }
    let y_size = div_ceil(num_items, x_size);
    let mut width = x_size as f64 * (max_w + PADDING) + PADDING;
    let mut height = y_size as f64 * (max_h + PADDING) + PADDING;

    // 6. If the sibling envelope is bigger than our natural size, grow
    //    to fill it — and re-stretch children to fit. This reproduces
    //    the "Detected too-small sibling" branch in upstream layout.ts.
    if width < sibling_w {
        width = sibling_w;
        height = sibling_h;
        let child_width = if x_size > 0 {
            (sibling_w - x_size as f64 * PADDING - PADDING) / x_size as f64
        } else {
            max_w
        };
        let child_height = if y_size > 0 {
            (sibling_h - y_size as f64 * PADDING - PADDING) / y_size as f64
        } else {
            max_h
        };
        for child in &mut node.children {
            let wic = child.width_in_columns.max(1) as f64;
            child.width = child_width * wic + PADDING * (wic - 1.0);
            child.height = child_height;
        }
    }
    // 7. If an explicit block.size (via block_beta `columns N`) is
    //    larger than computed width, grow the children too. Mirror
    //    upstream's final width < block.size.width branch.
    if width < node.width {
        width = node.width;
        let num = if columns > 0 {
            node.children.len().min(columns as usize) as f64
        } else {
            node.children.len() as f64
        };
        if num > 0.0 {
            let child_width = (width - num * PADDING - PADDING) / num;
            for child in &mut node.children {
                child.width = child_width;
            }
        }
    }
    node.width = width;
    node.height = height;
}

fn div_ceil(a: i64, b: i64) -> i64 {
    if b == 0 {
        return 0;
    }
    (a + b - 1) / b
}

/// Mirror of upstream `calculateBlockPosition`.
fn calc_block_position(columns: i64, position: i64) -> (i64, i64) {
    if columns < 0 {
        (position, 0)
    } else if columns == 1 {
        (0, position)
    } else {
        (position % columns, position / columns)
    }
}

fn position_tree(node: &mut Tree, px: f64, py: f64) {
    node.x = px;
    node.y = py;
    if node.children.is_empty() {
        return;
    }
    let columns = node.columns;
    // Pre-compute per-row max height.
    let mut row_heights: Vec<(i64, f64)> = Vec::new();
    let mut col_pos: i64 = 0;
    for child in &node.children {
        let (_, row) = calc_block_position(columns, col_pos);
        match row_heights.iter_mut().find(|(r, _)| *r == row) {
            Some(entry) => {
                if child.height > entry.1 {
                    entry.1 = child.height;
                }
            }
            None => row_heights.push((row, child.height)),
        }
        let mut filled = child.width_in_columns.max(1);
        if columns > 0 {
            filled = filled.min(columns - (col_pos % columns));
        }
        col_pos += filled;
    }
    row_heights.sort_by_key(|(r, _)| *r);
    let mut row_y_offsets: Vec<(i64, f64)> = Vec::new();
    let mut offset = 0.0;
    for &(r, h) in &row_heights {
        row_y_offsets.push((r, offset));
        offset += h + PADDING;
    }
    let row_max_h = |row: i64| -> f64 {
        row_heights
            .iter()
            .find(|(r, _)| *r == row)
            .map(|(_, h)| *h)
            .unwrap_or(0.0)
    };
    let row_offset_y = |row: i64| -> f64 {
        row_y_offsets
            .iter()
            .find(|(r, _)| *r == row)
            .map(|(_, o)| *o)
            .unwrap_or(0.0)
    };

    // Iterate children and assign (x, y).
    // startingPosX = parent.x + -parent.width/2 (or -padding if no size).
    let mut starting_pos_x = if node.id == "root" {
        -PADDING
    } else {
        node.x - node.width / 2.0
    };
    let mut current_row: i64 = 0;
    col_pos = 0;
    // Iterate by index to allow immediately setting child positions while
    // we still need previous row's starting_pos_x.
    for i in 0..node.children.len() {
        let child_width = node.children[i].width;
        let child_height = node.children[i].height;
        let child_wic = node.children[i].width_in_columns.max(1);
        let (_, py_idx) = calc_block_position(columns, col_pos);
        if py_idx != current_row {
            current_row = py_idx;
            starting_pos_x = if node.id == "root" {
                -PADDING
            } else {
                node.x - node.width / 2.0
            };
        }
        let half_w = child_width / 2.0;
        let cx = starting_pos_x + PADDING + half_w;
        starting_pos_x = cx + half_w;
        let row_y_off = row_offset_y(py_idx);
        let row_h = row_max_h(py_idx);
        let cy = node.y - node.height / 2.0 + row_y_off + row_h / 2.0 + PADDING;
        node.children[i].x = cx;
        node.children[i].y = cy;
        // Recurse — children know their own x/y now so composites can
        // place their descendants relative to themselves.
        if !node.children[i].children.is_empty() {
            position_tree(&mut node.children[i], cx, cy);
        } else {
            // Leaves — no descendants to position.
            let _ = child_height;
        }
        let mut filled = child_wic;
        if columns > 0 {
            filled = filled.min(columns - (col_pos % columns));
        }
        col_pos += filled;
    }
}

/// Walk the tree for bounds. Excludes the root wrapper (matches
/// upstream `findBounds` which skips `block.id === 'root'`). The
/// seed `(0, 0, 0, 0)` is preserved — upstream clamps both axes to
/// non-negative extents even when every child sits left/above the
/// origin.
fn walk_bounds(node: &Tree, bx: &mut (f64, f64, f64, f64), init: &mut bool, is_root: bool) {
    if !is_root {
        let x0 = node.x - node.width / 2.0;
        let y0 = node.y - node.height / 2.0;
        let x1 = node.x + node.width / 2.0;
        let y1 = node.y + node.height / 2.0;
        if x0 < bx.0 {
            bx.0 = x0;
        }
        if y0 < bx.1 {
            bx.1 = y0;
        }
        if x1 > bx.2 {
            bx.2 = x1;
        }
        if y1 > bx.3 {
            bx.3 = y1;
        }
    }
    for child in &node.children {
        walk_bounds(child, bx, init, false);
    }
    let _ = init;
}

fn collect_nodes(node: &Tree, out: &mut Vec<NodeGeom>, is_root: bool) {
    if !is_root && !node.is_space() {
        out.push(NodeGeom {
            id: node.id.clone(),
            label: node.label.clone(),
            shape: node.shape,
            x: node.x,
            y: node.y,
            width: node.width,
            height: node.height,
            text_width: node.text_w,
            text_height: node.text_h,
            width_in_columns: node.width_in_columns,
            classes: node.classes.clone(),
            styles: node.styles.clone(),
            arrow_dirs: node.arrow_dirs.clone(),
        });
    }
    for child in &node.children {
        collect_nodes(child, out, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::block::parse;
    use crate::theme::get_theme;

    #[test]
    fn fixture_17_layout_cells() {
        let src = std::fs::read_to_string("tests/ext_fixtures/cypress/block/17.mmd")
            .expect("read 17.mmd");
        let d = parse(&src).unwrap();
        let t = get_theme("default");
        let l = layout(&d, &t).unwrap();
        let a = l.nodes.iter().find(|n| n.id == "A").expect("A present");
        assert!(
            (a.width - 159.99267578125).abs() < 1e-6,
            "A.width={}",
            a.width
        );
        assert!(
            (a.height - 28.4453125).abs() < 1e-6,
            "A.height={}",
            a.height
        );
        assert!((a.x - 79.996337890625).abs() < 1e-6, "A.x={}", a.x);
    }
}
