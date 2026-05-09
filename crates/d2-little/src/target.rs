//! d2-target: diagram, shape, and connection types for d2 rendering.
//!
//! Ported from Go `d2target/d2target.go`, `d2target/class.go`,
//! and `d2target/sqltable.go`.

use std::collections::HashMap;
use std::hash::Hash;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const DEFAULT_ICON_SIZE: i32 = 32;
pub const MAX_ICON_SIZE: i32 = 64;

pub const SHADOW_SIZE_X: i32 = 3;
pub const SHADOW_SIZE_Y: i32 = 5;
pub const THREE_DEE_OFFSET: i32 = 15;
pub const MULTIPLE_OFFSET: i32 = 10;

pub const INNER_BORDER_OFFSET: i32 = 5;

/// Background color (theme token).
pub const BG_COLOR: &str = crate::color::N7;
/// Foreground color (theme token).
pub const FG_COLOR: &str = crate::color::N1;

pub const MIN_ARROWHEAD_STROKE_WIDTH: f64 = 2.0;
pub const ARROWHEAD_PADDING: f64 = 2.0;

pub const CONNECTION_ICON_LABEL_GAP: f64 = 8.0;

// Class layout constants
pub const PREFIX_PADDING: i32 = 10;
pub const PREFIX_WIDTH: i32 = 20;
pub const CENTER_PADDING: i32 = 50;
pub const VERTICAL_PADDING: i32 = 20;

// SQL table layout constants
pub const NAME_PADDING: i32 = 10;
pub const TYPE_PADDING: i32 = 20;
pub const CONSTRAINT_PADDING: i32 = 20;
pub const HEADER_PADDING: i32 = 10;
pub const HEADER_FONT_ADD: i32 = 4;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub sketch: Option<bool>,
    pub theme_id: Option<i64>,
    pub dark_theme_id: Option<i64>,
    pub pad: Option<i64>,
    pub center: Option<bool>,
    pub layout_engine: Option<String>,
    pub theme_overrides: Option<crate::themes::ThemeOverrides>,
    pub dark_theme_overrides: Option<crate::themes::ThemeOverrides>,
    pub data: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Point
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

// ---------------------------------------------------------------------------
// Shape type constants
// ---------------------------------------------------------------------------

pub const SHAPE_RECTANGLE: &str = "rectangle";
pub const SHAPE_SQUARE: &str = "square";
pub const SHAPE_PAGE: &str = "page";
pub const SHAPE_PARALLELOGRAM: &str = "parallelogram";
pub const SHAPE_DOCUMENT: &str = "document";
pub const SHAPE_CYLINDER: &str = "cylinder";
pub const SHAPE_QUEUE: &str = "queue";
pub const SHAPE_PACKAGE: &str = "package";
pub const SHAPE_STEP: &str = "step";
pub const SHAPE_CALLOUT: &str = "callout";
pub const SHAPE_STORED_DATA: &str = "stored_data";
pub const SHAPE_PERSON: &str = "person";
pub const SHAPE_C4_PERSON: &str = "c4-person";
pub const SHAPE_DIAMOND: &str = "diamond";
pub const SHAPE_OVAL: &str = "oval";
pub const SHAPE_CIRCLE: &str = "circle";
pub const SHAPE_HEXAGON: &str = "hexagon";
pub const SHAPE_CLOUD: &str = "cloud";
pub const SHAPE_TEXT: &str = "text";
pub const SHAPE_CODE: &str = "code";
pub const SHAPE_CLASS: &str = "class";
pub const SHAPE_SQL_TABLE: &str = "sql_table";
pub const SHAPE_IMAGE: &str = "image";
pub const SHAPE_SEQUENCE_DIAGRAM: &str = "sequence_diagram";
pub const SHAPE_HIERARCHY: &str = "hierarchy";

pub const SHAPES: &[&str] = &[
    SHAPE_RECTANGLE,
    SHAPE_SQUARE,
    SHAPE_PAGE,
    SHAPE_PARALLELOGRAM,
    SHAPE_DOCUMENT,
    SHAPE_CYLINDER,
    SHAPE_QUEUE,
    SHAPE_PACKAGE,
    SHAPE_STEP,
    SHAPE_CALLOUT,
    SHAPE_STORED_DATA,
    SHAPE_PERSON,
    SHAPE_C4_PERSON,
    SHAPE_DIAMOND,
    SHAPE_OVAL,
    SHAPE_CIRCLE,
    SHAPE_HEXAGON,
    SHAPE_CLOUD,
    SHAPE_TEXT,
    SHAPE_CODE,
    SHAPE_CLASS,
    SHAPE_SQL_TABLE,
    SHAPE_IMAGE,
    SHAPE_SEQUENCE_DIAGRAM,
    SHAPE_HIERARCHY,
];

/// Check if a string is a recognized shape type.
///
/// Empty string defaults to rectangle and returns `true`.
pub fn is_shape(s: &str) -> bool {
    if s.is_empty() {
        return true; // default shape is rectangle
    }
    SHAPES.iter().any(|shape| shape.eq_ignore_ascii_case(s))
}

// ---------------------------------------------------------------------------
// Shape type to internal shape type mapping
// ---------------------------------------------------------------------------

/// Internal shape type constants (from Go `lib/shape`).
pub mod shape_type {
    pub const SQUARE: &str = "Square";
    pub const REAL_SQUARE: &str = "RealSquare";
    pub const PARALLELOGRAM: &str = "Parallelogram";
    pub const DOCUMENT: &str = "Document";
    pub const CYLINDER: &str = "Cylinder";
    pub const QUEUE: &str = "Queue";
    pub const PAGE: &str = "Page";
    pub const PACKAGE: &str = "Package";
    pub const STEP: &str = "Step";
    pub const CALLOUT: &str = "Callout";
    pub const STORED_DATA: &str = "StoredData";
    pub const PERSON: &str = "Person";
    pub const C4_PERSON: &str = "C4Person";
    pub const DIAMOND: &str = "Diamond";
    pub const OVAL: &str = "Oval";
    pub const CIRCLE: &str = "Circle";
    pub const HEXAGON: &str = "Hexagon";
    pub const CLOUD: &str = "Cloud";
    pub const TABLE: &str = "Table";
    pub const CLASS: &str = "Class";
    pub const TEXT: &str = "Text";
    pub const CODE: &str = "Code";
    pub const IMAGE: &str = "Image";
}

/// Map a DSL shape name to an internal shape type string.
///
/// Empty string maps to `"Square"` (the default rectangle).
pub fn dsl_shape_to_shape_type(dsl: &str) -> &'static str {
    match dsl {
        "" | SHAPE_RECTANGLE => shape_type::SQUARE,
        SHAPE_SQUARE => shape_type::REAL_SQUARE,
        SHAPE_PAGE => shape_type::PAGE,
        SHAPE_PARALLELOGRAM => shape_type::PARALLELOGRAM,
        SHAPE_DOCUMENT => shape_type::DOCUMENT,
        SHAPE_CYLINDER => shape_type::CYLINDER,
        SHAPE_QUEUE => shape_type::QUEUE,
        SHAPE_PACKAGE => shape_type::PACKAGE,
        SHAPE_STEP => shape_type::STEP,
        SHAPE_CALLOUT => shape_type::CALLOUT,
        SHAPE_STORED_DATA => shape_type::STORED_DATA,
        SHAPE_PERSON => shape_type::PERSON,
        SHAPE_C4_PERSON => shape_type::C4_PERSON,
        SHAPE_DIAMOND => shape_type::DIAMOND,
        SHAPE_OVAL => shape_type::OVAL,
        SHAPE_CIRCLE => shape_type::CIRCLE,
        SHAPE_HEXAGON => shape_type::HEXAGON,
        SHAPE_CLOUD => shape_type::CLOUD,
        SHAPE_TEXT => shape_type::TEXT,
        SHAPE_CODE => shape_type::CODE,
        SHAPE_CLASS => shape_type::CLASS,
        SHAPE_SQL_TABLE => shape_type::TABLE,
        SHAPE_IMAGE => shape_type::IMAGE,
        SHAPE_SEQUENCE_DIAGRAM | SHAPE_HIERARCHY => shape_type::SQUARE,
        _ => shape_type::SQUARE,
    }
}

/// Map an internal shape type to a DSL shape name.
pub fn shape_type_to_dsl_shape(st: &str) -> &'static str {
    match st {
        shape_type::SQUARE => SHAPE_RECTANGLE,
        shape_type::REAL_SQUARE => SHAPE_SQUARE,
        shape_type::PAGE => SHAPE_PAGE,
        shape_type::PARALLELOGRAM => SHAPE_PARALLELOGRAM,
        shape_type::DOCUMENT => SHAPE_DOCUMENT,
        shape_type::CYLINDER => SHAPE_CYLINDER,
        shape_type::QUEUE => SHAPE_QUEUE,
        shape_type::PACKAGE => SHAPE_PACKAGE,
        shape_type::STEP => SHAPE_STEP,
        shape_type::CALLOUT => SHAPE_CALLOUT,
        shape_type::STORED_DATA => SHAPE_STORED_DATA,
        shape_type::PERSON => SHAPE_PERSON,
        shape_type::C4_PERSON => SHAPE_C4_PERSON,
        shape_type::DIAMOND => SHAPE_DIAMOND,
        shape_type::OVAL => SHAPE_OVAL,
        shape_type::CIRCLE => SHAPE_CIRCLE,
        shape_type::HEXAGON => SHAPE_HEXAGON,
        shape_type::CLOUD => SHAPE_CLOUD,
        shape_type::TABLE => SHAPE_SQL_TABLE,
        shape_type::CLASS => SHAPE_CLASS,
        shape_type::TEXT => SHAPE_TEXT,
        shape_type::CODE => SHAPE_CODE,
        shape_type::IMAGE => SHAPE_IMAGE,
        _ => SHAPE_RECTANGLE,
    }
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Text {
    pub label: String,
    pub font_size: i32,
    pub font_family: String,
    pub language: String,
    pub color: String,

    pub italic: bool,
    pub bold: bool,
    pub underline: bool,

    pub label_width: i32,
    pub label_height: i32,
    pub label_fill: String,
}

// ---------------------------------------------------------------------------
// MText (measured text, used in Class and SQLTable)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct MText {
    pub text: String,
    pub font_size: i32,
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_underline: bool,
    pub language: String,
    pub shape: String,
    pub dimensions: Option<TextDimensions>,
}

impl MText {
    pub fn get_color(&self, is_italic: bool) -> &str {
        if is_italic {
            crate::color::N2
        } else {
            crate::color::N1
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TextDimensions {
    pub width: i32,
    pub height: i32,
}

impl TextDimensions {
    pub fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }
}

// ---------------------------------------------------------------------------
// Arrowhead
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Arrowhead {
    #[default]
    None,
    Arrow,
    UnfilledTriangle,
    Triangle,
    Diamond,
    FilledDiamond,
    Circle,
    FilledCircle,
    Cross,
    Box_,
    FilledBox,
    Line,
    CfOne,
    CfMany,
    CfOneRequired,
    CfManyRequired,
}

impl Arrowhead {
    pub const DEFAULT: Arrowhead = Arrowhead::Triangle;

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Arrow => "arrow",
            Self::UnfilledTriangle => "unfilled-triangle",
            Self::Triangle => "triangle",
            Self::Diamond => "diamond",
            Self::FilledDiamond => "filled-diamond",
            Self::Circle => "circle",
            Self::FilledCircle => "filled-circle",
            Self::Cross => "cross",
            Self::Box_ => "box",
            Self::FilledBox => "filled-box",
            Self::Line => "line",
            Self::CfOne => "cf-one",
            Self::CfMany => "cf-many",
            Self::CfOneRequired => "cf-one-required",
            Self::CfManyRequired => "cf-many-required",
        }
    }

    pub fn from_str_val(s: &str) -> Self {
        match s {
            "none" => Self::None,
            "arrow" => Self::Arrow,
            "unfilled-triangle" => Self::UnfilledTriangle,
            "triangle" => Self::Triangle,
            "diamond" => Self::Diamond,
            "filled-diamond" => Self::FilledDiamond,
            "circle" => Self::Circle,
            "filled-circle" => Self::FilledCircle,
            "cross" => Self::Cross,
            "box" => Self::Box_,
            "filled-box" => Self::FilledBox,
            "line" => Self::Line,
            "cf-one" => Self::CfOne,
            "cf-many" => Self::CfMany,
            "cf-one-required" => Self::CfOneRequired,
            "cf-many-required" => Self::CfManyRequired,
            _ => Self::DEFAULT,
        }
    }

    /// Convert an arrowhead type string with optional filled flag to an Arrowhead.
    pub fn to_arrowhead(arrowhead_type: &str, filled: Option<bool>) -> Self {
        match arrowhead_type {
            "diamond" => {
                if filled == Some(true) {
                    Self::FilledDiamond
                } else {
                    Self::Diamond
                }
            }
            "circle" => {
                if filled == Some(true) {
                    Self::FilledCircle
                } else {
                    Self::Circle
                }
            }
            "none" => Self::None,
            "arrow" => Self::Arrow,
            "triangle" => {
                if filled == Some(false) {
                    Self::UnfilledTriangle
                } else {
                    Self::Triangle
                }
            }
            "cross" => Self::Cross,
            "box" => {
                if filled == Some(true) {
                    Self::FilledBox
                } else {
                    Self::Box_
                }
            }
            "cf-one" => Self::CfOne,
            "cf-many" => Self::CfMany,
            "cf-one-required" => Self::CfOneRequired,
            "cf-many-required" => Self::CfManyRequired,
            _ => {
                // Default arrowhead is Triangle; respect filled=false
                if Self::DEFAULT == Self::Triangle && filled == Some(false) {
                    Self::UnfilledTriangle
                } else {
                    Self::DEFAULT
                }
            }
        }
    }

    /// Return the (width, height) dimensions of the arrowhead for the given stroke width.
    pub fn dimensions(&self, stroke_width: f64) -> (f64, f64) {
        let (base_w, base_h, w_mul, h_mul) = match self {
            Self::Arrow => (4.0, 4.0, 4.0, 4.0),
            Self::Triangle => (4.0, 4.0, 3.0, 4.0),
            Self::UnfilledTriangle => (7.0, 7.0, 3.0, 4.0),
            Self::Line => (0.0, 0.0, 5.0, 8.0),
            Self::FilledDiamond => (11.0, 7.0, 5.5, 3.5),
            Self::Diamond => (11.0, 9.0, 5.5, 4.5),
            Self::Cross => (7.0, 7.0, 5.0, 5.0),
            Self::FilledCircle | Self::Circle => (8.0, 8.0, 5.0, 5.0),
            Self::FilledBox | Self::Box_ => (6.0, 6.0, 5.0, 5.0),
            Self::CfOne | Self::CfMany | Self::CfOneRequired | Self::CfManyRequired => {
                (9.0, 9.0, 4.5, 4.5)
            }
            Self::None => (0.0, 0.0, 0.0, 0.0),
        };
        let clipped = stroke_width.max(MIN_ARROWHEAD_STROKE_WIDTH);
        (base_w + clipped * w_mul, base_h + clipped * h_mul)
    }
}

/// Valid arrowhead shape values for DSL parsing.
pub const ARROWHEADS: &[&str] = &[
    "none",
    "arrow",
    "triangle",
    "diamond",
    "circle",
    "box",
    "cf-one",
    "cf-many",
    "cf-one-required",
    "cf-many-required",
    "cross",
];

impl std::fmt::Display for Arrowhead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Class types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Class {
    pub fields: Vec<ClassField>,
    pub methods: Vec<ClassMethod>,
}

#[derive(Debug, Clone, Default)]
pub struct ClassField {
    pub name: String,
    pub type_: String,
    pub visibility: String,
    pub underline: bool,
}

impl ClassField {
    pub fn text(&self, font_size: i32) -> MText {
        MText {
            text: format!("{}{}", self.name, self.type_),
            font_size,
            is_bold: false,
            is_italic: false,
            is_underline: self.underline,
            shape: "class".to_owned(),
            ..Default::default()
        }
    }

    pub fn visibility_token(&self) -> &str {
        match self.visibility.as_str() {
            "protected" => "#",
            "private" => "-",
            _ => "+",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClassMethod {
    pub name: String,
    pub return_: String,
    pub visibility: String,
    pub underline: bool,
}

impl ClassMethod {
    pub fn text(&self, font_size: i32) -> MText {
        MText {
            text: format!("{}{}", self.name, self.return_),
            font_size,
            is_bold: false,
            is_italic: false,
            is_underline: self.underline,
            shape: "class".to_owned(),
            ..Default::default()
        }
    }

    pub fn visibility_token(&self) -> &str {
        match self.visibility.as_str() {
            "protected" => "#",
            "private" => "-",
            _ => "+",
        }
    }
}

// ---------------------------------------------------------------------------
// SQL Table types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct SQLTable {
    pub columns: Vec<SQLColumn>,
}

#[derive(Debug, Clone, Default)]
pub struct SQLColumn {
    pub name: Text,
    pub type_: Text,
    pub constraint: Vec<String>,
    pub reference: String,
}

impl SQLColumn {
    pub fn texts(&self, font_size: i32) -> Vec<MText> {
        vec![
            MText {
                text: self.name.label.clone(),
                font_size,
                shape: "sql_table".to_owned(),
                ..Default::default()
            },
            MText {
                text: self.type_.label.clone(),
                font_size,
                shape: "sql_table".to_owned(),
                ..Default::default()
            },
            MText {
                text: self.constraint_abbr(),
                font_size,
                shape: "sql_table".to_owned(),
                ..Default::default()
            },
        ]
    }

    pub fn constraint_abbr(&self) -> String {
        let abbrs: Vec<&str> = self
            .constraint
            .iter()
            .map(|c| match c.as_str() {
                "primary_key" => "PK",
                "foreign_key" => "FK",
                "unique" => "UNQ",
                other => other,
            })
            .collect();
        abbrs.join(", ")
    }
}

// ---------------------------------------------------------------------------
// Legend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Legend {
    pub label: String,
    pub shapes: Vec<Shape>,
    pub connections: Vec<Connection>,
}

// ---------------------------------------------------------------------------
// Shape
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Shape {
    pub id: String,
    pub type_: String,

    pub classes: Vec<String>,

    pub pos: Point,
    pub width: i32,
    pub height: i32,

    pub opacity: f64,
    pub stroke_dash: f64,
    pub stroke_width: i32,

    pub border_radius: i32,

    pub fill: String,
    pub fill_pattern: String,
    pub stroke: String,

    pub animated: bool,
    pub shadow: bool,
    pub three_dee: bool,
    pub multiple: bool,
    pub double_border: bool,

    pub tooltip: String,
    pub link: String,
    pub pretty_link: String,
    pub icon: Option<String>,
    pub icon_border_radius: i32,
    pub icon_position: String,

    /// Whether the shape should allow shapes behind it to bleed through.
    pub blend: bool,

    pub class: Class,
    pub sql_table: SQLTable,

    pub content_aspect_ratio: Option<f64>,

    pub text: Text,

    pub label_position: String,
    pub tooltip_position: String,

    pub z_index: i32,
    pub level: i32,

    pub primary_accent_color: String,
    pub secondary_accent_color: String,
    pub neutral_accent_color: String,
}

impl Shape {
    pub fn get_font_color(&self) -> &str {
        if self.type_ == SHAPE_CLASS || self.type_ == SHAPE_SQL_TABLE {
            if !crate::color::is_theme_color(&self.text.color) {
                return &self.text.color;
            }
            return &self.stroke;
        }
        if !self.text.color.is_empty() {
            return &self.text.color;
        }
        crate::color::N1
    }

    /// Set the shape type, normalizing synonyms.
    pub fn set_type(&mut self, t: &str) {
        let lower = t.to_ascii_lowercase();
        if lower == SHAPE_CIRCLE {
            self.type_ = SHAPE_OVAL.to_owned();
        } else if lower == SHAPE_SQUARE {
            self.type_ = SHAPE_RECTANGLE.to_owned();
        } else {
            self.type_ = lower;
        }
    }

    pub fn get_z_index(&self) -> i32 {
        self.z_index
    }

    pub fn get_id(&self) -> &str {
        &self.id
    }
}

/// Create a new base shape with default styling.
pub fn base_shape() -> Shape {
    Shape {
        opacity: 1.0,
        stroke_dash: 0.0,
        stroke_width: 2,
        text: Text {
            bold: true,
            font_family: "DEFAULT".to_owned(),
            ..Default::default()
        },
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Connection {
    pub id: String,

    pub classes: Vec<String>,

    pub src: String,
    pub src_arrow: Arrowhead,
    pub src_label: Option<Text>,

    pub dst: String,
    pub dst_arrow: Arrowhead,
    pub dst_label: Option<Text>,

    pub opacity: f64,
    pub stroke_dash: f64,
    pub stroke_width: i32,
    pub stroke: String,
    pub fill: String,
    pub border_radius: f64,

    pub text: Text,
    pub label_position: String,
    pub label_percentage: f64,

    pub link: String,
    pub pretty_link: String,

    pub route: Vec<crate::geo::Point>,
    pub is_curve: bool,

    pub animated: bool,
    pub tooltip: String,
    pub icon: Option<String>,
    pub icon_position: String,
    pub icon_border_radius: f64,

    pub z_index: i32,
}

impl Connection {
    pub fn get_font_color(&self) -> &str {
        if !self.text.color.is_empty() {
            return &self.text.color;
        }
        crate::color::N1
    }

    pub fn get_z_index(&self) -> i32 {
        self.z_index
    }

    pub fn get_id(&self) -> &str {
        &self.id
    }
}

/// Create a new base connection with default styling.
pub fn base_connection() -> Connection {
    Connection {
        src_arrow: Arrowhead::None,
        dst_arrow: Arrowhead::None,
        route: Vec::new(),
        opacity: 1.0,
        stroke_dash: 0.0,
        stroke_width: 2,
        border_radius: 10.0,
        text: Text {
            italic: true,
            font_family: "DEFAULT".to_owned(),
            ..Default::default()
        },
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Diagram
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Diagram {
    pub name: String,
    pub config: Option<Config>,
    pub is_folder_only: bool,
    pub description: String,
    pub font_family: Option<String>,
    pub mono_font_family: Option<String>,

    pub shapes: Vec<Shape>,
    pub connections: Vec<Connection>,

    pub root: Shape,
    pub legend: Option<Legend>,

    pub layers: Vec<Diagram>,
    pub scenarios: Vec<Diagram>,
    pub steps: Vec<Diagram>,
}

impl Diagram {
    /// Create a new diagram with the default root shape.
    pub fn new() -> Self {
        Self {
            root: Shape {
                fill: BG_COLOR.to_owned(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Navigate to a nested board by path.
    pub fn get_board(&self, board_path: &[&str]) -> Option<&Diagram> {
        if board_path.is_empty() {
            return Some(self);
        }

        let head = board_path[0];

        if board_path.len() == 1 && self.name == head {
            return Some(self);
        }

        match head {
            "layers" if board_path.len() >= 2 => {
                for b in &self.layers {
                    if b.name == board_path[1] {
                        return b.get_board(&board_path[2..]);
                    }
                }
            }
            "scenarios" if board_path.len() >= 2 => {
                for b in &self.scenarios {
                    if b.name == board_path[1] {
                        return b.get_board(&board_path[2..]);
                    }
                }
            }
            "steps" if board_path.len() >= 2 => {
                for b in &self.steps {
                    if b.name == board_path[1] {
                        return b.get_board(&board_path[2..]);
                    }
                }
            }
            _ => {}
        }

        // Also try matching directly by name
        for b in &self.layers {
            if b.name == head {
                return b.get_board(&board_path[1..]);
            }
        }
        for b in &self.scenarios {
            if b.name == head {
                return b.get_board(&board_path[1..]);
            }
        }
        for b in &self.steps {
            if b.name == head {
                return b.get_board(&board_path[1..]);
            }
        }
        None
    }

    /// Check if any shape in the diagram (including nested boards) satisfies a condition.
    pub fn has_shape<F>(&self, condition: &F) -> bool
    where
        F: Fn(&Shape) -> bool,
    {
        for d in &self.layers {
            if d.has_shape(condition) {
                return true;
            }
        }
        for d in &self.scenarios {
            if d.has_shape(condition) {
                return true;
            }
        }
        for d in &self.steps {
            if d.has_shape(condition) {
                return true;
            }
        }
        for s in &self.shapes {
            if condition(s) {
                return true;
            }
        }
        false
    }

    /// Compute a hash ID for the diagram, prefixed with "d2-".
    ///
    /// Matches Go `d2target.Diagram.HashID`: serialize the diagram via the
    /// Go-compatible JSON stream, hash with FNV-1a 32-bit, and format with
    /// the "d2-" prefix. An optional salt is mixed into the hash.
    pub fn hash_id(&self, salt: Option<&str>) -> String {
        let bytes = go_json::diagram_bytes(self);
        let mut h = fnv1a32(&bytes);
        if let Some(s) = salt {
            h = fnv1a32_mix(h, s.as_bytes());
        }
        format!("d2-{}", h)
    }
}

/// FNV-1a continuation: start from an existing hash state and mix in more bytes.
#[inline]
fn fnv1a32_mix(mut h: u32, data: &[u8]) -> u32 {
    for &b in data {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// Get the top-left of a label given its position string and the shape's
/// bounding box. Mirrors Go `label.Position.GetPointOnBox`.
fn label_top_left(
    pos: &str,
    shape_box: &crate::geo::Box2D,
    padding: f64,
    lw: f64,
    lh: f64,
) -> (f64, f64) {
    let sx = shape_box.top_left.x;
    let sy = shape_box.top_left.y;
    let sw = shape_box.width;
    let sh = shape_box.height;
    let cx = sx + sw / 2.0;
    let cy = sy + sh / 2.0;
    let (mut x, mut y) = (sx, sy);
    match pos {
        "OUTSIDE_TOP_LEFT" => {
            x -= padding;
            y -= padding + lh;
        }
        "OUTSIDE_TOP_CENTER" => {
            x = cx - lw / 2.0;
            y -= padding + lh;
        }
        "OUTSIDE_TOP_RIGHT" => {
            x += sw - lw - padding;
            y -= padding + lh;
        }
        "OUTSIDE_LEFT_TOP" => {
            x -= padding + lw;
            y += padding;
        }
        "OUTSIDE_LEFT_MIDDLE" => {
            x -= padding + lw;
            y = cy - lh / 2.0;
        }
        "OUTSIDE_LEFT_BOTTOM" => {
            x -= padding + lw;
            y += sh - lh - padding;
        }
        "OUTSIDE_RIGHT_TOP" => {
            x += sw + padding;
            y += padding;
        }
        "OUTSIDE_RIGHT_MIDDLE" => {
            x += sw + padding;
            y = cy - lh / 2.0;
        }
        "OUTSIDE_RIGHT_BOTTOM" => {
            x += sw + padding;
            y += sh - lh - padding;
        }
        "OUTSIDE_BOTTOM_LEFT" => {
            x += padding;
            y += sh + padding;
        }
        "OUTSIDE_BOTTOM_CENTER" => {
            x = cx - lw / 2.0;
            y += sh + padding;
        }
        "OUTSIDE_BOTTOM_RIGHT" => {
            x += sw - lw - padding;
            y += sh + padding;
        }
        "INSIDE_TOP_LEFT" => {
            x += padding;
            y += padding;
        }
        "INSIDE_TOP_CENTER" => {
            x = cx - lw / 2.0;
            y += padding;
        }
        "INSIDE_TOP_RIGHT" => {
            x += sw - lw - padding;
            y += padding;
        }
        "INSIDE_MIDDLE_LEFT" => {
            x += padding;
            y = cy - lh / 2.0;
        }
        "INSIDE_MIDDLE_CENTER" => {
            x = cx - lw / 2.0;
            y = cy - lh / 2.0;
        }
        "INSIDE_MIDDLE_RIGHT" => {
            x += sw - lw - padding;
            y = cy - lh / 2.0;
        }
        "INSIDE_BOTTOM_LEFT" => {
            x += padding;
            y += sh - lh - padding;
        }
        "INSIDE_BOTTOM_CENTER" => {
            x = cx - lw / 2.0;
            y += sh - lh - padding;
        }
        "INSIDE_BOTTOM_RIGHT" => {
            x += sw - lw - padding;
            y += sh - lh - padding;
        }
        // Border labels: centered on the shape's border (half in, half out).
        "BORDER_TOP_LEFT" => {
            x += padding;
            y -= lh / 2.0;
        }
        "BORDER_TOP_CENTER" => {
            x = cx - lw / 2.0;
            y -= lh / 2.0;
        }
        "BORDER_TOP_RIGHT" => {
            x += sw - lw - padding;
            y -= lh / 2.0;
        }
        "BORDER_LEFT_TOP" => {
            x -= lw / 2.0;
            y += padding;
        }
        "BORDER_LEFT_MIDDLE" => {
            x -= lw / 2.0;
            y = cy - lh / 2.0;
        }
        "BORDER_LEFT_BOTTOM" => {
            x -= lw / 2.0;
            y += sh - lh - padding;
        }
        "BORDER_RIGHT_TOP" => {
            x += sw - lw / 2.0;
            y += padding;
        }
        "BORDER_RIGHT_MIDDLE" => {
            x += sw - lw / 2.0;
            y = cy - lh / 2.0;
        }
        "BORDER_RIGHT_BOTTOM" => {
            x += sw - lw / 2.0;
            y += sh - lh - padding;
        }
        "BORDER_BOTTOM_LEFT" => {
            x += padding;
            y += sh - lh / 2.0;
        }
        "BORDER_BOTTOM_CENTER" => {
            x = cx - lw / 2.0;
            y += sh - lh / 2.0;
        }
        "BORDER_BOTTOM_RIGHT" => {
            x += sw - lw - padding;
            y += sh - lh / 2.0;
        }
        _ => {}
    }
    (x, y)
}

/// Port of Go `d2target.GetIconSize`. For outside / non-center icon
/// Compute arrowhead label top-left position for bounding box.
/// Mirrors Go `Connection.GetSrcArrowheadLabelPosition` / `GetDstArrowheadLabelPosition`.
fn arrowhead_label_tl(c: &Connection, is_dst: bool) -> Option<(f64, f64)> {
    if c.route.len() < 2 {
        return None;
    }
    let label = if is_dst {
        c.dst_label.as_ref()?
    } else {
        c.src_label.as_ref()?
    };

    let width = label.label_width as f64;
    let height = label.label_height as f64;
    let padding = 5.0; // label.PADDING

    // Arrowhead segment: (start, end) where end is the arrowhead tip.
    let (seg_start, seg_end) = if is_dst {
        let n = c.route.len();
        (&c.route[n - 2], &c.route[n - 1])
    } else {
        (&c.route[1], &c.route[0])
    };

    // Initial normal vector (end→start direction) for shift calculation.
    // GetUnitNormalVector(end, start) = (end.Y - start.Y)/L, (start.X - end.X)/L
    let sx = seg_end.x - seg_start.x;
    let sy = seg_end.y - seg_start.y;
    let seg_len = (sx * sx + sy * sy).sqrt();
    if seg_len == 0.0 {
        return None;
    }
    let norm_x0 = (seg_end.y - seg_start.y) / seg_len;
    let norm_y0 = (seg_start.x - seg_end.x) / seg_len;

    // Shift back from the endpoint to fit the label.
    let shift = norm_x0.abs() * (height / 2.0 + padding) + norm_y0.abs() * (width / 2.0 + padding);

    let route = crate::geo::Route(c.route.clone());
    let total_length = route.length();
    let position = if total_length > 0.0 {
        if is_dst {
            1.0 - shift / total_length
        } else {
            shift / total_length
        }
    } else if is_dst {
        1.0
    } else {
        0.0
    };

    // UnlockedTop.GetPointOnRoute: compute basePoint at `position * length`,
    // then offset perpendicular to the route in the top direction (flip).
    let base_distance = position * total_length;
    let (base_point, idx) = route.get_point_at_distance(base_distance);
    if idx + 1 >= c.route.len() {
        return None;
    }
    let ps = &c.route[idx];
    let pe = &c.route[idx + 1];
    let seg_dx = pe.x - ps.x;
    let seg_dy = pe.y - ps.y;
    let sl = (seg_dx * seg_dx + seg_dy * seg_dy).sqrt();
    if sl == 0.0 {
        return None;
    }
    // getOffsetLabelPosition uses GetUnitNormalVector(route[idx], route[idx+1]),
    // so normalX = (ps.Y - pe.Y)/L, normalY = (pe.X - ps.X)/L.
    // For UnlockedTop we flip (multiply by -1).
    let normal_x = -(ps.y - pe.y) / sl;
    let normal_y = -(pe.x - ps.x) / sl;

    let stroke_width = c.stroke_width as f64;
    let offset_x = stroke_width / 2.0 + padding + width / 2.0;
    let offset_y = stroke_width / 2.0 + padding + height / 2.0;

    let label_cx = base_point.x + normal_x * offset_x;
    let label_cy = base_point.y + normal_y * offset_y;

    // Convert center → top-left with chop_precision (matches Go's
    // GetPointOnRoute which chops right before returning).
    let mut label_tl_x = chop_precision_f32_round(label_cx - width / 2.0);
    let mut label_tl_y = chop_precision_f32_round(label_cy - height / 2.0);

    // If arrow is larger than label's fit offset, shift further to avoid
    // overlap with the arrowhead. Go GetArrowheadLabelPosition adds this
    // AFTER the chop_precision in GetPointOnRoute, without re-chopping.
    let arrow_size: f64 = if is_dst {
        if !matches!(c.dst_arrow, Arrowhead::None) {
            c.dst_arrow.dimensions(stroke_width).1
        } else {
            0.0
        }
    } else if !matches!(c.src_arrow, Arrowhead::None) {
        c.src_arrow.dimensions(stroke_width).1
    } else {
        0.0
    };
    if arrow_size > 0.0 {
        let extra = (arrow_size / 2.0 + ARROWHEAD_PADDING) - stroke_width / 2.0 - padding;
        if extra > 0.0 {
            label_tl_x += normal_x * extra;
            label_tl_y += normal_y * extra;
        }
    }

    Some((label_tl_x, label_tl_y))
}

/// chop_precision matching Go d2label.chopPrecision: cast to f32 then
/// round to the nearest integer (after multiplying/dividing by 10000).
fn chop_precision_f32_round(f: f64) -> f64 {
    let r = ((f as f32 * 10000.0) as f64 / 10000.0).round();
    if r == 0.0 { 0.0 } else { r }
}

/// positions the result is `min(minDim, max(DEFAULT_ICON_SIZE, ceil(minDim/2)))`,
/// clamped to `MAX_ICON_SIZE`. `InsideMiddleCenter` returns `halfMinDimension`
/// and clamps to the inner box minus label padding; that path isn't hit
/// from `bounding_box` (which only uses outside positions) so we model
/// only the outside branch here.
fn icon_size(sw: f64, sh: f64, is_inside_center: bool) -> i32 {
    let min_dim = sw.min(sh) as i32;
    let half_min = (0.5 * min_dim as f64).ceil() as i32;
    let size = if is_inside_center {
        half_min
    } else {
        min_dim.min(DEFAULT_ICON_SIZE.max(half_min))
    };
    size.min(MAX_ICON_SIZE)
}

/// Full Go-compatible `d2target.GetIconSize`: computes the icon-side length for
/// a shape of the given (w, h) and position. Matches Go's inside/outside
/// branches plus the inside-position clamp to `min(w-2p, h-2p)` with
/// `p = label::PADDING`. Returns 0 for empty or degenerate boxes.
pub fn get_icon_size(w: f64, h: f64, position: &str) -> i32 {
    let pos = crate::label::Position::from_string(position);
    let is_inside_center = pos == crate::label::Position::InsideMiddleCenter;
    let mut size = icon_size(w, h, is_inside_center);
    if !pos.is_outside() {
        let pad = crate::label::PADDING as i32;
        let avail_w = (w as i32 - 2 * pad).max(0);
        let avail_h = (h as i32 - 2 * pad).max(0);
        size = size.min(avail_w.min(avail_h));
    }
    size
}

/// Compute the top-left corner (x, y) of a positioned tooltip box, given the
/// host shape position/size, the tooltip box size, and the `near:` keyword
/// (e.g. `top-left`, `center-right`). Mirrors Go d2target.CalculateTooltipPosition.
pub fn calculate_tooltip_position(
    shape_x: f64,
    shape_y: f64,
    shape_width: f64,
    shape_height: f64,
    tooltip_width: i32,
    tooltip_height: i32,
    position: &str,
) -> (f64, f64) {
    let padding = 10.0;
    let tw = tooltip_width as f64;
    let th = tooltip_height as f64;
    match position {
        "top-left" => (shape_x, shape_y - th - padding),
        "top-center" => (
            shape_x + shape_width / 2.0 - tw / 2.0,
            shape_y - th - padding,
        ),
        "top-right" => (shape_x + shape_width - tw, shape_y - th - padding),
        "center-left" => (
            shape_x - tw - padding,
            shape_y + shape_height / 2.0 - th / 2.0,
        ),
        "center-right" => (
            shape_x + shape_width + padding,
            shape_y + shape_height / 2.0 - th / 2.0,
        ),
        "bottom-left" => (shape_x, shape_y + shape_height + padding),
        "bottom-center" => (
            shape_x + shape_width / 2.0 - tw / 2.0,
            shape_y + shape_height + padding,
        ),
        "bottom-right" => (shape_x + shape_width - tw, shape_y + shape_height + padding),
        _ => (
            shape_x + shape_width / 2.0 - tw / 2.0,
            shape_y - th - padding,
        ),
    }
}

/// Measure a positioned tooltip's box (width, height including inner padding)
/// using the shape's font settings. Falls back to a heuristic size when the
/// ruler or markdown measurement fails.
pub fn measure_positioned_tooltip(shape: &Shape) -> (i32, i32) {
    if shape.tooltip.is_empty() || shape.tooltip_position.is_empty() {
        return (0, 0);
    }
    let (mut tw, mut th) = {
        let text_len = shape.tooltip.chars().count() as i32;
        ((text_len * 8 + 20).min(200), 30)
    };
    if let Ok(mut ruler) = crate::textmeasure::Ruler::new() {
        let font_family = crate::fonts::d2_font_to_family(&shape.text.font_family);
        let font_size = if shape.text.font_size > 0 {
            shape.text.font_size
        } else {
            crate::fonts::FONT_SIZE_M
        };
        if let Ok((w, h)) = crate::textmeasure::measure_markdown(
            &shape.tooltip,
            &mut ruler,
            font_family,
            None,
            font_size,
        ) {
            tw = w + 20;
            th = h + 20;
        }
    }
    (tw, th)
}

/// Compute the axis-aligned bounding box of a positioned tooltip card for
/// `shape` (including the tooltip rect itself; the tail is ignored, matching
/// Go d2target.calculateTooltipBounds).
pub fn calculate_tooltip_bounds(shape: &Shape) -> (i32, i32, i32, i32) {
    if shape.tooltip.is_empty() || shape.tooltip_position.is_empty() {
        return (0, 0, 0, 0);
    }
    let (tw, th) = measure_positioned_tooltip(shape);
    let (x, y) = calculate_tooltip_position(
        shape.pos.x as f64,
        shape.pos.y as f64,
        shape.width as f64,
        shape.height as f64,
        tw,
        th,
        &shape.tooltip_position,
    );
    let min_x = x.floor() as i32;
    let min_y = y.floor() as i32;
    let max_x = x.ceil() as i32 + tw;
    let max_y = y.ceil() as i32 + th;
    (min_x, min_y, max_x, max_y)
}

impl Diagram {
    /// Compute the axis-aligned bounding box of all shapes and connections.
    ///
    /// Returns `(top_left, bottom_right)`.
    pub fn bounding_box(&self) -> (Point, Point) {
        if self.shapes.is_empty() {
            return (Point::new(0, 0), Point::new(0, 0));
        }

        fn clamp_i64_to_i32(v: i64) -> i32 {
            v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
        }

        let mut x1 = i64::from(i32::MAX);
        let mut y1 = i64::from(i32::MAX);
        let mut x2 = i64::from(i32::MIN);
        let mut y2 = i64::from(i32::MIN);

        for s in &self.shapes {
            let half_stroke = (s.stroke_width as f64 / 2.0).ceil() as i64;
            let pos_x = i64::from(s.pos.x);
            let pos_y = i64::from(s.pos.y);
            let width = i64::from(s.width);
            let height = i64::from(s.height);
            let stroke_width = i64::from(s.stroke_width);
            x1 = x1.min(pos_x - half_stroke);
            y1 = y1.min(pos_y - half_stroke);
            x2 = x2.max(pos_x + width + half_stroke);
            y2 = y2.max(pos_y + height + half_stroke);

            // c4-person has a head circle above the shape top, extend bbox upward.
            if s.type_ == SHAPE_C4_PERSON {
                let head_radius = (s.width as f64 * 0.22) as i64;
                let head_center_y = (s.height as f64 * 0.18) as i64;
                let head_top = pos_y + head_center_y - head_radius;
                y1 = y1.min(head_top - stroke_width);
            }

            // Reserve space for the top-right appendix icon when a shape has
            // tooltip or link. Go d2's d2target.Diagram.BoundingBox expands by
            // -16/+16 when there is no custom tooltipPosition; with a custom
            // tooltipPosition it grows to fit the positioned tooltip card.
            if !s.tooltip.is_empty() || !s.link.is_empty() {
                if !s.tooltip_position.is_empty() {
                    let (tmin_x, tmin_y, tmax_x, tmax_y) = calculate_tooltip_bounds(s);
                    x1 = x1.min(i64::from(tmin_x));
                    y1 = y1.min(i64::from(tmin_y));
                    x2 = x2.max(i64::from(tmax_x));
                    y2 = y2.max(i64::from(tmax_y));
                } else {
                    y1 = y1.min(pos_y - stroke_width - 16);
                    x2 = x2.max(pos_x + stroke_width + width + 16);
                }
            }

            if s.shadow {
                y2 = y2.max(pos_y + height + half_stroke + i64::from(SHADOW_SIZE_Y));
                x2 = x2.max(pos_x + width + half_stroke + i64::from(SHADOW_SIZE_X));
            }

            if s.three_dee {
                let offset_y = if s.type_ == SHAPE_HEXAGON {
                    THREE_DEE_OFFSET / 2
                } else {
                    THREE_DEE_OFFSET
                } as i64;
                y1 = y1.min(pos_y - offset_y - stroke_width);
                x2 = x2.max(pos_x + i64::from(THREE_DEE_OFFSET) + width + stroke_width);
            }
            if s.multiple {
                y1 = y1.min(pos_y - i64::from(MULTIPLE_OFFSET) - stroke_width);
                x2 = x2.max(pos_x + i64::from(MULTIPLE_OFFSET) + width + stroke_width);
            }

            // Include the shape's label box. For inside-label shapes the
            // label sits within the shape and doesn't extend the bbox, but
            // outside-label shapes (containers with default OUTSIDE_TOP_
            // CENTER, image with OUTSIDE_BOTTOM_CENTER, etc.) need their
            // label region included so the diagram has room above/below.
            if !s.text.label.is_empty() && !s.label_position.is_empty() {
                let lw = s.text.label_width as f64;
                let lh = s.text.label_height as f64;
                // label.PADDING from Go d2's lib/label = 5.
                const LABEL_PADDING: f64 = 5.0;
                let shape_box = crate::geo::Box2D::new(
                    crate::geo::Point::new(s.pos.x as f64, s.pos.y as f64),
                    s.width as f64,
                    s.height as f64,
                );
                let label_tl = label_top_left(&s.label_position, &shape_box, LABEL_PADDING, lw, lh);
                let mut label_x = label_tl.0;
                let mut label_y = label_tl.1;

                // 3D shapes: outside labels shift with the 3D offset
                if s.three_dee {
                    let offset = if s.type_ == SHAPE_HEXAGON {
                        THREE_DEE_OFFSET / 2
                    } else {
                        THREE_DEE_OFFSET
                    } as f64;
                    if s.label_position.starts_with("OUTSIDE_RIGHT") {
                        label_x += offset;
                    }
                    if s.label_position.starts_with("OUTSIDE_TOP") {
                        label_y -= offset;
                    }
                }
                // Multiple shapes: outside labels also shift
                if s.multiple {
                    let offset = MULTIPLE_OFFSET as f64;
                    if s.label_position.starts_with("OUTSIDE_RIGHT") {
                        label_x += offset;
                    }
                    if s.label_position.starts_with("OUTSIDE_TOP") {
                        label_y -= offset;
                    }
                }

                let lx = label_x as i64;
                let ly = label_y as i64;
                x1 = x1.min(lx);
                y1 = y1.min(ly);
                x2 = x2.max(lx + i64::from(s.text.label_width));
                y2 = y2.max(ly + i64::from(s.text.label_height));
            }

            if s.icon.is_some()
                && !s.icon_position.is_empty()
                && s.icon_position.contains("OUTSIDE")
            {
                // Go uses `strings.HasPrefix(IconPosition, "OUTSIDE_TOP")`
                // style checks here and only extends the bbox along the axis
                // the label/icon sits on. OUTSIDE_TOP_LEFT contributes solely
                // to y1 (not x1), etc.
                let icon_side = i64::from(icon_size(s.width as f64, s.height as f64, false));
                const LABEL_PADDING: i64 = 5;
                if s.icon_position.starts_with("OUTSIDE_TOP") {
                    y1 = y1.min(pos_y - LABEL_PADDING - icon_side);
                } else if s.icon_position.starts_with("OUTSIDE_BOTTOM") {
                    y2 = y2.max(pos_y + height + LABEL_PADDING + icon_side);
                } else if s.icon_position.starts_with("OUTSIDE_LEFT") {
                    x1 = x1.min(pos_x - LABEL_PADDING - icon_side);
                } else if s.icon_position.starts_with("OUTSIDE_RIGHT") {
                    x2 = x2.max(pos_x + width + LABEL_PADDING + icon_side);
                }
            }
        }

        for c in &self.connections {
            for point in &c.route {
                let half_stroke = (c.stroke_width as f64 / 2.0).ceil() as i64;
                x1 = x1.min(point.x.floor() as i64 - half_stroke);
                y1 = y1.min(point.y.floor() as i64 - half_stroke);
                x2 = x2.max(point.x.ceil() as i64 + half_stroke);
                y2 = y2.max(point.y.ceil() as i64 + half_stroke);
            }

            // Include arrowhead labels (src/dst) in the bounding box.
            if let Some(ref l) = c.src_label
                && !l.label.is_empty()
                && !c.route.is_empty()
                && let Some(tl) = arrowhead_label_tl(c, false)
            {
                x1 = x1.min(tl.0 as i64);
                y1 = y1.min(tl.1 as i64);
                x2 = x2.max(tl.0 as i64 + i64::from(l.label_width));
                y2 = y2.max(tl.1 as i64 + i64::from(l.label_height));
            }
            if let Some(ref l) = c.dst_label
                && !l.label.is_empty()
                && !c.route.is_empty()
                && let Some(tl) = arrowhead_label_tl(c, true)
            {
                x1 = x1.min(tl.0 as i64);
                y1 = y1.min(tl.1 as i64);
                x2 = x2.max(tl.0 as i64 + i64::from(l.label_width));
                y2 = y2.max(tl.1 as i64 + i64::from(l.label_height));
            }

            // Include the connection's label in the bounding box.
            if !c.text.label.is_empty() && !c.label_position.is_empty() && !c.route.is_empty() {
                let pos = crate::label::Position::from_string(&c.label_position);
                let route = crate::geo::Route(c.route.clone());
                if let Some((label_tl, _)) = pos.get_point_on_route(
                    &route,
                    c.stroke_width as f64,
                    c.label_percentage,
                    c.text.label_width as f64,
                    c.text.label_height as f64,
                ) {
                    x1 = x1.min(label_tl.x as i64);
                    y1 = y1.min(label_tl.y as i64);
                    x2 = x2.max(label_tl.x as i64 + i64::from(c.text.label_width));
                    y2 = y2.max(label_tl.y as i64 + i64::from(c.text.label_height));
                }
            }
        }

        (
            Point::new(clamp_i64_to_i32(x1), clamp_i64_to_i32(y1)),
            Point::new(clamp_i64_to_i32(x2), clamp_i64_to_i32(y2)),
        )
    }

    /// Compute the bounding box including nested layers, scenarios, and steps.
    pub fn nested_bounding_box(&self) -> (Point, Point) {
        let (mut tl, mut br) = self.bounding_box();
        for d in &self.layers {
            let (tl2, br2) = d.nested_bounding_box();
            tl.x = tl.x.min(tl2.x);
            tl.y = tl.y.min(tl2.y);
            br.x = br.x.max(br2.x);
            br.y = br.y.max(br2.y);
        }
        for d in &self.scenarios {
            let (tl2, br2) = d.nested_bounding_box();
            tl.x = tl.x.min(tl2.x);
            tl.y = tl.y.min(tl2.y);
            br.x = br.x.max(br2.x);
            br.y = br.y.max(br2.y);
        }
        for d in &self.steps {
            let (tl2, br2) = d.nested_bounding_box();
            tl.x = tl.x.min(tl2.x);
            tl.y = tl.y.min(tl2.y);
            br.x = br.x.max(br2.x);
            br.y = br.y.max(br2.y);
        }
        (tl, br)
    }
}

// ---------------------------------------------------------------------------
// Go-compatible JSON byte serialization
// ---------------------------------------------------------------------------
//
// This module reproduces Go `encoding/json` output for the subset of
// d2target types that feed into `Diagram.Bytes()` (used for the diagram
// hash ID). Byte-for-byte compatibility matters because the hash gets
// embedded in CSS selectors inside the rendered SVG and the e2e fixtures
// compare byte-identical output against Go's reference implementation.
//
// Field order, omitempty semantics, float formatting, and JSON string
// escaping all follow the Go conventions used by the upstream types in
// `d2/d2target/`.

pub mod go_json {
    use super::*;

    fn write_string(out: &mut Vec<u8>, s: &str) {
        out.push(b'"');
        for ch in s.chars() {
            match ch {
                '"' => out.extend_from_slice(b"\\\""),
                '\\' => out.extend_from_slice(b"\\\\"),
                '\n' => out.extend_from_slice(b"\\n"),
                '\r' => out.extend_from_slice(b"\\r"),
                '\t' => out.extend_from_slice(b"\\t"),
                '\u{08}' => out.extend_from_slice(b"\\b"),
                '\u{0c}' => out.extend_from_slice(b"\\f"),
                // Go's encoding/json HTML-escapes these by default.
                '<' => out.extend_from_slice(b"\\u003c"),
                '>' => out.extend_from_slice(b"\\u003e"),
                '&' => out.extend_from_slice(b"\\u0026"),
                c if (c as u32) < 0x20 => {
                    out.extend_from_slice(format!("\\u{:04x}", c as u32).as_bytes());
                }
                c => {
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
        }
        out.push(b'"');
    }

    fn write_bool(out: &mut Vec<u8>, b: bool) {
        out.extend_from_slice(if b { b"true" } else { b"false" });
    }

    fn write_i32(out: &mut Vec<u8>, n: i32) {
        out.extend_from_slice(n.to_string().as_bytes());
    }

    fn write_i64(out: &mut Vec<u8>, n: i64) {
        out.extend_from_slice(n.to_string().as_bytes());
    }

    /// Format a float using Go's `strconv.FormatFloat(f, 'g', -1, 64)` rules,
    /// which encoding/json uses for non-integer float64 values. Integer-valued
    /// floats print as `0`, `1`, ... (no trailing dot).
    fn write_f64(out: &mut Vec<u8>, f: f64) {
        if f == 0.0 {
            out.push(b'0');
            return;
        }
        if f.is_nan() || f.is_infinite() {
            // Go would error on these; best-effort output.
            out.extend_from_slice(b"0");
            return;
        }
        if f == f.trunc() && f.abs() < 1e16 {
            out.extend_from_slice((f as i64).to_string().as_bytes());
            return;
        }
        out.extend_from_slice(format_f64_go_compat(f).as_bytes());
    }

    /// Format `f` to mimic Go's `strconv.FormatFloat(f, 'g', -1, 64)` (which
    /// is what `encoding/json` uses for float64). Both Rust's std and Go
    /// produce a shortest-roundtrip representation, but the two algorithms
    /// can differ when *two* equally-short decimal strings round to the
    /// same f64.
    ///
    /// Algorithm: grab the exact 17-significant-digit decimal of `f` via
    /// Rust's scientific formatter (`{:.16e}` gives 1+16 = 17 sig digits),
    /// truncate to 16 sig digits with round-half-to-even, and check whether
    /// that 16-digit form parses back to the same f64. If it does, use the
    /// 16-digit form (matches Go's "shortest"). Otherwise use the 17-digit
    /// form.
    ///
    /// Two illustrative cases from our route data:
    ///
    /// * f64 `0x408dd599a000_0000` = 954.70001220703125 exact. Both
    ///   `954.7000122070312` and `954.7000122070313` round-trip; the
    ///   17th digit is `5` (true halfway), so round-to-even keeps the
    ///   `2`. Both Go and this function pick `954.7000122070312`. Rust's
    ///   `format!("{}", f)` would pick `...3`.
    /// * f64 `0x407b033340000000` = 432.20001220703125 exact. The
    ///   16-digit truncation `432.2000122070312` parses back to a
    ///   *different* f64, so we keep the full 17-digit form
    ///   `432.20001220703125`.
    fn format_f64_go_compat(f: f64) -> String {
        // Cheap fast-path: if Rust's default already matches what Go would
        // produce (e.g. integer-valued, simple terminating decimals), we
        // can skip the more elaborate path. The check is: does
        // `format!("{:.16e}", f)` end in zeros, indicating the value has
        // fewer than 17 significant digits?
        let sci17 = format!("{:.16e}", f);

        // Parse the scientific representation: "[-]M.MMMMMMMMMMMMMMMMeE"
        let bytes = sci17.as_bytes();
        let neg = bytes[0] == b'-';
        let unsigned = if neg { &sci17[1..] } else { &sci17[..] };
        let e_pos = unsigned.find('e').unwrap();
        let mantissa = &unsigned[..e_pos];
        let exp: i32 = unsigned[e_pos + 1..].parse().unwrap();
        let dot_pos = mantissa.find('.').unwrap();
        let int_part = &mantissa[..dot_pos];
        let frac_part = &mantissa[dot_pos + 1..];
        // 17 significant digits, no decimal point.
        let mut digits: Vec<u8> = int_part.bytes().chain(frac_part.bytes()).collect();
        // dp17: position of the decimal point counted from the left of `digits`,
        // assuming all 17 digits are significant.
        let dp17: i32 = int_part.len() as i32 + exp;

        // Strip trailing zeros so that `digits.len()` reflects the actual
        // number of significant digits Rust's scientific formatter felt
        // were needed. (For e.g. 1.0e0 → "1" with dp=1.)
        while digits.len() > 1 && *digits.last().unwrap() == b'0' {
            digits.pop();
        }
        // Range of plain-decimal output Go uses: roughly 10⁻⁴ ≤ |f| < 10²¹
        // for 'g' fmt with prec=-1. Outside that range Go switches to
        // scientific (with `e+XX`). Mirror that switch here.
        if !(-3..=21).contains(&dp17) {
            return format_f64_scientific_go(f, neg, &digits, dp17);
        }

        // Build a candidate "shortest" form by trying every length from 1
        // up to digits.len(); use the first one whose round-trip matches.
        // For reasonable values this resolves at length ≥ 15 in practice,
        // but the loop bounds the worst case.
        for len in 1..=digits.len() {
            let prefix = &digits[..len];
            let next_digit = digits.get(len).copied();
            // Round half-to-even.
            let mut rounded: Vec<u8> = prefix.to_vec();
            if let Some(nd) = next_digit {
                let round_up = match nd {
                    b'0'..=b'4' => false,
                    b'6'..=b'9' => true,
                    b'5' => {
                        // tie: check if any digit after `nd` is non-zero
                        let any_nonzero = digits[len + 1..].iter().any(|&b| b != b'0');
                        if any_nonzero {
                            true
                        } else {
                            // true halfway → round to even
                            (rounded[len - 1] - b'0') % 2 == 1
                        }
                    }
                    _ => false,
                };
                if round_up {
                    // Carry through the digits.
                    let mut idx = len;
                    let mut carry = true;
                    while carry && idx > 0 {
                        idx -= 1;
                        if rounded[idx] == b'9' {
                            rounded[idx] = b'0';
                        } else {
                            rounded[idx] += 1;
                            carry = false;
                        }
                    }
                    if carry {
                        // 9999... → 10000..., shift dp by 1.
                        rounded.insert(0, b'1');
                        // dp shifts right by 1
                        if let Ok(s) = format_decimal(neg, &rounded, dp17 + 1).parse::<f64>()
                            && s == f
                        {
                            return format_decimal(neg, &rounded, dp17 + 1);
                        }
                        continue;
                    }
                }
            }
            let candidate = format_decimal(neg, &rounded, dp17);
            if candidate.parse::<f64>().ok() == Some(f) {
                return candidate;
            }
        }
        // Fallback: full 17-digit form.
        format_decimal(neg, &digits, dp17)
    }

    /// Format a digit slice + decimal-point position into a plain decimal
    /// string, mirroring Go's `strconv` 'g' format for the in-range case.
    fn format_decimal(neg: bool, digits: &[u8], dp: i32) -> String {
        let sign = if neg { "-" } else { "" };
        let n = digits.len() as i32;
        if dp <= 0 {
            // 0.000ddd...
            let zeros = (-dp) as usize;
            let frac: String = std::iter::repeat_n('0', zeros)
                .chain(digits.iter().map(|&b| b as char))
                .collect();
            // Trim trailing zeros from frac.
            let frac = frac.trim_end_matches('0');
            if frac.is_empty() {
                return format!("{}0", sign);
            }
            format!("{}0.{}", sign, frac)
        } else if dp >= n {
            // dddd000... (no fractional part)
            let extra = (dp - n) as usize;
            let int: String = digits
                .iter()
                .map(|&b| b as char)
                .chain(std::iter::repeat_n('0', extra))
                .collect();
            format!("{}{}", sign, int)
        } else {
            // dddd.dddd
            let dp = dp as usize;
            let int_str: String = digits[..dp].iter().map(|&b| b as char).collect();
            let frac_str: String = digits[dp..].iter().map(|&b| b as char).collect();
            let frac_str = frac_str.trim_end_matches('0');
            if frac_str.is_empty() {
                format!("{}{}", sign, int_str)
            } else {
                format!("{}{}.{}", sign, int_str, frac_str)
            }
        }
    }

    /// Fallback for very large or very small magnitudes — Go uses
    /// scientific notation here. We don't expect to hit this in d2's JSON
    /// (layout coordinates stay in a sane range), so the implementation is
    /// best-effort.
    fn format_f64_scientific_go(f: f64, _neg: bool, _digits: &[u8], _dp17: i32) -> String {
        // Defer to Rust's default — for out-of-range values byte parity
        // with Go isn't currently exercised by our tests.
        format!("{}", f)
    }

    fn write_null(out: &mut Vec<u8>) {
        out.extend_from_slice(b"null");
    }

    /// Serialize an icon URL the same way Go's `encoding/json` serializes
    /// a `*url.URL`: as a struct with the parsed components. Go stores
    /// `Shape.Icon` and `Connection.Icon` as `*url.URL`, so
    /// `json.Marshal` emits
    /// `{"Scheme":..,"Opaque":"","User":null,"Host":..,"Path":..,..}`.
    /// Without this the Rust serialization drifts from Go's and the
    /// diagram hash — which is embedded in every CSS selector — differs.
    fn write_icon_url(out: &mut Vec<u8>, url: &str) {
        let parsed = parse_url(url);
        out.push(b'{');
        out.extend_from_slice(b"\"Scheme\":");
        write_string(out, &parsed.scheme);
        out.extend_from_slice(b",\"Opaque\":");
        write_string(out, &parsed.opaque);
        out.extend_from_slice(b",\"User\":null");
        out.extend_from_slice(b",\"Host\":");
        write_string(out, &parsed.host);
        out.extend_from_slice(b",\"Path\":");
        write_string(out, &parsed.path);
        out.extend_from_slice(b",\"RawPath\":");
        write_string(out, &parsed.raw_path);
        out.extend_from_slice(b",\"OmitHost\":false");
        out.extend_from_slice(b",\"ForceQuery\":");
        if parsed.force_query {
            out.extend_from_slice(b"true");
        } else {
            out.extend_from_slice(b"false");
        }
        out.extend_from_slice(b",\"RawQuery\":");
        write_string(out, &parsed.raw_query);
        out.extend_from_slice(b",\"Fragment\":");
        write_string(out, &parsed.fragment);
        out.extend_from_slice(b",\"RawFragment\":");
        write_string(out, &parsed.raw_fragment);
        out.push(b'}');
    }

    #[derive(Default, Debug)]
    struct ParsedUrl {
        scheme: String,
        opaque: String,
        host: String,
        path: String,
        raw_path: String,
        force_query: bool,
        raw_query: String,
        fragment: String,
        raw_fragment: String,
    }

    /// Minimal subset of Go `net/url.Parse` sufficient for the icon URLs
    /// d2 emits (scheme://host/path?query#fragment, with Go's escaping
    /// rules). Populates `raw_path` / `raw_fragment` only when the raw
    /// form differs from the decoded form (same as Go).
    fn parse_url(input: &str) -> ParsedUrl {
        let mut out = ParsedUrl::default();
        let mut rest = input;

        // Fragment (`#…`)
        if let Some(hash) = rest.find('#') {
            let frag = &rest[hash + 1..];
            rest = &rest[..hash];
            let decoded = url_unescape(frag);
            out.fragment = decoded.clone();
            if decoded != frag {
                out.raw_fragment = frag.to_owned();
            }
        }

        // Query (`?…`)
        if let Some(q) = rest.find('?') {
            out.raw_query = rest[q + 1..].to_owned();
            if rest.ends_with('?') {
                out.force_query = true;
            }
            rest = &rest[..q];
        }

        // Scheme (`scheme:`) — only matches if first non-alpha char is `:`.
        let bytes = rest.as_bytes();
        let mut scheme_end = 0;
        for (i, &b) in bytes.iter().enumerate() {
            if i == 0 {
                if !b.is_ascii_alphabetic() {
                    break;
                }
            } else if !(b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.') {
                if b == b':' {
                    scheme_end = i;
                }
                break;
            }
        }
        if scheme_end > 0 {
            out.scheme = rest[..scheme_end].to_ascii_lowercase();
            rest = &rest[scheme_end + 1..];
        }

        // Authority (`//host`)
        if let Some(stripped) = rest.strip_prefix("//") {
            // host ends at first '/', '?', or '#' (none of which are in rest
            // after we stripped query/fragment already; only '/' applies).
            if let Some(slash) = stripped.find('/') {
                out.host = stripped[..slash].to_owned();
                rest = &stripped[slash..];
            } else {
                out.host = stripped.to_owned();
                rest = "";
            }
        } else if !rest.is_empty() && !rest.starts_with('/') && !out.scheme.is_empty() {
            // `scheme:opaque` form (no authority).
            out.opaque = rest.to_owned();
            rest = "";
        }

        // Path — Go `url.Parse` sets RawPath only when
        // `url.EscapedPath()` would produce a *different* encoding than
        // the original raw form.  Standard percent-encoding (%20 for
        // space, etc.) is reproduced exactly by EscapedPath(), so Go
        // leaves RawPath empty.  We implement the same rule: set
        // raw_path only when decoding + re-encoding changes the form.
        if !rest.is_empty() {
            let decoded = url_unescape(rest);
            out.path = decoded.clone();
            // Re-encode using Go's path-escaping rules and compare
            let re_encoded = url_path_escape(&decoded);
            if re_encoded != rest {
                out.raw_path = rest.to_owned();
            }
        }

        out
    }

    /// Percent-decode a URL path/fragment. Only handles `%XX` sequences —
    /// sufficient for the icon URLs d2 emits.
    fn url_unescape(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
    }

    /// Re-encode a decoded URL path using Go's `url.EscapedPath` rules.
    /// Go percent-encodes any byte that `shouldEscape(c, encodePath)`
    /// returns true for. In practice this means: encode everything that
    /// is not an unreserved character (alpha, digit, `-._~`) or a path
    /// separator (`/`), plus the sub-delims that Go keeps raw (`!$&'()*+,;=:@`).
    fn url_path_escape(decoded: &str) -> String {
        let mut out = String::with_capacity(decoded.len() * 3);
        for &b in decoded.as_bytes() {
            if b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'-' | b'_'
                        | b'.'
                        | b'~'
                        | b'/'
                        | b'!'
                        | b'$'
                        | b'&'
                        | b'\''
                        | b'('
                        | b')'
                        | b'*'
                        | b'+'
                        | b','
                        | b';'
                        | b'='
                        | b':'
                        | b'@'
                )
            {
                out.push(b as char);
            } else {
                out.push_str(&format!("%{:02X}", b));
            }
        }
        out
    }

    fn write_field_name(out: &mut Vec<u8>, first: &mut bool, name: &str) {
        if !*first {
            out.push(b',');
        }
        *first = false;
        out.push(b'"');
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(b"\":");
    }

    fn marshal_point(out: &mut Vec<u8>, p: &Point) {
        out.extend_from_slice(b"{\"x\":");
        write_i32(out, p.x);
        out.extend_from_slice(b",\"y\":");
        write_i32(out, p.y);
        out.push(b'}');
    }

    fn marshal_geo_point(out: &mut Vec<u8>, p: &crate::geo::Point) {
        // Go `*geo.Point` struct: Point{X float64, Y float64}.
        out.extend_from_slice(b"{\"x\":");
        write_f64(out, p.x);
        out.extend_from_slice(b",\"y\":");
        write_f64(out, p.y);
        out.push(b'}');
    }

    fn marshal_class(out: &mut Vec<u8>, class: &Class) {
        // {"fields":[...],"methods":[...]}
        out.extend_from_slice(b"\"fields\":");
        if class.fields.is_empty() {
            out.extend_from_slice(b"null");
        } else {
            out.push(b'[');
            for (i, f) in class.fields.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.push(b'{');
                out.extend_from_slice(b"\"name\":");
                write_string(out, &f.name);
                out.extend_from_slice(b",\"type\":");
                write_string(out, &f.type_);
                out.extend_from_slice(b",\"visibility\":");
                write_string(out, &f.visibility);
                out.extend_from_slice(b",\"underline\":");
                write_bool(out, f.underline);
                out.push(b'}');
            }
            out.push(b']');
        }
        out.extend_from_slice(b",\"methods\":");
        if class.methods.is_empty() {
            out.extend_from_slice(b"null");
        } else {
            out.push(b'[');
            for (i, m) in class.methods.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.push(b'{');
                out.extend_from_slice(b"\"name\":");
                write_string(out, &m.name);
                out.extend_from_slice(b",\"return\":");
                write_string(out, &m.return_);
                out.extend_from_slice(b",\"visibility\":");
                write_string(out, &m.visibility);
                out.extend_from_slice(b",\"underline\":");
                write_bool(out, m.underline);
                out.push(b'}');
            }
            out.push(b']');
        }
    }

    fn marshal_sql_table(out: &mut Vec<u8>, t: &SQLTable) {
        out.extend_from_slice(b"\"columns\":");
        if t.columns.is_empty() {
            out.extend_from_slice(b"null");
        } else {
            out.push(b'[');
            for (i, c) in t.columns.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.push(b'{');
                out.extend_from_slice(b"\"name\":");
                marshal_text(out, &c.name);
                out.extend_from_slice(b",\"type\":");
                marshal_text(out, &c.type_);
                out.extend_from_slice(b",\"constraint\":");
                if c.constraint.is_empty() {
                    out.extend_from_slice(b"null");
                } else {
                    out.push(b'[');
                    for (j, s) in c.constraint.iter().enumerate() {
                        if j > 0 {
                            out.push(b',');
                        }
                        write_string(out, s);
                    }
                    out.push(b']');
                }
                out.extend_from_slice(b",\"reference\":");
                write_string(out, &c.reference);
                out.push(b'}');
            }
            out.push(b']');
        }
    }

    fn marshal_text(out: &mut Vec<u8>, t: &Text) {
        out.extend_from_slice(b"{\"label\":");
        write_string(out, &t.label);
        out.extend_from_slice(b",\"fontSize\":");
        write_i32(out, t.font_size);
        out.extend_from_slice(b",\"fontFamily\":");
        write_string(out, &t.font_family);
        out.extend_from_slice(b",\"language\":");
        write_string(out, &t.language);
        out.extend_from_slice(b",\"color\":");
        write_string(out, &t.color);
        out.extend_from_slice(b",\"italic\":");
        write_bool(out, t.italic);
        out.extend_from_slice(b",\"bold\":");
        write_bool(out, t.bold);
        out.extend_from_slice(b",\"underline\":");
        write_bool(out, t.underline);
        out.extend_from_slice(b",\"labelWidth\":");
        write_i32(out, t.label_width);
        out.extend_from_slice(b",\"labelHeight\":");
        write_i32(out, t.label_height);
        if !t.label_fill.is_empty() {
            out.extend_from_slice(b",\"labelFill\":");
            write_string(out, &t.label_fill);
        }
        out.push(b'}');
    }

    /// Marshal a Shape following Go field order in `d2target.Shape`.
    pub fn marshal_shape(out: &mut Vec<u8>, s: &Shape) {
        out.push(b'{');
        out.extend_from_slice(b"\"id\":");
        write_string(out, &s.id);
        out.extend_from_slice(b",\"type\":");
        write_string(out, &s.type_);
        // classes omitempty
        if !s.classes.is_empty() {
            out.extend_from_slice(b",\"classes\":[");
            for (i, c) in s.classes.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_string(out, c);
            }
            out.push(b']');
        }
        out.extend_from_slice(b",\"pos\":");
        marshal_point(out, &s.pos);
        out.extend_from_slice(b",\"width\":");
        write_i32(out, s.width);
        out.extend_from_slice(b",\"height\":");
        write_i32(out, s.height);
        out.extend_from_slice(b",\"opacity\":");
        write_f64(out, s.opacity);
        out.extend_from_slice(b",\"strokeDash\":");
        write_f64(out, s.stroke_dash);
        out.extend_from_slice(b",\"strokeWidth\":");
        write_i32(out, s.stroke_width);
        out.extend_from_slice(b",\"borderRadius\":");
        write_i32(out, s.border_radius);
        out.extend_from_slice(b",\"fill\":");
        write_string(out, &s.fill);
        // fillPattern omitempty
        if !s.fill_pattern.is_empty() {
            out.extend_from_slice(b",\"fillPattern\":");
            write_string(out, &s.fill_pattern);
        }
        out.extend_from_slice(b",\"stroke\":");
        write_string(out, &s.stroke);
        out.extend_from_slice(b",\"animated\":");
        write_bool(out, s.animated);
        out.extend_from_slice(b",\"shadow\":");
        write_bool(out, s.shadow);
        out.extend_from_slice(b",\"3d\":");
        write_bool(out, s.three_dee);
        out.extend_from_slice(b",\"multiple\":");
        write_bool(out, s.multiple);
        out.extend_from_slice(b",\"double-border\":");
        write_bool(out, s.double_border);
        out.extend_from_slice(b",\"tooltip\":");
        write_string(out, &s.tooltip);
        out.extend_from_slice(b",\"link\":");
        write_string(out, &s.link);
        // prettyLink omitempty
        if !s.pretty_link.is_empty() {
            out.extend_from_slice(b",\"prettyLink\":");
            write_string(out, &s.pretty_link);
        }
        out.extend_from_slice(b",\"icon\":");
        if let Some(ref i) = s.icon {
            write_icon_url(out, i);
        } else {
            write_null(out);
        }
        // iconBorderRadius omitempty
        if s.icon_border_radius != 0 {
            out.extend_from_slice(b",\"iconBorderRadius\":");
            write_i32(out, s.icon_border_radius);
        }
        out.extend_from_slice(b",\"iconPosition\":");
        write_string(out, &s.icon_position);
        out.extend_from_slice(b",\"blend\":");
        write_bool(out, s.blend);
        // Embedded Class
        out.push(b',');
        marshal_class(out, &s.class);
        // Embedded SQLTable
        out.push(b',');
        marshal_sql_table(out, &s.sql_table);
        // contentAspectRatio omitempty (pointer)
        if let Some(v) = s.content_aspect_ratio {
            out.extend_from_slice(b",\"contentAspectRatio\":");
            write_f64(out, v);
        }
        // Embedded Text (Go promotes fields; encoding/json flattens them)
        out.extend_from_slice(b",\"label\":");
        write_string(out, &s.text.label);
        out.extend_from_slice(b",\"fontSize\":");
        write_i32(out, s.text.font_size);
        out.extend_from_slice(b",\"fontFamily\":");
        write_string(out, &s.text.font_family);
        out.extend_from_slice(b",\"language\":");
        write_string(out, &s.text.language);
        out.extend_from_slice(b",\"color\":");
        write_string(out, &s.text.color);
        out.extend_from_slice(b",\"italic\":");
        write_bool(out, s.text.italic);
        out.extend_from_slice(b",\"bold\":");
        write_bool(out, s.text.bold);
        out.extend_from_slice(b",\"underline\":");
        write_bool(out, s.text.underline);
        out.extend_from_slice(b",\"labelWidth\":");
        write_i32(out, s.text.label_width);
        out.extend_from_slice(b",\"labelHeight\":");
        write_i32(out, s.text.label_height);
        if !s.text.label_fill.is_empty() {
            out.extend_from_slice(b",\"labelFill\":");
            write_string(out, &s.text.label_fill);
        }
        // labelPosition omitempty.
        //
        // Match Go: a shape with no label still keeps any class-derived
        // position in `attrs.LabelPosition`, but Go's d2graph clears it
        // before export when the label is empty (unlike the `label.near`
        // value, the position pointer is dropped). Mirror that effective
        // behavior here so the JSON byte stream — which feeds the diagram
        // hash — matches Go for empty-labeled shapes (e.g. `1*.label: ""`
        // siblings under a `class: note` glob).
        let suppress_for_empty_label = s.text.label.is_empty() && s.text.label_width == 0;
        if !s.label_position.is_empty() && !suppress_for_empty_label {
            out.extend_from_slice(b",\"labelPosition\":");
            write_string(out, &s.label_position);
        }
        // tooltipPosition omitempty
        if !s.tooltip_position.is_empty() {
            out.extend_from_slice(b",\"tooltipPosition\":");
            write_string(out, &s.tooltip_position);
        }
        out.extend_from_slice(b",\"zIndex\":");
        write_i32(out, s.z_index);
        out.extend_from_slice(b",\"level\":");
        write_i32(out, s.level);
        if !s.primary_accent_color.is_empty() {
            out.extend_from_slice(b",\"primaryAccentColor\":");
            write_string(out, &s.primary_accent_color);
        }
        if !s.secondary_accent_color.is_empty() {
            out.extend_from_slice(b",\"secondaryAccentColor\":");
            write_string(out, &s.secondary_accent_color);
        }
        if !s.neutral_accent_color.is_empty() {
            out.extend_from_slice(b",\"neutralAccentColor\":");
            write_string(out, &s.neutral_accent_color);
        }
        out.push(b'}');
    }

    /// Marshal a Connection following Go field order in `d2target.Connection`.
    pub fn marshal_connection(out: &mut Vec<u8>, c: &Connection) {
        out.push(b'{');
        out.extend_from_slice(b"\"id\":");
        write_string(out, &c.id);
        if !c.classes.is_empty() {
            out.extend_from_slice(b",\"classes\":[");
            for (i, cl) in c.classes.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_string(out, cl);
            }
            out.push(b']');
        }
        out.extend_from_slice(b",\"src\":");
        write_string(out, &c.src);
        out.extend_from_slice(b",\"srcArrow\":");
        write_string(out, c.src_arrow.as_str());
        if let Some(ref l) = c.src_label {
            out.extend_from_slice(b",\"srcLabel\":");
            marshal_text(out, l);
        }
        out.extend_from_slice(b",\"dst\":");
        write_string(out, &c.dst);
        out.extend_from_slice(b",\"dstArrow\":");
        write_string(out, c.dst_arrow.as_str());
        if let Some(ref l) = c.dst_label {
            out.extend_from_slice(b",\"dstLabel\":");
            marshal_text(out, l);
        }
        out.extend_from_slice(b",\"opacity\":");
        write_f64(out, c.opacity);
        out.extend_from_slice(b",\"strokeDash\":");
        write_f64(out, c.stroke_dash);
        out.extend_from_slice(b",\"strokeWidth\":");
        write_i32(out, c.stroke_width);
        out.extend_from_slice(b",\"stroke\":");
        write_string(out, &c.stroke);
        if !c.fill.is_empty() {
            out.extend_from_slice(b",\"fill\":");
            write_string(out, &c.fill);
        }
        if c.border_radius != 0.0 {
            out.extend_from_slice(b",\"borderRadius\":");
            write_f64(out, c.border_radius);
        }
        // Embedded Text fields
        out.extend_from_slice(b",\"label\":");
        write_string(out, &c.text.label);
        out.extend_from_slice(b",\"fontSize\":");
        write_i32(out, c.text.font_size);
        out.extend_from_slice(b",\"fontFamily\":");
        write_string(out, &c.text.font_family);
        out.extend_from_slice(b",\"language\":");
        write_string(out, &c.text.language);
        out.extend_from_slice(b",\"color\":");
        write_string(out, &c.text.color);
        out.extend_from_slice(b",\"italic\":");
        write_bool(out, c.text.italic);
        out.extend_from_slice(b",\"bold\":");
        write_bool(out, c.text.bold);
        out.extend_from_slice(b",\"underline\":");
        write_bool(out, c.text.underline);
        out.extend_from_slice(b",\"labelWidth\":");
        write_i32(out, c.text.label_width);
        out.extend_from_slice(b",\"labelHeight\":");
        write_i32(out, c.text.label_height);
        if !c.text.label_fill.is_empty() {
            out.extend_from_slice(b",\"labelFill\":");
            write_string(out, &c.text.label_fill);
        }
        out.extend_from_slice(b",\"labelPosition\":");
        write_string(out, &c.label_position);
        out.extend_from_slice(b",\"labelPercentage\":");
        write_f64(out, c.label_percentage);
        out.extend_from_slice(b",\"link\":");
        write_string(out, &c.link);
        if !c.pretty_link.is_empty() {
            out.extend_from_slice(b",\"prettyLink\":");
            write_string(out, &c.pretty_link);
        }
        out.extend_from_slice(b",\"route\":");
        if c.route.is_empty() {
            out.extend_from_slice(b"null");
        } else {
            out.push(b'[');
            for (i, p) in c.route.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                marshal_geo_point(out, p);
            }
            out.push(b']');
        }
        if c.is_curve {
            out.extend_from_slice(b",\"isCurve\":true");
        }
        out.extend_from_slice(b",\"animated\":");
        write_bool(out, c.animated);
        out.extend_from_slice(b",\"tooltip\":");
        write_string(out, &c.tooltip);
        out.extend_from_slice(b",\"icon\":");
        if let Some(ref i) = c.icon {
            write_icon_url(out, i);
        } else {
            write_null(out);
        }
        if !c.icon_position.is_empty() {
            out.extend_from_slice(b",\"iconPosition\":");
            write_string(out, &c.icon_position);
        }
        if c.icon_border_radius != 0.0 {
            out.extend_from_slice(b",\"iconBorderRadius\":");
            write_f64(out, c.icon_border_radius);
        }
        out.extend_from_slice(b",\"zIndex\":");
        write_i32(out, c.z_index);
        out.push(b'}');
    }

    fn marshal_theme_overrides(out: &mut Vec<u8>, t: &crate::themes::ThemeOverrides) {
        out.push(b'{');
        let mut first = true;

        macro_rules! write_override {
            ($json:literal, $field:ident) => {
                if let Some(ref v) = t.$field {
                    write_field_name(out, &mut first, $json);
                    write_string(out, v);
                }
            };
        }

        write_override!("n1", n1);
        write_override!("n2", n2);
        write_override!("n3", n3);
        write_override!("n4", n4);
        write_override!("n5", n5);
        write_override!("n6", n6);
        write_override!("n7", n7);
        write_override!("b1", b1);
        write_override!("b2", b2);
        write_override!("b3", b3);
        write_override!("b4", b4);
        write_override!("b5", b5);
        write_override!("b6", b6);
        write_override!("aa2", aa2);
        write_override!("aa4", aa4);
        write_override!("aa5", aa5);
        write_override!("ab4", ab4);
        write_override!("ab5", ab5);

        out.push(b'}');
    }

    fn marshal_string_map(out: &mut Vec<u8>, m: &std::collections::HashMap<String, String>) {
        out.push(b'{');
        let mut first = true;
        let mut entries: Vec<_> = m.iter().collect();
        entries.sort_by_key(|(ka, _)| *ka);
        for (k, v) in entries {
            write_field_name(out, &mut first, k);
            write_string(out, v);
        }
        out.push(b'}');
    }

    /// Marshal the diagram Config (matches Go `d2target.Config` json tags).
    pub fn marshal_config(out: &mut Vec<u8>, c: &Config) {
        out.push(b'{');
        let mut first = true;

        write_field_name(out, &mut first, "sketch");
        match c.sketch {
            Some(v) => write_bool(out, v),
            None => write_null(out),
        }
        write_field_name(out, &mut first, "themeID");
        match c.theme_id {
            Some(v) => write_i64(out, v),
            None => write_null(out),
        }
        write_field_name(out, &mut first, "darkThemeID");
        match c.dark_theme_id {
            Some(v) => write_i64(out, v),
            None => write_null(out),
        }
        write_field_name(out, &mut first, "pad");
        match c.pad {
            Some(v) => write_i64(out, v),
            None => write_null(out),
        }
        write_field_name(out, &mut first, "center");
        match c.center {
            Some(v) => write_bool(out, v),
            None => write_null(out),
        }
        write_field_name(out, &mut first, "layoutEngine");
        match c.layout_engine {
            Some(ref v) => write_string(out, v),
            None => write_null(out),
        }
        if let Some(ref v) = c.theme_overrides {
            write_field_name(out, &mut first, "themeOverrides");
            marshal_theme_overrides(out, v);
        }
        if let Some(ref v) = c.dark_theme_overrides {
            write_field_name(out, &mut first, "darkThemeOverrides");
            marshal_theme_overrides(out, v);
        }
        if !c.data.is_empty() {
            write_field_name(out, &mut first, "data");
            marshal_string_map(out, &c.data);
        }
        out.push(b'}');
    }

    /// Reproduce Go `Diagram.Bytes()` byte stream:
    ///   json(Shapes) + json(Connections) + json(Root) + [json(Config)] + nested boards' bytes
    pub fn diagram_bytes(diagram: &Diagram) -> Vec<u8> {
        let mut out = Vec::with_capacity(512);

        // shapes
        out.push(b'[');
        for (i, s) in diagram.shapes.iter().enumerate() {
            if i > 0 {
                out.push(b',');
            }
            marshal_shape(&mut out, s);
        }
        out.push(b']');

        // connections
        out.push(b'[');
        for (i, c) in diagram.connections.iter().enumerate() {
            if i > 0 {
                out.push(b',');
            }
            marshal_connection(&mut out, c);
        }
        out.push(b']');

        // root
        marshal_shape(&mut out, &diagram.root);

        // config (if present)
        if let Some(ref cfg) = diagram.config {
            marshal_config(&mut out, cfg);
        }

        // nested boards (layers, scenarios, steps)
        for d in &diagram.layers {
            out.extend_from_slice(&diagram_bytes(d));
        }
        for d in &diagram.scenarios {
            out.extend_from_slice(&diagram_bytes(d));
        }
        for d in &diagram.steps {
            out.extend_from_slice(&diagram_bytes(d));
        }

        out
    }
}

/// FNV-1a 32-bit hash (matches Go's `hash/fnv.New32a`).
fn fnv1a32(data: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in data {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- Shape type tests --

    #[test]
    fn is_shape_recognizes_all() {
        for s in SHAPES {
            assert!(is_shape(s), "is_shape should recognize: {s}");
        }
    }

    #[test]
    fn is_shape_case_insensitive() {
        assert!(is_shape("Rectangle"));
        assert!(is_shape("RECTANGLE"));
        assert!(is_shape("rectangle"));
    }

    #[test]
    fn is_shape_empty_is_default() {
        assert!(is_shape(""));
    }

    #[test]
    fn is_shape_rejects_unknown() {
        assert!(!is_shape("nonexistent"));
    }

    // -- DSL shape mapping --

    #[test]
    fn dsl_to_shape_type_mapping() {
        assert_eq!(dsl_shape_to_shape_type(""), shape_type::SQUARE);
        assert_eq!(dsl_shape_to_shape_type(SHAPE_RECTANGLE), shape_type::SQUARE);
        assert_eq!(
            dsl_shape_to_shape_type(SHAPE_SQUARE),
            shape_type::REAL_SQUARE
        );
        assert_eq!(dsl_shape_to_shape_type(SHAPE_OVAL), shape_type::OVAL);
        assert_eq!(dsl_shape_to_shape_type(SHAPE_SQL_TABLE), shape_type::TABLE);
        assert_eq!(dsl_shape_to_shape_type(SHAPE_CLASS), shape_type::CLASS);
        assert_eq!(
            dsl_shape_to_shape_type(SHAPE_SEQUENCE_DIAGRAM),
            shape_type::SQUARE
        );
    }

    #[test]
    fn shape_type_to_dsl_round_trip() {
        // Every shape type should map back to a dsl shape
        let test_cases = &[
            (shape_type::SQUARE, SHAPE_RECTANGLE),
            (shape_type::OVAL, SHAPE_OVAL),
            (shape_type::TABLE, SHAPE_SQL_TABLE),
            (shape_type::CLASS, SHAPE_CLASS),
            (shape_type::IMAGE, SHAPE_IMAGE),
        ];
        for &(st, expected_dsl) in test_cases {
            assert_eq!(shape_type_to_dsl_shape(st), expected_dsl);
        }
    }

    // -- Arrowhead tests --

    #[test]
    fn arrowhead_str_round_trip() {
        let cases = &[
            Arrowhead::None,
            Arrowhead::Arrow,
            Arrowhead::Triangle,
            Arrowhead::UnfilledTriangle,
            Arrowhead::Diamond,
            Arrowhead::FilledDiamond,
            Arrowhead::Circle,
            Arrowhead::FilledCircle,
            Arrowhead::Cross,
            Arrowhead::Box_,
            Arrowhead::FilledBox,
            Arrowhead::Line,
            Arrowhead::CfOne,
            Arrowhead::CfMany,
            Arrowhead::CfOneRequired,
            Arrowhead::CfManyRequired,
        ];
        for ah in cases {
            let s = ah.as_str();
            let parsed = Arrowhead::from_str_val(s);
            assert_eq!(&parsed, ah, "round trip failed for {s}");
        }
    }

    #[test]
    fn to_arrowhead_filled_diamond() {
        assert_eq!(
            Arrowhead::to_arrowhead("diamond", Some(true)),
            Arrowhead::FilledDiamond
        );
        assert_eq!(Arrowhead::to_arrowhead("diamond", None), Arrowhead::Diamond);
    }

    #[test]
    fn to_arrowhead_unfilled_triangle() {
        assert_eq!(
            Arrowhead::to_arrowhead("triangle", Some(false)),
            Arrowhead::UnfilledTriangle
        );
    }

    #[test]
    fn arrowhead_dimensions() {
        let (w, h) = Arrowhead::Triangle.dimensions(2.0);
        assert!(w > 0.0 && h > 0.0);

        let (w0, h0) = Arrowhead::None.dimensions(2.0);
        assert_eq!(w0, 0.0);
        assert_eq!(h0, 0.0);
    }

    // -- Shape set_type --

    #[test]
    fn shape_set_type_normalizes() {
        let mut s = Shape::default();
        s.set_type("circle");
        assert_eq!(s.type_, SHAPE_OVAL);

        s.set_type("square");
        assert_eq!(s.type_, SHAPE_RECTANGLE);

        s.set_type("Diamond");
        assert_eq!(s.type_, SHAPE_DIAMOND);
    }

    // -- Class / SQL Table --

    #[test]
    fn class_field_visibility_token() {
        let f = ClassField {
            visibility: "private".to_owned(),
            ..Default::default()
        };
        assert_eq!(f.visibility_token(), "-");

        let f2 = ClassField {
            visibility: "protected".to_owned(),
            ..Default::default()
        };
        assert_eq!(f2.visibility_token(), "#");

        let f3 = ClassField::default();
        assert_eq!(f3.visibility_token(), "+");
    }

    #[test]
    fn sql_column_constraint_abbr() {
        let col = SQLColumn {
            constraint: vec!["primary_key".to_owned(), "unique".to_owned()],
            ..Default::default()
        };
        assert_eq!(col.constraint_abbr(), "PK, UNQ");
    }

    // -- Diagram tests --

    #[test]
    fn diagram_new_has_bg_fill() {
        let d = Diagram::new();
        assert_eq!(d.root.fill, "N7");
    }

    #[test]
    fn diagram_bounding_box_empty() {
        let d = Diagram::new();
        let (tl, br) = d.bounding_box();
        assert_eq!(tl, Point::new(0, 0));
        assert_eq!(br, Point::new(0, 0));
    }

    #[test]
    fn diagram_bounding_box_single_shape() {
        let mut d = Diagram::new();
        d.shapes.push(Shape {
            pos: Point::new(10, 20),
            width: 100,
            height: 50,
            stroke_width: 2,
            ..Default::default()
        });
        let (tl, br) = d.bounding_box();
        assert_eq!(tl, Point::new(9, 19));
        assert_eq!(br, Point::new(111, 71));
    }

    #[test]
    fn diagram_hash_id_deterministic() {
        let mut d = Diagram::new();
        d.shapes.push(Shape {
            id: "a".to_owned(),
            text: Text {
                label: "hello".to_owned(),
                ..Default::default()
            },
            ..Default::default()
        });
        let h1 = d.hash_id(None);
        let h2 = d.hash_id(None);
        assert_eq!(h1, h2);
        assert!(h1.starts_with("d2-"));
    }

    #[test]
    fn diagram_hash_id_with_salt() {
        let d = Diagram::new();
        let h1 = d.hash_id(None);
        let h2 = d.hash_id(Some("salt"));
        assert_ne!(h1, h2);
    }

    #[test]
    fn diagram_get_board() {
        let mut d = Diagram::new();
        d.name = "root".to_owned();
        d.layers.push(Diagram {
            name: "layer1".to_owned(),
            ..Default::default()
        });

        assert!(d.get_board(&["layers", "layer1"]).is_some());
        assert!(d.get_board(&["layer1"]).is_some());
        assert!(d.get_board(&["nonexistent"]).is_none());
    }

    #[test]
    fn base_shape_defaults() {
        let s = base_shape();
        assert_eq!(s.opacity, 1.0);
        assert_eq!(s.stroke_width, 2);
        assert!(s.text.bold);
    }

    #[test]
    fn base_connection_defaults() {
        let c = base_connection();
        assert_eq!(c.src_arrow, Arrowhead::None);
        assert_eq!(c.dst_arrow, Arrowhead::None);
        assert_eq!(c.opacity, 1.0);
        assert_eq!(c.stroke_width, 2);
        assert!(c.text.italic);
    }
}
