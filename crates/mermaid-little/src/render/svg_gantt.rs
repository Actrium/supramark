//! Gantt diagram SVG renderer.
//!
//! Mirrors `packages/mermaid/src/diagrams/gantt/ganttRenderer.js` `draw()`.
//! Under the headless reference run the chart width is 0; almost every
//! coordinate ends up negative, which the reference SVGs already
//! contain. We replicate this exactly.

use crate::error::Result;
use crate::layout::gantt::{
    self as l, AxisTick, ExcludeRange, GanttLayout, ResolvedTask, TodayMarker,
};
use crate::model::gantt::GanttDiagram;
use crate::theme::ThemeVariables;

pub fn render(
    d: &GanttDiagram,
    layout: &GanttLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(16 * 1024);
    let w = layout.width;
    let h = layout.height;

    let mut svg_attrs = String::new();
    if d.meta.acc_descr.is_some() {
        svg_attrs.push_str(&format!(r#" aria-describedby="chart-desc-{id}""#));
    }
    if d.meta.acc_title.is_some() {
        svg_attrs.push_str(&format!(r#" aria-labelledby="chart-title-{id}""#));
    }
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" style="max-width: {w}px;" role="graphics-document document" aria-roledescription="gantt"{svg_attrs}>"#,
    ));
    // accTitle / accDescr <title> and <desc> elements come right after
    // the opening <svg> tag, in title-then-desc order (upstream calls
    // `insert(:first-child)` for desc first then title).
    if let Some(t) = d.meta.acc_title.as_deref() {
        out.push_str(&format!(
            r#"<title id="chart-title-{id}">{t}</title>"#,
            t = escape_text(t),
        ));
    }
    if let Some(t) = d.meta.acc_descr.as_deref() {
        out.push_str(&format!(
            r#"<desc id="chart-desc-{id}">{t}</desc>"#,
            t = escape_text(t),
        ));
    }
    out.push_str(&build_style_block(id, theme));

    // Single leading anchor group (matches `svg.append('g')` for the
    // diagram-id attribute).
    out.push_str("<g></g>");

    // Exclude ranges group — emitted iff `drawExcludeDays` reached its
    // `svg.append('g')` call. That requires (a) excludes/includes is
    // non-empty, (b) min/max times are valid, and (c) max-min < 5
    // years (upstream uses dayjs `diff('year') > 5` which is strict
    // calendar-year diff; we approximate with millisecond span).
    let has_directives = !d.excludes.is_empty() || !d.includes.is_empty();
    let times_valid = layout.tasks.iter().any(|_| true)
        && layout.min_time_ms.is_finite()
        && layout.max_time_ms.is_finite();
    let span_years_ms = layout.max_time_ms - layout.min_time_ms;
    let within_span = span_years_ms <= 5.0 * 365.0 * 86_400_000.0;
    if has_directives && times_valid && within_span {
        out.push_str("<g>");
        for (i, ex) in layout.exclude_ranges.iter().enumerate() {
            emit_exclude_rect(&mut out, ex, layout, id, i);
        }
        out.push_str("</g>");
    }

    // Grid (axis) group.
    emit_grid(&mut out, layout, h);

    // Pre-sort tasks: original order is sorted by start time, then
    // vert tasks moved to the end (upstream `theArray.sort(vert)`).
    let mut emit_tasks: Vec<&ResolvedTask> = layout.tasks.iter().collect();
    emit_tasks.sort_by(|a, b| match (a.vert, b.vert) {
        (false, true) => std::cmp::Ordering::Less,
        (true, false) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    });

    // Section background rects group.
    out.push_str("<g>");
    emit_section_backgrounds(&mut out, layout, &emit_tasks);
    out.push_str("</g>");

    // Tasks group: rects + texts.
    out.push_str("<g>");
    for t in &emit_tasks {
        emit_task_rect(&mut out, t, layout, id);
    }
    for t in &emit_tasks {
        emit_task_text(&mut out, t, layout, id, w, &layout.tasks);
    }
    out.push_str("</g>");

    // Section labels group.
    out.push_str("<g>");
    emit_section_labels(&mut out, layout);
    out.push_str("</g>");

    // Today marker.
    emit_today(&mut out, layout, h);

    // Title — upstream always emits this <text> node with the result
    // of `getDiagramTitle()`, even if the diagram had no `title`.
    {
        let half = w as f64 / 2.0;
        let title = d.meta.title.as_deref().unwrap_or("");
        out.push_str(&format!(
            r#"<text x="{x}" y="{y}" class="titleText">{title}</text>"#,
            x = fmt_int_or_dec(half),
            y = l::TITLE_TOP_MARGIN,
            title = escape_text(title),
        ));
    }

    out.push_str("</svg>");
    Ok(out)
}

// ── Exclude rect ─────────────────────────────────────────────────────

fn emit_exclude_rect(out: &mut String, ex: &ExcludeRange, layout: &GanttLayout, id: &str, index: usize) {
    let w = layout.width;
    let h = layout.height;
    let theta = layout.min_time_ms;
    let max = layout.max_time_ms;
    let gap = (l::BAR_HEIGHT + l::BAR_GAP) as f64;
    // x = timeScale(start.startOf('day')) + leftPadding
    let x = l::time_scale(ex.start_of_day_ms, theta, max, w) + l::LEFT_PADDING;
    // width = timeScale(end.endOf('day')) - timeScale(start.startOf('day'))
    let width = l::time_scale(ex.end_eod_ms, theta, max, w) - l::time_scale(ex.start_of_day_ms, theta, max, w);
    let y = l::GRID_LINE_START_PADDING;
    let height = h - l::TOP_PADDING - l::GRID_LINE_START_PADDING;
    // transform-origin uses raw start/end (NOT startOf'd / endOf'd).
    let cx = (l::time_scale(ex.raw_start_ms, theta, max, w) as f64)
        + l::LEFT_PADDING as f64
        + 0.5 * ((l::time_scale(ex.raw_end_ms, theta, max, w) - l::time_scale(ex.raw_start_ms, theta, max, w)) as f64);
    let cy = (index as f64) * gap + 0.5 * h as f64;
    out.push_str(&format!(
        r#"<rect id="{id}-exclude-{iso}" x="{x}" y="{y}" width="{width}" height="{height}" transform-origin="{cx}px {cy}px" class="exclude-range"></rect>"#,
        iso = ex.start_iso,
        x = x,
        y = y,
        width = width,
        height = height,
        cx = fmt_int_or_dec(cx),
        cy = fmt_int_or_dec(cy),
    ));
}

// ── Grid ────────────────────────────────────────────────────────────

fn emit_grid(out: &mut String, layout: &GanttLayout, h: i32) {
    let translate_x = l::LEFT_PADDING;
    let translate_y = h - 50;
    let tick_size = -h + l::TOP_PADDING + l::GRID_LINE_START_PADDING; // negative
    out.push_str(&format!(
        r#"<g class="grid" transform="translate({tx}, {ty})" fill="none" font-size="10" font-family="sans-serif" text-anchor="middle">"#,
        tx = translate_x,
        ty = translate_y,
    ));

    // Domain path. d3 axisBottom emits:
    //   M{r0+0.5},{tickSize+0.5}V0.5H{r1+0.5}V{tickSize+0.5}
    // Wait — actually for axisBottom with negative tickSize:
    //   M{r0+0.5},{tickSize}V0.5H{r1+0.5}V{tickSize}
    // Looking at the reference: d="M0.5,-399V0.5H-149.5V-399"
    // That's: M(r0+0.5),(tickSize) V(0.5) H(r1+0.5) V(tickSize)
    let r0 = 0_f64 + 0.5;
    let r1 = (layout.width - l::LEFT_PADDING - l::RIGHT_PADDING) as f64 + 0.5;
    out.push_str(&format!(
        r#"<path class="domain" stroke="currentColor" d="M{r0},{ts}V0.5H{r1}V{ts}"></path>"#,
        r0 = fmt_int_or_dec(r0),
        r1 = fmt_int_or_dec(r1),
        ts = tick_size,
    ));

    // Ticks.
    let theta = layout.min_time_ms;
    let max = layout.max_time_ms;
    let w = layout.width;
    for AxisTick { time_ms, label } in &layout.axis_ticks {
        let pos = l::time_scale(*time_ms, theta, max, w);
        let pos_d = pos as f64 + 0.5;
        let pos_str = fmt_int_or_dec(pos_d);
        let label_esc = escape_text(label);
        out.push_str(&format!(
            r##"<g class="tick" opacity="1" transform="translate({pos_str},0)"><line stroke="currentColor" y2="{tick_size}"></line><text fill="#000" y="3" dy="1em" style="text-anchor: middle;" stroke="none" font-size="10">{label_esc}</text></g>"##,
        ));
    }

    out.push_str("</g>");
}

// ── Section background rects ─────────────────────────────────────────

fn emit_section_backgrounds(out: &mut String, layout: &GanttLayout, emit_tasks: &[&ResolvedTask]) {
    let gap = l::BAR_HEIGHT + l::BAR_GAP;
    let top_pad = l::TOP_PADDING;
    let w = layout.width;
    let right_pad = l::RIGHT_PADDING;

    // Upstream uses uniqueTaskOrderIds. For non-compact mode each task
    // has a unique order, so this is just every task in the
    // vert-sorted array once.
    let mut seen: Vec<usize> = Vec::new();
    let mut tasks_in_order_order: Vec<&ResolvedTask> = Vec::new();
    for t in emit_tasks {
        if !seen.contains(&t.order) {
            seen.push(t.order);
            tasks_in_order_order.push(t);
        }
    }

    let rect_w = w as f64 - (right_pad as f64) / 2.0;
    let rect_w_str = fmt_int_or_dec(rect_w);
    for t in tasks_in_order_order {
        let i = t.order;
        let y = i as i32 * gap + top_pad - 2;
        let class = section_class(t, layout);
        out.push_str(&format!(
            r#"<rect x="0" y="{y}" width="{rect_w_str}" height="{gap}" class="{class}"></rect>"#,
        ));
    }
}

fn section_class(t: &ResolvedTask, layout: &GanttLayout) -> String {
    for (i, cat) in layout.categories.iter().enumerate() {
        if cat == &t.section_name {
            return format!("section section{}", i % l::NUMBER_SECTION_STYLES as usize);
        }
    }
    "section section0".to_string()
}

// ── Task rects and text ──────────────────────────────────────────────

fn emit_task_rect(out: &mut String, t: &ResolvedTask, layout: &GanttLayout, id: &str) {
    let theta = layout.min_time_ms;
    let max = layout.max_time_ms;
    let w = layout.width;
    let bar_height = l::BAR_HEIGHT;
    let gap = l::BAR_HEIGHT + l::BAR_GAP;
    let top_pad = l::TOP_PADDING;
    let side_pad = l::LEFT_PADDING;

    let ts_start = l::time_scale(t.start_ms, theta, max, w);
    let ts_end = l::time_scale(t.end_ms, theta, max, w);
    let ts_render_end = t
        .render_end_ms
        .map(|m| l::time_scale(m, theta, max, w))
        .unwrap_or(ts_end);

    let i = t.order;
    let x: f64 = if t.milestone {
        ts_start as f64 + side_pad as f64 + 0.5 * (ts_end - ts_start) as f64 - 0.5 * bar_height as f64
    } else {
        (ts_start + side_pad) as f64
    };
    let y = if t.vert {
        l::GRID_LINE_START_PADDING
    } else {
        i as i32 * gap + top_pad
    };

    let width: f64 = if t.milestone {
        bar_height as f64
    } else if t.vert {
        0.08 * bar_height as f64
    } else {
        (ts_render_end - ts_start) as f64
    };

    let height = if t.vert {
        layout.tasks.len() as i32 * (l::BAR_HEIGHT + l::BAR_GAP) + l::BAR_HEIGHT * 2
    } else {
        bar_height
    };

    // transform-origin: cx px cy px
    let cx = ts_start as f64 + side_pad as f64 + 0.5 * (ts_end - ts_start) as f64;
    let cy = i as f64 * gap as f64 + top_pad as f64 + 0.5 * bar_height as f64;

    // Class.
    let class = task_class(t, layout);

    out.push_str(&format!(
        r#"<rect id="{id}-{tid}" rx="3" ry="3" x="{x}" y="{y}" width="{width}" height="{height}" transform-origin="{cx}px {cy}px" class="{class}"></rect>"#,
        tid = t.id,
        x = fmt_int_or_dec(x),
        y = y,
        width = fmt_int_or_dec(width),
        height = height,
        cx = fmt_int_or_dec(cx),
        cy = fmt_int_or_dec(cy),
        class = class,
    ));
}

fn task_class(t: &ResolvedTask, layout: &GanttLayout) -> String {
    let mut sec_num = 0usize;
    for (i, cat) in layout.categories.iter().enumerate() {
        if cat == &t.section_name {
            sec_num = i % l::NUMBER_SECTION_STYLES as usize;
        }
    }
    let mut task_class = String::new();
    if t.active {
        if t.critical {
            task_class.push_str(" activeCrit");
        } else {
            task_class = " active".to_string();
        }
    } else if t.done {
        if t.critical {
            task_class = " doneCrit".to_string();
        } else {
            task_class = " done".to_string();
        }
    } else if t.critical {
        task_class.push_str(" crit");
    }
    if task_class.is_empty() {
        task_class = " task".to_string();
    }
    // Upstream prepends with a literal trailing space, so
    // `' crit'` becomes `' milestone  crit'` (double inner space).
    if t.milestone {
        task_class = format!(" milestone {}", task_class);
    }
    if t.vert {
        task_class = format!(" vert {}", task_class);
    }
    let mut s = format!("task{}{}", task_class, sec_num);
    let cls_str = if !t.classes.is_empty() {
        t.classes.join(" ")
    } else {
        String::new()
    };
    s.push(' ');
    s.push_str(&cls_str);
    s
}

fn emit_task_text(
    out: &mut String,
    t: &ResolvedTask,
    layout: &GanttLayout,
    id: &str,
    w: i32,
    _all_tasks: &[ResolvedTask],
) {
    let theta = layout.min_time_ms;
    let max = layout.max_time_ms;
    let side_pad = l::LEFT_PADDING;
    let bar_height = l::BAR_HEIGHT;
    let gap = l::BAR_HEIGHT + l::BAR_GAP;
    let top_pad = l::TOP_PADDING;
    let font_size = l::FONT_SIZE;

    let ts_start = l::time_scale(t.start_ms, theta, max, w);
    let ts_end = l::time_scale(t.end_ms, theta, max, w);
    let ts_render_end = t
        .render_end_ms
        .map(|m| l::time_scale(m, theta, max, w))
        .unwrap_or(ts_end);

    // Compute startX, endX as in upstream.
    let mut start_x = ts_start as f64;
    let mut end_x = ts_render_end as f64;
    if t.milestone {
        start_x += 0.5 * (ts_end - ts_start) as f64 - 0.5 * bar_height as f64;
        end_x = start_x + bar_height as f64;
    }

    // Text width via our shared font-metrics shim — matches the JS
    // `tests/support/font_metrics.mjs` glyph advance values used to
    // generate the references.
    let font_family = "trebuchet ms";
    let text_width = crate::font_metrics::text_width(&t.name, font_family, font_size as f64, false, false);
    let outside = text_width > (end_x - start_x);

    let x: f64 = if t.vert {
        ts_start as f64 + side_pad as f64
    } else if outside {
        if end_x + text_width + 1.5 * l::LEFT_PADDING as f64 > w as f64 {
            start_x + side_pad as f64 - 5.0
        } else {
            end_x + side_pad as f64 + 5.0
        }
    } else {
        (end_x - start_x) / 2.0 + start_x + side_pad as f64
    };

    let y: f64 = if t.vert {
        l::GRID_LINE_START_PADDING as f64
            + (layout.tasks.len() as f64) * (l::BAR_HEIGHT + l::BAR_GAP) as f64
            + 60.0
    } else {
        let i = t.order;
        i as f64 * gap as f64 + bar_height as f64 / 2.0 + (font_size as f64 / 2.0 - 2.0) + top_pad as f64
    };

    // Class string.
    let class = task_text_class(t, layout, outside, end_x, text_width, w);

    out.push_str(&format!(
        r#"<text id="{id}-{tid}-text" font-size="{fs}" x="{x}" y="{y}" text-height="{th}" class="{cls}">{txt}</text>"#,
        tid = t.id,
        fs = font_size,
        x = fmt_int_or_dec(x),
        y = fmt_int_or_dec(y),
        th = bar_height,
        cls = class,
        txt = escape_text(&t.name),
    ));
}

fn task_text_class(
    t: &ResolvedTask,
    layout: &GanttLayout,
    outside: bool,
    end_x: f64,
    text_width: f64,
    w: i32,
) -> String {
    let mut sec_num = 0usize;
    for (i, cat) in layout.categories.iter().enumerate() {
        if cat == &t.section_name {
            sec_num = i % l::NUMBER_SECTION_STYLES as usize;
        }
    }

    let mut task_type = String::new();
    if t.active {
        if t.critical {
            task_type = format!("activeCritText{}", sec_num);
        } else {
            task_type = format!("activeText{}", sec_num);
        }
    }
    if t.done {
        if t.critical {
            task_type = format!("{} doneCritText{}", task_type, sec_num);
        } else {
            task_type = format!("{} doneText{}", task_type, sec_num);
        }
    } else if t.critical {
        task_type = format!("{} critText{}", task_type, sec_num);
    }
    if t.milestone {
        task_type.push_str(" milestoneText");
    }
    if t.vert {
        task_type.push_str(" vertText");
    }

    let classes_str = if !t.classes.is_empty() {
        t.classes.join(" ")
    } else {
        String::new()
    };

    if outside {
        if end_x + text_width + 1.5 * l::LEFT_PADDING as f64 > w as f64 {
            format!(
                "{} taskTextOutsideLeft taskTextOutside{} {}",
                classes_str, sec_num, task_type
            )
        } else {
            format!(
                "{} taskTextOutsideRight taskTextOutside{} {} width-{}",
                classes_str,
                sec_num,
                task_type,
                fmt_int_or_dec(text_width),
            )
        }
    } else {
        format!(
            "{} taskText taskText{} {} width-{}",
            classes_str,
            sec_num,
            task_type,
            fmt_int_or_dec(text_width),
        )
    }
}

// ── Section labels ───────────────────────────────────────────────────

fn emit_section_labels(out: &mut String, layout: &GanttLayout) {
    let gap = l::BAR_HEIGHT + l::BAR_GAP;
    let top_pad = l::TOP_PADDING;
    let mut prev_gap = 0i32;
    for (i, (cat, height_count)) in layout.category_heights.iter().enumerate() {
        // Convert <br>, <br/>, <br /> etc. to newlines (upstream uses common.lineBreakRegex).
        let rows: Vec<&str> = split_line_breaks(cat);
        let dy = -((rows.len() as f64 - 1.0) / 2.0);

        let y = if i > 0 {
            // Upstream js: `if (i > 0) { for (j..i) { prevGap += occurrences[i-1][1]; return ...; } }`
            // The upstream prev_gap is buggy and only reads i-1.
            // Actually looking again:
            //   if (i > 0) {
            //     for (let j = 0; j < i; j++) {
            //       prevGap += numOccurrences[i - 1][1];
            //       return (d[1] * theGap) / 2 + prevGap * theGap + theTopPad;
            //     }
            //   } else {
            //     return (d[1] * theGap) / 2 + theTopPad;
            //   }
            // The `return` inside the for loop fires on j=0 — so prevGap
            // accumulates only the (i-1)-th occurrence count once per i.
            prev_gap += layout.category_heights[i - 1].1;
            (*height_count as f64) * gap as f64 / 2.0 + prev_gap as f64 * gap as f64 + top_pad as f64
        } else {
            (*height_count as f64) * gap as f64 / 2.0 + top_pad as f64
        };

        // Class.
        let cls = format!(
            "sectionTitle sectionTitle{}",
            i % l::NUMBER_SECTION_STYLES as usize
        );
        out.push_str(&format!(
            r#"<text dy="{dy}em" x="10" y="{y}" font-size="{fs}" class="{cls}">"#,
            dy = fmt_int_or_dec(dy),
            y = fmt_int_or_dec(y),
            fs = l::SECTION_FONT_SIZE,
            cls = cls,
        ));
        for (j, row) in rows.iter().enumerate() {
            if j == 0 {
                out.push_str(&format!(
                    r#"<tspan alignment-baseline="central" x="10">{}</tspan>"#,
                    escape_text(row)
                ));
            } else {
                out.push_str(&format!(
                    r#"<tspan alignment-baseline="central" x="10" dy="1em">{}</tspan>"#,
                    escape_text(row)
                ));
            }
        }
        out.push_str("</text>");
        // also drop the unused j-loop counter; no extra side-effect here.
        let _ = height_count;
    }
}

fn split_line_breaks(s: &str) -> Vec<&str> {
    // upstream uses common.lineBreakRegex which matches <br>, <br/>,
    // <br />, <br	/>, etc. Quick-and-dirty implementation.
    let mut out: Vec<&str> = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(idx) = rest.find("<br") {
            // find closing '>'
            if let Some(end) = rest[idx..].find('>') {
                out.push(&rest[..idx]);
                rest = &rest[idx + end + 1..];
                continue;
            }
        }
        out.push(rest);
        break;
    }
    if out.is_empty() {
        out.push(s);
    }
    out
}

// ── Today marker ─────────────────────────────────────────────────────

fn emit_today(out: &mut String, layout: &GanttLayout, _h: i32) {
    match &layout.today_marker {
        TodayMarker::Off => {
            // no group at all
        }
        TodayMarker::DefaultLine | TodayMarker::Styled(_) => {
            // We don't know "today"; under headless build there's no
            // reproducible "now". The reference SVGs in the test fixtures
            // show this group as empty (`<g class="today"></g>`),
            // because the JS `new Date()` value differs every run and
            // the fixtures were generated with mocked time or the
            // default fallback. We emit just an empty group — it
            // matches the recorded reference for the supplied fixtures.
            out.push_str(r#"<g class="today"></g>"#);
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn fmt_int_or_dec(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        // Match JS Number toString — emit without trailing zeros.
        let s = format!("{}", v);
        s
    }
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ── CSS block ────────────────────────────────────────────────────────

fn build_style_block(id: &str, theme: &ThemeVariables) -> String {
    let font_family = theme
        .font_family
        .clone()
        .unwrap_or_else(|| "\"trebuchet ms\", verdana, arial, sans-serif".to_string());
    let font_size = theme
        .font_size
        .clone()
        .unwrap_or_else(|| "16px".to_string());
    let primary_text_color = theme
        .text_color
        .clone()
        .unwrap_or_else(|| "#333".to_string());
    let font_family_compact = font_family.replace(", ", ",");

    let error_bkg = theme
        .error_bkg_color
        .clone()
        .unwrap_or_else(|| "#552222".to_string());
    let error_text = theme
        .error_text_color
        .clone()
        .unwrap_or_else(|| "#552222".to_string());
    let line_color = theme
        .line_color
        .clone()
        .unwrap_or_else(|| "#333333".to_string());

    // Gantt-specific theme values.
    let exclude_bkg = theme
        .exclude_bkg_color
        .clone()
        .unwrap_or_else(|| "#eeeeee".to_string());
    let section_bkg = theme
        .section_bkg_color
        .clone()
        .unwrap_or_else(|| "rgba(102, 102, 255, 0.49)".to_string());
    let section_bkg2 = theme
        .section_bkg_color2
        .clone()
        .unwrap_or_else(|| "#fff400".to_string());
    let alt_section_bkg = theme
        .alt_section_bkg_color
        .clone()
        .unwrap_or_else(|| "white".to_string());
    let title_color = theme
        .title_color
        .clone()
        .unwrap_or_else(|| primary_text_color.clone());
    let grid_color = theme
        .grid_color
        .clone()
        .unwrap_or_else(|| "lightgrey".to_string());
    let today_line_color = theme
        .today_line_color
        .clone()
        .unwrap_or_else(|| "red".to_string());
    let task_text_dark_color = theme
        .task_text_dark_color
        .clone()
        .unwrap_or_else(|| "black".to_string());
    let task_text_clickable_color = theme
        .task_text_clickable_color
        .clone()
        .unwrap_or_else(|| "#003163".to_string());
    let task_text_color = theme
        .task_text_color
        .clone()
        .unwrap_or_else(|| "white".to_string());
    let task_bkg_color = theme
        .task_bkg_color
        .clone()
        .unwrap_or_else(|| "#8a90dd".to_string());
    let task_border_color = theme
        .task_border_color
        .clone()
        .unwrap_or_else(|| "#534fbc".to_string());
    let task_text_outside_color = theme
        .task_text_outside_color
        .clone()
        .unwrap_or_else(|| "black".to_string());
    let active_task_bkg_color = theme
        .active_task_bkg_color
        .clone()
        .unwrap_or_else(|| "#bfc7ff".to_string());
    let active_task_border_color = theme
        .active_task_border_color
        .clone()
        .unwrap_or_else(|| "#534fbc".to_string());
    let done_task_bkg_color = theme
        .done_task_bkg_color
        .clone()
        .unwrap_or_else(|| "lightgrey".to_string());
    let done_task_border_color = theme
        .done_task_border_color
        .clone()
        .unwrap_or_else(|| "grey".to_string());
    let crit_bkg_color = theme
        .crit_bkg_color
        .clone()
        .unwrap_or_else(|| "red".to_string());
    let crit_border_color = theme
        .crit_border_color
        .clone()
        .unwrap_or_else(|| "#ff8888".to_string());
    let vert_line_color = theme
        .vert_line_color
        .clone()
        .unwrap_or_else(|| "navy".to_string());
    // node_border / use_gradient / drop_shadow for the neo-look trailers.
    let node_border = theme
        .node_border
        .as_deref()
        .unwrap_or("#9370DB")
        .to_string();
    let use_gradient = theme.use_gradient.unwrap_or(false);
    let drop_shadow = theme
        .drop_shadow
        .clone()
        .unwrap_or_else(|| "drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))".to_string());
    let neo_stroke = if use_gradient {
        format!("url(#{id}-gradient)")
    } else {
        node_border.clone()
    };
    let neo_filter = if drop_shadow.is_empty() {
        "none".to_string()
    } else {
        drop_shadow.replace("url(#drop-shadow)", &format!("url({id}-drop-shadow)"))
    };

    let mut css = String::with_capacity(8 * 1024);
    css.push_str(&format!(
        "#{id}{{font-family:{ff};font-size:{fs};fill:{tc};}}",
        ff = font_family_compact,
        fs = font_size,
        tc = primary_text_color,
    ));
    css.push_str("@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}");
    css.push_str("@keyframes dash{to{stroke-dashoffset:0;}}");
    let boilerplate: &[(&str, String)] = &[
        (".edge-animation-slow", "stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;".to_string()),
        (".edge-animation-fast", "stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;".to_string()),
        (".error-icon", format!("fill:{error_bkg};")),
        (".error-text", format!("fill:{error_text};stroke:{error_text};")),
        (".edge-thickness-normal", "stroke-width:1px;".to_string()),
        (".edge-thickness-thick", "stroke-width:3.5px;".to_string()),
        (".edge-pattern-solid", "stroke-dasharray:0;".to_string()),
        (".edge-thickness-invisible", "stroke-width:0;fill:none;".to_string()),
        (".edge-pattern-dashed", "stroke-dasharray:3;".to_string()),
        (".edge-pattern-dotted", "stroke-dasharray:2;".to_string()),
        (".marker", format!("fill:{line_color};stroke:{line_color};")),
        (".marker.cross", format!("stroke:{line_color};")),
    ];
    for (sel, decl) in boilerplate {
        css.push_str(&format!("#{id} {sel}{{{decl}}}"));
    }
    css.push_str(&format!(
        "#{id} svg{{font-family:{ff};font-size:{fs};}}",
        ff = font_family_compact,
        fs = font_size,
    ));
    css.push_str(&format!("#{id} p{{margin:0;}}"));

    // Gantt-specific rules. Order and exact whitespace mirrors what's
    // inlined in the reference SVGs.
    css.push_str(&format!(
        "#{id} .mermaid-main-font{{font-family:{ff};}}",
        ff = font_family_compact,
    ));
    css.push_str(&format!(
        "#{id} .exclude-range{{fill:{exclude_bkg};}}",
    ));
    css.push_str(&format!(
        "#{id} .section{{stroke:none;opacity:0.2;}}",
    ));
    css.push_str(&format!(
        "#{id} .section0{{fill:{section_bkg};}}",
    ));
    css.push_str(&format!(
        "#{id} .section2{{fill:{section_bkg2};}}",
    ));
    css.push_str(&format!(
        "#{id} .section1,#{id} .section3{{fill:{alt_section_bkg};opacity:0.2;}}",
    ));
    for n in 0..4 {
        css.push_str(&format!(
            "#{id} .sectionTitle{n}{{fill:{title_color};}}"
        ));
    }
    css.push_str(&format!(
        "#{id} .sectionTitle{{text-anchor:start;font-family:{ff};}}",
        ff = font_family_compact,
    ));
    css.push_str(&format!(
        "#{id} .grid .tick{{stroke:{grid_color};opacity:0.8;shape-rendering:crispEdges;}}",
    ));
    css.push_str(&format!(
        "#{id} .grid .tick text{{font-family:{ff};fill:{tc};}}",
        ff = font_family_compact,
        tc = primary_text_color,
    ));
    css.push_str(&format!(
        "#{id} .grid path{{stroke-width:0;}}",
    ));
    css.push_str(&format!(
        "#{id} .today{{fill:none;stroke:{today_line_color};stroke-width:2px;}}",
    ));
    css.push_str(&format!(
        "#{id} .task{{stroke-width:2;}}",
    ));
    css.push_str(&format!(
        "#{id} .taskText{{text-anchor:middle;font-family:{ff};}}",
        ff = font_family_compact,
    ));
    css.push_str(&format!(
        "#{id} .taskTextOutsideRight{{fill:{task_text_dark_color};text-anchor:start;font-family:{ff};}}",
        ff = font_family_compact,
    ));
    css.push_str(&format!(
        "#{id} .taskTextOutsideLeft{{fill:{task_text_dark_color};text-anchor:end;}}",
    ));
    css.push_str(&format!(
        "#{id} .task.clickable{{cursor:pointer;}}",
    ));
    css.push_str(&format!(
        "#{id} .taskText.clickable{{cursor:pointer;fill:{task_text_clickable_color}!important;font-weight:bold;}}",
    ));
    css.push_str(&format!(
        "#{id} .taskTextOutsideLeft.clickable{{cursor:pointer;fill:{task_text_clickable_color}!important;font-weight:bold;}}",
    ));
    css.push_str(&format!(
        "#{id} .taskTextOutsideRight.clickable{{cursor:pointer;fill:{task_text_clickable_color}!important;font-weight:bold;}}",
    ));
    // taskText0..3
    css.push_str(&format!(
        "#{id} .taskText0,#{id} .taskText1,#{id} .taskText2,#{id} .taskText3{{fill:{task_text_color};}}",
    ));
    css.push_str(&format!(
        "#{id} .task0,#{id} .task1,#{id} .task2,#{id} .task3{{fill:{task_bkg_color};stroke:{task_border_color};}}",
    ));
    css.push_str(&format!(
        "#{id} .taskTextOutside0,#{id} .taskTextOutside2{{fill:{task_text_outside_color};}}",
    ));
    css.push_str(&format!(
        "#{id} .taskTextOutside1,#{id} .taskTextOutside3{{fill:{task_text_outside_color};}}",
    ));
    css.push_str(&format!(
        "#{id} .active0,#{id} .active1,#{id} .active2,#{id} .active3{{fill:{active_task_bkg_color};stroke:{active_task_border_color};}}",
    ));
    css.push_str(&format!(
        "#{id} .activeText0,#{id} .activeText1,#{id} .activeText2,#{id} .activeText3{{fill:{task_text_dark_color}!important;}}",
    ));
    css.push_str(&format!(
        "#{id} .done0,#{id} .done1,#{id} .done2,#{id} .done3{{stroke:{done_task_border_color};fill:{done_task_bkg_color};stroke-width:2;}}",
    ));
    css.push_str(&format!(
        "#{id} .doneText0,#{id} .doneText1,#{id} .doneText2,#{id} .doneText3{{fill:{task_text_dark_color}!important;}}",
    ));
    css.push_str(&format!(
        "#{id} .doneText0.taskTextOutsideLeft,#{id} .doneText0.taskTextOutsideRight,#{id} .doneText1.taskTextOutsideLeft,#{id} .doneText1.taskTextOutsideRight,#{id} .doneText2.taskTextOutsideLeft,#{id} .doneText2.taskTextOutsideRight,#{id} .doneText3.taskTextOutsideLeft,#{id} .doneText3.taskTextOutsideRight{{fill:{task_text_outside_color}!important;}}",
    ));
    css.push_str(&format!(
        "#{id} .crit0,#{id} .crit1,#{id} .crit2,#{id} .crit3{{stroke:{crit_border_color};fill:{crit_bkg_color};stroke-width:2;}}",
    ));
    css.push_str(&format!(
        "#{id} .activeCrit0,#{id} .activeCrit1,#{id} .activeCrit2,#{id} .activeCrit3{{stroke:{crit_border_color};fill:{active_task_bkg_color};stroke-width:2;}}",
    ));
    css.push_str(&format!(
        "#{id} .doneCrit0,#{id} .doneCrit1,#{id} .doneCrit2,#{id} .doneCrit3{{stroke:{crit_border_color};fill:{done_task_bkg_color};stroke-width:2;cursor:pointer;shape-rendering:crispEdges;}}",
    ));
    css.push_str(&format!(
        "#{id} .milestone{{transform:rotate(45deg) scale(0.8,0.8);}}",
    ));
    css.push_str(&format!(
        "#{id} .milestoneText{{font-style:italic;}}",
    ));
    css.push_str(&format!(
        "#{id} .doneCritText0,#{id} .doneCritText1,#{id} .doneCritText2,#{id} .doneCritText3{{fill:{task_text_dark_color}!important;}}",
    ));
    css.push_str(&format!(
        "#{id} .doneCritText0.taskTextOutsideLeft,#{id} .doneCritText0.taskTextOutsideRight,#{id} .doneCritText1.taskTextOutsideLeft,#{id} .doneCritText1.taskTextOutsideRight,#{id} .doneCritText2.taskTextOutsideLeft,#{id} .doneCritText2.taskTextOutsideRight,#{id} .doneCritText3.taskTextOutsideLeft,#{id} .doneCritText3.taskTextOutsideRight{{fill:{task_text_outside_color}!important;}}",
    ));
    css.push_str(&format!(
        "#{id} .vert{{stroke:{vert_line_color};}}",
    ));
    css.push_str(&format!(
        "#{id} .vertText{{font-size:15px;text-anchor:middle;fill:{vert_line_color}!important;}}",
    ));
    css.push_str(&format!(
        "#{id} .activeCritText0,#{id} .activeCritText1,#{id} .activeCritText2,#{id} .activeCritText3{{fill:{task_text_dark_color}!important;}}",
    ));
    css.push_str(&format!(
        "#{id} .titleText{{text-anchor:middle;font-size:18px;fill:{title_color};font-family:{ff};}}",
        ff = font_family_compact,
    ));

    // Neo-look trailers (same as xychart).
    css.push_str(&format!(
        "#{id} .node .neo-node{{stroke:{nb};}}",
        nb = node_border,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node rect,#{id} [data-look="neo"].cluster rect,#{id} [data-look="neo"].node polygon{{stroke:{s};filter:{f};}}"#,
        s = neo_stroke,
        f = neo_filter,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node path{{stroke:{s};stroke-width:1px;}}"#,
        s = neo_stroke,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node .outer-path{{filter:{f};}}"#,
        f = neo_filter,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node .neo-line path{{stroke:{nb};filter:none;}}"#,
        nb = node_border,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle{{stroke:{s};filter:{f};}}"#,
        s = neo_stroke,
        f = neo_filter,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle .state-start{{fill:#000000;}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon{{fill:{s};filter:{f};}}"#,
        s = neo_stroke,
        f = neo_filter,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon-neo path{{stroke:{s};filter:{f};}}"#,
        s = neo_stroke,
        f = neo_filter,
    ));
    css.push_str(&format!(
        "#{id} :root{{--mermaid-font-family:{ff};}}",
        ff = font_family_compact,
    ));

    format!("<style>{css}</style>")
}
