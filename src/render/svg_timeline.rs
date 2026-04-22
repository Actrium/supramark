//! Timeline SVG renderer — targets byte-exact parity with upstream
//! `timelineRenderer.ts` + `timelineRendererVertical.ts`.
//!
//! The upstream pipeline:
//!   1. Calls `initGraphics` to append `<defs>`+`<marker arrowhead>`.
//!   2. Iterates sections (if any) and then tasks/events, each
//!      rendering into its own nested `<g>` wrapper.
//!   3. Appends the title `<text>` and the axis line.
//!
//! We emit the output in the exact same order so the pre-computed
//! layout drives `push_str` in a deterministic stream.

use crate::error::Result;
use crate::layout::timeline::{LaidNode, LaidNodeKind, TimelineLayout};
use crate::model::timeline::{TimelineDiagram, TimelineDirection};
use crate::theme::ThemeVariables;

pub fn render(
    d: &TimelineDiagram,
    l: &TimelineLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(16384);

    // Root <svg> tag — attribute order matches upstream.
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: {mw}px;" viewBox="{vb}" role="graphics-document document" aria-roledescription="timeline">"#,
        id = id,
        mw = fmt_num(l.max_width_px),
        vb = format!(
            "{} {} {} {}",
            fmt_num(l.viewbox[0]),
            fmt_num(l.viewbox[1]),
            fmt_num(l.viewbox[2]),
            fmt_num(l.viewbox[3]),
        ),
    ));

    // TD: `lineWrapper.lower()` at the end of draw moves the axis line
    // to the SVG's first-child slot. mermaidAPI.ts then `insertBefore`s
    // the `<style>` tag relative to whatever was the firstChild at the
    // time it captured the reference — which is BEFORE draw runs, so
    // firstChild is null and the style is appended. When the axis is
    // subsequently `lower()`-ed, it sits ahead of the style in the tree.
    //
    // LR: no `lower()` — style is appended first (no pre-existing
    // siblings), everything else follows, axis goes last.
    if matches!(d.direction, TimelineDirection::TD) {
        emit_axis(&mut out, d, l, id);
    }

    // Style block.
    out.push_str(&build_style_block(id, d, theme, l));

    // Upstream appends two empty `<g>` anchors before any geometry.
    // The first one comes from `svgDraw.initGraphics` which is appended
    // as a `<g>` with a nested `<defs>`+`<marker>`; the second comes
    // from `svg.append('g')` in the draw function itself.
    out.push_str("<g></g>");
    out.push_str("<g></g>");
    // LR calls `initGraphics(svg, id)`; TD calls `initGraphics(svg)`
    // with no second arg, which makes the marker id literally
    // `"undefined-arrowhead"` in the emitted SVG. Upstream bug, but
    // byte-exact parity means we reproduce it.
    let marker_id = match d.direction {
        TimelineDirection::LR => format!("{id}-arrowhead"),
        TimelineDirection::TD => "undefined-arrowhead".to_string(),
    };
    out.push_str(&format!(
        r#"<defs><marker id="{marker_id}" refX="5" refY="2" markerWidth="6" markerHeight="4" orient="auto"><path d="M 0,0 V 4 L6,2 Z"></path></marker></defs>"#
    ));

    // Geometry — walk the laid-out nodes in declaration order, grouping
    // tasks with their axis lines and follow-up events the same way
    // upstream's loop does.
    render_bodies(&mut out, d, l, id);

    // Title — upstream appends unconditionally, even on empty strings.
    if l.has_title {
        if let Some((tx, ty)) = l.title_xy {
            out.push_str(&format!(
                r#"<text x="{tx}" font-size="4ex" font-weight="bold" y="{ty}">{t}</text>"#,
                tx = fmt_num(tx),
                ty = fmt_num(ty),
                t = escape_text(&l.title_text),
            ));
        }
    }

    if matches!(d.direction, TimelineDirection::LR) {
        emit_axis(&mut out, d, l, id);
    }

    out.push_str("</svg>");
    Ok(out)
}

fn emit_axis(out: &mut String, d: &TimelineDiagram, l: &TimelineLayout, id: &str) {
    let Some(axis) = &l.axis else {
        return;
    };
    match d.direction {
        TimelineDirection::LR => {
            out.push_str(&format!(
                r#"<g class="lineWrapper"><line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke-width="{sw}" stroke="black" marker-end="url(#{id}-arrowhead)"></line></g>"#,
                x1 = fmt_num(axis.x1),
                y1 = fmt_num(axis.y1),
                x2 = fmt_num(axis.x2),
                y2 = fmt_num(axis.y2),
                sw = fmt_num(axis.stroke_width),
                id = id,
            ));
        }
        TimelineDirection::TD => {
            // Vertical renderer uses marker id `arrowhead` (no id prefix).
            out.push_str(&format!(
                r#"<g class="lineWrapper"><line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke-width="{sw}" stroke="black" marker-end="url(#arrowhead)"></line></g>"#,
                x1 = fmt_num(axis.x1),
                y1 = fmt_num(axis.y1),
                x2 = fmt_num(axis.x2),
                y2 = fmt_num(axis.y2),
                sw = fmt_num(axis.stroke_width),
            ));
        }
    }
}

fn render_bodies(out: &mut String, d: &TimelineDiagram, l: &TimelineLayout, id: &str) {
    // Walk nodes in emission order. Tasks and events interleave with
    // dashed lines to match upstream. We reconstruct that by tracking
    // a running index into `l.lines` and emitting each task's dashed
    // line immediately after the task wrapper.
    let mut line_cursor = 0usize;

    for node in &l.nodes {
        match node.kind {
            LaidNodeKind::Section => emit_section(out, node, id, d),
            LaidNodeKind::Task => {
                emit_node(out, node, "taskWrapper", id, d);
                // Attach the task's dashed line, if any.
                if let Some(line) = l.lines.get(line_cursor) {
                    if matches!(d.direction, TimelineDirection::LR) && line.dashed {
                        // Emit dashed vertical line.
                        out.push_str(&format!(
                            r#"<g class="lineWrapper"><line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke-width="{sw}" stroke="black" marker-end="url(#{id}-arrowhead)" stroke-dasharray="5,5"></line></g>"#,
                            x1 = fmt_num(line.x1),
                            y1 = fmt_num(line.y1),
                            x2 = fmt_num(line.x2),
                            y2 = fmt_num(line.y2),
                            sw = fmt_num(line.stroke_width),
                            id = id,
                        ));
                        line_cursor += 1;
                    }
                }
            }
            LaidNodeKind::Event => {
                emit_node(out, node, "eventWrapper", id, d);
                if matches!(d.direction, TimelineDirection::TD) {
                    if let Some(line) = l.lines.get(line_cursor) {
                        if line.dashed {
                            out.push_str(&format!(
                                r#"<g class="lineWrapper"><line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke-width="{sw}" stroke="black" marker-end="url(#arrowhead)" stroke-dasharray="5,5"></line></g>"#,
                                x1 = fmt_num(line.x1),
                                y1 = fmt_num(line.y1),
                                x2 = fmt_num(line.x2),
                                y2 = fmt_num(line.y2),
                                sw = fmt_num(line.stroke_width),
                            ));
                            line_cursor += 1;
                        }
                    }
                }
            }
        }
    }
}

fn emit_section(out: &mut String, node: &LaidNode, id: &str, d: &TimelineDiagram) {
    let section_class = section_class_name(node.section_index);
    out.push_str(&format!(
        r#"<g transform="translate({x}, {y})">"#,
        x = fmt_num(node.x),
        y = fmt_num(node.y),
    ));
    out.push_str(&format!(r#"<g class="timeline-node {section_class}">"#));
    emit_node_body(out, node, id, d);
    out.push_str("</g></g>");
}

fn emit_node(
    out: &mut String,
    node: &LaidNode,
    wrapper_class: &str,
    id: &str,
    d: &TimelineDiagram,
) {
    let section_class = section_class_name(node.section_index);
    out.push_str(&format!(
        r#"<g class="{wrapper_class}" transform="translate({x}, {y})">"#,
        x = fmt_num(node.x),
        y = fmt_num(node.y),
    ));
    out.push_str(&format!(r#"<g class="timeline-node {section_class}">"#));
    emit_node_body(out, node, id, d);
    out.push_str("</g></g>");
}

fn emit_node_body(out: &mut String, node: &LaidNode, id: &str, d: &TimelineDiagram) {
    let w = node.width;
    let h = node.height;
    let rd = 5.0;
    let body_inner_w = w - 2.0 * node_padding(d, node);

    // Mermaid's defaultBkg path (non-redux):
    //   `M0 {h-rd} v{-h + 2*rd} q0,-5,5,-5 h{w - 2*rd} q5,0,5,5 v{h - rd} H0 Z`
    let path = format!(
        "M0 {h_minus_rd} v{v_top} q0,-5,5,-5 h{h_edge} q5,0,5,5 v{v_body} H0 Z",
        h_minus_rd = fmt_num(h - rd),
        v_top = fmt_num(-h + 2.0 * rd),
        h_edge = fmt_num(w - 2.0 * rd),
        v_body = fmt_num(h - rd),
    );

    out.push_str("<g>");
    // LR calls `drawNode(..., diagramId)`; TD calls `drawNode(...)`
    // without it (see timelineRendererVertical.ts), which makes the
    // interpolated `diagramId + '-node-' + n` string literally start
    // with `"undefined"` in the SVG. Byte-exact parity keeps the quirk.
    let node_id_prefix = match d.direction {
        TimelineDirection::LR => id.to_string(),
        TimelineDirection::TD => "undefined".to_string(),
    };
    out.push_str(&format!(
        r#"<path id="{pfx}-node-{nid}" class="node-bkg node-undefined" d="{path_d}"></path>"#,
        pfx = node_id_prefix,
        nid = node.node_id,
        path_d = path,
    ));
    // The `node-line-N` sibling that defaultBkg adds.
    let line_class = line_class_name(node.section_index);
    out.push_str(&format!(
        r#"<line class="{lc}" x1="0" y1="{y}" x2="{w}" y2="{y}"></line>"#,
        lc = line_class,
        y = fmt_num(h),
        w = fmt_num(w),
    ));
    out.push_str("</g>");

    // Text group — upstream applies `translate(width/2, padding/2)`.
    let pad = node_padding(d, node);
    out.push_str(&format!(
        r#"<g transform="translate({x}, {y})"><text dy="1em" alignment-baseline="middle" dominant-baseline="middle" text-anchor="middle">"#,
        x = fmt_num(w / 2.0),
        y = fmt_num(pad / 2.0),
    ));
    for (i, line) in node.lines.iter().enumerate() {
        if i == 0 {
            out.push_str(&format!(
                r#"<tspan x="0" dy="1em">{}</tspan>"#,
                escape_text(line),
            ));
        } else {
            out.push_str(&format!(
                r#"<tspan x="0" dy="1.1em">{}</tspan>"#,
                escape_text(line),
            ));
        }
    }
    out.push_str("</text></g>");

    let _ = body_inner_w;
}

fn node_padding(d: &TimelineDiagram, node: &LaidNode) -> f64 {
    match (d.direction, node.kind) {
        (TimelineDirection::LR, _) => crate::layout::timeline::LR_NODE_PADDING,
        (TimelineDirection::TD, _) => crate::layout::timeline::TD_NODE_PADDING,
    }
}

fn section_class_name(idx: i32) -> String {
    // Upstream maps `fullSection % THEME_COLOR_LIMIT - 1` into the
    // section class. The CSS emits `.section--1` for the fallback
    // slot. We already did the modular arithmetic in layout, so just
    // stringify the signed index.
    format!("section-{idx}")
}

fn line_class_name(idx: i32) -> String {
    format!("node-line-{idx}")
}

/// Mermaid-style stylis-minified CSS block. Driven by the theme.
fn build_style_block(
    id: &str,
    _d: &TimelineDiagram,
    theme: &ThemeVariables,
    l: &TimelineLayout,
) -> String {
    let font_family_min = minify_font_family(&l.font_family_css);
    let font_size = &l.font_size_css;
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let error_bkg = theme.error_bkg_color.as_deref().unwrap_or("#552222");
    let error_text = theme.error_text_color.as_deref().unwrap_or("#552222");
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");
    let stroke_width = theme.stroke_width.unwrap_or(1);
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let drop_shadow = theme
        .drop_shadow
        .as_deref()
        .unwrap_or("drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))");
    let git0 = theme.git0.as_deref().unwrap_or("#ECECFF");
    let git_branch_label0 = theme.git_branch_label0.as_deref().unwrap_or("#ffffff");

    let theme_color_limit = theme.theme_color_limit.unwrap_or(12) as usize;

    let mut css = String::with_capacity(6144);
    css.push_str(&format!(
        "<style>#{id}{{font-family:{font_family_min};font-size:{font_size};fill:{text_color};}}"
    ));
    css.push_str("@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}");
    css.push_str("@keyframes dash{to{stroke-dashoffset:0;}}");
    css.push_str(&format!("#{id} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}"));
    css.push_str(&format!("#{id} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}"));
    css.push_str(&format!("#{id} .error-icon{{fill:{error_bkg};}}"));
    css.push_str(&format!(
        "#{id} .error-text{{fill:{error_text};stroke:{error_text};}}"
    ));
    css.push_str(&format!(
        "#{id} .edge-thickness-normal{{stroke-width:{stroke_width}px;}}"
    ));
    css.push_str(&format!(
        "#{id} .edge-thickness-thick{{stroke-width:3.5px;}}"
    ));
    css.push_str(&format!("#{id} .edge-pattern-solid{{stroke-dasharray:0;}}"));
    css.push_str(&format!(
        "#{id} .edge-thickness-invisible{{stroke-width:0;fill:none;}}"
    ));
    css.push_str(&format!(
        "#{id} .edge-pattern-dashed{{stroke-dasharray:3;}}"
    ));
    css.push_str(&format!(
        "#{id} .edge-pattern-dotted{{stroke-dasharray:2;}}"
    ));
    css.push_str(&format!(
        "#{id} .marker{{fill:{line_color};stroke:{line_color};}}"
    ));
    css.push_str(&format!("#{id} .marker.cross{{stroke:{line_color};}}"));
    css.push_str(&format!(
        "#{id} svg{{font-family:{font_family_min};font-size:{font_size};}}"
    ));
    css.push_str(&format!("#{id} p{{margin:0;}}"));
    css.push_str(&format!("#{id} .edge{{stroke-width:3;}}"));

    // Per-section palette rules (0..THEME_COLOR_LIMIT).
    for i in 0..theme_color_limit {
        let idx = i as i64 - 1; // upstream indexes section classes from -1.
        let c_scale = c_scale_value(theme, i);
        let c_scale_inv = c_scale_inv_value(theme, i);
        let c_scale_label = c_scale_label_value(theme, i);
        let sw = 17 - 3 * i as i32;
        css.push_str(&format!(
            "#{id} .section-{idx} rect,#{id} .section-{idx} path,#{id} .section-{idx} circle,#{id} .section-{idx} path{{fill:{c_scale};}}"
        ));
        css.push_str(&format!(
            "#{id} .section-{idx} text{{fill:{c_scale_label};}}"
        ));
        css.push_str(&format!(
            "#{id} .node-icon-{idx}{{font-size:40px;color:{c_scale_label};}}"
        ));
        css.push_str(&format!("#{id} .section-edge-{idx}{{stroke:{c_scale};}}"));
        css.push_str(&format!("#{id} .edge-depth-{idx}{{stroke-width:{sw};}}"));
        css.push_str(&format!(
            "#{id} .section-{idx} line{{stroke:{c_scale_inv};stroke-width:3;}}"
        ));
        css.push_str(&format!(
            "#{id} .lineWrapper line{{stroke:{c_scale_label};}}"
        ));
        css.push_str(&format!(
            "#{id} .disabled,#{id} .disabled circle,#{id} .disabled text{{fill:{};}}",
            theme.tertiary_color.as_deref().unwrap_or("lightgray")
        ));
        css.push_str(&format!(
            "#{id} .disabled text{{fill:{};}}",
            theme.cluster_border.as_deref().unwrap_or("#efefef")
        ));
    }

    // Gradient sections — upstream emits these when `useGradient` is
    // true and the theme is NOT `neutral`. They're the "neo look"
    // overlay rules that swap the section's rect/path/circle fill to
    // `mainBkg` and re-point strokes at `url(#{id}-gradient)`.
    //
    // Neutral has `useGradient: true` too but the gradient block is
    // skipped for it upstream (`!isNeutralTheme`). We detect neutral
    // by its signature `primaryColor: "#eee"` to avoid piping the
    // theme *name* through the render layer.
    let is_neutral = theme.primary_color.as_deref() == Some("#eee");
    if theme.use_gradient.unwrap_or(false) && !is_neutral {
        let main_bkg = theme.main_bkg.as_deref().unwrap_or("#eee");
        for i in 0..theme_color_limit {
            let idx = i as i64 - 1;
            css.push_str(&format!(
                r#"#{id} .section-{idx}[data-look="neo"] rect,#{id} .section-{idx}[data-look="neo"] path,#{id} .section-{idx}[data-look="neo"] circle{{fill:{main_bkg};stroke:url(#{id}-gradient);stroke-width:2;}}"#
            ));
            css.push_str(&format!(
                r#"#{id} .section-{idx}[data-look="neo"] line{{stroke:url(#{id}-gradient);stroke-width:2;}}"#
            ));
        }
    }

    css.push_str(&format!(
        "#{id} .section-root rect,#{id} .section-root path,#{id} .section-root circle{{fill:{git0};}}"
    ));
    css.push_str(&format!(
        "#{id} .section-root text{{fill:{git_branch_label0};}}"
    ));
    css.push_str(&format!("#{id} .icon-container{{height:100%;display:flex;justify-content:center;align-items:center;}}"));
    css.push_str(&format!("#{id} .edge{{fill:none;}}"));
    css.push_str(&format!("#{id} .eventWrapper{{filter:brightness(120%);}}"));
    css.push_str(&format!("#{id} .node .neo-node{{stroke:{node_border};}}"));
    // Upstream's `styles.ts` swaps several `stroke`/`fill` values to
    // `url(#{id}-gradient)` whenever `useGradient` is true — applies
    // regardless of diagram theme (including neutral, unlike the
    // section-loop gradient block).
    let gradient_url = format!("url(#{id}-gradient)");
    let border = if theme.use_gradient.unwrap_or(false) {
        gradient_url.as_str()
    } else {
        node_border
    };
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node rect,#{id} [data-look="neo"].cluster rect,#{id} [data-look="neo"].node polygon{{stroke:{border};filter:{drop_shadow};}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node path{{stroke:{border};stroke-width:{stroke_width}px;}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node .outer-path{{filter:{drop_shadow};}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node .neo-line path{{stroke:{node_border};filter:none;}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle{{stroke:{border};filter:{drop_shadow};}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle .state-start{{fill:#000000;}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon{{fill:{border};filter:{drop_shadow};}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon-neo path{{stroke:{border};filter:{drop_shadow};}}"#
    ));
    css.push_str(&format!(
        "#{id} :root{{--mermaid-font-family:{font_family_min};}}"
    ));
    css.push_str("</style>");
    css
}

fn c_scale_value(theme: &ThemeVariables, i: usize) -> String {
    match i {
        0 => theme.c_scale0.clone().unwrap_or_default(),
        1 => theme.c_scale1.clone().unwrap_or_default(),
        2 => theme.c_scale2.clone().unwrap_or_default(),
        3 => theme.c_scale3.clone().unwrap_or_default(),
        4 => theme.c_scale4.clone().unwrap_or_default(),
        5 => theme.c_scale5.clone().unwrap_or_default(),
        6 => theme.c_scale6.clone().unwrap_or_default(),
        7 => theme.c_scale7.clone().unwrap_or_default(),
        8 => theme.c_scale8.clone().unwrap_or_default(),
        9 => theme.c_scale9.clone().unwrap_or_default(),
        10 => theme.c_scale10.clone().unwrap_or_default(),
        11 => theme.c_scale11.clone().unwrap_or_default(),
        _ => String::new(),
    }
}
fn c_scale_inv_value(theme: &ThemeVariables, i: usize) -> String {
    match i {
        0 => theme.c_scale_inv0.clone().unwrap_or_default(),
        1 => theme.c_scale_inv1.clone().unwrap_or_default(),
        2 => theme.c_scale_inv2.clone().unwrap_or_default(),
        3 => theme.c_scale_inv3.clone().unwrap_or_default(),
        4 => theme.c_scale_inv4.clone().unwrap_or_default(),
        5 => theme.c_scale_inv5.clone().unwrap_or_default(),
        6 => theme.c_scale_inv6.clone().unwrap_or_default(),
        7 => theme.c_scale_inv7.clone().unwrap_or_default(),
        8 => theme.c_scale_inv8.clone().unwrap_or_default(),
        9 => theme.c_scale_inv9.clone().unwrap_or_default(),
        10 => theme.c_scale_inv10.clone().unwrap_or_default(),
        11 => theme.c_scale_inv11.clone().unwrap_or_default(),
        _ => String::new(),
    }
}
fn c_scale_label_value(theme: &ThemeVariables, i: usize) -> String {
    match i {
        0 => theme.c_scale_label0.clone().unwrap_or_default(),
        1 => theme.c_scale_label1.clone().unwrap_or_default(),
        2 => theme.c_scale_label2.clone().unwrap_or_default(),
        3 => theme.c_scale_label3.clone().unwrap_or_default(),
        4 => theme.c_scale_label4.clone().unwrap_or_default(),
        5 => theme.c_scale_label5.clone().unwrap_or_default(),
        6 => theme.c_scale_label6.clone().unwrap_or_default(),
        7 => theme.c_scale_label7.clone().unwrap_or_default(),
        8 => theme.c_scale_label8.clone().unwrap_or_default(),
        9 => theme.c_scale_label9.clone().unwrap_or_default(),
        10 => theme.c_scale_label10.clone().unwrap_or_default(),
        11 => theme.c_scale_label11.clone().unwrap_or_default(),
        _ => String::new(),
    }
}

fn minify_font_family(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_quote = false;
    let mut prev_comma = false;
    for c in s.chars() {
        if c == '"' {
            in_quote = !in_quote;
            out.push(c);
            prev_comma = false;
            continue;
        }
        if !in_quote {
            if c == ',' {
                out.push(c);
                prev_comma = true;
                continue;
            }
            if prev_comma && c == ' ' {
                prev_comma = false;
                continue;
            }
        }
        out.push(c);
        prev_comma = false;
    }
    out
}

fn escape_text(s: &str) -> String {
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

/// Mirrors pie's `fmt_num` — integers print as integers, floats with
/// Rust's shortest round-trip representation (which matches JavaScript
/// `Number.toString()` in the range mermaid actually uses).
pub fn fmt_num(x: f64) -> String {
    if x.fract() == 0.0 && x.is_finite() {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}
