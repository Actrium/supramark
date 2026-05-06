//! WBS diagram layout engine.
//!
//! Converts a `WbsDiagram` into a fully positioned `WbsLayout` ready for
//! SVG rendering.  The algorithm uses a top-down tree placement: root at
//! the top center, children spread horizontally below.

use std::collections::HashMap;

use log::debug;

use crate::font_metrics;
use crate::model::hyperlink::extract_hyperlinks;
use crate::model::richtext::plain_text;
use crate::model::wbs::{WbsDiagram, WbsNode, WbsNote};
use crate::parser::creole::parse_creole;
use crate::Result;

// ---------------------------------------------------------------------------
// Layout output types
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct WbsLayout {
    pub nodes: Vec<WbsNodeLayout>,
    pub edges: Vec<WbsEdgeLayout>,
    pub extra_links: Vec<WbsEdgeLayout>,
    pub notes: Vec<WbsNoteLayout>,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
pub struct WbsNodeLayout {
    pub text: String,
    pub alias: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub level: usize,
}

#[derive(Debug)]
pub struct WbsEdgeLayout {
    pub from_x: f64,
    pub from_y: f64,
    pub to_x: f64,
    pub to_y: f64,
}

#[derive(Debug, Clone)]
pub struct WbsNoteLayout {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub connector: Option<(f64, f64, f64, f64)>,
}

// ---------------------------------------------------------------------------
// Constants — derived from Java PlantUML WBS reference output
// ---------------------------------------------------------------------------

const FONT_SIZE: f64 = 12.0;
/// AWT line height for SansSerif 12pt = ascent + descent = 13.96875
const LINE_HEIGHT: f64 = 13.96875;
const PAD_H: f64 = 10.0;
const PAD_V: f64 = 10.0;
/// Vertical gap: parent bottom to connector, and connector to child top
#[allow(dead_code)] // Java-ported layout constant
const EDGE_GAP: f64 = 20.0;
#[allow(dead_code)] // Java-ported layout constant
const NODE_SPACING: f64 = 20.0;
const MARGIN: f64 = 10.0;
const NOTE_GAP: f64 = 16.0;
const MIN_NOTE_WIDTH: f64 = 60.0;
const MIN_NOTE_HEIGHT: f64 = 28.0;

// ---------------------------------------------------------------------------
// Text measurement
// ---------------------------------------------------------------------------

fn text_width(text: &str) -> f64 {
    extract_hyperlinks(text)
        .0
        .lines()
        .map(|l| font_metrics::text_width(l, "SansSerif", FONT_SIZE, false, false))
        .fold(0.0_f64, f64::max)
}

fn node_size(text: &str) -> (f64, f64) {
    let visible = extract_hyperlinks(text).0;
    let line_count = visible.lines().count().max(1) as f64;
    let w = text_width(text) + 2.0 * PAD_H;
    let h = line_count * LINE_HEIGHT + 2.0 * PAD_V;
    (w, h)
}

fn note_size(text: &str) -> (f64, f64) {
    let plain = plain_text(&parse_creole(text))
        .replace("\\n", "\n")
        .replace(crate::NEWLINE_CHAR, "\n");
    let lines: Vec<&str> = plain.lines().collect();
    let max_width = lines
        .iter()
        .map(|line| font_metrics::text_width(line, "SansSerif", FONT_SIZE, false, false))
        .fold(0.0_f64, f64::max);
    let width = (max_width + 2.0 * PAD_H).max(MIN_NOTE_WIDTH);
    let height = (lines.len().max(1) as f64 * LINE_HEIGHT + 2.0 * PAD_V).max(MIN_NOTE_HEIGHT);
    (width, height)
}

// ---------------------------------------------------------------------------
// Java WBS layout: two-level model
// Root uses Fork (horizontal children, deltay=40)
// Deeper uses ITFComposed (vertical left/right stacks, marginBottom=15)
// ---------------------------------------------------------------------------

/// Fork constants (root level)
const FORK_DELTA1X: f64 = 20.0;
const FORK_DELTAY: f64 = 40.0;
/// ITFComposed constants (deeper levels)
const ITF_DELTA1X: f64 = 10.0;
const ITF_MARGIN_BOTTOM: f64 = 15.0;

use crate::model::wbs::WbsDirection;

/// Split children into (left, right) groups by direction. Default → right.
fn split_children(node: &WbsNode) -> (Vec<&WbsNode>, Vec<&WbsNode>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for child in &node.children {
        match child.direction {
            WbsDirection::Left => left.push(child),
            _ => right.push(child),
        }
    }
    (left, right)
}

/// ITFComposed subtree dimension (width, height) for non-root nodes.
fn itf_dim(node: &WbsNode) -> (f64, f64) {
    let (main_w, main_h) = node_size(&node.text);
    if node.children.is_empty() {
        return (main_w, main_h);
    }
    let (left, right) = split_children(node);
    let left_w: f64 = left.iter().map(|c| itf_dim(c).0).fold(0.0_f64, f64::max);
    let right_w: f64 = right.iter().map(|c| itf_dim(c).0).fold(0.0_f64, f64::max);
    let left_h: f64 = left.iter().map(|c| ITF_MARGIN_BOTTOM + itf_dim(c).1).sum();
    let right_h: f64 = right.iter().map(|c| ITF_MARGIN_BOTTOM + itf_dim(c).1).sum();
    let w = (main_w / 2.0).max(ITF_DELTA1X + left_w) + (main_w / 2.0).max(ITF_DELTA1X + right_w);
    let h = main_h + left_h.max(right_h);
    (w, h)
}

/// ITFComposed.getw1: x-offset of center from left edge.
fn itf_w1(node: &WbsNode) -> f64 {
    let (main_w, _) = node_size(&node.text);
    let (left, _) = split_children(node);
    let left_w: f64 = left.iter().map(|c| itf_dim(c).0).fold(0.0_f64, f64::max);
    (main_w / 2.0).max(ITF_DELTA1X + left_w)
}

// ---------------------------------------------------------------------------
// Layout: ITFComposed (vertical fork for non-root levels)
// ---------------------------------------------------------------------------

fn layout_itf(
    node: &WbsNode,
    origin_x: f64,
    origin_y: f64,
    nodes: &mut Vec<WbsNodeLayout>,
    edges: &mut Vec<WbsEdgeLayout>,
) {
    let (main_w, main_h) = node_size(&node.text);
    let w1 = itf_w1(node);
    let node_x = origin_x + w1 - main_w / 2.0;
    nodes.push(WbsNodeLayout {
        text: node.text.clone(),
        alias: node.alias.clone(),
        x: node_x,
        y: origin_y,
        width: main_w,
        height: main_h,
        level: node.level,
    });

    if node.children.is_empty() {
        return;
    }

    let (left_children, right_children) = split_children(node);
    let parent_cx = origin_x + w1;
    let parent_by = origin_y + main_h;

    // Left children: stacked vertically, placed left of parent center
    let mut y = parent_by;
    for child in &left_children {
        y += ITF_MARGIN_BOTTOM;
        let child_dim = itf_dim(child);
        let child_ox = parent_cx - child_dim.0 - ITF_DELTA1X;
        let child_cx = child_ox + itf_w1(child);
        edges.push(WbsEdgeLayout {
            from_x: parent_cx,
            from_y: parent_by,
            to_x: child_cx,
            to_y: y,
        });
        layout_itf(child, child_ox, y, nodes, edges);
        y += child_dim.1;
    }

    // Right children: stacked vertically, placed right of parent center
    let mut y = parent_by;
    for child in &right_children {
        y += ITF_MARGIN_BOTTOM;
        let child_ox = parent_cx + ITF_DELTA1X;
        let child_cx = child_ox + itf_w1(child);
        edges.push(WbsEdgeLayout {
            from_x: parent_cx,
            from_y: parent_by,
            to_x: child_cx,
            to_y: y,
        });
        layout_itf(child, child_ox, y, nodes, edges);
        y += itf_dim(child).1;
    }
}

// ---------------------------------------------------------------------------
// Layout: Fork (horizontal spread for root level)
// ---------------------------------------------------------------------------

fn layout_fork(
    node: &WbsNode,
    origin_x: f64,
    origin_y: f64,
    nodes: &mut Vec<WbsNodeLayout>,
    edges: &mut Vec<WbsEdgeLayout>,
) {
    let (main_w, main_h) = node_size(&node.text);

    // All root-level children go right in the Fork model
    let children: Vec<&WbsNode> = node.children.iter().collect();
    if children.is_empty() {
        nodes.push(WbsNodeLayout {
            text: node.text.clone(),
            alias: node.alias.clone(),
            x: origin_x,
            y: origin_y,
            width: main_w,
            height: main_h,
            level: node.level,
        });
        // Java Fork draws a stub line to y0+deltay/2 when there are no children.
        let stub_draw_y = origin_y + main_h + FORK_DELTAY / 2.0;
        let cx = origin_x + main_w / 2.0;
        edges.push(WbsEdgeLayout {
            from_x: cx,
            from_y: origin_y + main_h,
            to_x: cx,
            to_y: stub_draw_y,
        });
        return;
    }

    // Compute child subtree dimensions and positions
    let child_dims: Vec<(f64, f64)> = children.iter().map(|c| itf_dim(c)).collect();
    let child_y = origin_y + main_h + FORK_DELTAY;

    // Compute child x-positions and centers (before laying out, for root positioning)
    let mut child_positions: Vec<(f64, f64)> = Vec::new(); // (child_ox, child_cx)
    let mut child_x = origin_x;
    for (i, child) in children.iter().enumerate() {
        let child_cx = child_x + itf_w1(child);
        child_positions.push((child_x, child_cx));
        child_x += child_dims[i].0 + FORK_DELTA1X;
    }

    let first_child_cx = child_positions[0].1;
    let last_child_cx = child_positions.last().unwrap().1;

    // Position root: Java Fork centers root between first and last child connections.
    // With 1 child: root is centered in the total fork width (= child subtree width).
    let root_cx = if first_child_cx < last_child_cx {
        // Multiple children: center between first and last connection points,
        // then adjust for root width
        let pos_main = first_child_cx + (last_child_cx - first_child_cx - main_w) / 2.0;
        pos_main + main_w / 2.0
    } else {
        // 1 child: center in total fork width
        let total_fork_w: f64 = child_dims.iter().map(|d| d.0).sum::<f64>()
            + (children.len().max(1) as f64 - 1.0) * FORK_DELTA1X;
        origin_x + total_fork_w / 2.0
    };
    let root_x = root_cx - main_w / 2.0;

    // Push root FIRST (so nodes[0] is always the root)
    nodes.push(WbsNodeLayout {
        text: node.text.clone(),
        alias: node.alias.clone(),
        x: root_x,
        y: origin_y,
        width: main_w,
        height: main_h,
        level: node.level,
    });

    // Create edges from root to each child
    for &(_, child_cx) in &child_positions {
        edges.push(WbsEdgeLayout {
            from_x: root_cx,
            from_y: origin_y + main_h,
            to_x: child_cx,
            to_y: child_y,
        });
    }

    // Now layout children recursively
    for (i, child) in children.iter().enumerate() {
        let (child_ox, _) = child_positions[i];
        layout_itf(child, child_ox, child_y, nodes, edges);
    }
}

fn layout_notes(notes: &[WbsNote], root: &WbsNodeLayout) -> Vec<WbsNoteLayout> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut result = Vec::new();
    let rcx = root.x + root.width / 2.0;
    let rcy = root.y + root.height / 2.0;

    for note in notes {
        let (width, height) = note_size(&note.text);
        let si = {
            let c = counts.entry(note.position.as_str()).or_insert(0);
            let v = *c as f64;
            *c += 1;
            v
        };
        let (x, y, conn) = match note.position.as_str() {
            "left" => {
                let x = root.x - NOTE_GAP - width;
                let y = root.y + si * (height + NOTE_GAP);
                (x, y, Some((root.x, rcy, x + width, y + height / 2.0)))
            }
            "top" => {
                let x = rcx - width / 2.0 + si * (NOTE_GAP + 20.0);
                let y = root.y - NOTE_GAP - height;
                (x, y, Some((rcx, root.y, x + width / 2.0, y + height)))
            }
            "bottom" => {
                let x = rcx - width / 2.0 + si * (NOTE_GAP + 20.0);
                let y = root.y + root.height + NOTE_GAP;
                (x, y, Some((rcx, root.y + root.height, x + width / 2.0, y)))
            }
            _ => {
                let x = root.x + root.width + NOTE_GAP;
                let y = root.y + si * (height + NOTE_GAP);
                (x, y, Some((root.x + root.width, rcy, x, y + height / 2.0)))
            }
        };
        result.push(WbsNoteLayout {
            text: note.text.clone(),
            x,
            y,
            width,
            height,
            connector: conn,
        });
    }
    result
}

pub fn layout_wbs(wd: &WbsDiagram) -> Result<WbsLayout> {
    debug!("layout_wbs: root='{}'", wd.root.text);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    // Root uses Fork layout (horizontal children), deeper uses ITFComposed (vertical)
    layout_fork(&wd.root, MARGIN, MARGIN, &mut nodes, &mut edges);

    // Build alias -> node rect for edge-to-edge arrow connections
    let alias_rect: HashMap<String, (f64, f64, f64, f64)> = nodes
        .iter()
        .filter_map(|n| {
            n.alias
                .as_ref()
                .map(|a| (a.clone(), (n.x, n.y, n.width, n.height)))
        })
        .collect();

    let mut extra_links = Vec::new();
    for link in &wd.links {
        if let (Some(&(fx, fy, fw, fh)), Some(&(tx, _ty, tw, _th))) =
            (alias_rect.get(&link.from), alias_rect.get(&link.to))
        {
            let from_cx = fx + fw / 2.0;
            let to_cx = tx + tw / 2.0;
            let link_y = fy + fh / 2.0;
            // Arrow from near edge of source to near edge of target
            let (lx_from, lx_to) = if from_cx > to_cx {
                // Source is to the right, arrow points left
                (fx, tx + tw)
            } else {
                // Source is to the left, arrow points right
                (fx + fw, tx)
            };
            extra_links.push(WbsEdgeLayout {
                from_x: lx_from,
                from_y: link_y,
                to_x: lx_to,
                to_y: link_y,
            });
        }
    }

    let root_layout = nodes
        .iter()
        .find(|n| n.level == 1)
        .cloned()
        .unwrap_or_else(|| nodes[0].clone());
    let mut notes = layout_notes(&wd.notes, &root_layout);

    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (0.0_f64, 0.0_f64);
    // Java `LimitFinder.drawRectangle` records `(x + width - 1, y + height - 1)`,
    // so node/note rectangles contribute `bottom-1` / `right-1` to the LF max
    // (see `klimt/drawing/LimitFinder.java` lines 175-178).  Without this -1 a
    // rect-dominated diagram still yields the right Java-matching viewport
    // because the +1 cancels with the `getFinalDimension` +1, but a
    // line-dominated diagram (e.g. WBS link-tooltip cases where the Fork stub
    // line extends beyond every box) loses 1px from the height.  Track bounds
    // Java-style here so the formula can be expressed once at the bottom.
    for n in &nodes {
        min_x = min_x.min(n.x);
        min_y = min_y.min(n.y);
        max_x = max_x.max(n.x + n.width - 1.0);
        max_y = max_y.max(n.y + n.height - 1.0);
    }
    // Include edge endpoints in bounds (e.g. Fork stub lines extend below nodes).
    // Java `LimitFinder.drawULine` records line endpoints verbatim — no -1.
    for e in &edges {
        max_x = max_x.max(e.from_x).max(e.to_x);
        max_y = max_y.max(e.from_y).max(e.to_y);
    }
    for n in &notes {
        min_x = min_x.min(n.x);
        min_y = min_y.min(n.y);
        max_x = max_x.max(n.x + n.width - 1.0);
        max_y = max_y.max(n.y + n.height - 1.0);
    }

    let sx = if min_x < MARGIN { MARGIN - min_x } else { 0.0 };
    let sy = if min_y < MARGIN { MARGIN - min_y } else { 0.0 };
    if sx > 0.0 || sy > 0.0 {
        for n in &mut nodes {
            n.x += sx;
            n.y += sy;
        }
        for e in &mut edges {
            e.from_x += sx;
            e.to_x += sx;
            e.from_y += sy;
            e.to_y += sy;
        }
        for l in &mut extra_links {
            l.from_x += sx;
            l.to_x += sx;
            l.from_y += sy;
            l.to_y += sy;
        }
        for n in &mut notes {
            n.x += sx;
            n.y += sy;
            if let Some((x1, y1, x2, y2)) = n.connector.as_mut() {
                *x1 += sx;
                *x2 += sx;
                *y1 += sy;
                *y2 += sy;
            }
        }
        max_x += sx;
        max_y += sy;
    }

    // Java `ImageBuilder.getFinalDimension` returns
    //   `LF_max + 1 + margin_left + margin_right` (resp. top/bottom).
    // Our `max_x`/`max_y` already include the top/left margin offset (nodes are
    // placed at `MARGIN`), so adding the bottom/right MARGIN once plus the +1
    // from `getFinalDimension` yields the same value Java feeds into
    // `SvgGraphics.ensureVisible`.  The renderer then applies
    // `ensure_visible_int` (= Java's `(int)(x + 1)`), reproducing Java's
    // compound `(int)(LF_max + 1 + margins + 1)` rounding byte-exactly.
    Ok(WbsLayout {
        nodes,
        edges,
        extra_links,
        notes,
        width: max_x + MARGIN + 1.0,
        height: max_y + MARGIN + 1.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::wbs::{WbsDiagram, WbsDirection, WbsLink, WbsNode, WbsNote};

    fn leaf(text: &str, level: usize) -> WbsNode {
        WbsNode {
            text: text.to_string(),
            children: vec![],
            direction: WbsDirection::Default,
            alias: None,
            level,
        }
    }
    fn leaf_alias(text: &str, alias: &str, level: usize) -> WbsNode {
        WbsNode {
            text: text.to_string(),
            children: vec![],
            direction: WbsDirection::Default,
            alias: Some(alias.into()),
            level,
        }
    }
    fn mkd(root: WbsNode) -> WbsDiagram {
        WbsDiagram {
            root,
            links: vec![],
            notes: vec![],
        }
    }

    #[test]
    fn test_single_root() {
        let l = layout_wbs(&mkd(leaf("Root", 1))).unwrap();
        assert_eq!(l.nodes.len(), 1);
        // Fork with 0 children creates a visible stub edge
        assert_eq!(l.edges.len(), 1);
    }
    #[test]
    fn test_root_with_children() {
        let r = WbsNode {
            text: "Root".into(),
            children: vec![leaf("A", 2), leaf("B", 2)],
            direction: WbsDirection::Default,
            alias: None,
            level: 1,
        };
        let l = layout_wbs(&mkd(r)).unwrap();
        assert_eq!(l.nodes.len(), 3);
        assert_eq!(l.edges.len(), 2);
    }
    #[test]
    fn test_children_below() {
        let r = WbsNode {
            text: "Root".into(),
            children: vec![leaf("A", 2)],
            direction: WbsDirection::Default,
            alias: None,
            level: 1,
        };
        let l = layout_wbs(&mkd(r)).unwrap();
        assert!(l.nodes[1].y > l.nodes[0].y);
    }
    #[test]
    fn test_multiline() {
        let (_, h1) = node_size("One");
        let (_, h2) = node_size("A\nB");
        assert!(h2 > h1);
    }
    #[test]
    fn test_extra_links() {
        let r = WbsNode {
            text: "R".into(),
            children: vec![leaf_alias("A", "AA", 2), leaf_alias("B", "BB", 2)],
            direction: WbsDirection::Default,
            alias: None,
            level: 1,
        };
        let d = WbsDiagram {
            root: r,
            links: vec![WbsLink {
                from: "AA".into(),
                to: "BB".into(),
            }],
            notes: vec![],
        };
        assert_eq!(layout_wbs(&d).unwrap().extra_links.len(), 1);
    }
    #[test]
    fn test_note() {
        let d = WbsDiagram {
            root: leaf("R", 1),
            links: vec![],
            notes: vec![WbsNote {
                text: "hi".into(),
                position: "right".into(),
            }],
        };
        let l = layout_wbs(&d).unwrap();
        assert_eq!(l.notes.len(), 1);
        assert!(l.notes[0].x > l.nodes[0].x + l.nodes[0].width);
    }
    #[test]
    fn test_bbox() {
        let r = WbsNode {
            text: "R".into(),
            children: vec![leaf("A", 2), leaf("B", 2)],
            direction: WbsDirection::Default,
            alias: None,
            level: 1,
        };
        let l = layout_wbs(&mkd(r)).unwrap();
        for n in &l.nodes {
            assert!(n.x + n.width <= l.width);
            assert!(n.y + n.height <= l.height);
        }
    }
}
