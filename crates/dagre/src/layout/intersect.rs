//! Geometric intersection helpers for laying out edge endpoints on node
//! boundaries.
//!
//! `dagre.js` itself only ships `intersectRect`; downstream renderers
//! (notably `dagre-d3-es` and `mermaid`) layer their own polygon / ellipse
//! callbacks on top of dagre's output to clip edges against the actual
//! shape of each node. dagre-rs's internal layout pipeline calls into
//! these helpers from [`crate::layout::util::intersect_node`] for the
//! shapes it knows about (rect, ellipse, diamond) and exposes the same
//! helpers publicly here so downstream Rust code does not have to vendor
//! its own copy.
//!
//! All functions take primitive coordinates rather than `NodeLabel` so
//! callers can use them with their own data types. The math mirrors
//! `dagre-d3-es`'s `intersect/*.js` files.
//!
//! # Conventions
//!
//! * `point` is the *target* (typically the next waypoint or the other
//!   endpoint of the edge).
//! * The shape is centered at `(cx, cy)` (or implied from `vertices` for
//!   polygons).
//! * Returned point is the intersection of the *ray from center to
//!   `point`* with the shape's boundary, projected onto the shape side
//!   facing `point`. If `point` lies on the center, the center is
//!   returned (degenerate fallback matching upstream).
//!
//! # Example
//!
//! ```
//! use dagre::layout::intersect::{intersect_rect, intersect_ellipse, intersect_polygon};
//! use dagre::Point;
//!
//! // Where does the line from (0,0) → (200,0) leave a 100×60 rect?
//! let p = intersect_rect(0.0, 0.0, 100.0, 60.0, &Point { x: 200.0, y: 0.0 });
//! assert!((p.x - 50.0).abs() < 1e-9 && p.y.abs() < 1e-9);
//! ```

use crate::layout::types::Point;

/// Intersect the ray from `(cx, cy)` toward `point` with the boundary of
/// an axis-aligned rectangle of size `w × h` centered at `(cx, cy)`.
///
/// Ports `dagre-d3-es`'s `intersect/intersect-rect.js`.
pub fn intersect_rect(cx: f64, cy: f64, w: f64, h: f64, point: &Point) -> Point {
    let dx = point.x - cx;
    let dy = point.y - cy;
    let mut hw = w / 2.0;
    let mut hh = h / 2.0;

    if dx == 0.0 && dy == 0.0 {
        return Point { x: cx, y: cy };
    }

    let (sx, sy);
    if dy.abs() * hw > dx.abs() * hh {
        if dy < 0.0 {
            hh = -hh;
        }
        sx = if dy != 0.0 { hh * dx / dy } else { 0.0 };
        sy = hh;
    } else {
        if dx < 0.0 {
            hw = -hw;
        }
        sx = hw;
        sy = if dx != 0.0 { hw * dy / dx } else { 0.0 };
    }

    Point {
        x: cx + sx,
        y: cy + sy,
    }
}

/// Intersect the ray from `(cx, cy)` toward `point` with the boundary of
/// an axis-aligned ellipse with semi-axes `rx, ry` centered at `(cx, cy)`.
/// Pass `rx == ry` for a circle.
///
/// Ports `dagre-d3-es`'s `intersect/intersect-ellipse.js` /
/// `intersect-circle.js`.
pub fn intersect_ellipse(cx: f64, cy: f64, rx: f64, ry: f64, point: &Point) -> Point {
    let px = cx - point.x;
    let py = cy - point.y;
    if px == 0.0 && py == 0.0 {
        return Point { x: cx, y: cy };
    }
    let det = (rx * rx * py * py + ry * ry * px * px).sqrt();
    if det == 0.0 {
        return Point { x: cx, y: cy };
    }
    let mut dx = (rx * ry * px / det).abs();
    if point.x < cx {
        dx = -dx;
    }
    let mut dy = (rx * ry * py / det).abs();
    if point.y < cy {
        dy = -dy;
    }
    Point {
        x: cx + dx,
        y: cy + dy,
    }
}

/// Intersect the ray from `center` toward `point` with the boundary of a
/// closed polygon defined by `vertices` (in order, no repeated closing
/// point). Returns the closest intersection along the ray.
///
/// Ports `dagre-d3-es`'s `intersect/intersect-polygon.js`. The vertex
/// order need only define the boundary; convex / concave does not matter.
/// If `point` coincides with `center` or no edge of the polygon is
/// crossed (degenerate), `point` itself is returned — same fallback as
/// the JS version.
pub fn intersect_polygon(vertices: &[Point], center: &Point, point: &Point) -> Point {
    let dx = point.x - center.x;
    let dy = point.y - center.y;

    if dx == 0.0 && dy == 0.0 {
        return Point {
            x: center.x,
            y: center.y,
        };
    }

    let n = vertices.len();
    if n < 2 {
        return Point {
            x: point.x,
            y: point.y,
        };
    }

    let mut best_t: Option<f64> = None;
    for i in 0..n {
        let (x1, y1) = (vertices[i].x, vertices[i].y);
        let (x2, y2) = (vertices[(i + 1) % n].x, vertices[(i + 1) % n].y);
        let ex = x2 - x1;
        let ey = y2 - y1;
        let fx = x1 - center.x;
        let fy = y1 - center.y;
        let denom = dx * ey - dy * ex;
        if denom.abs() < 1e-10 {
            continue;
        }
        let t = (fx * ey - fy * ex) / denom;
        let u = (fx * dy - fy * dx) / denom;
        if t >= 0.0 && (0.0..=1.0).contains(&u) {
            match best_t {
                None => best_t = Some(t),
                Some(prev) if t < prev => best_t = Some(t),
                _ => {}
            }
        }
    }

    match best_t {
        Some(t) => Point {
            x: center.x + dx * t,
            y: center.y + dy * t,
        },
        None => Point {
            x: point.x,
            y: point.y,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(p: &Point, x: f64, y: f64) {
        assert!(
            (p.x - x).abs() < 1e-9 && (p.y - y).abs() < 1e-9,
            "expected ({x}, {y}), got ({}, {})",
            p.x,
            p.y
        );
    }

    #[test]
    fn rect_horizontal_ray() {
        let p = intersect_rect(0.0, 0.0, 100.0, 60.0, &Point { x: 200.0, y: 0.0 });
        approx(&p, 50.0, 0.0);
    }

    #[test]
    fn rect_vertical_ray() {
        let p = intersect_rect(0.0, 0.0, 100.0, 60.0, &Point { x: 0.0, y: -200.0 });
        approx(&p, 0.0, -30.0);
    }

    #[test]
    fn rect_at_center_returns_center() {
        let p = intersect_rect(5.0, 7.0, 100.0, 60.0, &Point { x: 5.0, y: 7.0 });
        approx(&p, 5.0, 7.0);
    }

    #[test]
    fn circle_horizontal_ray() {
        // r=10 circle, target due east → should land at (10, 0).
        let p = intersect_ellipse(0.0, 0.0, 10.0, 10.0, &Point { x: 100.0, y: 0.0 });
        approx(&p, 10.0, 0.0);
    }

    #[test]
    fn ellipse_diagonal_ray() {
        // 8×6 ellipse, ray to (8,6): result lies on the ellipse and on
        // the ray, so x/y = 8/6 and x²/64 + y²/36 = 1.
        let p = intersect_ellipse(0.0, 0.0, 8.0, 6.0, &Point { x: 8.0, y: 6.0 });
        assert!(p.x > 0.0 && p.y > 0.0);
        assert!((p.y / p.x - 6.0 / 8.0).abs() < 1e-9);
        assert!((p.x * p.x / 64.0 + p.y * p.y / 36.0 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn polygon_diamond_horizontal_ray() {
        // Diamond at origin with width=100, height=60.
        let verts = [
            Point { x: 0.0, y: -30.0 },
            Point { x: 50.0, y: 0.0 },
            Point { x: 0.0, y: 30.0 },
            Point { x: -50.0, y: 0.0 },
        ];
        let p = intersect_polygon(
            &verts,
            &Point { x: 0.0, y: 0.0 },
            &Point { x: 200.0, y: 0.0 },
        );
        approx(&p, 50.0, 0.0);
    }

    #[test]
    fn polygon_at_center_returns_center() {
        let verts = [
            Point { x: 0.0, y: -10.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 0.0, y: 10.0 },
            Point { x: -10.0, y: 0.0 },
        ];
        let p = intersect_polygon(&verts, &Point { x: 0.0, y: 0.0 }, &Point { x: 0.0, y: 0.0 });
        approx(&p, 0.0, 0.0);
    }

    #[test]
    fn polygon_hexagon_ray_to_vertex() {
        // Regular hexagon with vertices at distance 10 from origin.
        let verts: Vec<Point> = (0..6)
            .map(|i| {
                let theta = std::f64::consts::PI / 3.0 * i as f64;
                Point {
                    x: 10.0 * theta.cos(),
                    y: 10.0 * theta.sin(),
                }
            })
            .collect();
        let p = intersect_polygon(
            &verts,
            &Point { x: 0.0, y: 0.0 },
            &Point { x: 50.0, y: 0.0 },
        );
        approx(&p, 10.0, 0.0);
    }
}
