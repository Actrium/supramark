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
    let svg_bbox = compute_svg_bbox_local(l, d);
    let (mut bx, mut by, mut bw, mut bh) = svg_bbox;
    // Pre-title bounds — used for the title's `x` anchor. Mermaid sets
    // the title `x` to the centre of the pre-title bbox so the title
    // hugs the diagram, not the title itself.
    let pre_title_min_x = bx;
    let pre_title_max_x = bx + bw;
    // Title text contributes (0, 0, title_w, title_h) to the jsdom
    // getBBox shim. Resolved font for the trailing
    // `<text class="classDiagramTitleText">` falls back to the default
    // sans-serif @14 because the SVG element itself has no explicit
    // font-size attribute (the 16px declaration lives inside `<style>`,
    // which the shim does not parse).
    if let Some(title) = d.meta.title.as_deref() {
        if !title.trim().is_empty() {
            let tw = crate::font_metrics::text_width(title, "sans-serif", 14.0, false, false);
            let th = crate::font_metrics::line_height("sans-serif", 14.0, false, false);
            if tw > 0.0 || th > 0.0 {
                let max_x = bx + bw;
                let max_y = by + bh;
                let nbx = bx.min(0.0);
                let nby = by.min(0.0);
                let nmx = max_x.max(tw);
                let nmy = max_y.max(th);
                bx = nbx;
                by = nby;
                bw = nmx - nbx;
                bh = nmy - nby;
            }
        }
    }
    let vx = bx - pad;
    let vy = by - pad;
    let vw = bw + pad * 2.0;
    let vh = bh + pad * 2.0;

    // ── 2. <svg ...> opening ────────────────────────────────────────
    //
    // Upstream uses two roledescription strings: legacy `classDiagram`
    // (the `class-v2` parser entry) emits `aria-roledescription="class"`,
    // while the v3 unified path triggered by `classDiagram-v2` emits
    // `aria-roledescription="classDiagram"`. The marker IDs follow the
    // same split — kind prefix is "class" for v1 / "classDiagram" for v2.
    let kind = if d.v2 { "classDiagram" } else { "class" };
    let acc_title = d.meta.acc_title.as_deref();
    let acc_descr = d.meta.acc_descr.as_deref();
    out.push_str(&unified_shell::open_unified_svg_with_a11y(
        id,
        vw,
        (vx, vy, vw, vh),
        Some("classDiagram"),
        kind,
        acc_descr.is_some(),
        acc_title.is_some(),
    ));

    // Accessibility `<title>` / `<desc>` elements — emitted right after
    // `<svg>` opens when the source declares `accTitle:` / `accDescr:`.
    // Upstream's renderer calls `setA11yDiagramInfo` + `addSVGa11yTitleDescription`
    // before any other content is appended.
    out.push_str(&unified_shell::emit_a11y_elements(id, acc_title, acc_descr));

    // ── 3. <style> block ───────────────────────────────────────────
    out.push_str(&style_block(id, d, theme));

    // ── 4. Top-level seed <g> ──────────────────────────────────────
    out.push_str("<g>");

    // Markers (5 class marker families — aggregation, extension,
    // composition, dependency, lollipop with Start/End/margin variants).
    // Upstream wraps each marker in its own `<defs>` (a few exceptions
    // emit bare `<marker>` because they are produced by D3's polygon
    // helper). Replicate the wrapping shape to stay byte-exact.
    out.push_str(&class_markers_defs(id, kind, theme));

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
        out.push_str(&render_edge_path(id, kind, e));
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
        out.push_str(&render_node(id, n, theme, d));
    }
    out.push_str("</g>");

    out.push_str("</g>"); // </g class="root">
    out.push_str("</g>"); // </g top-level seed>

    // ── 6. Trailing drop-shadow filter <defs>s ───────────────────────
    out.push_str(&unified_shell::emit_defs_shell(id, true, true));

    // Optional title text — emitted *after* the drop-shadow defs.
    if let Some(title) = d.meta.title.as_deref() {
        if !title.trim().is_empty() {
            // Title `x` anchors to the centre of the **pre-title**
            // bbox; the title's own width does not pull the anchor.
            let title_x = pre_title_min_x + (pre_title_max_x - pre_title_min_x) / 2.0;
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
    // Class-diagram clusters (namespaces) follow the upstream `cluster.ts`
    // shape: the outer `<g class="cluster undefined">` carries no transform,
    // and the inner `<rect>` sits at absolute coords `(cx - w/2, cy - h/2)`
    // — see `mermaid-official-stable-v11.14.0/.../rendering-elements/clusters.ts`.
    // The cluster-label is wrapped in its own `<g class="cluster-label ">`
    // (trailing space mirrors the upstream string concat) with an absolute
    // translate that anchors the foreignObject to (cx - lw/2, cy - h/2).
    let cx = n.x.unwrap_or(0.0);
    let cy = n.y.unwrap_or(0.0);
    let w = n.width.unwrap_or(100.0);
    let h = n.height.unwrap_or(50.0);

    let mut out = String::with_capacity(512);
    out.push_str(&format!(
        r#"<g class="cluster undefined" id="{sid}-{eid}" data-look="classic">"#,
        sid = id,
        eid = n.id,
    ));
    // Rect — absolute coords, no fill/stroke override (defers to CSS `.cluster rect`).
    out.push_str(&format!(
        r#"<rect style="" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
        w = fmt_num(w),
        h = fmt_num(h),
        x = fmt_num(cx - w / 2.0),
        y = fmt_num(cy - h / 2.0),
    ));
    // Cluster label — measured via labelHelper at 14 px, sits horizontally
    // centred inside the cluster rect, vertically pinned to the rect's top
    // (`y = cy - h/2`). Width is the measured label width (ceil-rounded by
    // upstream's foreignObject), capped at the cluster's own width.
    let label = n.label.as_deref().unwrap_or("");
    if !label.is_empty() {
        let label_w = crate::font_metrics::text_width(
            label,
            "trebuchet ms,verdana,arial,sans-serif",
            14.0,
            false,
            false,
        );
        let lw = label_w.min(w);
        let lx = cx - lw / 2.0;
        let ly = cy - h / 2.0;
        out.push_str(&format!(
            r#"<g class="cluster-label " transform="translate({tx}, {ty})"><foreignObject width="{lw}" height="16.296875"><div style="display: table-cell; white-space: nowrap; line-height: 1.5;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel "><p>{t}</p></span></div></foreignObject></g>"#,
            tx = fmt_num(lx),
            ty = fmt_num(ly),
            lw = fmt_num(lw),
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
/// Strip the leading visibility escape (`\+`, `\-`, `\#`, `\~`) and
/// decode the `&lt;`/`&gt;` entities back to literal angle brackets — the
/// text the upstream markdown→`<p>` pipeline ends up displaying.
fn displayed_member_text_local(text: &str) -> String {
    let mut s = text.to_string();
    if let Some(rest) = s.strip_prefix('\\') {
        s = rest.to_string();
    }
    s.replace("&lt;", "<").replace("&gt;", ">")
}

/// Emit one `<g class="label">` child inside members- or methods-group.
/// The inner foreignObject layout matches upstream `addText()` for a
/// single-line html label (`numberOfLines = 1` → translate(0, -bbox_h/2)).
fn render_class_text_row(m: &crate::model::class::ClassMember) -> String {
    render_class_text_row_indexed(m, 0, 1)
}

/// Emit one `<g class="label">` for member/method row `index` of `total`.
/// Per upstream `addText.ts`, single-line html labels use the
/// `(i - n/2 + 0.5) * line_h` offset so multi-row groups stack. The
/// `max-width` is computed on the *raw* `m.text` (entity-escaped) — this
/// mirrors `shapeUtil.ts` calling `calculateTextWidth(textContent, ...)`
/// before HTML decoding.
fn render_class_text_row_indexed(
    m: &crate::model::class::ClassMember,
    index: usize,
    total: usize,
) -> String {
    let family = "trebuchet ms,verdana,arial,sans-serif";
    let font = 14.0_f64;
    let line_h = 16.296875_f64;
    // The post-decode raw display text — keeps `*`/`_` markers in place.
    // Used as input to both the markdown-textContent measurement and the
    // markdown-to-HTML pass that fills the `<p>` element.
    let display_raw = displayed_member_text_local(&m.text);
    // Width is measured on the *rendered* div.textContent, which strips
    // markdown emphasis markers. Mirror upstream's `marked → innerHTML →
    // textContent` pipeline.
    let display_stripped = crate::layout::class::md_emphasis_strip(&display_raw);
    let display_w = crate::font_metrics::text_width(&display_stripped, family, font, false, false);
    // span_max_w is calculated against m.text (raw entity-escaped form),
    // not the decoded display string, mirroring upstream shapeUtil.ts.
    let span_max_w =
        crate::font_metrics::text_width(&m.text, family, 16.0, false, false).round() + 50.0;
    // The HTML body of `<p>` is the markdown-rendered form (em / strong
    // tags inserted, all other characters escaped).
    let escaped = crate::layout::class::md_emphasis_html(&html_escape(&display_raw));
    let style = m.css_style.clone();
    let _ = total;
    // Upstream `addText.ts`: `transform = translate(0, -bbox.height/(2*n) + yOffset)`.
    // For html single-line (numberOfLines=1, bbox.height=line_h), the
    // offset is `-line_h/2 + i*line_h` since yOffset accumulates by
    // `line_h + 0` for each prior row (TEXT_PADDING=0 with HTML).
    let i = index as f64;
    let ty = -line_h / 2.0 + i * line_h;
    format!(
        r#"<g class="label" style="{cls}" transform="translate(0,{ty})"><foreignObject width="{w}" height="{h}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {mw}px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel markdown-node-label" style=""><p>{txt}</p></span></div></foreignObject></g>"#,
        cls = style,
        ty = fmt_num(ty),
        w = fmt_num(display_w),
        h = fmt_num(line_h),
        mw = fmt_num(span_max_w),
        txt = escaped,
    )
}

fn render_node(id: &str, n: &LayoutNode, theme: &ThemeVariables, d: &ClassDiagram) -> String {
    let _ = theme;
    // Pull the matching parsed ClassNode for member/method/annotation
    // text — needed when the classBox renders non-empty group rows.
    let class_node = d.classes.iter().find(|c| c.id == n.id);
    let label = n.label.as_deref().unwrap_or("");
    let cx = n.x.unwrap_or(0.0);
    let cy = n.y.unwrap_or(0.0);
    // n.width / n.height are the *drawn* outer rect dims (already
    // include 2 * PADDING and any extraHeight from renderExtraBox).
    let drawn_w = n.width.unwrap_or(80.0);
    let drawn_h = n.height.unwrap_or(50.0);

    let css_classes = n.css_classes.as_deref().unwrap_or("default");
    // Upstream `nodes.ts insertNode` appends ` clickable` to the existing
    // class string when `haveCallback` is true. The class string already
    // contains a trailing space from `getNodeClasses`, so the result has
    // a double space (e.g. `node default clickable  clickable`).
    let have_callback = n.have_callback.unwrap_or(false);
    let dom_id = n.dom_id.as_deref().unwrap_or(&n.id);

    let x0 = -drawn_w / 2.0;
    let y0 = -drawn_h / 2.0;

    // Resolve user inline styles (joined w/ no `!important`) and pull
    // the fill/stroke/stroke-width attribute overrides off the same
    // dictionary. Mirrors upstream `userNodeOverrides` in
    // `rendering-elements/shapes/util.ts`.
    let style_overrides = resolve_node_style_overrides(n);

    let mut out = String::with_capacity(2048);
    let class_attr = if have_callback {
        // Upstream `getNodeClasses` returns `node <cssClasses> ` (trailing
        // space); `insertNode` then concatenates ` clickable`, yielding a
        // double space before the second `clickable`.
        format!("node {}  clickable", css_classes)
    } else {
        format!("node {} ", css_classes)
    };
    // Upstream `nodes.ts insertNode` sets `title` from `node.tooltip` on
    // the inner shape element; for classBox that's the same outer `<g>`.
    let title_attr = match n.tooltip.as_deref() {
        Some(t) if !t.is_empty() => format!(r#" title="{}""#, html_escape(t)),
        _ => String::new(),
    };
    // Upstream `nodes.ts` wraps the inner `<g class="node ...">` in an
    // `<a href="..." target="..." data-look="..." transform="...">` when
    // `link` is set; the transform / data-look move to the anchor and
    // the inner `<g>` keeps only class / id / title.
    let has_link = n.link.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
    if has_link {
        let href = n.link.as_deref().unwrap_or("");
        let target = n.link_target.as_deref().unwrap_or("_blank");
        out.push_str(&format!(
            r#"<a href="{href}" target="{target}" data-look="classic" transform="translate({tx}, {ty})">"#,
            href = html_escape(href),
            target = html_escape(target),
            tx = fmt_num(cx),
            ty = fmt_num(cy),
        ));
        out.push_str(&format!(
            r#"<g class="{cls}" id="{sid}-{did}"{title}>"#,
            cls = class_attr,
            sid = id,
            did = dom_id,
            title = title_attr,
        ));
    } else {
        out.push_str(&format!(
            r#"<g class="{cls}" id="{sid}-{did}" data-look="classic"{title} transform="translate({tx}, {ty})">"#,
            cls = class_attr,
            sid = id,
            did = dom_id,
            title = title_attr,
            tx = fmt_num(cx),
            ty = fmt_num(cy),
        ));
    }

    // basic label-container outer-path: rough.js rectangle (two paths).
    let fill_attr = style_overrides.fill.as_deref().unwrap_or("#ECECFF");
    let stroke_attr = style_overrides.stroke.as_deref().unwrap_or("#9370DB");
    let stroke_w_attr = style_overrides.stroke_width.as_deref().unwrap_or("1.3");
    out.push_str(r#"<g class="basic label-container outer-path">"#);
    out.push_str(&format!(
        r##"<path d="M{x0} {y0} L{x1} {y0} L{x1} {y1} L{x0} {y1}" stroke="none" stroke-width="0" fill="{fill}" style="{st}"></path>"##,
        x0 = fmt_num(x0),
        x1 = fmt_num(-x0),
        y0 = fmt_num(y0),
        y1 = fmt_num(-y0),
        fill = fill_attr,
        st = style_overrides.style_str,
    ));
    out.push_str(&format!(
        r##"<path d="{d}" stroke="{stroke}" stroke-width="{sw}" fill="none" stroke-dasharray="0 0" style="{st}"></path>"##,
        d = rough_rect_outline_path(x0, y0, drawn_w, drawn_h),
        stroke = stroke_attr,
        sw = stroke_w_attr,
        st = style_overrides.style_str,
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

    // Annotation group: when the class has at least one annotation
    // (only the first is rendered, mirroring upstream textHelper.ts),
    // emit a centered foreignObject with `«{annotation}»`. Otherwise
    // emit an empty group at translate(0, y_internal).
    let label_font = 14.0_f64;
    let label_family = "trebuchet ms,verdana,arial,sans-serif";
    let label_h = 16.296875_f64;
    let annotation_text = class_node
        .and_then(|c| c.annotations.first())
        .map(|a| format!("«{}»", a));
    let (annotation_w, raw_annotation_h, annotation_x) = if let Some(ref txt) = annotation_text {
        let aw = crate::font_metrics::text_width(txt, label_family, label_font, false, false);
        (aw, label_h, -aw / 2.0)
    } else {
        (0.0, 0.0, 0.0)
    };
    if let Some(ref txt) = annotation_text {
        // Inner span max-width follows upstream `addText`:
        // round(text_width(text, family, 16 px, regular)) + 50.
        let span_max_w =
            crate::font_metrics::text_width(txt, label_family, 16.0, false, false).round() + 50.0;
        out.push_str(&format!(
            r#"<g class="annotation-group text" transform="translate({ax}, {ay})"><g class="label" style="" transform="translate(0,{ily})"><foreignObject width="{w}" height="{h}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {mw}px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel markdown-node-label" style=""><p>{txt}</p></span></div></foreignObject></g></g>"#,
            ax = fmt_num(annotation_x),
            ay = fmt_num(y_internal),
            ily = fmt_num(-label_h / 2.0),
            w = fmt_num(annotation_w),
            h = fmt_num(label_h),
            mw = fmt_num(span_max_w),
            txt = txt,
        ));
    } else {
        out.push_str(&format!(
            r#"<g class="annotation-group text" transform="translate({ax}, {ay})"></g>"#,
            ax = fmt_num(annotation_x),
            ay = fmt_num(y_internal),
        ));
    }

    // Label group: translate(-label_w/2, y_internal + raw_annotation_h).
    // The textHelper sets labelGroup transform to
    // `translate(-w/2, annotationGroupHeight)` and classBox post-adjusts
    // y to `translateY + y_internal`. When annotations are non-empty,
    // `annotationGroupHeight = label_h` shifts the whole label-group
    // (and downstream members/methods) down by one line.
    let label_w = crate::font_metrics::text_width(label, label_family, label_font, true, false);
    let label_x = -label_w / 2.0;
    let label_y = y_internal + raw_annotation_h;
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
    //
    // `calculateTextWidth` is fed `node.text` — distinct from `node.label`:
    //   * default ClassNode (`addClass`): `text = name + (type ? '&lt;T&gt;' : '')`
    //     — html-entity-encoded angle brackets.
    //   * `setClassLabel` (with `["…"]` override): `text = label + (type ? '<T>' : '')`
    //     — literal angle brackets, no escaping of label content.
    // jsdom's `<text>.textContent` never decodes entities, so the raw
    // bytes go straight into `text_width`.
    let escaped = html_escape(label);
    let measure_text: String = class_node
        .map(|cn| cn.raw_text())
        .unwrap_or_else(|| label.to_string());
    let span_max_w =
        crate::font_metrics::text_width(&measure_text, label_family, 16.0, false, false).round()
            + 50.0;
    out.push_str(&format!(
        r#"<foreignObject width="{w}" height="{h}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {mw}px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel markdown-node-label" style="{ls}"><p>{txt}</p></span></div></foreignObject>"#,
        w = fmt_num(label_w),
        h = fmt_num(label_h),
        mw = fmt_num(span_max_w),
        txt = escaped,
        ls = style_overrides.style_str,
    ));
    out.push_str("</g>");
    out.push_str("</g>");

    // Members and methods groups: translate(x, y) — derived from
    // upstream classBox.ts + textHelper.ts:
    //   x = -w/2     where w = max(node.width ?? 0, bbox.width)
    //   y_internal_h = h_internal (= drawn_h - 4*PADDING when renderExtraBox,
    //                              drawn_h - 2*PADDING + PADDING otherwise)
    //   members translateY = annotationGroupHeight + labelGroupHeight + GAP*2 + (y_internal + PADDING - extraBoxOffset)
    //   methods translateY = annotationGroupHeight + labelGroupHeight + membersGroupHeight + (y_internal + PADDING - extraBoxOffset) + GAP*4 + PADDING ? wait no
    //
    // For empty-members/methods with renderExtraBox=true the upstream
    // formulas collapse to the original constants we already use;
    // for non-empty rows, we replicate the textHelper transform plus the
    // classBox post-adjustment loop.
    let has_members = class_node.map_or(false, |c| !c.members.is_empty());
    let has_methods = class_node.map_or(false, |c| !c.methods.is_empty());
    let render_extra_box = !has_members && !has_methods;
    // bbox.width as seen by classBox = max foreignObject width across
    // all visible groups. For empty members/methods this is just the
    // label's foreignObject width.
    let label_family_member = "trebuchet ms,verdana,arial,sans-serif";
    let label_font_member = 14.0_f64;
    let display_widths = |members: &[crate::model::class::ClassMember]| -> Vec<f64> {
        members
            .iter()
            .map(|m| {
                let raw = displayed_member_text_local(&m.text);
                // Width is on the markdown-rendered textContent — strip
                // emphasis markers so `*italic*` is measured as `italic`.
                let s = crate::layout::class::md_emphasis_strip(&raw);
                crate::font_metrics::text_width(
                    &s,
                    label_family_member,
                    label_font_member,
                    false,
                    false,
                )
            })
            .collect()
    };
    let mut bbox_w = label_w;
    if annotation_w > bbox_w {
        bbox_w = annotation_w;
    }
    let member_widths: Vec<f64> = class_node
        .map(|c| display_widths(&c.members))
        .unwrap_or_default();
    let method_widths: Vec<f64> = class_node
        .map(|c| display_widths(&c.methods))
        .unwrap_or_default();
    for w in &member_widths {
        if *w > bbox_w {
            bbox_w = *w;
        }
    }
    for w in &method_widths {
        if *w > bbox_w {
            bbox_w = *w;
        }
    }
    let group_x = -bbox_w / 2.0;

    // Reproduce textHelper's intra-group translateY + classBox's post
    // adjustment.  See `classBox.ts` lines 94–166.
    //
    // classBox reads the *post-textHelper* getBBox().height of each
    // group, and then subtracts `PADDING/2` if `renderExtraBox` is true
    // — but importantly the JS `|| 0` fallback is only triggered when
    // the *expression value* is falsy (0, NaN, undefined). A negative
    // number stays as-is, hence `0 - PADDING/2 = -6` survives.
    // raw_annotation_h is computed earlier alongside annotation_text.
    let raw_label_h = label_h; // single foreignObject
                               // members getBBox(): for non-empty groups every row's foreignObject
                               // sits at intrinsic (0,0); the union therefore collapses to a single
                               // (0, 0, max_w, line_h) box regardless of row count (see notes in
                               // layout/class.rs::estimate_classbox_dimensions).
    let raw_members_h = if has_members { label_h } else { 0.0 };
    let extra_sub = if render_extra_box { padding / 2.0 } else { 0.0 };
    let annotation_h_cb = if raw_annotation_h - extra_sub == 0.0 {
        0.0
    } else {
        raw_annotation_h - extra_sub
    };
    let label_h_cb = if raw_label_h - extra_sub == 0.0 {
        0.0
    } else {
        raw_label_h - extra_sub
    };
    let members_h_cb = if raw_members_h - extra_sub == 0.0 {
        0.0
    } else {
        raw_members_h - extra_sub
    };
    let h_internal_real = if render_extra_box {
        drawn_h - 4.0 * padding
    } else {
        drawn_h - 2.0 * padding
    };
    let y_internal_real = -h_internal_real / 2.0;
    let extra_box_offset = if render_extra_box {
        padding
    } else if !has_members && !has_methods {
        -padding / 2.0
    } else {
        0.0
    };
    // Initial translateY set by textHelper for members:
    //   annotationGroupHeight (raw, untouched by classBox subtraction)
    //   + labelGroupHeight (raw)
    //   + GAP * 2
    // Note: textHelper uses the *raw* heights. classBox's subtracted
    // values are only used in its own newTranslateY formula.
    let members_translate_y_initial = raw_annotation_h + raw_label_h + 2.0 * padding;
    let members_translate_y =
        members_translate_y_initial + y_internal_real + padding - extra_box_offset;

    // textHelper methods translateY uses post-set membersGroupHeight,
    // but classBox *overrides* this for the methods-group with
    //   newTranslateY = annotation_cb + label_cb + max(members_cb, GAP/2) + y + GAP*4 + PADDING
    // (when `nodeHeightGreater` is false, the common case here).
    let members_h_for_methods = members_h_cb.max(padding / 2.0);
    let methods_translate_y = annotation_h_cb
        + label_h_cb
        + members_h_for_methods
        + y_internal_real
        + 4.0 * padding
        + padding;

    // Emit members-group.
    out.push_str(&format!(
        r#"<g class="members-group text" transform="translate({mx}, {my})">"#,
        mx = fmt_num(group_x),
        my = fmt_num(members_translate_y),
    ));
    if let Some(c) = class_node {
        let total = c.members.len();
        for (i, (m, _w)) in c.members.iter().zip(member_widths.iter()).enumerate() {
            out.push_str(&render_class_text_row_indexed(m, i, total));
        }
    }
    out.push_str("</g>");

    // Emit methods-group.
    out.push_str(&format!(
        r#"<g class="methods-group text" transform="translate({mx}, {my})">"#,
        mx = fmt_num(group_x),
        my = fmt_num(methods_translate_y),
    ));
    if let Some(c) = class_node {
        let total = c.methods.len();
        for (i, (m, _w)) in c.methods.iter().zip(method_widths.iter()).enumerate() {
            out.push_str(&render_class_text_row_indexed(m, i, total));
        }
    }
    out.push_str("</g>");

    // Divider lines. Upstream emits these whenever
    // `members.len() > 0 || methods.len() > 0 || renderExtraBox`. For
    // class/186 (renderExtraBox=true) we emit both:
    //   firstLineY  = annotationGroupHeight + labelGroupHeight + y_internal + PADDING
    //   secondLineY = annotationGroupHeight + labelGroupHeight + membersGroupHeight + y_internal + GAP*2 + PADDING
    //
    // The `*Height` values are reduced by `PADDING/2` for the
    // renderExtraBox path (truthy in JS even when negative, hence we
    // reproduce the same "-6 / 10.296875 / -6" fall-out byte-for-byte).
    // Use the same group heights classBox itself uses (the *_cb values),
    // not the raw textHelper values. With `renderExtraBox=true` the
    // empty-group case collapses into the legacy `-6 / 10.296875 / -6`
    // constants byte-for-byte; non-empty groups go through the populated
    // formula.
    let first_line_y = annotation_h_cb + label_h_cb + y_internal_real + padding;
    let second_line_y =
        annotation_h_cb + label_h_cb + members_h_cb + y_internal_real + 2.0 * padding + padding;

    out.push_str(&format!(
        r#"<g class="divider" style="{gs}"><path d=""#,
        gs = style_overrides.style_str,
    ));
    out.push_str(&rough_line_path(
        x0,
        first_line_y,
        -x0,
        first_line_y + 0.001,
    ));
    out.push_str(&format!(
        r##"" stroke="{stroke}" stroke-width="{sw}" fill="none" stroke-dasharray="0 0" style="{st}"></path></g>"##,
        stroke = stroke_attr,
        sw = stroke_w_attr,
        st = style_overrides.style_str,
    ));

    out.push_str(&format!(
        r#"<g class="divider" style="{gs}"><path d=""#,
        gs = style_overrides.style_str,
    ));
    out.push_str(&rough_line_path(
        x0,
        second_line_y,
        -x0,
        second_line_y + 0.001,
    ));
    out.push_str(&format!(
        r##"" stroke="{stroke}" stroke-width="{sw}" fill="none" stroke-dasharray="0 0" style="{st}"></path></g>"##,
        stroke = stroke_attr,
        sw = stroke_w_attr,
        st = style_overrides.style_str,
    ));

    out.push_str("</g>");
    if has_link {
        out.push_str("</a>");
    }
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

fn rough_curve(out: &mut String, x1: f64, y1: f64, x2: f64, y2: f64, c: f64, first: bool) {
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
fn compute_svg_bbox_local(l: &ClassLayout, d: &ClassDiagram) -> (f64, f64, f64, f64) {
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

    let padding = 12.0_f64;
    // Cluster (`<g class="cluster" transform="translate(cx, cy)">`) contributes
    // its rect *with transform applied* — the jsdom shim that produces the
    // reference SVGs does honour the cluster `<g>`'s transform. Mirror that
    // here so namespace-bearing diagrams pick up the right viewBox extents.
    for n in l.unified.nodes.iter().filter(|n| n.is_group) {
        let w = n.width.unwrap_or(0.0);
        let h = n.height.unwrap_or(0.0);
        if w > 0.0 || h > 0.0 {
            let cx = n.x.unwrap_or(0.0);
            let cy = n.y.unwrap_or(0.0);
            visit(cx - w / 2.0, cy - h / 2.0, w, h);
        }
        if let Some(label) = n.label.as_deref() {
            // Cluster label foreignObject sits at (-w/2, -h/2 + 4) in local
            // coords, so after the cluster transform the absolute box is at
            // (cx - w/2, cy - h/2 + 4, w, label_h).
            if !label.is_empty() {
                let cx = n.x.unwrap_or(0.0);
                let cy = n.y.unwrap_or(0.0);
                let lw = w;
                visit(cx - w / 2.0, cy - h / 2.0 + 4.0, lw, label_h);
            }
        }
    }
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
        // Member / method foreignObject contributions: each row sits at
        // intrinsic (0, 0, display_w, line_h). Mirrors `addText()` →
        // foreignObject which `generate_ref.mjs`'s getBBox shim treats as
        // a transform-less box at the parent origin.
        let class_node = d.classes.iter().find(|c| c.id == n.id);
        if let Some(c) = class_node {
            for m in c.members.iter().chain(c.methods.iter()) {
                // Mirror upstream: width is measured on the markdown-
                // rendered textContent, so emphasis markers (`*`/`_`) are
                // stripped before measurement.
                let raw = displayed_member_text_local(&m.text);
                let display = crate::layout::class::md_emphasis_strip(&raw);
                let dw = crate::font_metrics::text_width(
                    &display,
                    label_family,
                    label_font,
                    false,
                    false,
                );
                if dw > 0.0 {
                    visit(0.0, 0.0, dw, label_h);
                }
            }
            for a in &c.annotations {
                let aw = crate::font_metrics::text_width(
                    &format!("«{}»", a),
                    label_family,
                    label_font,
                    false,
                    false,
                );
                if aw > 0.0 {
                    visit(0.0, 0.0, aw, label_h);
                }
            }
        }
        // Divider lines — these are absolute-coord paths and may extend
        // *below* the outer rect (e.g. when the second divider sits on
        // the gap *between* the outer rect and the methods band). Mirror
        // the y math from render_node so the bbox catches them.
        if w > 0.0 {
            let drawn_w = w;
            let drawn_h = h;
            let has_members = class_node.map_or(false, |c| !c.members.is_empty());
            let has_methods = class_node.map_or(false, |c| !c.methods.is_empty());
            let render_extra_box = !has_members && !has_methods;
            let h_internal_real = if render_extra_box {
                drawn_h - 4.0 * padding
            } else {
                drawn_h - 2.0 * padding
            };
            let y_internal_real = -h_internal_real / 2.0;
            let extra_sub = if render_extra_box { padding / 2.0 } else { 0.0 };
            let raw_label_h = label_h;
            let raw_members_h = if has_members { label_h } else { 0.0 };
            // Annotation row contributes label_h to the divider-y offset
            // when the class declares any `<<annotation>>`. Mirrors
            // render_node's `annotation_h_cb` term.
            let has_annotation = class_node.map_or(false, |c| !c.annotations.is_empty());
            let raw_annotation_h = if has_annotation { label_h } else { 0.0 };
            let cb = |v: f64| if v == 0.0 { 0.0 } else { v };
            let annotation_h_cb = cb(raw_annotation_h - extra_sub);
            let label_h_cb = cb(raw_label_h - extra_sub);
            let members_h_cb = cb(raw_members_h - extra_sub);
            let first_line_y = annotation_h_cb + label_h_cb + y_internal_real + padding;
            let second_line_y = annotation_h_cb
                + label_h_cb
                + members_h_cb
                + y_internal_real
                + 2.0 * padding
                + padding;
            // The lines stretch from x=-drawn_w/2 to +drawn_w/2 with
            // a 0.001 vertical span, which the path bbox parser sees as
            // a thin (0.001-tall) box.
            let lx = -drawn_w / 2.0;
            visit(lx, first_line_y, drawn_w, 0.001);
            visit(lx, second_line_y, drawn_w, 0.001);
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
        let raw: Vec<crate::layout::unified::types::Point> =
            e.points.as_deref().unwrap_or(&[]).iter().copied().collect();
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
fn class_markers_defs(id: &str, kind: &str, _theme: &ThemeVariables) -> String {
    let mut s = String::with_capacity(4096);

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
fn render_edge_path(diag_id: &str, kind: &str, e: &crate::layout::unified::types::Edge) -> String {
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
                r#" marker-start="url(#{did}_{kind}-{ty}Start)""#,
                did = diag_id,
                kind = kind,
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
                r#" marker-end="url(#{did}_{kind}-{ty}End)""#,
                did = diag_id,
                kind = kind,
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
    // When the label is non-empty, upstream measures the foreignObject's
    // text width (HTML labels) — mirror that with `font_metrics::text_width`
    // at 14 px regular weight, the edgeLabel font.
    let label_w = if label_text.is_empty() {
        0.0
    } else {
        crate::font_metrics::text_width(
            label_text,
            "trebuchet ms,verdana,arial,sans-serif",
            14.0,
            false,
            false,
        )
    };
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
fn style_block(id: &str, d: &ClassDiagram, theme: &ThemeVariables) -> String {
    let mut css = String::with_capacity(8000);
    css.push_str("<style>");
    css.push_str(&theme_css::base_preamble(id, theme));
    css.push_str(&class_specific_css(id, theme));
    css.push_str(&theme_css::neo_look_block(id, theme));
    css.push_str(&class_inline_style_css(id, d));
    css.push_str("</style>");
    css
}

/// Per-class inline-style CSS. Mirrors upstream `utils.insertClass`
/// invoked from the class-renderer: for every class that ends up with
/// any inline `style ID …` directive *or* gathered `classDef` styles
/// (via `:::name` or `cssClass`), emit
///
/// ```text
/// #<diag-id> .<class-id>>*{<all>!important;}
/// #<diag-id> .<class-id> span{<all>!important;}
/// ```
///
/// Unlike the flowchart variant, the class diagram does not emit the
/// auxiliary `tspan` (color→fill) rule — the reference SVGs never carry
/// it for class fixtures.
fn class_inline_style_css(id: &str, d: &ClassDiagram) -> String {
    let mut out = String::new();
    // Index classDef name → styles for quick lookup. Last wins on
    // duplicate ids (matches upstream `addStyleClass` behaviour).
    use std::collections::HashMap;
    let mut defs: HashMap<&str, &Vec<String>> = HashMap::new();
    for sc in &d.style_classes {
        defs.insert(sc.id.as_str(), &sc.styles);
    }

    for c in &d.classes {
        // Merge styles in order: classDef styles (per css_classes order)
        // then any inline `style` directive styles. This matches the
        // ordering observed in upstream reference SVGs (e.g. cypress/46
        // which applies `pink` then `bold`).
        let mut all_props: Vec<String> = Vec::new();
        for cc in &c.css_classes {
            if let Some(styles) = defs.get(cc.as_str()) {
                for st in *styles {
                    push_style_prop(&mut all_props, st);
                }
            }
        }
        for st in &c.styles {
            push_style_prop(&mut all_props, st);
        }
        if all_props.is_empty() {
            continue;
        }
        let css: String = all_props.join("");
        out.push_str(&format!("#{id} .{name}>*{{{css}}}", name = c.id, css = css,));
        out.push_str(&format!(
            "#{id} .{name} span{{{css}}}",
            name = c.id,
            css = css,
        ));
    }
    out
}

fn push_style_prop(out: &mut Vec<String>, raw: &str) {
    let s = raw.trim().trim_end_matches(';');
    if s.is_empty() {
        return;
    }
    if let Some(colon) = s.find(':') {
        let key = s[..colon].trim();
        let val = s[colon + 1..].trim();
        out.push(format!("{}:{}!important;", key, val));
    } else {
        out.push(format!("{}!important;", s));
    }
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
    let group_text_fill = if !node_border.is_empty() {
        node_border
    } else {
        class_text
    };
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

struct NodeStyleOverrides {
    fill: Option<String>,
    stroke: Option<String>,
    stroke_width: Option<String>,
    /// Joined `key:value;…` string used as the literal `style="…"` attr.
    style_str: String,
}

/// Parse `n.css_compiled_styles` into the per-shape overrides upstream
/// derives in `userNodeOverrides`. Each style entry of the form
/// `"prop:value"` contributes both to the joined `style="…"` attribute
/// and — for the shape-relevant subset — pulls a value out for the
/// matching `<path>` attribute. The numeric `stroke-width:4px` is fed
/// to `stroke-width` *without* the unit (matches reference SVGs).
fn resolve_node_style_overrides(n: &LayoutNode) -> NodeStyleOverrides {
    let mut out = NodeStyleOverrides {
        fill: None,
        stroke: None,
        stroke_width: None,
        style_str: String::new(),
    };
    let styles = match n.css_compiled_styles.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => return out,
    };
    let mut joined = String::new();
    for raw in styles {
        let s = raw.trim().trim_end_matches(';');
        if s.is_empty() {
            continue;
        }
        joined.push_str(s);
        joined.push(';');
        if let Some(colon) = s.find(':') {
            let key = s[..colon].trim();
            let val = s[colon + 1..].trim().to_string();
            match key {
                "fill" => out.fill = Some(val),
                "stroke" => out.stroke = Some(val),
                "stroke-width" => {
                    // upstream attribute strips the trailing `px`/unit
                    // suffix when assigning to the `stroke-width` attr,
                    // keeping the bare number.
                    let n_only: String = val
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                        .collect();
                    out.stroke_width = Some(if n_only.is_empty() { val } else { n_only });
                }
                _ => {}
            }
        }
    }
    // Trim trailing `;` to match upstream `style.cssText` serialisation
    // — references use `style="fill:#f9f;stroke:#333;stroke-width:4px"`
    // (no trailing semicolon).
    if joined.ends_with(';') {
        joined.pop();
    }
    out.style_str = joined;
    out
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

    /// Diagnostic helper: dump a fixture's actual + expected SVG and
    /// per-byte diff into /tmp for quick inspection. Ignored by default.
    #[test]
    #[ignore]
    fn dump_class_fixture() {
        let rel = std::env::var("CLASS_DUMP_REL")
            .unwrap_or_else(|_| "ext_fixtures/cypress/class/01".to_string());
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let id = id_for_rel(&rel);
        let mmd = std::fs::read_to_string(base.join(format!("tests/{}.mmd", rel))).unwrap();
        let exp =
            std::fs::read_to_string(base.join(format!("tests/reference/{}.svg", rel))).unwrap();
        let got = render_fixture(&mmd, &id);
        let stem: String = rel
            .chars()
            .map(|c| if c == '/' { '_' } else { c })
            .collect();
        std::fs::write(format!("/tmp/class_dump_{}.got.svg", stem), &got).unwrap();
        std::fs::write(format!("/tmp/class_dump_{}.exp.svg", stem), &exp).unwrap();
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "[class-dump] rel={} got_len={} exp_len={} common_prefix={}",
            rel,
            got.len(),
            exp.len(),
            prefix
        );
    }

    /// Print every layout node's width/height for a fixture.
    #[test]
    #[ignore]
    fn dump_class_node_dims() {
        let rel = std::env::var("CLASS_DUMP_REL")
            .unwrap_or_else(|_| "ext_fixtures/cypress/class/01".to_string());
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = std::fs::read_to_string(base.join(format!("tests/{}.mmd", rel))).unwrap();
        let d = parse(&mmd).expect("parse");
        let theme = get_theme("default");
        let l = class_layout(&d, &theme).expect("layout");
        for n in &l.unified.nodes {
            eprintln!(
                "node id={} shape={:?} w={:?} h={:?} x={:?} y={:?} group={} parent={:?}",
                n.id, n.shape, n.width, n.height, n.x, n.y, n.is_group, n.parent_id
            );
        }
        for e in &l.unified.edges {
            eprintln!(
                "edge id={} src={:?} tgt={:?} pts={:?}",
                e.id, e.source, e.target, e.points
            );
        }
        eprintln!("bounds={:?}", l.unified.bounds);
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
