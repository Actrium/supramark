//! Mindmap layout.
//!
//! Upstream renders mindmaps with the cose-bilkent force-directed
//! layout (cytoscape extension, ~3000 LOC physics simulation) for the
//! default `layout` setting, and with `non-layered-tidy-tree-layout`
//! for the `tidy-tree` setting (used in cypress fixtures 01..04).
//!
//! Single-node fast path: cose-bilkent's quality:"proof", animate:false
//! mode places a lone node at (W/2 + 15, H/2 + 15) — i.e. the centre
//! of the layout's container with a 15-px margin on the upper-left.
//! This is deterministic and verified empirically against cypress
//! fixtures 05 / 06.
//!
//! Multi-node graphs need the actual physics simulation; those
//! fixtures stay in `tests/known_ignored.txt` for now.

use crate::error::Result;
use crate::font_metrics::{line_height, text_width};
use crate::layout::cose_bilkent;
use crate::model::mindmap::{MindmapDiagram, MindmapNode, MindmapNodeType, NodeId};
use crate::theme::ThemeVariables;

/// `setupViewPortForSVG` outer padding (mindmap.padding default).
pub const VIEWPORT_PADDING: f64 = 10.0;

/// Section index assigned by upstream when a node is a depth-0 root or
/// a depth-1 sub-root. Values mirror `mindmapDb.section`:
/// root gets `-1`, the first depth-1 child gets `0`, second gets `1`,
/// etc., wrapping after `MAX_SECTIONS - 1` (= 11).
pub const MAX_SECTIONS: i32 = 12;

/// cose-bilkent's single-node margin (constant, observed via probing
/// `cytoscape-cose-bilkent` v4.x with quality:"proof", animate:false).
const COSE_SINGLE_NODE_MARGIN: f64 = 15.0;

#[derive(Debug, Clone, Default)]
pub struct MindmapLayout {
    pub nodes: Vec<PositionedNode>,
    /// Width × height of the union bbox of all node geometry (paths,
    /// lines, foreign objects in their LOCAL coordinates — transforms
    /// are ignored, matching the jsdom shim's `elementBBox` walk).
    pub content_bbox: BBox,
    /// Edge endpoints (start, mid, end) in absolute coordinates. Indexed
    /// by child node index (the edge connects `parent → child`); root
    /// nodes have `None`. Computed by clipping the centre-to-centre line
    /// against cytoscape's default 30 × 30 node bbox.
    pub edges: Vec<Option<EdgePoints>>,
}

#[derive(Debug, Clone, Copy)]
pub struct EdgePoints {
    pub start: (f64, f64),
    pub mid: (f64, f64),
    pub end: (f64, f64),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone)]
pub struct PositionedNode {
    pub id: NodeId,
    /// Absolute centre coordinates after layout (cose-bilkent's
    /// `node.position()`).
    pub x: f64,
    pub y: f64,
    /// `bbox.width` — text width returned by jsdom's
    /// `getBoundingClientRect` (used by the renderer to size the
    /// inner `<foreignObject>` and as the input to the shape's outer
    /// width formula).
    pub bbox_w: f64,
    pub bbox_h: f64,
    /// Effective shape outer width / height (path / rect dims).
    pub shape_w: f64,
    pub shape_h: f64,
    /// Union bbox dimensions (shape ∪ foreignObject, transforms ignored —
    /// matches JSDOM's `getBBox()`). These are the values mermaid feeds
    /// into cose-bilkent as `data.{width,height}` after `insertNode()`.
    pub cose_w: f64,
    pub cose_h: f64,
    /// Node padding after the renderer's per-shape override.
    pub padding: f64,
    /// Section index (`-1` for root, `0..MAX_SECTIONS-1` for sub-trees).
    pub section: i32,
}

/// Default font face / size used by the jsdom shim when no explicit
/// attribute is set on the label DOM. mermaid never sets `font-family`
/// or `font-size` on `<foreignObject>` `<div>` elements for mindmap, so
/// every label measures at this default.
const SHIM_FONT_FAMILY: &str = "sans-serif";
const SHIM_FONT_SIZE_PX: f64 = 14.0;

pub fn layout(d: &MindmapDiagram, _theme: &ThemeVariables) -> Result<MindmapLayout> {
    if d.nodes.is_empty() {
        return Ok(MindmapLayout::default());
    }

    let mut positioned: Vec<PositionedNode> =
        d.nodes.iter().map(|n| size_node(n, d)).collect();

    if d.nodes.len() == 1 {
        // cose-bilkent single-node fast path: centre = (W/2 + 15, H/2 + 15).
        // Empirically verified against cypress fixtures 05 (default
        // shape) and 06 (rect shape).
        let n = &mut positioned[0];
        let local = local_bbox(n);
        n.x = local.w / 2.0 + COSE_SINGLE_NODE_MARGIN;
        n.y = local.h / 2.0 + COSE_SINGLE_NODE_MARGIN;
        return Ok(MindmapLayout {
            nodes: positioned,
            content_bbox: local,
            edges: vec![None],
        });
    }

    // Multi-node fallback: build the input rectangles and edge list
    // and hand them to the cose_bilkent simulation. NOT byte-exact yet
    // (reduceTrees / FR-grid / Coarsening pieces still missing), but
    // produces plausible centre coordinates so the renderer can emit a
    // visible diagram for diagnostics.
    // Feed the union bbox dims (shape ∪ foreignObject) to cose-bilkent —
    // upstream pulls these from `getBBox()` after inserting the node into
    // the DOM. Without this, x/y centres drift by tens of pixels because
    // a default `<g class="label">` extends past the shape outline (it's
    // anchored at origin, not centred).
    let cose_nodes: Vec<(NodeId, cose_bilkent::RectangleD)> = positioned
        .iter()
        .map(|n| {
            (
                n.id,
                cose_bilkent::RectangleD::new(0.0, 0.0, n.cose_w, n.cose_h),
            )
        })
        .collect();
    let cose_edges: Vec<(usize, usize)> = d
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.parent.map(|p| (p, i)))
        .collect();
    let outcome = cose_bilkent::run_layout(&cose_nodes, &cose_edges, 0x1234_5678);
    if let cose_bilkent::LayoutOutcome::Ok(positions) = outcome {
        for (id, (x, y)) in positions {
            if let Some(n) = positioned.iter_mut().find(|n| n.id == id) {
                n.x = x;
                n.y = y;
            }
        }
    }

    // Compute edge endpoints. Cytoscape uses its default 30 × 30 node
    // bbox to anchor edges (since no `width`/`height` style is applied
    // in the layout-only `styleEnabled: false` setup), so the start /
    // end are the line's intersection with a 30 × 30 box centred at each
    // node. Mid is the midpoint of (start, end).
    let mut edges_out: Vec<Option<EdgePoints>> = vec![None; positioned.len()];
    for (i, src) in d.nodes.iter().enumerate() {
        let Some(p) = src.parent else { continue };
        let pn = &positioned[p];
        let cn = &positioned[i];
        let start = clip_to_default_bbox((pn.x, pn.y), (cn.x, cn.y));
        let end = clip_to_default_bbox((cn.x, cn.y), (pn.x, pn.y));
        let mid = ((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0);
        edges_out[i] = Some(EdgePoints { start, mid, end });
    }

    // Aggregate content bbox.  JSDOM's `getBBox()` shim ignores transforms
    // (see generate_ref.mjs::elementBBox), so per-node geometry is read
    // in node-local coordinates. The content bbox is the UNION of:
    //   - each node's local bbox (NOT translated by node centre);
    //   - each edge `<path>`'s control points (which carry absolute
    //     coordinates, since no transform wraps `<g class="edgePaths">`).
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for n in &positioned {
        let lb = local_bbox(n);
        min_x = min_x.min(lb.x);
        min_y = min_y.min(lb.y);
        max_x = max_x.max(lb.x + lb.w);
        max_y = max_y.max(lb.y + lb.h);
    }
    // The edge `<path>`'s coord text is rounded to 3 decimals by d3-path
    // (`Math.round(v * 1000) / 1000`); JSDOM's `pathBBox` parses the
    // string back, so we must mirror that rounding when building the
    // content bbox — otherwise viewBox dims drift by ~1e-3.
    for ep in edges_out.iter().flatten() {
        let (x0, y0) = ep.start;
        let (x1, y1) = ep.mid;
        let (x2, y2) = ep.end;
        // Sample every coord that lands in the path string: M/L start,
        // first L (5*P0+P1)/6, two C control + dest sets, final L end.
        let xs = [
            x0,
            (5.0 * x0 + x1) / 6.0,
            (2.0 * x0 + x1) / 3.0,
            (x0 + 2.0 * x1) / 3.0,
            (x0 + 4.0 * x1 + x2) / 6.0,
            (2.0 * x1 + x2) / 3.0,
            (x1 + 2.0 * x2) / 3.0,
            (x1 + 5.0 * x2) / 6.0,
            x2,
        ];
        let ys = [
            y0,
            (5.0 * y0 + y1) / 6.0,
            (2.0 * y0 + y1) / 3.0,
            (y0 + 2.0 * y1) / 3.0,
            (y0 + 4.0 * y1 + y2) / 6.0,
            (2.0 * y1 + y2) / 3.0,
            (y1 + 2.0 * y2) / 3.0,
            (y1 + 5.0 * y2) / 6.0,
            y2,
        ];
        for x in xs {
            let xr = (x * 1000.0).round() / 1000.0;
            min_x = min_x.min(xr);
            max_x = max_x.max(xr);
        }
        for y in ys {
            let yr = (y * 1000.0).round() / 1000.0;
            min_y = min_y.min(yr);
            max_y = max_y.max(yr);
        }
    }
    let content_bbox = if min_x.is_finite() {
        BBox {
            x: min_x,
            y: min_y,
            w: max_x - min_x,
            h: max_y - min_y,
        }
    } else {
        BBox::default()
    };

    Ok(MindmapLayout {
        nodes: positioned,
        content_bbox,
        edges: edges_out,
    })
}

/// Return the point on the circle of radius 15 centred at `from`, on the
/// side facing `to`. Mirrors cytoscape's `intersectLineEllipse` operation
/// order BIT-FOR-BIT (see vendor/cytoscape.umd.js#4077): the length is
/// computed from RADIUS-NORMALISED displacements, but the proportional
/// scaling is applied to the RAW displacements. Re-arranging into a
/// single `(R / len)` factor produces a different rounding pattern.
fn clip_to_default_bbox(from: (f64, f64), to: (f64, f64)) -> (f64, f64) {
    const R: f64 = 15.0;
    // Cytoscape's `intersectLineEllipse(x, y, centerX, centerY, r, r)`
    // returns the intersection on the boundary nearest `(x, y)`. Map
    // our `(from, to)` to cytoscape's `(centerX, centerY) = from`,
    // `(x, y) = to`.
    let disp_x = (from.0 - to.0) / R;
    let disp_y = (from.1 - to.1) / R;
    let len = (disp_x * disp_x + disp_y * disp_y).sqrt();
    let new_length = len - 1.0;
    if new_length < 0.0 {
        return from;
    }
    let len_prop = new_length / len;
    let raw_dx = from.0 - to.0;
    let raw_dy = from.1 - to.1;
    (raw_dx * len_prop + to.0, raw_dy * len_prop + to.1)
}

/// Compute width × height for a node. Mirrors upstream's
/// `mindmapRenderer.ts` per-shape padding override followed by the
/// shape-specific `labelHelper` formula.
fn size_node(n: &MindmapNode, d: &MindmapDiagram) -> PositionedNode {
    // bbox = JSDOM `getBBox()` shim's `measureTextBlock` over the same
    // text the renderer emits inside the foreignObject `<span>`. The
    // span contains `raw_descr` either verbatim (when marked's
    // markdownToHTML falls through to `node.raw` for an indented code
    // block) or wrapped in `<p>...</p>`. In both cases the `textContent`
    // that the bbox shim measures is exactly `raw_descr` — the `<p>`
    // tags are not part of textContent.
    let (bbox_w, bbox_h) =
        measure_multiline_raw(&n.raw_descr, SHIM_FONT_FAMILY, SHIM_FONT_SIZE_PX);

    let padding = match n.node_type {
        MindmapNodeType::RoundedRect => 15.0,
        MindmapNodeType::Circle => 10.0,
        MindmapNodeType::Rect => 10.0,
        MindmapNodeType::Default => 10.0,
        MindmapNodeType::Hexagon | MindmapNodeType::Cloud | MindmapNodeType::Bang => n.padding,
    };

    let half_padding = padding / 2.0;
    let (shape_w, shape_h) = match n.node_type {
        MindmapNodeType::Default => {
            (bbox_w + 8.0 * half_padding, bbox_h + 2.0 * half_padding)
        }
        MindmapNodeType::Rect => {
            (bbox_w + 4.0 * padding, bbox_h + 2.0 * padding)
        }
        MindmapNodeType::Circle => {
            let r = (bbox_w / 2.0).max(bbox_h / 2.0) + padding;
            (2.0 * r, 2.0 * r)
        }
        MindmapNodeType::RoundedRect => {
            (bbox_w + 2.0 * padding, bbox_h + 2.0 * padding)
        }
        _ => (bbox_w + 8.0 * half_padding, bbox_h + 2.0 * half_padding),
    };

    // Union bbox (shape ∪ foreignObject, transforms ignored — JSDOM
    // shim semantics). Mermaid feeds these values to cose-bilkent as the
    // node's `data.{width, height}` after `getBBox()`. Matches:
    //   * shape: centred at origin → covers `[-shape_w/2, shape_w/2]`
    //     × `[-shape_h/2, shape_h/2]`.
    //   * foreignObject: ignored transform → covers `[0, bbox_w]`
    //     × `[0, bbox_h]`.
    let cose_w = shape_w / 2.0 + bbox_w.max(shape_w / 2.0);
    let cose_h = shape_h / 2.0 + bbox_h.max(shape_h / 2.0);

    PositionedNode {
        id: n.id,
        x: 0.0,
        y: 0.0,
        bbox_w,
        bbox_h,
        shape_w,
        shape_h,
        cose_w,
        cose_h,
        padding,
        section: section_for(n, d),
    }
}

fn measure_multiline_raw(text: &str, family: &str, size: f64) -> (f64, f64) {
    let lh = line_height(family, size, false, false);
    let mut max_w = 0.0_f64;
    let mut line_count = 0usize;
    for line in text.split('\n') {
        let w = text_width(line, family, size, false, false);
        max_w = max_w.max(w);
        line_count += 1;
    }
    if line_count == 0 {
        line_count = 1;
    }
    (max_w, line_count as f64 * lh)
}

/// Section index assignment matches upstream `mindmapDb.section`:
///   * root → `-1`
///   * each depth-1 child gets a unique index counted in source order,
///     wrapped modulo (MAX_SECTIONS - 1).
///   * deeper descendants inherit their depth-1 ancestor's section.
fn section_for(n: &MindmapNode, d: &MindmapDiagram) -> i32 {
    if n.is_root || n.parent.is_none() {
        return -1;
    }
    let mut cur = n.id;
    while let Some(p) = d.nodes[cur].parent {
        if d.nodes[p].is_root {
            if let Some(idx) = d.nodes[p].children.iter().position(|c| *c == cur) {
                return (idx as i32) % (MAX_SECTIONS - 1);
            }
            return 0;
        }
        cur = p;
    }
    -1
}

/// Compute the local bbox for a single node — the union of its inner
/// shape and `<foreignObject>` rectangles in node-local coordinates
/// (transforms are ignored, matching the jsdom shim).
///
/// All currently supported shapes (default, rect) draw a centred body
/// in `[-w/2, w/2] × [-h/2, h/2]`. The `<foreignObject>` is wrapped in
/// a `<g class="label" transform="translate(-bbox_w/2, -bbox_h/2)">`
/// (transform ignored), so it contributes `(0, 0, bbox_w, bbox_h)`.
fn local_bbox(n: &PositionedNode) -> BBox {
    let shape_min_x = -n.shape_w / 2.0;
    let shape_max_x = n.shape_w / 2.0;
    let shape_min_y = -n.shape_h / 2.0;
    let shape_max_y = n.shape_h / 2.0;
    let fo_min_x = 0.0;
    let fo_max_x = n.bbox_w;
    let fo_min_y = 0.0;
    let fo_max_y = n.bbox_h;
    let min_x = shape_min_x.min(fo_min_x);
    let min_y = shape_min_y.min(fo_min_y);
    let max_x = shape_max_x.max(fo_max_x);
    let max_y = shape_max_y.max(fo_max_y);
    BBox {
        x: min_x,
        y: min_y,
        w: max_x - min_x,
        h: max_y - min_y,
    }
}
