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

    // Multi-node fallback: positions are zeroed; the renderer detects
    // this by reporting `Unsupported` for now.
    Ok(MindmapLayout {
        nodes: positioned,
        content_bbox: BBox::default(),
    })
}

/// Compute width × height for a node. Mirrors upstream's
/// `mindmapRenderer.ts` per-shape padding override followed by the
/// shape-specific `labelHelper` formula.
fn size_node(n: &MindmapNode, d: &MindmapDiagram) -> PositionedNode {
    // Measure the label (jsdom shim font: sans-serif 14px, non-bold).
    let bbox_w = text_width(&n.descr, SHIM_FONT_FAMILY, SHIM_FONT_SIZE_PX, false, false);
    let bbox_h = line_height(SHIM_FONT_FAMILY, SHIM_FONT_SIZE_PX, false, false);

    // Per-shape padding override from `mindmapRenderer.ts`.
    let padding = match n.node_type {
        MindmapNodeType::RoundedRect => 15.0,
        MindmapNodeType::Circle => 10.0,
        MindmapNodeType::Rect => 10.0,
        MindmapNodeType::Default => 10.0,
        MindmapNodeType::Hexagon | MindmapNodeType::Cloud | MindmapNodeType::Bang => n.padding,
    };

    // halfPadding = padding / 2 in upstream `util.ts::labelHelper`.
    let half_padding = padding / 2.0;
    let (shape_w, shape_h) = match n.node_type {
        MindmapNodeType::Default => {
            // defaultMindmapNode.ts: w = bbox.w + 8*halfPadding,
            //                       h = bbox.h + 2*halfPadding.
            (bbox_w + 8.0 * half_padding, bbox_h + 2.0 * half_padding)
        }
        MindmapNodeType::Rect => {
            // squareRect (classic): labelPaddingX = padding * 2,
            // labelPaddingY = padding. Total = bbox + 2 * paddingX/Y.
            (bbox_w + 4.0 * padding, bbox_h + 2.0 * padding)
        }
        // Other shapes — not yet supported by the single-node fast
        // path; size with the default formula as a placeholder.
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
