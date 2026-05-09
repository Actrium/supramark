//! XY chart layout — port of upstream `chartBuilder/*`.
//!
//! The layout produces a flat list of [`DrawableElem`] in the exact
//! order the upstream renderer would hand them to d3. Each element
//! carries an upstream-matching `groupTexts` path so the svg renderer
//! can emit nested `<g>` wrappers byte-identically.
//!
//! Upstream reference files (translated here):
//!   - `orchestrator.ts`                         — space allocation.
//!   - `components/chartTitle.ts`                — title block.
//!   - `components/axis/baseAxis.ts`             — axis line + ticks +
//!                                                 labels + title.
//!   - `components/axis/bandAxis.ts`             — band (categorical) scale.
//!   - `components/axis/linearAxis.ts`           — linear (numeric) scale.
//!   - `components/plot/barPlot.ts`              — bar rects.
//!   - `components/plot/linePlot.ts`             — line-plot path.
//!
//! Text dimensions come from the `font_metrics` crate (DejaVu Sans,
//! jsdom CSS defaults → `sans-serif 14px`). Numeric ticks come from a
//! verbatim port of d3-array's `ticks()` algorithm — the only way to
//! pick identical tick values to upstream.

use crate::error::Result;
use crate::font_metrics::{line_height, text_width};
use crate::model::xychart::{
    AxisSpec, ChartOrientation, PlotSpec, XyAxisConfig, XychartConfig, XychartDiagram,
};
use crate::theme::ThemeVariables;

// ── Drawable element types (mirror upstream `DrawableElem`) ──────────

#[derive(Debug, Clone)]
pub struct DrawableRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub fill: String,
    pub stroke_fill: String,
    pub stroke_width: f64,
}

#[derive(Debug, Clone)]
pub struct DrawableText {
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub fill: String,
    pub font_size: f64,
    pub rotation: f64,
    /// `top` or `middle` (upstream `TextVerticalPos`).
    pub vertical_pos: TextVerticalPos,
    /// `left` / `center` / `right`.
    pub horizontal_pos: TextHorizontalPos,
}

#[derive(Debug, Clone)]
pub struct DrawablePath {
    pub path: String,
    pub fill: Option<String>, // `None` serialises as `fill="none"`.
    pub stroke_fill: String,
    pub stroke_width: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextVerticalPos {
    Top,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextHorizontalPos {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub enum DrawableElem {
    Rect {
        group_texts: Vec<&'static str>,
        data: Vec<DrawableRect>,
    },
    Text {
        group_texts: Vec<&'static str>,
        data: Vec<DrawableText>,
    },
    Path {
        group_texts: Vec<&'static str>,
        data: Vec<DrawablePath>,
    },
    /// Bar plot with captured label values — renderer uses these to
    /// emit data-label `<text>` elements when `showDataLabel` is on.
    RectWithLabels {
        group_texts: Vec<&'static str>,
        data: Vec<DrawableRect>,
        labels: Vec<String>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct XychartLayout {
    pub width: f64,
    pub height: f64,
    pub background_color: String,
    pub elements: Vec<DrawableElem>,
}

/// Layout entry point.
pub fn layout(d: &XychartDiagram, theme: &ThemeVariables) -> Result<XychartLayout> {
    // Resolve theme colours with default fallbacks.
    let theme_xy = theme.xy_chart.clone().unwrap_or_default();
    let bg = d
        .theme_override
        .background_color
        .clone()
        .or(theme_xy.background_color.clone())
        .unwrap_or_else(|| "white".to_string());
    let title_color = d
        .theme_override
        .title_color
        .clone()
        .or(theme_xy.title_color.clone())
        .unwrap_or_else(|| "#000".to_string());
    let data_label_color = d
        .theme_override
        .data_label_color
        .clone()
        .or(theme_xy.data_label_color.clone())
        .unwrap_or_else(|| "#000".to_string());
    let x_axis_theme = AxisTheme {
        title_color: d
            .theme_override
            .x_axis_title_color
            .clone()
            .or(theme_xy.x_axis_title_color.clone())
            .unwrap_or_else(|| "#000".to_string()),
        label_color: d
            .theme_override
            .x_axis_label_color
            .clone()
            .or(theme_xy.x_axis_label_color.clone())
            .unwrap_or_else(|| "#000".to_string()),
        tick_color: d
            .theme_override
            .x_axis_tick_color
            .clone()
            .or(theme_xy.x_axis_tick_color.clone())
            .unwrap_or_else(|| "#000".to_string()),
        axis_line_color: d
            .theme_override
            .x_axis_line_color
            .clone()
            .or(theme_xy.x_axis_line_color.clone())
            .unwrap_or_else(|| "#000".to_string()),
    };
    let y_axis_theme = AxisTheme {
        title_color: d
            .theme_override
            .y_axis_title_color
            .clone()
            .or(theme_xy.y_axis_title_color.clone())
            .unwrap_or_else(|| "#000".to_string()),
        label_color: d
            .theme_override
            .y_axis_label_color
            .clone()
            .or(theme_xy.y_axis_label_color.clone())
            .unwrap_or_else(|| "#000".to_string()),
        tick_color: d
            .theme_override
            .y_axis_tick_color
            .clone()
            .or(theme_xy.y_axis_tick_color.clone())
            .unwrap_or_else(|| "#000".to_string()),
        axis_line_color: d
            .theme_override
            .y_axis_line_color
            .clone()
            .or(theme_xy.y_axis_line_color.clone())
            .unwrap_or_else(|| "#000".to_string()),
    };

    let cfg = &d.config;

    // Build axis objects.
    let mut x_axis = Axis::new(&d.data.x_axis, cfg.x_axis.clone(), x_axis_theme.clone());
    let mut y_axis = Axis::new(&d.data.y_axis, cfg.y_axis.clone(), y_axis_theme.clone());
    let has_bar = d
        .data
        .plots
        .iter()
        .any(|p| matches!(p, PlotSpec::Bar { .. }));

    let mut orch = Orchestrator {
        cfg,
        x_axis: &mut x_axis,
        y_axis: &mut y_axis,
        title_text: &d.data.title,
        has_bar,
        show_chart_title: false,
        title_bb: BoundingRect::default(),
        plot_bb: BoundingRect::default(),
    };
    orch.calculate_space();
    let plot_bb = orch.plot_bb.clone();
    let show_title = orch.show_chart_title;
    let title_bb = orch.title_bb.clone();

    // Build drawable elements in upstream order: title → plot → xAxis → yAxis.
    let mut elements: Vec<DrawableElem> = Vec::new();

    // Title.
    if show_title {
        elements.push(DrawableElem::Text {
            group_texts: vec!["chart-title"],
            data: vec![DrawableText {
                x: title_bb.x + title_bb.width / 2.0,
                y: title_bb.y + title_bb.height / 2.0,
                text: d.data.title.clone(),
                fill: title_color.clone(),
                font_size: cfg.title_font_size,
                rotation: 0.0,
                vertical_pos: TextVerticalPos::Middle,
                horizontal_pos: TextHorizontalPos::Center,
            }],
        });
    }

    // Resolve plot-colour palette: frontmatter override wins, else the
    // theme's default palette, else the built-in default.
    let palette_src = d
        .theme_override
        .plot_color_palette
        .clone()
        .or(theme_xy.plot_color_palette.clone())
        .unwrap_or_else(|| {
            "#ECECFF,#8493A6,#FFC3A0,#DCDDE1,#B8E994,#D1A36F,#C3CDE6,#FFB6C1,#496078,#F8F3E3"
                .to_string()
        });
    let palette: Vec<String> = palette_src
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let palette_color = |idx: usize| -> String {
        palette
            .get(if idx == 0 {
                0
            } else {
                idx % palette.len().max(1)
            })
            .cloned()
            .unwrap_or_default()
    };

    // Plot elements.
    for (i, plot) in d.data.plots.iter().enumerate() {
        match plot {
            PlotSpec::Bar { plot_index, data } => {
                let fill = palette_color(*plot_index);
                let (rects, labels) = bar_plot_rects(data, &x_axis, &y_axis, &plot_bb, cfg, &fill);
                if cfg.show_data_label {
                    elements.push(DrawableElem::RectWithLabels {
                        group_texts: leak_group(&["plot", &format!("bar-plot-{i}")]),
                        data: rects,
                        labels,
                    });
                } else {
                    elements.push(DrawableElem::Rect {
                        group_texts: leak_group(&["plot", &format!("bar-plot-{i}")]),
                        data: rects,
                    });
                }
            }
            PlotSpec::Line {
                plot_index,
                stroke_width,
                data,
            } => {
                let stroke_fill = palette_color(*plot_index);
                let path = line_plot_path(data, &x_axis, &y_axis, cfg);
                if let Some(path) = path {
                    elements.push(DrawableElem::Path {
                        group_texts: leak_group(&["plot", &format!("line-plot-{i}")]),
                        data: vec![DrawablePath {
                            path,
                            fill: None,
                            stroke_fill,
                            stroke_width: *stroke_width,
                        }],
                    });
                }
            }
        }
    }

    // Axis elements. Order: the axis that sits on the plot x-direction
    // (bottom or left depending on orientation) goes first.
    let x_elems = axis_drawable_elements(
        &x_axis,
        &data_label_color,
        Side::XAxisSide(cfg.chart_orientation),
    );
    let y_elems = axis_drawable_elements(
        &y_axis,
        &data_label_color,
        Side::YAxisSide(cfg.chart_orientation),
    );
    elements.extend(x_elems);
    elements.extend(y_elems);
    let _ = data_label_color; // reserved for showDataLabel path

    Ok(XychartLayout {
        width: cfg.width,
        height: cfg.height,
        background_color: bg,
        elements,
    })
}

// ── Group-text leaking ───────────────────────────────────────────────

/// Leak a group of strings to `'static` for the `DrawableElem::group_texts`
/// field. We only call this for per-plot labels where the index is small.
fn leak_group(names: &[&str]) -> Vec<&'static str> {
    names
        .iter()
        .map(|s| {
            let b: &'static str = Box::leak(s.to_string().into_boxed_str());
            b
        })
        .collect()
}

// ── Orchestrator ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct BoundingRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

struct Orchestrator<'a> {
    cfg: &'a XychartConfig,
    x_axis: &'a mut Axis,
    y_axis: &'a mut Axis,
    title_text: &'a str,
    has_bar: bool,
    show_chart_title: bool,
    title_bb: BoundingRect,
    plot_bb: BoundingRect,
}

impl<'a> Orchestrator<'a> {
    fn calculate_space(&mut self) {
        match self.cfg.chart_orientation {
            ChartOrientation::Vertical => self.calculate_vertical(),
            ChartOrientation::Horizontal => self.calculate_horizontal(),
        }
    }

    fn calculate_vertical(&mut self) {
        let mut available_width = self.cfg.width;
        let mut available_height = self.cfg.height;
        let plot_x;
        let mut plot_y = 0.0;
        let mut chart_width =
            (self.cfg.width * self.cfg.plot_reserved_space_percent / 100.0).floor();
        let mut chart_height =
            (self.cfg.height * self.cfg.plot_reserved_space_percent / 100.0).floor();

        // Plot initial.
        let used = (chart_width, chart_height);
        available_width -= used.0;
        available_height -= used.1;

        // Title.
        let title_height = title_space(self.cfg, self.title_text);
        if title_height > 0.0 && title_height <= available_height {
            self.show_chart_title = true;
            self.title_bb.width = title_box_width(self.cfg, self.title_text);
            self.title_bb.height = title_height;
            plot_y = title_height;
            available_height -= title_height;
        }

        // xAxis bottom.
        self.x_axis.set_axis_position(AxisPosition::Bottom);
        let used = self
            .x_axis
            .calculate_space(available_width, available_height);
        available_height -= used.1;

        // yAxis left.
        self.y_axis.set_axis_position(AxisPosition::Left);
        let used = self
            .y_axis
            .calculate_space(available_width, available_height);
        plot_x = used.0;
        available_width -= used.0;

        if available_width > 0.0 {
            chart_width += available_width;
            available_width = 0.0;
        }
        if available_height > 0.0 {
            chart_height += available_height;
            available_height = 0.0;
        }
        let _ = available_width;
        let _ = available_height;

        self.plot_bb = BoundingRect {
            x: plot_x,
            y: plot_y,
            width: chart_width,
            height: chart_height,
        };
        if self.show_chart_title {
            self.title_bb.x = 0.0;
            self.title_bb.y = 0.0;
        }
        self.x_axis.set_range((plot_x, plot_x + chart_width));
        self.x_axis.set_bounding_box(
            plot_x,
            plot_y + chart_height,
            chart_width,
            self.x_axis.bb.height,
        );
        self.y_axis.set_range((plot_y, plot_y + chart_height));
        self.y_axis
            .set_bounding_box(0.0, plot_y, self.y_axis.bb.width, chart_height);
        if self.has_bar {
            self.x_axis.recalculate_outer_padding_for_bar();
        }
    }

    fn calculate_horizontal(&mut self) {
        let mut available_width = self.cfg.width;
        let mut available_height = self.cfg.height;
        let mut title_y_end = 0.0;
        let plot_x;
        let plot_y;
        let mut chart_width =
            (self.cfg.width * self.cfg.plot_reserved_space_percent / 100.0).floor();
        let mut chart_height =
            (self.cfg.height * self.cfg.plot_reserved_space_percent / 100.0).floor();

        available_width -= chart_width;
        available_height -= chart_height;

        // Title.
        let title_height = title_space(self.cfg, self.title_text);
        if title_height > 0.0 && title_height <= available_height {
            self.show_chart_title = true;
            self.title_bb.width = title_box_width(self.cfg, self.title_text);
            self.title_bb.height = title_height;
            title_y_end = title_height;
            available_height -= title_height;
        }

        // xAxis left.
        self.x_axis.set_axis_position(AxisPosition::Left);
        let used = self
            .x_axis
            .calculate_space(available_width, available_height);
        available_width -= used.0;
        plot_x = used.0;

        // yAxis top.
        self.y_axis.set_axis_position(AxisPosition::Top);
        let used = self
            .y_axis
            .calculate_space(available_width, available_height);
        available_height -= used.1;
        plot_y = title_y_end + used.1;

        if available_width > 0.0 {
            chart_width += available_width;
            available_width = 0.0;
        }
        if available_height > 0.0 {
            chart_height += available_height;
            available_height = 0.0;
        }
        let _ = available_width;
        let _ = available_height;

        self.plot_bb = BoundingRect {
            x: plot_x,
            y: plot_y,
            width: chart_width,
            height: chart_height,
        };
        if self.show_chart_title {
            self.title_bb.x = 0.0;
            self.title_bb.y = 0.0;
        }
        // yAxis (top) range runs horizontally on x; set plotX..plotX+chartWidth.
        self.y_axis.set_range((plot_x, plot_x + chart_width));
        self.y_axis
            .set_bounding_box(plot_x, title_y_end, chart_width, self.y_axis.bb.height);
        // xAxis (left) range runs vertically on y; set plotY..plotY+chartHeight.
        self.x_axis.set_range((plot_y, plot_y + chart_height));
        self.x_axis
            .set_bounding_box(0.0, plot_y, self.x_axis.bb.width, chart_height);
        if self.has_bar {
            self.x_axis.recalculate_outer_padding_for_bar();
        }
    }
}

/// Title space = font line height + 2*titlePadding. Returns 0 when
/// the title is absent or `showTitle` is false.
fn title_space(cfg: &XychartConfig, title: &str) -> f64 {
    if !cfg.show_title || title.is_empty() {
        return 0.0;
    }
    let h = line_height("sans-serif", cfg.title_font_size, false, false);
    h + 2.0 * cfg.title_padding
}

/// Compute the title bounding-box width: `max(titleTextWidth, availableWidth)`.
/// Upstream uses the available width when the text fits, otherwise the
/// longer text width — which in turn shifts the centre x position.
fn title_box_width(cfg: &XychartConfig, title: &str) -> f64 {
    if !cfg.show_title || title.is_empty() {
        return cfg.width;
    }
    let tw = text_width(title, "sans-serif", cfg.title_font_size, false, false);
    tw.max(cfg.width)
}

// ── Axis ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct AxisTheme {
    title_color: String,
    label_color: String,
    tick_color: String,
    axis_line_color: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AxisPosition {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone)]
enum AxisKind {
    Band { categories: Vec<String> },
    Linear { domain: (f64, f64) },
}

#[derive(Debug, Clone)]
struct Axis {
    kind: AxisKind,
    cfg: XyAxisConfig,
    theme: AxisTheme,
    title: String,
    range: (f64, f64),
    bb: BoundingRect,
    axis_position: AxisPosition,
    show_title: bool,
    show_label: bool,
    show_tick: bool,
    show_axis_line: bool,
    outer_padding: f64,
    title_text_height: f64,
    label_text_height: f64,
    /// Scale state for band axis — paddingInner=1, paddingOuter=0, align=0.5.
    /// For linear axis, we derive scale on-the-fly.
    /// Stored after `recalculate_scale` for consistent getScaleValue.
    band_step: f64,
    band_start: f64,
}

const BAR_WIDTH_TO_TICK_WIDTH_RATIO: f64 = 0.7;
const MAX_OUTER_PADDING_PCT: f64 = 0.2;

impl Axis {
    fn new(spec: &AxisSpec, cfg: XyAxisConfig, theme: AxisTheme) -> Self {
        let (kind, title) = match spec {
            AxisSpec::Band { title, categories } => (
                AxisKind::Band {
                    categories: categories.clone(),
                },
                title.clone(),
            ),
            AxisSpec::Linear { title, min, max } => (
                AxisKind::Linear {
                    domain: (*min, *max),
                },
                title.clone(),
            ),
        };
        Self {
            kind,
            cfg,
            theme,
            title,
            range: (0.0, 10.0),
            bb: BoundingRect::default(),
            axis_position: AxisPosition::Left,
            show_title: false,
            show_label: false,
            show_tick: false,
            show_axis_line: false,
            outer_padding: 0.0,
            title_text_height: 0.0,
            label_text_height: 0.0,
            band_step: 0.0,
            band_start: 0.0,
        }
    }

    fn set_axis_position(&mut self, p: AxisPosition) {
        self.axis_position = p;
        self.set_range(self.range);
    }

    fn is_vertical(&self) -> bool {
        matches!(self.axis_position, AxisPosition::Left | AxisPosition::Right)
    }

    fn set_range(&mut self, r: (f64, f64)) {
        self.range = r;
        if self.is_vertical() {
            self.bb.height = r.1 - r.0;
        } else {
            self.bb.width = r.1 - r.0;
        }
        self.recalculate_scale();
    }

    fn set_bounding_box(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.bb.x = x;
        self.bb.y = y;
        self.bb.width = w;
        self.bb.height = h;
    }

    /// `getRange()` from upstream — shrunk by outer padding on both ends.
    fn effective_range(&self) -> (f64, f64) {
        (
            self.range.0 + self.outer_padding,
            self.range.1 - self.outer_padding,
        )
    }

    fn tick_values(&self) -> Vec<String> {
        match &self.kind {
            AxisKind::Band { categories } => categories.clone(),
            AxisKind::Linear { domain } => {
                // Upstream `LinearAxis.recalculateScale` copies the
                // domain and reverses it for the left axis; subsequent
                // `.ticks()` returns them in the reversed order.
                let (a, b) = if self.axis_position == AxisPosition::Left {
                    (domain.1, domain.0)
                } else {
                    (domain.0, domain.1)
                };
                let ticks = d3_ticks(a, b, 10);
                ticks.iter().map(|v| js_num_to_string(*v)).collect()
            }
        }
    }

    fn tick_distance(&self) -> f64 {
        let r = self.effective_range();
        let len = self.tick_values().len();
        if len == 0 {
            return 0.0;
        }
        (r.0 - r.1).abs() / len as f64
    }

    fn recalculate_outer_padding_for_bar(&mut self) {
        if BAR_WIDTH_TO_TICK_WIDTH_RATIO * self.tick_distance() > self.outer_padding * 2.0 {
            self.outer_padding =
                (BAR_WIDTH_TO_TICK_WIDTH_RATIO * self.tick_distance() / 2.0).floor();
        }
        self.recalculate_scale();
    }

    fn recalculate_scale(&mut self) {
        match &self.kind {
            AxisKind::Band { categories } => {
                let r = self.effective_range();
                let n = categories.len() as f64;
                let padding_inner = 1.0;
                let padding_outer = 0.0;
                let align = 0.5;
                let step = if n.max(1.0) - padding_inner + padding_outer * 2.0 > 0.0 {
                    (r.1 - r.0) / (n - padding_inner + padding_outer * 2.0).max(1.0)
                } else {
                    r.1 - r.0
                };
                let start = r.0 + (r.1 - r.0 - step * (n - padding_inner)) * align;
                self.band_step = step;
                self.band_start = start;
            }
            AxisKind::Linear { .. } => {
                // No pre-computation needed; scale value computed on demand.
            }
        }
    }

    fn scale_band(&self, category: &str) -> Option<f64> {
        if let AxisKind::Band { categories } = &self.kind {
            let idx = categories.iter().position(|c| c == category)?;
            Some(self.band_start + self.band_step * idx as f64)
        } else {
            None
        }
    }

    /// `scale(value)` for linear; `scale(category)` for band.
    ///
    /// Reproduces d3-scale `continuous.scale(x)` exactly:
    ///   * `normalize(a, b)(x) = (x - a) / (b - a)` where `a = d0` and
    ///     `b - a` is pre-computed.
    ///   * `interpolateNumber(a, b)(t) = a * (1 - t) + b * t`.
    ///   * For descending domains (`d1 < d0`), d3's `bimap` swaps to
    ///     `normalize(d1, d0)` and `interpolate(r1, r0)`.
    /// Matching the exact operation order is necessary for byte-exact
    /// parity — the naive `a + (b - a) * t` shape differs by 1 ULP.
    fn scale_numeric(&self, value: f64) -> f64 {
        if let AxisKind::Linear { domain } = &self.kind {
            let r = self.effective_range();
            // LinearAxis.recalculateScale reverses the domain when on
            // the left axis. Starting from that reversed domain, bimap
            // may flip again if d1 < d0. So the end effect for the
            // left-axis case is: normalize(d_min, d_max), interpolate(r_max, r_min).
            let (da, db) = if self.axis_position == AxisPosition::Left {
                (domain.1, domain.0)
            } else {
                (domain.0, domain.1)
            };
            // Apply bimap's descending-domain handling.
            let (dn_a, dn_b, in_a, in_b) = if db < da {
                (db, da, r.1, r.0)
            } else {
                (da, db, r.0, r.1)
            };
            if dn_b == dn_a {
                return (r.0 + r.1) / 2.0;
            }
            let t = (value - dn_a) / (dn_b - dn_a);
            in_a * (1.0 - t) + in_b * t
        } else {
            0.0
        }
    }

    /// Compute space usage — returns (width, height) claimed.
    fn calculate_space(&mut self, avail_w: f64, avail_h: f64) -> (f64, f64) {
        if self.is_vertical() {
            self.calc_space_vertical(avail_w, avail_h);
        } else {
            self.calc_space_horizontal(avail_w, avail_h);
        }
        self.recalculate_scale();
        (self.bb.width, self.bb.height)
    }

    fn calc_space_horizontal(&mut self, avail_w: f64, avail_h: f64) {
        let mut h = avail_h;
        self.show_axis_line = false;
        self.show_label = false;
        self.show_tick = false;
        self.show_title = false;

        if self.cfg.show_axis_line && h > self.cfg.axis_line_width {
            h -= self.cfg.axis_line_width;
            self.show_axis_line = true;
        }
        if self.cfg.show_label {
            let (lw, lh) = max_text_dim(&self.tick_values(), self.cfg.label_font_size);
            let max_pad = MAX_OUTER_PADDING_PCT * avail_w;
            self.outer_padding = (lw / 2.0).min(max_pad);
            let height_required = lh + self.cfg.label_padding * 2.0;
            self.label_text_height = lh;
            if height_required <= h {
                h -= height_required;
                self.show_label = true;
            }
        }
        if self.cfg.show_tick && h >= self.cfg.tick_length {
            self.show_tick = true;
            h -= self.cfg.tick_length;
        }
        if self.cfg.show_title && !self.title.is_empty() {
            let (_tw, th) = max_text_dim(&[self.title.clone()], self.cfg.title_font_size);
            let height_required = th + self.cfg.title_padding * 2.0;
            self.title_text_height = th;
            if height_required <= h {
                h -= height_required;
                self.show_title = true;
            }
        }
        self.bb.width = avail_w;
        self.bb.height = avail_h - h;
    }

    fn calc_space_vertical(&mut self, avail_w: f64, avail_h: f64) {
        let mut w = avail_w;
        self.show_axis_line = false;
        self.show_label = false;
        self.show_tick = false;
        self.show_title = false;

        if self.cfg.show_axis_line && w > self.cfg.axis_line_width {
            w -= self.cfg.axis_line_width;
            self.show_axis_line = true;
        }
        if self.cfg.show_label {
            let (lw, lh) = max_text_dim(&self.tick_values(), self.cfg.label_font_size);
            let max_pad = MAX_OUTER_PADDING_PCT * avail_h;
            self.outer_padding = (lh / 2.0).min(max_pad);
            let width_required = lw + self.cfg.label_padding * 2.0;
            if width_required <= w {
                w -= width_required;
                self.show_label = true;
            }
        }
        if self.cfg.show_tick && w >= self.cfg.tick_length {
            self.show_tick = true;
            w -= self.cfg.tick_length;
        }
        if self.cfg.show_title && !self.title.is_empty() {
            let (_tw, th) = max_text_dim(&[self.title.clone()], self.cfg.title_font_size);
            let width_required = th + self.cfg.title_padding * 2.0;
            self.title_text_height = th;
            if width_required <= w {
                w -= width_required;
                self.show_title = true;
            }
        }
        self.bb.width = avail_w - w;
        self.bb.height = avail_h;
    }
}

/// Max (width, height) across a set of strings at `size` px sans-serif.
fn max_text_dim(texts: &[String], size: f64) -> (f64, f64) {
    let mut w = 0.0f64;
    let h = line_height("sans-serif", size, false, false);
    for t in texts {
        let tw = text_width(t, "sans-serif", size, false, false);
        if tw > w {
            w = tw;
        }
    }
    (w, h)
}

// ── Axis drawable output ─────────────────────────────────────────────

enum Side {
    XAxisSide(ChartOrientation),
    YAxisSide(ChartOrientation),
}

fn axis_drawable_elements(axis: &Axis, _data_label_color: &str, side: Side) -> Vec<DrawableElem> {
    let mut out = Vec::new();
    match side {
        Side::XAxisSide(ChartOrientation::Vertical) => {
            bottom_axis_elements(axis, &mut out, "bottom-axis", "axis-line");
        }
        Side::XAxisSide(ChartOrientation::Horizontal) => {
            left_axis_elements(axis, &mut out, "left-axis", "axisl-line");
        }
        Side::YAxisSide(ChartOrientation::Vertical) => {
            left_axis_elements(axis, &mut out, "left-axis", "axisl-line");
        }
        Side::YAxisSide(ChartOrientation::Horizontal) => {
            top_axis_elements(axis, &mut out, "top-axis", "axis-line");
        }
    }
    out
}

fn bottom_axis_elements(
    axis: &Axis,
    out: &mut Vec<DrawableElem>,
    parent_class: &'static str,
    line_class: &'static str,
) {
    if axis.show_axis_line {
        let y = axis.bb.y + axis.cfg.axis_line_width / 2.0;
        out.push(DrawableElem::Path {
            group_texts: vec![parent_class, line_class],
            data: vec![DrawablePath {
                path: format!(
                    "M {},{} L {},{}",
                    fmt_num(axis.bb.x),
                    fmt_num(y),
                    fmt_num(axis.bb.x + axis.bb.width),
                    fmt_num(y),
                ),
                fill: None,
                stroke_fill: axis.theme.axis_line_color.clone(),
                stroke_width: axis.cfg.axis_line_width,
            }],
        });
    }
    if axis.show_label {
        let y = axis.bb.y
            + axis.cfg.label_padding
            + if axis.show_tick {
                axis.cfg.tick_length
            } else {
                0.0
            }
            + if axis.show_axis_line {
                axis.cfg.axis_line_width
            } else {
                0.0
            };
        let data: Vec<DrawableText> = axis
            .tick_values()
            .iter()
            .map(|t| DrawableText {
                x: scale_tick(axis, t),
                y,
                text: t.clone(),
                fill: axis.theme.label_color.clone(),
                font_size: axis.cfg.label_font_size,
                rotation: 0.0,
                vertical_pos: TextVerticalPos::Top,
                horizontal_pos: TextHorizontalPos::Center,
            })
            .collect();
        out.push(DrawableElem::Text {
            group_texts: vec![parent_class, "label"],
            data,
        });
    }
    if axis.show_tick {
        let y_top = axis.bb.y
            + if axis.show_axis_line {
                axis.cfg.axis_line_width
            } else {
                0.0
            };
        let data: Vec<DrawablePath> = axis
            .tick_values()
            .iter()
            .map(|t| {
                let x = scale_tick(axis, t);
                DrawablePath {
                    path: format!(
                        "M {},{} L {},{}",
                        fmt_num(x),
                        fmt_num(y_top),
                        fmt_num(x),
                        fmt_num(y_top + axis.cfg.tick_length),
                    ),
                    fill: None,
                    stroke_fill: axis.theme.tick_color.clone(),
                    stroke_width: axis.cfg.tick_width,
                }
            })
            .collect();
        out.push(DrawableElem::Path {
            group_texts: vec![parent_class, "ticks"],
            data,
        });
    }
    if axis.show_title {
        let x = axis.range.0 + (axis.range.1 - axis.range.0) / 2.0;
        let y = axis.bb.y + axis.bb.height - axis.cfg.title_padding - axis.title_text_height;
        out.push(DrawableElem::Text {
            group_texts: vec![parent_class, "title"],
            data: vec![DrawableText {
                x,
                y,
                text: axis.title.clone(),
                fill: axis.theme.title_color.clone(),
                font_size: axis.cfg.title_font_size,
                rotation: 0.0,
                vertical_pos: TextVerticalPos::Top,
                horizontal_pos: TextHorizontalPos::Center,
            }],
        });
    }
}

fn left_axis_elements(
    axis: &Axis,
    out: &mut Vec<DrawableElem>,
    parent_class: &'static str,
    line_class: &'static str,
) {
    if axis.show_axis_line {
        let x = axis.bb.x + axis.bb.width - axis.cfg.axis_line_width / 2.0;
        // Upstream emits a trailing space and differing inner spaces for
        // this path: `"M {x},{y} L {x},{y2} "`. Replicate verbatim.
        out.push(DrawableElem::Path {
            group_texts: vec![parent_class, line_class],
            data: vec![DrawablePath {
                path: format!(
                    "M {},{} L {},{} ",
                    fmt_num(x),
                    fmt_num(axis.bb.y),
                    fmt_num(x),
                    fmt_num(axis.bb.y + axis.bb.height),
                ),
                fill: None,
                stroke_fill: axis.theme.axis_line_color.clone(),
                stroke_width: axis.cfg.axis_line_width,
            }],
        });
    }
    if axis.show_label {
        let x = axis.bb.x + axis.bb.width
            - (if axis.show_label {
                axis.cfg.label_padding
            } else {
                0.0
            })
            - (if axis.show_tick {
                axis.cfg.tick_length
            } else {
                0.0
            })
            - (if axis.show_axis_line {
                axis.cfg.axis_line_width
            } else {
                0.0
            });
        let data: Vec<DrawableText> = axis
            .tick_values()
            .iter()
            .map(|t| DrawableText {
                x,
                y: scale_tick(axis, t),
                text: t.clone(),
                fill: axis.theme.label_color.clone(),
                font_size: axis.cfg.label_font_size,
                rotation: 0.0,
                vertical_pos: TextVerticalPos::Middle,
                horizontal_pos: TextHorizontalPos::Right,
            })
            .collect();
        out.push(DrawableElem::Text {
            group_texts: vec![parent_class, "label"],
            data,
        });
    }
    if axis.show_tick {
        let x_right = axis.bb.x + axis.bb.width
            - if axis.show_axis_line {
                axis.cfg.axis_line_width
            } else {
                0.0
            };
        let data: Vec<DrawablePath> = axis
            .tick_values()
            .iter()
            .map(|t| {
                let y = scale_tick(axis, t);
                DrawablePath {
                    path: format!(
                        "M {},{} L {},{}",
                        fmt_num(x_right),
                        fmt_num(y),
                        fmt_num(x_right - axis.cfg.tick_length),
                        fmt_num(y),
                    ),
                    fill: None,
                    stroke_fill: axis.theme.tick_color.clone(),
                    stroke_width: axis.cfg.tick_width,
                }
            })
            .collect();
        out.push(DrawableElem::Path {
            group_texts: vec![parent_class, "ticks"],
            data,
        });
    }
    if axis.show_title {
        let x = axis.bb.x + axis.cfg.title_padding;
        let y = axis.bb.y + axis.bb.height / 2.0;
        out.push(DrawableElem::Text {
            group_texts: vec![parent_class, "title"],
            data: vec![DrawableText {
                x,
                y,
                text: axis.title.clone(),
                fill: axis.theme.title_color.clone(),
                font_size: axis.cfg.title_font_size,
                rotation: 270.0,
                vertical_pos: TextVerticalPos::Top,
                horizontal_pos: TextHorizontalPos::Center,
            }],
        });
    }
}

fn top_axis_elements(
    axis: &Axis,
    out: &mut Vec<DrawableElem>,
    parent_class: &'static str,
    line_class: &'static str,
) {
    if axis.show_axis_line {
        let y = axis.bb.y + axis.bb.height - axis.cfg.axis_line_width / 2.0;
        out.push(DrawableElem::Path {
            group_texts: vec![parent_class, line_class],
            data: vec![DrawablePath {
                path: format!(
                    "M {},{} L {},{}",
                    fmt_num(axis.bb.x),
                    fmt_num(y),
                    fmt_num(axis.bb.x + axis.bb.width),
                    fmt_num(y),
                ),
                fill: None,
                stroke_fill: axis.theme.axis_line_color.clone(),
                stroke_width: axis.cfg.axis_line_width,
            }],
        });
    }
    if axis.show_label {
        let y = axis.bb.y
            + if axis.show_title {
                axis.title_text_height + axis.cfg.title_padding * 2.0
            } else {
                0.0
            }
            + axis.cfg.label_padding;
        let data: Vec<DrawableText> = axis
            .tick_values()
            .iter()
            .map(|t| DrawableText {
                x: scale_tick(axis, t),
                y,
                text: t.clone(),
                fill: axis.theme.label_color.clone(),
                font_size: axis.cfg.label_font_size,
                rotation: 0.0,
                vertical_pos: TextVerticalPos::Top,
                horizontal_pos: TextHorizontalPos::Center,
            })
            .collect();
        out.push(DrawableElem::Text {
            group_texts: vec![parent_class, "label"],
            data,
        });
    }
    if axis.show_tick {
        let y_base_bottom = axis.bb.y + axis.bb.height
            - if axis.show_axis_line {
                axis.cfg.axis_line_width
            } else {
                0.0
            };
        let y_top = y_base_bottom - axis.cfg.tick_length;
        let data: Vec<DrawablePath> = axis
            .tick_values()
            .iter()
            .map(|t| {
                let x = scale_tick(axis, t);
                DrawablePath {
                    path: format!(
                        "M {},{} L {},{}",
                        fmt_num(x),
                        fmt_num(y_base_bottom),
                        fmt_num(x),
                        fmt_num(y_top),
                    ),
                    fill: None,
                    stroke_fill: axis.theme.tick_color.clone(),
                    stroke_width: axis.cfg.tick_width,
                }
            })
            .collect();
        out.push(DrawableElem::Path {
            group_texts: vec![parent_class, "ticks"],
            data,
        });
    }
    if axis.show_title {
        let x = axis.bb.x + axis.bb.width / 2.0;
        let y = axis.bb.y + axis.cfg.title_padding;
        out.push(DrawableElem::Text {
            group_texts: vec![parent_class, "title"],
            data: vec![DrawableText {
                x,
                y,
                text: axis.title.clone(),
                fill: axis.theme.title_color.clone(),
                font_size: axis.cfg.title_font_size,
                rotation: 0.0,
                vertical_pos: TextVerticalPos::Top,
                horizontal_pos: TextHorizontalPos::Center,
            }],
        });
    }
}

/// Return the numeric scale position for the given tick label.
fn scale_tick(axis: &Axis, tick: &str) -> f64 {
    match &axis.kind {
        AxisKind::Band { .. } => axis.scale_band(tick).unwrap_or(axis.effective_range().0),
        AxisKind::Linear { .. } => {
            // Recover the numeric value from the label string.
            let v: f64 = tick.parse().unwrap_or(0.0);
            axis.scale_numeric(v)
        }
    }
}

// ── Plots ────────────────────────────────────────────────────────────

/// Compute bar rects (vertical orientation) or (horizontal orientation).
fn bar_plot_rects(
    data: &[(String, f64)],
    x_axis: &Axis,
    y_axis: &Axis,
    plot_bb: &BoundingRect,
    cfg: &XychartConfig,
    fill: &str,
) -> (Vec<DrawableRect>, Vec<String>) {
    let bar_padding_percent = 0.05;
    let bar_width =
        ((x_axis.outer_padding * 2.0).min(x_axis.tick_distance())) * (1.0 - bar_padding_percent);
    let bar_width_half = bar_width / 2.0;
    let labels: Vec<String> = data.iter().map(|(_, v)| js_value_label(*v)).collect();
    let rects: Vec<DrawableRect> = data
        .iter()
        .map(|(cat, val)| {
            let sx = scale_tick_from_cat(x_axis, cat);
            let sy = y_axis.scale_numeric(*val);
            match cfg.chart_orientation {
                ChartOrientation::Vertical => DrawableRect {
                    x: sx - bar_width_half,
                    y: sy,
                    width: bar_width,
                    height: plot_bb.y + plot_bb.height - sy,
                    fill: fill.to_string(),
                    stroke_fill: fill.to_string(),
                    stroke_width: 0.0,
                },
                ChartOrientation::Horizontal => DrawableRect {
                    x: plot_bb.x,
                    y: sx - bar_width_half,
                    width: sy - plot_bb.x,
                    height: bar_width,
                    fill: fill.to_string(),
                    stroke_fill: fill.to_string(),
                    stroke_width: 0.0,
                },
            }
        })
        .collect();
    (rects, labels)
}

fn scale_tick_from_cat(axis: &Axis, cat: &str) -> f64 {
    match &axis.kind {
        AxisKind::Band { .. } => axis.scale_band(cat).unwrap_or(axis.effective_range().0),
        AxisKind::Linear { .. } => {
            let v: f64 = cat.parse().unwrap_or(0.0);
            axis.scale_numeric(v)
        }
    }
}

/// d3-line generator output — `M x,y L x,y L ...`. Coordinates
/// formatted with `Math.round(v * 1000) / 1000` then JS toString
/// (d3-path's `digits = 3` default).
fn line_plot_path(
    data: &[(String, f64)],
    x_axis: &Axis,
    y_axis: &Axis,
    cfg: &XychartConfig,
) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    let mut s = String::new();
    for (i, (cat, val)) in data.iter().enumerate() {
        let (ax, ay) = {
            let sx = scale_tick_from_cat(x_axis, cat);
            let sy = y_axis.scale_numeric(*val);
            match cfg.chart_orientation {
                ChartOrientation::Vertical => (sx, sy),
                ChartOrientation::Horizontal => (sy, sx),
            }
        };
        if i == 0 {
            s.push('M');
        } else {
            s.push('L');
        }
        s.push_str(&fmt_num3(ax));
        s.push(',');
        s.push_str(&fmt_num3(ay));
    }
    Some(s)
}

// ── Formatting helpers ───────────────────────────────────────────────

/// Stringify like JS's `Number.prototype.toString()`. Delegates to the
/// shared `math::js_number` helper so Ryu's round-half-up tie-breaking
/// is converted to JS's round-half-to-even.
pub(crate) fn fmt_num(v: f64) -> String {
    crate::math::js_number::js_number_to_string(v)
}

/// d3-path's `digits=3` rounding for line-plot paths.
fn fmt_num3(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 {
        return "0".to_string();
    }
    if r.fract() == 0.0 && r.is_finite() {
        return format!("{}", r as i64);
    }
    format!("{}", r)
}

fn js_num_to_string(v: f64) -> String {
    fmt_num(v)
}

fn js_value_label(v: f64) -> String {
    fmt_num(v)
}

// ── d3-array tick algorithm ──────────────────────────────────────────

/// Verbatim port of d3-array's `ticks(start, stop, count)`.
fn d3_ticks(start: f64, stop: f64, count: usize) -> Vec<f64> {
    if count == 0 || !start.is_finite() || !stop.is_finite() {
        return Vec::new();
    }
    let reverse = stop < start;
    let (a, b) = if reverse {
        (stop, start)
    } else {
        (start, stop)
    };
    let step_neg = tick_increment(a, b, count);
    if step_neg == 0.0 || !step_neg.is_finite() {
        return Vec::new();
    }
    let mut out: Vec<f64>;
    if step_neg > 0.0 {
        let step = step_neg;
        let mut r0 = (a / step).round() as i64;
        let mut r1 = (b / step).round() as i64;
        if (r0 as f64) * step < a {
            r0 += 1;
        }
        if (r1 as f64) * step > b {
            r1 -= 1;
        }
        let n = (r1 - r0 + 1).max(0) as usize;
        out = Vec::with_capacity(n);
        for i in 0..n {
            out.push((r0 + i as i64) as f64 * step);
        }
    } else {
        let step = -step_neg;
        let mut r0 = (a * step).round() as i64;
        let mut r1 = (b * step).round() as i64;
        if (r0 as f64) / step < a {
            r0 += 1;
        }
        if (r1 as f64) / step > b {
            r1 -= 1;
        }
        let n = (r1 - r0 + 1).max(0) as usize;
        out = Vec::with_capacity(n);
        for i in 0..n {
            out.push((r0 + i as i64) as f64 / step);
        }
    }
    let _ = (a, b);
    if reverse {
        out.reverse();
    }
    out
}

fn tick_increment(start: f64, stop: f64, count: usize) -> f64 {
    // sqrt(50) ≈ 7.0710678118654755
    // sqrt(10) ≈ 3.1622776601683795
    // sqrt(2)  ≈ 1.4142135623730951
    let e10 = (50f64).sqrt();
    let e5 = (10f64).sqrt();
    let e2 = (2f64).sqrt();

    let step = (stop - start) / (count.max(1) as f64);
    let power = step.log10().floor() as i32;
    let error = step / 10f64.powi(power);
    let factor = if error >= e10 {
        10.0
    } else if error >= e5 {
        5.0
    } else if error >= e2 {
        2.0
    } else {
        1.0
    };
    if power >= 0 {
        factor * 10f64.powi(power)
    } else {
        -10f64.powi(-power) / factor
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d3_ticks_simple() {
        let t = d3_ticks(10.0, 30.0, 10);
        assert_eq!(
            t,
            vec![10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 30.0]
        );
    }

    #[test]
    fn d3_ticks_revenue() {
        let t = d3_ticks(4000.0, 11000.0, 10);
        assert_eq!(
            t,
            vec![
                4000.0, 4500.0, 5000.0, 5500.0, 6000.0, 6500.0, 7000.0, 7500.0, 8000.0, 8500.0,
                9000.0, 9500.0, 10000.0, 10500.0, 11000.0,
            ]
        );
    }

    #[test]
    fn d3_ticks_reversed() {
        let t = d3_ticks(30.0, 10.0, 10);
        assert_eq!(t.first().copied(), Some(30.0));
        assert_eq!(t.last().copied(), Some(10.0));
    }

    #[test]
    fn d3_ticks_fractional() {
        let t = d3_ticks(1.0, 3.0, 10);
        assert_eq!(t.len(), 11);
        assert_eq!(t[0], 1.0);
        assert!((t[1] - 1.2).abs() < 1e-12);
    }
}
