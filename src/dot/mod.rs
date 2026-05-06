// dot - Graphviz integration
// Port of Java PlantUML's net.sourceforge.plantuml.dot package
//
// Modules:
// - graphviz: Graphviz trait, native execution, process management, exe state
// - dot_data: DotData input container for layout engine
// - dot_splines: DotSplines edge routing mode enum
// - version: GraphvizVersion detection and version-dependent behavior flags

pub mod dot_data;
pub mod dot_splines;
pub mod graphviz;
pub mod version;

pub use dot_data::{DotData, DotEntity, DotLink};
pub use dot_splines::DotSplines;
pub use graphviz::{ExeState, Graphviz, ProcessState, DEFAULT_IMAGE_LIMIT};

pub use version::{GraphvizVersion, DOT_VERSION_LIMIT};

// ---------------------------------------------------------------------------
// Neighborhood — port of net.sourceforge.plantuml.dot.Neighborhood
// ---------------------------------------------------------------------------

/// A point in 2D space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

impl Point2D {
    pub fn new(x: f64, y: f64) -> Self {
        Point2D { x, y }
    }
}

/// Axis-aligned rectangle.
#[derive(Debug, Clone, Copy)]
pub struct Rect2D {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect2D {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Rect2D {
            x,
            y,
            width,
            height,
        }
    }

    pub fn min_x(&self) -> f64 {
        self.x
    }
    pub fn min_y(&self) -> f64 {
        self.y
    }
    pub fn max_x(&self) -> f64 {
        self.x + self.width
    }
    pub fn max_y(&self) -> f64 {
        self.y + self.height
    }
    pub fn center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }
    pub fn center_y(&self) -> f64 {
        self.y + self.height / 2.0
    }
}

/// Node neighborhood for edge routing at sametail inheritance arrows.
///
/// Port of Java `Neighborhood`. Computes intersection points between
/// lines from contact points to the node rectangle boundary, used for
/// drawing grouped inheritance arrows.
pub struct Neighborhood {
    pub leaf_uid: String,
    pub sametail_contact_points: Vec<Point2D>,
    pub other_contact_points: Vec<(Point2D, bool)>, // (point, is_start)
}

impl Neighborhood {
    pub fn new(leaf_uid: String) -> Self {
        Neighborhood {
            leaf_uid,
            sametail_contact_points: Vec::new(),
            other_contact_points: Vec::new(),
        }
    }
}

/// Compute the intersection of line segment (pt1→pt2) with line segment (pt3→pt4).
///
/// Port of Java `Neighborhood.intersection(x1,y1,x2,y2,x3,y3,x4,y4)`.
/// Returns `None` if segments don't intersect or are parallel.
pub fn segment_intersection(p1: Point2D, p2: Point2D, p3: Point2D, p4: Point2D) -> Option<Point2D> {
    const EPSILON: f64 = 0.001;

    let d = (p1.x - p2.x) * (p3.y - p4.y) - (p1.y - p2.y) * (p3.x - p4.x);
    if d.abs() < f64::EPSILON {
        return None;
    }

    let xi = ((p3.x - p4.x) * (p1.x * p2.y - p1.y * p2.x)
        - (p1.x - p2.x) * (p3.x * p4.y - p3.y * p4.x))
        / d;
    let yi = ((p3.y - p4.y) * (p1.x * p2.y - p1.y * p2.x)
        - (p1.y - p2.y) * (p3.x * p4.y - p3.y * p4.x))
        / d;

    // Check that intersection is within both segments
    if xi + EPSILON < p1.x.min(p2.x) || xi - EPSILON > p1.x.max(p2.x) {
        return None;
    }
    if xi + EPSILON < p3.x.min(p4.x) || xi - EPSILON > p3.x.max(p4.x) {
        return None;
    }
    if yi + EPSILON < p1.y.min(p2.y) || yi - EPSILON > p1.y.max(p2.y) {
        return None;
    }
    if yi + EPSILON < p3.y.min(p4.y) || yi - EPSILON > p3.y.max(p4.y) {
        return None;
    }

    Some(Point2D::new(xi, yi))
}

/// Find the intersection of a line (from `center` to `target`) with a rectangle boundary.
///
/// Port of Java `Neighborhood.intersection(XRectangle2D, XPoint2D center, XPoint2D target)`.
/// Tests all four rectangle edges and returns the first intersection found.
pub fn rect_line_intersection(rect: &Rect2D, center: Point2D, target: Point2D) -> Option<Point2D> {
    let tl = Point2D::new(rect.min_x(), rect.min_y());
    let tr = Point2D::new(rect.max_x(), rect.min_y());
    let bl = Point2D::new(rect.min_x(), rect.max_y());
    let br = Point2D::new(rect.max_x(), rect.max_y());

    // Top edge
    if let Some(p) = segment_intersection(tl, tr, center, target) {
        return Some(p);
    }
    // Bottom edge
    if let Some(p) = segment_intersection(bl, br, center, target) {
        return Some(p);
    }
    // Left edge
    if let Some(p) = segment_intersection(tl, bl, center, target) {
        return Some(p);
    }
    // Right edge
    if let Some(p) = segment_intersection(tr, br, center, target) {
        return Some(p);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point2d_creation() {
        let p = Point2D::new(1.0, 2.0);
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
    }

    #[test]
    fn rect2d_accessors() {
        let r = Rect2D::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(r.min_x(), 10.0);
        assert_eq!(r.min_y(), 20.0);
        assert_eq!(r.max_x(), 110.0);
        assert_eq!(r.max_y(), 70.0);
        assert_eq!(r.center_x(), 60.0);
        assert_eq!(r.center_y(), 45.0);
    }

    #[test]
    fn segment_intersection_crossing() {
        // Crossing segments: (0,0)-(10,10) and (0,10)-(10,0)
        let p1 = Point2D::new(0.0, 0.0);
        let p2 = Point2D::new(10.0, 10.0);
        let p3 = Point2D::new(0.0, 10.0);
        let p4 = Point2D::new(10.0, 0.0);
        let result = segment_intersection(p1, p2, p3, p4);
        assert!(result.is_some());
        let pt = result.unwrap();
        assert!((pt.x - 5.0).abs() < 0.01);
        assert!((pt.y - 5.0).abs() < 0.01);
    }

    #[test]
    fn segment_intersection_parallel() {
        // Parallel lines: (0,0)-(10,0) and (0,5)-(10,5)
        let p1 = Point2D::new(0.0, 0.0);
        let p2 = Point2D::new(10.0, 0.0);
        let p3 = Point2D::new(0.0, 5.0);
        let p4 = Point2D::new(10.0, 5.0);
        assert!(segment_intersection(p1, p2, p3, p4).is_none());
    }

    #[test]
    fn segment_intersection_no_overlap() {
        // Non-overlapping segments
        let p1 = Point2D::new(0.0, 0.0);
        let p2 = Point2D::new(1.0, 0.0);
        let p3 = Point2D::new(2.0, -1.0);
        let p4 = Point2D::new(2.0, 1.0);
        assert!(segment_intersection(p1, p2, p3, p4).is_none());
    }

    #[test]
    fn rect_line_intersection_top() {
        let rect = Rect2D::new(0.0, 0.0, 100.0, 50.0);
        let center = Point2D::new(50.0, 25.0); // center of rect
        let target = Point2D::new(50.0, -100.0); // above rect
        let result = rect_line_intersection(&rect, center, target);
        assert!(result.is_some());
        let pt = result.unwrap();
        assert!(
            (pt.y - 0.0).abs() < 0.01,
            "should hit top edge, got y={}",
            pt.y
        );
        assert!((pt.x - 50.0).abs() < 0.01);
    }

    #[test]
    fn rect_line_intersection_right() {
        let rect = Rect2D::new(0.0, 0.0, 100.0, 50.0);
        let center = Point2D::new(50.0, 25.0);
        let target = Point2D::new(200.0, 25.0); // to the right
        let result = rect_line_intersection(&rect, center, target);
        assert!(result.is_some());
        let pt = result.unwrap();
        assert!((pt.x - 100.0).abs() < 0.01, "should hit right edge");
    }

    #[test]
    fn neighborhood_creation() {
        let n = Neighborhood::new("test".into());
        assert_eq!(n.leaf_uid, "test");
        assert!(n.sametail_contact_points.is_empty());
        assert!(n.other_contact_points.is_empty());
    }

    #[test]
    fn re_exports_work() {
        // Verify that re-exports compile correctly
        let _splines = DotSplines::default();
        let _version = GraphvizVersion::DEFAULT;
        let _state = ExeState::Ok;
        let _ps = ProcessState::TerminatedOk;
        assert_eq!(DOT_VERSION_LIMIT, 226);
        assert_eq!(DEFAULT_IMAGE_LIMIT, 4096);
    }
}
