//! timeline diagram parsed model.
//!
//! Mirrors upstream `diagrams/timeline/timelineDb.js` — a timeline is a
//! list of **sections** plus a flat list of **tasks**, where each task
//! carries zero or more follow-up **events**. When no sections appear
//! the section name is the empty string and the single implicit
//! "(no-section)" bucket holds all tasks in declaration order.
//!
//! Only structure lives here — the renderer and layout modules handle
//! coordinates, colours, CSS.

use crate::model::DiagramMeta;

/// One task row inside a timeline — matches upstream's `TimelineTask`
/// shape (`{ section, task, events: string[] }`). Section is the raw
/// user-supplied label (no `"section "` prefix).
#[derive(Debug, Clone, Default)]
pub struct TimelineTask {
    pub section: String,
    pub task: String,
    pub events: Vec<String>,
}

/// Timeline direction: `LR` (horizontal — the default / `timeline TD`)
/// or `TD` (vertical). Upstream exposes two separate renderers keyed on
/// this flag; we dispatch inside [`crate::render::svg_timeline`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimelineDirection {
    /// Horizontal layout (upstream `timelineRenderer.ts`, the default).
    #[default]
    LR,
    /// Vertical layout (upstream `timelineRendererVertical.ts`).
    TD,
}

#[derive(Debug, Clone, Default)]
pub struct TimelineDiagram {
    pub meta: DiagramMeta,
    pub direction: TimelineDirection,
    /// Section names in the order they were declared. May be empty when
    /// the source omits `section` keywords entirely.
    pub sections: Vec<String>,
    /// Tasks in source order. `section` is the empty string when the
    /// task appeared before any `section` keyword.
    pub tasks: Vec<TimelineTask>,
    /// `%%{init:...}%%` → `timeline.disableMulticolor`. When true, all
    /// tasks share the same colour slot (first palette entry). Only
    /// meaningful when the diagram has no sections.
    pub disable_multicolor: bool,
    /// `%%{init:...}%%` → `timeline.leftMargin`. Upstream default is 50.
    pub left_margin: f64,
    /// `themeVariables` overrides captured from the init directive /
    /// frontmatter. Kept as raw strings so we can feed them to the
    /// theme-variable merge later.
    pub theme_overrides: ThemeOverrides,
    /// `theme` name from an init directive, when present. Falls back to
    /// the outer preprocessor's decision otherwise.
    pub theme_name: Option<String>,
    /// `fontFamily` / `fontSize` overrides from frontmatter or init.
    pub font_family: Option<String>,
    pub font_size: Option<String>,
}

/// Narrow set of theme-variable keys the timeline fixtures exercise.
/// Anything else from an init directive is ignored by this module.
#[derive(Debug, Clone, Default)]
pub struct ThemeOverrides {
    pub c_scale: [Option<String>; 12],
}
