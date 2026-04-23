//! Gantt diagram SVG renderer — stub.
//!
//! Returns `Err(Unsupported)` until the full gantt renderer is
//! implemented in a later wave.

use crate::error::{MermaidError, Result};
use crate::layout::gantt::GanttLayout;
use crate::model::gantt::GanttDiagram;
use crate::theme::ThemeVariables;

pub fn render(
    _d: &GanttDiagram,
    _l: &GanttLayout,
    _theme: &ThemeVariables,
    _id: &str,
) -> Result<String> {
    Err(MermaidError::Unsupported(
        "gantt renderer not yet implemented".into(),
    ))
}
