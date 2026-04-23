//! Gantt layout — compute task bar geometry, time axis, and viewBox.
//!
//! This is a layout stub that returns a minimal `GanttLayout` so the
//! pipeline doesn't panic. Full geometry computation (matching upstream
//! `ganttRenderer.js`) will be filled in a later wave.

use crate::error::Result;
use crate::model::gantt::GanttDiagram;
use crate::theme::ThemeVariables;

/// Full gantt layout ready for rendering.
#[derive(Debug, Clone, Default)]
pub struct GanttLayout {
    pub width: f64,
    pub height: f64,
    pub viewbox_x: f64,
    pub viewbox_y: f64,
    pub viewbox_w: f64,
    pub viewbox_h: f64,
    // TODO: add task bar geometry, time axis ticks, section positions, etc.
}

pub fn layout(_d: &GanttDiagram, _theme: &ThemeVariables) -> Result<GanttLayout> {
    // Stub: return a minimal layout that at least doesn't panic.
    // Full geometry computation will be implemented in a later wave.
    Ok(GanttLayout {
        width: 0.0,
        height: 0.0,
        viewbox_x: 0.0,
        viewbox_y: 0.0,
        viewbox_w: 0.0,
        viewbox_h: 0.0,
    })
}
