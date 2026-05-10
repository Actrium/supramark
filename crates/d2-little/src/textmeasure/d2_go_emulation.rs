//! D2 / Go upstream byte-equal text measurement engine.
//!
//! Ported from Go `lib/textmeasure/textmeasure.go` and `lib/textmeasure/atlas.go`.
//!
//! Strategy: byte-equal reproduction of Go upstream's `golang/freetype/truetype`
//! plus `fixed.Int26_6`. All Go-correlated intermediate values flow through
//! Int26_6 (i32 representing value * 64) integer arithmetic; conversion to
//! `f64` pixels via `i2f` happens only at the final return boundary.
//!
//! Key points:
//! * Use ttf-parser's `glyph_bounding_box` to obtain the funit-level tight
//!   control-point bounding box (equivalent to Go freetype walking every
//!   control point and `g.Bounds`-extending — ttf-parser's `OutlineBuilder`
//!   `extend_by`s every raw control point too, and the synthesized midpoints
//!   never widen the box).
//! * Per-glyph points are scaled with Go's `Font.scale` formula:
//!   `scaled = (scale_26_6 * funit + sign(funit) * fupe / 2) / fupe`,
//!   where `scale_26_6 = round(size * dpi * 64 / 72)` and `fupe` is the
//!   font's units-per-em.
//! * Floor / Ceil to integer pixel boundaries, accumulating frame / dot per
//!   Go `makeMapping`.
//! * Inside `DrawRune`, replace the rect height with `ascent + descent` to
//!   match Go.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::fonts::{FONT_FAMILIES, FONT_STYLES, Font, FontFamily, FontStyle};
use markdown::{CompileOptions, Constructs, Options, ParseOptions};
use regex::Regex;
use ttf_parser::Face;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const TAB_SIZE: f64 = 4.0;
const SIZELESS_FONT_SIZE: i32 = 0;
const REPLACEMENT_CHAR: char = '\u{FFFD}';

static HREF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"href="([^"]*)""#).expect("href regex"));

/// Default rune set baked into the atlas at construction time.
/// ASCII + Latin-1 Supplement + Geometric Shapes (matches Go `init()`).
fn default_runes() -> Vec<char> {
    let mut runes = Vec::with_capacity(512);
    for c in 0x0000u32..=0x007F {
        if let Some(ch) = char::from_u32(c) {
            runes.push(ch);
        }
    }
    for c in 0x0080u32..=0x00FF {
        if let Some(ch) = char::from_u32(c) {
            runes.push(ch);
        }
    }
    for c in 0x25A0u32..=0x25FF {
        if let Some(ch) = char::from_u32(c) {
            runes.push(ch);
        }
    }
    runes
}

// ---------------------------------------------------------------------------
// Fixed-point (Int26_6) helpers — strict reproduction of Go `fixed.Int26_6`.
// ---------------------------------------------------------------------------

/// Pixel float64 (`x / 64`) for the Int26_6 value `x`.
#[inline]
fn i2f(x: i32) -> f64 {
    x as f64 / 64.0
}

/// `fixed.I(i)`: lift integer pixels to Int26_6 (= `i * 64`).
#[inline]
fn i_pixel(i: i32) -> i32 {
    i << 6
}

/// Go `fixed.Int26_6.Floor()` — arithmetic right shift by 6.
#[inline]
fn floor_26_6(x: i32) -> i32 {
    // Rust signed-int right shift is arithmetic, matching Go.
    x >> 6
}

/// Go `fixed.Int26_6.Ceil()` — `(x + 0x3f) >> 6`.
#[inline]
fn ceil_26_6(x: i32) -> i32 {
    (x + 0x3f) >> 6
}

/// Go `truetype.Font.scale`: round `scale_26_6 * funit` by `fupe`.
///
/// ```text
/// if x >= 0 { x += fupe / 2 } else { x -= fupe / 2 }
/// return x / fupe
/// ```
///
/// Note: argument `x` is already `scale_26_6 * funit` (still at the Int26_6
/// *64 magnitude).
#[inline]
fn font_scale_div(x: i64, fupe: i32) -> i32 {
    let fupe64 = fupe as i64;
    let y = if x >= 0 {
        x + fupe64 / 2
    } else {
        x - fupe64 / 2
    };
    // Go integer division truncates toward zero for negatives — same as Rust.
    (y / fupe64) as i32
}

/// Scale a single FUnit coordinate to Int26_6 pixel units the Go-freetype way.
#[inline]
fn scale_funit_to_26_6(funit: i32, scale_26_6: i32, fupe: i32) -> i32 {
    // Go performs `Int26_6 * Int26_6` with both operands being i32 and the
    // result falling back to i32. We promote to i64 to avoid overflow. With
    // fupe < 1000, scale around 1024, and funit in the low thousands i32
    // would suffice, but i64 is safer.
    let prod = scale_26_6 as i64 * funit as i64;
    font_scale_div(prod, fupe)
}

// ---------------------------------------------------------------------------
// Rect (internal bounding-box type)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Rect {
    tl_x: f64,
    tl_y: f64,
    br_x: f64,
    br_y: f64,
}

impl Rect {
    fn zero() -> Self {
        Self {
            tl_x: 0.0,
            tl_y: 0.0,
            br_x: 0.0,
            br_y: 0.0,
        }
    }

    fn w(&self) -> f64 {
        self.br_x - self.tl_x
    }

    fn h(&self) -> f64 {
        self.br_y - self.tl_y
    }

    fn norm(self) -> Self {
        Self {
            tl_x: self.tl_x.min(self.br_x),
            tl_y: self.tl_y.min(self.br_y),
            br_x: self.tl_x.max(self.br_x),
            br_y: self.tl_y.max(self.br_y),
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            tl_x: self.tl_x.min(other.tl_x),
            tl_y: self.tl_y.min(other.tl_y),
            br_x: self.br_x.max(other.br_x),
            br_y: self.br_y.max(other.br_y),
        }
    }
}

// ---------------------------------------------------------------------------
// Glyph + Atlas
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Glyph {
    dot_x: f64,
    dot_y: f64,
    frame: Rect,
    advance: f64,
}

/// Atlas keeps the precomputed glyph metrics for one (font, size) pair.
struct Atlas {
    mapping: HashMap<char, Glyph>,
    ascent: f64,
    descent: f64,
    line_height: f64,
}

/// Per-glyph metrics in Int26_6 pixel units, post Go-freetype scaling.
#[derive(Debug, Clone, Copy)]
struct GlyphMetrics {
    /// Int26_6 glyph control-point bounding box (already scaled, with Y
    /// flipped to "positive-down"). Aligns with the `bounds` returned by
    /// Go `face.GlyphBounds`.
    bx_min: i32,
    by_min: i32,
    bx_max: i32,
    by_max: i32,
    /// Horizontal advance in Int26_6.
    advance: i32,
}

impl Atlas {
    /// Build the atlas following Go's `NewAtlas` logic.
    fn new(face: &Face<'_>, size: i32, runes: &[char]) -> Self {
        let fupe = face.units_per_em() as i32;
        // Default dpi = 72: scale = round(size * 72 * 64 / 72 + 0.5) =
        // round(size*64+0.5). For integer `size` that equals `size * 64`.
        // We mirror Go's expression here for parity.
        let scale_26_6 = (0.5 + (size as f64 * 72.0 * 64.0 / 72.0)) as i32;

        // Go `face.Metrics()`:
        //   Height  = a.scale                              (Int26_6)
        //   Ascent  = Int26_6(Ceil(scale * ascent / fupe)) (raw Int26_6 value!)
        //   Descent = Int26_6(Ceil(scale * -descent / fupe))
        let scale_f = scale_26_6 as f64;
        let ascent_raw = (scale_f * face.ascender() as f64 / fupe as f64).ceil() as i32;
        let descent_raw = (scale_f * (-face.descender() as f64) / fupe as f64).ceil() as i32;
        let ascent = i2f(ascent_raw);
        let descent = i2f(descent_raw);
        let line_height = i2f(scale_26_6);

        // Go treats Ascent / Descent as a "pixel count" added directly to
        // `dot`, so we keep the same Int26_6 raw representation as the
        // atlas layout (`face.Metrics().Ascent + face.Metrics().Descent`).
        let row_step_26_6 = ascent_raw + descent_raw;

        // --- collect runes + precompute Int26_6 metrics ---------------------
        use std::collections::HashSet;
        let mut seen: HashSet<char> = HashSet::new();
        let mut order: Vec<char> = Vec::with_capacity(runes.len() + 1);
        order.push(REPLACEMENT_CHAR);
        seen.insert(REPLACEMENT_CHAR);
        for &r in runes {
            if seen.insert(r) {
                order.push(r);
            }
        }

        // Keep only glyphs whose metrics resolved successfully (Go skips
        // entries when `face.GlyphBounds` returns ok=false).
        let mut metrics: HashMap<char, GlyphMetrics> = HashMap::new();
        let mut valid_runes: Vec<char> = Vec::with_capacity(order.len());
        for r in order {
            if let Some(m) = compute_glyph_metrics(face, r, scale_26_6, fupe) {
                metrics.insert(r, m);
                valid_runes.push(r);
            }
        }

        // --- run Go's makeSquareMapping -------------------------------------
        // This only affects the atlas Y coordinates: rows wrap when width
        // is reached. Final `MeasurePrecise` results barely change, but we
        // still execute it to stay byte-equal with Go.
        let padding_26_6 = i_pixel(2);
        let lo_init = 0i32;
        let hi_init = i_pixel(1024 * 1024);
        let mut lo = lo_init;
        let mut hi = hi_init;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let (_mapping, bounds) =
                make_mapping(&valid_runes, &metrics, padding_26_6, mid, row_step_26_6);
            let bw = bounds.max_x - bounds.min_x;
            let bh = bounds.max_y - bounds.min_y;
            if bw >= bh {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        let best_width = lo;
        let (fixed_mapping, fixed_bounds) = make_mapping(
            &valid_runes,
            &metrics,
            padding_26_6,
            best_width,
            row_step_26_6,
        );

        // Convert the Int26_6 mapping into f64 pixels and flip Y (same as
        // Go atlas.go).
        let bounds_tl_y = i2f(fixed_bounds.min_y);
        let bounds_br_y = i2f(fixed_bounds.max_y);

        let mut mapping: HashMap<char, Glyph> = HashMap::new();
        for (r, fg) in fixed_mapping {
            let dot_x = i2f(fg.dot_x);
            let dot_y = bounds_br_y - (i2f(fg.dot_y) - bounds_tl_y);

            let frame_tl_x = i2f(fg.frame_min_x);
            let frame_tl_y = bounds_br_y - (i2f(fg.frame_min_y) - bounds_tl_y);
            let frame_br_x = i2f(fg.frame_max_x);
            let frame_br_y = bounds_br_y - (i2f(fg.frame_max_y) - bounds_tl_y);

            let frame = Rect {
                tl_x: frame_tl_x,
                tl_y: frame_tl_y,
                br_x: frame_br_x,
                br_y: frame_br_y,
            }
            .norm();

            mapping.insert(
                r,
                Glyph {
                    dot_x,
                    dot_y,
                    frame,
                    advance: i2f(fg.advance),
                },
            );
        }

        Self {
            mapping,
            ascent,
            descent,
            line_height,
        }
    }

    fn contains(&self, r: char) -> bool {
        self.mapping.contains_key(&r)
    }

    fn glyph(&self, r: char) -> Glyph {
        self.mapping
            .get(&r)
            .copied()
            .unwrap_or_else(|| self.mapping[&REPLACEMENT_CHAR])
    }

    /// Mirrors Go freetype's `Face.Kern` — only reads the legacy `kern`
    /// table. Always 0 for fonts without that table (e.g. Source Sans Pro).
    fn kern(&self, _r0: char, _r1: char) -> f64 {
        0.0
    }

    /// Draw one rune. Returns (rect, frame, bounds, new_dot_x, new_dot_y).
    fn draw_rune(
        &self,
        prev_r: Option<char>,
        r: char,
        dot_x: f64,
        dot_y: f64,
    ) -> (Rect, Rect, Rect, f64, f64) {
        let r = if self.contains(r) {
            r
        } else {
            REPLACEMENT_CHAR
        };
        if !self.contains(REPLACEMENT_CHAR) {
            return (Rect::zero(), Rect::zero(), Rect::zero(), dot_x, dot_y);
        }

        let mut dx = dot_x;
        let dy = dot_y;

        if let Some(prev) = prev_r {
            let prev_eff = if self.contains(prev) {
                prev
            } else {
                REPLACEMENT_CHAR
            };
            dx += self.kern(prev_eff, r);
        }

        let glyph = self.glyph(r);

        let sub_x = dx - glyph.dot_x;
        let sub_y = dy - glyph.dot_y;

        let rect2 = Rect {
            tl_x: glyph.frame.tl_x + sub_x,
            tl_y: glyph.frame.tl_y + sub_y,
            br_x: glyph.frame.br_x + sub_x,
            br_y: glyph.frame.br_y + sub_y,
        };

        let mut bounds = rect2;
        if bounds.w() * bounds.h() != 0.0 {
            bounds = Rect {
                tl_x: bounds.tl_x,
                tl_y: dy - self.descent,
                br_x: bounds.br_x,
                br_y: dy + self.ascent,
            };
        }

        let new_dx = dx + glyph.advance;
        (rect2, glyph.frame, bounds, new_dx, dy)
    }
}

// ---------------------------------------------------------------------------
// make_mapping — corresponds to Go `makeMapping`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct FixedGlyph {
    dot_x: i32,
    dot_y: i32,
    frame_min_x: i32,
    frame_min_y: i32,
    frame_max_x: i32,
    frame_max_y: i32,
    advance: i32,
}

#[derive(Debug, Clone, Copy, Default)]
struct FixedBounds {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

impl FixedBounds {
    fn union_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        // Go's Union differs slightly between "current rect non-empty" and
        // "empty" cases, but it has no direct effect on measurements (the
        // bounds==0 branch in `measure` guards it). We use a consistent
        // "empty -> replace" rule that matches `fixed.Rectangle26_6{}.Union`.
        if self.min_x == 0 && self.min_y == 0 && self.max_x == 0 && self.max_y == 0 {
            self.min_x = x0;
            self.min_y = y0;
            self.max_x = x1;
            self.max_y = y1;
            return;
        }
        if x0 < self.min_x {
            self.min_x = x0;
        }
        if y0 < self.min_y {
            self.min_y = y0;
        }
        if x1 > self.max_x {
            self.max_x = x1;
        }
        if y1 > self.max_y {
            self.max_y = y1;
        }
    }
}

fn make_mapping(
    runes: &[char],
    metrics: &HashMap<char, GlyphMetrics>,
    padding_26_6: i32,
    width_26_6: i32,
    row_step_26_6: i32,
) -> (HashMap<char, FixedGlyph>, FixedBounds) {
    let mut mapping: HashMap<char, FixedGlyph> = HashMap::new();
    let mut bounds = FixedBounds::default();

    let mut dot_x = 0i32;
    let mut dot_y = 0i32;

    for &r in runes {
        let m = match metrics.get(&r) {
            Some(m) => m,
            None => continue,
        };

        // Floor / Ceil-align to integer pixels (Int26_6 still stores them
        // as multiples of 64).
        let frame_min_x_0 = i_pixel(floor_26_6(m.bx_min));
        let frame_min_y_0 = i_pixel(floor_26_6(m.by_min));
        let frame_max_x_0 = i_pixel(ceil_26_6(m.bx_max));
        let frame_max_y_0 = i_pixel(ceil_26_6(m.by_max));

        // dot.X -= frame.Min.X
        dot_x -= frame_min_x_0;

        // frame = frame.Add(dot)
        let frame_min_x = frame_min_x_0 + dot_x;
        let frame_min_y = frame_min_y_0 + dot_y;
        let frame_max_x = frame_max_x_0 + dot_x;
        let frame_max_y = frame_max_y_0 + dot_y;

        mapping.insert(
            r,
            FixedGlyph {
                dot_x,
                dot_y,
                frame_min_x,
                frame_min_y,
                frame_max_x,
                frame_max_y,
                advance: m.advance,
            },
        );

        bounds.union_rect(frame_min_x, frame_min_y, frame_max_x, frame_max_y);

        // dot.X = frame.Max.X
        dot_x = frame_max_x;
        // padding + align to integer pixel
        dot_x += padding_26_6;
        dot_x = i_pixel(ceil_26_6(dot_x));

        // wrap to next row when the width is exceeded
        if frame_max_x >= width_26_6 {
            dot_x = 0;
            dot_y += row_step_26_6;
            dot_y += padding_26_6;
            dot_y = i_pixel(ceil_26_6(dot_y));
        }
    }

    (mapping, bounds)
}

/// Scaled-bounds output for a glyph. `b*` are Int26_6 values in Go's
/// Y-inverted coordinate system (`xmin = +Min.X`, `ymin = -Max.Y`,
/// `xmax = +Max.X`, `ymax = -Min.Y`).
#[derive(Clone, Copy, Debug, Default)]
struct ScaledBounds {
    has_any: bool,
    x_min: i32,
    y_min: i32,
    x_max: i32,
    y_max: i32,
}

impl ScaledBounds {
    fn ingest(&mut self, x: i32, y: i32) {
        if !self.has_any {
            self.has_any = true;
            self.x_min = x;
            self.x_max = x;
            self.y_min = y;
            self.y_max = y;
            return;
        }
        if x < self.x_min {
            self.x_min = x;
        }
        if x > self.x_max {
            self.x_max = x;
        }
        if y < self.y_min {
            self.y_min = y;
        }
        if y > self.y_max {
            self.y_max = y;
        }
    }

    fn union(&mut self, other: &ScaledBounds) {
        if !other.has_any {
            return;
        }
        self.ingest(other.x_min, other.y_min);
        self.ingest(other.x_max, other.y_max);
    }

    fn shift(&self, dx: i32, dy: i32) -> ScaledBounds {
        ScaledBounds {
            has_any: self.has_any,
            x_min: self.x_min + dx,
            x_max: self.x_max + dx,
            y_min: self.y_min + dy,
            y_max: self.y_max + dy,
        }
    }
}

/// Read the raw glyf slice for the given gid so we can detect compound
/// glyphs (numContours < 0) and iterate their component records. Simple
/// glyphs use ttf-parser's regular bounding-box path.
fn get_glyf_slice<'a>(face: &Face<'a>, gid: ttf_parser::GlyphId) -> Option<&'a [u8]> {
    // ttf-parser doesn't expose a public helper to grab the raw glyf
    // range, so pull it out of the RawFace table list. We look up the
    // `loca` and `glyf` tables manually and index into them.
    let raw = face.raw_face();
    let loca_data = raw.table(ttf_parser::Tag::from_bytes(b"loca"))?;
    let glyf_data = raw.table(ttf_parser::Tag::from_bytes(b"glyf"))?;
    let head_data = raw.table(ttf_parser::Tag::from_bytes(b"head"))?;
    // head.indexToLocFormat at byte 50 (u16)
    if head_data.len() < 52 {
        return None;
    }
    let loca_fmt = u16::from_be_bytes([head_data[50], head_data[51]]);
    let idx = gid.0 as usize;
    let (start, end) = if loca_fmt == 0 {
        // short format: u16 offsets * 2
        if loca_data.len() < 2 * idx + 4 {
            return None;
        }
        let a = u16::from_be_bytes([loca_data[2 * idx], loca_data[2 * idx + 1]]) as usize * 2;
        let b = u16::from_be_bytes([loca_data[2 * idx + 2], loca_data[2 * idx + 3]]) as usize * 2;
        (a, b)
    } else {
        if loca_data.len() < 4 * idx + 8 {
            return None;
        }
        let a = u32::from_be_bytes([
            loca_data[4 * idx],
            loca_data[4 * idx + 1],
            loca_data[4 * idx + 2],
            loca_data[4 * idx + 3],
        ]) as usize;
        let b = u32::from_be_bytes([
            loca_data[4 * idx + 4],
            loca_data[4 * idx + 5],
            loca_data[4 * idx + 6],
            loca_data[4 * idx + 7],
        ]) as usize;
        (a, b)
    };
    if start >= end || end > glyf_data.len() {
        return None;
    }
    Some(&glyf_data[start..end])
}

/// Compute Go-compatible `glyphBuf.Bounds` for a single glyph (scaled
/// to Int26_6) by walking its compound components when necessary and
/// applying `roundXYToGrid` to the component translations. Returns the
/// bounds in the *pre-Y-inversion* coordinate system — callers must
/// still apply the `(xmin=+Min.X, ymin=-Max.Y, …)` flip before storing
/// `GlyphMetrics`.
fn compute_glyph_bounds_scaled(
    face: &Face<'_>,
    gid: ttf_parser::GlyphId,
    scale_26_6: i32,
    fupe: i32,
    recursion: u32,
) -> ScaledBounds {
    if recursion >= 32 {
        return ScaledBounds::default();
    }
    let gd = match get_glyf_slice(face, gid) {
        Some(d) => d,
        None => return ScaledBounds::default(),
    };
    if gd.len() < 10 {
        return ScaledBounds::default();
    }
    let ne = i16::from_be_bytes([gd[0], gd[1]]);
    if ne >= 0 {
        // Simple glyph: ttf-parser's glyph_bounding_box already returns
        // the control-point rectangle in funit. Scale it and return.
        let bb = match face.glyph_bounding_box(gid) {
            Some(b) => b,
            None => return ScaledBounds::default(),
        };
        let mut out = ScaledBounds::default();
        out.ingest(
            scale_funit_to_26_6(bb.x_min as i32, scale_26_6, fupe),
            scale_funit_to_26_6(bb.y_min as i32, scale_26_6, fupe),
        );
        out.ingest(
            scale_funit_to_26_6(bb.x_max as i32, scale_26_6, fupe),
            scale_funit_to_26_6(bb.y_max as i32, scale_26_6, fupe),
        );
        return out;
    }

    // Compound glyph: walk each component record, load its own bounds
    // recursively, apply the transform/translation and merge.
    const FLAG_ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
    const FLAG_ARGS_ARE_XY_VALUES: u16 = 0x0002;
    const FLAG_ROUND_XY_TO_GRID: u16 = 0x0004;
    const FLAG_WE_HAVE_A_SCALE: u16 = 0x0008;
    const FLAG_MORE_COMPONENTS: u16 = 0x0020;
    const FLAG_WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
    const FLAG_WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;

    let mut out = ScaledBounds::default();
    let mut p = 10usize;
    loop {
        if p + 4 > gd.len() {
            break;
        }
        let flags = u16::from_be_bytes([gd[p], gd[p + 1]]);
        let comp_idx = u16::from_be_bytes([gd[p + 2], gd[p + 3]]);
        p += 4;

        let (dx_raw, dy_raw) = if flags & FLAG_ARG_1_AND_2_ARE_WORDS != 0 {
            if p + 4 > gd.len() {
                break;
            }
            let dx = i16::from_be_bytes([gd[p], gd[p + 1]]) as i32;
            let dy = i16::from_be_bytes([gd[p + 2], gd[p + 3]]) as i32;
            p += 4;
            (dx, dy)
        } else {
            if p + 2 > gd.len() {
                break;
            }
            let dx = (gd[p] as i8) as i32;
            let dy = (gd[p + 1] as i8) as i32;
            p += 2;
            (dx, dy)
        };

        // We only support `args are XY values` + no transform (the
        // common case for Source Sans Pro). Glyphs requesting a point
        // matching / scale / 2x2 transform fall through to the
        // component's bounds without adjustment — they're rare enough
        // that we'd rather log an approximation than panic.
        let has_transform = flags
            & (FLAG_WE_HAVE_A_SCALE | FLAG_WE_HAVE_AN_X_AND_Y_SCALE | FLAG_WE_HAVE_A_TWO_BY_TWO)
            != 0;
        if has_transform {
            // Skip past the transform bytes.
            let skip = if flags & FLAG_WE_HAVE_A_SCALE != 0 {
                2
            } else if flags & FLAG_WE_HAVE_AN_X_AND_Y_SCALE != 0 {
                4
            } else {
                8
            };
            p += skip;
        }

        let component_bounds = compute_glyph_bounds_scaled(
            face,
            ttf_parser::GlyphId(comp_idx),
            scale_26_6,
            fupe,
            recursion + 1,
        );

        if flags & FLAG_ARGS_ARE_XY_VALUES != 0 {
            // Translate by (dx, dy). Go scales the raw Int26_6 dx by
            // `font.scale(g.scale * dx)` which is effectively
            // `scale_funit_to_26_6(dx_raw, scale_26_6, fupe)`.
            let mut dx_scaled = scale_funit_to_26_6(dx_raw, scale_26_6, fupe);
            let mut dy_scaled = scale_funit_to_26_6(dy_raw, scale_26_6, fupe);
            if flags & FLAG_ROUND_XY_TO_GRID != 0 {
                // `(v + 32) &^ 63` — round to the nearest integer pixel
                // boundary in Int26_6.
                dx_scaled = (dx_scaled + 32) & !63;
                dy_scaled = (dy_scaled + 32) & !63;
            }
            let shifted = component_bounds.shift(dx_scaled, dy_scaled);
            out.union(&shifted);
        } else {
            // Point matching: not yet supported, just merge without
            // translation. Should not affect Source Sans Pro tests.
            out.union(&component_bounds);
        }

        if flags & FLAG_MORE_COMPONENTS == 0 {
            break;
        }
    }

    out
}

/// Build `GlyphMetrics` (Int26_6 control-box + advance) for a char,
/// matching Go freetype's `face.GlyphBounds` output — including the
/// `roundXYToGrid` adjustment applied by compound glyph records. Using
/// ttf-parser's raw `glyph_bounding_box` for compound glyphs is off by
/// up to one pixel because it skips that rounding step.
fn compute_glyph_metrics(
    face: &Face<'_>,
    ch: char,
    scale_26_6: i32,
    fupe: i32,
) -> Option<GlyphMetrics> {
    let gid = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
    let advance_funit = face.glyph_hor_advance(gid)? as i32;
    let advance = scale_funit_to_26_6(advance_funit, scale_26_6, fupe);

    let bounds = compute_glyph_bounds_scaled(face, gid, scale_26_6, fupe, 0);
    if !bounds.has_any {
        // Go returns (0,0,0,0) + the normal advance for glyphs with no
        // outline (e.g. space). Mirror that so `.w() * .h() == 0` falls
        // through in the caller.
        return Some(GlyphMetrics {
            bx_min: 0,
            by_min: 0,
            bx_max: 0,
            by_max: 0,
            advance,
        });
    }

    // Go stores glyph bounds in a Y-inverted coordinate system:
    //   xmin = +Min.X, ymin = -Max.Y, xmax = +Max.X, ymax = -Min.Y
    let bx_min = bounds.x_min;
    let bx_max = bounds.x_max;
    let by_min = -bounds.y_max;
    let by_max = -bounds.y_min;

    if bx_min > bx_max || by_min > by_max {
        return None;
    }

    Some(GlyphMetrics {
        bx_min,
        by_min,
        bx_max,
        by_max,
        advance,
    })
}

// ---------------------------------------------------------------------------
// D2GoEmulationRuler
// ---------------------------------------------------------------------------

/// Text-measurement Ruler — keeps one Atlas per (family, style, size).
///
/// Renamed from the historical `Ruler` to flag this as the byte-equal
/// reproduction of Go upstream's freetype + Int26_6 path. d2 layout uses
/// this concrete type directly; see
/// [`crate::textmeasure::D2GoEmulationMetrics`] for the
/// `font_metrics_core::Metrics` adapter that wasm production wiring
/// dispatches through.
pub struct D2GoEmulationRuler {
    orig_x: f64,
    orig_y: f64,
    dot_x: f64,
    dot_y: f64,
    pub line_height_factor: f64,

    line_heights: HashMap<FontKey, f64>,
    tab_widths: HashMap<FontKey, f64>,
    atlases: HashMap<FontKey, Atlas>,
    /// Raw TTF bytes (keyed by family+style; size-independent).
    ttfs: HashMap<FontKey, &'static [u8]>,

    prev_r: Option<char>,
    bounds: Rect,
    /// Sibling [`super::d2_emulation_metrics`] toggles this when entering
    /// markdown / mono measurement so its [`super::D2Metrics`] impl can
    /// drive the trait-generic markdown walker without going through
    /// `Self::measure_markdown` (which needs `&mut Self`, incompatible
    /// with the `&self` interior-mutability shape of the trait).
    pub(super) bounds_with_dot: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FontKey {
    family: FontFamily,
    style: FontStyle,
    size: i32,
}

impl From<Font> for FontKey {
    fn from(f: Font) -> Self {
        Self {
            family: f.family,
            style: f.style,
            size: f.size,
        }
    }
}

impl FontKey {
    fn sizeless(self) -> Self {
        Self {
            size: SIZELESS_FONT_SIZE,
            ..self
        }
    }
}

impl D2GoEmulationRuler {
    /// Construct a Ruler and load every built-in font's TTF bytes.
    pub fn new() -> Result<Self, String> {
        let mut ttfs: HashMap<FontKey, &'static [u8]> = HashMap::new();

        for &family in FONT_FAMILIES {
            for &style in FONT_STYLES {
                let key = FontKey {
                    family,
                    style,
                    size: SIZELESS_FONT_SIZE,
                };
                if ttfs.contains_key(&key) {
                    continue;
                }
                let face_data = crate::fonts::lookup_font_face(family, style);
                // Trial-parse to make sure the bytes are valid.
                Face::parse(face_data, 0)
                    .map_err(|e| format!("failed to parse font {:?} {:?}: {}", family, style, e))?;
                ttfs.insert(key, face_data);
            }
        }

        Ok(Self {
            orig_x: 0.0,
            orig_y: 0.0,
            dot_x: 0.0,
            dot_y: 0.0,
            line_height_factor: 1.0,
            line_heights: HashMap::new(),
            tab_widths: HashMap::new(),
            atlases: HashMap::new(),
            ttfs,
            prev_r: None,
            bounds: Rect::zero(),
            bounds_with_dot: false,
        })
    }

    fn add_font_size(&mut self, font: Font) {
        let key = FontKey::from(font);
        let sizeless = key.sizeless();
        let runes = default_runes();

        let data = self.ttfs[&sizeless];
        let face = Face::parse(data, 0).expect("previously validated");
        let atlas = Atlas::new(&face, font.size, &runes);

        let lh = atlas.line_height;
        let tw = atlas.glyph(' ').advance * TAB_SIZE;

        self.line_heights.insert(key, lh);
        self.tab_widths.insert(key, tw);
        self.atlases.insert(key, atlas);
    }

    fn clear(&mut self) {
        self.prev_r = None;
        self.bounds = Rect::zero();
        self.dot_x = self.orig_x;
        self.dot_y = self.orig_y;
    }

    fn control_rune(&self, r: char, dot_x: f64, dot_y: f64, font: Font) -> Option<(f64, f64)> {
        let key = FontKey::from(font);
        match r {
            '\n' => {
                let new_x = self.orig_x;
                let new_y = dot_y - self.line_height_factor * self.line_heights[&key];
                Some((new_x, new_y))
            }
            '\r' => Some((self.orig_x, dot_y)),
            '\t' => {
                let tw = self.tab_widths[&key];
                let mut rem = (dot_x - self.orig_x) % tw;
                rem = rem % (rem + tw);
                if rem == 0.0 {
                    rem = tw;
                }
                Some((dot_x + rem, dot_y))
            }
            _ => None,
        }
    }

    fn draw_buf(&mut self, font: Font, text: &str) {
        let key = FontKey::from(font);
        for ch in text.chars() {
            if let Some((nx, ny)) = self.control_rune(ch, self.dot_x, self.dot_y, font) {
                self.dot_x = nx;
                self.dot_y = ny;
                continue;
            }

            let (_, _, bounds, new_dx, new_dy) =
                self.atlases[&key].draw_rune(self.prev_r, ch, self.dot_x, self.dot_y);

            self.prev_r = Some(ch);
            self.dot_x = new_dx;
            self.dot_y = new_dy;

            if self.bounds_with_dot {
                let dot_rect = Rect {
                    tl_x: self.dot_x,
                    tl_y: self.dot_y,
                    br_x: self.dot_x,
                    br_y: self.dot_y,
                };
                self.bounds = self.bounds.union(dot_rect);
                self.bounds = self.bounds.union(bounds);
            } else if self.bounds.w() * self.bounds.h() == 0.0 {
                self.bounds = bounds;
            } else {
                self.bounds = self.bounds.union(bounds);
            }
        }
    }

    /// Precise text measurement: returns floating-point width and height.
    pub fn measure_precise(&mut self, font: Font, s: &str) -> (f64, f64) {
        let key = FontKey::from(font);
        if !self.atlases.contains_key(&key) {
            self.add_font_size(font);
        }
        self.clear();
        self.draw_buf(font, s);
        (self.bounds.w(), self.bounds.h())
    }

    /// Return the (ascent, descent) in CSS pixels for the font's atlas.
    /// Forces atlas creation if not already cached. Required by the
    /// `font_metrics_core::Metrics` adapter (`D2GoEmulationMetrics`)
    /// which needs face-level vertical metrics in addition to width.
    pub fn face_metrics_for(&mut self, font: Font) -> (f64, f64) {
        let key = FontKey::from(font);
        if !self.atlases.contains_key(&key) {
            self.add_font_size(font);
        }
        let atlas = &self.atlases[&key];
        (atlas.ascent, atlas.descent)
    }

    /// Measure text: ceil to i32 and apply the non-BMP composite-grapheme
    /// (e.g. emoji) adjustment.
    pub fn measure(&mut self, font: Font, s: &str) -> (i32, i32) {
        let (w, h) = self.measure_precise(font, s);
        let w = self.scale_unicode(w, font, s);
        (w.ceil() as i32, h.ceil() as i32)
    }

    /// Mono mode: include `dot` in the bounds.
    pub fn measure_mono(&mut self, font: Font, s: &str) -> (i32, i32) {
        let orig = self.bounds_with_dot;
        self.bounds_with_dot = true;
        let result = self.measure(font, s);
        self.bounds_with_dot = orig;
        result
    }

    /// Width of the space glyph in the supplied font, in pixels.
    pub fn space_width(&mut self, font: Font) -> f64 {
        let key = FontKey::from(font);
        if !self.atlases.contains_key(&key) {
            self.add_font_size(font);
        }
        self.atlases[&key].glyph(' ').advance
    }

    /// Read the current line-height factor (defaults to 1.0). Mutated for
    /// markdown / code-block measurement.
    pub fn line_height_factor(&self) -> f64 {
        self.line_height_factor
    }

    /// Override the line-height factor used by [`Self::draw_buf`] when
    /// advancing past a `\n`. Callers usually save the previous value and
    /// restore it after the temporary override.
    pub fn set_line_height_factor(&mut self, value: f64) {
        self.line_height_factor = value;
    }

    /// CJK width-fallback heuristic: replace the measured Latin-fallback
    /// width with `space_width(mono) * unicode_width` for non-1-cell
    /// graphemes. Returns the corrected width.
    pub fn scale_unicode(&mut self, mut w: f64, font: Font, s: &str) -> f64 {
        let grapheme_count = s.graphemes(true).count();
        if grapheme_count != s.len() {
            for line in s.split('\n') {
                let (line_w, _) = self.measure_precise(font, line);
                let mut adjusted_w = line_w;

                let mono = Font::new(FontFamily::SourceCodePro, font.style, font.size);
                for grapheme in line.graphemes(true) {
                    let unicode_width = UnicodeWidthStr::width(grapheme);
                    if unicode_width == 1 {
                        continue;
                    }

                    let key = FontKey::from(font);
                    let mut prev_r: Option<char> = None;
                    let dot_x_start = self.orig_x;
                    let dot_y_start = self.orig_y;
                    let mut dx = dot_x_start;
                    let mut dy = dot_y_start;
                    let mut b = Rect::zero();

                    for ch in grapheme.chars() {
                        if let Some((nx, ny)) = self.control_rune(ch, dx, dy, font) {
                            dx = nx;
                            dy = ny;
                            continue;
                        }
                        let (_, _, bounds, new_dx, new_dy) =
                            self.atlases[&key].draw_rune(prev_r, ch, dx, dy);
                        b = b.union(bounds);
                        prev_r = Some(ch);
                        dx = new_dx;
                        dy = new_dy;
                    }

                    adjusted_w -= b.w();
                    adjusted_w += self.space_width(mono) * unicode_width as f64;
                }

                w = w.max(adjusted_w);
            }
        }
        w
    }
}

// Markdown measurement now lives on
// [`super::D2GoEmulationMetrics::measure_markdown`] (D2Metrics impl).
// The trait-generic walker in super::markdown takes &dyn D2Metrics and
// re-borrows the underlying ruler through the metrics adapter for every
// inner call, so the old &mut Self entry point is no longer needed.

// ---------------------------------------------------------------------------
// Markdown render — `markdown` (commonmark + gfm) → sanitised HTML.
// ---------------------------------------------------------------------------

fn markdown_options() -> Options {
    Options {
        parse: ParseOptions {
            constructs: Constructs {
                gfm_strikethrough: true,
                gfm_table: true,
                ..Constructs::default()
            },
            ..ParseOptions::default()
        },
        compile: CompileOptions {
            allow_dangerous_html: true,
            allow_dangerous_protocol: true,
            ..CompileOptions::default()
        },
    }
}

fn sanitize_links(input: &str) -> String {
    HREF_RE
        .replace_all(input, |caps: &regex::Captures<'_>| {
            let value = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let value = value.replace("&amp;", "TEMP_AMP");
            let value = value.replace('&', "&amp;");
            let value = value.replace("TEMP_AMP", "&amp;");
            format!(r#"href="{}""#, value)
        })
        .into_owned()
}

pub(super) fn render_markdown(input: &str) -> Result<String, String> {
    let rendered = markdown::to_html_with_options(input, &markdown_options())
        .map_err(|e| format!("markdown render failed: {e}"))?;
    let mut rendered = sanitize_links(&rendered);
    if !rendered.is_empty() && !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    Ok(rendered)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::*;

    #[test]
    fn test_ruler_creation() {
        let ruler = D2GoEmulationRuler::new();
        assert!(ruler.is_ok());
    }

    #[test]
    fn test_measure_precise_basic() {
        let mut ruler = D2GoEmulationRuler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);
        let (w, h) = ruler.measure_precise(font, "Hello");
        assert!(w > 0.0, "width should be positive, got {}", w);
        assert!(h > 0.0, "height should be positive, got {}", h);
    }

    /// Go golden values for "Hello" — the core correctness gauge for this crate.
    #[test]
    fn test_measure_hello_matches_go() {
        let mut ruler = D2GoEmulationRuler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);
        let (w, h) = ruler.measure_precise(font, "Hello");
        assert_eq!(w, 33.53125, "width of 'Hello' should match Go: got {}", w);
        assert_eq!(h, 20.125, "height of 'Hello' should match Go: got {}", h);
    }

    #[test]
    fn test_measure_single_chars_match_go() {
        let mut ruler = D2GoEmulationRuler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);

        let cases: &[(&str, f64, f64)] = &[
            ("a", 7.0, 20.125),
            ("b", 8.0, 20.125),
            ("c", 7.0, 20.125),
            ("h", 7.0, 20.125),
            ("l", 3.0, 20.125),
        ];
        for (s, ew, eh) in cases {
            let (w, h) = ruler.measure_precise(font, s);
            assert_eq!(w, *ew, "width of '{}' mismatch: got {}", s, w);
            assert_eq!(h, *eh, "height of '{}' mismatch: got {}", s, h);
        }
    }

    #[test]
    fn test_measure_hello_world_matches_go() {
        let mut ruler = D2GoEmulationRuler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);
        let (w, _h) = ruler.measure_precise(font, "Hello World");
        assert_eq!(w, 76.28125, "width of 'Hello World' mismatch: got {}", w);
    }

    #[test]
    fn test_measure_increasing_chars() {
        let mut ruler = D2GoEmulationRuler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);
        let text = "abcdefghij";
        for i in 1..text.len() {
            let (w1, h1) = ruler.measure(font, &text[..i]);
            let (w2, h2) = ruler.measure(font, &text[..i + 1]);
            assert_eq!(h1, h2, "height should not change for single line");
            assert!(
                w1 < w2,
                "width should increase: '{}' ({}) vs '{}' ({})",
                &text[..i],
                w1,
                &text[..i + 1],
                w2
            );
        }
    }

    #[test]
    fn test_measure_newlines_increase_height() {
        let mut ruler = D2GoEmulationRuler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);
        let (_, h1) = ruler.measure(font, "Hello");
        let (_, h2) = ruler.measure(font, "Hello\nWorld");
        assert!(h2 > h1, "newline should increase height: {} vs {}", h1, h2);
    }

    #[test]
    fn test_font_sizes_increasing() {
        let mut ruler = D2GoEmulationRuler::new().unwrap();
        let text = "The quick brown fox";
        for i in 0..FONT_SIZES.len() - 1 {
            let f1 = FontFamily::SourceSansPro.font(FONT_SIZES[i], FontStyle::Regular);
            let f2 = FontFamily::SourceSansPro.font(FONT_SIZES[i + 1], FontStyle::Regular);
            let (w1, h1) = ruler.measure(f1, text);
            let (w2, h2) = ruler.measure(f2, text);
            assert!(
                w1 < w2,
                "larger font size should produce wider text: size {} ({}) vs size {} ({})",
                FONT_SIZES[i],
                w1,
                FONT_SIZES[i + 1],
                w2
            );
            assert!(
                h1 < h2,
                "larger font size should produce taller text: size {} ({}) vs size {} ({})",
                FONT_SIZES[i],
                h1,
                FONT_SIZES[i + 1],
                h2
            );
        }
    }

    #[test]
    fn test_measure_empty_string() {
        let mut ruler = D2GoEmulationRuler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);
        let (w, h) = ruler.measure(font, "");
        assert_eq!(w, 0);
        assert_eq!(h, 0);
    }

    #[test]
    fn test_measure_single_chars() {
        let mut ruler = D2GoEmulationRuler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);

        let (w, h) = ruler.measure(font, "a");
        assert!(w > 0, "single 'a' width should be > 0, got {}", w);
        assert!(h > 0, "single 'a' height should be > 0, got {}", h);

        let (wa, _) = ruler.measure(font, "a");
        let (ww, _) = ruler.measure(font, "w");
        assert!(
            ww >= wa,
            "'w' should be at least as wide as 'a': {} vs {}",
            ww,
            wa
        );
    }

    #[test]
    fn test_measure_cjk_matches_go_fixture() {
        let mut ruler = D2GoEmulationRuler::new().unwrap();

        let font_16 = FontFamily::SourceSansPro.font(16, FontStyle::Regular);
        // Japanese kanji "Soushinki" (transmitter): 3 CJK Unified Ideographs.
        let (w16, h16) = ruler.measure(font_16, "\u{9001}\u{4FE1}\u{6A5F}");
        assert_eq!(w16, 58);
        assert_eq!(h16, 21);

        let font_24 = FontFamily::SourceSansPro.font(24, FontStyle::Regular);
        // Katakana "online portal": 9 graphemes including a prolonged sound mark.
        let (w24, h24) = ruler.measure(
            font_24,
            "\u{30AA}\u{30F3}\u{30E9}\u{30A4}\u{30F3}\u{30DD}\u{30FC}\u{30BF}\u{30EB}",
        );
        assert_eq!(w24, 271);
        assert_eq!(h24, 31);
    }
}
