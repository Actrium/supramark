//! Geometric intersection helpers for edge-endpoint clipping.
//!
//! Adapted from mermaid-rs-renderer (https://github.com/1jehuang/mermaid-rs-renderer),
//! MIT license. Specifically `ray_polygon_intersection`,
//! `ray_ellipse_intersection` from src/layout/routing.rs. These
//! mirror upstream mermaid-js's `intersectPolygon` /
//! `intersectRect` used at edge-to-node clipping time.

/// 2-D point. Matches mmdr's `(f32, f32)` convention so call-sites can
/// pass tuples directly.
pub type Point = (f32, f32);

/// Intersect a ray with a closed polygon's boundary.
///
/// The ray starts at `origin` and travels along `dir` (not necessarily
/// a unit vector). `poly` is a sequence of vertices in order — the
/// segment wrapping from the last back to the first is included.
///
/// Returns the farthest forward-hit along the ray — the intersection
/// with the *exit* edge when `origin` lies inside the polygon. This
/// matches upstream mermaid-js's `intersectPolygon` behaviour used for
/// edge clipping from a node's centre outward.
///
/// `None` when the ray fails to cross any edge (e.g. `origin` is
/// outside the polygon and the ray points away).
pub fn ray_polygon_intersection(origin: Point, dir: Point, poly: &[Point]) -> Option<Point> {
    let mut best_t: Option<f32> = None;
    let ox = origin.0;
    let oy = origin.1;
    let rx = dir.0;
    let ry = dir.1;
    if poly.len() < 2 {
        return None;
    }
    for i in 0..poly.len() {
        let (x1, y1) = poly[i];
        let (x2, y2) = poly[(i + 1) % poly.len()];
        let sx = x2 - x1;
        let sy = y2 - y1;
        let qx = x1 - ox;
        let qy = y1 - oy;
        let denom = rx * sy - ry * sx;
        if denom.abs() < 1e-6 {
            continue;
        }
        let t = (qx * sy - qy * sx) / denom;
        let u = (qx * ry - qy * rx) / denom;
        if t >= 0.0 && (0.0..=1.0).contains(&u) {
            match best_t {
                Some(best) if t >= best => {}
                _ => best_t = Some(t),
            }
        }
    }
    best_t.map(|t| (ox + rx * t, oy + ry * t))
}

/// Intersect a ray with an axis-aligned ellipse boundary.
///
/// `origin` is the ray's start, `dir` its direction (not necessarily
/// unit). `center` is the ellipse's centre and `rx`/`ry` its radii
/// along the x/y axes. Returns the nearest non-negative-t hit, or
/// `None` if the ray misses the ellipse or is parallel to degeneracy.
///
/// Mirrors upstream mermaid-js's `intersectRect`-family helper for
/// ellipse / circle node shapes.
pub fn ray_ellipse_intersection(
    origin: Point,
    dir: Point,
    center: Point,
    rx: f32,
    ry: f32,
) -> Option<Point> {
    let (ox, oy) = origin;
    let (dx, dy) = dir;
    let (cx, cy) = center;
    let ox_local = ox - cx;
    let oy_local = oy - cy;
    let a = (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry);
    let b = 2.0 * ((ox_local * dx) / (rx * rx) + (oy_local * dy) / (ry * ry));
    let c = (ox_local * ox_local) / (rx * rx) + (oy_local * oy_local) / (ry * ry) - 1.0;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 || a.abs() < 1e-6 {
        return None;
    }
    let sqrt_disc = disc.sqrt();
    let t1 = (-b - sqrt_disc) / (2.0 * a);
    let t2 = (-b + sqrt_disc) / (2.0 * a);
    let t = if t1 >= 0.0 {
        t1
    } else if t2 >= 0.0 {
        t2
    } else {
        return None;
    };
    Some((origin.0 + dx * t, origin.1 + dy * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn polygon_square_ray_right_hits_right_edge() {
        // Unit square centred at (0,0), ray from centre pointing +x.
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
        let hit =
            ray_ellipse_intersection((0.0, 0.0), (1.0, 0.0), (0.0, 0.0), 2.0, 1.0).unwrap();
        assert!(approx_eq(hit.0, 2.0));
        assert!(approx_eq(hit.1, 0.0));
    }

    #[test]
    fn ellipse_ray_from_centre_hits_radius_y() {
        let hit =
            ray_ellipse_intersection((0.0, 0.0), (0.0, 1.0), (0.0, 0.0), 2.0, 1.0).unwrap();
        assert!(approx_eq(hit.0, 0.0));
        assert!(approx_eq(hit.1, 1.0));
    }

    #[test]
    fn ellipse_miss_returns_none() {
        // Ray parallel to x-axis starting well above the ellipse: never hits.
        assert!(
            ray_ellipse_intersection((5.0, 10.0), (1.0, 0.0), (0.0, 0.0), 2.0, 1.0).is_none()
        );
    }
}
