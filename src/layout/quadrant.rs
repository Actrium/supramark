//! Quadrant-chart layout — port of upstream `QuadrantBuilder.build()`.
//!
//! Produces a `QuadrantLayout` with every coordinate + resolved colour
//! pre-computed. The render stage is a pure `format!` walk over this
//! structure; no geometry happens there.
//!
//! Upstream reference:
//! /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/quadrant-chart/quadrantBuilder.ts

use crate::error::Result;
use crate::model::quadrant::{QuadrantDiagram, QuadrantStyles};
use crate::theme::ThemeVariables;

// -------------------------------------------------------------------------------------------------
// Upstream defaults (quadrantChart section of defaultConfig.ts).
// -------------------------------------------------------------------------------------------------
const DEF_CHART_WIDTH: f64 = 500.0;
const DEF_CHART_HEIGHT: f64 = 500.0;
const DEF_TITLE_PADDING: f64 = 10.0;
const DEF_TITLE_FONT_SIZE: f64 = 20.0;
const DEF_QUADRANT_PADDING: f64 = 5.0;
const DEF_X_AXIS_LABEL_PADDING: f64 = 5.0;
const DEF_Y_AXIS_LABEL_PADDING: f64 = 5.0;
const DEF_X_AXIS_LABEL_FONT_SIZE: f64 = 16.0;
const DEF_Y_AXIS_LABEL_FONT_SIZE: f64 = 16.0;
const DEF_QUADRANT_LABEL_FONT_SIZE: f64 = 16.0;
const DEF_QUADRANT_TEXT_TOP_PADDING: f64 = 5.0;
const DEF_POINT_TEXT_PADDING: f64 = 5.0;
const DEF_POINT_LABEL_FONT_SIZE: f64 = 12.0;
const DEF_POINT_RADIUS: f64 = 5.0;
const DEF_X_AXIS_POSITION: &str = "top";
const DEF_Y_AXIS_POSITION: &str = "left";
const DEF_INTERNAL_BORDER_STROKE_WIDTH: f64 = 1.0;
const DEF_EXTERNAL_BORDER_STROKE_WIDTH: f64 = 2.0;

#[derive(Debug, Clone, Copy)]
pub enum VerticalPos {
    Left,
    Center,
}

#[derive(Debug, Clone, Copy)]
pub enum HorizontalPos {
    Top,
    Middle,
}

impl VerticalPos {
    /// `getTextAnchor` — `left` → `start`, else `middle`.
    pub fn text_anchor(self) -> &'static str {
        match self {
            VerticalPos::Left => "start",
            VerticalPos::Center => "middle",
        }
    }
}

impl HorizontalPos {
    /// `getDominantBaseLine` — `top` → `hanging`, else `middle`.
    pub fn dominant_baseline(self) -> &'static str {
        match self {
            HorizontalPos::Top => "hanging",
            HorizontalPos::Middle => "middle",
        }
    }
}

#[derive(Debug, Clone)]
pub struct QuadrantText {
    pub text: String,
    pub fill: String,
    pub x: f64,
    pub y: f64,
    pub font_size: f64,
    pub vertical_pos: VerticalPos,
    pub horizontal_pos: HorizontalPos,
    pub rotation: f64,
}

#[derive(Debug, Clone)]
pub struct QuadrantRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub fill: String,
    pub text: QuadrantText,
}

#[derive(Debug, Clone)]
pub struct QuadrantLine {
    pub stroke: String,
    pub stroke_width: f64,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

#[derive(Debug, Clone)]
pub struct QuadrantPointOut {
    pub cx: f64,
    pub cy: f64,
    pub radius: f64,
    pub fill: String,
    pub stroke: String,
    pub stroke_width: String,
    pub text: QuadrantText,
}

/// Layout output — a fully resolved, ready-to-render view.
#[derive(Debug, Clone, Default)]
pub struct QuadrantLayout {
    pub chart_width: f64,
    pub chart_height: f64,
    pub quadrants: Vec<QuadrantRect>,
    pub border_lines: Vec<QuadrantLine>,
    pub axis_labels: Vec<QuadrantText>,
    pub title: Option<QuadrantText>,
    pub points: Vec<QuadrantPointOut>,

    /// Theme after applying `themeVariables` overrides — the renderer
    /// uses this for the CSS block.
    pub effective_theme: ThemeVariables,
}

pub fn layout(d: &QuadrantDiagram, theme: &ThemeVariables) -> Result<QuadrantLayout> {
    // 1. Resolve effective theme with any `themeVariables` overrides
    //    applied on top.
    let effective_theme = resolve_effective_theme(theme, d);

    // 2. Resolve config.
    let chart_width = d.config.chart_width.unwrap_or(DEF_CHART_WIDTH);
    let chart_height = d.config.chart_height.unwrap_or(DEF_CHART_HEIGHT);
    let title_padding = d.config.title_padding.unwrap_or(DEF_TITLE_PADDING);
    let title_font_size = d.config.title_font_size.unwrap_or(DEF_TITLE_FONT_SIZE);
    let quadrant_padding = d.config.quadrant_padding.unwrap_or(DEF_QUADRANT_PADDING);
    let x_axis_label_padding = d
        .config
        .x_axis_label_padding
        .unwrap_or(DEF_X_AXIS_LABEL_PADDING);
    let y_axis_label_padding = d
        .config
        .y_axis_label_padding
        .unwrap_or(DEF_Y_AXIS_LABEL_PADDING);
    let x_axis_label_font_size = d
        .config
        .x_axis_label_font_size
        .unwrap_or(DEF_X_AXIS_LABEL_FONT_SIZE);
    let y_axis_label_font_size = d
        .config
        .y_axis_label_font_size
        .unwrap_or(DEF_Y_AXIS_LABEL_FONT_SIZE);
    let quadrant_label_font_size = d
        .config
        .quadrant_label_font_size
        .unwrap_or(DEF_QUADRANT_LABEL_FONT_SIZE);
    let quadrant_text_top_padding = d
        .config
        .quadrant_text_top_padding
        .unwrap_or(DEF_QUADRANT_TEXT_TOP_PADDING);
    let point_text_padding = d
        .config
        .point_text_padding
        .unwrap_or(DEF_POINT_TEXT_PADDING);
    let point_label_font_size = d
        .config
        .point_label_font_size
        .unwrap_or(DEF_POINT_LABEL_FONT_SIZE);
    let point_radius = d.config.point_radius.unwrap_or(DEF_POINT_RADIUS);
    let cfg_x_pos = d
        .config
        .x_axis_position
        .as_deref()
        .unwrap_or(DEF_X_AXIS_POSITION);
    let y_axis_position = d
        .config
        .y_axis_position
        .as_deref()
        .unwrap_or(DEF_Y_AXIS_POSITION);
    let internal_border_stroke_width = d
        .config
        .quadrant_internal_border_stroke_width
        .unwrap_or(DEF_INTERNAL_BORDER_STROKE_WIDTH);
    let external_border_stroke_width = d
        .config
        .quadrant_external_border_stroke_width
        .unwrap_or(DEF_EXTERNAL_BORDER_STROKE_WIDTH);

    // 3. `build()` prologue — the flags that downstream helpers key on.
    let show_x_axis = !d.x_axis_left_text.is_empty() || !d.x_axis_right_text.is_empty();
    let show_y_axis = !d.y_axis_bottom_text.is_empty() || !d.y_axis_top_text.is_empty();
    let show_title = d
        .meta
        .title
        .as_deref()
        .map(|t| !t.is_empty())
        .unwrap_or(false);

    // Upstream: `xAxisPosition = points.length > 0 ? 'bottom' : config.xAxisPosition`.
    let x_axis_position: &str = if !d.points.is_empty() {
        "bottom"
    } else {
        cfg_x_pos
    };

    // 4. calculateSpace.
    let x_axis_space_calc = x_axis_label_padding * 2.0 + x_axis_label_font_size;
    let x_axis_space_top = if x_axis_position == "top" && show_x_axis {
        x_axis_space_calc
    } else {
        0.0
    };
    let x_axis_space_bottom = if x_axis_position == "bottom" && show_x_axis {
        x_axis_space_calc
    } else {
        0.0
    };
    let y_axis_space_calc = y_axis_label_padding * 2.0 + y_axis_label_font_size;
    let y_axis_space_left = if y_axis_position == "left" && show_y_axis {
        y_axis_space_calc
    } else {
        0.0
    };
    let y_axis_space_right = if y_axis_position == "right" && show_y_axis {
        y_axis_space_calc
    } else {
        0.0
    };
    let title_space_top = if show_title {
        title_font_size + title_padding * 2.0
    } else {
        0.0
    };

    let quadrant_left = quadrant_padding + y_axis_space_left;
    let quadrant_top = quadrant_padding + x_axis_space_top + title_space_top;
    let quadrant_width =
        chart_width - quadrant_padding * 2.0 - y_axis_space_left - y_axis_space_right;
    let quadrant_height = chart_height
        - quadrant_padding * 2.0
        - x_axis_space_top
        - x_axis_space_bottom
        - title_space_top;
    let quadrant_half_width = quadrant_width / 2.0;
    let quadrant_half_height = quadrant_height / 2.0;

    // 5. Build sections — order in the struct doesn't matter, but we
    //    match the renderer's emission order below.
    let q1_fill = theme_str(&effective_theme.quadrant1_fill);
    let q2_fill = theme_str(&effective_theme.quadrant2_fill);
    let q3_fill = theme_str(&effective_theme.quadrant3_fill);
    let q4_fill = theme_str(&effective_theme.quadrant4_fill);
    let q1_tf = theme_str(&effective_theme.quadrant1_text_fill);
    let q2_tf = theme_str(&effective_theme.quadrant2_text_fill);
    let q3_tf = theme_str(&effective_theme.quadrant3_text_fill);
    let q4_tf = theme_str(&effective_theme.quadrant4_text_fill);
    let point_fill = theme_str(&effective_theme.quadrant_point_fill);
    let point_text_fill = theme_str(&effective_theme.quadrant_point_text_fill);
    let x_axis_text_fill = theme_str(&effective_theme.quadrant_x_axis_text_fill);
    let y_axis_text_fill = theme_str(&effective_theme.quadrant_y_axis_text_fill);
    let title_fill = theme_str(&effective_theme.quadrant_title_fill);
    let ext_border_fill = theme_str(&effective_theme.quadrant_external_border_stroke_fill);
    let int_border_fill = theme_str(&effective_theme.quadrant_internal_border_stroke_fill);

    // --- Quadrants ---
    let points_exist = !d.points.is_empty();
    let mut quadrants = Vec::with_capacity(4);

    let entries: [(f64, f64, String, String, &str); 4] = [
        // (x, y, fill, text, texttext)
        (
            quadrant_left + quadrant_half_width,
            quadrant_top,
            q1_fill.clone(),
            q1_tf.clone(),
            d.quadrant1_text.as_str(),
        ),
        (
            quadrant_left,
            quadrant_top,
            q2_fill.clone(),
            q2_tf.clone(),
            d.quadrant2_text.as_str(),
        ),
        (
            quadrant_left,
            quadrant_top + quadrant_half_height,
            q3_fill.clone(),
            q3_tf.clone(),
            d.quadrant3_text.as_str(),
        ),
        (
            quadrant_left + quadrant_half_width,
            quadrant_top + quadrant_half_height,
            q4_fill.clone(),
            q4_tf.clone(),
            d.quadrant4_text.as_str(),
        ),
    ];
    for (qx, qy, fill, text_fill, text) in entries.iter() {
        let width = quadrant_half_width;
        let height = quadrant_half_height;
        let tx = qx + width / 2.0;
        let (ty, hpos) = if points_exist {
            (qy + quadrant_text_top_padding, HorizontalPos::Top)
        } else {
            (qy + height / 2.0, HorizontalPos::Middle)
        };
        quadrants.push(QuadrantRect {
            x: *qx,
            y: *qy,
            width,
            height,
            fill: fill.clone(),
            text: QuadrantText {
                text: text.to_string(),
                fill: text_fill.clone(),
                x: tx,
                y: ty,
                font_size: quadrant_label_font_size,
                vertical_pos: VerticalPos::Center,
                horizontal_pos: hpos,
                rotation: 0.0,
            },
        });
    }

    // --- Border lines (6 entries, order matches upstream). ---
    let half_ext = external_border_stroke_width / 2.0;
    let mut border_lines = Vec::with_capacity(6);
    // top
    border_lines.push(QuadrantLine {
        stroke: ext_border_fill.clone(),
        stroke_width: external_border_stroke_width,
        x1: quadrant_left - half_ext,
        y1: quadrant_top,
        x2: quadrant_left + quadrant_width + half_ext,
        y2: quadrant_top,
    });
    // right
    border_lines.push(QuadrantLine {
        stroke: ext_border_fill.clone(),
        stroke_width: external_border_stroke_width,
        x1: quadrant_left + quadrant_width,
        y1: quadrant_top + half_ext,
        x2: quadrant_left + quadrant_width,
        y2: quadrant_top + quadrant_height - half_ext,
    });
    // bottom
    border_lines.push(QuadrantLine {
        stroke: ext_border_fill.clone(),
        stroke_width: external_border_stroke_width,
        x1: quadrant_left - half_ext,
        y1: quadrant_top + quadrant_height,
        x2: quadrant_left + quadrant_width + half_ext,
        y2: quadrant_top + quadrant_height,
    });
    // left
    border_lines.push(QuadrantLine {
        stroke: ext_border_fill.clone(),
        stroke_width: external_border_stroke_width,
        x1: quadrant_left,
        y1: quadrant_top + half_ext,
        x2: quadrant_left,
        y2: quadrant_top + quadrant_height - half_ext,
    });
    // vertical inner
    border_lines.push(QuadrantLine {
        stroke: int_border_fill.clone(),
        stroke_width: internal_border_stroke_width,
        x1: quadrant_left + quadrant_half_width,
        y1: quadrant_top + half_ext,
        x2: quadrant_left + quadrant_half_width,
        y2: quadrant_top + quadrant_height - half_ext,
    });
    // horizontal inner
    border_lines.push(QuadrantLine {
        stroke: int_border_fill,
        stroke_width: internal_border_stroke_width,
        x1: quadrant_left + half_ext,
        y1: quadrant_top + quadrant_half_height,
        x2: quadrant_left + quadrant_width - half_ext,
        y2: quadrant_top + quadrant_half_height,
    });

    // --- Axis labels (order: xLeft, xRight, yBottom, yTop). ---
    let mut axis_labels: Vec<QuadrantText> = Vec::new();
    let draw_x_in_middle = !d.x_axis_right_text.is_empty();
    let draw_y_in_middle = !d.y_axis_top_text.is_empty();

    if !d.x_axis_left_text.is_empty() && show_x_axis {
        axis_labels.push(QuadrantText {
            text: d.x_axis_left_text.clone(),
            fill: x_axis_text_fill.clone(),
            x: quadrant_left
                + if draw_x_in_middle {
                    quadrant_half_width / 2.0
                } else {
                    0.0
                },
            y: if x_axis_position == "top" {
                x_axis_label_padding + title_space_top
            } else {
                x_axis_label_padding + quadrant_top + quadrant_height + quadrant_padding
            },
            font_size: x_axis_label_font_size,
            vertical_pos: if draw_x_in_middle {
                VerticalPos::Center
            } else {
                VerticalPos::Left
            },
            horizontal_pos: HorizontalPos::Top,
            rotation: 0.0,
        });
    }
    if !d.x_axis_right_text.is_empty() && show_x_axis {
        axis_labels.push(QuadrantText {
            text: d.x_axis_right_text.clone(),
            fill: x_axis_text_fill.clone(),
            x: quadrant_left
                + quadrant_half_width
                + if draw_x_in_middle {
                    quadrant_half_width / 2.0
                } else {
                    0.0
                },
            y: if x_axis_position == "top" {
                x_axis_label_padding + title_space_top
            } else {
                x_axis_label_padding + quadrant_top + quadrant_height + quadrant_padding
            },
            font_size: x_axis_label_font_size,
            vertical_pos: if draw_x_in_middle {
                VerticalPos::Center
            } else {
                VerticalPos::Left
            },
            horizontal_pos: HorizontalPos::Top,
            rotation: 0.0,
        });
    }
    if !d.y_axis_bottom_text.is_empty() && show_y_axis {
        axis_labels.push(QuadrantText {
            text: d.y_axis_bottom_text.clone(),
            fill: y_axis_text_fill.clone(),
            x: if y_axis_position == "left" {
                y_axis_label_padding
            } else {
                y_axis_label_padding + quadrant_left + quadrant_width + quadrant_padding
            },
            y: quadrant_top + quadrant_height
                - if draw_y_in_middle {
                    quadrant_half_height / 2.0
                } else {
                    0.0
                },
            font_size: y_axis_label_font_size,
            vertical_pos: if draw_y_in_middle {
                VerticalPos::Center
            } else {
                VerticalPos::Left
            },
            horizontal_pos: HorizontalPos::Top,
            rotation: -90.0,
        });
    }
    if !d.y_axis_top_text.is_empty() && show_y_axis {
        axis_labels.push(QuadrantText {
            text: d.y_axis_top_text.clone(),
            fill: y_axis_text_fill.clone(),
            x: if y_axis_position == "left" {
                y_axis_label_padding
            } else {
                y_axis_label_padding + quadrant_left + quadrant_width + quadrant_padding
            },
            y: quadrant_top + quadrant_half_height
                - if draw_y_in_middle {
                    quadrant_half_height / 2.0
                } else {
                    0.0
                },
            font_size: y_axis_label_font_size,
            vertical_pos: if draw_y_in_middle {
                VerticalPos::Center
            } else {
                VerticalPos::Left
            },
            horizontal_pos: HorizontalPos::Top,
            rotation: -90.0,
        });
    }

    // --- Title ---
    let title = if show_title {
        Some(QuadrantText {
            text: d.meta.title.clone().unwrap_or_default(),
            fill: title_fill.clone(),
            x: chart_width / 2.0,
            y: title_padding,
            font_size: title_font_size,
            vertical_pos: VerticalPos::Center,
            horizontal_pos: HorizontalPos::Top,
            rotation: 0.0,
        })
    } else {
        None
    };

    // --- Points ---
    // Upstream `d3.scaleLinear().domain([0, 1]).range([a, b])` uses
    // `d3-interpolate.interpolateNumber(a, b)(t) = a*(1-t) + b*t` —
    // NOT `a + t*(b-a)`. The two are algebraically equal but diverge
    // at the last bit of the mantissa; byte-exact parity needs the
    // former.
    let x_range_a = quadrant_left;
    let x_range_b = quadrant_width + quadrant_left;
    let y_range_a = quadrant_height + quadrant_top;
    let y_range_b = quadrant_top;
    let map_x = |t: f64| x_range_a * (1.0 - t) + x_range_b * t;
    let map_y = |t: f64| y_range_a * (1.0 - t) + y_range_b * t;

    // Build className → styles lookup. Upstream uses the *last* entry
    // for a duplicate name; in our fixtures every class name is unique.
    let class_for = |name: &str| -> Option<&QuadrantStyles> {
        d.classes
            .iter()
            .rev()
            .find(|c| c.name == name)
            .map(|c| &c.styles)
    };

    let mut points_out = Vec::with_capacity(d.points.len());
    for p in &d.points {
        // Upstream:
        //   point = { ...classStyles, ...point };
        // — point fields overwrite class fields when both set.
        let class_styles = p.class_name.as_deref().and_then(class_for);
        let radius = p
            .styles
            .radius
            .or_else(|| class_styles.and_then(|c| c.radius))
            .unwrap_or(point_radius);
        let color = p
            .styles
            .color
            .clone()
            .or_else(|| class_styles.and_then(|c| c.color.clone()));
        let stroke_color = p
            .styles
            .stroke_color
            .clone()
            .or_else(|| class_styles.and_then(|c| c.stroke_color.clone()));
        let stroke_width = p
            .styles
            .stroke_width
            .clone()
            .or_else(|| class_styles.and_then(|c| c.stroke_width.clone()));

        let cx = map_x(p.x);
        let cy = map_y(p.y);
        let fill = color.unwrap_or_else(|| point_fill.clone());
        let stroke = stroke_color.unwrap_or_else(|| point_fill.clone());
        let sw = stroke_width.unwrap_or_else(|| "0px".to_string());

        points_out.push(QuadrantPointOut {
            cx,
            cy,
            radius,
            fill,
            stroke,
            stroke_width: sw,
            text: QuadrantText {
                text: p.text.clone(),
                fill: point_text_fill.clone(),
                x: cx,
                y: cy + point_text_padding,
                font_size: point_label_font_size,
                vertical_pos: VerticalPos::Center,
                horizontal_pos: HorizontalPos::Top,
                rotation: 0.0,
            },
        });
    }

    Ok(QuadrantLayout {
        chart_width,
        chart_height,
        quadrants,
        border_lines,
        axis_labels,
        title,
        points: points_out,
        effective_theme,
    })
}

// -------------------------------------------------------------------------------------------------
// Theme resolution.
// -------------------------------------------------------------------------------------------------

/// Merge any `themeVariables` overrides captured from an init directive
/// on top of the baseline `theme`. Operates purely on the fields the
/// renderer reads, so unknown keys in the JSON are silently dropped.
fn resolve_effective_theme(base: &ThemeVariables, d: &QuadrantDiagram) -> ThemeVariables {
    let mut out = base.clone();
    let Some(v) = d.theme_overrides_json.as_ref() else {
        return out;
    };
    let Some(obj) = v.as_object() else {
        return out;
    };

    macro_rules! take {
        ($key:literal, $field:ident) => {
            if let Some(serde_json::Value::String(s)) = obj.get($key) {
                out.$field = Some(s.clone());
            }
        };
    }
    // Quadrant-specific keys.
    take!("quadrant1Fill", quadrant1_fill);
    take!("quadrant2Fill", quadrant2_fill);
    take!("quadrant3Fill", quadrant3_fill);
    take!("quadrant4Fill", quadrant4_fill);
    take!("quadrant1TextFill", quadrant1_text_fill);
    take!("quadrant2TextFill", quadrant2_text_fill);
    take!("quadrant3TextFill", quadrant3_text_fill);
    take!("quadrant4TextFill", quadrant4_text_fill);
    take!("quadrantPointFill", quadrant_point_fill);
    take!("quadrantPointTextFill", quadrant_point_text_fill);
    take!("quadrantXAxisTextFill", quadrant_x_axis_text_fill);
    take!("quadrantYAxisTextFill", quadrant_y_axis_text_fill);
    take!(
        "quadrantInternalBorderStrokeFill",
        quadrant_internal_border_stroke_fill
    );
    take!(
        "quadrantExternalBorderStrokeFill",
        quadrant_external_border_stroke_fill
    );
    take!("quadrantTitleFill", quadrant_title_fill);
    // Common CSS keys the style-block consumes.
    take!("fontFamily", font_family);
    take!("fontSize", font_size);
    take!("textColor", text_color);
    take!("titleColor", title_color);
    take!("lineColor", line_color);
    take!("nodeBorder", node_border);
    take!("errorBkgColor", error_bkg_color);
    take!("errorTextColor", error_text_color);
    out
}

fn theme_str(slot: &Option<String>) -> String {
    slot.clone().unwrap_or_default()
}
