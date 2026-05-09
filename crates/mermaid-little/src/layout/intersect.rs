//! Geometric intersection helpers for edge-endpoint clipping.
//!
//! Thin adapters over [`dagre::intersect`]. We previously vendored
//! `ray_polygon_intersection` and `ray_ellipse_intersection` because
//! dagre-rs exposed only `pub(crate)` versions; that constraint is gone
//! as of `dagre@a998f68` so the math now lives upstream and this module
//! only translates the `(f32, f32)` tuple convention used by the
//! renderer to / from dagre's `Point { x: f64, y: f64 }`.

use dagre::Point as DagrePoint;

/// 2-D point as `(f32, f32)`. Renderer-side call sites pass these
/// directly out of edge waypoint computations.
pub type Point = (f32, f32);

fn dp(p: Point) -> DagrePoint {
    DagrePoint {
        x: p.0 as f64,
        y: p.1 as f64,
    }
}

/// Intersect a ray with a closed polygon's boundary.
///
/// The ray starts at `origin` and travels along `dir`. `poly` is the
/// polygon's vertices in order (no repeated closing vertex). Returns
/// the closest forward-hit along the ray.
///
/// Wraps [`dagre::intersect::intersect_polygon`], which always returns a
/// point (falls back to `origin + dir` when no edge is crossed). We
/// preserve the `Option` shape by returning `None` when the polygon is
/// degenerate (< 2 vertices), matching the previous behaviour expected
/// by the renderer.
pub fn ray_polygon_intersection(origin: Point, dir: Point, poly: &[Point]) -> Option<Point> {
    if poly.len() < 2 {
        return None;
    }
    let center = dp(origin);
    let target = DagrePoint {
        x: (origin.0 + dir.0) as f64,
        y: (origin.1 + dir.1) as f64,
    };
    let vertices: Vec<DagrePoint> = poly.iter().copied().map(dp).collect();
    let hit = dagre::intersect::intersect_polygon(&vertices, &center, &target);
    Some((hit.x as f32, hit.y as f32))
}

/// Intersect a ray with an axis-aligned ellipse boundary.
///
/// `origin` is the ray's start, `dir` its direction. `center`/`rx`/`ry`
/// describe the ellipse. Returns the nearest non-negative-t hit, or
/// `None` if the ray is degenerate (zero direction).
///
/// Wraps [`dagre::intersect::intersect_ellipse`].
pub fn ray_ellipse_intersection(
    origin: Point,
    dir: Point,
    center: Point,
    rx: f32,
    ry: f32,
) -> Option<Point> {
    if dir.0.abs() < f32::EPSILON && dir.1.abs() < f32::EPSILON {
        return None;
    }
    let target = DagrePoint {
        x: (origin.0 + dir.0) as f64,
        y: (origin.1 + dir.1) as f64,
    };
    let hit = dagre::intersect::intersect_ellipse(
        center.0 as f64,
        center.1 as f64,
        rx as f64,
        ry as f64,
        &target,
    );
    Some((hit.x as f32, hit.y as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn polygon_square_ray_right_hits_right_edge() {
        let poly = vec![(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        let hit = ray_polygon_intersection((0.0, 0.0), (1.0, 0.0), &poly).unwrap();
        assert!(approx_eq(hit.0, 1.0));
        assert!(approx_eq(hit.1, 0.0));
    }

    #[test]
    fn polygon_degenerate_returns_none() {
        assert!(ray_polygon_intersection((0.0, 0.0), (1.0, 0.0), &[]).is_none());
        assert!(ray_polygon_intersection((0.0, 0.0), (1.0, 0.0), &[(0.0, 0.0)]).is_none());
    }

    #[test]
    fn ellipse_ray_from_centre_hits_radius_x() {
        let hit = ray_ellipse_intersection((0.0, 0.0), (1.0, 0.0), (0.0, 0.0), 2.0, 1.0).unwrap();
        assert!(approx_eq(hit.0, 2.0));
        assert!(approx_eq(hit.1, 0.0));
    }

    #[test]
    fn ellipse_ray_from_centre_hits_radius_y() {
        let hit = ray_ellipse_intersection((0.0, 0.0), (0.0, 1.0), (0.0, 0.0), 2.0, 1.0).unwrap();
        assert!(approx_eq(hit.0, 0.0));
        assert!(approx_eq(hit.1, 1.0));
    }

    #[test]
    fn ellipse_zero_direction_returns_none() {
        // Degenerate dir → None, matching the previous local behaviour.
        assert!(ray_ellipse_intersection((5.0, 10.0), (0.0, 0.0), (0.0, 0.0), 2.0, 1.0).is_none());
    }
}
