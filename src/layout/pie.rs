//! Pie layout — compute slice geometry, legend placement and viewBox.
//!
//! Port of `pieRenderer.ts` (199 LoC upstream) — arc math via a
//! hand-rolled equivalent of d3-shape's `pie()` + `arc()` that
//! preserves bit-level floating-point behaviour so our output matches
//! the jsdom reference byte-for-byte.
//!
//! Portions inspired by mermaid-rs-renderer
//! (https://github.com/1jehuang/mermaid-rs-renderer) — that project
//! targets its own aesthetic, ours targets upstream mermaid byte
//! parity, so code is written from scratch against d3-shape's sources.
//!
//! The `v8_math` submodule contains a direct Rust port of V8 11.3's
//! fdlibm-derived `cos` / `sin` (`src/base/ieee754.cc`) — required
//! because neither Rust's std trig (glibc libm) nor the `libm` crate
//! (MSUN) matches V8's bit-exact output on every pie input.

use crate::error::Result;
use crate::font_metrics::text_width;
use crate::model::pie::PieDiagram;
use crate::theme::ThemeVariables;

/// Fixed layout constants from `pieRenderer.ts`.
const MARGIN: f64 = 40.0;
const LEGEND_RECT_SIZE: f64 = 18.0;
const LEGEND_SPACING: f64 = 4.0;
const HEIGHT: f64 = 450.0;
const PIE_WIDTH: f64 = 450.0;

/// Inner pie radius: `Math.min(pieWidth, height)/2 - MARGIN` = 185.
const RADIUS: f64 = 185.0;

/// A single slice in its final geometric form.
#[derive(Debug, Clone)]
pub struct SliceGeometry {
    /// Original slice label (used for legend + color mapping).
    pub label: String,
    /// Color fill (one of theme.pie1..pie12, cycling by index).
    pub fill: String,
    /// Percentage text (`"<n>%"`) — `(value/sum*100).toFixed(0)`.
    pub percent_text: String,
    /// d-attribute of the `<path>`.
    pub path_d: String,
    /// Text `transform="translate(x,y)"` centroid.
    pub centroid_x: f64,
    pub centroid_y: f64,
    /// Skip rendering — this is only here because rounded percent is "0".
    pub render_slice: bool,
}

/// A single legend row.
#[derive(Debug, Clone)]
pub struct LegendRow {
    pub label_text: String,
    pub fill: String,
    pub dx: f64,
    pub dy: f64,
}

/// Full pie layout ready for rendering.
#[derive(Debug, Clone, Default)]
pub struct PieLayout {
    pub width: f64,
    pub height: f64,
    pub viewbox_x: f64,
    pub viewbox_y: f64,
    pub viewbox_w: f64,
    pub viewbox_h: f64,
    pub slices: Vec<SliceGeometry>,
    pub legends: Vec<LegendRow>,
    /// Raw title text (empty = no title). The renderer emits `<text>` either way.
    pub title: String,
    /// `r` attribute of the outer circle: `radius + outerStrokeWidth/2`.
    pub outer_circle_r: f64,
}

pub fn layout(d: &PieDiagram, theme: &ThemeVariables) -> Result<PieLayout> {
    // ── Color palette ────────────────────────────────────────────────
    let colors: [&str; 12] = [
        theme.pie1.as_deref().unwrap_or("#ECECFF"),
        theme.pie2.as_deref().unwrap_or("#ffffde"),
        theme
            .pie3
            .as_deref()
            .unwrap_or("hsl(80, 100%, 56.2745098039%)"),
        theme
            .pie4
            .as_deref()
            .unwrap_or("hsl(240, 100%, 86.2745098039%)"),
        theme
            .pie5
            .as_deref()
            .unwrap_or("hsl(60, 100%, 63.5294117647%)"),
        theme
            .pie6
            .as_deref()
            .unwrap_or("hsl(80, 100%, 76.2745098039%)"),
        theme
            .pie7
            .as_deref()
            .unwrap_or("hsl(300, 100%, 76.2745098039%)"),
        theme
            .pie8
            .as_deref()
            .unwrap_or("hsl(180, 100%, 56.2745098039%)"),
        theme
            .pie9
            .as_deref()
            .unwrap_or("hsl(0, 100%, 56.2745098039%)"),
        theme
            .pie10
            .as_deref()
            .unwrap_or("hsl(300, 100%, 56.2745098039%)"),
        theme
            .pie11
            .as_deref()
            .unwrap_or("hsl(150, 100%, 56.2745098039%)"),
        theme
            .pie12
            .as_deref()
            .unwrap_or("hsl(0, 100%, 66.2745098039%)"),
    ];

    // ── Total sum (all sections, including ones filtered from pie) ───
    let total_sum: f64 = d.slices.iter().map(|s| s.value).sum();

    // ── Filtered-for-pie slices: (value/total_sum)*100 >= 1 ──────────
    // Note: upstream computes the `>= 1` filter against the TOTAL sum,
    // not the filtered sum. The angles are then computed from the
    // filtered subset's own sum.
    let pie_indices: Vec<usize> = if total_sum > 0.0 {
        d.slices
            .iter()
            .enumerate()
            .filter(|(_, s)| (s.value / total_sum) * 100.0 >= 1.0)
            .map(|(i, _)| i)
            .collect()
    } else {
        (0..d.slices.len()).collect()
    };
    let filtered_sum: f64 = pie_indices.iter().map(|&i| d.slices[i].value).sum();

    // ── Angles: d3 pie with no sort ──────────────────────────────────
    // a0 starts at 0, a1 = a0 + value * (tau / filtered_sum).
    use std::f64::consts::{PI, TAU};
    let k = if filtered_sum > 0.0 {
        TAU / filtered_sum
    } else {
        0.0
    };
    let mut arcs: Vec<(usize, f64, f64)> = Vec::with_capacity(pie_indices.len()); // (orig_idx, a0, a1)
    {
        let mut a0 = 0.0f64;
        for &idx in &pie_indices {
            let a1 = a0 + d.slices[idx].value * k;
            arcs.push((idx, a0, a1));
            a0 = a1;
        }
    }

    // ── Build SliceGeometry list ─────────────────────────────────────
    let label_r = RADIUS * d.text_position;
    let half_pi = PI / 2.0;

    let mut slices: Vec<SliceGeometry> = Vec::with_capacity(arcs.len());
    for &(idx, a0, a1) in &arcs {
        let slice = &d.slices[idx];
        let color = colors[idx % 12];
        // Percent uses the ORIGINAL total sum.
        let pct = if total_sum > 0.0 {
            ((slice.value / total_sum) * 100.0).round() as i64
        } else {
            0
        };
        let percent_text = format!("{pct}%");
        // Render this <path>/<text> unless percent rounds to "0".
        let render_slice = pct != 0;

        // d3-shape arc() math: a01 = startAngle - π/2, a11 = endAngle - π/2.
        let a01 = a0 - half_pi;
        let a11 = a1 - half_pi;
        let x01 = RADIUS * v8_math::cos(a01);
        let y01 = RADIUS * v8_math::sin(a01);
        let x_end = RADIUS * v8_math::cos(a11);
        let y_end = RADIUS * v8_math::sin(a11);

        // Large-arc flag: d3-path.arc emits `+(da >= pi)`.
        let da = a1 - a0;
        let large = if da >= PI { 1 } else { 0 };

        let path_d = format!(
            "M{sx},{sy}A{r},{r},0,{large},1,{ex},{ey}L0,0Z",
            sx = fmt3(x01),
            sy = fmt3(y01),
            r = fmt_int_or_trim(RADIUS),
            large = large,
            ex = fmt3(x_end),
            ey = fmt3(y_end),
        );

        // Centroid: d3 arc.centroid with innerR=outerR=label_r.
        // d3 uses `(startAngle + endAngle)/2 - π/2`. We also use `/2`
        // (not `*0.5`) to match the exact floating-point trace.
        let mid_a = (a0 + a1) / 2.0 - half_pi;
        let cx = v8_math::cos(mid_a) * label_r;
        let cy = v8_math::sin(mid_a) * label_r;

        slices.push(SliceGeometry {
            label: slice.label.clone(),
            fill: color.to_string(),
            percent_text,
            path_d,
            centroid_x: cx,
            centroid_y: cy,
            render_slice,
        });
    }

    // ── Legend rows — ALL original sections, not the filtered subset ──
    let legend_step = LEGEND_RECT_SIZE + LEGEND_SPACING; // 22
    let n = d.slices.len() as f64;
    let offset = legend_step * n / 2.0;
    let horizontal = 12.0 * LEGEND_RECT_SIZE; // 216

    let mut legends: Vec<LegendRow> = Vec::with_capacity(d.slices.len());
    for (i, s) in d.slices.iter().enumerate() {
        let color = colors[i % 12];
        let vertical = (i as f64) * legend_step - offset;
        let label_text = if d.show_data {
            format!("{} [{}]", s.label, format_value(s.value))
        } else {
            s.label.clone()
        };
        legends.push(LegendRow {
            label_text,
            fill: color.to_string(),
            dx: horizontal,
            dy: vertical,
        });
    }

    // ── Measure longest legend text at 14px / sans-serif ─────────────
    // The jsdom reference pipeline computes `.getBoundingClientRect()`
    // on legend `<text>` nodes whose font-family/size come from CSS in
    // `<style>`; jsdom does not apply stylesheets, so resolveFont falls
    // back to its defaults of 14px / "sans-serif". We mirror that
    // here — otherwise the viewBox width drifts by ~17% per glyph.
    let longest_text_width = legends
        .iter()
        .map(|r| text_width(&r.label_text, "sans-serif", 14.0, false, false))
        .fold(0.0f64, f64::max);

    let chart_and_legend_w =
        PIE_WIDTH + MARGIN + LEGEND_RECT_SIZE + LEGEND_SPACING + longest_text_width;

    // Title width with the SAME 14-px / sans-serif fallback.
    let title = d.meta.title.clone().unwrap_or_default();
    let title_width = text_width(&title, "sans-serif", 14.0, false, false);

    let title_left = PIE_WIDTH / 2.0 - title_width / 2.0;
    let title_right = PIE_WIDTH / 2.0 + title_width / 2.0;
    let viewbox_x = 0f64.min(title_left);
    let viewbox_right = chart_and_legend_w.max(title_right);
    let total_width = viewbox_right - viewbox_x;

    // Outer circle radius: r + outerStrokeWidth/2. outerStrokeWidth is
    // a CSS length — parse it with mermaid's `parseFontSize` analogue,
    // which defaults to NaN (then ??= 2) if it can't pull a leading
    // number.
    let outer_stroke_width_px = parse_leading_px(&d.outer_stroke_width).unwrap_or(2.0);
    let outer_circle_r = RADIUS + outer_stroke_width_px / 2.0;

    Ok(PieLayout {
        width: total_width,
        height: HEIGHT,
        viewbox_x,
        viewbox_y: 0.0,
        viewbox_w: total_width,
        viewbox_h: HEIGHT,
        slices,
        legends,
        title,
        outer_circle_r,
    })
}

/// Format a number as d3-path does with `digits=3`:
/// `Math.round(x * 1000) / 1000` then JS default stringification.
///
/// JS default: integers print without decimal point, otherwise the
/// shortest round-trip representation. Rust's default `f64` print uses
/// the shortest round-trip too, so `format!("{r}")` matches **except**
/// that Rust prints `-0` as `-0` while JS prints `0`. d3-path output
/// has never produced `-0` for our fixtures (all points are exact or
/// non-zero), so we don't need the guard. A `-0` for the odd case is
/// handled by the fmt function regardless.
pub fn fmt3(x: f64) -> String {
    let r = (x * 1000.0).round() / 1000.0;
    if r == 0.0 {
        return "0".to_string();
    }
    if r.fract() == 0.0 && r.is_finite() {
        return format!("{}", r as i64);
    }
    format!("{r}")
}

/// Integer formatting for path radius etc — keeps `185` as `"185"`
/// rather than `"185.0"`. For non-integer we fall back to `fmt3`.
fn fmt_int_or_trim(x: f64) -> String {
    if x.fract() == 0.0 && x.is_finite() {
        format!("{}", x as i64)
    } else {
        fmt3(x)
    }
}

/// Pull the leading number out of a CSS length like `"5px"` or `"2.5"`.
/// Returns `None` if the input doesn't start with a parseable number.
fn parse_leading_px(s: &str) -> Option<f64> {
    let bytes = s.as_bytes();
    let mut end = 0;
    let mut saw_digit = false;
    let mut saw_dot = false;
    for (j, c) in s.char_indices() {
        match c {
            '-' | '+' if j == 0 => {
                end = j + c.len_utf8();
            }
            '0'..='9' => {
                saw_digit = true;
                end = j + c.len_utf8();
            }
            '.' if !saw_dot => {
                saw_dot = true;
                end = j + c.len_utf8();
            }
            _ => break,
        }
    }
    if !saw_digit {
        return None;
    }
    let _ = bytes;
    s[..end].parse::<f64>().ok()
}

/// Format a slice value for the legend's `[N]` block.
/// Upstream JavaScript uses `${value}` which defers to `Number.prototype.toString()`
/// — integers drop their decimal point. We mirror that.
fn format_value(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Direct Rust port of V8 11.3.244's fdlibm-derived `cos` / `sin`.
///
/// Source: `v8/src/base/ieee754.cc` — specifically `__kernel_cos`,
/// `__kernel_sin` and `__ieee754_rem_pio2` (medium-size branch only;
/// pie inputs never exceed `2^19 * π/2`).
///
/// Why a bespoke port rather than `f64::cos` / `libm::cos`?
///   - `f64::cos` on Linux forwards to glibc libm, whose output differs
///     from V8's by 1 ULP on inputs like `cos(0.1)`.
///   - `libm` crate (MSUN port) matches V8 on some inputs but not
///     others (e.g. `sin(0.82279807594018405936)`).
///   - V8's port has specific adjustments (e.g. the `qx` correction in
///     `__kernel_cos` for `|x| > 0.3`) that neither the system libm
///     nor libm-crate reproduce identically.
///
/// The reference SVGs are rendered by jsdom+Node on an x86_64 Linux
/// box, where Node is built with `v8_use_libm_trig_functions = false`
/// (GCC build → fdlibm path), so V8's Math.cos/sin goes straight
/// through this exact code.
///
/// Licensing: the fdlibm portions are Copyright (C) 1993 by Sun
/// Microsystems with Google modifications. Freely usable per the SunSoft
/// licence preserved in the header comment of `ieee754.cc`. Apache-2 is
/// compatible.
use crate::math::v8_trig as v8_math;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_integers_without_decimal() {
        assert_eq!(fmt3(185.0), "185");
        assert_eq!(fmt3(-185.0), "-185");
    }
    #[test]
    fn rounds_to_three_decimals() {
        assert_eq!(fmt3(172.21164349917777), "172.212");
        assert_eq!(fmt3(-67.58808950778311), "-67.588");
    }
    #[test]
    fn parses_px_length() {
        assert_eq!(parse_leading_px("5px"), Some(5.0));
        assert_eq!(parse_leading_px("2.5"), Some(2.5));
        assert_eq!(parse_leading_px("abc"), None);
    }
}
