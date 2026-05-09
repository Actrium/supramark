// d2-shape: shape library for d2 diagram layout engine.
//
// Ported from Go: /ext/d2/lib/shape/

use crate::geo::{Box2D, Ellipse, Intersectable, PathElement, Point, Segment};
use crate::svg_path::SvgPathContext;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_PADDING: f64 = 40.0;

// Shape type name constants
pub const SQUARE_TYPE: &str = "Square";
pub const REAL_SQUARE_TYPE: &str = "RealSquare";
pub const PARALLELOGRAM_TYPE: &str = "Parallelogram";
pub const DOCUMENT_TYPE: &str = "Document";
pub const CYLINDER_TYPE: &str = "Cylinder";
pub const QUEUE_TYPE: &str = "Queue";
pub const PAGE_TYPE: &str = "Page";
pub const PACKAGE_TYPE: &str = "Package";
pub const STEP_TYPE: &str = "Step";
pub const CALLOUT_TYPE: &str = "Callout";
pub const STORED_DATA_TYPE: &str = "StoredData";
pub const PERSON_TYPE: &str = "Person";
pub const C4_PERSON_TYPE: &str = "C4Person";
pub const DIAMOND_TYPE: &str = "Diamond";
pub const OVAL_TYPE: &str = "Oval";
pub const CIRCLE_TYPE: &str = "Circle";
pub const HEXAGON_TYPE: &str = "Hexagon";
pub const CLOUD_TYPE: &str = "Cloud";
pub const TABLE_TYPE: &str = "Table";
pub const CLASS_TYPE: &str = "Class";
pub const TEXT_TYPE: &str = "Text";
pub const CODE_TYPE: &str = "Code";
pub const IMAGE_TYPE: &str = "Image";

// ---------------------------------------------------------------------------
// ShapeOps trait
// ---------------------------------------------------------------------------

pub trait ShapeOps {
    fn get_type(&self) -> &str;
    fn get_box(&self) -> &Box2D;
    fn get_inner_box(&self) -> Box2D;
    fn get_dimensions_to_fit(&self, w: f64, h: f64, px: f64, py: f64) -> (f64, f64);
    fn get_default_padding(&self) -> (f64, f64);
    fn perimeter(&self) -> Vec<Box<dyn Intersectable>>;
    fn get_svg_path_data(&self) -> Vec<String>;

    fn is(&self, shape_type: &str) -> bool {
        self.get_type() == shape_type
    }

    fn aspect_ratio_1(&self) -> bool {
        false
    }

    fn is_rectangular(&self) -> bool {
        false
    }

    fn get_inside_placement(
        &self,
        _width: f64,
        _height: f64,
        padding_x: f64,
        padding_y: f64,
    ) -> Point {
        let inner_tl = self.get_inner_box().top_left;
        Point::new(inner_tl.x + padding_x / 2.0, inner_tl.y + padding_y / 2.0)
    }

    fn get_inner_box_for_content(&self, _width: f64, _height: f64) -> Option<Box2D> {
        None
    }
}

// ---------------------------------------------------------------------------
// Shape enum
// ---------------------------------------------------------------------------

/// All supported shape types as an enum.
pub struct Shape {
    pub shape_type: String,
    pub bbox: Box2D,
    kind: ShapeKind,
}

enum ShapeKind {
    Square,
    RealSquare,
    Rectangle, // default fallback for unknown types
    Oval,
    Circle,
    Diamond,
    Hexagon,
    Cloud { inner_box_aspect_ratio: f64 },
    Person,
    C4Person,
    Cylinder,
    Queue,
    Package,
    Step,
    Callout,
    StoredData,
    Page,
    Parallelogram,
    Document,
    Text,
    Code,
    Class,
    Table,
    Image,
}

impl Shape {
    pub fn new(shape_type: &str, bbox: Box2D) -> Self {
        let kind = match shape_type {
            SQUARE_TYPE => ShapeKind::Square,
            REAL_SQUARE_TYPE => ShapeKind::RealSquare,
            OVAL_TYPE => ShapeKind::Oval,
            CIRCLE_TYPE => ShapeKind::Circle,
            DIAMOND_TYPE => ShapeKind::Diamond,
            HEXAGON_TYPE => ShapeKind::Hexagon,
            CLOUD_TYPE => ShapeKind::Cloud {
                inner_box_aspect_ratio: 0.0,
            },
            PERSON_TYPE => ShapeKind::Person,
            C4_PERSON_TYPE => ShapeKind::C4Person,
            CYLINDER_TYPE => ShapeKind::Cylinder,
            QUEUE_TYPE => ShapeKind::Queue,
            PACKAGE_TYPE => ShapeKind::Package,
            STEP_TYPE => ShapeKind::Step,
            CALLOUT_TYPE => ShapeKind::Callout,
            STORED_DATA_TYPE => ShapeKind::StoredData,
            PAGE_TYPE => ShapeKind::Page,
            PARALLELOGRAM_TYPE => ShapeKind::Parallelogram,
            DOCUMENT_TYPE => ShapeKind::Document,
            TEXT_TYPE => ShapeKind::Text,
            CODE_TYPE => ShapeKind::Code,
            CLASS_TYPE => ShapeKind::Class,
            TABLE_TYPE => ShapeKind::Table,
            IMAGE_TYPE => ShapeKind::Image,
            _ => ShapeKind::Rectangle,
        };
        Shape {
            shape_type: shape_type.to_string(),
            bbox,
            kind,
        }
    }

    /// Set the inner box aspect ratio (only used by cloud shape).
    pub fn set_inner_box_aspect_ratio(&mut self, aspect_ratio: f64) {
        if let ShapeKind::Cloud {
            ref mut inner_box_aspect_ratio,
        } = self.kind
        {
            *inner_box_aspect_ratio = aspect_ratio;
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: limit aspect ratio
// ---------------------------------------------------------------------------

pub fn limit_ar(width: f64, height: f64, aspect_ratio: f64) -> (f64, f64) {
    let mut w = width;
    let mut h = height;
    if w > aspect_ratio * h {
        h = (w / aspect_ratio).round();
    } else if h > aspect_ratio * w {
        w = (h / aspect_ratio).round();
    }
    (w, h)
}

// ---------------------------------------------------------------------------
// Helper: convert path elements to trait objects
// ---------------------------------------------------------------------------

fn path_elements_to_intersectables(elements: Vec<PathElement>) -> Vec<Box<dyn Intersectable>> {
    elements
        .into_iter()
        .map(|e| -> Box<dyn Intersectable> {
            match e {
                PathElement::Segment(s) => Box::new(s),
                PathElement::Bezier(b) => Box::new(b),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Shape-specific path builders
// ---------------------------------------------------------------------------

// -- Diamond --

fn diamond_path(bbox: &Box2D) -> SvgPathContext {
    let mut pc = SvgPathContext::new(bbox.top_left, bbox.width / 77.0, bbox.height / 76.9);
    pc.start_at(pc.absolute(38.5, 76.9));
    pc.c(true, [(-0.3, 0.0), (-0.5, -0.1), (-0.7, -0.3)]);
    pc.l(false, 0.3, 39.2);
    pc.c(true, [(-0.4, -0.4), (-0.4, -1.0), (0.0, -1.4)]);
    pc.l(false, 37.8, 0.3);
    pc.c(true, [(0.4, -0.4), (1.0, -0.4), (1.4, 0.0)]);
    pc.l(true, 37.5, 37.5);
    pc.c(true, [(0.4, 0.4), (0.4, 1.0), (0.0, 1.4)]);
    pc.l(false, 39.2, 76.6);
    pc.c(false, [(39.0, 76.8), (38.8, 76.9), (38.5, 76.9)]);
    pc.z();
    pc
}

// -- Hexagon --

fn hexagon_path(bbox: &Box2D) -> SvgPathContext {
    let half_y_factor = 43.6 / 87.3;
    let mut pc = SvgPathContext::new(bbox.top_left, bbox.width, bbox.height);
    pc.start_at(pc.absolute(0.25, 0.0));
    pc.l(false, 0.0, half_y_factor);
    pc.l(false, 0.25, 1.0);
    pc.l(false, 0.75, 1.0);
    pc.l(false, 1.0, half_y_factor);
    pc.l(false, 0.75, 0.0);
    pc.z();
    pc
}

// -- Cloud --

const CLOUD_WIDE_INNER_X: f64 = 0.085;
const CLOUD_WIDE_INNER_Y: f64 = 0.409;
const CLOUD_WIDE_INNER_WIDTH: f64 = 0.819;
const CLOUD_WIDE_INNER_HEIGHT: f64 = 0.548;
const CLOUD_WIDE_ASPECT_BOUNDARY: f64 =
    (1.0 + CLOUD_WIDE_INNER_WIDTH / CLOUD_WIDE_INNER_HEIGHT) / 2.0;

const CLOUD_TALL_INNER_X: f64 = 0.228;
const CLOUD_TALL_INNER_Y: f64 = 0.179;
const CLOUD_TALL_INNER_WIDTH: f64 = 0.549;
const CLOUD_TALL_INNER_HEIGHT: f64 = 0.820;
const CLOUD_TALL_ASPECT_BOUNDARY: f64 =
    (1.0 + CLOUD_TALL_INNER_WIDTH / CLOUD_TALL_INNER_HEIGHT) / 2.0;

const CLOUD_SQUARE_INNER_X: f64 = 0.167;
const CLOUD_SQUARE_INNER_Y: f64 = 0.335;
const CLOUD_SQUARE_INNER_WIDTH: f64 = 0.663;
const CLOUD_SQUARE_INNER_HEIGHT: f64 = 0.663;

fn cloud_path(bbox: &Box2D) -> SvgPathContext {
    let mut pc = SvgPathContext::new(bbox.top_left, bbox.width / 834.0, bbox.height / 523.0);
    pc.start_at(pc.absolute(137.833, 182.833));
    pc.c(true, [(0.0, 5.556), (-5.556, 11.111), (-11.111, 11.111)]);
    pc.c(
        true,
        [(-70.833, 6.944), (-126.389, 77.778), (-126.389, 163.889)],
    );
    pc.c(true, [(0.0, 91.667), (62.5, 165.278), (141.667, 165.278)]);
    pc.h(true, 537.5);
    pc.c(true, [(84.723, 0.0), (154.167, -79.167), (154.167, -175.0)]);
    pc.c(
        true,
        [(0.0, -91.667), (-63.89, -168.056), (-144.444, -173.611)],
    );
    pc.c(true, [(-5.556, 0.0), (-11.111, -4.167), (-12.5, -11.111)]);
    pc.c(
        true,
        [(-18.056, -93.055), (-101.39, -162.5), (-198.611, -162.5)],
    );
    pc.c(true, [(-63.889, 0.0), (-120.834, 29.167), (-156.944, 75.0)]);
    pc.c(true, [(-4.167, 5.556), (-11.111, 6.945), (-15.278, 5.556)]);
    pc.c(
        true,
        [(-13.889, -5.556), (-29.166, -8.333), (-45.833, -8.333)],
    );
    pc.c(
        false,
        [(196.167, 71.722), (143.389, 120.333), (137.833, 182.833)],
    );
    pc.z();
    pc
}

fn cloud_get_inside_placement(
    bbox: &Box2D,
    width: f64,
    height: f64,
    padding_x: f64,
    padding_y: f64,
) -> Point {
    let w = width + padding_x;
    let h = height + padding_y;
    let aspect_ratio = w / h;
    if aspect_ratio > CLOUD_WIDE_ASPECT_BOUNDARY {
        Point::new(
            bbox.top_left.x + (bbox.width * CLOUD_WIDE_INNER_X + padding_x / 2.0).ceil(),
            bbox.top_left.y + (bbox.height * CLOUD_WIDE_INNER_Y + padding_y / 2.0).ceil(),
        )
    } else if aspect_ratio < CLOUD_TALL_ASPECT_BOUNDARY {
        Point::new(
            bbox.top_left.x + (bbox.width * CLOUD_TALL_INNER_X + padding_x / 2.0).ceil(),
            bbox.top_left.y + (bbox.height * CLOUD_TALL_INNER_Y + padding_y / 2.0).ceil(),
        )
    } else {
        Point::new(
            bbox.top_left.x + (bbox.width * CLOUD_SQUARE_INNER_X + padding_x / 2.0).ceil(),
            bbox.top_left.y + (bbox.height * CLOUD_SQUARE_INNER_Y + padding_y / 2.0).ceil(),
        )
    }
}

fn cloud_get_inner_box_for_content(bbox: &Box2D, width: f64, height: f64) -> Box2D {
    let inside_tl = cloud_get_inside_placement(bbox, width, height, 0.0, 0.0);
    let aspect_ratio = width / height;
    let (w, h) = if aspect_ratio > CLOUD_WIDE_ASPECT_BOUNDARY {
        (
            bbox.width * CLOUD_WIDE_INNER_WIDTH,
            bbox.height * CLOUD_WIDE_INNER_HEIGHT,
        )
    } else if aspect_ratio < CLOUD_TALL_ASPECT_BOUNDARY {
        (
            bbox.width * CLOUD_TALL_INNER_WIDTH,
            bbox.height * CLOUD_TALL_INNER_HEIGHT,
        )
    } else {
        (
            bbox.width * CLOUD_SQUARE_INNER_WIDTH,
            bbox.height * CLOUD_SQUARE_INNER_HEIGHT,
        )
    };
    Box2D::new(inside_tl, w, h)
}

// -- Person --

const PERSON_AR_LIMIT: f64 = 1.5;
const PERSON_SHOULDER_WIDTH_FACTOR: f64 = 20.2 / 68.3;

fn person_path(bbox: &Box2D) -> SvgPathContext {
    let mut pc = SvgPathContext::new(bbox.top_left, bbox.width / 68.3, bbox.height / 77.4);
    pc.start_at(pc.absolute(68.3, 77.4));
    pc.h(false, 0.0);
    pc.v(true, -1.1);
    pc.c(true, [(0.0, -13.2), (7.5, -25.1), (19.3, -30.8)]);
    pc.c(false, [(12.8, 40.9), (8.9, 33.4), (8.9, 25.2)]);
    pc.c(false, [(8.9, 11.3), (20.2, 0.0), (34.1, 0.0)]);
    // s 25.2,11.3, 25.2,25.2 -> mirrored last control point
    pc.c(true, [(13.9, 0.0), (25.2, 11.3), (25.2, 25.2)]);
    pc.c(true, [(0.0, 8.2), (-3.8, 15.6), (-10.4, 20.4)]);
    pc.c(true, [(11.8, 5.7), (19.3, 17.6), (19.3, 30.8)]);
    pc.v(true, 1.0);
    pc.h(false, 68.3);
    pc.z();
    pc
}

// -- C4 Person --

const C4_PERSON_AR_LIMIT: f64 = 1.5;
const HEAD_RADIUS_FACTOR: f64 = 0.22;
const BODY_TOP_FACTOR: f64 = 0.8;
const CORNER_RADIUS_FACTOR: f64 = 0.175;

fn c4_person_body_path(bbox: &Box2D) -> SvgPathContext {
    let width = bbox.width;
    let height = bbox.height;
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);

    let head_radius = width * HEAD_RADIUS_FACTOR;
    let head_center_y = head_radius;
    let body_top = head_center_y + head_radius * BODY_TOP_FACTOR;
    let body_width = width;
    let body_height = height - body_top;
    let body_left = 0.0;

    let corner_radius = (width * CORNER_RADIUS_FACTOR).min(body_height * 0.25);
    let k = 4.0 * (std::f64::consts::SQRT_2 - 1.0) / 3.0;

    pc.start_at(pc.absolute(body_left, body_top + corner_radius));
    pc.c(
        true,
        [
            (0.0, -k * corner_radius),
            (k * corner_radius, -corner_radius),
            (corner_radius, -corner_radius),
        ],
    );
    pc.h(true, body_width - 2.0 * corner_radius);
    pc.c(
        true,
        [
            (k * corner_radius, 0.0),
            (corner_radius, k * corner_radius),
            (corner_radius, corner_radius),
        ],
    );
    pc.v(true, body_height - 2.0 * corner_radius);
    pc.c(
        true,
        [
            (0.0, k * corner_radius),
            (-k * corner_radius, corner_radius),
            (-corner_radius, corner_radius),
        ],
    );
    pc.h(true, -(body_width - 2.0 * corner_radius));
    pc.c(
        true,
        [
            (-k * corner_radius, 0.0),
            (-corner_radius, -k * corner_radius),
            (-corner_radius, -corner_radius),
        ],
    );
    pc.z();
    pc
}

fn c4_person_head_path(bbox: &Box2D) -> SvgPathContext {
    let width = bbox.width;
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);

    let head_radius = width * HEAD_RADIUS_FACTOR;
    let head_center_x = width / 2.0;
    let head_center_y = head_radius;
    let k = 4.0 * (std::f64::consts::SQRT_2 - 1.0) / 3.0;

    pc.start_at(pc.absolute(head_center_x, head_center_y - head_radius));
    pc.c(
        false,
        [
            (head_center_x + head_radius * k, head_center_y - head_radius),
            (head_center_x + head_radius, head_center_y - head_radius * k),
            (head_center_x + head_radius, head_center_y),
        ],
    );
    pc.c(
        false,
        [
            (head_center_x + head_radius, head_center_y + head_radius * k),
            (head_center_x + head_radius * k, head_center_y + head_radius),
            (head_center_x, head_center_y + head_radius),
        ],
    );
    pc.c(
        false,
        [
            (head_center_x - head_radius * k, head_center_y + head_radius),
            (head_center_x - head_radius, head_center_y + head_radius * k),
            (head_center_x - head_radius, head_center_y),
        ],
    );
    pc.c(
        false,
        [
            (head_center_x - head_radius, head_center_y - head_radius * k),
            (head_center_x - head_radius * k, head_center_y - head_radius),
            (head_center_x, head_center_y - head_radius),
        ],
    );
    pc
}

// -- Cylinder --

const DEFAULT_ARC_DEPTH: f64 = 24.0;

fn get_arc_height(bbox: &Box2D) -> f64 {
    let mut arc_height = DEFAULT_ARC_DEPTH;
    if bbox.height < arc_height * 2.0 {
        arc_height = bbox.height / 2.0;
    }
    arc_height
}

fn cylinder_outer_path(bbox: &Box2D) -> SvgPathContext {
    let arc_height = get_arc_height(bbox);
    let multiplier = 0.45;
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);
    pc.start_at(pc.absolute(0.0, arc_height));
    pc.c(
        false,
        [
            (0.0, 0.0),
            (bbox.width * multiplier, 0.0),
            (bbox.width / 2.0, 0.0),
        ],
    );
    pc.c(
        false,
        [
            (bbox.width - bbox.width * multiplier, 0.0),
            (bbox.width, 0.0),
            (bbox.width, arc_height),
        ],
    );
    pc.v(true, bbox.height - arc_height * 2.0);
    pc.c(
        false,
        [
            (bbox.width, bbox.height),
            (bbox.width - bbox.width * multiplier, bbox.height),
            (bbox.width / 2.0, bbox.height),
        ],
    );
    pc.c(
        false,
        [
            (bbox.width * multiplier, bbox.height),
            (0.0, bbox.height),
            (0.0, bbox.height - arc_height),
        ],
    );
    pc.v(true, -(bbox.height - arc_height * 2.0));
    pc.z();
    pc
}

fn cylinder_inner_path(bbox: &Box2D) -> SvgPathContext {
    let arc_height = get_arc_height(bbox);
    let multiplier = 0.45;
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);
    pc.start_at(pc.absolute(0.0, arc_height));
    pc.c(
        false,
        [
            (0.0, arc_height * 2.0),
            (bbox.width * multiplier, arc_height * 2.0),
            (bbox.width / 2.0, arc_height * 2.0),
        ],
    );
    pc.c(
        false,
        [
            (bbox.width - bbox.width * multiplier, arc_height * 2.0),
            (bbox.width, arc_height * 2.0),
            (bbox.width, arc_height),
        ],
    );
    pc
}

// -- Queue --

fn get_arc_width(bbox: &Box2D) -> f64 {
    let mut arc_width = DEFAULT_ARC_DEPTH;
    if bbox.width < arc_width * 2.0 {
        arc_width = bbox.width / 2.0;
    }
    arc_width
}

fn queue_outer_path(bbox: &Box2D) -> SvgPathContext {
    let arc_width = get_arc_width(bbox);
    let multiplier = 0.45;
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);
    pc.start_at(pc.absolute(arc_width, 0.0));
    pc.h(true, bbox.width - 2.0 * arc_width);
    pc.c(
        false,
        [
            (bbox.width, 0.0),
            (bbox.width, bbox.height * multiplier),
            (bbox.width, bbox.height / 2.0),
        ],
    );
    pc.c(
        false,
        [
            (bbox.width, bbox.height - bbox.height * multiplier),
            (bbox.width, bbox.height),
            (bbox.width - arc_width, bbox.height),
        ],
    );
    pc.h(true, -(bbox.width - 2.0 * arc_width));
    pc.c(
        false,
        [
            (0.0, bbox.height),
            (0.0, bbox.height - bbox.height * multiplier),
            (0.0, bbox.height / 2.0),
        ],
    );
    pc.c(
        false,
        [
            (0.0, bbox.height * multiplier),
            (0.0, 0.0),
            (arc_width, 0.0),
        ],
    );
    pc.z();
    pc
}

fn queue_inner_path(bbox: &Box2D) -> SvgPathContext {
    let arc_width = get_arc_width(bbox);
    let multiplier = 0.45;
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);
    pc.start_at(pc.absolute(bbox.width - arc_width, 0.0));
    pc.c(
        false,
        [
            (bbox.width - 2.0 * arc_width, 0.0),
            (bbox.width - 2.0 * arc_width, bbox.height * multiplier),
            (bbox.width - 2.0 * arc_width, bbox.height / 2.0),
        ],
    );
    pc.c(
        false,
        [
            (
                bbox.width - 2.0 * arc_width,
                bbox.height - bbox.height * multiplier,
            ),
            (bbox.width - 2.0 * arc_width, bbox.height),
            (bbox.width - arc_width, bbox.height),
        ],
    );
    pc
}

// -- Package --

const _PACKAGE_TOP_MIN_HEIGHT: f64 = 34.0;
const PACKAGE_TOP_MAX_HEIGHT: f64 = 55.0;
const PACKAGE_TOP_MIN_WIDTH: f64 = 50.0;
const PACKAGE_TOP_MAX_WIDTH: f64 = 150.0;
const PACKAGE_HORIZONTAL_SCALAR: f64 = 0.5;
const PACKAGE_VERTICAL_SCALAR: f64 = 0.2;

fn get_top_dimensions(bbox: &Box2D) -> (f64, f64) {
    let mut width = bbox.width * PACKAGE_HORIZONTAL_SCALAR;
    if bbox.width >= 2.0 * PACKAGE_TOP_MIN_WIDTH {
        width = width.clamp(PACKAGE_TOP_MIN_WIDTH, PACKAGE_TOP_MAX_WIDTH);
    }
    let height = (bbox.height * PACKAGE_VERTICAL_SCALAR).min(PACKAGE_TOP_MAX_HEIGHT);
    (width, height)
}

fn package_path(bbox: &Box2D) -> SvgPathContext {
    let (top_width, top_height) = get_top_dimensions(bbox);
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);
    pc.start_at(pc.absolute(0.0, 0.0));
    pc.l(false, top_width, 0.0);
    pc.l(false, top_width, top_height);
    pc.l(false, bbox.width, top_height);
    pc.l(false, bbox.width, bbox.height);
    pc.l(false, 0.0, bbox.height);
    pc.z();
    pc
}

// -- Step --

const STEP_WEDGE_WIDTH: f64 = 35.0;

fn step_path(bbox: &Box2D) -> SvgPathContext {
    let mut wedge_width = STEP_WEDGE_WIDTH;
    if bbox.width <= wedge_width {
        wedge_width = bbox.width / 2.0;
    }
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);
    pc.start_at(pc.absolute(0.0, 0.0));
    pc.l(false, bbox.width - wedge_width, 0.0);
    pc.l(false, bbox.width, bbox.height / 2.0);
    pc.l(false, bbox.width - wedge_width, bbox.height);
    pc.l(false, 0.0, bbox.height);
    pc.l(false, wedge_width, bbox.height / 2.0);
    pc.z();
    pc
}

// -- Callout --

const DEFAULT_TIP_WIDTH: f64 = 30.0;
const DEFAULT_TIP_HEIGHT: f64 = 45.0;

fn get_tip_width(bbox: &Box2D) -> f64 {
    let tip_width = DEFAULT_TIP_WIDTH;
    if bbox.width < tip_width * 2.0 {
        bbox.width / 2.0
    } else {
        tip_width
    }
}

fn get_tip_height(bbox: &Box2D) -> f64 {
    let tip_height = DEFAULT_TIP_HEIGHT;
    if bbox.height < tip_height * 2.0 {
        bbox.height / 2.0
    } else {
        tip_height
    }
}

fn callout_path(bbox: &Box2D) -> SvgPathContext {
    let tip_width = get_tip_width(bbox);
    let tip_height = get_tip_height(bbox);
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);
    pc.start_at(pc.absolute(0.0, 0.0));
    pc.v(true, bbox.height - tip_height);
    pc.h(true, bbox.width / 2.0);
    pc.v(true, tip_height);
    pc.l(true, tip_width, -tip_height);
    pc.h(true, bbox.width / 2.0 - tip_width);
    pc.v(true, -(bbox.height - tip_height));
    pc.h(true, -bbox.width);
    pc.z();
    pc
}

// -- Stored Data --

const STORED_DATA_WEDGE_WIDTH: f64 = 15.0;

fn stored_data_path(bbox: &Box2D) -> SvgPathContext {
    let mut wedge_width = STORED_DATA_WEDGE_WIDTH;
    let multiplier = 0.27;
    if bbox.width < wedge_width * 2.0 {
        wedge_width = bbox.width / 2.0;
    }
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);
    pc.start_at(pc.absolute(wedge_width, 0.0));
    pc.h(true, bbox.width - wedge_width);
    pc.c(
        false,
        [
            (bbox.width - wedge_width * multiplier, 0.0),
            (bbox.width - wedge_width, bbox.height * multiplier),
            (bbox.width - wedge_width, bbox.height / 2.0),
        ],
    );
    pc.c(
        false,
        [
            (
                bbox.width - wedge_width,
                bbox.height - bbox.height * multiplier,
            ),
            (bbox.width - wedge_width * multiplier, bbox.height),
            (bbox.width, bbox.height),
        ],
    );
    pc.h(true, -(bbox.width - wedge_width));
    pc.c(
        false,
        [
            (wedge_width - wedge_width * multiplier, bbox.height),
            (0.0, bbox.height - bbox.height * multiplier),
            (0.0, bbox.height / 2.0),
        ],
    );
    pc.c(
        false,
        [
            (0.0, bbox.height * multiplier),
            (wedge_width - wedge_width * multiplier, 0.0),
            (wedge_width, 0.0),
        ],
    );
    pc.z();
    pc
}

// -- Page --

const PAGE_CORNER_WIDTH: f64 = 20.8164;
const PAGE_CORNER_HEIGHT: f64 = 20.348;

fn page_outer_path(bbox: &Box2D) -> SvgPathContext {
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);
    pc.start_at(pc.absolute(0.5, 0.0));
    pc.h(false, bbox.width - 20.8164);
    pc.c(
        false,
        [
            (bbox.width - 19.6456, 0.0),
            (bbox.width - 18.521, 0.456297),
            (bbox.width - 17.6811, 1.27202),
        ],
    );
    pc.l(false, bbox.width - 1.3647, 17.12);
    pc.c(
        false,
        [
            (bbox.width - 0.4923, 17.9674),
            (bbox.width, 19.1318),
            (bbox.width, 20.348),
        ],
    );
    pc.v(false, bbox.height - 0.5);
    pc.c(
        false,
        [
            (bbox.width, bbox.height - 0.2239),
            (bbox.width - 0.2239, bbox.height),
            (bbox.width - 0.5, bbox.height),
        ],
    );
    pc.h(false, 0.499999);
    pc.c(
        false,
        [
            (0.223857, bbox.height),
            (0.0, bbox.height - 0.2239),
            (0.0, bbox.height - 0.5),
        ],
    );
    pc.v(false, 0.499999);
    pc.c(false, [(0.0, 0.223857), (0.223857, 0.0), (0.5, 0.0)]);
    pc.z();
    pc
}

fn page_inner_path(bbox: &Box2D) -> SvgPathContext {
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);
    pc.start_at(pc.absolute(bbox.width - 1.08197, bbox.height));
    pc.h(false, 1.08196);
    pc.c(
        true,
        [(-0.64918, 0.0), (-1.08196, -0.43287), (-1.08196, -1.08219)],
    );
    pc.v(false, 1.08219);
    pc.c(
        true,
        [(0.0, -0.64931), (0.43278, -1.08219), (1.08196, -1.08219)],
    );
    pc.h(true, bbox.width - 22.72132);
    pc.c(
        true,
        [(0.64918, 0.0), (1.08196, 0.43287), (1.08196, 1.08219)],
    );
    pc.v(true, 17.09863);
    pc.c(
        true,
        [(0.0, 1.29863), (0.86557, 2.38082), (2.38032, 2.38082)],
    );
    pc.h(false, bbox.width - 1.08197);
    pc.c(
        true,
        [(0.64918, 0.0), (1.08196, 0.43287), (1.08196, 1.08196)],
    );
    pc.v(false, bbox.height - 1.0822);
    pc.c(
        false,
        [
            (bbox.width - 1.0, bbox.height - 0.43288),
            (bbox.width - 0.43279, bbox.height),
            (bbox.width - 1.08197, bbox.height),
        ],
    );
    pc.z();
    pc
}

// -- Parallelogram --

const PARALLEL_WEDGE_WIDTH: f64 = 26.0;

fn parallelogram_path(bbox: &Box2D) -> SvgPathContext {
    let mut wedge_width = PARALLEL_WEDGE_WIDTH;
    if bbox.width <= wedge_width {
        wedge_width = bbox.width / 2.0;
    }
    let mut pc = SvgPathContext::new(bbox.top_left, 1.0, 1.0);
    pc.start_at(pc.absolute(wedge_width, 0.0));
    pc.l(false, bbox.width, 0.0);
    pc.l(false, bbox.width - wedge_width, bbox.height);
    pc.l(false, 0.0, bbox.height);
    pc.l(false, 0.0, bbox.height); // matches Go: L(false, 0, box.Height)
    pc.z();
    pc
}

// -- Document --

const DOC_PATH_HEIGHT: f64 = 18.925;
const DOC_PATH_INNER_BOTTOM: f64 = 14.0;
const DOC_PATH_BOTTOM: f64 = 16.3;

fn document_path(bbox: &Box2D) -> SvgPathContext {
    let mut pc = SvgPathContext::new(bbox.top_left, bbox.width, bbox.height);
    pc.start_at(pc.absolute(0.0, DOC_PATH_BOTTOM / DOC_PATH_HEIGHT));
    pc.l(false, 0.0, 0.0);
    pc.l(false, 1.0, 0.0);
    pc.l(false, 1.0, DOC_PATH_BOTTOM / DOC_PATH_HEIGHT);
    pc.c(
        false,
        [
            (5.0 / 6.0, 12.8 / DOC_PATH_HEIGHT),
            (2.0 / 3.0, 12.8 / DOC_PATH_HEIGHT),
            (1.0 / 2.0, DOC_PATH_BOTTOM / DOC_PATH_HEIGHT),
        ],
    );
    pc.c(
        false,
        [
            (1.0 / 3.0, 19.8 / DOC_PATH_HEIGHT),
            (1.0 / 6.0, 19.8 / DOC_PATH_HEIGHT),
            (0.0, DOC_PATH_BOTTOM / DOC_PATH_HEIGHT),
        ],
    );
    pc.z();
    pc
}

// -- Oval get_inside_placement --

fn oval_get_inside_placement(
    bbox: &Box2D,
    _width: f64,
    _height: f64,
    padding_x: f64,
    padding_y: f64,
) -> Point {
    let rx = bbox.width / 2.0;
    let ry = bbox.height / 2.0;
    let theta = (ry.atan2(rx) as f32) as f64;
    let sin = theta.sin();
    let cos = theta.cos();
    let r = rx * ry / (((rx * sin).powi(2) + (ry * cos).powi(2)).sqrt());
    Point::new(
        bbox.top_left.x + (rx - cos * (r - padding_x / 2.0)).ceil(),
        bbox.top_left.y + (ry - sin * (r - padding_y / 2.0)).ceil(),
    )
}

// ---------------------------------------------------------------------------
// ShapeOps implementation for Shape
// ---------------------------------------------------------------------------

impl ShapeOps for Shape {
    fn get_type(&self) -> &str {
        &self.shape_type
    }

    fn get_box(&self) -> &Box2D {
        &self.bbox
    }

    fn aspect_ratio_1(&self) -> bool {
        matches!(self.kind, ShapeKind::RealSquare | ShapeKind::Circle)
    }

    fn is_rectangular(&self) -> bool {
        matches!(
            self.kind,
            ShapeKind::Square
                | ShapeKind::RealSquare
                | ShapeKind::Rectangle
                | ShapeKind::Image
                | ShapeKind::Text
                | ShapeKind::Code
                | ShapeKind::Class
                | ShapeKind::Table
        )
    }

    fn get_inner_box(&self) -> Box2D {
        let bbox = &self.bbox;
        match &self.kind {
            ShapeKind::Oval => {
                let inside_tl = oval_get_inside_placement(bbox, bbox.width, bbox.height, 0.0, 0.0);
                let tl = bbox.top_left;
                let width = bbox.width - 2.0 * (inside_tl.x - tl.x);
                let height = bbox.height - 2.0 * (inside_tl.y - tl.y);
                Box2D::new(inside_tl, width, height)
            }
            ShapeKind::Circle => {
                let r = bbox.width / 2.0;
                let half_length = r * std::f64::consts::SQRT_2 / 2.0;
                let inside_tl = Point::new(
                    bbox.top_left.x + (r - half_length).ceil(),
                    bbox.top_left.y + (r - half_length).ceil(),
                );
                let tl = bbox.top_left;
                let width = bbox.width - 2.0 * (inside_tl.x - tl.x);
                let height = bbox.height - 2.0 * (inside_tl.y - tl.y);
                Box2D::new(inside_tl, width, height)
            }
            ShapeKind::Diamond => {
                let tl = Point::new(
                    bbox.top_left.x + bbox.width / 4.0,
                    bbox.top_left.y + bbox.height / 4.0,
                );
                Box2D::new(tl, bbox.width / 2.0, bbox.height / 2.0)
            }
            ShapeKind::Hexagon => {
                let tl = Point::new(
                    bbox.top_left.x + bbox.width / 6.0,
                    bbox.top_left.y + bbox.height / 6.0,
                );
                Box2D::new(tl, bbox.width / 1.5, bbox.height / 1.5)
            }
            ShapeKind::Cloud {
                inner_box_aspect_ratio,
            } => {
                if *inner_box_aspect_ratio != 0.0 {
                    cloud_get_inner_box_for_content(bbox, *inner_box_aspect_ratio, 1.0)
                } else {
                    cloud_get_inner_box_for_content(bbox, bbox.width, bbox.height)
                }
            }
            ShapeKind::Person => {
                let shoulder_width = PERSON_SHOULDER_WIDTH_FACTOR * bbox.width;
                let tl = Point::new(bbox.top_left.x + shoulder_width, bbox.top_left.y);
                Box2D::new(tl, bbox.width - shoulder_width * 2.0, bbox.height)
            }
            ShapeKind::C4Person => {
                let head_radius = bbox.width * HEAD_RADIUS_FACTOR;
                let head_center_y = head_radius;
                let body_top = head_center_y + head_radius * BODY_TOP_FACTOR;
                let horizontal_padding = bbox.width * 0.05;
                let vertical_padding = bbox.height * 0.03;
                let tl = Point::new(
                    bbox.top_left.x + horizontal_padding,
                    bbox.top_left.y + body_top + vertical_padding,
                );
                let inner_width = bbox.width - horizontal_padding * 2.0;
                let inner_height = bbox.height - body_top - vertical_padding * 2.0;
                Box2D::new(tl, inner_width, inner_height)
            }
            ShapeKind::Cylinder => {
                let arc = get_arc_height(bbox);
                let tl = Point::new(bbox.top_left.x, bbox.top_left.y + 2.0 * arc);
                Box2D::new(tl, bbox.width, bbox.height - 3.0 * arc)
            }
            ShapeKind::Queue => {
                let arc_width = get_arc_width(bbox);
                let tl = Point::new(bbox.top_left.x + arc_width, bbox.top_left.y);
                Box2D::new(tl, bbox.width - 3.0 * arc_width, bbox.height)
            }
            ShapeKind::Package => {
                let (_, top_height) = get_top_dimensions(bbox);
                let tl = Point::new(bbox.top_left.x, bbox.top_left.y + top_height);
                Box2D::new(tl, bbox.width, bbox.height - top_height)
            }
            ShapeKind::Step => {
                let tl = Point::new(bbox.top_left.x + STEP_WEDGE_WIDTH, bbox.top_left.y);
                Box2D::new(tl, bbox.width - 2.0 * STEP_WEDGE_WIDTH, bbox.height)
            }
            ShapeKind::Callout => {
                let tip_height = get_tip_height(bbox);
                Box2D::new(bbox.top_left, bbox.width, bbox.height - tip_height)
            }
            ShapeKind::StoredData => {
                let tl = Point::new(bbox.top_left.x + STORED_DATA_WEDGE_WIDTH, bbox.top_left.y);
                Box2D::new(tl, bbox.width - 2.0 * STORED_DATA_WEDGE_WIDTH, bbox.height)
            }
            ShapeKind::Page => {
                let mut width = bbox.width;
                if bbox.height < 3.0 * PAGE_CORNER_HEIGHT {
                    width -= PAGE_CORNER_WIDTH;
                }
                Box2D::new(bbox.top_left, width, bbox.height)
            }
            ShapeKind::Parallelogram => {
                let tl = Point::new(bbox.top_left.x + PARALLEL_WEDGE_WIDTH, bbox.top_left.y);
                Box2D::new(tl, bbox.width - 2.0 * PARALLEL_WEDGE_WIDTH, bbox.height)
            }
            ShapeKind::Document => {
                let height = bbox.height * DOC_PATH_INNER_BOTTOM / DOC_PATH_HEIGHT;
                Box2D::new(bbox.top_left, bbox.width, height)
            }
            // Square, RealSquare, Rectangle, Text, Code, Class, Table, Image
            _ => *bbox,
        }
    }

    fn get_dimensions_to_fit(&self, w: f64, h: f64, px: f64, py: f64) -> (f64, f64) {
        match &self.kind {
            ShapeKind::RealSquare => {
                let side = (w + px).max(h + py).ceil();
                (side, side)
            }
            ShapeKind::Oval => {
                let theta = ((h.atan2(w)) as f32) as f64;
                let padded_width = w + px * theta.cos();
                let padded_height = h + py * theta.sin();
                let tw = (std::f64::consts::SQRT_2 * padded_width).ceil();
                let th = (std::f64::consts::SQRT_2 * padded_height).ceil();
                let (tw, th) = limit_ar(tw, th, 3.0);
                (tw, th)
            }
            ShapeKind::Circle => {
                let length = (w + px).max(h + py);
                let diameter = (std::f64::consts::SQRT_2 * length).ceil();
                (diameter, diameter)
            }
            ShapeKind::Diamond => {
                let tw = (2.0 * (w + px)).ceil();
                let th = (2.0 * (h + py)).ceil();
                (tw, th)
            }
            ShapeKind::Hexagon => {
                let tw = (1.5 * (w + px)).ceil();
                let th = (1.5 * (h + py)).ceil();
                (tw, th)
            }
            ShapeKind::Cloud { .. } => {
                let cw = w + px;
                let ch = h + py;
                let aspect_ratio = cw / ch;
                if aspect_ratio > CLOUD_WIDE_ASPECT_BOUNDARY {
                    (
                        (cw / CLOUD_WIDE_INNER_WIDTH).ceil(),
                        (ch / CLOUD_WIDE_INNER_HEIGHT).ceil(),
                    )
                } else if aspect_ratio < CLOUD_TALL_ASPECT_BOUNDARY {
                    (
                        (cw / CLOUD_TALL_INNER_WIDTH).ceil(),
                        (ch / CLOUD_TALL_INNER_HEIGHT).ceil(),
                    )
                } else {
                    (
                        (cw / CLOUD_SQUARE_INNER_WIDTH).ceil(),
                        (ch / CLOUD_SQUARE_INNER_HEIGHT).ceil(),
                    )
                }
            }
            ShapeKind::Person => {
                let total_width_base = w + px;
                let shoulder_width = total_width_base * PERSON_SHOULDER_WIDTH_FACTOR
                    / (1.0 - 2.0 * PERSON_SHOULDER_WIDTH_FACTOR);
                let total_width = total_width_base + 2.0 * shoulder_width;
                let total_height = h + py;
                let (tw, th) = limit_ar(total_width, total_height, PERSON_AR_LIMIT);
                (tw.ceil(), th.ceil())
            }
            ShapeKind::C4Person => {
                let content_width = w + px;
                let content_height = h + py;
                let total_width = content_width / 0.9;
                let head_radius = total_width * HEAD_RADIUS_FACTOR;
                let head_center_y = head_radius;
                let body_top = head_center_y + head_radius * BODY_TOP_FACTOR;
                let vertical_padding = total_width * 0.06;
                let mut total_height = content_height + body_top + vertical_padding;
                let min_height = total_width * 0.95;
                if total_height < min_height {
                    total_height = min_height;
                }
                let (tw, th) = limit_ar(total_width, total_height, C4_PERSON_AR_LIMIT);
                (tw.ceil(), th.ceil())
            }
            ShapeKind::Cylinder => {
                let th = h + py + 3.0 * DEFAULT_ARC_DEPTH;
                ((w + px).ceil(), th.ceil())
            }
            ShapeKind::Queue => {
                let tw = 3.0 * DEFAULT_ARC_DEPTH + w + px;
                (tw.ceil(), (h + py).ceil())
            }
            ShapeKind::Package => {
                let inner_height = h + py;
                let top_height =
                    inner_height * PACKAGE_VERTICAL_SCALAR / (1.0 - PACKAGE_VERTICAL_SCALAR);
                let total_height = inner_height + top_height.min(PACKAGE_TOP_MAX_HEIGHT);
                ((w + px).ceil(), total_height.ceil())
            }
            ShapeKind::Step => {
                let tw = w + px + 2.0 * STEP_WEDGE_WIDTH;
                (tw.ceil(), (h + py).ceil())
            }
            ShapeKind::Callout => {
                let mut base_height = h + py;
                if base_height < DEFAULT_TIP_HEIGHT {
                    base_height *= 2.0;
                } else {
                    base_height += DEFAULT_TIP_HEIGHT;
                }
                ((w + px).ceil(), base_height.ceil())
            }
            ShapeKind::StoredData => {
                let tw = w + px + 2.0 * STORED_DATA_WEDGE_WIDTH;
                (tw.ceil(), (h + py).ceil())
            }
            ShapeKind::Page => {
                let mut tw = w + px;
                let mut th = h + py;
                if th < 3.0 * PAGE_CORNER_HEIGHT {
                    tw += PAGE_CORNER_WIDTH;
                }
                tw = tw.max(2.0 * PAGE_CORNER_WIDTH);
                th = th.max(PAGE_CORNER_HEIGHT);
                (tw.ceil(), th.ceil())
            }
            ShapeKind::Parallelogram => {
                let tw = w + px + PARALLEL_WEDGE_WIDTH * 2.0;
                (tw.ceil(), (h + py).ceil())
            }
            ShapeKind::Document => {
                let base_height = (h + py) * DOC_PATH_HEIGHT / DOC_PATH_INNER_BOTTOM;
                ((w + px).ceil(), base_height.ceil())
            }
            // Square, Rectangle, Text, Code, Class, Table, Image
            _ => ((w + px).ceil(), (h + py).ceil()),
        }
    }

    fn get_default_padding(&self) -> (f64, f64) {
        match &self.kind {
            ShapeKind::Circle => (
                DEFAULT_PADDING / std::f64::consts::SQRT_2,
                DEFAULT_PADDING / std::f64::consts::SQRT_2,
            ),
            ShapeKind::Diamond => (DEFAULT_PADDING / 4.0, DEFAULT_PADDING / 2.0),
            ShapeKind::Hexagon => (DEFAULT_PADDING / 2.0, DEFAULT_PADDING / 2.0),
            ShapeKind::Cloud { .. } => (DEFAULT_PADDING, DEFAULT_PADDING / 2.0),
            ShapeKind::Person => (10.0, DEFAULT_PADDING),
            ShapeKind::C4Person => (10.0, DEFAULT_PADDING),
            ShapeKind::Cylinder => (DEFAULT_PADDING, DEFAULT_PADDING / 2.0),
            ShapeKind::Queue => (DEFAULT_PADDING / 2.0, DEFAULT_PADDING),
            ShapeKind::Package => (DEFAULT_PADDING, 0.8 * DEFAULT_PADDING),
            ShapeKind::Step => (DEFAULT_PADDING / 4.0, DEFAULT_PADDING + STEP_WEDGE_WIDTH),
            ShapeKind::Callout => (DEFAULT_PADDING, DEFAULT_PADDING / 2.0),
            ShapeKind::StoredData => (DEFAULT_PADDING - 10.0, DEFAULT_PADDING),
            ShapeKind::Page => (DEFAULT_PADDING, PAGE_CORNER_HEIGHT + DEFAULT_PADDING),
            ShapeKind::Document => (
                DEFAULT_PADDING,
                DEFAULT_PADDING * DOC_PATH_INNER_BOTTOM / DOC_PATH_HEIGHT,
            ),
            ShapeKind::Text
            | ShapeKind::Code
            | ShapeKind::Class
            | ShapeKind::Table
            | ShapeKind::Image => (0.0, 0.0),
            // Square, RealSquare, Rectangle, Parallelogram
            _ => (DEFAULT_PADDING, DEFAULT_PADDING),
        }
    }

    fn perimeter(&self) -> Vec<Box<dyn Intersectable>> {
        let bbox = &self.bbox;
        match &self.kind {
            ShapeKind::Oval | ShapeKind::Circle => {
                let center = bbox.center();
                let ellipse = Ellipse::new(center, bbox.width / 2.0, bbox.height / 2.0);
                vec![Box::new(ellipse)]
            }
            ShapeKind::Diamond => path_elements_to_intersectables(diamond_path(bbox).path),
            ShapeKind::Hexagon => path_elements_to_intersectables(hexagon_path(bbox).path),
            ShapeKind::Cloud { .. } => path_elements_to_intersectables(cloud_path(bbox).path),
            ShapeKind::Person => path_elements_to_intersectables(person_path(bbox).path),
            ShapeKind::C4Person => {
                let mut result = path_elements_to_intersectables(c4_person_body_path(bbox).path);
                let head_radius = bbox.width * HEAD_RADIUS_FACTOR;
                let head_center_x = bbox.top_left.x + bbox.width / 2.0;
                let head_center_y = bbox.top_left.y + head_radius;
                let head_center = Point::new(head_center_x, head_center_y);
                let head_ellipse = Ellipse::new(head_center, head_radius, head_radius);
                result.push(Box::new(head_ellipse));
                result
            }
            ShapeKind::Cylinder => path_elements_to_intersectables(cylinder_outer_path(bbox).path),
            ShapeKind::Queue => path_elements_to_intersectables(queue_outer_path(bbox).path),
            ShapeKind::Package => path_elements_to_intersectables(package_path(bbox).path),
            ShapeKind::Step => path_elements_to_intersectables(step_path(bbox).path),
            ShapeKind::Callout => path_elements_to_intersectables(callout_path(bbox).path),
            ShapeKind::StoredData => path_elements_to_intersectables(stored_data_path(bbox).path),
            ShapeKind::Page => path_elements_to_intersectables(page_outer_path(bbox).path),
            ShapeKind::Parallelogram => {
                path_elements_to_intersectables(parallelogram_path(bbox).path)
            }
            ShapeKind::Document => path_elements_to_intersectables(document_path(bbox).path),
            // Rectangular shapes and others have no custom perimeter
            _ => vec![],
        }
    }

    fn get_svg_path_data(&self) -> Vec<String> {
        let bbox = &self.bbox;
        match &self.kind {
            ShapeKind::Diamond => vec![diamond_path(bbox).path_data()],
            ShapeKind::Hexagon => vec![hexagon_path(bbox).path_data()],
            ShapeKind::Cloud { .. } => vec![cloud_path(bbox).path_data()],
            ShapeKind::Person => vec![person_path(bbox).path_data()],
            ShapeKind::C4Person => vec![
                c4_person_body_path(bbox).path_data(),
                c4_person_head_path(bbox).path_data(),
            ],
            ShapeKind::Cylinder => vec![
                cylinder_outer_path(bbox).path_data(),
                cylinder_inner_path(bbox).path_data(),
            ],
            ShapeKind::Queue => vec![
                queue_outer_path(bbox).path_data(),
                queue_inner_path(bbox).path_data(),
            ],
            ShapeKind::Package => vec![package_path(bbox).path_data()],
            ShapeKind::Step => vec![step_path(bbox).path_data()],
            ShapeKind::Callout => vec![callout_path(bbox).path_data()],
            ShapeKind::StoredData => vec![stored_data_path(bbox).path_data()],
            ShapeKind::Page => vec![
                page_outer_path(bbox).path_data(),
                page_inner_path(bbox).path_data(),
            ],
            ShapeKind::Parallelogram => vec![parallelogram_path(bbox).path_data()],
            ShapeKind::Document => vec![document_path(bbox).path_data()],
            // Shapes without custom SVG paths (rectangle, oval, circle etc.)
            _ => vec![],
        }
    }

    fn get_inside_placement(
        &self,
        width: f64,
        height: f64,
        padding_x: f64,
        padding_y: f64,
    ) -> Point {
        match &self.kind {
            ShapeKind::Oval => {
                oval_get_inside_placement(&self.bbox, width, height, padding_x, padding_y)
            }
            ShapeKind::Circle => {
                let r = self.bbox.width / 2.0;
                let half_length = r * std::f64::consts::SQRT_2 / 2.0;
                Point::new(
                    self.bbox.top_left.x + (r - half_length + padding_x / 2.0).ceil(),
                    self.bbox.top_left.y + (r - half_length + padding_y / 2.0).ceil(),
                )
            }
            ShapeKind::Cloud { .. } => {
                cloud_get_inside_placement(&self.bbox, width, height, padding_x, padding_y)
            }
            _ => {
                let inner_tl = self.get_inner_box().top_left;
                Point::new(inner_tl.x + padding_x / 2.0, inner_tl.y + padding_y / 2.0)
            }
        }
    }

    fn get_inner_box_for_content(&self, width: f64, height: f64) -> Option<Box2D> {
        match &self.kind {
            ShapeKind::Cloud { .. } => {
                Some(cloud_get_inner_box_for_content(&self.bbox, width, height))
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// TraceToShapeBorder
// ---------------------------------------------------------------------------

pub fn trace_to_shape_border(
    shape: &Shape,
    rect_border_point: &Point,
    prev_point: &Point,
) -> Point {
    if shape.shape_type.is_empty() || shape.is_rectangular() {
        return *rect_border_point;
    }

    let mut scale_size = shape.bbox.width;
    if prev_point.x == rect_border_point.x {
        scale_size = shape.bbox.height;
    }
    let vector = prev_point.vector_to(rect_border_point);
    let vector = vector.add_length(scale_size);
    let extended_end = prev_point.add_vector(&vector);
    let extended_segment = Segment::new(*prev_point, extended_end);

    let mut closest_d = f64::INFINITY;
    let mut closest_point = *rect_border_point;

    for perimiter_segment in shape.perimeter() {
        for intersecting_point in perimiter_segment.intersections(&extended_segment) {
            let d = crate::geo::euclidean_distance(
                rect_border_point.x,
                rect_border_point.y,
                intersecting_point.x,
                intersecting_point.y,
            );
            if d < closest_d {
                closest_d = d;
                closest_point = intersecting_point;
            }
        }
    }

    let mut cp = closest_point;
    cp.truncate_float32();
    Point::new(cp.x.round(), cp.y.round())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_box(x: f64, y: f64, w: f64, h: f64) -> Box2D {
        Box2D::new(Point::new(x, y), w, h)
    }

    #[test]
    fn test_shape_type_constants() {
        assert_eq!(SQUARE_TYPE, "Square");
        assert_eq!(DIAMOND_TYPE, "Diamond");
        assert_eq!(OVAL_TYPE, "Oval");
        assert_eq!(CLOUD_TYPE, "Cloud");
    }

    #[test]
    fn test_shape_new_and_get_type() {
        let s = Shape::new(SQUARE_TYPE, make_box(0.0, 0.0, 100.0, 100.0));
        assert_eq!(s.get_type(), SQUARE_TYPE);
        assert!(s.is(SQUARE_TYPE));
        assert!(s.is_rectangular());
    }

    #[test]
    fn test_rectangle_svg_path_data() {
        let s = Shape::new(SQUARE_TYPE, make_box(0.0, 0.0, 100.0, 50.0));
        // Rectangle shapes produce no custom SVG path data
        assert!(s.get_svg_path_data().is_empty());
    }

    #[test]
    fn test_diamond_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 77.0, 76.9);
        let s = Shape::new(DIAMOND_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 1);
        let path = &paths[0];
        // The path should start with M and end with Z.
        assert!(path.starts_with("M "), "path should start with M: {}", path);
        assert!(path.ends_with("Z"), "path should end with Z: {}", path);
        // After Go's chopPrecision the midpoint of a 77x76.9 diamond rounds
        // to (39, 77); make sure that landmark is present in the output.
        assert!(
            path.contains("39 77"),
            "path should contain rounded midpoint 39 77: {}",
            path
        );
    }

    #[test]
    fn test_diamond_svg_path_data_scaled() {
        let bbox = make_box(10.0, 20.0, 154.0, 153.8);
        let s = Shape::new(DIAMOND_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 1);
        let path = &paths[0];
        assert!(path.starts_with("M "), "path: {}", path);
        assert!(path.ends_with("Z"), "path: {}", path);
    }

    #[test]
    fn test_oval_perimeter_is_ellipse() {
        let bbox = make_box(0.0, 0.0, 200.0, 100.0);
        let s = Shape::new(OVAL_TYPE, bbox);
        let perimeter = s.perimeter();
        assert_eq!(perimeter.len(), 1);
        // Oval has no SVG path data
        assert!(s.get_svg_path_data().is_empty());
    }

    #[test]
    fn test_hexagon_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 100.0, 100.0);
        let s = Shape::new(HEXAGON_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 1);
        let path = &paths[0];
        assert!(path.starts_with("M "), "path: {}", path);
        assert!(path.ends_with("Z"), "path: {}", path);
    }

    #[test]
    fn test_cloud_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 834.0, 523.0);
        let s = Shape::new(CLOUD_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 1);
        let path = &paths[0];
        assert!(path.starts_with("M "), "path: {}", path);
        assert!(path.ends_with("Z"), "path: {}", path);
    }

    #[test]
    fn test_person_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 68.3, 77.4);
        let s = Shape::new(PERSON_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn test_cylinder_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 100.0, 200.0);
        let s = Shape::new(CYLINDER_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 2); // outer + inner
    }

    #[test]
    fn test_queue_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 200.0, 100.0);
        let s = Shape::new(QUEUE_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn test_page_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 100.0, 150.0);
        let s = Shape::new(PAGE_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 2); // outer + inner
    }

    #[test]
    fn test_c4_person_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 100.0, 150.0);
        let s = Shape::new(C4_PERSON_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 2); // body + head
    }

    #[test]
    fn test_default_padding_values() {
        let bbox = make_box(0.0, 0.0, 100.0, 100.0);

        let s = Shape::new(SQUARE_TYPE, bbox);
        assert_eq!(s.get_default_padding(), (40.0, 40.0));

        let s = Shape::new(DIAMOND_TYPE, bbox);
        assert_eq!(s.get_default_padding(), (10.0, 20.0));

        let s = Shape::new(TEXT_TYPE, bbox);
        assert_eq!(s.get_default_padding(), (0.0, 0.0));

        let s = Shape::new(IMAGE_TYPE, bbox);
        assert_eq!(s.get_default_padding(), (0.0, 0.0));
    }

    #[test]
    fn test_dimensions_to_fit_rectangle() {
        let bbox = make_box(0.0, 0.0, 200.0, 100.0);
        let s = Shape::new(SQUARE_TYPE, bbox);
        let (w, h) = s.get_dimensions_to_fit(80.0, 40.0, 10.0, 10.0);
        assert_eq!(w, 90.0);
        assert_eq!(h, 50.0);
    }

    #[test]
    fn test_dimensions_to_fit_diamond() {
        let bbox = make_box(0.0, 0.0, 200.0, 200.0);
        let s = Shape::new(DIAMOND_TYPE, bbox);
        let (w, h) = s.get_dimensions_to_fit(50.0, 30.0, 10.0, 10.0);
        assert_eq!(w, 120.0);
        assert_eq!(h, 80.0);
    }

    #[test]
    fn test_dimensions_to_fit_circle() {
        let bbox = make_box(0.0, 0.0, 100.0, 100.0);
        let s = Shape::new(CIRCLE_TYPE, bbox);
        let (w, h) = s.get_dimensions_to_fit(30.0, 30.0, 10.0, 10.0);
        assert_eq!(w, h); // circle must be equal
    }

    #[test]
    fn test_inner_box_diamond() {
        let bbox = make_box(0.0, 0.0, 200.0, 200.0);
        let s = Shape::new(DIAMOND_TYPE, bbox);
        let inner = s.get_inner_box();
        assert_eq!(inner.top_left.x, 50.0);
        assert_eq!(inner.top_left.y, 50.0);
        assert_eq!(inner.width, 100.0);
        assert_eq!(inner.height, 100.0);
    }

    #[test]
    fn test_inner_box_cylinder() {
        let bbox = make_box(0.0, 0.0, 100.0, 200.0);
        let s = Shape::new(CYLINDER_TYPE, bbox);
        let inner = s.get_inner_box();
        assert_eq!(inner.top_left.y, 48.0); // 2 * 24
        assert_eq!(inner.height, 128.0); // 200 - 3 * 24
    }

    #[test]
    fn test_aspect_ratio_1() {
        let bbox = make_box(0.0, 0.0, 100.0, 100.0);
        assert!(Shape::new(REAL_SQUARE_TYPE, bbox).aspect_ratio_1());
        assert!(Shape::new(CIRCLE_TYPE, bbox).aspect_ratio_1());
        assert!(!Shape::new(SQUARE_TYPE, bbox).aspect_ratio_1());
        assert!(!Shape::new(OVAL_TYPE, bbox).aspect_ratio_1());
    }

    #[test]
    fn test_is_rectangular() {
        let bbox = make_box(0.0, 0.0, 100.0, 100.0);
        assert!(Shape::new(SQUARE_TYPE, bbox).is_rectangular());
        assert!(Shape::new(REAL_SQUARE_TYPE, bbox).is_rectangular());
        assert!(Shape::new(IMAGE_TYPE, bbox).is_rectangular());
        assert!(!Shape::new(DIAMOND_TYPE, bbox).is_rectangular());
        assert!(!Shape::new(OVAL_TYPE, bbox).is_rectangular());
        assert!(!Shape::new(CLOUD_TYPE, bbox).is_rectangular());
    }

    #[test]
    fn test_unknown_shape_type_defaults_to_rectangle() {
        let bbox = make_box(0.0, 0.0, 100.0, 100.0);
        let s = Shape::new("UnknownType", bbox);
        assert_eq!(s.get_type(), "UnknownType");
        assert!(s.is_rectangular());
        assert!(s.get_svg_path_data().is_empty());
    }

    #[test]
    fn test_limit_ar() {
        let (w, h) = limit_ar(100.0, 10.0, 3.0);
        assert_eq!(w, 100.0);
        assert_eq!(h, 33.0); // round(100/3) = 33

        let (w, h) = limit_ar(10.0, 100.0, 3.0);
        assert_eq!(w, 33.0);
        assert_eq!(h, 100.0);
    }

    #[test]
    fn test_all_shapes_construct() {
        let bbox = make_box(0.0, 0.0, 200.0, 150.0);
        let types = [
            SQUARE_TYPE,
            REAL_SQUARE_TYPE,
            PARALLELOGRAM_TYPE,
            DOCUMENT_TYPE,
            CYLINDER_TYPE,
            QUEUE_TYPE,
            PAGE_TYPE,
            PACKAGE_TYPE,
            STEP_TYPE,
            CALLOUT_TYPE,
            STORED_DATA_TYPE,
            PERSON_TYPE,
            C4_PERSON_TYPE,
            DIAMOND_TYPE,
            OVAL_TYPE,
            CIRCLE_TYPE,
            HEXAGON_TYPE,
            CLOUD_TYPE,
            TABLE_TYPE,
            CLASS_TYPE,
            TEXT_TYPE,
            CODE_TYPE,
            IMAGE_TYPE,
        ];
        for t in &types {
            let s = Shape::new(t, bbox);
            assert_eq!(s.get_type(), *t);
            // These should not panic:
            let _ = s.get_inner_box();
            let _ = s.get_dimensions_to_fit(50.0, 30.0, 10.0, 10.0);
            let _ = s.get_default_padding();
            let _ = s.perimeter();
            let _ = s.get_svg_path_data();
        }
    }

    #[test]
    fn test_step_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 200.0, 100.0);
        let s = Shape::new(STEP_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 1);
        let path = &paths[0];
        assert!(path.starts_with("M "));
        assert!(path.ends_with("Z"));
    }

    #[test]
    fn test_callout_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 200.0, 150.0);
        let s = Shape::new(CALLOUT_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn test_stored_data_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 200.0, 100.0);
        let s = Shape::new(STORED_DATA_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn test_parallelogram_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 200.0, 100.0);
        let s = Shape::new(PARALLELOGRAM_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn test_document_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 200.0, 100.0);
        let s = Shape::new(DOCUMENT_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn test_package_svg_path_data() {
        let bbox = make_box(0.0, 0.0, 200.0, 150.0);
        let s = Shape::new(PACKAGE_TYPE, bbox);
        let paths = s.get_svg_path_data();
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn test_trace_to_shape_border_rectangular() {
        let bbox = make_box(0.0, 0.0, 100.0, 100.0);
        let s = Shape::new(SQUARE_TYPE, bbox);
        let rect_pt = Point::new(50.0, 0.0);
        let prev_pt = Point::new(50.0, -50.0);
        let result = trace_to_shape_border(&s, &rect_pt, &prev_pt);
        // For rectangular shapes, trace just returns the rect border point
        assert_eq!(result.x, rect_pt.x);
        assert_eq!(result.y, rect_pt.y);
    }
}
