//! XY chart (a.k.a. `xychart-beta`) parsed model.
//!
//! Port of upstream `diagrams/xychart/xychartDb.ts` and the interface
//! types in `chartBuilder/interfaces.ts`. The model mirrors upstream
//! structure field-for-field so the layout stage can translate the
//! orchestrator / axis / plot classes with minimal renaming.
//!
//! Shape summary:
//!   - [`XychartDiagram`] carries the merged config, theme overrides,
//!     and populated [`XychartData`] (axes + plots + title).
//!   - [`XAxisSpec`] / [`YAxisSpec`] are the parsed axis descriptors;
//!     both either hold a band domain (categories) or a linear one.
//!   - [`PlotSpec`] captures one `bar` / `line` directive with its
//!     data values and the plot-palette colour assigned at parse time.
//!
//! Any field whose upstream default depends on the schema is either
//! represented as `Option<T>` (unset → use default at layout time) or
//! carries the default baked in at construction.

use crate::model::DiagramMeta;

/// Top-level parsed model for an `xychart` / `xychart-beta` diagram.
#[derive(Debug, Clone, Default)]
pub struct XychartDiagram {
    pub meta: DiagramMeta,
    /// Per-diagram config (width / height / padding / axis toggles …),
    /// already merged with frontmatter + `%%{init:…}%%` overrides.
    pub config: XychartConfig,
    /// Per-diagram theme overrides merged on top of the chosen theme's
    /// built-in `xyChart` sub-struct. `None` keeps the theme default.
    pub theme_override: XychartThemeOverride,
    /// Parsed axes + plots + title (title is also mirrored on `meta`).
    pub data: XychartData,
    /// Theme name selected via `config.theme` (frontmatter) — e.g.
    /// `"default"`, `"dark"`, `"forest"`, `"neutral"`, `"base"`. The
    /// parser extracts this so the render harness can select the
    /// right `ThemeVariables` block without going through the global
    /// preprocess pipeline.
    pub theme_name: Option<String>,
}

/// Vertical vs. horizontal plot orientation (upstream `chartOrientation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartOrientation {
    #[default]
    Vertical,
    Horizontal,
}

/// Per-axis layout config. Mirrors upstream `XYChartAxisConfig` one-to-one.
#[derive(Debug, Clone, PartialEq)]
pub struct XyAxisConfig {
    pub show_label: bool,
    pub label_font_size: f64,
    pub label_padding: f64,
    pub show_title: bool,
    pub title_font_size: f64,
    pub title_padding: f64,
    pub show_tick: bool,
    pub tick_length: f64,
    pub tick_width: f64,
    pub show_axis_line: bool,
    pub axis_line_width: f64,
}

impl Default for XyAxisConfig {
    fn default() -> Self {
        // Defaults mirror `schemas/config.schema.yaml` → `XYChartAxisConfig`.
        Self {
            show_label: true,
            label_font_size: 14.0,
            label_padding: 5.0,
            show_title: true,
            title_font_size: 16.0,
            title_padding: 5.0,
            show_tick: true,
            tick_length: 5.0,
            tick_width: 2.0,
            show_axis_line: true,
            axis_line_width: 2.0,
        }
    }
}

/// Top-level xychart config. Mirrors upstream `XYChartConfig`.
#[derive(Debug, Clone, PartialEq)]
pub struct XychartConfig {
    pub width: f64,
    pub height: f64,
    pub title_font_size: f64,
    pub title_padding: f64,
    pub show_title: bool,
    pub show_data_label: bool,
    pub show_data_label_outside_bar: bool,
    pub x_axis: XyAxisConfig,
    pub y_axis: XyAxisConfig,
    pub chart_orientation: ChartOrientation,
    pub plot_reserved_space_percent: f64,
}

impl Default for XychartConfig {
    fn default() -> Self {
        // Defaults from upstream `defaultConfig.ts` + `config.schema.yaml`.
        Self {
            width: 700.0,
            height: 500.0,
            title_font_size: 20.0,
            title_padding: 10.0,
            show_title: true,
            show_data_label: false,
            show_data_label_outside_bar: false,
            x_axis: XyAxisConfig::default(),
            y_axis: XyAxisConfig::default(),
            chart_orientation: ChartOrientation::Vertical,
            plot_reserved_space_percent: 50.0,
        }
    }
}

/// Frontmatter / directive-derived theme overrides for xychart.
/// All fields are `Option<T>` so a partial override leaves other
/// colour slots on their theme-default value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XychartThemeOverride {
    pub background_color: Option<String>,
    pub title_color: Option<String>,
    pub data_label_color: Option<String>,
    pub x_axis_label_color: Option<String>,
    pub x_axis_line_color: Option<String>,
    pub x_axis_tick_color: Option<String>,
    pub x_axis_title_color: Option<String>,
    pub y_axis_label_color: Option<String>,
    pub y_axis_line_color: Option<String>,
    pub y_axis_tick_color: Option<String>,
    pub y_axis_title_color: Option<String>,
    pub plot_color_palette: Option<String>,
}

/// One axis descriptor — either a categorical band or a linear range.
#[derive(Debug, Clone, PartialEq)]
pub enum AxisSpec {
    Band {
        title: String,
        categories: Vec<String>,
    },
    Linear {
        title: String,
        min: f64,
        max: f64,
    },
}

impl AxisSpec {
    pub fn default_x() -> Self {
        AxisSpec::Band {
            title: String::new(),
            categories: Vec::new(),
        }
    }
    pub fn default_y() -> Self {
        // Upstream seeds y-axis as linear with `min: +∞, max: -∞` so the
        // plot-data min/max replaces both without clamping. We keep the
        // same semantic with a pair of sentinel floats.
        AxisSpec::Linear {
            title: String::new(),
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            AxisSpec::Band { title, .. } | AxisSpec::Linear { title, .. } => title,
        }
    }
    pub fn set_title(&mut self, s: String) {
        match self {
            AxisSpec::Band { title, .. } | AxisSpec::Linear { title, .. } => *title = s,
        }
    }
}

/// One plot directive (`bar` or `line`). The palette index is stored
/// here; the concrete colour is resolved at layout time from the
/// merged theme palette. This lets a theme change (default → dark)
/// pick a different colour without re-parsing.
#[derive(Debug, Clone, PartialEq)]
pub enum PlotSpec {
    Bar {
        plot_index: usize,
        /// `(category, value)` pairs — when the x-axis is linear,
        /// categories are the upstream-generated `i.to_string()` labels
        /// that act as keys for the band scale fallback.
        data: Vec<(String, f64)>,
    },
    Line {
        plot_index: usize,
        stroke_width: f64,
        data: Vec<(String, f64)>,
    },
}

/// Parsed axes + plots + title. Mirrors upstream `XYChartData`.
#[derive(Debug, Clone)]
pub struct XychartData {
    pub title: String,
    pub x_axis: AxisSpec,
    pub y_axis: AxisSpec,
    pub plots: Vec<PlotSpec>,
}

impl Default for XychartData {
    fn default() -> Self {
        Self {
            title: String::new(),
            x_axis: AxisSpec::default_x(),
            y_axis: AxisSpec::default_y(),
            plots: Vec::new(),
        }
    }
}
