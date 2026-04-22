//! Journey (user-journey) parsed model.
//!
//! Upstream reference: `packages/mermaid/src/diagrams/user-journey/journeyDb.js`
//! — the data structure is a flat list of tasks, each carrying its
//! section name, score, and list of people. Sections are inferred
//! from the `section` keyword that preceded a task; tasks before any
//! `section` get an empty string for their section name.

use crate::model::DiagramMeta;

/// One task row — mirrors upstream's `rawTask` object in `journeyDb.js`.
#[derive(Debug, Clone, Default)]
pub struct JourneyTask {
    /// Section this task belongs to (`""` if the task preceded any
    /// `section` keyword — upstream tolerates that).
    pub section: String,
    /// Task description (the `taskName` token before the first `:`).
    pub task: String,
    /// Score value — `NaN` in upstream when the `:score` part failed
    /// to parse as a number. We store it as `Option<f64>` where
    /// `None == NaN`.
    pub score: Option<f64>,
    /// Actors after the second `:`. Empty if absent. Preserves the
    /// `[""]` case (single empty string) that upstream produces when
    /// the line is `Task: 5:` with a trailing colon.
    pub people: Vec<String>,
}

/// Per-diagram config pulled out of `%%{init}%%` / frontmatter — only
/// the keys we actually consume. Upstream's defaults live in the
/// `JourneyDiagramConfig` schema.
#[derive(Debug, Clone)]
pub struct JourneyConfig {
    pub max_label_width: f64,
    pub left_margin: f64,
    pub width: f64,
    pub height: f64,
    pub diagram_margin_x: f64,
    pub diagram_margin_y: f64,
    pub task_margin: f64,
    pub box_text_margin: f64,
    pub title_color: String,
    pub title_font_family: String,
    pub title_font_size: String,
    pub actor_colours: Vec<String>,
    pub section_fills: Vec<String>,
    pub section_colours: Vec<String>,
    pub text_placement: String,
    pub task_font_size: i64,
    pub task_font_family: String,
}

impl Default for JourneyConfig {
    fn default() -> Self {
        JourneyConfig {
            max_label_width: 360.0,
            left_margin: 150.0,
            width: 150.0,
            height: 50.0,
            diagram_margin_x: 50.0,
            diagram_margin_y: 10.0,
            task_margin: 50.0,
            box_text_margin: 5.0,
            title_color: String::new(),
            title_font_family: "\"trebuchet ms\", verdana, arial, sans-serif".to_string(),
            title_font_size: "4ex".to_string(),
            actor_colours: vec![
                "#8FBC8F".into(),
                "#7CFC00".into(),
                "#00FFFF".into(),
                "#20B2AA".into(),
                "#B0E0E6".into(),
                "#FFFFE0".into(),
            ],
            section_fills: vec![
                "#191970".into(),
                "#8B008B".into(),
                "#4B0082".into(),
                "#2F4F4F".into(),
                "#800000".into(),
                "#8B4513".into(),
                "#00008B".into(),
            ],
            section_colours: vec!["#fff".into()],
            text_placement: "fo".to_string(),
            task_font_size: 14,
            task_font_family: "\"Open Sans\", sans-serif".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct JourneyDiagram {
    pub meta: DiagramMeta,
    pub title: Option<String>,
    pub tasks: Vec<JourneyTask>,
    pub config: JourneyConfig,
}
