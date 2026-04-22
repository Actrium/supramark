//! Journey (user-journey) SVG renderer — byte-exact parity with
//! upstream mermaid@11.14.0.
//!
//! Upstream reference: `packages/mermaid/src/diagrams/user-journey/journeyRenderer.ts`
//! and `svgDraw.js`.
//!
//! Output structure (in emit order):
//!
//! 1. `<svg ...>` opening tag. Attribute order: `id → width → xmlns →
//!    style → viewBox → preserveAspectRatio → height → role →
//!    aria-roledescription`.
//! 2. `<style>…</style>` — the standard mermaid stylesheet + journey
//!    task-type/section-type fills, all scoped under `#<id>`.
//! 3. `<g></g>` — the empty seed group d3 creates first.
//! 4. `<defs><marker>…</marker></defs>` — arrowhead.
//! 5. For each actor in the sorted legend: a `<circle>` and one
//!    `<text>` per wrapped line.
//! 6. For each task: a `<g>` wrapping:
//!    - section `<g><rect/><switch>…</switch></g>` when the section
//!      changes, followed by
//!    - task `<g><line/><circle class=face/><g>eyes+mouth</g><rect/>
//!      <circle actor/>…<switch>…</switch></g>`.
//! 7. Title `<text>`.
//! 8. Activity-line `<line marker-end=…>`.
//! 9. `</svg>`.

use crate::error::Result;
use crate::layout::journey::{
    JourneyActorLegend, JourneyFace, JourneyLayout, JourneySectionLayout, JourneyTaskLayout,
};
use crate::model::journey::JourneyDiagram;
use crate::theme::ThemeVariables;

const JOURNEY_CSS_TEMPLATE: &str = concat!(
    "#{{ID}}{font-family:\"trebuchet ms\",verdana,arial,sans-serif;font-size:16px;fill:#333;}",
    "@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}",
    "@keyframes dash{to{stroke-dashoffset:0;}}",
    "#{{ID}} .edge-animation-slow{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}",
    "#{{ID}} .edge-animation-fast{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}",
    "#{{ID}} .error-icon{fill:#552222;}",
    "#{{ID}} .error-text{fill:#552222;stroke:#552222;}",
    "#{{ID}} .edge-thickness-normal{stroke-width:1px;}",
    "#{{ID}} .edge-thickness-thick{stroke-width:3.5px;}",
    "#{{ID}} .edge-pattern-solid{stroke-dasharray:0;}",
    "#{{ID}} .edge-thickness-invisible{stroke-width:0;fill:none;}",
    "#{{ID}} .edge-pattern-dashed{stroke-dasharray:3;}",
    "#{{ID}} .edge-pattern-dotted{stroke-dasharray:2;}",
    "#{{ID}} .marker{fill:#333333;stroke:#333333;}",
    "#{{ID}} .marker.cross{stroke:#333333;}",
    "#{{ID}} svg{font-family:\"trebuchet ms\",verdana,arial,sans-serif;font-size:16px;}",
    "#{{ID}} p{margin:0;}",
    "#{{ID}} .label{font-family:\"trebuchet ms\",verdana,arial,sans-serif;color:#333;}",
    "#{{ID}} .mouth{stroke:#666;}",
    "#{{ID}} line{stroke:#333;}",
    "#{{ID}} .legend{fill:#333;font-family:\"trebuchet ms\",verdana,arial,sans-serif;}",
    "#{{ID}} .label text{fill:#333;}",
    "#{{ID}} .label{color:#333;}",
    "#{{ID}} .face{fill:#FFF8DC;stroke:#999;}",
    "#{{ID}} .node rect,#{{ID}} .node circle,#{{ID}} .node ellipse,#{{ID}} .node polygon,#{{ID}} .node path{fill:#ECECFF;stroke:#9370DB;stroke-width:1px;}",
    "#{{ID}} .node .label{text-align:center;}",
    "#{{ID}} .node.clickable{cursor:pointer;}",
    "#{{ID}} .arrowheadPath{fill:#333333;}",
    "#{{ID}} .edgePath .path{stroke:#333333;stroke-width:1.5px;}",
    "#{{ID}} .flowchart-link{stroke:#333333;fill:none;}",
    "#{{ID}} .edgeLabel{background-color:rgba(232,232,232, 0.8);text-align:center;}",
    "#{{ID}} .edgeLabel rect{opacity:0.5;}",
    "#{{ID}} .cluster text{fill:#333;}",
    "#{{ID}} div.mermaidTooltip{position:absolute;text-align:center;max-width:200px;padding:2px;font-family:\"trebuchet ms\",verdana,arial,sans-serif;font-size:12px;background:hsl(80, 100%, 96.2745098039%);border:1px solid #aaaa33;border-radius:2px;pointer-events:none;z-index:100;}",
    "#{{ID}} .task-type-0,#{{ID}} .section-type-0{fill:#ECECFF;}",
    "#{{ID}} .task-type-1,#{{ID}} .section-type-1{fill:#ffffde;}",
    "#{{ID}} .task-type-2,#{{ID}} .section-type-2{fill:hsl(304, 100%, 96.2745098039%);}",
    "#{{ID}} .task-type-3,#{{ID}} .section-type-3{fill:hsl(124, 100%, 93.5294117647%);}",
    "#{{ID}} .task-type-4,#{{ID}} .section-type-4{fill:hsl(176, 100%, 96.2745098039%);}",
    "#{{ID}} .task-type-5,#{{ID}} .section-type-5{fill:hsl(-4, 100%, 93.5294117647%);}",
    "#{{ID}} .task-type-6,#{{ID}} .section-type-6{fill:hsl(8, 100%, 96.2745098039%);}",
    "#{{ID}} .task-type-7,#{{ID}} .section-type-7{fill:hsl(188, 100%, 93.5294117647%);}",
    "#{{ID}} .label-icon{display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;}",
    "#{{ID}} .node .label-icon path{fill:currentColor;stroke:revert;stroke-width:revert;}",
    "#{{ID}} .node .neo-node{stroke:#9370DB;}",
    "#{{ID}} [data-look=\"neo\"].node rect,#{{ID}} [data-look=\"neo\"].cluster rect,#{{ID}} [data-look=\"neo\"].node polygon{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}",
    "#{{ID}} [data-look=\"neo\"].node path{stroke:#9370DB;stroke-width:1px;}",
    "#{{ID}} [data-look=\"neo\"].node .outer-path{filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}",
    "#{{ID}} [data-look=\"neo\"].node .neo-line path{stroke:#9370DB;filter:none;}",
    "#{{ID}} [data-look=\"neo\"].node circle{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}",
    "#{{ID}} [data-look=\"neo\"].node circle .state-start{fill:#000000;}",
    "#{{ID}} [data-look=\"neo\"].icon-shape .icon{fill:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}",
    "#{{ID}} [data-look=\"neo\"].icon-shape .icon-neo path{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}",
    "#{{ID}} :root{--mermaid-font-family:\"trebuchet ms\",verdana,arial,sans-serif;}",
);

pub fn render(
    d: &JourneyDiagram,
    l: &JourneyLayout,
    _theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(JOURNEY_CSS_TEMPLATE.len() + 4096);

    // 1. SVG opening. Attribute order observed in reference fixtures:
    //    id, width="100%", xmlns, style="max-width: Npx;", viewBox,
    //    preserveAspectRatio, height, role, aria-roledescription
    //    [, aria-describedby, aria-labelledby].
    let vb_width = l.width;
    let vb_height = l.height + l.title_extra;
    let h_attr = vb_height + 25.0;
    let has_acc_title = d.meta.acc_title.is_some();
    let has_acc_descr = d.meta.acc_descr.is_some();
    let mut aria_extras = String::new();
    if has_acc_descr {
        aria_extras.push_str(&format!(r#" aria-describedby="chart-desc-{id}""#));
    }
    if has_acc_title {
        aria_extras.push_str(&format!(r#" aria-labelledby="chart-title-{id}""#));
    }
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: {w}px;" viewBox="0 -25 {w} {h}" preserveAspectRatio="xMinYMin meet" height="{ha}" role="graphics-document document" aria-roledescription="journey"{ax}>"#,
        id = id,
        w = fmt_num(vb_width),
        h = fmt_num(vb_height),
        ha = fmt_num(h_attr),
        ax = aria_extras,
    ));

    // Inline <title> / <desc> appear BEFORE <style>, in the order
    // `<title>` first then `<desc>` (as upstream `addSVGAccessibilityFields`).
    if let Some(t) = &d.meta.acc_title {
        out.push_str(&format!(
            r#"<title id="chart-title-{id}">{t}</title>"#,
            id = id,
            t = escape_text(t),
        ));
    }
    if let Some(desc) = &d.meta.acc_descr {
        out.push_str(&format!(
            r#"<desc id="chart-desc-{id}">{t}</desc>"#,
            id = id,
            t = escape_text(desc),
        ));
    }

    // 2. Style block.
    out.push_str("<style>");
    out.push_str(&JOURNEY_CSS_TEMPLATE.replace("{{ID}}", id));
    out.push_str("</style>");

    // 3. Seed empty <g>.
    out.push_str("<g></g>");

    // 4. <defs><marker ... arrowhead ...
    out.push_str(&format!(
        r#"<defs><marker id="{id}-arrowhead" refX="5" refY="2" markerWidth="6" markerHeight="4" orient="auto"><path d="M 0,0 V 4 L6,2 Z"></path></marker></defs>"#,
        id = id,
    ));

    // 5. Actor legend.
    for actor in &l.actors {
        render_actor_legend(&mut out, actor);
    }

    // 6. Tasks — interleaved with section headers.
    //    Upstream `drawTasks` loop emits the section <g> when the
    //    section changes, then emits the task <g>. Our layout records
    //    the sections in appearance order; we walk tasks and, whenever
    //    the task is the first of a new section, emit the section
    //    first.
    let mut section_iter = l.sections.iter().peekable();
    let mut last_section: Option<String> = None;
    for task in &l.tasks {
        let task_section = &d.tasks[task.index].section;
        let new_section = match &last_section {
            None => true,
            Some(s) => s != task_section,
        };
        if new_section {
            if let Some(sec) = section_iter.next() {
                render_section(&mut out, sec, task.y, l, id);
            }
            last_section = Some(task_section.clone());
        }
        render_task(&mut out, task, l, id);
    }

    // 7. Title text.
    if l.has_title {
        out.push_str(&format!(
            r#"<text x="{x}" font-size="{fs}" font-weight="bold" y="25" fill="{fill}" font-family="{ff}">{title}</text>"#,
            x = fmt_num(l.left_margin),
            fs = &l.title_font_size,
            fill = &l.title_color,
            ff = escape_attr_family(&l.title_font_family),
            title = escape_text(&l.title),
        ));
    }

    // 8. Activity arrow line.
    out.push_str(&format!(
        "<line x1=\"{x1}\" y1=\"{y}\" x2=\"{x2}\" y2=\"{y}\" stroke-width=\"4\" stroke=\"black\" marker-end=\"url(#{id}-arrowhead)\"></line>",
        x1 = fmt_num(l.arrow_x1),
        x2 = fmt_num(l.arrow_x2),
        y = fmt_num(l.arrow_y),
        id = id,
    ));

    // 9. Close svg.
    out.push_str("</svg>");
    Ok(out)
}

fn render_actor_legend(out: &mut String, a: &JourneyActorLegend) {
    // <circle cx="20" cy="{yPos}" class="actor-{pos}" fill="{colour}" stroke="#000" r="7"></circle>
    out.push_str(&format!(
        "<circle cx=\"20\" cy=\"{cy}\" class=\"actor-{pos}\" fill=\"{fill}\" stroke=\"#000\" r=\"7\"></circle>",
        cy = fmt_num(a.circle_cy),
        pos = a.pos,
        fill = &a.colour,
    ));
    // Each line: <text x="40" y="{ly}" class="legend"><tspan x="50">{line}</tspan></text>
    for line in &a.lines {
        out.push_str(&format!(
            r#"<text x="40" y="{y}" class="legend"><tspan x="50">{t}</tspan></text>"#,
            y = fmt_num(line.y),
            t = escape_text(&line.text),
        ));
    }
}

fn render_section(
    out: &mut String,
    sec: &JourneySectionLayout,
    _task_y: f64,
    _l: &JourneyLayout,
    _id: &str,
) {
    // Upstream drawSection wraps in <g> then draws rect + _drawTextCandidateFunc.
    // Section y=50, height=conf.height=50, rx=3, ry=3.
    let sec_y = 50.0;
    let sec_h = 50.0;
    out.push_str("<g>");
    // <rect x y fill stroke width height rx ry class=journey-section section-type-N>
    out.push_str(&format!(
        "<rect x=\"{x}\" y=\"{sy}\" fill=\"{fill}\" stroke=\"#666\" width=\"{w}\" height=\"{h}\" rx=\"3\" ry=\"3\" class=\"journey-section section-type-{n}\"></rect>",
        x = fmt_num(sec.x),
        sy = fmt_num(sec_y),
        fill = &sec.fill,
        w = fmt_num(sec.width),
        h = fmt_num(sec_h),
        n = sec.num,
    ));
    // byFo: <switch><foreignObject …><div…>…</div></foreignObject><text>…<tspan…>…</tspan></text></switch>
    emit_fo_label(
        out,
        sec.x,
        sec_y,
        sec.width,
        sec_h,
        &format!("journey-section section-type-{}", sec.num),
        &format!("journey-section section-type-{}", sec.num),
        &sec.text,
    );
    out.push_str("</g>");
}

fn render_task(out: &mut String, task: &JourneyTaskLayout, l: &JourneyLayout, id: &str) {
    // <g><line task-line/>...</g>
    out.push_str("<g>");
    // Vertical dashed line through the center.
    let center = task.x + 150.0 / 2.0; // conf.width / 2
    let max_height = 300.0 + 5.0 * 30.0; // 450
    out.push_str(&format!(
        "<line id=\"{id}-task{i}\" x1=\"{cx}\" y1=\"{y}\" x2=\"{cx}\" y2=\"{mh}\" class=\"task-line\" stroke-width=\"1px\" stroke-dasharray=\"4 2\" stroke=\"#666\"></line>",
        id = id,
        i = task.index,
        cx = fmt_num(center),
        y = fmt_num(task.y),
        mh = fmt_num(max_height),
    ));
    // Face.
    render_face(out, center, task.face_cy, task.face);

    // Task rect.
    let w = 150.0;
    let h = 50.0;
    out.push_str(&format!(
        "<rect x=\"{x}\" y=\"{y}\" fill=\"{fill}\" stroke=\"#666\" width=\"{w}\" height=\"{h}\" rx=\"3\" ry=\"3\" class=\"task task-type-{n}\"></rect>",
        x = fmt_num(task.x),
        y = fmt_num(task.y),
        fill = &task.fill,
        w = fmt_num(w),
        h = fmt_num(h),
        n = task.num,
    ));

    // Actor circles along top edge.
    for chip in &task.actor_circles {
        // <circle cx="{cx}" cy="{y}" class="actor-{pos}" fill="{colour}" stroke="#000" r="7"><title>{name}</title></circle>
        out.push_str(&format!(
            "<circle cx=\"{cx}\" cy=\"{cy}\" class=\"actor-{pos}\" fill=\"{fill}\" stroke=\"#000\" r=\"7\"><title>{title}</title></circle>",
            cx = fmt_num(chip.cx),
            cy = fmt_num(task.y),
            pos = chip.pos,
            fill = &chip.colour,
            title = escape_text(&chip.title),
        ));
    }

    // Task label via foreignObject+text (byFo) — class is just "task" here.
    emit_fo_label(out, task.x, task.y, w, h, "task", "task", &task.task);

    out.push_str("</g>");

    let _ = l;
}

fn render_face(out: &mut String, cx: f64, cy: f64, face: JourneyFace) {
    // <circle cx cy class="face" r="15" stroke-width="2" overflow="visible"></circle>
    out.push_str(&format!(
        r#"<circle cx="{cx}" cy="{cy}" class="face" r="15" stroke-width="2" overflow="visible"></circle>"#,
        cx = fmt_num(cx),
        cy = fmt_num(cy),
    ));
    // <g>eyes + mouth</g>
    out.push_str("<g>");
    // radius/3 = 5 (since r=15). Eyes at (cx-5, cy-5) and (cx+5, cy-5), r=1.5.
    let eye_r = 1.5;
    let eye_dx = 5.0; // radius/3
    let eye_dy = 5.0; // radius/3
    let lx = cx - eye_dx;
    let ex = cx + eye_dx;
    let ey = cy - eye_dy;
    out.push_str(&format!(
        "<circle cx=\"{x}\" cy=\"{y}\" r=\"{r}\" stroke-width=\"2\" fill=\"#666\" stroke=\"#666\"></circle>",
        x = fmt_num(lx),
        y = fmt_num(ey),
        r = fmt_num(eye_r),
    ));
    out.push_str(&format!(
        "<circle cx=\"{x}\" cy=\"{y}\" r=\"{r}\" stroke-width=\"2\" fill=\"#666\" stroke=\"#666\"></circle>",
        x = fmt_num(ex),
        y = fmt_num(ey),
        r = fmt_num(eye_r),
    ));
    // Mouth: smile / sad / ambivalent.
    match face {
        JourneyFace::Smile => {
            // d3 arc with startAngle=PI/2, endAngle=3*PI/2, innerRadius=r/2=7.5,
            // outerRadius=r/2.2≈6.818. d attribute:
            // "M7.5,0A7.5,7.5,0,1,1,-7.5,0L-6.818,0A6.818,6.818,0,1,0,6.818,0Z"
            // transform translate(cx, cy+2).
            out.push_str(&format!(
                r#"<path class="mouth" d="M7.5,0A7.5,7.5,0,1,1,-7.5,0L-6.818,0A6.818,6.818,0,1,0,6.818,0Z" transform="translate({cx},{y})"></path>"#,
                cx = fmt_num(cx),
                y = fmt_num(cy + 2.0),
            ));
        }
        JourneyFace::Sad => {
            // startAngle=3PI/2, endAngle=5PI/2 → inverted.
            // d: "M-7.5,0A7.5,7.5,0,1,1,7.5,0L6.818,0A6.818,6.818,0,1,0,-6.818,0Z"
            // transform translate(cx, cy+7).
            out.push_str(&format!(
                r#"<path class="mouth" d="M-7.5,0A7.5,7.5,0,1,1,7.5,0L6.818,0A6.818,6.818,0,1,0,-6.818,0Z" transform="translate({cx},{y})"></path>"#,
                cx = fmt_num(cx),
                y = fmt_num(cy + 7.0),
            ));
        }
        JourneyFace::Ambivalent => {
            // <line class=mouth stroke=#666 x1 y1 x2 y2 stroke-width=1px>
            out.push_str(&format!(
                "<line class=\"mouth\" stroke=\"#666\" x1=\"{x1}\" y1=\"{y}\" x2=\"{x2}\" y2=\"{y}\" stroke-width=\"1px\"></line>",
                x1 = fmt_num(cx - 5.0),
                x2 = fmt_num(cx + 5.0),
                y = fmt_num(cy + 7.0),
            ));
        }
        JourneyFace::None => {}
    }
    out.push_str("</g>");
}

/// Emit the upstream `byFo` <switch>/<foreignObject>+<text> structure
/// for a centered label.
///
/// Upstream emit order (observed in fixture 02):
/// `<switch><foreignObject x y width height position=fixed>
///   <div style="display: table; height: 100%; width: 100%;" class="{div_class}">
///     <div class="label" style="display: table-cell; text-align: center; vertical-align: middle;">{content}</div>
///   </div>
/// </foreignObject>
/// <text x={x+w/2} y={y+h/2} style="text-anchor: middle; font-family: &quot;Open Sans&quot;, sans-serif;" dominant-baseline="central" alignment-baseline="central" class="{text_class}">
///   <tspan x={x+w/2} dy="0">{content}</tspan>
/// </text></switch>`
fn emit_fo_label(
    out: &mut String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    div_class: &str,
    text_class: &str,
    content: &str,
) {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    out.push_str(&format!(
        r#"<switch><foreignObject x="{x}" y="{y}" width="{w}" height="{h}" position="fixed">"#,
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    // Note: upstream appends xhtml:div but the serializer writes it as
    // plain <div> (without the xhtml: prefix) because the default
    // namespace is already xhtml inside foreignObject.
    out.push_str(&format!(
        r#"<div style="display: table; height: 100%; width: 100%;" class="{dc}">"#,
        dc = div_class,
    ));
    out.push_str(&format!(
        r#"<div class="label" style="display: table-cell; text-align: center; vertical-align: middle;">{c}</div></div></foreignObject>"#,
        c = escape_text(content),
    ));
    // <text>
    out.push_str(&format!(
        r#"<text x="{cx}" y="{cy}" style="text-anchor: middle; font-family: &quot;Open Sans&quot;, sans-serif;" dominant-baseline="central" alignment-baseline="central" class="{tc}"><tspan x="{cx}" dy="0">{c}</tspan></text></switch>"#,
        cx = fmt_num(cx),
        cy = fmt_num(cy),
        tc = text_class,
        c = escape_text(content),
    ));
}

fn fmt_num(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.fract() == 0.0 {
        return format!("{}", v as i64);
    }
    // Match JS Number-to-string: no trailing zeros, up to 17 digits of
    // precision. f64's default Display is very close to JS behaviour.
    format!("{}", v)
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr_family(s: &str) -> String {
    // Double-quote inside an attribute must become &quot;.
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
