//! quadrant-chart parsed model.
//!
//! Mirrors upstream `QuadrantBuilderData` + configurable knobs in
//! `QuadrantBuilderConfig` and the subset of `themeVariables` that the
//! renderer reads. Stored as free-form overrides so the layout stage can
//! merge them on top of theme / defaults without re-parsing.

use crate::model::DiagramMeta;

/// Raw styles captured for a single point (or `classDef` block).
/// Every field is `None` when the source omitted it; the layout stage
/// resolves fallbacks by consulting (point > class > theme).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuadrantStyles {
    pub radius: Option<f64>,
    pub color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<String>,
}

/// A data point in the 0..1 x 0..1 logical space.
#[derive(Debug, Clone, Default)]
pub struct QuadrantPoint {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub class_name: Option<String>,
    pub styles: QuadrantStyles,
}

/// `classDef className key:val, key:val` entry.
#[derive(Debug, Clone, Default)]
pub struct QuadrantClassDef {
    pub name: String,
    pub styles: QuadrantStyles,
}

/// Upstream-matched free-form config overrides. Parsed from the
/// `quadrantChart` branch of a `%%{init: ...}%%` directive.
#[derive(Debug, Clone, Default)]
pub struct QuadrantConfigOverride {
    pub chart_width: Option<f64>,
    pub chart_height: Option<f64>,
    pub title_padding: Option<f64>,
    pub title_font_size: Option<f64>,
    pub quadrant_padding: Option<f64>,
    pub quadrant_text_top_padding: Option<f64>,
    pub quadrant_label_font_size: Option<f64>,
    pub quadrant_internal_border_stroke_width: Option<f64>,
    pub quadrant_external_border_stroke_width: Option<f64>,
    pub x_axis_label_padding: Option<f64>,
    pub x_axis_label_font_size: Option<f64>,
    pub y_axis_label_padding: Option<f64>,
    pub y_axis_label_font_size: Option<f64>,
    pub point_text_padding: Option<f64>,
    pub point_label_font_size: Option<f64>,
    pub point_radius: Option<f64>,
    pub x_axis_position: Option<String>, // "top" | "bottom"
    pub y_axis_position: Option<String>, // "left" | "right"
}

#[derive(Debug, Clone, Default)]
pub struct QuadrantDiagram {
    pub meta: DiagramMeta,

    pub quadrant1_text: String,
    pub quadrant2_text: String,
    pub quadrant3_text: String,
    pub quadrant4_text: String,
    pub x_axis_left_text: String,
    pub x_axis_right_text: String,
    pub y_axis_bottom_text: String,
    pub y_axis_top_text: String,

    /// `addPoints` unshifts new points onto the front of the list — we
    /// store them in the same reversed order so the render order matches.
    pub points: Vec<QuadrantPoint>,
    pub classes: Vec<QuadrantClassDef>,

    /// Effective quadrantChart config overrides from `%%{init:...}%%`.
    pub config: QuadrantConfigOverride,

    /// Raw `themeVariables` JSON value from `%%{init:...}%%` (if any).
    /// The layout stage decodes the quadrant-scoped subset.
    pub theme_overrides_json: Option<serde_json::Value>,

    /// Optional `"theme": "..."` name set via `%%{init:...}%%`.
    pub theme_name: Option<String>,
}
