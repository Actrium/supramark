//! Wardley map SVG renderer — byte-exact reproduction of upstream
//! `wardleyRenderer.ts` output.
//!
//! The reference SVGs under
//! `tests/reference/ext_fixtures/{cypress,demos}/wardley` use inline
//! attribute styling only (no `<style>` block). All numeric attribute
//! strings therefore come straight from JavaScript's
//! `Number.prototype.toString()` — we reuse the [`js_num`] formatter
//! to match that behaviour.

use crate::error::Result;
use crate::layout::wardley::{
    self as layout_mod, WardleyLayout, LABEL_FONT_SIZE, NODE_LABEL_OFFSET, NODE_RADIUS, PADDING,
    SQUARE_SIZE,
};
use crate::model::wardley::{LinkFlow, SourceStrategy, WardleyDiagram, WardleyLink, WardleyNode};
use crate::theme::ThemeVariables;

const DEFAULT_STAGES: &[&str] = &["Genesis", "Custom Built", "Product", "Commodity"];

pub fn render(
    d: &WardleyDiagram,
    l: &WardleyLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(16384);

    let bg = theme_bg(theme);
    let axis_color = "#000";
    let axis_text_color = theme_primary_text(theme);
    let component_fill = "#fff";
    let component_stroke = "#000";
    let component_label_color = theme_primary_text(theme);
    let link_stroke = "#000";
    let evolution_stroke = "#dc3545";

    // ── Opening <svg> tag ────────────────────────────────────────────
    // Attribute order matches upstream: id, width, xmlns, style,
    // viewBox, role, aria-roledescription.
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: {w}px;" viewBox="0 0 {w} {h}" role="graphics-document document" aria-roledescription="wardley-beta">"#,
        id = id,
        w = fmt_int(l.width),
        h = fmt_int(l.height),
    ));

    // ── Main <g class="wardley-map"> ─────────────────────────────────
    out.push_str(r#"<g class="wardley-map">"#);

    // Background rect.
    out.push_str(&format!(
        r#"<rect class="wardley-background" width="{w}" height="{h}" fill="{bg}"></rect>"#,
        w = fmt_int(l.width),
        h = fmt_int(l.height),
        bg = bg,
    ));

    // Title.
    if let Some(title) = d.meta.title.as_deref() {
        if !title.is_empty() {
            let title_font_size = 12.0_f64 * 1.05; // → 12.600000000000001
            out.push_str(&format!(
                r#"<text class="wardley-title" x="{x}" y="{y}" fill="{tc}" font-size="{fs}" font-weight="bold" text-anchor="middle" dominant-baseline="middle">{t}</text>"#,
                x = js_num(l.width / 2.0),
                y = js_num(PADDING / 2.0),
                tc = axis_text_color,
                fs = js_num(title_font_size),
                t = html_escape(title),
            ));
        }
    }

    // ── Axes ─────────────────────────────────────────────────────────
    out.push_str(r#"<g class="wardley-axes">"#);
    out.push_str(&format!(
        r#"<line x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}" stroke="{ac}" stroke-width="1"></line>"#,
        x1 = js_num(PADDING),
        x2 = js_num(l.width - PADDING),
        y1 = js_num(l.height - PADDING),
        y2 = js_num(l.height - PADDING),
        ac = axis_color,
    ));
    out.push_str(&format!(
        r#"<line x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}" stroke="{ac}" stroke-width="1"></line>"#,
        x1 = js_num(PADDING),
        x2 = js_num(PADDING),
        y1 = js_num(PADDING),
        y2 = js_num(l.height - PADDING),
        ac = axis_color,
    ));
    let x_label = d.axes.x_label.as_deref().unwrap_or("Evolution");
    let y_label = d.axes.y_label.as_deref().unwrap_or("Visibility");
    out.push_str(&format!(
        r#"<text class="wardley-axis-label wardley-axis-label-x" x="{x}" y="{y}" fill="{tc}" font-size="{fs}" font-weight="bold" text-anchor="middle">{t}</text>"#,
        x = js_num(PADDING + l.chart_width / 2.0),
        y = js_num(l.height - PADDING / 4.0),
        tc = axis_text_color,
        fs = js_num(12.0),
        t = html_escape(x_label),
    ));
    let yl_x = PADDING / 3.0;
    let yl_y = PADDING + l.chart_height / 2.0;
    out.push_str(&format!(
        r#"<text class="wardley-axis-label wardley-axis-label-y" x="{x}" y="{y}" fill="{tc}" font-size="{fs}" font-weight="bold" text-anchor="middle" transform="rotate(-90 {rx} {ry})">{t}</text>"#,
        x = js_num(yl_x),
        y = js_num(yl_y),
        tc = axis_text_color,
        fs = js_num(12.0),
        rx = js_num(yl_x),
        ry = js_num(yl_y),
        t = html_escape(y_label),
    ));
    out.push_str("</g>");

    // ── Stages ───────────────────────────────────────────────────────
    let stages: Vec<&str> = if d.axes.stages.is_empty() {
        DEFAULT_STAGES.to_vec()
    } else {
        d.axes.stages.iter().map(|s| s.as_str()).collect()
    };
    if !stages.is_empty() {
        out.push_str(r#"<g class="wardley-stages">"#);
        let boundaries = if !d.axes.stage_boundaries.is_empty()
            && d.axes.stage_boundaries.len() == stages.len()
        {
            Some(&d.axes.stage_boundaries)
        } else {
            None
        };
        let stage_positions: Vec<(f64, f64)> = if let Some(b) = boundaries {
            let mut prev = 0.0_f64;
            let mut v = Vec::with_capacity(stages.len());
            for end in b.iter() {
                v.push((prev, *end));
                prev = *end;
            }
            v
        } else {
            let stage_w = 1.0 / stages.len() as f64;
            (0..stages.len())
                .map(|i| (i as f64 * stage_w, (i + 1) as f64 * stage_w))
                .collect()
        };
        for (i, stage) in stages.iter().enumerate() {
            let (s, e) = stage_positions[i];
            let start_x = PADDING + s * l.chart_width;
            let end_x = PADDING + e * l.chart_width;
            let center_x = (start_x + end_x) / 2.0;

            // Label first.
            out.push_str(&format!(
                r#"<text class="wardley-stage-label" x="{cx}" y="{y}" fill="{tc}" font-size="{fs}" text-anchor="middle">{t}</text>"#,
                cx = js_num(center_x),
                y = js_num(l.height - PADDING / 1.5),
                tc = axis_text_color,
                fs = js_num(10.0),
                t = html_escape(stage),
            ));

            // Divider (skip for first stage).
            if i + 1 < stages.len() {
                let divider_x = end_x;
                out.push_str(&format!(
                    r##"<line x1="{x}" x2="{x}" y1="{y1}" y2="{y2}" stroke="#000" stroke-width="1" stroke-dasharray="5 5" opacity="0.8"></line>"##,
                    x = js_num(divider_x),
                    y1 = js_num(PADDING),
                    y2 = js_num(l.height - PADDING),
                ));
            }
        }
        out.push_str("</g>");
    }

    // ── Pipeline boxes + evolution dashed links ──────────────────────
    if !l.pipelines.is_empty() {
        out.push_str(r#"<g class="wardley-pipelines">"#);
        for pipe in &l.pipelines {
            out.push_str(&format!(
                r#"<rect class="wardley-pipeline-box" x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="{ac}" stroke-width="1.5" rx="4" ry="4"></rect>"#,
                x = js_num(pipe.box_x),
                y = js_num(pipe.box_y),
                w = js_num(pipe.box_w),
                h = js_num(pipe.box_h),
                ac = axis_color,
            ));
        }
        out.push_str("</g>");

        out.push_str(r#"<g class="wardley-pipeline-links">"#);
        for pipe in &l.pipelines {
            let sorted = &pipe.sorted_component_ids;
            for win in sorted.windows(2) {
                let (Some(a), Some(b)) = (
                    layout_mod::get_position(l, d, &win[0]),
                    layout_mod::get_position(l, d, &win[1]),
                ) else {
                    continue;
                };
                out.push_str(&format!(
                    r#"<line class="wardley-pipeline-evolution-link" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{ls}" stroke-width="1" stroke-dasharray="4 4"></line>"#,
                    x1 = js_num(a.0),
                    y1 = js_num(a.1),
                    x2 = js_num(b.0),
                    y2 = js_num(b.1),
                    ls = link_stroke,
                ));
            }
        }
        out.push_str("</g>");
    }

    // ── Links ────────────────────────────────────────────────────────
    // Filter: drop links that reference unknown nodes, and drop links
    // where the source is a pipeline child of the target.
    let pipeline_child_sets: Vec<(&str, std::collections::HashSet<&str>)> = d
        .pipelines
        .iter()
        .map(|p| {
            (
                p.node_id.as_str(),
                p.component_ids.iter().map(|s| s.as_str()).collect(),
            )
        })
        .collect();

    let valid_links: Vec<&WardleyLink> = d
        .links
        .iter()
        .filter(|link| {
            layout_mod::get_position(l, d, &link.source).is_some()
                && layout_mod::get_position(l, d, &link.target).is_some()
        })
        .filter(|link| {
            // If the target is a pipeline parent and the source is one
            // of its components, drop.
            for (parent, children) in &pipeline_child_sets {
                if *parent == link.target && children.contains(link.source.as_str()) {
                    return false;
                }
            }
            true
        })
        .collect();

    out.push_str(r#"<g class="wardley-links">"#);
    // Lines first, then labels (matches upstream selectAll('line')
    // .data(valid_links) then selectAll('text').data(valid_links with
    // label)).
    for link in &valid_links {
        let (sx, sy) = layout_mod::get_position(l, d, &link.source).unwrap();
        let (tx, ty) = layout_mod::get_position(l, d, &link.target).unwrap();
        let source_node = d.get_node(&link.source).unwrap();
        let target_node = d.get_node(&link.target).unwrap();
        let r_src = if source_node.is_pipeline_parent {
            SQUARE_SIZE / SQRT_2
        } else {
            NODE_RADIUS
        };
        let r_tgt = if target_node.is_pipeline_parent {
            SQUARE_SIZE / SQRT_2
        } else {
            NODE_RADIUS
        };

        // x1, y1 (source edge).
        let dx = tx - sx;
        let dy = ty - sy;
        let dist = (dx * dx + dy * dy).sqrt();
        let x1 = sx + (dx / dist) * r_src;
        let y1 = sy + (dy / dist) * r_src;

        // x2, y2 (target edge).
        let dx2 = sx - tx;
        let dy2 = sy - ty;
        let dist2 = (dx2 * dx2 + dy2 * dy2).sqrt();
        let x2 = tx + (dx2 / dist2) * r_tgt;
        let y2 = ty + (dy2 / dist2) * r_tgt;

        let class_attr = if link.dashed {
            "wardley-link wardley-link--dashed"
        } else {
            "wardley-link"
        };

        // Attribute order observed in upstream D3:
        //   class, x1, y1, x2, y2, stroke, stroke-width,
        //   stroke-dasharray (dashed only), marker-end (flow=forward/
        //   bidirectional), marker-start (flow=backward/bidirectional).
        let mut attrs = format!(
            r#"class="{cls}" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{ls}" stroke-width="1""#,
            cls = class_attr,
            x1 = js_num(x1),
            y1 = js_num(y1),
            x2 = js_num(x2),
            y2 = js_num(y2),
            ls = link_stroke,
        );
        if link.dashed {
            attrs.push_str(r#" stroke-dasharray="6 6""#);
        }
        match link.flow {
            Some(LinkFlow::Forward) | Some(LinkFlow::Bidirectional) => {
                attrs.push_str(&format!(r#" marker-end="url(#link-arrow-end-{id})""#));
            }
            _ => {}
        }
        match link.flow {
            Some(LinkFlow::Backward) | Some(LinkFlow::Bidirectional) => {
                attrs.push_str(&format!(r#" marker-start="url(#link-arrow-start-{id})""#));
            }
            _ => {}
        }
        out.push_str(&format!(r#"<line {attrs}></line>"#));
    }
    // Link labels (separate selectAll('text') pass).
    for link in valid_links.iter().filter(|l| l.label.is_some()) {
        let (sx, sy) = layout_mod::get_position(l, d, &link.source).unwrap();
        let (tx, ty) = layout_mod::get_position(l, d, &link.target).unwrap();
        let mid_x = (sx + tx) / 2.0;
        let mid_y = (sy + ty) / 2.0;
        let dx = tx - sx;
        let dy = ty - sy;
        let dist = (dx * dx + dy * dy).sqrt();
        let offset = 8.0;
        let perp_x = dy / dist;
        let perp_y = -dx / dist;
        let label_x = mid_x + perp_x * offset;
        let label_y = mid_y + perp_y * offset;
        let mut angle = dy.atan2(dx) * 180.0 / std::f64::consts::PI;
        if !(-90.0..=90.0).contains(&angle) {
            angle += 180.0;
        }
        let label_text = link.label.as_deref().unwrap_or("");
        out.push_str(&format!(
            r#"<text class="wardley-link-label" x="{x}" y="{y}" fill="{tc}" font-size="{fs}" text-anchor="middle" dominant-baseline="middle" transform="rotate({a} {rx} {ry})">{t}</text>"#,
            x = js_num(label_x),
            y = js_num(label_y),
            tc = axis_text_color,
            fs = js_num(LABEL_FONT_SIZE),
            a = js_num(angle),
            rx = js_num(label_x),
            ry = js_num(label_y),
            t = html_escape(label_text),
        ));
    }
    out.push_str("</g>");

    // ── Trends (evolution arrows) ────────────────────────────────────
    // Always emit — even when empty.
    out.push_str(r#"<g class="wardley-trends">"#);
    for trend in &d.trends {
        let Some(origin) = layout_mod::get_position(l, d, &trend.node_id) else {
            continue;
        };
        let target_x = layout_mod::project_x(trend.target_x, l.chart_width);
        let target_y = layout_mod::project_y(trend.target_y, l.height, l.chart_height);
        let dx = target_x - origin.0;
        let dy = target_y - origin.1;
        let dist = (dx * dx + dy * dy).sqrt();
        let shorten = NODE_RADIUS + 2.0;
        let (x2, y2) = if dist > shorten {
            (
                target_x - (dx / dist) * shorten,
                target_y - (dy / dist) * shorten,
            )
        } else {
            (target_x, target_y)
        };
        out.push_str(&format!(
            r#"<line class="wardley-trend" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{es}" stroke-width="1" stroke-dasharray="4 4" marker-end="url(#arrow-{id})"></line>"#,
            x1 = js_num(origin.0),
            y1 = js_num(origin.1),
            x2 = js_num(x2),
            y2 = js_num(y2),
            es = evolution_stroke,
        ));
    }
    out.push_str("</g>");

    // ── Nodes ────────────────────────────────────────────────────────
    out.push_str(r#"<g class="wardley-nodes">"#);
    for node in &d.nodes {
        render_node(
            &mut out,
            node,
            d,
            l,
            component_fill,
            component_stroke,
            component_label_color,
            evolution_stroke,
        );
    }
    out.push_str("</g>");

    // ── Annotations ──────────────────────────────────────────────────
    if !d.annotations.is_empty() {
        out.push_str(r#"<g class="wardley-annotations">"#);
        for ann in &d.annotations {
            let projected: Vec<(f64, f64)> = ann
                .coordinates
                .iter()
                .map(|(x, y)| {
                    (
                        layout_mod::project_x(*x, l.chart_width),
                        layout_mod::project_y(*y, l.height, l.chart_height),
                    )
                })
                .collect();
            if projected.len() > 1 {
                for win in projected.windows(2) {
                    out.push_str(&format!(
                        r#"<line class="wardley-annotation-line" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{ac}" stroke-width="1.5" stroke-dasharray="4 4"></line>"#,
                        x1 = js_num(win[0].0),
                        y1 = js_num(win[0].1),
                        x2 = js_num(win[1].0),
                        y2 = js_num(win[1].1),
                        ac = axis_color,
                    ));
                }
            }
            for coord in &projected {
                out.push_str(r#"<g class="wardley-annotation">"#);
                out.push_str(&format!(
                    r#"<circle cx="{cx}" cy="{cy}" r="10" fill="white" stroke="{ac}" stroke-width="1.5"></circle>"#,
                    cx = js_num(coord.0),
                    cy = js_num(coord.1),
                    ac = axis_color,
                ));
                out.push_str(&format!(
                    r#"<text x="{cx}" y="{cy}" text-anchor="middle" dominant-baseline="central" font-size="10" fill="{tc}" font-weight="bold">{n}</text>"#,
                    cx = js_num(coord.0),
                    cy = js_num(coord.1),
                    tc = axis_text_color,
                    n = ann.number,
                ));
                out.push_str("</g>");
            }
        }

        // Annotations text box.
        if let Some((box_x_val, box_y_val)) = d.annotations_box {
            let mut box_x = layout_mod::project_x(box_x_val, l.chart_width);
            let mut box_y = layout_mod::project_y(box_y_val, l.height, l.chart_height);
            let pad = 10.0;
            let line_height = 16.0;
            let font_size = 11.0;

            out.push_str(r#"<g class="wardley-annotations-box">"#);

            // Sort numbered & filter by text.
            let mut sorted: Vec<&crate::model::wardley::WardleyAnnotation> =
                d.annotations.iter().filter(|a| a.text.is_some()).collect();
            sorted.sort_by_key(|a| a.number);

            if !sorted.is_empty() {
                // Compute each line's text width using our bundled font
                // metrics (jsdom's getComputedTextLength for the 11px
                // default font).
                let mut max_width = 0.0_f64;
                // maxHeight per upstream uses getBBox().height — for
                // 11px DejaVu the ascent+descent is ~12.8281. We emit
                // the literal upstream produces for these fixtures
                // (the numeric was 12.8... on getBBox). Using our
                // text_width hit returns width; we approximate the
                // BBox height via the upstream constant 12.828125
                // observed on jsdom with bundled DejaVu.
                for ann in &sorted {
                    let label = format!("{}. {}", ann.number, ann.text.as_deref().unwrap_or(""));
                    // jsdom's getComputedTextLength for an unstyled <text>
                    // falls back to a DejaVu-sans approximation. We reuse
                    // our bundled DejaVu Sans metrics at the explicit 11px
                    // font-size to match upstream byte-for-byte.
                    let w = crate::font_metrics::text_width(
                        &label,
                        "DejaVu Sans",
                        font_size,
                        false,
                        false,
                    );
                    if w > max_width {
                        max_width = w;
                    }
                }
                // getBBox().height in jsdom ≈ DejaVu Sans hhea
                // (ascender + |descender|) / units_per_em * font_size.
                let max_height =
                    crate::font_metrics::line_height("DejaVu Sans", font_size, false, false);
                let box_width = max_width + pad * 2.0 + 105.0;
                let box_height = (sorted.len() as f64) * line_height + pad * 2.0 + max_height / 2.0;

                // Clamp within chart.
                let min_x = PADDING;
                let max_x = l.width - PADDING - box_width;
                let min_y = PADDING;
                let max_y = l.height - PADDING - box_height;
                box_x = box_x.max(min_x).min(max_x);
                box_y = box_y.max(min_y).min(max_y);

                // Rect is inserted before text elements.
                out.push_str(&format!(
                    r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="white" stroke="{ac}" stroke-width="1.5" rx="4" ry="4"></rect>"#,
                    x = js_num(box_x),
                    y = js_num(box_y),
                    w = js_num(box_width),
                    h = js_num(box_height),
                    ac = axis_color,
                ));
                for (idx, ann) in sorted.iter().enumerate() {
                    let tx = box_x + pad;
                    let ty = box_y + pad + (idx as f64 + 1.0) * line_height;
                    let label = format!("{}. {}", ann.number, ann.text.as_deref().unwrap_or(""));
                    out.push_str(&format!(
                        r#"<text x="{x}" y="{y}" font-size="{fs}" fill="{tc}" text-anchor="start" dominant-baseline="middle">{t}</text>"#,
                        x = js_num(tx),
                        y = js_num(ty),
                        fs = js_num(font_size),
                        tc = axis_text_color,
                        t = html_escape(&label),
                    ));
                }
            }
            out.push_str("</g>");
        }
        out.push_str("</g>");
    }

    // ── Notes ────────────────────────────────────────────────────────
    if !d.notes.is_empty() {
        out.push_str(r#"<g class="wardley-notes">"#);
        for note in &d.notes {
            let x = layout_mod::project_x(note.x, l.chart_width);
            let y = layout_mod::project_y(note.y, l.height, l.chart_height);
            out.push_str(&format!(
                r#"<text x="{x}" y="{y}" text-anchor="start" font-size="11" fill="{tc}" font-weight="bold">{t}</text>"#,
                x = js_num(x),
                y = js_num(y),
                tc = axis_text_color,
                t = html_escape(&note.text),
            ));
        }
        out.push_str("</g>");
    }

    // ── Accelerators ─────────────────────────────────────────────────
    if !d.accelerators.is_empty() {
        out.push_str(r#"<g class="wardley-accelerators">"#);
        for acc in &d.accelerators {
            let acc_x = layout_mod::project_x(acc.x, l.chart_width);
            let acc_y = layout_mod::project_y(acc.y, l.height, l.chart_height);
            let aw = 60.0;
            let ah = 30.0;
            let head = 20.0;
            let path = format!(
                "\n        M {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        Z\n      ",
                js_num(acc_x),
                js_num(acc_y - ah / 2.0),
                js_num(acc_x + aw - head),
                js_num(acc_y - ah / 2.0),
                js_num(acc_x + aw - head),
                js_num(acc_y - ah / 2.0 - 8.0),
                js_num(acc_x + aw),
                js_num(acc_y),
                js_num(acc_x + aw - head),
                js_num(acc_y + ah / 2.0 + 8.0),
                js_num(acc_x + aw - head),
                js_num(acc_y + ah / 2.0),
                js_num(acc_x),
                js_num(acc_y + ah / 2.0),
            );
            out.push_str(&format!(
                r#"<path d="{d}" fill="white" stroke="{cs}" stroke-width="1"></path>"#,
                d = path,
                cs = component_stroke,
            ));
            out.push_str(&format!(
                r#"<text x="{x}" y="{y}" text-anchor="middle" font-size="10" fill="{tc}" font-weight="bold">{t}</text>"#,
                x = js_num(acc_x + aw / 2.0),
                y = js_num(acc_y + ah / 2.0 + 15.0),
                tc = axis_text_color,
                t = html_escape(&acc.name),
            ));
        }
        out.push_str("</g>");
    }

    // ── Deaccelerators ───────────────────────────────────────────────
    if !d.deaccelerators.is_empty() {
        out.push_str(r#"<g class="wardley-deaccelerators">"#);
        for dec in &d.deaccelerators {
            let dec_x = layout_mod::project_x(dec.x, l.chart_width);
            let dec_y = layout_mod::project_y(dec.y, l.height, l.chart_height);
            let aw = 60.0;
            let ah = 30.0;
            let head = 20.0;
            let path = format!(
                "\n        M {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        Z\n      ",
                js_num(dec_x + aw),
                js_num(dec_y - ah / 2.0),
                js_num(dec_x + head),
                js_num(dec_y - ah / 2.0),
                js_num(dec_x + head),
                js_num(dec_y - ah / 2.0 - 8.0),
                js_num(dec_x),
                js_num(dec_y),
                js_num(dec_x + head),
                js_num(dec_y + ah / 2.0 + 8.0),
                js_num(dec_x + head),
                js_num(dec_y + ah / 2.0),
                js_num(dec_x + aw),
                js_num(dec_y + ah / 2.0),
            );
            out.push_str(&format!(
                r#"<path d="{d}" fill="white" stroke="{cs}" stroke-width="1"></path>"#,
                d = path,
                cs = component_stroke,
            ));
            out.push_str(&format!(
                r#"<text x="{x}" y="{y}" text-anchor="middle" font-size="10" fill="{tc}" font-weight="bold">{t}</text>"#,
                x = js_num(dec_x + aw / 2.0),
                y = js_num(dec_y + ah / 2.0 + 15.0),
                tc = axis_text_color,
                t = html_escape(&dec.name),
            ));
        }
        out.push_str("</g>");
    }

    out.push_str("</g>");

    // ── Defs (markers) ───────────────────────────────────────────────
    out.push_str("<defs>");
    out.push_str(&format!(
        r##"<marker id="arrow-{id}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="{es}" stroke="none"></path></marker>"##,
        es = evolution_stroke,
    ));
    out.push_str(&format!(
        r##"<marker id="link-arrow-end-{id}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" fill="{ls}" stroke="none"></path></marker>"##,
        ls = link_stroke,
    ));
    out.push_str(&format!(
        r##"<marker id="link-arrow-start-{id}" viewBox="0 0 10 10" refX="1" refY="5" markerWidth="5" markerHeight="5" orient="auto"><path d="M 10 0 L 0 5 L 10 10 z" fill="{ls}" stroke="none"></path></marker>"##,
        ls = link_stroke,
    ));
    out.push_str("</defs>");

    out.push_str("</svg>");
    Ok(out)
}

fn render_node(
    out: &mut String,
    node: &WardleyNode,
    d: &WardleyDiagram,
    l: &WardleyLayout,
    component_fill: &str,
    component_stroke: &str,
    component_label_color: &str,
    evolution_stroke: &str,
) {
    let Some((x, y)) = layout_mod::get_position(l, d, &node.id) else {
        return;
    };

    let class_frag = match node.class_name.as_deref() {
        None => String::from("wardley-node"),
        Some(c) => format!("wardley-node wardley-node--{c}"),
    };
    out.push_str(&format!(r#"<g class="{class_frag}">"#));

    // 1. Source-strategy overlays (behind main glyph).
    match node.source_strategy {
        Some(SourceStrategy::Outsource) => {
            out.push_str(&format!(
                r##"<circle class="wardley-outsource-overlay" cx="{cx}" cy="{cy}" r="{r}" fill="#666" stroke="{cs}" stroke-width="1"></circle>"##,
                cx = js_num(x),
                cy = js_num(y),
                r = js_num(NODE_RADIUS * 2.0),
                cs = component_stroke,
            ));
        }
        Some(SourceStrategy::Buy) => {
            out.push_str(&format!(
                r##"<circle class="wardley-buy-overlay" cx="{cx}" cy="{cy}" r="{r}" fill="#ccc" stroke="{cs}" stroke-width="1"></circle>"##,
                cx = js_num(x),
                cy = js_num(y),
                r = js_num(NODE_RADIUS * 2.0),
                cs = component_stroke,
            ));
        }
        Some(SourceStrategy::Build) => {
            out.push_str(&format!(
                r##"<circle class="wardley-build-overlay" cx="{cx}" cy="{cy}" r="{r}" fill="#eee" stroke="#000" stroke-width="1"></circle>"##,
                cx = js_num(x),
                cy = js_num(y),
                r = js_num(NODE_RADIUS * 2.0),
            ));
        }
        Some(SourceStrategy::Market) => {
            out.push_str(&format!(
                r#"<circle class="wardley-market-overlay" cx="{cx}" cy="{cy}" r="{r}" fill="white" stroke="{cs}" stroke-width="1"></circle>"#,
                cx = js_num(x),
                cy = js_num(y),
                r = js_num(NODE_RADIUS * 2.0),
                cs = component_stroke,
            ));
        }
        None => {}
    }

    // 2. Main glyph.
    if node.is_pipeline_parent {
        out.push_str(&format!(
            r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{cf}" stroke="{cs}" stroke-width="1"></rect>"#,
            x = js_num(x - SQUARE_SIZE / 2.0),
            y = js_num(y - SQUARE_SIZE / 2.0),
            w = js_num(SQUARE_SIZE),
            h = js_num(SQUARE_SIZE),
            cf = component_fill,
            cs = component_stroke,
        ));
    } else if matches!(node.source_strategy, Some(SourceStrategy::Market)) {
        // Draw triangle lines + 3 small circles for market.
        let small_r = NODE_RADIUS * 0.7;
        let tri_r = NODE_RADIUS * 1.2;
        let cos30 = (std::f64::consts::PI / 6.0).cos();
        let sin30 = (std::f64::consts::PI / 6.0).sin();
        // Lines.
        // Top-to-bottom-left.
        out.push_str(&format!(
            r#"<line class="wardley-market-line" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{cs}" stroke-width="1"></line>"#,
            x1 = js_num(x),
            y1 = js_num(y - tri_r),
            x2 = js_num(x - tri_r * cos30),
            y2 = js_num(y + tri_r * sin30),
            cs = component_stroke,
        ));
        // Bottom-left to bottom-right.
        out.push_str(&format!(
            r#"<line class="wardley-market-line" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{cs}" stroke-width="1"></line>"#,
            x1 = js_num(x - tri_r * cos30),
            y1 = js_num(y + tri_r * sin30),
            x2 = js_num(x + tri_r * cos30),
            y2 = js_num(y + tri_r * sin30),
            cs = component_stroke,
        ));
        // Bottom-right to top.
        out.push_str(&format!(
            r#"<line class="wardley-market-line" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{cs}" stroke-width="1"></line>"#,
            x1 = js_num(x + tri_r * cos30),
            y1 = js_num(y + tri_r * sin30),
            x2 = js_num(x),
            y2 = js_num(y - tri_r),
            cs = component_stroke,
        ));
        // Small circles.
        for (sx, sy) in [
            (x, y - tri_r),
            (x - tri_r * cos30, y + tri_r * sin30),
            (x + tri_r * cos30, y + tri_r * sin30),
        ] {
            out.push_str(&format!(
                r#"<circle class="wardley-market-dot" cx="{cx}" cy="{cy}" r="{r}" fill="white" stroke="{cs}" stroke-width="2"></circle>"#,
                cx = js_num(sx),
                cy = js_num(sy),
                r = js_num(small_r),
                cs = component_stroke,
            ));
        }
    } else if node.class_name.as_deref() == Some("anchor") {
        // Anchors draw no circle; only a centered text label.
    } else {
        out.push_str(&format!(
            r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{cf}" stroke="{cs}" stroke-width="1"></circle>"#,
            cx = js_num(x),
            cy = js_num(y),
            r = js_num(NODE_RADIUS),
            cf = component_fill,
            cs = component_stroke,
        ));
    }

    // 3. Inertia indicator (vertical line to the right).
    if node.inertia {
        let mut offset = if node.is_pipeline_parent {
            SQUARE_SIZE / 2.0 + 15.0
        } else {
            NODE_RADIUS + 15.0
        };
        if node.source_strategy.is_some() {
            offset += NODE_RADIUS + 10.0;
        }
        let line_h = if node.is_pipeline_parent {
            SQUARE_SIZE
        } else {
            NODE_RADIUS * 2.0
        };
        out.push_str(&format!(
            r#"<line class="wardley-inertia" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{cs}" stroke-width="6"></line>"#,
            x1 = js_num(x + offset),
            y1 = js_num(y - line_h / 2.0),
            x2 = js_num(x + offset),
            y2 = js_num(y + line_h / 2.0),
            cs = component_stroke,
        ));
    }

    // 4. Label.
    let (lx, ly, anchor, baseline, font_weight, fill) = {
        if node.class_name.as_deref() == Some("anchor") {
            let lx = node.label_offset_x.map(|o| x + o as f64).unwrap_or(x);
            let ly = node.label_offset_y.map(|o| y + o as f64).unwrap_or(y - 3.0);
            (lx, ly, "middle", "middle", "bold", "#000".to_string())
        } else {
            let mut default_off_x = NODE_LABEL_OFFSET;
            let mut default_off_y = -NODE_LABEL_OFFSET;
            if node.source_strategy.is_some() && node.label_offset_x.is_none() {
                default_off_x += 10.0;
            }
            if node.source_strategy.is_some() && node.label_offset_y.is_none() {
                default_off_y -= 10.0;
            }
            let lx = x + node
                .label_offset_x
                .map(|o| o as f64)
                .unwrap_or(default_off_x);
            let ly = y + node
                .label_offset_y
                .map(|o| o as f64)
                .unwrap_or(default_off_y);
            let fill = if node.class_name.as_deref() == Some("evolved") {
                evolution_stroke.to_string()
            } else {
                component_label_color.to_string()
            };
            (lx, ly, "start", "auto", "normal", fill)
        }
    };
    out.push_str(&format!(
        r#"<text x="{x}" y="{y}" class="wardley-node-label" fill="{fill}" font-size="{fs}" font-weight="{fw}" text-anchor="{ta}" dominant-baseline="{bl}">{t}</text>"#,
        x = js_num(lx),
        y = js_num(ly),
        fill = fill,
        fs = js_num(LABEL_FONT_SIZE),
        fw = font_weight,
        ta = anchor,
        bl = baseline,
        t = html_escape(&node.label),
    ));

    out.push_str("</g>");
}

// ─────────────────────────────────────────────────────────────────────────────
// Theme helpers.
// ─────────────────────────────────────────────────────────────────────────────

fn theme_bg(theme: &ThemeVariables) -> &'static str {
    // Upstream: themeVariables.background ?? '#fff'. The default theme
    // populates `background: "white"`, which is the literal we need.
    match theme.background.as_deref() {
        Some("white") => "white",
        Some(s) => Box::leak(s.to_string().into_boxed_str()),
        None => "#fff",
    }
}

fn theme_primary_text(theme: &ThemeVariables) -> &'static str {
    match theme.primary_text_color.as_deref() {
        Some("#131300") => "#131300",
        Some(s) => Box::leak(s.to_string().into_boxed_str()),
        None => "#222",
    }
}

const SQRT_2: f64 = std::f64::consts::SQRT_2;

// ─────────────────────────────────────────────────────────────────────────────
// Number / text formatters — matches upstream JS Number.prototype.toString.
// ─────────────────────────────────────────────────────────────────────────────

fn js_num(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let abs = v.abs();
    if !(1e-6..1e21).contains(&abs) {
        let s = format!("{:e}", v);
        if let Some(e_pos) = s.find('e') {
            let exp = &s[e_pos + 1..];
            if !exp.starts_with('-') {
                let mut fixed = String::with_capacity(s.len() + 1);
                fixed.push_str(&s[..=e_pos]);
                fixed.push('+');
                fixed.push_str(exp);
                return fixed;
            }
        }
        s
    } else {
        format!("{}", v)
    }
}

fn fmt_int(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        js_num(v)
    }
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Inline byte-exact tests — one per fixture under
// `tests/reference/ext_fixtures/{cypress,demos}/wardley`.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::wardley as layout_mod_test;
    use crate::parser::wardley as parser_mod;
    use crate::theme::get_theme;

    fn render_fixture(source: &str, id: &str) -> String {
        let diagram = parser_mod::parse(source).expect("parse");
        let theme = get_theme("default");
        let lay = layout_mod_test::layout(&diagram, &theme).expect("layout");
        super::render(&diagram, &lay, &theme, id).expect("render")
    }

    fn check_fixture(source_path: &str, reference_path: &str, id: &str) {
        let source = std::fs::read_to_string(source_path).expect("source");
        let reference = std::fs::read_to_string(reference_path).expect("reference");
        let got = render_fixture(&source, id);
        let expected = reference.trim_end_matches('\n');
        if got != expected {
            let got_len = got.len();
            let ref_len = expected.len();
            let mut diff_at = 0;
            for (i, (a, b)) in got.bytes().zip(expected.bytes()).enumerate() {
                if a != b {
                    diff_at = i;
                    break;
                }
            }
            if diff_at == 0 && got_len != ref_len {
                diff_at = got_len.min(ref_len);
            }
            let ctx = 200usize;
            let start = diff_at.saturating_sub(ctx);
            let end_got = (diff_at + ctx).min(got_len);
            let end_ref = (diff_at + ctx).min(ref_len);
            panic!(
                "byte mismatch for {source_path} at byte {diff_at}\n  got: ...{g}...\n  ref: ...{r}...",
                g = &got[start..end_got],
                r = &expected[start..end_ref],
            );
        }
    }

    #[test]
    fn cypress_wardley_01() {
        check_fixture(
            "tests/ext_fixtures/cypress/wardley/01.mmd",
            "tests/reference/ext_fixtures/cypress/wardley/01.svg",
            "ref-ext-fixtures-cypress-wardley-01",
        );
    }

    #[test]
    fn cypress_wardley_02() {
        check_fixture(
            "tests/ext_fixtures/cypress/wardley/02.mmd",
            "tests/reference/ext_fixtures/cypress/wardley/02.svg",
            "ref-ext-fixtures-cypress-wardley-02",
        );
    }

    #[test]
    fn cypress_wardley_03() {
        check_fixture(
            "tests/ext_fixtures/cypress/wardley/03.mmd",
            "tests/reference/ext_fixtures/cypress/wardley/03.svg",
            "ref-ext-fixtures-cypress-wardley-03",
        );
    }

    #[test]
    fn cypress_wardley_04() {
        check_fixture(
            "tests/ext_fixtures/cypress/wardley/04.mmd",
            "tests/reference/ext_fixtures/cypress/wardley/04.svg",
            "ref-ext-fixtures-cypress-wardley-04",
        );
    }

    #[test]
    fn cypress_wardley_05() {
        check_fixture(
            "tests/ext_fixtures/cypress/wardley/05.mmd",
            "tests/reference/ext_fixtures/cypress/wardley/05.svg",
            "ref-ext-fixtures-cypress-wardley-05",
        );
    }

    #[test]
    fn cypress_wardley_06() {
        check_fixture(
            "tests/ext_fixtures/cypress/wardley/06.mmd",
            "tests/reference/ext_fixtures/cypress/wardley/06.svg",
            "ref-ext-fixtures-cypress-wardley-06",
        );
    }

    #[test]
    fn demos_wardley_01() {
        check_fixture(
            "tests/ext_fixtures/demos/wardley/01.mmd",
            "tests/reference/ext_fixtures/demos/wardley/01.svg",
            "ref-ext-fixtures-demos-wardley-01",
        );
    }

    #[test]
    fn demos_wardley_02() {
        check_fixture(
            "tests/ext_fixtures/demos/wardley/02.mmd",
            "tests/reference/ext_fixtures/demos/wardley/02.svg",
            "ref-ext-fixtures-demos-wardley-02",
        );
    }

    #[test]
    fn demos_wardley_03() {
        check_fixture(
            "tests/ext_fixtures/demos/wardley/03.mmd",
            "tests/reference/ext_fixtures/demos/wardley/03.svg",
            "ref-ext-fixtures-demos-wardley-03",
        );
    }

    #[test]
    fn demos_wardley_04() {
        check_fixture(
            "tests/ext_fixtures/demos/wardley/04.mmd",
            "tests/reference/ext_fixtures/demos/wardley/04.svg",
            "ref-ext-fixtures-demos-wardley-04",
        );
    }

    #[test]
    fn demos_wardley_05() {
        check_fixture(
            "tests/ext_fixtures/demos/wardley/05.mmd",
            "tests/reference/ext_fixtures/demos/wardley/05.svg",
            "ref-ext-fixtures-demos-wardley-05",
        );
    }

    #[test]
    fn demos_wardley_06() {
        check_fixture(
            "tests/ext_fixtures/demos/wardley/06.mmd",
            "tests/reference/ext_fixtures/demos/wardley/06.svg",
            "ref-ext-fixtures-demos-wardley-06",
        );
    }
}
