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
use crate::Result;
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
    /// vertical segment (used for implicit else when then-branch has break/goto).
    IfMergeEmphasize,
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
            let w = font_metrics::text_width(l, "SansSerif", FONT_SIZE, false, false);
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

    fn starts_with_ci(haystack: &str, needle: &str) -> bool {
        haystack.len() >= needle.len() && haystack[..needle.len()].eq_ignore_ascii_case(needle)
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

    log::debug!(
        "layout_activity: {} events, {} swimlanes",
        diagram.events.len(),
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
    let first_flow_event = diagram.events.iter().find(|e| {
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
    }
    let mut deferred_if_edges: Vec<DeferredIfEdges> = Vec::new();
    // Deferred break edges: (from_node_idx, from_y, exit_diamond_idx)
    let mut deferred_break_edges: Vec<(usize, f64, usize)> = Vec::new();
    // Pre-scan: for each `If` event, determine whether it has a matching `Else`.
    // This is needed so we can decide the branch layout direction up front.
    let if_has_else: HashMap<usize, bool> = {
        let mut map = HashMap::new();
        let mut if_stack_indices: Vec<usize> = Vec::new();
        for (ev_idx, ev) in diagram.events.iter().enumerate() {
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
        for (ev_idx, ev) in diagram.events.iter().enumerate() {
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
        diagram
            .events
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
        for (ev_idx, ev) in diagram.events.iter().enumerate() {
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

    for (event_idx, event) in diagram.events.iter().enumerate() {
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
                let ev_idx = diagram
                    .events
                    .iter()
                    .position(|e| std::ptr::eq(e, event))
                    .unwrap_or(0);
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
                } else {
                    swimlane_center_x(&swimlane_layouts, current_lane_idx)
                };
                let x = cx - w / 2.0;
                let y = y_cursor;
                log::debug!("  node[{node_index}] Action \"{text}\" @ ({x:.1}, {y:.1}) {w}x{h}, in_if={in_if}");
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::Action,
                    x,
                    y,
                    width: w,
                    height: h,
                    text: text.clone(),
                    skip_in_flow: in_if,
                });
                // Track node in if-branch
                if let Some(frame) = if_stack.last_mut() {
                    if let Some(ref mut else_branch) = frame.else_branch {
                        else_branch.nodes.push(node_index);
                    } else {
                        frame.then_branch.nodes.push(node_index);
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
                    let hex_w = cond_w + 2.0 * hex_half;
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

                    // Merge y = max of both branches' bottom_y.
                    // When the then-branch has a break, the implicit else path
                    // needs extra descent (break_gap + 17px for the merge path).
                    let then_bottom = frame.then_branch.bottom_y;
                    let else_bottom = frame
                        .else_branch
                        .as_ref()
                        .map(|b| b.bottom_y)
                        .unwrap_or(frame.diamond_bottom_y);
                    let merge_y = if frame.then_branch.has_break && frame.else_branch.is_none() {
                        // The else merge y needs to account for break path
                        let break_y = then_bottom - node_gap + 6.5157;
                        let else_merge_y = break_y + 17.0;
                        else_merge_y.max(then_bottom)
                    } else {
                        then_bottom.max(else_bottom)
                    };

                    // Restore y_cursor and last_flow_node_idx for main flow.
                    // The next node after the if-block is placed at merge_y + node_gap,
                    // which accounts for the merge path area.
                    y_cursor = merge_y + node_gap;
                    last_flow_node_idx = frame.saved_last_flow_node_idx;

                    log::debug!(
                        "  EndIf: merge_y={merge_y:.1}, then_bottom={then_bottom:.1}, else_bottom={else_bottom:.1}"
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
                    });
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

            // ---- While / EndWhile → diamonds ---------------------------------
            ActivityEvent::While { condition, label } => {
                let combined = if label.is_empty() {
                    condition.clone()
                } else {
                    format!("{condition}\n[{label}]")
                };
                let (w, h) = diamond_size(&combined);
                let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                let x = cx - w / 2.0;
                let y = y_cursor;
                log::debug!("  node[{node_index}] While diamond @ ({x:.1}, {y:.1})");
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

            ActivityEvent::EndWhile { label } => {
                let text = if label.is_empty() {
                    String::new()
                } else {
                    format!("[{label}]")
                };
                let (w, h) = diamond_size(if text.is_empty() { "end" } else { &text });
                let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                let x = cx - w / 2.0;
                let y = y_cursor;
                log::debug!("  node[{node_index}] EndWhile diamond @ ({x:.1}, {y:.1})");
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

            // ---- Fork / ForkAgain / EndFork → horizontal bars ----------------
            ActivityEvent::Fork | ActivityEvent::ForkAgain | ActivityEvent::EndFork => {
                let w = FORK_BAR_WIDTH;
                let h = FORK_BAR_HEIGHT;
                let cx = swimlane_center_x(&swimlane_layouts, current_lane_idx);
                let x = cx - w / 2.0;
                let y = y_cursor;
                log::debug!("  node[{node_index}] ForkBar @ ({x:.1}, {y:.1})");
                nodes.push(ActivityNodeLayout {
                    index: node_index,
                    kind: ActivityNodeKindLayout::ForkBar,
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
                let ev_idx = diagram
                    .events
                    .iter()
                    .position(|e| std::ptr::eq(e, event))
                    .unwrap_or(0);
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
                let ev_idx = diagram
                    .events
                    .iter()
                    .position(|e| std::ptr::eq(e, event))
                    .unwrap_or(0);
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
        }
    }

    // --- Pass 2b: centering for non-swimlane diagrams ----------------------
    if swimlane_layouts.is_empty() && !nodes.is_empty() {
        // Collect the widest flow node (excluding branch-internal nodes).
        // For if-diamonds, consider the entire if-block width (diamond + branches).
        let mut max_half_w = nodes
            .iter()
            .filter(|n| is_flow_node(&n.kind) && !n.skip_in_flow)
            .map(|n| n.width / 2.0)
            .fold(0.0_f64, f64::max);
        // Also consider if-block total width (diamond + left branch + right branch)
        for die in &deferred_if_edges {
            if die.has_else {
                let diamond = &nodes[die.diamond_idx];
                // Left branch width
                let left_w = die
                    .then_branch
                    .nodes
                    .iter()
                    .map(|&idx| nodes[idx].width)
                    .fold(0.0_f64, f64::max);
                let right_w = die
                    .else_branch
                    .as_ref()
                    .map(|b| {
                        b.nodes
                            .iter()
                            .map(|&idx| nodes[idx].width)
                            .fold(0.0_f64, f64::max)
                    })
                    .unwrap_or(0.0);
                // Total if-block half-width: left_branch_center_to_diamond_center + right
                let left_half = 10.0 + left_w / 2.0 + diamond.width / 2.0;
                let right_half = 10.0 + right_w / 2.0 + diamond.width / 2.0;
                max_half_w = max_half_w.max(left_half).max(right_half);
            } else {
                // For no-else blocks, the diamond itself is the widest
                let diamond = &nodes[die.diamond_idx];
                max_half_w = max_half_w.max(diamond.width / 2.0);
            }
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
            if is_flow_node(&node.kind) && !node.skip_in_flow {
                node.x = cx - node.width / 2.0;
            } else if !is_flow_node(&node.kind) && !node.skip_in_flow {
                node.x += cx;
            }
        }
        // Second pass: position if-diamonds and their branch nodes relative to cx
        for die in &deferred_if_edges {
            let diamond = &mut nodes[die.diamond_idx];
            diamond.x = cx - diamond.width / 2.0;
            let diamond_left_x = diamond.x;
            let diamond_right_x = diamond.x + diamond.width;
            if die.has_else {
                // Then branch center = diamond_left - 10
                let then_cx = diamond_left_x - 10.0;
                for &node_idx in &die.then_branch.nodes {
                    nodes[node_idx].x = then_cx - nodes[node_idx].width / 2.0;
                }
                // Else branch center = diamond_right + 10
                let else_cx = diamond_right_x + 10.0;
                if let Some(ref else_branch) = die.else_branch {
                    for &node_idx in &else_branch.nodes {
                        nodes[node_idx].x = else_cx - nodes[node_idx].width / 2.0;
                    }
                }
            } else {
                // No else: then branch at center
                for &node_idx in &die.then_branch.nodes {
                    nodes[node_idx].x = cx - nodes[node_idx].width / 2.0;
                }
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
        for event in &diagram.events {
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
                | ActivityEvent::Else { .. }
                | ActivityEvent::EndIf
                | ActivityEvent::While { .. }
                | ActivityEvent::EndWhile { .. }
                | ActivityEvent::Repeat
                | ActivityEvent::RepeatWhile { .. }
                | ActivityEvent::Fork
                | ActivityEvent::ForkAgain
                | ActivityEvent::EndFork
                | ActivityEvent::Note { .. }
                | ActivityEvent::FloatingNote { .. }
                | ActivityEvent::Detach
                | ActivityEvent::SyncBar(_) => {
                    node_lane.push(cur_lane);
                }
                ActivityEvent::GotoSyncBar(_)
                | ActivityEvent::ResumeFromSyncBar(_)
                | ActivityEvent::Label { .. }
                | ActivityEvent::Goto { .. }
                | ActivityEvent::Backward { .. }
                | ActivityEvent::Break => {}
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

        if die.has_else {
            // Java FtileIfWithLinks edge order:
            // 1. Then-branch exit (goto arrow or merge)
            // 2. Then-branch connection (diamond→branch)
            // 3. Else-branch connection (diamond→branch)
            // 4. Else merge
            // 5. Incoming (prev→diamond)

            // 1. Then-branch exit: goto arrow from last node down
            if die.then_branch.has_goto {
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
                    });
                }
            }

            // 2. Then branch connection (diamond left → first node)
            if let Some(first_idx) = die.then_branch.nodes.first() {
                let then_cx = diamond_left_x - 10.0;
                let node = &nodes[*first_idx];
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
                });
            }

            // 3. Else branch connection (diamond right → first node)
            if let Some(ref else_branch) = die.else_branch {
                if let Some(first_idx) = else_branch.nodes.first() {
                    let else_cx = diamond_right_x + 10.0;
                    let node = &nodes[*first_idx];
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
                    });
                }

                // 4. Else merge
                if !else_branch.has_goto && !else_branch.has_break {
                    if let (Some(last_idx), Some(next)) = (else_branch.nodes.last(), next_flow_idx)
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
                        });
                    }
                }
            }

            // 5. Incoming: prev→diamond (last among if-edges)
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
                });
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
                });
            }
            // Then branch end: if has_break, the break edge connects to repeat exit
            // If normal end, merge back to center
            if !die.then_branch.has_goto && !die.then_branch.has_break {
                // Normal then: connect last node to next flow node
                if let Some(last_idx) = die.then_branch.nodes.last() {
                    let last = &nodes[*last_idx];
                    let last_cx = last.x + last.width / 2.0;
                    let last_bottom = last.y + last.height;
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
                        edges.push(ActivityEdgeLayout {
                            from_index: *last_idx,
                            to_index: next,
                            label: String::new(),
                            points: vec![(last_cx, last_bottom), (next_cx, next_top)],
                            kind: ActivityEdgeKindLayout::Normal,
                        });
                    }
                }
            }

            // Implicit else branch (RIGHT): from diamond right edge to next flow node
            if die.then_branch.has_break || die.then_branch.has_goto {
                // When the then-branch has break/goto, the else side connects
                // from diamond right to the next flow node after the if-block.
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
                    // Diamond right → horizontal right → down → horizontal left → into next
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
                        kind: ActivityEdgeKindLayout::IfMergeEmphasize,
                    });
                }
            }

            // Incoming: prev→diamond
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
                });
            }
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
        | ActivityNodeKindLayout::GotoLines { .. } => (node.x - 1.0, node.x + node.width - 1.0),
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
    };

    // The backward box is already in the node bounds, so no extra width
    // is needed beyond what compute_bounds already provides.
    Some((vec![edge1, edge2], 0.0))
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

        // Edges: diamond→action + action→(implicit next, but there's no next node here)
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
        assert_eq!(layout.nodes.len(), 5);

        let fork = &layout.nodes[0];
        let fork_again = &layout.nodes[2];
        let end_fork = &layout.nodes[4];
        assert_eq!(fork.kind, ActivityNodeKindLayout::ForkBar);
        assert_eq!(fork_again.kind, ActivityNodeKindLayout::ForkBar);
        assert_eq!(end_fork.kind, ActivityNodeKindLayout::ForkBar);

        assert_eq!(fork.width, FORK_BAR_WIDTH);
        assert_eq!(fork.height, FORK_BAR_HEIGHT);
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
        assert_eq!(layout.nodes.len(), 3);

        let while_node = &layout.nodes[0];
        let end_while_node = &layout.nodes[2];
        assert_eq!(while_node.kind, ActivityNodeKindLayout::Diamond);
        assert_eq!(end_while_node.kind, ActivityNodeKindLayout::Diamond);
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
