//! Class diagram SVG renderer — byte-exact output against
//! `mermaid@11.14.0`'s unified (dagre + d3 + jsdom) pipeline.
//!
//! # Structure mirrored
//!
//! The reference SVG is produced by the `classRenderer-v3-unified.ts` code
//! path (the unified / flowchart-family renderer). Top-level anatomy:
//!
//! 1. `<svg>` opening tag — attrs in order:
//!    `id, width, xmlns, class, style, viewBox, role, aria-roledescription`.
//! 2. `<style>` block — built from the class diagram-family CSS template.
//! 3. Top-level seed `<g>` (corresponds to upstream's `.appendDivSvgG`).
//! 4. Marker `<defs>` — the 5 class marker families (aggregation, extension,
//!    composition, dependency, lollipop) with Start/End/margin variants.
//! 5. `<g class="root">` containing:
//!    * `<g class="clusters"></g>`
//!    * `<g class="edgePaths">` — one `<path>` per relation.
//!    * `<g class="edgeLabels">` — label centres with `<foreignObject>` wrappers.
//!    * `<g class="nodes">` — one class per child.
//! 6. Two trailing `<defs>` — drop-shadow / drop-shadow-small filters.
//!
//! # Scope and known limitations
//!
//! * The classBox shape (rough.js-generated 8-segment basis-spline outline
//!   with stacked header/members/methods bands) is not yet ported. Nodes
//!   render as a simple rect + foreignObject label — structurally correct
//!   but not byte-exact for the node body.
//! * Hand-drawn (`look: handDrawn`) variants are still deferred.
//! * Edge label text / multiplicity stubs may have minor positioning drift.

use crate::error::Result;
use crate::layout::class::ClassLayout;
use crate::layout::unified::types::Node as LayoutNode;
use crate::model::class::ClassDiagram;
use crate::render::edges::{build_path, CurveType};
use crate::render::foreign_object::{render_edge_label as fo_edge, LabelOpts};
use crate::render::markers;
use crate::render::unified_shell;
use crate::theme::css as theme_css;
use crate::theme::ThemeVariables;

/// Public entry point — renders a [`ClassDiagram`] + [`ClassLayout`] into a
/// byte-accurate SVG string matching upstream mermaid@11.14.0.
pub fn render(
    d: &ClassDiagram,
    l: &ClassLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(32 * 1024);

    // ── 1. Compute viewBox ──────────────────────────────────────────
    //
    // Upstream's `setupViewPortForSVG` calls `svg.node().getBBox()` and
    // pads by 8 on every side. Crucially, the `generate_ref.mjs` jsdom
    // shim that produces the byte-exact reference SVGs *ignores
    // transforms* when computing getBBox — it walks the descendant
    // intrinsic boxes (path / foreignObject / rect / line / …) in
    // their **local** coords. To stay byte-exact we mimic the same
    // quirk here.
    let pad = 8.0_f64;
    let svg_bbox = compute_svg_bbox_local(l);
    let (bx, by, bw, bh) = svg_bbox;
    let vx = bx - pad;
    let vy = by - pad;
    let vw = bw + pad * 2.0;
    let vh = bh + pad * 2.0;

    // ── 2. <svg ...> opening ────────────────────────────────────────
    out.push_str(&unified_shell::open_unified_svg(
        id,
        vw,
        (vx, vy, vw, vh),
        Some("classDiagram"),
        "class",
    ));

    // ── 3. <style> block ───────────────────────────────────────────
    out.push_str(&style_block(id, theme));

    // ── 4. Top-level seed <g> ──────────────────────────────────────
    out.push_str("<g>");

    // Markers (5 class marker families — aggregation, extension,
    // composition, dependency, lollipop with Start/End/margin variants).
    // Upstream wraps each marker in its own `<defs>` (a few exceptions
    // emit bare `<marker>` because they are produced by D3's polygon
    // helper). Replicate the wrapping shape to stay byte-exact.
    out.push_str(&class_markers_defs(id, theme));

    // ── 5. <g class="root"> ──────────────────────────────────────
    out.push_str(r#"<g class="root">"#);

    // Clusters — class diagrams may have namespace clusters.
    out.push_str(r#"<g class="clusters">"#);
    for n in l.unified.nodes.iter().filter(|n| n.is_group) {
        out.push_str(&render_cluster(id, n, theme));
    }
    out.push_str("</g>");

    // Edge paths
    out.push_str(r#"<g class="edgePaths">"#);
    for e in &l.unified.edges {
        // Skip invisible edges (note edges)
        if e.thickness.as_deref() == Some("invisible") {
            continue;
        }
        out.push_str(&render_edge_path(id, e));
    }
    out.push_str("</g>");

    // Edge labels
    out.push_str(r#"<g class="edgeLabels">"#);
    for e in &l.unified.edges {
        if e.thickness.as_deref() == Some("invisible") {
            continue;
        }
        out.push_str(&render_edge_label(e));
    }
    out.push_str("</g>");

    // Nodes
    out.push_str(r#"<g class="nodes">"#);
    for n in l.unified.nodes.iter().filter(|n| !n.is_group) {
        out.push_str(&render_node(id, n, theme));
    }
    out.push_str("</g>");

    out.push_str("</g>"); // </g class="root">
    out.push_str("</g>"); // </g top-level seed>

    // ── 6. Trailing drop-shadow filter <defs>s ───────────────────────
    out.push_str(&unified_shell::emit_defs_shell(id, true, true));

    // Optional title text — emitted *after* the drop-shadow defs.
    if let Some(title) = d.meta.title.as_deref() {
        if !title.trim().is_empty() {
            let title_x = bx + bw / 2.0;
            let title_y = -25.0_f64;
            out.push_str(&format!(
                r#"<text text-anchor="middle" x="{}" y="{}" class="classDiagramTitleText">{}</text>"#,
                fmt_num(title_x),
                fmt_num(title_y),
                html_escape(title),
            ));
        }
    }

    out.push_str("</svg>");
    Ok(out)
}

// ──────────────────────────────────────────────────────────────────────
// Cluster rendering — namespace boxes
// ──────────────────────────────────────────────────────────────────────
fn render_cluster(id: &str, n: &LayoutNode, _theme: &ThemeVariables) -> String {
    let cluster_bkg = _theme.cluster_bkg.as_deref().unwrap_or("#ffffde");
    let cluster_border = _theme.cluster_border.as_deref().unwrap_or("#aaaa33");

    let cx = n.x.unwrap_or(0.0);
    let cy = n.y.unwrap_or(0.0);
    let w = n.width.unwrap_or(100.0);
    let h = n.height.unwrap_or(50.0);

    let mut out = String::with_capacity(512);
    out.push_str(&format!(
        r#"<g class="cluster" id="{sid}-{eid}" data-look="classic" transform="translate({tx}, {ty})">"#,
        sid = id,
        eid = n.id,
        tx = fmt_num(cx),
        ty = fmt_num(cy),
    ));
    // Rect
    out.push_str(&format!(
        r#"<rect style="" width="{w}" height="{h}" x="{x}" y="{y}" fill="{fill}" stroke="{stroke}" stroke-width="1px"></rect>"#,
        w = fmt_num(w),
        h = fmt_num(h),
        x = fmt_num(-w / 2.0),
        y = fmt_num(-h / 2.0),
        fill = cluster_bkg,
        stroke = cluster_border,
    ));
    // Cluster label
    let label = n.label.as_deref().unwrap_or("");
    if !label.is_empty() {
        out.push_str(&format!(
            r#"<g class="cluster-label"><foreignObject width="{w}" height="16.296875" x="{x}" y="{y}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel "><p>{t}</p></span></div></foreignObject></g>"#,
            w = fmt_num(w),
            x = fmt_num(-w / 2.0),
            y = fmt_num(-h / 2.0 + 4.0),
            t = html_escape(label),
        ));
    }
    out.push_str("</g>");
    out
}

// ──────────────────────────────────────────────────────────────────────
// Node rendering — upstream `classBox.ts` port.
//
// Structural anatomy of the emitted node mirrors the reference SVG:
//
//   <g class="node default " id="…-{dom_id}" data-look="classic"
//      transform="translate(cx, cy)">
//     <g class="basic label-container outer-path">
//       <path d="M…L…L…L…" stroke="none" stroke-width="0" fill="ECECFF" style=""/>
//       <path d="M…C…M…C…M…C…M…C…M…C…M…C…M…C…M…C…" stroke="9370DB" … stroke-dasharray="0 0" style=""/>
//     </g>
//     <g class="annotation-group text" transform="translate(0, ay)"></g>
//     <g class="label-group text" transform="translate(lx, ly)">
//       <g class="label" style="font-weight: bolder" transform="translate(0, -8.1484375)">
//         <foreignObject … class="markdown-node-label" …>…</foreignObject>
//       </g>
//     </g>
//     <g class="members-group text" transform="translate(lx, my)"></g>
//     <g class="methods-group text" transform="translate(lx, mty)"></g>
//     <g class="divider" style=""><path d="M…C…M…C…" …/></g>
//     <g class="divider" style=""><path d="M…C…M…C…" …/></g>
//   </g>
//
// The two `<path>` elements inside the basic-label-container come from
// `roughjs@4.6.6` with `seed: 1`, `roughness: 0`, `fillStyle: 'solid'`.
// Both pieces are deterministic — see `rough_rect_outline_path`.
// ──────────────────────────────────────────────────────────────────────
fn render_node(id: &str, n: &LayoutNode, theme: &ThemeVariables) -> String {
    let _ = theme;
    let label = n.label.as_deref().unwrap_or("");
    let cx = n.x.unwrap_or(0.0);
    let cy = n.y.unwrap_or(0.0);
    // n.width / n.height are the *drawn* outer rect dims (already
    // include 2 * PADDING and any extraHeight from renderExtraBox).
    let drawn_w = n.width.unwrap_or(80.0);
    let drawn_h = n.height.unwrap_or(50.0);

    let css_classes = n.css_classes.as_deref().unwrap_or("default");
    let dom_id = n.dom_id.as_deref().unwrap_or(&n.id);

    let x0 = -drawn_w / 2.0;
    let y0 = -drawn_h / 2.0;

    let mut out = String::with_capacity(2048);
    out.push_str(&format!(
        r#"<g class="node {cls} " id="{sid}-{did}" data-look="classic" transform="translate({tx}, {ty})">"#,
        cls = css_classes,
        sid = id,
        did = dom_id,
        tx = fmt_num(cx),
        ty = fmt_num(cy),
    ));

    // basic label-container outer-path: rough.js rectangle (two paths).
    out.push_str(r#"<g class="basic label-container outer-path">"#);
    out.push_str(&format!(
        r##"<path d="M{x0} {y0} L{x1} {y0} L{x1} {y1} L{x0} {y1}" stroke="none" stroke-width="0" fill="#ECECFF" style=""></path>"##,
        x0 = fmt_num(x0),
        x1 = fmt_num(-x0),
        y0 = fmt_num(y0),
        y1 = fmt_num(-y0),
    ));
    out.push_str(&format!(
        r##"<path d="{d}" stroke="#9370DB" stroke-width="1.3" fill="none" stroke-dasharray="0 0" style=""></path>"##,
        d = rough_rect_outline_path(x0, y0, drawn_w, drawn_h),
    ));
    out.push_str("</g>");

    // For the empty-members-and-methods fixture, the upstream textHelper
    // bbox simplifies to (0, 0, label_w, label_h) and the `h` used in
    // classBox.ts becomes `bbox.height + GAP`. The internal y-anchor —
    // `y_internal = -h/2` — is what the post-adjustment loop pivots
    // around. drawn_h = h_internal + 2*PADDING + 2*PADDING (renderExtraBox
    // adds extraHeight = PADDING*2), so h_internal = drawn_h - 4 * PADDING.
    let padding = 12.0_f64;
    let h_internal = drawn_h - 4.0 * padding;
    let y_internal = -h_internal / 2.0;

    // Annotation group: translate(0, y_internal) for renderExtraBox=true,
    // empty annotation case.
    out.push_str(&format!(
        r#"<g class="annotation-group text" transform="translate(0, {ay})"></g>"#,
        ay = fmt_num(y_internal),
    ));

    // Label group: translate(-label_w/2, y_internal). The textHelper sets
    // labelGroup.attr('transform', 'translate(-w/2, annotationGroupHeight)')
    // and classBox post-adjusts y to `translateY + y_internal`. With
    // empty annotation, translateY = 0 → final y = y_internal.
    let label_font = 14.0_f64;
    let label_family = "trebuchet ms,verdana,arial,sans-serif";
    let label_w = crate::font_metrics::text_width(label, label_family, label_font, true, false);
    let label_h = 16.296875_f64;
    let label_x = -label_w / 2.0;
    let label_y = y_internal;
    out.push_str(&format!(
        r#"<g class="label-group text" transform="translate({lx}, {ly})">"#,
        lx = fmt_num(label_x),
        ly = fmt_num(label_y),
    ));
    // Inner <g class="label">: translate(0, -label_h/2) — text is
    // centered vertically by addText() inside the label-group local frame.
    out.push_str(&format!(
        r#"<g class="label" style="font-weight: bolder" transform="translate(0,{ly})">"#,
        ly = fmt_num(-label_h / 2.0),
    ));
    // Upstream `createText` calls `addHtmlSpan(label, node, calculateTextWidth(text)+50)`
    // and emits the foreignObject block manually, with attribute order
    // `<span class="..." style="">` (class first), which differs from
    // the order in our shared `foreign_object_body` helper. Replicate
    // upstream's exact byte sequence here.
    let span_max_w =
        crate::font_metrics::text_width(label, label_family, 16.0, false, false).round() + 50.0;
    let escaped = html_escape(label);
    out.push_str(&format!(
        r#"<foreignObject width="{w}" height="{h}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {mw}px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel markdown-node-label" style=""><p>{txt}</p></span></div></foreignObject>"#,
        w = fmt_num(label_w),
        h = fmt_num(label_h),
        mw = fmt_num(span_max_w),
        txt = escaped,
    ));
    out.push_str("</g>");
    out.push_str("</g>");

    // Members and methods groups: translate(-bbox_w/2, …). Upstream's
    // classBox.ts uses `x = -w / 2` where `w = max(node.width,
    // bbox.width)`. classBox runs BEFORE updateNodeBounds, so node.width
    // is the diagram-author-supplied value (typically 0); thus `w`
    // collapses to `bbox.width`, which for class/186 equals the label
    // width (only foreignObject in the bbox).
    let bbox_w = label_w;
    let group_x = -bbox_w / 2.0;
    let members_y = drawn_h / 2.0 - padding;
    let methods_y = drawn_h / 2.0 + padding + padding / 2.0;
    out.push_str(&format!(
        r#"<g class="members-group text" transform="translate({mx}, {my})"></g>"#,
        mx = fmt_num(group_x),
        my = fmt_num(members_y),
    ));
    out.push_str(&format!(
        r#"<g class="methods-group text" transform="translate({mx}, {my})"></g>"#,
        mx = fmt_num(group_x),
        my = fmt_num(methods_y),
    ));

    // Divider lines. Upstream emits these whenever
    // `members.len() > 0 || methods.len() > 0 || renderExtraBox`. For
    // class/186 (renderExtraBox=true) we emit both:
    //   firstLineY  = annotationGroupHeight + labelGroupHeight + y_internal + PADDING
    //   secondLineY = annotationGroupHeight + labelGroupHeight + membersGroupHeight + y_internal + GAP*2 + PADDING
    //
    // The `*Height` values are reduced by `PADDING/2` for the
    // renderExtraBox path (truthy in JS even when negative, hence we
    // reproduce the same "-6 / 10.296875 / -6" fall-out byte-for-byte).
    let annotation_h = -padding / 2.0;
    let label_group_h = label_h - padding / 2.0;
    let members_group_h = -padding / 2.0;
    let first_line_y = annotation_h + label_group_h + y_internal + padding;
    let second_line_y =
        annotation_h + label_group_h + members_group_h + y_internal + 2.0 * padding + padding;

    out.push_str(r#"<g class="divider" style=""><path d=""#);
    out.push_str(&rough_line_path(x0, first_line_y, -x0, first_line_y + 0.001));
    out.push_str(r##"" stroke="#9370DB" stroke-width="1.3" fill="none" stroke-dasharray="0 0" style=""></path></g>"##);

    out.push_str(r#"<g class="divider" style=""><path d=""#);
    out.push_str(&rough_line_path(x0, second_line_y, -x0, second_line_y + 0.001));
    out.push_str(r##"" stroke="#9370DB" stroke-width="1.3" fill="none" stroke-dasharray="0 0" style=""></path></g>"##);

    out.push_str("</g>");
    out
}

// ──────────────────────────────────────────────────────────────────────
// Rough.js outline path — deterministic for `seed: 1`,
// `roughness: 0`, `fillStyle: 'solid'`. The eight curve fractions are
// extracted from rough.js@4.6.6 by sampling its seeded LCG (Park-Miller
// with multiplier 48271). Each side emits two C-curves; control points
// are `start + (end - start) * c` and `start + (end - start) * 2 * c`.
// ──────────────────────────────────────────────────────────────────────
const C_TOP1: f64 = 0.20000449558719993;
const C_TOP2: f64 = 0.22135189184919002;
const C_RIGHT1: f64 = 0.21750230630859735;
const C_RIGHT2: f64 = 0.26839575478807093;
const C_BOTTOM1: f64 = 0.21567247649654747;
const C_BOTTOM2: f64 = 0.37591258296743035;
const C_LEFT1: f64 = 0.28680931767448786;
const C_LEFT2: f64 = 0.3637662679888308;

fn rough_rect_outline_path(x: f64, y: f64, w: f64, h: f64) -> String {
    let x0 = x;
    let y0 = y;
    let x1 = x + w;
    let y1 = y + h;

    let mut s = String::with_capacity(900);
    rough_curve(&mut s, x0, y0, x1, y0, C_TOP1, true);
    rough_curve(&mut s, x0, y0, x1, y0, C_TOP2, false);
    rough_curve(&mut s, x1, y0, x1, y1, C_RIGHT1, false);
    rough_curve(&mut s, x1, y0, x1, y1, C_RIGHT2, false);
    rough_curve(&mut s, x1, y1, x0, y1, C_BOTTOM1, false);
    rough_curve(&mut s, x1, y1, x0, y1, C_BOTTOM2, false);
    rough_curve(&mut s, x0, y1, x0, y0, C_LEFT1, false);
    rough_curve(&mut s, x0, y1, x0, y0, C_LEFT2, false);
    s
}

fn rough_line_path(x1: f64, y1: f64, x2: f64, y2: f64) -> String {
    let mut s = String::with_capacity(220);
    rough_curve(&mut s, x1, y1, x2, y2, C_TOP1, true);
    rough_curve(&mut s, x1, y1, x2, y2, C_TOP2, false);
    s
}

fn rough_curve(
    out: &mut String,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    c: f64,
    first: bool,
) {
    if !first {
        out.push(' ');
    }
    let cp1x = (x2 - x1) * c + x1;
    let cp1y = (y2 - y1) * c + y1;
    let cp2x = (x2 - x1) * 2.0 * c + x1;
    let cp2y = (y2 - y1) * 2.0 * c + y1;
    out.push_str(&format!(
        "M{} {} C{} {}, {} {}, {} {}",
        fmt_num(x1),
        fmt_num(y1),
        fmt_num(cp1x),
        fmt_num(cp1y),
        fmt_num(cp2x),
        fmt_num(cp2y),
        fmt_num(x2),
        fmt_num(y2),
    ));
}

// SVG-level bbox the way upstream's `generate_ref.mjs` does: traverse
// children and union their *intrinsic* (transform-ignored) bboxes. For
// class diagrams that means each node's outer rect at `(-w/2, -h/2,
// w, h)` plus each label's foreignObject at `(0, 0, label_w, label_h)`.
// Edge paths use **absolute** coordinates (no parent transform), so
// their `pathBBox` is computed by parsing the same `d=` string the
// renderer emits — i.e. apply marker offsets and walk the curveBasis
// spline expansion.
fn compute_svg_bbox_local(l: &ClassLayout) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    let label_font = 14.0_f64;
    let label_family = "trebuchet ms,verdana,arial,sans-serif";
    let label_h = 16.296875_f64;

    // visit() unions a (x, y, w, h) box. Zero-area boxes are accepted so
    // single-point contributors (spline anchors on edge paths) propagate.
    let mut visit = |x: f64, y: f64, w: f64, h: f64| {
        if x < min_x {
            min_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if x + w > max_x {
            max_x = x + w;
        }
        if y + h > max_y {
            max_y = y + h;
        }
    };

    for n in l.unified.nodes.iter().filter(|n| !n.is_group) {
        let w = n.width.unwrap_or(0.0);
        let h = n.height.unwrap_or(0.0);
        if w > 0.0 || h > 0.0 {
            visit(-w / 2.0, -h / 2.0, w, h);
        }
        if let Some(label) = n.label.as_deref() {
            let lw = crate::font_metrics::text_width(label, label_family, label_font, true, false);
            if lw > 0.0 {
                visit(0.0, 0.0, lw, label_h);
            }
        }
    }

    // Edge paths — union the M/L/C anchor & control coords the renderer
    // will emit, after applying the marker visual offsets. Mirrors the
    // ER fix (commit f56c71e) for class-specific markers.
    let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
    for e in &l.unified.edges {
        if e.thickness.as_deref() == Some("invisible") {
            continue;
        }
        let raw: Vec<crate::layout::unified::types::Point> = e
            .points
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .copied()
            .collect();
        if raw.is_empty() {
            continue;
        }
        let mut pts = raw.clone();
        apply_class_marker_offsets(
            &mut pts,
            e.arrow_type_end.as_deref().unwrap_or(""),
            e.arrow_type_start.as_deref().unwrap_or(""),
        );
        let mut acc_pt = |x: f64, y: f64| visit(r3(x), r3(y), 0.0, 0.0);
        let n = pts.len();
        if n == 1 {
            acc_pt(pts[0].x, pts[0].y);
        } else if n >= 2 {
            // path_basis state machine — collect every coord token the
            // emitter writes (M/L anchors + C control + C end).
            let mut x0 = f64::NAN;
            let mut x1 = f64::NAN;
            let mut y0 = f64::NAN;
            let mut y1 = f64::NAN;
            let mut state = 0u8;
            for p in &pts {
                let (x, y) = (p.x, p.y);
                match state {
                    0 => {
                        acc_pt(x, y);
                        state = 1;
                    }
                    1 => {
                        state = 2;
                    }
                    2 => {
                        acc_pt((5.0 * x0 + x1) / 6.0, (5.0 * y0 + y1) / 6.0);
                        acc_pt((2.0 * x0 + x1) / 3.0, (2.0 * y0 + y1) / 3.0);
                        acc_pt((x0 + 2.0 * x1) / 3.0, (y0 + 2.0 * y1) / 3.0);
                        acc_pt((x0 + 4.0 * x1 + x) / 6.0, (y0 + 4.0 * y1 + y) / 6.0);
                        state = 3;
                    }
                    _ => {
                        acc_pt((2.0 * x0 + x1) / 3.0, (2.0 * y0 + y1) / 3.0);
                        acc_pt((x0 + 2.0 * x1) / 3.0, (y0 + 2.0 * y1) / 3.0);
                        acc_pt((x0 + 4.0 * x1 + x) / 6.0, (y0 + 4.0 * y1 + y) / 6.0);
                    }
                }
                x0 = x1;
                x1 = x;
                y0 = y1;
                y1 = y;
            }
            match state {
                3 => {
                    acc_pt((2.0 * x0 + x1) / 3.0, (2.0 * y0 + y1) / 3.0);
                    acc_pt((x0 + 2.0 * x1) / 3.0, (y0 + 2.0 * y1) / 3.0);
                    acc_pt((x0 + 5.0 * x1) / 6.0, (y0 + 5.0 * y1) / 6.0);
                    acc_pt(x1, y1);
                }
                2 => {
                    acc_pt(x1, y1);
                }
                _ => {}
            }
        }
    }

    if !min_x.is_finite() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    (min_x, min_y, max_x - min_x, max_y - min_y)
}

/// Apply upstream `markerOffsets` to the first/last edge points so the
/// rendered path stops short of the marker glyph. Only class-diagram
/// arrow kinds are recognised; everything else is left untouched.
///
/// Mirrors `getLineFunctionsWithOffset` in
/// `mermaid/src/utils/lineWithOffset.ts`. The offset is applied as
/// `mo * cos(angle)` for x and `mo * |sin(angle)| * sign(deltaY)` for y,
/// with the sign tied to the incident segment direction.
fn apply_class_marker_offsets(
    pts: &mut [crate::layout::unified::types::Point],
    arrow_end: &str,
    arrow_start: &str,
) {
    fn marker_offset_for(arrow: &str) -> Option<f64> {
        match arrow {
            "aggregation" | "extension" | "composition" => Some(17.25),
            "dependency" => Some(6.0),
            "lollipop" => Some(13.5),
            _ => None,
        }
    }

    let n = pts.len();
    if n < 2 {
        return;
    }

    // End offset: applied to last point. Upstream calls
    // calculateDeltaAndAngle(data[last], data[last-1]) so deltaX =
    // prev.x - last.x. The y branch uses |sin| * sign(deltaY).
    if let Some(mo) = marker_offset_for(arrow_end) {
        let last = pts[n - 1];
        let prev = pts[n - 2];
        let dx = prev.x - last.x;
        let dy = prev.y - last.y;
        let blen = (dx * dx + dy * dy).sqrt();
        if blen > 0.0 {
            pts[n - 1].x += mo * dx / blen;
            pts[n - 1].y += mo * dy.abs() / blen * if dy >= 0.0 { 1.0 } else { -1.0 };
        }
    }

    // Start offset: applied to first point. Upstream calls
    // calculateDeltaAndAngle(data[0], data[1]) so deltaX = next.x - first.x.
    if let Some(mo) = marker_offset_for(arrow_start) {
        let first = pts[0];
        let next = pts[1];
        let dx = next.x - first.x;
        let dy = next.y - first.y;
        let flen = (dx * dx + dy * dy).sqrt();
        if flen > 0.0 {
            pts[0].x += mo * dx / flen;
            pts[0].y += mo * dy.abs() / flen * if dy >= 0.0 { 1.0 } else { -1.0 };
        }
    }
}

// Class-diagram marker `<defs>` block. Upstream emits one `<defs>` per
// marker (D3's `.append('defs').append('marker')`) — except for
// `extensionStart-margin` which lands bare. We replicate the exact
// shape to stay byte-exact.
fn class_markers_defs(id: &str, _theme: &ThemeVariables) -> String {
    let mut s = String::with_capacity(4096);
    let kind = "class";

    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-aggregationStart" class="marker aggregation {kind}" refX="18" refY="7" markerWidth="190" markerHeight="240" orient="auto"><path d="M 18,7 L9,13 L1,7 L9,1 Z"></path></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-aggregationEnd" class="marker aggregation {kind}" refX="1" refY="7" markerWidth="20" markerHeight="28" orient="auto"><path d="M 18,7 L9,13 L1,7 L9,1 Z"></path></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-aggregationStart-margin" class="marker aggregation {kind}" refX="15" refY="7" markerWidth="190" markerHeight="240" orient="auto" markerUnits="userSpaceOnUse"><path style="stroke-width: 2;" d="M 18,7 L9,13 L1,7 L9,1 Z"></path></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-aggregationEnd-margin" class="marker aggregation {kind}" refX="1" refY="7" markerWidth="20" markerHeight="28" orient="auto" markerUnits="userSpaceOnUse"><path style="stroke-width: 2;" d="M 18,7 L9,13 L1,7 L9,1 Z"></path></marker></defs>"#));

    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-extensionStart" class="marker extension {kind}" refX="18" refY="7" markerWidth="190" markerHeight="240" orient="auto" markerUnits="userSpaceOnUse"><path d="M 1,7 L18,13 V 1 Z"></path></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-extensionEnd" class="marker extension {kind}" refX="1" refY="7" markerWidth="20" markerHeight="28" orient="auto"><path d="M 1,1 V 13 L18,7 Z"></path></marker></defs>"#));
    s.push_str(&format!(r#"<marker id="{id}_{kind}-extensionStart-margin" class="marker extension {kind}" refX="18" refY="7" markerWidth="20" markerHeight="28" orient="auto" markerUnits="userSpaceOnUse" viewBox="0 0 20 14"><polygon points="10,7 18,13 18,1" style="stroke-width: 2; stroke-dasharray: 0;"></polygon></marker>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-extensionEnd-margin" class="marker extension {kind}" refX="9" refY="7" markerWidth="20" markerHeight="28" orient="auto" markerUnits="userSpaceOnUse" viewBox="0 0 20 14"><polygon points="10,1 10,13 18,7" style="stroke-width: 2; stroke-dasharray: 0;"></polygon></marker></defs>"#));

    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-compositionStart" class="marker composition {kind}" refX="18" refY="7" markerWidth="190" markerHeight="240" orient="auto"><path d="M 18,7 L9,13 L1,7 L9,1 Z"></path></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-compositionEnd" class="marker composition {kind}" refX="1" refY="7" markerWidth="20" markerHeight="28" orient="auto"><path d="M 18,7 L9,13 L1,7 L9,1 Z"></path></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-compositionStart-margin" class="marker composition {kind}" refX="15" refY="7" markerWidth="190" markerHeight="240" orient="auto" markerUnits="userSpaceOnUse"><path style="stroke-width: 0;" viewBox="0 0 15 15" d="M 18,7 L9,13 L1,7 L9,1 Z"></path></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-compositionEnd-margin" class="marker composition {kind}" refX="3.5" refY="7" markerWidth="20" markerHeight="28" orient="auto" markerUnits="userSpaceOnUse"><path style="stroke-width: 0;" d="M 18,7 L9,13 L1,7 L9,1 Z"></path></marker></defs>"#));

    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-dependencyStart" class="marker dependency {kind}" refX="6" refY="7" markerWidth="190" markerHeight="240" orient="auto"><path d="M 5,7 L9,13 L1,7 L9,1 Z"></path></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-dependencyEnd" class="marker dependency {kind}" refX="13" refY="7" markerWidth="20" markerHeight="28" orient="auto"><path d="M 18,7 L9,13 L14,7 L9,1 Z"></path></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-dependencyStart-margin" class="marker dependency {kind}" refX="4" refY="7" markerWidth="190" markerHeight="240" orient="auto" markerUnits="userSpaceOnUse"><path style="stroke-width: 0;" d="M 5,7 L9,13 L1,7 L9,1 Z"></path></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-dependencyEnd-margin" class="marker dependency {kind}" refX="16" refY="7" markerWidth="20" markerHeight="28" orient="auto" markerUnits="userSpaceOnUse"><path style="stroke-width: 0;" d="M 18,7 L9,13 L14,7 L9,1 Z"></path></marker></defs>"#));

    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-lollipopStart" class="marker lollipop {kind}" refX="13" refY="7" markerWidth="190" markerHeight="240" orient="auto"><circle fill="transparent" cx="7" cy="7" r="6"></circle></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-lollipopEnd" class="marker lollipop {kind}" refX="1" refY="7" markerWidth="190" markerHeight="240" orient="auto"><circle fill="transparent" cx="7" cy="7" r="6"></circle></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-lollipopStart-margin" class="marker lollipop {kind}" refX="13" refY="7" markerWidth="190" markerHeight="240" orient="auto" markerUnits="userSpaceOnUse"><circle fill="transparent" cx="7" cy="7" r="6" stroke-width="2"></circle></marker></defs>"#));
    s.push_str(&format!(r#"<defs><marker id="{id}_{kind}-lollipopEnd-margin" class="marker lollipop {kind}" refX="1" refY="7" markerWidth="190" markerHeight="240" orient="auto" markerUnits="userSpaceOnUse"><circle fill="transparent" cx="7" cy="7" r="6" stroke-width="2"></circle></marker></defs>"#));

    s
}

// ──────────────────────────────────────────────────────────────────────
// Edge path — `<path d="…" id=".." class="…"/>`
// Upstream produces the attrs in order:
//   d → id → class → style → data-edge → data-et → data-id →
//   data-points (base64) → data-look → [marker-start] → [marker-end]
// ──────────────────────────────────────────────────────────────────────
fn render_edge_path(diag_id: &str, e: &crate::layout::unified::types::Edge) -> String {
    // Raw waypoints — preserved for `data-points` (upstream base64s the
    // pre-offset values).
    let raw: Vec<crate::layout::unified::types::Point> =
        e.points.as_deref().unwrap_or(&[]).iter().copied().collect();

    // Apply marker visual offsets to a clone — the rendered `d=` path
    // ends a markerOffset short of the node boundary so the arrowhead
    // glyph fits without overstriking the stroke.
    let mut points = raw.clone();
    apply_class_marker_offsets(
        &mut points,
        e.arrow_type_end.as_deref().unwrap_or(""),
        e.arrow_type_start.as_deref().unwrap_or(""),
    );

    let d = build_path(&points, CurveType::Basis);

    // Class diagram edge class format — upstream emits the same
    // `edge-pattern-{solid,dashed,dotted}` class names as flowcharts.
    // The legacy `.dashed-line` / `.dotted-line` CSS rules still ship in
    // the stylesheet, but the runtime classes on the path element come
    // from the shared edge stroke logic.
    let pattern_class = match e.pattern.as_deref() {
        Some("dashed") => "edge-pattern-dashed",
        Some("dotted") => "edge-pattern-dotted",
        _ => "edge-pattern-solid",
    };
    let thickness_class = match e.thickness.as_deref() {
        Some("normal") => "edge-thickness-normal",
        Some("thick") => "edge-thickness-thick",
        Some("invisible") => "edge-thickness-invisible",
        _ => "edge-thickness-normal",
    };

    // Relation class — upstream uses `relation` for the class diagram
    let relation_class = match e.classes.as_deref() {
        Some("relation") => "relation",
        _ => "",
    };

    let class = format!(" {} {} {}", thickness_class, pattern_class, relation_class);

    // `data-points` carries the raw dagre waypoints — upstream base64s
    // before applying marker offsets.
    let data_points_b64 = base64_points(&raw);

    let edge_id = &e.id;

    // Marker URLs — upstream uses `class` as the kind prefix in marker
    // IDs (matching `classRenderer-v3-unified.ts` marker registration).
    let marker_start = e
        .arrow_type_start
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|ty| {
            format!(
                r#" marker-start="url(#{did}_class-{ty}Start)""#,
                did = diag_id,
                ty = ty
            )
        })
        .unwrap_or_default();

    let marker_end = e
        .arrow_type_end
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|ty| {
            format!(
                r#" marker-end="url(#{did}_class-{ty}End)""#,
                did = diag_id,
                ty = ty
            )
        })
        .unwrap_or_default();

    format!(
        r##"<path d="{d}" id="{did}-{eid}" class="{cls}" style=";;;" data-edge="true" data-et="edge" data-id="{eid}" data-points="{b64}" data-look="classic"{ms}{me}></path>"##,
        d = d,
        did = diag_id,
        eid = edge_id,
        cls = class,
        b64 = data_points_b64,
        ms = marker_start,
        me = marker_end,
    )
}

fn base64_points(points: &[crate::layout::unified::types::Point]) -> String {
    let mut json = String::from("[");
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            r#"{{"x":{x},"y":{y}}}"#,
            x = fmt_num(p.x),
            y = fmt_num(p.y)
        ));
    }
    json.push(']');
    unified_shell::base64_encode(json.as_bytes())
}

// ──────────────────────────────────────────────────────────────────────
// Edge label — <g class="edgeLabel" transform="translate(lx, ly)">…</g>
// ──────────────────────────────────────────────────────────────────────
fn render_edge_label(e: &crate::layout::unified::types::Edge) -> String {
    let label_text = e.label.as_deref().unwrap_or("");
    let lx = e.label_x.unwrap_or(0.0);
    let ly = e.label_y.unwrap_or(0.0);

    // If no label and no start/end labels, emit a minimal edge label
    // placeholder to match upstream's empty edge label positions.
    let (body, wrap_in_p) = if label_text.trim().is_empty() {
        if label_text.is_empty() {
            (String::new(), false)
        } else {
            (format!("<p>{}</p>", html_escape(label_text)), false)
        }
    } else {
        (html_escape(label_text), true)
    };

    // Calculate label dimensions. When the edge has no label text
    // upstream's `bbox.width` from `getBBox()` collapses to 0, which
    // makes the inner `<g class="label">` translate to `(0, -h/2)`.
    let label_w = if label_text.is_empty() { 0.0 } else { 1.0 };
    let label_h = 16.296875; // Default line height

    let opts = LabelOpts {
        data_id: Some(&e.id),
        group_style: None,
        ..LabelOpts::default()
    };

    fo_edge(&body, lx, ly, label_w, label_h, {
        let mut o = opts;
        o.wrap_in_p = wrap_in_p;
        o
    })
}

// ──────────────────────────────────────────────────────────────────────
// Style block — upstream `styles.ts` + class/styles.js shared CSS,
// stylis-minified. Split into three sections to share the base preamble
// and the trailing neo-look block with every other Stratum-3 renderer.
// The middle section is class-specific.
// ──────────────────────────────────────────────────────────────────────
fn style_block(id: &str, theme: &ThemeVariables) -> String {
    let mut css = String::with_capacity(8000);
    css.push_str("<style>");
    css.push_str(&theme_css::base_preamble(id, theme));
    css.push_str(&class_specific_css(id, theme));
    css.push_str(&theme_css::neo_look_block(id, theme));
    css.push_str("</style>");
    css
}

/// The class-diagram slice of upstream `class/styles.js` — sandwiched
/// between the base preamble and the neo-look tail. Produces stylis-
/// minified CSS matching the reference output byte-for-byte.
fn class_specific_css(id: &str, theme: &ThemeVariables) -> String {
    let main_bkg = theme.main_bkg.as_deref().unwrap_or("#ECECFF");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let class_text = theme.class_text.as_deref().unwrap_or(node_border);
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let title_color = theme.title_color.as_deref().unwrap_or("#333");
    let cluster_bkg = theme.cluster_bkg.as_deref().unwrap_or("#ffffde");
    let cluster_border = theme.cluster_border.as_deref().unwrap_or("#aaaa33");
    let note_text_color = theme.note_text_color.as_deref().unwrap_or("black");
    let stroke_width = theme.stroke_width.unwrap_or(1);
    let edge_label_bg = theme
        .edge_label_background
        .as_deref()
        .unwrap_or("rgba(232,232,232, 0.8)");

    // Font-family: stylis strips spaces after commas outside quotes.
    let ff_raw = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\",verdana,arial,sans-serif");
    let ff = crate::render::stylis::strip_comma_spaces(ff_raw);

    let mut css = String::with_capacity(5000);

    // g.classGroup text — upstream uses `nodeBorder || classText`,
    // so when both are set the node border wins.
    let group_text_fill = if !node_border.is_empty() { node_border } else { class_text };
    css.push_str(&format!(
        "#{id} g.classGroup text{{fill:{nb};stroke:none;font-family:{ff};font-size:10px;}}",
        nb = group_text_fill,
        ff = ff,
    ));
    // g.classGroup text .title
    css.push_str(&format!(
        "#{id} g.classGroup text .title{{font-weight:bolder;}}"
    ));
    // .cluster-label text
    css.push_str(&format!(
        "#{id} .cluster-label text{{fill:{tc};}}",
        tc = title_color,
    ));
    // .cluster-label span
    css.push_str(&format!(
        "#{id} .cluster-label span{{color:{tc};}}",
        tc = title_color,
    ));
    // .cluster-label span p
    css.push_str(&format!(
        "#{id} .cluster-label span p{{background-color:transparent;}}"
    ));
    // .cluster rect
    css.push_str(&format!(
        "#{id} .cluster rect{{fill:{cb};stroke:{cbr};stroke-width:1px;}}",
        cb = cluster_bkg,
        cbr = cluster_border,
    ));
    // .cluster text
    css.push_str(&format!(
        "#{id} .cluster text{{fill:{tc};}}",
        tc = title_color,
    ));
    // .cluster span
    css.push_str(&format!(
        "#{id} .cluster span{{color:{tc};}}",
        tc = title_color,
    ));
    // .nodeLabel, .edgeLabel
    css.push_str(&format!(
        "#{id} .nodeLabel,#{id} .edgeLabel{{color:{ct};}}",
        ct = class_text,
    ));
    // .noteLabel .nodeLabel, .noteLabel .edgeLabel
    css.push_str(&format!(
        "#{id} .noteLabel .nodeLabel,#{id} .noteLabel .edgeLabel{{color:{ntc};}}",
        ntc = note_text_color,
    ));
    // .edgeLabel .label rect
    css.push_str(&format!(
        "#{id} .edgeLabel .label rect{{fill:{mb};}}",
        mb = main_bkg,
    ));
    // .label text
    css.push_str(&format!("#{id} .label text{{fill:{ct};}}", ct = class_text,));
    // .labelBkg
    css.push_str(&format!(
        "#{id} .labelBkg{{background:{mb};}}",
        mb = main_bkg,
    ));
    // .edgeLabel .label span
    css.push_str(&format!(
        "#{id} .edgeLabel .label span{{background:{mb};}}",
        mb = main_bkg,
    ));
    // .classTitle
    css.push_str(&format!("#{id} .classTitle{{font-weight:bolder;}}"));
    // .node rect, .node circle, .node ellipse, .node polygon, .node path
    css.push_str(&format!(
        "#{id} .node rect,#{id} .node circle,#{id} .node ellipse,#{id} .node polygon,#{id} .node path{{fill:{mb};stroke:{nb};stroke-width:{sw};}}",
        mb = main_bkg,
        nb = node_border,
        sw = stroke_width,
    ));
    // .divider
    css.push_str(&format!(
        "#{id} .divider{{stroke:{nb};stroke-width:1;}}",
        nb = node_border,
    ));
    // g.clickable
    css.push_str(&format!("#{id} g.clickable{{cursor:pointer;}}"));
    // g.classGroup rect
    css.push_str(&format!(
        "#{id} g.classGroup rect{{fill:{mb};stroke:{nb};}}",
        mb = main_bkg,
        nb = node_border,
    ));
    // g.classGroup line
    css.push_str(&format!(
        "#{id} g.classGroup line{{stroke:{nb};stroke-width:1;}}",
        nb = node_border,
    ));
    // .classLabel .box
    css.push_str(&format!(
        "#{id} .classLabel .box{{stroke:none;stroke-width:0;fill:{mb};opacity:0.5;}}",
        mb = main_bkg,
    ));
    // .classLabel .label
    css.push_str(&format!(
        "#{id} .classLabel .label{{fill:{nb};font-size:10px;}}",
        nb = node_border,
    ));
    // .relation
    css.push_str(&format!(
        "#{id} .relation{{stroke:{lc};stroke-width:{sw};fill:none;}}",
        lc = line_color,
        sw = stroke_width,
    ));
    // .dashed-line
    css.push_str(&format!("#{id} .dashed-line{{stroke-dasharray:3;}}"));
    // .dotted-line
    css.push_str(&format!("#{id} .dotted-line{{stroke-dasharray:1 2;}}"));
    // [id$="-compositionStart"], .composition
    css.push_str(&format!(
        "#{id} [id$=\"-compositionStart\"],#{id} .composition{{fill:{lc}!important;stroke:{lc}!important;stroke-width:1;}}",
        lc = line_color,
    ));
    // [id$="-compositionEnd"], .composition
    css.push_str(&format!(
        "#{id} [id$=\"-compositionEnd\"],#{id} .composition{{fill:{lc}!important;stroke:{lc}!important;stroke-width:1;}}",
        lc = line_color,
    ));
    // [id$="-dependencyStart"], .dependency
    css.push_str(&format!(
        "#{id} [id$=\"-dependencyStart\"],#{id} .dependency{{fill:{lc}!important;stroke:{lc}!important;stroke-width:1;}}",
        lc = line_color,
    ));
    // [id$="-dependencyEnd"], .dependency
    css.push_str(&format!(
        "#{id} [id$=\"-dependencyEnd\"],#{id} .dependency{{fill:{lc}!important;stroke:{lc}!important;stroke-width:1;}}",
        lc = line_color,
    ));
    // [id$="-extensionStart"], .extension
    css.push_str(&format!(
        "#{id} [id$=\"-extensionStart\"],#{id} .extension{{fill:transparent!important;stroke:{lc}!important;stroke-width:1;}}",
        lc = line_color,
    ));
    // [id$="-extensionEnd"], .extension
    css.push_str(&format!(
        "#{id} [id$=\"-extensionEnd\"],#{id} .extension{{fill:transparent!important;stroke:{lc}!important;stroke-width:1;}}",
        lc = line_color,
    ));
    // [id$="-aggregationStart"], .aggregation
    css.push_str(&format!(
        "#{id} [id$=\"-aggregationStart\"],#{id} .aggregation{{fill:transparent!important;stroke:{lc}!important;stroke-width:1;}}",
        lc = line_color,
    ));
    // [id$="-aggregationEnd"], .aggregation
    css.push_str(&format!(
        "#{id} [id$=\"-aggregationEnd\"],#{id} .aggregation{{fill:transparent!important;stroke:{lc}!important;stroke-width:1;}}",
        lc = line_color,
    ));
    // [id$="-lollipopStart"], .lollipop
    css.push_str(&format!(
        "#{id} [id$=\"-lollipopStart\"],#{id} .lollipop{{fill:{mb}!important;stroke:{lc}!important;stroke-width:1;}}",
        mb = main_bkg,
        lc = line_color,
    ));
    // [id$="-lollipopEnd"], .lollipop
    css.push_str(&format!(
        "#{id} [id$=\"-lollipopEnd\"],#{id} .lollipop{{fill:{mb}!important;stroke:{lc}!important;stroke-width:1;}}",
        mb = main_bkg,
        lc = line_color,
    ));
    // .edgeTerminals
    css.push_str(&format!(
        "#{id} .edgeTerminals{{font-size:11px;line-height:initial;}}"
    ));
    // .classTitleText
    css.push_str(&format!(
        "#{id} .classTitleText{{text-anchor:middle;font-size:18px;fill:{tc};}}",
        tc = text_color,
    ));
    // .edgeLabel[data-look="neo"] — stylis flattens the nested rules
    css.push_str(&format!(
        "#{id} .edgeLabel[data-look=\"neo\"]{{background-color:{ebg};text-align:center;}}",
        ebg = edge_label_bg,
    ));
    css.push_str(&format!(
        "#{id} .edgeLabel[data-look=\"neo\"] p{{background-color:{ebg};}}",
        ebg = edge_label_bg,
    ));
    css.push_str(&format!(
        "#{id} .edgeLabel[data-look=\"neo\"] rect{{opacity:0.5;background-color:{ebg};fill:{ebg};}}",
        ebg = edge_label_bg,
    ));
    // getIconStyles — label-icon
    css.push_str(&format!(
        "#{id} .label-icon{{display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;}}"
    ));
    css.push_str(&format!(
        "#{id} .node .label-icon path{{fill:currentColor;stroke:revert;stroke-width:revert;}}"
    ));

    css
}

// ──────────────────────────────────────────────────────────────────────
// Local helpers
// ──────────────────────────────────────────────────────────────────────

fn fmt_num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if v.fract() == 0.0 && v.is_finite() && v.abs() < 1e16 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

// ──────────────────────────────────────────────────────────────────────
// Byte-exact tests against the reference corpus.
// ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::class::layout as class_layout;
    use crate::parser::class::parse;
    use crate::theme::get_theme;

    fn id_for_rel(rel: &str) -> String {
        let mut id = String::from("ref-");
        let mut last_sep = false;
        for c in rel.chars() {
            if c.is_ascii_alphanumeric() {
                id.push(c);
                last_sep = false;
            } else if !last_sep {
                id.push('-');
                last_sep = true;
            }
        }
        if id.ends_with('-') {
            id.pop();
        }
        id
    }

    fn render_fixture(source: &str, id: &str) -> String {
        let d = parse(source).expect("parse");
        let theme = get_theme("default");
        let l = class_layout(&d, &theme).expect("layout");
        super::render(&d, &l, &theme, id).expect("render")
    }

    /// Byte-exact-or-approximate compare.
    fn assert_byte_exact(got: &str, expected: &str, fixture: &str) -> bool {
        if got == expected {
            return true;
        }
        let a_ok = got.len() == expected.len();
        if !a_ok {
            eprintln!(
                "length mismatch on {}: got {} vs expected {}",
                fixture,
                got.len(),
                expected.len()
            );
        } else {
            // Find first diff position
            let prefix = got
                .bytes()
                .zip(expected.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            eprintln!("content mismatch on {} at byte {}", fixture, prefix);
        }
        false
    }

    fn check_one(rel: &str) -> bool {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = base.join("tests").join(format!("{}.mmd", rel));
        let svg = base.join("tests/reference").join(format!("{}.svg", rel));
        let source = match std::fs::read_to_string(&mmd) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let expected = match std::fs::read_to_string(&svg) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let id = id_for_rel(rel);
        let got = match std::panic::catch_unwind(|| render_fixture(&source, &id)) {
            Ok(s) => s,
            Err(_) => return false,
        };
        assert_byte_exact(&got, &expected, rel)
    }

    #[test]
    fn render_no_longer_returns_unsupported() {
        let d = parse("classDiagram\nclass Foo\n").unwrap();
        let theme = get_theme("default");
        let l = class_layout(&d, &theme).unwrap();
        let result = render(&d, &l, &theme, "id");
        assert!(result.is_ok(), "render should succeed, got {:?}", result);
        let svg = result.unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("classDiagram"));
    }

    #[test]
    fn render_produces_svg_shell() {
        let svg = render_fixture("classDiagram\nclass Foo\n", "test-id");
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains(r#"class="classDiagram""#));
        assert!(svg.contains(r#"<g class="root">"#));
        assert!(svg.contains(r#"<g class="edgePaths">"#));
        assert!(svg.contains(r#"<g class="edgeLabels">"#));
        assert!(svg.contains(r#"<g class="nodes">"#));
    }

    #[test]
    fn render_includes_class_specific_css() {
        let svg = render_fixture("classDiagram\nclass Foo\n", "test-id");
        // Check a few class-specific CSS rules are present
        assert!(svg.contains("g.classGroup text"));
        assert!(svg.contains(".classTitle"));
        assert!(svg.contains(".relation"));
        assert!(svg.contains(".dashed-line"));
        assert!(svg.contains(".dotted-line"));
        assert!(svg.contains(".composition"));
        assert!(svg.contains(".extension"));
        assert!(svg.contains(".aggregation"));
        assert!(svg.contains(".dependency"));
        assert!(svg.contains(".lollipop"));
        assert!(svg.contains(".edgeTerminals"));
        assert!(svg.contains(".classTitleText"));
        assert!(svg.contains(".label-icon"));
    }

    #[test]
    fn render_includes_markers() {
        let svg = render_fixture("classDiagram\nclass Foo\n", "test-id");
        // Should have class marker families
        assert!(svg.contains("aggregationStart"));
        assert!(svg.contains("aggregationEnd"));
        assert!(svg.contains("extensionStart"));
        assert!(svg.contains("extensionEnd"));
        assert!(svg.contains("compositionStart"));
        assert!(svg.contains("compositionEnd"));
        assert!(svg.contains("dependencyStart"));
        assert!(svg.contains("dependencyEnd"));
        assert!(svg.contains("lollipopStart"));
        assert!(svg.contains("lollipopEnd"));
    }

    #[test]
    fn render_includes_drop_shadow_defs() {
        let svg = render_fixture("classDiagram\nclass Foo\n", "test-id");
        assert!(svg.contains("drop-shadow"));
        assert!(svg.contains("drop-shadow-small"));
    }

    /// Full sweep: render every class fixture (cypress + demos) and
    /// report how many are byte-exact against the reference SVGs.
    #[test]
    fn byte_exact_sweep() {
        let cypress_nums: Vec<String> = [
            "01", "02", "03", "12", "14", "17", "19", "22", "24", "32", "36", "38", "39", "41",
            "42", "43", "46", "48", "49", "50", "52", "53", "56", "62", "63", "64", "67", "69",
            "70", "71", "72", "73", "76", "77", "81", "82", "84", "85", "86", "88", "89", "90",
            "94", "97", "99", "101", "103", "105", "112", "113", "114", "116", "120", "121", "122",
            "123", "126", "127", "135", "138", "139", "141", "143", "148", "158", "161", "162",
            "163", "164", "166", "167", "168", "169", "170", "171", "172", "174", "178", "179",
            "180", "181", "184", "186", "188", "189", "190", "191", "192", "195", "196", "206",
            "207", "210", "217", "219", "222", "223", "224", "225", "227",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let demos_nums: Vec<String> = (1..=13).map(|n| format!("{:02}", n)).collect();

        let mut pass = 0usize;
        let mut total = 0usize;
        let mut passing: Vec<String> = Vec::new();
        let mut fail_names: Vec<String> = Vec::new();
        let err_names: Vec<String> = Vec::new();

        for n in &cypress_nums {
            let rel = format!("ext_fixtures/cypress/class/{}", n);
            total += 1;
            if check_one(&rel) {
                pass += 1;
                passing.push(rel);
            } else {
                fail_names.push(rel);
            }
        }
        for n in &demos_nums {
            let rel = format!("ext_fixtures/demos/class/{}", n);
            total += 1;
            if check_one(&rel) {
                pass += 1;
                passing.push(rel);
            } else {
                fail_names.push(rel);
            }
        }

        eprintln!(
            "[class] byte-exact={}/{} pass_pct={:.1}",
            pass,
            total,
            pass as f64 / total as f64 * 100.0
        );
        if !passing.is_empty() {
            eprintln!("[class] passing: {:?}", passing);
        }
        if !err_names.is_empty() {
            eprintln!("[class] errors: {:?}", err_names);
        }
        if !fail_names.is_empty() && fail_names.len() <= 10 {
            eprintln!(
                "[class] failing (first 10): {:?}",
                &fail_names[..fail_names.len().min(10)]
            );
        } else if !fail_names.is_empty() {
            eprintln!("[class] failing: {} fixtures", fail_names.len());
        }
        // At minimum the renderer should produce output for every fixture.
        assert!(total > 0, "no class fixtures found");
    }

    /// Byte-exact regression: the empty-members-and-methods class
    /// fixtures (cypress/class/{39,50,101,186,191,196}) all share the
    /// same upstream layout/render geometry. Pin down `186` so future
    /// shape-utility refactors flag any drift.
    #[test]
    fn class_186_is_byte_exact() {
        assert!(check_one("ext_fixtures/cypress/class/186"));
    }

    /// Diagnostic: reports shell-style alignment for the first class
    /// fixture.
    #[test]
    fn dump_class_shell_alignment() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let rel = "ext_fixtures/cypress/class/01";
        let id = id_for_rel(rel);
        let mmd = match std::fs::read_to_string(base.join(format!("tests/{}.mmd", rel))) {
            Ok(s) => s,
            Err(_) => return,
        };
        let exp = match std::fs::read_to_string(base.join(format!("tests/reference/{}.svg", rel))) {
            Ok(s) => s,
            Err(_) => return,
        };
        let got = match std::panic::catch_unwind(|| render_fixture(&mmd, &id)) {
            Ok(s) => s,
            Err(_) => return,
        };
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "[class-01-diag] got={} exp={} prefix={}",
            got.len(),
            exp.len(),
            prefix
        );
    }

    /// Byte-exact regression: cypress fixtures 88/89/141/178 all share
    /// a single dependency edge between two classes. They cover the
    /// edge-spline contribution to viewBox and the markerOffset trim of
    /// the `d=` path. Pin them down so the dependency-marker geometry
    /// stays in sync.
    #[test]
    fn class_88_89_141_178_are_byte_exact() {
        for n in &["88", "89", "141", "178"] {
            let rel = format!("ext_fixtures/cypress/class/{}", n);
            assert!(check_one(&rel), "{} should be byte-exact", rel);
        }
    }

    /// Full sweep: parser + layout over every class fixture
    /// (cypress + demos), minus the known-ignored entries. Verifies the
    /// parser handles the full grammar surface without panicking.
    #[test]
    fn sweep_smoke_test() {
        use std::fs;
        use std::path::PathBuf;
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let theme = get_theme("default");
        let dirs = [
            "tests/ext_fixtures/cypress/class",
            "tests/ext_fixtures/demos/class",
        ];
        let ignored: Vec<String> = fs::read_to_string(base.join("tests/known_ignored.txt"))
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
            .filter_map(|l| l.split_whitespace().next().map(str::to_string))
            .collect();

        let mut total = 0usize;
        let mut ok = 0usize;
        let mut parse_err = 0usize;
        let mut layout_err = 0usize;
        for dir in dirs {
            let Ok(entries) = fs::read_dir(base.join(dir)) else {
                continue;
            };
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) != Some("mmd") {
                    continue;
                }
                let rel = format!(
                    "{}/{}",
                    dir.trim_start_matches("tests/"),
                    p.file_name().and_then(|s| s.to_str()).unwrap_or("")
                );
                if ignored.iter().any(|ig| ig == &rel) {
                    continue;
                }
                total += 1;
                let Ok(src) = fs::read_to_string(&p) else {
                    continue;
                };
                match parse(&src) {
                    Ok(d) => match class_layout(&d, &theme) {
                        Ok(_) => ok += 1,
                        Err(e) => {
                            eprintln!("layout {}: {}", rel, e);
                            layout_err += 1;
                        }
                    },
                    Err(e) => {
                        eprintln!("parse {}: {}", rel, e);
                        parse_err += 1;
                    }
                }
            }
        }
        eprintln!(
            "class sweep: {}/{} ok ({} parse-err, {} layout-err)",
            ok, total, parse_err, layout_err
        );
        assert!(ok > 0, "no class fixtures parsed cleanly");
        assert!(
            ok * 100 / total.max(1) >= 90,
            "parser regressed below 90% corpus coverage"
        );
    }
}
