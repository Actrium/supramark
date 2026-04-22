//! Wardley map (`wardley-beta`) model.
//!
//! This struct is the in-memory representation emitted by
//! [`crate::parser::wardley`] and consumed by
//! [`crate::layout::wardley`]. It mirrors
//! `packages/mermaid/src/diagrams/wardley/wardleyBuilder.ts`
//! from upstream v11.14.0 but stays pure-data (no iteration helpers
//! beyond `Vec<...>` ordering).
//!
//! Coordinate convention (matches upstream):
//! - `x` field = evolution value, already converted to 0..=100 percent
//!   (i.e. the projected horizontal axis input).
//! - `y` field = visibility value, already converted to 0..=100.
//! - Source text uses `[visibility, evolution]`; the parser swaps the
//!   order when building this struct so downstream code can treat
//!   `(x, y)` as raw map coordinates.

use crate::model::DiagramMeta;

/// Wardley component / anchor / pipeline-child node.
///
/// The `class_name` distinguishes how the renderer styles the node:
/// `"anchor"` draws a label-only node (no circle); `"component"` draws
/// the default circle; `"pipeline-component"` is a component that lives
/// inside a pipeline parent box. Upstream uses exactly these three
/// literal strings in `wardley-node--<className>` CSS classes, so we
/// keep them as strings rather than enums.
#[derive(Debug, Clone, Default)]
pub struct WardleyNode {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub class_name: Option<String>,
    pub label_offset_x: Option<i64>,
    pub label_offset_y: Option<i64>,
    pub in_pipeline: bool,
    pub is_pipeline_parent: bool,
    pub inertia: bool,
    pub source_strategy: Option<SourceStrategy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStrategy {
    Build,
    Buy,
    Outsource,
    Market,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkFlow {
    Forward,
    Backward,
    Bidirectional,
}

#[derive(Debug, Clone, Default)]
pub struct WardleyLink {
    pub source: String,
    pub target: String,
    pub dashed: bool,
    pub label: Option<String>,
    pub flow: Option<LinkFlow>,
}

#[derive(Debug, Clone)]
pub struct WardleyTrend {
    pub node_id: String,
    pub target_x: f64, // evolution, 0..=100
    pub target_y: f64, // visibility, 0..=100
}

#[derive(Debug, Clone, Default)]
pub struct WardleyPipeline {
    pub node_id: String,
    pub component_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WardleyAnnotation {
    pub number: i64,
    /// Multi-coordinate annotations draw connecting dashed lines.
    pub coordinates: Vec<(f64, f64)>,
    pub text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WardleyNote {
    pub text: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct WardleyAccelerator {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct WardleyDeaccelerator {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Default)]
pub struct WardleyAxesConfig {
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub stages: Vec<String>,
    pub stage_boundaries: Vec<f64>,
}

/// Top-level parsed diagram.
#[derive(Debug, Clone, Default)]
pub struct WardleyDiagram {
    pub meta: DiagramMeta,
    /// Preserves insertion order — upstream's `Map<string, Node>` iterates
    /// in insertion order and the renderer depends on that order for
    /// SVG byte-exact output.
    pub nodes: Vec<WardleyNode>,
    pub links: Vec<WardleyLink>,
    pub trends: Vec<WardleyTrend>,
    pub pipelines: Vec<WardleyPipeline>,
    pub annotations: Vec<WardleyAnnotation>,
    pub notes: Vec<WardleyNote>,
    pub accelerators: Vec<WardleyAccelerator>,
    pub deaccelerators: Vec<WardleyDeaccelerator>,
    pub annotations_box: Option<(f64, f64)>,
    pub axes: WardleyAxesConfig,
    /// `size [width, height]` override. Defaults (900, 600) applied at
    /// layout time when `None`.
    pub size: Option<(i64, i64)>,
}

impl WardleyDiagram {
    /// Look up a node by id (linear scan — node counts are < ~50 for
    /// every fixture).
    pub fn get_node(&self, id: &str) -> Option<&WardleyNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut WardleyNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }
}
