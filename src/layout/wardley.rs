//! Wardley map layout — resolves every node/link/pipeline coordinate
//! to absolute SVG pixels so the renderer can emit strings directly.
//!
//! All arithmetic here mirrors `wardleyRenderer.ts` (`packages/mermaid/
//! src/diagrams/wardley/`) exactly:
//!
//! - `projectX(v) = padding + (v / 100) * chartWidth`
//! - `projectY(v) = height - padding - (v / 100) * chartHeight`
//!
//! Pipelines require a two-step resolution: the parent's layout `x/y`
//! gets clobbered by the pipeline box centerX / boxTop. The upstream
//! code mutates `positions.get(parent).{x,y}` before any link geometry
//! is computed, so links originating at the parent use the post-
//! pipeline-adjust coordinates.
//!
//! Chart defaults (`wardleyConfig`):
//!   width=900, height=600, padding=48, nodeRadius=6,
//!   nodeLabelOffset=8, axisFontSize=12, labelFontSize=10.
//! Every fixture in scope overrides `size [W, H]`.

use crate::error::Result;
use crate::model::wardley::{WardleyDiagram, WardleyNode};
use crate::theme::ThemeVariables;

pub const DEFAULT_WIDTH: f64 = 900.0;
pub const DEFAULT_HEIGHT: f64 = 600.0;
pub const PADDING: f64 = 48.0;
pub const NODE_RADIUS: f64 = 6.0;
pub const NODE_LABEL_OFFSET: f64 = 8.0;
pub const AXIS_FONT_SIZE: f64 = 12.0;
pub const LABEL_FONT_SIZE: f64 = 10.0;
/// `nodeRadius * 1.6` — pipeline parent square side.
pub const SQUARE_SIZE: f64 = NODE_RADIUS * 1.6;

#[derive(Debug, Clone)]
pub struct LaidOutNode {
    /// Index into `WardleyDiagram::nodes`.
    pub src_index: usize,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct LaidOutPipeline {
    /// Node id of the parent (used to select the right square).
    pub parent_id: String,
    pub box_x: f64,
    pub box_y: f64,
    pub box_w: f64,
    pub box_h: f64,
    /// Component node ids sorted by ascending x (used for evolution
    /// dashed links).
    pub sorted_component_ids: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WardleyLayout {
    pub width: f64,
    pub height: f64,
    pub chart_width: f64,
    pub chart_height: f64,
    /// Node positions in insertion order — parallel to
    /// `WardleyDiagram::nodes`.
    pub node_positions: Vec<LaidOutNode>,
    pub pipelines: Vec<LaidOutPipeline>,
}

pub fn layout(d: &WardleyDiagram, _theme: &ThemeVariables) -> Result<WardleyLayout> {
    let (width, height) = d
        .size
        .map(|(w, h)| (w as f64, h as f64))
        .unwrap_or((DEFAULT_WIDTH, DEFAULT_HEIGHT));
    let chart_width = width - PADDING * 2.0;
    let chart_height = height - PADDING * 2.0;

    // Initial projection for every node.
    let mut positions: Vec<LaidOutNode> = d
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| LaidOutNode {
            src_index: i,
            x: project_x(n.x, chart_width),
            y: project_y(n.y, height, chart_height),
        })
        .collect();

    // Pipeline post-processing — clobber parent `{x, y}` the way the
    // upstream renderer does at line ~355-358.
    let mut pipelines_out: Vec<LaidOutPipeline> = Vec::with_capacity(d.pipelines.len());
    for pipe in &d.pipelines {
        if pipe.component_ids.is_empty() {
            continue;
        }

        // Gather (component_id, x, y, sort-key).
        let mut entries: Vec<(String, f64, f64, f64)> = Vec::new();
        for cid in &pipe.component_ids {
            if let (Some(idx), Some(pos)) = (
                d.nodes.iter().position(|n| &n.id == cid),
                position_for(&positions, d, cid),
            ) {
                let sort_key = d.nodes[idx].x; // stored evolution percentage
                entries.push((cid.clone(), pos.0, pos.1, sort_key));
            }
        }
        if entries.is_empty() {
            continue;
        }

        let min_x = entries.iter().fold(f64::INFINITY, |acc, e| acc.min(e.1));
        let max_x = entries
            .iter()
            .fold(f64::NEG_INFINITY, |acc, e| acc.max(e.1));
        let y = entries[0].2;

        let padding_inner = 15.0;
        let box_h = NODE_RADIUS * 4.0; // "height of the pipeline box"
        let box_top = y - box_h / 2.0;
        let center_x = (min_x + max_x) / 2.0;

        // Mutate parent position.
        if let Some(parent_idx) = d.nodes.iter().position(|n| n.id == pipe.node_id) {
            if let Some(p) = positions.iter_mut().find(|lp| lp.src_index == parent_idx) {
                p.x = center_x;
                p.y = box_top - SQUARE_SIZE / 6.0;
            }
        }

        // Sort component ids by x ascending (for pipeline-link order).
        let mut sorted = entries.clone();
        sorted.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));
        let sorted_ids: Vec<String> = sorted.iter().map(|e| e.0.clone()).collect();

        pipelines_out.push(LaidOutPipeline {
            parent_id: pipe.node_id.clone(),
            box_x: min_x - padding_inner,
            box_y: box_top,
            box_w: max_x - min_x + padding_inner * 2.0,
            box_h,
            sorted_component_ids: sorted_ids,
        });
    }

    Ok(WardleyLayout {
        width,
        height,
        chart_width,
        chart_height,
        node_positions: positions,
        pipelines: pipelines_out,
    })
}

/// Fetch `(x, y)` for a node id as currently laid out.
fn position_for(positions: &[LaidOutNode], d: &WardleyDiagram, id: &str) -> Option<(f64, f64)> {
    let idx = d.nodes.iter().position(|n| n.id == id)?;
    positions
        .iter()
        .find(|p| p.src_index == idx)
        .map(|p| (p.x, p.y))
}

pub fn project_x(v: f64, chart_width: f64) -> f64 {
    PADDING + (v / 100.0) * chart_width
}

pub fn project_y(v: f64, height: f64, chart_height: f64) -> f64 {
    height - PADDING - (v / 100.0) * chart_height
}

/// Public helper for the renderer — resolves a node id to its laid-out
/// `(x, y)`.
pub fn get_position(
    layout: &WardleyLayout,
    diagram: &WardleyDiagram,
    id: &str,
) -> Option<(f64, f64)> {
    position_for(&layout.node_positions, diagram, id)
}

/// Public helper — resolve a node id to its underlying
/// [`WardleyNode`] reference.
pub fn get_node<'a>(diagram: &'a WardleyDiagram, id: &str) -> Option<&'a WardleyNode> {
    diagram.get_node(id)
}
