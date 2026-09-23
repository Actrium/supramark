//! Activity diagram layout engine.
//!
//! Converts an `ActivityDiagram` (list of events + optional swimlanes) into a
//! fully positioned `ActivityLayout` ready for SVG rendering.  The algorithm is
//! a single top-to-bottom pass with a y-cursor, similar to how the sequence
//! diagram layout works with column-based placement.

use crate::font_metrics;
use crate::layout::graphviz::{
    layout_with_svek, transform_path_d, LayoutEdge, LayoutGraph, LayoutNode, RankDir,
};
use crate::model::activity::{
    ActivityDiagram, ActivityEvent, NotePosition, OldActivityGraph, OldActivityNodeKind,
};
use crate::render::svg_richtext::{
    creole_line_height, creole_plain_text, creole_text_width, measure_creole_display_lines,
};
use crate::{Error, Result};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Layout output types
// ---------------------------------------------------------------------------

/// Fully positioned activity diagram ready for rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivityLayout {
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<ActivityNodeLayout>,
    pub edges: Vec<ActivityEdgeLayout>,
    pub swimlane_layouts: Vec<SwimlaneLayout>,
    pub old_style_graphviz: bool,
    pub old_node_meta: Vec<Option<ActivityGraphvizNodeMeta>>,
    pub old_edge_meta: Vec<Option<ActivityGraphvizEdgeMeta>>,
    /// Optional render order for nodes — a permutation of `0..nodes.len()`.
    /// When present, the renderer iterates nodes in this order instead of
    /// `nodes`' natural order.  Used by `repeat`/`repeat while` blocks to
    /// match Java `FtileRepeat.drawU`'s "body → diamond1 → hex" sequence
    /// without disturbing the node index scheme that edges depend on.
    pub render_order: Option<Vec<usize>>,
}

/// A single positioned node.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivityNodeLayout {
    pub index: usize,
    pub kind: ActivityNodeKindLayout,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub text: String,
    /// When true, this node is excluded from automatic sequential edge building.
    /// Used for nodes inside if/else branches whose edges are built manually.
    pub skip_in_flow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityNoteModeLayout {
    Grouped,
    Single,
}

/// Visual kind of a node — determines how the renderer draws it.
#[derive(Debug, Clone, PartialEq)]
pub enum ActivityNodeKindLayout {
    Start,
    Stop,
    End,
    Action,
    Diamond,
    /// Hexagonal diamond (Java's `FtileDiamondInside`) used by
    /// `repeat while (cond) is (label)` — the test condition lives inside
    /// the hexagon (in `node.text`) and the optional `is` label is rendered
    /// to the right of the shape (East label).  Each entry in `east_lines`
    /// is rendered as a separate line of text.  The optional `not` label
    /// is rendered below the shape (South label).
    Hexagon {
        east_lines: Vec<String>,
        south_lines: Vec<String>,
    },
    ForkBar,
    SyncBar,
    Note {
        position: NotePositionLayout,
        mode: ActivityNoteModeLayout,
    },
    FloatingNote {
        position: NotePositionLayout,
        mode: ActivityNoteModeLayout,
    },
    Detach,
    /// Backward action box on the repeat loop-back path.
    /// Rendered like an Action but excluded from flow edges.
    BackwardAction,
    /// Hexagonal if-diamond (Java's `FtileDiamondInside` for `if`).
    /// Contains the condition text (in `node.text`) and branch labels drawn
    /// beside the shape.  `left_label` is rendered to the left (the "then"
    /// label) and `right_label` is rendered to the right (the "else" label).
    /// `bottom_label` is the implicit branch label rendered below the shape
    /// (used when the else branch is empty / not explicitly labelled).
    IfDiamond {
        left_label: String,
        right_label: String,
        bottom_label: String,
    },
    /// Goto loop-back rendered as plain lines (no arrowheads).
    /// Java's FtileGoto draws its connection inline during tile rendering.
    /// `points` contains pairs of (x1,y1,x2,y2) for each line segment.
    GotoLines {
        segments: Vec<(f64, f64, f64, f64)>,
    },
    /// Partition frame (Java `FtileGroup` + `USymbolFrame`).  Drawn as a
    /// rectangle with a folded-corner tab and a title.  The node's `x/y`
    /// is the frame's top-left corner; `width/height` is the full frame size.
    /// `text` holds the partition name.  The inner content nodes are laid out
    /// as normal flow nodes between PartitionStart and PartitionEnd; the frame
    /// is `skip_in_flow = true` so it does not participate in edges.
    Partition,
    /// Square diamond used by `switch (cond)` — the condition text lives in
    /// `node.text` and is centred inside the diamond.  Case labels are rendered
    /// as edge labels on the branch connections from the diamond's sides.
    SwitchDiamond,
}

/// Note position in the layout coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotePositionLayout {
    Left,
    Right,
}

/// Edge rendering kind — drives how `render_edge` draws the poly-line and
/// where the arrowheads sit.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ActivityEdgeKindLayout {
    /// Simple forward edge: straight / right-angle path with an arrow at the
    /// last point.
    #[default]
    Normal,
    /// `FtileRepeat.ConnectionBackSimple2` loop-back: 4-point snake (hex east
    /// → right → up → diamond1 right) with an end-arrow and an extra
    /// mid-segment UP arrow drawn over the vertical stretch.  `up_arrow_y`
    /// gives the polygon origin (tip) Y coordinate.
    LoopBackSimple2 { up_arrow_y: f64 },
    /// `FtileRepeat.ConnectionBackBackward1`: hex east → backward bottom.
    /// 3-point snake (right, then up) with UP arrow at end.
    LoopBackBackward1,
    /// `FtileRepeat.ConnectionBackBackward2`: backward top → diamond1 right.
    /// 3-point snake (up, then left) with LEFT arrow at end.
    LoopBackBackward2,
    /// Goto loop-back edge: from a node to a label position (up+left).
    GotoLoopBack,
    /// Break edge: from a break point to the repeat exit diamond.
    BreakEdge,
    /// If-branch connection from diamond to branch node.
    /// Multi-segment polyline with a down-arrow at end.
    IfBranch,
    /// If-merge connection from branch end back to center.
    /// Multi-segment polyline with a down-arrow at end.
    IfMerge,
    /// If-merge with an emphasize-direction DOWN arrow on the first long
    /// vertical segment (used for the while exit and the no-else implicit
    /// else).  When `mid_arrow_y` is set the arrow tip is placed there;
    /// otherwise it sits near the bottom of the segment (`sy2 - 26`).
    IfMergeEmphasize { mid_arrow_y: Option<f64> },
    /// Goto loop-back plain lines (no arrow).
    GotoNoArrow,
}

/// A directed edge between two nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivityEdgeLayout {
    pub from_index: usize,
    pub to_index: usize,
    pub label: String,
    pub points: Vec<(f64, f64)>,
    pub kind: ActivityEdgeKindLayout,
    /// Optional explicit label position; when set, the renderer places the
    /// label here instead of at the polyline midpoint.
    pub label_xy: Option<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityGraphvizNodeMeta {
    pub id: String,
    pub uid: String,
    pub qualified_name: String,
    /// Source line number from the first link that references this node as
    /// either endpoint. `None` if no such link exists (rare — only wholly
    /// disconnected nodes). Mirrors Java PlantUML's `data-source-line` on
    /// `start_entity` / `end_entity` wrappers.
    pub source_line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityGraphvizEdgeMeta {
    pub uid: String,
    pub from_id: String,
    pub to_id: String,
    /// UID of the source node, used for the `data-entity-1` attribute.
    pub from_uid: String,
    /// UID of the target node, used for the `data-entity-2` attribute.
    pub to_uid: String,
    /// Source line number of the link in the original `.puml` source.
    /// Mirrors Java PlantUML's `data-source-line` on `link` wrappers.
    pub source_line: usize,
    pub raw_path_d: Option<String>,
    pub arrow_polygon_points: Option<Vec<(f64, f64)>>,
    pub label_xy: Option<(f64, f64)>,
    pub head_label: Option<String>,
    pub head_label_xy: Option<(f64, f64)>,
}

/// A single swimlane column.
#[derive(Debug, Clone, PartialEq)]
pub struct SwimlaneLayout {
    pub name: String,
    pub x: f64,
    pub width: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ActivityTableKind {
    SingleColumn { rows: Vec<String> },
    MultiColumn,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const FONT_SIZE: f64 = 12.0;
const PADDING: f64 = 10.0;
/// Gap between consecutive flow nodes (matches Java PlantUML visual output).
const NODE_SPACING: f64 = 20.0;
/// Gap for old-style activity diagrams (emulates DOT ranksep ≈ 40px).
const OLD_STYLE_NODE_SPACING: f64 = 29.1;
/// Java FtileCircleStart: SIZE = 20, so radius = 10.
const START_RADIUS: f64 = 10.0;
/// Java FtileCircleStop: SIZE = 22, so radius = 11.
const STOP_RADIUS: f64 = 11.0;
const DIAMOND_SIZE: f64 = 20.0;
/// Java `Hexagon.hexagonHalfSize`.  Drives `FtileDiamond` (square diamond used
/// at `repeat` start) and `FtileDiamondInside` (hexagon used at `repeat while`).
const HEXAGON_HALF_SIZE: f64 = 12.0;
/// Font size for the test condition rendered inside the `repeat while` hexagon
/// and for the East `is` label rendered to its right.  Matches Java's
/// `activity.diamond.FontSize` (11pt).
const HEXAGON_LABEL_FONT_SIZE: f64 = 11.0;
/// Extra vertical gap inserted before a `repeat while` hexagon when an East
/// label is present.  Java's `FtileRepeat.calculateDimensionInternal` reserves
/// `8 * hexagonHalfSize = 96` total padding around the inner sequence, half of
/// which (48px) sits above the closing hexagon.  Without an East label the
/// usual 20px arrow gap is enough; with one we add 28px to reach the 48px
/// clearance Java provides.
const REPEAT_WHILE_EXTRA_GAP: f64 = 28.0;
/// Extra vertical slack inserted between the first two interior actions of a
/// `repeat`/`repeat while` block when the loop-back arrow runs along the
/// right side of the diagram.  Java's SlotFinder Y-compression reserves room
/// for the mid-segment UP arrow polygon (10px tall) plus ~8.75px padding on
/// each side, which works out to an extra 7.5px beyond the normal 20px node
/// gap (= final 27.5px gap observed in the reference SVG).
const REPEAT_INNER_ARROW_GAP_EXTRA: f64 = 7.5;
const FORK_BAR_HEIGHT: f64 = 6.0;
const FORK_BAR_WIDTH: f64 = 80.0;
/// Java sync bar height (old-style activity `===NAME===`).
const SYNC_BAR_HEIGHT: f64 = 8.0;
/// Font size for the partition frame title (Java `activityDiagram.composite`
/// style → FontSize 14, sans-serif).
const PARTITION_TITLE_FONT_SIZE: f64 = 14.0;
const NOTE_FONT_SIZE: f64 = 13.0;
const NOTE_MARGIN_X1: f64 = 6.0;
const NOTE_MARGIN_X2: f64 = 15.0;
const NOTE_MARGIN_Y: f64 = 5.0;
/// Java activity notes leave a 10px visible gap between the flow tile and the
/// note body. Wider spacing is handled separately in the lane composite-width
/// calculation via note margins, not by the placement gap itself.
const NOTE_OFFSET: f64 = 10.0;
#[allow(dead_code)] // reserved for future swimlane min-width enforcement
const SWIMLANE_MIN_WIDTH: f64 = 80.0;
const TOP_MARGIN: f64 = 11.0;
const BOTTOM_MARGIN: f64 = 7.0;
const SWIMLANE_HEADER_FONT_SIZE: f64 = 18.0;
/// Java activity cross-swimlane connections keep a short fixed vertical stub
/// before the horizontal transfer instead of routing at the arithmetic midline.
const CROSS_LANE_VERTICAL_STUB: f64 = 5.0;

// ---------------------------------------------------------------------------
// Text measurement helpers
// ---------------------------------------------------------------------------

/// Java creole table cell padding (from skinParam.getPadding(), default 2).
/// Applied as top+bottom padding on SheetBlock1 wrapping each table cell.
pub(crate) const TABLE_CELL_PADDING: f64 = 2.0;

pub(crate) fn classify_activity_table_lines(lines: &[&str]) -> Option<ActivityTableKind> {
    let mut saw_table = false;
    let mut saw_multi_column = false;
    let mut saw_nonempty_non_table = false;
    let mut single_column_rows = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !(trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 2) {
            saw_nonempty_non_table = true;
            continue;
        }

        let inner = &trimmed[1..trimmed.len() - 1];
        let cell_count = inner.split('|').count();
        saw_table = true;

        if cell_count >= 2 {
            saw_multi_column = true;
        } else {
            single_column_rows.push(inner.trim().to_string());
        }
    }

    if !saw_table {
        return None;
    }
    if saw_multi_column {
        return Some(ActivityTableKind::MultiColumn);
    }
    if saw_nonempty_non_table {
        return None;
    }
    Some(ActivityTableKind::SingleColumn {
        rows: single_column_rows,
    })
}

/// Estimate the bounding-box size of an action box.
/// Uses actual font metrics for precise sizing to match Java PlantUML.
/// Detects creole tables (`|...|` rows) and adds cell padding.
/// Detects inline sprite references (`<$name>`) and uses sprite viewBox
/// dimensions (scaled by `fontSize / (fontSize + 1)`) for sizing.
fn estimate_text_size(text: &str) -> (f64, f64) {
    // Java: Display.create() does NOT trim lines; leading/trailing spaces
    // are preserved and measured for width (AtomText includes spaces).
    let lines: Vec<&str> = text.split('\n').collect();
    match classify_activity_table_lines(&lines) {
        Some(ActivityTableKind::MultiColumn) => {
            let display_lines: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();
            let (content_width, content_height) = measure_creole_display_lines(
                &display_lines,
                "SansSerif",
                FONT_SIZE,
                false,
                false,
                false,
            );
            let width = content_width + 2.0 * PADDING;
            let height = content_height + 2.0 * PADDING;
            log::debug!(
                "estimate_text_size(table) -> {}x{} ({} lines)",
                width,
                height,
                lines.len()
            );
            return (width, height);
        }
        Some(ActivityTableKind::SingleColumn { rows }) => {
            let content_width = rows.iter().fold(0.0_f64, |acc, row| {
                acc.max(creole_text_width(row, "SansSerif", FONT_SIZE, false, false))
            });
            let content_height = rows
                .iter()
                .map(|row| creole_line_height(row, "SansSerif", FONT_SIZE))
                .sum::<f64>()
                + 2.0 * TABLE_CELL_PADDING;
            let width = content_width + 2.0 * PADDING;
            let height = content_height + 2.0 * PADDING;
            log::debug!(
                "estimate_text_size(single-col-table) -> {}x{} ({} rows)",
                width,
                height,
                rows.len()
            );
            return (width, height);
        }
        None => {}
    }

    // Java AtomImgSvg: sprite visual size = viewBox × fontSize / (fontSize + 1).
    let sprite_scale = FONT_SIZE / (FONT_SIZE + 1.0);
    let lh = font_metrics::line_height("SansSerif", FONT_SIZE, false, false);

    let mut max_line_width = 0.0_f64;
    let mut total_content_height = 0.0_f64;

    for l in &lines {
        let trimmed = l.trim();
        // Check for sprite-only line: `<$name>`
        if let Some(sprite_dim) = sprite_line_dimensions(trimmed, sprite_scale) {
            max_line_width = max_line_width.max(sprite_dim.0);
            total_content_height += sprite_dim.1;
        } else {
            // Java sizes activity boxes via `SheetBlock1.calculateDimension`,
            // which measures the *rendered* creole text — inline markup such as
            // `__underline__`, `**bold**`, `//italic//` is stripped before
            // measuring, so the box width matches the visible text.  Using raw
            // `text_width` here would count the markup characters (`__` adds
            // ~48px for the issue #30 else-branch), producing oversized boxes.
            //
            // `creole_text_width` runs the creole parser (stripping markup) but
            // trims trailing whitespace, whereas Java's `SheetBlock1` preserves
            // trailing spaces (e.g. `activity_mono_multi_line`'s `...executed:a  `
            // label).  Add the trailing-whitespace width back so both cases
            // match Java byte-for-byte.
            let w = creole_text_width(l, "SansSerif", FONT_SIZE, false, false);
            let trailing_ws = &l[l.trim_end().len()..];
            let w = w + font_metrics::text_width(trailing_ws, "SansSerif", FONT_SIZE, false, false);
            max_line_width = max_line_width.max(w);
            total_content_height += lh;
        }
    }

    let width = max_line_width + 2.0 * PADDING;
    let height = total_content_height + 2.0 * PADDING;
    log::debug!(
        "estimate_text_size -> {}x{} ({} lines)",
        width,
        height,
        lines.len()
    );
    (width, height)
}

/// If `line` is a sprite-only reference (e.g. `<$name>`), return its visual
/// (width, height) after scaling.  Returns `None` for normal text lines.
fn sprite_line_dimensions(line: &str, scale: f64) -> Option<(f64, f64)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("<$") || !trimmed.ends_with('>') {
        return None;
    }
    let inner = &trimmed[2..trimmed.len() - 1];
    let name = inner.split(',').next().unwrap_or(inner).trim();
    if name.is_empty() {
        return None;
    }
    let svg = crate::render::svg_richtext::get_sprite_svg(name)?;
    let (vb_w, vb_h) = parse_sprite_viewbox(&svg);
    Some((vb_w * scale, vb_h * scale))
}

/// Parse viewBox from SVG content to get (width, height).
fn parse_sprite_viewbox(svg: &str) -> (f64, f64) {
    if let Some(vb_start) = svg.find("viewBox=\"") {
        let rest = &svg[vb_start + 9..];
        if let Some(vb_end) = rest.find('"') {
            let parts: Vec<&str> = rest[..vb_end].split_whitespace().collect();
            if parts.len() == 4 {
                return (
                    parts[2].parse().unwrap_or(100.0),
                    parts[3].parse().unwrap_or(50.0),
                );
            }
        }
    }
    (100.0, 50.0)
}

/// Height of a `====` / `----` horizontal separator in a note (Java: 10.0).
/// Height of a `====` / `----` horizontal separator in a note (Java: 10.0).
pub const NOTE_SEPARATOR_HEIGHT: f64 = 10.0;

/// Estimate the size of a note, using note font size.
///
/// Java height model (FloatingNote -> SheetBlock1/2 -> Opale):
///   height = text_block_height + 2 * marginY
/// where text block height is the sum of stripe heights. For plain text lines
/// that is one `line_height` per line; `====`/`----` separators contribute the
/// `CreoleHorizontalLine` height directly.
fn estimate_note_size(text: &str) -> (f64, f64) {
    use crate::render::svg_richtext::creole_text_width;

    let note_lh = font_metrics::line_height("SansSerif", NOTE_FONT_SIZE, false, false);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut max_line_width = 0.0_f64;
    let mut n_text: usize = 0;
    let mut sep_height = 0.0_f64;
    for line in &lines {
        let trimmed = line.trim();
        let is_sep = trimmed.len() >= 4
            && (trimmed.chars().all(|c| c == '=') || trimmed.chars().all(|c| c == '-'));
        if is_sep {
            sep_height += NOTE_SEPARATOR_HEIGHT;
        } else {
            let w = creole_text_width(line, "SansSerif", NOTE_FONT_SIZE, false, false);
            max_line_width = max_line_width.max(w);
            n_text += 1;
        }
    }
    let width = max_line_width + NOTE_MARGIN_X1 + NOTE_MARGIN_X2;
    let text_height = n_text as f64 * note_lh + sep_height;
    let height = text_height + 2.0 * NOTE_MARGIN_Y;
    log::debug!(
        "estimate_note_size: {:.4}x{:.4} ({} text, max_lw={:.4})",
        width,
        height,
        n_text,
        max_line_width
    );
    (width, height)
}

/// Bullet list indent width in a note (Java: bullet ellipse + gap ≈ 18px).
const NOTE_BULLET_INDENT: f64 = 18.0;

/// Word-wrap note text to fit within `max_width` pixels.
///
/// Splits lines at word boundaries, measuring plain text (creole-stripped)
/// width while preserving the original creole markup in the output.
/// Bullet list items (`* ...`) reduce available width by the indent.
fn wrap_note_text(text: &str, max_width: f64) -> String {
    let mut result_lines: Vec<String> = Vec::new();

    for line in text.split('\n') {
        // Detect bullet list prefix `* ` and reduce effective wrap width.
        let (prefix, content, effective_width) = if line.trim_start().starts_with("* ") {
            let idx = line.find("* ").unwrap();
            ("* ", &line[idx + 2..], max_width - NOTE_BULLET_INDENT)
        } else {
            ("", line, max_width)
        };

        let plain = creole_plain_text(content);
        let line_w = font_metrics::text_width(&plain, "SansSerif", NOTE_FONT_SIZE, false, false);
        if line_w <= effective_width {
            result_lines.push(line.to_string());
            continue;
        }

        // Need to wrap: split by spaces and accumulate
        let words: Vec<&str> = content.split(' ').collect();
        let mut current_line = String::new();
        let mut carry_prefix = String::new();
        let mut is_first = true;
        for word in &words {
            if current_line.is_empty() {
                current_line = format!("{carry_prefix}{word}");
                continue;
            }

            let candidate = format!("{current_line} {word}");
            let candidate_plain = creole_plain_text(&candidate);
            let candidate_w = font_metrics::text_width(
                &candidate_plain,
                "SansSerif",
                NOTE_FONT_SIZE,
                false,
                false,
            );

            if candidate_w <= effective_width {
                current_line = candidate;
            } else {
                // Flush current line
                if is_first && !prefix.is_empty() {
                    result_lines.push(format!("{prefix}{current_line}"));
                    is_first = false;
                } else {
                    result_lines.push(current_line);
                }
                carry_prefix = collect_unclosed_creole_prefix(
                    result_lines.last().map(String::as_str).unwrap_or(""),
                );
                current_line = format!("{carry_prefix}{word}");
            }
        }
        if !current_line.is_empty() {
            if is_first && !prefix.is_empty() {
                result_lines.push(format!("{prefix}{current_line}"));
            } else {
                result_lines.push(current_line);
            }
        }
    }

    result_lines.join("\n")
}

fn collect_unclosed_creole_prefix(line: &str) -> String {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum TagKind {
        Bold,
        Italic,
        Underline,
        Strike,
        Back,
        Font,
        Color,
        Size,
    }

    // Compares bytes: `haystack` walks a label one character at a time, so
    // `haystack[..needle.len()]` would panic whenever a multi-byte character
    // is shorter than the tag being tested for.
    fn starts_with_ci(haystack: &str, needle: &str) -> bool {
        haystack.len() >= needle.len()
            && haystack.as_bytes()[..needle.len()].eq_ignore_ascii_case(needle.as_bytes())
    }

    let mut stack: Vec<(TagKind, String)> = Vec::new();
    let mut i = 0usize;
    while i < line.len() {
        let rest = &line[i..];
        let mut matched = false;
        for (open, close, kind) in [
            ("<b>", "</b>", TagKind::Bold),
            ("<i>", "</i>", TagKind::Italic),
            ("<u>", "</u>", TagKind::Underline),
            ("<s>", "</s>", TagKind::Strike),
        ] {
            if starts_with_ci(rest, open) {
                stack.push((kind, open.to_string()));
                i += open.len();
                matched = true;
                break;
            }
            if starts_with_ci(rest, close) {
                if let Some(pos) = stack.iter().rposition(|(k, _)| *k == kind) {
                    stack.remove(pos);
                }
                i += close.len();
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }

        for (prefix, close, kind) in [
            ("<back:", "</back>", TagKind::Back),
            ("<font:", "</font>", TagKind::Font),
            ("<color:", "</color>", TagKind::Color),
            ("<size:", "</size>", TagKind::Size),
        ] {
            if starts_with_ci(rest, prefix) {
                if let Some(end) = rest.find('>') {
                    stack.push((kind, rest[..=end].to_string()));
                    i += end + 1;
                    matched = true;
                    break;
                }
            }
            if starts_with_ci(rest, close) {
                if let Some(pos) = stack.iter().rposition(|(k, _)| *k == kind) {
                    stack.remove(pos);
                }
                i += close.len();
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }

        i += rest.chars().next().map(char::len_utf8).unwrap_or(1);
    }

    stack.into_iter().map(|(_, open)| open).collect()
}

// ---------------------------------------------------------------------------
// Swimlane helpers
// ---------------------------------------------------------------------------

/// Java LaneDivider half-space: 5px at edges, expands if title overflows content.
const LANE_DIVIDER_HALF: f64 = 5.0;

/// Compute initial swimlane column layouts from header text.
///
/// Java sizes lanes to content (via LimitFinder) then expands for title.
/// Here we start with header-text width; Pass 2c expands for content+notes.
fn compute_swimlane_layouts(swimlanes: &[String]) -> Vec<SwimlaneLayout> {
    if swimlanes.is_empty() {
        return Vec::new();
    }
    let lane_pad = 10.0;
    let mut layouts = Vec::new();
    // Java: first LaneDivider starts at edge half-space (5px each side = 10px)
    let mut x = LANE_DIVIDER_HALF * 2.0; // left divider width = 10
    for name in swimlanes.iter() {
        let title_width =
            font_metrics::text_width(name, "SansSerif", SWIMLANE_HEADER_FONT_SIZE, false, false);
        // Initial lane width from header text (no min-width — Java doesn't use one)
        let lane_width = title_width + 2.0 * lane_pad;
        layouts.push(SwimlaneLayout {
            name: name.clone(),
            x,
            width: lane_width,
        });
        // Java: inter-lane divider = halfMissing(i+1) + halfMissing(i+2)
        // Both default to 5px unless title overflows
        let divider = LANE_DIVIDER_HALF * 2.0;
        x += lane_width + divider;
    }
    layouts
}

/// Return the horizontal centre-x for a given swimlane index.  When no
/// swimlanes exist, fall back to a single centred column of
/// `SWIMLANE_MIN_WIDTH`.
fn swimlane_center_x(lanes: &[SwimlaneLayout], lane_idx: usize) -> f64 {
    if lanes.is_empty() {
        // Will be resolved in the centering pass.
        0.0
    } else {
        let lane = &lanes[lane_idx.min(lanes.len() - 1)];
        lane.x + lane.width / 2.0
    }
}

/// Resolve a swimlane name to its index.  Returns 0 when not found.
fn resolve_swimlane_index(swimlanes: &[String], name: &str) -> usize {
    swimlanes.iter().position(|n| n == name).unwrap_or(0)
}

/// Flatten the event tree so that `Partition { name, events }` becomes
/// `PartitionStart { name }` + inner events (recursively flattened) +
/// `PartitionEnd`.  This lets the single-pass layout loop handle nested
/// partitions without recursion.  The returned `Vec` owns cloned events so
/// that all pre-passes and the main loop share the same index space.
fn flatten_events(events: &[ActivityEvent]) -> Vec<ActivityEvent> {
    let mut result = Vec::new();
    for event in events {
        match event {
            ActivityEvent::Partition {
                name,
                events: inner,
            } => {
                result.push(ActivityEvent::PartitionStart { name: name.clone() });
                result.extend(flatten_events(inner));
                result.push(ActivityEvent::PartitionEnd);
            }
            _ => result.push(event.clone()),
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Layout entry point
// ---------------------------------------------------------------------------

/// Perform the complete layout of an activity diagram.
///
/// The result contains absolute positions for every node and edge so that a
/// renderer can draw them without further computation.
pub fn layout_activity(diagram: &ActivityDiagram) -> Result<ActivityLayout> {
    if let Some(old_graph) = diagram.old_graph.as_ref() {
        return layout_old_style_activity_graph(diagram, old_graph);
    }

    // Flatten partition events into PartitionStart/PartitionEnd markers so
    // the single-pass loop can handle nested groups without recursion.
    let events = flatten_events(&diagram.events);

    log::debug!(
        "layout_activity: {} events ({} after flatten), {} swimlanes",
        diagram.events.len(),
        events.len(),
        diagram.swimlanes.len()
    );

    // --- Pass 1: swimlane columns (initial sizing from header text) ---------
    let mut swimlane_layouts = compute_swimlane_layouts(&diagram.swimlanes);

    // --- Pass 2: place nodes ------------------------------------------------
    let mut nodes: Vec<ActivityNodeLayout> = Vec::new();
    // When swimlanes exist, push initial y below the header row.
    // Java a0002: header text baseline y=34.45, first node y=43.7.
    // header_height = header_top_margin(17.75) + ascent + descent + gap(5.05)
    let swimlane_header_height = if swimlane_layouts.is_empty() {
        0.0
    } else {
        let ha = font_metrics::ascent("SansSerif", SWIMLANE_HEADER_FONT_SIZE, false, false);
        let hd = font_metrics::descent("SansSerif", SWIMLANE_HEADER_FONT_SIZE, false, false);
        // Java: content_start = header_top + titles_height
        // header_top = 2019 font units at header font size (DejaVu Sans).
        // This value (=ascender(1901) + 118) comes from Java's global MinMax
        // y-offset in the rendering framework.
        let header_top = 2019.0 / 2048.0 * SWIMLANE_HEADER_FONT_SIZE;
        header_top + (ha + hd)
    };
    // Java's `FtileBox` (the Action rect tile) adds an internal 1px top
    // padding before its rectangle which, combined with the surrounding
    // `ImageBuilder.calculateMargin(10)`, puts the first rect at svg y=11
    // for rect-led diagrams.  `FtileDiamond`/`FtileDiamondInside`, which
    // lead `repeat`/`repeat while` blocks, skip that extra 1px so their
    // top vertex lands at svg y=10.  We emulate this by dropping the top
    // margin by 1 whenever the first flow node is a diamond/hex.
    // Determine the first visual flow element to set the correct top margin.
    // Java ImageBuilder.calculateMargin = 10 for all diagrams.
    // FtileBox (Action) adds an internal 1px top padding → first rect at y=11.
    // FtileDiamond/FtileCircleStart have no extra padding → first shape at y=10.
    let first_flow_event = events.iter().find(|e| {
        matches!(
            e,
            ActivityEvent::Start
                | ActivityEvent::Action { .. }
                | ActivityEvent::If { .. }
                | ActivityEvent::While { .. }
                | ActivityEvent::Repeat
        )
    });
    let first_is_action =
        first_flow_event.is_some_and(|e| matches!(e, ActivityEvent::Action { .. }));

    let mut y_cursor = if swimlane_layouts.is_empty() {
        if first_is_action {
            TOP_MARGIN // 11 = 10 + 1px FtileBox padding
        } else {
            TOP_MARGIN - 1.0 // 10 = pure ImageBuilder margin
        }
    } else {
        swimlane_header_height
    };
    let mut current_lane_idx: usize = 0;
    let mut node_index: usize = 0;

    // Track the index of the last *flow* node (i.e. not a note or swimlane
    // switch) so that notes can reference it.
    let node_gap = if diagram.is_old_style {
        OLD_STYLE_NODE_SPACING
    } else {
        NODE_SPACING
    };
    let mut last_flow_node_idx: Option<usize> = None;

    // --- Repeat / RepeatWhile tracking --------------------------------------
    // Stack of open `repeat` blocks.  Each entry holds the diamond1 node index
    // and the index of the first interior flow node (if any).  On `repeat
    // while`, the top of the stack is popped and a `LoopBackSimple2` edge is
    // emitted that snakes from the hex East side back to diamond1.
    //
    // Java's `FtileRepeat` layout compresses the inner sequence so that the
    // gap between the first two interior actions contains the mid-segment UP
    // arrow polygon; we compensate by bumping the gap before the second
    // interior node by `REPEAT_INNER_ARROW_GAP_EXTRA` when the loop-back is
    // active.
    struct BreakSource {
        from_node_idx: Option<usize>,
        from_y: f64,
    }
    struct GotoSource {
        label_name: String,
        from_node_idx: Option<usize>,
        from_y: f64,
    }
    struct RepeatFrame {
        diamond1_idx: usize,
        first_inner_idx: Option<usize>,
        second_inner_needs_extra: bool,
        backward_text: Option<String>,
        backward_width: f64,
        backward_height: f64,
        break_sources: Vec<BreakSource>,
        /// Event index of the Repeat event (for backward height lookup).
        repeat_event_idx: usize,
    }
    let mut repeat_stack: Vec<RepeatFrame> = Vec::new();
    // Deferred loop-back edges: filled when we hit `RepeatWhile` and then
    // appended after `build_edges` so the normal edges list stays linear.
    let mut repeat_loopbacks: Vec<(usize, usize)> = Vec::new();
    // Backward loop-backs: (hex_idx, diamond1_idx, backward_action_idx)
    let mut repeat_loopbacks_with_backward: Vec<(usize, usize, usize)> = Vec::new();
    // Deferred backward nodes: created after centering pass so x is correct.
    struct DeferredBackward {
        hex_idx: usize,
        diamond1_idx: usize,
        text: String,
        width: f64,
        height: f64,
    }
    let mut deferred_backward: Vec<DeferredBackward> = Vec::new();
    // Label positions and goto sources for goto/label feature.
    let mut label_positions: HashMap<String, f64> = HashMap::new();
    let mut goto_sources: Vec<GotoSource> = Vec::new();
    // --- If/Else/EndIf branch tracking ----------------------------------------
    // Stack of open `if` blocks. Each entry tracks the diamond node and
    // the nodes/edges for each branch (then / else).
    struct IfBranch {
        nodes: Vec<usize>, // node indices in this branch
        last_node_idx: Option<usize>,
        bottom_y: f64,   // y_cursor at end of branch
        has_goto: bool,  // branch ends with goto (no merge)
        has_break: bool, // branch ends with break
    }
    #[allow(dead_code)] // fields match Java structure
    struct IfFrame {
        diamond_idx: usize,
        diamond_cx: f64,       // center x of the diamond
        diamond_cy: f64,       // center y of the diamond
        diamond_left_x: f64,   // left point x of diamond
        diamond_right_x: f64,  // right point x of diamond
        diamond_bottom_y: f64, // bottom y of diamond
        then_label: String,
        else_label: String,
        has_else: bool,
        then_branch: IfBranch,
        else_branch: Option<IfBranch>,
        // y_cursor and last_flow_node_idx before the if-block
        saved_y_cursor: f64,
        saved_last_flow_node_idx: Option<usize>,
    }
    let mut if_stack: Vec<IfFrame> = Vec::new();
    // Deferred if-branch edges: generated after all nodes are placed.
    #[allow(dead_code)] // fields match Java structure
    struct DeferredIfEdges {
        diamond_idx: usize,
        diamond_cx: f64,
        diamond_cy: f64,
        diamond_left_x: f64,
        diamond_right_x: f64,
        diamond_bottom_y: f64,
        then_label: String,
        else_label: String,
        has_else: bool,
        then_branch: IfBranch,
        else_branch: Option<IfBranch>,
        merge_y: f64, // y where branches merge
        post_merge_node_idx: Option<usize>,
        /// The small 24×24 merge diamond drawn where the if-branches rejoin
        /// (Java's `FtileDiamond` merge). `None` for old-style / fallback.
        /// Branches route INTO this diamond; the diamond's bottom connects to
        /// the next flow node (non-nested) or to the enclosing switch's merge
        /// diamond (nested in a case).
        merge_diamond_idx: Option<usize>,
    }
    let mut deferred_if_edges: Vec<DeferredIfEdges> = Vec::new();

    // --- Switch/Case/EndSwitch branch tracking --------------------------------
    // Stack of open `switch` blocks.  Each frame tracks the diamond node and
    // N case branches (one per `case (label)`).  Like the if-frame, edges are
    // deferred to a post-placement pass so the diamond and all case nodes can
    // be connected with properly routed polylines.
    struct CaseBranch {
        label: String,
        nodes: Vec<usize>,
        last_node_idx: Option<usize>,
        bottom_y: f64,
        /// True when the last element added to this case is a nested
        /// `if`/`while` block (its entry diamond is in `nodes`). The nested
        /// machinery owns the case's exit edge, so the switch suppresses its
        /// own case-exit merge for this case. Reset to `false` whenever a
        /// direct action is subsequently claimed by the case.
        ends_with_nested: bool,
        /// When `ends_with_nested` via a nested `if`, the if's merge-diamond
        /// node index — the switch routes the case-exit from this diamond into
        /// the switch's own merge diamond. `None` for a nested `while` tail
        /// (the while machinery wires its own exit) and for non-nested cases.
        nested_exit_node_idx: Option<usize>,
    }
    struct SwitchFrame {
        diamond_idx: usize,
        diamond_cx: f64,
        diamond_bottom_y: f64,
        cases: Vec<CaseBranch>,
        saved_last_flow_node_idx: Option<usize>,
    }
    struct DeferredSwitchEdges {
        diamond_idx: usize,
        cases: Vec<CaseBranch>,
        merge_y: f64,
        /// The 24×24 merge diamond where all cases rejoin (Java's `FtileDiamond`
        /// merge). Each case's last node routes into it; its bottom connects to
        /// the next flow node.
        merge_diamond_idx: Option<usize>,
    }
    let mut switch_stack: Vec<SwitchFrame> = Vec::new();
    let mut deferred_switch_edges: Vec<DeferredSwitchEdges> = Vec::new();
    // Diamond indices that are the entry of a nested `if`/`while` block inside
    // a switch case. A node is claimed by exactly one layout machinery: when an
    // inner if/while frame is active, it owns the node and the switch case does
    // not. These diamonds ARE registered as the case's entry node so the switch
    // can connect `switch-diamond → entry-diamond` with the case label.
    let mut nested_entry_diamonds: std::collections::HashSet<usize> =
        std::collections::HashSet::new();
    // Deferred break edges: (from_node_idx, from_y, exit_diamond_idx)
    let mut deferred_break_edges: Vec<(usize, f64, usize)> = Vec::new();

    // While-loop tracking.  Each `while (cond) is (label)` opens a frame that
    // remembers the condition hexagon's geometry and collects the loop-body
    // node indices; the matching `endwhile (label)` closes it (without
    // creating a node) and records the loop-back (body bottom → while right)
    // plus the exit edge (while left → next) with the `is` / `endwhile`
    // labels.  Mirrors the if/else branch machinery: the while hexagon is a
    // two-way branch — down into the body (`is` label), left out to the next
    // flow node (`endwhile` label) — matching official PlantUML.
    struct WhileFrame {
        while_idx: usize,
        is_label: String,
        body_nodes: Vec<usize>,
    }
    let mut while_stack: Vec<WhileFrame> = Vec::new();
    // Closed while frames: (while_idx, body_nodes, is_label, endwhile_label, merge_y)
    let mut while_loopbacks: Vec<(usize, Vec<usize>, String, String, f64)> = Vec::new();
    // Pre-scan: for each `If` event, determine whether it has a matching `Else`.
    // This is needed so we can decide the branch layout direction up front.
    let if_has_else: HashMap<usize, bool> = {
        let mut map = HashMap::new();
        let mut if_stack_indices: Vec<usize> = Vec::new();
        for (ev_idx, ev) in events.iter().enumerate() {
            match ev {
                ActivityEvent::If { .. } => {
                    if_stack_indices.push(ev_idx);
                    map.insert(ev_idx, false);
                }
                ActivityEvent::Else { .. } => {
                    if let Some(&if_idx) = if_stack_indices.last() {
                        map.insert(if_idx, true);
                    }
                }
                ActivityEvent::EndIf => {
                    if_stack_indices.pop();
                }
                _ => {}
            }
        }
        map
    };
    // --- Old-style sync bar deferred placement ---
    // Pre-scan: find the LAST event index that references each sync bar name
    // (either SyncBar or GotoSyncBar). The bar is placed when we reach that event.
    let mut sync_bar_last_ref: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    // Count incoming references (GotoSyncBar) per sync bar name.
    // Also find the last incoming-reference event index for deferred placement.
    let mut sync_bar_goto_count: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    if diagram.is_old_style {
        for (ev_idx, ev) in events.iter().enumerate() {
            match ev {
                ActivityEvent::GotoSyncBar(name) => {
                    *sync_bar_goto_count.entry(name.clone()).or_insert(0) += 1;
                    sync_bar_last_ref.insert(name.clone(), ev_idx);
                }
                ActivityEvent::SyncBar(name) => {
                    // Include SyncBar in last_ref tracking only if there are
                    // also GotoSyncBar references — this is updated by
                    // GotoSyncBar above to the LAST incoming event.
                    // If no GotoSyncBar exists, the bar is placed immediately.
                    sync_bar_last_ref.entry(name.clone()).or_insert(ev_idx);
                }
                _ => {}
            }
        }
    }
    // For old-style diagrams, find the LAST Stop event index so intermediate
    // stops can be skipped (Java shares a single final stop node in DOT layout).
    let last_stop_idx: Option<usize> = if diagram.is_old_style {
        events
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e, ActivityEvent::Stop))
            .map(|(i, _)| i)
            .next_back()
    } else {
        None
    };

    // Pre-scan: find backward heights for repeat blocks.
    // This maps the event index of a Repeat event to the backward box height
    // (if the block contains a backward event).
    let mut repeat_backward_heights: HashMap<usize, f64> = HashMap::new();
    {
        let mut repeat_starts: Vec<usize> = Vec::new();
        for (ev_idx, ev) in events.iter().enumerate() {
            match ev {
                ActivityEvent::Repeat => {
                    repeat_starts.push(ev_idx);
                }
                ActivityEvent::Backward { text } => {
                    if let Some(&start_idx) = repeat_starts.last() {
                        let (_, bh) = estimate_text_size(text);
                        repeat_backward_heights.insert(start_idx, bh);
                    }
                }
                ActivityEvent::RepeatWhile { .. } => {
                    repeat_starts.pop();
                }
                _ => {}
            }
        }
    }

    // Deferred sync bar info: name → (pending, max_y_of_incoming_branches)
    let mut deferred_sync_bars: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    // Track placed sync bar y positions: name → y_below_bar
    let mut placed_sync_bars: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();

    // --- Partition frame tracking -------------------------------------------
    // Stack of open `partition` blocks.  Each entry records the frame top,
    // diffHeightTitle, title width, and the node_index at entry (so we can
    // scan inner nodes for the max content width on PartitionEnd).
    // Mirrors Java `FtileGroup` geometry: the frame rect is drawn at
    // (frame_top, frame_left) with size = (frame_width, frame_height).
    //   diffHeightTitle = max(25, titleH + 20)
    //   suppWidth = max(innerW, titleW + 20) - innerW   (innerW = contentW + 20)
    //   frame_width = innerW + suppWidth = max(contentW + 20, titleW + 20)
    //   frame_height = innerH + diffHeightTitle + diffYY2(20)
    //   innerH (post-Y-compression) = sum of inner tiles + gaps
    //   frame_bottom = last_inner_bottom + 12  (diffYY2=20, Y-compressed by 8)
    //   gap before frame = 10  (wide-open: the incoming arrow targets the
    //     first inner node deep inside the frame, so the Y-compression
    //     removes 25 of the 35px assembly gap, leaving 10)
    struct PartitionFrame {
        name: String,
        frame_top: f64,
        title_w: f64,
        first_node_idx: usize,
    }
    let mut partition_stack: Vec<PartitionFrame> = Vec::new();
    /// (first_inner_idx, partition_node_idx) for each finalized partition.
    /// Used to build a render_order that draws each frame before its content.
    let mut partition_ranges: Vec<(usize, usize)> = Vec::new();

    // ---- Fork parallel-branch state ----------------------------------------
    /// One entry per active `fork` block.  Pushed on `Fork`, popped on
    /// `EndFork`.
    struct ForkFrame {
        /// Index into `nodes` of the top fork bar.
        top_bar_idx: usize,
        /// Y position where the first branch starts (top bar bottom + gap).
        branch_start_y: f64,
        /// (first_node_idx, saved_last_flow_idx) for each branch.
        /// Pushed on Fork (branch 0) and ForkAgain (subsequent branches).
        branches: Vec<(usize, Option<usize>)>,
    }
    let mut fork_stack: Vec<ForkFrame> = Vec::new();
    /// Finalized fork groups: (top_bar_idx, bottom_bar_idx, branch_ranges).
    /// `branch_ranges` is a Vec of (first_node_idx, last_node_idx, max_width).
    /// Used for post-layout x-positioning and edge creation.
    struct ForkGroup {
        top_bar_idx: usize,
        bottom_bar_idx: usize,
        /// (first_node_idx, last_flow_idx, branch_max_width) per branch
        branches: Vec<(usize, Option<usize>, f64)>,
    }
    let mut fork_groups: Vec<ForkGroup> = Vec::new();

    for (event_idx, event) in events.iter().enumerate() {
        match event {
            // ---- Start circle ------------------------------------------------
            ActivityEvent::Start => {
                let diameter = 2.0 * START_RADIUS;
                let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                let x = cx - START_RADIUS;
                let y = y_cursor;
                log::debug!("  node[{node_index}] Start @ ({x:.1}, {y:.1})");
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::Start,
                    x,
                    y,
                    width: diameter,
                    height: diameter,
                    text: String::new(),
                    skip_in_flow: false,
                });
                last_flow_node_idx = Some(node_index);
                node_index += 1;
                y_cursor += diameter + node_gap;
            }

            // ---- Stop circle (Java FtileCircleStop: SIZE=22) ------------------
            ActivityEvent::Stop => {
                let ev_idx = event_idx;
                let is_intermediate = last_stop_idx.is_some_and(|last| ev_idx < last);
                if diagram.is_old_style && is_intermediate {
                    // Old-style: intermediate stops share the final stop node.
                    // Skip placing a visual node here.
                    log::debug!("  skipping intermediate Stop (old-style, ev_idx={ev_idx})");
                } else {
                    let diameter = 2.0 * STOP_RADIUS;
                    let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                    let x = cx - STOP_RADIUS;
                    let y = y_cursor;
                    log::debug!("  node[{node_index}] Stop @ ({x:.1}, {y:.1})");
                    nodes.push(ActivityNodeLayout {
                        index: node_index,
                        kind: ActivityNodeKindLayout::Stop,
                        x,
                        y,
                        width: diameter,
                        height: diameter,
                        text: String::new(),
                        skip_in_flow: false,
                    });
                    last_flow_node_idx = Some(node_index);
                    node_index += 1;
                    y_cursor += diameter + node_gap;
                }
            }

            // ---- Action box --------------------------------------------------
            ActivityEvent::Action { text } => {
                // When inside a `repeat` block, the gap between the first and
                // second interior actions is widened so that the loop-back
                // arrow's UP polygon (drawn along the right stretch) has room
                // to sit in the free slot without overlapping the tiles.
                // When a backward box is registered, the gap must accommodate
                // the backward box height instead.
                if let Some(frame) = repeat_stack.last_mut() {
                    if frame.second_inner_needs_extra {
                        if let Some(&bh) = repeat_backward_heights.get(&frame.repeat_event_idx) {
                            let extra = (bh.ceil() + 1.0 - node_gap).max(0.0);
                            y_cursor += extra;
                        } else {
                            y_cursor += REPEAT_INNER_ARROW_GAP_EXTRA;
                        }
                        frame.second_inner_needs_extra = false;
                    }
                }
                let (w, h) = estimate_text_size(text);
                let in_if = !if_stack.is_empty();
                let in_switch = !switch_stack.is_empty();
                let in_while = !while_stack.is_empty();
                let cx = if in_if {
                    let frame = if_stack.last().unwrap();
                    if frame.has_else {
                        // Determine which branch we're in
                        if frame.else_branch.is_some() {
                            // We're in the else branch (right)
                            frame.diamond_right_x + 10.0
                        } else {
                            // We're in the then branch (left)
                            frame.diamond_left_x - 10.0
                        }
                    } else {
                        // No else: then branch goes center-down
                        if frame.else_branch.is_none() {
                            frame.diamond_cx
                        } else {
                            frame.diamond_right_x + 10.0
                        }
                    }
                } else if in_switch {
                    // Inside a switch case: place at the diamond's centre for
                    // now; the centering pass spreads case branches horizontally.
                    switch_stack.last().unwrap().diamond_cx
                } else {
                    swimlane_center_x(&swimlane_layouts, current_lane_idx)
                };
                let x = cx - w / 2.0;
                let y = y_cursor;
                log::debug!(
                    "  node[{node_index}] Action \"{text}\" @ ({x:.1}, {y:.1}) {w}x{h}, in_if={in_if}, in_switch={in_switch}, in_while={in_while}"
                );
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::Action,
                    x,
                    y,
                    width: w,
                    height: h,
                    text: text.clone(),
                    skip_in_flow: in_if || in_switch || in_while,
                });
                // Track node in while-body
                if let Some(frame) = while_stack.last_mut() {
                    frame.body_nodes.push(node_index);
                }
                // Track node in if-branch
                if let Some(frame) = if_stack.last_mut() {
                    if let Some(ref mut else_branch) = frame.else_branch {
                        else_branch.nodes.push(node_index);
                    } else {
                        frame.then_branch.nodes.push(node_index);
                    }
                }
                // Track node in the current switch case branch — but only when
                // no inner if/while frame is active. The innermost branch
                // machinery owns the node; the switch case must not double-claim
                // it (that was the root cause of nested if/while-in-case
                // corrupting the layout: the same node was recorded by both,
                // producing duplicate edges and a centering clobber).
                if in_switch && !in_if && !in_while {
                    let frame = switch_stack.last_mut().unwrap();
                    if let Some(case) = frame.cases.last_mut() {
                        case.nodes.push(node_index);
                        case.ends_with_nested = false;
                    }
                }
                // Update repeat frame bookkeeping: record the first inner node
                // and arm the "next action needs extra gap" flag.
                if let Some(frame) = repeat_stack.last_mut() {
                    if frame.first_inner_idx.is_none() {
                        frame.first_inner_idx = Some(node_index);
                        frame.second_inner_needs_extra = true;
                    }
                }
                last_flow_node_idx = Some(node_index);
                node_index += 1;
                y_cursor += h + node_gap;
            }

            // ---- If / ElseIf / Else / EndIf → branching layout ----------------
            ActivityEvent::If {
                condition,
                then_label,
            } => {
                if diagram.is_old_style {
                    // Old-style: just a simple diamond (no branching)
                    let label = if then_label.is_empty() {
                        condition.clone()
                    } else {
                        format!("{condition}\n[{then_label}]")
                    };
                    let (w, h) = diamond_size(&label);
                    let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                    let x = cx - w / 2.0;
                    let y = y_cursor;
                    nodes.push(ActivityNodeLayout {
                        index: node_index,
                        kind: ActivityNodeKindLayout::Diamond,
                        x,
                        y,
                        width: w,
                        height: h,
                        text: label,
                        skip_in_flow: false,
                    });
                    last_flow_node_idx = Some(node_index);
                    node_index += 1;
                    y_cursor += h + node_gap;
                } else {
                    // New-style: hexagonal diamond with branching
                    let has_else = if_has_else.get(&event_idx).copied().unwrap_or(false);
                    let font_size = HEXAGON_LABEL_FONT_SIZE;
                    let cond_w =
                        font_metrics::text_width(condition, "SansSerif", font_size, false, false);
                    let hex_half = HEXAGON_HALF_SIZE;
                    let hex_w = cond_w.max(2.0 * hex_half) + 2.0 * hex_half;
                    let hex_h = 24.0_f64; // Java FtileDiamondInside default height
                    let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                    let hex_x = cx - hex_w / 2.0;
                    let hex_y = y_cursor;

                    let (left_label, right_label, bottom_label) = if has_else {
                        (then_label.clone(), String::new(), String::new())
                    } else {
                        (String::new(), String::new(), then_label.clone())
                    };
                    log::debug!("  node[{node_index}] IfDiamond @ ({hex_x:.1}, {hex_y:.1}) {hex_w:.1}x{hex_h:.1}, has_else={has_else}");
                    nodes.push(ActivityNodeLayout {
                        index: node_index,
                        kind: ActivityNodeKindLayout::IfDiamond {
                            left_label,
                            right_label: right_label.clone(),
                            bottom_label,
                        },
                        x: hex_x,
                        y: hex_y,
                        width: hex_w,
                        height: hex_h,
                        text: condition.clone(),
                        skip_in_flow: true, // edges built manually
                    });
                    let diamond_idx = node_index;
                    let diamond_left_x = hex_x;
                    let diamond_right_x = hex_x + hex_w;
                    let diamond_bottom_y = hex_y + hex_h;
                    // Consume repeat frame's second-inner extra gap flag (the
                    // if-diamond sits between the first and second interior
                    // actions, absorbing the extra gap).
                    if let Some(frame) = repeat_stack.last_mut() {
                        if frame.second_inner_needs_extra {
                            frame.second_inner_needs_extra = false;
                        }
                    }
                    node_index += 1;

                    // Push if-frame
                    if_stack.push(IfFrame {
                        diamond_idx,
                        diamond_cx: cx,
                        diamond_cy: hex_y + hex_h / 2.0,
                        diamond_left_x,
                        diamond_right_x,
                        diamond_bottom_y,
                        then_label: then_label.clone(),
                        else_label: right_label,
                        has_else,
                        then_branch: IfBranch {
                            nodes: Vec::new(),
                            last_node_idx: None,
                            bottom_y: diamond_bottom_y + 10.0,
                            has_goto: false,
                            has_break: false,
                        },
                        else_branch: None,
                        saved_y_cursor: y_cursor,
                        saved_last_flow_node_idx: last_flow_node_idx,
                    });

                    // When this if opens inside a switch case, the if-diamond is
                    // the case's entry node. Register it so the switch draws
                    // `switch-diamond → if-diamond` with the case label, and mark
                    // the case tail as a nested block so the switch defers the
                    // exit edge to the if machinery.
                    if let Some(frame) = switch_stack.last_mut() {
                        if let Some(case) = frame.cases.last_mut() {
                            case.nodes.push(diamond_idx);
                            case.ends_with_nested = true;
                            case.nested_exit_node_idx = None; // filled at EndIf
                        }
                        nested_entry_diamonds.insert(diamond_idx);
                    }

                    // Start the then-branch
                    // Java: FtileIfWithDiamonds positions branch tiles 10px below
                    // the diamond bottom (SUPP_WIDTH/2 = 10)
                    let branch_gap = 10.0;
                    if has_else {
                        y_cursor = diamond_bottom_y + branch_gap;
                    } else {
                        // Then goes center-down: extra gap for the then-label text below
                        y_cursor = diamond_bottom_y + node_gap + 4.4023;
                    }
                    last_flow_node_idx = Some(diamond_idx);
                }
            }

            ActivityEvent::ElseIf { condition, label } => {
                // Simplified: treat as sequential diamond (not fully branching)
                let combined = if label.is_empty() {
                    condition.clone()
                } else {
                    format!("{condition}\n[{label}]")
                };
                let (w, h) = diamond_size(&combined);
                let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                let x = cx - w / 2.0;
                let y = y_cursor;
                log::debug!("  node[{node_index}] ElseIf diamond @ ({x:.1}, {y:.1})");
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::Diamond,
                    x,
                    y,
                    width: w,
                    height: h,
                    text: combined,
                    skip_in_flow: false,
                });
                last_flow_node_idx = Some(node_index);
                node_index += 1;
                y_cursor += h + node_gap;
            }

            ActivityEvent::Else { label } => {
                if diagram.is_old_style {
                    log::debug!("  skipping Else diamond (old-style)");
                } else if let Some(frame) = if_stack.last_mut() {
                    // Finalize then-branch
                    frame.then_branch.bottom_y = y_cursor;
                    frame.then_branch.last_node_idx = last_flow_node_idx;
                    frame.else_label = label.clone();
                    // Update the diamond node's right_label
                    if let ActivityNodeKindLayout::IfDiamond {
                        ref mut right_label,
                        ..
                    } = nodes[frame.diamond_idx].kind
                    {
                        *right_label = label.clone();
                    }
                    log::debug!("  Else: then-branch finalized at y={y_cursor:.1}");

                    // Start else-branch from diamond bottom (10px gap)
                    frame.else_branch = Some(IfBranch {
                        nodes: Vec::new(),
                        last_node_idx: None,
                        bottom_y: frame.diamond_bottom_y + 10.0,
                        has_goto: false,
                        has_break: false,
                    });
                    y_cursor = frame.diamond_bottom_y + 10.0;
                    last_flow_node_idx = Some(frame.diamond_idx);
                } else {
                    // Fallback: simple diamond
                    let text = if label.is_empty() {
                        "else".to_string()
                    } else {
                        format!("[{label}]")
                    };
                    let (w, h) = diamond_size(&text);
                    let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                    let x = cx - w / 2.0;
                    let y = y_cursor;
                    nodes.push(ActivityNodeLayout {
                        index: node_index,
                        kind: ActivityNodeKindLayout::Diamond,
                        x,
                        y,
                        width: w,
                        height: h,
                        text,
                        skip_in_flow: false,
                    });
                    last_flow_node_idx = Some(node_index);
                    node_index += 1;
                    y_cursor += h + node_gap;
                }
            }

            ActivityEvent::EndIf => {
                if diagram.is_old_style {
                    log::debug!("  skipping EndIf diamond (old-style)");
                } else if let Some(mut frame) = if_stack.pop() {
                    // Finalize the current branch
                    if let Some(ref mut else_branch) = frame.else_branch {
                        else_branch.bottom_y = y_cursor;
                        else_branch.last_node_idx = last_flow_node_idx;
                    } else {
                        frame.then_branch.bottom_y = y_cursor;
                        frame.then_branch.last_node_idx = last_flow_node_idx;
                    }

                    // `merge_y` is the branch merge level used for the
                    // no-merge-diamond path (next-node placement and the
                    // else→next edge).  It tracks `y_cursor` (carrying the
                    // trailing `node_gap`), which the break path below relies
                    // on.  The merge *diamond* (when present) is positioned off
                    // the branches' actual last-node bottoms instead — see
                    // `md_y` below — because Java's `FtileGeometry.outY` is the
                    // bottom of the last action with no trailing gap.
                    let then_bottom = frame.then_branch.bottom_y;
                    let else_bottom = frame
                        .else_branch
                        .as_ref()
                        .map(|b| b.bottom_y)
                        .unwrap_or(frame.diamond_bottom_y);
                    let then_node_bottom = frame
                        .then_branch
                        .nodes
                        .last()
                        .map(|&idx| nodes[idx].y + nodes[idx].height);
                    let else_node_bottom = frame
                        .else_branch
                        .as_ref()
                        .and_then(|b| b.nodes.last())
                        .map(|&idx| nodes[idx].y + nodes[idx].height);
                    let merge_y = if frame.then_branch.has_break && frame.else_branch.is_none() {
                        // The else merge y needs to account for break path
                        let break_y = then_bottom - node_gap + 6.5157;
                        let else_merge_y = break_y + 17.0;
                        else_merge_y.max(then_bottom)
                    } else {
                        then_bottom.max(else_bottom)
                    };

                    // Java `ConditionalBuilder.createWithLinks` emits a merge
                    // diamond (`FtileDiamond` 24x24, `getShape2`) at the branch
                    // merge point when both branches have a normal out-point
                    // (`hasTwoBranches`).  A branch ending in `break`/`goto`
                    // has no out-point, so no merge diamond is drawn.
                    let branch_clean = |b: &IfBranch| !b.has_break && !b.has_goto;
                    let normal_merge = frame.has_else
                        && branch_clean(&frame.then_branch)
                        && frame
                            .else_branch
                            .as_ref()
                            .map(branch_clean)
                            .unwrap_or(false);

                    let merge_diamond_idx = if normal_merge {
                        // Merge diamond sits `getYdelta1b` (6px) below the
                        // longer branch's *actual* last-node bottom (Java
                        // `FtileGeometry.outY` — no trailing gap); its centre is
                        // the if-block's out-point on the centreline.
                        let longer_bottom = then_node_bottom
                            .unwrap_or(merge_y)
                            .max(else_node_bottom.unwrap_or(merge_y));
                        let md_y = longer_bottom + 6.0;
                        log::debug!(
                            "  node[{node_index}] MergeDiamond @ (placeholder, {md_y:.1}) 24x24"
                        );
                        nodes.push(ActivityNodeLayout {
                            index: node_index,
                            kind: ActivityNodeKindLayout::Diamond,
                            x: 0.0, // placeholder — repositioned to cx-12 in Pass 2b
                            y: md_y,
                            width: 24.0,
                            height: 24.0,
                            text: String::new(),
                            skip_in_flow: true, // edges built manually in Pass 3a
                        });
                        let md_idx = node_index;
                        node_index += 1;
                        // Next flow node sits one node_gap below the merge diamond.
                        y_cursor = md_y + 24.0 + node_gap;
                        Some(md_idx)
                    } else {
                        // No merge diamond: next node sits at merge_y + node_gap.
                        y_cursor = merge_y + node_gap;
                        None
                    };
                    last_flow_node_idx = frame.saved_last_flow_node_idx;

                    log::debug!(
                        "  EndIf: merge_y={merge_y:.1}, then_bottom={then_bottom:.1}, else_bottom={else_bottom:.1}, merge_diamond={merge_diamond_idx:?}"
                    );

                    deferred_if_edges.push(DeferredIfEdges {
                        diamond_idx: frame.diamond_idx,
                        diamond_cx: frame.diamond_cx,
                        diamond_cy: frame.diamond_cy,
                        diamond_left_x: frame.diamond_left_x,
                        diamond_right_x: frame.diamond_right_x,
                        diamond_bottom_y: frame.diamond_bottom_y,
                        then_label: frame.then_label,
                        else_label: frame.else_label,
                        has_else: frame.has_else,
                        then_branch: frame.then_branch,
                        else_branch: frame.else_branch,
                        merge_y,
                        post_merge_node_idx: None, // filled later
                        merge_diamond_idx,
                    });

                    // If this if was nested in a switch case, record its merge
                    // diamond as the case's nested-exit node so the switch
                    // routes the case-exit from it into the switch merge diamond.
                    if nested_entry_diamonds.contains(&frame.diamond_idx) {
                        if let Some(switch_frame) = switch_stack.last_mut() {
                            if let Some(case) = switch_frame.cases.last_mut() {
                                case.nested_exit_node_idx = merge_diamond_idx;
                            }
                        }
                    }
                } else {
                    // Fallback: simple diamond
                    let (w, h) = (DIAMOND_SIZE * 2.0, DIAMOND_SIZE * 2.0);
                    let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                    let x = cx - w / 2.0;
                    let y = y_cursor;
                    nodes.push(ActivityNodeLayout {
                        index: node_index,
                        kind: ActivityNodeKindLayout::Diamond,
                        x,
                        y,
                        width: w,
                        height: h,
                        text: String::new(),
                        skip_in_flow: false,
                    });
                    last_flow_node_idx = Some(node_index);
                    node_index += 1;
                    y_cursor += h + node_gap;
                }
            }

            // ---- Switch / Case / EndSwitch → multi-way branch ----------------
            ActivityEvent::Switch { condition } => {
                if !switch_stack.is_empty() {
                    log::warn!(
                        "switch nested inside a case is not fully supported: the \
                         nested switch's width is not counted in the case column \
                         (issue #39)"
                    );
                }
                if !swimlane_layouts.is_empty() {
                    log::warn!(
                        "switch inside a swimlane is not fully supported: case \
                         columns are not spread and may stack (issue #39)"
                    );
                }
                // NOTE: unlike If/While, a nested Switch does not register itself
                // onto the parent case's `case.nodes`, so its tile width is not
                // accounted for in `case_tile_geometry`. See the warn above.
                // Hexagonal diamond with the condition text inside (same shape
                // and sizing as the if-diamond, matching official PlantUML).
                let font_size = HEXAGON_LABEL_FONT_SIZE;
                let cond_w =
                    font_metrics::text_width(condition, "SansSerif", font_size, false, false);
                let hex_half = HEXAGON_HALF_SIZE;
                let (w, h) = (cond_w.max(2.0 * hex_half) + 2.0 * hex_half, 24.0_f64);
                let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                let x = cx - w / 2.0;
                let y = y_cursor;
                log::debug!(
                    "  node[{node_index}] SwitchDiamond \"{condition}\" @ ({x:.1}, {y:.1}) {w}x{h}"
                );
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::SwitchDiamond,
                    x,
                    y,
                    width: w,
                    height: h,
                    text: condition.clone(),
                    skip_in_flow: true, // edges built manually in deferred pass
                });
                let diamond_idx = node_index;
                node_index += 1;

                switch_stack.push(SwitchFrame {
                    diamond_idx,
                    diamond_cx: cx,
                    diamond_bottom_y: y + h,
                    cases: Vec::new(),
                    saved_last_flow_node_idx: last_flow_node_idx,
                });

                // First case starts below the diamond.  Java source:
                //   getYdelta1a = max(10, max_case_label_height) + 10
                //     (FtileSwitchWithManyLinks.getYdelta1a)
                //   FtileDecorateInLabel adds label_h to the tile top
                //   → case_start = diamond_bottom + getYdelta1a + label_h
                //     = diamond_bottom + max(10, label_h) + 10 + label_h
                let case_label_h =
                    font_metrics::line_height("SansSerif", HEXAGON_LABEL_FONT_SIZE, false, false);
                let switch_ydelta1a = case_label_h.max(10.0) + 10.0;
                y_cursor = y + h + switch_ydelta1a + case_label_h;
                last_flow_node_idx = Some(diamond_idx);
            }

            ActivityEvent::Case { label } => {
                if let Some(frame) = switch_stack.last_mut() {
                    // Finalize the previous case's bottom_y (if any).
                    if let Some(prev) = frame.cases.last_mut() {
                        prev.bottom_y = y_cursor;
                        prev.last_node_idx = last_flow_node_idx;
                    }
                    // Start a new case branch.  All case action nodes are
                    // initially placed at the diamond's centre x; the
                    // centering pass spreads them horizontally.
                    let case_label_h = font_metrics::line_height(
                        "SansSerif",
                        HEXAGON_LABEL_FONT_SIZE,
                        false,
                        false,
                    );
                    let switch_ydelta1a = case_label_h.max(10.0) + 10.0;
                    let case_start_y = frame.diamond_bottom_y + switch_ydelta1a + case_label_h;
                    frame.cases.push(CaseBranch {
                        label: label.clone(),
                        nodes: Vec::new(),
                        last_node_idx: None,
                        bottom_y: case_start_y,
                        ends_with_nested: false,
                        nested_exit_node_idx: None,
                    });
                    // Reset y_cursor to the case start so every branch begins
                    // at the same vertical position (branches are side by side).
                    y_cursor = case_start_y;
                    last_flow_node_idx = Some(frame.diamond_idx);
                    log::debug!("  Case \"{label}\" started at y={y_cursor:.1}");
                }
            }

            ActivityEvent::EndSwitch => {
                if let Some(mut frame) = switch_stack.pop() {
                    // Finalize the last case.
                    if let Some(last_case) = frame.cases.last_mut() {
                        last_case.bottom_y = y_cursor;
                        last_case.last_node_idx = last_flow_node_idx;
                    }
                    // Merge y = max of all case bottom_y values (used for the
                    // next-flow-node lookup).
                    let merge_y = frame
                        .cases
                        .iter()
                        .map(|c| c.bottom_y)
                        .fold(frame.diamond_bottom_y, f64::max);
                    // The switch merge diamond sits below the lowest case
                    // content.  Java source:
                    //   getTranslateDiamond2: y2 = dimTotal.height - dimDiamond2.height
                    //   dimTotal.height = dim1.h + dimNude.h + dim2.h + deltaHeight
                    //   deltaHeight = getYdelta1a + getYdelta1b + getYdeltaForLabels
                    //   → merge_y = dim1.bottom + dimNude.h + getYdelta1a + getYdelta1b + getYdeltaForLabels
                    // In our y_cursor model: bottom_y = case_start + action_h + node_gap
                    //   → merge_y = bottom_y - node_gap + getYdelta1b + getYdeltaForLabels
                    // getYdelta1b = 10 (Java), getYdeltaForLabels ≈ 10 (measured).
                    let md_w = HEXAGON_HALF_SIZE * 2.0; // 24
                    let merge_diamond_y = frame
                        .cases
                        .iter()
                        .map(|c| {
                            // bottom_y = content_bottom + node_gap.
                            // merge_y = bottom_y - node_gap + 20 (getYdelta1b(10) + getYdeltaForLabels(10))
                            c.bottom_y - node_gap + 20.0
                        })
                        .fold(frame.diamond_bottom_y, f64::max);
                    let merge_diamond_idx = node_index;
                    nodes.push(ActivityNodeLayout {
                        index: node_index,
                        kind: ActivityNodeKindLayout::Diamond,
                        x: frame.diamond_cx - md_w / 2.0,
                        y: merge_diamond_y,
                        width: md_w,
                        height: md_w,
                        text: String::new(),
                        skip_in_flow: true, // edges built manually
                    });
                    node_index += 1;
                    y_cursor = merge_diamond_y + md_w + node_gap;
                    last_flow_node_idx = frame.saved_last_flow_node_idx;
                    log::debug!(
                        "  EndSwitch: merge_y={merge_y:.1}, cases={}, merge-diamond={merge_diamond_idx}",
                        frame.cases.len()
                    );
                    deferred_switch_edges.push(DeferredSwitchEdges {
                        diamond_idx: frame.diamond_idx,
                        cases: frame.cases,
                        merge_y,
                        merge_diamond_idx: Some(merge_diamond_idx),
                    });
                }
            }

            // ---- While / EndWhile → diamonds ---------------------------------
            ActivityEvent::While { condition, label } => {
                // Hexagonal diamond (Java's `FtileDiamondInside`), same shape
                // as the if/switch diamond.  The condition lives inside; the
                // `is (label)` is rendered on the entry arrow (loop-back pass).
                let font_size = HEXAGON_LABEL_FONT_SIZE;
                let cond_w =
                    font_metrics::text_width(condition, "SansSerif", font_size, false, false);
                let hex_half = HEXAGON_HALF_SIZE;
                let (w, h) = (cond_w.max(2.0 * hex_half) + 2.0 * hex_half, 24.0_f64);
                let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                let x = cx - w / 2.0;
                let y = y_cursor;
                // When nested in a switch case the while-diamond is the case
                // entry; its incoming edge is drawn by the switch (with the
                // case label), so exclude it from build_edges (like the
                // if-diamond). Plain while keeps skip_in_flow=false so
                // build_edges still wires its incoming edge.
                let while_in_switch = !switch_stack.is_empty();
                log::debug!("  node[{node_index}] While diamond @ ({x:.1}, {y:.1}) in_switch={while_in_switch}");
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::IfDiamond {
                        left_label: String::new(),
                        right_label: String::new(),
                        bottom_label: String::new(),
                    },
                    x,
                    y,
                    width: w,
                    height: h,
                    text: condition.clone(),
                    skip_in_flow: while_in_switch,
                });
                while_stack.push(WhileFrame {
                    while_idx: node_index,
                    is_label: label.clone(),
                    body_nodes: Vec::new(),
                });
                let while_idx = node_index;
                // When this while opens inside a switch case, the while-diamond
                // is the case's entry node; the while machinery owns the exit.
                if let Some(frame) = switch_stack.last_mut() {
                    if let Some(case) = frame.cases.last_mut() {
                        case.nodes.push(while_idx);
                        case.ends_with_nested = true;
                        case.nested_exit_node_idx = None; // while wires its own exit
                    }
                    nested_entry_diamonds.insert(while_idx);
                }
                last_flow_node_idx = Some(node_index);
                node_index += 1;
                // The while→body gap = getYdelta1a (Java FtileIfWithDiamonds:
                // 10 for no swimlanes) + node_gap.  Official PlantUML places
                // the body 10px below the normal node gap to accommodate the
                // `is` label on the entry arrow.
                let while_ydelta1a = 10.0; // FtileIfWithDiamonds.getYdelta1a (no swimlanes)
                y_cursor += h + node_gap + while_ydelta1a;
            }

            ActivityEvent::EndWhile { label } => {
                // `endwhile` does NOT create a node — official PlantUML treats
                // the while hexagon as a two-way branch (down into the body,
                // left out to the next flow node), so the exit path leaves from
                // the while's left side, not from a separate merge diamond.
                if let Some(frame) = while_stack.pop() {
                    // merge_y = bottom of the body (or while bottom if empty),
                    // used to route the exit edge's horizontal segment.
                    let body_bottom = frame
                        .body_nodes
                        .last()
                        .and_then(|&i| nodes.get(i))
                        .map(|n| n.y + n.height)
                        .unwrap_or_else(|| {
                            nodes
                                .get(frame.while_idx)
                                .map(|n| n.y + n.height)
                                .unwrap_or(y_cursor)
                        });
                    let merge_y = body_bottom + node_gap;
                    while_loopbacks.push((
                        frame.while_idx,
                        frame.body_nodes,
                        frame.is_label,
                        label.clone(),
                        merge_y,
                    ));
                    // Reserve vertical space for the exit/loop-back routing so
                    // the next flow node is placed below the merge band.
                    y_cursor = merge_y + node_gap;
                }
                // last_flow_node_idx stays as the while diamond (the branch
                // point) so the next flow node after endwhile connects from it.
            }

            // ---- Repeat / RepeatWhile → diamond at end -----------------------
            ActivityEvent::Repeat => {
                // `repeat` start: Java `FtileDiamond` — small empty square
                // diamond sized 24×24 (`Hexagon.hexagonHalfSize * 2`).
                let (w, h) = (HEXAGON_HALF_SIZE * 2.0, HEXAGON_HALF_SIZE * 2.0);
                let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                let x = cx - w / 2.0;
                let y = y_cursor;
                log::debug!("  node[{node_index}] Repeat diamond @ ({x:.1}, {y:.1})");
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::Diamond,
                    x,
                    y,
                    width: w,
                    height: h,
                    text: String::new(),
                    skip_in_flow: false,
                });
                // Open a repeat frame so subsequent interior flow nodes can be
                // tracked for the loop-back edge.
                repeat_stack.push(RepeatFrame {
                    diamond1_idx: node_index,
                    first_inner_idx: None,
                    second_inner_needs_extra: false,
                    backward_text: None,
                    backward_width: 0.0,
                    backward_height: 0.0,
                    break_sources: Vec::new(),
                    repeat_event_idx: event_idx,
                });
                last_flow_node_idx = Some(node_index);
                node_index += 1;
                y_cursor += h + node_gap;
            }

            ActivityEvent::RepeatWhile {
                condition,
                is_text,
                not_text,
                ..
            } => {
                // Java `FtileDiamondInside`: hexagonal shape sized
                // (label_w + 24) × max(label_h, 24) where the test condition
                // sits inside.  An optional `is (label)` becomes the East
                // label, rendered to the right.
                let cond_w = font_metrics::text_width(
                    condition,
                    "SansSerif",
                    HEXAGON_LABEL_FONT_SIZE,
                    false,
                    false,
                );
                let cond_h = if condition.is_empty() {
                    0.0
                } else {
                    font_metrics::line_height("SansSerif", HEXAGON_LABEL_FONT_SIZE, false, false)
                };
                let hex_w = if cond_w == 0.0 || cond_h == 0.0 {
                    HEXAGON_HALF_SIZE * 2.0
                } else {
                    cond_w.max(HEXAGON_HALF_SIZE * 2.0) + HEXAGON_HALF_SIZE * 2.0
                };
                let hex_h = if cond_w == 0.0 || cond_h == 0.0 {
                    HEXAGON_HALF_SIZE * 2.0
                } else {
                    cond_h.max(HEXAGON_HALF_SIZE * 2.0)
                };

                let east_lines: Vec<String> = match is_text {
                    Some(label) => split_label_lines(label),
                    None => Vec::new(),
                };
                let south_lines: Vec<String> = match not_text {
                    Some(label) => split_label_lines(label),
                    None => Vec::new(),
                };

                // East label clears space ABOVE the hexagon.  Java's
                // `FtileRepeat` builds in `8 * hexagonHalfSize = 96` total
                // padding around the inner sequence so the East label
                // (centered vertically on the hexagon) overlaps the 48 px
                // above-hex gap.  Java then Y-compresses empty slots down to
                // the default 20 px arrow gap.  We emulate this by only
                // keeping the extra cushion when the label actually extends
                // above the hex top — i.e. when label_h/2 exceeds hex_h/2.
                if !east_lines.is_empty() {
                    let east_line_h = font_metrics::line_height(
                        "SansSerif",
                        HEXAGON_LABEL_FONT_SIZE,
                        false,
                        false,
                    );
                    let east_label_h = east_line_h * east_lines.len() as f64;
                    // `ext_above` = how far the centered label's top extends
                    // above the hex top.  When the label sits entirely
                    // between hex_center and hex_bottom Java compresses the
                    // 48 px gap down to the normal arrow gap and we emit no
                    // extra.  When the label pushes past the hex top we keep
                    // the full 48 px cushion (= 20 px arrow + 28 px extra).
                    let ext_above = (east_label_h - hex_h) / 2.0;
                    if ext_above > HEXAGON_HALF_SIZE / 2.0 {
                        y_cursor += REPEAT_WHILE_EXTRA_GAP;
                    }
                }

                let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                let x = cx - hex_w / 2.0;
                let y = y_cursor;
                log::debug!(
                    "  node[{node_index}] RepeatWhile hexagon @ ({x:.1}, {y:.1}) {hex_w}x{hex_h} east_lines={east_lines:?}"
                );
                // Compute south label height before moving south_lines.
                // Java: the south label is drawn at hex bottom and occupies
                // ~ascent + partial descent worth of vertical space.
                // The exact amount comes from Java's Y-compression pass
                // which squeezes the 96px padding down to the used space.
                let south_label_extra = if !south_lines.is_empty() {
                    let ascent =
                        font_metrics::ascent("SansSerif", HEXAGON_LABEL_FONT_SIZE, false, false);
                    // Java Y-compression gives south text extra ≈ ascent * 1.147
                    // per line, matching the observed reference output.
                    (ascent * 1.1469) * south_lines.len() as f64
                } else {
                    0.0
                };
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::Hexagon {
                        east_lines,
                        south_lines,
                    },
                    x,
                    y,
                    width: hex_w,
                    height: hex_h,
                    text: condition.clone(),
                    skip_in_flow: false,
                });
                let hex_idx = node_index;
                last_flow_node_idx = Some(hex_idx);
                node_index += 1;
                y_cursor += hex_h + south_label_extra + node_gap;

                // Close the repeat frame and schedule the loop-back edge.
                if let Some(frame) = repeat_stack.pop() {
                    // If there are break sources, create an exit diamond node.
                    let has_breaks = !frame.break_sources.is_empty();
                    let exit_diamond_idx = if has_breaks {
                        // Java's exit diamond is hexagonHalfSize*2 = 24x24
                        let (dw, dh) = (HEXAGON_HALF_SIZE * 2.0, HEXAGON_HALF_SIZE * 2.0);
                        let dx = cx - dw / 2.0;
                        let dy = y_cursor; // place after hex+south gap+node_gap
                        let exit_y = dy;
                        log::debug!(
                            "  node[{node_index}] Repeat exit diamond @ ({dx:.1}, {exit_y:.1})"
                        );
                        nodes.push(ActivityNodeLayout {
                            index: node_index,
                            kind: ActivityNodeKindLayout::Diamond,
                            x: dx,
                            y: exit_y,
                            width: dw,
                            height: dh,
                            text: String::new(),
                            skip_in_flow: false,
                        });
                        let idx = node_index;
                        node_index += 1;
                        y_cursor = exit_y + dh + node_gap;
                        Some(idx)
                    } else {
                        None
                    };

                    if let Some(text) = frame.backward_text {
                        deferred_backward.push(DeferredBackward {
                            hex_idx,
                            diamond1_idx: frame.diamond1_idx,
                            text,
                            width: frame.backward_width,
                            height: frame.backward_height,
                        });
                        log::debug!(
                            "  closing repeat frame with backward (deferred): diamond1={}, hex={}",
                            frame.diamond1_idx,
                            hex_idx
                        );
                    } else {
                        log::debug!(
                            "  closing repeat frame: diamond1={}, hex={}",
                            frame.diamond1_idx,
                            hex_idx
                        );
                        repeat_loopbacks.push((hex_idx, frame.diamond1_idx));
                    }

                    // Store deferred break edges for post-centering resolution
                    if has_breaks {
                        if let Some(exit_idx) = exit_diamond_idx {
                            for bs in &frame.break_sources {
                                if let Some(from_idx) = bs.from_node_idx {
                                    deferred_break_edges.push((from_idx, bs.from_y, exit_idx));
                                }
                            }
                        }
                    }
                }
            }

            // ---- Fork → top bar + start branch 0 ---------------------------
            // Issue #39: a fork nested inside a switch case is not fully
            // supported — the fork bar's width is not counted in the case column.
            ActivityEvent::Fork => {
                if !switch_stack.is_empty() {
                    log::warn!(
                        "fork nested inside a case is not fully supported: the \
                         fork bar's width is not counted in the case column \
                         (issue #39)"
                    );
                }
                let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                let bar_y = y_cursor;

                // Top fork bar (FtileBlackBlock: fill #555555, rounded 5).
                // Width is provisional — set by the post-layout fork pass.
                let top_bar_idx = nodes.len();
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::ForkBar,
                    x: cx, // provisional center; will be set by fork pass
                    y: bar_y,
                    width: FORK_BAR_WIDTH, // provisional; overwritten later
                    height: FORK_BAR_HEIGHT,
                    text: String::new(),
                    skip_in_flow: true,
                });
                node_index += 1;

                let branch_start_y = bar_y + FORK_BAR_HEIGHT + node_gap;
                y_cursor = branch_start_y;

                fork_stack.push(ForkFrame {
                    top_bar_idx,
                    branch_start_y,
                    branches: vec![(node_index, last_flow_node_idx)],
                });
                // The fork bar is the last flow node for edge purposes
                // (entry edge connects to it).
                last_flow_node_idx = Some(top_bar_idx);
                log::debug!("  Fork: top bar @ y={bar_y:.1}, branch_start_y={branch_start_y:.1}");
            }

            // ---- ForkAgain → end branch, start new branch -------------------
            ActivityEvent::ForkAgain => {
                let ff = fork_stack
                    .last_mut()
                    .ok_or_else(|| Error::Layout("ForkAgain without matching Fork".into()))?;
                // Record the current branch's end.
                let branch_idx = ff.branches.len() - 1;
                ff.branches[branch_idx].1 = last_flow_node_idx;
                // Reset y_cursor to branch start for the next branch.
                y_cursor = ff.branch_start_y;
                // Start a new branch.
                ff.branches.push((node_index, last_flow_node_idx));
                // last_flow_node_idx will be set when the new branch's
                // first node is pushed.
                last_flow_node_idx = Some(ff.top_bar_idx);
                log::debug!(
                    "  ForkAgain: branch {} ended, starting branch {}",
                    branch_idx,
                    ff.branches.len() - 1
                );
            }

            // ---- EndFork → bottom bar + finalize ---------------------------
            ActivityEvent::EndFork => {
                let ff = fork_stack
                    .pop()
                    .ok_or_else(|| Error::Layout("EndFork without matching Fork".into()))?;
                // Record the last branch's end.
                let last_branch = ff.branches.len() - 1;
                let mut branches = ff.branches;
                branches[last_branch].1 = last_flow_node_idx;

                // Find max branch bottom.
                // Partition frame nodes extend below the last flow node
                // (by the bottom margin), so we must include them.
                let bottom_bar_idx_placeholder = nodes.len();
                let mut max_bottom = ff.branch_start_y;
                for (i, &(_start, end)) in branches.iter().enumerate() {
                    let range_end = if i + 1 < branches.len() {
                        branches[i + 1].0
                    } else {
                        bottom_bar_idx_placeholder
                    };
                    let lower = end.unwrap_or(_start);
                    for n in &nodes[lower..range_end.min(nodes.len())] {
                        let bottom = n.y + n.height;
                        if bottom > max_bottom {
                            max_bottom = bottom;
                        }
                    }
                }

                // Compute branch max widths.
                // Each branch's range extends to the next branch's start (or
                // to the bottom fork bar) so that partition frame nodes —
                // which sit between the last flow node and the next branch —
                // are included in the width computation.
                let bottom_bar_idx_placeholder = nodes.len();
                let mut branch_info: Vec<(usize, Option<usize>, f64)> = Vec::new();
                for (i, &(start, end)) in branches.iter().enumerate() {
                    let mut max_w = 0.0_f64;
                    let end_idx = if i + 1 < branches.len() {
                        branches[i + 1].0
                    } else {
                        bottom_bar_idx_placeholder
                    };
                    for n in &nodes[start..end_idx.min(nodes.len())] {
                        if (is_flow_node(&n.kind) && !n.skip_in_flow)
                            || matches!(n.kind, ActivityNodeKindLayout::Partition)
                        {
                            if n.width > max_w {
                                max_w = n.width;
                            }
                        }
                    }
                    branch_info.push((start, end, max_w));
                }

                // Bottom fork bar.
                let bottom_bar_y = max_bottom + node_gap;
                let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                let bottom_bar_idx = nodes.len();
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::ForkBar,
                    x: cx, // provisional
                    y: bottom_bar_y,
                    width: FORK_BAR_WIDTH, // provisional
                    height: FORK_BAR_HEIGHT,
                    text: String::new(),
                    skip_in_flow: true,
                });
                node_index += 1;

                fork_groups.push(ForkGroup {
                    top_bar_idx: ff.top_bar_idx,
                    bottom_bar_idx,
                    branches: branch_info,
                });

                y_cursor = bottom_bar_y + FORK_BAR_HEIGHT + node_gap;
                // The bottom bar becomes the last flow node for the
                // next sequential edge.
                last_flow_node_idx = Some(bottom_bar_idx);
                log::debug!(
                    "  EndFork: bottom bar @ y={bottom_bar_y:.1}, {} branches",
                    branches.len()
                );
            }

            // ---- Swimlane switch (no node) -----------------------------------
            ActivityEvent::Swimlane { name } => {
                let idx = resolve_swimlane_index(&diagram.swimlanes, name);
                log::debug!("  swimlane switch -> \"{name}\" (idx={idx})");
                current_lane_idx = idx;
                // No node emitted, no y_cursor change.
            }

            // ---- Note (attached to previous flow node) -----------------------
            ActivityEvent::Note { position, text } => {
                let wrapped = if let Some(max_w) = diagram.note_max_width {
                    wrap_note_text(text, max_w)
                } else {
                    text.clone()
                };
                let (nw, nh) = estimate_note_size(&wrapped);
                let pos_layout = match position {
                    NotePosition::Left => NotePositionLayout::Left,
                    NotePosition::Right => NotePositionLayout::Right,
                };

                // Java vertically centres the note and its flow node.
                // When the note is taller than the flow node, both are
                // shifted so their midpoints align.
                let (nx, ny) = if let Some(prev_idx) = last_flow_node_idx {
                    let prev_x = nodes[prev_idx].x;
                    let prev_y = nodes[prev_idx].y;
                    let prev_w = nodes[prev_idx].width;
                    let prev_h = nodes[prev_idx].height;
                    let x = match pos_layout {
                        NotePositionLayout::Right => prev_x + prev_w + NOTE_OFFSET,
                        NotePositionLayout::Left => prev_x - NOTE_OFFSET - nw,
                    };

                    if nh > prev_h {
                        // Note is taller: push the flow node down so midpoints align
                        let delta = (nh - prev_h) / 2.0;
                        nodes[prev_idx].y += delta;
                        y_cursor += delta;
                        // Note y = original flow-node y (unshifted)
                        (x, prev_y)
                    } else {
                        // Flow node is taller: centre the note on the flow node
                        let delta = (prev_h - nh) / 2.0;
                        (x, prev_y + delta)
                    }
                } else {
                    // No previous node — place in the margin area.
                    let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                    let x = match pos_layout {
                        NotePositionLayout::Right => cx + NOTE_OFFSET,
                        NotePositionLayout::Left => cx - NOTE_OFFSET - nw,
                    };
                    (x, y_cursor)
                };

                log::debug!("  node[{node_index}] Note({pos_layout:?}) @ ({nx:.1}, {ny:.1})");
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::Note {
                        position: pos_layout,
                        mode: ActivityNoteModeLayout::Grouped,
                    },
                    x: nx,
                    y: ny,
                    width: nw,
                    height: nh,
                    text: wrapped,
                    skip_in_flow: false,
                });
                // Notes do NOT update last_flow_node_idx.
                node_index += 1;

                // Advance y_cursor so subsequent elements don't overlap.
                let note_bottom = ny + nh + node_gap;
                if note_bottom > y_cursor {
                    y_cursor = note_bottom;
                }
            }

            // ---- Floating note (not attached) --------------------------------
            // Java: floating notes sit beside the flow, like attached notes.
            // They do NOT consume vertical space or advance y_cursor.
            ActivityEvent::FloatingNote { position, text } => {
                let wrapped = if let Some(max_w) = diagram.note_max_width {
                    wrap_note_text(text, max_w)
                } else {
                    text.clone()
                };
                let (nw, nh) = estimate_note_size(&wrapped);
                let pos_layout = match position {
                    NotePosition::Left => NotePositionLayout::Left,
                    NotePosition::Right => NotePositionLayout::Right,
                };
                let (nx, ny) = if let Some(prev_idx) = last_flow_node_idx {
                    let prev_x = nodes[prev_idx].x;
                    let prev_y = nodes[prev_idx].y;
                    let prev_w = nodes[prev_idx].width;
                    let prev_h = nodes[prev_idx].height;
                    let x = match pos_layout {
                        NotePositionLayout::Right => prev_x + prev_w + NOTE_OFFSET,
                        NotePositionLayout::Left => prev_x - NOTE_OFFSET - nw,
                    };
                    // Java floating notes are visually attached to the previous
                    // flow tile, so keep their midpoints aligned with the tile.
                    let y = prev_y + (prev_h - nh) / 2.0;
                    (x, y)
                } else {
                    let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                    let x = match pos_layout {
                        NotePositionLayout::Right => cx + NOTE_OFFSET,
                        NotePositionLayout::Left => cx - NOTE_OFFSET - nw,
                    };
                    (x, y_cursor)
                };

                log::debug!(
                    "  node[{node_index}] FloatingNote({pos_layout:?}) @ ({nx:.1}, {ny:.1})"
                );
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::FloatingNote {
                        position: pos_layout,
                        mode: ActivityNoteModeLayout::Grouped,
                    },
                    x: nx,
                    y: ny,
                    width: nw,
                    height: nh,
                    text: wrapped,
                    skip_in_flow: false,
                });
                node_index += 1;
                // Java: floating notes do NOT advance y_cursor.
            }

            // ---- Detach (small marker) ---------------------------------------
            ActivityEvent::Detach => {
                let size = START_RADIUS;
                let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                let x = cx - size / 2.0;
                let y = y_cursor;
                log::debug!("  node[{node_index}] Detach @ ({x:.1}, {y:.1})");
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::Detach,
                    x,
                    y,
                    width: size,
                    height: size,
                    text: String::new(),
                    skip_in_flow: false,
                });
                last_flow_node_idx = Some(node_index);
                node_index += 1;
                y_cursor += size + node_gap;
            }

            // ---- Sync bar (old-style ===NAME===) ----------------------------
            ActivityEvent::SyncBar(name) => {
                let ev_idx = event_idx;
                let has_gotos = sync_bar_goto_count.get(name).copied().unwrap_or(0) > 0;
                let is_last_ref = sync_bar_last_ref.get(name).copied() == Some(ev_idx);
                if diagram.is_old_style && has_gotos && !is_last_ref {
                    // Defer placement: just record the current y_cursor as a
                    // candidate position for this bar.
                    let entry = deferred_sync_bars.entry(name.clone()).or_insert(0.0_f64);
                    *entry = entry.max(y_cursor);
                    log::debug!("  SyncBar({name}) deferred, max_y={:.1}", *entry);
                } else {
                    // Place immediately (either new-style or this is the last ref)
                    let bar_y = if diagram.is_old_style {
                        // Use the max y from all deferred references
                        let deferred_y = deferred_sync_bars.remove(name).unwrap_or(0.0);
                        deferred_y.max(y_cursor)
                    } else {
                        y_cursor
                    };
                    let w = FORK_BAR_WIDTH;
                    let h = SYNC_BAR_HEIGHT;
                    let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                    let x = cx - w / 2.0;
                    log::debug!("  node[{node_index}] SyncBar({name}) @ ({x:.1}, {bar_y:.1})");
                    nodes.push(ActivityNodeLayout {
                        index: node_index,
                        kind: ActivityNodeKindLayout::SyncBar,
                        x,
                        y: bar_y,
                        width: w,
                        height: h,
                        text: String::new(),
                        skip_in_flow: false,
                    });
                    placed_sync_bars.insert(name.clone(), bar_y + h + node_gap);
                    last_flow_node_idx = Some(node_index);
                    node_index += 1;
                    y_cursor = bar_y + h + node_gap;
                }
            }

            // ---- Goto sync bar (old-style convergence) ----------------------
            ActivityEvent::GotoSyncBar(name) => {
                let ev_idx = event_idx;
                let is_last_ref = sync_bar_last_ref.get(name).copied() == Some(ev_idx);
                // Update the deferred max-y for this bar
                let entry = deferred_sync_bars.entry(name.clone()).or_insert(0.0_f64);
                *entry = entry.max(y_cursor);
                log::debug!(
                    "  GotoSyncBar({name}), max_y={:.1}, is_last={}",
                    *entry,
                    is_last_ref
                );
                if is_last_ref {
                    // This is the last reference — place the bar NOW
                    let bar_y = *entry;
                    deferred_sync_bars.remove(name);
                    let w = FORK_BAR_WIDTH;
                    let h = SYNC_BAR_HEIGHT;
                    let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                    let x = cx - w / 2.0;
                    log::debug!(
                        "  node[{node_index}] SyncBar({name}) placed @ ({x:.1}, {bar_y:.1})"
                    );
                    nodes.push(ActivityNodeLayout {
                        index: node_index,
                        kind: ActivityNodeKindLayout::SyncBar,
                        x,
                        y: bar_y,
                        width: w,
                        height: h,
                        text: String::new(),
                        skip_in_flow: false,
                    });
                    placed_sync_bars.insert(name.clone(), bar_y + h + node_gap);
                    last_flow_node_idx = Some(node_index);
                    node_index += 1;
                    y_cursor = bar_y + h + node_gap;
                }
            }

            // ---- Resume from sync bar (old-style source) --------------------
            ActivityEvent::ResumeFromSyncBar(name) => {
                // Outgoing reference: ===Y1=== --> target.
                // In a sequential layout we cannot go backwards, so we only
                // advance forward (y_cursor keeps its current value, or moves
                // forward to below the bar if the bar is ahead).
                if let Some(bar_y_below) = placed_sync_bars.get(name) {
                    if *bar_y_below > y_cursor {
                        log::debug!("  ResumeFromSyncBar({name}) — y_cursor {y_cursor:.1} -> {bar_y_below:.1}");
                        y_cursor = *bar_y_below;
                    } else {
                        log::debug!("  ResumeFromSyncBar({name}) — bar below at {bar_y_below:.1}, keeping y_cursor at {y_cursor:.1}");
                    }
                } else {
                    log::debug!("  ResumeFromSyncBar({name}) — bar not yet placed, keeping y_cursor at {y_cursor:.1}");
                }
            }

            // ---- Label: no visual element, records y position --------
            // Java's InstructionLabel is an empty tile placed between the
            // previous action and the next one.  Its effective y sits at
            // `previous_node_bottom + 5` (half the standard arrow gap).
            ActivityEvent::Label { name } => {
                let label_y = if let Some(prev_idx) = last_flow_node_idx {
                    let prev = &nodes[prev_idx];
                    prev.y + prev.height + 5.0
                } else {
                    y_cursor
                };
                label_positions.insert(name.clone(), label_y);
                log::debug!("  label '{name}' recorded at y={label_y:.1}");
            }

            // ---- Goto: no visual element, terminates current branch ---------
            ActivityEvent::Goto { name } => {
                log::debug!("  goto '{name}' — flow terminates (loop-back edge deferred)");
                // When inside an if-block, create a GotoLines node for inline rendering.
                // Otherwise, defer the goto loop-back edge.
                let in_if = !if_stack.is_empty();
                if in_if {
                    if let Some(label_y) = label_positions.get(name).copied() {
                        // Create placeholder GotoLines node. The actual coordinates
                        // will be updated after the centering pass resolves branch
                        // positions. We store label_y and y_cursor in the text field
                        // as a hack to pass them through.
                        nodes.push(ActivityNodeLayout {
                            index: node_index,
                            kind: ActivityNodeKindLayout::GotoLines {
                                segments: Vec::new(), // filled during post-processing
                            },
                            x: 0.0,
                            y: label_y,
                            width: 0.0,
                            height: y_cursor - label_y,
                            text: format!("{label_y},{y_cursor}"),
                            skip_in_flow: true,
                        });
                        // Store the goto info for later segment resolution
                        // (not added to goto_sources since it's handled inline)
                        goto_sources.push(GotoSource {
                            label_name: name.clone(),
                            from_node_idx: last_flow_node_idx,
                            from_y: y_cursor,
                        });
                        node_index += 1;
                    }
                } else {
                    goto_sources.push(GotoSource {
                        label_name: name.clone(),
                        from_node_idx: last_flow_node_idx,
                        from_y: y_cursor,
                    });
                }
                // Mark if-branch as has_goto
                if let Some(frame) = if_stack.last_mut() {
                    if let Some(ref mut else_branch) = frame.else_branch {
                        else_branch.has_goto = true;
                    } else {
                        frame.then_branch.has_goto = true;
                    }
                }
            }

            // ---- Backward: action box on the repeat loop-back path ----------
            ActivityEvent::Backward { text } => {
                if let Some(frame) = repeat_stack.last_mut() {
                    let (bw, bh) = estimate_text_size(text);
                    frame.backward_text = Some(text.clone());
                    frame.backward_width = bw;
                    frame.backward_height = bh;
                    log::debug!(
                        "  backward '{text}' registered on repeat frame, size {bw:.1}x{bh:.1}"
                    );
                } else {
                    log::warn!("  backward outside repeat block, ignoring");
                }
            }

            // ---- Break: exits enclosing repeat loop -------------------------
            ActivityEvent::Break => {
                if let Some(frame) = repeat_stack.last_mut() {
                    frame.break_sources.push(BreakSource {
                        from_node_idx: last_flow_node_idx,
                        from_y: y_cursor,
                    });
                    log::debug!("  break registered on repeat frame");
                } else {
                    log::warn!("  break outside repeat block, ignoring");
                }
                // Mark if-branch as has_break
                if let Some(frame) = if_stack.last_mut() {
                    if let Some(ref mut else_branch) = frame.else_branch {
                        else_branch.has_break = true;
                    } else {
                        frame.then_branch.has_break = true;
                    }
                }
            }

            // ---- PartitionStart → begin partition frame ----------------------
            ActivityEvent::PartitionStart { name } => {
                // Title metrics (font-size 14, sans-serif — Java composite style).
                let title_w = font_metrics::text_width(
                    name,
                    "SansSerif",
                    PARTITION_TITLE_FONT_SIZE,
                    false,
                    false,
                );
                let title_h =
                    font_metrics::line_height("SansSerif", PARTITION_TITLE_FONT_SIZE, false, false);
                let diff_height_title = (25.0_f64).max(title_h + 20.0);

                // The incoming arrow targets the first inner node (at
                // frame_top + diffHeightTitle), NOT the frame top.  The
                // Y-compression therefore sees a wide-open gap between the
                // previous node and the frame tab, removing 25 of the 35px
                // assembly gap and leaving 10px.
                let (frame_top, _gap_before) = if let Some(prev_idx) = last_flow_node_idx {
                    let prev_bottom = nodes[prev_idx].y + nodes[prev_idx].height;
                    // When the previous flow node sits inside one or more closed
                    // partitions, the partition frame(s) extend below the inner
                    // node by the bottom margin.  The next partition's frame
                    // must sit below the *frame* bottom — using the inner node
                    // bottom would make consecutive partitions overlap.
                    let effective_bottom = partition_ranges
                        .iter()
                        .filter(|&&(first, p_idx)| prev_idx >= first && prev_idx < p_idx)
                        .map(|&(_, p_idx)| nodes[p_idx].y + nodes[p_idx].height)
                        .fold(prev_bottom, f64::max);
                    (effective_bottom + 10.0, 10.0)
                } else {
                    (y_cursor, 0.0)
                };

                partition_stack.push(PartitionFrame {
                    name: name.clone(),
                    frame_top,
                    title_w,
                    first_node_idx: node_index,
                });

                // Inner events start below the title.
                y_cursor = frame_top + diff_height_title;
                log::debug!(
                    "  PartitionStart '{name}' frame_top={frame_top:.1} diffH={diff_height_title:.1} title_w={title_w:.1}"
                );
            }

            // ---- PartitionEnd → finalize partition frame ---------------------
            ActivityEvent::PartitionEnd => {
                let pframe = partition_stack.pop().ok_or_else(|| {
                    Error::Layout("PartitionEnd without matching PartitionStart".into())
                })?;

                // Last inner flow node's bottom (or frame_top + diffH if empty).
                let last_inner_bottom = if let Some(idx) = last_flow_node_idx {
                    nodes[idx].y + nodes[idx].height
                } else {
                    y_cursor
                };

                // diffYY2 = 20.  The empty region between the last inner node
                // and the frame's bottom edge (20 - 2 = 18px gap) is
                // Y-compressed: smaller(5) removes 8, leaving 12px.
                let frame_bottom = last_inner_bottom + 12.0;
                let frame_height = frame_bottom - pframe.frame_top;

                // Frame width = max(content_width + 20, titleW + 20).
                let mut content_max_w = 0.0_f64;
                for n in &nodes[pframe.first_node_idx..] {
                    if !matches!(n.kind, ActivityNodeKindLayout::Partition) {
                        content_max_w = content_max_w.max(n.width);
                    }
                }
                let inner_w = content_max_w + 20.0; // addHorizontalMargin(content, 10)
                let frame_width = inner_w.max(pframe.title_w + 20.0);

                // Create the partition frame node (non-flow, skip_in_flow).
                let partition_node_idx = nodes.len();
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::Partition,
                    x: 0.0, // set by centering pass
                    y: pframe.frame_top,
                    width: frame_width,
                    height: frame_height,
                    text: pframe.name.clone(),
                    skip_in_flow: true,
                });
                node_index += 1;
                partition_ranges.push((pframe.first_node_idx, partition_node_idx));

                // Next node goes below the frame with a normal node_gap.
                y_cursor = frame_bottom + node_gap;
                // last_flow_node_idx stays as the last inner flow node so the
                // next node's edge connects to it (the arrow passes through
                // the frame bottom, which is ignoreForCompressionOnY).
                log::debug!(
                    "  PartitionEnd '{}' frame {}x{:.1} at y={:.1}, content_max_w={:.1}",
                    pframe.name,
                    frame_width,
                    frame_height,
                    pframe.frame_top,
                    content_max_w
                );
            }

            // ---- Partition (unreachable after flatten) -----------------------
            ActivityEvent::Partition { .. } => {
                // Events are flattened in layout_activity; this arm is unreachable.
            }
        }
    }

    // --- Pre-centering: compute fork bar widths ----------------------------
    // Each branch gets addHorizontalMargin(14, 14+supp) where supp =
    // getSuppForIncomingArrow(ftile) — the extra space needed for arrow
    // labels that extend beyond the ftile's right edge.  For diagrams without
    // arrow labels (the common case), supp = 0, so each branch's total width
    // = branch_width + 28.  Fork bar width = sum of all branch widths.
    for fg in &fork_groups {
        let bar_w: f64 = fg.branches.iter().map(|&(_, _, w)| w + 28.0).sum();
        nodes[fg.top_bar_idx].width = bar_w;
        nodes[fg.bottom_bar_idx].width = bar_w;
    }

    // --- Update partition frame widths for enclosed fork bars ---------------
    // Partition frame widths were computed at PartitionEnd time, when fork
    // bar widths were still provisional. Now that fork bar widths are final,
    // widen any partition frame that contains a fork bar to
    // fork_bar_width + 20 (10px margin each side).
    if !partition_ranges.is_empty() && !fork_groups.is_empty() {
        for &(p_start, p_end) in &partition_ranges {
            // Check if any fork bar is inside this partition range.
            let mut max_fork_w = 0.0_f64;
            for fg in &fork_groups {
                if fg.top_bar_idx >= p_start && fg.top_bar_idx < p_end {
                    max_fork_w = max_fork_w.max(nodes[fg.top_bar_idx].width);
                }
            }
            if max_fork_w > 0.0 {
                let pnode = &nodes[p_end];
                let needed_w = max_fork_w + 20.0;
                if needed_w > pnode.width {
                    nodes[p_end].width = needed_w;
                }
            }
        }
    }

    // --- Pass 2b: centering for non-swimlane diagrams ----------------------
    // Java FtileGeometry for a case tile.
    // The actual factory (FtileFactoryDelegatorSwitch.createWithLinks) wraps
    // each case body as: FtileDecorateOutLabel(FtileDecorateInLabel(ftile, dimLabelIn), dimLabelOut)
    // NOT FtileMinWidthCentered.  The decorate labels widen the tile when the
    // case label text is wider than rect_w/2.
    //
    // FtileDecorateInLabel: right = max(ftile_right, inlabel_w)
    // FtileDecorateOutLabel: right = max(prev_right, outlabel_w)
    // Final: width = left + max(ftile_right, inlabel_w, outlabel_w)
    fn case_tile_geometry(
        case: &CaseBranch,
        nodes: &[ActivityNodeLayout],
        nested: &std::collections::HashSet<usize>,
        if_by_diamond: &HashMap<usize, &DeferredIfEdges>,
        while_body_by_idx: &HashMap<usize, &Vec<usize>>,
    ) -> (f64, f64, f64) {
        // Compute the case body's FtileGeometry by assembling all nodes
        // vertically (Java's FtileGeometryMerger / appendBottom):
        //   left = max(all node lefts)
        //   width = left + max(all node rights)
        // where each node has (width, left=width/2, right=width/2) for plain
        // actions, and the if/while entry node contributes its full block width.
        //
        // For a nested if-block, the block width comes from:
        //   FtileIfWithDiamonds: w = max(diamond_w, then_w + 20 + else_w)
        // and is CENTERED (left = right = w/2).
        //
        // For a nested while-block: w = max(dw, bw) + 36, left = max(dw/2, bw/2, 24) + 24.
        //
        // The case body is a vertical stack (FtileAssemblySimple), so the merged
        // width comes from the widest element.  The merged left/right may be
        // asymmetric when the widest element is not the first one.
        let (_raw_w, raw_left, raw_right) = if case.nodes.is_empty() {
            (40.0, 20.0, 20.0)
        } else {
            // Java FtileGeometryMerger (appendBottom) over all case nodes:
            // Start with the first node, then merge each subsequent node.
            let mut merged_left = 0.0_f64;
            let mut merged_right = 0.0_f64;
            for &idx in &case.nodes {
                let (_nw, nl, nr) = if nested.contains(&idx) {
                    if let Some(die) = if_by_diamond.get(&idx) {
                        let dw = nodes[idx].width;
                        let then_w = die
                            .then_branch
                            .nodes
                            .iter()
                            .map(|&i| nodes[i].width)
                            .fold(0.0_f64, f64::max);
                        let else_w = die
                            .else_branch
                            .as_ref()
                            .map(|b| {
                                b.nodes
                                    .iter()
                                    .map(|&i| nodes[i].width)
                                    .fold(0.0_f64, f64::max)
                            })
                            .unwrap_or(0.0);
                        let nude_w = if die.has_else {
                            then_w + 20.0 + else_w
                        } else {
                            then_w
                        };
                        let w = dw.max(nude_w).max(HEXAGON_HALF_SIZE * 2.0);
                        (w, w / 2.0, w / 2.0)
                    } else if let Some(body) = while_body_by_idx.get(&idx) {
                        let dw: f64 = nodes[idx].width;
                        let bw: f64 = body.iter().map(|&i| nodes[i].width).fold(0.0_f64, f64::max);
                        let geo_w: f64 = dw.max(bw);
                        let w: f64 = geo_w + 24.0 + 12.0;
                        let left: f64 = geo_w.max(HEXAGON_HALF_SIZE * 2.0) / 2.0 + 24.0;
                        let right: f64 = w - left;
                        (w, left, right)
                    } else {
                        let w = nodes[idx].width;
                        (w, w / 2.0, w / 2.0)
                    }
                } else {
                    let w = nodes[idx].width;
                    (w, w / 2.0, w / 2.0)
                };
                // FtileGeometryMerger: left = max(merged_left, nl)
                merged_left = merged_left.max(nl);
                // right = max(merged_right, nr) — but we need to account for
                // the dx shift.  In appendBottom:
                //   dx1 = merged_left - prev_merged_left (shift of upper tile)
                //   width = max(prev_width + dx1, nw + (merged_left - nl))
                // Simplified: merged_right = max(merged_right, nr + (merged_left - nl))
                //            = max(merged_right, nr + merged_left - nl)
                // Since nl = nw/2 and nr = nw - nl for centered tiles:
                //   nr + merged_left - nl = (nw - nl) + merged_left - nl = nw + merged_left - 2*nl
                // For centered: nw + merged_left - nw = merged_left → doesn't help.
                // Actually: appendBottom merges the SECOND geometry's right.
                // right(after merge) = max(right_upper + dx_upper, right_lower + dx_lower)
                // where dx = merged_left - tile_left
                // For centered tiles: dx = merged_left - nw/2
                //   right = max(prev_right + dx_prev, nr + merged_left - nl)
                // This is complex.  For a simple vertical stack of centered tiles
                // where each tile has left = nw/2:
                //   merged_left = max(all nw/2) = widest_w/2
                //   merged_right = max(all nw - nw/2) = widest_w/2 (same for centered)
                // So width = widest_w, left = right = widest_w/2.
                //
                // BUT for non-centered tiles (like while-block with left != right),
                // the merger gives asymmetric results.
                merged_right = merged_right.max(nr + merged_left - nl);
            }
            let raw_w = merged_left + merged_right;
            (raw_w, merged_left, merged_right)
        };
        // FtileDecorateInLabel + FtileDecorateOutLabel:
        // right = max(raw_right, inlabel_w, outlabel_w)
        // width = raw_left + right
        // inlabel_w = case label text width (font 11pt)
        // outlabel_w = end label (usually 0)
        let inlabel_w = font_metrics::text_width(
            &case.label,
            "SansSerif",
            HEXAGON_LABEL_FONT_SIZE,
            false,
            false,
        );
        let outlabel_w = 0.0; // simple switch cases have no outlabel
        let tile_right = raw_right.max(inlabel_w).max(outlabel_w);
        let tile_w = raw_left + tile_right;
        let tile_left = raw_left;
        (tile_w, tile_left, tile_right)
    }

    /// Port of FtileSwitchNude + FtileSwitchWithDiamonds case placement.
    /// Returns each case's cx offset from the switch diamond cx, and the
    /// group's half-span (for diagram width).
    fn case_layout(
        tiles: &[(f64, f64, f64)], // (width, left, right) per case
        diamond_w: f64,
    ) -> (Vec<f64>, f64) {
        let n = tiles.len();
        if n == 0 {
            return (Vec::new(), 0.0);
        }
        // FtileSwitchWithDiamonds: determine BIG_DIAMOND vs SMALL_DIAMOND.
        // w13 = diamond_w - tile[0].right - tile[n-1].left
        // w9 = sum of middle tile widths (i=1..n-1)
        let w13 = diamond_w - tiles[0].2 - tiles[n - 1].1;
        let w9: f64 = (1..n.saturating_sub(1)).map(|i| tiles[i].0).sum();
        let is_big = w13 > w9;

        if is_big {
            // BIG_DIAMOND: cases spread with w13 + 30 gap.
            // suppx = (w13 - w9) / (n-1)
            // case 0 at dx=0, case i at cumulative (tile[i-1].w + suppx)
            // case n-1 at tile[0].w + w13 + 30
            // width = tile[0].w + 15 + w13 + 15 + tile[n-1].w
            // diamond at tile[0].left + 15 + diamond.left
            let suppx = if n > 1 {
                (w13 - w9) / (n as f64 - 1.0)
            } else {
                0.0
            };
            let mut rel = vec![0.0_f64; n];
            for i in 1..n {
                rel[i] = rel[i - 1] + tiles[i - 1].0 + suppx;
            }
            // Override last case position
            if n > 1 {
                rel[n - 1] = tiles[0].0 + w13 + 30.0;
            }
            let total_w = tiles[0].0 + 15.0 + w13 + 15.0 + tiles[n - 1].0;
            let diamond_cx = tiles[0].1 + 15.0 + diamond_w / 2.0;
            // case cx = rel[i] + tile[i].width/2
            let offsets: Vec<f64> = (0..n)
                .map(|i| rel[i] + tiles[i].0 / 2.0 - diamond_cx)
                .collect();
            let group_half = total_w / 2.0;
            (offsets, group_half)
        } else {
            // SMALL_DIAMOND: FtileSwitchNude — cases side by side,
            // xSeparation=20, group centered on switch diamond.
            // Java: getTranslateNude places tiles at cumulative x. The
            // FtileBox draws the rect at (0,0) within the tile, so the rect
            // LEFT EDGE = tile left = rel[i]. The rect centre = rel[i] +
            // tile[i].left (where left = rect_w/2 for centered tiles).
            let x_sep = 20.0;
            let mut rel = vec![0.0_f64; n];
            for i in 1..n {
                rel[i] = rel[i - 1] + tiles[i - 1].0 + x_sep;
            }
            let nude_w: f64 = tiles.iter().map(|t| t.0).sum::<f64>() + x_sep * (n as f64 - 1.0);
            let nude_cx = nude_w / 2.0;
            // rect cx = tile_left + tile[i].left (the FtileBox left, NOT
            // tile_w/2).  This differs from tile_w/2 when FtileDecorateInLabel
            // widens the tile (inlabel_w > rect_w/2).
            let offsets: Vec<f64> = (0..n).map(|i| rel[i] + tiles[i].1 - nude_cx).collect();
            let group_half = nude_w / 2.0;
            (offsets, group_half)
        }
    }

    // Lookups used by the nested-aware case-width helper (Pass 2b case
    // spreading and Pass 3e empty-case stub placement).
    let if_by_diamond: HashMap<usize, &DeferredIfEdges> = deferred_if_edges
        .iter()
        .map(|die| (die.diamond_idx, die))
        .collect();
    let while_body_by_idx: HashMap<usize, &Vec<usize>> = while_loopbacks
        .iter()
        .map(|(idx, body, ..)| (*idx, body))
        .collect();

    // Pass 2b: diagram centring + switch case-spreading. Gated on
    // `swimlane_layouts.is_empty()` because swimlane placement owns the x-axis
    // (each node sits at its lane centre). With swimlanes present this whole
    // pass is skipped, so switch case columns are NOT spread and stack at the
    // lane centre — the degraded swimlane+switch path (see the warn in the
    // Switch handler). Empty-case stubs fall back to the diamond centre via
    // `case_offsets` being empty.
    if swimlane_layouts.is_empty() && !nodes.is_empty() {
        // Collect the widest flow node (excluding branch-internal nodes).
        // For if-diamonds, consider the entire if-block width (diamond + branches).
        let mut max_half_w = nodes
            .iter()
            .filter(|n| {
                (is_flow_node(&n.kind) && !n.skip_in_flow)
                    || matches!(n.kind, ActivityNodeKindLayout::Partition)
            })
            .map(|n| n.width / 2.0)
            .fold(0.0_f64, f64::max);
        // Also consider while-loop body node widths — the body sits on the
        // centreline under the while diamond, so the widest body node's half
        // width must be covered by the centre margin.  The loop-back (right)
        // and exit (left) each extend a further HEXAGON_HALF_SIZE beyond the
        // while/body edge, so the centre margin must cover that too (otherwise
        // the exit clamps to the left edge).
        for &(while_idx, ref body_nodes, _, ref end_label, _) in &while_loopbacks {
            let body_max_w = body_nodes
                .iter()
                .map(|&i| nodes.get(i).map(|n| n.width).unwrap_or(0.0))
                .fold(0.0_f64, f64::max);
            let while_w = nodes.get(while_idx).map(|n| n.width).unwrap_or(0.0);
            let half = body_max_w.max(while_w) / 2.0;
            // The exit's `endwhile` label extends left of the while (by its text
            // width) and the loop-back extends right (by HEXAGON_HALF_SIZE);
            // official PlantUML reserves room for both, so the centre margin
            // covers half + max(end_label_w, HEXAGON_HALF_SIZE) + HEXAGON_HALF_SIZE.
            let end_label_w = if !end_label.is_empty() {
                font_metrics::text_width(
                    end_label,
                    "SansSerif",
                    HEXAGON_LABEL_FONT_SIZE,
                    false,
                    false,
                )
            } else {
                0.0
            };
            let ext = end_label_w.max(HEXAGON_HALF_SIZE);
            max_half_w = max_half_w.max(half + ext + HEXAGON_HALF_SIZE);
        }
        // Also consider if-block total width (diamond + left branch + right branch)
        for die in &deferred_if_edges {
            if die.has_else {
                let diamond = &nodes[die.diamond_idx];
                // Widest box in each branch (boxes are centred on the branch
                // centreline, so the widest determines the branch's extent).
                let box_then_w = die
                    .then_branch
                    .nodes
                    .iter()
                    .map(|&idx| nodes[idx].width)
                    .fold(0.0_f64, f64::max);
                let box_else_w = die
                    .else_branch
                    .as_ref()
                    .map(|b| {
                        b.nodes
                            .iter()
                            .map(|&idx| nodes[idx].width)
                            .fold(0.0_f64, f64::max)
                    })
                    .unwrap_or(0.0);
                // Java `FtileIfNude.widthInner` = tile1.w - tile1.left + tile2.left
                //   = box_then_w/2 + box_else_w/2 + 20  (FtileMarged ±10, min-width-centre)
                // `FtileIfWithDiamonds.widthInner` = max(widthInner, diamond1.w + 20).
                let width_inner = box_then_w / 2.0 + box_else_w / 2.0 + 20.0;
                let inner_margin = width_inner.max(diamond.width + 20.0);
                // The if-block is left-aligned: the centreline sits one
                // `left_extent` past the left margin, where
                //   left_extent = innerMargin/2 + box_then_w/2
                // (then-box left edge).  The right side extends freely and is
                // accounted for by `compute_bounds` (width = max right + 10).
                let left_extent = inner_margin / 2.0 + box_then_w / 2.0;
                max_half_w = max_half_w.max(left_extent);
            } else {
                // For no-else blocks, the diamond itself is the widest
                let diamond = &nodes[die.diamond_idx];
                max_half_w = max_half_w.max(diamond.width / 2.0);
            }
        }
        // Fork bars contribute their half-width to the centering calculation.
        for fg in &fork_groups {
            let bar_half = nodes[fg.top_bar_idx].width / 2.0;
            max_half_w = max_half_w.max(bar_half);
        }
        // Also consider switch-block total width (diamond + all case branches),
        // placed asymmetrically side by side (official PlantUML).
        for dse in &deferred_switch_edges {
            let diamond = &nodes[dse.diamond_idx];
            let n = dse.cases.len();
            if n == 0 {
                max_half_w = max_half_w.max(diamond.width / 2.0);
                continue;
            }
            let tiles: Vec<(f64, f64, f64)> = dse
                .cases
                .iter()
                .map(|c| {
                    case_tile_geometry(
                        c,
                        &nodes,
                        &nested_entry_diamonds,
                        &if_by_diamond,
                        &while_body_by_idx,
                    )
                })
                .collect();
            let (_, group_half) = case_layout(&tiles, diamond.width);
            max_half_w = max_half_w.max(group_half).max(diamond.width / 2.0);
        }
        // When there are break edges, the break path extends to x=10 which
        // needs more left margin (20px instead of 10px).
        let left_margin = if !deferred_break_edges.is_empty() {
            20.0 // break path extends to x=10; Java needs 20px left margin
        } else {
            TOP_MARGIN
        };
        let cx = left_margin + max_half_w;
        log::debug!(
            "  centering: left_margin={left_margin:.1}, max_half_w={max_half_w:.1}, cx={cx:.1}"
        );
        // First pass: center non-if-branch nodes
        for node in &mut nodes {
            if (is_flow_node(&node.kind) && !node.skip_in_flow)
                || matches!(node.kind, ActivityNodeKindLayout::Partition)
            {
                node.x = cx - node.width / 2.0;
            } else if !is_flow_node(&node.kind) && !node.skip_in_flow {
                node.x += cx;
            }
        }
        // Second pass: position switch diamonds and spread case branches.
        // Runs BEFORE the if pass so a nested if-diamond (which is a case's
        // entry node) receives its case centre x here; the if pass then
        // positions that diamond's branches relative to it.
        for dse in &deferred_switch_edges {
            // Read diamond info (immutable) before mutating nodes.
            let diamond_w = nodes[dse.diamond_idx].width;
            let n = dse.cases.len();
            if n == 0 {
                nodes[dse.diamond_idx].x = cx - diamond_w / 2.0;
                continue;
            }
            // Compute case tile geometries (immutable borrow of nodes).
            let tiles: Vec<(f64, f64, f64)> = dse
                .cases
                .iter()
                .map(|c| {
                    case_tile_geometry(
                        c,
                        &nodes,
                        &nested_entry_diamonds,
                        &if_by_diamond,
                        &while_body_by_idx,
                    )
                })
                .collect();
            let (offsets, _) = case_layout(&tiles, diamond_w);
            // Now mutate: position the switch diamond and cases.
            let diamond_cx = cx;
            nodes[dse.diamond_idx].x = cx - diamond_w / 2.0;
            for (i, case) in dse.cases.iter().enumerate() {
                let case_cx = diamond_cx + offsets[i];
                for &node_idx in &case.nodes {
                    nodes[node_idx].x = case_cx - nodes[node_idx].width / 2.0;
                }
            }
            // The switch merge diamond sits on the switch diamond's centreline.
            if let Some(md_idx) = dse.merge_diamond_idx {
                nodes[md_idx].x = diamond_cx - nodes[md_idx].width / 2.0;
            }
        }
        // Third pass: position if-diamonds and their branch nodes.
        for die in &deferred_if_edges {
            let is_nested = nested_entry_diamonds.contains(&die.diamond_idx);
            let diamond_width = nodes[die.diamond_idx].width;
            // A nested if-diamond was already placed at its case centre by the
            // switch pass above; keep that x. Non-nested ifs centre on `cx`.
            if !is_nested {
                nodes[die.diamond_idx].x = cx - diamond_width / 2.0;
            }
            let diamond_cx = nodes[die.diamond_idx].x + nodes[die.diamond_idx].width / 2.0;
            if die.has_else {
                // Recompute innerMargin (matches the max_half_w pass above).
                let box_then_w = die
                    .then_branch
                    .nodes
                    .iter()
                    .map(|&idx| nodes[idx].width)
                    .fold(0.0_f64, f64::max);
                let box_else_w = die
                    .else_branch
                    .as_ref()
                    .map(|b| {
                        b.nodes
                            .iter()
                            .map(|&idx| nodes[idx].width)
                            .fold(0.0_f64, f64::max)
                    })
                    .unwrap_or(0.0);
                let width_inner = box_then_w / 2.0 + box_else_w / 2.0 + 20.0;
                let inner_margin = width_inner.max(diamond_width + 20.0);
                // Java `FtileIfWithDiamonds`: branch1 centred at
                //   getTranslateBranch1.x (=0) → if-centre - innerMargin/2
                // branch2 at getTranslateBranch2.x (=dimTotal.w - tile2.w) →
                //   if-centre + innerMargin/2  (the if-block centreline is dimTotal.left).
                //
                // For an if nested in a switch case, the switch pass has already
                // placed the if diamond at the case centre; use that centreline,
                // not the global diagram centre.
                let then_cx = diamond_cx - inner_margin / 2.0;
                for &node_idx in &die.then_branch.nodes {
                    nodes[node_idx].x = then_cx - nodes[node_idx].width / 2.0;
                }
                let else_cx = diamond_cx + inner_margin / 2.0;
                if let Some(ref else_branch) = die.else_branch {
                    for &node_idx in &else_branch.nodes {
                        nodes[node_idx].x = else_cx - nodes[node_idx].width / 2.0;
                    }
                }
                // Merge diamond (`FtileDiamond`) centred on the centreline:
                // getTranslateDiamond2.x = dimTotal.left - diamond2.width/2.
                if let Some(md_idx) = die.merge_diamond_idx {
                    nodes[md_idx].x = diamond_cx - nodes[md_idx].width / 2.0;
                }
            } else {
                // No else: then branch at diamond centre (case centre for nested)
                for &node_idx in &die.then_branch.nodes {
                    nodes[node_idx].x = diamond_cx - nodes[node_idx].width / 2.0;
                }
            }
            // The if-merge diamond sits on the if-diamond's centreline (the
            // case centre for a nested if).
            if let Some(md_idx) = die.merge_diamond_idx {
                nodes[md_idx].x = diamond_cx - nodes[md_idx].width / 2.0;
            }
        }
        // Position while-loop body nodes at the while diamond's centre x.
        // Body nodes are skip_in_flow=true (edges built manually in Pass 3d),
        // so the first centering pass skipped them; place them now, directly
        // under the while diamond on the centreline.
        for &(while_idx, ref body_nodes, ..) in &while_loopbacks {
            let while_cx = nodes[while_idx].x + nodes[while_idx].width / 2.0;
            for &node_idx in body_nodes {
                nodes[node_idx].x = while_cx - nodes[node_idx].width / 2.0;
            }
        }
    }

    // --- Pass 2c: fork bar + branch x-positioning ---------------------------
    // After centering, set fork bars to span the parallel-branch width and
    // position branch nodes horizontally.  Each branch gets
    // addHorizontalMargin(14, 14+supp) so every branch has total width =
    // max_branch_width + 28.
    if !fork_groups.is_empty() {
        // Recompute cx from the first centered flow node (or use 0 for
        // swimlane diagrams — fork in swimlanes is not yet supported).
        let cx = if swimlane_layouts.is_empty() {
            nodes
                .iter()
                .find(|n| is_flow_node(&n.kind) && !n.skip_in_flow)
                .map(|n| n.x + n.width / 2.0)
                .unwrap_or(0.0)
        } else {
            swimlane_center_x(&swimlane_layouts, 0)
        };

        for fg in &fork_groups {
            let bar_w = nodes[fg.top_bar_idx].width;
            let bar_x = cx - bar_w / 2.0;
            nodes[fg.top_bar_idx].x = bar_x;
            nodes[fg.bottom_bar_idx].x = bar_x;

            // Each branch has width = branch_content_width + 28 (14px margin
            // on each side, with supp=0 when no arrow labels).  Branch i is
            // positioned at bar_x + offset_i, where offset_i accumulates the
            // widths of preceding branches.
            let mut offset = 0.0_f64;
            for (i, &(_first, _last, bw)) in fg.branches.iter().enumerate() {
                let branch_total = bw + 28.0;
                let branch_content_cx = bar_x + offset + 14.0 + bw / 2.0;
                // Center all flow nodes AND partition frames in this branch
                // on branch_content_cx.  The range extends to the start of
                // the next branch (or the bottom fork bar) so that partition
                // frame nodes — which sit between the last flow node and the
                // next branch — are included.
                let first = _first;
                let end_idx = if i + 1 < fg.branches.len() {
                    fg.branches[i + 1].0
                } else {
                    fg.bottom_bar_idx
                };
                let nlen = nodes.len();
                for n in &mut nodes[first..end_idx.min(nlen)] {
                    if (is_flow_node(&n.kind) && !n.skip_in_flow)
                        || matches!(n.kind, ActivityNodeKindLayout::Partition)
                    {
                        n.x = branch_content_cx - n.width / 2.0;
                    }
                }
                offset += branch_total;
            }
        }
    }

    assign_note_modes(&mut nodes);

    // --- Pass 2b2: resolve GotoLines segments --------------------------------
    // GotoLines nodes were created with empty segments during event processing.
    // Now that centering has resolved branch x-positions, fill in the actual
    // line coordinates.
    for gs in &goto_sources {
        if let Some(from_idx) = gs.from_node_idx {
            let from_cx = nodes[from_idx].x + nodes[from_idx].width / 2.0;
            // Find the GotoLines node that was created for this goto source.
            // It's the first GotoLines node after from_idx.
            if let Some(goto_node_idx) = nodes.iter().position(|n| {
                matches!(n.kind, ActivityNodeKindLayout::GotoLines { .. }) && n.index > from_idx
            }) {
                if let Some(label_y) = label_positions.get(&gs.label_name).copied() {
                    let goto_y = gs.from_y;
                    // Find the main center x (diamond center)
                    let main_cx = deferred_if_edges
                        .iter()
                        .find(|die| {
                            die.then_branch.nodes.contains(&from_idx)
                                || die
                                    .else_branch
                                    .as_ref()
                                    .map(|b| b.nodes.contains(&from_idx))
                                    .unwrap_or(false)
                        })
                        .map(|die| nodes[die.diamond_idx].x + nodes[die.diamond_idx].width / 2.0)
                        .unwrap_or(from_cx);
                    let segments = vec![
                        (from_cx, goto_y, main_cx, goto_y),
                        (main_cx, label_y, main_cx, goto_y),
                    ];
                    if let ActivityNodeKindLayout::GotoLines {
                        segments: ref mut segs,
                    } = nodes[goto_node_idx].kind
                    {
                        *segs = segments;
                    }
                }
            }
        }
    }

    // --- Pass 2c: expand swimlanes to fit content (Java LimitFinder compat) -
    // Java measures draw-time bounding boxes per-swimlane, then expands each
    // lane to max(headerWidth, contentWidth).  We replicate this by tracking
    // which lane each node belongs to and finding content bounds.
    if !swimlane_layouts.is_empty() {
        // 1) Build node→lane mapping by replaying event order
        let mut node_lane: Vec<usize> = Vec::with_capacity(nodes.len());
        let mut cur_lane: usize = 0;
        for event in &events {
            match event {
                ActivityEvent::Swimlane { name } => {
                    cur_lane = resolve_swimlane_index(&diagram.swimlanes, name);
                }
                // Every event that emits a node (same order as Pass 2)
                ActivityEvent::Start
                | ActivityEvent::Stop
                | ActivityEvent::Action { .. }
                | ActivityEvent::If { .. }
                | ActivityEvent::ElseIf { .. }
                | ActivityEvent::EndIf
                | ActivityEvent::While { .. }
                | ActivityEvent::Repeat
                | ActivityEvent::RepeatWhile { .. }
                | ActivityEvent::Fork
                | ActivityEvent::ForkAgain
                | ActivityEvent::EndFork
                | ActivityEvent::Switch { .. }
                | ActivityEvent::EndSwitch
                | ActivityEvent::Note { .. }
                | ActivityEvent::FloatingNote { .. }
                | ActivityEvent::Detach
                | ActivityEvent::SyncBar(_) => {
                    node_lane.push(cur_lane);
                }
                // Events that emit no node. Keeping node_lane 1:1 with `nodes`
                // avoids shifting later nodes' lane assignment. `Else` only
                // emits in the malformed else-without-if fallback; that
                // degraded input falls back to lane 0 at the consumer bounds
                // check.
                ActivityEvent::Else { .. }
                | ActivityEvent::EndWhile { .. }
                | ActivityEvent::Case { .. }
                | ActivityEvent::GotoSyncBar(_)
                | ActivityEvent::ResumeFromSyncBar(_)
                | ActivityEvent::Label { .. }
                | ActivityEvent::Goto { .. }
                | ActivityEvent::Backward { .. }
                | ActivityEvent::Break
                | ActivityEvent::Partition { .. }
                | ActivityEvent::PartitionStart { .. }
                | ActivityEvent::PartitionEnd => {}
            }
        }

        // 2) Compute content width per lane.
        //    Java FtileWithNotes: width = tile.w + left_notes.w + right_notes.w.
        //    We simulate this by finding each flow node's composite width
        //    (including adjacent notes) and tracking the max composite.
        let n_lanes = swimlane_layouts.len();
        let mut lane_max_composite_w = vec![0.0_f64; n_lanes];
        let mut lane_max_composite_single = vec![false; n_lanes];
        let mut lane_min_x = vec![f64::MAX; n_lanes];
        let mut lane_max_x = vec![f64::MIN; n_lanes];

        // For each flow node, find adjacent notes and compute composite width
        for (ni, node) in nodes.iter().enumerate() {
            let li = if ni < node_lane.len() {
                node_lane[ni]
            } else {
                0
            };
            let (left, right) = limitfinder_x_bounds(node);
            if left < lane_min_x[li] {
                lane_min_x[li] = left;
            }
            if right > lane_max_x[li] {
                lane_max_x[li] = right;
            }

            if is_flow_node(&node.kind) {
                // Find adjacent notes (immediately following this flow node)
                let mut left_note_w = 0.0_f64;
                let mut right_note_w = 0.0_f64;
                let mut note_count = 0usize;
                for node_j in &nodes[(ni + 1)..] {
                    match &node_j.kind {
                        ActivityNodeKindLayout::Note { position, .. }
                        | ActivityNodeKindLayout::FloatingNote { position, .. } => {
                            note_count += 1;
                            match position {
                                NotePositionLayout::Left => left_note_w += node_j.width,
                                NotePositionLayout::Right => right_note_w += node_j.width,
                            }
                        }
                        _ => break, // next flow node — stop looking
                    }
                }
                // Java FtileWithNotes: each note Opale is wrapped with
                // TextBlockUtils.withMargin(opale, 10, 10) → +20 per note side.
                let note_margin = 20.0; // Java: withMargin(opale, 10, 10)
                let left_total = if left_note_w > 0.0 {
                    left_note_w + note_margin
                } else {
                    0.0
                };
                let right_total = if right_note_w > 0.0 {
                    right_note_w + note_margin
                } else {
                    0.0
                };
                let composite_w = node.width + left_total + right_total;
                if composite_w > lane_max_composite_w[li] {
                    lane_max_composite_w[li] = composite_w;
                    lane_max_composite_single[li] = note_count == 1;
                }
            }
        }

        // 3) Expand each lane; Java LaneDivider: edge=5, between=5..N depending on title overflow
        // Left divider = halfMissing(0)(=5) + halfMissing(1)(=5 or more)
        let half_missing_edge = LANE_DIVIDER_HALF;
        let header_widths: Vec<f64> = diagram
            .swimlanes
            .iter()
            .map(|name| {
                font_metrics::text_width(name, "SansSerif", SWIMLANE_HEADER_FONT_SIZE, false, false)
            })
            .collect();

        // First pass: determine final lane widths (max of header and content)
        let mut lane_widths: Vec<f64> = Vec::with_capacity(n_lanes);
        for i in 0..n_lanes {
            // Use the max composite width (Java FtileWithNotes model).
            // Java LimitFinder tracks 1px wider than FtileWithNoteOpale.
            // calculateDimension for single-side note lanes (from Opale
            // stencil rendering offset in SheetBlock). FtileWithNotes
            // (both-side notes) doesn't have this offset.
            let stencil_correction =
                if lane_max_composite_w[i] > 0.0 && lane_max_composite_single[i] {
                    1.0
                } else {
                    0.0
                };
            let content_width = if lane_max_composite_w[i] > 0.0 {
                lane_max_composite_w[i] + stencil_correction
            } else if lane_max_x[i] > lane_min_x[i] {
                lane_max_x[i] - lane_min_x[i]
            } else {
                0.0
            };
            let hw = header_widths[i] + 2.0 * LANE_DIVIDER_HALF;
            // Java: lane visual width = actualWidth + dividerWidth.
            // When content > header, add divider to the lane width itself.
            let cw_with_div = if content_width > hw {
                content_width + 2.0 * LANE_DIVIDER_HALF
            } else {
                content_width
            };
            lane_widths.push(cw_with_div.max(hw));
        }

        // Java getHalfMissingSpace: if title > actualWidth, expand divider.
        // Since lane_widths already includes max(content, header+pad), title
        // overflow is already absorbed. half_missing returns the base 5px.
        let half_missing = |_lane_idx: usize| -> f64 { LANE_DIVIDER_HALF };

        // Java: left lane line consistently at x ≈ 20 (divider(10) + centering offset).
        // This comes from LaneDivider width + content minX compensation.
        // We approximate with edge(5) + halfMissing + content centering offset.
        let left_divider = half_missing_edge + half_missing(0);
        // Java: internal lane lines start at x≈5, then the entire diagram gets
        // a global +15 offset from the SVG rendering framework's MinMax margin.
        // We apply this combined offset directly: first lane starts at x ≈ 20.
        // Java internal lane lines start at x≈5; SVG renders them at x≈20
        // due to framework-level MinMax offset (~15px).  We apply the combined
        // left_divider + framework offset directly.
        let global_margin = 5.0;
        let mut x = left_divider + global_margin;
        for i in 0..n_lanes {
            let needed = lane_widths[i];
            let _old_x = swimlane_layouts[i].x;
            swimlane_layouts[i].x = x;
            swimlane_layouts[i].width = needed;

            // Shift nodes so content is centred within the new lane bounds.
            if lane_max_x[i] > lane_min_x[i] {
                let cw = lane_max_x[i] - lane_min_x[i];
                let target_left = x + (needed - cw) / 2.0;
                let dx = target_left - lane_min_x[i];
                if dx.abs() > 0.01 {
                    for (ni, node) in nodes.iter_mut().enumerate() {
                        if ni < node_lane.len() && node_lane[ni] == i {
                            node.x += dx;
                        }
                    }
                }
            }
            // Java: xpos += actualWidth + dividerWidth.
            // When lane width is header-driven (hw includes divider padding),
            // the divider is already absorbed. When content-driven (content > hw),
            // add the divider width explicitly.
            // Divider is already included in lane_widths for content-driven lanes.
            x += needed;
        }

        // 4) Re-normalize note groups around their flow node.
        // Java uses two distinct composite geometries:
        // - `FtileWithNotes` when notes exist on both sides: each side reserves
        //   `note_width + 20`, but the visible gap to the action box is 10.
        // - `FtileWithNoteOpale` for a single-side note: the side reserves
        //   `note_width + 19`, matching the 1px stencil correction seen in
        //   Java's LimitFinder path for one-sided note tiles.
        let mut i = 0usize;
        while i < nodes.len() {
            if !is_flow_node(&nodes[i].kind) {
                i += 1;
                continue;
            }

            let mut left_indices = Vec::new();
            let mut right_indices = Vec::new();
            let mut j = i + 1;
            while j < nodes.len() {
                match &nodes[j].kind {
                    ActivityNodeKindLayout::Note { position, .. }
                    | ActivityNodeKindLayout::FloatingNote { position, .. } => match position {
                        NotePositionLayout::Left => left_indices.push(j),
                        NotePositionLayout::Right => right_indices.push(j),
                    },
                    _ => break,
                }
                j += 1;
            }

            if left_indices.is_empty() && right_indices.is_empty() {
                i = j;
                continue;
            }

            let has_left = !left_indices.is_empty();
            let has_right = !right_indices.is_empty();
            let total_notes = left_indices.len() + right_indices.len();
            let single_group = total_notes == 1;
            let left_max_w = left_indices
                .iter()
                .map(|&idx| nodes[idx].width)
                .fold(0.0_f64, f64::max);
            let right_max_w = right_indices
                .iter()
                .map(|&idx| nodes[idx].width)
                .fold(0.0_f64, f64::max);
            let left_band = if has_left {
                left_max_w + if single_group { 19.0 } else { 20.0 }
            } else {
                0.0
            };
            let right_band = if has_right {
                right_max_w + if single_group { 19.0 } else { 20.0 }
            } else {
                0.0
            };

            let mut group_min_x = nodes[i].x;
            let mut group_max_x = nodes[i].x + nodes[i].width;
            for &idx in left_indices.iter().chain(right_indices.iter()) {
                group_min_x = group_min_x.min(nodes[idx].x);
                group_max_x = group_max_x.max(nodes[idx].x + nodes[idx].width);
            }
            let group_center = (group_min_x + group_max_x) / 2.0;
            let group_width = left_band + nodes[i].width + right_band;
            let group_left = group_center - group_width / 2.0;

            if has_left {
                let left_x = if single_group {
                    group_left
                } else {
                    group_left + 10.0
                };
                for &idx in &left_indices {
                    nodes[idx].x = left_x;
                }
            }

            nodes[i].x = group_left + left_band;

            if has_right {
                let right_gap = if single_group { 20.0 } else { 10.0 };
                let right_x = nodes[i].x + nodes[i].width + right_gap;
                for &idx in &right_indices {
                    nodes[idx].x = right_x;
                }
            }

            i = j;
        }

        // 5) Subsequent flow groups in the same swimlane should keep following
        // the previous flow column. Java activity tiles do not snap back to
        // the swimlane center after a one-sided note shifts the column.
        align_flow_groups_to_lane_columns(&mut nodes, &node_lane);
    }

    // --- Pass 2d: create deferred backward nodes (after centering) ----------
    for db in &deferred_backward {
        let diamond1 = &nodes[db.diamond1_idx];
        let hex_node = &nodes[db.hex_idx];
        // Java: backward box x = right of repeat body + hexagonHalfSize gap
        let body_right = nodes[db.diamond1_idx..=db.hex_idx]
            .iter()
            .filter(|n| is_flow_node(&n.kind))
            .map(|n| n.x + n.width)
            .fold(0.0_f64, f64::max);
        // Java gap between body right and backward box = 10 (ImageBuilder margin)
        let bx = body_right + (TOP_MARGIN - 1.0);
        // Vertical center between diamond1 mid-y and hex mid-y
        let d1_mid_y = diamond1.y + diamond1.height / 2.0;
        let hex_mid_y = hex_node.y + hex_node.height / 2.0;
        let by = (d1_mid_y + hex_mid_y) / 2.0 - db.height / 2.0;
        log::debug!(
            "  node[{node_index}] Backward action '{}' @ ({bx:.1}, {by:.1}) {:.1}x{:.1}",
            db.text,
            db.width,
            db.height
        );
        nodes.push(ActivityNodeLayout {
            index: node_index,
            kind: ActivityNodeKindLayout::BackwardAction,
            x: bx,
            y: by,
            width: db.width,
            height: db.height,
            text: db.text.clone(),
            skip_in_flow: false,
        });
        let backward_idx = node_index;
        node_index += 1;
        repeat_loopbacks_with_backward.push((db.hex_idx, db.diamond1_idx, backward_idx));
    }

    // --- Pass 3: edges connecting consecutive flow nodes --------------------
    let mut edges = build_edges(&nodes);

    // Remove edges that jump across an if-block.  build_edges connects
    // consecutive non-skip flow nodes, but nodes separated by an if-block
    // should NOT be directly connected (they're connected through branches).
    if !deferred_if_edges.is_empty() {
        let if_ranges: Vec<(usize, usize)> = deferred_if_edges
            .iter()
            .map(|die| {
                // Find the first non-skip flow node after this if-block
                let post_idx = nodes
                    .iter()
                    .find(|n| n.index > die.diamond_idx && is_flow_node(&n.kind) && !n.skip_in_flow)
                    .map(|n| n.index)
                    .unwrap_or(die.diamond_idx);
                (die.diamond_idx, post_idx)
            })
            .collect();
        edges.retain(|edge| {
            !if_ranges.iter().any(|&(diamond_idx, post_idx)| {
                edge.from_index < diamond_idx && edge.to_index >= post_idx
            })
        });
    }

    // Remove edges that jump across a switch-block (same rationale as if-block).
    if !deferred_switch_edges.is_empty() {
        let switch_ranges: Vec<(usize, usize)> = deferred_switch_edges
            .iter()
            .map(|dse| {
                let post_idx = nodes
                    .iter()
                    .find(|n| n.index > dse.diamond_idx && is_flow_node(&n.kind) && !n.skip_in_flow)
                    .map(|n| n.index)
                    .unwrap_or(dse.diamond_idx);
                (dse.diamond_idx, post_idx)
            })
            .collect();
        edges.retain(|edge| {
            !switch_ranges.iter().any(|&(diamond_idx, post_idx)| {
                edge.from_index < diamond_idx && edge.to_index >= post_idx
            })
        });
    }
    // Remove the build_edges edge that jumps from a while diamond straight
    // down to the first post-while flow node — it would cut through the loop
    // body (body nodes are skip_in_flow, so build_edges skips them but still
    // links while → next).  The exit edge is rebuilt properly in Pass 3d.
    if !while_loopbacks.is_empty() {
        let while_ranges: Vec<(usize, usize)> = while_loopbacks
            .iter()
            .map(|&(while_idx, _, _, _, _)| {
                let post_idx = nodes
                    .iter()
                    .find(|n| n.index > while_idx && is_flow_node(&n.kind) && !n.skip_in_flow)
                    .map(|n| n.index)
                    .unwrap_or(while_idx);
                (while_idx, post_idx)
            })
            .collect();
        edges.retain(|edge| {
            !while_ranges.iter().any(|&(while_idx, post_idx)| {
                edge.from_index == while_idx && edge.to_index >= post_idx
            })
        });
    }

    // --- Pass 3-break: break edges (before if-branch edges) ------------------
    // Break edges connect from a break source (inside an if-block within a
    // repeat) to the repeat's exit diamond.  In Java these are drawn as part
    // of the if-tile's connections, which are emitted before the if-diamond's
    // own branch connections.  Generating them before Pass 3a ensures the
    // correct draw order when inner edges are collected.
    for &(from_idx, _from_y, exit_idx) in &deferred_break_edges {
        let from_node = &nodes[from_idx];
        let from_cx = from_node.x + from_node.width / 2.0;
        let from_bottom = from_node.y + from_node.height;
        let exit_node = &nodes[exit_idx];
        let exit_cy = exit_node.y + exit_node.height / 2.0;
        let exit_left = exit_node.x;
        let break_y = from_bottom + 6.5157; // Java welding-point gap
        let left_x = 10.0;
        edges.push(ActivityEdgeLayout {
            from_index: from_idx,
            to_index: exit_idx,
            label: String::new(),
            points: vec![
                (from_cx, from_bottom),
                (from_cx, break_y),
                (left_x, break_y),
                (left_x, exit_cy),
                (exit_left, exit_cy),
            ],
            kind: ActivityEdgeKindLayout::BreakEdge,
            label_xy: None,
        });
    }

    // --- Pass 3a: if/else branch edges ----------------------------------------
    for die in &deferred_if_edges {
        let diamond = &nodes[die.diamond_idx];
        let diamond_cx = diamond.x + diamond.width / 2.0;
        let diamond_cy = diamond.y + diamond.height / 2.0;
        let diamond_left_x = diamond.x;
        let diamond_right_x = diamond.x + diamond.width;
        let diamond_bottom_y = diamond.y + diamond.height;
        let diamond_top_y = diamond.y;

        // Find previous and next flow nodes for this if-block
        let prev_flow_idx = if die.diamond_idx > 0 {
            nodes[..die.diamond_idx]
                .iter()
                .rev()
                .find(|n| is_flow_node(&n.kind) && !n.skip_in_flow)
                .map(|n| n.index)
        } else {
            None
        };
        let next_flow_idx = nodes
            .iter()
            .find(|n| {
                n.index > die.diamond_idx
                    && is_flow_node(&n.kind)
                    && !n.skip_in_flow
                    && n.y >= die.merge_y - 1.0
            })
            .map(|n| n.index);

        // A nested if (opened inside a switch case) is claimed by the switch:
        // the switch draws the case-entry edge (switch-diamond → if-diamond,
        // with the case label) and the case-exit (from the if's merge diamond
        // to the switch's merge diamond). So the if machinery suppresses its
        // own incoming edge and its merge-diamond → next exit edge; it only
        // routes the two branches INTO the if's merge diamond.
        let is_nested = nested_entry_diamonds.contains(&die.diamond_idx);
        // Merge diamond geometry (where the if-branches rejoin).
        let md = die.merge_diamond_idx.map(|i| &nodes[i]);
        let (md_cx, md_cy, md_top, md_bottom, _md_left, md_right) = match md {
            Some(m) => (
                m.x + m.width / 2.0,
                m.y + m.height / 2.0,
                m.y,
                m.y + m.height,
                m.x,
                m.x + m.width,
            ),
            None => (
                diamond_cx,
                die.merge_y,
                die.merge_y,
                die.merge_y,
                diamond_left_x,
                diamond_right_x,
            ),
        };

        if die.has_else {
            // Java FtileIfWithLinks edge order (normal two-branch merge):
            // 1. Then-branch connection (diamond left → first then-node, H-then-V
            //    via ConnectionHorizontalThenVertical)
            // 2. Else-branch connection (diamond right → first else-node, H-then-V)
            // 3. Then-merge (last then-node → merge diamond left, V-then-H via
            //    ConnectionVerticalThenHorizontal)
            // 4. Else-merge (last else-node → merge diamond right, V-then-H)
            // 5. Incoming (prev → diamond)
            // 6. Merge diamond → next flow node
            //
            // When a branch ends in break/goto there is no merge diamond
            // (Java `hasTwoBranches` is false, `getShape2` returns FtileEmpty);
            // the clean branch connects straight to the next flow node instead.

            // 1. Then-branch exit (goto arrow) — only when no merge diamond
            if die.merge_diamond_idx.is_none() && die.then_branch.has_goto {
                if let Some(last_idx) = die.then_branch.nodes.last() {
                    let last = &nodes[*last_idx];
                    let last_cx = last.x + last.width / 2.0;
                    let last_bottom = last.y + last.height;
                    let goto_y = die.then_branch.bottom_y;
                    edges.push(ActivityEdgeLayout {
                        from_index: *last_idx,
                        to_index: *last_idx,
                        label: String::new(),
                        points: vec![(last_cx, last_bottom), (last_cx, goto_y)],
                        kind: ActivityEdgeKindLayout::Normal,
                        label_xy: None,
                    });
                }
            }

            // 2. Then branch connection (diamond left → first node): H-then-V
            if let Some(first_idx) = die.then_branch.nodes.first() {
                let node = &nodes[*first_idx];
                let then_cx = node.x + node.width / 2.0;
                let node_top = node.y;
                edges.push(ActivityEdgeLayout {
                    from_index: die.diamond_idx,
                    to_index: *first_idx,
                    label: String::new(),
                    points: vec![
                        (diamond_left_x, diamond_cy),
                        (then_cx, diamond_cy),
                        (then_cx, node_top),
                    ],
                    kind: ActivityEdgeKindLayout::IfBranch,
                    label_xy: None,
                });
            }
            // Inter-then-branch-node edges
            for pair in die.then_branch.nodes.windows(2) {
                let from = &nodes[pair[0]];
                let to = &nodes[pair[1]];
                let from_cx = from.x + from.width / 2.0;
                let from_bottom = from.y + from.height;
                let to_cx = to.x + to.width / 2.0;
                let to_top = to.y;
                edges.push(ActivityEdgeLayout {
                    from_index: pair[0],
                    to_index: pair[1],
                    label: String::new(),
                    points: vec![(from_cx, from_bottom), (to_cx, to_top)],
                    kind: ActivityEdgeKindLayout::Normal,
                    label_xy: None,
                });
            }

            // 3. Else branch connection (diamond right → first node): H-then-V
            if let Some(ref else_branch) = die.else_branch {
                if let Some(first_idx) = else_branch.nodes.first() {
                    let node = &nodes[*first_idx];
                    let else_cx = node.x + node.width / 2.0;
                    let node_top = node.y;
                    edges.push(ActivityEdgeLayout {
                        from_index: die.diamond_idx,
                        to_index: *first_idx,
                        label: String::new(),
                        points: vec![
                            (diamond_right_x, diamond_cy),
                            (else_cx, diamond_cy),
                            (else_cx, node_top),
                        ],
                        kind: ActivityEdgeKindLayout::IfBranch,
                        label_xy: None,
                    });
                }
                // Inter-else-branch-node edges
                for pair in else_branch.nodes.windows(2) {
                    let from = &nodes[pair[0]];
                    let to = &nodes[pair[1]];
                    let from_cx = from.x + from.width / 2.0;
                    let from_bottom = from.y + from.height;
                    let to_cx = to.x + to.width / 2.0;
                    let to_top = to.y;
                    edges.push(ActivityEdgeLayout {
                        from_index: pair[0],
                        to_index: pair[1],
                        label: String::new(),
                        points: vec![(from_cx, from_bottom), (to_cx, to_top)],
                        kind: ActivityEdgeKindLayout::Normal,
                        label_xy: None,
                    });
                }
            }

            // 4-6. Merge + incoming
            if let Some(md_idx) = die.merge_diamond_idx {
                let merge_node = &nodes[md_idx];
                let merge_cy = merge_node.y + merge_node.height / 2.0;
                let merge_left = merge_node.x;
                let merge_right = merge_node.x + merge_node.width;
                let merge_bottom = merge_node.y + merge_node.height;
                let merge_cx = merge_node.x + merge_node.width / 2.0;

                // 4a. Then-merge (last then-node → merge diamond left): V-then-H
                if let Some(last_idx) = die.then_branch.nodes.last() {
                    let last = &nodes[*last_idx];
                    let last_cx = last.x + last.width / 2.0;
                    let last_bottom = last.y + last.height;
                    edges.push(ActivityEdgeLayout {
                        from_index: *last_idx,
                        to_index: md_idx,
                        label: String::new(),
                        points: vec![
                            (last_cx, last_bottom),
                            (last_cx, merge_cy),
                            (merge_left, merge_cy),
                        ],
                        kind: ActivityEdgeKindLayout::IfMerge,
                        label_xy: None,
                    });
                }
                // 4b. Else-merge (last else-node → merge diamond right): V-then-H
                if let Some(ref else_branch) = die.else_branch {
                    if let Some(last_idx) = else_branch.nodes.last() {
                        let last = &nodes[*last_idx];
                        let last_cx = last.x + last.width / 2.0;
                        let last_bottom = last.y + last.height;
                        edges.push(ActivityEdgeLayout {
                            from_index: *last_idx,
                            to_index: md_idx,
                            label: String::new(),
                            points: vec![
                                (last_cx, last_bottom),
                                (last_cx, merge_cy),
                                (merge_right, merge_cy),
                            ],
                            kind: ActivityEdgeKindLayout::IfMerge,
                            label_xy: None,
                        });
                    }
                }
                // 5. Incoming (prev → diamond). Suppressed for a nested if; the
                // switch draws the case-entry edge.
                if !is_nested {
                    if let Some(prev) = prev_flow_idx {
                        let prev_node = &nodes[prev];
                        let prev_cx = prev_node.x + prev_node.width / 2.0;
                        let prev_bottom = prev_node.y + prev_node.height;
                        edges.push(ActivityEdgeLayout {
                            from_index: prev,
                            to_index: die.diamond_idx,
                            label: String::new(),
                            points: vec![(prev_cx, prev_bottom), (diamond_cx, diamond_top_y)],
                            kind: ActivityEdgeKindLayout::Normal,
                            label_xy: None,
                        });
                    }
                }
                // 6. Merge diamond → next flow node. Suppressed for a nested if;
                // the switch draws the nested exit into the switch merge diamond.
                if !is_nested {
                    if let Some(next) = next_flow_idx {
                        let next_node = &nodes[next];
                        let next_top = next_node.y;
                        let next_cx = next_node.x + next_node.width / 2.0;
                        edges.push(ActivityEdgeLayout {
                            from_index: md_idx,
                            to_index: next,
                            label: String::new(),
                            points: vec![(merge_cx, merge_bottom), (next_cx, next_top)],
                            kind: ActivityEdgeKindLayout::Normal,
                            label_xy: None,
                        });
                    }
                }
            } else {
                // No merge diamond (a branch ends in break/goto): the clean
                // branch connects straight to the next flow node.
                if let Some(ref else_branch) = die.else_branch {
                    if !else_branch.has_goto && !else_branch.has_break {
                        if let (Some(last_idx), Some(next)) =
                            (else_branch.nodes.last(), next_flow_idx)
                        {
                            let last = &nodes[*last_idx];
                            let last_cx = last.x + last.width / 2.0;
                            let last_bottom = last.y + last.height;
                            let merge_mid_y = die.merge_y + 5.0;
                            let next_node = &nodes[next];
                            let next_top = next_node.y;
                            let next_cx = next_node.x + next_node.width / 2.0;
                            edges.push(ActivityEdgeLayout {
                                from_index: *last_idx,
                                to_index: next,
                                label: String::new(),
                                points: vec![
                                    (last_cx, last_bottom),
                                    (last_cx, merge_mid_y),
                                    (next_cx, merge_mid_y),
                                    (next_cx, next_top),
                                ],
                                kind: ActivityEdgeKindLayout::IfMerge,
                                label_xy: None,
                            });
                        }
                    }
                }
                // Incoming (prev → diamond). Suppressed for a nested if; the
                // switch draws the case-entry edge.
                if !is_nested {
                    if let Some(prev) = prev_flow_idx {
                        let prev_node = &nodes[prev];
                        let prev_cx = prev_node.x + prev_node.width / 2.0;
                        let prev_bottom = prev_node.y + prev_node.height;
                        edges.push(ActivityEdgeLayout {
                            from_index: prev,
                            to_index: die.diamond_idx,
                            label: String::new(),
                            points: vec![(prev_cx, prev_bottom), (diamond_cx, diamond_top_y)],
                            kind: ActivityEdgeKindLayout::Normal,
                            label_xy: None,
                        });
                    }
                }
            }
        } else {
            // No explicit else: then branch goes CENTER-DOWN, else goes RIGHT
            // ---- Then branch (CENTER) ----
            for node_idx in &die.then_branch.nodes {
                let node = &nodes[*node_idx];
                let node_cx = node.x + node.width / 2.0;
                let node_top = node.y;
                if die.then_branch.nodes.first() == Some(node_idx) {
                    // Diamond bottom-center → down → branch node
                    edges.push(ActivityEdgeLayout {
                        from_index: die.diamond_idx,
                        to_index: *node_idx,
                        label: String::new(),
                        points: vec![(diamond_cx, diamond_bottom_y), (node_cx, node_top)],
                        kind: ActivityEdgeKindLayout::Normal,
                        label_xy: None,
                    });
                }
            }
            for pair in die.then_branch.nodes.windows(2) {
                let from = &nodes[pair[0]];
                let to = &nodes[pair[1]];
                let from_cx = from.x + from.width / 2.0;
                let from_bottom = from.y + from.height;
                let to_cx = to.x + to.width / 2.0;
                let to_top = to.y;
                edges.push(ActivityEdgeLayout {
                    from_index: pair[0],
                    to_index: pair[1],
                    label: String::new(),
                    points: vec![(from_cx, from_bottom), (to_cx, to_top)],
                    kind: ActivityEdgeKindLayout::Normal,
                    label_xy: None,
                });
            }
            // Then branch end: if has_break, the break edge connects to repeat exit.
            // Normal end: merge straight down into the if-merge diamond's top.
            if !die.then_branch.has_goto && !die.then_branch.has_break {
                if let Some(last_idx) = die.then_branch.nodes.last() {
                    let last = &nodes[*last_idx];
                    let last_cx = last.x + last.width / 2.0;
                    let last_bottom = last.y + last.height;
                    edges.push(ActivityEdgeLayout {
                        from_index: *last_idx,
                        to_index: die.diamond_idx,
                        label: String::new(),
                        points: vec![(last_cx, last_bottom), (md_cx, md_top)],
                        kind: ActivityEdgeKindLayout::IfMerge,
                        label_xy: None,
                    });
                }
            }

            // Implicit else branch (RIGHT).
            if die.then_branch.has_break || die.then_branch.has_goto {
                // Legacy: then-branch breaks/gotos; the else side connects from
                // diamond right to the next flow node after the if-block.
                let next_idx = nodes
                    .iter()
                    .find(|n| {
                        n.index > die.diamond_idx
                            && is_flow_node(&n.kind)
                            && !n.skip_in_flow
                            && n.y >= die.merge_y - 1.0
                    })
                    .map(|n| n.index);
                if let Some(next) = next_idx {
                    let next_node = &nodes[next];
                    let next_top = next_node.y;
                    let next_cx = next_node.x + next_node.width / 2.0;
                    let else_x = diamond_right_x + 12.0;
                    let merge_mid_y = next_top - 20.0;
                    edges.push(ActivityEdgeLayout {
                        from_index: die.diamond_idx,
                        to_index: next,
                        label: String::new(),
                        points: vec![
                            (diamond_right_x, diamond_cy),
                            (else_x, diamond_cy),
                            (else_x, merge_mid_y),
                            (next_cx, merge_mid_y),
                            (next_cx, next_top),
                        ],
                        kind: ActivityEdgeKindLayout::IfMergeEmphasize { mid_arrow_y: None },
                        label_xy: None,
                    });
                }
            } else {
                // Normal: the implicit else path (diamond right) rejoins the
                // merge diamond's right point, with a mid-segment DOWN arrow
                // on the vertical (matching official PlantUML).
                let else_x = diamond_right_x + 12.0;
                edges.push(ActivityEdgeLayout {
                    from_index: die.diamond_idx,
                    to_index: die.diamond_idx,
                    label: String::new(),
                    points: vec![
                        (diamond_right_x, diamond_cy),
                        (else_x, diamond_cy),
                        (else_x, md_cy),
                        (md_right, md_cy),
                    ],
                    kind: ActivityEdgeKindLayout::IfMergeEmphasize {
                        mid_arrow_y: Some((diamond_cy + md_cy) / 2.0),
                    },
                    label_xy: None,
                });
            }

            // Merge diamond → next flow node (non-nested only). A nested if's
            // merge diamond connects to the enclosing switch's merge diamond.
            if !is_nested {
                if let (Some(md_idx), Some(next)) = (die.merge_diamond_idx, next_flow_idx) {
                    let next_node = &nodes[next];
                    let next_top = next_node.y;
                    let next_cx = next_node.x + next_node.width / 2.0;
                    edges.push(ActivityEdgeLayout {
                        from_index: md_idx,
                        to_index: next,
                        label: String::new(),
                        points: vec![(md_cx, md_bottom), (next_cx, next_top)],
                        kind: ActivityEdgeKindLayout::Normal,
                        label_xy: None,
                    });
                }
            }

            // Incoming: prev→diamond. Suppressed for a nested if (switch draws
            // the case-entry edge).
            if !is_nested {
                if let Some(prev) = prev_flow_idx {
                    let prev_node = &nodes[prev];
                    let prev_cx = prev_node.x + prev_node.width / 2.0;
                    let prev_bottom = prev_node.y + prev_node.height;
                    edges.push(ActivityEdgeLayout {
                        from_index: prev,
                        to_index: die.diamond_idx,
                        label: String::new(),
                        points: vec![(prev_cx, prev_bottom), (diamond_cx, diamond_top_y)],
                        kind: ActivityEdgeKindLayout::Normal,
                        label_xy: None,
                    });
                }
            }
        }
    }

    // --- Pass 3e: switch/case branch edges -------------------------------------
    // For each switch, build:
    //   1. prev flow node → switch diamond (incoming)
    //   2. switch diamond → each case's first node (with case label on edge)
    //   3. intra-case edges (between consecutive nodes in a case)
    //   4. each case's last node → next flow node (merge)
    for dse in &deferred_switch_edges {
        let diamond = &nodes[dse.diamond_idx];
        let diamond_cx = diamond.x + diamond.width / 2.0;
        let diamond_cy = diamond.y + diamond.height / 2.0;
        let diamond_left_x = diamond.x;
        let diamond_right_x = diamond.x + diamond.width;
        let diamond_bottom_y = diamond.y + diamond.height;

        // Find the previous and next flow nodes for this switch-block.
        let prev_flow_idx = if dse.diamond_idx > 0 {
            nodes[..dse.diamond_idx]
                .iter()
                .rev()
                .find(|n| is_flow_node(&n.kind) && !n.skip_in_flow)
                .map(|n| n.index)
        } else {
            None
        };
        let next_flow_idx = nodes
            .iter()
            .find(|n| {
                n.index > dse.diamond_idx
                    && is_flow_node(&n.kind)
                    && !n.skip_in_flow
                    && n.y >= dse.merge_y - 1.0
            })
            .map(|n| n.index);

        // Switch merge-diamond geometry (where all cases rejoin).
        let md = dse.merge_diamond_idx.map(|i| &nodes[i]);
        let (md_cx, md_cy, md_top, md_bottom) = match md {
            Some(m) => (
                m.x + m.width / 2.0,
                m.y + m.height / 2.0,
                m.y,
                m.y + m.height,
            ),
            None => (diamond_cx, dse.merge_y, dse.merge_y, dse.merge_y),
        };
        /// Pick the merge-diamond attachment point for a case whose centre is
        /// `case_cx`: left point, top point, or right point — matching official
        /// PlantUML (outer cases enter the sides, the middle case the top).
        fn md_attach(case_cx: f64, md_cx: f64, md_cy: f64, md_top: f64, md_w: f64) -> (f64, f64) {
            if case_cx < md_cx - 1.0 {
                (md_cx - md_w / 2.0, md_cy) // left point
            } else if case_cx > md_cx + 1.0 {
                (md_cx + md_w / 2.0, md_cy) // right point
            } else {
                (md_cx, md_top) // top point
            }
        }

        let n = dse.cases.len();
        // Case column offsets from Pass 2b's spread, reused for empty-case
        // stub placement so an empty case lines up with its column even when
        // its siblings are wide. Under swimlane, Pass 2b is skipped (cases
        // stack at the lane centre) so offsets stay empty and empty stubs
        // stack with the rest — the degraded swimlane+switch path.
        let case_offsets: Vec<f64> = if swimlane_layouts.is_empty() && n > 0 {
            let tiles: Vec<(f64, f64, f64)> = dse
                .cases
                .iter()
                .map(|c| {
                    case_tile_geometry(
                        c,
                        &nodes,
                        &nested_entry_diamonds,
                        &if_by_diamond,
                        &while_body_by_idx,
                    )
                })
                .collect();
            let (offsets, _) = case_layout(&tiles, diamond.width);
            offsets
        } else {
            Vec::new()
        };
        // 1. Diamond → each case's first node, with the case label as edge label.
        //    Outer cases enter from the diamond's left/right side at mid-height
        //    (a long downward arrow, matching official PlantUML); the middle
        //    case (if any) enters from the diamond bottom.
        for (i, case) in dse.cases.iter().enumerate() {
            if let Some(&first_idx) = case.nodes.first() {
                let first = &nodes[first_idx];
                let case_cx = first.x + first.width / 2.0;
                let case_top = first.y;
                // Route by case INDEX (not x position): first case from the
                // diamond left, last from the right, middle cases (if any) from
                // the diamond bottom — matching official PlantUML.  Using x
                // position fails with asymmetric spacing where the middle case
                // isn't exactly at the switch centre.
                let (start_x, start_y) = if i == 0 && n > 1 {
                    (diamond_left_x, diamond_cy) // first case → diamond left point
                } else if i == n - 1 && n > 1 {
                    (diamond_right_x, diamond_cy) // last case → diamond right point
                } else {
                    (diamond_cx, diamond_bottom_y) // middle case(s) → diamond bottom
                };
                // Case label at a fixed offset below the case top so all case
                // labels (left/middle/right) share the same height — matching
                // official PlantUML.
                let label_y = case_top - 24.0;
                // Middle case: diamond bottom → down 12px → horizontal → down
                // (official adds a stub before the horizontal). Side cases:
                // horizontal at diamond_cy → down.
                let is_middle_case = !(i == 0 && n > 1) && !(i == n - 1 && n > 1);
                let points = if is_middle_case && n > 2 {
                    let stub_y = start_y + HEXAGON_HALF_SIZE;
                    vec![
                        (start_x, start_y),
                        (start_x, stub_y),
                        (case_cx, stub_y),
                        (case_cx, case_top),
                    ]
                } else {
                    vec![(start_x, start_y), (case_cx, start_y), (case_cx, case_top)]
                };
                edges.push(ActivityEdgeLayout {
                    from_index: dse.diamond_idx,
                    to_index: first_idx,
                    label: case.label.clone(),
                    points,
                    kind: ActivityEdgeKindLayout::IfBranch,
                    label_xy: Some((case_cx, label_y)),
                });
            } else {
                // Empty case: draw a stub from the diamond down to the merge
                // diamond so the case label is still visible.
                let case_cx = if n == 1 {
                    diamond_cx
                } else if i < case_offsets.len() {
                    diamond_cx + case_offsets[i]
                } else {
                    // Swimlane (degraded): stack at the diamond centre.
                    diamond_cx
                };
                let (md_x, md_y) = md_attach(case_cx, md_cx, md_cy, md_top, 24.0);
                edges.push(ActivityEdgeLayout {
                    from_index: dse.diamond_idx,
                    to_index: dse.merge_diamond_idx.unwrap_or(dse.diamond_idx),
                    label: case.label.clone(),
                    points: vec![
                        (diamond_cx, diamond_bottom_y),
                        (case_cx, diamond_bottom_y),
                        (case_cx, md_y),
                        (md_x, md_y),
                    ],
                    kind: ActivityEdgeKindLayout::IfBranch,
                    label_xy: Some((case_cx, (diamond_bottom_y + md_y) / 2.0)),
                });
            }
        }

        // 2. Intra-case edges (between consecutive nodes in each case).
        for case in &dse.cases {
            for pair in case.nodes.windows(2) {
                let from = &nodes[pair[0]];
                let to = &nodes[pair[1]];
                let from_cx = from.x + from.width / 2.0;
                let from_bottom = from.y + from.height;
                let to_cx = to.x + to.width / 2.0;
                let to_top = to.y;
                edges.push(ActivityEdgeLayout {
                    from_index: pair[0],
                    to_index: pair[1],
                    label: String::new(),
                    points: vec![(from_cx, from_bottom), (to_cx, to_top)],
                    kind: ActivityEdgeKindLayout::Normal,
                    label_xy: None,
                });
            }
        }

        // 3. Each case's last node → switch merge diamond (no arrowhead — these
        //    are merges into the diamond).  A case whose tail is a nested `if`
        //    starts from the if's merge diamond; a case whose tail is a nested
        //    `while` is skipped (the while machinery wires its own exit).  The
        //    merge diamond → next edge is drawn once after the loop.
        for (i, case) in dse.cases.iter().enumerate() {
            if case.ends_with_nested && case.nested_exit_node_idx.is_none() {
                // Nested while tail: while machinery owns the exit.
                continue;
            }
            if case.nodes.is_empty() {
                // Empty case: the case-entry stub in step 1 already drew its
                // labelled stub into the merge diamond.
                continue;
            }
            let (origin_x, origin_y) = if let Some(nested_idx) = case.nested_exit_node_idx {
                // Nested if tail: start from the if's merge diamond bottom.
                let n = &nodes[nested_idx];
                (n.x + n.width / 2.0, n.y + n.height)
            } else {
                let last = &nodes[*case.nodes.last().expect("case has a last node")];
                (last.x + last.width / 2.0, last.y + last.height)
            };
            // Use case INDEX (not x position) to pick the merge-diamond attach
            // point: first case → left, last → right, middle → top.
            let (md_x, md_y) = if i == 0 && n > 1 {
                (md_cx - 12.0, md_cy) // left point
            } else if i == n - 1 && n > 1 {
                (md_cx + 12.0, md_cy) // right point
            } else {
                (md_cx, md_top) // top point (middle case)
            };
            // Middle case: down 5px stub → horizontal to md_cx → down to
            // merge top (official adds a stub before the horizontal). Side
            // cases: drop to md_cy → horizontal into left/right point.
            let is_middle = !(i == 0 && n > 1) && !(i == n - 1 && n > 1);
            let points = if is_middle {
                let stub_y = origin_y + 5.0;
                vec![
                    (origin_x, origin_y),
                    (origin_x, stub_y),
                    (md_x, stub_y),
                    (md_x, md_y),
                ]
            } else {
                vec![(origin_x, origin_y), (origin_x, md_y), (md_x, md_y)]
            };
            edges.push(ActivityEdgeLayout {
                from_index: dse.diamond_idx,
                to_index: dse.merge_diamond_idx.unwrap_or(dse.diamond_idx),
                label: String::new(),
                points,
                kind: ActivityEdgeKindLayout::IfMerge,
                label_xy: None,
            });
        }

        // 3b. Switch merge diamond → next flow node.
        if let (Some(md_idx), Some(next)) = (dse.merge_diamond_idx, next_flow_idx) {
            let next_node = &nodes[next];
            let next_top = next_node.y;
            let next_cx = next_node.x + next_node.width / 2.0;
            edges.push(ActivityEdgeLayout {
                from_index: md_idx,
                to_index: next,
                label: String::new(),
                points: vec![(md_cx, md_bottom), (next_cx, next_top)],
                kind: ActivityEdgeKindLayout::Normal,
                label_xy: None,
            });
        }

        // 4. Incoming: prev flow node → diamond (last, to match Java draw order).
        if let Some(prev) = prev_flow_idx {
            let prev_node = &nodes[prev];
            let prev_cx = prev_node.x + prev_node.width / 2.0;
            let prev_bottom = prev_node.y + prev_node.height;
            let diamond_top_y = diamond.y;
            edges.push(ActivityEdgeLayout {
                from_index: prev,
                to_index: dse.diamond_idx,
                label: String::new(),
                points: vec![(prev_cx, prev_bottom), (diamond_cx, diamond_top_y)],
                kind: ActivityEdgeKindLayout::Normal,
                label_xy: None,
            });
        }
    }

    // --- Pass 3a2: goto loop-back edges ----------------------------------------
    // Only generate GotoNoArrow edges for gotos NOT handled by inline GotoLines nodes.
    let has_goto_lines_node = |from_idx: usize| -> bool {
        nodes.iter().any(|n| {
            matches!(n.kind, ActivityNodeKindLayout::GotoLines { .. })
                && n.index > from_idx
                && n.index < from_idx + 5 // GotoLines is created shortly after the source
        })
    };
    for gs in &goto_sources {
        if let Some(from_idx) = gs.from_node_idx {
            if has_goto_lines_node(from_idx) {
                continue; // Already handled by inline GotoLines node
            }
        }
        if let Some(label_y) = label_positions.get(&gs.label_name) {
            if let Some(from_idx) = gs.from_node_idx {
                let from = &nodes[from_idx];
                let from_cx = from.x + from.width / 2.0;
                let main_cx = if !swimlane_layouts.is_empty() {
                    swimlane_center_x(&swimlane_layouts, 0)
                } else {
                    nodes
                        .iter()
                        .find(|n| is_flow_node(&n.kind) && !n.skip_in_flow)
                        .map(|n| n.x + n.width / 2.0)
                        .unwrap_or(from_cx)
                };
                edges.push(ActivityEdgeLayout {
                    from_index: from_idx,
                    to_index: from_idx,
                    label: String::new(),
                    points: vec![
                        (from_cx, gs.from_y),
                        (main_cx, gs.from_y),
                        (main_cx, *label_y),
                    ],
                    kind: ActivityEdgeKindLayout::GotoNoArrow,
                    label_xy: None,
                });
            }
        }
    }

    // --- Pass 3b: `repeat`/`repeat while` loop-back edges -------------------
    // For each closed repeat frame, append a `LoopBackSimple2` edge that
    // snakes from the hex East side back to the diamond1 right edge.  The
    // Y for the mid-segment UP arrow is pinned to the free slot between the
    // first and second interior actions (matches Java's SlotFinder output).
    let mut loopback_extra_right: f64 = 0.0;
    let mut loopback_info: Vec<(usize, usize, Vec<ActivityEdgeLayout>)> = Vec::new();
    for &(hex_idx, diamond1_idx) in &repeat_loopbacks {
        if let Some((loopback_edge, extra_right)) =
            build_repeat_loopback_edge(&nodes, hex_idx, diamond1_idx)
        {
            log::debug!(
                "  loop-back edge hex={hex_idx} → diamond1={diamond1_idx} extra_right={extra_right}"
            );
            if extra_right > loopback_extra_right {
                loopback_extra_right = extra_right;
            }
            loopback_info.push((diamond1_idx, hex_idx, vec![loopback_edge]));
        }
    }

    // --- Pass 3c: backward loop-back edges ------------------------------------
    for &(hex_idx, diamond1_idx, backward_idx) in &repeat_loopbacks_with_backward {
        if let Some((edges_pair, extra)) =
            build_backward_loopback_edges(&nodes, hex_idx, diamond1_idx, backward_idx)
        {
            log::debug!(
                "  backward loop-back hex={hex_idx} → backward={backward_idx} → diamond1={diamond1_idx} extra_right={extra}"
            );
            if extra > loopback_extra_right {
                loopback_extra_right = extra;
            }
            // Add both edges as a single frame entry
            loopback_info.push((diamond1_idx, hex_idx, edges_pair));
        }
    }

    // --- Pass 3d: `while`/`endwhile` branch + loop-back + exit edges --------
    // Official PlantUML treats the while diamond as a two-way branch:
    //   • down  into the loop body   — `is` label
    //   • left  out to the next node — `endwhile` label
    // The loop-back snakes from the body's bottom up the right side back to
    // the while diamond's right point.  There is NO separate endwhile node.
    for &(while_idx, ref body_nodes, ref is_label, ref end_label, merge_y) in &while_loopbacks {
        let while_node = &nodes[while_idx];
        let while_cx = while_node.x + while_node.width / 2.0;
        let while_cy = while_node.y + while_node.height / 2.0;
        let while_left_x = while_node.x;
        let while_bottom_y = while_node.y + while_node.height;

        // Next flow node after the while block (exit target).
        let next_idx = nodes
            .iter()
            .find(|n| {
                n.index > while_idx
                    && is_flow_node(&n.kind)
                    && !n.skip_in_flow
                    && n.y >= merge_y - 1.0
            })
            .map(|n| n.index);

        // 1. while → first body node (down), stamped with the `is` label.
        if let Some(&first_body) = body_nodes.first() {
            let first_node = &nodes[first_body];
            let first_top = first_node.y;
            let first_cx = first_node.x + first_node.width / 2.0;
            // `is` label sits just right of the entry line (which drops from
            // the while bottom at while_cx), 11pt left-anchored.  Official
            // places its baseline ~while_bottom + 10.6; the label renderer
            // shifts down by (ascent-descent)/2, so pass while_bottom + 6.8.
            let label_xy = Some((while_cx + 4.0, while_bottom_y + 6.8));
            edges.push(ActivityEdgeLayout {
                from_index: while_idx,
                to_index: first_body,
                label: is_label.clone(),
                label_xy,
                points: vec![(while_cx, while_bottom_y), (first_cx, first_top)],
                kind: ActivityEdgeKindLayout::IfBranch,
            });
        }

        // 2. Body-internal edges between consecutive body nodes.
        for pair in body_nodes.windows(2) {
            let from = &nodes[pair[0]];
            let to = &nodes[pair[1]];
            let from_cx = from.x + from.width / 2.0;
            let from_bottom = from.y + from.height;
            let to_cx = to.x + to.width / 2.0;
            let to_top = to.y;
            edges.push(ActivityEdgeLayout {
                from_index: pair[0],
                to_index: pair[1],
                label: String::new(),
                points: vec![(from_cx, from_bottom), (to_cx, to_top)],
                kind: ActivityEdgeKindLayout::Normal,
                label_xy: None,
            });
        }

        // 3. Loop-back edge: body bottom → right → up → while right.
        if !body_nodes.is_empty() {
            if let Some((loopback_edge, extra)) =
                build_while_loopback_edge(&nodes, while_idx, body_nodes)
            {
                log::debug!(
                    "  while loop-back while={while_idx} ← body={:?} extra_right={extra}",
                    body_nodes.last()
                );
                if extra > loopback_extra_right {
                    loopback_extra_right = extra;
                }
                edges.push(loopback_edge);
            }
        }

        // 4. Exit edge: while left point → left → down → right → next, stamped
        //    with the `endwhile` label.  When nested in a switch case the exit
        //    joins the enclosing switch's merge diamond (alongside the other
        //    cases); otherwise it drops into the next flow node.
        if nested_entry_diamonds.contains(&while_idx) {
            // Find the enclosing (innermost) switch's merge diamond.
            let switch_md = deferred_switch_edges
                .iter()
                .filter(|dse| dse.diamond_idx < while_idx && dse.merge_diamond_idx.is_some())
                .max_by(|a, b| a.diamond_idx.cmp(&b.diamond_idx))
                .and_then(|dse| dse.merge_diamond_idx);
            if let Some(md_idx) = switch_md {
                let md_node = &nodes[md_idx];
                let md_cy = md_node.y + md_node.height / 2.0;
                let md_left = md_node.x;
                let exit_left_x = (while_left_x - HEXAGON_HALF_SIZE).max(10.0);
                // `endwhile` (no) label is right-aligned at the while diamond's
                // left point (text ends at while_left), baseline just above the
                // exit horizontal — matching official PlantUML.
                let no_w = font_metrics::text_width(
                    end_label,
                    "SansSerif",
                    HEXAGON_LABEL_FONT_SIZE,
                    false,
                    false,
                );
                let label_xy = Some((while_left_x - no_w, while_cy - 6.1));
                edges.push(ActivityEdgeLayout {
                    from_index: while_idx,
                    to_index: md_idx,
                    label: end_label.clone(),
                    label_xy,
                    points: vec![
                        (while_left_x, while_cy),
                        (exit_left_x, while_cy),
                        (exit_left_x, md_cy),
                        (md_left, md_cy),
                    ],
                    kind: ActivityEdgeKindLayout::IfMergeEmphasize { mid_arrow_y: None },
                });
            }
        } else if let Some(next) = next_idx {
            let next_node = &nodes[next];
            let next_top = next_node.y;
            let next_cx = next_node.x + next_node.width / 2.0;
            // Exit routes one hex-half to the left of the while diamond's left
            // point (matching official PlantUML's left-side exit column).
            let exit_left_x = (while_left_x - HEXAGON_HALF_SIZE).max(10.0);
            // `endwhile` (no) label right-aligned at while_left, baseline above
            // the exit horizontal.
            let no_w = font_metrics::text_width(
                end_label,
                "SansSerif",
                HEXAGON_LABEL_FONT_SIZE,
                false,
                false,
            );
            let label_xy = Some((while_left_x - no_w, while_cy - 6.1));
            edges.push(ActivityEdgeLayout {
                from_index: while_idx,
                to_index: next,
                label: end_label.clone(),
                label_xy,
                points: vec![
                    (while_left_x, while_cy),
                    (exit_left_x, while_cy),
                    (exit_left_x, merge_y),
                    (next_cx, merge_y),
                    (next_cx, next_top),
                ],
                kind: ActivityEdgeKindLayout::IfMergeEmphasize { mid_arrow_y: None },
            });
        }
    }

    // --- Fork edges: replace build_edges connections that cross fork -------
    // build_edges connects consecutive non-skip flow nodes, producing wrong
    // edges across fork boundaries (e.g. start→A instead of start→top_bar→A).
    // Remove those and add proper fork edges.
    if !fork_groups.is_empty() {
        // Build a set of (from, to) pairs that cross fork boundaries.
        let mut remove_pairs: Vec<(usize, usize)> = Vec::new();
        // Fork-specific edges to add (appended after removal).
        let mut fork_edges: Vec<ActivityEdgeLayout> = Vec::new();

        for fg in &fork_groups {
            let top = fg.top_bar_idx;
            let bot = fg.bottom_bar_idx;

            // Identify all nodes inside this fork (across all branches).
            let mut inner_nodes: std::collections::HashSet<usize> =
                std::collections::HashSet::new();
            for &(first, last, _) in &fg.branches {
                let end = last.unwrap_or(first);
                for i in first..=end {
                    inner_nodes.insert(i);
                }
            }

            // Remove edges where from or to is inside the fork but the
            // other endpoint is the fork bar (or outside).
            // Also remove edges between different branches.
            edges.retain(|e| {
                let from_in = inner_nodes.contains(&e.from_index);
                let to_in = inner_nodes.contains(&e.to_index);
                let crosses = (from_in && !to_in)
                    || (!from_in && to_in)
                    || (from_in && to_in && e.from_index != e.to_index);
                if crosses {
                    remove_pairs.push((e.from_index, e.to_index));
                    false
                } else {
                    true
                }
            });

            // Find the node before the fork (connects to top bar).
            // It's the last non-skip flow node before top_bar_idx.
            let pre_fork = (0..top)
                .rev()
                .find(|&i| is_flow_node(&nodes[i].kind) && !nodes[i].skip_in_flow);

            // Find the node after the fork (bottom bar connects to it).
            let post_fork = (bot + 1..nodes.len())
                .find(|&i| is_flow_node(&nodes[i].kind) && !nodes[i].skip_in_flow);

            // Entry edge: pre_fork → top_bar
            if let Some(pre) = pre_fork {
                let from = &nodes[pre];
                let to = &nodes[top];
                fork_edges.push(simple_edge(
                    pre,
                    top,
                    from.x + from.width / 2.0,
                    from.y + from.height,
                    to.x + to.width / 2.0,
                    to.y,
                ));
            }

            // Branch down edges: top_bar → each branch first flow node
            for &(first, last, _) in &fg.branches {
                let end = last.unwrap_or(first);
                // Find first actual flow node in this branch
                if let Some(first_flow) =
                    (first..=end).find(|&i| is_flow_node(&nodes[i].kind) && !nodes[i].skip_in_flow)
                {
                    let bar = &nodes[top];
                    let to = &nodes[first_flow];
                    fork_edges.push(simple_edge(
                        top,
                        first_flow,
                        to.x + to.width / 2.0, // line at branch center
                        bar.y + bar.height,
                        to.x + to.width / 2.0,
                        to.y,
                    ));
                }
            }

            // Branch up edges: each branch last flow node → bottom_bar
            for &(first, last, _) in &fg.branches {
                let end = last.unwrap_or(first);
                // Find last actual flow node in this branch
                if let Some(last_flow) = (first..=end)
                    .rev()
                    .find(|&i| is_flow_node(&nodes[i].kind) && !nodes[i].skip_in_flow)
                {
                    let from = &nodes[last_flow];
                    let bar = &nodes[bot];
                    fork_edges.push(simple_edge(
                        last_flow,
                        bot,
                        from.x + from.width / 2.0,
                        from.y + from.height,
                        from.x + from.width / 2.0,
                        bar.y,
                    ));
                }
            }

            // Exit edge: bottom_bar → post_fork
            if let Some(post) = post_fork {
                let from = &nodes[bot];
                let to = &nodes[post];
                fork_edges.push(simple_edge(
                    bot,
                    post,
                    from.x + from.width / 2.0,
                    from.y + from.height,
                    to.x + to.width / 2.0,
                    to.y,
                ));
            }
        }

        // Reorder to match Java's draw order: branch edges first
        // (down then up), then entry, then exit.
        if fork_edges.len() >= 2 {
            let entry = fork_edges.remove(0);
            let exit = fork_edges.pop().unwrap();
            fork_edges.push(entry);
            fork_edges.push(exit);
        }
        edges.extend(fork_edges);
    }

    // Reorder edges to match Java's `FtileRepeat` + assembly draw order.
    //
    // Java builds the repeat body bottom-up: inner `ConnectionVerticalDown`s
    // are appended first (inner action → next inner action), then
    // `FtileRepeat.create` appends `ConnectionIn` (diamond1 → repeat top),
    // `ConnectionBackSimple2` (loop-back), `ConnectionOut` (repeat bottom
    // → hex).  The SVG connections render in that order, so for each open
    // repeat frame we emit:
    //   1. inner → inner edges (between interior action nodes)
    //   2. diamond1 → first interior action
    //   3. loop-back
    //   4. last interior action → hex
    let all_repeat_loopbacks: Vec<(usize, usize)> = repeat_loopbacks
        .iter()
        .copied()
        .chain(
            repeat_loopbacks_with_backward
                .iter()
                .map(|&(hex, d1, _)| (hex, d1)),
        )
        .collect();
    let render_order = if !loopback_info.is_empty() {
        edges = reorder_edges_for_repeat(edges, &loopback_info, &nodes);
        Some(compute_render_order_for_repeat(
            &nodes,
            &all_repeat_loopbacks,
            &repeat_loopbacks_with_backward,
        ))
    } else {
        None
    };

    // --- Merge partition render-order --------------------------------------
    // Partition frames must be drawn BEFORE their inner content, but in the
    // flat `nodes` array the Partition node is pushed after its children.
    // Build a render_order that moves each partition node to just before its
    // first inner child.  If a repeat render_order already exists, compose
    // the partition reordering on top of it.
    let render_order = if partition_ranges.is_empty() {
        render_order
    } else {
        let base: Vec<usize> = render_order.unwrap_or_else(|| (0..nodes.len()).collect());
        Some(apply_partition_render_order(base, &partition_ranges))
    };

    // --- X-compression (Java CompressionXorYBuilder ON_X) ------------------
    // Java scans all drawn shapes via SlotFinder to find occupied X-intervals,
    // reverses them to get empty gaps, filters with smaller(5.0) (removes gaps
    // ≤ 10px, shrinks survivors by 5px each side), then applies a
    // CompressionTransform that subtracts the gap sizes from X coordinates.
    // Shapes with `ignoreForCompressionOnX` (fork bars, partition frames,
    // sync bars) contribute only 2px at each edge instead of their full width.
    //
    // We only compute X-compression when fork groups are present, because
    // fork bars (ignoreForCompressionOnX) create real horizontal gaps between
    // branches. For other diagrams (if-else, notes, etc.), Java's SlotFinder
    // records many shapes we don't track (arrowhead polygons, note rects at
    // various x positions), and those fill the gaps. Applying our partial
    // interval set to those diagrams would over-compress.
    let x_compress_slots = if !fork_groups.is_empty() {
        compute_x_compression_slots(&nodes)
    } else {
        Vec::new()
    };
    if !x_compress_slots.is_empty() {
        log::debug!("  X-compression: {} slots", x_compress_slots.len());
        // Capture original right edges for `ignoreForCompressionOnX` shapes
        // (fork bars, partition frames) BEFORE transforming x.  These shapes
        // span one or more compression slots, so the right edge must be
        // transformed from the *original* right edge — not from the already-
        // transformed left + original width, which would double-apply the slot
        // sitting between the left and right edges and shrink the shape.
        let bar_orig_right: Vec<f64> = fork_groups
            .iter()
            .map(|fg| nodes[fg.top_bar_idx].x + nodes[fg.top_bar_idx].width)
            .collect();
        let part_orig_right: Vec<f64> = partition_ranges
            .iter()
            .map(|&(_, p_idx)| nodes[p_idx].x + nodes[p_idx].width)
            .collect();
        for node in &mut nodes {
            node.x = apply_x_compress(node.x, &x_compress_slots);
            // Width is NOT transformed — it's the difference between
            // transformed right and left edges.
        }
        // Recompute fork/partition bar widths from transformed edges.
        for (fg, orig_right) in fork_groups.iter().zip(bar_orig_right.iter()) {
            let left = nodes[fg.top_bar_idx].x;
            let right = apply_x_compress(*orig_right, &x_compress_slots);
            nodes[fg.top_bar_idx].width = right - left;
            nodes[fg.bottom_bar_idx].width = right - left;
            nodes[fg.bottom_bar_idx].x = left;
        }
        // Transform partition frame widths too.
        // partition_ranges stores (first_inner_idx, partition_node_idx).
        // The partition frame node is at the second index.
        for ((_, p_idx), orig_right) in partition_ranges.iter().zip(part_orig_right.iter()) {
            let left = nodes[*p_idx].x;
            let right = apply_x_compress(*orig_right, &x_compress_slots);
            nodes[*p_idx].width = right - left;
        }
        // Transform edge x-coordinates.
        for edge in &mut edges {
            for (x, _) in &mut edge.points {
                *x = apply_x_compress(*x, &x_compress_slots);
            }
            if let Some((lx, _)) = &mut edge.label_xy {
                *lx = apply_x_compress(*lx, &x_compress_slots);
            }
        }
    }

    // --- Compute total bounding box -----------------------------------------
    let (mut total_width, total_height) = compute_bounds(&nodes, &swimlane_layouts, y_cursor);
    if loopback_extra_right > 0.0 {
        total_width += loopback_extra_right;
    }

    // A `repeat while` hex can have an `is (...)` East label that is much
    // wider than any flow-chart element.  Java sizes the SVG viewport via
    // `LimitFinder.maxX + 1 + margin_right (= 10)`, so grow `total_width`
    // to match when the East label protrudes past the current bounds.
    for node in &nodes {
        if let ActivityNodeKindLayout::Hexagon { east_lines, .. } = &node.kind {
            if east_lines.is_empty() {
                continue;
            }
            let east_font_size = HEXAGON_LABEL_FONT_SIZE;
            let max_line_w = east_lines
                .iter()
                .map(|line| {
                    font_metrics::text_width(line, "SansSerif", east_font_size, false, false)
                })
                .fold(0.0_f64, f64::max);
            let east_right = node.x + node.width + max_line_w + 11.0;
            if east_right > total_width {
                total_width = east_right;
            }
        }
    }

    // Reorder edges to match Java's `FtileWithConnection.drawU` emission
    // order.  Java draws the delegated subtree first, then its own
    // connection — a post-order traversal where an inter-tile connection is
    // emitted AFTER the right tile's internal edges.  For the flat node
    // array this means: edge sort key = (top_level_tile_rep(to_index),
    // subgroup) where subgroup=0 for edges internal to a tile (from & to in
    // the same outermost tile) and subgroup=1 for inter-tile connections.
    // `top_level_tile_rep` is the start of the outermost partition/fork
    // containing the node (or the node itself for singletons).
    if !partition_ranges.is_empty() {
        let mut tiles: Vec<(usize, usize)> =
            partition_ranges.iter().map(|&(f, p)| (f, p)).collect();
        for fg in &fork_groups {
            tiles.push((fg.top_bar_idx, fg.bottom_bar_idx));
        }
        edges = reorder_edges_for_partitions(edges, &tiles);
    }

    log::debug!(
        "layout_activity: placed {} nodes, {} edges, total {}x{}",
        nodes.len(),
        edges.len(),
        total_width,
        total_height
    );

    let mut layout = ActivityLayout {
        width: total_width,
        height: total_height,
        nodes,
        edges,
        swimlane_layouts,
        old_style_graphviz: false,
        old_node_meta: Vec::new(),
        old_edge_meta: Vec::new(),
        render_order,
    };
    apply_direction_transform(&mut layout, &diagram.direction);

    Ok(layout)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// X-compression (Java CompressionXorYBuilder ON_X)
// ---------------------------------------------------------------------------

/// A compression slot: an empty X-interval [start, end] that will be
/// removed from the coordinate space.
struct CompressSlot {
    start: f64,
    end: f64,
}

/// Determine which node kinds have `ignoreForCompressionOnX` in Java.
/// Fork bars (FtileBlackBlock), sync bars (FtileBlackBlock), and partition
/// frames (BigFrame) all set this flag.
fn has_ignore_for_compression_on_x(kind: &ActivityNodeKindLayout) -> bool {
    matches!(
        kind,
        ActivityNodeKindLayout::ForkBar
            | ActivityNodeKindLayout::SyncBar
            | ActivityNodeKindLayout::Partition
    )
}

/// Compute the X-compression slots from the layout nodes.
///
/// Mirrors Java's `CompressionXorYBuilder`:
/// 1. Collect occupied X-intervals from all shapes.
///    - For `ignoreForCompressionOnX` shapes (fork bars, partition frames,
///      sync bars): only 2px at each edge (via `drawWhenCompressed`).
///    - For all other shapes: full [x, x+width].
/// 2. Merge overlapping intervals (SlotSet.addSlot).
/// 3. Reverse to get empty gaps (SlotSet.reverse).
/// 4. Filter with smaller(5.0): drop gaps ≤ 10px, shrink survivors by 5px
///    each side.
fn compute_x_compression_slots(nodes: &[ActivityNodeLayout]) -> Vec<CompressSlot> {
    // Step 1: collect occupied intervals.
    let mut occupied: Vec<(f64, f64)> = Vec::new();
    for node in nodes {
        if has_ignore_for_compression_on_x(&node.kind) {
            // Only 2px at each edge (drawWhenCompressed → UEmpty(2, h)).
            occupied.push((node.x, node.x + 2.0));
            occupied.push((node.x + node.width - 2.0, node.x + node.width));
            // Partition frames have a UPath (label tab) with
            // ignoreForCompressionOnX, but its drawWhenCompressed is EMPTY —
            // it contributes nothing.
            //
            // The partition title is wrapped in a SpecialText (when
            // frame_width - title_width >= 25), which implements
            // UShapeIgnorableForCompression.  Its drawWhenCompressed draws a
            // UEmpty(1,1) at (text_x + title_width, text_y) — contributing a
            // 1px occupied interval at [frame_x + 3 + title_w,
            // frame_x + 3 + title_w + 1].
            //
            // When frame_width - title_width < 25, the title is drawn as a
            // normal TextBlock, contributing [frame_x + 3, frame_x + 3 +
            // title_w].
            if matches!(node.kind, ActivityNodeKindLayout::Partition) {
                let title_w = font_metrics::text_width(&node.text, "SansSerif", 14.0, false, false);
                if node.width - title_w >= 25.0 {
                    // SpecialText path: UEmpty(1,1) at text_x + title_w
                    occupied.push((node.x + 3.0 + title_w, node.x + 4.0 + title_w));
                } else {
                    // Normal TextBlock path: full text width
                    occupied.push((node.x + 3.0, node.x + 3.0 + title_w));
                }
            }
        } else {
            match &node.kind {
                ActivityNodeKindLayout::GotoLines { .. } => {
                    // GotoLines are drawn as ULine segments — SlotFinder
                    // doesn't handle ULine, so they don't contribute.
                }
                _ => {
                    occupied.push((node.x, node.x + node.width));
                }
            }
        }
    }

    if occupied.is_empty() {
        return Vec::new();
    }

    // Step 2: merge overlapping intervals (SlotSet.addSlot).
    occupied.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (start, end) in &occupied {
        if let Some(last) = merged.last_mut() {
            if start <= &last.1 {
                // Overlapping — merge.
                if end > &last.1 {
                    last.1 = *end;
                }
                continue;
            }
        }
        merged.push((*start, *end));
    }

    // Step 3: reverse to get empty gaps.
    let mut gaps: Vec<(f64, f64)> = Vec::new();
    let mut prev_end = merged[0].1;
    for &(start, end) in &merged[1..] {
        gaps.push((prev_end, start));
        prev_end = end;
    }

    // Step 4: smaller(5.0) — filter and shrink.
    let margin = 5.0_f64;
    let mut slots = Vec::new();
    for (start, end) in &gaps {
        let size = end - start;
        if size <= 2.0 * margin {
            continue;
        }
        slots.push(CompressSlot {
            start: start + margin,
            end: end - margin,
        });
    }

    slots
}

/// Apply the compression transform to an X coordinate.
/// Mirrors Java's `CompressionTransform.transform(v)`:
/// For each slot, if v is past the slot, subtract the full slot size;
/// if v is inside the slot, subtract (v - slot.start).
fn apply_x_compress(v: f64, slots: &[CompressSlot]) -> f64 {
    let mut delta = 0.0_f64;
    for s in slots {
        if s.start > v {
            continue;
        }
        if v > s.end {
            delta += s.end - s.start;
        } else {
            delta += v - s.start;
        }
    }
    v - delta
}

/// Create a simple vertical edge between two nodes.
fn simple_edge(from: usize, to: usize, x1: f64, y1: f64, x2: f64, y2: f64) -> ActivityEdgeLayout {
    ActivityEdgeLayout {
        from_index: from,
        to_index: to,
        label: String::new(),
        points: vec![(x1, y1), (x2, y2)],
        kind: ActivityEdgeKindLayout::Normal,
        label_xy: None,
    }
}

/// Compute the diamond bounding box for a labelled condition.
fn diamond_size(label: &str) -> (f64, f64) {
    let (tw, th) = estimate_text_size(label);
    // A diamond is wider than the text because the corners are cut.
    let w = tw.max(DIAMOND_SIZE * 2.0);
    let h = th.max(DIAMOND_SIZE * 2.0);
    (w, h)
}

/// Split a multi-line label coming out of the parser into individual lines.
///
/// The parser preserves both the literal `\n` escape (Java's
/// `BackSlash.translateBackSlashes` converts it to a real newline at
/// `Display.create` time) and the U+E100 placeholder used for `%newline()` /
/// `%n()` macro expansions.  We normalise both into U+E100, then split.
///
/// A trailing separator yields a trailing empty entry, matching Java's
/// `Display` (e.g. `"a\nb\n"` produces three lines `["a", "b", ""]`).
pub(crate) fn split_label_lines(text: &str) -> Vec<String> {
    let normalised = text.replace("\\n", "\u{E100}");
    normalised
        .split('\u{E100}')
        .map(|s| s.to_string())
        .collect()
}

/// Apply a coordinate transform to the entire layout based on the diagram
/// direction.  The layout algorithm always computes positions in top-to-bottom
/// orientation; for other directions we transform after the fact.
///
/// - `LeftToRight`: swap x/y axes so the flow goes left-to-right.
/// - `RightToLeft`: swap x/y axes then mirror horizontally.
/// - `BottomToTop`: mirror the Y axis so the flow goes bottom-to-top.
fn apply_direction_transform(
    layout: &mut ActivityLayout,
    direction: &crate::model::diagram::Direction,
) {
    use crate::model::diagram::Direction;
    match direction {
        Direction::TopToBottom => {}
        Direction::LeftToRight => {
            for node in &mut layout.nodes {
                std::mem::swap(&mut node.x, &mut node.y);
                std::mem::swap(&mut node.width, &mut node.height);
            }
            for edge in &mut layout.edges {
                for pt in &mut edge.points {
                    std::mem::swap(&mut pt.0, &mut pt.1);
                }
            }
            std::mem::swap(&mut layout.width, &mut layout.height);
        }
        Direction::RightToLeft => {
            for node in &mut layout.nodes {
                std::mem::swap(&mut node.x, &mut node.y);
                std::mem::swap(&mut node.width, &mut node.height);
            }
            for edge in &mut layout.edges {
                for pt in &mut edge.points {
                    std::mem::swap(&mut pt.0, &mut pt.1);
                }
            }
            std::mem::swap(&mut layout.width, &mut layout.height);
            let w = layout.width;
            for node in &mut layout.nodes {
                node.x = w - node.x - node.width;
            }
            for edge in &mut layout.edges {
                for pt in &mut edge.points {
                    pt.0 = w - pt.0;
                }
            }
        }
        Direction::BottomToTop => {
            let h = layout.height;
            for node in &mut layout.nodes {
                node.y = h - node.y - node.height;
            }
            for edge in &mut layout.edges {
                for pt in &mut edge.points {
                    pt.1 = h - pt.1;
                }
            }
        }
    }
}

/// Returns true if a node is a "flow" node — i.e. it participates in
/// sequential edge connections.  Notes and swimlane markers are excluded.
fn is_flow_node(kind: &ActivityNodeKindLayout) -> bool {
    !matches!(
        kind,
        ActivityNodeKindLayout::Note { .. }
            | ActivityNodeKindLayout::FloatingNote { .. }
            | ActivityNodeKindLayout::BackwardAction
            | ActivityNodeKindLayout::GotoLines { .. }
            | ActivityNodeKindLayout::Partition
    )
}

fn assign_note_modes(nodes: &mut [ActivityNodeLayout]) {
    let mut i = 0usize;
    while i < nodes.len() {
        if !is_flow_node(&nodes[i].kind) {
            i += 1;
            continue;
        }

        let mut note_indices = Vec::new();
        let mut j = i + 1;
        while j < nodes.len() {
            match nodes[j].kind {
                ActivityNodeKindLayout::Note { .. }
                | ActivityNodeKindLayout::FloatingNote { .. } => {
                    note_indices.push(j);
                }
                _ => break,
            }
            j += 1;
        }

        let mode = if note_indices.len() == 1 {
            ActivityNoteModeLayout::Single
        } else {
            ActivityNoteModeLayout::Grouped
        };
        for idx in note_indices {
            match &mut nodes[idx].kind {
                ActivityNodeKindLayout::Note {
                    mode: note_mode, ..
                }
                | ActivityNodeKindLayout::FloatingNote {
                    mode: note_mode, ..
                } => *note_mode = mode,
                _ => {}
            }
        }

        i = j;
    }
}

fn align_flow_groups_to_lane_columns(nodes: &mut [ActivityNodeLayout], node_lane: &[usize]) {
    let lane_count = node_lane
        .iter()
        .copied()
        .max()
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let mut lane_flow_centers: Vec<Option<f64>> = vec![None; lane_count];
    let mut i = 0usize;

    while i < nodes.len() {
        if !is_flow_node(&nodes[i].kind) {
            i += 1;
            continue;
        }

        let mut j = i + 1;
        while j < nodes.len() {
            match nodes[j].kind {
                ActivityNodeKindLayout::Note { .. }
                | ActivityNodeKindLayout::FloatingNote { .. } => {
                    j += 1;
                }
                _ => break,
            }
        }

        let lane_idx = node_lane.get(i).copied().unwrap_or(0);
        if let Some(prev_center) = lane_flow_centers.get(lane_idx).copied().flatten() {
            let desired_x = prev_center - nodes[i].width / 2.0;
            let dx = desired_x - nodes[i].x;
            if dx.abs() > 0.01 {
                for node in &mut nodes[i..j] {
                    node.x += dx;
                }
            }
        }

        lane_flow_centers[lane_idx] = Some(nodes[i].x + nodes[i].width / 2.0);
        i = j;
    }
}

/// Java `LimitFinder` uses shape-specific bounds when computing swimlane
/// `MinMax`.  Activity swimlane centering must follow those bounds rather than
/// the plain layout box, otherwise simple action lanes end up 1px too far left.
fn limitfinder_x_bounds(node: &ActivityNodeLayout) -> (f64, f64) {
    match &node.kind {
        ActivityNodeKindLayout::Action
        | ActivityNodeKindLayout::BackwardAction
        | ActivityNodeKindLayout::ForkBar
        | ActivityNodeKindLayout::SyncBar => (node.x - 1.0, node.x + node.width - 1.0),
        ActivityNodeKindLayout::Diamond => (node.x - 10.0, node.x + node.width + 10.0),
        ActivityNodeKindLayout::Hexagon { .. }
        | ActivityNodeKindLayout::IfDiamond { .. }
        | ActivityNodeKindLayout::GotoLines { .. }
        | ActivityNodeKindLayout::SwitchDiamond => (node.x - 1.0, node.x + node.width - 1.0),
        ActivityNodeKindLayout::Start
        | ActivityNodeKindLayout::Stop
        | ActivityNodeKindLayout::End
        | ActivityNodeKindLayout::Detach => (node.x, node.x + node.width - 1.0),
        ActivityNodeKindLayout::Note { position, mode }
        | ActivityNodeKindLayout::FloatingNote { position, mode } => match (position, mode) {
            (NotePositionLayout::Right, ActivityNoteModeLayout::Single) => {
                (node.x, node.x + node.width + 1.0)
            }
            (NotePositionLayout::Left, ActivityNoteModeLayout::Single) => {
                (node.x - 1.0, node.x + node.width)
            }
            _ => (node.x, node.x + node.width),
        },
        ActivityNodeKindLayout::Partition => (node.x, node.x + node.width),
    }
}

/// Build edges between consecutive flow nodes.
///
/// When two consecutive nodes are in different horizontal positions (i.e.
/// different swimlanes), the edge is routed as an L-shaped polyline:
/// go down from the source, then horizontally, then down into the target.
fn build_edges(nodes: &[ActivityNodeLayout]) -> Vec<ActivityEdgeLayout> {
    let flow_indices: Vec<usize> = nodes
        .iter()
        .filter(|n| is_flow_node(&n.kind) && !n.skip_in_flow)
        .map(|n| n.index)
        .collect();

    let mut edges = Vec::new();
    for pair in flow_indices.windows(2) {
        let from_idx = pair[0];
        let to_idx = pair[1];
        let from = &nodes[from_idx];
        let to = &nodes[to_idx];

        let from_cx = from.x + from.width / 2.0;
        let from_bottom = from.y + from.height;
        let to_cx = to.x + to.width / 2.0;
        let to_top = to.y;

        let points = if (from_cx - to_cx).abs() < 1.0 {
            // Same lane: simple straight vertical line.
            vec![(from_cx, from_bottom), (to_cx, to_top)]
        } else {
            // Cross-lane: default to a short source stub. When the target flow
            // group has notes protruding above the target action, Java lifts
            // the horizontal crossing to just above that group.
            let target_group_top = flow_group_top(nodes, to_idx);
            let mid_y = if target_group_top + 0.01 < to_top {
                (target_group_top - CROSS_LANE_VERTICAL_STUB).max(from_bottom)
            } else {
                let dy = to_top - from_bottom;
                let stub = CROSS_LANE_VERTICAL_STUB.min(dy.abs()).copysign(dy);
                from_bottom + stub
            };
            vec![
                (from_cx, from_bottom),
                (from_cx, mid_y),
                (to_cx, mid_y),
                (to_cx, to_top),
            ]
        };
        edges.push(ActivityEdgeLayout {
            from_index: from_idx,
            to_index: to_idx,
            label: String::new(),
            points,
            kind: ActivityEdgeKindLayout::Normal,
            label_xy: None,
        });
    }
    edges
}

/// Build the 4-point snake path for `FtileRepeat.ConnectionBackSimple2` —
/// i.e. the loop-back arrow that runs from the hex East side around the right
/// of the diagram and into the diamond1 right edge.
///
/// Returns `Some((edge, extra_right))` where `extra_right` is the additional
/// SVG width (beyond the normal content bounds) that must be reserved so the
/// loop-back line and its up/left arrowheads stay visible.
fn build_repeat_loopback_edge(
    nodes: &[ActivityNodeLayout],
    hex_idx: usize,
    diamond1_idx: usize,
) -> Option<(ActivityEdgeLayout, f64)> {
    let hex = nodes.get(hex_idx)?;
    let diamond1 = nodes.get(diamond1_idx)?;

    // Java: x1 = p1.x + dimDiamond2.width (right edge of hex).
    let x1 = hex.x + hex.width;
    // Java: y1 = p1.y + dimDiamond2.height / 2 (vertical centre of hex).
    let y1 = hex.y + hex.height / 2.0;
    // Java: x2 = p2.x + dimDiamond1.width (right edge of diamond1).
    let x2 = diamond1.x + diamond1.width;
    // Java: y2 = p2.y + dimDiamond1.height / 2 (vertical centre of diamond1).
    let y2 = diamond1.y + diamond1.height / 2.0;

    // Java: xmax = dimTotal.width - hexagonHalfSize.
    // `dimTotal.width` is the ftile's composed width, which in our layout
    // corresponds to the widest rect/hex within the repeat.  We take the
    // widest node excluding the hex's own East label extension and add the
    // `hexagonHalfSize` padding that Java reserves on both sides.
    let mut max_right: f64 = 0.0;
    for node in nodes {
        // Skip non-flow nodes (notes, detach markers) — they live outside
        // the main flow column and don't drive the loop-back width.
        if !matches!(
            node.kind,
            ActivityNodeKindLayout::Action
                | ActivityNodeKindLayout::Diamond
                | ActivityNodeKindLayout::Hexagon { .. }
                | ActivityNodeKindLayout::IfDiamond { .. }
                | ActivityNodeKindLayout::Start
                | ActivityNodeKindLayout::Stop
                | ActivityNodeKindLayout::End
                | ActivityNodeKindLayout::ForkBar
        ) {
            continue;
        }
        let right = node.x + node.width;
        if right > max_right {
            max_right = right;
        }
    }
    // If the repeat body contains an IfDiamond, the implicit else path
    // extends HEXAGON_HALF_SIZE beyond the diamond right edge.
    let has_if_diamond = nodes
        .iter()
        .any(|n| matches!(n.kind, ActivityNodeKindLayout::IfDiamond { .. }));
    let if_else_extra = if has_if_diamond {
        HEXAGON_HALF_SIZE
    } else {
        0.0
    };
    let xmax = max_right + HEXAGON_HALF_SIZE + if_else_extra;

    // Locate the gap between the first two interior actions of the enclosing
    // repeat; the UP arrow's polygon sits anchored 10px below the first
    // interior action so that `base_y == first_bottom + 20`.  Java's
    // SlotFinder/CompressionTransform reaches the same value in the final
    // SVG for these fixtures.
    let up_arrow_y = repeat_up_arrow_y(nodes, diamond1_idx, hex_idx);

    // Width the loop-back extension needs beyond the current bounds:
    // the loop-back line is at `xmax`, the arrow polygon extends to
    // `xmax + 4`, plus Java's ftile adds another `hexagonHalfSize = 12` of
    // right padding on top of the shared 10px svg margin.  That works out
    // to `(xmax + 4) - max_right + hexagonHalfSize - 1` ≈ 27 for the
    // fixtures we care about.
    //
    // We encode the full increment explicitly: loop-back line 12 + arrow
    // half-width 4 + right padding 11 = 27 (matches observed jaws8/jaws9 Δ).
    let extra_right = (xmax + 4.0) - max_right + 11.0;

    let points = vec![(x1, y1), (xmax, y1), (xmax, y2), (x2, y2)];
    Some((
        ActivityEdgeLayout {
            from_index: hex_idx,
            to_index: diamond1_idx,
            label: String::new(),
            points,
            kind: ActivityEdgeKindLayout::LoopBackSimple2 { up_arrow_y },
            label_xy: None,
        },
        extra_right,
    ))
}

/// Build backward loop-back edges: hex→backward (going right+up) and
/// backward→diamond1 (going up+left).  Returns two edges plus the extra
/// right-side width needed.
///
/// Java `ConnectionBackBackward1`: hex East → backward bottom (3 points: right, then up)
/// Java `ConnectionBackBackward2`: backward top → diamond1 right (3 points: up, then left)
fn build_backward_loopback_edges(
    nodes: &[ActivityNodeLayout],
    hex_idx: usize,
    diamond1_idx: usize,
    backward_idx: usize,
) -> Option<(Vec<ActivityEdgeLayout>, f64)> {
    let hex = nodes.get(hex_idx)?;
    let diamond1 = nodes.get(diamond1_idx)?;
    let backward = nodes.get(backward_idx)?;

    // Edge 1: hex East → backward bottom (ConnectionBackBackward1)
    // x1 = hex right edge, y1 = hex vertical center
    let x1 = hex.x + hex.width;
    let y1 = hex.y + hex.height / 2.0;
    // p2 = backward center-x, backward bottom (outY)
    let bw_cx = backward.x + backward.width / 2.0;
    let bw_bottom = backward.y + backward.height;

    let edge1 = ActivityEdgeLayout {
        from_index: hex_idx,
        to_index: backward_idx,
        label: String::new(),
        points: vec![(x1, y1), (bw_cx, y1), (bw_cx, bw_bottom)],
        kind: ActivityEdgeKindLayout::LoopBackBackward1,
        label_xy: None,
    };

    // Edge 2: backward top → diamond1 right (ConnectionBackBackward2)
    let bw_top = backward.y;
    let d1_right = diamond1.x + diamond1.width;
    let d1_mid_y = diamond1.y + diamond1.height / 2.0;

    let edge2 = ActivityEdgeLayout {
        from_index: backward_idx,
        to_index: diamond1_idx,
        label: String::new(),
        points: vec![(bw_cx, bw_top), (bw_cx, d1_mid_y), (d1_right, d1_mid_y)],
        kind: ActivityEdgeKindLayout::LoopBackBackward2,
        label_xy: None,
    };

    // The backward box is already in the node bounds, so no extra width
    // is needed beyond what compute_bounds already provides.
    Some((vec![edge1, edge2], 0.0))
}

/// Build the `while`/`endwhile` loop-back edge: from the last loop-body
/// node's bottom-centre, snaking right and up to the `while` diamond's right
/// side (arrow pointing back at `while`).  Returns the edge plus the extra
/// right-side width the snake needs beyond the current content bounds.
///
/// This mirrors `build_repeat_loopback_edge` but for the simpler `while` loop:
/// there is no hexagon and no East `is` label on the shape, so the path is a
/// plain 4-point snake rendered as a `Normal` edge (polyline + end arrowhead).
fn build_while_loopback_edge(
    nodes: &[ActivityNodeLayout],
    while_idx: usize,
    body_nodes: &[usize],
) -> Option<(ActivityEdgeLayout, f64)> {
    let wd = nodes.get(while_idx)?;
    let &body_last_idx = body_nodes.last()?;
    let bd = nodes.get(body_last_idx)?;
    let while_right = wd.x + wd.width;
    let while_cy = wd.y + wd.height / 2.0;
    let body_cx = bd.x + bd.width / 2.0;
    let body_bottom = bd.y + bd.height;

    // Scan ALL body nodes (not just the last) for the rightmost edge — a wider
    // intermediate body node would otherwise have the loop-back routed through
    // it.  Matches `build_repeat_loopback_edge`'s scan.
    let body_max_right = body_nodes
        .iter()
        .filter_map(|&i| nodes.get(i).map(|n| n.x + n.width))
        .fold(0.0_f64, f64::max);

    // Snake out to the right of whichever is wider (while or body), then up to
    // the while diamond's vertical centre, then back left to its right point.
    // Official PlantUML offsets the loop-back 12px beyond the widest of
    // while/body (mirroring the 12px exit offset on the left, so the diagram
    // is symmetric around the while diamond).
    let max_right = while_right.max(body_max_right);
    let loop_x = max_right + HEXAGON_HALF_SIZE;
    // Extra width beyond the current content right edge (loop-back column +
    // arrow half-width + right padding).
    let extra_right = (loop_x - max_right) + 4.0 + 11.0;

    let points = vec![
        (body_cx, body_bottom),
        (body_cx, body_bottom + 10.0),
        (loop_x, body_bottom + 10.0),
        (loop_x, while_cy),
        (while_right, while_cy),
    ];
    // Mid-segment UP arrow sits at the midpoint of the up-vertical (the
    // segment from body_bottom+10 up to while_cy), matching official
    // PlantUML's two-arrowhead loop-back (mid UP + end LEFT).
    let up_arrow_y = (body_bottom + 10.0 + while_cy) / 2.0;
    Some((
        ActivityEdgeLayout {
            from_index: body_last_idx,
            to_index: while_idx,
            label: String::new(),
            points,
            kind: ActivityEdgeKindLayout::LoopBackSimple2 { up_arrow_y },
            label_xy: None,
        },
        extra_right,
    ))
}

/// Move each partition node to just before its first inner child in the
/// render order, so frames are drawn behind their content.
fn apply_partition_render_order(
    base: Vec<usize>,
    partition_ranges: &[(usize, usize)],
) -> Vec<usize> {
    // partition node indices that should be relocated
    let partition_nodes: std::collections::HashSet<usize> =
        partition_ranges.iter().map(|&(_, p)| p).collect();
    // map: first_inner_idx -> partition_node_idx
    let mut first_to_part: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    for &(first, part) in partition_ranges {
        first_to_part.insert(first, part);
    }

    let mut result: Vec<usize> = Vec::with_capacity(base.len());
    for &idx in &base {
        // If this index is a partition's first inner child, emit the
        // partition frame first.
        if let Some(&part_idx) = first_to_part.get(&idx) {
            result.push(part_idx);
        }
        // Skip partition nodes in their original position — they've been
        // relocated above.
        if !partition_nodes.contains(&idx) {
            result.push(idx);
        }
    }
    result
}

/// Reorder edges to match Java's `FtileWithConnection.drawU` emission
/// order.  `FtileWithConnection.drawU` draws its delegated subtree first,
/// then its own connection — a post-order traversal where an inter-tile
/// connection `[A→B]` is emitted AFTER both A's and B's internal edges.
/// For the flat node array this collapses to: emit `tile[0]`'s internal
/// edges, then for each subsequent tile `tile[k]` emit its internal edges
/// followed by the inter-tile connection `c[k-1,k]` leading into it.
///
/// We reproduce that order by stable-sorting edges by
/// `(top_level_tile_rep(to_index), subgroup)` where `subgroup=0` for edges
/// internal to a tile (from & to in the same outermost tile) and
/// `subgroup=1` for inter-tile connections.  `tiles` is the union of all
/// partition spans and fork spans; `top_level_tile_rep(idx)` is the start
/// of the outermost such tile containing `idx`, or `idx` itself when `idx`
/// is a singleton (start/stop/action outside any tile).
fn reorder_edges_for_partitions(
    edges: Vec<ActivityEdgeLayout>,
    tiles: &[(usize, usize)],
) -> Vec<ActivityEdgeLayout> {
    /// Start of the outermost tile containing `idx`, or `idx` itself when
    /// `idx` is not inside any tile.  Among nested containing tiles the
    /// outermost has the smallest start index.
    fn top_level_rep(tiles: &[(usize, usize)], idx: usize) -> usize {
        let mut best: Option<usize> = None;
        for &(s, e) in tiles {
            if idx >= s && idx <= e && best.map_or(true, |b| s < b) {
                best = Some(s);
            }
        }
        best.unwrap_or(idx)
    }

    let mut indexed: Vec<(usize, ActivityEdgeLayout)> = edges.into_iter().enumerate().collect();
    indexed.sort_by(|(i, a), (j, b)| {
        let a_to = top_level_rep(tiles, a.to_index);
        let a_from = top_level_rep(tiles, a.from_index);
        let b_to = top_level_rep(tiles, b.to_index);
        let b_from = top_level_rep(tiles, b.from_index);
        let a_sub = if a_from == a_to { 0u8 } else { 1u8 };
        let b_sub = if b_from == b_to { 0u8 } else { 1u8 };
        (a_to, a_sub, *i).cmp(&(b_to, b_sub, *j))
    });
    indexed.into_iter().map(|(_, e)| e).collect()
}

/// Compute a render-order permutation so the inner repeat body (the interior
/// actions between `diamond1` and `hex`) is drawn BEFORE `diamond1`, which
/// matches Java `FtileRepeat.drawU`'s order: `repeat` (body), then
/// `diamond1`, then `diamond2` (hex).  Returns a vector of `nodes` vec
/// positions in draw order.  The `nodes` vec itself is NOT reordered, so
/// `ActivityLayout.nodes[i].index == i` invariants remain valid.
fn compute_render_order_for_repeat(
    nodes: &[ActivityNodeLayout],
    repeat_loopbacks: &[(usize, usize)],
    backward_loopbacks: &[(usize, usize, usize)],
) -> Vec<usize> {
    // Default order = identity.
    if repeat_loopbacks.is_empty() && backward_loopbacks.is_empty() {
        return (0..nodes.len()).collect();
    }
    // Build a map `diamond1_idx -> hex_idx`.
    let mut hex_for_d1: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for &(hex, d1) in repeat_loopbacks {
        hex_for_d1.insert(d1, hex);
    }
    // Build backward map: diamond1_idx -> backward_idx
    let mut backward_for_d1: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    for &(hex, d1, bw) in backward_loopbacks {
        hex_for_d1.insert(d1, hex);
        backward_for_d1.insert(d1, bw);
    }

    let mut result: Vec<usize> = Vec::with_capacity(nodes.len());
    let mut consumed = vec![false; nodes.len()];
    let mut i = 0;
    while i < nodes.len() {
        if consumed[i] {
            i += 1;
            continue;
        }
        let node = &nodes[i];
        if let Some(&hex_idx) = hex_for_d1.get(&node.index) {
            // Emit inner body first, then diamond1, then hex, then backward.
            // Java's FtileIfWithDiamonds draws the inner ftile (then-branch)
            // before the diamond shapes, so when we encounter an IfDiamond
            // node, we first emit its then-branch actions, then the diamond.
            let body_end = (i + 1..nodes.len())
                .find(|&j| nodes[j].index == hex_idx)
                .unwrap_or(nodes.len());
            let mut body: Vec<usize> = Vec::new();
            #[allow(clippy::needless_range_loop)]
            for j in (i + 1)..body_end {
                if !consumed[j] {
                    body.push(j);
                }
            }
            // Reorder if-blocks: move then-branch actions before their IfDiamond
            let reordered = reorder_if_nodes_for_draw(nodes, &body);
            for j in reordered {
                result.push(j);
                consumed[j] = true;
            }
            result.push(i);
            consumed[i] = true;
            // Emit hex next.
            for j in (i + 1)..nodes.len() {
                if nodes[j].index == hex_idx {
                    result.push(j);
                    consumed[j] = true;
                    break;
                }
            }
            // Emit backward node if present.
            if let Some(&bw_idx) = backward_for_d1.get(&node.index) {
                for j in 0..nodes.len() {
                    if nodes[j].index == bw_idx && !consumed[j] {
                        result.push(j);
                        consumed[j] = true;
                        break;
                    }
                }
            }
            i += 1;
            continue;
        }
        result.push(i);
        consumed[i] = true;
        i += 1;
    }
    result
}

/// Reorder nodes within a repeat body so that Java's `FtileIfWithDiamonds`
/// draw order is matched: when an IfDiamond node is encountered, its
/// then-branch actions (immediately following nodes with `skip_in_flow=true`)
/// are emitted *before* the IfDiamond itself.
fn reorder_if_nodes_for_draw(nodes: &[ActivityNodeLayout], body: &[usize]) -> Vec<usize> {
    let mut result = Vec::with_capacity(body.len());
    let mut k = 0;
    while k < body.len() {
        let j = body[k];
        if matches!(nodes[j].kind, ActivityNodeKindLayout::IfDiamond { .. }) {
            // Collect then-branch actions: consecutive nodes after the
            // IfDiamond that have skip_in_flow=true (they are inside the if).
            let mut then_end = k + 1;
            while then_end < body.len() {
                let nj = body[then_end];
                if nodes[nj].skip_in_flow {
                    then_end += 1;
                } else {
                    break;
                }
            }
            // Emit then-branch first, then the IfDiamond
            for &b in &body[(k + 1)..then_end] {
                result.push(b);
            }
            result.push(j);
            k = then_end;
        } else {
            result.push(j);
            k += 1;
        }
    }
    result
}

/// Reorder the edges list so that every closed `repeat`/`repeat while`
/// block renders its connections in Java's order:
///   1. inner → inner body edges (between interior actions)
///   2. diamond1 → first interior action (ConnectionIn)
///   3. loop-back edge (ConnectionBackSimple2)
///   4. last interior action → hex (ConnectionOut)
///
/// Edges outside any repeat block keep their natural top-to-bottom order.
fn reorder_edges_for_repeat(
    edges: Vec<ActivityEdgeLayout>,
    loopbacks: &[(usize, usize, Vec<ActivityEdgeLayout>)],
    nodes: &[ActivityNodeLayout],
) -> Vec<ActivityEdgeLayout> {
    // Build a lookup: for each repeat frame (diamond1, hex), collect the
    // interior edge indices and emit them in the required order.
    let mut result: Vec<ActivityEdgeLayout> = Vec::with_capacity(edges.len() + loopbacks.len());

    // Index edges by their from_index for quick removal.
    let mut consumed = vec![false; edges.len()];

    // Maintain a stack of open frames keyed by diamond1 index so that
    // enter-edges and exit-edges can be identified.
    let mut frames: Vec<(usize, usize, &Vec<ActivityEdgeLayout>)> = loopbacks
        .iter()
        .map(|(d1, hex, edges)| (*d1, *hex, edges))
        .collect();
    frames.sort_by_key(|f| f.0);

    // For each edge in the original order, emit it at its natural spot
    // unless it is the "enter" (d1 → first) or "exit" (last → hex) of a
    // repeat frame, in which case we defer the enter/exit edges to the
    // correct per-frame order.
    //
    // The simplest correct ordering rule is: iterate edges top-to-bottom;
    // when we reach the `d1→first_inner` edge of a repeat, defer it.  When
    // we reach the `last_inner→hex` edge, first flush the deferred enter
    // edge followed by the loop-back, THEN emit the exit edge — but only
    // AFTER emitting all other inner edges that precede it.  Java actually
    // emits `inner→inner` edges BEFORE the `enter` edge, so we additionally
    // pre-emit those.

    // Classify edges by which frame (if any) they belong to.
    let mut frame_enter: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut frame_exit: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut frame_inner: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    // hex→exit_diamond continuation edge (emitted after exit edge, before outer edges)
    let mut frame_continuation: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();

    for (ei, edge) in edges.iter().enumerate() {
        for (d1, hex, _) in &frames {
            if edge.from_index == *d1 && edge.to_index > *d1 && edge.to_index < *hex {
                frame_enter.insert(*d1, ei);
            } else if edge.to_index == *hex && edge.from_index > *d1 && edge.from_index < *hex {
                frame_exit.insert(*d1, ei);
            } else if edge.from_index == *hex
                && edge.to_index == *hex + 1
                && nodes
                    .get(*hex + 1)
                    .is_some_and(|n| matches!(n.kind, ActivityNodeKindLayout::Diamond))
            {
                // hex → exit diamond: continuation edge within repeat
                // (only when the next node is actually a Diamond, not a Stop etc.)
                frame_continuation.insert(*d1, ei);
            } else if edge.from_index > *d1
                && edge.from_index < *hex
                && (edge.to_index < *hex || matches!(edge.kind, ActivityEdgeKindLayout::BreakEdge))
            {
                // Inner edges include break edges that exit the repeat body
                // to the exit diamond — Java draws these as part of the
                // if-block connections within the repeat body.
                frame_inner.entry(*d1).or_default().push(ei);
            }
        }
    }

    // Build the output list: walk the original edges in order, and when we
    // hit an edge tied to a repeat frame, emit the whole frame in Java order
    // the first time we encounter it, then skip any later edges from the
    // same frame.
    // Collect edges that enter/leave a frame from/to outside.
    // These are deferred until after the frame's internal edges.
    let frame_d1s: std::collections::HashSet<usize> = frames.iter().map(|(d1, _, _)| *d1).collect();
    let frame_hexs: std::collections::HashSet<usize> =
        frames.iter().map(|(_, hex, _)| *hex).collect();
    let mut deferred_outer: Vec<usize> = Vec::new();

    let mut emitted_frame = std::collections::HashSet::new();
    for (ei, edge) in edges.iter().enumerate() {
        if consumed[ei] {
            continue;
        }
        // Check if this is an outer edge (from outside → diamond1, or hex → outside)
        if frame_d1s.contains(&edge.to_index)
            && !frame_d1s.contains(&edge.from_index)
            && edge.from_index < edge.to_index
        {
            deferred_outer.push(ei);
            consumed[ei] = true;
            continue;
        }
        if frame_hexs.contains(&edge.from_index)
            && !frame_hexs.contains(&edge.to_index)
            && edge.to_index > edge.from_index
        {
            // Don't defer continuation edges (hex→exit_diamond); they are
            // part of the repeat frame and handled via frame_continuation.
            if !frame_continuation.values().any(|&ci| ci == ei) {
                deferred_outer.push(ei);
                consumed[ei] = true;
                continue;
            }
        }
        // Also defer edges starting from the exit diamond (hex+1) that go
        // further out — these are outer assembly connections (e.g., exit→stop).
        // Only applies when hex+1 is actually a Diamond (exit diamond).
        {
            let is_exit_diamond_source = frames.iter().any(|(_, hex, _)| {
                edge.from_index == *hex + 1
                    && nodes
                        .get(*hex + 1)
                        .is_some_and(|n| matches!(n.kind, ActivityNodeKindLayout::Diamond))
            });
            if is_exit_diamond_source && edge.to_index > edge.from_index {
                deferred_outer.push(ei);
                consumed[ei] = true;
                continue;
            }
        }
        // Does this edge belong to a repeat frame?
        let mut owner: Option<usize> = None;
        for (d1, hex, _) in &frames {
            if edge.from_index >= *d1 && edge.to_index <= *hex && (edge.from_index != edge.to_index)
            {
                owner = Some(*d1);
                break;
            }
        }
        if let Some(d1) = owner {
            if !emitted_frame.contains(&d1) {
                emitted_frame.insert(d1);
                // 1) Inner → inner edges (in their original order).
                if let Some(inners) = frame_inner.get(&d1) {
                    for &ii in inners {
                        result.push(edges[ii].clone());
                        consumed[ii] = true;
                    }
                }
                // 2) Enter edge (diamond1 → first interior).
                if let Some(&enter_ei) = frame_enter.get(&d1) {
                    result.push(edges[enter_ei].clone());
                    consumed[enter_ei] = true;
                }
                // 3) Loop-back edge(s) (may be multiple for backward).
                if let Some((_, _, loopback_edges)) = frames.iter().find(|f| f.0 == d1) {
                    for e in loopback_edges.iter() {
                        result.push(e.clone());
                    }
                }
                // 4) Exit edge (last interior → hex).
                if let Some(&exit_ei) = frame_exit.get(&d1) {
                    result.push(edges[exit_ei].clone());
                    consumed[exit_ei] = true;
                }
                // 5) Continuation edge (hex → exit diamond).
                if let Some(&cont_ei) = frame_continuation.get(&d1) {
                    result.push(edges[cont_ei].clone());
                    consumed[cont_ei] = true;
                }
            }
            // Already emitted via the frame — skip.
            consumed[ei] = true;
            continue;
        }
        // Non-repeat edge: emit as-is.
        result.push(edge.clone());
        consumed[ei] = true;
    }

    // Emit deferred outer edges (Start→Diamond1, Hex→Stop) in their natural order
    for &ei in &deferred_outer {
        result.push(edges[ei].clone());
    }

    result
}

/// Compute the Y position of the mid-segment UP arrow polygon origin (tip)
/// for a `ConnectionBackSimple2` loop-back.  The polygon anchor sits 10px
/// below the bottom of the first interior flow node of the enclosing repeat
/// block — empirically matching Java's SlotFinder/CompressionTransform result
/// on these fixtures.  Falls back to the vertical-segment midpoint when no
/// suitable interior node can be located.
fn repeat_up_arrow_y(nodes: &[ActivityNodeLayout], diamond1_idx: usize, hex_idx: usize) -> f64 {
    // Look for the first interior flow action between diamond1 and hex.
    for n in nodes
        .iter()
        .skip(diamond1_idx + 1)
        .take(hex_idx - diamond1_idx - 1)
    {
        if matches!(n.kind, ActivityNodeKindLayout::Action) {
            return n.y + n.height + 10.0;
        }
    }
    // Fallback: midpoint of the vertical segment.
    let d1_mid = nodes[diamond1_idx].y + nodes[diamond1_idx].height / 2.0;
    let hex_mid = nodes[hex_idx].y + nodes[hex_idx].height / 2.0;
    (d1_mid + hex_mid) / 2.0
}

fn flow_group_top(nodes: &[ActivityNodeLayout], flow_idx: usize) -> f64 {
    let mut top = nodes[flow_idx].y;
    let mut j = flow_idx + 1;
    while j < nodes.len() {
        match nodes[j].kind {
            ActivityNodeKindLayout::Note { .. } | ActivityNodeKindLayout::FloatingNote { .. } => {
                top = top.min(nodes[j].y);
            }
            _ => break,
        }
        j += 1;
    }
    top
}

const OLD_ACTIVITY_BRANCH_SIZE: f64 = 24.0;
const OLD_ACTIVITY_EDGE_FONT_SIZE: f64 = 11.0;

fn old_activity_center_label_dimension(text: &str) -> (f64, f64) {
    let line_h = font_metrics::line_height("SansSerif", OLD_ACTIVITY_EDGE_FONT_SIZE, false, false);
    let text_w =
        font_metrics::text_width(text, "SansSerif", OLD_ACTIVITY_EDGE_FONT_SIZE, false, false);
    (text_w + 2.0, line_h + 2.0)
}

fn old_activity_side_label_dimension(text: &str) -> (f64, f64) {
    let display = if text.is_empty() { " " } else { text };
    let line_h = font_metrics::line_height("SansSerif", OLD_ACTIVITY_EDGE_FONT_SIZE, false, false);
    let text_w = font_metrics::text_width(
        display,
        "SansSerif",
        OLD_ACTIVITY_EDGE_FONT_SIZE,
        false,
        false,
    );
    (text_w, line_h)
}

fn layout_old_style_activity_graph(
    _diagram: &ActivityDiagram,
    old_graph: &OldActivityGraph,
) -> Result<ActivityLayout> {
    let nodes: Vec<LayoutNode> = old_graph
        .nodes
        .iter()
        .map(|node| {
            let (shape, width, height, text) = match node.kind {
                OldActivityNodeKind::Start => (
                    Some(crate::svek::shape_type::ShapeType::Circle),
                    20.0,
                    20.0,
                    String::new(),
                ),
                OldActivityNodeKind::End => (
                    Some(crate::svek::shape_type::ShapeType::Circle),
                    22.0,
                    22.0,
                    String::new(),
                ),
                OldActivityNodeKind::Action => {
                    let (w, h) = estimate_text_size(&node.text);
                    (
                        Some(crate::svek::shape_type::ShapeType::RoundRectangle),
                        w,
                        h,
                        node.text.clone(),
                    )
                }
                OldActivityNodeKind::Branch => (
                    Some(crate::svek::shape_type::ShapeType::Diamond),
                    OLD_ACTIVITY_BRANCH_SIZE,
                    OLD_ACTIVITY_BRANCH_SIZE,
                    String::new(),
                ),
                OldActivityNodeKind::SyncBar => (
                    Some(crate::svek::shape_type::ShapeType::Rectangle),
                    80.0,
                    8.0,
                    String::new(),
                ),
            };
            LayoutNode {
                id: node.id.clone(),
                label: text,
                width_pt: width,
                height_pt: height,
                shape,
                shield: None,
                entity_position: None,
                max_label_width: None,
                port_label_width: None,
                order: None,
                image_width_pt: None,
                image_height_pt: None,
                lf_extra_left: 0.0,
                lf_rect_correction: true,
                lf_has_body_separator: false,
                lf_node_polygon: false,
                lf_polygon_hack: false,
                lf_actor_stickman: false,
                hidden: false,
            }
        })
        .collect();

    let edges: Vec<LayoutEdge> = old_graph
        .links
        .iter()
        .map(|link| LayoutEdge {
            from: link.from_id.clone(),
            to: link.to_id.clone(),
            label: link.label.clone(),
            label_dimension: link
                .label
                .as_deref()
                .map(old_activity_center_label_dimension),
            tail_label: None,
            tail_label_boxed: false,
            head_label: link.head_label.clone(),
            head_label_boxed: false,
            tail_decoration: crate::svek::edge::LinkDecoration::None,
            head_decoration: crate::svek::edge::LinkDecoration::None,
            line_style: crate::svek::edge::LinkStyle::Normal,
            minlen: link.length.saturating_sub(1),
            invisible: false,
            is_opale: false,
            no_constraint: false,
            tail_label_dimension: None,
            head_label_dimension: link
                .head_label
                .as_deref()
                .map(old_activity_side_label_dimension),
        })
        .collect();

    let graph = LayoutGraph {
        nodes,
        edges,
        clusters: Vec::new(),
        rankdir: RankDir::TopToBottom,
        is_activity: false,
        ranksep_override: Some(40.0),
        nodesep_override: Some(20.0),
        use_simplier_dot_link_strategy: false,
        arrow_font_size: None,
    };

    let gl = layout_with_svek(&graph)?;
    let edge_offset_x = gl.render_offset.0;
    let edge_offset_y = gl.render_offset.1;

    let node_by_id: std::collections::HashMap<&str, &crate::layout::graphviz::NodeLayout> = gl
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();

    // Precompute the first (smallest) source_line that references each node
    // as either endpoint. Java PlantUML emits this as `data-source-line` on
    // `start_entity` / `end_entity` wrappers.
    let mut node_source_line: HashMap<String, usize> = HashMap::new();
    for link in &old_graph.links {
        for endpoint in [&link.from_id, &link.to_id] {
            node_source_line
                .entry(endpoint.clone())
                .and_modify(|existing| {
                    if link.source_line < *existing {
                        *existing = link.source_line;
                    }
                })
                .or_insert(link.source_line);
        }
    }

    // Also precompute id -> uid for edges so the renderer can populate
    // `data-entity-1` / `data-entity-2` (which require the uid, not the
    // raw id like "start" / "end").
    let node_uid_by_id: HashMap<String, String> = old_graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.uid.clone()))
        .collect();

    let mut activity_nodes = Vec::with_capacity(old_graph.nodes.len());
    let mut old_node_meta = Vec::with_capacity(old_graph.nodes.len());
    let mut node_layout_index = HashMap::new();

    for (idx, node) in old_graph.nodes.iter().enumerate() {
        let gv = node_by_id.get(node.id.as_str()).copied().ok_or_else(|| {
            crate::Error::Layout(format!("missing old-style activity node {}", node.id))
        })?;
        let kind = match node.kind {
            OldActivityNodeKind::Start => ActivityNodeKindLayout::Start,
            OldActivityNodeKind::End => ActivityNodeKindLayout::Stop,
            OldActivityNodeKind::Action => ActivityNodeKindLayout::Action,
            OldActivityNodeKind::Branch => ActivityNodeKindLayout::Diamond,
            OldActivityNodeKind::SyncBar => ActivityNodeKindLayout::SyncBar,
        };
        activity_nodes.push(ActivityNodeLayout {
            index: idx,
            kind,
            x: gv.min_x + edge_offset_x,
            y: gv.min_y + edge_offset_y,
            width: gv.width,
            height: gv.height,
            text: node.text.clone(),
            skip_in_flow: false,
        });
        old_node_meta.push(Some(ActivityGraphvizNodeMeta {
            id: node.id.clone(),
            uid: node.uid.clone(),
            qualified_name: node.qualified_name.clone(),
            source_line: node_source_line.get(&node.id).copied(),
        }));
        node_layout_index.insert(node.id.clone(), idx);
    }

    let mut activity_edges = Vec::with_capacity(old_graph.links.len());
    let mut old_edge_meta = Vec::with_capacity(old_graph.links.len());
    for (idx, link) in old_graph.links.iter().enumerate() {
        let gv = gl.edges.get(idx).ok_or_else(|| {
            crate::Error::Layout(format!("missing old-style activity edge {}", link.uid))
        })?;
        let from_index = *node_layout_index.get(&link.from_id).ok_or_else(|| {
            crate::Error::Layout(format!("missing activity edge source {}", link.from_id))
        })?;
        let to_index = *node_layout_index.get(&link.to_id).ok_or_else(|| {
            crate::Error::Layout(format!("missing activity edge target {}", link.to_id))
        })?;
        let shifted_points: Vec<(f64, f64)> = gv
            .points
            .iter()
            .map(|&(x, y)| (x + edge_offset_x, y + edge_offset_y))
            .collect();
        let label_xy = gv.label_xy.map(|(x, y)| {
            (
                x + gl.move_delta.0 - gl.normalize_offset.0 + edge_offset_x,
                y + gl.move_delta.1 - gl.normalize_offset.1 + edge_offset_y,
            )
        });
        let head_label_xy = gv.head_label_xy.map(|(x, y)| {
            (
                x + gl.move_delta.0 - gl.normalize_offset.0 + edge_offset_x,
                y + gl.move_delta.1 - gl.normalize_offset.1 + edge_offset_y,
            )
        });
        activity_edges.push(ActivityEdgeLayout {
            from_index,
            to_index,
            label: link.label.clone().unwrap_or_default(),
            points: shifted_points,
            kind: ActivityEdgeKindLayout::Normal,
            label_xy: None,
        });
        let from_uid = node_uid_by_id
            .get(&link.from_id)
            .cloned()
            .unwrap_or_default();
        let to_uid = node_uid_by_id.get(&link.to_id).cloned().unwrap_or_default();
        old_edge_meta.push(Some(ActivityGraphvizEdgeMeta {
            uid: link.uid.clone(),
            from_id: link.from_id.clone(),
            to_id: link.to_id.clone(),
            from_uid,
            to_uid,
            source_line: link.source_line,
            raw_path_d: gv
                .raw_path_d
                .as_ref()
                .map(|raw| transform_path_d(raw, edge_offset_x, edge_offset_y)),
            arrow_polygon_points: gv.arrow_polygon_points.as_ref().map(|pts| {
                pts.iter()
                    .map(|&(x, y)| (x + edge_offset_x, y + edge_offset_y))
                    .collect()
            }),
            label_xy,
            head_label: link.head_label.clone(),
            head_label_xy,
        }));
    }

    Ok(ActivityLayout {
        width: gl.total_width + 12.0,
        height: gl.total_height + 12.0,
        nodes: activity_nodes,
        edges: activity_edges,
        swimlane_layouts: Vec::new(),
        old_style_graphviz: true,
        old_node_meta,
        old_edge_meta,
        render_order: None,
    })
}

/// Compute the total bounding box of the diagram.
fn compute_bounds(
    nodes: &[ActivityNodeLayout],
    swimlane_layouts: &[SwimlaneLayout],
    _y_cursor: f64,
) -> (f64, f64) {
    if nodes.is_empty() && swimlane_layouts.is_empty() {
        return (2.0 * TOP_MARGIN, 2.0 * TOP_MARGIN);
    }

    let mut max_x: f64 = 0.0;
    let mut max_y: f64 = 0.0;

    for node in nodes {
        let right = node.x + node.width;
        let bottom = node.y + node.height;
        if right > max_x {
            max_x = right;
        }
        if bottom > max_y {
            max_y = bottom;
        }
    }

    if !swimlane_layouts.is_empty() {
        for lane in swimlane_layouts {
            let right = lane.x + lane.width;
            if right > max_x {
                max_x = right;
            }
        }
        (max_x + BOTTOM_MARGIN + 12.0, max_y + BOTTOM_MARGIN + 4.0)
    } else {
        (max_x + BOTTOM_MARGIN + 3.0, max_y + BOTTOM_MARGIN + 3.0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a diagram with given events and no swimlanes.
    fn diagram(events: Vec<ActivityEvent>) -> ActivityDiagram {
        ActivityDiagram {
            events,
            swimlanes: vec![],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        }
    }

    // 1. Empty diagram -------------------------------------------------------

    #[test]
    fn empty_diagram() {
        let d = diagram(vec![]);
        let layout = layout_activity(&d).unwrap();
        assert!(layout.nodes.is_empty());
        assert!(layout.edges.is_empty());
        assert!(layout.swimlane_layouts.is_empty());
        assert!(layout.width > 0.0);
        assert!(layout.height > 0.0);
    }

    // 1b. Creole table height includes cell padding (Java +4px) ---------------

    #[test]
    fn creole_table_height_includes_cell_padding() {
        // Java CreoleTableMetricsTest: table row adds 4px total to action height
        // Plain "text": action_h = 33.97 (line_height + 2*PADDING)
        // Table "|text|": action_h = 37.97 (+4 from cell padding 2+2)
        let (_, h_plain) = estimate_text_size("plain text");
        let (_, h_table) = estimate_text_size("|table cell|");
        let diff = h_table - h_plain;
        assert!(
            (diff - 4.0).abs() < 0.1,
            "table should be 4px taller than plain: diff={diff:.1} (table={h_table:.1} plain={h_plain:.1})"
        );
    }

    // 2. Single action -------------------------------------------------------

    #[test]
    fn single_action() {
        let d = diagram(vec![ActivityEvent::Action {
            text: "Hello".into(),
        }]);
        let layout = layout_activity(&d).unwrap();
        assert_eq!(layout.nodes.len(), 1);
        assert_eq!(layout.edges.len(), 0);
        let node = &layout.nodes[0];
        assert_eq!(node.kind, ActivityNodeKindLayout::Action);
        assert_eq!(node.text, "Hello");
        assert!(node.width >= 30.0);
        assert!(node.height >= 20.0);
    }

    // 2b. Java circle sizes: start=20, stop=22 (FtileCircleStart/Stop) ------

    #[test]
    fn stop_circle_size_matches_java() {
        // Java: FtileCircleStart SIZE=20, FtileCircleStop SIZE=22
        // start diameter=20, stop diameter=22 (outer ring r=11)
        let d = diagram(vec![ActivityEvent::Start, ActivityEvent::Stop]);
        let layout = layout_activity(&d).unwrap();
        let start = &layout.nodes[0];
        let stop = &layout.nodes[1];
        assert!(
            (start.height - 20.0).abs() < 0.1,
            "start height should be 20 (Java FtileCircleStart SIZE=20), got {}",
            start.height
        );
        assert!(
            (stop.height - 22.0).abs() < 0.1,
            "stop height should be 22 (Java FtileCircleStop SIZE=22), got {}",
            stop.height
        );
    }

    // 3. Start -> Stop -------------------------------------------------------

    #[test]
    fn start_stop() {
        let d = diagram(vec![ActivityEvent::Start, ActivityEvent::Stop]);
        let layout = layout_activity(&d).unwrap();
        assert_eq!(layout.nodes.len(), 2);
        assert_eq!(layout.edges.len(), 1);

        let start = &layout.nodes[0];
        let stop = &layout.nodes[1];
        assert_eq!(start.kind, ActivityNodeKindLayout::Start);
        assert_eq!(stop.kind, ActivityNodeKindLayout::Stop);

        // Stop should be below Start.
        assert!(stop.y > start.y + start.height);

        // Edge connects them.
        let edge = &layout.edges[0];
        assert_eq!(edge.from_index, 0);
        assert_eq!(edge.to_index, 1);
    }

    // 4. Swimlanes -----------------------------------------------------------

    #[test]
    fn swimlanes() {
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Swimlane {
                    name: "Lane A".into(),
                },
                ActivityEvent::Action {
                    text: "Task A".into(),
                },
                ActivityEvent::Swimlane {
                    name: "Lane B".into(),
                },
                ActivityEvent::Action {
                    text: "Task B".into(),
                },
            ],
            swimlanes: vec!["Lane A".into(), "Lane B".into()],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();
        assert_eq!(layout.swimlane_layouts.len(), 2);
        assert_eq!(layout.nodes.len(), 2);

        let node_a = &layout.nodes[0];
        let node_b = &layout.nodes[1];

        // Lane A center should differ from Lane B center.
        let center_a = node_a.x + node_a.width / 2.0;
        let center_b = node_b.x + node_b.width / 2.0;
        assert!(
            (center_a - center_b).abs() > 1.0,
            "nodes should be in different lanes"
        );

        // Lane B should be to the right of Lane A.
        assert!(
            layout.swimlane_layouts[1].x > layout.swimlane_layouts[0].x,
            "lane B should be to the right"
        );
    }

    // 4b. Swimlane left margin matches Java divider (Java=20px for simple case)

    #[test]
    fn swimlane_left_margin_matches_java() {
        // Java CreoleNoteMetricsTest.swimlaneDividerAndMargins:
        //   Lane A left x = 20.0 (LaneDivider x1=5 + x2 expansion)
        //   Lane A width = 71.6, Lane B width = 71.6
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Swimlane {
                    name: "Lane A".into(),
                },
                ActivityEvent::Start,
                ActivityEvent::Action {
                    text: "task A".into(),
                },
                ActivityEvent::Stop,
                ActivityEvent::Swimlane {
                    name: "Lane B".into(),
                },
                ActivityEvent::Action {
                    text: "task B".into(),
                },
            ],
            swimlanes: vec!["Lane A".into(), "Lane B".into()],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();
        let lane_a = &layout.swimlane_layouts[0];
        // Java Lane A left x ≈ 20; Rust should be > 5 (old value) and reasonable
        assert!(
            lane_a.x >= 8.0,
            "Lane A x ({:.1}) should be > 8 (Java=20, left divider expands for title)",
            lane_a.x
        );
        // Java Lane A width = 71.6, should not be inflated to 80 by min-width
        assert!(
            lane_a.width < 80.0,
            "Lane A width ({:.1}) should be < 80 (Java=71.6, no artificial min-width)",
            lane_a.width
        );
    }

    // 4c. Swimlane width expands for note content (Java compat) ---------------

    #[test]
    fn swimlane_width_accommodates_note() {
        // Java CreoleNoteMetricsTest.swimlaneWidthWithNotes:
        //   Lane A width = 188px (includes action + note + gap)
        //   Lane B width = 72px
        // Swimlane must expand to fit the composite (flow node + note) width.
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Swimlane {
                    name: "Lane A".into(),
                },
                ActivityEvent::Start,
                ActivityEvent::Action {
                    text: "action".into(),
                },
                ActivityEvent::Note {
                    position: NotePosition::Right,
                    text: "a short note".into(),
                },
                ActivityEvent::Swimlane {
                    name: "Lane B".into(),
                },
                ActivityEvent::Action {
                    text: "task2".into(),
                },
                ActivityEvent::Stop,
            ],
            swimlanes: vec!["Lane A".into(), "Lane B".into()],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();
        let lane_a = &layout.swimlane_layouts[0];
        // Java Lane A ≈ 188px.  Must be wider than the base header-only width.
        assert!(
            lane_a.width >= 150.0,
            "Lane A width ({:.1}) should be >= 150 to fit action + note. Java=188",
            lane_a.width
        );
        // Note must be fully inside Lane A boundary
        let note = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::Note { .. }))
            .unwrap();
        let note_right = note.x + note.width;
        let lane_a_right = lane_a.x + lane_a.width;
        assert!(
            note_right <= lane_a_right + 1.0,
            "note right ({:.1}) should be within Lane A right ({:.1})",
            note_right,
            lane_a_right
        );
    }

    #[test]
    fn swimlane_content_shift_uses_limitfinder_rectangle_bounds() {
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Swimlane {
                    name: "Swimlane1".into(),
                },
                ActivityEvent::Start,
                ActivityEvent::Action {
                    text: "Action 1".into(),
                },
                ActivityEvent::Swimlane {
                    name: "Swimlane2".into(),
                },
                ActivityEvent::Action {
                    text: "Action 2".into(),
                },
            ],
            swimlanes: vec!["Swimlane1".into(), "Swimlane2".into()],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();
        let lane_a = &layout.swimlane_layouts[0];
        let lane_b = &layout.swimlane_layouts[1];
        let action_a = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::Action) && n.text == "Action 1")
            .unwrap();
        let action_b = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::Action) && n.text == "Action 2")
            .unwrap();

        let expected_a = lane_a.x + (lane_a.width - action_a.width) / 2.0 + 1.0;
        let expected_b = lane_b.x + (lane_b.width - action_b.width) / 2.0 + 1.0;
        assert!(
            (action_a.x - expected_a).abs() < 0.01,
            "lane A action.x ({:.4}) should include the LimitFinder rectangle shift ({:.4})",
            action_a.x,
            expected_a
        );
        assert!(
            (action_b.x - expected_b).abs() < 0.01,
            "lane B action.x ({:.4}) should include the LimitFinder rectangle shift ({:.4})",
            action_b.x,
            expected_b
        );
    }

    // 5. Note beside action --------------------------------------------------

    #[test]
    fn note_beside_action() {
        let d = diagram(vec![
            ActivityEvent::Action {
                text: "Do work".into(),
            },
            ActivityEvent::Note {
                position: NotePosition::Right,
                text: "This is a note".into(),
            },
        ]);
        let layout = layout_activity(&d).unwrap();
        assert_eq!(layout.nodes.len(), 2);

        let action = &layout.nodes[0];
        let note = &layout.nodes[1];
        assert_eq!(
            note.kind,
            ActivityNodeKindLayout::Note {
                position: NotePositionLayout::Right,
                mode: ActivityNoteModeLayout::Single,
            }
        );

        // Note should be to the right of the action.
        assert!(note.x > action.x + action.width);

        // Note and action should be vertically centred on each other.
        let action_mid = action.y + action.height / 2.0;
        let note_mid = note.y + note.height / 2.0;
        assert!(
            (action_mid - note_mid).abs() < 1.0,
            "midpoints should align: action_mid={action_mid:.1}, note_mid={note_mid:.1}"
        );

        // Edge list should NOT include the note.
        assert_eq!(layout.edges.len(), 0);
    }

    // 6. Left note -----------------------------------------------------------

    #[test]
    fn note_left_of_action() {
        let d = diagram(vec![
            ActivityEvent::Action {
                text: "Do work".into(),
            },
            ActivityEvent::Note {
                position: NotePosition::Left,
                text: "Left note".into(),
            },
        ]);
        let layout = layout_activity(&d).unwrap();
        let action = &layout.nodes[0];
        let note = &layout.nodes[1];

        // Note should be to the left.
        assert!(note.x + note.width < action.x);
    }

    // 7. If / EndIf diamonds -------------------------------------------------

    #[test]
    fn if_endif_diamonds() {
        let d = diagram(vec![
            ActivityEvent::If {
                condition: "x > 0".into(),
                then_label: "yes".into(),
            },
            ActivityEvent::Action {
                text: "positive".into(),
            },
            ActivityEvent::EndIf,
        ]);
        let layout = layout_activity(&d).unwrap();
        // New-style if creates an IfDiamond + action (no EndIf diamond node).
        assert_eq!(layout.nodes.len(), 2);

        let if_node = &layout.nodes[0];
        assert!(matches!(
            if_node.kind,
            ActivityNodeKindLayout::IfDiamond { .. }
        ));

        let action = &layout.nodes[1];
        assert!(action.y > if_node.y + if_node.height);

        // Edges: diamond→action + action→(implicit next, but there's no next node here).
        // With deferred if-edges, there should be at least the diamond→action edge.
        assert!(!layout.edges.is_empty());
    }

    // 8. Fork bar ------------------------------------------------------------

    #[test]
    fn fork_bar() {
        let d = diagram(vec![
            ActivityEvent::Fork,
            ActivityEvent::Action {
                text: "branch 1".into(),
            },
            ActivityEvent::ForkAgain,
            ActivityEvent::Action {
                text: "branch 2".into(),
            },
            ActivityEvent::EndFork,
        ]);
        let layout = layout_activity(&d).unwrap();
        // A fork renders exactly two bars: the top bar from `fork` and the
        // bottom bar from `end fork`. `fork again` only separates branches
        // — it does not emit a bar — matching the official PlantUML jar.
        assert_eq!(layout.nodes.len(), 4);

        let fork = &layout.nodes[0];
        let action1 = &layout.nodes[1];
        let action2 = &layout.nodes[2];
        let end_fork = &layout.nodes[3];
        assert_eq!(fork.kind, ActivityNodeKindLayout::ForkBar);
        assert_eq!(end_fork.kind, ActivityNodeKindLayout::ForkBar);
        assert!(matches!(action1.kind, ActivityNodeKindLayout::Action));
        assert!(matches!(action2.kind, ActivityNodeKindLayout::Action));

        assert_eq!(fork.height, FORK_BAR_HEIGHT);
        assert_eq!(end_fork.height, FORK_BAR_HEIGHT);
        // Both bars span the full set of branches, so they share a width.
        assert_eq!(fork.width, end_fork.width);
        // The bottom bar sits below both branch actions.
        assert!(end_fork.y >= action1.y + action1.height);
        assert!(end_fork.y >= action2.y + action2.height);
    }

    // 8b. Fork + swimlane is an unsupported path --------------------------------
    // When swimlanes are present, Pass 2c positions the fork bar from the
    // lane-0 geometry captured at that moment and packs every branch
    // side-by-side relative to it ("fork in swimlanes is not yet supported").
    // A `|Lane|` switch inside a branch is therefore ignored — branch content
    // is NOT routed into the switched lane. A byte-exact fork+swimlane
    // implementation (Java FtileFactory + swimlane composite) is deferred; this
    // test pins the current limited behavior so a future fix is flagged. See PR
    // #77 review item 3 — "lane replay can shift after ForkAgain stops emitting
    // a layout node".
    #[test]
    fn fork_in_swimlanes_is_unsupported() {
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Swimlane { name: "A".into() },
                ActivityEvent::Fork,
                // Branch 0 starts in lane A, then switches to lane B before the
                // branch ends — the lane replay that shifts after ForkAgain.
                ActivityEvent::Action {
                    text: "branch 0".into(),
                },
                ActivityEvent::Swimlane { name: "B".into() },
                ActivityEvent::ForkAgain,
                // Branch 1 inherits the shifted lane (B) rather than the fork's
                // original lane (A) — the symptom of the unsupported path.
                ActivityEvent::Action {
                    text: "branch 1".into(),
                },
                ActivityEvent::EndFork,
            ],
            swimlanes: vec!["A".into(), "B".into()],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };

        // Must not panic / error on the unsupported path.
        let layout = layout_activity(&d).unwrap();

        // Exactly two fork bars (top from `fork`, bottom from `end fork`) and
        // two branch actions.
        let bars: Vec<_> = layout
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, ActivityNodeKindLayout::ForkBar))
            .collect();
        assert_eq!(bars.len(), 2, "fork renders exactly two bars");
        let actions: Vec<_> = layout
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, ActivityNodeKindLayout::Action))
            .collect();
        assert_eq!(actions.len(), 2, "two branch actions");
        // Actions are emitted in branch order: branch 0, then branch 1.
        let branch0_cx = actions[0].x + actions[0].width / 2.0;
        let branch1_cx = actions[1].x + actions[1].width / 2.0;

        // The pin: branch 1 (preceded by a `|B|` switch) is packed beside
        // branch 0 by the fork layout — it is NOT placed in lane B. A correct
        // implementation would route branch 1 into lane B (distance ~0 to lane
        // B, far from branch 0), flipping this inequality and flagging the fix.
        let lane_b_cx = swimlane_center_x(&layout.swimlane_layouts, 1);
        assert!(
            (branch1_cx - branch0_cx).abs() < (branch1_cx - lane_b_cx).abs(),
            "unsupported fork+swimlane: branch 1 ({branch1_cx:.4}) should be packed beside \
             branch 0 ({branch0_cx:.4}), not placed in lane B ({lane_b_cx:.4})"
        );
    }

    // 8b. Switch / Case / EndSwitch -------------------------------------------

    #[test]
    fn switch_case() {
        let d = diagram(vec![
            ActivityEvent::Action {
                text: "Receive webhook".into(),
            },
            ActivityEvent::Switch {
                condition: "event type?".into(),
            },
            ActivityEvent::Case {
                label: "payment.succeeded".into(),
            },
            ActivityEvent::Action {
                text: "Mark invoice paid".into(),
            },
            ActivityEvent::Case {
                label: "invoice.failed".into(),
            },
            ActivityEvent::Action {
                text: "Notify customer".into(),
            },
            ActivityEvent::Case {
                label: "other".into(),
            },
            ActivityEvent::Action {
                text: "Ignore event".into(),
            },
            ActivityEvent::EndSwitch,
        ]);
        let layout = layout_activity(&d).unwrap();

        // 1 action + 1 switch diamond + 3 case actions + 1 switch merge
        // diamond = 6 nodes.
        assert_eq!(layout.nodes.len(), 6);

        // The switch diamond is node index 1 (after the leading action).
        let switch_node = &layout.nodes[1];
        assert!(
            matches!(switch_node.kind, ActivityNodeKindLayout::SwitchDiamond),
            "expected SwitchDiamond, got {:?}",
            switch_node.kind
        );
        assert_eq!(switch_node.text, "event type?");

        // The three case action nodes carry their action text.
        let action_texts: Vec<&str> = layout.nodes[2..5].iter().map(|n| n.text.as_str()).collect();
        assert_eq!(
            action_texts,
            vec!["Mark invoice paid", "Notify customer", "Ignore event"]
        );

        // The last node is the switch merge diamond where cases rejoin.
        assert!(matches!(
            layout.nodes[5].kind,
            ActivityNodeKindLayout::Diamond
        ));

        // Edges should include the 3 diamond→case branch edges (labelled with
        // the case labels).
        let branch_labels: Vec<&str> = layout
            .edges
            .iter()
            .filter(|e| !e.label.is_empty())
            .map(|e| e.label.as_str())
            .collect();
        assert_eq!(
            branch_labels,
            vec!["payment.succeeded", "invoice.failed", "other"],
            "case labels should appear on branch edges"
        );
    }

    // 8b. switch/case with a nested if/else (regression: PR #55 / issue #39) -
    //     an `if` inside a `case` must not double-track action nodes (which
    //     previously collapsed the two if-branches onto the same x and
    //     mis-wired the edges).
    #[test]
    fn switch_case_with_nested_if() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Switch {
                condition: "test?".into(),
            },
            ActivityEvent::Case { label: "A".into() },
            ActivityEvent::If {
                condition: "x?".into(),
                then_label: "yes".into(),
            },
            ActivityEvent::Action {
                text: "A yes".into(),
            },
            ActivityEvent::Else { label: "no".into() },
            ActivityEvent::Action {
                text: "A no".into(),
            },
            ActivityEvent::EndIf,
            ActivityEvent::Case { label: "B".into() },
            ActivityEvent::Action {
                text: "Action B".into(),
            },
            ActivityEvent::EndSwitch,
            ActivityEvent::Action {
                text: "after".into(),
            },
            ActivityEvent::Stop,
        ]);
        let layout = layout_activity(&d).unwrap();

        // Locate the two if-branch action nodes and the switch diamond.
        let a_yes = layout
            .nodes
            .iter()
            .find(|n| n.text == "A yes")
            .expect("A yes node");
        let a_no = layout
            .nodes
            .iter()
            .find(|n| n.text == "A no")
            .expect("A no node");
        let switch = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::SwitchDiamond))
            .expect("switch diamond");
        let if_diamond = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::IfDiamond { .. }) && n.text == "x?")
            .expect("if diamond");

        // The two if-branches must sit side by side at DISTINCT x centres
        // (the bug collapsed them onto the same x).
        let yes_cx = a_yes.x + a_yes.width / 2.0;
        let no_cx = a_no.x + a_no.width / 2.0;
        assert!(
            (yes_cx - no_cx).abs() > 30.0,
            "if-branch nodes must not overlap: yes_cx={yes_cx}, no_cx={no_cx}"
        );

        // The if-diamond sits under case A (left of the switch centre), and
        // Action B sits under case B (right of the switch centre).
        let switch_cx = switch.x + switch.width / 2.0;
        let if_cx = if_diamond.x + if_diamond.width / 2.0;
        assert!(
            if_cx < switch_cx,
            "if-diamond should be in case A (left of switch): if_cx={if_cx}, switch_cx={switch_cx}"
        );
        let action_b = layout
            .nodes
            .iter()
            .find(|n| n.text == "Action B")
            .expect("Action B node");
        let b_cx = action_b.x + action_b.width / 2.0;
        assert!(
            b_cx > switch_cx,
            "Action B should be in case B (right of switch): b_cx={b_cx}, switch_cx={switch_cx}"
        );

        // Exactly ONE labelled edge into the if-diamond (the switch's case-A
        // entry edge). The bug drew a duplicate incoming edge from the if
        // machinery's prev→diamond path; that is now suppressed. (Other
        // inbound edges are unlabelled branch-merge segments ending at the
        // diamond's merge point, not incoming connections.)
        let labelled_inbound = layout
            .edges
            .iter()
            .filter(|e| e.to_index == if_diamond.index && !e.label.is_empty())
            .count();
        assert_eq!(
            labelled_inbound, 1,
            "if-diamond should have exactly one labelled inbound edge (the case entry), got {labelled_inbound}"
        );
        // That edge carries the case-A label.
        assert!(layout
            .edges
            .iter()
            .any(|e| { e.to_index == if_diamond.index && e.label == "A" }));

        // The case labels A and B both appear on switch branch edges.
        let labels: Vec<&str> = layout
            .edges
            .iter()
            .filter(|e| !e.label.is_empty())
            .map(|e| e.label.as_str())
            .collect();
        assert!(labels.contains(&"A"), "missing case A label edge");
        assert!(labels.contains(&"B"), "missing case B label edge");
    }

    // 8c. switch/case with a nested while loop (parallel to the nested-if case).
    #[test]
    fn switch_case_with_nested_while() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Switch {
                condition: "test?".into(),
            },
            ActivityEvent::Case { label: "A".into() },
            ActivityEvent::While {
                condition: "x?".into(),
                label: "yes".into(),
            },
            ActivityEvent::Action {
                text: "A body".into(),
            },
            ActivityEvent::EndWhile { label: "no".into() },
            ActivityEvent::Case { label: "B".into() },
            ActivityEvent::Action {
                text: "Action B".into(),
            },
            ActivityEvent::EndSwitch,
            ActivityEvent::Action {
                text: "after".into(),
            },
            ActivityEvent::Stop,
        ]);
        let layout = layout_activity(&d).unwrap();

        let switch = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::SwitchDiamond))
            .expect("switch diamond");
        let while_diamond = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::IfDiamond { .. }) && n.text == "x?")
            .expect("while diamond");
        let body = layout
            .nodes
            .iter()
            .find(|n| n.text == "A body")
            .expect("A body node");
        let action_b = layout
            .nodes
            .iter()
            .find(|n| n.text == "Action B")
            .expect("Action B node");

        let switch_cx = switch.x + switch.width / 2.0;
        let while_cx = while_diamond.x + while_diamond.width / 2.0;
        let body_cx = body.x + body.width / 2.0;
        let b_cx = action_b.x + action_b.width / 2.0;

        // while-diamond and its body sit under case A (left); Action B under
        // case B (right).
        assert!(
            while_cx < switch_cx && b_cx > switch_cx,
            "while should be in case A, Action B in case B: while_cx={while_cx}, switch_cx={switch_cx}, b_cx={b_cx}"
        );
        // The while body shares the while-diamond's column (centred under it).
        assert!(
            (body_cx - while_cx).abs() < 1.0,
            "while body should be centred under the while diamond: body_cx={body_cx}, while_cx={while_cx}"
        );
        // Exactly one labelled inbound edge to the while-diamond (the case-A
        // entry). The while loop-back is also inbound but unlabelled.
        let labelled_inbound = layout
            .edges
            .iter()
            .filter(|e| e.to_index == while_diamond.index && !e.label.is_empty())
            .count();
        assert_eq!(
            labelled_inbound, 1,
            "while-diamond should have exactly one labelled inbound edge (the case entry), got {labelled_inbound}"
        );
    }

    // 8d. switch with a single case — degenerate but must not panic.
    #[test]
    fn switch_single_case() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Switch {
                condition: "x?".into(),
            },
            ActivityEvent::Case { label: "A".into() },
            ActivityEvent::Action {
                text: "do A".into(),
            },
            ActivityEvent::EndSwitch,
            ActivityEvent::Stop,
        ]);
        let layout = layout_activity(&d).unwrap();
        // switch diamond + 1 case action + merge diamond = 3 nodes (plus start/stop).
        assert!(layout.nodes.len() >= 5);
        let switch = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::SwitchDiamond))
            .expect("switch diamond");
        let action = layout
            .nodes
            .iter()
            .find(|n| n.text == "do A")
            .expect("case action");
        // The case action should be below the switch diamond.
        assert!(
            action.y > switch.y,
            "case action should be below switch diamond"
        );
        // Case label "A" should appear on an edge.
        assert!(
            layout.edges.iter().any(|e| e.label == "A"),
            "case A label missing"
        );
    }

    // 8e. switch with an empty case (no action) — must not panic, case label visible.
    #[test]
    fn switch_empty_case() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Switch {
                condition: "x?".into(),
            },
            ActivityEvent::Case { label: "A".into() },
            ActivityEvent::Action {
                text: "do A".into(),
            },
            ActivityEvent::Case { label: "B".into() },
            // Empty case B — no action.
            ActivityEvent::EndSwitch,
            ActivityEvent::Stop,
        ]);
        let layout = layout_activity(&d).unwrap();
        // Both case labels should appear on edges.
        let labels: Vec<&str> = layout
            .edges
            .iter()
            .filter(|e| !e.label.is_empty())
            .map(|e| e.label.as_str())
            .collect();
        assert!(labels.contains(&"A"), "case A label missing");
        assert!(labels.contains(&"B"), "case B label missing (empty case)");
    }

    // 8f. switch as the last block (no following flow node) — merge diamond
    // connects to nothing; must not panic.
    #[test]
    fn switch_no_following_node() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Switch {
                condition: "x?".into(),
            },
            ActivityEvent::Case { label: "A".into() },
            ActivityEvent::Action {
                text: "do A".into(),
            },
            ActivityEvent::Case { label: "B".into() },
            ActivityEvent::Action {
                text: "do B".into(),
            },
            ActivityEvent::EndSwitch,
            // No action/stop after endswitch.
        ]);
        let layout = layout_activity(&d).unwrap();
        // Should produce a valid layout with switch + cases + merge diamond.
        assert!(layout.nodes.len() >= 4);
        assert!(layout.edges.iter().any(|e| e.label == "A"));
        assert!(layout.edges.iter().any(|e| e.label == "B"));
        // Even with no following flow node, both cases must route into the
        // switch's merge diamond — the terminal merge still collects them, so
        // cases do not dangle. (Only the merge→next edge is absent, which is
        // correct for a terminal switch.)
        let switch = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::SwitchDiamond))
            .expect("switch diamond");
        let merge = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::Diamond))
            .expect("merge diamond");
        let merge_edges = layout
            .edges
            .iter()
            .filter(|e| e.from_index == switch.index && e.to_index == merge.index)
            .count();
        assert!(
            merge_edges >= 2,
            "both cases should route into the merge diamond, got {merge_edges}"
        );
    }

    // 8g. switch with 4 cases — middle cases (indices 1, 2) enter from
    // diamond bottom, outer (0, 3) from sides.
    #[test]
    fn switch_four_cases() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Switch {
                condition: "x?".into(),
            },
            ActivityEvent::Case { label: "A".into() },
            ActivityEvent::Action { text: "a".into() },
            ActivityEvent::Case { label: "B".into() },
            ActivityEvent::Action { text: "b".into() },
            ActivityEvent::Case { label: "C".into() },
            ActivityEvent::Action { text: "c".into() },
            ActivityEvent::Case { label: "D".into() },
            ActivityEvent::Action { text: "d".into() },
            ActivityEvent::EndSwitch,
            ActivityEvent::Stop,
        ]);
        let layout = layout_activity(&d).unwrap();
        // 4 case labels.
        let labels: Vec<&str> = layout
            .edges
            .iter()
            .filter(|e| !e.label.is_empty())
            .map(|e| e.label.as_str())
            .collect();
        assert!(labels.contains(&"A") && labels.contains(&"B"));
        assert!(labels.contains(&"C") && labels.contains(&"D"));
        // The first case should be left of the switch, last case right.
        let switch = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::SwitchDiamond))
            .expect("switch diamond");
        let switch_cx = switch.x + switch.width / 2.0;
        let case_a = layout.nodes.iter().find(|n| n.text == "a").expect("case A");
        let case_d = layout.nodes.iter().find(|n| n.text == "d").expect("case D");
        let a_cx = case_a.x + case_a.width / 2.0;
        let d_cx = case_d.x + case_d.width / 2.0;
        assert!(a_cx < switch_cx, "case A should be left of switch");
        assert!(d_cx > switch_cx, "case D should be right of switch");
    }

    // 8h. switch with multiple actions per case (intra-case edges).
    #[test]
    fn switch_multi_action_case() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Switch {
                condition: "x?".into(),
            },
            ActivityEvent::Case { label: "A".into() },
            ActivityEvent::Action {
                text: "step 1".into(),
            },
            ActivityEvent::Action {
                text: "step 2".into(),
            },
            ActivityEvent::Case { label: "B".into() },
            ActivityEvent::Action {
                text: "step 3".into(),
            },
            ActivityEvent::EndSwitch,
            ActivityEvent::Stop,
        ]);
        let layout = layout_activity(&d).unwrap();
        // Intra-case edge: step 1 → step 2 (within case A).
        let step1 = layout
            .nodes
            .iter()
            .find(|n| n.text == "step 1")
            .expect("step 1");
        let step2 = layout
            .nodes
            .iter()
            .find(|n| n.text == "step 2")
            .expect("step 2");
        let intra = layout
            .edges
            .iter()
            .find(|e| e.from_index == step1.index && e.to_index == step2.index);
        assert!(intra.is_some(), "intra-case edge step1→step2 missing");
        // step 2 should be below step 1.
        assert!(step2.y > step1.y, "step 2 should be below step 1");
    }

    // 8i. switch with no-else if nested in case (the PR #55 regression test).
    #[test]
    fn switch_case_with_noelse_if() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Switch {
                condition: "t?".into(),
            },
            ActivityEvent::Case { label: "A".into() },
            ActivityEvent::If {
                condition: "x?".into(),
                then_label: "y".into(),
            },
            ActivityEvent::Action {
                text: "A yes".into(),
            },
            ActivityEvent::EndIf,
            ActivityEvent::Case { label: "B".into() },
            ActivityEvent::Action {
                text: "Action B".into(),
            },
            ActivityEvent::EndSwitch,
            ActivityEvent::Stop,
        ]);
        let layout = layout_activity(&d).unwrap();
        // if-diamond and A yes must not overlap (the original bug).
        let a_yes = layout
            .nodes
            .iter()
            .find(|n| n.text == "A yes")
            .expect("A yes");
        let if_diamond = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::IfDiamond { .. }) && n.text == "x?")
            .expect("if diamond");
        let if_cx = if_diamond.x + if_diamond.width / 2.0;
        let yes_cx = a_yes.x + a_yes.width / 2.0;
        // In no-else, A yes is centred under the if-diamond.
        assert!(
            (yes_cx - if_cx).abs() < 5.0,
            "no-else then-branch should be centred under if-diamond: yes_cx={yes_cx}, if_cx={if_cx}"
        );
        // Exactly one labelled inbound edge to the if-diamond (case-A entry).
        let labelled_inbound = layout
            .edges
            .iter()
            .filter(|e| e.to_index == if_diamond.index && !e.label.is_empty())
            .count();
        assert_eq!(
            labelled_inbound, 1,
            "if-diamond should have one labelled inbound (case entry)"
        );
    }

    // 8j. switch with has-else if nested in case (PR #55 original test).
    #[test]
    fn switch_case_with_has_else_if() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Switch {
                condition: "test?".into(),
            },
            ActivityEvent::Case { label: "A".into() },
            ActivityEvent::If {
                condition: "x?".into(),
                then_label: "yes".into(),
            },
            ActivityEvent::Action {
                text: "A yes".into(),
            },
            ActivityEvent::Else { label: "no".into() },
            ActivityEvent::Action {
                text: "A no".into(),
            },
            ActivityEvent::EndIf,
            ActivityEvent::Case { label: "B".into() },
            ActivityEvent::Action {
                text: "Action B".into(),
            },
            ActivityEvent::EndSwitch,
            ActivityEvent::Action {
                text: "after".into(),
            },
            ActivityEvent::Stop,
        ]);
        let layout = layout_activity(&d).unwrap();
        // Branch nodes must not overlap (the original bug).
        let a_yes = layout
            .nodes
            .iter()
            .find(|n| n.text == "A yes")
            .expect("A yes");
        let a_no = layout
            .nodes
            .iter()
            .find(|n| n.text == "A no")
            .expect("A no");
        let yes_cx = a_yes.x + a_yes.width / 2.0;
        let no_cx = a_no.x + a_no.width / 2.0;
        assert!(
            (yes_cx - no_cx).abs() > 30.0,
            "if-branch nodes must not overlap: yes_cx={yes_cx}, no_cx={no_cx}"
        );
        // if-diamond sits under case A (left of switch), Action B under case B (right).
        let switch = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::SwitchDiamond))
            .expect("switch diamond");
        let switch_cx = switch.x + switch.width / 2.0;
        let if_diamond = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::IfDiamond { .. }) && n.text == "x?")
            .expect("if diamond");
        let if_cx = if_diamond.x + if_diamond.width / 2.0;
        assert!(
            if_cx < switch_cx,
            "if-diamond should be in case A (left of switch)"
        );
    }

    // 8k. FtileSwitchNude SMALL_DIAMOND mode: 3 equal cases should be
    // symmetrically spread around the switch diamond, with xSeparation=20.
    #[test]
    fn switch_small_diamond_mode_symmetric() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Switch {
                condition: "x?".into(),
            },
            ActivityEvent::Case { label: "A".into() },
            ActivityEvent::Action {
                text: "do A".into(),
            },
            ActivityEvent::Case { label: "B".into() },
            ActivityEvent::Action {
                text: "do B".into(),
            },
            ActivityEvent::Case { label: "C".into() },
            ActivityEvent::Action {
                text: "do C".into(),
            },
            ActivityEvent::EndSwitch,
            ActivityEvent::Stop,
        ]);
        let layout = layout_activity(&d).unwrap();
        let switch = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::SwitchDiamond))
            .expect("switch diamond");
        let switch_cx = switch.x + switch.width / 2.0;
        let case_a = layout
            .nodes
            .iter()
            .find(|n| n.text == "do A")
            .expect("case A");
        let case_c = layout
            .nodes
            .iter()
            .find(|n| n.text == "do C")
            .expect("case C");
        let a_cx = case_a.x + case_a.width / 2.0;
        let c_cx = case_c.x + case_c.width / 2.0;
        // Cases should be roughly symmetric around the switch.
        let a_dist = switch_cx - a_cx;
        let c_dist = c_cx - switch_cx;
        assert!(
            (a_dist - c_dist).abs() < 5.0,
            "cases should be roughly symmetric: A dist={a_dist}, C dist={c_dist}"
        );
    }

    // 8l. case_tile_geometry via layout: action case width should match the
    // action rect width (no inflation), verifying FtileMinWidthCentered(30)
    // doesn't widen cases that are already > 30px.
    #[test]
    fn switch_action_tile_not_inflated() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Switch {
                condition: "x?".into(),
            },
            ActivityEvent::Case { label: "A".into() },
            ActivityEvent::Action {
                text: "do something long enough".into(),
            },
            ActivityEvent::Case { label: "B".into() },
            ActivityEvent::Action {
                text: "also long enough action".into(),
            },
            ActivityEvent::EndSwitch,
            ActivityEvent::Stop,
        ]);
        let layout = layout_activity(&d).unwrap();
        // Both cases should have their action rect width as the tile width
        // (no min-width inflation since actions are > 30px).
        let case_a = layout
            .nodes
            .iter()
            .find(|n| n.text == "do something long enough")
            .expect("case A");
        assert!(case_a.width > 30.0, "action should be > 30px wide");
    }

    // 9. Text sizing ---------------------------------------------------------

    #[test]
    fn text_sizing() {
        // Single short line.
        let (w, h) = estimate_text_size("Hi");
        assert!(w >= 20.0);
        assert!(h >= 20.0);

        // Multi-line text.
        let (w2, h2) = estimate_text_size("Line one\nLine two\nLine three");
        assert!(h2 > h, "more lines should be taller");
        // Width driven by longest line.
        assert!(
            w2 >= crate::font_metrics::text_width(
                "Line three",
                "SansSerif",
                FONT_SIZE,
                false,
                false
            )
        ); // "Line three" = 10 chars

        // Very long line.
        let long_text = "A".repeat(100);
        let (w3, _) = estimate_text_size(&long_text);
        assert!(w3 > 30.0);
    }

    // 10. While loop diamond --------------------------------------------------

    #[test]
    fn while_loop() {
        let d = diagram(vec![
            ActivityEvent::While {
                condition: "count < 10".into(),
                label: "yes".into(),
            },
            ActivityEvent::Action {
                text: "increment".into(),
            },
            ActivityEvent::EndWhile {
                label: "done".into(),
            },
        ]);
        let layout = layout_activity(&d).unwrap();
        // EndWhile no longer creates a node — the while diamond is a two-way
        // branch (down into the body, left out to the next node), matching
        // official PlantUML.  Only the while diamond + body action remain.
        assert_eq!(layout.nodes.len(), 2);

        let while_node = &layout.nodes[0];
        assert!(matches!(
            while_node.kind,
            ActivityNodeKindLayout::IfDiamond { .. }
        ));
        assert!(while_node.text.contains("count < 10"));
    }

    // 11. Detach marker -------------------------------------------------------

    #[test]
    fn detach_marker() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Action {
                text: "work".into(),
            },
            ActivityEvent::Detach,
        ]);
        let layout = layout_activity(&d).unwrap();
        assert_eq!(layout.nodes.len(), 3);
        assert_eq!(layout.nodes[2].kind, ActivityNodeKindLayout::Detach);
        // Detach participates in edges.
        assert_eq!(layout.edges.len(), 2);
    }

    // 12. Floating note does NOT advance y_cursor (Java compat) ---------------

    #[test]
    fn floating_note_does_not_advance_y() {
        // Java: floating notes sit beside the flow without consuming vertical
        // space, just like attached notes.  The next flow node should be at
        // the same y_cursor, not pushed below the floating note.
        let d = diagram(vec![
            ActivityEvent::Action {
                text: "work".into(),
            },
            ActivityEvent::FloatingNote {
                position: NotePosition::Left,
                text: "floating".into(),
            },
            ActivityEvent::Action {
                text: "after note".into(),
            },
        ]);
        let layout = layout_activity(&d).unwrap();
        assert_eq!(layout.nodes.len(), 3);
        let action1 = &layout.nodes[0];
        let note = &layout.nodes[1];
        let action2 = &layout.nodes[2];
        // Floating note should be placed at the same y as action1's bottom + spacing,
        // but should NOT push action2 further down.
        let expected_action2_y = action1.y + action1.height + NODE_SPACING;
        assert!(
            (action2.y - expected_action2_y).abs() < 1.0,
            "action2.y ({:.1}) should be at {:.1} (action1 bottom + spacing), \
             floating note should not push it down. note.y={:.1} note.h={:.1}",
            action2.y,
            expected_action2_y,
            note.y,
            note.height
        );
    }

    #[test]
    fn floating_note_is_attached_to_previous_flow_node() {
        let d = diagram(vec![
            ActivityEvent::Action {
                text: "work".into(),
            },
            ActivityEvent::FloatingNote {
                position: NotePosition::Left,
                text: "floating".into(),
            },
        ]);
        let layout = layout_activity(&d).unwrap();
        assert_eq!(layout.nodes.len(), 2);
        let action = &layout.nodes[0];
        let note = &layout.nodes[1];
        assert!(
            (note.x - (action.x - NOTE_OFFSET - note.width)).abs() < 0.01,
            "floating note.x ({:.4}) should sit {}px left of the previous flow node ({:.4})",
            note.x,
            NOTE_OFFSET,
            action.x
        );
        assert!(
            (note.y - (action.y + (action.height - note.height) / 2.0)).abs() < 0.01,
            "floating note.y ({:.4}) should be vertically centered on action.y ({:.4})",
            note.y,
            action.y
        );
    }

    #[test]
    fn stop_keeps_previous_flow_column_after_single_note_group() {
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Swimlane {
                    name: "Lane".into(),
                },
                ActivityEvent::Action {
                    text: "work".into(),
                },
                ActivityEvent::Note {
                    position: NotePosition::Right,
                    text: "single note".into(),
                },
                ActivityEvent::Stop,
            ],
            swimlanes: vec!["Lane".into()],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();
        let action = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::Action))
            .unwrap();
        let stop = layout
            .nodes
            .iter()
            .find(|n| matches!(n.kind, ActivityNodeKindLayout::Stop))
            .unwrap();
        let action_cx = action.x + action.width / 2.0;
        let stop_cx = stop.x + stop.width / 2.0;
        assert!(
            (action_cx - stop_cx).abs() < 0.01,
            "stop center ({stop_cx:.4}) should follow previous flow column ({action_cx:.4})"
        );
    }

    // 13. Note without preceding flow node -----------------------------------

    #[test]
    fn note_without_preceding_node() {
        let d = diagram(vec![ActivityEvent::Note {
            position: NotePosition::Right,
            text: "orphan note".into(),
        }]);
        let layout = layout_activity(&d).unwrap();
        assert_eq!(layout.nodes.len(), 1);
        // Should not panic.
        assert_eq!(layout.edges.len(), 0);
    }

    // 14. Edges skip notes ---------------------------------------------------

    #[test]
    fn edges_skip_notes() {
        let d = diagram(vec![
            ActivityEvent::Start,
            ActivityEvent::Action { text: "A".into() },
            ActivityEvent::Note {
                position: NotePosition::Right,
                text: "note on A".into(),
            },
            ActivityEvent::Action { text: "B".into() },
            ActivityEvent::Stop,
        ]);
        let layout = layout_activity(&d).unwrap();
        // 5 nodes: start, A, note, B, stop
        assert_eq!(layout.nodes.len(), 5);
        // 4 flow nodes: start, A, B, stop → 3 edges
        assert_eq!(layout.edges.len(), 3);
        // Edge from A (index 1) to B (index 3) — skipping note (index 2).
        let edge_a_b = &layout.edges[1];
        assert_eq!(edge_a_b.from_index, 1);
        assert_eq!(edge_a_b.to_index, 3);
    }

    #[test]
    fn cross_lane_edge_routes_above_target_note_group() {
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Swimlane { name: "A".into() },
                ActivityEvent::Action { text: "A1".into() },
                ActivityEvent::Swimlane { name: "B".into() },
                ActivityEvent::Action { text: "B1".into() },
                ActivityEvent::Note {
                    position: NotePosition::Right,
                    text: "line1\nline2\nline3\nline4".into(),
                },
            ],
            swimlanes: vec!["A".into(), "B".into()],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();
        let cross = layout
            .edges
            .iter()
            .find(|edge| edge.from_index == 0 && edge.to_index == 1)
            .unwrap();
        let note = layout
            .nodes
            .iter()
            .find(|node| matches!(node.kind, ActivityNodeKindLayout::Note { .. }))
            .unwrap();
        let target = layout
            .nodes
            .iter()
            .find(|node| matches!(node.kind, ActivityNodeKindLayout::Action) && node.text == "B1")
            .unwrap();
        assert_eq!(cross.points.len(), 4);
        assert!(
            note.y < target.y,
            "test fixture must make the note protrude above the target action"
        );
        assert!(
            (cross.points[1].1 - (note.y - CROSS_LANE_VERTICAL_STUB)).abs() < 0.01,
            "cross-lane horizontal level ({:.4}) should route above the target note group ({:.4})",
            cross.points[1].1,
            note.y - CROSS_LANE_VERTICAL_STUB
        );
    }

    // 15. Else / ElseIf nodes ------------------------------------------------

    #[test]
    fn else_elseif_nodes() {
        let d = diagram(vec![
            ActivityEvent::If {
                condition: "a".into(),
                then_label: "yes".into(),
            },
            ActivityEvent::Action {
                text: "do a".into(),
            },
            ActivityEvent::ElseIf {
                condition: "b".into(),
                label: "maybe".into(),
            },
            ActivityEvent::Action {
                text: "do b".into(),
            },
            ActivityEvent::Else { label: "no".into() },
            ActivityEvent::Action {
                text: "do c".into(),
            },
            ActivityEvent::EndIf,
        ]);
        let layout = layout_activity(&d).unwrap();
        // IfDiamond + "do a" in then-branch, then ElseIf (Diamond), "do b",
        // Else switches to else-branch, "do c", EndIf.
        // The exact node count depends on the if/elseif/else handling.
        assert!(
            layout.nodes.len() >= 4,
            "expected at least 4 nodes, got {}",
            layout.nodes.len()
        );
    }

    // 16. Repeat / RepeatWhile -----------------------------------------------

    #[test]
    fn repeat_loop() {
        let d = diagram(vec![
            ActivityEvent::Repeat,
            ActivityEvent::Action {
                text: "step".into(),
            },
            ActivityEvent::RepeatWhile {
                condition: "again?".into(),
                is_text: None,
                not_text: None,
            },
        ]);
        let layout = layout_activity(&d).unwrap();
        assert_eq!(layout.nodes.len(), 3);
        // `repeat` start is a small square diamond (Java FtileDiamond).
        assert_eq!(layout.nodes[0].kind, ActivityNodeKindLayout::Diamond);
        // `repeat while` end is a hexagonal diamond (Java FtileDiamondInside)
        // with no East label since `is (...)` is absent.
        assert!(matches!(
            &layout.nodes[2].kind,
            ActivityNodeKindLayout::Hexagon { east_lines, .. } if east_lines.is_empty()
        ));
        assert!(layout.nodes[2].text.contains("again?"));
    }

    #[test]
    fn repeat_while_with_is_label() {
        let d = diagram(vec![
            ActivityEvent::Repeat,
            ActivityEvent::Action { text: "do".into() },
            ActivityEvent::RepeatWhile {
                condition: "x".into(),
                is_text: Some("a\\nb\\nc\\n".into()),
                not_text: None,
            },
        ]);
        let layout = layout_activity(&d).unwrap();
        assert_eq!(layout.nodes.len(), 3);
        let hex_node = &layout.nodes[2];
        match &hex_node.kind {
            ActivityNodeKindLayout::Hexagon { east_lines, .. } => {
                // Trailing `\n` produces an empty trailing line, matching
                // Java's `Display.create()` behaviour.
                assert_eq!(
                    east_lines,
                    &vec![
                        "a".to_string(),
                        "b".to_string(),
                        "c".to_string(),
                        "".to_string(),
                    ]
                );
            }
            other => panic!("expected Hexagon kind, got {other:?}"),
        }
        assert_eq!(hex_node.text, "x");
        // Hexagon width = label_w + 24, height = 24 (label fits in min height).
        assert!(hex_node.width > 24.0);
        assert_eq!(hex_node.height, 24.0);
    }

    // 17. LeftToRight direction: width > height (wider than tall) ----------

    #[test]
    fn left_to_right_direction() {
        use crate::model::diagram::Direction;
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Start,
                ActivityEvent::Action {
                    text: "Step 1".into(),
                },
                ActivityEvent::Action {
                    text: "Step 2".into(),
                },
                ActivityEvent::Stop,
            ],
            swimlanes: vec![],
            direction: Direction::LeftToRight,
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();

        // With LR direction, the diagram should be wider than tall
        assert!(
            layout.width > layout.height,
            "LR: width ({:.1}) should be > height ({:.1})",
            layout.width,
            layout.height
        );

        // Nodes should flow left-to-right: x positions should increase
        let flow_nodes: Vec<&ActivityNodeLayout> = layout
            .nodes
            .iter()
            .filter(|n| is_flow_node(&n.kind))
            .collect();
        for pair in flow_nodes.windows(2) {
            assert!(
                pair[1].x >= pair[0].x,
                "LR: node {} x ({:.1}) should be >= node {} x ({:.1})",
                pair[1].index,
                pair[1].x,
                pair[0].index,
                pair[0].x
            );
        }
    }

    // 18. TB direction: height > width (taller than wide) -----------------

    #[test]
    fn top_to_bottom_direction() {
        use crate::model::diagram::Direction;
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Start,
                ActivityEvent::Action {
                    text: "Step 1".into(),
                },
                ActivityEvent::Action {
                    text: "Step 2".into(),
                },
                ActivityEvent::Stop,
            ],
            swimlanes: vec![],
            direction: Direction::TopToBottom,
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();

        // With TB direction, the diagram should be taller than wide
        assert!(
            layout.height > layout.width,
            "TB: height ({:.1}) should be > width ({:.1})",
            layout.height,
            layout.width
        );

        // Nodes should flow top-to-bottom: y positions should increase
        let flow_nodes: Vec<&ActivityNodeLayout> = layout
            .nodes
            .iter()
            .filter(|n| is_flow_node(&n.kind))
            .collect();
        for pair in flow_nodes.windows(2) {
            assert!(
                pair[1].y >= pair[0].y,
                "TB: node {} y ({:.1}) should be >= node {} y ({:.1})",
                pair[1].index,
                pair[1].y,
                pair[0].index,
                pair[0].y
            );
        }
    }

    // 19. BottomToTop direction: first node is at the bottom ---------------

    #[test]
    fn bottom_to_top_direction() {
        use crate::model::diagram::Direction;
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Start,
                ActivityEvent::Action {
                    text: "Step 1".into(),
                },
                ActivityEvent::Stop,
            ],
            swimlanes: vec![],
            direction: Direction::BottomToTop,
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();

        // Start should be below Stop in BT direction
        let start = &layout.nodes[0];
        let stop = &layout.nodes[2];
        assert!(
            start.y > stop.y,
            "BT: start y ({:.1}) should be > stop y ({:.1})",
            start.y,
            stop.y
        );
    }

    // 19. Swimlane header offset -------------------------------------------

    #[test]
    fn swimlane_nodes_start_below_header() {
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Swimlane {
                    name: "Lane A".into(),
                },
                ActivityEvent::Start,
                ActivityEvent::Action {
                    text: "Task".into(),
                },
                ActivityEvent::Stop,
            ],
            swimlanes: vec!["Lane A".into()],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();
        // All flow nodes should start below the swimlane header
        for node in &layout.nodes {
            assert!(
                node.y >= 20.0,
                "node at y={:.1} must be below header area",
                node.y,
            );
        }
    }

    // 20. Cross-lane edges are L-shaped ------------------------------------

    #[test]
    fn cross_lane_edges_are_polyline() {
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Swimlane {
                    name: "Lane A".into(),
                },
                ActivityEvent::Action {
                    text: "In A".into(),
                },
                ActivityEvent::Swimlane {
                    name: "Lane B".into(),
                },
                ActivityEvent::Action {
                    text: "In B".into(),
                },
            ],
            swimlanes: vec!["Lane A".into(), "Lane B".into()],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();

        // Should have 1 edge between the two actions
        assert_eq!(layout.edges.len(), 1);

        let edge = &layout.edges[0];
        // Cross-lane edge must have 4 points (L-shaped route)
        assert_eq!(
            edge.points.len(),
            4,
            "cross-lane edge should have 4 points, got {}",
            edge.points.len()
        );

        // Verify L-shape: first two points share X, middle two share Y, last two share X
        let (x0, _y0) = edge.points[0];
        let (x1, y1) = edge.points[1];
        let (_x2, y2) = edge.points[2];
        let (x3, _y3) = edge.points[3];
        assert!((x0 - x1).abs() < 0.01, "first segment should be vertical");
        assert!(
            (y1 - y2).abs() < 0.01,
            "middle segment should be horizontal"
        );
        assert!(
            ((y1 - edge.points[0].1) - CROSS_LANE_VERTICAL_STUB).abs() < 0.01,
            "cross-lane edge should use a {CROSS_LANE_VERTICAL_STUB}px source stub"
        );
        // x3 should be the target lane center (different from x0)
        assert!(
            (x0 - x3).abs() > 1.0,
            "start and end X should differ for cross-lane"
        );
    }

    // 21. Same-lane edges remain 2-point -----------------------------------

    #[test]
    fn same_lane_edges_are_straight() {
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Swimlane {
                    name: "Lane A".into(),
                },
                ActivityEvent::Start,
                ActivityEvent::Action {
                    text: "Task".into(),
                },
                ActivityEvent::Stop,
            ],
            swimlanes: vec!["Lane A".into()],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();

        // All edges are within same lane, so each should be 2-point
        for (i, edge) in layout.edges.iter().enumerate() {
            assert_eq!(
                edge.points.len(),
                2,
                "same-lane edge {} should have 2 points, got {}",
                i,
                edge.points.len()
            );
        }
    }

    #[test]
    fn estimate_note_size_strips_creole() {
        // <b>HTML</b> should measure based on the TEXT "HTML", not include literal tag chars.
        // Bold text is slightly wider than plain text due to font weight,
        // but must be much narrower than if the tags were included literally.
        let (w_markup, _) = estimate_note_size("contain <b>HTML</b>");
        let (w_literal, _) = estimate_note_size("contain <b>HTML</b>EXTRA");
        assert!(
            w_markup < w_literal,
            "creole markup should be stripped: markup_w={w_markup} should be less than literal_w={w_literal}"
        );
    }

    #[test]
    fn wrap_note_text_basic() {
        // With a small max_width, long text should be wrapped into multiple lines
        let text = "A Long Long Long Long Long Long note";
        let wrapped = wrap_note_text(text, 80.0);
        let line_count = wrapped.split('\n').count();
        assert!(
            line_count > 1,
            "should wrap into multiple lines, got {line_count}: {wrapped:?}"
        );
    }

    #[test]
    fn wrap_note_text_short_line_unchanged() {
        let text = "Short";
        let wrapped = wrap_note_text(text, 200.0);
        assert_eq!(wrapped, text);
    }

    #[test]
    fn wrap_note_text_preserves_existing_newlines() {
        let text = "Line one\nLine two";
        let wrapped = wrap_note_text(text, 200.0);
        assert_eq!(wrapped, text, "existing newlines should be preserved");
    }

    #[test]
    fn wrap_note_text_with_creole_markup() {
        // Creole markup should be preserved in output but not counted for width
        let text = "This has //italic// and <b>bold</b> words here";
        let wrapped = wrap_note_text(text, 100.0);
        // Should contain the original markup
        assert!(wrapped.contains("//italic//"));
        assert!(wrapped.contains("<b>bold</b>"));
    }

    #[test]
    fn note_font_metrics_match_java() {
        // Java a0002: line dy = 15.1328, top_margin = 17.0669, bottom_margin = 8.066
        // ascent_offset = 7.0669, descent_pad = 8.066
        let lh = font_metrics::line_height("SansSerif", NOTE_FONT_SIZE, false, false);
        let asc = font_metrics::ascent("SansSerif", NOTE_FONT_SIZE, false, false);
        let desc = font_metrics::descent("SansSerif", NOTE_FONT_SIZE, false, false);
        println!(
            "note lh={lh:.4}, asc={asc:.4}, desc={desc:.4}, asc+desc={:.4}",
            asc + desc
        );
        // line_height should be ≈ 15.13
        assert!(
            (lh - 15.13).abs() < 0.5,
            "line_height({lh:.4}) should be ≈ 15.13"
        );
    }

    #[test]
    fn estimate_note_separator_adds_less_height_than_text_line() {
        // Adding a separator (====) should increase height by 10px,
        // while adding a text line increases by ~15.13px (line_height).
        let (_, h_base) = estimate_note_size("line1\nline2");
        let (_, h_with_sep) = estimate_note_size("line1\n====\nline2");
        let (_, h_with_text) = estimate_note_size("line1\nline2\nline3");
        let sep_delta = h_with_sep - h_base;
        let text_delta = h_with_text - h_base;
        assert!(
            sep_delta < text_delta,
            "separator delta ({sep_delta:.1}) should be < text delta ({text_delta:.1})"
        );
        assert!(
            (sep_delta - 10.0).abs() < 1.0,
            "separator delta ({sep_delta:.1}) should be ≈ 10.0"
        );
    }

    #[test]
    fn estimate_note_size_one_line_matches_java_opale_height() {
        let (_, h) = estimate_note_size("This is a note");
        let expected = font_metrics::line_height("SansSerif", NOTE_FONT_SIZE, false, false)
            + 2.0 * NOTE_MARGIN_Y;
        assert!(
            (h - expected).abs() < 0.0001,
            "one-line note height should be text height + 2*marginY: {h:.4} vs {expected:.4}"
        );
    }

    #[test]
    fn estimate_note_size_monospace_uses_correct_font() {
        // Monospace text `""foo()""` should be measured with monospace metrics
        let (w_mono, _) = estimate_note_size(r#"method ""foo()"" is"#);
        let (w_plain, _) = estimate_note_size("method foo() is");
        // Monospace "foo()" is wider per-char than SansSerif, so the line
        // with monospace should be at least as wide (often wider).
        assert!(
            (w_mono - w_plain).abs() > 0.5 || w_mono >= w_plain,
            "monospace should affect width: mono={w_mono}, plain={w_plain}"
        );
    }

    #[test]
    fn wrap_note_text_bullet_list_uses_reduced_width() {
        // Java reference data (from CreoleNoteMetricsTest):
        //   bullet at MaxWidth=100: "Calling the" / "method" / "foo() is" / "prohibited" / "overlap" = 5 lines
        //   plain  at MaxWidth=100: "Calling the" / "method foo()" / "is prohibited" / "overlap" = 4 lines
        let bullet = r#"* Calling the method ""foo()"" is prohibited overlap"#;
        let plain = r#"Calling the method ""foo()"" is prohibited overlap"#;
        let wrapped_bullet = wrap_note_text(bullet, 100.0);
        let wrapped_plain = wrap_note_text(plain, 100.0);
        let bullet_lines: Vec<&str> = wrapped_bullet.split('\n').collect();
        let plain_lines: Vec<&str> = wrapped_plain.split('\n').collect();
        // Bullet should produce MORE lines than plain due to indent
        assert!(
            bullet_lines.len() > plain_lines.len(),
            "bullet ({}) should produce more lines than plain ({}).\n  bullet: {bullet_lines:?}\n  plain:  {plain_lines:?}",
            bullet_lines.len(), plain_lines.len()
        );
        // First line should retain the `* ` prefix
        assert!(
            wrapped_bullet.starts_with("* "),
            "first line should start with '* ': {wrapped_bullet:?}"
        );
        // Continuation lines should NOT have `* ` prefix
        for cl in bullet_lines.iter().skip(1) {
            assert!(
                !cl.starts_with("* "),
                "continuation line should not start with '* ': {cl:?}"
            );
        }
    }

    #[test]
    fn wrap_note_text_carries_unclosed_back_highlight_to_continuation_lines() {
        let wrapped = wrap_note_text(
            r#"* Calling the method is <back:red>prohibited overlap"#,
            100.0,
        );
        let lines: Vec<&str> = wrapped.split('\n').collect();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("<back:red>prohibited")),
            "first wrapped highlight line should keep the opening tag: {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("<back:red>overlap")),
            "continuation line should inherit the unclosed <back:...> tag: {lines:?}"
        );
    }

    #[test]
    fn wrap_with_max_width_integrates_in_layout() {
        let d = ActivityDiagram {
            events: vec![
                ActivityEvent::Action {
                    text: "work".into(),
                },
                ActivityEvent::Note {
                    position: NotePosition::Right,
                    text: "A Long Long Long Long Long Long Long Long Long note".into(),
                },
            ],
            swimlanes: vec![],
            direction: Default::default(),
            note_max_width: Some(80.0),
            is_old_style: false,
            old_graph: None,
        };
        let layout = layout_activity(&d).unwrap();
        let note = &layout.nodes[1];
        // The note text should have been wrapped (contains newlines)
        assert!(
            note.text.contains('\n'),
            "note text should be wrapped: {:?}",
            note.text
        );
    }
}
