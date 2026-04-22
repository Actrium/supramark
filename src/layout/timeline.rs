//! Timeline layout — produces geometry in the byte-exact shape
//! upstream's `timelineRenderer.ts` (horizontal, default) + its
//! `timelineRendererVertical.ts` variant emit.
//!
//! Upstream key numbers (LR / horizontal):
//!   * task node body: 150 px wide pre-padding, +2×20 padding ⇒ 190 px.
//!   * section node body: `200 * max(tasksForSection, 1) - 50` wide
//!     pre-padding, +40 padding ⇒ `200*N - 50 + 40`; the upstream
//!     renderer then offsets it by `+50` leftmost.
//!   * row Y origins: `50` (section top) / `50 + maxSectionHeight + 50`
//!     (task top) / task origin + `100` (events top).
//!   * row heights: `bbox.height + fontSize*1.1*0.5 + padding` clamped
//!     by `maxHeight` — we mirror that per-node with the DejaVu font
//!     metrics in `crate::font_metrics` (+ same d3 `wrap` splitter).
//!
//! The vertical renderer uses a smaller set of constants (see the
//! module body) and a different line-wrapper drawing order.

use crate::error::Result;
use crate::font_metrics::text_width;
use crate::model::timeline::{TimelineDiagram, TimelineDirection};
use crate::theme::ThemeVariables;

// ── Upstream constants (LR renderer) ─────────────────────────────
pub(crate) const LR_NODE_WIDTH: f64 = 150.0;
pub(crate) const LR_NODE_PADDING: f64 = 20.0;
pub(crate) const LR_SECTION_Y: f64 = 50.0;
pub(crate) const LR_LEFT_MARGIN: f64 = 50.0;

// ── Upstream constants (TD / vertical renderer) ──────────────────
pub(crate) const TD_NODE_WIDTH: f64 = 200.0;
pub(crate) const TD_NODE_PADDING: f64 = 5.0;
pub(crate) const TD_EVENT_WIDTH: f64 = TD_NODE_WIDTH + 100.0;
pub(crate) const TD_EVENT_SPACING: f64 = 10.0;
pub(crate) const TD_SECTION_TASK_GAP: f64 = 20.0;
pub(crate) const TD_TASK_AXIS_GAP: f64 = 20.0;
pub(crate) const TD_TASK_VERTICAL_GAP: f64 = 30.0;
pub(crate) const TD_EVENT_AXIS_GAP: f64 = 50.0;

#[derive(Debug, Clone, Default)]
pub struct TimelineLayout {
    pub direction: TimelineDirection,
    /// Resolved font family CSS token (canonical list form).
    pub font_family_css: String,
    /// Resolved font size as CSS string (`"16px"`).
    pub font_size_css: String,
    /// Numeric font size (pixels).
    pub font_size_px: f64,
    /// Viewbox x/y/width/height — the four numbers inside `viewBox=""`.
    pub viewbox: [f64; 4],
    /// `style="max-width: {px}px;"` — the upstream renderer emits an
    /// integer-looking number but the underlying value carries fractional
    /// digits when the bbox does, so we keep the raw f64.
    pub max_width_px: f64,
    /// Rendered nodes (tasks, events, section headers) with geometry.
    pub nodes: Vec<LaidNode>,
    /// Lines between tasks and their events (dashed), plus axis line.
    pub lines: Vec<LaidLine>,
    /// Axis line shown at the bottom (LR) or right (TD) of the diagram.
    pub axis: Option<LaidLine>,
    /// Title x/y, if a title is configured.
    pub title_xy: Option<(f64, f64)>,
    /// True when we have a title.
    pub has_title: bool,
    /// Value of the title text (already user-escaped).
    pub title_text: String,
}

/// Kind of node being rendered. Affects CSS class prefix and section
/// colour index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaidNodeKind {
    Section,
    Task,
    Event,
}

#[derive(Debug, Clone)]
pub struct LaidNode {
    pub kind: LaidNodeKind,
    /// Translation applied to the wrapper `<g>` element.
    pub x: f64,
    pub y: f64,
    /// Node body width (post-padding).
    pub width: f64,
    /// Node body height (the upstream "node.height" after wrap).
    pub height: f64,
    /// Stroke/fill CSS section index. `-1` is the "no-section" slot
    /// upstream keys on when no `section` keyword is present.
    pub section_index: i32,
    /// Pre-wrapped lines of descriptive text. Each entry has the
    /// pre-`.trim()` form so the renderer can drop the leading/trailing
    /// whitespace deterministically via `String::trim`.
    pub lines: Vec<String>,
    /// Upstream per-node monotonic id counter.
    pub node_id: usize,
}

#[derive(Debug, Clone)]
pub struct LaidLine {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub stroke_width: f64,
    pub dashed: bool,
}

pub fn layout(d: &TimelineDiagram, theme: &ThemeVariables) -> Result<TimelineLayout> {
    // Resolve font family / size once.
    let font_family_css = d
        .font_family
        .clone()
        .or_else(|| theme.font_family.clone())
        .unwrap_or_else(|| "\"trebuchet ms\", verdana, arial, sans-serif".to_string());
    let font_size_css = d
        .font_size
        .clone()
        .or_else(|| theme.font_size.clone())
        .unwrap_or_else(|| "16px".to_string());
    let font_size_px = parse_px(&font_size_css).unwrap_or(16.0);

    match d.direction {
        TimelineDirection::LR => layout_lr(d, font_family_css, font_size_css, font_size_px),
        TimelineDirection::TD => layout_td(d, font_family_css, font_size_css, font_size_px),
    }
}

fn layout_lr(
    d: &TimelineDiagram,
    font_family_css: String,
    font_size_css: String,
    font_size_px: f64,
) -> Result<TimelineLayout> {
    let mut l = TimelineLayout {
        direction: TimelineDirection::LR,
        font_family_css: font_family_css.clone(),
        font_size_css,
        font_size_px,
        ..TimelineLayout::default()
    };
    l.has_title = d.meta.title.is_some();
    l.title_text = d.meta.title.clone().unwrap_or_default();

    // Pre-compute node heights via the d3 `wrap` emulation.
    let max_section_height = d
        .sections
        .iter()
        .map(|name| {
            node_height(
                name,
                LR_NODE_WIDTH,
                LR_NODE_PADDING,
                &font_family_css,
                font_size_px,
            ) + 20.0
        })
        .fold(0.0f64, f64::max);
    let max_task_height = d
        .tasks
        .iter()
        .map(|t| {
            node_height(
                &t.task,
                LR_NODE_WIDTH,
                LR_NODE_PADDING,
                &font_family_css,
                font_size_px,
            ) + 20.0
        })
        .fold(0.0f64, f64::max);

    // maxEventLineLength: per task, sum event heights + spacing.
    let mut max_event_line_length: f64 = 0.0;
    for t in &d.tasks {
        let mut sum = 0.0;
        for e in &t.events {
            sum += node_height(
                e,
                LR_NODE_WIDTH,
                LR_NODE_PADDING,
                &font_family_css,
                font_size_px,
            );
        }
        if !t.events.is_empty() {
            sum += (t.events.len() as f64 - 1.0) * 10.0;
        }
        if sum > max_event_line_length {
            max_event_line_length = sum;
        }
    }

    // Layout section / task grid.
    let left_margin = if d.left_margin > 0.0 {
        d.left_margin
    } else {
        LR_LEFT_MARGIN
    };
    let mut master_x = 50.0 + left_margin; // 100 for default.
    let section_begin_y = LR_SECTION_Y;
    let master_y_tasks = section_begin_y + max_section_height + 50.0;
    let events_y = master_y_tasks + 100.0;

    let mut node_id = 0usize;
    let has_sections = !d.sections.is_empty();

    if has_sections {
        let mut section_index: i32 = 0;
        for sname in &d.sections {
            let tasks_for_section: Vec<_> =
                d.tasks.iter().filter(|t| &t.section == sname).collect();
            let n = tasks_for_section.len().max(1);
            let sec_body_w = 200.0 * n as f64 - 50.0;
            let sec_total_w = sec_body_w + 2.0 * LR_NODE_PADDING;
            let sec_h = node_height(
                sname,
                sec_body_w,
                LR_NODE_PADDING,
                &font_family_css,
                font_size_px,
            )
            .max(max_section_height);

            l.nodes.push(LaidNode {
                kind: LaidNodeKind::Section,
                x: master_x,
                y: section_begin_y,
                width: sec_total_w,
                height: sec_h,
                section_index,
                lines: wrap_text(sname, sec_body_w, &font_family_css, font_size_px),
                node_id,
            });
            node_id += 1;

            for (i, t) in tasks_for_section.iter().enumerate() {
                let task_x = master_x + 200.0 * i as f64;
                let task_total_w = LR_NODE_WIDTH + 2.0 * LR_NODE_PADDING;
                let th = node_height(
                    &t.task,
                    LR_NODE_WIDTH,
                    LR_NODE_PADDING,
                    &font_family_css,
                    font_size_px,
                )
                .max(max_task_height);
                l.nodes.push(LaidNode {
                    kind: LaidNodeKind::Task,
                    x: task_x,
                    y: master_y_tasks,
                    width: task_total_w,
                    height: th,
                    section_index,
                    lines: wrap_text(&t.task, LR_NODE_WIDTH, &font_family_css, font_size_px),
                    node_id,
                });
                node_id += 1;

                // dashed axis line (one per task with any events)
                if !t.events.is_empty() {
                    l.lines.push(LaidLine {
                        x1: task_x + 95.0,
                        y1: master_y_tasks + max_task_height,
                        x2: task_x + 95.0,
                        y2: master_y_tasks
                            + max_task_height
                            + 100.0
                            + max_event_line_length
                            + 100.0,
                        stroke_width: 2.0,
                        dashed: true,
                    });
                }

                let mut ey = events_y;
                for e in &t.events {
                    let eh = node_height(
                        e,
                        LR_NODE_WIDTH,
                        LR_NODE_PADDING,
                        &font_family_css,
                        font_size_px,
                    );
                    l.nodes.push(LaidNode {
                        kind: LaidNodeKind::Event,
                        x: task_x,
                        y: ey,
                        width: task_total_w,
                        height: eh,
                        section_index,
                        lines: wrap_text(e, LR_NODE_WIDTH, &font_family_css, font_size_px),
                        node_id,
                    });
                    node_id += 1;
                    ey += 10.0 + eh;
                }
            }

            master_x += 200.0 * n as f64;
            section_index += 1;
        }
    } else {
        let mut section_index: i32 = -1; // no-section slot
        for (i, t) in d.tasks.iter().enumerate() {
            let task_x = master_x + 200.0 * i as f64;
            let th = node_height(
                &t.task,
                LR_NODE_WIDTH,
                LR_NODE_PADDING,
                &font_family_css,
                font_size_px,
            )
            .max(max_task_height);
            let task_total_w = LR_NODE_WIDTH + 2.0 * LR_NODE_PADDING;
            l.nodes.push(LaidNode {
                kind: LaidNodeKind::Task,
                x: task_x,
                y: master_y_tasks,
                width: task_total_w,
                height: th,
                section_index,
                lines: wrap_text(&t.task, LR_NODE_WIDTH, &font_family_css, font_size_px),
                node_id,
            });
            node_id += 1;

            if !t.events.is_empty() {
                l.lines.push(LaidLine {
                    x1: task_x + 95.0,
                    y1: master_y_tasks + max_task_height,
                    x2: task_x + 95.0,
                    y2: master_y_tasks + max_task_height + 100.0 + max_event_line_length + 100.0,
                    stroke_width: 2.0,
                    dashed: true,
                });
            }

            let mut ey = events_y;
            for e in &t.events {
                let eh = node_height(
                    e,
                    LR_NODE_WIDTH,
                    LR_NODE_PADDING,
                    &font_family_css,
                    font_size_px,
                );
                l.nodes.push(LaidNode {
                    kind: LaidNodeKind::Event,
                    x: task_x,
                    y: ey,
                    width: task_total_w,
                    height: eh,
                    section_index,
                    lines: wrap_text(e, LR_NODE_WIDTH, &font_family_css, font_size_px),
                    node_id,
                });
                node_id += 1;
                ey += 10.0 + eh;
            }

            if !d.disable_multicolor {
                section_index += 1;
            }
        }
    }

    // BBox of the laid-out nodes (approximates svg.getBBox behaviour).
    let (bbox_x, bbox_y, bbox_w, bbox_h) = compute_bbox(&l.nodes, &l.lines);

    // Title placement (at the time of upstream's title append: box comes
    // from pre-title SVG nodes only).
    if l.has_title {
        let title_x = bbox_w / 2.0 - left_margin;
        l.title_xy = Some((title_x, 20.0));
    }

    // Axis: horizontal line across the diagram.
    let depth_y = if has_sections {
        max_section_height + max_task_height + 150.0
    } else {
        max_task_height + 100.0
    };
    l.axis = Some(LaidLine {
        x1: left_margin,
        y1: depth_y,
        x2: bbox_w + 3.0 * left_margin,
        y2: depth_y,
        stroke_width: 4.0,
        dashed: false,
    });

    // Final bbox including axis + title.
    let final_w = (bbox_x + bbox_w).max(l.axis.as_ref().map(|a| a.x2).unwrap_or(0.0));
    let final_h = (bbox_y + bbox_h).max(l.axis.as_ref().map(|a| a.y2).unwrap_or(0.0));
    let padding = 50.0;
    // setupGraphViewbox expands the BBox by `padding` on every side and
    // uses the SVG's natural bounds (starting at 0,0 because node Y's
    // are positive).
    l.viewbox = [
        -padding,
        -padding,
        final_w + 2.0 * padding,
        final_h + 2.0 * padding,
    ];
    l.max_width_px = final_w + 2.0 * padding;
    Ok(l)
}

fn layout_td(
    d: &TimelineDiagram,
    font_family_css: String,
    font_size_css: String,
    font_size_px: f64,
) -> Result<TimelineLayout> {
    let mut l = TimelineLayout {
        direction: TimelineDirection::TD,
        font_family_css: font_family_css.clone(),
        font_size_css,
        font_size_px,
        ..TimelineLayout::default()
    };
    l.has_title = d.meta.title.is_some();
    l.title_text = d.meta.title.clone().unwrap_or_default();

    // Vertical layout constants.
    let node_total_width = TD_NODE_WIDTH + 2.0 * TD_NODE_PADDING;
    let event_total_width = TD_EVENT_WIDTH + 2.0 * TD_NODE_PADDING;
    let left_width = node_total_width + TD_TASK_AXIS_GAP;
    let right_width = event_total_width + TD_EVENT_AXIS_GAP;
    let section_width = (50.0f64).max(left_width + right_width - 2.0 * TD_NODE_PADDING);

    // Heights.
    let max_section_height = d
        .sections
        .iter()
        .map(|s| {
            node_height(
                s,
                section_width,
                TD_NODE_PADDING,
                &font_family_css,
                font_size_px,
            )
        })
        .fold(0.0f64, f64::max);
    let max_task_height = d
        .tasks
        .iter()
        .map(|t| {
            node_height(
                &t.task,
                TD_NODE_WIDTH,
                TD_NODE_PADDING,
                &font_family_css,
                font_size_px,
            )
        })
        .fold(0.0f64, f64::max);

    let mut max_event_stack_height: f64 = 0.0;
    for t in &d.tasks {
        let mut sum = 0.0;
        for e in &t.events {
            sum += node_height(
                e,
                TD_EVENT_WIDTH,
                TD_NODE_PADDING,
                &font_family_css,
                font_size_px,
            );
        }
        if !t.events.is_empty() {
            sum += (t.events.len() as f64 - 1.0) * TD_EVENT_SPACING;
        }
        if sum > max_event_stack_height {
            max_event_stack_height = sum;
        }
    }

    let task_block_height = max_task_height.max(max_event_stack_height);
    let task_spacing = task_block_height + TD_TASK_VERTICAL_GAP;

    let left_margin = if d.left_margin > 0.0 {
        d.left_margin
    } else {
        LR_LEFT_MARGIN
    };
    let master_x = 50.0 + left_margin;
    let mut master_y = 50.0;
    let content_top_y = master_y;
    let section_begin_x = master_x;
    let axis_x = section_begin_x + left_width;
    let has_sections = !d.sections.is_empty();
    let timeline_x = if has_sections {
        axis_x
    } else {
        master_x + left_width
    };

    let mut node_id = 0usize;
    if has_sections {
        let mut section_index: i32 = 0;
        for sname in &d.sections {
            let tasks_for_section: Vec<_> =
                d.tasks.iter().filter(|t| &t.section == sname).collect();
            let section_x = timeline_x - left_width;
            let sec_h = node_height(
                sname,
                section_width,
                TD_NODE_PADDING,
                &font_family_css,
                font_size_px,
            )
            .max(max_section_height);
            l.nodes.push(LaidNode {
                kind: LaidNodeKind::Section,
                x: section_x,
                y: master_y,
                width: section_width + 2.0 * TD_NODE_PADDING,
                height: sec_h,
                section_index,
                lines: wrap_text(sname, section_width, &font_family_css, font_size_px),
                node_id,
            });
            node_id += 1;

            let task_start_y = master_y + sec_h + TD_SECTION_TASK_GAP;
            let mut task_y = task_start_y;
            for t in &tasks_for_section {
                let th = node_height(
                    &t.task,
                    TD_NODE_WIDTH,
                    TD_NODE_PADDING,
                    &font_family_css,
                    font_size_px,
                )
                .max(max_task_height);
                let task_x = timeline_x - TD_TASK_AXIS_GAP - node_total_width;
                l.nodes.push(LaidNode {
                    kind: LaidNodeKind::Task,
                    x: task_x,
                    y: task_y,
                    width: node_total_width,
                    height: th,
                    section_index,
                    lines: wrap_text(&t.task, TD_NODE_WIDTH, &font_family_css, font_size_px),
                    node_id,
                });
                node_id += 1;

                let events_x = timeline_x + TD_EVENT_AXIS_GAP;
                let mut ey = task_y;
                for e in &t.events {
                    let eh = node_height(
                        e,
                        TD_EVENT_WIDTH,
                        TD_NODE_PADDING,
                        &font_family_css,
                        font_size_px,
                    );
                    l.nodes.push(LaidNode {
                        kind: LaidNodeKind::Event,
                        x: events_x,
                        y: ey,
                        width: event_total_width,
                        height: eh,
                        section_index,
                        lines: wrap_text(e, TD_EVENT_WIDTH, &font_family_css, font_size_px),
                        node_id,
                    });
                    node_id += 1;
                    // dashed line from axis to event.
                    let line_y = ey + eh / 2.0;
                    l.lines.push(LaidLine {
                        x1: timeline_x,
                        y1: line_y,
                        x2: events_x,
                        y2: line_y,
                        stroke_width: 2.0,
                        dashed: true,
                    });
                    ey += eh + TD_EVENT_SPACING;
                }
                task_y += task_spacing;
            }

            let task_count = tasks_for_section.len();
            let section_height =
                sec_h + TD_SECTION_TASK_GAP + task_spacing * task_count.max(1) as f64
                    - if task_count > 0 {
                        TD_TASK_VERTICAL_GAP * 2.0
                    } else {
                        0.0
                    };
            master_y += section_height;
            section_index += 1;
        }
    } else {
        let mut section_index: i32 = -1;
        let mut task_y = master_y;
        for t in &d.tasks {
            let th = node_height(
                &t.task,
                TD_NODE_WIDTH,
                TD_NODE_PADDING,
                &font_family_css,
                font_size_px,
            )
            .max(max_task_height);
            let task_x = timeline_x - TD_TASK_AXIS_GAP - node_total_width;
            l.nodes.push(LaidNode {
                kind: LaidNodeKind::Task,
                x: task_x,
                y: task_y,
                width: node_total_width,
                height: th,
                section_index,
                lines: wrap_text(&t.task, TD_NODE_WIDTH, &font_family_css, font_size_px),
                node_id,
            });
            node_id += 1;

            let events_x = timeline_x + TD_EVENT_AXIS_GAP;
            let mut ey = task_y;
            for e in &t.events {
                let eh = node_height(
                    e,
                    TD_EVENT_WIDTH,
                    TD_NODE_PADDING,
                    &font_family_css,
                    font_size_px,
                );
                l.nodes.push(LaidNode {
                    kind: LaidNodeKind::Event,
                    x: events_x,
                    y: ey,
                    width: event_total_width,
                    height: eh,
                    section_index,
                    lines: wrap_text(e, TD_EVENT_WIDTH, &font_family_css, font_size_px),
                    node_id,
                });
                node_id += 1;
                let line_y = ey + eh / 2.0;
                l.lines.push(LaidLine {
                    x1: timeline_x,
                    y1: line_y,
                    x2: events_x,
                    y2: line_y,
                    stroke_width: 2.0,
                    dashed: true,
                });
                ey += eh + TD_EVENT_SPACING;
            }
            task_y += task_spacing;
            if !d.disable_multicolor {
                section_index += 1;
            }
        }
    }

    // BBox.
    let (bbox_x, bbox_y, bbox_w, bbox_h) = compute_bbox(&l.nodes, &l.lines);

    // Axis placement (vertical line at `timeline_x`).
    let arrow_top_offset = font_size_px * 2.0;
    let arrow_bottom_padding = font_size_px * 0.5 + 20.0;
    let axis_y1 = content_top_y - arrow_top_offset;
    let axis_y2 = bbox_y + bbox_h + arrow_bottom_padding;

    if l.has_title {
        let title_x = bbox_w / 2.0 - left_margin;
        l.title_xy = Some((title_x, 20.0));
    }

    l.axis = Some(LaidLine {
        x1: timeline_x,
        y1: axis_y1,
        x2: timeline_x,
        y2: axis_y2,
        stroke_width: 4.0,
        dashed: false,
    });

    let _ = section_begin_x;
    let _ = (bbox_x, bbox_y);

    let final_w = (bbox_x + bbox_w).max(timeline_x);
    let final_h = (bbox_y + bbox_h).max(axis_y2);
    let padding = 50.0;
    l.viewbox = [
        -padding,
        -padding,
        final_w + 2.0 * padding,
        final_h + 2.0 * padding,
    ];
    l.max_width_px = final_w + 2.0 * padding;
    Ok(l)
}

pub(crate) fn compute_bbox(nodes: &[LaidNode], _lines: &[LaidLine]) -> (f64, f64, f64, f64) {
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for n in nodes {
        x0 = x0.min(n.x);
        y0 = y0.min(n.y);
        x1 = x1.max(n.x + n.width);
        y1 = y1.max(n.y + n.height);
    }
    if !x0.is_finite() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    (x0, y0, x1 - x0, y1 - y0)
}

pub(crate) fn parse_px(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    let num = trimmed.trim_end_matches("px");
    num.parse::<f64>().ok()
}

/// Emulate mermaid's d3 `wrap` helper: split on `/(\s+|<br>)/`, walk
/// the pieces in original order joining with single spaces, and when
/// the rendered width exceeds `width` start a new line with the
/// previous word. Returns the list of trimmed per-line strings that
/// the renderer will emit as consecutive `<tspan>`s.
pub(crate) fn wrap_text(
    text: &str,
    width: f64,
    font_family: &str,
    font_size_px: f64,
) -> Vec<String> {
    // Step 1: tokenise preserving whitespace runs + `<br>` markers.
    let tokens = tokenise(text);
    let mut out_lines: Vec<String> = Vec::new();
    let mut line: Vec<String> = Vec::new();

    for token in tokens {
        if token == "<br>" {
            out_lines.push(line.join(" ").trim().to_string());
            line.clear();
            continue;
        }
        line.push(token.clone());
        let joined = line.join(" ");
        let w = text_width(&joined, font_family, font_size_px, false, false);
        if w > width {
            // Pop the last word, flush the current line, start fresh.
            line.pop();
            out_lines.push(line.join(" ").trim().to_string());
            line.clear();
            line.push(token);
        }
    }
    if !line.is_empty() || out_lines.is_empty() {
        out_lines.push(line.join(" ").trim().to_string());
    }
    // Upstream behaviour on a `<br>` as the FIRST token: the initial
    // empty-string tspan comes back as `""`; keep it for parity.
    out_lines
}

/// Split `text` on `/(\s+|<br\s*\/?>)/` while preserving separators as
/// distinct tokens, mirroring JavaScript's `text.split(re)` with a
/// capture group (which keeps the separators in the output array).
fn tokenise(text: &str) -> Vec<String> {
    // Normalise `<br>` / `<br/>` / `<BR>` to the upstream canonical
    // form the split regex treats.
    let mut out: Vec<String> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut buf = String::new();
    while i < bytes.len() {
        // Match <br...>
        if bytes[i] == b'<'
            && i + 3 <= bytes.len()
            && (bytes[i + 1] == b'b' || bytes[i + 1] == b'B')
            && (bytes[i + 2] == b'r' || bytes[i + 2] == b'R')
        {
            // Walk to `>`
            let mut j = i + 3;
            while j < bytes.len() && bytes[j] != b'>' {
                j += 1;
            }
            if j < bytes.len() {
                if !buf.is_empty() {
                    out.push(std::mem::take(&mut buf));
                }
                out.push("<br>".to_string());
                i = j + 1;
                continue;
            }
        }
        let c = text[i..].chars().next().unwrap();
        if c.is_whitespace() {
            // Consume a run of whitespace.
            if !buf.is_empty() {
                out.push(std::mem::take(&mut buf));
            }
            let mut ws = String::new();
            let mut k = i;
            while k < bytes.len() {
                let cc = text[k..].chars().next().unwrap();
                if cc.is_whitespace() {
                    ws.push(cc);
                    k += cc.len_utf8();
                } else {
                    break;
                }
            }
            out.push(ws);
            i = k;
            continue;
        }
        buf.push(c);
        i += c.len_utf8();
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

/// Height a wrap'd node ends up with, matching
/// `svgDraw.getVirtualNodeHeight`:
///   `bbox.height + fontSize * 1.1 * 0.5 + node.padding`
/// where `bbox.height = font_line_height * n_lines`.
pub(crate) fn node_height(
    text: &str,
    width: f64,
    padding: f64,
    font_family: &str,
    font_size_px: f64,
) -> f64 {
    let lines = wrap_text(text, width, font_family, font_size_px);
    // jsdom's measureTextBlock reports `lineHeight * n_lines` for the
    // bbox height. mermaid adds `fontSize * 1.1 * 0.5 + padding` to it.
    let lh = crate::font_metrics::line_height(font_family, font_size_px, false, false);
    lh * lines.len().max(1) as f64 + font_size_px * 1.1 * 0.5 + padding
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_short_text_single_line() {
        let lines = wrap_text(
            "2002",
            150.0,
            "\"trebuchet ms\", verdana, arial, sans-serif",
            16.0,
        );
        assert_eq!(lines, vec!["2002".to_string()]);
    }

    #[test]
    fn wrap_double_spaces_mermaid_quirk() {
        // The d3 wrap helper inserts an extra space for whitespace
        // tokens, producing `Industry   1.0` on output.
        let lines = wrap_text(
            "Industry 1.0",
            150.0,
            "\"trebuchet ms\", verdana, arial, sans-serif",
            16.0,
        );
        assert_eq!(lines, vec!["Industry   1.0".to_string()]);
    }
}
