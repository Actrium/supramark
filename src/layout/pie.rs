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
        theme.pie3.as_deref().unwrap_or("hsl(80, 100%, 56.2745098039%)"),
        theme.pie4.as_deref().unwrap_or("hsl(240, 100%, 86.2745098039%)"),
        theme.pie5.as_deref().unwrap_or("hsl(60, 100%, 63.5294117647%)"),
        theme.pie6.as_deref().unwrap_or("hsl(80, 100%, 76.2745098039%)"),
        theme.pie7.as_deref().unwrap_or("hsl(300, 100%, 76.2745098039%)"),
        theme.pie8.as_deref().unwrap_or("hsl(180, 100%, 56.2745098039%)"),
        theme.pie9.as_deref().unwrap_or("hsl(0, 100%, 56.2745098039%)"),
        theme.pie10.as_deref().unwrap_or("hsl(300, 100%, 56.2745098039%)"),
        theme.pie11.as_deref().unwrap_or("hsl(150, 100%, 56.2745098039%)"),
        theme.pie12.as_deref().unwrap_or("hsl(0, 100%, 66.2745098039%)"),
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
    let k = if filtered_sum > 0.0 { TAU / filtered_sum } else { 0.0 };
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
#[allow(
    clippy::approx_constant,
    clippy::eq_op,
    clippy::excessive_precision,
    clippy::useless_let_if_seq
)]
mod v8_math {
    use std::f64;

    // High/low word extraction — V8's GET_HIGH_WORD / GET_LOW_WORD.
    #[inline]
    fn hi(x: f64) -> i32 {
        (x.to_bits() >> 32) as i32
    }
    #[inline]
    fn set_high(x: f64, hi: u32) -> f64 {
        let lo = x.to_bits() & 0xFFFF_FFFF;
        f64::from_bits(((hi as u64) << 32) | lo)
    }

    // ── __kernel_cos ─────────────────────────────────────────────────
    // Polynomial approximation on [-π/4, π/4].
    fn kernel_cos(x: f64, y: f64) -> f64 {
        const ONE: f64 = 1.0;
        const C1: f64 = 4.16666666666666019037e-02;
        const C2: f64 = -1.38888888888741095749e-03;
        const C3: f64 = 2.48015872894767294178e-05;
        const C4: f64 = -2.75573143513906633035e-07;
        const C5: f64 = 2.08757232129817482790e-09;
        const C6: f64 = -1.13596475577881948265e-11;

        let ix = hi(x) & 0x7FFF_FFFF;
        if ix < 0x3E40_0000 && x as i32 == 0 {
            // |x| < 2^-27, and x == 0 (exact) — cos(0) = 1.
            return ONE;
        }
        let z = x * x;
        let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
        if ix < 0x3FD3_3333 {
            // |x| < 0.3
            ONE - (0.5 * z - (z * r - x * y))
        } else {
            let qx = if ix > 0x3FE9_0000 {
                // x > 0.78125
                0.28125
            } else {
                // qx = x/4 with low word masked off.
                set_high(0.0, (ix - 0x0020_0000) as u32)
            };
            let iz = 0.5 * z - qx;
            let a = ONE - qx;
            a - (iz - (z * r - x * y))
        }
    }

    // ── __kernel_sin ────────────────────────────────────────────────
    fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
        const HALF: f64 = 0.5;
        const S1: f64 = -1.66666666666666324348e-01;
        const S2: f64 = 8.33333333332248946124e-03;
        const S3: f64 = -1.98412698298579493134e-04;
        const S4: f64 = 2.75573137070700676789e-06;
        const S5: f64 = -2.50507602534068634195e-08;
        const S6: f64 = 1.58969099521155010221e-10;

        let ix = hi(x) & 0x7FFF_FFFF;
        if ix < 0x3E40_0000 && x as i32 == 0 {
            return x;
        }
        let z = x * x;
        let v = z * x;
        let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
        if iy == 0 {
            x + v * (S1 + z * r)
        } else {
            x - ((z * (HALF * y - v * r) - y) - v * S1)
        }
    }

    // ── __ieee754_rem_pio2 (medium-size branch) ─────────────────────
    // Returns (n, y0, y1). We never hit the large-arg branch in pie.
    fn rem_pio2(x: f64) -> (i32, f64, f64) {
        const TWO_24: f64 = 1.67772160000000000000e+07;
        const INVPIO2: f64 = 6.36619772367581382433e-01;
        const PIO2_1: f64 = 1.57079632673412561417e+00;
        const PIO2_1T: f64 = 6.07710050650619224932e-11;
        const PIO2_2: f64 = 6.07710050630396597660e-11;
        const PIO2_2T: f64 = 2.02226624879595063154e-21;
        const PIO2_3: f64 = 2.02226624871116645580e-21;
        const PIO2_3T: f64 = 8.47842766036889956997e-32;
        // npio2_hw table — high word of n*(pi/2) for n = 1..=32.
        const NPIO2_HW: [i32; 32] = [
            0x3FF921FBu32 as i32,
            0x400921FBu32 as i32,
            0x4012D97Cu32 as i32,
            0x401921FBu32 as i32,
            0x401F6A7Au32 as i32,
            0x4022D97Cu32 as i32,
            0x4025FDBBu32 as i32,
            0x402921FBu32 as i32,
            0x402C463Au32 as i32,
            0x402F6A7Au32 as i32,
            0x4031475Cu32 as i32,
            0x4032D97Cu32 as i32,
            0x40346B9Cu32 as i32,
            0x4035FDBBu32 as i32,
            0x40378FDBu32 as i32,
            0x403921FBu32 as i32,
            0x403AB41Bu32 as i32,
            0x403C463Au32 as i32,
            0x403DD85Au32 as i32,
            0x403F6A7Au32 as i32,
            0x40407E4Cu32 as i32,
            0x4041475Cu32 as i32,
            0x4042106Cu32 as i32,
            0x4042D97Cu32 as i32,
            0x4043A28Cu32 as i32,
            0x40446B9Cu32 as i32,
            0x404534ACu32 as i32,
            0x4045FDBBu32 as i32,
            0x4046C6CBu32 as i32,
            0x40478FDBu32 as i32,
            0x404858EBu32 as i32,
            0x404921FBu32 as i32,
        ];
        let _ = TWO_24;

        let hx = hi(x);
        let ix = hx & 0x7FFF_FFFF;
        if ix <= 0x3FE9_21FB {
            // |x| <= π/4
            return (0, x, 0.0);
        }
        if ix < 0x4002_D97C {
            // |x| < 3π/4 — n = ±1.
            if hx > 0 {
                let z = x - PIO2_1;
                let (y0, y1) = if ix != 0x3FF9_21FB {
                    let y0 = z - PIO2_1T;
                    let y1 = (z - y0) - PIO2_1T;
                    (y0, y1)
                } else {
                    let z2 = z - PIO2_2;
                    let y0 = z2 - PIO2_2T;
                    let y1 = (z2 - y0) - PIO2_2T;
                    (y0, y1)
                };
                return (1, y0, y1);
            } else {
                let z = x + PIO2_1;
                let (y0, y1) = if ix != 0x3FF9_21FB {
                    let y0 = z + PIO2_1T;
                    let y1 = (z - y0) + PIO2_1T;
                    (y0, y1)
                } else {
                    let z2 = z + PIO2_2;
                    let y0 = z2 + PIO2_2T;
                    let y1 = (z2 - y0) + PIO2_2T;
                    (y0, y1)
                };
                return (-1, y0, y1);
            }
        }
        if ix <= 0x4139_21FB {
            // |x| <= 2^19*(π/2)
            let t = x.abs();
            let n = (t * INVPIO2 + 0.5) as i32;
            let fn_d = n as f64;
            let mut r = t - fn_d * PIO2_1;
            let mut w = fn_d * PIO2_1T;
            let j = ix >> 20;
            let mut y0 = r - w;
            let need_2nd = {
                if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
                    false
                } else {
                    let high = hi(y0);
                    let i = j - ((high >> 20) & 0x7FF);
                    i > 16
                }
            };
            if need_2nd {
                let t2 = r;
                w = fn_d * PIO2_2;
                r = t2 - w;
                w = fn_d * PIO2_2T - ((t2 - r) - w);
                y0 = r - w;
                let high = hi(y0);
                let i2 = j - ((high >> 20) & 0x7FF);
                if i2 > 49 {
                    // 3rd iteration (rare; included for completeness).
                    let t3 = r;
                    w = fn_d * PIO2_3;
                    r = t3 - w;
                    w = fn_d * PIO2_3T - ((t3 - r) - w);
                    y0 = r - w;
                }
            }
            let y1 = (r - y0) - w;
            if hx < 0 {
                return (-n, -y0, -y1);
            } else {
                return (n, y0, y1);
            }
        }
        // Pie never reaches here. Return 0/x/0 as a safe fallback.
        (0, x, 0.0)
    }

    // ── cos ─────────────────────────────────────────────────────────
    pub fn cos(x: f64) -> f64 {
        let ix = hi(x) & 0x7FFF_FFFF;
        if ix <= 0x3FE9_21FB {
            // |x| <= π/4
            return kernel_cos(x, 0.0);
        }
        if ix >= 0x7FF0_0000 {
            return x - x; // NaN
        }
        let (n, y0, y1) = rem_pio2(x);
        match n & 3 {
            0 => kernel_cos(y0, y1),
            1 => -kernel_sin(y0, y1, 1),
            2 => -kernel_cos(y0, y1),
            _ => kernel_sin(y0, y1, 1),
        }
    }

    // ── sin ─────────────────────────────────────────────────────────
    pub fn sin(x: f64) -> f64 {
        let ix = hi(x) & 0x7FFF_FFFF;
        if ix <= 0x3FE9_21FB {
            // |x| <= π/4
            if ix < 0x3E50_0000 {
                // |x| < 2^-26
                if x as i32 == 0 {
                    return x;
                }
            }
            return kernel_sin(x, 0.0, 0);
        }
        if ix >= 0x7FF0_0000 {
            return x - x; // NaN
        }
        let (n, y0, y1) = rem_pio2(x);
        match n & 3 {
            0 => kernel_sin(y0, y1, 1),
            1 => kernel_cos(y0, y1),
            2 => -kernel_sin(y0, y1, 1),
            _ => -kernel_cos(y0, y1),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        // Values taken directly from V8 (Node 20 Linux x86_64).
        #[test]
        fn matches_v8_cos_01() {
            // cos(0.1) differs between glibc libm and V8 by 1 ULP.
            assert_eq!(cos(0.1).to_bits(), 0.99500416527802570954f64.to_bits());
        }
        #[test]
        fn matches_v8_sin_pie_input() {
            // sin(0.82279807594018405936) — slice-1 centroid y for
            // the Sports-in-Sweden fixture.
            assert_eq!(
                sin(0.82279807594018405936).to_bits(),
                0.73305187182982645133f64.to_bits()
            );
        }
        #[test]
        fn matches_v8_cos_pie_input() {
            assert_eq!(
                cos(0.82279807594018405936).to_bits(),
                0.68017273777091924458f64.to_bits()
            );
        }
        #[test]
        fn handles_negative_quadrant() {
            // cos(-π/4) ≈ 0.7071067811865476 (shortest form). The
            // `-0.7853981633974482` input is 1 ULP larger in magnitude
            // than -π/4 and produces a different bit pattern in V8.
            let a = -0.7853981633974482f64;
            assert_eq!(
                cos(a).to_bits(),
                0.7071067811865477f64.to_bits()
            );
        }
    }
}

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
