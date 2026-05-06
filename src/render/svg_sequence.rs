use std::fmt::Write;
use std::sync::OnceLock;

use regex::Regex;

use crate::layout::sequence::{
    ActivationLayout, DelayLayout, DestroyLayout, DividerLayout, FragmentLayout, GroupLayout,
    MessageLayout, NoteLayout, ParticipantLayout, RefLayout, SeqLayout,
};
use crate::model::sequence::{FragmentKind, ParticipantKind, SeqArrowHead};
use crate::model::SequenceDiagram;
use crate::style::SkinParams;
use crate::Result;

use crate::font_metrics;
use crate::klimt::drawable::{DrawStyle, Drawable, LineShape};

use super::svg::{
    compute_viewport, ensure_visible_int, write_bg_rect, write_svg_root_bg, ViewportConfig,
};
use super::svg_richtext::{
    disable_path_sprites, enable_path_sprites, render_creole_note_content, render_creole_text,
    set_default_font_family, take_back_filters,
};
use crate::klimt::hand::{
    line_to_hand_path, polygon_points_svg, polygon_to_hand, rect_to_hand_polygon, JavaRandom,
    PathSegment,
};
use crate::klimt::sanitize_group_metadata_value;
use crate::klimt::svg::{fmt_coord, xml_escape, LengthAdjust, SvgGraphic};

// ── Handwritten helpers ─────────────────────────────────────────────

/// Java seed for handwritten mode (`new Random(424242L)`).
/// Each shape gets a fresh RNG — Java's `UGraphicHandwritten.apply()` creates
/// a new wrapper with `new Random(424242L)` on every `apply(UChange)`.
const HAND_SEED: i64 = 424242;

/// Emit a rect as a hand-drawn polygon when `handwritten`, or normal rect otherwise.
fn emit_rect(
    sg: &mut SvgGraphic,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    fill: &str,
    fill_opacity: Option<&str>,
    style: &str,
    handwritten: bool,
) {
    if handwritten {
        let mut rng = JavaRandom::new(HAND_SEED);
        let pts = rect_to_hand_polygon(w, h, 0.0, 0.0, &mut rng);
        let translated: Vec<(f64, f64)> = pts.iter().map(|(px, py)| (px + x, py + y)).collect();
        let pts_str = polygon_points_svg(&translated);
        let opacity = fill_opacity
            .map(|o| format!(" fill-opacity=\"{o}\""))
            .unwrap_or_default();
        let style_attr = if style.is_empty() {
            String::new()
        } else {
            format!(" style=\"{style}\"")
        };
        sg.push_raw(&format!(
            "<polygon fill=\"{fill}\"{opacity} points=\"{pts_str}\"{style_attr}/>"
        ));
    } else {
        let opacity = fill_opacity
            .map(|o| format!(" fill-opacity=\"{o}\""))
            .unwrap_or_default();
        let style_attr = if style.is_empty() {
            String::new()
        } else {
            format!(" style=\"{style}\"")
        };
        sg.push_raw(&format!(
            "<rect fill=\"{fill}\"{opacity} height=\"{h}\"{style_attr} width=\"{w}\" x=\"{x}\" y=\"{y}\"/>",
            h = fmt_coord(h), w = fmt_coord(w), x = fmt_coord(x), y = fmt_coord(y),
        ));
    }
}

/// Emit a line as a hand-drawn path when `handwritten`, or normal line otherwise.
fn emit_line(
    sg: &mut SvgGraphic,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    style: &str,
    handwritten: bool,
) {
    if handwritten {
        let mut rng = JavaRandom::new(HAND_SEED);
        let segs = line_to_hand_path(x2 - x1, y2 - y1, &mut rng);
        let mut d = String::new();
        for seg in &segs {
            match seg {
                PathSegment::MoveTo(sx, sy) => {
                    write!(d, "M{},{}", fmt_coord(sx + x1), fmt_coord(sy + y1)).unwrap();
                }
                PathSegment::LineTo(sx, sy) => {
                    write!(d, " L{},{}", fmt_coord(sx + x1), fmt_coord(sy + y1)).unwrap();
                }
            }
        }
        sg.push_raw(&format!(
            "<path d=\"{d}\" fill=\"none\" style=\"{style}\"/>"
        ));
    } else {
        sg.push_raw(&format!(
            "<line style=\"{style}\" x1=\"{x1}\" x2=\"{x2}\" y1=\"{y1}\" y2=\"{y2}\"/>",
            x1 = fmt_coord(x1),
            x2 = fmt_coord(x2),
            y1 = fmt_coord(y1),
            y2 = fmt_coord(y2),
        ));
    }
}

/// Emit a polygon with hand-jiggled points when `handwritten`, or clean polygon otherwise.
fn emit_polygon(
    sg: &mut SvgGraphic,
    points: &[(f64, f64)],
    fill: &str,
    style: &str,
    handwritten: bool,
) {
    if handwritten {
        let mut rng = JavaRandom::new(HAND_SEED);
        let jiggled = polygon_to_hand(points, &mut rng);
        let pts_str = polygon_points_svg(&jiggled);
        sg.push_raw(&format!(
            "<polygon fill=\"{fill}\" points=\"{pts_str}\" style=\"{style}\"/>"
        ));
    } else {
        let pts_str = polygon_points_svg(points);
        sg.push_raw(&format!(
            "<polygon fill=\"{fill}\" points=\"{pts_str}\" style=\"{style}\"/>"
        ));
    }
}

// ── Style constants ─────────────────────────────────────────────────

const FONT_SIZE: f64 = 13.0;
use crate::skin::rose::{
    ACTIVATION_BG, BORDER_COLOR, DESTROY_COLOR, GROUP_BG, NOTE_BG, NOTE_BORDER, PARTICIPANT_BG,
    TEXT_COLOR,
};

#[allow(dead_code)] // Java-ported rendering constant
const MARGIN: f64 = 5.0;

// Fragment tab geometry (from Java AWT font metrics)
const FRAG_TAB_LEFT_PAD: f64 = 15.0;
const FRAG_TAB_RIGHT_PAD: f64 = 30.0;
/// Fragment tab height must match layout's frag_header_height = h13 + 2.0.
/// Computed dynamically to preserve full f64 precision and avoid rounding drift.
fn frag_tab_height() -> f64 {
    font_metrics::line_height("SansSerif", FONT_SIZE, false, false) + 2.0
}
const FRAG_TAB_NOTCH: f64 = 10.0;
const FRAG_KIND_LABEL_Y_OFFSET: f64 = 13.0669;
const FRAG_GUARD_FONT_SIZE: f64 = 11.0;
const FRAG_GUARD_GAP: f64 = 15.0;
/// Guard label Y offset: Java uses marginY=2 + ascent(11pt).
/// Computed dynamically via `frag_guard_label_y_offset()` to avoid
/// precision loss from a truncated constant.
const FRAG_GUARD_MARGIN_Y: f64 = 2.0;
/// Guard label Y offset: marginY(2) + ascent(SansSerif, 11pt).
fn frag_guard_label_y_offset() -> f64 {
    FRAG_GUARD_MARGIN_Y + font_metrics::ascent("SansSerif", FRAG_GUARD_FONT_SIZE, true, false)
}

const DELAY_FONT_SIZE: f64 = 11.0;

const REF_TAB_HEIGHT: f64 = 17.0;
const REF_TAB_NOTCH: f64 = 10.0;
const REF_TAB_LEFT_PAD: f64 = 13.0;
const REF_KIND_LABEL_Y_OFFSET: f64 = 14.0669;
const REF_LABEL_FONT_SIZE: f64 = 12.0;
const REF_FRAME_STROKE: &str = "#000000";

/// Format stroke width for SVG: integer when whole, decimal otherwise.
fn fmt_stroke_width(w: f64) -> String {
    if (w - w.round()).abs() < f64::EPSILON {
        format!("{}", w as i32)
    } else {
        format!("{w}")
    }
}

fn svg_font_family_attr(font_family: &str) -> &str {
    match font_family {
        "SansSerif" => "sans-serif",
        "Serif" => "serif",
        "Monospaced" => "monospace",
        _ => font_family,
    }
}

fn svg_font_family_to_metrics_family(font_family: &str) -> &str {
    match font_family {
        "sans-serif" => "SansSerif",
        "serif" => "Serif",
        "monospace" => "Monospaced",
        _ => font_family,
    }
}

#[derive(Default)]
struct SequenceSvgBounds {
    // LimitFinder-style extent: used with `+1 + marginL + marginR` for
    // Java's getFinalDimension pass. Polygons contribute HACK_X_FOR_POLYGON
    // padding but no shadow padding (rect shadows use `2*deltaShadow` too,
    // mirroring LimitFinder.drawRectangle).
    max_x: f64,
    max_y: f64,
    // SvgGraphics ensureVisible-style extent: used with just `+1` for Java's
    // SvgGraphics.maxX which tracks the largest drawn coord inflated by
    // `2*deltaShadow` for shadowed shapes. No polygon HACK on this axis.
    sg_max_x: f64,
    sg_max_y: f64,
    seen: bool,
}

impl SequenceSvgBounds {
    fn add_point(&mut self, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        if !self.seen {
            self.max_x = x;
            self.max_y = y;
            self.sg_max_x = x;
            self.sg_max_y = y;
            self.seen = true;
            return;
        }
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
        self.sg_max_x = self.sg_max_x.max(x);
        self.sg_max_y = self.sg_max_y.max(y);
    }

    /// Add a point to only the SvgGraphics-style tracker (ensureVisible).
    fn add_sg_point(&mut self, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        if !self.seen {
            self.sg_max_x = x;
            self.sg_max_y = y;
            // Don't mark `seen` without also updating LimitFinder-style;
            // the caller is responsible for also calling `add_point` for
            // the unshadowed extent.
            return;
        }
        self.sg_max_x = self.sg_max_x.max(x);
        self.sg_max_y = self.sg_max_y.max(y);
    }

    fn track_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        self.add_point(x - 1.0, y - 1.0);
        self.add_point(x + width - 1.0, y + height - 1.0);
    }

    /// Track a rectangle that carries a drop-shadow filter. Java's
    /// LimitFinder.drawRectangle records `x + width - 1 + 2*deltaShadow` for
    /// getFinalDimension, while SvgGraphics.svgRectangle independently calls
    /// `ensureVisible(x + width + 2*deltaShadow, ...)` during draw. We track
    /// both: the LimitFinder value feeds `+1 + marginL + marginR`, and the
    /// ensureVisible value feeds `+1`.
    fn track_rect_shadowed(&mut self, x: f64, y: f64, width: f64, height: f64, delta_shadow: f64) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        self.add_point(x - 1.0, y - 1.0);
        self.add_point(
            x + width + 2.0 * delta_shadow - 1.0,
            y + height + 2.0 * delta_shadow - 1.0,
        );
        // SvgGraphics.svgRectangle ensureVisible: no -1 offset.
        self.add_sg_point(
            x + width + 2.0 * delta_shadow,
            y + height + 2.0 * delta_shadow,
        );
    }

    fn track_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.add_point(x1, y1);
        self.add_point(x2, y2);
    }

    fn track_line_shadowed(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, delta_shadow: f64) {
        // LimitFinder side: raw line bounds (no shadow pad).
        self.add_point(x1, y1);
        self.add_point(x2, y2);
        // Draw side: SvgGraphics.svgLine ensureVisible adds 2*deltaShadow.
        let pad = 2.0 * delta_shadow;
        self.add_sg_point(x1 + pad, y1 + pad);
        self.add_sg_point(x2 + pad, y2 + pad);
    }

    fn track_ellipse(&mut self, cx: f64, cy: f64, rx: f64, ry: f64) {
        self.add_point(cx - rx, cy - ry);
        self.add_point(cx + rx - 1.0, cy + ry - 1.0);
    }

    fn track_text(
        &mut self,
        x: f64,
        y: f64,
        text_width: f64,
        text_height: f64,
        anchor: Option<&str>,
    ) {
        let left_x = match anchor {
            Some("middle") => x - text_width / 2.0,
            Some("end") => x - text_width,
            _ => x,
        };
        let y_adj = y - text_height + 1.5;
        self.add_point(left_x, y_adj);
        self.add_point(left_x + text_width, y_adj + text_height);
    }

    fn track_points(&mut self, coords: &[f64]) {
        for pair in coords.chunks_exact(2) {
            self.add_point(pair[0], pair[1]);
        }
    }

    /// Track path segment endpoints with Java's `2*deltaShadow` padding.
    /// LimitFinder.drawUPath records raw UPath min/max; SvgGraphics.svgPath
    /// independently calls `ensureVisible(coord + 2*deltaShadow, ...)` for
    /// every segment endpoint when the path carries a drop-shadow filter.
    fn track_points_shadowed(&mut self, coords: &[f64], delta_shadow: f64) {
        // LimitFinder side: raw coords.
        for pair in coords.chunks_exact(2) {
            self.add_point(pair[0], pair[1]);
        }
        // Draw side: SvgGraphics.svgPath ensureVisible adds 2*deltaShadow.
        let pad = 2.0 * delta_shadow;
        for pair in coords.chunks_exact(2) {
            self.add_sg_point(pair[0] + pad, pair[1] + pad);
        }
    }

    /// Track polygon bounds with Java's `HACK_X_FOR_POLYGON = 10` horizontal
    /// padding. Java `LimitFinder.drawUPolygon` extends polygon bounds by 10
    /// on both ends of the x axis so that arrow-head triangles at the right
    /// edge of a sequence diagram don't clip the viewport. Mirroring this
    /// is required for byte-exact viewport widths to match Java output
    /// (notably for teoz diagrams with `->]` boundary arrows).
    fn track_polygon_points(&mut self, coords: &[f64]) {
        const HACK_X_FOR_POLYGON: f64 = 10.0;
        if coords.len() < 2 {
            return;
        }
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for pair in coords.chunks_exact(2) {
            min_x = min_x.min(pair[0]);
            max_x = max_x.max(pair[0]);
            min_y = min_y.min(pair[1]);
            max_y = max_y.max(pair[1]);
        }
        if !min_x.is_finite() || !max_x.is_finite() {
            return;
        }
        self.add_point(min_x - HACK_X_FOR_POLYGON, min_y);
        self.add_point(max_x + HACK_X_FOR_POLYGON, max_y);
    }

    /// Track a polygon as if it were the underlying URectangle.  Used for
    /// handwritten participant rect-replacements where Java's LimitFinder
    /// sees the un-jiggled rect (`drawRectangle` adds `(x-1, y-1)` and
    /// `(x+w-1, y+h-1)`).  We don't have the original rect dims here but
    /// the polygon's bounding box approximates them; the `-1` offset on
    /// the max corner mirrors `drawRectangle`'s rounding.  We also drop
    /// the `HACK_X_FOR_POLYGON` (it's only there for arrow-head
    /// triangles that might clip the right edge — irrelevant for boxes).
    fn track_polygon_as_rect(&mut self, coords: &[f64]) {
        if coords.len() < 2 {
            return;
        }
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for pair in coords.chunks_exact(2) {
            min_x = min_x.min(pair[0]);
            max_x = max_x.max(pair[0]);
            min_y = min_y.min(pair[1]);
            max_y = max_y.max(pair[1]);
        }
        if !min_x.is_finite() || !max_x.is_finite() {
            return;
        }
        // Mirror Java's `LimitFinder.drawRectangle`: the `-1` matches the
        // rounding semantics for rectangle bounds vs polygon bounds.
        self.add_point(min_x - 1.0, min_y - 1.0);
        self.add_point(max_x - 1.0, max_y - 1.0);
        // SvgGraphics.svgPolygon ensureVisible per vertex (no -1, but only
        // shadow-padded — for un-shadowed we mirror as the raw vertex).
        for pair in coords.chunks_exact(2) {
            self.add_sg_point(pair[0], pair[1]);
        }
    }

    /// Track polygon bounds for a shadowed polygon. Java's LimitFinder applies
    /// `HACK_X_FOR_POLYGON = 10` on the x axis; its SvgGraphics separately
    /// calls `ensureVisible(point + 2*deltaShadow)` per vertex when a drop
    /// shadow is present. We track both so the caller can take the final max.
    fn track_polygon_points_shadowed(&mut self, coords: &[f64], delta_shadow: f64) {
        const HACK_X_FOR_POLYGON: f64 = 10.0;
        if coords.len() < 2 {
            return;
        }
        let pad = 2.0 * delta_shadow;
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for pair in coords.chunks_exact(2) {
            min_x = min_x.min(pair[0]);
            max_x = max_x.max(pair[0]);
            min_y = min_y.min(pair[1]);
            max_y = max_y.max(pair[1]);
        }
        if !min_x.is_finite() || !max_x.is_finite() {
            return;
        }
        // LimitFinder side: HACK_X_FOR_POLYGON on x axis only.
        self.add_point(min_x - HACK_X_FOR_POLYGON, min_y);
        self.add_point(max_x + HACK_X_FOR_POLYGON, max_y);
        // Draw side: SvgGraphics.svgPolygon ensureVisible per vertex.
        for pair in coords.chunks_exact(2) {
            self.add_sg_point(pair[0] + pad, pair[1] + pad);
        }
    }

    /// LimitFinder-style dimensions to feed `+1 + marginL + marginR`.
    fn raw_body_dim(&self) -> Option<(f64, f64)> {
        self.seen.then_some((self.max_x + 1.0, self.max_y + 1.0))
    }

    /// SvgGraphics.ensureVisible-style dimensions to feed `+1`. These track
    /// shape extents inflated by `2*deltaShadow` for shadowed elements, which
    /// can exceed the LimitFinder extent (e.g. shadowed note paths on teoz
    /// sequence diagrams).
    fn sg_body_dim(&self) -> Option<(f64, f64)> {
        self.seen.then_some((self.sg_max_x, self.sg_max_y))
    }
}

fn parse_svg_number(raw: Option<&str>) -> Option<f64> {
    raw.map(|s| s.trim_end_matches("px"))
        .and_then(|s| s.parse::<f64>().ok())
}

/// Extract coordinate pairs from SVG path data or points attributes.
/// For `<polygon>`/`<polyline>` `points` attributes, all numbers are coordinates.
/// For `<path>` `d` attributes, we parse commands to extract only endpoint coordinates,
/// skipping arc parameters (radii, flags, rotation) that would corrupt bounds.
fn parse_svg_points(raw: Option<&str>) -> Vec<f64> {
    static FLOAT_RE: OnceLock<Regex> = OnceLock::new();
    let re = FLOAT_RE.get_or_init(|| Regex::new(r"-?\d+(?:\.\d+)?").unwrap());
    raw.map(|s| {
        re.find_iter(s)
            .filter_map(|m| m.as_str().parse::<f64>().ok())
            .collect()
    })
    .unwrap_or_default()
}

/// Extract only endpoint coordinate pairs from SVG path `d` data,
/// properly handling arc commands to avoid treating radii/flags as points.
fn parse_path_endpoints(d: &str) -> Vec<f64> {
    static NUM_RE: OnceLock<Regex> = OnceLock::new();
    let re = NUM_RE.get_or_init(|| Regex::new(r"[A-Za-z]|-?\d+(?:\.\d+)?").unwrap());
    let tokens: Vec<&str> = re.find_iter(d).map(|m| m.as_str()).collect();

    let mut coords = Vec::new();
    let mut i = 0;
    let mut cmd = ' ';

    while i < tokens.len() {
        let t = tokens[i];
        if t.len() == 1 && t.as_bytes()[0].is_ascii_alphabetic() {
            cmd = t.as_bytes()[0] as char;
            i += 1;
            continue;
        }
        match cmd {
            'M' | 'L' | 'T' | 'l' | 'm' | 't' => {
                // 2 numbers: x, y
                if i + 1 < tokens.len() {
                    if let (Ok(x), Ok(y)) = (tokens[i].parse::<f64>(), tokens[i + 1].parse::<f64>())
                    {
                        if cmd.is_ascii_uppercase() {
                            coords.push(x);
                            coords.push(y);
                        }
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            'C' | 'c' => {
                // 6 numbers: x1,y1 x2,y2 x,y — all are coordinates
                for _ in 0..3 {
                    if i + 1 < tokens.len() {
                        if let (Ok(x), Ok(y)) =
                            (tokens[i].parse::<f64>(), tokens[i + 1].parse::<f64>())
                        {
                            if cmd.is_ascii_uppercase() {
                                coords.push(x);
                                coords.push(y);
                            }
                        }
                        i += 2;
                    } else {
                        i += 1;
                        break;
                    }
                }
            }
            'S' | 'Q' | 's' | 'q' => {
                // 4 numbers: two coordinate pairs
                for _ in 0..2 {
                    if i + 1 < tokens.len() {
                        if let (Ok(x), Ok(y)) =
                            (tokens[i].parse::<f64>(), tokens[i + 1].parse::<f64>())
                        {
                            if cmd.is_ascii_uppercase() {
                                coords.push(x);
                                coords.push(y);
                            }
                        }
                        i += 2;
                    } else {
                        i += 1;
                        break;
                    }
                }
            }
            'A' | 'a' => {
                // 7 numbers: rx, ry, x-rotation, large-arc-flag, sweep-flag, x, y
                // Only the last two (x, y) are actual endpoint coordinates
                if i + 6 < tokens.len() {
                    if let (Ok(x), Ok(y)) =
                        (tokens[i + 5].parse::<f64>(), tokens[i + 6].parse::<f64>())
                    {
                        if cmd.is_ascii_uppercase() {
                            coords.push(x);
                            coords.push(y);
                        }
                    }
                    i += 7;
                } else {
                    i = tokens.len();
                }
            }
            'H' | 'h' => {
                // 1 number: x
                i += 1;
            }
            'V' | 'v' => {
                // 1 number: y
                i += 1;
            }
            'Z' | 'z' => {
                // no numbers
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    coords
}

/// Measure the final SVG body dimensions.
///
/// Returns `(limit_finder_dim, sg_graphics_dim)` where:
/// - `limit_finder_dim` = `(max_x + 1, max_y + 1)` in the style of Java's
///   LimitFinder (polygon HACK applied, no draw-time shadow padding). Feed
///   this to `+ marginL + marginR` to match Java's getFinalDimension.
/// - `sg_graphics_dim` = `(sg_max_x, sg_max_y)` tracking Java's SvgGraphics
///   ensureVisible calls during draw (shadow padding applied per shape). Feed
///   this to `+ 1` to match Java's `maxX = (int)(x + 1)` rounding.
///
/// Callers combine both via `max(limit_finder_dim_with_margins,
/// sg_graphics_dim_with_rounding)` to pick the larger of the two widths,
/// matching Java's two-pass behaviour where either pass can dominate
/// depending on which shapes carry drop shadows.
fn measure_sequence_body_dim_full(body: &str) -> Option<((f64, f64), (f64, f64))> {
    static TAG_RE: OnceLock<Regex> = OnceLock::new();
    static ATTR_RE: OnceLock<Regex> = OnceLock::new();
    static PART_G_RE: OnceLock<Regex> = OnceLock::new();
    let tag_re = TAG_RE.get_or_init(|| {
        Regex::new(r#"<(rect|line|text|ellipse|circle|polygon|polyline|path)\b([^>]*)>"#).unwrap()
    });
    let attr_re =
        ATTR_RE.get_or_init(|| Regex::new(r#"([A-Za-z_:][-A-Za-z0-9_:.]*)="([^"]*)""#).unwrap());
    // Pre-scan for `<g class="participant ...">` and `<g class="participant-lifeline">`
    // ranges.  Polygons emitted inside these (for handwritten participant boxes
    // / lifelines) are rect-replacements: Java's `LimitFinder` sees the
    // un-jiggled `URectangle` because `udrawable.drawU` is invoked on a
    // *non*-handwritten LimitFinder.  We mirror that by tracking these
    // polygons rect-style (with a `-1` offset on max coords) instead of
    // polygon-style (no -1, with HACK_X_FOR_POLYGON).  See
    // `klimt/drawing/LimitFinder.java::drawRectangle/drawUPolygon`.
    let part_g_re = PART_G_RE.get_or_init(|| {
        Regex::new(r#"<g class="(?:participant participant-head|participant participant-tail|participant-lifeline)\b"#)
            .unwrap()
    });
    // Collect (start, end) byte ranges of participant-box / lifeline group bodies.
    // For each `<g class="participant...">` start, find the matching `</g>`
    // (groups are emitted as a single non-nested wrapper here, with optional
    // inner `<g><title>...</title>` for lifelines — they all close before the
    // outer `</g>`).
    let mut part_ranges: Vec<(usize, usize)> = Vec::new();
    for m in part_g_re.find_iter(body) {
        let start = m.start();
        // Find the next `</g>` and walk past nested `<g`/`</g>` pairs.
        let mut depth: i32 = 1;
        let mut idx = m.end();
        while depth > 0 {
            let open = body[idx..].find("<g").map(|p| idx + p);
            let close = body[idx..].find("</g>").map(|p| idx + p);
            match (open, close) {
                (Some(o), Some(c)) if o < c => {
                    depth += 1;
                    idx = o + 2;
                }
                (_, Some(c)) => {
                    depth -= 1;
                    idx = c + 4;
                }
                _ => {
                    // Unbalanced; bail out and don't record this range.
                    idx = body.len();
                    break;
                }
            }
        }
        part_ranges.push((start, idx));
    }
    let in_participant_group =
        |pos: usize| -> bool { part_ranges.iter().any(|&(s, e)| pos >= s && pos < e) };

    let mut bounds = SequenceSvgBounds::default();

    // Java's default drop-shadow delta for sequence diagrams. Matches
    // ComponentRoseParticipant.deltaShadow (4) and ComponentRoseNote shadows.
    // Elements carrying `filter="url(#fXXX)"` in the emitted SVG were drawn by
    // SvgGraphics.svgPath / svgPolygon / svgRectangle with deltaShadow=4,
    // whose ensureVisible calls inflate the viewport by 2*deltaShadow = 8.
    const DEFAULT_SHADOW: f64 = 4.0;
    for cap in tag_re.captures_iter(body) {
        let tag = &cap[1];
        let attrs = &cap[2];
        let attr_map: std::collections::HashMap<&str, &str> = attr_re
            .captures_iter(attrs)
            .map(|m| (m.get(1).unwrap().as_str(), m.get(2).unwrap().as_str()))
            .collect();

        // Detect shadow: an element with a `filter="url(#fXXX)"` attribute was
        // drawn via the deltaShadow-aware path in Java's SvgGraphics.
        let has_shadow = attr_map
            .get("filter")
            .copied()
            .is_some_and(|f| f.starts_with("url(#f"));

        match tag {
            "rect" => {
                if let (Some(x), Some(y), Some(width), Some(height)) = (
                    parse_svg_number(attr_map.get("x").copied()),
                    parse_svg_number(attr_map.get("y").copied()),
                    parse_svg_number(attr_map.get("width").copied()),
                    parse_svg_number(attr_map.get("height").copied()),
                ) {
                    if has_shadow {
                        bounds.track_rect_shadowed(x, y, width, height, DEFAULT_SHADOW);
                    } else {
                        bounds.track_rect(x, y, width, height);
                    }
                }
            }
            "line" => {
                if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
                    parse_svg_number(attr_map.get("x1").copied()),
                    parse_svg_number(attr_map.get("y1").copied()),
                    parse_svg_number(attr_map.get("x2").copied()),
                    parse_svg_number(attr_map.get("y2").copied()),
                ) {
                    if has_shadow {
                        bounds.track_line_shadowed(x1, y1, x2, y2, DEFAULT_SHADOW);
                    } else {
                        bounds.track_line(x1, y1, x2, y2);
                    }
                }
            }
            "ellipse" => {
                if let (Some(cx), Some(cy), Some(rx), Some(ry)) = (
                    parse_svg_number(attr_map.get("cx").copied()),
                    parse_svg_number(attr_map.get("cy").copied()),
                    parse_svg_number(attr_map.get("rx").copied()),
                    parse_svg_number(attr_map.get("ry").copied()),
                ) {
                    bounds.track_ellipse(cx, cy, rx, ry);
                }
            }
            "circle" => {
                if let (Some(cx), Some(cy), Some(r)) = (
                    parse_svg_number(attr_map.get("cx").copied()),
                    parse_svg_number(attr_map.get("cy").copied()),
                    parse_svg_number(attr_map.get("r").copied()),
                ) {
                    bounds.track_ellipse(cx, cy, r, r);
                }
            }
            "text" => {
                if let (Some(x), Some(y)) = (
                    parse_svg_number(attr_map.get("x").copied()),
                    parse_svg_number(attr_map.get("y").copied()),
                ) {
                    let text_width =
                        parse_svg_number(attr_map.get("textLength").copied()).unwrap_or(0.0);
                    let font_size =
                        parse_svg_number(attr_map.get("font-size").copied()).unwrap_or(FONT_SIZE);
                    let font_family = svg_font_family_to_metrics_family(
                        attr_map.get("font-family").copied().unwrap_or("sans-serif"),
                    );
                    let font_weight = attr_map.get("font-weight").copied().unwrap_or("");
                    let bold =
                        font_weight == "bold" || font_weight.parse::<u32>().is_ok_and(|w| w >= 700);
                    let italic = attr_map.get("font-style").copied() == Some("italic");
                    let text_height =
                        font_metrics::line_height(font_family, font_size, bold, italic);
                    bounds.track_text(
                        x,
                        y,
                        text_width,
                        text_height,
                        attr_map.get("text-anchor").copied(),
                    );
                }
            }
            "polygon" | "polyline" => {
                let coords = parse_svg_points(attr_map.get("points").copied());
                let pos = cap.get(0).map(|m| m.start()).unwrap_or(0);
                if in_participant_group(pos) {
                    // Handwritten participant rect-replacement polygon.  Track
                    // as rect using polygon's bounding box (Java LF sees the
                    // un-jiggled URectangle).
                    bounds.track_polygon_as_rect(&coords);
                } else if has_shadow {
                    bounds.track_polygon_points_shadowed(&coords, DEFAULT_SHADOW);
                } else {
                    bounds.track_polygon_points(&coords);
                }
            }
            "path" => {
                let coords = parse_path_endpoints(attr_map.get("d").copied().unwrap_or(""));
                if has_shadow {
                    bounds.track_points_shadowed(&coords, DEFAULT_SHADOW);
                } else {
                    bounds.track_points(&coords);
                }
            }
            _ => {}
        }
    }

    let lf = bounds.raw_body_dim()?;
    let sg = bounds.sg_body_dim()?;
    Some((lf, sg))
}

#[allow(dead_code)] // convenience wrapper
fn measure_sequence_body_dim(body: &str) -> Option<(f64, f64)> {
    measure_sequence_body_dim_full(body).map(|(lf, _)| lf)
}

// ── Arrow marker defs ───────────────────────────────────────────────

fn write_seq_defs(sg: &mut SvgGraphic) {
    sg.push_raw("<defs/>");
}

/// Encode a `[[url text]]` link for the SVG `<title>` element.
/// Java replaces `:`, `/`, `\` with `.` and wraps with `..` prefix/suffix.
fn encode_link_title(url: &str, display_text: &str) -> String {
    let encoded_url: String = url
        .chars()
        .map(|c| match c {
            ':' | '/' | '\\' => '.',
            _ => c,
        })
        .collect();
    // When [[url text]] was not stripped (multiline case), extract text from markup.
    let raw_text = if display_text.starts_with("[[") {
        if let Some(end) = display_text.find("]]") {
            let inner = &display_text[2..end];
            // Skip past the URL (first token)
            if let Some(sp) = inner.find(' ') {
                &inner[sp + 1..]
            } else {
                ""
            }
        } else {
            display_text
        }
    } else {
        display_text
    };
    let encoded_text: String = raw_text
        .chars()
        .map(|c| match c {
            '\\' => '.',
            c if c == crate::NEWLINE_CHAR => '.',
            _ => c,
        })
        .collect();
    format!("..{encoded_url} {encoded_text}..")
}

fn first_metadata_title_line(display_text: &str) -> &str {
    let first_literal = display_text.split("\\n").next().unwrap_or(display_text);
    let first_encoded = first_literal
        .split(crate::NEWLINE_CHAR)
        .next()
        .unwrap_or(first_literal);
    first_encoded.lines().next().unwrap_or(first_encoded)
}

fn encode_metadata_title(display_text: &str) -> String {
    xml_escape(&sanitize_group_metadata_value(first_metadata_title_line(
        display_text,
    )))
}

// ── Lifelines ───────────────────────────────────────────────────────

/// Compute lifeline invisible rect height from layout bounds.
///
/// Java's `LivingParticipantBox` accumulates its preferred-size dimension
/// through multiple `addDim()` calls.  When the diagram contains a `group`
/// fragment, the grouping header's dimension causes an additional f32
fn draw_lifelines(
    sg: &mut SvgGraphic,
    layout: &SeqLayout,
    skin: &SkinParams,
    sd: &SequenceDiagram,
    handwritten: bool,
) {
    let ll_color = skin.sequence_lifeline_border_color(BORDER_COLOR);
    // Collect delay break segments sorted by y
    let mut delay_breaks: Vec<(f64, f64)> = layout
        .delays
        .iter()
        .map(|d| (d.lifeline_break_y, d.lifeline_break_y + d.height))
        .collect();
    delay_breaks.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    for (i, p) in layout.participants.iter().enumerate() {
        let part_idx = i + 1;
        let qualified_name = xml_escape(&p.name);
        let participant = sd.participants.get(i);
        let display = participant
            .and_then(|pp| pp.display_name.as_deref())
            .unwrap_or(&p.name);
        let title_text = if let Some(url) = participant.and_then(|pp| pp.link_url.as_deref()) {
            encode_link_title(url, display)
        } else {
            encode_metadata_title(display)
        };

        // Java lifeline position: box_x + (int)(box_width) / 2 (Java integer division)
        let box_x = p.x - p.box_width / 2.0;
        // Java teoz vs classic lifeline positioning:
        // Teoz: ComponentRoseLine area width = 1 → line at posC, rect at posC - 3.5
        // Classic: area width = headWidth + 2*outMargin → line at box_x + (int)(headWidth)/2
        let (lifeline_x, rect_x) = if sd.teoz_mode {
            (p.x, p.x - 3.5)
        } else {
            (box_x + (p.box_width as i32 / 2) as f64, p.x - 4.0)
        };

        if sd.teoz_mode {
            // Teoz: single segment, no delay splitting
            let ll_height = layout.lifeline_bottom - layout.lifeline_top;
            let mut tmp = String::new();
            write!(tmp, "<g><title>{dname}</title>", dname = title_text).unwrap();
            sg.push_raw(&tmp);

            emit_rect(
                sg,
                rect_x,
                layout.lifeline_top,
                8.0,
                ll_height,
                "#000000",
                Some("0.00000"),
                "",
                handwritten,
            );

            emit_line(
                sg,
                lifeline_x,
                layout.lifeline_top,
                lifeline_x,
                layout.lifeline_bottom,
                &format!("stroke:{};stroke-width:0.5;stroke-dasharray:5,5;", ll_color),
                handwritten,
            );
            sg.push_raw("</g>");
        } else if delay_breaks.is_empty() {
            // No delays: single continuous lifeline
            let ll_height = layout.lifeline_bottom - layout.lifeline_top;
            let mut tmp = String::new();
            write!(
                tmp,
                r#"<g class="participant-lifeline" data-entity-uid="part{idx}" data-qualified-name="{qname}" id="part{idx}-lifeline"><g><title>{dname}</title>"#,
                idx = part_idx, qname = qualified_name, dname = title_text,
            ).unwrap();
            sg.push_raw(&tmp);

            emit_rect(
                sg,
                rect_x,
                layout.lifeline_top,
                8.0,
                ll_height,
                "#000000",
                Some("0.00000"),
                "",
                handwritten,
            );

            emit_line(
                sg,
                lifeline_x,
                layout.lifeline_top,
                lifeline_x,
                layout.lifeline_bottom,
                &format!("stroke:{};stroke-width:0.5;stroke-dasharray:5,5;", ll_color),
                handwritten,
            );
            sg.push_raw("</g></g>");
        } else {
            // Delays present: split lifeline into segments with delay-style breaks.
            // Java: LivingParticipantBox splits its lifeline at delay segments.
            // Structure:
            //   <g class="participant-lifeline" ...>
            //     <g><title>...</title> <rect/> <line dasharray=5,5/> </g>  -- segment 1
            //     <line dasharray=1,4/>  -- delay break
            //     <g><title>...</title> <rect/> <line dasharray=5,5/> </g>  -- segment 2
            //     ...
            //   </g>
            let mut tmp = String::new();
            write!(
                tmp,
                r#"<g class="participant-lifeline" data-entity-uid="part{idx}" data-qualified-name="{qname}" id="part{idx}-lifeline">"#,
                idx = part_idx, qname = qualified_name,
            ).unwrap();
            sg.push_raw(&tmp);

            // Build segment boundaries from delays
            let mut seg_start = layout.lifeline_top;
            for &(break_start, break_end) in &delay_breaks {
                // Normal segment before this delay
                let seg_height = break_start - seg_start;
                let mut tmp = String::new();
                write!(tmp, "<g><title>{dname}</title>", dname = title_text).unwrap();
                sg.push_raw(&tmp);

                let mut tmp = String::new();
                let _ = write!(
                    tmp,
                    "<rect fill=\"#000000\" fill-opacity=\"0.00000\" height=\"{h}\" width=\"8\" x=\"{x}\" y=\"{y}\"/>",
                    h = fmt_coord(seg_height), x = fmt_coord(rect_x), y = fmt_coord(seg_start),
                );
                sg.push_raw(&tmp);

                let mut tmp = String::new();
                write!(
                    tmp,
                    r#"<line style="stroke:{color};stroke-width:0.5;stroke-dasharray:5,5;" x1="{x}" x2="{x}" y1="{y1}" y2="{y2}"/>"#,
                    x = fmt_coord(lifeline_x),
                    y1 = fmt_coord(seg_start),
                    y2 = fmt_coord(break_start),
                    color = ll_color,
                ).unwrap();
                sg.push_raw(&tmp);
                sg.push_raw("</g>");

                // Delay break line (dotted with stroke-dasharray:1,4)
                let mut tmp = String::new();
                write!(
                    tmp,
                    r#"<line style="stroke:{color};stroke-width:0.5;stroke-dasharray:1,4;" x1="{x}" x2="{x}" y1="{y1}" y2="{y2}"/>"#,
                    x = fmt_coord(lifeline_x),
                    y1 = fmt_coord(break_start),
                    y2 = fmt_coord(break_end),
                    color = ll_color,
                ).unwrap();
                sg.push_raw(&tmp);

                seg_start = break_end;
            }

            // Final segment after last delay
            let seg_height = layout.lifeline_bottom - seg_start;
            let mut tmp = String::new();
            write!(tmp, "<g><title>{dname}</title>", dname = title_text).unwrap();
            sg.push_raw(&tmp);

            let mut tmp = String::new();
            let _ = write!(
                tmp,
                "<rect fill=\"#000000\" fill-opacity=\"0.00000\" height=\"{h}\" width=\"8\" x=\"{x}\" y=\"{y}\"/>",
                h = fmt_coord(seg_height), x = fmt_coord(rect_x), y = fmt_coord(seg_start),
            );
            sg.push_raw(&tmp);

            let mut tmp = String::new();
            write!(
                tmp,
                r#"<line style="stroke:{color};stroke-width:0.5;stroke-dasharray:5,5;" x1="{x}" x2="{x}" y1="{y1}" y2="{y2}"/>"#,
                x = fmt_coord(lifeline_x),
                y1 = fmt_coord(seg_start),
                y2 = fmt_coord(layout.lifeline_bottom),
                color = ll_color,
            ).unwrap();
            sg.push_raw(&tmp);
            sg.push_raw("</g>");

            sg.push_raw("</g>");
        }
    }
}

/// Teoz-mode interleaved rendering: draw each participant's lifeline
/// followed by that participant's activation bars.
/// Java MainTile draws components per-participant, so activations appear
/// immediately after their participant's lifeline group.
fn draw_lifelines_with_activations(
    sg: &mut SvgGraphic,
    layout: &SeqLayout,
    skin: &SkinParams,
    sd: &SequenceDiagram,
    _display_names: &std::collections::HashMap<&str, &str>,
    shadow_attr: &str,
) {
    let ll_color = skin.sequence_lifeline_border_color(BORDER_COLOR);

    for (i, p) in layout.participants.iter().enumerate() {
        let participant = sd.participants.get(i);
        let display = participant
            .and_then(|pp| pp.display_name.as_deref())
            .unwrap_or(&p.name);
        let title_text = if let Some(url) = participant.and_then(|pp| pp.link_url.as_deref()) {
            encode_link_title(url, display)
        } else {
            encode_metadata_title(display)
        };

        // Teoz: ComponentRoseLine area width = 1 → line at posC, rect at posC - 3.5
        let lifeline_x = p.x;
        let rect_x = p.x - 3.5;

        // Draw this participant's lifeline
        let ll_height = layout.lifeline_bottom - layout.lifeline_top;
        let mut tmp = String::new();
        write!(tmp, "<g><title>{dname}</title>", dname = title_text).unwrap();
        sg.push_raw(&tmp);

        let mut tmp = String::new();
        let _ = write!(
            tmp,
            "<rect fill=\"#000000\" fill-opacity=\"0.00000\" height=\"{h}\" width=\"8\" x=\"{x}\" y=\"{y}\"/>",
            h = fmt_coord(ll_height), x = fmt_coord(rect_x), y = fmt_coord(layout.lifeline_top),
        );
        sg.push_raw(&tmp);

        let mut tmp = String::new();
        write!(
            tmp,
            r#"<line style="stroke:{color};stroke-width:0.5;stroke-dasharray:5,5;" x1="{x}" x2="{x}" y1="{y1}" y2="{y2}"/>"#,
            x = fmt_coord(lifeline_x),
            y1 = fmt_coord(layout.lifeline_top),
            y2 = fmt_coord(layout.lifeline_bottom),
            color = ll_color,
        ).unwrap();
        sg.push_raw(&tmp);
        sg.push_raw("</g>");

        // Draw activation bars belonging to this participant.
        // Java teoz LiveBoxes.drawOneLevel iterates per level. After each
        // bar's doDrawing, drawDestroyIfNeeded emits the destroy cross at the
        // bar's terminating Y. We approximate this by emitting the destroy
        // cross right after the activation bar that ends at the destroy Y.
        //
        // Java teoz: LiveBoxesDrawer passes null for stringsToDisplay → Display.NULL → empty tooltip
        for act in &layout.activations {
            if act.participant == p.name {
                draw_activation(sg, act, "", shadow_attr, ll_color);
                // If this bar ends exactly at a destroy for the same
                // participant, emit the destroy cross immediately after
                // (matches Java drawDestroyIfNeeded order).
                for d in &layout.destroys {
                    if d.participant == p.name && (d.y - act.y_end).abs() < 0.001 {
                        draw_destroy(sg, d);
                    }
                }
            }
        }
    }
}

// ── Color utilities ─────────────────────────────────────────────────

/// Resolve a color string into SVG fill + optional fill-opacity attributes.
/// Handles: "transparent", "#RRGGBBAA" (8-digit hex), "#RRGGBB", named colors.
fn resolve_fill_attrs(color: &str) -> String {
    let c = color.trim();
    if c.eq_ignore_ascii_case("transparent") || c.eq_ignore_ascii_case("#transparent") {
        return r#"fill="none""#.to_string();
    }
    // 8-digit hex: #RRGGBBAA
    if c.starts_with('#') && c.len() == 9 {
        let rgb = &c[..7];
        if let Ok(alpha) = u8::from_str_radix(&c[7..9], 16) {
            if alpha == 0 {
                return r#"fill="none""#.to_string();
            } else if alpha == 255 {
                return format!(r#"fill="{rgb}""#);
            } else {
                let opacity = alpha as f64 / 255.0;
                return format!(r#"fill="{rgb}" fill-opacity="{opacity:.5}""#);
            }
        }
    }
    format!(r#"fill="{c}""#)
}

// ── Participant box ─────────────────────────────────────────────────

fn draw_participant_box_with_font(
    sg: &mut SvgGraphic,
    p: &ParticipantLayout,
    y: f64,
    display_name: Option<&str>,
    bg: &str,
    border: &str,
    text_color: &str,
    part_font_family: &str,
    part_font_size: f64,
    part_font_style: Option<&str>,
    head: bool,
    link_url: Option<&str>,
    shadow_attr: &str,
    stroke_width: f64,
    rounded: bool,
    delta_shadow: f64,
    handwritten: bool,
) {
    let fill = p.color.as_deref().unwrap_or(bg);

    match &p.kind {
        ParticipantKind::Actor => {
            draw_participant_actor(
                sg,
                p,
                y,
                display_name,
                fill,
                border,
                text_color,
                0.5,
                "SansSerif",
            );
        }
        ParticipantKind::Boundary => {
            draw_participant_boundary(sg, p, y, display_name, fill, border, text_color, head);
        }
        ParticipantKind::Control => {
            draw_participant_control(sg, p, y, display_name, fill, border, text_color, head);
        }
        ParticipantKind::Entity => {
            draw_participant_entity(sg, p, y, display_name, fill, border, text_color, head);
        }
        ParticipantKind::Database => {
            draw_participant_database(sg, p, y, display_name, fill, border, text_color, head);
        }
        ParticipantKind::Collections => {
            draw_participant_collections(sg, p, y, display_name, fill, border, text_color, head);
        }
        ParticipantKind::Queue => {
            draw_participant_queue(sg, p, y, display_name, fill, border, text_color, head);
        }
        ParticipantKind::Default => {
            draw_participant_rect_with_font(
                sg,
                p,
                y,
                display_name,
                fill,
                border,
                text_color,
                part_font_family,
                part_font_size,
                part_font_style,
                link_url,
                shadow_attr,
                stroke_width,
                rounded,
                delta_shadow,
                handwritten,
            );
        }
    }
}

fn draw_participant_rect_with_font(
    sg: &mut SvgGraphic,
    p: &ParticipantLayout,
    y: f64,
    display_name: Option<&str>,
    bg: &str,
    border: &str,
    text_color: &str,
    font_family: &str,
    font_size: f64,
    font_style: Option<&str>,
    link_url: Option<&str>,
    shadow_attr: &str,
    stroke_width: f64,
    rounded: bool,
    delta_shadow: f64,
    handwritten: bool,
) {
    let name = display_name.unwrap_or(&p.name);
    let lines: Vec<&str> = name
        .split("\\n")
        .flat_map(|s| s.split(crate::NEWLINE_CHAR))
        .collect();
    let padding = 7.0;
    let box_width = p.box_width;
    let box_height = p.box_height;
    // Java: posC (lifeline center) = posB + preferredWidth/2 where
    // preferredWidth = box_width + deltaShadow. The rect is drawn at posB,
    // so rect_x = posC - preferredWidth/2 = p.x - (box_width + deltaShadow)/2.
    let x = p.x - (box_width + delta_shadow) / 2.0;
    let text_x = x + padding;
    let italic = font_style == Some("italic");
    let text_y_base = y + 19.9951 + (font_size - 14.0) * 0.92825;
    let line_h = font_metrics::line_height(font_family, font_size, false, italic);
    let svg_font_family = svg_font_family_attr(font_family);
    let line_widths: Vec<f64> = lines
        .iter()
        .map(|line| font_metrics::text_width(line, font_family, font_size, false, italic))
        .collect();
    let max_line_width = line_widths.iter().copied().fold(0.0_f64, f64::max);

    let fill_attrs = resolve_fill_attrs(bg);
    if handwritten {
        // Handwritten: draw as a jiggled polygon with fresh RNG per shape.
        // Java's `URectangleHand` receives the raw `rect.getRx()` from
        // `URectangle.rounded(roundCorner)` and halves it itself
        // (`Math.min(rx/2, width/2)`).  For sequence participants the skin
        // sets `RoundCorner 5`, so the value handed to URectangleHand is `5`.
        // Mirror that semantic: pass the un-halved corner here, the
        // `rect_to_hand_polygon` helper halves it internally.
        let rx_val = if rounded { 5.0 } else { 0.0 };
        let ry_val = rx_val;
        let mut rng = JavaRandom::new(HAND_SEED);
        let pts = rect_to_hand_polygon(box_width, box_height, rx_val, ry_val, &mut rng);
        let translated: Vec<(f64, f64)> = pts.iter().map(|(px, py)| (px + x, py + y)).collect();
        let pts_str = polygon_points_svg(&translated);
        let sw_str = fmt_coord(stroke_width);
        sg.push_raw(&format!(
            "<polygon {fill_attrs}{shadow_attr} points=\"{pts_str}\" style=\"stroke:{border};stroke-width:{sw_str};\"/>"
        ));
    } else {
        let round_attrs = if rounded { r#" rx="2.5" ry="2.5""# } else { "" };
        let mut tmp = String::new();
        write!(
            tmp,
            r#"<rect {fill_attrs}{shadow} height="{h}"{round} style="stroke:{border};stroke-width:{sw};" width="{w}" x="{x}" y="{y}"/>"#,
            h = fmt_coord(box_height),
            w = fmt_coord(box_width),
            x = fmt_coord(x),
            y = fmt_coord(y),
            shadow = shadow_attr,
            sw = fmt_coord(stroke_width),
            round = round_attrs,
        )
        .unwrap();
        sg.push_raw(&tmp);
    }

    let effective_text_color = if link_url.is_some() {
        "#0000FF"
    } else {
        text_color
    };
    let text_decoration = if link_url.is_some() {
        Some("underline")
    } else {
        None
    };

    // Java: each line of multiline text gets its own <a> wrapper
    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(url) = link_url {
            sg.push_raw(&format!(
                r#"<a href="{url}" target="_top" title="{url}" xlink:actuate="onRequest" xlink:href="{url}" xlink:show="new" xlink:title="{url}" xlink:type="simple">"#
            ));
        }
        let text_y = text_y_base + line_idx as f64 * line_h;
        let line_w = line_widths[line_idx];
        let line_x = text_x + (max_line_width - line_w) / 2.0;
        sg.set_fill_color(effective_text_color);
        sg.svg_text(
            line,
            line_x,
            text_y,
            Some(svg_font_family),
            font_size,
            None,
            font_style,
            text_decoration,
            line_w,
            LengthAdjust::Spacing,
            None,
            0,
            None,
        );
        if link_url.is_some() {
            sg.push_raw("</a>");
        }
    }
}

/// Actor: Java renders TEXT first (plain), then ELLIPSE (filled head),
/// then single PATH (body+arms+legs). Stroke-width=0.5.
///
/// Java ActorStickMan constants:
///   headDiam=16, bodyLength=27, armsY=8, armsLength=13, legsX=13, legsY=15
///   thickness = stroke.thickness = 0.5
///   startX = max(armsLength,legsX) - headDiam/2 + thickness = 5.5
///   centerX = startX + headDiam/2 = 13.5
///   prefWidth = max(armsLength,legsX)*2 + 2*thickness = 27
///   prefHeight = headDiam + bodyLength + legsY + 2*thickness + deltaShadow + 1 = 60
///
/// Java ComponentRoseActor (head=true):
///   marginX1=3, marginX2=3
///   textWidth = pureTextWidth + 6
///   prefWidth = max(stickmanWidth(27), textWidth)
///   textMiddlePos = (prefWidth - textWidth) / 2
///   text rendered at: (textMiddlePos, stickmanHeight) relative to component origin
///   stickman at: (delta, 0) where delta = (prefWidth - stickmanWidth) / 2
fn draw_participant_actor(
    sg: &mut SvgGraphic,
    p: &ParticipantLayout,
    y: f64,
    display_name: Option<&str>,
    fill: &str,
    border: &str,
    text_color: &str,
    thickness: f64,
    font_family: &str,
) {
    let name = display_name.unwrap_or(&p.name);
    let cx = p.x; // participant center x from layout

    // Java ActorStickMan constants
    let head_diam = 16.0_f64;
    let head_r = head_diam / 2.0;
    let body_length = 27.0_f64;
    let arms_y = 8.0_f64;
    let arms_length = 13.0_f64;
    let legs_x = 13.0_f64;
    let legs_y = 15.0_f64;
    let stickman_width = arms_length.max(legs_x) * 2.0 + 2.0 * thickness;
    let stickman_height = head_diam + body_length + legs_y + 2.0 * thickness + 1.0;

    // Java: startX = max(arms,legs) - headDiam/2 + thickness
    let start_x = arms_length.max(legs_x) - head_diam / 2.0 + thickness;
    // Java: centerX = startX + headDiam/2
    let center_x = start_x + head_diam / 2.0;

    // Text metrics - use effective font from skin params
    let font_size = 14.0;
    let svg_ff = svg_font_family_attr(font_family);
    let tl = font_metrics::text_width(name, font_family, font_size, false, false);
    let margin_x1 = 3.0;
    let margin_x2 = 3.0;
    let text_width = tl + margin_x1 + margin_x2;
    let pref_width = stickman_width.max(text_width);
    let text_middle_pos = (pref_width - text_width) / 2.0;

    // Java: outMargin = 5, startingX = 0 for first participant
    // component_x = startingX + outMargin = p.x - pref_width/2 - outMargin + outMargin
    // Actually, p.x is the CENTER of the participant box from layout.
    // Java: getCenterX = startingX + prefWidth/2 + outMargin
    // So: component_x = p.x - pref_width/2
    // But Java adds outMargin to the drawing position.
    // Let's derive from known: Java ellipse cx = 24.8335 = component_x + startX + headR
    //   component_x + 5.5 + 8 = 24.8335 → component_x = 11.3335
    // But we need component_x from p.x. p.x = getCenterX = startingX + prefWidth/2 + outMargin
    // For Alice: prefWidth = 39.667, outMargin = 5
    //   getCenterX = 0 + 39.667/2 + 5 = 24.8335
    // So p.x = 24.8335. component_x = p.x - prefWidth/2 = 24.8335 - 19.8335 = 5.0
    // BUT Java drawU applies UTranslate(getMinX, y1) where getMinX = startingX + outMargin = 5
    // So component origin is at x=5.

    // For general case: component_x = p.x - pref_width / 2.0
    let component_x = cx - pref_width / 2.0;

    // 1. Text first
    // Java: textBlock.drawU at (textMiddlePos, stickmanHeight).
    // marginX1 is already baked into textWidth for centering calculation,
    // but does NOT add to the SVG x coordinate.
    let text_x = component_x + text_middle_pos;
    let text_y = y + stickman_height + font_metrics::ascent(font_family, font_size, false, false);
    sg.set_fill_color(text_color);
    sg.svg_text(
        name,
        text_x,
        text_y,
        Some(svg_ff),
        font_size,
        None,
        None,
        None,
        tl,
        LengthAdjust::Spacing,
        None,
        0,
        None,
    );

    // 2. Ellipse head
    // Java: head at (startX, thickness) relative to component + delta
    let delta = (pref_width - stickman_width) / 2.0;
    let head_cx = component_x + delta + start_x + head_r;
    let head_cy = y + thickness + head_r;
    let hcx = fmt_coord(head_cx);
    let hcy = fmt_coord(head_cy);
    let hr = fmt_coord(head_r);
    let sw = fmt_stroke_width(thickness);
    let fill_attrs = resolve_fill_attrs(fill);
    let mut el = String::new();
    write!(el,
        "<ellipse cx=\"{hcx}\" cy=\"{hcy}\" {fill_attrs} rx=\"{hr}\" ry=\"{hr}\" style=\"stroke:{border};stroke-width:{sw};\"/>"
    ).unwrap();
    sg.push_raw(&el);

    // 3. Single path for body+arms+legs
    // Java path origin at (centerX, headDiam + thickness) relative to component + delta
    let path_ox = component_x + delta + center_x;
    let path_oy = y + head_diam + thickness;
    // Path segments (relative to path origin):
    //   M(0,0) L(0,bodyLength) M(-arms,armsY) L(arms,armsY) M(0,bodyLength) L(-legsX,bodyLength+legsY) M(0,bodyLength) L(legsX,bodyLength+legsY)
    let pcx = fmt_coord(path_ox);
    let bt = fmt_coord(path_oy);
    let bb = fmt_coord(path_oy + body_length);
    let la = fmt_coord(path_ox - arms_length);
    let ra = fmt_coord(path_ox + arms_length);
    let ay = fmt_coord(path_oy + arms_y);
    let ll = fmt_coord(path_ox - legs_x);
    let rl = fmt_coord(path_ox + legs_x);
    let lf = fmt_coord(path_oy + body_length + legs_y);
    let mut pa = String::new();
    write!(pa,
        "<path d=\"M{pcx},{bt} L{pcx},{bb} M{la},{ay} L{ra},{ay} M{pcx},{bb} L{ll},{lf} M{pcx},{bb} L{rl},{lf}\" fill=\"none\" style=\"stroke:{border};stroke-width:{sw};\"/>"
    ).unwrap();
    sg.push_raw(&pa);
}

/// Actor tail: Java ComponentRoseActor(head=false).
/// Text is ABOVE, stickman BELOW. Same constants as head.
fn draw_participant_actor_tail(
    sg: &mut SvgGraphic,
    p: &ParticipantLayout,
    y: f64,
    display_name: Option<&str>,
    fill: &str,
    border: &str,
    text_color: &str,
    thickness: f64,
    font_family: &str,
) {
    let name = display_name.unwrap_or(&p.name);
    let cx = p.x;

    // Same constants as draw_participant_actor (head)
    let head_diam = 16.0_f64;
    let head_r = head_diam / 2.0;
    let body_length = 27.0_f64;
    let arms_y = 8.0_f64;
    let arms_length = 13.0_f64;
    let legs_x = 13.0_f64;
    let legs_y = 15.0_f64;
    let stickman_width = arms_length.max(legs_x) * 2.0 + 2.0 * thickness;
    let _stickman_height = head_diam + body_length + legs_y + 2.0 * thickness + 1.0;
    let start_x = arms_length.max(legs_x) - head_diam / 2.0 + thickness;
    let center_x = start_x + head_diam / 2.0;

    let font_size = 14.0;
    let svg_ff = svg_font_family_attr(font_family);
    let tl = font_metrics::text_width(name, font_family, font_size, false, false);
    let margin_x1 = 3.0;
    let margin_x2 = 3.0;
    let text_width = tl + margin_x1 + margin_x2;
    let pref_width = stickman_width.max(text_width);
    let text_middle_pos = (pref_width - text_width) / 2.0;
    let component_x = cx - pref_width / 2.0;

    // Java (head=false): text at (textMiddlePos, 0), stickman at (delta, textHeight)
    let text_height = font_metrics::line_height(font_family, font_size, false, false);
    let text_x = component_x + text_middle_pos;
    let text_y = y + font_metrics::ascent(font_family, font_size, false, false);

    // 1. Text first
    sg.set_fill_color(text_color);
    sg.svg_text(
        name,
        text_x,
        text_y,
        Some(svg_ff),
        font_size,
        None,
        None,
        None,
        tl,
        LengthAdjust::Spacing,
        None,
        0,
        None,
    );

    // 2. Stickman below text
    let delta = (pref_width - stickman_width) / 2.0;
    let stickman_y = y + text_height;

    // Ellipse head
    let head_cx = component_x + delta + start_x + head_r;
    let head_cy = stickman_y + thickness + head_r;
    let hcx = fmt_coord(head_cx);
    let hcy = fmt_coord(head_cy);
    let hr = fmt_coord(head_r);
    let sw = fmt_stroke_width(thickness);
    let fill_attrs = resolve_fill_attrs(fill);
    let mut el = String::new();
    write!(el,
        "<ellipse cx=\"{hcx}\" cy=\"{hcy}\" {fill_attrs} rx=\"{hr}\" ry=\"{hr}\" style=\"stroke:{border};stroke-width:{sw};\"/>"
    ).unwrap();
    sg.push_raw(&el);

    // Body path
    let path_ox = component_x + delta + center_x;
    let path_oy = stickman_y + head_diam + thickness;
    let pcx = fmt_coord(path_ox);
    let bt = fmt_coord(path_oy);
    let bb = fmt_coord(path_oy + body_length);
    let la = fmt_coord(path_ox - arms_length);
    let ra = fmt_coord(path_ox + arms_length);
    let ay = fmt_coord(path_oy + arms_y);
    let ll = fmt_coord(path_ox - legs_x);
    let rl = fmt_coord(path_ox + legs_x);
    let lf = fmt_coord(path_oy + body_length + legs_y);
    let mut pa = String::new();
    write!(pa,
        "<path d=\"M{pcx},{bt} L{pcx},{bb} M{la},{ay} L{ra},{ay} M{pcx},{bb} L{ll},{lf} M{pcx},{bb} L{rl},{lf}\" fill=\"none\" style=\"stroke:{border};stroke-width:{sw};\"/>"
    ).unwrap();
    sg.push_raw(&pa);
}

/// Boundary: vertical line + horizontal connector + ellipse, with text below.
/// Matches Java: Boundary.java (margin=4, radius=12, left=17) +
/// ComponentRoseBoundary.java (head=true: text at dimStickman.height, icon at delta).
/// Boundary: vertical line + horizontal connector + ellipse, with text below (head) or above (tail).
/// Matches Java: Boundary.java (margin=4, radius=12, left=17) +
/// ComponentRoseBoundary.java (head: text at dimStickman.height; tail: icon at textHeight).
fn draw_participant_boundary(
    sg: &mut SvgGraphic,
    p: &ParticipantLayout,
    y: f64,
    display_name: Option<&str>,
    bg: &str,
    border: &str,
    text_color: &str,
    head: bool,
) {
    let name = display_name.unwrap_or(&p.name);
    let cx = p.x;

    // Java Boundary.java constants
    let margin = 4.0_f64;
    let radius = 12.0_f64;
    let left = 17.0_f64;
    let icon_width = radius * 2.0 + left + 2.0 * margin; // 49
    let icon_height = radius * 2.0 + 2.0 * margin; // 32

    // Text metrics (Java: marginX1=3, marginX2=3)
    let font_size = 14.0;
    let margin_x1 = 3.0;
    let margin_x2 = 3.0;
    let tl = font_metrics::text_width(name, "SansSerif", font_size, false, false);
    let text_width = tl + margin_x1 + margin_x2;
    let pref_width = icon_width.max(text_width);
    let text_middle_pos = (pref_width - text_width) / 2.0;
    let component_x = cx - pref_width / 2.0;
    let delta = (pref_width - icon_width) / 2.0;
    let text_height = font_metrics::line_height("SansSerif", font_size, false, false);

    if head {
        // Head: text below icon
        // 1. Text at (textMiddlePos, icon_height)
        let text_x = component_x + text_middle_pos;
        let text_y = y + icon_height + font_metrics::ascent("SansSerif", font_size, false, false);
        sg.set_fill_color(text_color);
        sg.svg_text(
            name,
            text_x,
            text_y,
            Some("sans-serif"),
            font_size,
            None,
            None,
            None,
            tl,
            LengthAdjust::Spacing,
            None,
            0,
            None,
        );

        // 2. Path at (delta + margin, margin)
        let px = component_x + delta + margin;
        let py = y + margin;
        draw_boundary_icon(sg, px, py, radius, left, bg, border);
    } else {
        // Tail: text above icon
        // 1. Text at (textMiddlePos, 0)
        let text_x = component_x + text_middle_pos;
        let text_y = y + font_metrics::ascent("SansSerif", font_size, false, false);
        sg.set_fill_color(text_color);
        sg.svg_text(
            name,
            text_x,
            text_y,
            Some("sans-serif"),
            font_size,
            None,
            None,
            None,
            tl,
            LengthAdjust::Spacing,
            None,
            0,
            None,
        );

        // 2. Icon at (delta, textHeight)
        let px = component_x + delta + margin;
        let py = y + text_height + margin;
        draw_boundary_icon(sg, px, py, radius, left, bg, border);
    }
}

/// Draw the boundary icon (path + ellipse) at the given origin.
fn draw_boundary_icon(
    sg: &mut SvgGraphic,
    px: f64,
    py: f64,
    radius: f64,
    left: f64,
    bg: &str,
    border: &str,
) {
    let px_s = fmt_coord(px);
    let py_top = fmt_coord(py);
    let py_bot = fmt_coord(py + radius * 2.0);
    let py_mid = fmt_coord(py + radius);
    let px_right = fmt_coord(px + left);
    let mut pa = String::new();
    write!(pa,
        "<path d=\"M{px_s},{py_top} L{px_s},{py_bot} M{px_s},{py_mid} L{px_right},{py_mid}\" fill=\"none\" style=\"stroke:{border};stroke-width:0.5;\"/>"
    ).unwrap();
    sg.push_raw(&pa);

    let ecx = px + left + radius;
    let ecy = py + radius;
    let mut el = String::new();
    write!(el,
        "<ellipse cx=\"{}\" cy=\"{}\" fill=\"{bg}\" rx=\"{r}\" ry=\"{r}\" style=\"stroke:{border};stroke-width:0.5;\"/>",
        fmt_coord(ecx), fmt_coord(ecy), r = fmt_coord(radius)
    ).unwrap();
    sg.push_raw(&el);
}

/// Control: ellipse + small arrow polygon. Matches Java Control.java.
fn draw_participant_control(
    sg: &mut SvgGraphic,
    p: &ParticipantLayout,
    y: f64,
    display_name: Option<&str>,
    bg: &str,
    border: &str,
    text_color: &str,
    head: bool,
) {
    draw_iconic_participant(
        sg,
        p,
        y,
        display_name,
        bg,
        border,
        text_color,
        head,
        |sg, px, py, radius, bg, border| {
            // Ellipse
            let ecx = px + radius;
            let ecy = py + radius;
            let mut el = String::new();
            write!(el,
                "<ellipse cx=\"{}\" cy=\"{}\" fill=\"{bg}\" rx=\"{r}\" ry=\"{r}\" style=\"stroke:{border};stroke-width:0.5;\"/>",
                fmt_coord(ecx), fmt_coord(ecy), r = fmt_coord(radius)
            ).unwrap();
            sg.push_raw(&el);

            // Arrow polygon (Java: Control.java xWing=6, yAperture=5, xContact=4)
            let x_wing = 6.0_f64;
            let y_aperture = 5.0_f64;
            let x_contact = 4.0_f64;
            let ax = px + radius - x_contact;
            let ay = py;
            let pts = format!(
                "{},{},{},{},{},{},{},{},{},{}",
                fmt_coord(ax),
                fmt_coord(ay),
                fmt_coord(ax + x_wing),
                fmt_coord(ay - y_aperture),
                fmt_coord(ax + x_contact),
                fmt_coord(ay),
                fmt_coord(ax + x_wing),
                fmt_coord(ay + y_aperture),
                fmt_coord(ax),
                fmt_coord(ay),
            );
            let mut pg = String::new();
            write!(pg,
                "<polygon fill=\"{border}\" points=\"{pts}\" style=\"stroke:{border};stroke-width:1;\"/>"
            ).unwrap();
            sg.push_raw(&pg);
        },
    );
}

/// Entity: ellipse + horizontal underline. Matches Java EntityDomain.java.
fn draw_participant_entity(
    sg: &mut SvgGraphic,
    p: &ParticipantLayout,
    y: f64,
    display_name: Option<&str>,
    bg: &str,
    border: &str,
    text_color: &str,
    head: bool,
) {
    draw_iconic_participant(
        sg,
        p,
        y,
        display_name,
        bg,
        border,
        text_color,
        head,
        |sg, px, py, radius, bg, border| {
            // Ellipse
            let ecx = px + radius;
            let ecy = py + radius;
            let mut el = String::new();
            write!(el,
                "<ellipse cx=\"{}\" cy=\"{}\" fill=\"{bg}\" rx=\"{r}\" ry=\"{r}\" style=\"stroke:{border};stroke-width:0.5;\"/>",
                fmt_coord(ecx), fmt_coord(ecy), r = fmt_coord(radius)
            ).unwrap();
            sg.push_raw(&el);

            // Underline (Java: suppY=2, hline at y + 2*radius + suppY)
            let supp_y = 2.0;
            let line_y = py + 2.0 * radius + supp_y;
            let mut ln = String::new();
            write!(ln,
                "<line style=\"stroke:{border};stroke-width:0.5;\" x1=\"{}\" x2=\"{}\" y1=\"{}\" y2=\"{}\"/>",
                fmt_coord(px), fmt_coord(px + 2.0 * radius),
                fmt_coord(line_y), fmt_coord(line_y)
            ).unwrap();
            sg.push_raw(&ln);
        },
    );
}

/// Generic iconic participant rendering (boundary/control/entity pattern).
/// Java: ComponentRose{Boundary,Control,Entity} all share the same layout:
/// head=true: text below icon, head=false (tail): text above icon.
fn draw_iconic_participant(
    sg: &mut SvgGraphic,
    p: &ParticipantLayout,
    y: f64,
    display_name: Option<&str>,
    bg: &str,
    border: &str,
    text_color: &str,
    head: bool,
    draw_icon: impl FnOnce(&mut SvgGraphic, f64, f64, f64, &str, &str),
) {
    let name = display_name.unwrap_or(&p.name);
    let cx = p.x;
    let margin = 4.0;
    let radius = 12.0;
    let icon_width: f64 = radius * 2.0 + 2.0 * margin; // 32
    let icon_height: f64 = radius * 2.0 + 2.0 * margin; // 32

    let font_size = 14.0_f64;
    let margin_x1 = 3.0_f64;
    let margin_x2 = 3.0_f64;
    let tl = font_metrics::text_width(name, "SansSerif", font_size, false, false);
    let text_width = tl + margin_x1 + margin_x2;
    let pref_width = icon_width.max(text_width);
    let text_middle_pos = (pref_width - text_width) / 2.0;
    let component_x = cx - pref_width / 2.0;
    let delta = (pref_width - icon_width) / 2.0;
    let text_height = font_metrics::line_height("SansSerif", font_size, false, false);

    if head {
        // Text at (textMiddlePos, icon_height)
        let text_x = component_x + text_middle_pos;
        let text_y = y + icon_height + font_metrics::ascent("SansSerif", font_size, false, false);
        sg.set_fill_color(text_color);
        sg.svg_text(
            name,
            text_x,
            text_y,
            Some("sans-serif"),
            font_size,
            None,
            None,
            None,
            tl,
            LengthAdjust::Spacing,
            None,
            0,
            None,
        );
        // Icon at (delta + margin, margin)
        draw_icon(
            sg,
            component_x + delta + margin,
            y + margin,
            radius,
            bg,
            border,
        );
    } else {
        // Text at (textMiddlePos, 0)
        let text_x = component_x + text_middle_pos;
        let text_y = y + font_metrics::ascent("SansSerif", font_size, false, false);
        sg.set_fill_color(text_color);
        sg.svg_text(
            name,
            text_x,
            text_y,
            Some("sans-serif"),
            font_size,
            None,
            None,
            None,
            tl,
            LengthAdjust::Spacing,
            None,
            0,
            None,
        );
        // Icon at (delta + margin, textHeight + margin)
        draw_icon(
            sg,
            component_x + delta + margin,
            y + text_height + margin,
            radius,
            bg,
            border,
        );
    }
}

/// Database: cylinder shape using cubic bezier paths.
/// Matches Java: USymbolDatabase.drawDatabase + ComponentRoseDatabase.
/// Stickman = asSmall(empty(16,17)) + Margin(10,10,24,5) → dim=(36, 46).
fn draw_participant_database(
    sg: &mut SvgGraphic,
    p: &ParticipantLayout,
    y: f64,
    display_name: Option<&str>,
    bg: &str,
    border: &str,
    text_color: &str,
    head: bool,
) {
    let name = display_name.unwrap_or(&p.name);
    let cx = p.x;

    let icon_width = 36.0_f64; // DATABASE_ICON_WIDTH
    let icon_height = 46.0_f64; // dimStickman.height
    let curve_h = 10.0_f64; // Java drawDatabase hardcoded curve constant

    let font_size = 14.0_f64;
    let margin_x1 = 3.0_f64;
    let margin_x2 = 3.0_f64;
    let tl = font_metrics::text_width(name, "SansSerif", font_size, false, false);
    let text_width = tl + margin_x1 + margin_x2;
    let pref_width = icon_width.max(text_width);
    let text_middle_pos = (pref_width - text_width) / 2.0;
    let component_x = cx - pref_width / 2.0;
    let delta = (pref_width - icon_width) / 2.0;
    let text_height = font_metrics::line_height("SansSerif", font_size, false, false);

    let (text_x, text_y, cyl_x, cyl_y);
    if head {
        text_x = component_x + text_middle_pos;
        text_y = y + icon_height + font_metrics::ascent("SansSerif", font_size, false, false);
        cyl_x = component_x + delta;
        cyl_y = y;
    } else {
        text_x = component_x + text_middle_pos;
        text_y = y + font_metrics::ascent("SansSerif", font_size, false, false);
        cyl_x = component_x + delta;
        cyl_y = y + text_height;
    }

    // 1. Text first
    sg.set_fill_color(text_color);
    sg.svg_text(
        name,
        text_x,
        text_y,
        Some("sans-serif"),
        font_size,
        None,
        None,
        None,
        tl,
        LengthAdjust::Spacing,
        None,
        0,
        None,
    );

    // 2. Cylinder body path (Java: USymbolDatabase.drawDatabase)
    draw_database_cylinder(
        sg,
        cyl_x,
        cyl_y,
        icon_width,
        icon_height,
        curve_h,
        bg,
        border,
    );
}

/// Draw the database cylinder using cubic bezier paths matching Java USymbolDatabase.
fn draw_database_cylinder(
    sg: &mut SvgGraphic,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    ch: f64,
    bg: &str,
    border: &str,
) {
    let mid = w / 2.0;
    // Path 1: cylinder body
    // M(0,ch) C(0,0, mid,0, mid,0) C(mid,0, w,0, w,ch) L(w,h-ch) C(w,h, mid,h, mid,h) C(mid,h, 0,h, 0,h-ch) L(0,ch)
    let x0 = fmt_coord(x);
    let xm = fmt_coord(x + mid);
    let xw = fmt_coord(x + w);
    let yt = fmt_coord(y); // 0
    let yc = fmt_coord(y + ch); // ch (top of body)
    let yb = fmt_coord(y + h - ch); // h-ch (bottom of body)
    let yh = fmt_coord(y + h); // h (bottom control)
    let mut body = String::new();
    write!(body,
        "<path d=\"M{x0},{yc} C{x0},{yt} {xm},{yt} {xm},{yt} C{xm},{yt} {xw},{yt} {xw},{yc} L{xw},{yb} C{xw},{yh} {xm},{yh} {xm},{yh} C{xm},{yh} {x0},{yh} {x0},{yb} L{x0},{yc}\" fill=\"{bg}\" style=\"stroke:{border};stroke-width:0.5;\"/>"
    ).unwrap();
    sg.push_raw(&body);

    // Path 2: inner top curve (closing/front ellipse)
    let yc2 = fmt_coord(y + ch * 2.0); // 2*ch
    let mut top = String::new();
    write!(top,
        "<path d=\"M{x0},{yc} C{x0},{yc2} {xm},{yc2} {xm},{yc2} C{xm},{yc2} {xw},{yc2} {xw},{yc}\" fill=\"none\" style=\"stroke:{border};stroke-width:0.5;\"/>"
    ).unwrap();
    sg.push_raw(&top);
}

/// Collections: two stacked rectangles + text inside main rect.
/// Matches Java: ComponentRoseCollections — shadow rect offset by COLLECTIONS_DELTA=4.
/// Same for both head and tail (text always inside main rect).
fn draw_participant_collections(
    sg: &mut SvgGraphic,
    p: &ParticipantLayout,
    y: f64,
    display_name: Option<&str>,
    bg: &str,
    border: &str,
    text_color: &str,
    _head: bool,
) {
    let name = display_name.unwrap_or(&p.name);
    let cx = p.x;
    let delta = 4.0_f64; // COLLECTIONS_DELTA

    let font_size = 14.0_f64;
    let tl = font_metrics::text_width(name, "SansSerif", font_size, false, false);
    let rect_w = tl + 2.0 * 7.0; // text + padding (like default participant)
    let rect_h = p.box_height - delta; // base participant height (30.2969)
    let pref_width = rect_w + delta;
    let component_x = cx - pref_width / 2.0;

    // Shadow rect at (component_x + delta, y)
    let mut tmp = String::new();
    write!(tmp,
        r#"<rect fill="{bg}" height="{}" style="stroke:{border};stroke-width:0.5;" width="{}" x="{}" y="{}"/>"#,
        fmt_coord(rect_h), fmt_coord(rect_w), fmt_coord(component_x + delta), fmt_coord(y),
    ).unwrap();
    sg.push_raw(&tmp);

    // Main rect at (component_x, y + delta)
    let main_y = y + delta;
    let mut tmp = String::new();
    write!(tmp,
        r#"<rect fill="{bg}" height="{}" style="stroke:{border};stroke-width:0.5;" width="{}" x="{}" y="{}"/>"#,
        fmt_coord(rect_h), fmt_coord(rect_w), fmt_coord(component_x), fmt_coord(main_y),
    ).unwrap();
    sg.push_raw(&tmp);

    // Text inside main rect
    let text_x = component_x + 7.0;
    let text_y = main_y + 7.0 + font_metrics::ascent("SansSerif", font_size, false, false);
    sg.set_fill_color(text_color);
    sg.svg_text(
        name,
        text_x,
        text_y,
        Some("sans-serif"),
        font_size,
        None,
        None,
        None,
        tl,
        LengthAdjust::Spacing,
        None,
        0,
        None,
    );
}

/// Queue: rounded-right rectangle with text inside, using cubic-bezier curves.
/// Matches Java: USymbolQueue.drawQueue (dx=5, margin 5,15,5,5).
/// Text is inside the shape (no head/tail text separation).
fn draw_participant_queue(
    sg: &mut SvgGraphic,
    p: &ParticipantLayout,
    y: f64,
    display_name: Option<&str>,
    bg: &str,
    border: &str,
    text_color: &str,
    _head: bool,
) {
    let name = display_name.unwrap_or(&p.name);
    let cx = p.x;
    let dx = 5.0_f64; // Java USymbolQueue.dx

    let font_size = 14.0_f64;
    let tl = font_metrics::text_width(name, "SansSerif", font_size, false, false);
    let text_height = font_metrics::line_height("SansSerif", font_size, false, false);

    // Queue margin: x1=5, x2=15, y1=5, y2=5
    let margin_x1 = 5.0_f64;
    let margin_x2 = 15.0_f64;
    let _margin_y1 = 5.0_f64;
    let w = tl + margin_x1 + margin_x2; // shape width
    let h = text_height + 10.0; // shape height (margin_y1 + margin_y2)

    let pref_width = w;
    let component_x = cx - pref_width / 2.0;
    let mid_y = h / 2.0;

    // Draw body path
    let x0 = component_x;
    let y0 = y;
    let x0s = fmt_coord(x0 + dx);
    let x1s = fmt_coord(x0 + w - dx);
    let xws = fmt_coord(x0 + w);
    let x0f = fmt_coord(x0);
    let y0s = fmt_coord(y0);
    let yms = fmt_coord(y0 + mid_y);
    let yhs = fmt_coord(y0 + h);
    let mut body = String::new();
    write!(body,
        "<path d=\"M{x0s},{y0s} L{x1s},{y0s} C{xws},{y0s} {xws},{yms} {xws},{yms} C{xws},{yms} {xws},{yhs} {x1s},{yhs} L{x0s},{yhs} C{x0f},{yhs} {x0f},{yms} {x0f},{yms} C{x0f},{yms} {x0f},{y0s} {x0s},{y0s}\" fill=\"{bg}\" style=\"stroke:{border};stroke-width:0.5;\"/>"
    ).unwrap();
    sg.push_raw(&body);

    // Inner right curve (closing path)
    let x2s = fmt_coord(x0 + w - dx * 2.0);
    let mut closing = String::new();
    write!(closing,
        "<path d=\"M{x1s},{y0s} C{x2s},{y0s} {x2s},{yms} {x2s},{yms} C{x2s},{yhs} {x1s},{yhs} {x1s},{yhs}\" fill=\"none\" style=\"stroke:{border};stroke-width:0.5;\"/>"
    ).unwrap();
    sg.push_raw(&closing);

    // Text inside shape at (margin_x1, vertically centered)
    let text_x = x0 + margin_x1;
    let text_y =
        y0 + (h - text_height) / 2.0 + font_metrics::ascent("SansSerif", font_size, false, false);
    sg.set_fill_color(text_color);
    sg.svg_text(
        name,
        text_x,
        text_y,
        Some("sans-serif"),
        font_size,
        None,
        None,
        None,
        tl,
        LengthAdjust::Spacing,
        None,
        0,
        None,
    );
}

/// Render a single text line word-by-word (Java beta5 behavior when maxmessagesize is set).
/// Each word becomes a separate `<text>` element, with `&#160;` elements between words.
/// `metrics_font` is the internal font name for width calculations (e.g. "SansSerif").
/// `svg_font` is the SVG font-family attribute value (e.g. "sans-serif").
fn render_word_by_word(
    sg: &mut SvgGraphic,
    line: &str,
    x: f64,
    y: f64,
    metrics_font: &str,
    svg_font: &str,
    font_size: f64,
) {
    let words: Vec<&str> = line.split(' ').collect();
    let mut cur_x = x;
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            // Render &#160; (non-breaking space) between words
            let space_w =
                font_metrics::text_width("\u{00a0}", metrics_font, font_size, false, false);
            sg.set_fill_color(TEXT_COLOR);
            sg.svg_text(
                "\u{00a0}",
                cur_x,
                y,
                Some(svg_font),
                font_size,
                None,
                None,
                None,
                space_w,
                LengthAdjust::Spacing,
                None,
                0,
                None,
            );
            cur_x += space_w;
        }
        if word.is_empty() {
            continue;
        }
        let word_w = font_metrics::text_width(word, metrics_font, font_size, false, false);
        sg.set_fill_color(TEXT_COLOR);
        sg.svg_text(
            word,
            cur_x,
            y,
            Some(svg_font),
            font_size,
            None,
            None,
            None,
            word_w,
            LengthAdjust::Spacing,
            None,
            0,
            None,
        );
        cur_x += word_w;
    }
}

// ── Messages ────────────────────────────────────────────────────────

fn draw_message(
    sg: &mut SvgGraphic,
    msg: &MessageLayout,
    arrow_color: &str,
    arrow_thickness: f64,
    msg_font_family: &str,
    msg_svg_family: &str,
    msg_font_size: f64,
    msg_text_color: &str,
    from_idx: usize,
    to_idx: usize,
    msg_idx: usize,
    _source_line: Option<usize>,
    word_by_word: bool,
    teoz_mode: bool,
    handwritten: bool,
) {
    // Java teoz does not wrap messages in <g class="message">
    if !teoz_mode {
        sg.push_raw(&format!(
            r#"<g class="message" data-entity-1="part{}" data-entity-2="part{}" id="msg{}">"#,
            from_idx, to_idx, msg_idx,
        ));
    }

    let sw = arrow_thickness as u32;

    // Java constants for circle decorations
    const DIAM_CIRCLE: f64 = 8.0;
    const THIN_CIRCLE: f64 = 1.5;

    // Java constants for cross (X) decorations
    const SPACE_CROSS_X: f64 = 6.0;
    const ARROW_DELTA_X: f64 = 10.0;

    // Circle at target shifts the arrowhead inward by diamCircle/2 + thinCircle
    let circle_to_shift = if msg.circle_to { 4.0 + 1.5 } else { 0.0 };
    // Circle at source: for bidirectional, the from-end also has an arrowhead so it
    // needs the full arrowhead shift (4.0 + 1.5). For non-bidir, only the line shifts (4.0).
    let circle_from_shift = if msg.circle_from {
        if msg.bidirectional {
            4.0 + 1.5
        } else {
            4.0
        }
    } else {
        0.0
    };

    // Determine arrow tip position and line endpoints
    // Java insets the arrow tip 2px from the participant center
    let (tip_x, line_x1, _line_x2) = if msg.is_left {
        // Right-to-left: arrow points left, tip 1px inset from target center
        (
            msg.to_x + 1.0 + circle_to_shift,
            msg.from_x - 1.0 - circle_from_shift,
            msg.to_x,
        )
    } else {
        // Left-to-right: arrow points right, tip 2px inset from target center
        (
            msg.to_x - 2.0 - circle_to_shift,
            msg.from_x + circle_from_shift,
            msg.to_x,
        )
    };

    // For bidirectional arrows, Java renders decorations grouped by side (left then right).
    // For non-bidirectional, circles are drawn first, then crosses, then arrowhead.
    // Determine left/right x for bidirectional from-end tip
    let bidir_from_tip_x = if msg.is_left {
        msg.from_x - 2.0 - circle_from_shift
    } else {
        msg.from_x + 1.0 + circle_from_shift
    };

    // Identify which side is "left" and which is "right" in the diagram
    let (left_circle, right_circle) = if msg.is_left {
        (msg.circle_to, msg.circle_from) // to is left, from is right
    } else {
        (msg.circle_from, msg.circle_to) // from is left, to is right
    };
    let (left_cross, right_cross) = if msg.is_left {
        (msg.cross_to, msg.cross_from)
    } else {
        (msg.cross_from, msg.cross_to)
    };
    let (left_cx, right_cx) = if msg.is_left {
        (msg.to_x, msg.from_x)
    } else {
        (msg.from_x, msg.to_x)
    };

    if msg.bidirectional {
        // Bidirectional: draw decorations grouped by side (left then right)
        // Left side: circle, then arrowhead/cross
        if left_circle {
            let cx = left_cx - 0.5;
            let cy = msg.y - 0.75;
            sg.push_raw(&format!(
                r##"<ellipse cx="{}" cy="{}" fill="#000000" rx="{}" ry="{}" style="stroke:{};stroke-width:{};"/>"##,
                fmt_coord(cx), fmt_coord(cy),
                fmt_coord(DIAM_CIRCLE / 2.0), fmt_coord(DIAM_CIRCLE / 2.0),
                arrow_color, fmt_coord(THIN_CIRCLE),
            ));
        }
        // Left arrowhead (pointing left)
        if left_cross {
            // Cross at left side
            let x0 = if msg.is_left {
                tip_x + SPACE_CROSS_X
            } else {
                msg.from_x + SPACE_CROSS_X + 1.0
            };
            let half = ARROW_DELTA_X / 2.0;
            let mut tmp = String::new();
            write!(tmp, r#"<line style="stroke:{color};stroke-width:2;" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}"/>"#,
                color = arrow_color, x1 = fmt_coord(x0), x2 = fmt_coord(x0 + ARROW_DELTA_X),
                y1 = fmt_coord(msg.y - half), y2 = fmt_coord(msg.y + half)).unwrap();
            write!(tmp, r#"<line style="stroke:{color};stroke-width:2;" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}"/>"#,
                color = arrow_color, x1 = fmt_coord(x0), x2 = fmt_coord(x0 + ARROW_DELTA_X),
                y1 = fmt_coord(msg.y + half), y2 = fmt_coord(msg.y - half)).unwrap();
            sg.push_raw(&tmp);
        } else {
            // Left arrowhead polygon (pointing left)
            let left_tip = if msg.is_left { tip_x } else { bidir_from_tip_x };
            let arm_x = left_tip + 10.0;
            let inner_x = left_tip + 6.0;
            let pts = vec![
                (arm_x, msg.y - 4.0),
                (left_tip, msg.y),
                (arm_x, msg.y + 4.0),
                (inner_x, msg.y),
            ];
            emit_polygon(
                sg,
                &pts,
                arrow_color,
                &format!("stroke:{};stroke-width:1;", arrow_color),
                handwritten,
            );
        }

        // Right side: circle, then arrowhead/cross
        if right_circle {
            let cx = right_cx - 0.5;
            let cy = msg.y - 0.75;
            sg.push_raw(&format!(
                r##"<ellipse cx="{}" cy="{}" fill="#000000" rx="{}" ry="{}" style="stroke:{};stroke-width:{};"/>"##,
                fmt_coord(cx), fmt_coord(cy),
                fmt_coord(DIAM_CIRCLE / 2.0), fmt_coord(DIAM_CIRCLE / 2.0),
                arrow_color, fmt_coord(THIN_CIRCLE),
            ));
        }
        // Right arrowhead (pointing right)
        if right_cross {
            // Cross at right side
            let x0 = if msg.is_left {
                msg.from_x - SPACE_CROSS_X - ARROW_DELTA_X - 2.0
            } else {
                tip_x - SPACE_CROSS_X - ARROW_DELTA_X
            };
            let half = ARROW_DELTA_X / 2.0;
            let mut tmp = String::new();
            write!(tmp, r#"<line style="stroke:{color};stroke-width:2;" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}"/>"#,
                color = arrow_color, x1 = fmt_coord(x0), x2 = fmt_coord(x0 + ARROW_DELTA_X),
                y1 = fmt_coord(msg.y - half), y2 = fmt_coord(msg.y + half)).unwrap();
            write!(tmp, r#"<line style="stroke:{color};stroke-width:2;" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}"/>"#,
                color = arrow_color, x1 = fmt_coord(x0), x2 = fmt_coord(x0 + ARROW_DELTA_X),
                y1 = fmt_coord(msg.y + half), y2 = fmt_coord(msg.y - half)).unwrap();
            sg.push_raw(&tmp);
        } else {
            // Right arrowhead polygon (pointing right)
            let right_tip = if msg.is_left { bidir_from_tip_x } else { tip_x };
            let arm_x = right_tip - 10.0;
            let inner_x = right_tip - 6.0;
            let pts = vec![
                (arm_x, msg.y - 4.0),
                (right_tip, msg.y),
                (arm_x, msg.y + 4.0),
                (inner_x, msg.y),
            ];
            emit_polygon(
                sg,
                &pts,
                arrow_color,
                &format!("stroke:{};stroke-width:1;", arrow_color),
                handwritten,
            );
        }
    } else {
        // Non-bidirectional: Java renders left-side decorations first, then right-side.
        // This means spatial order: leftmost elements first, rightmost last.

        // Helper: draw circle at given participant x
        let draw_circle = |sg: &mut SvgGraphic, px: f64| {
            let cx = px - 0.5;
            let cy = msg.y - 0.75;
            sg.push_raw(&format!(
                r##"<ellipse cx="{}" cy="{}" fill="#000000" rx="{}" ry="{}" style="stroke:{};stroke-width:{};"/>"##,
                fmt_coord(cx), fmt_coord(cy),
                fmt_coord(DIAM_CIRCLE / 2.0), fmt_coord(DIAM_CIRCLE / 2.0),
                arrow_color, fmt_coord(THIN_CIRCLE),
            ));
        };

        // Helper: draw cross at given position
        let draw_cross = |sg: &mut SvgGraphic, x0: f64| {
            let half = ARROW_DELTA_X / 2.0;
            let mut tmp = String::new();
            write!(tmp, r#"<line style="stroke:{color};stroke-width:2;" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}"/>"#,
                color = arrow_color, x1 = fmt_coord(x0), x2 = fmt_coord(x0 + ARROW_DELTA_X),
                y1 = fmt_coord(msg.y - half), y2 = fmt_coord(msg.y + half)).unwrap();
            write!(tmp, r#"<line style="stroke:{color};stroke-width:2;" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}"/>"#,
                color = arrow_color, x1 = fmt_coord(x0), x2 = fmt_coord(x0 + ARROW_DELTA_X),
                y1 = fmt_coord(msg.y + half), y2 = fmt_coord(msg.y - half)).unwrap();
            sg.push_raw(&tmp);
        };

        // Helper: draw arrowhead at tip_x
        let draw_arrowhead = |sg: &mut SvgGraphic| {
            if msg.cross_to {
                return; // cross replaces arrowhead
            }
            if msg.has_open_head {
                let arm_offset = if msg.is_left { 10.0 } else { -10.0 };
                let arm_x = tip_x + arm_offset;
                let skip_top = if msg.is_left {
                    matches!(msg.arrow_head, SeqArrowHead::HalfBottom)
                } else {
                    matches!(msg.arrow_head, SeqArrowHead::HalfTop)
                };
                let skip_bottom = if msg.is_left {
                    matches!(msg.arrow_head, SeqArrowHead::HalfTop)
                } else {
                    matches!(msg.arrow_head, SeqArrowHead::HalfBottom)
                };
                if !skip_top {
                    emit_line(
                        sg,
                        tip_x,
                        msg.y,
                        arm_x,
                        msg.y - 4.0,
                        &format!("stroke:{};stroke-width:{};", arrow_color, sw),
                        handwritten,
                    );
                }
                if !skip_bottom {
                    emit_line(
                        sg,
                        tip_x,
                        msg.y,
                        arm_x,
                        msg.y + 4.0,
                        &format!("stroke:{};stroke-width:{};", arrow_color, sw),
                        handwritten,
                    );
                }
            } else {
                let arm_x = if msg.is_left {
                    tip_x + 10.0
                } else {
                    tip_x - 10.0
                };
                match msg.arrow_head {
                    SeqArrowHead::FilledHalfTop => {
                        let pts = vec![(arm_x, msg.y - 4.0), (tip_x, msg.y), (arm_x, msg.y)];
                        emit_polygon(
                            sg,
                            &pts,
                            arrow_color,
                            &format!("stroke:{};stroke-width:1;", arrow_color),
                            handwritten,
                        );
                    }
                    SeqArrowHead::FilledHalfBottom => {
                        let pts = vec![(arm_x, msg.y), (tip_x, msg.y), (arm_x, msg.y + 4.0)];
                        emit_polygon(
                            sg,
                            &pts,
                            arrow_color,
                            &format!("stroke:{};stroke-width:1;", arrow_color),
                            handwritten,
                        );
                    }
                    _ => {
                        let inner_x = if msg.is_left {
                            tip_x + 6.0
                        } else {
                            tip_x - 6.0
                        };
                        let pts = vec![
                            (arm_x, msg.y - 4.0),
                            (tip_x, msg.y),
                            (arm_x, msg.y + 4.0),
                            (inner_x, msg.y),
                        ];
                        emit_polygon(
                            sg,
                            &pts,
                            arrow_color,
                            &format!("stroke:{};stroke-width:1;", arrow_color),
                            handwritten,
                        );
                    }
                }
            }
        };

        // Compute cross positions
        let cross_from_x0 = if msg.is_left {
            msg.from_x - SPACE_CROSS_X - ARROW_DELTA_X - 2.0
        } else {
            msg.from_x + SPACE_CROSS_X + 1.0
        };
        let cross_to_x0 = if msg.is_left {
            tip_x + SPACE_CROSS_X
        } else {
            tip_x - SPACE_CROSS_X - ARROW_DELTA_X
        };

        // Draw left-side elements first, then right-side elements
        if msg.is_left {
            // is_left: to (arrowhead) is on LEFT, from (decorations) is on RIGHT
            // Left side = to: circle_to, cross_to, arrowhead
            if msg.circle_to {
                draw_circle(sg, msg.to_x);
            }
            if msg.cross_to {
                draw_cross(sg, cross_to_x0);
            }
            draw_arrowhead(sg);
            // Right side = from: circle_from, cross_from
            if msg.circle_from {
                draw_circle(sg, msg.from_x);
            }
            if msg.cross_from {
                draw_cross(sg, cross_from_x0);
            }
        } else {
            // is_left=false: from is on LEFT, to (arrowhead) is on RIGHT
            // Left side = from: circle_from, cross_from
            if msg.circle_from {
                draw_circle(sg, msg.from_x);
            }
            if msg.cross_from {
                draw_cross(sg, cross_from_x0);
            }
            // Right side = to: circle_to, cross_to, arrowhead
            if msg.circle_to {
                draw_circle(sg, msg.to_x);
            }
            if msg.cross_to {
                draw_cross(sg, cross_to_x0);
            }
            draw_arrowhead(sg);
        }
    }

    // Message line
    let dash_style = if msg.is_dashed {
        "stroke-dasharray:2,2;"
    } else {
        ""
    };
    // Compute line endpoints, adjusted for cross/arrowhead decorations
    let is_half_filled = matches!(
        msg.arrow_head,
        SeqArrowHead::FilledHalfTop | SeqArrowHead::FilledHalfBottom
    );
    let adjusted_x2 = if msg.cross_to {
        // CrossX at target: line ends at X center
        if msg.is_left {
            tip_x + SPACE_CROSS_X + ARROW_DELTA_X / 2.0
        } else {
            tip_x - SPACE_CROSS_X - ARROW_DELTA_X / 2.0
        }
    } else if msg.has_open_head || is_half_filled {
        if msg.is_left {
            msg.to_x + circle_to_shift
        } else {
            tip_x + 1.0
        }
    } else if msg.is_left {
        tip_x + 4.0
    } else {
        tip_x - 4.0
    };
    let adjusted_x1 = if msg.cross_from {
        // CrossX at source: line starts at X center
        if msg.is_left {
            line_x1 - 2.0 * SPACE_CROSS_X
        } else {
            line_x1 + 2.0 * SPACE_CROSS_X
        }
    } else if msg.bidirectional && !msg.cross_from {
        // Bidirectional: "from" arrowhead also insets the line by 4px
        if msg.is_left {
            // from is on right, arrowhead points right: line ends 4px left of from tip
            let from_tip = msg.from_x - 2.0 - circle_from_shift;
            from_tip - 4.0
        } else {
            // from is on left, arrowhead points left: line ends 4px right of from tip
            let from_tip = msg.from_x + 1.0 + circle_from_shift;
            from_tip + 4.0
        }
    } else {
        line_x1
    };
    // For left-pointing arrows, swap x1/x2 so smaller x comes first
    let (lx1, lx2) = if msg.is_left {
        (adjusted_x2, adjusted_x1)
    } else {
        (adjusted_x1, adjusted_x2)
    };
    emit_line(
        sg,
        lx1,
        msg.y,
        lx2,
        msg.y,
        &format!("stroke:{};stroke-width:{};{}", arrow_color, sw, dash_style),
        handwritten,
    );

    // Label text above the line — each line as a separate <text> element
    let has_text = !msg.text.is_empty() || msg.autonumber.is_some();
    if has_text {
        // Java ComponentRoseArrow: when direction2 is BOTH_DIRECTION or
        // RIGHT_TO_LEFT_REVERSE, textPos += arrowDeltaX.  This applies when:
        // - cross_from (dressing1.head = CROSSX, makes direction2 = BOTH)
        // - bidirectional (both dressing heads non-NONE, makes direction2 = BOTH)
        // Cross/bidir at source shifts text only for LeftToRight.
        let from_decoration_text_offset = if !msg.is_left && (msg.cross_from || msg.bidirectional) {
            ARROW_DELTA_X
        } else {
            0.0
        };
        let base_text_x = if msg.is_left {
            // Left arrow: text positioned after the left arrowhead.
            // Use base tip (without circle shift) — Java doesn't shift text for circles.
            let base_tip = msg.to_x + 1.0;
            base_tip + 16.0
        } else {
            // Java CommunicationExoTile: for boundary arrows text_delta_x
            // shifts text to remain at the participant-relative position
            // rather than the border edge position.
            msg.from_x + 7.0 + from_decoration_text_offset + msg.text_delta_x.max(0.0)
        };

        // If autonumber, compute the offset for message text (number is bold)
        let text_x = if let Some(ref num_str) = msg.autonumber {
            let num_w =
                font_metrics::text_width(num_str, msg_font_family, msg_font_size, true, false);
            base_text_x + num_w + 4.0
        } else {
            base_text_x
        };

        let msg_line_spacing =
            font_metrics::line_height(msg_font_family, msg_font_size, false, false);
        let num_lines = msg.text_lines.len().max(1);
        // When text has <sub>, the subscript extends below the baseline, adding
        // extra height below the text block. This shifts the text baseline up
        // relative to the arrow position. Superscript extends above but does
        // NOT shift the text baseline.
        let sub_extra = msg
            .text_lines
            .first()
            .map(|line| {
                crate::render::svg_richtext::creole_sub_extra_height(
                    line,
                    msg_font_family,
                    msg_font_size,
                )
            })
            .unwrap_or(0.0);
        let first_text_y = msg.y
            - (font_metrics::descent(msg_font_family, msg_font_size, false, false) + 2.0)
            - (num_lines as f64 - 1.0) * msg_line_spacing
            - sub_extra;

        // Draw autonumber as separate bold text element
        if let Some(ref num_str) = msg.autonumber {
            let num_tl =
                font_metrics::text_width(num_str, msg_font_family, msg_font_size, true, false);
            sg.set_fill_color(msg_text_color);
            sg.svg_text(
                num_str,
                base_text_x,
                first_text_y,
                Some(msg_svg_family),
                msg_font_size,
                Some("bold"),
                None,
                None,
                num_tl,
                LengthAdjust::Spacing,
                None,
                0,
                None,
            );
        }

        // Draw message text lines
        for (i, line) in msg.text_lines.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            let line_y = first_text_y + i as f64 * msg_line_spacing;
            if word_by_word {
                render_word_by_word(
                    sg,
                    line,
                    text_x,
                    line_y,
                    msg_font_family,
                    msg_svg_family,
                    msg_font_size,
                );
            } else {
                let mut tmp = String::new();
                render_creole_text(
                    &mut tmp,
                    line,
                    text_x,
                    line_y,
                    msg_line_spacing,
                    msg_text_color,
                    None,
                    &format!(r#"font-size="{msg_font_size}""#),
                );
                sg.push_raw(&tmp);
            }
        }
    }

    if !teoz_mode {
        sg.push_raw("</g>");
    }
}

fn draw_self_message(
    sg: &mut SvgGraphic,
    msg: &MessageLayout,
    arrow_color: &str,
    arrow_thickness: f64,
    msg_font_family: &str,
    msg_font_size: f64,
    from_idx: usize,
    msg_idx: usize,
    word_by_word: bool,
    teoz_mode: bool,
) {
    let sw = arrow_thickness as u32;
    let to_x = msg.to_x;
    let y = msg.y;
    let loop_height = 13.0;

    // Java: for reverse-define left self-messages, getMinX applies a flat -5
    // offset when level > 0. Line endpoints use x1_adj/x2_adj from drawLeftSide.
    // But circle/decoration positions need the full level shift.
    let left_level_shift = if msg.is_left && msg.active_level > 1 {
        (msg.active_level - 1) as f64 * 5.0
    } else {
        0.0
    };
    // from_x for line computations (no level shift — x1_adj handles it)
    let from_x = msg.from_x;
    let return_x = msg.self_return_x;
    // from_x for decorations (circles, arrowheads, text) that need level shift
    let from_x_decor = msg.from_x + left_level_shift;

    // Java teoz does not wrap messages in <g class="message">
    if !teoz_mode {
        sg.push_raw(&format!(
            r#"<g class="message" data-entity-1="part{}" data-entity-2="part{}" id="msg{}">"#,
            from_idx, from_idx, msg_idx,
        ));
    }

    // Java constants for circle decorations
    const DIAM_CIRCLE: f64 = 8.0;
    const THIN_CIRCLE: f64 = 1.5;

    // Draw circle decorations for self-messages
    // Circles are drawn at the activation bar edge - 0.5.
    // For left self-messages, from_x_decor includes left_level_shift.
    if msg.circle_from {
        let cx = from_x_decor - 0.5;
        let cy = y - 0.75;
        sg.push_raw(&format!(
            r##"<ellipse cx="{}" cy="{}" fill="#000000" rx="{}" ry="{}" style="stroke:{};stroke-width:{};"/>"##,
            fmt_coord(cx), fmt_coord(cy),
            fmt_coord(DIAM_CIRCLE / 2.0), fmt_coord(DIAM_CIRCLE / 2.0),
            arrow_color, fmt_coord(THIN_CIRCLE),
        ));
    }
    if msg.circle_to {
        let cx = from_x_decor - 0.5;
        let cy = (y + loop_height) - 0.75;
        sg.push_raw(&format!(
            r##"<ellipse cx="{}" cy="{}" fill="#000000" rx="{}" ry="{}" style="stroke:{};stroke-width:{};"/>"##,
            fmt_coord(cx), fmt_coord(cy),
            fmt_coord(DIAM_CIRCLE / 2.0), fmt_coord(DIAM_CIRCLE / 2.0),
            arrow_color, fmt_coord(THIN_CIRCLE),
        ));
    }

    let dash_style = if msg.is_dashed {
        "stroke-dasharray:2,2;"
    } else {
        ""
    };

    // Java constants for cross (X) decorations
    const SPACE_CROSS_X: f64 = 6.0;
    const ARROW_DELTA_X_SELF: f64 = 10.0;

    // 3-line self-message: horizontal out, vertical down, horizontal return
    let mut tmp = String::new();

    // For right self-messages: right→down→left (arrowhead points left)
    // For left self-messages: left→down→right (arrowhead points right)
    // `from_x` is the start point (at lifeline/activation edge)
    // `to_x` is the far end of the horizontal
    // `return_x` is the return line endpoint (at lifeline/activation edge)

    // Cross offsets: for left self-messages, Java uses level-based indent
    // (spaceCrossX + level*deltaSize) instead of simple 2*spaceCrossX.
    let level = msg.active_level;
    let extra_live_delta_indent = level as f64 * 5.0;
    let cross_from_offset = if msg.cross_from {
        if msg.is_left {
            SPACE_CROSS_X
                + if level > 0 {
                    extra_live_delta_indent
                } else {
                    SPACE_CROSS_X
                }
        } else {
            2.0 * SPACE_CROSS_X
        }
    } else {
        0.0
    };
    let cross_to_offset = if msg.cross_to {
        if msg.is_left {
            SPACE_CROSS_X
                + if level > 0 {
                    extra_live_delta_indent
                } else {
                    SPACE_CROSS_X
                }
        } else {
            2.0 * SPACE_CROSS_X
        }
    } else {
        0.0
    };
    // Circle offset: circle at from shifts outgoing line.
    // For bidirectional (arrowhead at outgoing), full shift (diam/2 + thin).
    // For non-bidirectional, only diam/2.
    let circle_from_line_offset = if msg.circle_from {
        if msg.bidirectional {
            DIAM_CIRCLE / 2.0 + THIN_CIRCLE
        } else {
            DIAM_CIRCLE / 2.0
        }
    } else {
        0.0
    };
    let circle_to_line_offset = if msg.circle_to {
        DIAM_CIRCLE / 2.0 + THIN_CIRCLE
    } else {
        0.0
    };

    // Java ComponentRoseSelfArrow.drawLeftSide x1/x2 adjustment based on deltaX1 and level.
    // deltaX1 = (levelIgnore - levelConsidere) * 5, level = levelIgnore.
    // x1 adjusts outgoing right endpoint, x2 adjusts return right endpoint.
    // Positive x1_adj means shift endpoint LEFT (away from activation bar).
    //
    // Java ComponentRoseSelfArrow.drawLeftSide x1/x2 adjustment.
    // These apply to LINE ENDPOINTS only (not decorations).
    // left_level_shift is used separately for decoration positions (circles, etc.).
    let dx = msg.delta_x1;
    let (x1_adj, x2_adj) = if msg.is_left {
        let delta_size = 5.0_f64;
        let extra_live_delta_indent = level as f64 * delta_size;
        if dx < 0.0 {
            // Activation starting: return line needs to accommodate new bar
            let x2a = if level > 0 {
                -extra_live_delta_indent
            } else {
                delta_size
            };
            (0.0, x2a)
        } else if dx > 0.0 {
            // Deactivation: outgoing line adjusts for shrinking bar
            let x1a = if level > 1 {
                delta_size - extra_live_delta_indent
            } else {
                0.0
            };
            let x2a = if level == 1 { -delta_size } else { 0.0 };
            (x1a, x2a)
        } else if level > 1 {
            // Same level, stacked bars: both lines shift to span bars
            let shift = -(extra_live_delta_indent - delta_size);
            (shift, shift)
        } else {
            (0.0, 0.0)
        }
    } else {
        (0.0, 0.0)
    };

    // Line 1: outgoing horizontal
    let (line1_x1, line1_x2) = if msg.is_left {
        // Left self-msg: Java drawLeftSide x1 computation.
        // For circle_from: x1 += diamCircle/2 - thinCircle + (head1==NONE ? thinCircle : 0)
        // For left self-msgs (reverseDefine), outgoing end always has head=NONE,
        // so circle_from shortens by full diamCircle/2 (= 4.0).
        let circle_from_outgoing_shift = if msg.circle_from {
            DIAM_CIRCLE / 2.0
        } else {
            0.0
        };
        (
            to_x,
            from_x - 1.0 - x1_adj - circle_from_outgoing_shift - cross_from_offset,
        )
    } else {
        // Right self-msg: outgoing goes right from lifeline
        // For bidirectional with circle, outgoing line starts at the arrowhead tip.
        // Without circles: from_x. With circles: return_x + circle_shift.
        let base = if msg.bidirectional {
            if msg.circle_from {
                return_x + circle_to_line_offset
            } else {
                from_x
            }
        } else {
            from_x + circle_from_line_offset
        };
        (base + cross_from_offset, to_x)
    };
    write!(
        tmp,
        r#"<line style="stroke:{color};stroke-width:{sw};{dash}" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y1}"/>"#,
        color = arrow_color,
        dash = dash_style,
        x1 = fmt_coord(line1_x1),
        x2 = fmt_coord(line1_x2),
        y1 = fmt_coord(y),
    )
    .unwrap();

    // Line 2: vertical down
    write!(
        tmp,
        r#"<line style="stroke:{color};stroke-width:{sw};{dash}" x1="{x}" x2="{x}" y1="{y1}" y2="{y2}"/>"#,
        color = arrow_color,
        dash = dash_style,
        x = fmt_coord(to_x),
        y1 = fmt_coord(y),
        y2 = fmt_coord(y + loop_height),
    )
    .unwrap();

    // Line 3: return horizontal
    let (line3_x1, line3_x2) = if msg.is_left {
        // Java: extraline = 1 only when dressing2 is FULL + NORMAL (Filled arrowhead)
        // and there's no cross replacing the arrowhead.
        // Half arrows and open heads do NOT get extraline.
        let extraline = if matches!(msg.arrow_head, SeqArrowHead::Filled)
            && !msg.has_open_head
            && !msg.cross_to
        {
            1.0
        } else {
            0.0
        };
        // Java drawLeftSide: return line shortened by cross_to, circle_to, and level adjustment
        (
            to_x,
            return_x - x2_adj - extraline - cross_to_offset - circle_to_line_offset,
        )
    } else {
        // Right self-msg: adjust for cross_to, circle_to, or open head
        let base_x1 = if msg.cross_to {
            // Cross replaces arrowhead: return starts after cross space
            return_x + cross_to_offset
        } else {
            let extraline_r = if msg.has_open_head { 1.0 } else { 0.0 };
            return_x - extraline_r + circle_to_line_offset
        };
        (base_x1, to_x)
    };
    write!(
        tmp,
        r#"<line style="stroke:{color};stroke-width:{sw};{dash}" x1="{x1}" x2="{x2}" y1="{y}" y2="{y}"/>"#,
        color = arrow_color,
        dash = dash_style,
        x1 = fmt_coord(line3_x1),
        x2 = fmt_coord(line3_x2),
        y = fmt_coord(y + loop_height),
    )
    .unwrap();

    // Cross (X) decoration at source (outgoing line) — drawn before arrowhead
    if msg.cross_from {
        let x0 = if msg.is_left {
            // Left self-msg: Java draws at (prefTextWidth - x1 - spaceCrossX/2)
            // which equals line1_x2 - spaceCrossX/2
            line1_x2 - SPACE_CROSS_X / 2.0
        } else {
            // Right self-msg: cross on outgoing (left side)
            from_x + SPACE_CROSS_X
        };
        let half = ARROW_DELTA_X_SELF / 2.0;
        write!(
            tmp,
            r#"<line style="stroke:{color};stroke-width:2;" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}"/>"#,
            color = arrow_color,
            x1 = fmt_coord(x0), x2 = fmt_coord(x0 + ARROW_DELTA_X_SELF),
            y1 = fmt_coord(y - half), y2 = fmt_coord(y + half),
        ).unwrap();
        write!(
            tmp,
            r#"<line style="stroke:{color};stroke-width:2;" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}"/>"#,
            color = arrow_color,
            x1 = fmt_coord(x0), x2 = fmt_coord(x0 + ARROW_DELTA_X_SELF),
            y1 = fmt_coord(y + half), y2 = fmt_coord(y - half),
        ).unwrap();
    }

    // Bidirectional: draw arrowhead at the outgoing (top) line, pointing toward participant
    if msg.bidirectional && !msg.cross_from {
        if msg.is_left {
            // Left self-msg bidirectional: outgoing arrowhead points RIGHT at from_x
            let bidir_tip = from_x - 1.0;
            let bidir_arm = bidir_tip - 10.0;
            let bidir_inner = bidir_tip - 6.0;
            write!(tmp, r#"<polygon fill="{color}" points="{},{},{},{},{},{},{},{}" style="stroke:{color};stroke-width:1;"/>"#,
                fmt_coord(bidir_arm), fmt_coord(y - 4.0),
                fmt_coord(bidir_tip), fmt_coord(y),
                fmt_coord(bidir_arm), fmt_coord(y + 4.0),
                fmt_coord(bidir_inner), fmt_coord(y),
                color = arrow_color).unwrap();
        } else {
            // Right self-msg bidirectional: outgoing arrowhead points LEFT
            // Without circles: tip at from_x. With circles: tip at return_x + circle_shift.
            let bidir_tip = if msg.circle_from {
                return_x + circle_to_line_offset
            } else {
                from_x
            };
            let bidir_arm = bidir_tip + 10.0;
            let bidir_inner = bidir_tip + 6.0;
            write!(tmp, r#"<polygon fill="{color}" points="{},{},{},{},{},{},{},{}" style="stroke:{color};stroke-width:1;"/>"#,
                fmt_coord(bidir_arm), fmt_coord(y - 4.0),
                fmt_coord(bidir_tip), fmt_coord(y),
                fmt_coord(bidir_arm), fmt_coord(y + 4.0),
                fmt_coord(bidir_inner), fmt_coord(y),
                color = arrow_color).unwrap();
        }
    }

    // Arrowhead or cross at return
    let ret_y = y + loop_height;

    if msg.cross_to {
        // Cross (X) at return line replaces arrowhead
        let x0 = if msg.is_left {
            // Java: (prefTextWidth - x2 - spaceCrossX/2) = line3_x2 - spaceCrossX/2
            // (x2 not adjusted by NORMAL/ASYNC post-increment since cross replaces arrow)
            line3_x2 - SPACE_CROSS_X / 2.0
        } else {
            from_x + SPACE_CROSS_X
        };
        let half = ARROW_DELTA_X_SELF / 2.0;
        write!(
            tmp,
            r#"<line style="stroke:{color};stroke-width:2;" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}"/>"#,
            color = arrow_color,
            x1 = fmt_coord(x0), x2 = fmt_coord(x0 + ARROW_DELTA_X_SELF),
            y1 = fmt_coord(ret_y - half), y2 = fmt_coord(ret_y + half),
        ).unwrap();
        write!(
            tmp,
            r#"<line style="stroke:{color};stroke-width:2;" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}"/>"#,
            color = arrow_color,
            x1 = fmt_coord(x0), x2 = fmt_coord(x0 + ARROW_DELTA_X_SELF),
            y1 = fmt_coord(ret_y + half), y2 = fmt_coord(ret_y - half),
        ).unwrap();
    } else if msg.is_left {
        // Left self-message: arrowhead points RIGHT at return
        // Java polygon vertex for full arrow: tip at (0,0) → line3_x2.
        // Half arrows and open heads: tip at (-1,0) → line3_x2 - 1.
        let is_full_filled =
            matches!(msg.arrow_head, SeqArrowHead::Filled) && !msg.has_open_head && !msg.cross_to;
        let tip_x = if is_full_filled {
            line3_x2
        } else {
            line3_x2 - 1.0
        };
        if msg.has_open_head {
            if !matches!(msg.arrow_head, SeqArrowHead::HalfBottom) {
                write!(
                    tmp,
                    r#"<line style="stroke:{color};stroke-width:{sw};" x1="{tx}" x2="{ax}" y1="{y}" y2="{y1}"/>"#,
                    color = arrow_color,
                    tx = fmt_coord(tip_x),
                    ax = fmt_coord(tip_x - 10.0),
                    y = fmt_coord(ret_y),
                    y1 = fmt_coord(ret_y - 4.0),
                )
                .unwrap();
            }
            if !matches!(msg.arrow_head, SeqArrowHead::HalfTop) {
                write!(
                    tmp,
                    r#"<line style="stroke:{color};stroke-width:{sw};" x1="{tx}" x2="{ax}" y1="{y}" y2="{y1}"/>"#,
                    color = arrow_color,
                    tx = fmt_coord(tip_x),
                    ax = fmt_coord(tip_x - 10.0),
                    y = fmt_coord(ret_y),
                    y1 = fmt_coord(ret_y + 4.0),
                )
                .unwrap();
            }
        } else if matches!(msg.arrow_head, SeqArrowHead::FilledHalfTop) {
            // 3-point triangle: top half only (flat bottom at ret_y)
            write!(
                tmp,
                r#"<polygon fill="{color}" points="{p1x},{p1y},{p2x},{p2y},{p3x},{p3y}" style="stroke:{color};stroke-width:1;"/>"#,
                color = arrow_color,
                p1x = fmt_coord(tip_x - 10.0),
                p1y = fmt_coord(ret_y - 4.0),
                p2x = fmt_coord(tip_x),
                p2y = fmt_coord(ret_y),
                p3x = fmt_coord(tip_x - 10.0),
                p3y = fmt_coord(ret_y),
            )
            .unwrap();
        } else if matches!(msg.arrow_head, SeqArrowHead::FilledHalfBottom) {
            // 3-point triangle: bottom half only (flat top at ret_y)
            write!(
                tmp,
                r#"<polygon fill="{color}" points="{p1x},{p1y},{p2x},{p2y},{p3x},{p3y}" style="stroke:{color};stroke-width:1;"/>"#,
                color = arrow_color,
                p1x = fmt_coord(tip_x - 10.0),
                p1y = fmt_coord(ret_y),
                p2x = fmt_coord(tip_x),
                p2y = fmt_coord(ret_y),
                p3x = fmt_coord(tip_x - 10.0),
                p3y = fmt_coord(ret_y + 4.0),
            )
            .unwrap();
        } else {
            // Full 4-point diamond arrowhead
            write!(
                tmp,
                r#"<polygon fill="{color}" points="{p1x},{p1y},{p2x},{p2y},{p3x},{p3y},{p4x},{p4y}" style="stroke:{color};stroke-width:1;"/>"#,
                color = arrow_color,
                p1x = fmt_coord(tip_x - 10.0),
                p1y = fmt_coord(ret_y - 4.0),
                p2x = fmt_coord(tip_x),
                p2y = fmt_coord(ret_y),
                p3x = fmt_coord(tip_x - 10.0),
                p3y = fmt_coord(ret_y + 4.0),
                p4x = fmt_coord(tip_x - 6.0),
                p4y = fmt_coord(ret_y),
            )
            .unwrap();
        }
    } else {
        // Right self-message: arrowhead points LEFT at return.
        let is_half = matches!(
            msg.arrow_head,
            SeqArrowHead::HalfTop
                | SeqArrowHead::HalfBottom
                | SeqArrowHead::FilledHalfTop
                | SeqArrowHead::FilledHalfBottom
        );
        let tip_x = if matches!(
            msg.arrow_head,
            SeqArrowHead::FilledHalfTop | SeqArrowHead::FilledHalfBottom
        ) {
            return_x - 1.0 + circle_to_line_offset
        } else {
            return_x + circle_to_line_offset
        };
        if msg.has_open_head {
            let skip_top = if is_half {
                matches!(msg.arrow_head, SeqArrowHead::HalfTop)
            } else {
                false
            };
            let skip_bottom = if is_half {
                matches!(msg.arrow_head, SeqArrowHead::HalfBottom)
            } else {
                false
            };
            if !skip_top {
                write!(
                    tmp,
                    r#"<line style="stroke:{color};stroke-width:{sw};" x1="{tx}" x2="{ax}" y1="{y}" y2="{y1}"/>"#,
                    color = arrow_color,
                    tx = fmt_coord(tip_x),
                    ax = fmt_coord(tip_x + 10.0),
                    y = fmt_coord(ret_y),
                    y1 = fmt_coord(ret_y - 4.0),
                )
                .unwrap();
            }
            if !skip_bottom {
                write!(
                    tmp,
                    r#"<line style="stroke:{color};stroke-width:{sw};" x1="{tx}" x2="{ax}" y1="{y}" y2="{y1}"/>"#,
                    color = arrow_color,
                    tx = fmt_coord(tip_x),
                    ax = fmt_coord(tip_x + 10.0),
                    y = fmt_coord(ret_y),
                    y1 = fmt_coord(ret_y + 4.0),
                )
                .unwrap();
            }
        } else {
            let arm_x = tip_x + 10.0;
            match msg.arrow_head {
                SeqArrowHead::FilledHalfTop => {
                    write!(tmp, r#"<polygon fill="{color}" points="{},{},{},{},{},{}" style="stroke:{color};stroke-width:1;"/>"#,
                        fmt_coord(arm_x), fmt_coord(ret_y - 4.0),
                        fmt_coord(tip_x), fmt_coord(ret_y),
                        fmt_coord(arm_x), fmt_coord(ret_y),
                        color = arrow_color).unwrap();
                }
                SeqArrowHead::FilledHalfBottom => {
                    write!(tmp, r#"<polygon fill="{color}" points="{},{},{},{},{},{}" style="stroke:{color};stroke-width:1;"/>"#,
                        fmt_coord(arm_x), fmt_coord(ret_y),
                        fmt_coord(tip_x), fmt_coord(ret_y),
                        fmt_coord(arm_x), fmt_coord(ret_y + 4.0),
                        color = arrow_color).unwrap();
                }
                _ => {
                    write!(tmp, r#"<polygon fill="{color}" points="{},{},{},{},{},{},{},{}" style="stroke:{color};stroke-width:1;"/>"#,
                        fmt_coord(arm_x), fmt_coord(ret_y - 4.0),
                        fmt_coord(tip_x), fmt_coord(ret_y),
                        fmt_coord(arm_x), fmt_coord(ret_y + 4.0),
                        fmt_coord(tip_x + 6.0), fmt_coord(ret_y),
                        color = arrow_color).unwrap();
                }
            }
        }
    }
    sg.push_raw(&tmp);

    // Label text above the first horizontal line — each line as separate <text>
    if !msg.text.is_empty() {
        let text_x = if msg.is_left {
            // Left self-message: text starts at from_x - preferredWidth + marginX1(7).
            // For activated participants, from_x is the activation bar left edge.
            let text_w = msg
                .text_lines
                .iter()
                .map(|line| {
                    crate::render::svg_richtext::creole_text_width(
                        line,
                        msg_font_family,
                        msg_font_size,
                        false,
                        false,
                    )
                })
                .fold(0.0_f64, f64::max);
            let preferred = f64::max(text_w + 14.0, crate::skin::rose::SELF_ARROW_WIDTH + 5.0);
            // Use unshifted from_x for text (Java: text at origin + marginX1,
            // not level-shifted)
            msg.from_x - preferred + 7.0
        } else {
            return_x + 6.0
        };
        let msg_line_spacing =
            font_metrics::line_height(msg_font_family, msg_font_size, false, false);
        let num_lines = msg.text_lines.len();
        let sub_extra = msg
            .text_lines
            .first()
            .map(|line| {
                crate::render::svg_richtext::creole_sub_extra_height(
                    line,
                    msg_font_family,
                    msg_font_size,
                )
            })
            .unwrap_or(0.0);
        let first_text_y = y
            - (font_metrics::descent(msg_font_family, msg_font_size, false, false) + 2.0)
            - (num_lines as f64 - 1.0) * msg_line_spacing
            - sub_extra;
        for (i, line) in msg.text_lines.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            let line_y = first_text_y + i as f64 * msg_line_spacing;
            if word_by_word {
                let svg_family = svg_font_family_attr(msg_font_family);
                render_word_by_word(
                    sg,
                    line,
                    text_x,
                    line_y,
                    msg_font_family,
                    svg_family,
                    msg_font_size,
                );
            } else {
                let mut tmp = String::new();
                render_creole_text(
                    &mut tmp,
                    line,
                    text_x,
                    line_y,
                    msg_line_spacing,
                    TEXT_COLOR,
                    None,
                    &format!(r#"font-size="{msg_font_size}""#),
                );
                sg.push_raw(&tmp);
            }
        }
    }

    if !teoz_mode {
        sg.push_raw("</g>");
    }
}

// ── Activation bars ─────────────────────────────────────────────────

fn draw_activation(
    sg: &mut SvgGraphic,
    act: &ActivationLayout,
    title: &str,
    shadow_attr: &str,
    border_color: &str,
) {
    let width = 10.0;
    let height = act.y_end - act.y_start;

    let mut tmp = String::new();
    // Java: empty Display → <title/>, non-empty → <title>name</title>
    let title_tag = if title.is_empty() {
        "<title/>".to_string()
    } else {
        format!("<title>{}</title>", encode_metadata_title(title))
    };
    // Use custom activation color if provided, otherwise default
    let bg = act.color.as_deref().unwrap_or(ACTIVATION_BG);
    write!(
        tmp,
        r#"<g>{title_tag}<rect fill="{bg}"{shadow} height="{}" style="stroke:{border};stroke-width:1;" width="{}" x="{}" y="{}"/></g>"#,
        fmt_coord(height),
        fmt_coord(width),
        fmt_coord(act.x),
        fmt_coord(act.y_start),
        title_tag = title_tag,
        bg = bg,
        border = border_color,
        shadow = shadow_attr,
    )
    .unwrap();
    sg.push_raw(&tmp);
}

// ── Destroy marker ──────────────────────────────────────────────────

fn draw_destroy(sg: &mut SvgGraphic, d: &DestroyLayout) {
    let size = 9.0;
    let style = DrawStyle::outline(DESTROY_COLOR, 2.0);
    // First diagonal: top-left to bottom-right
    LineShape {
        x1: d.x - size,
        y1: d.y - size,
        x2: d.x + size,
        y2: d.y + size,
    }
    .draw(sg, &style);

    // Second diagonal: bottom-left to top-right (matching Java PlantUML order)
    LineShape {
        x1: d.x - size,
        y1: d.y + size,
        x2: d.x + size,
        y2: d.y - size,
    }
    .draw(sg, &style);
}

// ── Notes ───────────────────────────────────────────────────────────

fn draw_note(sg: &mut SvgGraphic, note: &NoteLayout, shadow_attr: &str, skin: &SkinParams) {
    let fold = 10.0; // folded corner size
                     // Java NoteBox.getStartingX uses (int) truncation, and AbstractComponent.drawU
                     // applies UTranslate(paddingX, paddingY). Self-msg notes have this baked into
                     // note.x during layout. For non-self notes in classic mode, truncation is
                     // applied here to match Java's rendering.
    let x = if note.teoz_mode || note.is_self_msg_note {
        note.x
    } else {
        note.x.trunc()
    };
    let y = note.y;
    // Java truncates polygon width to int in ComponentRoseNote.drawInternalU():
    //   int x2 = (int) getTextWidth(stringBounder)
    let w = note.width.trunc();
    let h = note.height;

    // Note colors: per-note override > skinparam > constant default
    let skin_bg = skin.background_color("note", NOTE_BG);
    let bg = note.color.as_deref().unwrap_or(skin_bg);
    let border = skin.border_color("note", NOTE_BORDER);
    let stroke_w = skin.line_thickness("note", 0.5);

    // Body: hexagonal path with folded top-right corner (Java: Opale.getPolygonNormal)
    {
        let x0 = fmt_coord(x);
        let y0 = fmt_coord(y);
        let x1 = fmt_coord(x + w);
        let y1 = fmt_coord(y + h);
        let xf = fmt_coord(x + w - fold);
        let yf = fmt_coord(y + fold);
        sg.push_raw(&format!(
            "<path d=\"M{x0},{y0} L{x0},{y1} L{x1},{y1} L{x1},{yf} L{xf},{y0} L{x0},{y0}\" fill=\"{bg}\"{shadow} style=\"stroke:{border};stroke-width:{stroke_w};\"/>",
            bg = bg,
            border = border,
            shadow = shadow_attr,
            stroke_w = stroke_w,
        ));
    }

    // Fold corner triangle (Java: Opale.getCorner)
    {
        let cx_s = fmt_coord(x + w - fold);
        let cy_s = fmt_coord(y);
        let cy2 = fmt_coord(y + fold);
        let cx2 = fmt_coord(x + w);
        sg.push_raw(&format!(
            "<path d=\"M{cx_s},{cy_s} L{cx_s},{cy2} L{cx2},{cy2} L{cx_s},{cy_s}\" fill=\"{bg}\" style=\"stroke:{border};stroke-width:{stroke_w};\"/>",
            bg = bg,
            border = border,
            stroke_w = stroke_w,
        ));
    }

    // Render note content with proper block-level creole (bullets, tables, hrules).
    // HR lines use the un-truncated width for stencil-based rendering (Java
    // SheetBlock2 stencil uses marginX1+textBlock.width+marginX2, not truncated).
    let mut tmp = String::new();
    render_creole_note_content(
        &mut tmp, &note.text, x, y, note.width, // un-truncated for HR stencil
        TEXT_COLOR, FONT_SIZE, border,
    );
    sg.push_raw(&tmp);
}

// ── Group frames ────────────────────────────────────────────────────

fn draw_group(sg: &mut SvgGraphic, group: &GroupLayout) {
    let height = group.y_end - group.y_start;

    // Frame rectangle
    let mut tmp = String::new();
    write!(
        tmp,
        r#"<rect fill="{bg}" fill-opacity="0.30000" height="{}" style="stroke:{border};stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
        fmt_coord(height), fmt_coord(group.width), fmt_coord(group.x), fmt_coord(group.y_start),
        bg = GROUP_BG,
        border = TEXT_COLOR,
    )
    .unwrap();
    sg.push_raw(&tmp);
    sg.push_raw("\n");

    // Label in top-left corner
    if let Some(label) = &group.label {
        let label_x = group.x + 6.0;
        let label_y = group.y_start + FONT_SIZE + 2.0;

        // Label background tab
        let label_width =
            font_metrics::text_width(label, "SansSerif", FONT_SIZE, true, false) + 12.0;
        let label_height = FONT_SIZE + 6.0;
        let mut tmp = String::new();
        write!(
            tmp,
            r#"<rect fill="{bg}" height="{}" style="stroke:{border};stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
            fmt_coord(label_height), fmt_coord(label_width), fmt_coord(group.x), fmt_coord(group.y_start),
            bg = GROUP_BG,
            border = TEXT_COLOR,
        )
        .unwrap();
        sg.push_raw(&tmp);
        sg.push_raw("\n");

        let tl = font_metrics::text_width(label, "SansSerif", FONT_SIZE, true, false);
        sg.set_fill_color(TEXT_COLOR);
        sg.svg_text(
            label,
            label_x,
            label_y,
            Some("sans-serif"),
            FONT_SIZE,
            Some("bold"),
            None,
            None,
            tl,
            LengthAdjust::Spacing,
            None,
            0,
            None,
        );
        sg.push_raw("\n");
    }
}

// ── Fragment frames ──────────────────────────────────────────────────

/// Draw complete fragment: pentagon tab, frame rect, labels, and separators.
/// Java GroupingTile.drawU() draws header, then drawAllElses(), then child tiles.
fn draw_fragment_details(sg: &mut SvgGraphic, frag: &FragmentLayout) {
    let fx = fmt_coord(frag.x);
    let fy = fmt_coord(frag.y);
    let fw = fmt_coord(frag.width);
    let fh = fmt_coord(frag.height);

    // Background color rects are drawn early (before lifelines) in
    // the render_sequence_inner step 3 to match Java's element order.

    // Label tab (pentagon in top-left)
    // For Group, the tab shows the label directly; for others, tab shows the keyword
    let is_group = frag.kind == FragmentKind::Group;
    let tab_text = if is_group && !frag.label.is_empty() {
        frag.label.clone()
    } else {
        frag.kind.label().to_string()
    };
    let tab_text_w = font_metrics::text_width(&tab_text, "SansSerif", FONT_SIZE, true, false);
    let tab_right = frag.x + FRAG_TAB_LEFT_PAD + tab_text_w + FRAG_TAB_RIGHT_PAD;

    // Pentagon path
    sg.push_raw(&format!(
        "<path d=\"M{fx},{fy} L{},{fy} L{},{} L{},{} L{fx},{} L{fx},{fy}\" fill=\"#EEEEEE\" style=\"stroke:#000000;stroke-width:1.5;\"/>",
        fmt_coord(tab_right),
        fmt_coord(tab_right), fmt_coord(frag.y + frag_tab_height() - FRAG_TAB_NOTCH),
        fmt_coord(tab_right - FRAG_TAB_NOTCH), fmt_coord(frag.y + frag_tab_height()),
        fmt_coord(frag.y + frag_tab_height()),
    ));

    // Second frame rect (Java emits two)
    sg.push_raw(&format!(
        "<rect fill=\"none\" height=\"{fh}\" style=\"stroke:#000000;stroke-width:1.5;\" width=\"{fw}\" x=\"{fx}\" y=\"{fy}\"/>"
    ));

    // Tab label text (font-size 13, bold)
    let text_x = frag.x + FRAG_TAB_LEFT_PAD;
    let text_y = frag.y + FRAG_KIND_LABEL_Y_OFFSET;
    sg.set_fill_color("#000000");
    sg.svg_text(
        &tab_text,
        text_x,
        text_y,
        Some("sans-serif"),
        13.0,
        Some("bold"),
        None,
        None,
        tab_text_w,
        LengthAdjust::Spacing,
        None,
        0,
        None,
    );

    // Guard text (font-size 11, bold) — only for non-Group fragments
    if !is_group && !frag.label.is_empty() {
        let guard_text = format!("[{}]", frag.label);
        let guard_w =
            font_metrics::text_width(&guard_text, "SansSerif", FRAG_GUARD_FONT_SIZE, true, false);
        let guard_x = tab_right + FRAG_GUARD_GAP;
        let guard_y = frag.y + frag_guard_label_y_offset();
        sg.set_fill_color("#000000");
        sg.svg_text(
            &guard_text,
            guard_x,
            guard_y,
            Some("sans-serif"),
            FRAG_GUARD_FONT_SIZE,
            Some("bold"),
            None,
            None,
            guard_w,
            LengthAdjust::Spacing,
            None,
            0,
            None,
        );
    }

    // For teoz fragments (first_msg_index set), draw separators as part of the
    // fragment detail. Java GroupingTile.drawU() calls drawAllElses() right after
    // the header, before any child tiles.
    if frag.first_msg_index.is_some() {
        for (sep_y, sep_label) in &frag.separators {
            draw_fragment_separator(sg, frag, *sep_y, sep_label, true);
        }
    }
}

/// Draw a single separator line + label within a fragment
///
/// In Java teoz, the else separator component is drawn at `tile_y + MARGINY_MAGIC/2`.
/// Inside the component, the dashed line is at dy=1, and the text is at
/// `(marginY + 2, marginX1)` = `(3, 5)`.  So:
///   line absolute Y = tile_y + 10 + 1 = tile_y + 11
///   text absolute Y = tile_y + 10 + 3 + ascent_11 = tile_y + 10 + 3 + ascent(11pt)
fn draw_fragment_separator(
    sg: &mut SvgGraphic,
    frag: &FragmentLayout,
    sep_y: f64,
    sep_label: &str,
    teoz: bool,
) {
    // In teoz mode, the stored sep_y is the raw tile Y.
    // Java: ComponentRoseGroupingElse draws dashed line at component_y + 1
    // component_y = tile_y + MARGINY_MAGIC/2 = tile_y + 10
    // So line_y = tile_y + 11
    // In puma mode, sep_y is already the correct line Y.
    let line_y = if teoz { sep_y + 11.0 } else { sep_y };
    let fx = fmt_coord(frag.x);
    let y_s = fmt_coord(line_y);
    sg.push_raw(&format!(
        "<line style=\"stroke:#000000;stroke-width:1;stroke-dasharray:2,2;\" x1=\"{fx}\" x2=\"{}\" y1=\"{y_s}\" y2=\"{y_s}\"/>",
        fmt_coord(frag.x + frag.width),
    ));

    if !sep_label.is_empty() {
        let bracket_text = format!("[{sep_label}]");
        let sep_tl = font_metrics::text_width(&bracket_text, "SansSerif", 11.0, true, false);
        let label_x = frag.x + 5.0;
        let label_y = if teoz {
            // Java teoz: text at (marginX1=5, marginY+2=3) within component.
            // component_y = tile_y + 10. Text SVG y = component_y + 3 + ascent_11.
            let ascent_11 = font_metrics::ascent("SansSerif", 11.0, true, false);
            sep_y + 10.0 + 3.0 + ascent_11
        } else {
            sep_y + font_metrics::ascent("SansSerif", 11.0, true, false)
        };
        sg.set_fill_color("#000000");
        sg.svg_text(
            &bracket_text,
            label_x,
            label_y,
            Some("sans-serif"),
            11.0,
            Some("bold"),
            None,
            None,
            sep_tl,
            LengthAdjust::Spacing,
            None,
            0,
            None,
        );
    }
}

// ── Divider ──────────────────────────────────────────────────────────

/// Draw a divider. Java: ComponentRoseDivider.drawInternalU
///
/// The divider component draws relative to its component origin (component_y),
/// which corresponds to Java's startingY (= freeY at divider creation time).
fn draw_divider(sg: &mut SvgGraphic, divider: &DividerLayout) {
    // Java: center_y = component_y + area.height / 2
    let center_y = divider.component_y + divider.height / 2.0;

    // Divider colors: Java default rose.skin separator style
    // background = #EEEEEE, borderColor = #000000
    let bg_color = "#EEEEEE";
    let border_color = "#000000";

    // Java: drawRectLong at center - 1, height=3, stroke=simple(bg), fill=bg
    let rect_y = center_y - 1.0;
    let mut tmp = String::new();
    write!(
        tmp,
        r#"<rect fill="{bg}" height="3" style="stroke:{bg};stroke-width:1;" width="{w}" x="{x}" y="{y}"/>"#,
        bg = bg_color,
        w = fmt_coord(divider.width),
        x = fmt_coord(divider.x),
        y = fmt_coord(rect_y),
    )
    .unwrap();
    sg.push_raw(&tmp);

    // Java: drawDoubleLine - two lines at center-1 and center+2, stroke=borderColor
    {
        let mut tmp = String::new();
        write!(
            tmp,
            r#"<line style="stroke:{color};stroke-width:1;" x1="{x1}" x2="{x2}" y1="{y}" y2="{y}"/>"#,
            color = border_color,
            x1 = fmt_coord(divider.x),
            x2 = fmt_coord(divider.x + divider.width),
            y = fmt_coord(center_y - 1.0),
        )
        .unwrap();
        sg.push_raw(&tmp);
    }
    {
        let mut tmp = String::new();
        write!(
            tmp,
            r#"<line style="stroke:{color};stroke-width:1;" x1="{x1}" x2="{x2}" y1="{y}" y2="{y}"/>"#,
            color = border_color,
            x1 = fmt_coord(divider.x),
            x2 = fmt_coord(divider.x + divider.width),
            y = fmt_coord(center_y + 2.0),
        )
        .unwrap();
        sg.push_raw(&tmp);
    }

    // Centered label text with bordered rect
    if let Some(text) = &divider.text {
        // Java: textHeight = textBlock.height + 2*marginY(4)
        // For single-line: textBlock.height = line_height(13) = 15.1328
        // textHeight = 15.1328 + 8 = 23.1328
        let text_line_h = font_metrics::line_height("SansSerif", FONT_SIZE, false, false);
        let margin_y = 4.0;
        let text_height = text_line_h + 2.0 * margin_y;

        // Java: textWidth = textBlock.width + marginX1(4) + marginX2(4)
        // Java's divider regex captures the label with a trailing space
        // (e.g., "Initialization " from "== Initialization =="), so the
        // textBlock dimension includes the space advance. The SVG text
        // rendering trims it, but the rect size uses the untrimmed width.
        let text_with_space = format!("{} ", text);
        let text_block_w =
            font_metrics::text_width(&text_with_space, "SansSerif", FONT_SIZE, true, false);
        let text_width = text_block_w + 4.0 + 4.0;
        let delta_x = 6.0;

        // Position centered in area
        let _xpos = divider.component_y; // dummy, we compute from area
        let area_width = divider.width;
        let rect_x = (area_width - text_width - delta_x) / 2.0 + divider.x;
        let rect_y = divider.component_y + (divider.height - text_height) / 2.0;

        // Java: rect with stroke=borderColor, stroke-width=2 (UStroke default)
        let mut tmp = String::new();
        write!(
            tmp,
            r#"<rect fill="{bg}" height="{h}" style="stroke:{border};stroke-width:2;" width="{w}" x="{x}" y="{y}"/>"#,
            bg = bg_color,
            h = fmt_coord(text_height),
            w = fmt_coord(text_width + delta_x),
            x = fmt_coord(rect_x),
            y = fmt_coord(rect_y),
            border = border_color,
        )
        .unwrap();
        sg.push_raw(&tmp);

        // Java: textBlock drawn at (xpos + deltaX, ypos + marginY)
        let text_x = rect_x + delta_x;
        let text_baseline_y =
            rect_y + margin_y + font_metrics::ascent("SansSerif", FONT_SIZE, true, false);
        let tl = font_metrics::text_width(text, "SansSerif", FONT_SIZE, true, false);
        sg.set_fill_color(TEXT_COLOR);
        sg.svg_text(
            text,
            text_x,
            text_baseline_y,
            Some("sans-serif"),
            FONT_SIZE,
            Some("bold"),
            None,
            None,
            tl,
            LengthAdjust::Spacing,
            None,
            0,
            None, // left-aligned (not centered)
        );
    }
}

// ── Delay ────────────────────────────────────────────────────────────

/// Draw delay text. Java: ComponentRoseDelayText.drawInternalU + GraphicalDelayText
///
/// The delay text is centered between the first and last participant's
/// lifeline positions. The dotted lifeline is handled by lifeline splitting.
fn draw_delay(sg: &mut SvgGraphic, delay: &DelayLayout, layout: &SeqLayout) {
    // Java: ComponentRoseDelayText only draws text, no dots/circles.
    if let Some(text) = &delay.text {
        let tl = font_metrics::text_width(text, "SansSerif", DELAY_FONT_SIZE, false, false);

        // Java: GraphicalDelayText computes middle from first/last participant getCenterX.
        // getCenterX = startingX + head.preferredWidth/2.0 + outMargin (exact, no integer div).
        // Our p.x corresponds to getCenterX.
        let first_p = layout.participants.first();
        let last_p = layout.participants.last();
        let mid_x = match (first_p, last_p) {
            (Some(fp), Some(lp)) => (fp.x + lp.x) / 2.0,
            _ => delay.x + delay.width / 2.0,
        };
        let text_x = mid_x - tl / 2.0;

        // Y position: centered in component area, then offset by marginY + ascent
        let text_line_h = font_metrics::line_height("SansSerif", DELAY_FONT_SIZE, false, false);
        let margin_y = 4.0;
        let text_height = text_line_h + 2.0 * margin_y;
        let ypos = (delay.height - text_height) / 2.0;
        let text_y = delay.lifeline_break_y
            + ypos
            + margin_y
            + font_metrics::ascent("SansSerif", DELAY_FONT_SIZE, false, false);

        sg.set_fill_color(TEXT_COLOR);
        sg.svg_text(
            text,
            text_x,
            text_y,
            Some("sans-serif"),
            DELAY_FONT_SIZE,
            None,
            None,
            None,
            tl,
            LengthAdjust::Spacing,
            None,
            0,
            None,
        );
    }
}

// ── Ref ──────────────────────────────────────────────────────────────

fn draw_ref(sg: &mut SvgGraphic, r: &RefLayout) {
    let ref_text_w = font_metrics::text_width("ref", "SansSerif", FONT_SIZE, true, false);
    let tab_text_w_int = ref_text_w.floor();
    let tab_right = r.x + FRAG_TAB_LEFT_PAD + tab_text_w_int + FRAG_TAB_RIGHT_PAD;
    let rx_s = fmt_coord(r.x);
    let ry_s = fmt_coord(r.y);
    sg.push_raw(&format!(
        "<rect fill=\"none\" height=\"{}\" style=\"stroke:{REF_FRAME_STROKE};stroke-width:1.5;\" width=\"{}\" x=\"{rx_s}\" y=\"{ry_s}\"/>",
        fmt_coord(r.height), fmt_coord(r.width),
    ));
    sg.push_raw(&format!(
        "<path d=\"M{rx_s},{ry_s} L{},{ry_s} L{},{} L{},{} L{rx_s},{} L{rx_s},{ry_s}\" fill=\"{GROUP_BG}\" style=\"stroke:{REF_FRAME_STROKE};stroke-width:2;\"/>",
        fmt_coord(tab_right),
        fmt_coord(tab_right), fmt_coord(r.y + REF_TAB_HEIGHT - REF_TAB_NOTCH),
        fmt_coord(tab_right - REF_TAB_NOTCH), fmt_coord(r.y + REF_TAB_HEIGHT),
        fmt_coord(r.y + REF_TAB_HEIGHT),
    ));
    sg.set_fill_color(TEXT_COLOR);
    sg.svg_text(
        "ref",
        r.x + REF_TAB_LEFT_PAD,
        r.y + REF_KIND_LABEL_Y_OFFSET,
        Some("sans-serif"),
        FONT_SIZE,
        Some("bold"),
        None,
        None,
        ref_text_w,
        LengthAdjust::Spacing,
        None,
        0,
        None,
    );
    let label_w =
        font_metrics::text_width(&r.label, "SansSerif", REF_LABEL_FONT_SIZE, false, false);
    let center_x = r.x + r.width / 2.0;
    let label_x = center_x - label_w / 2.0;
    let body_top = r.y + REF_TAB_HEIGHT;
    let body_height = r.height - REF_TAB_HEIGHT;
    let line_h = font_metrics::line_height("SansSerif", REF_LABEL_FONT_SIZE, false, false);
    let asc = font_metrics::ascent("SansSerif", REF_LABEL_FONT_SIZE, false, false);
    let top_margin = ((body_height - line_h) / 2.0).floor();
    let label_y = body_top + top_margin + asc;
    sg.set_fill_color(TEXT_COLOR);
    sg.svg_text(
        &r.label,
        label_x,
        label_y,
        Some("sans-serif"),
        REF_LABEL_FONT_SIZE,
        None,
        None,
        None,
        label_w,
        LengthAdjust::Spacing,
        None,
        0,
        None,
    );
}

// ── Public entry point ──────────────────────────────────────────────

/// Build a mapping from participant name -> 1-based index for data-entity-uid.
fn build_participant_index(sd: &SequenceDiagram) -> std::collections::HashMap<String, usize> {
    sd.participants
        .iter()
        .enumerate()
        .map(|(i, p)| (p.name.clone(), i + 1))
        .collect()
}

/// Draw the handwritten-mode warning banner.
///
/// Java renders this at the `ImageBuilder` level **before** enabling the
/// handwritten UGraphic wrapper, so it produces a plain `<rect>` — not a
/// jiggled polygon.  The rect width is `diagramBodyWidth - 10` (matching
/// the main diagram width) and the text baseline is calculated from the
/// font metrics.  We need to consume RNG calls so that subsequent
/// handwritten shapes stay deterministic.
fn draw_handwritten_banner(sg: &mut SvgGraphic, diagram_width: f64) {
    let banner_text = "Please use '!option handwritten true' to enable handwritten";
    let text_w = font_metrics::text_width(banner_text, "Monospaced", 10.0, false, false);
    let line_h = font_metrics::line_height("Monospaced", 10.0, false, false);
    // Java: dimWarning = textDim.delta(10, 5) → height = lineHeight + 5
    //        rect height = dimWarning.getHeight() + 10 = lineHeight + 15
    let rect_h = line_h + 15.0;
    // Java: URectangle.build(fullWidth - 10, ...).rounded(5)
    // fullWidth = dim.getWidth() = diagram body content width.
    // In the final SVG, the rect width is diagramBodyWidth - 10.
    let rect_w = diagram_width - 10.0;
    let text_x = 10.0_f64;
    // Java: text baseline = UTranslate.dy(5) + font ascent at Monospaced 10.
    // Reference output shows y="20", which is 5 (dy) + 15 (ascent baseline).
    let text_y = 20.0_f64;

    // Consume the RNG calls that the old polygon code would have made,
    // so that participant/arrow jiggle stays in sync with Java.
    // Java: the banner is drawn BEFORE handwriting is enabled, so
    // no RNG consumption for the banner rect.  But we created the RNG
    // before drawing the banner.  Actually in Java, the seed is computed
    // from the diagram source and the handwritten UG is created AFTER
    // the banner.  Our RNG was just created, no calls consumed yet.
    // The rect_to_hand_polygon would consume calls — we must NOT
    // consume them to stay in sync with Java.
    // (No RNG calls here — the banner is plain.)

    let h_str = fmt_coord(rect_h);
    let w_str = fmt_coord(rect_w);
    sg.push_raw(&format!(
        "<rect fill=\"#FFFFCC\" height=\"{h_str}\" rx=\"2.5\" ry=\"2.5\" style=\"stroke:#FFDD88;stroke-width:3;\" width=\"{w_str}\" x=\"5\" y=\"5\"/>"
    ));
    let escaped_text = banner_text.replace(' ', "\u{00a0}");
    let escaped_text = xml_escape(&escaped_text);
    let tl = fmt_coord(text_w);
    let txt_x = fmt_coord(text_x);
    let txt_y = fmt_coord(text_y);
    sg.push_raw(&format!(
        "<text fill=\"#000000\" font-family=\"monospace\" font-size=\"10\" lengthAdjust=\"spacing\" textLength=\"{tl}\" x=\"{txt_x}\" y=\"{txt_y}\">{escaped_text}</text>"
    ));
}

/// Render a SequenceDiagram + SeqLayout into an SVG string.
pub fn render_sequence(
    sd: &SequenceDiagram,
    layout: &SeqLayout,
    skin: &SkinParams,
) -> Result<String> {
    // Apply skinparam font overrides
    // Note: handwritten mode does NOT change fonts. It only affects shape rendering
    // (jiggling) and adds a warning banner.
    let font = skin.default_font_name().map(|s| s.to_string());
    set_default_font_family(font);
    enable_path_sprites();
    crate::render::svg_sprite::clear_gradient_defs();
    crate::render::svg_sprite::set_monochrome(skin.is_monochrome());
    let result = render_sequence_inner(sd, layout, skin);
    crate::render::svg_sprite::set_monochrome(false);
    disable_path_sprites();
    set_default_font_family(None);
    result
}

fn render_sequence_inner(
    sd: &SequenceDiagram,
    layout: &SeqLayout,
    skin: &SkinParams,
) -> Result<String> {
    let bg = skin.get_or("backgroundcolor", "#FFFFFF");

    // Build the sequence body first, then measure the emitted geometry using
    // Java-like visible-bounds semantics to derive the final SVG viewport.
    let mut sg = SvgGraphic::new(0, 1.0);
    let shadow_filter_id = crate::klimt::svg::current_shadow_id();

    // Write defs placeholder and open group
    write_seq_defs(&mut sg);
    sg.push_raw("<g>");

    // Handwritten mode: each shape gets a fresh RNG (seed 424242).
    // Java's UGraphicHandwritten.apply() creates a new wrapper with new Random(424242L)
    // on every apply(UChange), so each draw() call starts from the same seed.
    let handwritten = skin.is_handwritten();

    // Handwritten warning banner (before any diagram content).
    // Java draws this at the ImageBuilder level BEFORE enabling the handwritten
    // UGraphic, so it is a plain rect (no jiggle).
    if handwritten {
        draw_handwritten_banner(&mut sg, layout.total_width);
    }

    // Shadow filter attribute for elements that support shadows (skin rose, etc.)
    let shadow_attr = if sd.delta_shadow > 0.0 {
        format!(r#" filter="url(#{})""#, shadow_filter_id)
    } else {
        String::new()
    };

    // Build participant name -> index mapping
    let part_index = build_participant_index(sd);
    let display_names: std::collections::HashMap<&str, &str> = sd
        .participants
        .iter()
        .filter_map(|p| p.display_name.as_deref().map(|dn| (p.name.as_str(), dn)))
        .collect();

    // 3. Fragment frame rects and background rects.
    // Puma: first rect BEFORE lifelines (Java DrawableSet order).
    // Teoz: background rects with color BEFORE lifelines (Java GroupingTile drawU order).
    {
        let mut sorted_frags: Vec<&FragmentLayout> = layout.fragments.iter().collect();
        sorted_frags.sort_by(|a, b| {
            a.y.partial_cmp(&b.y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
        });
        if sd.teoz_mode {
            // Teoz: draw background rects for fragments with a color, BEFORE lifelines.
            // Java GroupingTile.drawU() emits the background rect as the first element.
            for frag in &sorted_frags {
                if let Some(ref color) = frag.color {
                    let fx = fmt_coord(frag.x);
                    let fy = fmt_coord(frag.y);
                    let fw = fmt_coord(frag.width);
                    let fh = fmt_coord(frag.height);
                    // color is already in SVG format (e.g., "#FFFFAA")
                    sg.push_raw(&format!(
                        "<rect fill=\"{color}\" height=\"{fh}\" style=\"stroke:{color};stroke-width:1;\" width=\"{fw}\" x=\"{fx}\" y=\"{fy}\"/>"
                    ));
                }
            }
        } else {
            // Puma: draw frame outline rects (first of two)
            for frag in &sorted_frags {
                let fx = fmt_coord(frag.x);
                let fy = fmt_coord(frag.y);
                let fw = fmt_coord(frag.width);
                let fh = fmt_coord(frag.height);
                sg.push_raw(&format!(
                    "<rect fill=\"none\" height=\"{fh}\" style=\"stroke:#000000;stroke-width:1.5;\" width=\"{fw}\" x=\"{fx}\" y=\"{fy}\"/>"
                ));
            }
        }
    }

    // 4/5. Activation bars and lifelines — order depends on engine:
    // Teoz: per-participant interleaving: lifeline then its activations
    //       (Java MainTile draws each participant's components together)
    // Puma: activations first, then lifelines (Java DrawableSet draw order)
    let act_border = skin.sequence_lifeline_border_color(BORDER_COLOR);
    if sd.teoz_mode {
        draw_lifelines_with_activations(&mut sg, layout, skin, sd, &display_names, &shadow_attr);
    } else {
        for act in &layout.activations {
            let title = display_names
                .get(act.participant.as_str())
                .copied()
                .unwrap_or(&act.participant);
            draw_activation(&mut sg, act, title, &shadow_attr, act_border);
        }
        draw_lifelines(&mut sg, layout, skin, sd, handwritten);
    }

    // 5b. Group frames (legacy, puma only)
    for group in &layout.groups {
        draw_group(&mut sg, group);
    }

    // 5c/5d. Dividers and delays are rendered after participant heads/tails,
    // interleaved with messages (see step 8).

    // 5e. Refs are interleaved with messages (see step 8)

    let default_font = skin
        .get("defaultfontname")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "SansSerif".to_string());
    // Participant-specific font (for participant labels). Java's
    // ParticipantBox uses FontParam.PARTICIPANT which falls back to
    // defaultFontName when not set. Web fonts (Roboto, etc.) only need to
    // be picked up here so that participant text emits the right family.
    let participant_font = skin
        .get("participantfontname")
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_font.clone());
    // Participant font style (italic / bold). Java FontParam.PARTICIPANT
    // resolves the same key as participant.fontstyle.
    let participant_font_style: Option<String> = skin
        .get("participantfontstyle")
        .map(std::string::ToString::to_string);
    let msg_font_size: f64 = skin
        .get("defaultfontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(FONT_SIZE);
    let seq_svg_font_family = svg_font_family_attr(&default_font);

    let monochrome = skin.is_monochrome();
    let is_skin_rose = skin.get("_skin_rose").is_some();
    let part_bg = if monochrome {
        "#E3E3E3"
    } else {
        skin.background_color("participant", PARTICIPANT_BG)
    };
    let part_border = skin.border_color("participant", BORDER_COLOR);
    let part_thickness = skin.line_thickness("participant", if is_skin_rose { 1.5 } else { 0.5 });
    let part_rounded = !is_skin_rose;
    let part_font = skin.font_color("participant", TEXT_COLOR);
    let part_font_size: f64 = skin
        .get("participantfontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| {
            skin.get("defaultfontsize")
                .and_then(|s| s.parse::<f64>().ok())
        })
        .unwrap_or(14.0);

    // 6. Participant head + tail boxes
    // Puma: interleaved per participant (head then tail for each).
    // Teoz: all heads first, then all tails (Java: LivingSpaces.drawHeads called twice).
    let max_ph = layout
        .participants
        .iter()
        .map(|pp| pp.box_height)
        .fold(0.0_f64, f64::max);
    // Puma: tail y = lifeline_bottom - 1 (drawing convention)
    // Teoz: tail y = lifeline_bottom (Java: PlayingSpaceWithParticipants draws
    //        tails at ug + dy(height + headHeight), height = getPreferredHeight)
    let bottom_y = if sd.teoz_mode {
        layout.lifeline_bottom
    } else {
        layout.lifeline_bottom - 1.0
    };

    // Helper closure for drawing one participant head or tail
    let draw_part = |sg: &mut SvgGraphic,
                     i: usize,
                     p: &ParticipantLayout,
                     y: f64,
                     is_head: bool,
                     handwritten: bool| {
        let part_idx = i + 1;
        let dn = display_names.get(p.name.as_str()).copied();
        let qualified_name = xml_escape(&p.name);
        let kind = sd.participants.get(i).map(|pp| &pp.kind);
        let is_actor = matches!(kind, Some(ParticipantKind::Actor));
        let part_text_color = skin.font_color("participant", TEXT_COLOR);
        // link_url is used for <a> wrapping. When [[...]] spans multiple lines
        // (contains NEWLINE_CHAR), Java renders the raw markup text — no hyperlink.
        // The URL is still stored in link_url for the lifeline <title> encoding.
        let part_link_url = sd.participants.get(i).and_then(|pp| pp.link_url.as_deref());
        let display_has_raw_link = dn.is_some_and(|d| d.contains("[["));
        let part_link_url = if display_has_raw_link {
            None
        } else {
            part_link_url
        };

        // Puma mode wraps in a group with class/data attributes; teoz does not
        if !sd.teoz_mode {
            let role = if is_head { "head" } else { "tail" };
            let mut tmp = String::new();
            write!(
                tmp,
                r#"<g class="participant participant-{role}" data-entity-uid="part{idx}" data-qualified-name="{name}" id="part{idx}-{role}">"#,
                idx = part_idx,
                name = qualified_name,
                role = role,
            )
            .unwrap();
            sg.push_raw(&tmp);
        }
        if is_actor {
            if is_head {
                draw_participant_actor(
                    sg,
                    p,
                    y,
                    dn,
                    part_bg,
                    part_border,
                    part_text_color,
                    part_thickness,
                    &participant_font,
                );
            } else {
                draw_participant_actor_tail(
                    sg,
                    p,
                    y,
                    dn,
                    part_bg,
                    part_border,
                    part_text_color,
                    part_thickness,
                    &participant_font,
                );
            }
        } else {
            draw_participant_box_with_font(
                sg,
                p,
                y,
                dn,
                part_bg,
                part_border,
                part_font,
                &participant_font,
                part_font_size,
                participant_font_style.as_deref(),
                is_head,
                part_link_url,
                &shadow_attr,
                part_thickness,
                part_rounded,
                sd.delta_shadow,
                handwritten,
            );
        }
        if !sd.teoz_mode {
            sg.push_raw("</g>");
        }
    };

    if sd.teoz_mode {
        // Teoz: all heads, then all tails
        // Java: headHeight = max(preferredHeight) = max_ph + 1 + deltaShadow.
        // Heads drawn at base ug (STARTING_Y), lifelines at ug + dy(headHeight).
        // For BOTTOM alignment: y = headHeight - dimHead.height = max_ph - p.box_height.
        // So top_y = STARTING_Y + (max_ph - p.box_height).
        // Since lifeline_top = STARTING_Y + max_ph + 1 + deltaShadow:
        //   STARTING_Y = lifeline_top - max_ph - 1 - deltaShadow.
        for (i, p) in layout.participants.iter().enumerate() {
            let head_base = layout.lifeline_top - max_ph - 1.0 - sd.delta_shadow;
            let top_y = head_base + max_ph - p.box_height;
            draw_part(&mut sg, i, p, top_y, true, handwritten);
        }
        if !sd.hide_footbox {
            for (i, p) in layout.participants.iter().enumerate() {
                // Java teoz: LivingSpace.drawHead() always uses headType (not tailType),
                // so all participant types render with head=true layout (icon on top, text below).
                draw_part(&mut sg, i, p, bottom_y, true, handwritten);
            }
        }
    } else {
        // Puma: interleaved head + tail per participant
        // Use lifeline_top as anchor (accounts for handwritten banner dy).
        // Equivalent to MARGIN + max_ph - p.box_height when lifeline_top = MARGIN + max_ph + 1.
        for (i, p) in layout.participants.iter().enumerate() {
            let top_y = layout.lifeline_top - 1.0 - p.box_height;
            draw_part(&mut sg, i, p, top_y, true, handwritten);
            if !sd.hide_footbox {
                draw_part(&mut sg, i, p, bottom_y, false, handwritten);
            }
        }
    }

    // 6b. Fragment rendering handled in interstitial events (step 8)
    // for both teoz and puma modes. Fragments are drawn interleaved with
    // messages at their correct Y positions (Java MainTile order).

    // 7. Activation bars foreground pass (puma only).
    // Teoz activations are already drawn in draw_lifelines_with_activations.
    if !sd.teoz_mode {
        for act in &layout.activations {
            let title = display_names
                .get(act.participant.as_str())
                .copied()
                .unwrap_or(&act.participant);
            draw_activation(&mut sg, act, title, &shadow_attr, act_border);
        }
    }

    // 8. Messages interleaved with fragment details and destroy markers
    // Build a y-sorted list of interstitial events (fragment details + separators)
    // that should be emitted between messages at the appropriate y positions.
    let seq_arrow_color = skin.sequence_arrow_color(BORDER_COLOR);
    let seq_arrow_thickness = skin.sequence_arrow_thickness().unwrap_or(1.0);
    // Java: message text uses root.fontcolor (from theme style) for the text fill.
    let seq_msg_text_color = skin.font_color("sequence", TEXT_COLOR);
    let word_by_word = skin.get("maxmessagesize").is_some();
    let mut msg_seq_counter: usize = 0;

    // Collect interstitial events (separators, refs, destroys, dividers, delays).
    // Fragment details are handled separately via first_msg_index.
    enum InterstitialEvent<'a> {
        FragmentDetail(&'a FragmentLayout),
        Separator(&'a FragmentLayout, f64, &'a str),
        Ref(&'a RefLayout),
        Destroy(&'a DestroyLayout),
        Divider(&'a DividerLayout),
        Delay(&'a DelayLayout),
    }
    let mut interstitials: Vec<(f64, InterstitialEvent)> = Vec::new();
    // Build fragment-by-message-index map for ordered fragment rendering.
    // Fragments with first_msg_index are drawn just before that message.
    // Fragments without first_msg_index use Y-based ordering.
    let mut frag_by_msg_idx: std::collections::BTreeMap<usize, Vec<&FragmentLayout>> =
        std::collections::BTreeMap::new();
    for frag in &layout.fragments {
        if let Some(mi) = frag.first_msg_index {
            // Teoz: fragment detail + separators are drawn together
            // before the first message (handled via frag_by_msg_idx)
            frag_by_msg_idx.entry(mi).or_default().push(frag);
        } else {
            // Puma: fragment detail and separators use Y-based interstitials
            interstitials.push((frag.y, InterstitialEvent::FragmentDetail(frag)));
            for (sep_y, sep_label) in &frag.separators {
                interstitials.push((
                    *sep_y,
                    InterstitialEvent::Separator(frag, *sep_y, sep_label),
                ));
            }
        }
    }
    for r in &layout.refs {
        interstitials.push((r.y, InterstitialEvent::Ref(r)));
    }
    // In teoz mode, destroy crosses are drawn per-participant alongside
    // activation bars (matching Java LiveBoxesDrawer.drawDestroyIfNeeded
    // order). Only puma mode emits destroys via interstitials.
    if !sd.teoz_mode {
        for d in &layout.destroys {
            interstitials.push((d.y, InterstitialEvent::Destroy(d)));
        }
    }
    for div in &layout.dividers {
        interstitials.push((div.y, InterstitialEvent::Divider(div)));
    }
    for delay in &layout.delays {
        interstitials.push((delay.y, InterstitialEvent::Delay(delay)));
    }
    interstitials.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut interstitial_idx = 0;
    let mut drawn_notes = std::collections::HashSet::new();
    for (msg_idx, msg) in layout.messages.iter().enumerate() {
        msg_seq_counter += 1;

        // Draw fragment details that should appear before this message
        if let Some(frags) = frag_by_msg_idx.remove(&msg_idx) {
            for frag in frags {
                draw_fragment_details(&mut sg, frag);
            }
        }

        // Emit interstitial events that come before this message's y
        while interstitial_idx < interstitials.len() && interstitials[interstitial_idx].0 < msg.y {
            match &interstitials[interstitial_idx].1 {
                InterstitialEvent::FragmentDetail(frag) => {
                    draw_fragment_details(&mut sg, frag);
                }
                InterstitialEvent::Separator(frag, sep_y, sep_label) => {
                    draw_fragment_separator(&mut sg, frag, *sep_y, sep_label, false);
                }
                InterstitialEvent::Ref(r) => {
                    draw_ref(&mut sg, r);
                }
                InterstitialEvent::Destroy(d) => {
                    draw_destroy(&mut sg, d);
                }
                InterstitialEvent::Divider(div) => {
                    draw_divider(&mut sg, div);
                }
                InterstitialEvent::Delay(delay) => {
                    draw_delay(&mut sg, delay, layout);
                }
            }
            interstitial_idx += 1;
        }

        // Draw the message
        let from_idx = find_participant_idx_by_x(&layout.participants, msg.from_x, &part_index);
        let to_idx = if msg.is_self {
            from_idx
        } else {
            find_participant_idx_by_x(&layout.participants, msg.to_x, &part_index)
        };

        // Per-message color override from [#color] syntax
        let effective_color = msg
            .color
            .as_ref()
            .map(|c| crate::style::normalize_color(c))
            .unwrap_or_else(|| seq_arrow_color.to_string());
        let effective_color = effective_color.as_str();

        if msg.is_self {
            draw_self_message(
                &mut sg,
                msg,
                effective_color,
                seq_arrow_thickness,
                &default_font,
                msg_font_size,
                from_idx,
                msg_seq_counter,
                word_by_word,
                sd.teoz_mode,
            );
        } else {
            draw_message(
                &mut sg,
                msg,
                effective_color,
                seq_arrow_thickness,
                &default_font,
                seq_svg_font_family,
                msg_font_size,
                seq_msg_text_color,
                from_idx,
                to_idx,
                msg_seq_counter,
                msg.source_line,
                word_by_word,
                sd.teoz_mode,
                handwritten,
            );
        }

        // Draw notes associated with this message. Use explicit message
        // index association when available; fall back to y-range heuristic.
        // Defer notes when the next message is at the same y (parallel messages):
        // Java renders all parallel messages first, then their notes.
        let next_msg_y_val = layout.messages.get(msg_idx + 1).map(|m| m.y);
        let same_y_next = next_msg_y_val.is_some_and(|ny| (ny - msg.y).abs() < 0.01);
        if !same_y_next {
            // Find the next message that is at a DIFFERENT y position (skip parallel siblings)
            let effective_next_y = {
                let mut next_y = f64::MAX;
                for future in &layout.messages[msg_idx + 1..] {
                    if (future.y - msg.y).abs() > 0.01 {
                        next_y = future.y;
                        break;
                    }
                }
                next_y
            };
            let note_back_threshold = if msg.is_self { 200.0 } else { 100.0 };
            let mut has_note = false;
            for (ni, note) in layout.notes.iter().enumerate() {
                if drawn_notes.contains(&ni) {
                    continue;
                }
                let belongs = if let Some(assoc_idx) = note.assoc_message_idx {
                    // Match notes associated with this message or any preceding
                    // parallel message at the same y position.
                    assoc_idx <= msg_idx
                        && layout
                            .messages
                            .get(assoc_idx)
                            .is_some_and(|am| (am.y - msg.y).abs() < 0.01)
                } else {
                    // Fallback: y-range heuristic for notes without explicit association
                    note.y >= msg.y - note_back_threshold && note.y < effective_next_y
                };
                if belongs {
                    draw_note(&mut sg, note, &shadow_attr, skin);
                    drawn_notes.insert(ni);
                    has_note = true;
                }
            }
            // In Java, when a message has notes, it's wrapped in ArrowAndNoteBox
            // which consumes an extra counter value. Advance to match Java's
            // msg id numbering.
            if has_note {
                msg_seq_counter += 1;
            }
        }
    }

    // Draw any remaining fragment details not associated with messages
    for (_mi, frags) in frag_by_msg_idx {
        for frag in frags {
            draw_fragment_details(&mut sg, frag);
        }
    }

    // Draw any remaining notes not yet drawn (standalone or missed by association)
    for (ni, note) in layout.notes.iter().enumerate() {
        if !drawn_notes.contains(&ni) {
            draw_note(&mut sg, note, &shadow_attr, skin);
            drawn_notes.insert(ni);
        }
    }

    // Emit any remaining interstitial events
    while interstitial_idx < interstitials.len() {
        match &interstitials[interstitial_idx].1 {
            InterstitialEvent::FragmentDetail(frag) => {
                draw_fragment_details(&mut sg, frag);
            }
            InterstitialEvent::Separator(frag, sep_y, sep_label) => {
                draw_fragment_separator(&mut sg, frag, *sep_y, sep_label, false);
            }
            InterstitialEvent::Ref(r) => {
                draw_ref(&mut sg, r);
            }
            InterstitialEvent::Destroy(d) => {
                draw_destroy(&mut sg, d);
            }
            InterstitialEvent::Divider(div) => {
                draw_divider(&mut sg, div);
            }
            InterstitialEvent::Delay(delay) => {
                draw_delay(&mut sg, delay, layout);
            }
        }
        interstitial_idx += 1;
    }

    sg.push_raw("</g>");

    let mut body = sg.body().to_string();

    // Post-process: inject gradient defs, shadow filter, and filter definitions
    let gradient_defs = crate::render::svg_sprite::take_gradient_defs();
    let filters = take_back_filters();
    let has_shadow = sd.delta_shadow > 0.0;
    if !gradient_defs.is_empty() || !filters.is_empty() || has_shadow {
        let mut defs_content = String::new();
        // Shadow filter: Java SvgGraphics.manageShadow() — Gaussian blur + offset shadow
        if has_shadow {
            let ds = sd.delta_shadow as i32;
            write!(
                defs_content,
                concat!(
                    r#"<filter height="300%" id="{id}" width="300%" x="-1" y="-1">"#,
                    r#"<feGaussianBlur result="blurOut" stdDeviation="2"/>"#,
                    r#"<feColorMatrix in="blurOut" result="blurOut2" type="matrix" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 .4 0"/>"#,
                    r#"<feOffset dx="{ds}" dy="{ds}" in="blurOut2" result="blurOut3"/>"#,
                    r#"<feBlend in="SourceGraphic" in2="blurOut3" mode="normal"/>"#,
                    r#"</filter>"#,
                ),
                id = shadow_filter_id,
                ds = ds,
            )
            .unwrap();
        }
        for (_id, def_xml) in &gradient_defs {
            defs_content.push_str(def_xml);
        }
        for (id, hex_color) in &filters {
            write!(
                defs_content,
                r#"<filter height="1" id="{}" width="1" x="0" y="0"><feFlood flood-color="{}" result="flood"/><feComposite in="SourceGraphic" in2="flood" operator="over"/></filter>"#,
                id, hex_color,
            )
            .unwrap();
        }
        body = body.replacen("<defs/>", &format!("<defs>{}</defs>", defs_content), 1);
    }

    // Apply root line thickness: replace default stroke-width:0.5 with root theme value.
    // This applies to lifelines, notes, etc. — but NOT participant boxes which get
    // their thickness explicitly from the rendering code.
    let root_thickness = skin.line_thickness("root", 0.5);
    if (root_thickness - 0.5).abs() > f64::EPSILON {
        let sw_str = fmt_stroke_width(root_thickness);
        body = body.replace("stroke-width:0.5;", &format!("stroke-width:{sw_str};"));
    }

    let (svg_w, svg_h) =
        if let Some(((raw_w, raw_h), (sg_w, sg_h))) = measure_sequence_body_dim_full(&body) {
            // LimitFinder-style: `(max_x + 1) + marginR + 1 rounding`.
            let (lf_w, lf_h) = compute_viewport(raw_w, raw_h, &ViewportConfig::SEQUENCE_LF);
            // SvgGraphics-style: `ensureVisible_maxX + 1`, without extra margins
            // since the body coords already include the left/top margin offset.
            let sg_w_int = ensure_visible_int(sg_w) as f64;
            let sg_h_int = ensure_visible_int(sg_h) as f64;
            // Final viewport = max of both (Java uses whichever pass produced the
            // larger maxX during SvgGraphics.ensureVisible + initial minDim).
            (lf_w.max(sg_w_int), lf_h.max(sg_h_int))
        } else {
            (
                ensure_visible_int(layout.total_width) as f64,
                ensure_visible_int(layout.total_height) as f64,
            )
        };

    if !bg.eq_ignore_ascii_case("#FFFFFF") {
        let mut bg_rect = String::new();
        write_bg_rect(&mut bg_rect, svg_w, svg_h, bg);
        body = body.replacen("<g>", &format!("<g>{bg_rect}"), 1);
    }

    let mut buf = String::with_capacity(body.len() + 256);
    write_svg_root_bg(&mut buf, svg_w, svg_h, "SEQUENCE", bg);
    buf.push_str(&body);
    buf.push_str("</svg>");

    Ok(buf)
}

/// Find the 1-based participant index whose center x is closest to the given x.
fn find_participant_idx_by_x(
    participants: &[ParticipantLayout],
    x: f64,
    part_index: &std::collections::HashMap<String, usize>,
) -> usize {
    let mut best_idx = 1;
    let mut best_dist = f64::MAX;
    for p in participants {
        let dist = (p.x - x).abs();
        if dist < best_dist {
            best_dist = dist;
            if let Some(&idx) = part_index.get(&p.name) {
                best_idx = idx;
            }
        }
    }
    best_idx
}

// ── Tests ───────────────────────────────────────────────────────────

// Rendering correctness is verified by reference_tests.rs (full-pipeline SVG
// compared against Java gold-standard SVGs) — the same method Java uses
// (checkImage → TestResult comparison).  These smoke tests only verify:
// no panic, output is valid SVG.  All structural/coordinate assertions
// belong in reference_tests, not here.

#[cfg(test)]
mod tests {
    use super::encode_metadata_title;

    fn convert(puml: &str) -> String {
        crate::convert(puml).expect("convert must succeed")
    }

    #[test]
    fn metadata_title_uses_java_ugroup_fix_semantics() {
        assert_eq!(encode_metadata_title(":Order"), ".Order");
        assert_eq!(encode_metadata_title("«datastore»"), ".datastore.");
        assert_eq!(encode_metadata_title("«datastore»\\nOrders"), ".datastore.");
        assert_eq!(
            encode_metadata_title(&format!("«datastore»{}Orders", crate::NEWLINE_CHAR)),
            ".datastore."
        );
    }

    #[test]
    fn smoke_simple_message() {
        let svg = convert("@startuml\nAlice -> Bob : hello\n@enduml");
        assert!(svg.starts_with("<?plantuml "));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn smoke_self_message() {
        let svg = convert("@startuml\nA -> A : self\n@enduml");
        assert!(svg.starts_with("<?plantuml "));
    }

    #[test]
    fn smoke_dashed_open_head() {
        let svg = convert("@startuml\nA --> B : reply\n@enduml");
        assert!(svg.starts_with("<?plantuml "));
    }

    #[test]
    fn smoke_destroy() {
        let svg = convert("@startuml\nA -> B : kill\ndestroy B\n@enduml");
        assert!(svg.starts_with("<?plantuml "));
    }

    #[test]
    fn smoke_note() {
        let svg = convert("@startuml\nA -> B : msg\nnote right: a note\n@enduml");
        assert!(svg.starts_with("<?plantuml "));
    }

    #[test]
    fn smoke_activation() {
        let svg =
            convert("@startuml\nA -> B : req\nactivate B\nB --> A : resp\ndeactivate B\n@enduml");
        assert!(svg.starts_with("<?plantuml "));
    }

    #[test]
    fn smoke_all_participant_kinds() {
        let svg = convert(
            "@startuml\nactor A\nboundary B\ncontrol C\ndatabase D\n\
             entity E\ncollections F\nqueue G\nparticipant H\nA -> H : msg\n@enduml",
        );
        assert!(svg.starts_with("<?plantuml "));
    }

    #[test]
    fn smoke_empty() {
        // Empty @startuml/@enduml may not parse as sequence diagram;
        // just verify it doesn't panic
        let _ = crate::convert("@startuml\n@enduml");
    }

    #[test]
    fn participant_visible_text_is_not_sanitized_in_metadata_title_fix() {
        let svg = convert("@startuml\nparticipant \":Order\" as o\no -> o : msg\n@enduml");
        assert!(svg.contains("<title>.Order</title>"));
        assert!(svg.contains(">:Order</text>"));
    }

    #[test]
    fn theme_plain_renders_white_actor_head_and_participant_boxes() {
        let svg =
            convert("@startuml\n!theme plain\nactor Alice\nparticipant Bob\nAlice -> Bob\n@enduml");
        assert!(svg.contains(r#"<ellipse cx=""#));
        assert!(svg.contains("fill=\"#FFFFFF\""));
        assert!(svg.contains("<rect fill=\"#FFFFFF\""));
    }

    #[test]
    fn multiline_default_participant_centers_each_line() {
        let svg = convert(
            "@startuml\n!theme plain\nparticipant \"«datastore»%newline()Orders\" as d\n@enduml",
        );
        let datastore_idx = svg.find("&#171;datastore&#187;</text>").unwrap();
        let datastore_prefix = &svg[..datastore_idx];
        let datastore_x_start = datastore_prefix.rfind(" x=\"").unwrap() + 4;
        let datastore_x_end =
            datastore_prefix[datastore_x_start..].find('"').unwrap() + datastore_x_start;
        let datastore_x: f64 = datastore_prefix[datastore_x_start..datastore_x_end]
            .parse()
            .unwrap();

        let orders_idx = svg.find(">Orders</text>").unwrap();
        let orders_prefix = &svg[..orders_idx];
        let orders_x_start = orders_prefix.rfind(" x=\"").unwrap() + 4;
        let orders_x_end = orders_prefix[orders_x_start..].find('"').unwrap() + orders_x_start;
        let orders_x: f64 = orders_prefix[orders_x_start..orders_x_end].parse().unwrap();

        assert!(orders_x > datastore_x);
    }
}
