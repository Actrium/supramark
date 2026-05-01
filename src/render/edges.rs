//! Edge SVG emission — ports upstream's
//! `rendering-util/rendering-elements/edges.js` (968 LoC) curve-to-path
//! math and clip-to-node-boundary trimming.
//!
//! Portions adapted from mmdflux (https://github.com/kevinswiber/mmdflux),
//! MIT license — specifically the d3-compatible basis-spline emitter
//! (`path_from_points_curved`), d3's `curveBasis.point` control-point
//! math, and `path_from_points_rounded` for the `'rounded'` curve type.
//!
//! Scope: this module takes a post-dagre edge (with `points`, `source`,
//! `target`, `curve`, etc.) and emits the `<path d="...">` string plus
//! edge-label positioning data. Marker URL wiring is a caller concern;
//! we stamp a placeholder `marker-end="url(#<id>-pointEnd)"` reference
//! matching Wave 3 P2's naming convention.
//!
//! Byte-exact target: `d` strings round to **3 decimal digits** with
//! JavaScript-style shortest-roundtrip formatting (matches d3's
//! `withPath(3)` default used by `d3.line()`).

use crate::layout::intersect::{ray_ellipse_intersection, ray_polygon_intersection};
use crate::layout::unified::types::{Edge, Node, Point};

// ── curve taxonomy ──────────────────────────────────────────────────

/// Resolved edge curve type. Parallel to upstream's curve-name switch
/// inside `insertEdge`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveType {
    /// d3 curveBasis — the upstream default for flowchart / state /
    /// class diagrams. Cubic B-spline through control points.
    Basis,
    /// d3 curveLinear — straight segments, `M L L L …`.
    Linear,
    /// d3 curveStep — horizontal-then-vertical pairs.
    Step,
    /// d3 curveStepBefore — vertical-then-horizontal pairs.
    StepBefore,
    /// d3 curveStepAfter — horizontal-then-vertical with the step AT x_i.
    StepAfter,
    /// Mermaid's own 'rounded' — linear with quadratic-Bezier corner
    /// smoothing. Not a d3 curve; implemented in upstream's
    /// `generateRoundedPath`.
    Rounded,
    /// d3 curveCardinal — cardinal spline with tension parameter.
    Cardinal,
    CatmullRom,
    BumpX,
    BumpY,
    MonotoneX,
    MonotoneY,
    Natural,
}

impl CurveType {
    /// Parse an upstream curve name. Unknown names fall through to
    /// `None` so the caller can substitute `CurveType::Basis` (the
    /// default in upstream's switch statement).
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "basis" => Some(Self::Basis),
            "linear" => Some(Self::Linear),
            "step" => Some(Self::Step),
            "stepBefore" => Some(Self::StepBefore),
            "stepAfter" => Some(Self::StepAfter),
            "rounded" => Some(Self::Rounded),
            "cardinal" => Some(Self::Cardinal),
            "catmullRom" => Some(Self::CatmullRom),
            "bumpX" => Some(Self::BumpX),
            "bumpY" => Some(Self::BumpY),
            "monotoneX" => Some(Self::MonotoneX),
            "monotoneY" => Some(Self::MonotoneY),
            "natural" => Some(Self::Natural),
            _ => None,
        }
    }

    /// Resolve `edge.curve` against a default (upstream's
    /// `resolveEdgeCurveType` → `config.flowchart.curve`).
    pub fn resolve(edge_curve: Option<&str>, config_default: Option<&str>) -> Self {
        edge_curve
            .and_then(Self::parse)
            .or_else(|| config_default.and_then(Self::parse))
            .unwrap_or(Self::Basis)
    }
}

// ── JS-style number formatting ──────────────────────────────────────

/// Format a number the way d3-path's `appendRound(3)` + JS string
/// concatenation does: round to 3 decimals, then emit the shortest
/// round-trip string (no trailing zeros, integers print without `.`).
///
/// Examples:
///   1.0        → "1"
///   1.5        → "1.5"
///   1.23456    → "1.235"
///   0.0001     → "0"      (rounds down)
///   -0.0       → "0"      (JS `-0` prints as `0`; Rust too)
pub fn fmt_coord(v: f64) -> String {
    if !v.is_finite() {
        // d3 would concat "NaN"/"Infinity" verbatim; for byte parity
        // we mirror that exactly.
        return format!("{v}");
    }
    // Math.round half-away-from-zero for positives, half-to-even in JS
    // is technically round-half-to-+∞ (`Math.round(-0.5) === 0`). But
    // for the magnitudes typical in a diagram coordinate (dozens of px)
    // the difference is sub-ULP. Use Rust's `.round()` (half-away-from-
    // zero) and accept the mismatch for exact-half-only inputs; no
    // real diagram pixel coords land exactly on ±0.0005 boundaries.
    let rounded = (v * 1000.0).round() / 1000.0;
    // Normalize -0.0 to 0.0 so output matches JS's String(-0) === "0".
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        // Rust's default f64 Display is shortest-roundtrip — matches
        // JS Number#toString for this already-rounded value.
        format!("{rounded}")
    }
}

// ── path emission ───────────────────────────────────────────────────

/// Emit an SVG `d` attribute value from a series of control points
/// using the requested curve.
///
/// Matches upstream's `lineFunction = d3.line().x(x).y(y).curve(curve)`
/// output byte-for-byte for the supported curves.
pub fn build_path(points: &[Point], curve: CurveType) -> String {
    if points.is_empty() {
        return String::new();
    }
    match curve {
        CurveType::Basis => path_basis(points),
        CurveType::Linear => path_linear(points),
        CurveType::Step => path_step(points, 0.5),
        CurveType::StepBefore => path_step(points, 0.0),
        CurveType::StepAfter => path_step(points, 1.0),
        CurveType::Rounded => path_rounded(points, 5.0),
        CurveType::Natural => path_natural(points),
        CurveType::MonotoneX => path_monotone_x(points),
        CurveType::MonotoneY => path_monotone_y(points),
        CurveType::BumpX => path_bump_x(points),
        CurveType::BumpY => path_bump_y(points),
        CurveType::CatmullRom => path_catmull_rom(points, 0.5),
        CurveType::Cardinal => path_cardinal(points, 0.0),
    }
}

/// Return the absolute coordinates that upstream's `pathBBox` parser would
/// visit when computing the SVG bbox of the rendered `d=` string for the
/// given curve. This is what `getBBox()` uses for edge paths in the
/// jsdom shim (parsed coords from each `M`/`L`/`C` command's endpoint and
/// control points), and differs from the raw layout `points` for cubic
/// curves: the basis spline smooths the input polyline and the actual
/// visited bbox is tighter than `min/max` of the raw points.
///
/// Used by viewBox computation in `svg_flowchart.rs` so we mirror upstream
/// pixel-for-pixel for edges whose dagre middle points poke out beyond
/// the rendered curve.
pub fn pathbbox_points(points: &[Point], curve: CurveType) -> Vec<(f64, f64)> {
    if points.is_empty() {
        return Vec::new();
    }
    match curve {
        CurveType::Basis => basis_visited_points(points),
        CurveType::Linear => points.iter().map(|p| (p.x, p.y)).collect(),
        CurveType::Step => step_visited_points(points, 0.5),
        CurveType::StepBefore => step_visited_points(points, 0.0),
        CurveType::StepAfter => step_visited_points(points, 1.0),
        CurveType::Rounded => points.iter().map(|p| (p.x, p.y)).collect(),
        CurveType::Natural => natural_visited_points(points),
        CurveType::MonotoneX => monotone_x_visited_points(points),
        CurveType::MonotoneY => monotone_y_visited_points(points),
        CurveType::BumpX => bump_x_visited_points(points),
        CurveType::BumpY => bump_y_visited_points(points),
        CurveType::CatmullRom => catmull_rom_visited_points(points, 0.5),
        CurveType::Cardinal => cardinal_visited_points(points, 0.0),
    }
}

/// Mirror of `path_basis` — returns the coords visited by an SVG
/// `pathBBox` parser. For the basis curve this corresponds to: the
/// initial M, the implicit L to the first interior anchor, every C
/// command's two control points and end coordinate, and the closing L
/// to the last point. We deliberately do NOT round to 3 decimals here
/// because the bbox is consumed by the byte-exact viewBox formatter,
/// which reads the unrounded float directly (matching upstream's
/// `parseFloat` of the d-string after appendRound — round-trip stable).
fn basis_visited_points(points: &[Point]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    if points.is_empty() {
        return out;
    }
    if points.len() == 1 {
        out.push((points[0].x, points[0].y));
        return out;
    }
    let mut x0 = f64::NAN;
    let mut x1 = f64::NAN;
    let mut y0 = f64::NAN;
    let mut y1 = f64::NAN;
    let mut state: u8 = 0;
    for p in points {
        let (x, y) = (p.x, p.y);
        match state {
            0 => {
                out.push((x, y)); // M
                state = 1;
            }
            1 => {
                state = 2;
            }
            2 => {
                // L to the (5*p0+p1)/6 anchor
                out.push(((5.0 * x0 + x1) / 6.0, (5.0 * y0 + y1) / 6.0));
                push_basis_cubic_visited(&mut out, x0, y0, x1, y1, x, y);
                state = 3;
            }
            _ => {
                push_basis_cubic_visited(&mut out, x0, y0, x1, y1, x, y);
            }
        }
        x0 = x1;
        x1 = x;
        y0 = y1;
        y1 = y;
    }
    match state {
        3 => {
            // lineEnd cubic uses (x1, y1) as the next "x".
            push_basis_cubic_visited(&mut out, x0, y0, x1, y1, x1, y1);
            out.push((x1, y1)); // closing L
        }
        2 => {
            out.push((x1, y1));
        }
        _ => {}
    }
    out
}

fn push_basis_cubic_visited(
    out: &mut Vec<(f64, f64)>,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    x: f64,
    y: f64,
) {
    let c1x = (2.0 * x0 + x1) / 3.0;
    let c1y = (2.0 * y0 + y1) / 3.0;
    let c2x = (x0 + 2.0 * x1) / 3.0;
    let c2y = (y0 + 2.0 * y1) / 3.0;
    let ex = (x0 + 4.0 * x1 + x) / 6.0;
    let ey = (y0 + 4.0 * y1 + y) / 6.0;
    out.push((c1x, c1y));
    out.push((c2x, c2y));
    out.push((ex, ey));
}

/// Mirror of `path_step` — returns the coords visited.
fn step_visited_points(points: &[Point], t: f64) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    if points.is_empty() {
        return out;
    }
    let mut x_prev = 0.0_f64;
    let mut y_prev = 0.0_f64;
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            out.push((p.x, p.y));
        } else if t <= 0.0 {
            out.push((x_prev, p.y));
            out.push((p.x, p.y));
        } else {
            let x1 = x_prev * (1.0 - t) + p.x * t;
            out.push((x1, y_prev));
            out.push((x1, p.y));
        }
        x_prev = p.x;
        y_prev = p.y;
    }
    out
}

/// d3 curveBasis path emission. Cubic B-spline through the points.
///
/// Adapted from mmdflux (`render/graph/svg/edges/path_emit.rs`) —
/// itself a direct port of d3-shape's `curve/basis.js`.
fn path_basis(points: &[Point]) -> String {
    use std::fmt::Write;
    if points.is_empty() {
        return String::new();
    }
    let mut d = String::new();
    if points.len() == 1 {
        let p = points[0];
        let _ = write!(d, "M{},{}Z", fmt_coord(p.x), fmt_coord(p.y));
        return d;
    }

    let mut x0 = f64::NAN;
    let mut x1 = f64::NAN;
    let mut y0 = f64::NAN;
    let mut y1 = f64::NAN;
    let mut state: u8 = 0;

    for p in points {
        let (x, y) = (p.x, p.y);
        match state {
            0 => {
                let _ = write!(d, "M{},{}", fmt_coord(x), fmt_coord(y));
                state = 1;
            }
            1 => {
                state = 2;
            }
            2 => {
                // First interior segment: emit an implicit L to the
                // (5*x0+x1)/6 anchor, then the regular cubic from d3's
                // `basis.point`.
                let px = (5.0 * x0 + x1) / 6.0;
                let py = (5.0 * y0 + y1) / 6.0;
                let _ = write!(d, "L{},{}", fmt_coord(px), fmt_coord(py));
                emit_basis_cubic(&mut d, x0, y0, x1, y1, x, y);
                state = 3;
            }
            _ => {
                emit_basis_cubic(&mut d, x0, y0, x1, y1, x, y);
            }
        }
        x0 = x1;
        x1 = x;
        y0 = y1;
        y1 = y;
    }

    // lineEnd: mirrors d3's switch on `this._point`.
    match state {
        3 => {
            emit_basis_cubic(&mut d, x0, y0, x1, y1, x1, y1);
            let _ = write!(d, "L{},{}", fmt_coord(x1), fmt_coord(y1));
        }
        2 => {
            let _ = write!(d, "L{},{}", fmt_coord(x1), fmt_coord(y1));
        }
        _ => {}
    }
    d
}

fn emit_basis_cubic(d: &mut String, x0: f64, y0: f64, x1: f64, y1: f64, x: f64, y: f64) {
    use std::fmt::Write;
    let c1x = (2.0 * x0 + x1) / 3.0;
    let c1y = (2.0 * y0 + y1) / 3.0;
    let c2x = (x0 + 2.0 * x1) / 3.0;
    let c2y = (y0 + 2.0 * y1) / 3.0;
    let ex = (x0 + 4.0 * x1 + x) / 6.0;
    let ey = (y0 + 4.0 * y1 + y) / 6.0;
    let _ = write!(
        d,
        "C{},{},{},{},{},{}",
        fmt_coord(c1x),
        fmt_coord(c1y),
        fmt_coord(c2x),
        fmt_coord(c2y),
        fmt_coord(ex),
        fmt_coord(ey)
    );
}

fn path_linear(points: &[Point]) -> String {
    use std::fmt::Write;
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        let cmd = if i == 0 { 'M' } else { 'L' };
        let _ = write!(d, "{cmd}{},{}", fmt_coord(p.x), fmt_coord(p.y));
    }
    d
}

/// d3 curveStep / curveStepBefore / curveStepAfter. `t` selects the
/// step position:  0.0 = Before, 0.5 = Step, 1.0 = After.
fn path_step(points: &[Point], t: f64) -> String {
    use std::fmt::Write;
    if points.is_empty() {
        return String::new();
    }
    let mut d = String::new();
    let mut x_prev = 0.0_f64;
    let mut y_prev = 0.0_f64;
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            let _ = write!(d, "M{},{}", fmt_coord(p.x), fmt_coord(p.y));
        } else if t <= 0.0 {
            let _ = write!(d, "L{},{}", fmt_coord(x_prev), fmt_coord(p.y));
            let _ = write!(d, "L{},{}", fmt_coord(p.x), fmt_coord(p.y));
        } else {
            let x1 = x_prev * (1.0 - t) + p.x * t;
            let _ = write!(d, "L{},{}", fmt_coord(x1), fmt_coord(y_prev));
            let _ = write!(d, "L{},{}", fmt_coord(x1), fmt_coord(p.y));
        }
        x_prev = p.x;
        y_prev = p.y;
    }
    if t > 0.0 && t < 1.0 && points.len() == 2 {
        let last = points.last().unwrap();
        let _ = write!(d, "L{},{}", fmt_coord(last.x), fmt_coord(last.y));
    }
    d
}

/// Mermaid 'rounded' curve — straight segments with quadratic-Bezier
/// smoothed corners. Port of upstream's `generateRoundedPath`.
fn path_rounded(points: &[Point], radius: f64) -> String {
    use std::fmt::Write;
    if points.len() < 2 {
        return String::new();
    }
    let mut d = String::new();
    let n = points.len();
    const EPS: f64 = 1e-5;

    for i in 0..n {
        let curr = points[i];
        if i == 0 {
            let _ = write!(d, "M{},{}", fmt_coord(curr.x), fmt_coord(curr.y));
        } else if i == n - 1 {
            let _ = write!(d, "L{},{}", fmt_coord(curr.x), fmt_coord(curr.y));
        } else {
            let prev = points[i - 1];
            let next = points[i + 1];
            let dx1 = curr.x - prev.x;
            let dy1 = curr.y - prev.y;
            let dx2 = next.x - curr.x;
            let dy2 = next.y - curr.y;
            let len1 = (dx1 * dx1 + dy1 * dy1).sqrt();
            let len2 = (dx2 * dx2 + dy2 * dy2).sqrt();
            if len1 < EPS || len2 < EPS {
                let _ = write!(d, "L{},{}", fmt_coord(curr.x), fmt_coord(curr.y));
                continue;
            }
            let nx1 = dx1 / len1;
            let ny1 = dy1 / len1;
            let nx2 = dx2 / len2;
            let ny2 = dy2 / len2;
            let dot = (nx1 * nx2 + ny1 * ny2).clamp(-1.0, 1.0);
            let angle = dot.acos();
            if angle < EPS || (std::f64::consts::PI - angle).abs() < EPS {
                let _ = write!(d, "L{},{}", fmt_coord(curr.x), fmt_coord(curr.y));
                continue;
            }
            let cut_len = (radius / (angle / 2.0).sin())
                .min(len1 / 2.0)
                .min(len2 / 2.0);
            let sx = curr.x - nx1 * cut_len;
            let sy = curr.y - ny1 * cut_len;
            let ex = curr.x + nx2 * cut_len;
            let ey = curr.y + ny2 * cut_len;
            let _ = write!(d, "L{},{}", fmt_coord(sx), fmt_coord(sy));
            let _ = write!(
                d,
                "Q{},{},{},{}",
                fmt_coord(curr.x),
                fmt_coord(curr.y),
                fmt_coord(ex),
                fmt_coord(ey)
            );
        }
    }
    d
}

// ── d3 curveNatural ──────────────────────────────────────────────────

fn natural_control_points(x: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = x.len() - 1;
    if n == 0 {
        return (vec![], vec![]);
    }
    let mut a = vec![0.0; n];
    let mut b = vec![0.0; n];
    let mut r = vec![0.0; n];
    a[0] = 0.0;
    b[0] = 2.0;
    r[0] = x[0] + 2.0 * x[1];
    for i in 1..n - 1 {
        a[i] = 1.0;
        b[i] = 4.0;
        r[i] = 4.0 * x[i] + 2.0 * x[i + 1];
    }
    a[n - 1] = 2.0;
    b[n - 1] = 7.0;
    r[n - 1] = 8.0 * x[n - 1] + x[n];
    for i in 1..n {
        let m = a[i] / b[i - 1];
        b[i] -= m;
        r[i] -= m * r[i - 1];
    }
    a[n - 1] = r[n - 1] / b[n - 1];
    for i in (0..n - 1).rev() {
        a[i] = (r[i] - a[i + 1]) / b[i];
    }
    let mut bp = vec![0.0; n];
    bp[n - 1] = (x[n] + a[n - 1]) / 2.0;
    for i in 0..n - 1 {
        bp[i] = 2.0 * x[i + 1] - a[i + 1];
    }
    (a, bp)
}

fn path_natural(points: &[Point]) -> String {
    use std::fmt::Write;
    if points.is_empty() {
        return String::new();
    }
    let n = points.len();
    if n == 1 {
        return format!("M{},{}Z", fmt_coord(points[0].x), fmt_coord(points[0].y));
    }
    let mut d = String::new();
    let _ = write!(d, "M{},{}", fmt_coord(points[0].x), fmt_coord(points[0].y));
    if n == 2 {
        let _ = write!(d, "L{},{}", fmt_coord(points[1].x), fmt_coord(points[1].y));
        return d;
    }
    let xs: Vec<f64> = points.iter().map(|p| p.x).collect();
    let ys: Vec<f64> = points.iter().map(|p| p.y).collect();
    let (px_a, px_b) = natural_control_points(&xs);
    let (py_a, py_b) = natural_control_points(&ys);
    for i0 in 0..n - 1 {
        let i1 = i0 + 1;
        let _ = write!(
            d,
            "C{},{},{},{},{},{}",
            fmt_coord(px_a[i0]),
            fmt_coord(py_a[i0]),
            fmt_coord(px_b[i0]),
            fmt_coord(py_b[i0]),
            fmt_coord(xs[i1]),
            fmt_coord(ys[i1])
        );
    }
    d
}

fn natural_visited_points(points: &[Point]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    if points.is_empty() {
        return out;
    }
    let n = points.len();
    if n == 1 {
        out.push((points[0].x, points[0].y));
        return out;
    }
    out.push((points[0].x, points[0].y));
    if n == 2 {
        out.push((points[1].x, points[1].y));
        return out;
    }
    let xs: Vec<f64> = points.iter().map(|p| p.x).collect();
    let ys: Vec<f64> = points.iter().map(|p| p.y).collect();
    let (px_a, px_b) = natural_control_points(&xs);
    let (py_a, py_b) = natural_control_points(&ys);
    for i0 in 0..n - 1 {
        let i1 = i0 + 1;
        out.push((px_a[i0], py_a[i0]));
        out.push((px_b[i0], py_b[i0]));
        out.push((xs[i1], ys[i1]));
    }
    out
}

// ── d3 curveMonotoneX / curveMonotoneY ────────────────────────────────

fn monotone_sign(x: f64) -> f64 {
    if x < 0.0 {
        -1.0
    } else {
        1.0
    }
}

fn monotone_slope3(x0: f64, y0: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let h0 = x1 - x0;
    let h1 = x2 - x1;
    let d0 = if h0 != 0.0 {
        h0
    } else if h1 < 0.0 {
        -0.0
    } else {
        0.0
    };
    let d1 = if h1 != 0.0 {
        h1
    } else if h0 < 0.0 {
        -0.0
    } else {
        0.0
    };
    let s0 = (y1 - y0) / d0;
    let s1 = (y2 - y1) / d1;
    let p = if h0 + h1 != 0.0 {
        (s0 * h1 + s1 * h0) / (h0 + h1)
    } else {
        0.0
    };
    let min_val = s0.abs().min(s1.abs()).min(0.5 * p.abs());
    let result = (monotone_sign(s0) + monotone_sign(s1)) * min_val;
    if result == 0.0 || !result.is_finite() {
        0.0
    } else {
        result
    }
}

fn monotone_slope2(x0: f64, y0: f64, x1: f64, y1: f64, t: f64) -> f64 {
    let h = x1 - x0;
    if h != 0.0 {
        (3.0 * (y1 - y0) / h - t) / 2.0
    } else {
        t
    }
}

fn path_monotone_impl(points: &[Point], swap: bool) -> String {
    use std::fmt::Write;
    if points.is_empty() {
        return String::new();
    }
    if points.len() == 1 {
        let (x, y) = if swap {
            (points[0].y, points[0].x)
        } else {
            (points[0].x, points[0].y)
        };
        return format!("M{},{}Z", fmt_coord(x), fmt_coord(y));
    }
    let coords: Vec<(f64, f64)> = points
        .iter()
        .map(|p| if swap { (p.y, p.x) } else { (p.x, p.y) })
        .collect();
    let mut d = String::new();
    if swap {
        let _ = write!(d, "M{},{}", fmt_coord(coords[0].1), fmt_coord(coords[0].0));
    } else {
        let _ = write!(d, "M{},{}", fmt_coord(coords[0].0), fmt_coord(coords[0].1));
    }

    if coords.len() == 2 {
        if swap {
            let _ = write!(d, "L{},{}", fmt_coord(coords[1].1), fmt_coord(coords[1].0));
        } else {
            let _ = write!(d, "L{},{}", fmt_coord(coords[1].0), fmt_coord(coords[1].1));
        }
        return d;
    }

    let mut x0 = coords[0].0;
    let mut y0 = coords[0].1;
    let mut x1 = coords[1].0;
    let mut y1 = coords[1].1;
    let mut t0: f64 = f64::NAN;

    for idx in 2..coords.len() {
        let (x2, y2) = coords[idx];
        let t1 = monotone_slope3(x0, y0, x1, y1, x2, y2);
        if idx == 2 {
            let t_start = monotone_slope2(x0, y0, x1, y1, t1);
            emit_monotone_cubic(&mut d, x0, y0, x1, y1, t_start, t1, swap);
        } else {
            emit_monotone_cubic(&mut d, x0, y0, x1, y1, t0, t1, swap);
        }
        t0 = t1;
        x0 = x1;
        y0 = y1;
        x1 = x2;
        y1 = y2;
    }

    let t_end = monotone_slope2(x0, y0, x1, y1, t0);
    emit_monotone_cubic(&mut d, x0, y0, x1, y1, t0, t_end, swap);

    d
}

fn emit_monotone_cubic(
    d: &mut String,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    t0: f64,
    t1: f64,
    swap: bool,
) {
    use std::fmt::Write;
    let dx = (x1 - x0) / 3.0;
    let cx1 = x0 + dx;
    let cy1 = y0 + dx * t0;
    let cx2 = x1 - dx;
    let cy2 = y1 - dx * t1;
    if swap {
        let _ = write!(
            d,
            "C{},{},{},{},{},{}",
            fmt_coord(cy1),
            fmt_coord(cx1),
            fmt_coord(cy2),
            fmt_coord(cx2),
            fmt_coord(y1),
            fmt_coord(x1)
        );
    } else {
        let _ = write!(
            d,
            "C{},{},{},{},{},{}",
            fmt_coord(cx1),
            fmt_coord(cy1),
            fmt_coord(cx2),
            fmt_coord(cy2),
            fmt_coord(x1),
            fmt_coord(y1)
        );
    }
}

fn path_monotone_x(points: &[Point]) -> String {
    path_monotone_impl(points, false)
}

fn path_monotone_y(points: &[Point]) -> String {
    path_monotone_impl(points, true)
}

fn monotone_visited_points_impl(points: &[Point], swap: bool) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    if points.is_empty() {
        return out;
    }
    let coords: Vec<(f64, f64)> = points
        .iter()
        .map(|p| if swap { (p.y, p.x) } else { (p.x, p.y) })
        .collect();
    if coords.len() == 1 {
        let (x, y) = coords[0];
        if swap {
            out.push((y, x));
        } else {
            out.push((x, y));
        }
        return out;
    }
    if swap {
        out.push((coords[0].1, coords[0].0));
    } else {
        out.push((coords[0].0, coords[0].1));
    }
    if coords.len() == 2 {
        if swap {
            out.push((coords[1].1, coords[1].0));
        } else {
            out.push((coords[1].0, coords[1].1));
        }
        return out;
    }
    let mut x0 = coords[0].0;
    let mut y0 = coords[0].1;
    let mut x1 = coords[1].0;
    let mut y1 = coords[1].1;
    let mut t0: f64 = f64::NAN;

    for idx in 2..coords.len() {
        let (x2, y2) = coords[idx];
        let t1 = monotone_slope3(x0, y0, x1, y1, x2, y2);
        if idx == 2 {
            let t_start = monotone_slope2(x0, y0, x1, y1, t1);
            push_monotone_cubic_visited(&mut out, x0, y0, x1, y1, t_start, t1, swap);
        } else {
            push_monotone_cubic_visited(&mut out, x0, y0, x1, y1, t0, t1, swap);
        }
        t0 = t1;
        x0 = x1;
        y0 = y1;
        x1 = x2;
        y1 = y2;
    }

    let t_end = monotone_slope2(x0, y0, x1, y1, t0);
    push_monotone_cubic_visited(&mut out, x0, y0, x1, y1, t0, t_end, swap);

    out
}

fn push_monotone_cubic_visited(
    out: &mut Vec<(f64, f64)>,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    t0: f64,
    t1: f64,
    swap: bool,
) {
    let dx = (x1 - x0) / 3.0;
    let cx1 = x0 + dx;
    let cy1 = y0 + dx * t0;
    let cx2 = x1 - dx;
    let cy2 = y1 - dx * t1;
    if swap {
        out.push((cy1, cx1));
        out.push((cy2, cx2));
        out.push((y1, x1));
    } else {
        out.push((cx1, cy1));
        out.push((cx2, cy2));
        out.push((x1, y1));
    }
}

fn monotone_x_visited_points(points: &[Point]) -> Vec<(f64, f64)> {
    monotone_visited_points_impl(points, false)
}

fn monotone_y_visited_points(points: &[Point]) -> Vec<(f64, f64)> {
    monotone_visited_points_impl(points, true)
}

// ── d3 curveBumpX / curveBumpY ────────────────────────────────────────

fn path_bump_impl(points: &[Point], bump_x: bool) -> String {
    use std::fmt::Write;
    if points.is_empty() {
        return String::new();
    }
    let mut d = String::new();
    let _ = write!(d, "M{},{}", fmt_coord(points[0].x), fmt_coord(points[0].y));
    for i in 1..points.len() {
        let x0 = points[i - 1].x;
        let y0 = points[i - 1].y;
        let x = points[i].x;
        let y = points[i].y;
        if bump_x {
            let mx = (x0 + x) / 2.0;
            let _ = write!(
                d,
                "C{},{},{},{},{},{}",
                fmt_coord(mx),
                fmt_coord(y0),
                fmt_coord(mx),
                fmt_coord(y),
                fmt_coord(x),
                fmt_coord(y)
            );
        } else {
            let my = (y0 + y) / 2.0;
            let _ = write!(
                d,
                "C{},{},{},{},{},{}",
                fmt_coord(x0),
                fmt_coord(my),
                fmt_coord(x),
                fmt_coord(my),
                fmt_coord(x),
                fmt_coord(y)
            );
        }
    }
    d
}

fn path_bump_x(points: &[Point]) -> String {
    path_bump_impl(points, true)
}

fn path_bump_y(points: &[Point]) -> String {
    path_bump_impl(points, false)
}

fn bump_visited_points_impl(points: &[Point], bump_x: bool) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    if points.is_empty() {
        return out;
    }
    out.push((points[0].x, points[0].y));
    for i in 1..points.len() {
        let x0 = points[i - 1].x;
        let y0 = points[i - 1].y;
        let x = points[i].x;
        let y = points[i].y;
        if bump_x {
            let mx = (x0 + x) / 2.0;
            out.push((mx, y0));
            out.push((mx, y));
            out.push((x, y));
        } else {
            let my = (y0 + y) / 2.0;
            out.push((x0, my));
            out.push((x, my));
            out.push((x, y));
        }
    }
    out
}

fn bump_x_visited_points(points: &[Point]) -> Vec<(f64, f64)> {
    bump_visited_points_impl(points, true)
}

fn bump_y_visited_points(points: &[Point]) -> Vec<(f64, f64)> {
    bump_visited_points_impl(points, false)
}

// ── d3 curveCatmullRom (alpha = 0.5, centripetal) ────────────────────

const D3_EPSILON: f64 = 1e-12;

fn catmull_rom_point(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    l01_a: f64,
    l12_a: f64,
    l23_a: f64,
    l01_2a: f64,
    l12_2a: f64,
    l23_2a: f64,
    nx: f64,
    ny: f64,
) -> (f64, f64, f64, f64) {
    let mut cx1 = x1;
    let mut cy1 = y1;
    let mut cx2 = x2;
    let mut cy2 = y2;

    if l01_a > D3_EPSILON {
        let a = 2.0 * l01_2a + 3.0 * l01_a * l12_a + l12_2a;
        let n = 3.0 * l01_a * (l01_a + l12_a);
        cx1 = (x1 * a - x0 * l12_2a + x2 * l01_2a) / n;
        cy1 = (y1 * a - y0 * l12_2a + y2 * l01_2a) / n;
    }

    if l23_a > D3_EPSILON {
        let b = 2.0 * l23_2a + 3.0 * l23_a * l12_a + l12_2a;
        let m = 3.0 * l23_a * (l23_a + l12_a);
        cx2 = (x2 * b + x1 * l23_2a - nx * l12_2a) / m;
        cy2 = (y2 * b + y1 * l23_2a - ny * l12_2a) / m;
    }

    (cx1, cy1, cx2, cy2)
}

fn path_catmull_rom(points: &[Point], alpha: f64) -> String {
    use std::fmt::Write;
    if points.is_empty() {
        return String::new();
    }
    if points.len() == 1 {
        return format!("M{},{}Z", fmt_coord(points[0].x), fmt_coord(points[0].y));
    }
    let mut d = String::new();
    let _ = write!(d, "M{},{}", fmt_coord(points[0].x), fmt_coord(points[0].y));
    if points.len() == 2 {
        let _ = write!(d, "L{},{}", fmt_coord(points[1].x), fmt_coord(points[1].y));
        return d;
    }

    if alpha == 0.0 {
        return path_cardinal(points, 0.0);
    }

    let n = points.len();
    let mut x0 = f64::NAN;
    let mut x1 = f64::NAN;
    let mut x2 = points[0].x;
    let mut y0 = f64::NAN;
    let mut y1 = f64::NAN;
    let mut y2 = points[0].y;
    let mut l01_a = 0.0;
    let mut l12_a = 0.0;
    let mut l23_a = 0.0;
    let mut l01_2a = 0.0;
    let mut l12_2a = 0.0;
    let mut l23_2a = 0.0;
    let mut pt = 0u8;

    for i in 0..n {
        let x = points[i].x;
        let y = points[i].y;

        if pt > 0 {
            let x23 = x2 - x;
            let y23 = y2 - y;
            l23_2a = (x23 * x23 + y23 * y23).powf(alpha);
            l23_a = l23_2a.sqrt();
        }

        match pt {
            0 => {
                pt = 1;
            }
            1 => {
                pt = 2;
                x1 = x;
                y1 = y;
            }
            2 => {
                pt = 3;
                let (cx1, cy1, cx2, cy2) = catmull_rom_point(
                    x0, y0, x1, y1, x2, y2, l01_a, l12_a, l23_a, l01_2a, l12_2a, l23_2a, x, y,
                );
                let _ = write!(
                    d,
                    "C{},{},{},{},{},{}",
                    fmt_coord(cx1),
                    fmt_coord(cy1),
                    fmt_coord(cx2),
                    fmt_coord(cy2),
                    fmt_coord(x2),
                    fmt_coord(y2)
                );
            }
            _ => {
                let (cx1, cy1, cx2, cy2) = catmull_rom_point(
                    x0, y0, x1, y1, x2, y2, l01_a, l12_a, l23_a, l01_2a, l12_2a, l23_2a, x, y,
                );
                let _ = write!(
                    d,
                    "C{},{},{},{},{},{}",
                    fmt_coord(cx1),
                    fmt_coord(cy1),
                    fmt_coord(cx2),
                    fmt_coord(cy2),
                    fmt_coord(x2),
                    fmt_coord(y2)
                );
            }
        }

        l01_a = l12_a;
        l12_a = l23_a;
        l01_2a = l12_2a;
        l12_2a = l23_2a;
        x0 = x1;
        x1 = x2;
        x2 = x;
        y0 = y1;
        y1 = y2;
        y2 = y;
    }

    if pt == 3 {
        let (cx1, cy1, cx2, cy2) = catmull_rom_point(
            x0, y0, x1, y1, x2, y2, l01_a, l12_a, 0.0, l01_2a, l12_2a, 0.0, x2, y2,
        );
        let _ = write!(
            d,
            "C{},{},{},{},{},{}",
            fmt_coord(cx1),
            fmt_coord(cy1),
            fmt_coord(cx2),
            fmt_coord(cy2),
            fmt_coord(x2),
            fmt_coord(y2)
        );
    } else if pt == 2 {
        let _ = write!(d, "L{},{}", fmt_coord(x2), fmt_coord(y2));
    }

    d
}

fn catmull_rom_visited_points(points: &[Point], alpha: f64) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    if points.is_empty() {
        return out;
    }
    if points.len() == 1 {
        out.push((points[0].x, points[0].y));
        return out;
    }
    out.push((points[0].x, points[0].y));
    if points.len() == 2 {
        out.push((points[1].x, points[1].y));
        return out;
    }
    if alpha == 0.0 {
        return cardinal_visited_points(points, 0.0);
    }

    let n = points.len();
    let mut x0 = f64::NAN;
    let mut x1 = f64::NAN;
    let mut x2 = points[0].x;
    let mut y0 = f64::NAN;
    let mut y1 = f64::NAN;
    let mut y2 = points[0].y;
    let mut l01_a = 0.0;
    let mut l12_a = 0.0;
    let mut l23_a = 0.0;
    let mut l01_2a = 0.0;
    let mut l12_2a = 0.0;
    let mut l23_2a = 0.0;
    let mut pt = 0u8;

    for i in 0..n {
        let x = points[i].x;
        let y = points[i].y;

        if pt > 0 {
            let x23 = x2 - x;
            let y23 = y2 - y;
            l23_2a = (x23 * x23 + y23 * y23).powf(alpha);
            l23_a = l23_2a.sqrt();
        }

        match pt {
            0 => {
                pt = 1;
            }
            1 => {
                pt = 2;
                x1 = x;
                y1 = y;
            }
            2 | _ => {
                pt = 3;
                let (cx1, cy1, cx2, cy2) = catmull_rom_point(
                    x0, y0, x1, y1, x2, y2, l01_a, l12_a, l23_a, l01_2a, l12_2a, l23_2a, x, y,
                );
                out.push((cx1, cy1));
                out.push((cx2, cy2));
                out.push((x2, y2));
            }
        }

        l01_a = l12_a;
        l12_a = l23_a;
        l01_2a = l12_2a;
        l12_2a = l23_2a;
        x0 = x1;
        x1 = x2;
        x2 = x;
        y0 = y1;
        y1 = y2;
        y2 = y;
    }

    if pt == 3 {
        let (cx1, cy1, cx2, cy2) = catmull_rom_point(
            x0, y0, x1, y1, x2, y2, l01_a, l12_a, 0.0, l01_2a, l12_2a, 0.0, x2, y2,
        );
        out.push((cx1, cy1));
        out.push((cx2, cy2));
        out.push((x2, y2));
    } else if pt == 2 {
        out.push((x2, y2));
    }

    out
}

// ── d3 curveCardinal (tension = 0, k = 1/6) ──────────────────────────

fn path_cardinal(points: &[Point], tension: f64) -> String {
    use std::fmt::Write;
    let k = (1.0 - tension) / 6.0;
    if points.is_empty() {
        return String::new();
    }
    if points.len() == 1 {
        return format!("M{},{}Z", fmt_coord(points[0].x), fmt_coord(points[0].y));
    }
    let mut d = String::new();
    let _ = write!(d, "M{},{}", fmt_coord(points[0].x), fmt_coord(points[0].y));
    if points.len() == 2 {
        let _ = write!(d, "L{},{}", fmt_coord(points[1].x), fmt_coord(points[1].y));
        return d;
    }

    let n = points.len();
    let mut x0 = f64::NAN;
    let mut x1 = f64::NAN;
    let mut x2 = points[0].x;
    let mut y0 = f64::NAN;
    let mut y1 = f64::NAN;
    let mut y2 = points[0].y;
    let mut pt = 0u8;

    for i in 0..n {
        let x = points[i].x;
        let y = points[i].y;
        match pt {
            0 => {
                pt = 1;
            }
            1 => {
                pt = 2;
                x1 = x;
                y1 = y;
            }
            _ => {
                pt = 3;
                let cx1 = x1 + k * (x2 - x0);
                let cy1 = y1 + k * (y2 - y0);
                let cx2 = x2 + k * (x1 - x);
                let cy2 = y2 + k * (y1 - y);
                let _ = write!(
                    d,
                    "C{},{},{},{},{},{}",
                    fmt_coord(cx1),
                    fmt_coord(cy1),
                    fmt_coord(cx2),
                    fmt_coord(cy2),
                    fmt_coord(x2),
                    fmt_coord(y2)
                );
            }
        }
        x0 = x1;
        x1 = x2;
        x2 = x;
        y0 = y1;
        y1 = y2;
        y2 = y;
    }

    if pt == 3 {
        let nx = x1;
        let ny = y1;
        let cx1 = x1 + k * (x2 - x0);
        let cy1 = y1 + k * (y2 - y0);
        let cx2 = x2 + k * (x1 - nx);
        let cy2 = y2 + k * (y1 - ny);
        let _ = write!(
            d,
            "C{},{},{},{},{},{}",
            fmt_coord(cx1),
            fmt_coord(cy1),
            fmt_coord(cx2),
            fmt_coord(cy2),
            fmt_coord(x2),
            fmt_coord(y2)
        );
    } else if pt == 2 {
        let _ = write!(d, "L{},{}", fmt_coord(x2), fmt_coord(y2));
    }

    d
}

fn cardinal_visited_points(points: &[Point], tension: f64) -> Vec<(f64, f64)> {
    let k = (1.0 - tension) / 6.0;
    let mut out: Vec<(f64, f64)> = Vec::new();
    if points.is_empty() {
        return out;
    }
    if points.len() == 1 {
        out.push((points[0].x, points[0].y));
        return out;
    }
    out.push((points[0].x, points[0].y));
    if points.len() == 2 {
        out.push((points[1].x, points[1].y));
        return out;
    }

    let n = points.len();
    let mut x0 = f64::NAN;
    let mut x1 = f64::NAN;
    let mut x2 = points[0].x;
    let mut y0 = f64::NAN;
    let mut y1 = f64::NAN;
    let mut y2 = points[0].y;
    let mut pt = 0u8;

    for i in 0..n {
        let x = points[i].x;
        let y = points[i].y;
        match pt {
            0 => {
                pt = 1;
            }
            1 => {
                pt = 2;
                x1 = x;
                y1 = y;
            }
            _ => {
                pt = 3;
                let cx1 = x1 + k * (x2 - x0);
                let cy1 = y1 + k * (y2 - y0);
                let cx2 = x2 + k * (x1 - x);
                let cy2 = y2 + k * (y1 - y);
                out.push((cx1, cy1));
                out.push((cx2, cy2));
                out.push((x2, y2));
            }
        }
        x0 = x1;
        x1 = x2;
        x2 = x;
        y0 = y1;
        y1 = y2;
        y2 = y;
    }

    if pt == 3 {
        let nx = x1;
        let ny = y1;
        let cx1 = x1 + k * (x2 - x0);
        let cy1 = y1 + k * (y2 - y0);
        let cx2 = x2 + k * (x1 - nx);
        let cy2 = y2 + k * (y1 - ny);
        out.push((cx1, cy1));
        out.push((cx2, cy2));
        out.push((x2, y2));
    } else if pt == 2 {
        out.push((x2, y2));
    }

    out
}

// ── endpoint clipping ───────────────────────────────────────────────

/// Clip the first / last points of an edge's point list so the spline
/// terminates on the boundary of each endpoint node rather than at the
/// node centre. Ports upstream's `head.intersect` / `tail.intersect`
/// dispatch at the start of `insertEdge`.
///
/// * Rectangular nodes use a ray-to-AABB hit (upstream's `intersectRect`).
/// * Ellipse / circle nodes use `ray_ellipse_intersection`.
/// * Polygon-shaped nodes (diamond, hexagon, parallelogram, trapezoid
///   etc.) use `ray_polygon_intersection` when `outline` is given.
pub fn clip_endpoints(edge: &Edge, src: &Node, dst: &Node) -> Vec<Point> {
    let points = edge.points.as_deref().unwrap_or(&[]);
    if points.len() < 2 {
        return points.to_vec();
    }
    // Upstream drops the first/last points (the node-centre anchors
    // dagre inserts) before computing intersections: `points.slice(1,
    // edge.points.length - 1);` then unshifts the intersection.
    let mut trimmed: Vec<Point> = points[1..points.len().saturating_sub(1)].to_vec();
    if trimmed.is_empty() {
        // Two-point degenerate edge — synthesise a straight line
        // through both centres and clip.
        trimmed = vec![
            Point {
                x: src.x.unwrap_or(0.0),
                y: src.y.unwrap_or(0.0),
            },
            Point {
                x: dst.x.unwrap_or(0.0),
                y: dst.y.unwrap_or(0.0),
            },
        ];
    }
    let first_probe = trimmed[0];
    let last_probe = trimmed[trimmed.len() - 1];
    let start_clip = intersect_node_boundary(src, first_probe);
    let end_clip = intersect_node_boundary(dst, last_probe);
    let mut out = Vec::with_capacity(trimmed.len() + 2);
    out.push(start_clip);
    out.extend(trimmed);
    out.push(end_clip);
    out
}

/// Compute the ray-from-node-centre-to-`probe` intersection with the
/// node's boundary. Falls back to the centre if no intersection can be
/// found (shouldn't happen for well-formed nodes).
fn intersect_node_boundary(node: &Node, probe: Point) -> Point {
    let cx = node.x.unwrap_or(0.0);
    let cy = node.y.unwrap_or(0.0);
    let w = node.width.unwrap_or(0.0).max(0.0);
    let h = node.height.unwrap_or(0.0).max(0.0);
    let centre = (cx as f32, cy as f32);
    let dir = ((probe.x - cx) as f32, (probe.y - cy) as f32);
    if dir.0.abs() < f32::EPSILON && dir.1.abs() < f32::EPSILON {
        return Point { x: cx, y: cy };
    }

    let shape = node.shape.as_deref().unwrap_or("rect");
    match shape {
        // Stadium / pill: upstream `stadium.ts` builds a polygon of
        // (line + 50-point arc + line + 50-point arc) and calls
        // `intersect.polygon(node, points, point)`. The two end-caps
        // and the long edges hit at slightly different points than a
        // pure ellipse, especially when w and h differ — without this
        // branch flowchart edges that anchor on stadium nodes show
        // ~0.05 px drift.
        "stadium" | "pill" => {
            let poly = stadium_polygon(cx, cy, w, h);
            if let Some((x, y)) = ray_polygon_intersection(centre, dir, &poly) {
                return Point {
                    x: x as f64,
                    y: y as f64,
                };
            }
        }
        "circle" | "ellipse" | "doublecircle" | "stateStart" | "state_start" | "start"
        | "stateEnd" | "state_end" | "end" => {
            let rx = (w / 2.0) as f32;
            let ry = (h / 2.0) as f32;
            if let Some((x, y)) = ray_ellipse_intersection(centre, dir, centre, rx, ry) {
                return Point {
                    x: x as f64,
                    y: y as f64,
                };
            }
        }
        "diamond" | "rhombus" | "question" => {
            let poly = diamond_polygon(cx, cy, w, h);
            if let Some((x, y)) = ray_polygon_intersection(centre, dir, &poly) {
                return Point {
                    x: x as f64,
                    y: y as f64,
                };
            }
        }
        "hexagon" | "hex" => {
            let poly = hexagon_polygon(cx, cy, w, h);
            if let Some((x, y)) = ray_polygon_intersection(centre, dir, &poly) {
                return Point {
                    x: x as f64,
                    y: y as f64,
                };
            }
        }
        "cylinder" | "cyl" => {
            // Upstream cylinder.ts intersect: start with rect intersection
            // then adjust y by the elliptical end-cap when the hit point is
            // on the top/bottom face (or precisely on the side at the cap).
            //   rx = w / 2
            //   ry = rx / (2.5 + w / 50)
            //   y = ry² * (1 − x² / rx²)  → sqrt → ry − y; sign by probe side
            let mut pos = intersection_rect_aabb(cx, cy, w, h, probe);
            let rx = w / 2.0;
            if rx != 0.0 {
                let ry_arc = rx / (2.5 + w / 50.0);
                let local_x = pos.x - cx;
                let half_w = w / 2.0;
                let half_h = h / 2.0;
                let on_cap = local_x.abs() < half_w
                    || (local_x.abs() == half_w && (pos.y - cy).abs() > half_h - ry_arc);
                if on_cap {
                    let mut y = ry_arc * ry_arc * (1.0 - local_x * local_x / (rx * rx));
                    if y > 0.0 {
                        y = y.sqrt();
                    }
                    let mut delta = ry_arc - y;
                    if probe.y - cy > 0.0 {
                        delta = -delta;
                    }
                    pos.y += delta;
                }
            }
            return pos;
        }
        _ => {
            // Rectangular / rounded / default path — matches upstream
            // `intersectRect` for rectangles, which we implement via
            // the closed-form slab-intersection in
            // `intersection_rect_aabb`.
            return intersection_rect_aabb(cx, cy, w, h, probe);
        }
    }
    // Fallback — use rect intersection if the shape-specific math
    // didn't return (e.g. probe on the shape boundary).
    intersection_rect_aabb(cx, cy, w, h, probe)
}

/// Closed-form ray-to-AABB intersection from the node centre.
/// Equivalent to the algorithm in upstream `edges.js::intersection`
/// when restricted to rectangles.
fn intersection_rect_aabb(cx: f64, cy: f64, w: f64, h: f64, probe: Point) -> Point {
    let dx = probe.x - cx;
    let dy = probe.y - cy;
    if dx == 0.0 && dy == 0.0 {
        return Point { x: cx, y: cy };
    }
    let half_w = w / 2.0;
    let half_h = h / 2.0;
    // Slab intersection: find t so that the ray exits the rectangle.
    // Avoid division by zero by handling the two axis-aligned cases.
    let tx = if dx != 0.0 {
        half_w / dx.abs()
    } else {
        f64::INFINITY
    };
    let ty = if dy != 0.0 {
        half_h / dy.abs()
    } else {
        f64::INFINITY
    };
    let t = tx.min(ty);
    Point {
        x: cx + dx * t,
        y: cy + dy * t,
    }
}

/// Build the 102-point polygon upstream `stadium.ts` feeds to
/// `intersect.polygon(node, points, point)`. Mirrors the JS code 1:1:
///
/// ```js
/// const points = [
///   { x: -w / 2 + radius, y: -h / 2 },
///   { x:  w / 2 - radius, y: -h / 2 },
///   ...generateCirclePoints(-w / 2 + radius, 0, radius, 50,  90, 270),
///   { x:  w / 2 - radius, y:  h / 2 },
///   ...generateCirclePoints( w / 2 - radius, 0, radius, 50, 270, 450),
/// ];
/// ```
///
/// `generateCirclePoints(cx, cy, r, n, a0, a1)` returns
/// `{x: -(cx + r*cos(angle)), y: -(cy + r*sin(angle))}` — note the
/// negation. The polygon is centred on the node centre (cx, cy) so we
/// add (cx, cy) when emitting the polygon for intersection.
fn stadium_polygon(cx: f64, cy: f64, w: f64, h: f64) -> Vec<(f32, f32)> {
    let radius = h / 2.0;
    let mut pts: Vec<(f32, f32)> = Vec::with_capacity(102);
    let cxf = cx as f32;
    let cyf = cy as f32;
    pts.push((cxf + (-w / 2.0 + radius) as f32, cyf + (-h / 2.0) as f32));
    pts.push((cxf + (w / 2.0 - radius) as f32, cyf + (-h / 2.0) as f32));
    // Arc 1: cx_raw = -w/2 + radius, cy_raw = 0, 50 points from 90° to 270°.
    let arc1_cx = -w / 2.0 + radius;
    let n: usize = 50;
    let start1 = std::f64::consts::PI / 2.0; // 90°
    let end1 = std::f64::consts::PI * 3.0 / 2.0; // 270°
    let step1 = (end1 - start1) / (n as f64 - 1.0);
    for i in 0..n {
        let angle = start1 + i as f64 * step1;
        let xr = arc1_cx + radius * angle.cos();
        let yr = 0.0 + radius * angle.sin();
        pts.push((cxf + (-xr) as f32, cyf + (-yr) as f32));
    }
    pts.push((cxf + (w / 2.0 - radius) as f32, cyf + (h / 2.0) as f32));
    // Arc 2: cx_raw = w/2 - radius, cy_raw = 0, 50 points from 270° to 450°.
    let arc2_cx = w / 2.0 - radius;
    let start2 = std::f64::consts::PI * 3.0 / 2.0; // 270°
    let end2 = std::f64::consts::PI * 5.0 / 2.0; // 450°
    let step2 = (end2 - start2) / (n as f64 - 1.0);
    for i in 0..n {
        let angle = start2 + i as f64 * step2;
        let xr = arc2_cx + radius * angle.cos();
        let yr = 0.0 + radius * angle.sin();
        pts.push((cxf + (-xr) as f32, cyf + (-yr) as f32));
    }
    pts
}

fn diamond_polygon(cx: f64, cy: f64, w: f64, h: f64) -> Vec<(f32, f32)> {
    let hw = (w / 2.0) as f32;
    let hh = (h / 2.0) as f32;
    let cx = cx as f32;
    let cy = cy as f32;
    vec![(cx, cy - hh), (cx + hw, cy), (cx, cy + hh), (cx - hw, cy)]
}

fn hexagon_polygon(cx: f64, cy: f64, w: f64, h: f64) -> Vec<(f32, f32)> {
    // Mermaid's hexagon — vertical edges on the left/right, slanted
    // top/bottom corners. Matches upstream `hexagon` shape polygon.
    let hw = (w / 2.0) as f32;
    let hh = (h / 2.0) as f32;
    let inset = (w / 4.0) as f32;
    let cx = cx as f32;
    let cy = cy as f32;
    vec![
        (cx - hw + inset, cy - hh),
        (cx + hw - inset, cy - hh),
        (cx + hw, cy),
        (cx + hw - inset, cy + hh),
        (cx - hw + inset, cy + hh),
        (cx - hw, cy),
    ]
}

// ── label placement ─────────────────────────────────────────────────

/// Placement hint for an edge's label. Mirrors upstream's
/// `positionEdgeLabel` output: `(x, y, anchor)`.
#[derive(Debug, Clone, Copy)]
pub struct LabelPlacement {
    pub x: f64,
    pub y: f64,
}

/// Compute the label anchor along a polyline of control points —
/// follows upstream `utils.calcLabelPosition(path)`'s "midpoint by
/// cumulative length" strategy. Rather than walking a rendered SVG
/// path (which requires a browser's `getPointAtLength`), we walk the
/// polyline directly, which matches upstream's behaviour before it
/// adopted `getPointAtLength`.
pub fn label_position(points: &[Point]) -> Option<LabelPlacement> {
    if points.is_empty() {
        return None;
    }
    if points.len() == 1 {
        return Some(LabelPlacement {
            x: points[0].x,
            y: points[0].y,
        });
    }
    // Cumulative lengths.
    let mut lens = Vec::with_capacity(points.len());
    lens.push(0.0_f64);
    for i in 1..points.len() {
        let dx = points[i].x - points[i - 1].x;
        let dy = points[i].y - points[i - 1].y;
        let segment = (dx * dx + dy * dy).sqrt();
        lens.push(lens[i - 1] + segment);
    }
    let total = *lens.last().expect("non-empty");
    if total == 0.0 {
        return Some(LabelPlacement {
            x: points[0].x,
            y: points[0].y,
        });
    }
    let target = total / 2.0;
    for i in 1..points.len() {
        if lens[i] >= target {
            let prev_len = lens[i - 1];
            let seg = lens[i] - prev_len;
            let f = if seg > 0.0 {
                (target - prev_len) / seg
            } else {
                0.0
            };
            let a = points[i - 1];
            let b = points[i];
            return Some(LabelPlacement {
                x: a.x + (b.x - a.x) * f,
                y: a.y + (b.y - a.y) * f,
            });
        }
    }
    let last = *points.last().expect("non-empty");
    Some(LabelPlacement {
        x: last.x,
        y: last.y,
    })
}

// ── marker URL wiring ───────────────────────────────────────────────

/// Build the marker-end URL for an edge, matching Wave 3 P2's
/// convention of `url(#<diagram-id>-<arrow-type>)`. Returns `None`
/// when the edge has no explicit arrow type (upstream: no marker-end).
pub fn marker_end_url(edge: &Edge, diagram_id: &str) -> Option<String> {
    let ty = edge.arrow_type_end.as_deref()?;
    if ty.is_empty() {
        return None;
    }
    Some(format!("url(#{diagram_id}-{ty})"))
}

/// Build the marker-start URL (mirror of [`marker_end_url`]).
pub fn marker_start_url(edge: &Edge, diagram_id: &str) -> Option<String> {
    let ty = edge.arrow_type_start.as_deref()?;
    if ty.is_empty() {
        return None;
    }
    Some(format!("url(#{diagram_id}-{ty})"))
}

// ── tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_coord_integers_omit_decimal() {
        assert_eq!(fmt_coord(1.0), "1");
        assert_eq!(fmt_coord(100.0), "100");
        assert_eq!(fmt_coord(-5.0), "-5");
    }

    #[test]
    fn fmt_coord_rounds_to_three_decimals() {
        assert_eq!(fmt_coord(1.23456), "1.235");
        assert_eq!(fmt_coord(0.0001), "0");
        assert_eq!(fmt_coord(1.5), "1.5");
    }

    #[test]
    fn fmt_coord_negative_zero_collapses() {
        assert_eq!(fmt_coord(-0.0), "0");
        assert_eq!(fmt_coord(-0.0001), "0");
    }

    #[test]
    fn curve_type_parses_known_names() {
        assert_eq!(CurveType::parse("basis"), Some(CurveType::Basis));
        assert_eq!(CurveType::parse("linear"), Some(CurveType::Linear));
        assert_eq!(CurveType::parse("rounded"), Some(CurveType::Rounded));
        assert_eq!(CurveType::parse("unknown"), None);
    }

    #[test]
    fn curve_type_resolve_falls_back_to_basis() {
        assert_eq!(CurveType::resolve(None, None), CurveType::Basis);
        assert_eq!(CurveType::resolve(Some("linear"), None), CurveType::Linear);
        assert_eq!(CurveType::resolve(None, Some("step")), CurveType::Step);
    }

    #[test]
    fn basis_spline_four_control_points_exact_bytes() {
        // Four axis-aligned control points at (0,0), (10,0), (10,10),
        // (20,10). Computed by hand-applying d3's curveBasis algorithm:
        //   first L = ((5*0 + 10)/6, (5*0 + 0)/6) = (1.667, 0)
        //   first C from (0,0),(10,0) to (10,10):
        //     c1 = (2*0+10)/3, (2*0+0)/3 = (3.333, 0)
        //     c2 = (0+2*10)/3, (0+2*0)/3 = (6.667, 0)
        //     ex = (0+4*10+10)/6, (0+4*0+10)/6 = (8.333, 1.667)
        //   second C from (10,0),(10,10) to (20,10):
        //     c1 = (2*10+10)/3, (2*0+10)/3 = (10, 3.333)
        //     c2 = (10+2*10)/3, (0+2*10)/3 = (10, 6.667)
        //     ex = (10+4*10+20)/6, (0+4*10+10)/6 = (11.667, 8.333)
        //   tail C from (10,10),(20,10) to (20,10):
        //     c1 = (2*10+20)/3, (2*10+10)/3 = (13.333, 10)
        //     c2 = (10+2*20)/3, (10+2*10)/3 = (16.667, 10)
        //     ex = (10+4*20+20)/6, (10+4*10+10)/6 = (18.333, 10)
        //   then L 20,10.
        let pts = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 },
            Point { x: 20.0, y: 10.0 },
        ];
        let d = build_path(&pts, CurveType::Basis);
        assert_eq!(
            d,
            "M0,0L1.667,0C3.333,0,6.667,0,8.333,1.667C10,3.333,10,6.667,11.667,8.333\
             C13.333,10,16.667,10,18.333,10L20,10"
        );
    }

    #[test]
    fn linear_path_one_line_per_point() {
        let pts = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 5.5, y: 2.25 },
            Point { x: 10.0, y: 4.0 },
        ];
        assert_eq!(build_path(&pts, CurveType::Linear), "M0,0L5.5,2.25L10,4");
    }

    #[test]
    fn step_path_emits_horizontal_then_vertical() {
        let pts = vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 10.0 }];
        assert_eq!(build_path(&pts, CurveType::Step), "M0,0L5,0L5,10L10,10");
    }

    #[test]
    fn step_before_path_is_vertical_then_horizontal() {
        let pts = vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 10.0 }];
        assert_eq!(build_path(&pts, CurveType::StepBefore), "M0,0L0,10L10,10");
    }

    #[test]
    fn step_after_path_is_horizontal_then_vertical() {
        let pts = vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 10.0 }];
        assert_eq!(build_path(&pts, CurveType::StepAfter), "M0,0L10,0L10,10");
    }

    #[test]
    fn step_after_with_three_points_emits_duplicate_corner() {
        let pts = vec![
            Point { x: 5.0, y: 0.0 },
            Point { x: 10.0, y: 5.0 },
            Point { x: 10.0, y: 10.0 },
        ];
        assert_eq!(
            build_path(&pts, CurveType::StepAfter),
            "M5,0L10,0L10,5L10,5L10,10"
        );
    }

    #[test]
    fn rounded_path_emits_quad_bezier_corner() {
        // Right-angle corner at (10,0).
        let pts = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 },
        ];
        let d = build_path(&pts, CurveType::Rounded);
        assert!(d.starts_with("M0,0"));
        assert!(d.contains('Q'));
        assert!(d.ends_with("L10,10"));
    }

    #[test]
    fn label_position_midpoint_of_straight_segment() {
        let pts = vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 }];
        let pos = label_position(&pts).unwrap();
        assert!((pos.x - 5.0).abs() < 1e-9);
        assert!((pos.y - 0.0).abs() < 1e-9);
    }

    #[test]
    fn label_position_midpoint_of_corner_polyline() {
        // 10-long horizontal + 10-long vertical → total 20, midpoint
        // lies at end of the first segment → (10, 0).
        let pts = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 },
        ];
        let pos = label_position(&pts).unwrap();
        assert!((pos.x - 10.0).abs() < 1e-9);
        assert!((pos.y - 0.0).abs() < 1e-9);
    }

    #[test]
    fn clip_endpoints_trims_rectangular_nodes() {
        // Two rectangular nodes, centres at (0,0) and (100,0), width
        // 40 height 20. An edge through the centres should clip to
        // (20,0) and (80,0).
        let src = Node {
            id: "a".into(),
            shape: Some("rect".into()),
            x: Some(0.0),
            y: Some(0.0),
            width: Some(40.0),
            height: Some(20.0),
            ..Default::default()
        };
        let dst = Node {
            id: "b".into(),
            shape: Some("rect".into()),
            x: Some(100.0),
            y: Some(0.0),
            width: Some(40.0),
            height: Some(20.0),
            ..Default::default()
        };
        let mut edge = Edge::default();
        edge.points = Some(vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 50.0, y: 0.0 },
            Point { x: 100.0, y: 0.0 },
        ]);
        let clipped = clip_endpoints(&edge, &src, &dst);
        assert_eq!(clipped.len(), 3);
        assert!((clipped[0].x - 20.0).abs() < 1e-9);
        assert!((clipped[2].x - 80.0).abs() < 1e-9);
    }

    #[test]
    fn marker_url_uses_diagram_prefix() {
        let mut edge = Edge::default();
        edge.arrow_type_end = Some("pointEnd".into());
        assert_eq!(
            marker_end_url(&edge, "flow-1").as_deref(),
            Some("url(#flow-1-pointEnd)")
        );
        assert_eq!(marker_start_url(&edge, "flow-1"), None);
    }

    #[test]
    fn natural_curve_matches_d3() {
        let pts4 = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 },
            Point { x: 20.0, y: 10.0 },
        ];
        assert_eq!(
            build_path(&pts4, CurveType::Natural),
            "M0,0C4.444,-1.111,8.889,-2.222,10,0C11.111,2.222,8.889,7.778,10,10C11.111,12.222,15.556,11.111,20,10"
        );

        let pts3 = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 50.0, y: 0.0 },
            Point { x: 100.0, y: 50.0 },
        ];
        assert_eq!(
            build_path(&pts3, CurveType::Natural),
            "M0,0C16.667,-4.167,33.333,-8.333,50,0C66.667,8.333,83.333,29.167,100,50"
        );

        let pts2 = vec![Point { x: 0.0, y: 0.0 }, Point { x: 100.0, y: 0.0 }];
        assert_eq!(build_path(&pts2, CurveType::Natural), "M0,0L100,0");

        let pts1 = vec![Point { x: 0.0, y: 0.0 }];
        assert_eq!(build_path(&pts1, CurveType::Natural), "M0,0Z");
    }

    #[test]
    fn monotone_x_curve_matches_d3() {
        let pts4 = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 },
            Point { x: 20.0, y: 10.0 },
        ];
        assert_eq!(
            build_path(&pts4, CurveType::MonotoneX),
            "M0,0C3.333,0,6.667,0,10,0C10,0,10,10,10,10C13.333,10,16.667,10,20,10"
        );

        let pts3 = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 50.0, y: 0.0 },
            Point { x: 100.0, y: 50.0 },
        ];
        assert_eq!(
            build_path(&pts3, CurveType::MonotoneX),
            "M0,0C16.667,0,33.333,0,50,0C66.667,0,83.333,25,100,50"
        );

        let pts2 = vec![Point { x: 0.0, y: 0.0 }, Point { x: 100.0, y: 0.0 }];
        assert_eq!(build_path(&pts2, CurveType::MonotoneX), "M0,0L100,0");

        let pts1 = vec![Point { x: 0.0, y: 0.0 }];
        assert_eq!(build_path(&pts1, CurveType::MonotoneX), "M0,0Z");
    }

    #[test]
    fn monotone_y_curve_matches_d3() {
        let pts3 = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 50.0, y: 0.0 },
            Point { x: 100.0, y: 50.0 },
        ];
        assert_eq!(
            build_path(&pts3, CurveType::MonotoneY),
            "M0,0C0,0,50,0,50,0C83.333,16.667,91.667,33.333,100,50"
        );

        let pts2 = vec![Point { x: 0.0, y: 0.0 }, Point { x: 100.0, y: 0.0 }];
        assert_eq!(build_path(&pts2, CurveType::MonotoneY), "M0,0L100,0");
    }

    #[test]
    fn bump_x_curve_matches_d3() {
        let pts3 = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 50.0, y: 0.0 },
            Point { x: 100.0, y: 50.0 },
        ];
        assert_eq!(
            build_path(&pts3, CurveType::BumpX),
            "M0,0C25,0,25,0,50,0C75,0,75,50,100,50"
        );

        let pts2 = vec![Point { x: 0.0, y: 0.0 }, Point { x: 100.0, y: 0.0 }];
        assert_eq!(build_path(&pts2, CurveType::BumpX), "M0,0C50,0,50,0,100,0");
    }

    #[test]
    fn bump_y_curve_matches_d3() {
        let pts3 = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 50.0, y: 0.0 },
            Point { x: 100.0, y: 50.0 },
        ];
        assert_eq!(
            build_path(&pts3, CurveType::BumpY),
            "M0,0C0,0,50,0,50,0C50,25,100,25,100,50"
        );

        let pts2 = vec![Point { x: 0.0, y: 0.0 }, Point { x: 100.0, y: 0.0 }];
        assert_eq!(build_path(&pts2, CurveType::BumpY), "M0,0C0,0,100,0,100,0");
    }

    #[test]
    fn catmull_rom_curve_matches_d3() {
        let pts4 = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 },
            Point { x: 20.0, y: 10.0 },
        ];
        assert_eq!(
            build_path(&pts4, CurveType::CatmullRom),
            "M0,0C0,0,8.333,-1.667,10,0C11.667,1.667,8.333,8.333,10,10C11.667,11.667,20,10,20,10"
        );

        let pts3 = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 50.0, y: 0.0 },
            Point { x: 100.0, y: 50.0 },
        ];
        assert_eq!(
            build_path(&pts3, CurveType::CatmullRom),
            "M0,0C0,0,34.545,-6.402,50,0C68.38,7.613,100,50,100,50"
        );

        let pts2 = vec![Point { x: 0.0, y: 0.0 }, Point { x: 100.0, y: 0.0 }];
        assert_eq!(build_path(&pts2, CurveType::CatmullRom), "M0,0L100,0");

        let pts1 = vec![Point { x: 0.0, y: 0.0 }];
        assert_eq!(build_path(&pts1, CurveType::CatmullRom), "M0,0Z");
    }

    #[test]
    fn cardinal_curve_matches_d3() {
        let pts4 = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 },
            Point { x: 20.0, y: 10.0 },
        ];
        assert_eq!(
            build_path(&pts4, CurveType::Cardinal),
            "M0,0C0,0,8.333,-1.667,10,0C11.667,1.667,8.333,8.333,10,10C11.667,11.667,20,10,20,10"
        );

        let pts3 = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 50.0, y: 0.0 },
            Point { x: 100.0, y: 50.0 },
        ];
        assert_eq!(
            build_path(&pts3, CurveType::Cardinal),
            "M0,0C0,0,33.333,-8.333,50,0C66.667,8.333,100,50,100,50"
        );

        let pts2 = vec![Point { x: 0.0, y: 0.0 }, Point { x: 100.0, y: 0.0 }];
        assert_eq!(build_path(&pts2, CurveType::Cardinal), "M0,0L100,0");

        let pts1 = vec![Point { x: 0.0, y: 0.0 }];
        assert_eq!(build_path(&pts1, CurveType::Cardinal), "M0,0Z");
    }

    #[test]
    fn five_point_curves_match_d3() {
        let pts5 = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 30.0, y: 50.0 },
            Point { x: 60.0, y: 20.0 },
            Point { x: 90.0, y: 80.0 },
            Point { x: 120.0, y: 40.0 },
        ];
        assert_eq!(
            build_path(&pts5, CurveType::Natural),
            "M0,0C10,26.548,20,53.095,30,50C40,46.905,50,14.167,60,20C70,25.833,80,70.238,90,80C100,89.762,110,64.881,120,40"
        );
        assert_eq!(
            build_path(&pts5, CurveType::MonotoneX),
            "M0,0C10,25,20,50,30,50C40,50,50,20,60,20C70,20,80,80,90,80C100,80,110,60,120,40"
        );
        assert_eq!(
            build_path(&pts5, CurveType::MonotoneY),
            "M0,0C15,16.667,30,33.333,30,50C30,40,60,30,60,20C60,40,90,60,90,80C90,66.667,105,53.333,120,40"
        );
        assert_eq!(
            build_path(&pts5, CurveType::BumpX),
            "M0,0C15,0,15,50,30,50C45,50,45,20,60,20C75,20,75,80,90,80C105,80,105,40,120,40"
        );
        assert_eq!(
            build_path(&pts5, CurveType::BumpY),
            "M0,0C0,25,30,25,30,50C30,35,60,35,60,20C60,50,90,50,90,80C90,60,120,60,120,40"
        );
        assert_eq!(
            build_path(&pts5, CurveType::CatmullRom),
            "M0,0C0,0,19.07,48.654,30,50C39.323,51.148,50.907,18.524,60,20C71.434,21.855,79.15,79.022,90,80C99.367,80.845,120,40,120,40"
        );
        assert_eq!(
            build_path(&pts5, CurveType::Cardinal),
            "M0,0C0,0,20,46.667,30,50C40,53.333,50,15,60,20C70,25,80,76.667,90,80C100,83.333,120,40,120,40"
        );
    }
}
