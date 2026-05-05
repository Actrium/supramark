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
use crate::render::rough::fmt_num;
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
    /// Shape kind (carried over from the source node so per-shape bbox
    /// helpers can be invoked at content-bbox aggregation time without
    /// re-resolving the diagram graph).
    pub kind: MindmapNodeType,
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

    let mut positioned: Vec<PositionedNode> = d.nodes.iter().map(|n| size_node(n, d)).collect();

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
    let (bbox_w, bbox_h) = measure_multiline_raw(&n.raw_descr, SHIM_FONT_FAMILY, SHIM_FONT_SIZE_PX);

    let padding = match n.node_type {
        MindmapNodeType::RoundedRect => 15.0,
        MindmapNodeType::Circle => 10.0,
        MindmapNodeType::Rect => 10.0,
        MindmapNodeType::Default => 10.0,
        MindmapNodeType::Hexagon | MindmapNodeType::Cloud | MindmapNodeType::Bang => n.padding,
    };

    let half_padding = padding / 2.0;
    let (shape_w, shape_h) = match n.node_type {
        MindmapNodeType::Default => (bbox_w + 8.0 * half_padding, bbox_h + 2.0 * half_padding),
        MindmapNodeType::Rect => (bbox_w + 4.0 * padding, bbox_h + 2.0 * padding),
        MindmapNodeType::Circle => {
            let r = (bbox_w / 2.0).max(bbox_h / 2.0) + padding;
            (2.0 * r, 2.0 * r)
        }
        MindmapNodeType::RoundedRect => (bbox_w + 2.0 * padding, bbox_h + 2.0 * padding),
        MindmapNodeType::Bang => {
            // Upstream `bangShape`:
            //   w = bbox.width  + 10 * halfPadding
            //   h = bbox.height +  8 * halfPadding
            //   minWidth  = bbox.width  + 20
            //   minHeight = bbox.height + 20
            //   effectiveWidth  = max(w, minWidth)
            //   effectiveHeight = max(h, minHeight)
            let w = bbox_w + 10.0 * half_padding;
            let h = bbox_h + 8.0 * half_padding;
            let min_w = bbox_w + 20.0;
            let min_h = bbox_h + 20.0;
            (w.max(min_w), h.max(min_h))
        }
        MindmapNodeType::Cloud => {
            // Upstream `cloudShape`:
            //   w = bbox.width  + 2 * halfPadding
            //   h = bbox.height + 2 * halfPadding
            (bbox_w + 2.0 * half_padding, bbox_h + 2.0 * half_padding)
        }
        _ => (bbox_w + 8.0 * half_padding, bbox_h + 2.0 * half_padding),
    };

    // Union bbox (shape ∪ foreignObject, transforms ignored — JSDOM
    // `getBBox()` shim semantics). Mermaid feeds these values to
    // cose-bilkent as the node's `data.{width, height}` after the node
    // is inserted into the DOM. Synthetic formulae are unsound here
    // because shape paths use SVG `q`/`a` commands whose CONTROL points
    // (or arc endpoints) extend past the half-w/half-h envelope —
    // mirror `generate_ref.mjs::pathBBox` (endpoints + control points
    // only, arcs sampled at endpoints) by parsing the same `d` string
    // the renderer emits.
    //
    // Foreign object: inner `<g class="label">` carries
    // `translate(-bbox_w/2, -bbox_h/2)` (transform ignored), so its
    // contribution is `(0, 0, bbox_w, bbox_h)`.
    let shape_box = shape_intrinsic_box(n.node_type, shape_w, shape_h);
    let fo_box = (0.0_f64, 0.0_f64, bbox_w, bbox_h);
    let (_ux, _uy, cose_w, cose_h) = union_bbox(&[shape_box, Some(fo_box)]);

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
        kind: n.node_type,
    }
}

fn measure_multiline_raw(text: &str, family: &str, size: f64) -> (f64, f64) {
    // Mermaid's pipeline: descr → markdownToHTML → `<p>...<br/>...</p>` →
    // span.html(...) → div. The JSDOM bbox shim measures `el.textContent`,
    // which excludes element-level markup like `<br/>`. So bbox.width is
    // textWidth of the visible characters with `<br/>` tags stripped, and
    // bbox.height is a single line (the shim doesn't introduce breaks for
    // `<br/>`). Mirror that here — otherwise nodes containing `<br/>`
    // (e.g. cypress mindmap 13 `gc6((grand<br/>child 6))`) measure ~30 px
    // wider than upstream, which propagates into cose-bilkent's input
    // dimensions and shifts the simulated layout.
    let stripped = strip_br(text);
    let lh = line_height(family, size, false, false);
    let mut max_w = 0.0_f64;
    let mut line_count = 0usize;
    for line in stripped.split('\n') {
        let w = text_width(line, family, size, false, false);
        max_w = max_w.max(w);
        line_count += 1;
    }
    if line_count == 0 {
        line_count = 1;
    }
    (max_w, line_count as f64 * lh)
}

/// Remove `<br/>`, `<br>`, `<br />` (any case, optional whitespace) — the
/// JSDOM bbox shim's `textContent` walk skips these elements, so the
/// measured width matches the text-only contents.
fn strip_br(s: &str) -> String {
    // Cheap regex-free pass: scan for "<br" then advance past the matching
    // ">"; pass everything else through verbatim. Case-insensitive.
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 3 <= bytes.len()
            && bytes[i] == b'<'
            && bytes[i + 1].eq_ignore_ascii_case(&b'b')
            && bytes[i + 2].eq_ignore_ascii_case(&b'r')
        {
            // After `<br`, peek the next char: must be space, `/`, or `>`
            // for it to be a real `<br>` tag.
            let after = bytes.get(i + 3).copied();
            let is_tag = matches!(after, Some(b' ') | Some(b'/') | Some(b'>') | Some(b'\t'));
            if is_tag {
                if let Some(end) = bytes[i..].iter().position(|&b| b == b'>') {
                    i += end + 1;
                    continue;
                }
            }
        }
        // Not a `<br>` tag — copy the next UTF-8 char.
        let ch_len = utf8_char_len(bytes[i]);
        out.push_str(&s[i..i + ch_len.min(bytes.len() - i)]);
        i += ch_len;
    }
    out
}

fn utf8_char_len(first_byte: u8) -> usize {
    if first_byte < 0x80 {
        1
    } else if first_byte < 0xC0 {
        1
    } else if first_byte < 0xE0 {
        2
    } else if first_byte < 0xF0 {
        3
    } else {
        4
    }
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
/// (transforms are ignored, matching the jsdom shim's `getBBox()` walk
/// in `generate_ref.mjs::elementBBox`).
fn local_bbox(n: &PositionedNode) -> BBox {
    let shape_box = shape_intrinsic_box(n.kind, n.shape_w, n.shape_h);
    let fo_box = (0.0_f64, 0.0_f64, n.bbox_w, n.bbox_h);
    let (x, y, w, h) = union_bbox(&[shape_box, Some(fo_box)]);
    BBox { x, y, w, h }
}

// ─── jsdom-compatible bbox helpers ────────────────────────────────────────
//
// Mirror `tests/support/generate_ref.mjs` `pathBBox` / `polyBBox` /
// `unionBox` / `intrinsicBox` exactly — these compute the dimensions
// upstream's mermaid feeds into cose-bilkent (and into the viewport
// padding pass) via `g.getBBox()` in the jsdom shim. Synthetic formulae
// drift because SVG `q` control points and `a` arc endpoints extend past
// the half-w/half-h envelope.

/// Pick a per-shape intrinsic bbox `(x, y, w, h)` matching the geometry
/// the renderer emits. Returns `None` if the shape contributes nothing
/// (defensive — every supported shape has a non-empty body).
pub(crate) fn shape_intrinsic_box(
    kind: MindmapNodeType,
    shape_w: f64,
    shape_h: f64,
) -> Option<(f64, f64, f64, f64)> {
    let half_w = shape_w / 2.0;
    let half_h = shape_h / 2.0;
    match kind {
        MindmapNodeType::Default => {
            // Path = `default_path_d`. Includes `<line>` overlay across
            // the bottom (`y = half_h`); both contribute, but the path
            // itself extends past `[-half_w, half_w]` because of the
            // 5px corner radius `q` control points.
            let d = default_path_d(shape_w, shape_h);
            let pb = path_bbox(&d);
            // The line element runs from (-half_w, half_h) to (half_w,
            // half_h). It's a degenerate horizontal segment so its
            // bbox is (half_w * 2) × 0 — width=2*half_w, height=0.
            // jsdom `intrinsicBox` for `<line>` uses min/abs (always
            // produces (-half_w, half_h, shape_w, 0)), and `unionBox`
            // SKIPS boxes with `width==0 && height==0` only — a
            // height=0 box still contributes its x range.
            let line_box = Some((-half_w, half_h, shape_w, 0.0));
            Some(union_two(pb, line_box))
        }
        MindmapNodeType::Rect => Some((-half_w, -half_h, shape_w, shape_h)),
        MindmapNodeType::Circle => {
            // squareCircle: r = max(half_w, half_h).
            let r = half_w.max(half_h);
            Some((-r, -r, 2.0 * r, 2.0 * r))
        }
        MindmapNodeType::RoundedRect => Some((-half_w, -half_h, shape_w, shape_h)),
        MindmapNodeType::Hexagon => {
            // Polygon points (matches `emit_shape_body` Hexagon branch).
            let f = half_h / 3.0_f64.sqrt();
            let m = f / 2.0;
            let xs = [
                -half_w + m,
                half_w - m,
                half_w,
                half_w - m,
                -half_w + m,
                -half_w,
            ];
            let ys = [-half_h, -half_h, 0.0, half_h, half_h, 0.0];
            let mut min_x = f64::INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            for i in 0..6 {
                if xs[i] < min_x {
                    min_x = xs[i];
                }
                if xs[i] > max_x {
                    max_x = xs[i];
                }
                if ys[i] < min_y {
                    min_y = ys[i];
                }
                if ys[i] > max_y {
                    max_y = ys[i];
                }
            }
            Some((min_x, min_y, max_x - min_x, max_y - min_y))
        }
        MindmapNodeType::Bang => {
            // jsdom ignores the `transform="translate(...)"` on the
            // path element itself, so the path bbox lives at the M0 0
            // origin in local coords.
            let d = bang_path_d(shape_w, shape_h);
            Some(path_bbox(&d))
        }
        MindmapNodeType::Cloud => {
            let d = cloud_path_d(shape_w, shape_h);
            Some(path_bbox(&d))
        }
    }
}

/// Build the same `d` attribute string the renderer emits for the
/// default mindmap shape (rounded-bottom rectangle with 5px corner
/// radius). Numbers must be formatted via `fmt_num` so the parsed bbox
/// matches the jsdom shim's `parseFloat` round-trip exactly.
pub(crate) fn default_path_d(shape_w: f64, shape_h: f64) -> String {
    let half_w = shape_w / 2.0;
    let half_h = shape_h / 2.0;
    let inner_w = shape_w - 10.0;
    let inner_h = shape_h - 10.0;
    format!(
        "\n    M{nx} {hh}\n    v{nih}\n    q0,-5 5,-5\n    h{iw}\n    q5,0 5,5\n    v{ih}\n    q0,5 -5,5\n    h{niw}\n    q-5,0 -5,-5\n    Z\n  ",
        nx = fmt_num(-half_w),
        hh = fmt_num(half_h - 5.0),
        nih = fmt_num(-inner_h),
        iw = fmt_num(inner_w),
        ih = fmt_num(inner_h),
        niw = fmt_num(-inner_w),
    )
}

/// Build the bang shape `d` attribute (12-arc explosion, M0 0 origin,
/// translate baked into path geometry — translate attribute itself is
/// ignored by jsdom shim).
///
/// `-1.0 * x` literals mirror upstream `bangShape`'s JS source — kept
/// verbatim so byte-exact diff against `mermaid.js` stays trivial.
#[allow(clippy::neg_multiply)]
pub(crate) fn bang_path_d(ew: f64, eh: f64) -> String {
    let r = 0.15 * ew;
    let r08 = r * 0.8;
    format!(
        "M0 0 \n    a{r},{r} 1 0,0 {a1},{b1}\n    a{r},{r} 1 0,0 {a1},{z}\n    a{r},{r} 1 0,0 {a1},{z}\n    a{r},{r} 1 0,0 {a1},{b2}\n\n    a{r},{r} 1 0,0 {c1},{d1}\n    a{r08},{r08} 1 0,0 0,{d2}\n    a{r},{r} 1 0,0 {c2},{d1}\n\n    a{r},{r} 1 0,0 {e1},{f1}\n    a{r},{r} 1 0,0 {e1},0\n    a{r},{r} 1 0,0 {e1},0\n    a{r},{r} 1 0,0 {e1},{f2}\n\n    a{r},{r} 1 0,0 {g1},{h1}\n    a{r08},{r08} 1 0,0 0,{h2}\n    a{r},{r} 1 0,0 {g2},{h1}\n  H0 V0 Z",
        r = fmt_num(r),
        r08 = fmt_num(r08),
        a1 = fmt_num(ew * 0.25),
        b1 = fmt_num(-1.0 * eh * 0.1),
        z = fmt_num(0.0),
        b2 = fmt_num(eh * 0.1),
        c1 = fmt_num(ew * 0.15),
        d1 = fmt_num(eh * 0.33),
        d2 = fmt_num(eh * 0.34),
        c2 = fmt_num(-1.0 * ew * 0.15),
        e1 = fmt_num(-1.0 * ew * 0.25),
        f1 = fmt_num(eh * 0.15),
        f2 = fmt_num(-1.0 * eh * 0.15),
        g1 = fmt_num(-1.0 * ew * 0.1),
        h1 = fmt_num(-1.0 * eh * 0.33),
        h2 = fmt_num(-1.0 * eh * 0.34),
        g2 = fmt_num(ew * 0.1),
    )
}

/// Build the cloud shape `d` attribute (9-arc puffy outline).
///
/// `-1.0 * x` literals mirror upstream `cloudShape`'s JS source — kept
/// verbatim so byte-exact diff against `mermaid.js` stays trivial.
#[allow(clippy::neg_multiply)]
pub(crate) fn cloud_path_d(w: f64, h: f64) -> String {
    let r1 = 0.15 * w;
    let r2 = 0.25 * w;
    let r3 = 0.35 * w;
    let r4 = 0.20 * w;
    format!(
        "M0 0 \n    a{r1},{r1} 0 0,1 {a1},{b1}\n    a{r3},{r3} 1 0,1 {a2},{b1}\n    a{r2},{r2} 1 0,1 {a3},{b2}\n\n    a{r1},{r1} 1 0,1 {c1},{d1}\n    a{r4},{r4} 1 0,1 {c2},{d2}\n\n    a{r2},{r1} 1 0,1 {e1},{f1}\n    a{r3},{r3} 1 0,1 {e2},0\n    a{r1},{r1} 1 0,1 {e1},{f2}\n\n    a{r1},{r1} 1 0,1 {g1},{h1}\n    a{r4},{r4} 1 0,1 {g2},{h2}\n  H0 V0 Z",
        r1 = fmt_num(r1),
        r2 = fmt_num(r2),
        r3 = fmt_num(r3),
        r4 = fmt_num(r4),
        a1 = fmt_num(w * 0.25),
        b1 = fmt_num(-1.0 * w * 0.1),
        a2 = fmt_num(w * 0.4),
        a3 = fmt_num(w * 0.35),
        b2 = fmt_num(w * 0.2),
        c1 = fmt_num(w * 0.15),
        d1 = fmt_num(h * 0.35),
        c2 = fmt_num(-1.0 * w * 0.15),
        d2 = fmt_num(h * 0.65),
        e1 = fmt_num(-1.0 * w * 0.25),
        f1 = fmt_num(w * 0.15),
        e2 = fmt_num(-1.0 * w * 0.5),
        f2 = fmt_num(-1.0 * w * 0.15),
        g1 = fmt_num(-1.0 * w * 0.1),
        h1 = fmt_num(-1.0 * h * 0.35),
        g2 = fmt_num(w * 0.1),
        h2 = fmt_num(-1.0 * h * 0.65),
    )
}

/// Replicate `generate_ref.mjs::pathBBox`. Returns `(x, y, w, h)`.
///
/// Curves: cubic / quadratic include all control points (super-set of
/// the true curve bbox — same approximation upstream's jsdom shim
/// uses); arcs (`A`/`a`) sample the END point only, NOT the bulge.
/// `H`/`V`/`Z` updated state correctly. An empty / unrecognised string
/// returns `(0, 0, 0, 0)`.
pub(crate) fn path_bbox(d: &str) -> (f64, f64, f64, f64) {
    if d.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let toks = tokenize_path(d);
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut cx = 0.0_f64;
    let mut cy = 0.0_f64;
    let mut sx = 0.0_f64;
    let mut sy = 0.0_f64;
    let mut cmd: char = ' ';
    let mut i = 0usize;

    macro_rules! addp {
        ($x:expr, $y:expr) => {{
            let xx: f64 = $x;
            let yy: f64 = $y;
            if xx < min_x {
                min_x = xx;
            }
            if yy < min_y {
                min_y = yy;
            }
            if xx > max_x {
                max_x = xx;
            }
            if yy > max_y {
                max_y = yy;
            }
        }};
    }

    while i < toks.len() {
        if let PathTok::Cmd(c) = toks[i] {
            cmd = c;
            i += 1;
        }
        if cmd == ' ' {
            i += 1;
            continue;
        }
        let rel = cmd.is_ascii_lowercase();
        // Pull n numbers; abort if not enough.
        match cmd {
            'M' | 'm' => {
                let (Some(mut x), Some(mut y)) = (read_num(&toks, &mut i), read_num(&toks, &mut i))
                else {
                    continue;
                };
                if rel {
                    x += cx;
                    y += cy;
                }
                cx = x;
                cy = y;
                sx = x;
                sy = y;
                addp!(x, y);
                cmd = if rel { 'l' } else { 'L' };
            }
            'L' | 'l' => {
                let (Some(mut x), Some(mut y)) = (read_num(&toks, &mut i), read_num(&toks, &mut i))
                else {
                    continue;
                };
                if rel {
                    x += cx;
                    y += cy;
                }
                cx = x;
                cy = y;
                addp!(x, y);
            }
            'H' | 'h' => {
                let Some(mut x) = read_num(&toks, &mut i) else {
                    continue;
                };
                if rel {
                    x += cx;
                }
                cx = x;
                addp!(x, cy);
            }
            'V' | 'v' => {
                let Some(mut y) = read_num(&toks, &mut i) else {
                    continue;
                };
                if rel {
                    y += cy;
                }
                cy = y;
                addp!(cx, y);
            }
            'C' | 'c' => {
                let (
                    Some(mut x1),
                    Some(mut y1),
                    Some(mut x2),
                    Some(mut y2),
                    Some(mut x),
                    Some(mut y),
                ) = (
                    read_num(&toks, &mut i),
                    read_num(&toks, &mut i),
                    read_num(&toks, &mut i),
                    read_num(&toks, &mut i),
                    read_num(&toks, &mut i),
                    read_num(&toks, &mut i),
                )
                else {
                    continue;
                };
                if rel {
                    x1 += cx;
                    y1 += cy;
                    x2 += cx;
                    y2 += cy;
                    x += cx;
                    y += cy;
                }
                addp!(x1, y1);
                addp!(x2, y2);
                addp!(x, y);
                cx = x;
                cy = y;
            }
            'S' | 's' => {
                let (Some(mut x2), Some(mut y2), Some(mut x), Some(mut y)) = (
                    read_num(&toks, &mut i),
                    read_num(&toks, &mut i),
                    read_num(&toks, &mut i),
                    read_num(&toks, &mut i),
                ) else {
                    continue;
                };
                if rel {
                    x2 += cx;
                    y2 += cy;
                    x += cx;
                    y += cy;
                }
                addp!(x2, y2);
                addp!(x, y);
                cx = x;
                cy = y;
            }
            'Q' | 'q' => {
                let (Some(mut x1), Some(mut y1), Some(mut x), Some(mut y)) = (
                    read_num(&toks, &mut i),
                    read_num(&toks, &mut i),
                    read_num(&toks, &mut i),
                    read_num(&toks, &mut i),
                ) else {
                    continue;
                };
                if rel {
                    x1 += cx;
                    y1 += cy;
                    x += cx;
                    y += cy;
                }
                addp!(x1, y1);
                addp!(x, y);
                cx = x;
                cy = y;
            }
            'T' | 't' => {
                let (Some(mut x), Some(mut y)) = (read_num(&toks, &mut i), read_num(&toks, &mut i))
                else {
                    continue;
                };
                if rel {
                    x += cx;
                    y += cy;
                }
                addp!(x, y);
                cx = x;
                cy = y;
            }
            'A' | 'a' => {
                // rx ry x-axis-rotation large-arc sweep x y
                if read_num(&toks, &mut i).is_none() {
                    continue;
                }
                if read_num(&toks, &mut i).is_none() {
                    continue;
                }
                if read_num(&toks, &mut i).is_none() {
                    continue;
                }
                if read_num(&toks, &mut i).is_none() {
                    continue;
                }
                if read_num(&toks, &mut i).is_none() {
                    continue;
                }
                let (Some(mut x), Some(mut y)) = (read_num(&toks, &mut i), read_num(&toks, &mut i))
                else {
                    continue;
                };
                if rel {
                    x += cx;
                    y += cy;
                }
                addp!(x, y);
                cx = x;
                cy = y;
            }
            'Z' | 'z' => {
                cx = sx;
                cy = sy;
            }
            _ => {
                i += 1;
            }
        }
    }

    if !min_x.is_finite() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

#[derive(Debug, Clone, Copy)]
enum PathTok {
    Cmd(char),
    Num(f64),
}

fn tokenize_path(d: &str) -> Vec<PathTok> {
    let mut out = Vec::new();
    let bytes = d.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_alphabetic() && b"MmLlHhVvZzCcSsQqTtAa".contains(&c) {
            out.push(PathTok::Cmd(c as char));
            i += 1;
        } else if c == b'-' || c == b'.' || c.is_ascii_digit() {
            let start = i;
            if c == b'-' {
                i += 1;
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                    i += 1;
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if start == i {
                i += 1;
                continue;
            }
            let s = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
            if let Ok(v) = s.parse::<f64>() {
                out.push(PathTok::Num(v));
            }
        } else {
            i += 1;
        }
    }
    out
}

fn read_num(toks: &[PathTok], i: &mut usize) -> Option<f64> {
    if *i < toks.len() {
        match toks[*i] {
            PathTok::Num(v) => {
                *i += 1;
                return Some(v);
            }
            PathTok::Cmd(_) => return None,
        }
    }
    None
}

/// Replicate `generate_ref.mjs::unionBox`. Skips boxes with both
/// `width==0` AND `height==0`, and returns `(0, 0, 0, 0)` if every
/// input is skipped or `None`.
pub(crate) fn union_bbox(boxes: &[Option<(f64, f64, f64, f64)>]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut found = false;
    for b in boxes {
        let Some((x, y, w, h)) = *b else { continue };
        if w == 0.0 && h == 0.0 {
            continue;
        }
        found = true;
        if x < min_x {
            min_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if x + w > max_x {
            max_x = x + w;
        }
        if y + h > max_y {
            max_y = y + h;
        }
    }
    if !found {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

fn union_two(a: (f64, f64, f64, f64), b: Option<(f64, f64, f64, f64)>) -> (f64, f64, f64, f64) {
    union_bbox(&[Some(a), b])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_bbox_simple_line() {
        let (x, y, w, h) = path_bbox("M0 0 L 10 5");
        assert_eq!((x, y, w, h), (0.0, 0.0, 10.0, 5.0));
    }

    #[test]
    fn path_bbox_arc_uses_endpoints_only() {
        // A semicircle from (0,0) to (10,0) via radius 5 — endpoint
        // sample only, NOT the bulge at y=5.
        let (x, y, w, h) = path_bbox("M0 0 A5,5 0 0,1 10,0");
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
        assert_eq!(w, 10.0);
        assert_eq!(h, 0.0);
    }

    #[test]
    fn path_bbox_quadratic_includes_control() {
        // Q control point at (5, 10), end at (10, 0) — control is in.
        let (x, y, w, h) = path_bbox("M0 0 Q 5 10 10 0");
        assert_eq!((x, y, w, h), (0.0, 0.0, 10.0, 10.0));
    }

    #[test]
    fn path_bbox_relative_z_resets() {
        // After Z, cursor should be at (0, 0); next m relative to origin.
        let (x, y, w, h) = path_bbox("M0 0 L 5 5 Z m 10 10 l 5 5");
        // Path covers (0,0)-(5,5), then jumps to (10,10), goes to (15,15).
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
        assert_eq!(w, 15.0);
        assert_eq!(h, 15.0);
    }

    #[test]
    fn bang_path_bbox_matches_js_oracle() {
        // From cypress/mindmap/10.svg path 0 (root bang).
        // ew = 4 * 21.306396484375 + 8.52255859375 = ?  Actually back out
        // from the JS oracle: x=-8.52255859375, y=-5.6296875, w=106.531..,
        // h=70.37109375. Original ew = 4 * 21.306396484375 = 85.225586,
        // shape_w = ew = ?, this is path bbox post-fmt_num.
        // Use bang_path_d directly with the same ew/eh that produced
        // those numbers: ew=85.22..., eh=56.296875.
        let ew = 85.22558593750_f64;
        let eh = 56.296875_f64;
        let d = bang_path_d(ew, eh);
        let (x, y, w, h) = path_bbox(&d);
        // JS oracle on the live SVG d-string returns these values.
        assert!((x - (-8.52255859375)).abs() < 1e-9, "x = {}", x);
        assert!((y - (-5.6296875)).abs() < 1e-9, "y = {}", y);
        assert!((w - 106.531982421875).abs() < 1e-9, "w = {}", w);
        assert!((h - 70.37109375).abs() < 1e-9, "h = {}", h);
    }

    #[test]
    fn union_skips_empty_boxes() {
        let u = union_bbox(&[Some((0.0, 0.0, 0.0, 0.0)), Some((1.0, 2.0, 3.0, 4.0))]);
        assert_eq!(u, (1.0, 2.0, 3.0, 4.0));
    }

    #[test]
    fn union_keeps_zero_height_box() {
        // A horizontal line (h=0, w>0) is not empty; jsdom unionBox
        // skips ONLY when both are zero.
        let u = union_bbox(&[Some((-5.0, 10.0, 10.0, 0.0)), Some((0.0, 0.0, 1.0, 1.0))]);
        assert_eq!(u, (-5.0, 0.0, 10.0, 10.0));
    }
}
