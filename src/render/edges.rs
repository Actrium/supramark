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
    /// Stubbed — upstream supports curveCardinal / catmullRom / bumpX/Y
    /// / monotoneX/Y / natural. Fall back to curveBasis for those until
    /// the math is ported.
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
/// output byte-for-byte for the supported curves. Cardinal /
/// catmullRom / bump* / monotone* / natural currently fall back to
/// curveBasis (see module-level TODO).
pub fn build_path(points: &[Point], curve: CurveType) -> String {
    if points.is_empty() {
        return String::new();
    }
    match curve {
        CurveType::Basis
        | CurveType::Cardinal
        | CurveType::CatmullRom
        | CurveType::BumpX
        | CurveType::BumpY
        | CurveType::MonotoneX
        | CurveType::MonotoneY
        | CurveType::Natural => path_basis(points),
        CurveType::Linear => path_linear(points),
        CurveType::Step => path_step(points, 0.5),
        CurveType::StepBefore => path_step(points, 0.0),
        CurveType::StepAfter => path_step(points, 1.0),
        CurveType::Rounded => path_rounded(points, 5.0),
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
        CurveType::Basis
        | CurveType::Cardinal
        | CurveType::CatmullRom
        | CurveType::BumpX
        | CurveType::BumpY
        | CurveType::MonotoneX
        | CurveType::MonotoneY
        | CurveType::Natural => basis_visited_points(points),
        CurveType::Linear => points.iter().map(|p| (p.x, p.y)).collect(),
        CurveType::Step => step_visited_points(points, 0.5),
        CurveType::StepBefore => step_visited_points(points, 0.0),
        CurveType::StepAfter => step_visited_points(points, 1.0),
        CurveType::Rounded => points.iter().map(|p| (p.x, p.y)).collect(),
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
    let mut x_prev = 0.0;
    let mut y_prev = 0.0;
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            out.push((p.x, p.y));
        } else {
            let x_bend = x_prev + (p.x - x_prev) * t;
            if x_bend != x_prev {
                out.push((x_bend, y_prev));
            }
            if x_bend != p.x {
                out.push((x_bend, p.y));
            }
            out.push((p.x, p.y));
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
    let mut x_prev = 0.0;
    let mut y_prev = 0.0;
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            let _ = write!(d, "M{},{}", fmt_coord(p.x), fmt_coord(p.y));
        } else {
            // step point: horizontal bend at x = lerp(x_prev, p.x, t).
            let x_bend = x_prev + (p.x - x_prev) * t;
            if x_bend != x_prev {
                let _ = write!(d, "L{},{}", fmt_coord(x_bend), fmt_coord(y_prev));
            }
            if x_bend != p.x {
                let _ = write!(d, "L{},{}", fmt_coord(x_bend), fmt_coord(p.y));
            }
            let _ = write!(d, "L{},{}", fmt_coord(p.x), fmt_coord(p.y));
        }
        x_prev = p.x;
        y_prev = p.y;
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
        "circle" | "ellipse" | "doublecircle" | "stadium" | "stateStart" | "state_start"
        | "start" | "stateEnd" | "state_end" | "end" => {
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
}
