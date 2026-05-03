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
        });
    }

    // Multi-node fallback: build the input rectangles and edge list
    // and hand them to the cose_bilkent simulation. NOT byte-exact yet
    // (reduceTrees / FR-grid / Coarsening pieces still missing), but
    // produces plausible centre coordinates so the renderer can emit a
    // visible diagram for diagnostics.
    let cose_nodes: Vec<(NodeId, cose_bilkent::RectangleD)> = positioned
        .iter()
        .map(|n| {
            (
                n.id,
                cose_bilkent::RectangleD::new(0.0, 0.0, n.shape_w.max(40.0), n.shape_h.max(40.0)),
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

    // Aggregate content bbox in node-local space, then translate it via
    // each node's centre. Upstream `setupViewPortForSVG` wraps the
    // entire `<g>` with no transform (mindmap renders absolute coords),
    // so the bbox is the union of every node's translated local bbox.
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for n in &positioned {
        let lb = local_bbox(n);
        min_x = min_x.min(n.x + lb.x);
        min_y = min_y.min(n.y + lb.y);
        max_x = max_x.max(n.x + lb.x + lb.w);
        max_y = max_y.max(n.y + lb.y + lb.h);
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
    })
}

/// Compute width × height for a node. Mirrors upstream's
/// `mindmapRenderer.ts` per-shape padding override followed by the
/// shape-specific `labelHelper` formula.
fn size_node(n: &MindmapNode, d: &MindmapDiagram) -> PositionedNode {
    let (bbox_w, bbox_h) = match n.node_type {
        MindmapNodeType::Circle | MindmapNodeType::RoundedRect => {
            measure_multiline_raw(&n.raw_descr, SHIM_FONT_FAMILY, SHIM_FONT_SIZE_PX)
        }
        _ => {
            let bw = text_width(&n.descr, SHIM_FONT_FAMILY, SHIM_FONT_SIZE_PX, false, false);
            let bh = line_height(SHIM_FONT_FAMILY, SHIM_FONT_SIZE_PX, false, false);
            (bw, bh)
        }
    };

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

    PositionedNode {
        id: n.id,
        x: 0.0,
        y: 0.0,
        bbox_w,
        bbox_h,
        shape_w,
        shape_h,
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
