//! Flowchart layout — converts a `FlowchartDiagram` AST into a
//! `LayoutData` envelope, hands it to the dagre bridge, and packages
//! the result (nodes + edges + clusters + bounds) into a
//! `FlowchartLayout` struct the renderer can consume.
//!
//! Upstream reference:
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/flowchart/flowRenderer-v3-unified.ts`
//! — which calls `getData()` to build a `data4Layout`, runs
//! `layoutRenderer.render()`, and yields nodes/edges with coordinates.

use crate::error::Result;
use crate::font_metrics;
use crate::layout::unified::{self, Bounds, LayoutData, LayoutResult};
use crate::model::flowchart::{
    ArrowType, ClassDef, Edge as ModelEdge, EdgeStroke, FlowchartDiagram, Label, LabelKind,
    LinkStyle, Vertex,
};
use crate::theme::ThemeVariables;
use std::collections::BTreeMap;

/// Post-layout result.
#[derive(Debug, Clone, Default)]
pub struct FlowchartLayout {
    /// Post-layout nodes (unified::Node).
    pub nodes: Vec<unified::Node>,
    /// Post-layout edges (unified::Edge).
    pub edges: Vec<unified::Edge>,
    /// Post-layout cluster bounds.
    pub clusters: Vec<unified::Cluster>,
    /// Tight AABB over the graph.
    pub bounds: Bounds,
    /// Padding applied around the bounds for the viewBox.
    pub diagram_padding: f64,
    /// `aria-roledescription` — derived from the header keyword:
    /// `flowchart-elk`, `flowchart-v2`, or `flowchart-v1`.
    pub aria_kind: String,
    /// IDs of clusters that were laid out via the recursive inner-layout
    /// algorithm (isolated clusters — no cross-boundary edges).
    /// These are rendered as inner `<g class="root">` groups inside
    /// the outer `<g class="nodes">` section, not in `<g class="clusters">`.
    pub isolated_cluster_ids: std::collections::HashSet<String>,
}

/// Font sizing defaults (upstream `flowchart.nodePadding=8, ranksep=50, nodesep=50`).
const NODE_PADDING_X: f64 = 8.0;
const NODE_PADDING_Y: f64 = 8.0;
const DEFAULT_FONT_FAMILY: &str = "trebuchet ms,verdana,arial,sans-serif";
/// Upstream's `labelHelper` uses `div.getBoundingClientRect()` on the
/// foreignObject HTML label, which inherits 14 px sans-serif from the
/// SVG root — NOT the theme fontSize (16 px). Using 14 px here makes
/// dagre assign the same node dimensions as upstream.
const LABEL_FONT_SIZE: f64 = 14.0;
/// Upstream `config.flowchart?.padding` default (from config.schema.yaml).
/// Used by shape functions to compute the total node size around the
/// label bounding box:
/// - rect (squareRect): labelPaddingX = padding * 2, labelPaddingY = padding
/// - round (roundedRect): labelPaddingX = padding, labelPaddingY = padding
/// - diamond: s = (labelW + padding) + (labelH + padding)
const FLOWCHART_PADDING: f64 = 15.0;

/// Lay out a flowchart diagram. Uses dagre for the graph geometry.
pub fn layout(d: &FlowchartDiagram, theme: &ThemeVariables) -> Result<FlowchartLayout> {
    let layout_data = build_layout_data(d);
    let LayoutResult {
        nodes,
        edges,
        clusters,
        bounds,
        isolated_cluster_ids,
    } = unified::layout(&layout_data, "dagre", theme)?;

    // Dagre's `assign_node_intersects` always uses `intersect_rect`, which
    // produces a point on the node's axis-aligned bounding box. Upstream
    // mermaid instead calls each shape's `intersect()` callback — for the
    // diamond/question shape and trapezoid/lean shapes this is
    // `intersectPolygon` against the actual polygon vertices. Recompute the
    // entry/exit point for those endpoints here so the rendered path matches
    // upstream byte-for-byte.
    let mut edges = edges;
    fix_polygon_edge_endpoints(&mut edges, &nodes);
    // Circle / ellipse / doublecircle nodes: dagre's `intersect_rect` clips
    // to the AABB; upstream calls `intersect.circle` / `intersect.ellipse`
    // which clip to the actual circular/elliptical boundary. Recompute
    // those endpoints here.
    fix_ellipse_edge_endpoints(&mut edges, &nodes);
    // Fallback: when an edge whose both endpoints sit inside the same
    // (isolated) cluster ends up without dagre-computed spline points,
    // synthesize a 3-point straight-line path from src boundary, midpoint,
    // dst boundary.  Upstream emits these short intra-cluster edges via
    // a basis spline through exactly those three points, so the renderer
    // can rebuild the byte-exact `d=` once we provide the raw waypoints.
    synthesize_missing_intra_cluster_edge_points(&mut edges, &nodes);

    Ok(FlowchartLayout {
        nodes,
        edges,
        clusters,
        bounds,
        diagram_padding: 8.0,
        // Upstream always uses "flowchart-v2" for the aria-roledescription,
        // even for diagrams that start with the `graph` keyword. Only
        // flowchart-elk gets its own label.
        aria_kind: if d.header_keyword == "flowchart-elk" {
            "flowchart-elk".to_string()
        } else {
            "flowchart-v2".to_string()
        },
        isolated_cluster_ids,
    })
}

/// Polygon shape descriptor used to recompute edge endpoints.
///
/// `vertices` are the polygon vertices in ABSOLUTE coordinates (already
/// translated by upstream's `left/top` shift inside `intersectPolygon`).
/// `adjustment` is subtracted from the resulting intersection point —
/// only `diamond/question` shapes apply the upstream `-0.5,-0.5` nudge
/// (see `question.ts::calcIntersect`). Other polygon shapes (trapezoid,
/// inv_trapezoid, lean_left, lean_right) feed the raw intersection back
/// to dagre's edge points.
struct PolygonInfo {
    vertices: Vec<(f64, f64)>,
    cx: f64,
    cy: f64,
    adjustment: (f64, f64),
}

/// Replace the first/last edge waypoint with the polygon intersection for
/// nodes whose render-time `intersect` callback uses `intersect.polygon`
/// (diamond/question, trapezoid, inverted-trapezoid, lean_left, lean_right).
///
/// dagre-rs only ever calls `intersect_rect`, so without this fix the path
/// endpoint sits on the node's axis-aligned bounding box rather than the
/// actual polygon boundary — diverging from mermaid.js by up to half the
/// shape's shear (≈ h/2 px for trapezoid / lean shapes).
///
/// Mirrors upstream's `insertEdge()` in
/// `rendering-util/rendering-elements/edges.js`:
///   points = points.slice(1, -1);
///   points.unshift(tail.intersect(points[0]));
///   points.push(head.intersect(points[points.length - 1]));
/// where `head/tail.intersect` invokes the per-shape callback set up in
/// each shape's renderer.
fn fix_polygon_edge_endpoints(edges: &mut [unified::Edge], nodes: &[unified::Node]) {
    use crate::layout::unified::types::Point;

    // Build per-node polygon descriptors keyed by node id.
    let mut info_map: BTreeMap<&str, PolygonInfo> = BTreeMap::new();
    for n in nodes {
        let shape = match n.shape.as_deref() {
            Some(s) => s,
            None => continue,
        };
        let cx = n.x.unwrap_or(0.0);
        let cy = n.y.unwrap_or(0.0);
        let w = n.width.unwrap_or(0.0);
        let h = n.height.unwrap_or(0.0);
        let info = match shape {
            "diamond" | "question" => {
                // Layout stores width = height = s (s = w_inner + h_inner).
                let s = w;
                let half = s / 2.0;
                PolygonInfo {
                    vertices: vec![
                        (cx, cy + half),
                        (cx + half, cy),
                        (cx, cy - half),
                        (cx - half, cy),
                    ],
                    cx,
                    cy,
                    // Upstream `question.ts::calcIntersect` subtracts (0.5, 0.5)
                    // from the raw polygon intersection ("Adjusted result").
                    adjustment: (-0.5, -0.5),
                }
            }
            // Trapezoid family — upstream stores `node.width = visual width`
            // (= w_inner + h) after `updateNodeBounds`. Recover w_inner so
            // we can place the polygon vertices in the same coordinate frame
            // upstream's `intersectPolygon` uses (`left/top` shift derived
            // from `node.width/height` and the polygon's minX/minY).
            "trapezoid" | "trap" => {
                let h_in = h;
                let w_in = (w - h_in).max(0.0);
                // Upstream points (local): [(-h/2, 0), (w+h/2, 0), (w, -h), (0, -h)]
                // minX = -h/2, minY = -h
                // left = cx - w_visual/2 - minX = cx - (w_in + h)/2 + h/2 = cx - w_in/2
                // top  = cy - h/2 - (-h) = cy + h/2
                let left = cx - w_in / 2.0;
                let top = cy + h_in / 2.0;
                PolygonInfo {
                    vertices: vec![
                        (left + (-h_in / 2.0), top + 0.0),
                        (left + (w_in + h_in / 2.0), top + 0.0),
                        (left + w_in, top + (-h_in)),
                        (left + 0.0, top + (-h_in)),
                    ],
                    cx,
                    cy,
                    adjustment: (0.0, 0.0),
                }
            }
            "inv_trapezoid" | "invertedTrapezoid" => {
                let h_in = h;
                let w_in = (w - h_in).max(0.0);
                // Upstream points (local): [(0, 0), (w, 0), (w+h/2, -h), (-h/2, -h)]
                // minX = -h/2, minY = -h
                // left = cx - w_visual/2 - minX = cx - (w_in+h)/2 + h/2 = cx - w_in/2
                // top  = cy - h/2 - (-h) = cy + h/2
                let left = cx - w_in / 2.0;
                let top = cy + h_in / 2.0;
                PolygonInfo {
                    vertices: vec![
                        (left + 0.0, top + 0.0),
                        (left + w_in, top + 0.0),
                        (left + (w_in + h_in / 2.0), top + (-h_in)),
                        (left + (-h_in / 2.0), top + (-h_in)),
                    ],
                    cx,
                    cy,
                    adjustment: (0.0, 0.0),
                }
            }
            "lean_right" | "lean-right" => {
                let h_in = h;
                let w_in = (w - h_in).max(0.0);
                // Upstream points (local): [(-h/2, 0), (w, 0), (w+h/2, -h), (0, -h)]
                let left = cx - w_in / 2.0;
                let top = cy + h_in / 2.0;
                PolygonInfo {
                    vertices: vec![
                        (left + (-h_in / 2.0), top + 0.0),
                        (left + w_in, top + 0.0),
                        (left + (w_in + h_in / 2.0), top + (-h_in)),
                        (left + 0.0, top + (-h_in)),
                    ],
                    cx,
                    cy,
                    adjustment: (0.0, 0.0),
                }
            }
            "lean_left" | "lean-left" => {
                let h_in = h;
                let w_in = (w - h_in).max(0.0);
                // Upstream points (local): [(0, 0), (w+h/2, 0), (w, -h), (-h/2, -h)]
                let left = cx - w_in / 2.0;
                let top = cy + h_in / 2.0;
                PolygonInfo {
                    vertices: vec![
                        (left + 0.0, top + 0.0),
                        (left + (w_in + h_in / 2.0), top + 0.0),
                        (left + w_in, top + (-h_in)),
                        (left + (-h_in / 2.0), top + (-h_in)),
                    ],
                    cx,
                    cy,
                    adjustment: (0.0, 0.0),
                }
            }
            // Upstream stadium.ts builds a 102-point polygon:
            //   [{-w/2 + r, -h/2}, {w/2 - r, -h/2},
            //    ...generateCirclePoints(-w/2 + r, 0, r, 50, 90, 270),
            //    {w/2 - r, h/2},
            //    ...generateCirclePoints( w/2 - r, 0, r, 50, 270, 450)]
            // and feeds those to intersect.polygon. Without this we
            // fall back to dagre's AABB rectangle clip which sits a
            // pixel or two inside the rounded end-caps.
            //
            // NB: `node.width` here is the polygon-bbox-corrected dagre width
            // (see `measure_vertex_box`). The polygon must be sized at the
            // analytical width `w + 2*radius*(1 - cos(pi/(2*49)))` so the
            // sample points lie on the same circle the upstream code uses.
            "stadium" | "pill" => {
                let radius = h / 2.0;
                let n = 50usize;
                let half_step = std::f64::consts::PI / (2.0 * (n as f64 - 1.0));
                let correction = 2.0 * radius * (1.0 - half_step.cos());
                let w_analytical = w + correction;
                let mut verts: Vec<(f64, f64)> = Vec::with_capacity(102);
                verts.push((cx + (-w_analytical / 2.0 + radius), cy + (-h / 2.0)));
                verts.push((cx + (w_analytical / 2.0 - radius), cy + (-h / 2.0)));
                let arc1_cx = -w_analytical / 2.0 + radius;
                let start1 = std::f64::consts::PI / 2.0;
                let end1 = std::f64::consts::PI * 3.0 / 2.0;
                let step1 = (end1 - start1) / (n as f64 - 1.0);
                for i in 0..n {
                    let angle = start1 + i as f64 * step1;
                    let xr = arc1_cx + radius * angle.cos();
                    let yr = radius * angle.sin();
                    verts.push((cx + (-xr), cy + (-yr)));
                }
                verts.push((cx + (w_analytical / 2.0 - radius), cy + (h / 2.0)));
                let arc2_cx = w_analytical / 2.0 - radius;
                let start2 = std::f64::consts::PI * 3.0 / 2.0;
                let end2 = std::f64::consts::PI * 5.0 / 2.0;
                let step2 = (end2 - start2) / (n as f64 - 1.0);
                for i in 0..n {
                    let angle = start2 + i as f64 * step2;
                    let xr = arc2_cx + radius * angle.cos();
                    let yr = radius * angle.sin();
                    verts.push((cx + (-xr), cy + (-yr)));
                }
                PolygonInfo {
                    vertices: verts,
                    cx,
                    cy,
                    adjustment: (0.0, 0.0),
                }
            }
            // Upstream rectLeftInvArrow.ts (asymmetric / `>text]` syntax):
            // raw local points (line 45043-45049):
            //   { x: x + notch, y },  { x, y: 0 },  { x + notch, -y },
            //   { -x, -y },           { -x, y }
            // where x = -w_inner/2, y = -h/2, notch = y/2 = -h/4.
            // The polygon is then translated by `(-notch/2, 0) = (h/8, 0)`
            // visually, but the `points` array fed to `intersect_default.polygon`
            // is the UNTRANSLATED list.
            //
            // We don't have `w_inner` directly here — `n.width` is the
            // visual width set by `updateNodeBounds(node, polygon)` which
            // reads the **post-translate** bbox: visual = w_inner + h/4.
            // So recover `w_inner = n.width - h/4` and emit the polygon
            // in the same world frame upstream's `intersectPolygon`
            // produces (after its `left/top` shift).
            //
            // Effect: end-cap world coords:
            //   notch tip:    (cx - w_inner/2 - h/8, cy ± h/2)
            //   body left:    (cx - w_inner/2 + h/8, cy)
            //   body right:   (cx + w_inner/2 + h/8, cy ± h/2)
            "rect_left_inv_arrow" | "odd" | "asymmetric" => {
                let w_inner = (w - h / 4.0).max(0.0);
                let half_w = w_inner / 2.0;
                let half_h = h / 2.0;
                let dx = h / 8.0;
                PolygonInfo {
                    vertices: vec![
                        (cx - half_w - dx, cy - half_h),
                        (cx - half_w + dx, cy),
                        (cx - half_w - dx, cy + half_h),
                        (cx + half_w + dx, cy + half_h),
                        (cx + half_w + dx, cy - half_h),
                    ],
                    cx,
                    cy,
                    adjustment: (0.0, 0.0),
                }
            }
            // Upstream hexagon.ts: m = h/f (f=4 for default look), w_total = w + 2m
            // local points: [(m, 0), (w-m, 0), (w, -h/2), (w-m, -h), (m, -h), (0, -h/2)]
            // intersectPolygon: minX=0, minY=-h → left = cx - w/2, top = cy + h/2.
            // Default look only (look=neo uses f=3.5, but neo isn't on the byte-exact path).
            "hexagon" | "hex" => {
                let m = h / 4.0;
                PolygonInfo {
                    vertices: vec![
                        (cx - w / 2.0 + m, cy + h / 2.0),
                        (cx + w / 2.0 - m, cy + h / 2.0),
                        (cx + w / 2.0, cy),
                        (cx + w / 2.0 - m, cy - h / 2.0),
                        (cx - w / 2.0 + m, cy - h / 2.0),
                        (cx - w / 2.0, cy),
                    ],
                    cx,
                    cy,
                    adjustment: (0.0, 0.0),
                }
            }
            _ => continue,
        };
        info_map.insert(n.id.as_str(), info);
    }
    if info_map.is_empty() {
        return;
    }

    for e in edges.iter_mut() {
        let Some(points) = e.points.as_mut() else {
            continue;
        };
        if points.len() < 2 {
            continue;
        }
        // Start endpoint (anchor follows `start` field which may have been
        // retargeted from a cluster — the actual leaf node is what matters).
        if let Some(start_id) = e.start.as_deref() {
            if let Some(info) = info_map.get(start_id) {
                let next = points[1];
                if let Some(p) =
                    polygon_intersection((info.cx, info.cy), (next.x, next.y), &info.vertices)
                {
                    points[0] = Point {
                        x: p.0 + info.adjustment.0,
                        y: p.1 + info.adjustment.1,
                    };
                }
            }
        }
        if let Some(end_id) = e.end.as_deref() {
            if let Some(info) = info_map.get(end_id) {
                let n = points.len();
                let prev = points[n - 2];
                if let Some(p) =
                    polygon_intersection((info.cx, info.cy), (prev.x, prev.y), &info.vertices)
                {
                    points[n - 1] = Point {
                        x: p.0 + info.adjustment.0,
                        y: p.1 + info.adjustment.1,
                    };
                }
            }
        }
    }
}

/// Replace the first/last edge waypoint with the ellipse intersection for
/// circular / elliptical nodes (circle, ellipse, doublecircle).
///
/// Upstream mermaid calls `intersect.circle(node, r, point)` /
/// `intersect.ellipse(node, rx, ry, point)` which clip the ray from the
/// node centre to the ellipse boundary. dagre-rs only ever clips to the
/// AABB, which on a circle of radius `r` produces a point at distance
/// `r·max(|dx|,|dy|)/min(|dx|,|dy|)` from the centre instead of `r` — a
/// noticeable divergence whenever an edge lands at a non-axis-aligned
/// angle.
///
/// Mirrors upstream `intersect-ellipse.js` exactly:
///     px = cx - point.x
///     py = cy - point.y
///     det = sqrt(rx² · py² + ry² · px²)
///     dx  = |rx · ry · px / det|, sign = sign(point.x - cx)
///     dy  = |rx · ry · py / det|, sign = sign(point.y - cy)
///     return (cx + dx, cy + dy)
fn fix_ellipse_edge_endpoints(edges: &mut [unified::Edge], nodes: &[unified::Node]) {
    use crate::layout::unified::types::Point;

    enum ShapeInfo {
        Ellipse { cx: f64, cy: f64, rx: f64, ry: f64 },
        Cylinder { cx: f64, cy: f64, w: f64, h: f64 },
    }

    let mut info_map: BTreeMap<&str, ShapeInfo> = BTreeMap::new();
    for n in nodes {
        let shape = match n.shape.as_deref() {
            Some(s) => s,
            None => continue,
        };
        let (cx, cy, w, h) = (
            n.x.unwrap_or(0.0),
            n.y.unwrap_or(0.0),
            n.width.unwrap_or(0.0),
            n.height.unwrap_or(0.0),
        );
        match shape {
            "circle" | "circ" | "doublecircle" | "ellipse" => {
                info_map.insert(
                    n.id.as_str(),
                    ShapeInfo::Ellipse {
                        cx,
                        cy,
                        rx: w / 2.0,
                        ry: h / 2.0,
                    },
                );
            }
            "cylinder" | "cyl" => {
                info_map.insert(n.id.as_str(), ShapeInfo::Cylinder { cx, cy, w, h });
            }
            _ => {}
        }
    }
    if info_map.is_empty() {
        return;
    }

    let intersect_ellipse = |cx: f64, cy: f64, rx: f64, ry: f64, target: Point| -> Point {
        let px = cx - target.x;
        let py = cy - target.y;
        let det = (rx * rx * py * py + ry * ry * px * px).sqrt();
        if det == 0.0 {
            return Point { x: cx, y: cy };
        }
        let mut dx = (rx * ry * px / det).abs();
        if target.x < cx {
            dx = -dx;
        }
        let mut dy = (rx * ry * py / det).abs();
        if target.y < cy {
            dy = -dy;
        }
        Point {
            x: cx + dx,
            y: cy + dy,
        }
    };

    // Upstream cylinder.ts intersect: rect AABB → adjust y by elliptical cap.
    let intersect_cylinder = |cx: f64, cy: f64, w: f64, h: f64, target: Point| -> Point {
        let dx = target.x - cx;
        let dy = target.y - cy;
        if dx == 0.0 && dy == 0.0 {
            return Point { x: cx, y: cy };
        }
        let half_w = w / 2.0;
        let half_h = h / 2.0;
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
        let mut pos = Point {
            x: cx + dx * t,
            y: cy + dy * t,
        };
        let rx = w / 2.0;
        if rx != 0.0 {
            let ry_arc = rx / (2.5 + w / 50.0);
            let local_x = pos.x - cx;
            let on_cap = local_x.abs() < half_w
                || (local_x.abs() == half_w && (pos.y - cy).abs() > half_h - ry_arc);
            if on_cap {
                let mut y = ry_arc * ry_arc * (1.0 - local_x * local_x / (rx * rx));
                if y > 0.0 {
                    y = y.sqrt();
                }
                let mut delta = ry_arc - y;
                if target.y - cy > 0.0 {
                    delta = -delta;
                }
                pos.y += delta;
            }
        }
        pos
    };

    let intersect_for = |info: &ShapeInfo, target: Point| -> Point {
        match info {
            ShapeInfo::Ellipse { cx, cy, rx, ry } => intersect_ellipse(*cx, *cy, *rx, *ry, target),
            ShapeInfo::Cylinder { cx, cy, w, h } => intersect_cylinder(*cx, *cy, *w, *h, target),
        }
    };

    for e in edges.iter_mut() {
        let Some(points) = e.points.as_mut() else {
            continue;
        };
        if points.len() < 2 {
            continue;
        }
        if let Some(start_id) = e.start.as_deref() {
            if let Some(info) = info_map.get(start_id) {
                let next = points[1];
                points[0] = intersect_for(info, next);
            }
        }
        if let Some(end_id) = e.end.as_deref() {
            if let Some(info) = info_map.get(end_id) {
                let n = points.len();
                let prev = points[n - 2];
                points[n - 1] = intersect_for(info, prev);
            }
        }
    }
}

/// Mirror of upstream `intersectPolygon(node, polyPoints, point)` from
/// `rendering-util/rendering-elements/intersect/intersect-polygon.js`.
///
/// Returns the polygon-edge intersection nearest to `target`, or `None` when
/// no segment intersects the line `(center → target)`. The polygon points
/// are already in absolute coordinates (caller did the `left/top` shift).
///
/// Implementation faithfully reproduces upstream's segment-segment test
/// (`intersect-line.js`) including the +/- offset rounding trick on the
/// numerator, which materially affects the last bit of the f64 result and
/// is required for byte-exact `data-points` parity.
fn polygon_intersection(
    center: (f64, f64),
    target: (f64, f64),
    poly: &[(f64, f64)],
) -> Option<(f64, f64)> {
    let mut hits: Vec<(f64, f64)> = Vec::with_capacity(poly.len());
    for i in 0..poly.len() {
        let p1 = poly[i];
        let p2 = poly[(i + 1) % poly.len()];
        if let Some(p) = intersect_line(center, target, p1, p2) {
            hits.push(p);
        }
    }
    if hits.is_empty() {
        return None;
    }
    if hits.len() > 1 {
        hits.sort_by(|a, b| {
            let da = (a.0 - target.0).powi(2) + (a.1 - target.1).powi(2);
            let db = (b.0 - target.0).powi(2) + (b.1 - target.1).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    Some(hits[0])
}

/// Mirror of upstream `intersectLine(p1, p2, q1, q2)`. Returns the
/// intersection of two line *segments* or `None` if they don't intersect.
fn intersect_line(
    p1: (f64, f64),
    p2: (f64, f64),
    q1: (f64, f64),
    q2: (f64, f64),
) -> Option<(f64, f64)> {
    let a1 = p2.1 - p1.1;
    let b1 = p1.0 - p2.0;
    let c1 = p2.0 * p1.1 - p1.0 * p2.1;
    let r3 = a1 * q1.0 + b1 * q1.1 + c1;
    let r4 = a1 * q2.0 + b1 * q2.1 + c1;
    if r3 != 0.0 && r4 != 0.0 && r3 * r4 > 0.0 {
        return None;
    }
    let a2 = q2.1 - q1.1;
    let b2 = q1.0 - q2.0;
    let c2 = q2.0 * q1.1 - q1.0 * q2.1;
    let r1 = a2 * p1.0 + b2 * p1.1 + c2;
    let r2 = a2 * p2.0 + b2 * p2.1 + c2;
    let epsilon = 1e-6_f64;
    if r1.abs() > epsilon && r2.abs() > epsilon && r1 * r2 > 0.0 {
        return None;
    }
    let denom = a1 * b2 - a2 * b1;
    if denom == 0.0 {
        return None;
    }
    let offset = (denom / 2.0).abs();
    let num_x = b1 * c2 - b2 * c1;
    let x = if num_x < 0.0 {
        (num_x - offset) / denom
    } else {
        (num_x + offset) / denom
    };
    let num_y = a2 * c1 - a1 * c2;
    let y = if num_y < 0.0 {
        (num_y - offset) / denom
    } else {
        (num_y + offset) / denom
    };
    Some((x, y))
}

/// Synthesize a 3-point spline (src boundary → midpoint → dst boundary) for
/// edges whose dagre-bridge output left `points = None`.  This happens when
/// both endpoints sit inside an isolated cluster whose inner-graph dagre
/// pass does not surface the spline (e.g. the simple `subgraph S; a-->b; end`
/// case).  Without these waypoints the renderer skips the edge `<path>`,
/// breaking byte-exactness.
///
/// We reconstruct the same three points dagre would have placed:
///   - src boundary along the line (src_center → dst_center)
///   - midpoint of the two centres
///   - dst boundary along the line (dst_center → src_center)
///
/// For axis-aligned pairs (same x or same y, the dominant intra-cluster
/// case) this matches upstream byte-for-byte; for diagonal pairs the
/// fallback still produces a valid renderable path even if not byte-exact.
fn synthesize_missing_intra_cluster_edge_points(
    edges: &mut [unified::Edge],
    nodes: &[unified::Node],
) {
    use crate::layout::unified::types::Point;
    // node-id → (cx, cy, w, h, parent)
    let mut info: BTreeMap<&str, (f64, f64, f64, f64, Option<&str>)> = BTreeMap::new();
    for n in nodes {
        if n.is_group {
            continue;
        }
        let (Some(cx), Some(cy), Some(w), Some(h)) = (n.x, n.y, n.width, n.height) else {
            continue;
        };
        info.insert(n.id.as_str(), (cx, cy, w, h, n.parent_id.as_deref()));
    }

    for e in edges.iter_mut() {
        if e.points.is_some() {
            continue;
        }
        let (Some(s_id), Some(t_id)) = (e.start.as_deref(), e.end.as_deref()) else {
            continue;
        };
        let Some(&(sx, sy, sw, sh, sp)) = info.get(s_id) else {
            continue;
        };
        let Some(&(tx, ty, tw, th, tp)) = info.get(t_id) else {
            continue;
        };
        // Only act when both endpoints share a parent cluster — these are
        // the cases the inner-cluster dagre pass occasionally leaves
        // routeless.  Edges with leaf-leaf pairs at the root will already
        // have spline points from the outer dagre pass.
        if sp.is_none() || sp != tp {
            continue;
        }
        // Compute boundary points using axis-aligned rectangle intersection
        // along the centre-to-centre line. Mirrors dagre's `intersectRect`
        // with center=(cx,cy) and target=(other_cx, other_cy).
        let s_pt = intersect_rect((sx, sy, sw, sh), (tx, ty));
        let t_pt = intersect_rect((tx, ty, tw, th), (sx, sy));
        let mid = ((sx + tx) / 2.0, (sy + ty) / 2.0);
        e.points = Some(vec![
            Point {
                x: s_pt.0,
                y: s_pt.1,
            },
            Point { x: mid.0, y: mid.1 },
            Point {
                x: t_pt.0,
                y: t_pt.1,
            },
        ]);
        log::debug!(
            "flowchart layout: synthesized 3-point spline for intra-cluster edge {} ({} → {})",
            e.id,
            s_id,
            t_id
        );
    }
}

/// Mirror of dagre's `intersectRect`: clip the line from `target` to the
/// rectangle centre at the rectangle border.  `(cx, cy, w, h)` is the
/// rectangle (centre + size); `(tx, ty)` is the external target point.
fn intersect_rect(rect: (f64, f64, f64, f64), target: (f64, f64)) -> (f64, f64) {
    let (cx, cy, w, h) = rect;
    let dx = target.0 - cx;
    let dy = target.1 - cy;
    if dx == 0.0 && dy == 0.0 {
        return (cx, cy);
    }
    let half_w = w / 2.0;
    let half_h = h / 2.0;
    // Same algorithm as dagre's util.intersectRect.
    let (sx, sy);
    if dy.abs() * half_w > dx.abs() * half_h {
        // intersection is on top/bottom edge
        let s = if dy < 0.0 { -half_h } else { half_h };
        sx = s * dx / dy;
        sy = s;
    } else {
        // intersection is on left/right edge
        let s = if dx < 0.0 { -half_w } else { half_w };
        sx = s;
        sy = s * dy / dx;
    }
    (cx + sx, cy + sy)
}

/// Build a unified `LayoutData` from a flowchart AST.
fn build_layout_data(d: &FlowchartDiagram) -> LayoutData {
    let mut data = LayoutData::default();
    data.diagram_type = Some("flowchart-v2".into());
    data.direction = Some(d.direction.as_str().into());
    data.node_spacing = Some(d.node_spacing.map(f64::from).unwrap_or(50.0));
    data.rank_spacing = Some(d.rank_spacing.map(f64::from).unwrap_or(50.0));
    data.layout_algorithm = Some("dagre".into());

    // Class-def lookup for inline CSS.
    let class_map: BTreeMap<&str, &ClassDef> =
        d.class_defs.iter().map(|c| (c.name.as_str(), c)).collect();

    // Build a parent-id map from subgraph membership.
    //
    // Subtlety: a vertex may appear in multiple subgraphs' membership
    // (e.g. declared in `subcontainer`, then re-referenced inside the
    // outer `main` via an edge). Upstream's flowDb assigns the parent
    // based on the deepest enclosing subgraph at the time the vertex
    // was actually FIRST declared. We approximate that by preferring
    // the deepest subgraph (max depth from root) when a vertex is
    // claimed by more than one — this matches upstream for nested cases
    // like fixture 136 (subcontainer-child belongs to `subcontainer`,
    // not the outer `main`) while leaving flat cases alone.
    let depth_of: BTreeMap<&str, usize> = {
        // Compute depth via parent links inferred from `children`.
        let mut parent_link: BTreeMap<&str, &str> = BTreeMap::new();
        for sg in &d.subgraphs {
            for ch in &sg.children {
                parent_link.insert(ch.as_str(), sg.id.as_str());
            }
        }
        let mut depth: BTreeMap<&str, usize> = BTreeMap::new();
        for sg in &d.subgraphs {
            let mut d_count = 0usize;
            let mut cur = sg.id.as_str();
            while let Some(&p) = parent_link.get(cur) {
                d_count += 1;
                cur = p;
                if d_count > 64 {
                    break; // safety
                }
            }
            depth.insert(sg.id.as_str(), d_count);
        }
        depth
    };
    let mut parent_of: BTreeMap<String, String> = BTreeMap::new();
    let upsert_deeper = |map: &mut BTreeMap<String, String>, key: &str, sg_id: &str| {
        let cand_depth = depth_of.get(sg_id).copied().unwrap_or(0);
        match map.get(key) {
            None => {
                map.insert(key.to_string(), sg_id.to_string());
            }
            Some(prev) => {
                let prev_depth = depth_of.get(prev.as_str()).copied().unwrap_or(0);
                if cand_depth > prev_depth {
                    map.insert(key.to_string(), sg_id.to_string());
                }
            }
        }
    };
    for sg in &d.subgraphs {
        for child in &sg.children {
            // children link is unambiguous — parent must be `sg`.
            parent_of.insert(child.clone(), sg.id.clone());
        }
        for m in &sg.members {
            upsert_deeper(&mut parent_of, m, &sg.id);
        }
    }

    // Set of subgraph IDs — used to skip vertices that are actually subgraph
    // references (e.g. `B` inside `subgraph A` when `B` is itself a subgraph).
    let subgraph_ids: std::collections::HashSet<&str> =
        d.subgraphs.iter().map(|sg| sg.id.as_str()).collect();

    // ── Subgraph cluster nodes first ─────────────────────────────────────
    //
    // Upstream `flowDb.getData()` pushes subgraph cluster nodes BEFORE
    // leaf-vertex nodes, iterating in REVERSE declaration order
    // (`for i = subGraphs.length - 1; i >= 0; i--`). This insertion order
    // is observable downstream because dagre's DFS-based feedback-arc-set
    // (`acyclic.dfsFAS`) walks `graph.nodes()` in insertion order, so
    // mirror it byte-for-byte to keep cycle resolution identical to the
    // reference (e.g. dual-cluster fixtures 151/152/153/154 where
    // edge `cluster_a → cluster_b` plus `cluster_a → leaf_inside_b`
    // forms a 2-cycle and the chosen feedback edge depends on which
    // node DFS visits first).
    for sg in d.subgraphs.iter().rev() {
        let (w, h) = measure_subgraph_title_box(sg.title.as_ref());
        let mut node = unified::Node::default();
        node.id = sg.id.clone();
        // Upstream cluster DOM id is just the subgraph id — no "flowchart-" prefix.
        // render_cluster prepends the SVG element id when emitting.
        node.dom_id = Some(sg.id.clone());
        node.label = sg.title.as_ref().map(|l| l.text.clone());
        node.label_type = sg.title.as_ref().map(|l| {
            use crate::model::flowchart::LabelKind;
            match l.kind {
                LabelKind::Markdown => "markdown",
                LabelKind::String => "string",
                LabelKind::Text => "text",
            }
            .to_string()
        });
        node.shape = Some("rect".into());
        node.width = Some(w);
        node.height = Some(h);
        node.padding = Some(8.0);
        node.is_group = true;
        node.look = Some("classic".into());
        // Per-cluster direction. Upstream `mermaid-graphlib`'s `extractor.ts`
        // (line 339) flips inner rankdir as `outer === 'TB' ? 'LR' : 'TB'`,
        // so any non-TB outer (LR/BT/RL) yields a TB inner pass. Our
        // `dagre_bridge::opposite_rankdir` does a 4-way symmetric flip
        // (RL → BT, BT → RL) which produces correctly placed nodes but
        // reverses the vertical edge point order for inner-pass edges in
        // BT/RL outer diagrams (cypress fixture 159). Force the inner
        // cluster to TB whenever the user didn't request an explicit
        // `direction` line, matching upstream byte-for-byte.
        node.dir = sg.dir.map(|d| d.as_str().to_string()).or_else(|| {
            if d.inherit_dir {
                Some(d.direction.as_str().to_string())
            } else if d.direction.as_str() != "TB" && d.direction.as_str() != "TD" {
                Some("TB".to_string())
            } else {
                None
            }
        });
        node.parent_id = parent_of.get(&sg.id).cloned();
        // Cluster CSS class: extra classes from `class <subgraph-id> <name>`
        // directives are appended to the rendered DOM as `class="cluster <names…>"`.
        // None / empty here causes the renderer to emit `class="cluster "`.
        if !sg.classes.is_empty() {
            node.css_classes = Some(sg.classes.join(" "));
        } else {
            node.css_classes = None;
        }
        // `style <subgraph-id> ...` directives land on the matching Vertex (if any)
        // because the parser calls `ensure_vertex` on the id. Apply those styles here.
        // Additionally, classes attached to the subgraph itself (via
        // `class <subgraph-id> <name>`) resolve to inline styles by walking
        // `classDef` entries in declaration order — last-wins per CSS key,
        // mirroring `collect_styles` semantics for vertices. Without this,
        // multiple `class <id> <a>` / `class <id> <b>` directives leak both
        // `fill:` declarations into the rendered `style` attribute.
        // See cypress fixture 143 (`class T Test`, `class T TestSub`).
        let synthetic = synthesize_vertex_for_subgraph(sg, d);
        let merged = collect_styles(&synthetic, &class_map);
        if !merged.is_empty() {
            node.css_styles = Some(merged);
        }
        data.nodes.push(node);
    }

    // Nodes: vertices.
    for v in &d.vertices {
        // Skip vertices whose ID matches a subgraph — they are cluster references,
        // not standalone nodes, and will be rendered as clusters.
        if subgraph_ids.contains(v.id.as_str()) {
            continue;
        }
        let shape_id = canon_shape(v.shape.as_deref().unwrap_or("rect"));
        // Resolve styles first so that `font-weight:bold` is reflected in
        // the label text-width measurement (matches upstream's
        // `getBoundingClientRect()` on the rendered foreignObject div).
        let merged_styles = collect_styles(v, &class_map);
        let is_bold = styles_have_bold(&merged_styles);
        let font_size_px = styles_font_size_px(&merged_styles);
        let (w, h) = measure_vertex_box(v, is_bold, font_size_px);
        let label_text = if shape_id == "icon" && v.label.is_none() {
            String::new()
        } else {
            display_label(v)
        };
        let mut node = unified::Node::default();
        node.id = v.id.clone();
        node.dom_id = Some(flowchart_dom_id(&v.id, v.order));
        node.label = Some(label_text.clone());
        node.label_type = Some(label_kind_string(v.label.as_ref()).to_string());
        node.shape = Some(shape_id.to_string());
        node.width = Some(w);
        node.height = Some(h);
        node.padding = Some(FLOWCHART_PADDING);
        if shape_id == "icon" {
            node.icon = v.shape_data.clone();
        }
        node.look = Some("classic".into());
        node.parent_id = parent_of.get(&v.id).cloned();
        // CSS classes — upstream: `'default ' + vertex.classes.join(' ')`.
        // `"default "` has a trailing space; when classes are appended via
        // join(' '), the result is `"default dark"` (no trailing space) for
        // one class, or `"default "` (trailing space) when the list is empty.
        // The shape renderer then formats `"node {cssClasses} "` which
        // produces `"node default  "` (double space) when no extra classes,
        // and `"node default dark "` (one trailing space) when "dark" is last.
        let classes = if v.classes.is_empty() {
            "default ".to_string()
        } else {
            format!("default {}", v.classes.join(" "))
        };
        node.css_classes = Some(classes);
        // Inline styles.
        if !merged_styles.is_empty() {
            node.css_styles = Some(merged_styles);
        }
        node.link = v.link.clone();
        node.link_target = v.link_target.clone();
        node.tooltip = v.tooltip.clone();
        if v.callback.is_some() {
            node.have_callback = Some(true);
        }
        // Rectangle radii (only set for `round`).
        if shape_id == "round" {
            node.rx = Some(5.0);
            node.ry = Some(5.0);
        }
        data.nodes.push(node);
    }

    // Edges. Retarget any edge that points at a subgraph id to the
    // first non-cluster descendant — dagre-rs panics when a compound
    // node is used as an edge endpoint. Upstream mermaid does the
    // equivalent remapping inside `mermaid-graphlib::findNonClusterChild`.
    // Upstream edge IDs use a per-pair counter (see `getEdgeId`):
    //   L_{start}_{end}_0 for the first edge between a pair,
    //   L_{start}_{end}_1 for the second, etc.
    use std::collections::HashMap;
    let mut pair_count: HashMap<(String, String), usize> = HashMap::new();
    // Two-pass insertion: edges with leaf endpoints first, then edges that
    // originally pointed at a cluster (and were retargeted to a leaf). This
    // matches upstream mermaid's traversal order where cluster-endpoint edges
    // are processed in a follow-up pass — and crucially it matches how dagre
    // resolves parallel-edge ordering between the same (src, dst) pair.
    let mut leaf_edges: Vec<unified::Edge> = Vec::new();
    let mut cluster_edges: Vec<unified::Edge> = Vec::new();
    for e in &d.edges {
        let start = e.start.clone();
        let end = e.end.clone();
        let raw = *pair_count
            .entry((start.clone(), end.clone()))
            .and_modify(|c| *c += 1)
            .or_insert(0);
        // Reproduce upstream `flowDb.addSingleLink` (mermaid v11 line 306-318):
        //   if existingLinks.length === 0 → counter = 0
        //   else                            → counter = existingLinks.length + 1
        // Concretely: 1st duplicate gets `_0`, 2nd gets `_2`, 3rd `_3`, etc.
        // Cypress/flowchart/159 (2 edges) → `_0`,`_2`; cypress/flowchart/55
        // (3 edges, elk-fallback) → `_0`,`_2`,`_3`.
        let counter = if raw == 0 { 0 } else { raw + 1 };
        let mut ue = build_edge(
            e,
            d,
            counter,
            &class_map,
            d.html_labels.unwrap_or(true),
            d.curve.as_deref().unwrap_or("basis"),
        );
        // Record original endpoints before retargeting so the isolation check
        // in dagre_bridge can test against the pre-retarget cluster IDs.
        ue.extra.insert("orig_start".into(), e.start.clone());
        ue.extra.insert("orig_end".into(), e.end.clone());
        let touched_cluster =
            d.find_subgraph(&e.start).is_some() || d.find_subgraph(&e.end).is_some();
        retarget_cluster_endpoints(&mut ue, d);
        if touched_cluster {
            cluster_edges.push(ue);
        } else {
            leaf_edges.push(ue);
        }
    }
    data.edges.extend(leaf_edges);
    data.edges.extend(cluster_edges);

    data
}

fn retarget_cluster_endpoints(ue: &mut unified::Edge, d: &FlowchartDiagram) {
    if let Some(sid) = ue.start.clone() {
        if d.find_subgraph(&sid).is_some() {
            if let Some(child) = first_non_cluster_descendant(&sid, d) {
                ue.start = Some(child);
            }
        }
    }
    if let Some(sid) = ue.end.clone() {
        if d.find_subgraph(&sid).is_some() {
            if let Some(child) = first_non_cluster_descendant(&sid, d) {
                ue.end = Some(child);
            }
        }
    }
}

fn first_non_cluster_descendant(sid: &str, d: &FlowchartDiagram) -> Option<String> {
    let sg = d.find_subgraph(sid)?;
    for m in &sg.members {
        // `members` only holds vertex ids (parser didn't add subgraphs
        // as members), but double-check.
        if d.find_vertex(m).is_some() {
            return Some(m.clone());
        }
    }
    for child in &sg.children {
        if let Some(x) = first_non_cluster_descendant(child, d) {
            return Some(x);
        }
    }
    None
}

/// Map upstream shape aliases to the shape registry's canonical ids.
fn canon_shape(s: &str) -> &'static str {
    match s {
        "square" | "rect" => "rect",
        "round" | "rounded" => "round",
        "stadium" | "pill" => "stadium",
        "subroutine" => "subroutine",
        "cylinder" | "cyl" => "cylinder",
        "circle" | "circ" => "circle",
        "doublecircle" => "doublecircle",
        "ellipse" => "ellipse",
        "diamond" | "question" | "diam" => "diamond",
        "hexagon" | "hex" => "hexagon",
        "lean_right" | "lean-right" => "lean_right",
        "lean_left" | "lean-left" => "lean_left",
        "trapezoid" | "trap" => "trapezoid",
        "inv_trapezoid" | "invertedTrapezoid" => "inv_trapezoid",
        "odd" => "rect_left_inv_arrow",
        "note" => "note",
        "icon" => "icon",
        _ => "rect",
    }
}

fn display_label(v: &Vertex) -> String {
    // Fall back to the id only when the source has no label record at all
    // (e.g. plain `A`). When the source explicitly supplied an empty / blank
    // label such as `A(( ))`, upstream `vertex.text` is the empty string and
    // the rendered foreignObject contains no visible text — falling back to
    // the id here inflates the node bbox and leaks the id into the SVG.
    match v.label.as_ref() {
        Some(l) => l.text.clone(),
        None => v.id.clone(),
    }
}

fn label_kind_string(l: Option<&Label>) -> &'static str {
    match l.map(|l| l.kind) {
        Some(LabelKind::Markdown) => "markdown",
        Some(LabelKind::String) => "string",
        _ => "text",
    }
}

/// Strip markdown syntax markers from a label to get the plain text that
/// jsdom `textContent` would return after markdown→HTML conversion.
///
/// Markdown `**bold**` → `<strong>bold</strong>` → textContent `bold`.
/// Markdown `*italic*` → `<em>italic</em>` → textContent `italic`.
/// HTML tags like `<br>` embedded in markdown are stripped by textContent.
/// The `\n` → `<br/>` → stripped. Result: plain text, single line.
///
/// Mirrors marked.lexer's paragraph tokenisation: blank-line-separated
/// runs become separate `<p>` elements, and each paragraph drops its
/// trailing whitespace. The textContent of `<p>p1</p><p>p2</p>` is
/// `p1` concatenated with `p2` (no separator, since paragraph tags
/// themselves contribute nothing to textContent), which is what we
/// reproduce here.
fn strip_markdown_for_measure(label: &str) -> String {
    let paragraphs = split_paragraphs_for_measure(label);
    let mut out = String::with_capacity(label.len());
    for para in &paragraphs {
        out.push_str(&strip_markdown_paragraph_for_measure(para));
    }
    out
}

/// Split a markdown source into paragraph chunks at runs of blank lines
/// (lines containing only whitespace), trimming trailing whitespace from
/// each chunk. Mirrors marked.lexer paragraph tokenisation.
fn split_paragraphs_for_measure(src: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in src.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let is_blank = body.chars().all(|c| c.is_whitespace());
        if is_blank {
            if !current.is_empty() {
                out.push(current.trim_end().to_string());
                current = String::new();
            }
        } else {
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        out.push(current.trim_end().to_string());
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn strip_markdown_paragraph_for_measure(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let bytes = label.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'*' {
            // Skip `**` or `*` markers
            if i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                i += 2; // skip **
            } else {
                i += 1; // skip *
            }
        } else if bytes[i] == b'`' {
            i += 1; // skip backtick (inline code marker)
        } else if bytes[i] == b'<' {
            // HTML tag embedded in markdown: skip to '>'
            if let Some(rel_end) = label[i..].find('>') {
                i += rel_end + 1; // skip the tag
            } else {
                // Bare '<' with no '>' — treat as literal
                out.push('<');
                i += 1;
            }
        } else if bytes[i] == b'\n' {
            // \n → <br/> in HTML → stripped by textContent
            i += 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Measure a vertex's bounding box including its intrinsic shape padding.
/// These padding values must match what the upstream shape renderers
/// compute at draw time, so that dagre assigns the correct node
/// dimensions.
///
/// `is_bold` is set when the vertex's resolved styles include
/// `font-weight:bold` — text segments then measure at bold weight to
/// match upstream's `getBoundingClientRect()` on the foreignObject div.
fn measure_vertex_box(v: &Vertex, is_bold: bool, font_size_px: Option<f64>) -> (f64, f64) {
    let label = display_label(v);
    // For markdown labels, the `**bold**` syntax is rendered as HTML and
    // textContent strips the markers — measure the plain-text equivalent.
    let is_markdown = v
        .label
        .as_ref()
        .map(|l| l.kind == LabelKind::Markdown)
        .unwrap_or(false);
    let measure_label = if is_markdown {
        strip_markdown_for_measure(&label)
    } else {
        label.clone()
    };
    // KaTeX `$$..$$` math labels: the rendered HTML is a structured KaTeX
    // tree. Measuring its textContent (via `measure_html_markup_label`)
    // reproduces what jsdom's `getBoundingClientRect` shim does on the
    // wrapping `<div>` after mermaid splices KaTeX into it. Without this
    // detour the bbox is computed from the raw `$$..$$` source text and
    // gets the node sized as if the LaTeX were a plain string — much wider
    // than the rendered KaTeX result.
    let (tw, th) = if crate::render::foreign_object::contains_katex_marker(&measure_label) {
        let font = crate::render::foreign_object::HtmlLabelFont::default();
        match crate::render::foreign_object::try_render_katex_label(&measure_label, &font) {
            Some((_, w, h)) => (w, h),
            None => measure_text_with_size(&measure_label, is_bold, font_size_px),
        }
    } else {
        measure_text_with_size(&measure_label, is_bold, font_size_px)
    };
    // Upstream shape helpers compute total size from the label bbox
    // plus per-shape padding. The `node.padding` config default is 15.
    //
    // squareRect: totalW = bbox.w + padding*4, totalH = bbox.h + padding*2
    //   (labelPaddingX = padding*2, applied twice = padding*4)
    //   (labelPaddingY = padding, applied twice = padding*2)
    // roundedRect: totalW = bbox.w + padding*2, totalH = bbox.h + padding*2
    // diamond: s = (bbox.w + padding) + (bbox.h + padding)
    // hexagon: uses nodePadding directly
    // stadium: wider by label_height
    // cylinder: extra 24 for arcs
    // circle: max(tw,th) + 32
    // doublecircle: max(tw,th) + 48
    // Apply canonical shape mapping so v11+ shape aliases (e.g. `diam`,
    // `circ`, `hex`, …) feed into the per-shape padding tables below
    // instead of falling through to the rect default.
    let shape = canon_shape(v.shape.as_deref().unwrap_or("rect"));
    let p = FLOWCHART_PADDING;
    let (pad_x, pad_y) = match shape {
        "circle" | "circ" => {
            // Upstream circle.ts: r = bbox.width/2 + halfPadding
            // halfPadding = node.padding/2 = p/2
            // d = 2*r = bbox.width + node.padding = tw + p
            // Uses label WIDTH only (not max(tw,th)) to match upstream bbox.width.
            let d = tw + p;
            return (d, d);
        }
        "doublecircle" => {
            // Upstream: r = bbox.width/2 + labelPadding*2 (look="neo") or + halfPadding*3
            // Using approximate: d = tw + p*2 (matching observed behavior)
            let d = tw + p * 2.0;
            return (d, d);
        }
        "diamond" | "question" => {
            let w = tw + p;
            let h = th + p;
            let s = w + h;
            return (s, s);
        }
        // Upstream hexagon.ts:
        //   labelPaddingX = labelPaddingY = nodePadding (default look)
        //   h3 = bbox.height + labelPaddingX
        //   m  = h3 / 4
        //   w4 = bbox.width + 2*m + labelPaddingY
        //   updateNodeBounds → node.width = w4, node.height = h3 (polygon
        //   spans 0..w4 horizontally, -h3..0 vertically; its own transform
        //   `translate(-w4/2, h3/2)` centres it but jsdom getBBox reads
        //   the raw bbox without applying that transform).
        // Feed (w4, h3) directly to dagre — no `+ 2*shear` baking like
        // trapezoid, since the polygon already lives inside [0..w4].
        "hexagon" | "hex" => {
            let h_inner = th + p;
            let m = h_inner / 4.0;
            let w_inner = tw + 2.0 * m + p;
            return (w_inner, h_inner);
        }
        // Upstream stadium.ts (default look, mermaid 11.14.0
        // mermaid.js:45403-45444):
        //   labelPaddingX = labelPaddingY = nodePadding (= p)
        //   h = bbox.height + labelPaddingY                 → th + p
        //   w = bbox.width  + h/4         + labelPaddingX   → tw + (th+p)/4 + p
        //   radius = h/2  (used for the rounded end-caps)
        //
        // The actual polygon fed to roughjs/intersect uses
        // `generateCirclePoints(..., 50, ...)` for each rounded end-cap, sampling
        // the half-circle at 50 evenly-spaced angles. With the 90→270 sweep this
        // means the closest sample to the true horizontal extreme (180°) sits at
        //   delta = (180° / 49) / 2  = 1.83673...° away,
        // so each end-cap's bbox falls short of `radius` by
        //   radius * (1 - cos(delta))
        // and the polygon's total width is
        //   w_polygon = w_analytical - 2 * radius * (1 - cos(delta))
        // upstream's `updateNodeBounds(node, polygon)` reads that polygon bbox
        // (not the analytical w), so dagre receives the corrected width. We
        // bake the same correction here to match byte-for-byte.
        // (height correction is zero — top/bottom horizontal lines reach ±h/2 exactly.)
        "stadium" | "pill" => {
            let h_stadium = th + p;
            let w_analytical = tw + h_stadium / 4.0 + p;
            // delta in radians: half the angular step of generateCirclePoints(.., 50, 90, 270)
            let step = std::f64::consts::PI / 49.0;
            let delta = step / 2.0;
            let radius = h_stadium / 2.0;
            let correction = 2.0 * radius * (1.0 - delta.cos());
            let w_polygon = w_analytical - correction;
            return (w_polygon, h_stadium);
        }
        // Upstream cylinder.ts (mermaid 11.14.0 mermaid.js:43045-43113):
        //   labelPaddingX = labelPaddingY = nodePadding (= p) for non-neo look
        //   w4 = bbox.width  + labelPaddingY  → tw + p
        //   rx = w4 / 2;  ry = rx / (2.5 + w4 / 50)
        //   h3 = bbox.height + labelPaddingX + ry  → th + p + ry
        // The path is M0,ry then arc->end + lineto + arc->end + lineto. Under
        // jsdom's pathBBox shim arc commands only register their endpoint
        // (not the arc bulge), so the measured bbox spans (w4, h3) — NOT
        // (w4, h3 + 2*ry). updateNodeBounds therefore feeds dagre:
        //   node.width  = w4 = tw + p
        //   node.height = h3 = th + p + ry  (ry depends on w4)
        "cylinder" | "cyl" => {
            let w4 = tw + p;
            let rx = w4 / 2.0;
            let ry = rx / (2.5 + w4 / 50.0);
            return (w4, th + p + ry);
        }
        // Upstream subroutine.ts (chunk-C7LX3TON.mjs:3464-3477):
        //   FRAME_WIDTH = 8, labelPaddingX = labelPaddingY = nodePadding (= p)
        //   totalWidth  = bbox.width  + 2*FRAME_WIDTH + labelPaddingX  → tw + 16 + p
        //   totalHeight = bbox.height + labelPaddingY                  → th + p
        // updateNodeBounds feeds (totalWidth, totalHeight) to dagre, so
        // pad_x must be 16 + p (not 4*p), pad_y must be p (not 2*p).
        "subroutine" => (p + 16.0, p),
        // Upstream trapezoid.ts / leanLeft.ts / leanRight.ts:
        //   labelPaddingX = labelPaddingY = nodePadding (look=neo doubles X)
        //   w = bbox.width + nodePadding,  h = bbox.height + nodePadding
        //   updateNodeBounds → node.width = polygon.getBBox().width
        //   = w + 2*shear = w + h (visual width fed to dagre).
        // We bake that here so dagre sees the correct horizontal extent;
        // shapes recover the base w as `node.width - node.height`.
        "trapezoid" | "trap" | "lean_left" | "lean-left" | "lean_right" | "lean-right" => {
            let h_inner = th + p;
            let w_inner = tw + p;
            return (w_inner + h_inner, h_inner);
        }
        // Upstream invertedTrapezoid.ts:
        //   w = bbox.width + p*2,  h = bbox.height + p*2 (non-neo)
        //   visual width = w + 2*shear = w + h.
        "inv_trapezoid" | "invertedTrapezoid" => {
            let h_inner = th + p * 2.0;
            let w_inner = tw + p * 2.0;
            return (w_inner + h_inner, h_inner);
        }
        // Upstream rectLeftInvArrow.ts (asymmetric / `>text]` syntax):
        //   labelPaddingX = labelPaddingY = nodePadding (default look)
        //   w = bbox.width  + labelPaddingX  → tw + p
        //   h = bbox.height + labelPaddingY  → th + p
        //   notch = -h/4 (used for the left-pointing arrow tip)
        // The polygon spans [-w/2 - h/4, w/2] horizontally; after the
        // shape's own translate(-notch/2, 0) = translate(h/8, 0) the
        // visual bbox spans [-w/2 - h/8, w/2 + h/8] (width = w + h/4).
        // updateNodeBounds reads that visual bbox, so dagre sees:
        //   node.width  = w + h/4
        //   node.height = h
        "rect_left_inv_arrow" | "odd" | "asymmetric" => {
            let h_inner = th + p;
            let w_inner = tw + p;
            return (w_inner + h_inner / 4.0, h_inner);
        }
        "round" | "rounded" => (p * 2.0, p * 2.0),
        "icon" => {
            let has_label = v.label.is_some();
            let is_markdown = v
                .label
                .as_ref()
                .map(|l| l.kind == LabelKind::Markdown)
                .unwrap_or(false);
            let label_text = if has_label {
                let raw = display_label(v);
                if is_markdown {
                    strip_markdown_for_measure(&raw)
                } else {
                    raw
                }
            } else {
                String::new()
            };
            let (tw_icon, th_icon) = if has_label && !label_text.is_empty() {
                measure_text_with_size(&label_text, is_bold, font_size_px)
            } else {
                let fs = font_size_px.unwrap_or(LABEL_FONT_SIZE);
                let lh = font_metrics::line_height(DEFAULT_FONT_FAMILY, fs, false, false);
                (0.0, lh)
            };
            let icon_size = 48.0;
            let gap = if has_label { 8.0 } else { 0.0 };
            let total_w = if tw_icon > icon_size { tw_icon } else { icon_size };
            let total_h = icon_size + gap + th_icon;
            return (total_w, total_h);
        }
        _ => (p * 4.0, p * 2.0), // rect / squareRect: labelPaddingX = p*2, ×2 sides = p*4
    };
    (tw + pad_x, th + pad_y)
}

/// Strip FontAwesome icon prefixes from a label string before measurement.
/// Upstream replaces `fa:fa-car` with `<i class="fa fa-car"></i>` at render
/// time; the `<i>` element contributes negligible width under the jsdom shim,
/// so we remove those tokens before measuring text width.
fn strip_fa_icons(text: &str) -> String {
    // Match patterns like `fa:fa-car`, `fas:fa-spinner`, `fab:fa-github`, etc.
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("fa") {
        // Check whether this starts a "fa[bklrs]?:fa-<name>" sequence.
        let tail = &rest[pos..];
        // Find the colon.
        let prefix_end = tail.find(':').unwrap_or(tail.len());
        let prefix = &tail[..prefix_end];
        // Valid FA prefixes: fa, fab, fak, fal, far, fas
        let valid_prefix = matches!(prefix, "fa" | "fab" | "fak" | "fal" | "far" | "fas");
        if valid_prefix && tail[prefix_end + 1..].starts_with("fa-") {
            // Consume leading text up to this match.
            out.push_str(&rest[..pos]);
            // Skip past "prefix:fa-name" where name is [a-z0-9-]+.
            let icon_tail = &tail[prefix_end + 1 + 3..]; // after "fa-"
            let icon_end = icon_tail
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                .unwrap_or(icon_tail.len());
            rest = &rest[pos + prefix_end + 1 + 3 + icon_end..];
        } else {
            // Not a valid FA token — emit up to and including "fa" and move on.
            out.push_str(&rest[..pos + 2]);
            rest = &rest[pos + 2..];
        }
    }
    out.push_str(rest);
    out
}

/// Split label text into measurement lines, treating `<br>` / `<br/>` /
/// `<br />` as line breaks. All other HTML tags are stripped and `\n`
/// characters are dropped.
///
/// Used by [`measure_text`] when the rendered foreignObject `<div>` needs
/// the per-line maximum width. Upstream `string_label_to_html` converts
/// the source `\n` into `<br/>` BEFORE passing the string to the renderer
/// — but its measurement runs on the post-conversion HTML, so only the
/// `<br>` variants count as line breaks here. Bare `\n` characters that
/// survive (e.g. inside a markdown paragraph) are treated like upstream
/// `textContent` and ignored.
fn split_html_into_lines(s: &str) -> Vec<String> {
    let mut lines: Vec<String> = vec![String::new()];
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if let Some(rel_end) = s[i..].find('>') {
                let tag_full = &s[i..i + rel_end + 1];
                let lowered: String = tag_full.chars().map(|c| c.to_ascii_lowercase()).collect();
                let trimmed = lowered
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .trim_end_matches('/')
                    .trim();
                if trimmed == "br" {
                    lines.push(String::new());
                }
                i += rel_end + 1;
            } else {
                lines.last_mut().unwrap().push('<');
                i += 1;
            }
        } else if bytes[i] == b'\n' {
            // jsdom textContent on `<p>foo\nbar</p>` keeps the newline as
            // whitespace (or strips it, depending on context). Treat as
            // dropped to match the legacy single-line behaviour.
            i += 1;
        } else {
            lines.last_mut().unwrap().push(bytes[i] as char);
            i += 1;
        }
    }
    lines
}

/// Measure the overall width/height of the (possibly multi-line) label.
///
/// Upstream mermaid measures node labels via `measureTextBlock` which puts
/// the rendered HTML into a jsdom `<div>` and then reads `el.textContent`.
/// `textContent` strips ALL HTML tags (including `<br/>`) and returns the
/// concatenated plain text — which never contains `\n` since `\n` in the
/// original label was already converted to `<br/>` before measurement.
/// Therefore the measured block is always exactly ONE line, regardless of
/// how many `<br/>` or `\n` appear in the source label.
///
/// Width is the width of the concatenated plain text (with bold spans
/// measured at bold weight). Height is always one `line_height`.
///
/// `force_bold` is set when the vertex's resolved styles (classDef +
/// inline style) include `font-weight:bold` — in which case ALL text
/// segments measure at bold width regardless of inner `<strong>` tags.
fn measure_text(label: &str, force_bold: bool) -> (f64, f64) {
    measure_text_with_size(label, force_bold, None)
}

/// Like [`measure_text`] but lets the caller override the font-size used
/// for both width and line-height. `font_size_px = None` falls back to
/// the default `LABEL_FONT_SIZE` (14 px). When the resolved styles
/// include `font-size: 30px` etc. the rendered foreignObject `<div>`'s
/// inline `font-size:30px !important;` propagates to its `<span>` /
/// `<p>` content (cypress fixture 150's `classDef larger font-size:30px`),
/// so the jsdom shim measures the bbox at the larger font.
fn measure_text_with_size(label: &str, force_bold: bool, font_size_px: Option<f64>) -> (f64, f64) {
    let font_size = font_size_px.unwrap_or(LABEL_FONT_SIZE);
    if label.is_empty() {
        return (0.0, font_size);
    }
    // Strip FA icon tokens first — they render as <i> elements with no width.
    let stripped = strip_fa_icons(label);
    let lh = font_metrics::line_height(DEFAULT_FONT_FAMILY, font_size, false, false);

    // Upstream measures the rendered foreignObject `<div>` via
    // `el.textContent`, which strips ALL HTML tags (including `<br/>`) and
    // returns the concatenated plain text as a SINGLE line. The block height
    // is therefore exactly one `line_height`, regardless of how many `<br/>`
    // or `\n` appear in the source label. Cypress fixtures 67 / 200 / 214 and
    // demos 06 / 07 all encode multi-line diamond / hexagon labels via
    // `<br/>` and expect the foreignObject geometry of the concatenated text.
    //
    // Width is the width of the concatenated lines, measured as one segment.
    let lines = split_html_into_lines(&stripped);
    let concat: String = lines.concat();
    let width =
        font_metrics::text_width(&concat, DEFAULT_FONT_FAMILY, font_size, force_bold, false);
    (width, lh)
}

fn measure_subgraph_title_box(title: Option<&Label>) -> (f64, f64) {
    let Some(label) = title else {
        let (w, h) = measure_text("", false);
        return (w + 16.0, h + 16.0);
    };
    // Markdown labels render through `markdownToHtml`, which expands
    // `**bold**` / `*italic*` into `<strong>`/`<em>` tags. Width measurement
    // must therefore strip those backtick markers and measure the inner
    // text with the appropriate weight.
    let measure_input: String = match label.kind {
        crate::model::flowchart::LabelKind::Markdown => {
            crate::render::foreign_object::markdown_label_to_html(&label.text)
        }
        _ => label.text.clone(),
    };
    let (w, h) = measure_text(&measure_input, false);
    (w + 16.0, h + 16.0)
}

/// Measure edge label dimensions to match the foreignObject rendered at runtime.
/// Upstream edge labels use the jsdom default font: sans-serif 14px non-bold,
/// which differs from the node-label font (trebuchet ms 14px).
///
/// Upstream renders the edge label as `<p>…</p>` and measures the result via
/// `getBoundingClientRect()`. The jsdom shim collapses to `textContent` width
/// — which strips ALL HTML tags (including `<br/>`) and measures the
/// concatenated plain text as a single line. Sources with `<br>` line breaks
/// therefore measure to the same width as the joined text would, NOT to the
/// raw markup width that would include the literal `<br>` characters.
///
/// `html_labels` toggles the htmlLabels=true (default, foreignObject) vs
/// htmlLabels=false (text/tspan with `<rect>` background) measurement. In
/// the htmlLabels=false branch upstream's `bbox = labelGroup.getBBox()`
/// inflates the text bbox by the rect's 2-px padding on every side
/// (createText.ts:178-183), so the dimensions dagre sees gain `+4` on
/// each axis.
///
/// `is_markdown` controls whether we strip paired markdown emphasis
/// markers before measuring — under htmlLabels=false the rendered text
/// content drops the `**`/`*`/`__`/`_` markers, so the bbox width
/// follows the post-strip text.
fn measure_edge_label(text: &str, html_labels: bool, is_markdown: bool) -> (f64, f64) {
    const EDGE_LABEL_FONT: &str = "sans-serif";
    const EDGE_LABEL_SIZE: f64 = 14.0;
    let h = font_metrics::line_height(EDGE_LABEL_FONT, EDGE_LABEL_SIZE, false, false);
    if text.is_empty() {
        return (0.0, h);
    }
    // KaTeX `$$..$$` math edge labels: the layout engine must reserve the
    // post-render bbox, not the raw `$$..$$` source width.
    if html_labels && crate::render::foreign_object::contains_katex_marker(text) {
        let font = crate::render::foreign_object::HtmlLabelFont::default();
        if let Some((_, w, h)) =
            crate::render::foreign_object::try_render_katex_label(text, &font)
        {
            return (w, h);
        }
    }
    let measure_text = if is_markdown {
        // Under htmlLabels=false the SVG <text> textContent comes from
        // `markdownToLines` → `updateTextContentAndStyles`, which only
        // emits the marker-free text. Under htmlLabels=true the foreignObject
        // <p> displays the rendered HTML, but jsdom's getBoundingClientRect
        // still measures the textContent (i.e. marker-free string). In
        // both cases reuse `markdownToHTML` so the measured width matches
        // upstream — strip_html_for_measurement removes the inserted
        // `<strong>`/`<em>` tags and yields the plain text.
        crate::render::foreign_object::markdown_label_to_html(text)
    } else {
        text.to_string()
    };
    // Mirror `parse_html_text_segments`/textContent semantics: strip HTML
    // tags (`<br>`, `<strong>`, …) and decode entities, then measure the
    // result as ONE line — `<br>` does not split because `textContent`
    // collapses break tags. `\n` characters survive as whitespace and are
    // dropped here to match upstream's `measureTextBlock` shim.
    let plain = strip_html_for_measurement(&measure_text);
    let w = font_metrics::text_width(&plain, EDGE_LABEL_FONT, EDGE_LABEL_SIZE, false, false);
    if !html_labels {
        // `bbox` of labelGroup = unionOf(rect{-2,-2,w+4,h+4}, text{0,0,w,h})
        //                     = {x:-2, y:-2, w:w+4, h:h+4}
        // → dagre sees the inflated dimensions.
        return (w + 4.0, h + 4.0);
    }
    (w, h)
}

/// Strip HTML tags and decode common entities to mirror jsdom's
/// `textContent` for edge-label width measurement.
fn strip_html_for_measurement(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // A `<` only starts an HTML tag when followed by an ASCII letter
            // or `/letter`. Anything else (`<<`, `< `, `<1`, `<!`, `<?`) is
            // treated as literal text — matching parse_html_text_segments.
            let next = bytes.get(i + 1).copied();
            let is_tag_start = match next {
                Some(c) if c.is_ascii_alphabetic() => true,
                Some(b'/') => bytes
                    .get(i + 2)
                    .map(|c| c.is_ascii_alphabetic())
                    .unwrap_or(false),
                _ => false,
            };
            if is_tag_start {
                if let Some(rel_end) = s[i..].find('>') {
                    i += rel_end + 1;
                    continue;
                }
            }
            out.push('<');
            i += 1;
        } else if bytes[i] == b'&' {
            if let Some(semi_rel) = s[i..].find(';') {
                let entity = &s[i + 1..i + semi_rel];
                let ch = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some('\u{00A0}'),
                    _ => None,
                };
                if let Some(c) = ch {
                    out.push(c);
                    i += semi_rel + 1;
                    continue;
                }
            }
            out.push('&');
            i += 1;
        } else if bytes[i] == b'\n' {
            // textContent treats inline `\n` as whitespace; under
            // measureTextBlock the legacy single-line behaviour drops it.
            i += 1;
        } else {
            // UTF-8-safe copy of the next char.
            let mut len = 1usize;
            while len < 4 && i + len < bytes.len() && (bytes[i + len] & 0xC0) == 0x80 {
                len += 1;
            }
            out.push_str(&s[i..i + len]);
            i += len;
        }
    }
    out
}

/// Build a unified::Edge from a model Edge, applying link-style overrides.
/// `pair_counter` is the per-(start,end) duplicate count — 0 for the first
/// edge between a given pair, 1 for the second, etc. (upstream `getEdgeId`).
fn build_edge<'a>(
    e: &ModelEdge,
    d: &FlowchartDiagram,
    pair_counter: usize,
    class_map: &BTreeMap<&'a str, &'a ClassDef>,
    html_labels: bool,
    config_curve: &str,
) -> unified::Edge {
    let mut ue = unified::Edge::default();
    // Custom-id syntax `A name@-->B` lets the source set an explicit edge id
    // (`name`). Upstream uses that id directly; only fall back to the
    // synthetic `L_{start}_{end}_{counter}` form when no custom id is given.
    ue.id = match &e.id {
        Some(custom) if !custom.is_empty() => custom.clone(),
        _ => format!("L_{}_{}_{}", e.start, e.end, pair_counter),
    };
    ue.start = Some(e.start.clone());
    ue.end = Some(e.end.clone());
    ue.minlen = Some(e.length as i32);
    ue.label = e.label.as_ref().map(|l| l.text.clone());
    ue.label_type = Some(label_kind_string(e.label.as_ref()).to_string());
    ue.arrow_type_end = Some(arrow_kind_string(e.arrow_end).to_string());
    ue.arrow_type_start = Some(arrow_kind_string(e.arrow_start).to_string());
    let (thickness, pattern) = stroke_descriptor(e.stroke);
    ue.thickness = Some(thickness.into());
    ue.pattern = Some(pattern.into());
    ue.stroke = Some(thickness.into());
    ue.interpolate = Some(config_curve.into());
    ue.curve = Some(config_curve.into());
    if let Some(curve_override) = &e.curve {
        ue.interpolate = Some(curve_override.clone());
        ue.curve = Some(curve_override.clone());
    }
    // `@{ animation: slow|fast }` propagates to the unified edge so the
    // renderer can append the matching `edge-animation-{slow,fast}` CSS
    // class to the `<path>` element. Cypress fixtures 113 / 237.
    if let Some(anim) = &e.animation {
        ue.animation = Some(anim.clone());
    }
    // dagre needs edge label dimensions to reserve space between ranks;
    // labelpos="c" centres the label on the spline (upstream flowchart default).
    ue.labelpos = Some("c".into());
    let label_text = e.label.as_ref().map(|l| l.text.as_str()).unwrap_or("");
    let is_markdown_label = e
        .label
        .as_ref()
        .map(|l| matches!(l.kind, crate::model::flowchart::LabelKind::Markdown))
        .unwrap_or(false);
    let (lw, lh) = measure_edge_label(label_text, html_labels, is_markdown_label);
    ue.extra.insert("label_width".into(), lw.to_string());
    ue.extra.insert("label_height".into(), lh.to_string());

    // Resolve class-based edge styles (`class <edge-id> myClass`). Upstream
    // pushes the classDef styles into `edge.style` and the colour-subset
    // (any property whose key contains `color`) into `edge.labelStyle` so
    // the path style renderer can emit them in the right order.
    let mut applied_styles: Vec<String> = Vec::new();
    let mut applied_text_styles: Vec<String> = Vec::new();
    for cls in &e.classes {
        if let Some(cd) = class_map.get(cls.as_str()) {
            for s in &cd.styles {
                applied_styles.push(s.clone());
                let trimmed = s.trim().trim_end_matches(';');
                if let Some(colon) = trimmed.find(':') {
                    let key = trimmed[..colon].trim();
                    if key.contains("color") {
                        applied_text_styles.push(s.clone());
                    }
                }
            }
        }
    }
    // Apply link-style overrides.
    let mut interpolate: Option<String> = None;
    for ls in &d.link_styles {
        if apply_link_style(ls, e.index) {
            for s in &ls.styles {
                applied_styles.push(s.clone());
                // Mirror upstream `flowDb.updateLink`: when a linkStyle entry
                // contains a `color`-related property (e.g. `color:Sienna`),
                // upstream pushes it onto `defaultStyle.labelStyle` so the
                // label's <div>/<span> render with the matching color.
                let trimmed = s.trim().trim_end_matches(';');
                if let Some(colon) = trimmed.find(':') {
                    let key = trimmed[..colon].trim();
                    if key.contains("color") {
                        applied_text_styles.push(s.clone());
                    }
                }
            }
            if let Some(i) = &ls.interpolate {
                interpolate = Some(i.clone());
            }
        }
    }
    if !applied_styles.is_empty() {
        ue.style = Some(applied_styles);
    }
    if !applied_text_styles.is_empty() {
        ue.label_style = Some(applied_text_styles);
    }
    if let Some(i) = interpolate {
        ue.interpolate = Some(i.clone());
        ue.curve = Some(i);
    }
    ue.look = Some("classic".into());
    if let Some(scope) = &e.scope {
        ue.extra.insert("scope_parent".into(), scope.clone());
    }
    ue
}

fn apply_link_style(ls: &LinkStyle, idx: usize) -> bool {
    ls.is_default || ls.indices.iter().any(|&i| i == idx)
}

fn arrow_kind_string(a: ArrowType) -> &'static str {
    match a {
        ArrowType::None => "none",
        ArrowType::Arrow => "arrow_point",
        ArrowType::Circle => "arrow_circle",
        ArrowType::Cross => "arrow_cross",
        ArrowType::Point => "arrow_point",
    }
}

fn stroke_descriptor(s: EdgeStroke) -> (&'static str, &'static str) {
    match s {
        EdgeStroke::Normal => ("normal", "solid"),
        EdgeStroke::Thick => ("thick", "solid"),
        EdgeStroke::Dotted => ("normal", "dotted"),
        EdgeStroke::Invisible => ("invisible", "solid"),
    }
}

/// Detect whether a resolved style list contains `font-weight:bold` or a
/// numeric font-weight ≥ 700. Used by the layout to widen text
/// measurement when a vertex's classDef / inline style applies bold —
/// matching upstream's `getBoundingClientRect()` on the bold-styled
/// foreignObject div.
/// Public re-export for the renderer's `font_size_postprocess_node_svg`.
/// Keeping the helper inside this module so the layout-side measurement
/// and the render-side post-processor share a single parser, avoiding
/// drift between resolved widths and emitted SVG numbers.
pub fn styles_font_size_px_pub(styles: &[String]) -> Option<f64> {
    styles_font_size_px(styles)
}

/// Extract a `font-size` value (in px) from a list of CSS declarations.
///
/// Recognises the common forms emitted by `classDef`/`style` directives:
///   - `font-size:30px`
///   - `font-size: 30px !important`
///   - `font-size: 1.25em` (treated as multiplier × `LABEL_FONT_SIZE`).
///
/// Returns `None` when no `font-size` is set; the caller falls back to the
/// default 14 px label font. Used to correct width / height measurement of
/// nodes whose `classDef` upgrades the rendered foreignObject font (e.g.
/// cypress fixture 150's `classDef larger font-size:30px`).
fn styles_font_size_px(styles: &[String]) -> Option<f64> {
    for s in styles {
        let trimmed = s.trim().trim_end_matches(';');
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        let key = trimmed[..colon].trim();
        if !key.eq_ignore_ascii_case("font-size") {
            continue;
        }
        let value = trimmed[colon + 1..].trim();
        let val_no_important = value
            .trim_end_matches("!important")
            .trim()
            .trim_end_matches('!')
            .trim();
        if let Some(stripped) = val_no_important.strip_suffix("px") {
            if let Ok(n) = stripped.trim().parse::<f64>() {
                return Some(n);
            }
        } else if let Some(stripped) = val_no_important.strip_suffix("em") {
            if let Ok(n) = stripped.trim().parse::<f64>() {
                return Some(n * LABEL_FONT_SIZE);
            }
        } else if let Some(stripped) = val_no_important.strip_suffix("rem") {
            if let Ok(n) = stripped.trim().parse::<f64>() {
                return Some(n * LABEL_FONT_SIZE);
            }
        }
    }
    None
}

fn styles_have_bold(styles: &[String]) -> bool {
    for s in styles {
        let trimmed = s.trim().trim_end_matches(';');
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        let key = trimmed[..colon].trim();
        if !key.eq_ignore_ascii_case("font-weight") {
            continue;
        }
        let value = trimmed[colon + 1..].trim();
        // Trim trailing `!important` for keyword/numeric checks.
        let val_no_important = value
            .trim_end_matches("!important")
            .trim()
            .trim_end_matches('!')
            .trim();
        if val_no_important.eq_ignore_ascii_case("bold")
            || val_no_important.eq_ignore_ascii_case("bolder")
        {
            return true;
        }
        if let Ok(n) = val_no_important.parse::<u32>() {
            if n >= 700 {
                return true;
            }
        }
    }
    false
}

/// Compose styles from classDef + inline styles. Returns `Vec<String>`
/// of `"key:value"` entries.
///
/// Mirrors upstream `compileStyles(node)` which builds a `Map<key,value>`
/// from `[...cssCompiledStyles, ...cssStyles, ...labelStyle]` and then
/// emits `[...stylesMap]`. The `Map` semantics dedupe by key, with later
/// entries overriding earlier ones — so e.g. `classDef node color:red`
/// followed by a vertex's own `classDef myClass1 color:#0000ff` results
/// in a single `color:#0000ff` entry, not two competing `color:` rules
/// in the inline `style="…"` attribute.
/// Build a synthetic [`Vertex`] that represents a subgraph for style-collection.
///
/// Combines:
/// - the existing `style <id> ...` Vertex entry (if any) — same path used for
///   inline-styled vertices.
/// - the `class <id> <className>` directive → adds class names so the
///   subsequent `collect_styles` pass walks each `classDef`'s style list
///   and dedupes across them by CSS key (last-wins).
fn synthesize_vertex_for_subgraph(
    sg: &crate::model::flowchart::Subgraph,
    d: &FlowchartDiagram,
) -> Vertex {
    let mut v = Vertex::default();
    v.id = sg.id.clone();
    if let Some(existing) = d.find_vertex(&sg.id) {
        v.styles = existing.styles.clone();
        v.classes.extend(existing.classes.iter().cloned());
    }
    for cls in &sg.classes {
        if !v.classes.iter().any(|c| c == cls) {
            v.classes.push(cls.clone());
        }
    }
    v
}

fn collect_styles<'a>(v: &'a Vertex, class_map: &BTreeMap<&'a str, &'a ClassDef>) -> Vec<String> {
    // Upstream: getCompiledStyles(["default", "node", ...vertex.classes])
    let mut raw: Vec<String> = Vec::new();
    for builtin in &["default", "node"] {
        if let Some(cd) = class_map.get(*builtin) {
            raw.extend(cd.styles.iter().cloned());
        }
    }
    for cls in &v.classes {
        if let Some(cd) = class_map.get(cls.as_str()) {
            raw.extend(cd.styles.iter().cloned());
        }
    }
    raw.extend(v.styles.iter().cloned());

    // Dedupe by CSS property key, preserving insertion order of the
    // *last* entry per key — mirrors upstream's `styles2Map` which uses
    // a `Map` keyed by the property name. We retain order based on the
    // first time the key was seen, then overwrite the value when a
    // later entry repeats the key (matches JS `Map.set` semantics).
    let mut order: Vec<String> = Vec::new();
    let mut by_key: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    for entry in raw {
        let trimmed = entry.trim().trim_end_matches(';');
        if trimmed.is_empty() {
            continue;
        }
        let Some(colon) = trimmed.find(':') else {
            // No `:` — keep the raw token under itself so it survives
            // the dedupe pass (rare, but mirrors `styles2Map` which
            // would also keep it as `key=raw, value=undefined`).
            let key = trimmed.to_string();
            if !by_key.contains_key(&key) {
                order.push(key.clone());
            }
            by_key.insert(key, (trimmed.to_string(), String::new()));
            continue;
        };
        let key = trimmed[..colon].trim().to_string();
        let value = trimmed[colon + 1..].trim().to_string();
        if !by_key.contains_key(&key) {
            order.push(key.clone());
        }
        by_key.insert(key.clone(), (key, value));
    }
    order
        .into_iter()
        .filter_map(|k| by_key.remove(&k))
        .map(|(k, v)| {
            if v.is_empty() {
                k
            } else {
                format!("{}:{}", k, v)
            }
        })
        .collect()
}

/// Compose the DOM id mermaid uses for a flowchart node:
/// `flowchart-<id>-<order>`. Upstream dedupes and coalesces this on
/// per-render basis — the order int is globally unique.
fn flowchart_dom_id(id: &str, order: usize) -> String {
    format!("flowchart-{}-{}", id, order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::flowchart as fcp;

    #[test]
    fn layout_minimal_two_node_graph() {
        let src = "flowchart TD\nA --> B\n";
        let d = fcp::parse(src).unwrap();
        let theme = ThemeVariables::default();
        let l = layout(&d, &theme).unwrap();
        assert_eq!(l.nodes.len(), 2);
        assert_eq!(l.edges.len(), 1);
        let a = l.nodes.iter().find(|n| n.id == "A").unwrap();
        assert!(a.x.is_some() && a.y.is_some());
    }

    #[test]
    fn layout_subgraph_creates_cluster() {
        let src = "flowchart TD\nsubgraph s1 [Title]\n  A-->B\nend\nA-->C\n";
        let d = fcp::parse(src).unwrap();
        let theme = ThemeVariables::default();
        let l = layout(&d, &theme).unwrap();
        assert!(l.clusters.iter().any(|c| c.id == "s1"));
        // members must have their parent_id set
        let a = l.nodes.iter().find(|n| n.id == "A").unwrap();
        assert_eq!(a.parent_id.as_deref(), Some("s1"));
    }

    #[test]
    fn layout_lr_direction_flows_horizontally() {
        let src = "flowchart LR\nA-->B\n";
        let d = fcp::parse(src).unwrap();
        let theme = ThemeVariables::default();
        let l = layout(&d, &theme).unwrap();
        let a = l.nodes.iter().find(|n| n.id == "A").unwrap();
        let b = l.nodes.iter().find(|n| n.id == "B").unwrap();
        assert!(b.x.unwrap() > a.x.unwrap());
    }
}
