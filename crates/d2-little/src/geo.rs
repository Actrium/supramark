// d2-geo: geometry primitives for d2 diagram layout engine.
//
// Ported from Go: /ext/d2/lib/geo/

use std::f64::consts::PI;
use std::fmt;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Precision threshold for floating-point comparisons.
pub const PRECISION: f64 = 0.0001;

// ---------------------------------------------------------------------------
// Math helpers
// ---------------------------------------------------------------------------

/// Euclidean distance between two points (x1,y1) and (x2,y2).
pub fn euclidean_distance(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    if x1 == x2 {
        (y1 - y2).abs()
    } else if y1 == y2 {
        (x1 - x2).abs()
    } else {
        ((x1 - x2).powi(2) + (y1 - y2).powi(2)).sqrt()
    }
}

/// Compare `a` and `b`, treating them as equal when their difference is less than `e`.
pub fn precision_compare(a: f64, b: f64, e: f64) -> i32 {
    if (a - b).abs() < e {
        0
    } else if a < b {
        -1
    } else {
        1
    }
}

/// Truncate a float to 3 decimal places (integer truncation, NOT rounding).
///
/// Matches the Go implementation: `float64(int(v*1000)) / 1000`.
pub fn truncate_decimals(v: f64) -> f64 {
    (v * 1000.0) as i64 as f64 / 1000.0
}

/// Sign of a float: -1, 0, or 1.
pub fn sign(i: f64) -> i32 {
    if i < 0.0 {
        -1
    } else if i > 0.0 {
        1
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Orientation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Orientation {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Top,
    Right,
    Bottom,
    Left,
    None,
}

impl Orientation {
    pub fn same_side(self, other: Orientation) -> bool {
        let sides: &[&[Orientation]] = &[
            &[
                Orientation::TopLeft,
                Orientation::Top,
                Orientation::TopRight,
            ],
            &[
                Orientation::BottomLeft,
                Orientation::Bottom,
                Orientation::BottomRight,
            ],
            &[
                Orientation::Left,
                Orientation::TopLeft,
                Orientation::BottomLeft,
            ],
            &[
                Orientation::Right,
                Orientation::TopRight,
                Orientation::BottomRight,
            ],
        ];
        for group in sides {
            if group.contains(&self) && group.contains(&other) {
                return true;
            }
        }
        false
    }

    pub fn is_diagonal(self) -> bool {
        matches!(
            self,
            Orientation::TopLeft
                | Orientation::TopRight
                | Orientation::BottomLeft
                | Orientation::BottomRight
        )
    }

    pub fn is_horizontal(self) -> bool {
        matches!(self, Orientation::Left | Orientation::Right)
    }

    pub fn is_vertical(self) -> bool {
        matches!(self, Orientation::Top | Orientation::Bottom)
    }

    pub fn get_opposite(self) -> Orientation {
        match self {
            Orientation::TopLeft => Orientation::BottomRight,
            Orientation::TopRight => Orientation::BottomLeft,
            Orientation::BottomLeft => Orientation::TopRight,
            Orientation::BottomRight => Orientation::TopLeft,
            Orientation::Top => Orientation::Bottom,
            Orientation::Bottom => Orientation::Top,
            Orientation::Right => Orientation::Left,
            Orientation::Left => Orientation::Right,
            Orientation::None => Orientation::None,
        }
    }
}

impl fmt::Display for Orientation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Orientation::TopLeft => "TopLeft",
            Orientation::TopRight => "TopRight",
            Orientation::BottomLeft => "BottomLeft",
            Orientation::BottomRight => "BottomRight",
            Orientation::Top => "Top",
            Orientation::Right => "Right",
            Orientation::Bottom => "Bottom",
            Orientation::Left => "Left",
            Orientation::None => "",
        };
        f.write_str(s)
    }
}

// ---------------------------------------------------------------------------
// Spacing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Spacing {
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
}

// ---------------------------------------------------------------------------
// RelativePoint
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelativePoint {
    pub x_percentage: f64,
    pub y_percentage: f64,
}

impl RelativePoint {
    pub fn new(x_percentage: f64, y_percentage: f64) -> Self {
        Self {
            x_percentage: truncate_decimals(x_percentage),
            y_percentage: truncate_decimals(y_percentage),
        }
    }
}

// ---------------------------------------------------------------------------
// Point
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn equals(&self, other: &Point) -> bool {
        self.x == other.x && self.y == other.y
    }

    pub fn compare(&self, other: &Point) -> i32 {
        let x_cmp = sign(self.x - other.x);
        if x_cmp == 0 {
            sign(self.y - other.y)
        } else {
            x_cmp
        }
    }

    pub fn copy(&self) -> Self {
        *self
    }

    /// Get orientation of `self` relative to `to`.
    /// E.g. if self is to the left of `to`, returns `Left`.
    pub fn get_orientation(&self, to: &Point) -> Orientation {
        if self.y < to.y {
            if self.x < to.x {
                return Orientation::TopLeft;
            }
            if self.x > to.x {
                return Orientation::TopRight;
            }
            return Orientation::Top;
        }

        if self.y > to.y {
            if self.x < to.x {
                return Orientation::BottomLeft;
            }
            if self.x > to.x {
                return Orientation::BottomRight;
            }
            return Orientation::Bottom;
        }

        if self.x < to.x {
            return Orientation::Left;
        }
        if self.x > to.x {
            return Orientation::Right;
        }

        Orientation::None
    }

    /// Shortest distance from this point to the line segment (p1, p2).
    pub fn distance_to_line(&self, p1: &Point, p2: &Point) -> f64 {
        let a = self.x - p1.x;
        let b = self.y - p1.y;
        let c = p2.x - p1.x;
        let d = p2.y - p1.y;

        let dot = a * c + b * d;
        let len_sq = c * c + d * d;

        let param = if len_sq != 0.0 { dot / len_sq } else { -1.0 };

        let (xx, yy) = if param < 0.0 {
            (p1.x, p1.y)
        } else if param > 1.0 {
            (p2.x, p2.y)
        } else {
            (p1.x + param * c, p1.y + param * d)
        };

        let dx = self.x - xx;
        let dy = self.y - yy;
        (dx * dx + dy * dy).sqrt()
    }

    /// Move this point by the given vector.
    pub fn add_vector(&self, v: &Vector) -> Point {
        self.to_vector().add(v).to_point()
    }

    /// Create a vector from `self` to `endpoint`.
    pub fn vector_to(&self, endpoint: &Point) -> Vector {
        endpoint.to_vector().minus(&self.to_vector())
    }

    pub fn formatted_coordinates(&self) -> String {
        format!("{},{}", self.x as i64, self.y as i64)
    }

    /// Returns true if point is on the orthogonal segment between a and b.
    pub fn on_orthogonal_segment(&self, a: &Point, b: &Point) -> bool {
        if a.x < b.x {
            if self.x < a.x || b.x < self.x {
                return false;
            }
        } else if self.x < b.x || a.x < self.x {
            return false;
        }
        if a.y < b.y {
            if self.y < a.y || b.y < self.y {
                return false;
            }
        } else if self.y < b.y || a.y < self.y {
            return false;
        }
        true
    }

    /// Create a vector pointing to this point (from origin).
    pub fn to_vector(&self) -> Vector {
        Vector(vec![self.x, self.y])
    }

    pub fn transpose(&mut self) {
        std::mem::swap(&mut self.x, &mut self.y);
    }

    /// Point t% of the way between self and b.
    pub fn interpolate(&self, b: &Point, t: f64) -> Point {
        Point::new(self.x * (1.0 - t) + b.x * t, self.y * (1.0 - t) + b.y * t)
    }

    pub fn truncate_float32(&mut self) {
        self.x = self.x as f32 as f64;
        self.y = self.y as f32 as f64;
    }

    pub fn truncate_decimals(&mut self) {
        self.x = truncate_decimals(self.x);
        self.y = truncate_decimals(self.y);
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

// ---------------------------------------------------------------------------
// Points (collection helpers)
// ---------------------------------------------------------------------------

/// Check if two point slices are equal (unordered, set-like comparison).
pub fn points_equals(ps: &[Point], other: &[Point]) -> bool {
    use std::collections::HashSet;

    #[derive(Hash, Eq, PartialEq)]
    struct HashablePoint(u64, u64);

    let set: HashSet<HashablePoint> = ps
        .iter()
        .map(|p| HashablePoint(p.x.to_bits(), p.y.to_bits()))
        .collect();
    other
        .iter()
        .all(|p| set.contains(&HashablePoint(p.x.to_bits(), p.y.to_bits())))
}

/// Get the median point from a slice of points.
pub fn points_get_median(ps: &[Point]) -> Point {
    let mut xs: Vec<f64> = ps.iter().map(|p| p.x).collect();
    let mut ys: Vec<f64> = ps.iter().map(|p| p.y).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mid = xs.len() / 2;
    let mut median_x = xs[mid];
    let mut median_y = ys[mid];

    if xs.len().is_multiple_of(2) {
        median_x = (median_x + xs[mid - 1]) / 2.0;
        median_y = (median_y + ys[mid - 1]) / 2.0;
    }

    Point::new(median_x, median_y)
}

/// Format a slice of points as a string.
pub fn points_to_string(points: &[Point]) -> String {
    points
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Remove points at indices where `to_remove[i]` is true.
pub fn remove_points(points: &[Point], to_remove: &[bool]) -> Vec<Point> {
    points
        .iter()
        .enumerate()
        .filter(|(i, _)| !to_remove[*i])
        .map(|(_, p)| *p)
        .collect()
}

// ---------------------------------------------------------------------------
// Intersection (line segment -- line segment)
// ---------------------------------------------------------------------------

/// Get the intersection point of segments (u0,u1) and (v0,v1), or None.
pub fn intersection_point(u0: &Point, u1: &Point, v0: &Point, v1: &Point) -> Option<Point> {
    let udx = u1.x - u0.x;
    let vdx = v1.x - v0.x;
    let uvdx = v0.x - u0.x;
    let udy = u1.y - u0.y;
    let vdy = v1.y - v0.y;
    let uvdy = v0.y - u0.y;

    let denom = udy * vdx - udx * vdy;
    if denom == 0.0 {
        // lines are parallel
        return None;
    }
    // Cramer's rule
    let s = (vdx * uvdy - vdy * uvdx) / denom;
    let t = (udx * uvdy - udy * uvdx) / denom;

    if !(0.0..=1.0).contains(&s) || !(0.0..=1.0).contains(&t) {
        return None;
    }

    Some(Point::new(
        u0.x + (s * udx).round(),
        u0.y + (s * udy).round(),
    ))
}

// ---------------------------------------------------------------------------
// Vector
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Vector(pub Vec<f64>);

impl Vector {
    pub fn new(components: &[f64]) -> Self {
        Vector(components.to_vec())
    }

    /// New 2D vector from length and angle (in radians).
    pub fn from_properties(length: f64, angle_in_radians: f64) -> Self {
        Vector(vec![
            length * angle_in_radians.sin(),
            length * angle_in_radians.cos(),
        ])
    }

    /// Extend vector length by `length`.
    pub fn add_length(&self, length: f64) -> Self {
        self.unit().multiply(self.length() + length)
    }

    pub fn add(&self, b: &Vector) -> Self {
        Vector(self.0.iter().zip(&b.0).map(|(a, b)| a + b).collect())
    }

    pub fn minus(&self, b: &Vector) -> Self {
        Vector(self.0.iter().zip(&b.0).map(|(a, b)| a - b).collect())
    }

    pub fn multiply(&self, v: f64) -> Self {
        Vector(self.0.iter().map(|a| a * v).collect())
    }

    pub fn length(&self) -> f64 {
        self.0.iter().map(|c| c * c).sum::<f64>().sqrt()
    }

    /// Unit vector in the same direction.
    pub fn unit(&self) -> Self {
        self.multiply(1.0 / self.length())
    }

    pub fn to_point(&self) -> Point {
        Point::new(self.0[0], self.0[1])
    }

    pub fn radians(&self) -> f64 {
        (self.0[1].atan2(self.0[0]) as f32) as f64
    }

    pub fn degrees(&self) -> f64 {
        self.radians() * 180.0 / PI
    }

    pub fn reverse(&self) -> Self {
        self.multiply(-1.0)
    }

    /// Approximate equality (within PRECISION per component).
    pub fn approx_equals(&self, other: &Vector) -> bool {
        if self.0.len() != other.0.len() {
            return false;
        }
        self.0
            .iter()
            .zip(&other.0)
            .all(|(a, b)| precision_compare(*a, *b, PRECISION) == 0)
    }
}

/// Return the normal vector (rotated 90 degrees counter-clockwise).
pub fn get_normal_vector(x1: f64, y1: f64, x2: f64, y2: f64) -> (f64, f64) {
    (y1 - y2, x2 - x1)
}

/// Return the unit normal vector.
pub fn get_unit_normal_vector(x1: f64, y1: f64, x2: f64, y2: f64) -> (f64, f64) {
    let (nx, ny) = get_normal_vector(x1, y1, x2, y2);
    let length = euclidean_distance(x1, y1, x2, y2);
    (nx / length, ny / length)
}

// ---------------------------------------------------------------------------
// Segment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub start: Point,
    pub end: Point,
}

impl Segment {
    pub fn new(start: Point, end: Point) -> Self {
        Self { start, end }
    }

    pub fn overlaps(&self, other: &Segment, is_horizontal: bool, buffer: f64) -> bool {
        if is_horizontal {
            if self.start.y.min(self.end.y) - other.start.y.max(other.end.y) >= buffer {
                return false;
            }
            if other.start.y.min(other.end.y) - self.start.y.max(self.end.y) >= buffer {
                return false;
            }
            true
        } else {
            if self.start.x.min(self.end.x) - other.start.x.max(other.end.x) >= buffer {
                return false;
            }
            if other.start.x.min(other.end.x) - self.start.x.max(self.end.x) >= buffer {
                return false;
            }
            true
        }
    }

    pub fn intersects(&self, other: &Segment) -> bool {
        intersection_point(&self.start, &self.end, &other.start, &other.end).is_some()
    }

    pub fn intersections(&self, other: &Segment) -> Vec<Point> {
        match intersection_point(&self.start, &self.end, &other.start, &other.end) {
            Some(p) => vec![p],
            None => vec![],
        }
    }

    /// Get floor and ceiling bounds for shifting this segment among `segments`.
    pub fn get_bounds(&self, segments: &[Segment], buffer: f64) -> (f64, f64) {
        let mut ceil = f64::INFINITY;
        let mut floor = f64::NEG_INFINITY;
        if self.start.x == self.end.x && self.start.y == self.end.y {
            return (floor, ceil);
        }
        let is_horizontal = self.start.x == self.end.x;
        for other in segments {
            if is_horizontal {
                if other.end.y < self.start.y - buffer {
                    continue;
                }
                if other.start.y > self.end.y + buffer {
                    continue;
                }
                if other.start.x <= self.start.x {
                    floor = floor.max(other.start.x);
                }
                if other.start.x > self.start.x {
                    ceil = ceil.min(other.start.x);
                }
            } else {
                if other.end.x < self.start.x - buffer {
                    continue;
                }
                if other.start.x > self.end.x + buffer {
                    continue;
                }
                if other.start.y <= self.start.y {
                    floor = floor.max(other.start.y);
                }
                if other.start.y > self.start.y {
                    ceil = ceil.min(other.start.y);
                }
            }
        }
        (floor, ceil)
    }

    pub fn length(&self) -> f64 {
        euclidean_distance(self.start.x, self.start.y, self.end.x, self.end.y)
    }

    pub fn to_vector(&self) -> Vector {
        Vector::new(&[self.end.x - self.start.x, self.end.y - self.start.y])
    }
}

impl fmt::Display for Segment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} -> {}", self.start, self.end)
    }
}

/// Trait for types that can compute intersections with a segment.
pub trait Intersectable {
    fn intersections(&self, segment: &Segment) -> Vec<Point>;
}

impl Intersectable for Segment {
    fn intersections(&self, segment: &Segment) -> Vec<Point> {
        Segment::intersections(self, segment)
    }
}

// ---------------------------------------------------------------------------
// PathElement
// ---------------------------------------------------------------------------

/// Geometric path element: either a line segment or a Bezier curve.
#[derive(Debug, Clone)]
pub enum PathElement {
    Segment(Segment),
    Bezier(BezierCurve),
}

impl Intersectable for PathElement {
    fn intersections(&self, segment: &Segment) -> Vec<Point> {
        match self {
            PathElement::Segment(s) => s.intersections(segment),
            PathElement::Bezier(b) => b.intersections(segment),
        }
    }
}

// ---------------------------------------------------------------------------
// Box2D (also exported as `Box` for API compatibility)
// ---------------------------------------------------------------------------

/// Type alias: consumers may use `crate::geo::Box` as a convenience name.
pub type Box = Box2D;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box2D {
    pub top_left: Point,
    pub width: f64,
    pub height: f64,
}

impl Box2D {
    pub fn new(top_left: Point, width: f64, height: f64) -> Self {
        Self {
            top_left,
            width,
            height,
        }
    }

    pub fn center(&self) -> Point {
        Point::new(
            self.top_left.x + self.width / 2.0,
            self.top_left.y + self.height / 2.0,
        )
    }

    /// Returns true if `segment` comes within `buffer` of the box.
    pub fn intersects_segment(&self, s: &Segment, buffer: f64) -> bool {
        let tl = Point::new(self.top_left.x - buffer, self.top_left.y - buffer);
        let tr = Point::new(tl.x + self.width + buffer * 2.0, tl.y);
        let br = Point::new(tr.x, tr.y + self.height + buffer * 2.0);
        let bl = Point::new(tl.x, br.y);

        intersection_point(&s.start, &s.end, &tl, &tr).is_some()
            || intersection_point(&s.start, &s.end, &tr, &br).is_some()
            || intersection_point(&s.start, &s.end, &br, &bl).is_some()
            || intersection_point(&s.start, &s.end, &bl, &tl).is_some()
    }

    /// Return all intersection points of `segment` with box edges.
    pub fn intersections(&self, s: &Segment) -> Vec<Point> {
        let tl = self.top_left;
        let tr = Point::new(tl.x + self.width, tl.y);
        let br = Point::new(tr.x, tr.y + self.height);
        let bl = Point::new(tl.x, br.y);

        let mut pts = Vec::new();
        if let Some(p) = intersection_point(&s.start, &s.end, &tl, &tr) {
            pts.push(p);
        }
        if let Some(p) = intersection_point(&s.start, &s.end, &tr, &br) {
            pts.push(p);
        }
        if let Some(p) = intersection_point(&s.start, &s.end, &br, &bl) {
            pts.push(p);
        }
        if let Some(p) = intersection_point(&s.start, &s.end, &bl, &tl) {
            pts.push(p);
        }
        pts
    }

    pub fn contains(&self, p: &Point) -> bool {
        !(p.x < self.top_left.x
            || self.top_left.x + self.width < p.x
            || p.y < self.top_left.y
            || self.top_left.y + self.height < p.y)
    }

    pub fn overlaps(&self, other: &Box2D) -> bool {
        (self.top_left.x < other.top_left.x + other.width)
            && (self.top_left.x + self.width > other.top_left.x)
            && (self.top_left.y < other.top_left.y + other.height)
            && (self.top_left.y + self.height > other.top_left.y)
    }
}

impl fmt::Display for Box2D {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{TopLeft: {}, Width: {:.0}, Height: {:.0}}}",
            self.top_left, self.width, self.height
        )
    }
}

// ---------------------------------------------------------------------------
// Route
// ---------------------------------------------------------------------------

/// Compatibility trait -- all methods are inherent on `Route`, so this
/// trait is empty and auto-implemented.  Consumers that `use crate::geo::RouteExt`
/// will compile without changes.
pub trait RouteExt {
    fn length(&self) -> f64;
    fn get_point_at_distance(&self, distance: f64) -> (Point, usize);
}

#[derive(Debug, Clone, PartialEq)]
pub struct Route(pub Vec<Point>);

impl RouteExt for Route {
    fn length(&self) -> f64 {
        Route::length(self)
    }
    fn get_point_at_distance(&self, distance: f64) -> (Point, usize) {
        Route::get_point_at_distance(self, distance)
    }
}

impl From<Vec<Point>> for Route {
    fn from(v: Vec<Point>) -> Self {
        Route(v)
    }
}

impl std::ops::Deref for Route {
    type Target = [Point];
    fn deref(&self) -> &[Point] {
        &self.0
    }
}

impl std::ops::Index<usize> for Route {
    type Output = Point;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl Route {
    pub fn new(points: Vec<Point>) -> Self {
        Route(points)
    }

    pub fn length(&self) -> f64 {
        let mut l = 0.0;
        for i in 0..self.0.len().saturating_sub(1) {
            l += euclidean_distance(self.0[i].x, self.0[i].y, self.0[i + 1].x, self.0[i + 1].y);
        }
        l
    }

    /// Return the point at `distance` along the route and the segment index.
    pub fn get_point_at_distance(&self, distance: f64) -> (Point, usize) {
        let mut remaining = distance;
        let pts = &self.0;
        let mut curr_idx = 0;
        let mut seg_len = 0.0f64;

        for i in 0..pts.len().saturating_sub(1) {
            curr_idx = i;
            seg_len = euclidean_distance(pts[i].x, pts[i].y, pts[i + 1].x, pts[i + 1].y);
            if remaining <= seg_len {
                let t = remaining / seg_len;
                return (pts[i].interpolate(&pts[i + 1], t), i);
            }
            remaining -= seg_len;
        }

        // distance > total length: extrapolate from last segment
        let last = pts.len().saturating_sub(2);
        let t = 1.0 + remaining / seg_len;
        (pts[curr_idx].interpolate(&pts[curr_idx + 1], t), last)
    }

    /// Bounding box as (top_left, bottom_right).
    pub fn get_bounding_box(&self) -> (Point, Point) {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for p in &self.0 {
            if p.x < min_x {
                min_x = p.x;
            }
            if p.x > max_x {
                max_x = p.x;
            }
            if p.y < min_y {
                min_y = p.y;
            }
            if p.y > max_y {
                max_y = p.y;
            }
        }
        (Point::new(min_x, min_y), Point::new(max_x, max_y))
    }
}

// ---------------------------------------------------------------------------
// Ellipse
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipse {
    pub center: Point,
    pub rx: f64,
    pub ry: f64,
}

impl Ellipse {
    pub fn new(center: Point, rx: f64, ry: f64) -> Self {
        Self { center, rx, ry }
    }

    pub fn intersections(&self, segment: &Segment) -> Vec<Point> {
        let mut results: Vec<Point> = Vec::new();

        let a = self.rx;
        let b = self.ry;
        let a2 = a * a;
        let b2 = b * b;
        if a <= 0.0 || b <= 0.0 {
            return results;
        }

        let x1 = segment.start.x - self.center.x;
        let y1 = segment.start.y - self.center.y;
        let x2 = segment.end.x - self.center.x;
        let y2 = segment.end.y - self.center.y;

        // Vertical line case
        if x1 == x2 {
            let disc = a2 - x1 * x1;
            if disc < 0.0 {
                return results;
            }
            let i1 = Point::new(x1, b * disc.sqrt() / a);
            let i2 = Point::new(x1, -i1.y);

            let is_on_line = |p: &Point| {
                let mut ps = [p.y, y1, y2];
                ps.sort_by(|a, b| a.partial_cmp(b).unwrap());
                ps[1] == p.y
            };

            if is_on_line(&i1) {
                results.push(Point::new(i1.x + self.center.x, i1.y + self.center.y));
            }
            if i2.y != 0.0 && is_on_line(&i2) {
                results.push(Point::new(i2.x + self.center.x, i2.y + self.center.y));
            }
            return results;
        }

        // General case: y = mx + c
        let m = (y2 - y1) / (x2 - x1);
        let c = y1 - m * x1;

        let is_on_line = |p: &Point| {
            let line_start = Point::new(x1, y1);
            let line_end = Point::new(x2, y2);
            precision_compare(p.distance_to_line(&line_start, &line_end), 0.0, PRECISION) == 0
        };

        let denom = a2 * m * m + b2;
        let inner = a2 * b2 * (denom - c * c);
        if inner < -(PRECISION * PRECISION) {
            return results;
        }
        let root = inner.max(0.0).sqrt();

        let i1 = Point::new((-m * c * a2 + root) / denom, (c * b2 + m * root) / denom);
        let i2 = Point::new((-m * c * a2 - root) / denom, (c * b2 - m * root) / denom);

        if is_on_line(&i1) {
            results.push(Point::new(i1.x + self.center.x, i1.y + self.center.y));
        }
        if !i1.equals(&i2) && is_on_line(&i2) {
            results.push(Point::new(i2.x + self.center.x, i2.y + self.center.y));
        }

        results
    }
}

impl Intersectable for Ellipse {
    fn intersections(&self, segment: &Segment) -> Vec<Point> {
        Ellipse::intersections(self, segment)
    }
}

// ---------------------------------------------------------------------------
// Bezier internals
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct BezierPoint {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, Copy)]
struct BezierControlPoint {
    point: BezierPoint,
    control: BezierPoint,
}

#[derive(Debug, Clone)]
struct BezierCurveImpl(Vec<BezierControlPoint>);

impl BezierCurveImpl {
    fn new(cp: &[BezierPoint]) -> Self {
        if cp.is_empty() {
            return BezierCurveImpl(Vec::new());
        }

        let n = cp.len();
        let mut c: Vec<BezierControlPoint> = cp
            .iter()
            .map(|p| BezierControlPoint {
                point: *p,
                control: BezierPoint { x: 0.0, y: 0.0 },
            })
            .collect();

        let mut w: f64 = 0.0;
        for (i, ci) in c.iter_mut().enumerate() {
            match i {
                0 => w = 1.0,
                1 => w = (n as f64) - 1.0,
                _ => w *= (n - i) as f64 / i as f64,
            }
            ci.control.x = ci.point.x * w;
            ci.control.y = ci.point.y * w;
        }

        BezierCurveImpl(c)
    }

    fn point_at(&mut self, t: f64) -> BezierPoint {
        let c = &mut self.0;
        c[0].point = c[0].control;
        let mut u = t;
        for ci in c.iter_mut().skip(1) {
            ci.point = BezierPoint {
                x: ci.control.x * u,
                y: ci.control.y * u,
            };
            u *= t;
        }

        let t1 = 1.0 - t;
        let mut tt = t1;
        let mut p = c[c.len() - 1].point;
        for i in (0..c.len() - 1).rev() {
            p.x += c[i].point.x * tt;
            p.y += c[i].point.y * tt;
            tt *= t1;
        }
        p
    }
}

// ---------------------------------------------------------------------------
// BezierCurve (public)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BezierCurve {
    curve: BezierCurveImpl,
    points: Vec<Point>,
}

impl BezierCurve {
    pub fn new(points: Vec<Point>) -> Self {
        let local: Vec<BezierPoint> = points
            .iter()
            .map(|p| BezierPoint { x: p.x, y: p.y })
            .collect();
        let curve = BezierCurveImpl::new(&local);
        BezierCurve { curve, points }
    }

    pub fn at(&mut self, t: f64) -> Point {
        let bp = self.curve.point_at(t);
        Point::new(bp.x, bp.y)
    }

    /// Intersections of this cubic bezier with a line segment.
    pub fn intersections(&self, segment: &Segment) -> Vec<Point> {
        assert!(
            self.points.len() == 4,
            "BezierCurve.intersections requires exactly 4 control points"
        );
        compute_intersections(
            &[
                self.points[0].x,
                self.points[1].x,
                self.points[2].x,
                self.points[3].x,
            ],
            &[
                self.points[0].y,
                self.points[1].y,
                self.points[2].y,
                self.points[3].y,
            ],
            &[segment.start.x, segment.end.x],
            &[segment.start.y, segment.end.y],
        )
    }

    /// Access control points.
    pub fn points(&self) -> &[Point] {
        &self.points
    }
}

impl Intersectable for BezierCurve {
    fn intersections(&self, segment: &Segment) -> Vec<Point> {
        BezierCurve::intersections(self, segment)
    }
}

// ---------------------------------------------------------------------------
// Bezier-line intersection helpers
// ---------------------------------------------------------------------------

pub fn compute_intersections(px: &[f64], py: &[f64], lx: &[f64], ly: &[f64]) -> Vec<Point> {
    let mut out = Vec::new();

    let a_val = ly[1] - ly[0];
    let b_val = lx[0] - lx[1];
    let c_val = lx[0] * (ly[0] - ly[1]) + ly[0] * (lx[1] - lx[0]);

    let bx = bezier_coeffs(px[0], px[1], px[2], px[3]);
    let by = bezier_coeffs(py[0], py[1], py[2], py[3]);

    let p = [
        a_val * bx[0] + b_val * by[0],
        a_val * bx[1] + b_val * by[1],
        a_val * bx[2] + b_val * by[2],
        a_val * bx[3] + b_val * by[3] + c_val,
    ];

    let r = cubic_roots(&p);

    for &t in &r {
        let point = Point::new(
            bx[0] * t * t * t + bx[1] * t * t + bx[2] * t + bx[3],
            by[0] * t * t * t + by[1] * t * t + by[2] * t + by[3],
        );

        let s = if (lx[1] - lx[0]) != 0.0 {
            (point.x - lx[0]) / (lx[1] - lx[0])
        } else {
            (point.y - ly[0]) / (ly[1] - ly[0])
        };

        let t_lt_0 = precision_compare(t, 0.0, PRECISION) < 0;
        let t_gt_1 = precision_compare(t, 1.0, PRECISION) > 0;
        let s_lt_0 = precision_compare(s, 0.0, PRECISION) < 0;
        let s_gt_1 = precision_compare(s, 1.0, PRECISION) > 0;
        if !(t_lt_0 || t_gt_1 || s_lt_0 || s_gt_1) {
            out.push(point);
        }
    }

    out
}

fn cubic_roots(p: &[f64; 4]) -> [f64; 3] {
    if precision_compare(p[0], 0.0, PRECISION) == 0 {
        if precision_compare(p[1], 0.0, PRECISION) == 0 {
            let mut t = [-p[3] / p[2], -1.0, -1.0];

            // Go code loops for i in 0..1 (just index 0)
            if precision_compare(t[0], 0.0, PRECISION) < 0
                || precision_compare(t[0], 1.0, PRECISION) > 0
            {
                t[0] = -1.0;
            }

            sort_special(&mut t);
            return t;
        }

        let dq = p[2].powi(2) - 4.0 * p[1] * p[3];
        if precision_compare(dq, 0.0, PRECISION) >= 0 {
            let dq_sqrt = dq.sqrt();
            let mut t = [
                -((dq_sqrt + p[2]) / (2.0 * p[1])),
                (dq_sqrt - p[2]) / (2.0 * p[1]),
                -1.0,
            ];

            // Go code has a loop that always returns on first iteration
            if precision_compare(t[0], 0.0, PRECISION) < 0
                || precision_compare(t[0], 1.0, PRECISION) > 0
            {
                t[0] = -1.0;
            }

            sort_special(&mut t);
            return t;
        }
    }

    let (a, b, c, d) = (p[0], p[1], p[2], p[3]);

    let cap_a = b / a;
    let cap_b = c / a;
    let cap_c = d / a;

    let q = (3.0 * cap_b - cap_a.powi(2)) / 9.0;
    let r = (9.0 * cap_a * cap_b - 27.0 * cap_c - 2.0 * cap_a.powi(3)) / 54.0;
    let big_d = q.powi(3) + r.powi(2);

    let mut t = [0.0f64; 3];

    if precision_compare(big_d, 0.0, PRECISION) >= 0 {
        let s_val = sgn(r + big_d.sqrt()) * (r + big_d.sqrt()).abs().powf(1.0 / 3.0);
        let t_val = sgn(r - big_d.sqrt()) * (r - big_d.sqrt()).abs().powf(1.0 / 3.0);

        t[0] = -cap_a / 3.0 + (s_val + t_val);
        t[1] = -cap_a / 3.0 - (s_val + t_val) / 2.0;
        t[2] = -cap_a / 3.0 - (s_val + t_val) / 2.0;
        let im = (3.0f64.sqrt() * (s_val - t_val) / 2.0).abs();

        if precision_compare(im, 0.0, PRECISION) != 0 {
            t[1] = -1.0;
            t[2] = -1.0;
        }
    } else {
        let th = (r / (-q.powi(3)).sqrt()).acos();
        t[0] = 2.0 * (-q).sqrt() * (th / 3.0).cos() - cap_a / 3.0;
        t[1] = 2.0 * (-q).sqrt() * ((th + 2.0 * PI) / 3.0).cos() - cap_a / 3.0;
        t[2] = 2.0 * (-q).sqrt() * ((th + 4.0 * PI) / 3.0).cos() - cap_a / 3.0;
    }

    for ti in &mut t {
        if precision_compare(*ti, 0.0, PRECISION) < 0 || precision_compare(*ti, 1.0, PRECISION) > 0
        {
            *ti = -1.0;
        }
    }

    sort_special(&mut t);
    t
}

fn sort_special(a: &mut [f64; 3]) {
    loop {
        let mut flip = false;
        for i in 0..a.len() - 1 {
            let ai1_gte_0 = precision_compare(a[i + 1], 0.0, PRECISION) >= 0;
            let ai_gt_ai1 = precision_compare(a[i], a[i + 1], PRECISION) > 0;
            let ai_lt_0 = precision_compare(a[i], 0.0, PRECISION) < 0;
            if ai1_gte_0 && (ai_gt_ai1 || ai_lt_0) {
                flip = true;
                a.swap(i, i + 1);
            }
        }
        if !flip {
            break;
        }
    }
}

fn sgn(x: f64) -> f64 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

fn bezier_coeffs(p0: f64, p1: f64, p2: f64, p3: f64) -> [f64; 4] {
    [
        -p0 + 3.0 * p1 - 3.0 * p2 + p3,
        3.0 * p0 - 6.0 * p1 + 3.0 * p2,
        -3.0 * p0 + 3.0 * p1,
        p0,
    ]
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::SQRT_2;

    // -- Math helpers -------------------------------------------------------

    #[test]
    fn test_truncate_decimals() {
        assert_eq!(truncate_decimals(1.23456), 1.234);
        assert_eq!(truncate_decimals(-0.9999), -0.999);
        assert_eq!(truncate_decimals(0.0), 0.0);
    }

    #[test]
    fn test_sign() {
        assert_eq!(sign(-5.0), -1);
        assert_eq!(sign(0.0), 0);
        assert_eq!(sign(3.5), 1);
    }

    #[test]
    fn test_precision_compare() {
        assert_eq!(precision_compare(1.0, 1.00001, 0.001), 0);
        assert_eq!(precision_compare(1.0, 2.0, 0.001), -1);
        assert_eq!(precision_compare(2.0, 1.0, 0.001), 1);
    }

    // -- Point --------------------------------------------------------------

    #[test]
    fn test_point_distance_to_line() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(100.0, 0.0);
        let p = Point::new(50.0, 70.0);
        assert_eq!(p.distance_to_line(&p1, &p2), 70.0);
    }

    #[test]
    fn test_add_vector() {
        let start = Point::new(1.5, 5.3);
        let v = Vector::new(&[-3.5, -2.3]);
        let p2 = start.add_vector(&v);
        assert_eq!(p2.x, -2.0);
        assert_eq!(p2.y, 3.0);
    }

    #[test]
    fn test_to_vector() {
        let p = Point::new(3.5, 6.7);
        let v = p.to_vector();
        assert_eq!(v.0[0], p.x);
        assert_eq!(v.0[1], p.y);
        assert_eq!(v.0.len(), 2);
    }

    #[test]
    fn test_vector_to() {
        let p1 = Point::new(1.5, 5.3);
        let p2 = Point::new(-2.0, 3.0);
        let c = p1.vector_to(&p2);
        assert!(c.approx_equals(&Vector::new(&[-3.5, -2.3])));

        let c2 = p2.vector_to(&p1);
        assert!(c2.approx_equals(&Vector::new(&[3.5, 2.3])));
    }

    #[test]
    fn test_point_interpolate() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(10.0, 20.0);
        let mid = a.interpolate(&b, 0.5);
        assert_eq!(mid.x, 5.0);
        assert_eq!(mid.y, 10.0);
    }

    #[test]
    fn test_point_orientation() {
        let center = Point::new(5.0, 5.0);
        assert_eq!(
            center.get_orientation(&Point::new(5.0, 10.0)),
            Orientation::Top
        );
        assert_eq!(
            center.get_orientation(&Point::new(5.0, 0.0)),
            Orientation::Bottom
        );
        assert_eq!(
            center.get_orientation(&Point::new(10.0, 5.0)),
            Orientation::Left
        );
        assert_eq!(
            center.get_orientation(&Point::new(0.0, 5.0)),
            Orientation::Right
        );
        assert_eq!(
            center.get_orientation(&Point::new(5.0, 5.0)),
            Orientation::None
        );
    }

    // -- Vector -------------------------------------------------------------

    #[test]
    fn test_extend_vertical_line_segments() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(0.0, 1.0);

        let v = p1.vector_to(&p2).multiply(2.0);
        let p2_new = p1.add_vector(&v);
        assert_eq!(p2_new, Point::new(0.0, 2.0));

        let v2 = p2.vector_to(&p1).multiply(2.0);
        let p1_new = p2.add_vector(&v2);
        assert_eq!(p1_new, Point::new(0.0, -1.0));
    }

    #[test]
    fn test_extend_horizontal_line_segment() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(1.0, 0.0);

        let v = p1.vector_to(&p2).multiply(1.5);
        let p2_new = p1.add_vector(&v);
        assert_eq!(p2_new, Point::new(1.5, 0.0));

        let v2 = p2.vector_to(&p1).multiply(1.5);
        let p1_new = p2.add_vector(&v2);
        assert_eq!(p1_new, Point::new(-0.5, 0.0));
    }

    #[test]
    fn test_extend_diagonal_line_segment() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(3.0, 1.0);

        let v = p1.vector_to(&p2).multiply(2.0);
        let p2_new = p1.add_vector(&v);
        assert_eq!(p2_new, Point::new(6.0, 2.0));

        let v2 = p2.vector_to(&p1).multiply(2.0);
        let p1_new = p2.add_vector(&v2);
        assert_eq!(p1_new, Point::new(-3.0, -1.0));
    }

    #[test]
    fn test_vector_add() {
        let a = Vector::new(&[1.0, 2.0]);
        let b = Vector::new(&[3.0, 4.0]);
        let c = a.add(&b);
        assert!(c.approx_equals(&Vector::new(&[4.0, 6.0])));
    }

    #[test]
    fn test_vector_minus() {
        let a = Vector::new(&[1.0, 2.0]);
        let b = Vector::new(&[3.0, 4.0]);
        let c = a.minus(&b);
        assert!(c.approx_equals(&Vector::new(&[-2.0, -2.0])));
    }

    #[test]
    fn test_vector_multiply() {
        let a = Vector::new(&[1.0, 2.0]);
        let c = a.multiply(3.0);
        assert!(c.approx_equals(&Vector::new(&[3.0, 6.0])));
    }

    #[test]
    fn test_vector_length() {
        let a = Vector::new(&[3.0, 4.0]);
        assert_eq!(a.length(), 5.0);
    }

    #[test]
    fn test_new_vector_from_properties() {
        let a = Vector::from_properties(3.0, PI / 3.0);
        assert!(a.approx_equals(&Vector::new(&[2.59807621135, 1.5])));

        let b = Vector::from_properties(3.0, -PI / 3.0);
        assert!(b.approx_equals(&Vector::new(&[-2.59807621135, 1.5])));

        let c = Vector::from_properties(3.0, PI * 2.0 / 3.0);
        assert!(c.approx_equals(&Vector::new(&[2.59807621135, -1.5])));

        let d = Vector::from_properties(3.0, -PI * 2.0 / 3.0);
        assert!(d.approx_equals(&Vector::new(&[-2.59807621135, -1.5])));
    }

    #[test]
    fn test_vector_unit() {
        let a = Vector::new(&[3.0, 4.0]).unit();
        let expected = Vector::new(&[3.0 / 5.0, 4.0 / 5.0]);
        assert!(a.approx_equals(&expected));
    }

    #[test]
    fn test_vector_add_length() {
        let a = Vector::new(&[3.0, 4.0]);
        let b = a.add_length(8.0);
        assert_eq!(precision_compare(b.length(), 13.0, PRECISION), 0);
    }

    #[test]
    fn test_vector_equals() {
        let a = Vector::new(&[1.0, 2.0]);
        assert!(a.approx_equals(&a));
        assert!(a.approx_equals(&Vector::new(&[1.0, 2.0])));
        assert!(!a.approx_equals(&Vector::new(&[1.0, 2.0, 3.0])));
        assert!(!a.approx_equals(&Vector::new(&[2.0, 2.0])));
    }

    #[test]
    fn test_vector_to_point() {
        let v = Vector::new(&[3.789, -0.731]);
        let p = v.to_point();
        assert_eq!(v.0[0], p.x);
        assert_eq!(v.0[1], p.y);
    }

    // -- Segment ------------------------------------------------------------

    #[test]
    fn test_segment_intersections() {
        // mid intersection
        let s1 = Segment::new(Point::new(0.0, 0.0), Point::new(10.0, 10.0));
        let s2 = Segment::new(Point::new(0.0, 10.0), Point::new(10.0, 0.0));
        let intersections = s1.intersections(&s2);
        assert_eq!(intersections.len(), 1);
        assert!(intersections[0].equals(&Point::new(5.0, 5.0)));

        // intersection at end
        let s3 = Segment::new(Point::new(10.0, 10.0), Point::new(10.0, 0.0));
        let intersections = s1.intersections(&s3);
        assert_eq!(intersections.len(), 1);
        assert!(intersections[0].equals(&Point::new(10.0, 10.0)));

        // intersection at beginning
        let s4 = Segment::new(Point::new(0.0, 0.0), Point::new(0.0, 10.0));
        let intersections = s1.intersections(&s4);
        assert_eq!(intersections.len(), 1);
        assert!(intersections[0].equals(&Point::new(0.0, 0.0)));

        // no intersection
        let s5 = Segment::new(Point::new(3.0, 8.0), Point::new(2.0, 15.0));
        let intersections = s1.intersections(&s5);
        assert_eq!(intersections.len(), 0);
    }

    // -- Ellipse ------------------------------------------------------------

    #[test]
    fn test_ellipse_line_intersections() {
        let e = Ellipse::new(Point::new(0.0, 0.0), 11.0, 11.0);

        // vertical through center
        let intersections =
            e.intersections(&Segment::new(Point::new(0.0, 20.0), Point::new(0.0, -20.0)));
        assert_eq!(intersections.len(), 2);
        assert!(intersections[0].equals(&Point::new(0.0, 11.0)));
        assert!(intersections[1].equals(&Point::new(0.0, -11.0)));

        // vertical segment inside ellipse (no intersection)
        let intersections =
            e.intersections(&Segment::new(Point::new(0.0, 2.0), Point::new(0.0, -2.0)));
        assert_eq!(intersections.len(), 0);

        // segment fully inside (no intersection)
        let intersections =
            e.intersections(&Segment::new(Point::new(2.0, 2.0), Point::new(5.0, 5.0)));
        assert_eq!(intersections.len(), 0);

        // diagonal exits at one point
        let intersections =
            e.intersections(&Segment::new(Point::new(2.0, 2.0), Point::new(50.0, 50.0)));
        let xv = SQRT_2 / 2.0 * 11.0;
        assert_eq!(intersections.len(), 1);
        assert!(
            precision_compare(intersections[0].x, xv, PRECISION) == 0
                && precision_compare(intersections[0].y, xv, PRECISION) == 0,
            "expected ({}, {}), got ({}, {})",
            xv,
            xv,
            intersections[0].x,
            intersections[0].y,
        );

        // test with cx,cy offset
        let e2 = Ellipse::new(Point::new(100.0, 200.0), 21.0, 21.0);
        let intersections = e2.intersections(&Segment::new(
            Point::new(0.0, 0.0),
            Point::new(100.0, 150.0),
        ));
        assert_eq!(intersections.len(), 0);

        let intersections = e2.intersections(&Segment::new(
            Point::new(50.0, 150.0),
            Point::new(200.0, 250.0),
        ));
        assert_eq!(intersections.len(), 2);

        // tangent horizontal
        let intersections = e2.intersections(&Segment::new(
            Point::new(0.0, 221.0),
            Point::new(200.0, 221.0),
        ));
        assert_eq!(intersections.len(), 1);

        // tangent vertical
        let intersections = e2.intersections(&Segment::new(
            Point::new(121.0, 100.0),
            Point::new(121.0, 300.0),
        ));
        assert_eq!(intersections.len(), 1);

        // diagonal tangent (floating point may produce 2 very close intersections)
        let e3 = Ellipse::new(Point::new(1.0, 1.0), 2.0 / SQRT_2, 2.0 / SQRT_2);
        let intersections =
            e3.intersections(&Segment::new(Point::new(1.0, 3.0), Point::new(3.0, 1.0)));
        assert!(!intersections.is_empty(), "should intersect tangent");
    }

    // -- Box ----------------------------------------------------------------

    #[test]
    fn test_box_contains() {
        let b = Box2D::new(Point::new(0.0, 0.0), 10.0, 10.0);
        assert!(b.contains(&Point::new(5.0, 5.0)));
        assert!(b.contains(&Point::new(0.0, 0.0)));
        assert!(b.contains(&Point::new(10.0, 10.0)));
        assert!(!b.contains(&Point::new(-1.0, 5.0)));
        assert!(!b.contains(&Point::new(5.0, 11.0)));
    }

    #[test]
    fn test_box_center() {
        let b = Box2D::new(Point::new(10.0, 20.0), 100.0, 50.0);
        assert_eq!(b.center(), Point::new(60.0, 45.0));
    }

    #[test]
    fn test_box_overlaps() {
        let b1 = Box2D::new(Point::new(0.0, 0.0), 10.0, 10.0);
        let b2 = Box2D::new(Point::new(5.0, 5.0), 10.0, 10.0);
        assert!(b1.overlaps(&b2));

        let b3 = Box2D::new(Point::new(20.0, 20.0), 10.0, 10.0);
        assert!(!b1.overlaps(&b3));
    }

    #[test]
    fn test_box_intersections() {
        let b = Box2D::new(Point::new(0.0, 0.0), 10.0, 10.0);
        // segment crossing through the box
        let s = Segment::new(Point::new(-5.0, 5.0), Point::new(15.0, 5.0));
        let pts = b.intersections(&s);
        assert_eq!(pts.len(), 2);
    }

    // -- Route --------------------------------------------------------------

    #[test]
    fn test_route_length() {
        let r = Route::new(vec![
            Point::new(0.0, 0.0),
            Point::new(3.0, 0.0),
            Point::new(3.0, 4.0),
        ]);
        assert_eq!(r.length(), 7.0);
    }

    #[test]
    fn test_route_bounding_box() {
        let r = Route::new(vec![
            Point::new(1.0, 2.0),
            Point::new(5.0, 8.0),
            Point::new(3.0, 1.0),
        ]);
        let (tl, br) = r.get_bounding_box();
        assert_eq!(tl, Point::new(1.0, 1.0));
        assert_eq!(br, Point::new(5.0, 8.0));
    }

    #[test]
    fn test_route_get_point_at_distance() {
        let r = Route::new(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
        ]);
        let (p, idx) = r.get_point_at_distance(5.0);
        assert_eq!(idx, 0);
        assert_eq!(p, Point::new(5.0, 0.0));

        let (p2, idx2) = r.get_point_at_distance(15.0);
        assert_eq!(idx2, 1);
        assert_eq!(p2, Point::new(10.0, 5.0));
    }

    // -- BezierCurve --------------------------------------------------------

    #[test]
    fn test_bezier_at() {
        let mut bc = BezierCurve::new(vec![
            Point::new(0.0, 0.0),
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
            Point::new(10.0, 0.0),
        ]);
        let start = bc.at(0.0);
        assert!(
            precision_compare(start.x, 0.0, PRECISION) == 0
                && precision_compare(start.y, 0.0, PRECISION) == 0
        );
        let end = bc.at(1.0);
        assert!(
            precision_compare(end.x, 10.0, PRECISION) == 0
                && precision_compare(end.y, 0.0, PRECISION) == 0
        );
    }

    // -- Orientation --------------------------------------------------------

    #[test]
    fn test_orientation_opposite() {
        assert_eq!(Orientation::Top.get_opposite(), Orientation::Bottom);
        assert_eq!(Orientation::Left.get_opposite(), Orientation::Right);
        assert_eq!(
            Orientation::TopLeft.get_opposite(),
            Orientation::BottomRight
        );
    }

    #[test]
    fn test_orientation_same_side() {
        assert!(Orientation::TopLeft.same_side(Orientation::Top));
        assert!(Orientation::TopLeft.same_side(Orientation::TopRight));
        assert!(!Orientation::TopLeft.same_side(Orientation::Bottom));
        // TopLeft is also on the Left side
        assert!(Orientation::TopLeft.same_side(Orientation::Left));
    }

    #[test]
    fn test_orientation_predicates() {
        assert!(Orientation::TopLeft.is_diagonal());
        assert!(!Orientation::Top.is_diagonal());
        assert!(Orientation::Left.is_horizontal());
        assert!(!Orientation::Top.is_horizontal());
        assert!(Orientation::Top.is_vertical());
        assert!(!Orientation::Left.is_vertical());
    }

    // -- Points helpers -----------------------------------------------------

    #[test]
    fn test_points_get_median() {
        let pts = vec![
            Point::new(1.0, 10.0),
            Point::new(3.0, 20.0),
            Point::new(5.0, 30.0),
        ];
        let m = points_get_median(&pts);
        assert_eq!(m, Point::new(3.0, 20.0));
    }

    #[test]
    fn test_remove_points() {
        let pts = vec![
            Point::new(1.0, 1.0),
            Point::new(2.0, 2.0),
            Point::new(3.0, 3.0),
        ];
        let result = remove_points(&pts, &[false, true, false]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], Point::new(1.0, 1.0));
        assert_eq!(result[1], Point::new(3.0, 3.0));
    }

    // -- RelativePoint ------------------------------------------------------

    #[test]
    fn test_relative_point() {
        let rp = RelativePoint::new(0.12345, 0.67891);
        assert_eq!(rp.x_percentage, truncate_decimals(0.12345));
        assert_eq!(rp.y_percentage, truncate_decimals(0.67891));
    }
}
