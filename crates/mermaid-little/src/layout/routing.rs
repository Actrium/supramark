//! Post-dagre edge routing: spline smoothing, endpoint clipping,
//! along-path label placement, self-loop synthesis, cluster-aware
//! clipping.
//!
//! Wave 3 P0 populated this module with the basic
//! [`refine_edges`] pipeline (dedupe + midpoint label placement).
//! Wave 3 P3 expanded it with:
//!
//! * [`self_loop_points`] — synthesise a teardrop path for edges
//!   whose source and target are the same node (dagre collapses
//!   those to a degenerate point).
//! * [`clip_to_cluster_border`] — trim the first points inside a
//!   cluster AABB back to the border, matching upstream's
//!   `cutPathAtIntersect` in edges.js.
//! * [`place_label_midpoint`] / [`split_for_label`] — midpoint
//!   placement and before/after halves so the renderer can leave a
//!   gap under the label.
//!
//! Portions adapted from mmdflux (https://github.com/kevinswiber/mmdflux),
//! MIT license — overall structure of the self-loop / cluster-clip /
//! label-gap trio mirrors the `graph/routing/` subtree there.
//!
//! Endpoint-clipping primitives for individual node shapes live in
//! [`crate::layout::intersect`]; this module chains them into an
//! edge-level pipeline.

use crate::layout::unified::{Bounds, Edge, Node, Point};

// ── existing Wave 3 P0 surface ──────────────────────────────────────

/// Refine edges after dagre has placed them. Today this only fills in
/// `label_x` / `label_y` at the spline's arc-length midpoint when dagre
/// didn't set them.
///
/// We used to also dedupe consecutive near-duplicate vertices here;
/// `dagre@a998f68` does that itself (`dedupe_adjacent_edge_points`,
/// step 25b in its layout pipeline) so the local pass is gone.
///
/// More sophisticated routing (corridor avoidance, orthogonal routing,
/// label-lane packing, arrowhead clipping onto shape silhouettes) is a
/// follow-up per-diagram concern — see the helpers below.
pub fn refine_edges(_nodes: &[Node], edges: &[Edge]) -> Vec<Edge> {
    edges
        .iter()
        .map(|e| {
            let mut out = e.clone();
            if let Some(pts) = out.points.as_ref() {
                if pts.len() >= 2 && (out.label_x.is_none() || out.label_y.is_none()) {
                    let (mx, my) = midpoint_along(pts);
                    out.label_x.get_or_insert(mx);
                    out.label_y.get_or_insert(my);
                }
            }
            out
        })
        .collect()
}

/// Walk the polyline and return the (x, y) at its arc-length midpoint.
/// Degenerate paths fall back to the first point.
fn midpoint_along(points: &[Point]) -> (f64, f64) {
    if points.is_empty() {
        return (0.0, 0.0);
    }
    if points.len() == 1 {
        return (points[0].x, points[0].y);
    }

    // Sum segment lengths, then walk until we pass half.
    let lengths: Vec<f64> = points
        .windows(2)
        .map(|w| {
            let dx = w[1].x - w[0].x;
            let dy = w[1].y - w[0].y;
            (dx * dx + dy * dy).sqrt()
        })
        .collect();
    let total: f64 = lengths.iter().sum();
    if total <= 0.0 {
        return (points[0].x, points[0].y);
    }
    let target = total * 0.5;
    let mut acc = 0.0;
    for (i, &seg) in lengths.iter().enumerate() {
        if acc + seg >= target {
            let t = if seg > 0.0 { (target - acc) / seg } else { 0.0 };
            let a = points[i];
            let b = points[i + 1];
            return (a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
        }
        acc += seg;
    }
    let last = points[points.len() - 1];
    (last.x, last.y)
}

// ── self-loop synthesis ─────────────────────────────────────────────

/// Quadrant selector for [`self_loop_points`]. Upstream defaults to
/// `TopRight` unless the node sits in the top row of the graph (then
/// `BottomRight`) or the last column (then `TopLeft`). The caller
/// picks based on graph direction + node position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfLoopQuadrant {
    TopRight,
    BottomRight,
    BottomLeft,
    TopLeft,
}

/// Synthesise a self-loop path for an edge whose source and target
/// are the same node. Dagre collapses such edges to one or two
/// points; we expand them into a five-point teardrop anchored in one
/// of four quadrants outside the node.
///
/// The path's start/end points sit on the node boundary so
/// downstream basis-spline emission produces a smooth loop ending
/// right on the boundary (where markers attach).
pub fn self_loop_points(node: &Node, quadrant: SelfLoopQuadrant) -> Vec<Point> {
    let cx = node.x.unwrap_or(0.0);
    let cy = node.y.unwrap_or(0.0);
    let w = node.width.unwrap_or(0.0).max(0.0);
    let h = node.height.unwrap_or(0.0).max(0.0);
    let hw = w / 2.0;
    let hh = h / 2.0;
    // Loop extent — scales with node size but capped so large nodes
    // don't produce absurdly wide loops.
    let loop_w = (w * 0.6).clamp(20.0, 60.0);
    let loop_h = (h * 0.6).clamp(20.0, 40.0);

    match quadrant {
        SelfLoopQuadrant::TopRight => {
            let start = Point {
                x: cx + hw * 0.4,
                y: cy - hh,
            };
            let apex1 = Point {
                x: cx + hw * 0.4,
                y: cy - hh - loop_h,
            };
            let apex2 = Point {
                x: cx + hw + loop_w,
                y: cy - hh - loop_h,
            };
            let apex3 = Point {
                x: cx + hw + loop_w,
                y: cy - hh * 0.4,
            };
            let end = Point {
                x: cx + hw,
                y: cy - hh * 0.4,
            };
            vec![start, apex1, apex2, apex3, end]
        }
        SelfLoopQuadrant::BottomRight => {
            let start = Point {
                x: cx + hw,
                y: cy + hh * 0.4,
            };
            let apex1 = Point {
                x: cx + hw + loop_w,
                y: cy + hh * 0.4,
            };
            let apex2 = Point {
                x: cx + hw + loop_w,
                y: cy + hh + loop_h,
            };
            let apex3 = Point {
                x: cx + hw * 0.4,
                y: cy + hh + loop_h,
            };
            let end = Point {
                x: cx + hw * 0.4,
                y: cy + hh,
            };
            vec![start, apex1, apex2, apex3, end]
        }
        SelfLoopQuadrant::BottomLeft => {
            let start = Point {
                x: cx - hw * 0.4,
                y: cy + hh,
            };
            let apex1 = Point {
                x: cx - hw * 0.4,
                y: cy + hh + loop_h,
            };
            let apex2 = Point {
                x: cx - hw - loop_w,
                y: cy + hh + loop_h,
            };
            let apex3 = Point {
                x: cx - hw - loop_w,
                y: cy + hh * 0.4,
            };
            let end = Point {
                x: cx - hw,
                y: cy + hh * 0.4,
            };
            vec![start, apex1, apex2, apex3, end]
        }
        SelfLoopQuadrant::TopLeft => {
            let start = Point {
                x: cx - hw,
                y: cy - hh * 0.4,
            };
            let apex1 = Point {
                x: cx - hw - loop_w,
                y: cy - hh * 0.4,
            };
            let apex2 = Point {
                x: cx - hw - loop_w,
                y: cy - hh - loop_h,
            };
            let apex3 = Point {
                x: cx - hw * 0.4,
                y: cy - hh - loop_h,
            };
            let end = Point {
                x: cx - hw * 0.4,
                y: cy - hh,
            };
            vec![start, apex1, apex2, apex3, end]
        }
    }
}

// ── cluster-crossing clipping ───────────────────────────────────────

/// Clip the endpoints of an edge that crosses into a cluster AABB.
/// Mirrors upstream Mermaid's `outsideNode` + `intersection` +
/// `cutPathAtIntersect` pipeline from `dagre-wrapper/edges.js`.
///
/// Important quirks preserved for byte parity:
/// * Points ON the boundary count as outside (`>=`, not `>`).
/// * The intersection math is Mermaid's bespoke box-line solver,
///   not a generic segment clipper.
/// * Once the path enters the cluster, all remaining interior points
///   are dropped.
///
/// Returns a fresh point list. Untouched when no point lies inside
/// the cluster.
pub fn clip_to_cluster_border(points: &[Point], cluster: &Bounds) -> Vec<Point> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    let mut last_outside = points[0];
    let mut is_inside = false;

    for &p in points {
        let outside = point_outside_cluster(cluster, p);
        if !outside && !is_inside {
            // Transition outside → inside. Compute the intersection
            // of the segment (last_outside → p) with the cluster AABB.
            let hit = cluster_intersection(cluster, last_outside, p);
            if !out.iter().any(|q| same_point(q, &hit)) {
                out.push(hit);
            }
            is_inside = true;
        } else if outside {
            last_outside = p;
            if !is_inside {
                out.push(p);
            }
        }
    }
    out
}

// ── label placement / gapping ───────────────────────────────────────

/// Midpoint placement for an edge label. Walks the polyline and
/// returns the point at half total arc-length — the same algorithm
/// used inside [`refine_edges`], exposed here for callers that want
/// the midpoint without re-running the full refine pass.
pub fn place_label_midpoint(points: &[Point]) -> Option<Point> {
    if points.is_empty() {
        return None;
    }
    let (x, y) = midpoint_along(points);
    Some(Point { x, y })
}

/// Split the polyline into a "before" and "after" half separated by
/// `gap` pixels centred on the label midpoint. Used by the renderer
/// when it wants to draw the label on top of the edge without
/// overstriking the stroke.
///
/// Inspired by mmdflux's `label_gap` corridor routine (reduced to
/// the simple mid-segment case here).
pub fn split_for_label(points: &[Point], gap: f64) -> (Vec<Point>, Vec<Point>) {
    if points.len() < 2 || gap <= 0.0 {
        return (points.to_vec(), Vec::new());
    }
    let mut total = 0.0;
    for i in 1..points.len() {
        total += seg_len(points[i - 1], points[i]);
    }
    if total <= gap {
        return (points.to_vec(), Vec::new());
    }
    let mid = total / 2.0;
    let half_gap = gap / 2.0;
    let split_a = (mid - half_gap).max(0.0);
    let split_b = (mid + half_gap).min(total);

    let before = truncate_polyline(points, split_a);
    let after = tail_polyline(points, split_b);
    (before, after)
}

// ── edge-level orchestration ────────────────────────────────────────

/// "Route this single edge" convenience. When source == target, runs
/// [`self_loop_points`] (TopRight quadrant by default). Always
/// updates `label_x` / `label_y` to the polyline midpoint when there
/// are enough points.
pub fn route_edge(edge: &mut Edge, source: &Node, target: &Node) {
    if source.id == target.id {
        edge.points = Some(self_loop_points(source, SelfLoopQuadrant::TopRight));
    }
    if let Some(pts) = edge.points.as_deref() {
        if let Some(mid) = place_label_midpoint(pts) {
            edge.label_x = Some(mid.x);
            edge.label_y = Some(mid.y);
        }
    }
    // Reference `target` so test-only call paths don't flag it unused.
    let _ = &target.id;
}

// ── geometry primitives ─────────────────────────────────────────────

fn point_outside_cluster(b: &Bounds, p: Point) -> bool {
    let cx = b.x + b.width / 2.0;
    let cy = b.y + b.height / 2.0;
    let dx = (p.x - cx).abs();
    let dy = (p.y - cy).abs();
    let w = b.width / 2.0;
    let h = b.height / 2.0;
    dx >= w || dy >= h
}

fn same_point(a: &Point, b: &Point) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
}

fn seg_len(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}

/// Mirror Mermaid's `intersection(boundaryNode, outsidePoint, insidePoint)`.
/// This intentionally preserves upstream's branchy rectangle solver instead
/// of using a generic line-clipper.
fn cluster_intersection(b: &Bounds, outside: Point, inside: Point) -> Point {
    let node_x = b.x + b.width / 2.0;
    let node_y = b.y + b.height / 2.0;
    let w = b.width / 2.0;
    let h = b.height / 2.0;

    let q_total = (outside.y - inside.y).abs();
    let r_total = (outside.x - inside.x).abs();

    if (node_y - outside.y).abs() * w > (node_x - outside.x).abs() * h {
        let q = if inside.y < outside.y {
            outside.y - h - node_y
        } else {
            node_y - h - outside.y
        };
        let r = r_total * q / q_total;
        let mut res_x = if inside.x < outside.x {
            inside.x + r
        } else {
            inside.x - r_total + r
        };
        let mut res_y = if inside.y < outside.y {
            inside.y + q_total - q
        } else {
            inside.y - q_total + q
        };
        if r == 0.0 {
            res_x = outside.x;
            res_y = outside.y;
        }
        if r_total == 0.0 {
            res_x = outside.x;
        }
        if q_total == 0.0 {
            res_y = outside.y;
        }
        Point { x: res_x, y: res_y }
    } else {
        let r = if inside.x < outside.x {
            outside.x - w - node_x
        } else {
            node_x - w - outside.x
        };
        let q = q_total * r / r_total;
        let mut res_x = if inside.x < outside.x {
            inside.x + r_total - r
        } else {
            inside.x - r_total + r
        };
        let mut res_y = if inside.y < outside.y {
            inside.y + q
        } else {
            inside.y - q
        };
        if r == 0.0 {
            res_x = outside.x;
            res_y = outside.y;
        }
        if r_total == 0.0 {
            res_x = outside.x;
        }
        if q_total == 0.0 {
            res_y = outside.y;
        }
        Point { x: res_x, y: res_y }
    }
}

fn truncate_polyline(points: &[Point], target_len: f64) -> Vec<Point> {
    if points.is_empty() || target_len <= 0.0 {
        return points.first().copied().into_iter().collect();
    }
    let mut out = Vec::with_capacity(points.len());
    out.push(points[0]);
    let mut acc = 0.0;
    for i in 1..points.len() {
        let seg = seg_len(points[i - 1], points[i]);
        if acc + seg >= target_len {
            let f = if seg > 0.0 {
                (target_len - acc) / seg
            } else {
                0.0
            };
            let a = points[i - 1];
            let b = points[i];
            out.push(Point {
                x: a.x + (b.x - a.x) * f,
                y: a.y + (b.y - a.y) * f,
            });
            return out;
        }
        out.push(points[i]);
        acc += seg;
    }
    out
}

fn tail_polyline(points: &[Point], start_len: f64) -> Vec<Point> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut acc = 0.0;
    for i in 1..points.len() {
        let seg = seg_len(points[i - 1], points[i]);
        if acc + seg >= start_len {
            let f = if seg > 0.0 {
                (start_len - acc) / seg
            } else {
                0.0
            };
            let a = points[i - 1];
            let b = points[i];
            let mut out = vec![Point {
                x: a.x + (b.x - a.x) * f,
                y: a.y + (b.y - a.y) * f,
            }];
            out.extend_from_slice(&points[i..]);
            return out;
        }
        acc += seg;
    }
    points.last().map(|p| vec![*p]).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::unified::Point;

    #[test]
    fn midpoint_of_straight_line_is_centre() {
        let pts = vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 }];
        let (x, y) = midpoint_along(&pts);
        assert!((x - 5.0).abs() < 1e-9);
        assert!(y.abs() < 1e-9);
    }

    #[test]
    fn midpoint_handles_empty() {
        let (x, y) = midpoint_along(&[]);
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn refine_populates_label_coords_when_missing() {
        let pts = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 20.0, y: 0.0 },
        ];
        let mut e = Edge::default();
        e.id = "x".into();
        e.points = Some(pts);

        let out = refine_edges(&[], &[e]);
        let e = &out[0];
        assert!(e.label_x.is_some());
        assert!(e.label_y.is_some());
        // Midpoint of a flat horizontal line from 0..20 is x=10.
        assert!((e.label_x.unwrap() - 10.0).abs() < 1e-6);
    }

    fn demo_node() -> Node {
        let mut n = Node::default();
        n.id = "n".into();
        n.x = Some(100.0);
        n.y = Some(100.0);
        n.width = Some(40.0);
        n.height = Some(40.0);
        n
    }

    #[test]
    fn self_loop_40x40_top_right_has_five_points() {
        let node = demo_node();
        let pts = self_loop_points(&node, SelfLoopQuadrant::TopRight);
        assert_eq!(pts.len(), 5);
        // Start sits on top edge, end on right edge.
        let cx = node.x.unwrap();
        let cy = node.y.unwrap();
        let hw = node.width.unwrap() / 2.0;
        let hh = node.height.unwrap() / 2.0;
        assert!((pts[0].y - (cy - hh)).abs() < 1e-9);
        assert!(pts[0].x > cx, "start x > cx for top-right quadrant");
        assert!((pts[4].x - (cx + hw)).abs() < 1e-9);
        assert!(pts[4].y < cy, "end y < cy for top-right quadrant");
        // Apex points sit outside the node bounding box.
        assert!(pts[1].y < cy - hh);
        assert!(pts[2].x > cx + hw);
        assert!(pts[2].y < cy - hh);
    }

    #[test]
    fn self_loop_quadrants_symmetric_by_sign() {
        let node = demo_node();
        let tr = self_loop_points(&node, SelfLoopQuadrant::TopRight);
        let bl = self_loop_points(&node, SelfLoopQuadrant::BottomLeft);
        let cx = node.x.unwrap();
        let cy = node.y.unwrap();
        let hw = node.width.unwrap() / 2.0;
        let hh = node.height.unwrap() / 2.0;
        assert!(tr[2].x > cx + hw);
        assert!(tr[2].y < cy - hh);
        assert!(bl[2].x < cx - hw);
        assert!(bl[2].y > cy + hh);
    }

    #[test]
    fn place_label_midpoint_corner_polyline() {
        // 10 + 10 = 20 total; midpoint at (10, 0).
        let pts = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 },
        ];
        let mid = place_label_midpoint(&pts).unwrap();
        assert!((mid.x - 10.0).abs() < 1e-9);
        assert!((mid.y - 0.0).abs() < 1e-9);
    }

    #[test]
    fn split_for_label_gaps_straight_line() {
        let pts = vec![Point { x: 0.0, y: 0.0 }, Point { x: 100.0, y: 0.0 }];
        let (a, b) = split_for_label(&pts, 20.0);
        assert!(!a.is_empty());
        assert!(!b.is_empty());
        assert!((a.last().unwrap().x - 40.0).abs() < 1e-6);
        assert!((b[0].x - 60.0).abs() < 1e-6);
    }

    #[test]
    fn clip_to_cluster_border_trims_interior_points() {
        let cluster = Bounds {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let pts = vec![Point { x: -10.0, y: 50.0 }, Point { x: 50.0, y: 50.0 }];
        let clipped = clip_to_cluster_border(&pts, &cluster);
        assert_eq!(clipped.len(), 2);
        assert!((clipped[0].x + 10.0).abs() < 1e-9);
        assert!((clipped[1].x - 0.0).abs() < 1e-9);
    }

    #[test]
    fn clip_to_cluster_border_treats_boundary_point_as_outside() {
        let cluster = Bounds {
            x: 157.970703125,
            y: 115.296875,
            width: 119.9609375,
            height: 82.296875,
        };
        let pts = vec![
            Point {
                x: 89.75970740578128,
                y: 65.296875,
            },
            Point {
                x: 107.98046875,
                y: 90.296875,
            },
            Point {
                x: 107.98046875,
                y: 115.296875,
            },
            Point {
                x: 192.970703125,
                y: 147.09821160975935,
            },
        ];
        let clipped = clip_to_cluster_border(&pts, &cluster);
        let last = clipped.last().copied().expect("cluster hit");
        assert!((last.x - 157.970703125).abs() < 1e-9);
        assert!((last.y - 128.39304763120504).abs() < 1e-9);
    }

    #[test]
    fn route_edge_self_loop_produces_five_points() {
        let node = demo_node();
        let mut edge = Edge::default();
        edge.id = "e1".into();
        edge.source = Some("n".into());
        edge.target = Some("n".into());
        route_edge(&mut edge, &node, &node);
        let pts = edge.points.as_ref().expect("self-loop points");
        assert_eq!(pts.len(), 5);
        assert!(edge.label_x.is_some());
        assert!(edge.label_y.is_some());
    }
}
