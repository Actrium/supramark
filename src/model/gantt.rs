//! Gantt diagram parsed model.
//!
//! Upstream: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/gantt/
//! Jison grammar: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/gantt/parser/gantt.jison
//!
//! The parser preserves insertion order — upstream's `ganttDb.addTask` /
//! `addSection` push into arrays whose ordering the renderer depends on
//! for vertical sequencing.

use crate::model::DiagramMeta;

/// Parsed gantt chart.
#[derive(Debug, Clone, Default)]
pub struct GanttDiagram {
    pub meta: DiagramMeta,
    /// `dateFormat YYYY-MM-DD` — required for date parsing.
    pub date_format: String,
    /// `axisFormat %m/%d` — optional date format for the time axis.
    pub axis_format: Option<String>,
    /// `tickInterval` — optional custom tick interval for the time axis.
    pub tick_interval: Option<String>,
    /// `excludes weekends,friday,...` — days to skip when computing duration.
    pub excludes: Vec<String>,
    /// `includes 2024-12-25,...` — days to force-include even if excluded by weekends.
    pub includes: Vec<String>,
    /// `todayMarker` — marker line style (e.g. `off` to disable, or a CSS stroke spec).
    pub today_marker: Option<String>,
    /// `inclusiveEndDates` — when true, end dates are treated as inclusive.
    pub inclusive_end_dates: bool,
    /// `topAxis` — render the time axis above the chart.
    pub top_axis: bool,
    /// `displayMode` — `compact` or default (empty string).
    pub display_mode: Option<String>,
    /// `weekday` — which day starts the week (default: `sunday`).
    pub weekday: String,
    /// `weekend` — which day the weekend starts on (default: `saturday`).
    pub weekend: String,
    /// Theme name override from `%%{init:...}%%` directives.
    /// (Mirrors `timeline.theme_name` pattern.)
    pub theme_name: Option<String>,
    /// Sections in insertion order.
    pub sections: Vec<Section>,
    /// Tasks in insertion order.
    pub tasks: Vec<Task>,
}

/// A gantt section — groups tasks visually.
#[derive(Debug, Clone)]
pub struct Section {
    pub name: String,
    /// Indices into `GanttDiagram.tasks` that belong to this section.
    pub task_indices: Vec<usize>,
}

/// A single gantt task.
#[derive(Debug, Clone)]
pub struct Task {
    /// User-supplied task ID (e.g. `a1`). Auto-generated if absent.
    pub id: Option<String>,
    /// Display name / description text.
    pub name: String,
    /// Start date — either a date string, `after <id>`, or empty
    /// (inherits previous task's end time).
    pub start: Option<String>,
    /// End date or duration string — a date, duration like `30d`,
    /// or `until <id>`.
    pub end: Option<String>,
    /// Status flags parsed from the task data.
    pub done: bool,
    pub active: bool,
    pub critical: bool,
    pub milestone: bool,
    /// Vertical milestone marker (`vert` tag). Renders as a thin
    /// vertical bar spanning the full chart height.
    pub vert: bool,
    /// Section index into `GanttDiagram.sections`.
    pub section: usize,
    /// CSS classes added via `click` / `setClass`.
    pub classes: Vec<String>,
}
