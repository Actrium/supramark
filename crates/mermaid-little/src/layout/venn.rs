//! Venn diagram layout — port of @upsetjs/venn.js (BSD-2-Clause).
//!
//! Upstream source (vendored copy):
//!   /ext/mermaid/tests/support/node_modules/@upsetjs/venn.js/src/{layout,circleintersection,diagram}.js
//!
//! We port the portions that are deterministic enough to reach
//! byte-exact parity with mermaid@11.14.0:
//!
//!   - `circleOverlap` / `distanceFromIntersectArea`  — circle-circle
//!     overlap area + bisection inversion. (deterministic)
//!   - `greedyLayout`  — initial positions, axis-aligned, no random.
//!   - `nelderMead`  — derivative-free downhill simplex; deterministic.
//!   - `normalizeSolution` + `scaleSolution`  — orient largest circle
//!     at origin, rotate second-largest to `Math.PI/2`, fit to box.
//!   - `intersectionAreaArcs` + `arcsToPath`  — SVG path generation.
//!   - `computeTextCentre`  — nelderMead-driven text anchor.
//!
//! The `constrainedMDS` branch (areas.length >= 8) is **not** ported:
//! it depends on `Math.random()` via `restarts` random initial guesses,
//! and there's no way to reproduce V8's random seed. Fixtures that
//! tip into that branch are marked `known_ignored`.

use crate::error::Result;
use crate::model::venn::VennDiagram;
use crate::theme::ThemeVariables;
use std::collections::BTreeMap;

const SMALL: f64 = 1e-10;

// All transcendental and root math in this module is routed through the
// `libm` crate (a pure-Rust port of fdlibm). V8's Math.* implementations
// are also fdlibm-derived, so going through libm gives byte-for-byte
// parity on acos/atan2/sqrt/sin/cos — eliminating ULP-level drift in
// the Nelder-Mead simplex trajectory that otherwise propagates to scaled
// SVG path coordinates and breaks byte-exact fixture comparison.
//
// Plain IEEE-754 ops (powi, abs, min, max, fract, comparisons) are NOT
// rerouted: the hardware path is identical across V8 and Rust.
#[inline] fn fsqrt(x: f64) -> f64 { libm::sqrt(x) }
#[inline] fn facos(x: f64) -> f64 { libm::acos(x) }
#[inline] fn fsin(x: f64) -> f64 { libm::sin(x) }
#[inline] fn fcos(x: f64) -> f64 { libm::cos(x) }
#[inline] fn fatan2(y: f64, x: f64) -> f64 { libm::atan2(y, x) }
#[inline] fn ffloor(x: f64) -> f64 { libm::floor(x) }
#[inline] fn fround(x: f64) -> f64 {
    // V8 Math.round: round-half-to-+infinity (toward +∞ on .5).
    // libm::round is round-half-away-from-zero (e.g. -0.5 -> -1.0),
    // which differs from V8 only on negative .5 values. Emulate V8's
    // semantics directly: floor(x + 0.5).
    libm::floor(x + 0.5)
}

#[derive(Debug, Clone)]
pub struct VennLayout {
    pub viewbox_w: f64,
    pub viewbox_h: f64,
    pub title_height: f64,
    pub scale: f64,
    /// Final laid-out circles by setid.
    pub circles: BTreeMap<String, Circle>,
    /// Per-area data ready for SVG output, in input order.
    pub areas: Vec<AreaLayout>,
    /// Theme color list to cycle through (venn1..venn8).
    pub theme_colors: Vec<String>,
    pub title_text_color: String,
    pub set_text_color: String,
}

#[derive(Debug, Clone, Copy)]
pub struct Circle {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

#[derive(Debug, Clone)]
pub struct AreaLayout {
    pub sets: Vec<String>,
    pub size: f64,
    pub label: Option<String>,
    /// One sub-circle per setid in `sets` (looked up from `circles`).
    pub circles: Vec<Circle>,
    /// Path d=… (already includes the leading `\n`).
    pub path: String,
    /// Text centre (Math.floor in JS) — for the visible venn-circle/intersection label.
    pub text_x: i64,
    pub text_y: i64,
    /// Raw float text centre — UNUSED for visible label. Kept for future use.
    pub text_x_f: f64,
    pub text_y_f: f64,
    /// Set the label uses for the circle case (single-set: id; otherwise empty
    /// unless data.label is set).
    pub render_label: String,
    /// Per-area circles re-scaled with the venn config padding (default 8).
    /// Used ONLY by the text-node `foreignObject` positioning, mirroring
    /// upstream's separate `venn.layout(..., {padding: config.padding ?? 15})`
    /// call where the merged config supplies `padding=8` while the
    /// `VennDiagram` component (visible circles) hardcodes `padding=15`.
    pub text_node_circles: Vec<Circle>,
    /// Text centre computed from the padding=8 circles, raw float.
    /// Mirrors `area.text.x/y` from the upstream `layout()` helper.
    pub text_node_centre_x: f64,
    pub text_node_centre_y: f64,
}

pub fn layout(d: &VennDiagram, theme: &ThemeVariables) -> Result<VennLayout> {
    let svg_w = 800.0_f64;
    let svg_h = 450.0_f64;
    let reference_w = 1600.0_f64;
    let scale = svg_w / reference_w;
    let title_h = if d.meta.title.is_some() { 48.0 * scale } else { 0.0 };
    // VennDiagram component's hardcoded internal default — drives the
    // visible circles, paths, and label positions.
    let padding_visible = 15.0_f64;
    // Mermaid venn config default — drives the SECOND `venn.layout(...)`
    // call upstream uses to compute text-node `foreignObject` positions.
    // This value comes from `defaultConfig.venn.padding = 8` and is NOT
    // overridable through .mmd syntax (no init/config block in venn-beta).
    let padding_text_node = 8.0_f64;

    // Filter out empty sets (size==0 single-element subsets) and any
    // unions referencing them. (mirrors `chart` in diagram.js).
    let mut sets: Vec<&crate::model::venn::VennSubset> = d.subsets.iter().collect();
    let to_remove: std::collections::BTreeSet<String> = sets
        .iter()
        .filter(|s| s.size == 0.0 && s.sets.len() == 1)
        .map(|s| s.sets[0].clone())
        .collect();
    sets.retain(|s| !s.sets.iter().any(|x| to_remove.contains(x)));

    // Build areas representation as (sets, size, weight) tuples.
    let mut areas_vec: Vec<Area> = sets
        .iter()
        .map(|s| Area {
            sets: s.sets.clone(),
            size: s.size,
            weight: 1.0,
        })
        .collect();

    let mut circle_map: BTreeMap<String, Circle> = BTreeMap::new();
    let mut circle_map_text_node: BTreeMap<String, Circle> = BTreeMap::new();

    if !areas_vec.is_empty() {
        // --- venn() ---
        // 1) addMissingAreas
        let augmented = add_missing_areas(&areas_vec);
        // 2) greedyLayout — initial positions
        let mut initial = greedy_layout(&augmented);
        // 3) nelderMead optimisation
        // Determine setids order from circles (insertion).
        let setids: Vec<String> = initial.keys().cloned().collect();
        let mut x0: Vec<f64> = Vec::with_capacity(setids.len() * 2);
        for id in &setids {
            let c = initial.get(id).unwrap();
            x0.push(c.x);
            x0.push(c.y);
        }
        // Take a copy of radii (immutable across optimisation).
        let radii: BTreeMap<String, f64> = setids
            .iter()
            .map(|id| (id.clone(), initial.get(id).unwrap().radius))
            .collect();

        let solution = nelder_mead(
            |values| {
                let mut current: BTreeMap<String, Circle> = BTreeMap::new();
                for (i, id) in setids.iter().enumerate() {
                    current.insert(
                        id.clone(),
                        Circle {
                            x: values[2 * i],
                            y: values[2 * i + 1],
                            radius: radii[id],
                        },
                    );
                }
                loss_function(&current, &augmented)
            },
            &x0,
            NelderMeadParams::default(),
        );

        for (i, id) in setids.iter().enumerate() {
            let c = initial.get_mut(id).unwrap();
            c.x = solution.x[2 * i];
            c.y = solution.x[2 * i + 1];
        }
        let initial = initial;

        // 4) normalize — used by both scale calls.
        let normalized = normalize_solution(&initial, std::f64::consts::PI / 2.0);
        // 5a) scale to view with padding=15 (VennDiagram component).
        circle_map = scale_solution(&normalized, svg_w, svg_h - title_h, padding_visible);
        // 5b) scale to view with padding=8 (config.venn.padding default) for
        // text-node positioning. Mirrors the second `venn.layout(...)` call
        // upstream uses to populate `layoutByKey`.
        circle_map_text_node =
            scale_solution(&normalized, svg_w, svg_h - title_h, padding_text_node);

        // Use original (post-filter) areas only — not augmented — for output.
        areas_vec = sets
            .iter()
            .map(|s| Area {
                sets: s.sets.clone(),
                size: s.size,
                weight: 1.0,
            })
            .collect();
    }

    // Per-area output layout.
    let mut out_areas: Vec<AreaLayout> = Vec::with_capacity(areas_vec.len());
    let labels_by_key: BTreeMap<String, String> = sets
        .iter()
        .filter_map(|s| s.label.clone().map(|l| (s.sets.join(","), l)))
        .collect();

    // For text-centre computation, also need a list of ALL areas (to mirror
    // upstream computeTextCentres which iterates the original areas).
    // Visible label centres come from the padding=15 circles…
    let text_centres = compute_text_centres(&circle_map, &areas_vec);
    // …text-node centres come from the padding=8 circles.
    let text_centres_text_node =
        compute_text_centres(&circle_map_text_node, &areas_vec);

    for a in &areas_vec {
        let circles_for_area: Vec<Circle> = a
            .sets
            .iter()
            .filter_map(|id| circle_map.get(id).copied())
            .collect();
        let arcs = intersection_area_arcs(&circles_for_area);
        // diagram.js renders paths with round=null (no rounding); the
        // `layout()` helper uses round=2 but its output is only used
        // for `textCentres` lookup, never for the rendered SVG.
        let path = arcs_to_path(&arcs, None);
        let key = a.sets.join(",");
        let centre = text_centres.get(&key).copied().unwrap_or((0.0, 0.0));
        let centre_tn = text_centres_text_node
            .get(&key)
            .copied()
            .unwrap_or((0.0, 0.0));
        let circles_for_text_node: Vec<Circle> = a
            .sets
            .iter()
            .filter_map(|id| circle_map_text_node.get(id).copied())
            .collect();
        let render_label = if let Some(l) = labels_by_key.get(&key) {
            l.clone()
        } else if a.sets.len() == 1 {
            a.sets[0].clone()
        } else {
            String::new()
        };
        out_areas.push(AreaLayout {
            sets: a.sets.clone(),
            size: a.size,
            label: labels_by_key.get(&key).cloned(),
            circles: circles_for_area,
            path,
            text_x: ffloor(centre.0) as i64,
            text_y: ffloor(centre.1) as i64,
            text_x_f: centre.0,
            text_y_f: centre.1,
            render_label,
            text_node_circles: circles_for_text_node,
            text_node_centre_x: centre_tn.0,
            text_node_centre_y: centre_tn.1,
        });
    }

    let theme_colors: Vec<String> = [
        theme.venn1.as_ref(),
        theme.venn2.as_ref(),
        theme.venn3.as_ref(),
        theme.venn4.as_ref(),
        theme.venn5.as_ref(),
        theme.venn6.as_ref(),
        theme.venn7.as_ref(),
        theme.venn8.as_ref(),
    ]
    .iter()
    .filter_map(|x| x.cloned())
    .collect();

    let title_text_color = theme
        .venn_title_text_color
        .clone()
        .or_else(|| theme.title_color.clone())
        .unwrap_or_else(|| "#333".into());

    let set_text_color = theme
        .venn_set_text_color
        .clone()
        .or_else(|| theme.text_color.clone())
        .unwrap_or_else(|| "#333".into());

    Ok(VennLayout {
        viewbox_w: svg_w,
        viewbox_h: svg_h,
        title_height: title_h,
        scale,
        circles: circle_map,
        areas: out_areas,
        theme_colors,
        title_text_color,
        set_text_color,
    })
}

// ---------------------------------------------------------------------------
// Internal types

#[derive(Debug, Clone)]
struct Area {
    sets: Vec<String>,
    size: f64,
    weight: f64,
}

// ---------------------------------------------------------------------------
// Geometry (circleintersection.js port)

fn distance(a: &Circle, b: &Circle) -> f64 {
    fsqrt((a.x - b.x).powi(2) + (a.y - b.y).powi(2))
}

fn dist_xy(a: (f64, f64), b: (f64, f64)) -> f64 {
    fsqrt((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2))
}

/// circular segment area: r² acos(1-w/r) - (r-w) sqrt(w(2r-w))
fn circle_area(r: f64, width: f64) -> f64 {
    r * r * facos(1.0 - width / r) - (r - width) * fsqrt(width * (2.0 * r - width))
}

fn circle_overlap(r1: f64, r2: f64, d: f64) -> f64 {
    if d >= r1 + r2 {
        return 0.0;
    }
    if d <= (r1 - r2).abs() {
        let m = r1.min(r2);
        return std::f64::consts::PI * m * m;
    }
    let w1 = r1 - (d * d - r2 * r2 + r1 * r1) / (2.0 * d);
    let w2 = r2 - (d * d - r1 * r1 + r2 * r2) / (2.0 * d);
    circle_area(r1, w1) + circle_area(r2, w2)
}

fn distance_from_intersect_area(r1: f64, r2: f64, overlap: f64) -> f64 {
    let m = r1.min(r2);
    if m * m * std::f64::consts::PI <= overlap + SMALL {
        return (r1 - r2).abs();
    }
    bisect(|d| circle_overlap(r1, r2, d) - overlap, 0.0, r1 + r2)
}

fn bisect<F: Fn(f64) -> f64>(f: F, mut a: f64, b: f64) -> f64 {
    let max_iters = 100;
    let tolerance = 1e-10_f64;
    let f_a = f(a);
    let f_b = f(b);
    let mut delta = b - a;
    if f_a * f_b > 0.0 {
        // Upstream throws; for our purposes return midpoint as a safe fallback.
        return (a + b) * 0.5;
    }
    if f_a == 0.0 {
        return a;
    }
    if f_b == 0.0 {
        return b;
    }
    for _ in 0..max_iters {
        delta /= 2.0;
        let mid = a + delta;
        let f_mid = f(mid);
        if f_mid * f_a >= 0.0 {
            a = mid;
        }
        if delta.abs() < tolerance || f_mid == 0.0 {
            return mid;
        }
    }
    a + delta
}

fn circle_circle_intersection(p1: &Circle, p2: &Circle) -> Vec<(f64, f64)> {
    let d = distance(p1, p2);
    let r1 = p1.radius;
    let r2 = p2.radius;
    if d >= r1 + r2 || d <= (r1 - r2).abs() {
        return Vec::new();
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h = fsqrt(r1 * r1 - a * a);
    let x0 = p1.x + a * (p2.x - p1.x) / d;
    let y0 = p1.y + a * (p2.y - p1.y) / d;
    let rx = -(p2.y - p1.y) * (h / d);
    let ry = -(p2.x - p1.x) * (h / d);
    vec![(x0 + rx, y0 - ry), (x0 - rx, y0 + ry)]
}

#[derive(Debug, Clone)]
struct InnerPt {
    x: f64,
    y: f64,
    parent_index: [usize; 2],
    angle: f64,
}

fn get_intersection_points(circles: &[Circle]) -> Vec<InnerPt> {
    let mut ret = Vec::new();
    for i in 0..circles.len() {
        for j in (i + 1)..circles.len() {
            let pts = circle_circle_intersection(&circles[i], &circles[j]);
            for (x, y) in pts {
                ret.push(InnerPt {
                    x,
                    y,
                    parent_index: [i, j],
                    angle: 0.0,
                });
            }
        }
    }
    ret
}

fn contained_in_circles(point: (f64, f64), circles: &[Circle]) -> bool {
    circles
        .iter()
        .all(|c| dist_xy(point, (c.x, c.y)) < c.radius + SMALL)
}

fn get_center(points: &[InnerPt]) -> (f64, f64) {
    let mut x = 0.0;
    let mut y = 0.0;
    for p in points {
        x += p.x;
        y += p.y;
    }
    let n = points.len() as f64;
    (x / n, y / n)
}

#[derive(Debug, Clone)]
pub struct Arc {
    pub circle: Circle,
    pub p1: (f64, f64),
    pub p2: (f64, f64),
    pub width: f64,
    pub large: bool,
    pub sweep: bool,
}

#[derive(Debug, Default)]
struct AreaStats {
    arcs: Vec<Arc>,
}

fn intersection_area(circles: &[Circle], stats: &mut AreaStats) -> f64 {
    let mut intersection_points = get_intersection_points(circles);

    let mut inner_points: Vec<InnerPt> = intersection_points
        .drain(..)
        .filter(|p| contained_in_circles((p.x, p.y), circles))
        .collect();

    let mut arc_area = 0.0;
    let mut polygon_area = 0.0;
    let mut arcs: Vec<Arc> = Vec::new();

    if inner_points.len() > 1 {
        let centre = get_center(&inner_points);
        for p in inner_points.iter_mut() {
            p.angle = fatan2(p.x - centre.0, p.y - centre.1);
        }
        // Sort descending by angle.
        inner_points.sort_by(|a, b| b.angle.partial_cmp(&a.angle).unwrap_or(std::cmp::Ordering::Equal));

        let n = inner_points.len();
        let mut p2 = inner_points[n - 1].clone();
        for i in 0..n {
            let p1 = inner_points[i].clone();
            polygon_area += (p2.x + p1.x) * (p1.y - p2.y);
            let mid = ((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
            let mut arc: Option<Arc> = None;
            for &pj in p1.parent_index.iter() {
                if p2.parent_index.contains(&pj) {
                    let circle = circles[pj];
                    let a1 = fatan2(p1.x - circle.x, p1.y - circle.y);
                    let a2 = fatan2(p2.x - circle.x, p2.y - circle.y);
                    let mut angle_diff = a2 - a1;
                    if angle_diff < 0.0 {
                        angle_diff += 2.0 * std::f64::consts::PI;
                    }
                    let a = a2 - angle_diff / 2.0;
                    let mut width = dist_xy(
                        mid,
                        (
                            circle.x + circle.radius * fsin(a),
                            circle.y + circle.radius * fcos(a),
                        ),
                    );
                    if width > circle.radius * 2.0 {
                        width = circle.radius * 2.0;
                    }
                    if arc.is_none() || arc.as_ref().unwrap().width > width {
                        arc = Some(Arc {
                            circle,
                            p1: (p1.x, p1.y),
                            p2: (p2.x, p2.y),
                            width,
                            large: width > circle.radius,
                            sweep: true,
                        });
                    }
                }
            }
            if let Some(a) = arc {
                arc_area += circle_area(a.circle.radius, a.width);
                arcs.push(a);
                p2 = p1;
            }
        }
    } else {
        // No intersection points: disjoint or fully contained.
        let mut smallest = circles[0];
        for c in circles.iter().skip(1) {
            if c.radius < smallest.radius {
                smallest = *c;
            }
        }
        let mut disjoint = false;
        for c in circles {
            if dist_xy((c.x, c.y), (smallest.x, smallest.y)) > (smallest.radius - c.radius).abs() {
                disjoint = true;
                break;
            }
        }
        if disjoint {
            arc_area = 0.0;
            polygon_area = 0.0;
        } else {
            arc_area = smallest.radius * smallest.radius * std::f64::consts::PI;
            arcs.push(Arc {
                circle: smallest,
                p1: (smallest.x, smallest.y + smallest.radius),
                p2: (smallest.x - SMALL, smallest.y + smallest.radius),
                width: smallest.radius * 2.0,
                large: true,
                sweep: true,
            });
        }
    }

    polygon_area /= 2.0;
    stats.arcs = arcs;
    arc_area + polygon_area
}

fn intersection_area_arcs(circles: &[Circle]) -> Vec<Arc> {
    if circles.is_empty() {
        return Vec::new();
    }
    let mut stats = AreaStats::default();
    intersection_area(circles, &mut stats);
    stats.arcs
}

fn arcs_to_path(arcs: &[Arc], round: Option<i32>) -> String {
    if arcs.is_empty() {
        return "M 0 0".into();
    }
    let r_factor = 10f64.powi(round.unwrap_or(0));
    let r = |v: f64| -> f64 {
        if round.is_some() {
            // V8 Math.round semantics — see fround() in this module.
            fround(v * r_factor) / r_factor
        } else {
            v
        }
    };
    if arcs.len() == 1 {
        let c = arcs[0].circle;
        // circlePath: M x y \n m -r 0 \n a r r 0 1 0 (2r) 0 \n a r r 0 1 0 (-2r) 0
        return format!(
            "\nM {} {} \nm {} 0 \na {} {} 0 1 0 {} 0 \na {} {} 0 1 0 {} 0",
            fmt_num(r(c.x)),
            fmt_num(r(c.y)),
            fmt_num(-r(c.radius)),
            fmt_num(r(c.radius)),
            fmt_num(r(c.radius)),
            fmt_num(r(c.radius) * 2.0),
            fmt_num(r(c.radius)),
            fmt_num(r(c.radius)),
            fmt_num(-r(c.radius) * 2.0),
        );
    }
    // multi-arc
    let mut out = format!("\nM {} {}", fmt_num(r(arcs[0].p2.0)), fmt_num(r(arcs[0].p2.1)));
    for arc in arcs {
        let radius = r(arc.circle.radius);
        out.push_str(&format!(
            " \nA {} {} 0 {} {} {} {}",
            fmt_num(radius),
            fmt_num(radius),
            if arc.large { 1 } else { 0 },
            if arc.sweep { 1 } else { 0 },
            fmt_num(r(arc.p1.0)),
            fmt_num(r(arc.p1.1)),
        ));
    }
    out
}

/// JS-style number formatting: Number.toString() — full precision
/// where needed but no trailing `.0` for integer values.
pub fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

// ---------------------------------------------------------------------------
// Greedy layout (layout.js port)

fn add_missing_areas(areas: &[Area]) -> Vec<Area> {
    // No `distinct` mode for venn-beta.
    let mut r: Vec<Area> = areas.to_vec();
    let mut ids = Vec::<String>::new();
    let mut pairs = std::collections::BTreeSet::<String>::new();
    for a in &r {
        if a.sets.len() == 1 {
            ids.push(a.sets[0].clone());
        } else if a.sets.len() == 2 {
            pairs.insert(format!("{};{}", a.sets[0], a.sets[1]));
            pairs.insert(format!("{};{}", a.sets[1], a.sets[0]));
        }
    }
    ids.sort();

    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let key = format!("{};{}", ids[i], ids[j]);
            if !pairs.contains(&key) {
                r.push(Area {
                    sets: vec![ids[i].clone(), ids[j].clone()],
                    size: 0.0,
                    weight: 1.0,
                });
            }
        }
    }
    r
}

/// Insertion-ordered `setid -> Circle` map. Behaves like JS object key order.
#[derive(Debug, Clone, Default)]
struct OrderedMap {
    keys: Vec<String>,
    values: Vec<Circle>,
}

impl OrderedMap {
    fn new() -> Self {
        Self { keys: Vec::new(), values: Vec::new() }
    }
    fn insert(&mut self, k: String, v: Circle) {
        if let Some(idx) = self.keys.iter().position(|x| x == &k) {
            self.values[idx] = v;
        } else {
            self.keys.push(k);
            self.values.push(v);
        }
    }
    fn get(&self, k: &str) -> Option<&Circle> {
        self.keys.iter().position(|x| x == k).map(|i| &self.values[i])
    }
    fn get_mut(&mut self, k: &str) -> Option<&mut Circle> {
        self.keys.iter().position(|x| x == k).map(|i| &mut self.values[i])
    }
    fn keys(&self) -> impl Iterator<Item = &String> {
        self.keys.iter()
    }
    fn iter(&self) -> impl Iterator<Item = (&String, &Circle)> {
        self.keys.iter().zip(self.values.iter())
    }
    fn len(&self) -> usize {
        self.keys.len()
    }
}

fn greedy_layout(areas: &[Area]) -> OrderedMap {
    // Per-set: (Circle, size). Use parallel vectors to preserve insertion order.
    let mut order: Vec<String> = Vec::new();
    let mut circles_data: Vec<(Circle, f64)> = Vec::new();
    let mut set_overlaps: BTreeMap<String, Vec<(String, f64, f64)>> = BTreeMap::new();
    for area in areas {
        if area.sets.len() == 1 {
            let setid = &area.sets[0];
            let r = fsqrt(area.size / std::f64::consts::PI);
            if !order.iter().any(|x| x == setid) {
                order.push(setid.clone());
                circles_data.push((
                    Circle { x: 1e10, y: 1e10, radius: r },
                    area.size,
                ));
            }
            set_overlaps.entry(setid.clone()).or_default();
        }
    }
    // Helper: get size by setid.
    let size_of = |s: &str| -> f64 {
        let i = order.iter().position(|x| x == s).unwrap();
        circles_data[i].1
    };

    let pairs: Vec<&Area> = areas.iter().filter(|a| a.sets.len() == 2).collect();

    for current in &pairs {
        let mut weight = current.weight;
        let left = &current.sets[0];
        let right = &current.sets[1];
        let lsz = size_of(left);
        let rsz = size_of(right);
        if current.size + SMALL >= lsz.min(rsz) {
            weight = 0.0;
        }
        set_overlaps
            .entry(left.clone())
            .or_default()
            .push((right.clone(), current.size, weight));
        set_overlaps
            .entry(right.clone())
            .or_default()
            .push((left.clone(), current.size, weight));
    }

    // Most-overlapped: walk in insertion order so we match V8's
    // `Object.keys(setOverlaps)` semantics (the keys were inserted as
    // sets are first encountered, which is the same order as `order`).
    let mut most_overlapped: Vec<(String, f64)> = order
        .iter()
        .map(|s| {
            let total: f64 = set_overlaps[s]
                .iter()
                .map(|(_, sz, w)| sz * w)
                .sum();
            (s.clone(), total)
        })
        .collect();

    // Stable sort descending by size.
    most_overlapped.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut positioned: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut output = OrderedMap::new();
    for (i, setid) in order.iter().enumerate() {
        output.insert(setid.clone(), circles_data[i].0);
    }

    if most_overlapped.is_empty() {
        return output;
    }

    // First set at (0, 0).
    let first = most_overlapped[0].0.clone();
    {
        let c = output.get_mut(&first).unwrap();
        c.x = 0.0;
        c.y = 0.0;
    }
    positioned.insert(first);

    for i in 1..most_overlapped.len() {
        let setid = most_overlapped[i].0.clone();
        let mut overlap: Vec<(String, f64, f64)> = set_overlaps[&setid]
            .iter()
            .filter(|(s, _, _)| positioned.contains(s))
            .cloned()
            .collect();
        overlap.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if overlap.is_empty() {
            continue;
        }

        let set_radius = output.get(&setid).unwrap().radius;

        let mut points: Vec<(f64, f64)> = Vec::new();
        for j in 0..overlap.len() {
            let p1 = *output.get(&overlap[j].0).unwrap();
            let d1 = distance_from_intersect_area(set_radius, p1.radius, overlap[j].1);
            points.push((p1.x + d1, p1.y));
            points.push((p1.x - d1, p1.y));
            points.push((p1.x, p1.y + d1));
            points.push((p1.x, p1.y - d1));
            for k in (j + 1)..overlap.len() {
                let p2 = *output.get(&overlap[k].0).unwrap();
                let d2 = distance_from_intersect_area(set_radius, p2.radius, overlap[k].1);
                let extra = circle_circle_intersection(
                    &Circle { x: p1.x, y: p1.y, radius: d1 },
                    &Circle { x: p2.x, y: p2.y, radius: d2 },
                );
                points.extend(extra);
            }
        }

        let mut best_loss = 1e50_f64;
        let mut best_pt = points[0];
        // Upstream `greedyLayout` mutates `areas` to pair-only at the very
        // top of the function (`areas = areas.filter(a => a.sets.length === 2)`),
        // so the loss probe at each candidate position only counts pairwise
        // intersection mismatches. Three-or-more set areas are NEVER included.
        //
        // Without this filter, three-circle layouts that are symmetric in y
        // (the AB axis lies on y=0 by construction) develop a tie-break
        // disagreement: the mirror-image candidates `(x, +h)` and `(x, -h)`
        // have identical pairwise loss but slightly different triple-area
        // contributions, so the FIRST candidate to win the tie depends on
        // ULP-level differences in `intersectionArea` for the triple. JS
        // never sees that contribution → both mirrors tie at e-20 → the
        // first candidate (-h) wins; the unfiltered Rust version saw it →
        // the second candidate (+h) wins → the simplex starts on the
        // mirror side and converges to the mirrored solution, which then
        // breaks byte-exact parity for every coordinate downstream.
        let pair_areas: Vec<&Area> = areas.iter().filter(|a| a.sets.len() == 2).collect();
        for p in &points {
            {
                let cur = output.get_mut(&setid).unwrap();
                cur.x = p.0;
                cur.y = p.1;
            }
            let snapshot: BTreeMap<String, Circle> = output.iter().map(|(k, v)| (k.clone(), *v)).collect();
            let mut l = 0.0_f64;
            for a in &pair_areas {
                let lc = &snapshot[&a.sets[0]];
                let rc = &snapshot[&a.sets[1]];
                let overlap = circle_overlap(lc.radius, rc.radius, distance(lc, rc));
                l += a.weight * (overlap - a.size).powi(2);
            }
            if l < best_loss {
                best_loss = l;
                best_pt = *p;
            }
        }
        let cur = output.get_mut(&setid).unwrap();
        cur.x = best_pt.0;
        cur.y = best_pt.1;
        positioned.insert(setid);
    }
    output
}

fn loss_function(circles: &BTreeMap<String, Circle>, areas: &[Area]) -> f64 {
    loss_function_areas(circles, areas)
}

fn loss_function_areas(circles: &BTreeMap<String, Circle>, areas: &[Area]) -> f64 {
    let mut output = 0.0_f64;
    for area in areas {
        if area.sets.len() == 1 {
            continue;
        }
        let overlap = if area.sets.len() == 2 {
            let l = &circles[&area.sets[0]];
            let r = &circles[&area.sets[1]];
            circle_overlap(l.radius, r.radius, distance(l, r))
        } else {
            let cs: Vec<Circle> = area.sets.iter().map(|s| circles[s]).collect();
            let mut stats = AreaStats::default();
            intersection_area(&cs, &mut stats)
        };
        let weight = area.weight;
        output += weight * (overlap - area.size).powi(2);
    }
    output
}

// ---------------------------------------------------------------------------
// Nelder-Mead (fmin port, deterministic)

#[derive(Debug, Clone)]
pub struct NelderMeadResult {
    pub x: Vec<f64>,
    pub fx: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct NelderMeadParams {
    pub max_iterations: Option<usize>,
    pub non_zero_delta: f64,
    pub zero_delta: f64,
    pub min_error_delta: f64,
    pub min_tolerance: f64,
    pub rho: f64,
    pub chi: f64,
    pub psi: f64,
    pub sigma: f64,
}

impl Default for NelderMeadParams {
    fn default() -> Self {
        Self {
            max_iterations: None,
            non_zero_delta: 1.05,
            zero_delta: 1e-3,
            min_error_delta: 1e-6,
            // Bug-for-bug parity with upstream: `minTolerance`
            // is initialised from `parameters.minErrorDelta || 1e-5`,
            // which means it ALWAYS equals `min_error_delta` once the
            // user provides one (or 1e-5 fallback when neither is set).
            min_tolerance: 1e-5,
            rho: 1.0,
            chi: 2.0,
            psi: -0.5,
            sigma: 0.5,
        }
    }
}

pub fn nelder_mead<F>(f: F, x0: &[f64], params: NelderMeadParams) -> NelderMeadResult
where
    F: Fn(&[f64]) -> f64,
{
    let n = x0.len();
    let max_iters = params.max_iterations.unwrap_or(n * 200);

    // Match upstream: `minTolerance = parameters.minErrorDelta || 1e-5`.
    let min_tolerance = params.min_tolerance;

    // Build simplex of N+1 points.
    let mut simplex: Vec<Vec<f64>> = Vec::with_capacity(n + 1);
    simplex.push(x0.to_vec());
    let mut fx_vec: Vec<f64> = vec![f(x0)];

    for i in 0..n {
        let mut p = x0.to_vec();
        p[i] = if p[i] != 0.0 { p[i] * params.non_zero_delta } else { params.zero_delta };
        let val = f(&p);
        simplex.push(p);
        fx_vec.push(val);
    }

    let mut centroid = vec![0.0; n];
    let mut reflected = vec![0.0; n];
    let mut contracted = vec![0.0; n];
    let mut expanded = vec![0.0; n];

    for _ in 0..max_iters {
        // Sort simplex ascending by fx.
        let mut order: Vec<usize> = (0..=n).collect();
        order.sort_by(|a, b| fx_vec[*a].partial_cmp(&fx_vec[*b]).unwrap_or(std::cmp::Ordering::Equal));
        let new_simplex: Vec<Vec<f64>> = order.iter().map(|&i| simplex[i].clone()).collect();
        let new_fx: Vec<f64> = order.iter().map(|&i| fx_vec[i]).collect();
        simplex = new_simplex;
        fx_vec = new_fx;

        let mut max_diff = 0.0_f64;
        for i in 0..n {
            let d = (simplex[0][i] - simplex[1][i]).abs();
            if d > max_diff {
                max_diff = d;
            }
        }
        if (fx_vec[0] - fx_vec[n]).abs() < params.min_error_delta && max_diff < min_tolerance {
            break;
        }

        // centroid of all but worst.
        for i in 0..n {
            centroid[i] = 0.0;
            for j in 0..n {
                centroid[i] += simplex[j][i];
            }
            centroid[i] /= n as f64;
        }

        let worst = simplex[n].clone();

        // reflected = (1+rho)*centroid + (-rho)*worst
        weighted_sum(&mut reflected, 1.0 + params.rho, &centroid, -params.rho, &worst);
        let fx_reflected = f(&reflected);

        if fx_reflected < fx_vec[0] {
            // expand
            weighted_sum(&mut expanded, 1.0 + params.chi, &centroid, -params.chi, &worst);
            let fx_expanded = f(&expanded);
            if fx_expanded < fx_reflected {
                simplex[n] = expanded.clone();
                fx_vec[n] = fx_expanded;
            } else {
                simplex[n] = reflected.clone();
                fx_vec[n] = fx_reflected;
            }
        } else if fx_reflected >= fx_vec[n - 1] {
            let mut should_reduce = false;
            if fx_reflected > fx_vec[n] {
                weighted_sum(&mut contracted, 1.0 + params.psi, &centroid, -params.psi, &worst);
                let fx_contracted = f(&contracted);
                if fx_contracted < fx_vec[n] {
                    simplex[n] = contracted.clone();
                    fx_vec[n] = fx_contracted;
                } else {
                    should_reduce = true;
                }
            } else {
                weighted_sum(
                    &mut contracted,
                    1.0 - params.psi * params.rho,
                    &centroid,
                    params.psi * params.rho,
                    &worst,
                );
                let fx_contracted = f(&contracted);
                if fx_contracted < fx_reflected {
                    simplex[n] = contracted.clone();
                    fx_vec[n] = fx_contracted;
                } else {
                    should_reduce = true;
                }
            }
            if should_reduce {
                if params.sigma >= 1.0 {
                    break;
                }
                for i in 1..=n {
                    let s0 = simplex[0].clone();
                    let mut new_pt = vec![0.0; n];
                    weighted_sum(&mut new_pt, 1.0 - params.sigma, &s0, params.sigma, &simplex[i]);
                    simplex[i] = new_pt;
                    fx_vec[i] = f(&simplex[i]);
                }
            }
        } else {
            simplex[n] = reflected.clone();
            fx_vec[n] = fx_reflected;
        }
    }

    // Sort once more for return.
    let mut order: Vec<usize> = (0..=n).collect();
    order.sort_by(|a, b| fx_vec[*a].partial_cmp(&fx_vec[*b]).unwrap_or(std::cmp::Ordering::Equal));
    let best = order[0];
    NelderMeadResult {
        x: simplex[best].clone(),
        fx: fx_vec[best],
    }
}

fn weighted_sum(ret: &mut [f64], w1: f64, v1: &[f64], w2: f64, v2: &[f64]) {
    for j in 0..ret.len() {
        ret[j] = w1 * v1[j] + w2 * v2[j];
    }
}

// ---------------------------------------------------------------------------
// Normalize / scale (layout.js port)

#[derive(Debug, Clone)]
struct ClusterCircle {
    setid: String,
    x: f64,
    y: f64,
    radius: f64,
}

fn normalize_solution(
    solution: &OrderedMap,
    orientation: f64,
) -> Vec<ClusterCircle> {
    let mut circles: Vec<ClusterCircle> = solution
        .iter()
        .map(|(k, c)| ClusterCircle {
            setid: k.clone(),
            x: c.x,
            y: c.y,
            radius: c.radius,
        })
        .collect();

    // Disjoint clusters via union-find.
    let n = circles.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], i: usize) -> usize {
        if parent[i] != i {
            let p = parent[i];
            let r = find(parent, p);
            parent[i] = r;
        }
        parent[i]
    }
    for i in 0..n {
        for j in (i + 1)..n {
            let max_d = circles[i].radius + circles[j].radius;
            let d = fsqrt(
                (circles[i].x - circles[j].x).powi(2)
                    + (circles[i].y - circles[j].y).powi(2),
            );
            if d + 1e-10 < max_d {
                let ri = find(&mut parent, i);
                let rj = find(&mut parent, j);
                parent[ri] = rj;
            }
        }
    }
    // Group by root, preserving first-encounter order of root indices.
    let mut clusters_by_root: Vec<(usize, Vec<ClusterCircle>)> = Vec::new();
    for (i, c) in circles.iter().enumerate() {
        let r = find(&mut parent, i);
        match clusters_by_root.iter_mut().find(|(rr, _)| *rr == r) {
            Some((_, list)) => list.push(c.clone()),
            None => clusters_by_root.push((r, vec![c.clone()])),
        }
    }

    // Orient each cluster.
    let mut clusters: Vec<(Vec<ClusterCircle>, BoundingBox, f64)> = Vec::new();
    for (_, mut cluster) in clusters_by_root {
        orientate_circles(&mut cluster, orientation);
        let bounds = bounding_box(&cluster);
        let size = (bounds.x_max - bounds.x_min) * (bounds.y_max - bounds.y_min);
        clusters.push((cluster, bounds, size));
    }
    // Sort largest first.
    clusters.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let (mut result, mut return_bounds, _size) = clusters.remove(0);
    let spacing = (return_bounds.x_max - return_bounds.x_min) / 50.0;

    let mut idx = 0;
    while idx < clusters.len() {
        // Replicate upstream's pattern verbatim:
        //   addCluster(clusters[idx],   true,  false);
        //   addCluster(clusters[idx+1], false, true);
        //   addCluster(clusters[idx+2], true,  true);
        if idx < clusters.len() {
            let c0 = clusters[idx].clone();
            add_cluster(&mut result, &mut return_bounds, &c0, true, false, spacing);
        }
        if idx + 1 < clusters.len() {
            let c1 = clusters[idx + 1].clone();
            add_cluster(&mut result, &mut return_bounds, &c1, false, true, spacing);
        }
        if idx + 2 < clusters.len() {
            let c2 = clusters[idx + 2].clone();
            add_cluster(&mut result, &mut return_bounds, &c2, true, true, spacing);
        }
        idx += 3;
        return_bounds = bounding_box(&result);
    }

    result
}

fn add_cluster(
    result: &mut Vec<ClusterCircle>,
    return_bounds: &mut BoundingBox,
    cluster_tuple: &(Vec<ClusterCircle>, BoundingBox, f64),
    right: bool,
    bottom: bool,
    spacing: f64,
) {
    let cluster = &cluster_tuple.0;
    let bounds = &cluster_tuple.1;
    let mut x_offset;
    let mut y_offset;
    if right {
        x_offset = return_bounds.x_max - bounds.x_min + spacing;
    } else {
        x_offset = return_bounds.x_max - bounds.x_max;
        let centring = (bounds.x_max - bounds.x_min) / 2.0
            - (return_bounds.x_max - return_bounds.x_min) / 2.0;
        if centring < 0.0 {
            x_offset += centring;
        }
    }
    if bottom {
        y_offset = return_bounds.y_max - bounds.y_min + spacing;
    } else {
        y_offset = return_bounds.y_max - bounds.y_max;
        let centring = (bounds.y_max - bounds.y_min) / 2.0
            - (return_bounds.y_max - return_bounds.y_min) / 2.0;
        if centring < 0.0 {
            y_offset += centring;
        }
    }
    for c in cluster {
        result.push(ClusterCircle {
            setid: c.setid.clone(),
            x: c.x + x_offset,
            y: c.y + y_offset,
            radius: c.radius,
        });
    }
    *return_bounds = bounding_box(result);
}

#[derive(Debug, Clone, Copy)]
struct BoundingBox {
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
}

fn bounding_box(cluster: &[ClusterCircle]) -> BoundingBox {
    let mut x_max = f64::NEG_INFINITY;
    let mut x_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    for c in cluster {
        if c.x + c.radius > x_max {
            x_max = c.x + c.radius;
        }
        if c.x - c.radius < x_min {
            x_min = c.x - c.radius;
        }
        if c.y + c.radius > y_max {
            y_max = c.y + c.radius;
        }
        if c.y - c.radius < y_min {
            y_min = c.y - c.radius;
        }
    }
    BoundingBox { x_min, x_max, y_min, y_max }
}

fn orientate_circles(circles: &mut Vec<ClusterCircle>, orientation: f64) {
    // Sort largest radius first. Note: Array.sort in V8 is stable, so
    // ties resolve by original (insertion) order.
    circles.sort_by(|a, b| b.radius.partial_cmp(&a.radius).unwrap_or(std::cmp::Ordering::Equal));

    if circles.is_empty() {
        return;
    }
    let lx = circles[0].x;
    let ly = circles[0].y;
    for c in circles.iter_mut() {
        c.x -= lx;
        c.y -= ly;
    }

    if circles.len() == 2 {
        let dx = circles[0].x - circles[1].x;
        let dy = circles[0].y - circles[1].y;
        let d = fsqrt(dx * dx + dy * dy);
        if d < (circles[1].radius - circles[0].radius).abs() {
            circles[1].x = circles[0].x + circles[0].radius - circles[1].radius - 1e-10;
            circles[1].y = circles[0].y;
        }
    }

    if circles.len() > 1 {
        let rotation = fatan2(circles[1].x, circles[1].y) - orientation;
        let cos_r = fcos(rotation);
        let sin_r = fsin(rotation);
        for c in circles.iter_mut() {
            let x = c.x;
            let y = c.y;
            c.x = cos_r * x - sin_r * y;
            c.y = sin_r * x + cos_r * y;
        }
    }

    if circles.len() > 2 {
        let mut angle = fatan2(circles[2].x, circles[2].y) - orientation;
        while angle < 0.0 {
            angle += 2.0 * std::f64::consts::PI;
        }
        while angle > 2.0 * std::f64::consts::PI {
            angle -= 2.0 * std::f64::consts::PI;
        }
        if angle > std::f64::consts::PI {
            let slope = circles[1].y / (1e-10 + circles[1].x);
            for c in circles.iter_mut() {
                let d = (c.x + slope * c.y) / (1.0 + slope * slope);
                c.x = 2.0 * d - c.x;
                c.y = 2.0 * d * slope - c.y;
            }
        }
    }
}

fn scale_solution(
    solution: &[ClusterCircle],
    width: f64,
    height: f64,
    padding: f64,
) -> BTreeMap<String, Circle> {
    let w = width - 2.0 * padding;
    let h = height - 2.0 * padding;
    let bounds = bounding_box(solution);
    if bounds.x_max == bounds.x_min || bounds.y_max == bounds.y_min {
        let mut out = BTreeMap::new();
        for c in solution {
            out.insert(
                c.setid.clone(),
                Circle {
                    x: c.x,
                    y: c.y,
                    radius: c.radius,
                },
            );
        }
        return out;
    }
    let x_scaling = w / (bounds.x_max - bounds.x_min);
    let y_scaling = h / (bounds.y_max - bounds.y_min);
    let scaling = x_scaling.min(y_scaling);
    let x_off = (w - (bounds.x_max - bounds.x_min) * scaling) / 2.0;
    let y_off = (h - (bounds.y_max - bounds.y_min) * scaling) / 2.0;
    let mut out = BTreeMap::new();
    for c in solution {
        out.insert(
            c.setid.clone(),
            Circle {
                radius: scaling * c.radius,
                x: padding + x_off + (c.x - bounds.x_min) * scaling,
                y: padding + y_off + (c.y - bounds.y_min) * scaling,
            },
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Text-centre computation (diagram.js port).

fn circle_margin(
    current: (f64, f64),
    interior: &[Circle],
    exterior: &[Circle],
) -> f64 {
    let mut margin = interior[0].radius - dist_xy((interior[0].x, interior[0].y), current);
    for i in 1..interior.len() {
        let m = interior[i].radius - dist_xy((interior[i].x, interior[i].y), current);
        if m <= margin {
            margin = m;
        }
    }
    for e in exterior {
        let m = dist_xy((e.x, e.y), current) - e.radius;
        if m <= margin {
            margin = m;
        }
    }
    margin
}

fn compute_text_centre_one(interior: &[Circle], exterior: &[Circle]) -> (f64, f64, bool) {
    let mut points: Vec<(f64, f64)> = Vec::new();
    for c in interior {
        points.push((c.x, c.y));
        points.push((c.x + c.radius / 2.0, c.y));
        points.push((c.x - c.radius / 2.0, c.y));
        points.push((c.x, c.y + c.radius / 2.0));
        points.push((c.x, c.y - c.radius / 2.0));
    }
    let mut initial = points[0];
    let mut margin = circle_margin(initial, interior, exterior);
    for &p in points.iter().skip(1) {
        let m = circle_margin(p, interior, exterior);
        if m >= margin {
            initial = p;
            margin = m;
        }
    }

    // Maximize margin via nelderMead.
    let interior_owned: Vec<Circle> = interior.to_vec();
    let exterior_owned: Vec<Circle> = exterior.to_vec();
    let res = nelder_mead(
        move |p| -circle_margin((p[0], p[1]), &interior_owned, &exterior_owned),
        &[initial.0, initial.1],
        NelderMeadParams {
            max_iterations: Some(500),
            min_error_delta: 1e-10,
            min_tolerance: 1e-10,
            ..NelderMeadParams::default()
        },
    );
    let ret = (res.x[0], res.x[1]);

    let mut valid = true;
    for i in interior {
        if dist_xy(ret, (i.x, i.y)) > i.radius {
            valid = false;
            break;
        }
    }
    if valid {
        for e in exterior {
            if dist_xy(ret, (e.x, e.y)) < e.radius {
                valid = false;
                break;
            }
        }
    }
    if valid {
        return (ret.0, ret.1, false);
    }

    if interior.len() == 1 {
        return (interior[0].x, interior[0].y, false);
    }
    let mut stats = AreaStats::default();
    intersection_area(interior, &mut stats);
    if stats.arcs.is_empty() {
        return (0.0, -1000.0, true); // disjoint
    }
    if stats.arcs.len() == 1 {
        return (stats.arcs[0].circle.x, stats.arcs[0].circle.y, false);
    }
    if !exterior.is_empty() {
        return compute_text_centre_one(interior, &[]);
    }
    // Average of arc.p1 points.
    let mut sx = 0.0;
    let mut sy = 0.0;
    for a in &stats.arcs {
        sx += a.p1.0;
        sy += a.p1.1;
    }
    let n = stats.arcs.len() as f64;
    (sx / n, sy / n, false)
}

fn get_overlapping_circles(
    circles: &BTreeMap<String, Circle>,
) -> BTreeMap<String, Vec<String>> {
    let mut ret: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let ids: Vec<String> = circles.keys().cloned().collect();
    for id in &ids {
        ret.insert(id.clone(), Vec::new());
    }
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let a = &circles[&ids[i]];
            let b = &circles[&ids[j]];
            let d = distance(a, b);
            if d + b.radius <= a.radius + 1e-10 {
                ret.get_mut(&ids[j]).unwrap().push(ids[i].clone());
            } else if d + a.radius <= b.radius + 1e-10 {
                ret.get_mut(&ids[i]).unwrap().push(ids[j].clone());
            }
        }
    }
    ret
}

fn compute_text_centres(
    circles: &BTreeMap<String, Circle>,
    areas: &[Area],
) -> BTreeMap<String, (f64, f64)> {
    let mut ret = BTreeMap::new();
    let overlapped = get_overlapping_circles(circles);
    for area in areas {
        let mut areaids = std::collections::BTreeSet::new();
        let mut exclude = std::collections::BTreeSet::new();
        for s in &area.sets {
            areaids.insert(s.clone());
            if let Some(list) = overlapped.get(s) {
                for x in list {
                    exclude.insert(x.clone());
                }
            }
        }
        let mut interior: Vec<Circle> = Vec::new();
        let mut exterior: Vec<Circle> = Vec::new();
        for (id, c) in circles {
            if areaids.contains(id) {
                interior.push(*c);
            } else if !exclude.contains(id) {
                exterior.push(*c);
            }
        }
        if interior.is_empty() {
            continue;
        }
        let centre = compute_text_centre_one(&interior, &exterior);
        ret.insert(area.sets.join(","), (centre.0, centre.1));
    }
    ret
}
