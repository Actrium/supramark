//! Ishikawa hand-drawn variant — produced when the source declares
//! `%%{init: { 'look': 'handDrawn' } }%%`. Routes every line, arrow,
//! head shape, and label box through the rough.js port in
//! [`crate::render::rough`], which carries a Lehmer LCG seeded from
//! `handDrawnSeed` so the emitted bezier control points round-trip
//! byte-for-byte with upstream `roughjs@4.6.x`.
//!
//! The output structure mirrors upstream `ishikawaRenderer.ts::draw`:
//!
//! 1. `<g class="ishikawa-head-group" transform="translate(0,spineY)">`
//!    - rough head: `<g class="ishikawa-head"><path/><path/></g>` (the
//!      class is upstream's `insert(roughNode, ':first-child')
//!      .attr('class', 'ishikawa-head')` override).
//!    - head label `<text>` (no rough).
//! 2. one `<g class="ishikawa-pair">` per pair, containing per-cause:
//!    - rough branch line: `<g class="ishikawa-branch"><path/></g>`.
//!    - rough arrow marker: `<g><path/><path/></g>` (no class — the
//!      arrow falls under the parent `pair`/`sub-group`).
//!    - `<g class="ishikawa-label-group">`:
//!      - rough rectangle (hachure fill): `<g class="ishikawa-label-box">…</g>`.
//!      - label `<text>`.
//!    - per descendant sub-branch:
//!      - `<g class="ishikawa-sub-group">`:
//!        - rough sub-branch: `<g class="ishikawa-sub-branch"><path/></g>`.
//!        - rough arrow: `<g>…</g>`.
//!        - sub-branch `<text>`.
//! 3. final `<g class="ishikawa-spine"><path/></g>` — emitted AFTER the
//!    pair loop in the handDrawn variant (upstream defers its spine
//!    `drawLine` until `spineX` is final).
//!
//! The viewBox is derived from `bbox_of_sets` over every rough OpSet
//! plus the text/line/rect coordinates for non-rough geometry.

use crate::error::Result;
use crate::font_metrics::text_width;
use crate::layout::ishikawa::{Branch, IshikawaLayout, Pair, SubBranch};
use crate::model::ishikawa::IshikawaDiagram;
use crate::render::rough::{
    bbox_of_sets, path_out_to_svg, to_paths, OpSet, RoughGenerator, RoughOptions,
};
use crate::theme::ThemeVariables;

use super::{build_style_block, html_escape, js_num};

const BBOX_FAMILY: &str = "sans-serif";
const BBOX_FONT_SIZE: f64 = 14.0;

/// Emit the full handDrawn SVG. Mirrors the non-handDrawn `render`
/// function's `<svg>` envelope but routes every primitive through
/// rough.js.
pub(super) fn render(
    d: &IshikawaDiagram,
    l: &IshikawaLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let seed = d.hand_drawn_seed.unwrap_or(0);
    let line_color = theme.line_color.clone().unwrap_or_else(|| "#333333".into());
    let fill_color = theme.main_bkg.clone().unwrap_or_else(|| "#fff".into());

    let mut body = String::with_capacity(16384);
    let mut all_sets: Vec<OpSet> = Vec::with_capacity(64);
    let mut text_boxes: Vec<(f64, f64, f64, f64)> = Vec::new();

    let mut rc = RoughGenerator::new();

    if l.has_root {
        emit_head(
            &mut body,
            l,
            &mut rc,
            seed,
            &line_color,
            &fill_color,
            &mut all_sets,
            &mut text_boxes,
        );

        for pair in &l.pairs {
            body.push_str(r#"<g class="ishikawa-pair">"#);
            if let Some(b) = &pair.upper {
                emit_branch(
                    &mut body,
                    pair,
                    b,
                    l,
                    &mut rc,
                    seed,
                    &line_color,
                    &fill_color,
                    &mut all_sets,
                    &mut text_boxes,
                );
            }
            if let Some(b) = &pair.lower {
                emit_branch(
                    &mut body,
                    pair,
                    b,
                    l,
                    &mut rc,
                    seed,
                    &line_color,
                    &fill_color,
                    &mut all_sets,
                    &mut text_boxes,
                );
            }
            body.push_str("</g>");
        }

        // Final spine — deferred until after the pair loop in the
        // handDrawn variant. Upstream emits this with `(spineX, spineY,
        // 0, spineY)`. spineX has been folded into `l.spine_x_left`
        // (always 0 in jsdom because text bboxes have x=0).
        emit_rough_line(
            &mut body,
            l.spine_x_left,
            l.spine_y,
            0.0,
            l.spine_y,
            "ishikawa-spine",
            &mut rc,
            seed,
            &line_color,
            &mut all_sets,
        );
    }

    // ── ViewBox: union of every rough bbox + every text bbox.
    // Mirrors upstream `applyPaddedViewBox`, which calls `getBBox()` on
    // the SVG element. The jsdom shim ignores transforms, so the head
    // text/path bboxes contribute in their LOCAL frame (around 0,0).
    let mut xmin = f64::INFINITY;
    let mut ymin = f64::INFINITY;
    let mut xmax = f64::NEG_INFINITY;
    let mut ymax = f64::NEG_INFINITY;
    let mut have = false;
    if let Some((x0, y0, x1, y1)) = bbox_of_sets(&all_sets) {
        xmin = xmin.min(x0);
        ymin = ymin.min(y0);
        xmax = xmax.max(x1);
        ymax = ymax.max(y1);
        have = true;
    }
    for (x, y, w, h) in text_boxes.iter().copied() {
        if w == 0.0 && h == 0.0 {
            continue;
        }
        have = true;
        xmin = xmin.min(x);
        ymin = ymin.min(y);
        xmax = xmax.max(x + w);
        ymax = ymax.max(y + h);
    }
    let (vx, vy, vw, vh) = if have {
        let pad = l.padding;
        (
            xmin - pad,
            ymin - pad,
            (xmax - xmin) + pad * 2.0,
            (ymax - ymin) + pad * 2.0,
        )
    } else {
        (-l.padding, -l.padding, l.padding * 2.0, l.padding * 2.0)
    };

    let mut out = String::with_capacity(body.len() + 4096);
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: {mw}px;" viewBox="{vx} {vy} {vw} {vh}" role="graphics-document document" aria-roledescription="ishikawa">"#,
        id = id,
        mw = js_num(vw),
        vx = js_num(vx),
        vy = js_num(vy),
        vw = js_num(vw),
        vh = js_num(vh),
    ));
    out.push_str(&build_style_block(id, theme));
    out.push_str("<g></g>");
    out.push_str(r#"<g class="ishikawa">"#);
    out.push_str(&body);
    out.push_str("</g></svg>");
    Ok(out)
}

// ── Head ─────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn emit_head(
    out: &mut String,
    l: &IshikawaLayout,
    rc: &mut RoughGenerator,
    seed: i32,
    line_color: &str,
    fill_color: &str,
    all_sets: &mut Vec<OpSet>,
    text_boxes: &mut Vec<(f64, f64, f64, f64)>,
) {
    out.push_str(&format!(
        r#"<g class="ishikawa-head-group" transform="translate(0,{y})">"#,
        y = js_num(l.spine_y),
    ));

    // The head path matches upstream verbatim:
    //   `M 0 ${-h/2} L 0 ${h/2} Q ${w*2.4} 0 0 ${-h/2} Z`
    let h = l.head_h;
    let w = l.head_w;
    let head_path = format!(
        "M 0 {} L 0 {} Q {} 0 0 {} Z",
        js_num(-h / 2.0),
        js_num(h / 2.0),
        js_num(w * 2.4),
        js_num(-h / 2.0),
    );

    let mut o = RoughOptions {
        roughness: 1.5,
        seed,
        fill: Some(fill_color.to_string()),
        fill_style: "hachure".into(),
        fill_weight: 2.5,
        hachure_gap: 5.0,
        stroke: line_color.to_string(),
        stroke_width: 2.0,
        ..RoughOptions::default()
    };
    o.fill_line_dash = Vec::new();
    o.stroke_line_dash = Vec::new();
    o.omit_dash_attrs = true;
    let drawable = rc.path(&head_path, &o);

    out.push_str(r#"<g class="ishikawa-head">"#);
    let paths = to_paths(&drawable, &o);
    for p in &paths {
        out.push_str(&path_out_to_svg(p));
    }
    out.push_str("</g>");

    for s in &drawable.sets {
        all_sets.push(s.clone());
    }

    // Head label.
    out.push_str(&format!(
        r#"<text class="ishikawa-head-label" text-anchor="start" x="0" y="{y}" transform="translate({tx},{ty})">"#,
        y = js_num(l.head_text_y),
        tx = js_num(l.head_text_x_shift),
        ty = js_num(l.head_text_y_shift),
    ));
    for (i, line) in l.head_text_lines.iter().enumerate() {
        let dy = if i == 0 { 0.0 } else { l.head_text_dy };
        out.push_str(&format!(
            r#"<tspan x="0" dy="{dy}">{t}</tspan>"#,
            dy = js_num(dy),
            t = html_escape(line),
        ));
    }
    out.push_str("</text></g>");

    let concat: String = l.head_text_lines.join("");
    let tb_w = text_width(&concat, BBOX_FAMILY, BBOX_FONT_SIZE, false, false);
    let tb_h = crate::font_metrics::line_height(BBOX_FAMILY, BBOX_FONT_SIZE, false, false);
    text_boxes.push((0.0, 0.0, tb_w, tb_h));
}

// ── Branch ───────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn emit_branch(
    out: &mut String,
    _pair: &Pair,
    b: &Branch,
    l: &IshikawaLayout,
    rc: &mut RoughGenerator,
    seed: i32,
    line_color: &str,
    fill_color: &str,
    all_sets: &mut Vec<OpSet>,
    text_boxes: &mut Vec<(f64, f64, f64, f64)>,
) {
    // Branch line.
    let (x0, y0) = b.start;
    let (x1, y1) = b.end;
    emit_rough_line(
        out,
        x0,
        y0,
        x1,
        y1,
        "ishikawa-branch",
        rc,
        seed,
        line_color,
        all_sets,
    );

    // Arrow marker — drawArrowMarker(g, startX=x0, startY=y0,
    // dx=x0-x1, dy=y0-y1).
    emit_arrow_marker(
        out,
        x0,
        y0,
        x0 - x1,
        y0 - y1,
        rc,
        seed,
        line_color,
        all_sets,
    );

    // Cause label group.
    out.push_str(r#"<g class="ishikawa-label-group">"#);
    let (rx, ry, rw, rh) = b.label_rect;
    let mut o = RoughOptions {
        roughness: 1.5,
        seed,
        fill: Some(fill_color.to_string()),
        fill_style: "hachure".into(),
        fill_weight: 2.5,
        hachure_gap: 5.0,
        stroke: line_color.to_string(),
        stroke_width: 2.0,
        ..RoughOptions::default()
    };
    o.fill_line_dash = Vec::new();
    o.stroke_line_dash = Vec::new();
    o.omit_dash_attrs = true;
    let drawable = rc.rectangle(rx, ry, rw, rh, &o);
    out.push_str(r#"<g class="ishikawa-label-box">"#);
    let paths = to_paths(&drawable, &o);
    for p in &paths {
        out.push_str(&path_out_to_svg(p));
    }
    out.push_str("</g>");
    for s in &drawable.sets {
        all_sets.push(s.clone());
    }

    // Cause label text.
    out.push_str(&format!(
        r#"<text class="ishikawa-label cause" text-anchor="middle" x="{x}" y="{y}">"#,
        x = js_num(b.label_text_x),
        y = js_num(b.label_text_y),
    ));
    for (i, line) in b.label_text.iter().enumerate() {
        let dy = if i == 0 { 0.0 } else { b.label_text_dy };
        out.push_str(&format!(
            r#"<tspan x="{x}" dy="{dy}">{t}</tspan>"#,
            x = js_num(b.label_text_x),
            dy = js_num(dy),
            t = html_escape(line),
        ));
    }
    out.push_str("</text></g>");

    let concat: String = b.label_text.join("");
    let tb_w = text_width(&concat, BBOX_FAMILY, BBOX_FONT_SIZE, false, false);
    let tb_h = crate::font_metrics::line_height(BBOX_FAMILY, BBOX_FONT_SIZE, false, false);
    text_boxes.push((0.0, 0.0, tb_w, tb_h));

    // Sub-branches.
    for sb in &b.sub_branches {
        emit_sub(out, sb, b, l, rc, seed, line_color, all_sets, text_boxes);
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_sub(
    out: &mut String,
    sb: &SubBranch,
    _b: &Branch,
    _l: &IshikawaLayout,
    rc: &mut RoughGenerator,
    seed: i32,
    line_color: &str,
    all_sets: &mut Vec<OpSet>,
    text_boxes: &mut Vec<(f64, f64, f64, f64)>,
) {
    out.push_str(r#"<g class="ishikawa-sub-group">"#);

    let (sx1, sy1, sx2, sy2) = sb.line;
    emit_rough_line(
        out,
        sx1,
        sy1,
        sx2,
        sy2,
        "ishikawa-sub-branch",
        rc,
        seed,
        line_color,
        all_sets,
    );

    // Arrow marker tip is anchored at the bone START (sx1, sy1) — the
    // end at the parent diagonal/horizontal. Direction:
    //   even-depth (horizontal):  (dx, dy) = (1, 0).
    //   odd-depth  (diagonal):    (dx, dy) = (sx1-sx2, sy1-sy2).
    let is_horizontal = (sy2 - sy1).abs() < 1e-12;
    if is_horizontal {
        emit_arrow_marker(out, sx1, sy1, 1.0, 0.0, rc, seed, line_color, all_sets);
    } else {
        emit_arrow_marker(
            out,
            sx1,
            sy1,
            sx1 - sx2,
            sy1 - sy2,
            rc,
            seed,
            line_color,
            all_sets,
        );
    }

    // Sub-branch label text.
    out.push_str(&format!(
        r#"<text class="{cls}" text-anchor="end" x="{x}" y="{y}">"#,
        cls = sb.text_class,
        x = js_num(sb.text_x),
        y = js_num(sb.text_y),
    ));
    for (i, line) in sb.text_lines.iter().enumerate() {
        let dy = if i == 0 { 0.0 } else { sb.text_dy };
        out.push_str(&format!(
            r#"<tspan x="{x}" dy="{dy}">{t}</tspan>"#,
            x = js_num(sb.text_x),
            dy = js_num(dy),
            t = html_escape(line),
        ));
    }
    out.push_str("</text>");

    out.push_str("</g>");

    let concat: String = sb.text_lines.join("");
    let tb_w = text_width(&concat, BBOX_FAMILY, BBOX_FONT_SIZE, false, false);
    let tb_h = crate::font_metrics::line_height(BBOX_FAMILY, BBOX_FONT_SIZE, false, false);
    text_boxes.push((0.0, 0.0, tb_w, tb_h));
}

// ── Arrow marker ─────────────────────────────────────────────────────

/// Mirrors upstream `drawArrowMarker`. Produces the ~6-px solid-fill
/// triangle pointing along (-dx, -dy) anchored at (x, y). Emits
/// `<g><path/><path/></g>` (no class — the parent group provides the
/// semantic class).
#[allow(clippy::too_many_arguments)]
fn emit_arrow_marker(
    out: &mut String,
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    rc: &mut RoughGenerator,
    seed: i32,
    line_color: &str,
    all_sets: &mut Vec<OpSet>,
) {
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let s: f64 = 6.0;
    let px = -uy * s;
    let py = ux * s;
    let tip_x = x;
    let tip_y = y;
    let d = format!(
        "M {} {} L {} {} L {} {} Z",
        js_num(tip_x),
        js_num(tip_y),
        js_num(tip_x - ux * s * 2.0 + px),
        js_num(tip_y - uy * s * 2.0 + py),
        js_num(tip_x - ux * s * 2.0 - px),
        js_num(tip_y - uy * s * 2.0 - py),
    );

    let mut o = RoughOptions {
        roughness: 1.0,
        seed,
        fill: Some(line_color.to_string()),
        fill_style: "solid".into(),
        stroke: line_color.to_string(),
        stroke_width: 1.0,
        ..RoughOptions::default()
    };
    o.fill_line_dash = Vec::new();
    o.stroke_line_dash = Vec::new();
    o.omit_dash_attrs = true;
    let drawable = rc.path(&d, &o);
    out.push_str("<g>");
    let paths = to_paths(&drawable, &o);
    for p in &paths {
        out.push_str(&path_out_to_svg(p));
    }
    out.push_str("</g>");
    for s in &drawable.sets {
        all_sets.push(s.clone());
    }
}

// ── Rough line wrapper ───────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn emit_rough_line(
    out: &mut String,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    cls: &str,
    rc: &mut RoughGenerator,
    seed: i32,
    line_color: &str,
    all_sets: &mut Vec<OpSet>,
) {
    let mut o = RoughOptions {
        roughness: 1.5,
        seed,
        stroke: line_color.to_string(),
        stroke_width: 2.0,
        ..RoughOptions::default()
    };
    o.fill_line_dash = Vec::new();
    o.stroke_line_dash = Vec::new();
    o.omit_dash_attrs = true;
    let drawable = rc.line(x1, y1, x2, y2, &o);
    out.push_str(&format!(r#"<g class="{cls}">"#));
    let paths = to_paths(&drawable, &o);
    for p in &paths {
        out.push_str(&path_out_to_svg(p));
    }
    out.push_str("</g>");
    for s in &drawable.sets {
        all_sets.push(s.clone());
    }
}
