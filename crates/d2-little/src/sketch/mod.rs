//! d2-sketch: sketch/hand-drawn rendering via a pure-Rust port of rough.js.
//!
//! The shape / arrowhead / connection routines mirror Go
//! `d2renderers/d2sketch/sketch.go` one-to-one — every `rc.*` call site in the
//! Go source is reproduced by a direct `rough::draw_*` call here. Downstream
//! we truncate the `d` path string's decimal literals to 6 digits, because
//! Go's sketch renderer does the same to paper over Math.sin/cos drift between
//! JS engines; keeping the same truncation guarantees byte-equivalent output
//! on every case that already matched under the rquickjs pipeline.
//!
//! Licensing: this module (being a port of Go `d2renderers/d2sketch/sketch.go`)
//! is MPL-2.0 per the root `LICENSE`. The rough.js port in `rough.rs` remains
//! under the upstream MIT license (Copyright (c) 2019 Preet Shihn); see
//! `src/sketch/LICENSE` for the full MIT text.

use std::fmt::Write as _;

use crate::geo::{Point, Segment};
use crate::target::{Arrowhead, Connection, INNER_BORDER_OFFSET, Shape};
use crate::themes::ThemableElement;

pub mod rough;

use rough::{Opts, RoughPath as NativeRoughPath};

// ---------------------------------------------------------------------------
// Embedded assets
// ---------------------------------------------------------------------------

/// CSS `<defs>` template for the four streak overlays (bright/normal/dark/
/// darker). Three `%s` placeholders: luminance category, diagram hash, fill.
const STREAKS_TEMPLATE: &str = include_str!("../../assets/streaks.txt");

// ---------------------------------------------------------------------------
// Sketch runner
// ---------------------------------------------------------------------------

/// Kept as a public type for API compatibility with `d2-svg-render`. After
/// porting rough.js natively there is no persistent runtime state left —
/// this is effectively a marker that "sketch mode is active" and is cheap
/// to construct per render.
#[derive(Debug, Default)]
pub struct SketchRunner;

impl SketchRunner {
    /// Infallible in the native implementation; still returns `Result` so
    /// upstream callers that `?`-propagate keep compiling.
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }
}

// ---------------------------------------------------------------------------
// rough.js result wrapping
// ---------------------------------------------------------------------------

/// Mirror of the rquickjs-era `RoughPath` carrying a decimal-truncated `d`
/// and the stroke / fill / stroke-width string trio the SVG writer needs.
#[derive(Debug, Default, Clone)]
struct RoughPath {
    d: String,
    stroke: String,
    stroke_width: String,
    fill: String,
}

impl RoughPath {
    /// `style` fragment mirroring Go `roughPath.StyleCSS`: only stroke-width.
    fn style_css(&self) -> String {
        if self.stroke_width.is_empty() {
            String::new()
        } else {
            format!("stroke-width:{};", self.stroke_width)
        }
    }
}

impl From<NativeRoughPath> for RoughPath {
    fn from(p: NativeRoughPath) -> Self {
        Self {
            d: truncate_floats(&p.d),
            stroke: p.stroke,
            stroke_width: p.stroke_width,
            fill: p.fill,
        }
    }
}

fn to_paths(raw: Vec<NativeRoughPath>) -> Vec<RoughPath> {
    raw.into_iter().map(RoughPath::from).collect()
}

fn path_data(raw: Vec<NativeRoughPath>) -> Vec<String> {
    raw.into_iter().map(|p| truncate_floats(&p.d)).collect()
}

// ---------------------------------------------------------------------------
// Option presets
// ---------------------------------------------------------------------------

/// Shape rough options: the `baseRoughProps` constant from sketch.go
/// (`fillWeight: 2.0, hachureGap: 16, fillStyle: "solid", bowing: 2, seed: 1`)
/// combined with the shape's fill/stroke colors (black for the synthetic
/// sketched path; the real colors are applied via SVG attrs downstream).
fn shape_opts(stroke_width: f64) -> Opts {
    let mut o = Opts::default_base();
    o.stroke = "#000".into();
    o.stroke_width = stroke_width;
    o.fill = Some("#000".into());
    o.fill_style = "solid".into();
    o.fill_weight = 2.0;
    o.hachure_gap = 16.0;
    o.bowing = 2.0;
    o.seed = 1;
    o
}

// ---------------------------------------------------------------------------
// Float truncation
// ---------------------------------------------------------------------------

/// Truncate every decimal literal to at most 6 decimal places. Matches
/// Go's `floatRE = regexp.MustCompile(\`(\d+)\.(\d+)\`)` replacement:
/// truncation (not rounding) to 6 digits past the dot.
///
/// Implemented as a state machine — input is a single SVG path string and
/// we call this once per sketched element, so no regex engine is needed.
fn truncate_floats(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // Match `(\d+)\.(\d+)`.
        if c.is_ascii_digit() {
            let int_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len()
                && bytes[i] == b'.'
                && i + 1 < bytes.len()
                && bytes[i + 1].is_ascii_digit()
            {
                let dot_pos = i;
                i += 1; // skip '.'
                let dec_start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                let decimal_len = i - dec_start;
                let keep = decimal_len.min(6);
                out.push_str(&input[int_start..=dot_pos]);
                out.push_str(&input[dec_start..dec_start + keep]);
            } else {
                out.push_str(&input[int_start..i]);
            }
        } else {
            out.push(c as char);
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Fill patterns (streaks)
// ---------------------------------------------------------------------------

/// Append `<defs>…</defs>` with the four streak fill patterns used by sketched
/// overlays, keyed by `diagramHash`.  Only emits each pattern if the rendered
/// SVG source already references it (matches Go DefineFillPatterns).
pub fn define_fill_patterns(buf: &mut String, diagram_hash: &str) {
    let source_snapshot = buf.clone();
    buf.push_str("<defs>");
    for (lc, fill) in &[
        ("bright", "rgba(0, 0, 0, 0.1)"),
        ("normal", "rgba(0, 0, 0, 0.16)"),
        ("dark", "rgba(0, 0, 0, 0.32)"),
        ("darker", "rgba(255, 255, 255, 0.24)"),
    ] {
        let trigger = format!("url(#streaks-{lc}-{diagram_hash})");
        if source_snapshot.contains(&trigger) {
            buf.push_str(&format_streaks(STREAKS_TEMPLATE, lc, diagram_hash, fill));
        }
    }
    buf.push_str("</defs>");
}

fn format_streaks(template: &str, a: &str, b: &str, c: &str) -> String {
    // streaks.txt uses three `%s` placeholders in order.
    let mut out = String::with_capacity(template.len() + 64);
    let mut idx = 0;
    let bytes = template.as_bytes();
    let mut i = 0;
    let substs = [a, b, c];
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 1 < bytes.len() && bytes[i + 1] == b's' {
            if idx < substs.len() {
                out.push_str(substs[idx]);
                idx += 1;
            }
            i += 2;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Shape/connection helpers
// ---------------------------------------------------------------------------

fn shape_theme_fill_stroke(shape: &Shape) -> (String, String) {
    if shape.type_ == crate::target::SHAPE_CLASS || shape.type_ == crate::target::SHAPE_SQL_TABLE {
        (shape.stroke.clone(), shape.fill.clone())
    } else {
        (shape.fill.clone(), shape.stroke.clone())
    }
}

fn shape_css_style(s: &Shape) -> String {
    let mut out = format!("stroke-width:{};", s.stroke_width);
    if s.stroke_dash != 0.0 {
        let (dash, gap) =
            crate::svg_path::get_stroke_dash_attributes(s.stroke_width as f64, s.stroke_dash);
        write!(out, "stroke-dasharray:{:.6},{:.6};", dash, gap).unwrap();
    }
    out
}

fn connection_css_style(c: &Connection) -> String {
    let mut out = format!("stroke-width:{};", c.stroke_width);
    let mut stroke_dash = c.stroke_dash;
    if stroke_dash == 0.0 && c.animated {
        stroke_dash = 5.0;
    }
    if stroke_dash != 0.0 {
        let (dash, gap) =
            crate::svg_path::get_stroke_dash_attributes(c.stroke_width as f64, stroke_dash);
        write!(out, "stroke-dasharray:{:.6},{:.6};", dash, gap).unwrap();
        if c.animated {
            let mut dash_offset: f64 = -10.0;
            if c.src_arrow != Arrowhead::None && c.dst_arrow == Arrowhead::None {
                dash_offset = 10.0;
            }
            write!(out, "stroke-dashoffset:{:.6};", dash_offset * (dash + gap)).unwrap();
            write!(
                out,
                "animation: dashdraw {:.6}s linear infinite;",
                gap * 0.5
            )
            .unwrap();
        }
    }
    out
}

/// Append the `sketch-overlay-<category>` class to `el` based on `fill`'s
/// luminance and render.  Mirrors Go `ThemableSketchOverlay.Render`.
///
/// Go unconditionally concatenates with a leading space (`" sketch-overlay-X"`)
/// so the class attribute always has a leading space even when `class_name`
/// was previously empty — mirror that byte-for-byte.
fn render_sketch_overlay(el: &mut ThemableElement, fill: &str) -> Result<String, String> {
    if crate::color::is_theme_color(fill) {
        el.class_name.push_str(&format!(" sketch-overlay-{fill}"));
    } else {
        let lc = crate::color::luminance_category(fill)
            .map_err(|e| format!("luminance_category({fill}): {e}"))?;
        el.class_name
            .push_str(&format!(" sketch-overlay-{}", lc.as_str()));
    }
    Ok(el.render())
}

// ---------------------------------------------------------------------------
// Shape primitives
// ---------------------------------------------------------------------------

/// `d2sketch.Rect`: rough-rendered rectangle with streak overlay.
pub fn rect(_runner: &SketchRunner, shape: &Shape, diagram_hash: &str) -> Result<String, String> {
    let o = shape_opts(shape.stroke_width as f64);
    let paths = path_data(rough::draw_rectangle(
        0.0,
        0.0,
        shape.width as f64,
        shape.height as f64,
        &o,
    ));
    let (fill, stroke) = shape_theme_fill_stroke(shape);

    let mut path_el = ThemableElement::new("path", None);
    path_el.set_translate(shape.pos.x as f64, shape.pos.y as f64);
    path_el.fill = fill.clone();
    path_el.stroke = stroke.clone();
    path_el.fill_pattern = shape.fill_pattern.clone();
    path_el.class_name = "shape".to_owned();
    path_el.style = shape_css_style(shape);

    if !shape.text.label.is_empty() {
        let lp = crate::label::Position::from_string(&shape.label_position);
        if lp.is_border() {
            path_el.mask = format!("url(#{diagram_hash})");
        }
    }

    let mut out = String::new();
    for p in paths {
        path_el.d = p;
        out.push_str(&path_el.render());
    }

    let mut so_el = ThemableElement::new("rect", None);
    so_el.set_translate(shape.pos.x as f64, shape.pos.y as f64);
    so_el.width = Some(shape.width as f64);
    so_el.height = Some(shape.height as f64);
    if !shape.text.label.is_empty() {
        let lp = crate::label::Position::from_string(&shape.label_position);
        if lp.is_border() {
            so_el.mask = format!("url(#{diagram_hash})");
        }
    }
    out.push_str(&render_sketch_overlay(&mut so_el, &fill)?);
    Ok(out)
}

/// `d2sketch.DoubleRect`: two concentric rough rectangles + streak overlay.
pub fn double_rect(
    _runner: &SketchRunner,
    shape: &Shape,
    diagram_hash: &str,
) -> Result<String, String> {
    let o = shape_opts(shape.stroke_width as f64);
    let paths_big = path_data(rough::draw_rectangle(
        0.0,
        0.0,
        shape.width as f64,
        shape.height as f64,
        &o,
    ));
    let paths_small = path_data(rough::draw_rectangle(
        0.0,
        0.0,
        (shape.width - INNER_BORDER_OFFSET * 2) as f64,
        (shape.height - INNER_BORDER_OFFSET * 2) as f64,
        &o,
    ));

    let (fill, stroke) = shape_theme_fill_stroke(shape);
    let mut out = String::new();

    let mut path_el = ThemableElement::new("path", None);
    path_el.set_translate(shape.pos.x as f64, shape.pos.y as f64);
    path_el.fill = fill.clone();
    path_el.stroke = stroke.clone();
    path_el.fill_pattern = shape.fill_pattern.clone();
    path_el.class_name = "shape".to_owned();
    path_el.style = shape_css_style(shape);
    if !shape.text.label.is_empty() {
        let lp = crate::label::Position::from_string(&shape.label_position);
        if lp.is_border() {
            path_el.mask = format!("url(#{diagram_hash})");
        }
    }
    for p in paths_big {
        path_el.d = p;
        out.push_str(&path_el.render());
    }

    let mut inner = ThemableElement::new("path", None);
    inner.set_translate(
        (shape.pos.x + INNER_BORDER_OFFSET) as f64,
        (shape.pos.y + INNER_BORDER_OFFSET) as f64,
    );
    let (_, stroke_i) = shape_theme_fill_stroke(shape);
    inner.fill = "transparent".to_owned();
    inner.stroke = stroke_i;
    inner.class_name = "shape".to_owned();
    inner.style = shape_css_style(shape);
    if !shape.text.label.is_empty() {
        let lp = crate::label::Position::from_string(&shape.label_position);
        if lp.is_border() {
            inner.mask = format!("url(#{diagram_hash})");
        }
    }
    for p in paths_small {
        inner.d = p;
        out.push_str(&inner.render());
    }

    let mut so = ThemableElement::new("rect", None);
    so.set_translate(shape.pos.x as f64, shape.pos.y as f64);
    so.width = Some(shape.width as f64);
    so.height = Some(shape.height as f64);
    if !shape.text.label.is_empty() {
        let lp = crate::label::Position::from_string(&shape.label_position);
        if lp.is_border() {
            so.mask = format!("url(#{diagram_hash})");
        }
    }
    // NOTE: Go passes `shape.Fill` (not the swapped pair) for DoubleRect's
    // overlay.  Mirror exactly — see sketch.go line 188.
    out.push_str(&render_sketch_overlay(&mut so, &shape.fill)?);
    Ok(out)
}

/// `d2sketch.Oval`.
pub fn oval(_runner: &SketchRunner, shape: &Shape, diagram_hash: &str) -> Result<String, String> {
    let o = shape_opts(shape.stroke_width as f64);
    let paths = path_data(rough::draw_ellipse(
        (shape.width / 2) as f64,
        (shape.height / 2) as f64,
        shape.width as f64,
        shape.height as f64,
        &o,
    ));
    let (fill, stroke) = shape_theme_fill_stroke(shape);

    let mut path_el = ThemableElement::new("path", None);
    path_el.set_translate(shape.pos.x as f64, shape.pos.y as f64);
    path_el.fill = fill.clone();
    path_el.stroke = stroke.clone();
    path_el.fill_pattern = shape.fill_pattern.clone();
    path_el.class_name = "shape".to_owned();
    path_el.style = shape_css_style(shape);
    if !shape.text.label.is_empty() {
        let lp = crate::label::Position::from_string(&shape.label_position);
        if lp.is_border() {
            path_el.mask = format!("url(#{diagram_hash})");
        }
    }
    let mut out = String::new();
    for p in paths {
        path_el.d = p;
        out.push_str(&path_el.render());
    }

    let mut so = ThemableElement::new("ellipse", None);
    so.set_translate(
        (shape.pos.x + shape.width / 2) as f64,
        (shape.pos.y + shape.height / 2) as f64,
    );
    so.rx = Some((shape.width / 2) as f64);
    so.ry = Some((shape.height / 2) as f64);
    if !shape.text.label.is_empty() {
        let lp = crate::label::Position::from_string(&shape.label_position);
        if lp.is_border() {
            so.mask = format!("url(#{diagram_hash})");
        }
    }
    out.push_str(&render_sketch_overlay(&mut so, &fill)?);
    Ok(out)
}

/// `d2sketch.DoubleOval`.
pub fn double_oval(
    _runner: &SketchRunner,
    shape: &Shape,
    diagram_hash: &str,
) -> Result<String, String> {
    let o = shape_opts(shape.stroke_width as f64);
    let big_paths = path_data(rough::draw_ellipse(
        (shape.width / 2) as f64,
        (shape.height / 2) as f64,
        shape.width as f64,
        shape.height as f64,
        &o,
    ));
    let small_paths = path_data(rough::draw_ellipse(
        (shape.width / 2) as f64,
        (shape.height / 2) as f64,
        (shape.width - INNER_BORDER_OFFSET * 2) as f64,
        (shape.height - INNER_BORDER_OFFSET * 2) as f64,
        &o,
    ));

    let (fill, stroke) = shape_theme_fill_stroke(shape);
    let mut out = String::new();
    let mut path_el = ThemableElement::new("path", None);
    path_el.set_translate(shape.pos.x as f64, shape.pos.y as f64);
    path_el.fill = fill.clone();
    path_el.stroke = stroke.clone();
    path_el.fill_pattern = shape.fill_pattern.clone();
    path_el.class_name = "shape".to_owned();
    path_el.style = shape_css_style(shape);
    if !shape.text.label.is_empty() {
        let lp = crate::label::Position::from_string(&shape.label_position);
        if lp.is_border() {
            path_el.mask = format!("url(#{diagram_hash})");
        }
    }
    for p in big_paths {
        path_el.d = p;
        out.push_str(&path_el.render());
    }

    let mut inner = ThemableElement::new("path", None);
    inner.set_translate(shape.pos.x as f64, shape.pos.y as f64);
    inner.fill = "transparent".to_owned();
    inner.stroke = stroke.clone();
    inner.class_name = "shape".to_owned();
    inner.style = shape_css_style(shape);
    if !shape.text.label.is_empty() {
        let lp = crate::label::Position::from_string(&shape.label_position);
        if lp.is_border() {
            inner.mask = format!("url(#{diagram_hash})");
        }
    }
    for p in small_paths {
        inner.d = p;
        out.push_str(&inner.render());
    }

    let mut so = ThemableElement::new("ellipse", None);
    so.set_translate(
        (shape.pos.x + shape.width / 2) as f64,
        (shape.pos.y + shape.height / 2) as f64,
    );
    so.rx = Some((shape.width / 2) as f64);
    so.ry = Some((shape.height / 2) as f64);
    if !shape.text.label.is_empty() {
        let lp = crate::label::Position::from_string(&shape.label_position);
        if lp.is_border() {
            so.mask = format!("url(#{diagram_hash})");
        }
    }
    out.push_str(&render_sketch_overlay(&mut so, &shape.fill)?);
    Ok(out)
}

/// `d2sketch.Paths`: rough each SVG path independently + per-path overlay.
pub fn paths(
    _runner: &SketchRunner,
    shape: &Shape,
    diagram_hash: &str,
    paths_in: &[String],
) -> Result<String, String> {
    let o = shape_opts(shape.stroke_width as f64);
    let mut out = String::new();
    for path in paths_in {
        let sketch_paths = path_data(rough::draw_path(path, &o));
        let (fill, stroke) = shape_theme_fill_stroke(shape);
        let mut path_el = ThemableElement::new("path", None);
        path_el.fill = fill.clone();
        path_el.stroke = stroke;
        path_el.fill_pattern = shape.fill_pattern.clone();
        path_el.class_name = "shape".to_owned();
        path_el.style = shape_css_style(shape);
        if !shape.text.label.is_empty() {
            let lp = crate::label::Position::from_string(&shape.label_position);
            if lp.is_border() {
                path_el.mask = format!("url(#{diagram_hash})");
            }
        }
        for p in &sketch_paths {
            path_el.d = p.clone();
            out.push_str(&path_el.render());
        }

        // One sketch-overlay per path, re-using pathEl's fill.
        let mut so = ThemableElement::new("path", None);
        if !shape.text.label.is_empty() {
            let lp = crate::label::Position::from_string(&shape.label_position);
            if lp.is_border() {
                so.mask = format!("url(#{diagram_hash})");
            }
        }
        for p in sketch_paths {
            so.d = p;
            out.push_str(&render_sketch_overlay(&mut so, &fill)?);
            // render_sketch_overlay mutates class_name by appending
            // " sketch-overlay-X" — reset before reuse.
            so.class_name.clear();
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// `d2sketch.Connection`: render the connection stroke (animated or not).
/// Called with the already-computed SVG path and the `attrs` string (Go
/// passes `mask="url(#diagramHash)"` for masking).
pub fn connection(
    _runner: &SketchRunner,
    connection: &Connection,
    path: &str,
    attrs: &str,
) -> Result<String, String> {
    let animated_class = if connection.animated {
        " animated-connection"
    } else {
        ""
    };

    if connection.animated {
        // Mirror Go: bidirectional or absent arrows → split path into two
        // halves with reverse animation direction.  Otherwise emit a single
        // sketched path.
        let bidirectional = (connection.dst_arrow == Arrowhead::None
            && connection.src_arrow == Arrowhead::None)
            || (connection.dst_arrow != Arrowhead::None && connection.src_arrow != Arrowhead::None);

        if bidirectional {
            let (p1, p2) =
                crate::svg_path::split_path(path, 0.5).map_err(|e| format!("split_path: {e}"))?;

            let mut el1 = ThemableElement::new("path", None);
            el1.d = p1;
            el1.fill = "none".to_owned();
            el1.stroke = connection.stroke.clone();
            el1.class_name = format!("connection{}", animated_class);
            el1.style = connection_css_style(connection);
            el1.style.push_str("animation-direction: reverse;");
            el1.attributes = attrs.to_owned();

            let mut el2 = ThemableElement::new("path", None);
            el2.d = p2;
            el2.fill = "none".to_owned();
            el2.stroke = connection.stroke.clone();
            el2.class_name = format!("connection{}", animated_class);
            el2.style = connection_css_style(connection);
            el2.attributes = attrs.to_owned();

            return Ok(format!("{} {}", el1.render(), el2.render()));
        }

        let mut el = ThemableElement::new("path", None);
        el.d = path.to_owned();
        el.fill = "none".to_owned();
        el.stroke = connection.stroke.clone();
        el.class_name = format!("connection{}", animated_class);
        el.style = connection_css_style(connection);
        el.attributes = attrs.to_owned();
        return Ok(el.render());
    }

    // Non-animated: sketch the path via rough.js with roughness=0.5.
    //
    // Go: `rc.path(path, {roughness: 0.5, seed: 1})`.
    let mut o = Opts::default_base();
    o.stroke = "#000".into();
    o.seed = 1;
    o.roughness = 0.5;
    let sketch_paths = path_data(rough::draw_path(path, &o));

    let mut out = String::new();
    let mut el = ThemableElement::new("path", None);
    el.fill = "none".to_owned();
    el.stroke = connection.stroke.clone();
    el.class_name = format!("connection{}", animated_class);
    el.style = connection_css_style(connection);
    el.attributes = attrs.to_owned();
    for p in sketch_paths {
        el.d = p;
        out.push_str(&el.render());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Arrowheads
// ---------------------------------------------------------------------------

/// Compute the rough paths for a single arrowhead.  Mirrors Go
/// `ArrowheadJS` exactly — seeds, fillStyle, coordinate arrays must match
/// the Go source or the sketched output diverges bit-for-bit.
///
/// Returns `(primary, extra)` path lists; extra is non-empty for the
/// two-piece arrowheads (CfMany, CfOne) that draw a circle alongside the
/// bar.
fn arrowhead_paths(
    arrowhead: Arrowhead,
    stroke: &str,
    stroke_width: f64,
) -> (Vec<RoughPath>, Vec<RoughPath>) {
    // Shared preset builder: everything derives from rough.js defaults and
    // then overrides just what the Go call sites pass in.
    fn base(stroke: &str, stroke_width: f64) -> Opts {
        let mut o = Opts::default_base();
        o.stroke = stroke.into();
        o.stroke_width = stroke_width;
        o
    }

    const BG_COLOR: &str = crate::color::N7;
    match arrowhead {
        Arrowhead::Arrow => {
            let mut o = base(stroke, stroke_width);
            o.seed = 3;
            let primary = rough::draw_linear_path(&[(-10.0, -4.0), (0.0, 0.0), (-10.0, 4.0)], &o);
            (to_paths(primary), Vec::new())
        }
        Arrowhead::Triangle => {
            let mut o = base(stroke, stroke_width);
            o.fill = Some(stroke.into());
            o.fill_style = "solid".into();
            o.seed = 2;
            let primary = rough::draw_polygon(&[(-10.0, -4.0), (0.0, 0.0), (-10.0, 4.0)], &o);
            (to_paths(primary), Vec::new())
        }
        Arrowhead::UnfilledTriangle => {
            let mut o = base(stroke, stroke_width);
            o.fill = Some(BG_COLOR.into());
            o.fill_style = "solid".into();
            o.seed = 2;
            let primary = rough::draw_polygon(&[(-10.0, -4.0), (0.0, 0.0), (-10.0, 4.0)], &o);
            (to_paths(primary), Vec::new())
        }
        Arrowhead::Diamond => {
            let mut o = base(stroke, stroke_width);
            o.fill = Some(BG_COLOR.into());
            o.fill_style = "solid".into();
            o.seed = 1;
            let primary = rough::draw_polygon(
                &[
                    (-20.0, 0.0),
                    (-10.0, 5.0),
                    (0.0, 0.0),
                    (-10.0, -5.0),
                    (-20.0, 0.0),
                ],
                &o,
            );
            (to_paths(primary), Vec::new())
        }
        Arrowhead::FilledDiamond => {
            let mut o = base(stroke, stroke_width);
            o.fill = Some(stroke.into());
            o.fill_style = "zigzag".into();
            o.fill_weight = 4.0;
            o.seed = 1;
            let primary = rough::draw_polygon(
                &[
                    (-20.0, 0.0),
                    (-10.0, 5.0),
                    (0.0, 0.0),
                    (-10.0, -5.0),
                    (-20.0, 0.0),
                ],
                &o,
            );
            (to_paths(primary), Vec::new())
        }
        Arrowhead::Cross => {
            let mut o = base(stroke, stroke_width);
            o.seed = 3;
            let primary = rough::draw_linear_path(
                &[
                    (-6.0, -6.0),
                    (6.0, 6.0),
                    (0.0, 0.0),
                    (-6.0, 6.0),
                    (0.0, 0.0),
                    (6.0, -6.0),
                ],
                &o,
            );
            (to_paths(primary), Vec::new())
        }
        Arrowhead::CfManyRequired => {
            let mut o = base(stroke, stroke_width);
            o.fill = Some(stroke.into());
            o.fill_style = "solid".into();
            o.fill_weight = 4.0;
            o.seed = 2;
            let primary = rough::draw_path("M-15,-10 -15,10 M0,10 -15,0 M0,-10 -15,0", &o);
            (to_paths(primary), Vec::new())
        }
        Arrowhead::CfMany => {
            let mut o = base(stroke, stroke_width);
            o.fill = Some(stroke.into());
            o.fill_style = "solid".into();
            o.fill_weight = 4.0;
            o.seed = 8;
            let primary = rough::draw_path("M0,10 -15,0 M0,-10 -15,0", &o);
            let mut o2 = base(stroke, stroke_width);
            o2.fill = Some(BG_COLOR.into());
            o2.fill_style = "solid".into();
            o2.fill_weight = 1.0;
            o2.seed = 4;
            let extra = rough::draw_circle(-20.0, 0.0, 8.0, &o2);
            (to_paths(primary), to_paths(extra))
        }
        Arrowhead::CfOneRequired => {
            let mut o = base(stroke, stroke_width);
            o.fill = Some(stroke.into());
            o.fill_style = "solid".into();
            o.fill_weight = 4.0;
            o.seed = 2;
            let primary = rough::draw_path("M-15,-10 -15,10 M-10,-10 -10,10", &o);
            (to_paths(primary), Vec::new())
        }
        Arrowhead::CfOne => {
            let mut o = base(stroke, stroke_width);
            o.fill = Some(stroke.into());
            o.fill_style = "solid".into();
            o.fill_weight = 4.0;
            o.seed = 3;
            let primary = rough::draw_path("M-10,-10 -10,10", &o);
            let mut o2 = base(stroke, stroke_width);
            o2.fill = Some(BG_COLOR.into());
            o2.fill_style = "solid".into();
            o2.fill_weight = 1.0;
            o2.seed = 5;
            let extra = rough::draw_circle(-20.0, 0.0, 8.0, &o2);
            (to_paths(primary), to_paths(extra))
        }
        Arrowhead::Circle => {
            let mut o = base(stroke, stroke_width);
            o.fill = Some(BG_COLOR.into());
            o.fill_style = "solid".into();
            o.fill_weight = 1.0;
            o.seed = 5;
            let primary = rough::draw_circle(-2.0, -1.0, 8.0, &o);
            (to_paths(primary), Vec::new())
        }
        Arrowhead::Box_ => {
            let mut o = base(stroke, stroke_width);
            o.fill = Some(BG_COLOR.into());
            o.fill_style = "solid".into();
            o.seed = 1;
            let primary = rough::draw_polygon(
                &[(0.0, -10.0), (0.0, 10.0), (-20.0, 10.0), (-20.0, -10.0)],
                &o,
            );
            (to_paths(primary), Vec::new())
        }
        Arrowhead::FilledBox => {
            let mut o = base(stroke, stroke_width);
            o.fill = Some(stroke.into());
            o.fill_style = "solid".into();
            o.seed = 1;
            let primary = rough::draw_polygon(
                &[(0.0, -10.0), (0.0, 10.0), (-20.0, 10.0), (-20.0, -10.0)],
                &o,
            );
            (to_paths(primary), Vec::new())
        }
        // None / FilledCircle / Line aren't handled in Go (silently empty).
        _ => (Vec::new(), Vec::new()),
    }
}

/// Render Go-style `%v` for floats: if the value is integer-valued, print
/// without decimals; otherwise print the shortest form that round-trips
/// (matches `strconv.FormatFloat(f, 'g', -1, 64)`).
fn fmt_v(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_owned();
    }
    if f.is_infinite() {
        return if f > 0.0 {
            "+Inf".to_owned()
        } else {
            "-Inf".to_owned()
        };
    }
    // Go `%v` keeps -0 as "-0"; handle explicitly.
    if f == 0.0 {
        return if f.is_sign_negative() {
            "-0".to_owned()
        } else {
            "0".to_owned()
        };
    }
    // Rust's default `{f}` produces the shortest round-trippable form, which
    // matches Go `%v` for float64 byte-for-byte in every call site we hit.
    format!("{f}")
}

/// Render the source + destination arrowheads, mirroring Go `Arrowheads`.
pub fn arrowheads(
    _runner: &SketchRunner,
    connection: &Connection,
    src_adj: &Point,
    dst_adj: &Point,
) -> Result<String, String> {
    let mut parts: Vec<String> = Vec::new();

    if connection.src_arrow != Arrowhead::None {
        let (mut primary, mut extra) = arrowhead_paths(
            connection.src_arrow.clone(),
            &connection.stroke,
            connection.stroke_width as f64,
        );
        if primary.is_empty() && extra.is_empty() {
            return Ok(String::new());
        }
        let route = &connection.route;
        if route.len() < 2 {
            return Ok(String::new());
        }
        let starting = Segment::new(route[0], route[1]);
        let starting_vec = starting.to_vector().reverse();
        let angle = starting_vec.degrees();
        let transform = format!(
            "transform=\"translate({:.6} {:.6}) rotate({})\"",
            starting.start.x + src_adj.x,
            starting.start.y + src_adj.y,
            fmt_v(angle)
        );
        primary.append(&mut extra);
        let mut path_el = ThemableElement::new("path", None);
        path_el.class_name = "connection".to_owned();
        path_el.attributes = transform;
        for r in primary {
            path_el.d = r.d.clone();
            path_el.fill = r.fill.clone();
            path_el.stroke = r.stroke.clone();
            path_el.style = r.style_css();
            parts.push(path_el.render());
        }
    }

    if connection.dst_arrow != Arrowhead::None {
        let (mut primary, mut extra) = arrowhead_paths(
            connection.dst_arrow.clone(),
            &connection.stroke,
            connection.stroke_width as f64,
        );
        if primary.is_empty() && extra.is_empty() {
            return Ok(String::new());
        }
        let route = &connection.route;
        if route.len() < 2 {
            return Ok(String::new());
        }
        let last = route.len() - 1;
        let ending = Segment::new(route[last - 1], route[last]);
        let ending_vec = ending.to_vector();
        let angle = ending_vec.degrees();
        let transform = format!(
            "transform=\"translate({:.6} {:.6}) rotate({})\"",
            ending.end.x + dst_adj.x,
            ending.end.y + dst_adj.y,
            fmt_v(angle)
        );
        primary.append(&mut extra);
        let mut path_el = ThemableElement::new("path", None);
        path_el.class_name = "connection".to_owned();
        path_el.attributes = transform;
        for r in primary {
            path_el.d = r.d.clone();
            path_el.fill = r.fill.clone();
            path_el.stroke = r.stroke.clone();
            path_el.style = r.style_css();
            parts.push(path_el.render());
        }
    }

    Ok(parts.join(" "))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_floats_basic() {
        assert_eq!(truncate_floats("12.3456789"), "12.345678");
        assert_eq!(truncate_floats("0.1"), "0.1");
        assert_eq!(truncate_floats("M 10.12345678 20.1"), "M 10.123456 20.1");
        assert_eq!(truncate_floats("L -5.999999999"), "L -5.999999");
        assert_eq!(truncate_floats("no numbers"), "no numbers");
        assert_eq!(
            truncate_floats("1.123456789,2.987654321"),
            "1.123456,2.987654"
        );
    }

    #[test]
    fn fmt_v_matches_go() {
        assert_eq!(fmt_v(45.0), "45");
        assert_eq!(fmt_v(-90.0), "-90");
        // When computed via (atan2 as f32 as f64) * 180 / PI, the resulting
        // float64 has a bit pattern both Go and Rust serialize as
        // "90.00000250447816" — the bit-exact canonical form.
        let y = -86.0_f64;
        let x = 0.0_f64;
        let deg = ((y.atan2(x) as f32) as f64) * 180.0 / std::f64::consts::PI;
        assert_eq!(fmt_v(deg), "-90.00000250447816");
    }
}
