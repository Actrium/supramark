//! Mindmap SVG renderer.
//!
//! Targets byte-exact parity with upstream's `mindmapRenderer.ts` →
//! `defaultMindmapNode.ts` (and friends) → `setupViewPortForSVG.ts`
//! pipeline. Currently focused on the trivial single-node fixtures
//! (cypress 05, etc.) — multi-node fixtures require either the
//! `tidy-tree` or `cose-bilkent` layout port and are still routed
//! through `tests/known_ignored.txt`.

use crate::error::{MermaidError, Result};
use crate::layout::mindmap::{EdgePoints, MindmapLayout, PositionedNode, VIEWPORT_PADDING};
use crate::math::js_number::js_number_to_string;
use crate::model::mindmap::{is_indented_block, MindmapDiagram, MindmapNode, MindmapNodeType};
use crate::render::rough::fmt_num;
use crate::theme::ThemeVariables;

pub fn render(
    d: &MindmapDiagram,
    l: &MindmapLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    if d.nodes.is_empty() {
        return Err(MermaidError::Unsupported(
            "mindmap: empty diagram".into(),
        ));
    }
    if d.nodes.len() == 1 {
        return match d.nodes[0].node_type {
            MindmapNodeType::Default => render_single(d, l, theme, id, ShapeKind::Default),
            MindmapNodeType::Rect => render_single(d, l, theme, id, ShapeKind::Rect),
            MindmapNodeType::Circle => render_single(d, l, theme, id, ShapeKind::Circle),
            MindmapNodeType::RoundedRect => render_single(d, l, theme, id, ShapeKind::RoundedRect),
            other => Err(MermaidError::Unsupported(format!(
                "mindmap: node shape {:?} not yet supported",
                other
            ))),
        };
    }

    // Multi-node groundwork: scaffolding output covering Default, Rect,
    // Bang, Cloud, Hexagon shapes + parent-child edges. Positions are
    // produced by the cose-bilkent simulation port (NOT byte-exact yet
    // -- see src/layout/cose_bilkent.rs::run_layout). The render path
    // exists so cose-bilkent gaps can be diagnosed via diff against
    // upstream reference SVGs.
    render_multi(d, l, theme, id)
}

#[derive(Debug, Clone, Copy)]
enum ShapeKind {
    Default,
    Rect,
    Bang,
    Cloud,
    Hexagon,
    Circle,
    RoundedRect,
}

impl ShapeKind {
    fn from_node_type(t: MindmapNodeType) -> Self {
        match t {
            MindmapNodeType::Default => ShapeKind::Default,
            MindmapNodeType::Rect => ShapeKind::Rect,
            MindmapNodeType::Bang => ShapeKind::Bang,
            MindmapNodeType::Cloud => ShapeKind::Cloud,
            MindmapNodeType::Hexagon => ShapeKind::Hexagon,
            MindmapNodeType::Circle => ShapeKind::Circle,
            MindmapNodeType::RoundedRect => ShapeKind::RoundedRect,
        }
    }
}

fn render_single(
    d: &MindmapDiagram,
    l: &MindmapLayout,
    theme: &ThemeVariables,
    id: &str,
    shape: ShapeKind,
) -> Result<String> {
    let n = &l.nodes[0];

    // ─── ViewBox = local bbox + 10px viewport padding (set by
    //     setupViewPortForSVG with mindmap.padding default = 10).
    let bbox = l.content_bbox;
    let vb_x = bbox.x - VIEWPORT_PADDING;
    let vb_y = bbox.y - VIEWPORT_PADDING;
    let vb_w = bbox.w + 2.0 * VIEWPORT_PADDING;
    let vb_h = bbox.h + 2.0 * VIEWPORT_PADDING;

    let mut out = String::with_capacity(32 * 1024);

    // ─── <svg> root.
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" class="mindmapDiagram" style="max-width: {mw}px;" viewBox="{vbx} {vby} {vbw} {vbh}" role="graphics-document document" aria-roledescription="mindmap">"#,
        id = id,
        mw = fmt_num(vb_w),
        vbx = fmt_num(vb_x),
        vby = fmt_num(vb_y),
        vbw = fmt_num(vb_w),
        vbh = fmt_num(vb_h),
    ));

    // ─── <style> block.
    out.push_str(&build_style_block(id, theme));

    // ─── Markers + content groups (matches insertMarkers + render.ts).
    out.push_str("<g>");
    out.push_str(&build_markers(id));
    out.push_str(r#"<g class="subgraphs"></g>"#);
    out.push_str(r#"<g class="edgePaths"></g>"#);
    out.push_str(r#"<g class="edgeLabels"></g>"#);
    out.push_str(r#"<g class="nodes">"#);

    // ─── The single node.
    let node_dom_id = format!("{id}-node_{}", d.nodes[0].id);
    out.push_str(&format!(
        r#"<g class="node mindmap-node section-root section--1 " id="{ndom}" data-look="classic" transform="translate({tx}, {ty})">"#,
        ndom = node_dom_id,
        tx = fmt_num(n.x),
        ty = fmt_num(n.y),
    ));

    emit_shape_body(&mut out, shape, n, &node_dom_id);

    emit_label(&mut out, &d.nodes[0], shape, n.bbox_w, n.bbox_h);

    // Close node g + nodes g + outer g.
    out.push_str("</g></g></g>");

    // ─── trailing <defs> for drop-shadow filters (always emitted by
    //     unified renderer, even when not referenced).
    out.push_str(&format!(
        "<defs><filter id=\"{id}-drop-shadow\" height=\"130%\" width=\"130%\"><feDropShadow dx=\"4\" dy=\"4\" stdDeviation=\"0\" flood-opacity=\"0.06\" flood-color=\"#000000\"></feDropShadow></filter></defs>",
        id = id,
    ));
    out.push_str(&format!(
        "<defs><filter id=\"{id}-drop-shadow-small\" height=\"150%\" width=\"150%\"><feDropShadow dx=\"2\" dy=\"2\" stdDeviation=\"0\" flood-opacity=\"0.06\" flood-color=\"#000000\"></feDropShadow></filter></defs>",
        id = id,
    ));

    out.push_str("</svg>");
    Ok(out)
}

/// Multi-node mindmap renderer (scaffolding, NOT byte-exact yet).
///
/// Drives the same overall pipeline as `render_single` but iterates
/// over every node in `l.nodes`, emits parent->child edges as straight
/// `<path d="M...L...">` lines, and selects per-node shape geometry
/// from the parser's `MindmapNodeType`. Section CSS classes follow the
/// upstream convention: root gets `section-root section--1`, every
/// other node gets `section-{idx}` with `idx = layout::section`.
///
/// Known divergences from upstream byte-output (deliberate, until the
/// cose-bilkent + tidy-tree ports stabilise):
///   * positions come from the in-tree cose-bilkent simulation which
///     still lacks `reduceTrees` / FR-grid / Coarsening — coords drift;
///   * bang / cloud paths use simplified single-arc bodies, not the
///     12-arc `bangShape` / `cloudShape` formulas;
///   * `data-points` Base64 metadata on edges is omitted;
///   * `transform` on inner `<path>` (path-relative offset emitted by
///     `nodeHelper`) uses centre-origin rather than upper-left origin.
fn render_multi(
    d: &MindmapDiagram,
    l: &MindmapLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let bbox = l.content_bbox;
    let vb_x = bbox.x - VIEWPORT_PADDING;
    let vb_y = bbox.y - VIEWPORT_PADDING;
    let vb_w = bbox.w + 2.0 * VIEWPORT_PADDING;
    let vb_h = bbox.h + 2.0 * VIEWPORT_PADDING;

    let mut out = String::with_capacity(64 * 1024);

    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" class="mindmapDiagram" style="max-width: {mw}px;" viewBox="{vbx} {vby} {vbw} {vbh}" role="graphics-document document" aria-roledescription="mindmap">"#,
        id = id,
        mw = fmt_num(vb_w),
        vbx = fmt_num(vb_x),
        vby = fmt_num(vb_y),
        vbw = fmt_num(vb_w),
        vbh = fmt_num(vb_h),
    ));

    out.push_str(&build_style_block(id, theme));

    out.push_str("<g>");
    out.push_str(&build_markers(id));
    out.push_str(r#"<g class="subgraphs"></g>"#);

    // ─── Edges. Emit `<path d="...">` with d3-shape `curveBasis`
    //     interpolation over the (start, mid, end) control points
    //     produced by the layout. `data-points` is `btoa(JSON)` of those
    //     same three points.
    out.push_str(r#"<g class="edgePaths">"#);
    for (i, node) in d.nodes.iter().enumerate() {
        let Some(parent) = node.parent else { continue };
        let Some(ep) = l.edges.get(i).and_then(|e| e.as_ref()) else {
            continue;
        };
        let section = l.nodes[i].section;
        // edge-depth-N: N = parent.level + 1 (mirrors upstream
        // `mindmap-definition::generateEdges`).
        let edge_depth = d.nodes[parent].level + 1;
        let edge_id = format!("{id}-edge_{}_{}", parent, i);
        let d_attr = curve_basis_path(*ep);
        let points_attr = encode_data_points(*ep);
        out.push_str(&format!(
            r#"<path d="{d}" id="{eid}" class=" edge-thickness-normal edge-pattern-solid edge section-edge-{sec} edge-depth-{ed}" style="undefined;;;undefined" data-edge="true" data-et="edge" data-id="edge_{ps}_{cs}" data-points="{pts}" data-look="classic"></path>"#,
            d = d_attr,
            eid = edge_id,
            sec = section,
            ed = edge_depth,
            ps = parent,
            cs = i,
            pts = points_attr,
        ));
    }
    out.push_str("</g>");

    // ─── Edge labels (mindmap doesn't carry text on edges; emit empty
    //     stubs to mirror upstream's structural placeholders).
    out.push_str(r#"<g class="edgeLabels">"#);
    for (i, node) in d.nodes.iter().enumerate() {
        let Some(parent) = node.parent else { continue };
        out.push_str(&format!(
            r#"<g class="edgeLabel"><g class="label" data-id="edge_{p}_{c}" transform="translate(0, -8.1484375)"><foreignObject width="0" height="16.296875"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><span class="edgeLabel "></span></div></foreignObject></g></g>"#,
            p = parent,
            c = i,
        ));
    }
    out.push_str("</g>");

    // ─── Nodes.
    out.push_str(r#"<g class="nodes">"#);
    for (i, src) in d.nodes.iter().enumerate() {
        let n = &l.nodes[i];
        let kind = ShapeKind::from_node_type(src.node_type);
        let section = n.section;
        let section_class = if src.is_root {
            "section-root section--1".to_string()
        } else {
            format!("section-{}", section)
        };
        let dom_id = format!("{id}-node_{}", src.id);
        out.push_str(&format!(
            r#"<g class="node mindmap-node {sc} " id="{ndom}" data-look="classic" transform="translate({tx}, {ty})">"#,
            sc = section_class,
            ndom = dom_id,
            tx = fmt_num(n.x),
            ty = fmt_num(n.y),
        ));

        emit_shape_body(&mut out, kind, n, &dom_id);

        emit_label(&mut out, src, kind, n.bbox_w, n.bbox_h);

        out.push_str("</g>");
    }
    out.push_str("</g></g>");

    out.push_str(&format!(
        "<defs><filter id=\"{id}-drop-shadow\" height=\"130%\" width=\"130%\"><feDropShadow dx=\"4\" dy=\"4\" stdDeviation=\"0\" flood-opacity=\"0.06\" flood-color=\"#000000\"></feDropShadow></filter></defs>",
    ));
    out.push_str(&format!(
        "<defs><filter id=\"{id}-drop-shadow-small\" height=\"150%\" width=\"150%\"><feDropShadow dx=\"2\" dy=\"2\" stdDeviation=\"0\" flood-opacity=\"0.06\" flood-color=\"#000000\"></feDropShadow></filter></defs>",
    ));

    out.push_str("</svg>");
    Ok(out)
}

/// Emit the per-shape body (`<path>`, `<rect>`, `<polygon>`, `<circle>`)
/// for a single mindmap node. Centred on the node's local origin.
///
/// **Scaffolding only**: bang / cloud / hexagon use simplified geometry
/// that approximates upstream's path data without matching it byte for
/// Emit the `<g class="label">` containing the foreignObject + inner
/// span. Mirrors upstream `createText` → `markdownToHTML` semantics:
/// when the descr is parsed as an indented code block (any non-empty
/// line starts with 4+ spaces), `markdownToHTML` falls through to
/// `node.raw` so the span content is the raw text without `<p>`. For
/// regular single-line text the span wraps `<p>{descr}</p>`.
fn emit_label(out: &mut String, src: &MindmapNode, kind: ShapeKind, bbox_w: f64, bbox_h: f64) {
    let bkg = if src.icon.is_some() {
        r#" class="labelBkg""#
    } else {
        ""
    };
    // Mirror upstream `markdownToHTML` paragraph-vs-code branching:
    // markdown-rendered text without an indented-code-block trigger ends
    // up in a `<p>...</p>` wrapper. Indented code blocks (any non-empty
    // line starts with 4+ spaces) fall through to `node.raw`, so the
    // span content is the raw text. Shape kind alone is NOT the
    // discriminator — `((mindmap))` and `((\n  The root\n))` are both
    // Circle but render differently.
    let raw_text_branch = is_indented_block(&src.raw_descr);
    let span_inner = if raw_text_branch {
        html_escape(&src.raw_descr)
    } else {
        format!("<p>{}</p>", html_escape(&src.raw_descr))
    };
    out.push_str(&format!(
        r#"<g class="label" style="" transform="translate({tx}, {ty})"><rect></rect><foreignObject width="{w}" height="{h}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"{bkg}><span class="nodeLabel markdown-node-label">{inner}</span></div></foreignObject></g>"#,
        tx = fmt_num(-bbox_w / 2.0),
        ty = fmt_num(-bbox_h / 2.0),
        w = fmt_num(bbox_w),
        h = fmt_num(bbox_h),
        bkg = bkg,
        inner = span_inner,
    ));
}

/// byte. Sufficient for the renderer to produce non-empty SVG so the
/// layout pipeline can be diagnosed end-to-end.
fn emit_shape_body(out: &mut String, kind: ShapeKind, n: &PositionedNode, dom_id: &str) {
    let half_w = n.shape_w / 2.0;
    let half_h = n.shape_h / 2.0;
    match kind {
        ShapeKind::Default => {
            let inner_w = n.shape_w - 10.0;
            let inner_h = n.shape_h - 10.0;
            out.push_str(&format!(
                r#"<path id="{ndom}" class="node-bkg node-0" style="" d="
    M{nx} {hh}
    v{nih}
    q0,-5 5,-5
    h{iw}
    q5,0 5,5
    v{ih}
    q0,5 -5,5
    h{niw}
    q-5,0 -5,-5
    Z
  "></path>"#,
                ndom = dom_id,
                nx = fmt_num(-half_w),
                hh = fmt_num(half_h - 5.0),
                nih = fmt_num(-inner_h),
                iw = fmt_num(inner_w),
                ih = fmt_num(inner_h),
                niw = fmt_num(-inner_w),
            ));
            out.push_str(&format!(
                r#"<line class="node-line-" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}"></line>"#,
                x1 = fmt_num(-half_w),
                y1 = fmt_num(half_h),
                x2 = fmt_num(half_w),
                y2 = fmt_num(half_h),
            ));
        }
        ShapeKind::Rect => {
            out.push_str(&format!(
                r#"<rect class="basic label-container" style="" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
                x = fmt_num(-half_w),
                y = fmt_num(-half_h),
                w = fmt_num(n.shape_w),
                h = fmt_num(n.shape_h),
            ));
        }
        ShapeKind::Circle => {
            // squareCircle: r = max(half_w, half_h).
            let r = half_w.max(half_h);
            out.push_str(&format!(
                r#"<circle class="basic label-container" style="" r="{r}" cx="0" cy="0"></circle>"#,
                r = fmt_num(r),
            ));
        }
        ShapeKind::RoundedRect => {
            out.push_str(&format!(
                r#"<rect class="basic label-container" style="" rx="5" ry="5" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
                x = fmt_num(-half_w),
                y = fmt_num(-half_h),
                w = fmt_num(n.shape_w),
                h = fmt_num(n.shape_h),
            ));
        }
        ShapeKind::Hexagon => {
            // Simplified flat-top hexagon outline.
            let f = half_h / (3.0_f64.sqrt());
            let m = f / 2.0;
            let points = [
                (-half_w + m, -half_h),
                (half_w - m, -half_h),
                (half_w, 0.0),
                (half_w - m, half_h),
                (-half_w + m, half_h),
                (-half_w, 0.0),
            ];
            let pts = points
                .iter()
                .map(|(x, y)| format!("{},{}", fmt_num(*x), fmt_num(*y)))
                .collect::<Vec<_>>()
                .join(" ");
            out.push_str(&format!(
                r#"<polygon class="basic label-container" style="" points="{p}"></polygon>"#,
                p = pts,
            ));
        }
        ShapeKind::Bang => {
            // Upstream `bangShape` (src/rendering-util/rendering-elements/
            // shapes/bang.ts): 12-arc explosion path drawn from `M0 0`
            // with a translate of (-effectiveWidth/2, -effectiveHeight/2)
            // applied to the path itself. effectiveWidth/Height already
            // baked into shape_w/shape_h by the layout sizing pass.
            let ew = n.shape_w;
            let eh = n.shape_h;
            let r = 0.15 * ew;
            let r08 = r * 0.8;
            let path = format!(
                "M0 0 \n    a{r},{r} 1 0,0 {a1},{b1}\n    a{r},{r} 1 0,0 {a1},{z}\n    a{r},{r} 1 0,0 {a1},{z}\n    a{r},{r} 1 0,0 {a1},{b2}\n\n    a{r},{r} 1 0,0 {c1},{d1}\n    a{r08},{r08} 1 0,0 0,{d2}\n    a{r},{r} 1 0,0 {c2},{d1}\n\n    a{r},{r} 1 0,0 {e1},{f1}\n    a{r},{r} 1 0,0 {e1},0\n    a{r},{r} 1 0,0 {e1},0\n    a{r},{r} 1 0,0 {e1},{f2}\n\n    a{r},{r} 1 0,0 {g1},{h1}\n    a{r08},{r08} 1 0,0 0,{h2}\n    a{r},{r} 1 0,0 {g2},{h1}\n  H0 V0 Z",
                r = fmt_num(r),
                r08 = fmt_num(r08),
                a1 = fmt_num(ew * 0.25),
                b1 = fmt_num(-1.0 * eh * 0.1),
                z = fmt_num(0.0),
                b2 = fmt_num(eh * 0.1),
                c1 = fmt_num(ew * 0.15),
                d1 = fmt_num(eh * 0.33),
                d2 = fmt_num(eh * 0.34),
                c2 = fmt_num(-1.0 * ew * 0.15),
                e1 = fmt_num(-1.0 * ew * 0.25),
                f1 = fmt_num(eh * 0.15),
                f2 = fmt_num(-1.0 * eh * 0.15),
                g1 = fmt_num(-1.0 * ew * 0.1),
                h1 = fmt_num(-1.0 * eh * 0.33),
                h2 = fmt_num(-1.0 * eh * 0.34),
                g2 = fmt_num(ew * 0.1),
            );
            out.push_str(&format!(
                r#"<path class="basic label-container" style="" d="{d}" transform="translate({tx}, {ty})"></path>"#,
                d = path,
                tx = fmt_num(-ew / 2.0),
                ty = fmt_num(-eh / 2.0),
            ));
        }
        ShapeKind::Cloud => {
            // Upstream `cloudShape` (src/rendering-util/rendering-elements/
            // shapes/cloud.ts): 9-arc puffy cloud path drawn from `M0 0`
            // with a translate of (-w/2, -h/2). w/h already baked into
            // shape_w/shape_h by the layout sizing pass.
            let w = n.shape_w;
            let h = n.shape_h;
            let r1 = 0.15 * w;
            let r2 = 0.25 * w;
            let r3 = 0.35 * w;
            let r4 = 0.20 * w;
            let path = format!(
                "M0 0 \n    a{r1},{r1} 0 0,1 {a1},{b1}\n    a{r3},{r3} 1 0,1 {a2},{b1}\n    a{r2},{r2} 1 0,1 {a3},{b2}\n\n    a{r1},{r1} 1 0,1 {c1},{d1}\n    a{r4},{r4} 1 0,1 {c2},{d2}\n\n    a{r2},{r1} 1 0,1 {e1},{f1}\n    a{r3},{r3} 1 0,1 {e2},0\n    a{r1},{r1} 1 0,1 {e1},{f2}\n\n    a{r1},{r1} 1 0,1 {g1},{h1}\n    a{r4},{r4} 1 0,1 {g2},{h2}\n  H0 V0 Z",
                r1 = fmt_num(r1),
                r2 = fmt_num(r2),
                r3 = fmt_num(r3),
                r4 = fmt_num(r4),
                a1 = fmt_num(w * 0.25),
                b1 = fmt_num(-1.0 * w * 0.1),
                a2 = fmt_num(w * 0.4),
                a3 = fmt_num(w * 0.35),
                b2 = fmt_num(w * 0.2),
                c1 = fmt_num(w * 0.15),
                d1 = fmt_num(h * 0.35),
                c2 = fmt_num(-1.0 * w * 0.15),
                d2 = fmt_num(h * 0.65),
                e1 = fmt_num(-1.0 * w * 0.25),
                f1 = fmt_num(w * 0.15),
                e2 = fmt_num(-1.0 * w * 0.5),
                f2 = fmt_num(-1.0 * w * 0.15),
                g1 = fmt_num(-1.0 * w * 0.1),
                h1 = fmt_num(-1.0 * h * 0.35),
                g2 = fmt_num(w * 0.1),
                h2 = fmt_num(-1.0 * h * 0.65),
            );
            out.push_str(&format!(
                r#"<path class="basic label-container" style="" d="{d}" transform="translate({tx}, {ty})"></path>"#,
                d = path,
                tx = fmt_num(-w / 2.0),
                ty = fmt_num(-h / 2.0),
            ));
        }
    }
}

/// Emit the four `<marker>` definitions + opening element wrapper that
/// `insertMarkers` produces for mindmap diagrams.
fn build_markers(id: &str) -> String {
    let mut s = String::with_capacity(2048);
    s.push_str(&format!(
        r#"<marker id="{id}_mindmap-pointEnd" class="marker mindmap" viewBox="0 0 10 10" refX="5" refY="5" markerUnits="userSpaceOnUse" markerWidth="8" markerHeight="8" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1,0;"></path></marker>"#,
    ));
    s.push_str(&format!(
        r#"<marker id="{id}_mindmap-pointStart" class="marker mindmap" viewBox="0 0 10 10" refX="4.5" refY="5" markerUnits="userSpaceOnUse" markerWidth="8" markerHeight="8" orient="auto"><path d="M 0 5 L 10 10 L 10 0 z" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1,0;"></path></marker>"#,
    ));
    s.push_str(&format!(
        r#"<marker id="{id}_mindmap-pointEnd-margin" class="marker mindmap" viewBox="0 0 11.5 14" refX="11.5" refY="7" markerUnits="userSpaceOnUse" markerWidth="10.5" markerHeight="14" orient="auto"><path d="M 0 0 L 11.5 7 L 0 14 z" class="arrowMarkerPath" style="stroke-width: 0; stroke-dasharray: 1,0;"></path></marker>"#,
    ));
    s.push_str(&format!(
        r#"<marker id="{id}_mindmap-pointStart-margin" class="marker mindmap" viewBox="0 0 11.5 14" refX="1" refY="7" markerUnits="userSpaceOnUse" markerWidth="11.5" markerHeight="14" orient="auto"><polygon points="0,7 11.5,14 11.5,0" class="arrowMarkerPath" style="stroke-width: 0; stroke-dasharray: 1,0;"></polygon></marker>"#,
    ));
    s
}

/// Build the mindmap-specific `<style>` block. Mirrors
/// `packages/mermaid/src/diagrams/mindmap/styles.ts::getStyles`,
/// expanded for `THEME_COLOR_LIMIT = 12` sections plus the section-root
/// rules.
fn build_style_block(id: &str, theme: &ThemeVariables) -> String {
    let mut s = String::with_capacity(16 * 1024);
    s.push_str("<style>");

    let font_family = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\", verdana, arial, sans-serif");
    let font_size = theme.font_size.as_deref().unwrap_or("16px");
    let text_color = theme.text_color.as_deref().unwrap_or("#333");

    // The CSS uses `font-family:"...";` without the inter-name spaces
    // present in the source variable. mermaid's stylis transform
    // collapses the spaces; we replicate by stripping them between
    // commas.
    let ff_compact = font_family.replace(", ", ",");

    // Top block.
    s.push_str(&format!(
        "#{id}{{font-family:{ff};font-size:{fs};fill:{fc};}}",
        id = id,
        ff = ff_compact,
        fs = font_size,
        fc = text_color,
    ));
    s.push_str("@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}");
    s.push_str("@keyframes dash{to{stroke-dashoffset:0;}}");
    s.push_str(&format!(
        "#{id} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}"
    ));
    s.push_str(&format!(
        "#{id} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}"
    ));
    s.push_str(&format!(
        "#{id} .error-icon{{fill:#552222;}}#{id} .error-text{{fill:#552222;stroke:#552222;}}"
    ));
    s.push_str(&format!(
        "#{id} .edge-thickness-normal{{stroke-width:1px;}}#{id} .edge-thickness-thick{{stroke-width:3.5px;}}"
    ));
    s.push_str(&format!(
        "#{id} .edge-pattern-solid{{stroke-dasharray:0;}}#{id} .edge-thickness-invisible{{stroke-width:0;fill:none;}}"
    ));
    s.push_str(&format!(
        "#{id} .edge-pattern-dashed{{stroke-dasharray:3;}}#{id} .edge-pattern-dotted{{stroke-dasharray:2;}}"
    ));
    s.push_str(&format!(
        "#{id} .marker{{fill:#333333;stroke:#333333;}}#{id} .marker.cross{{stroke:#333333;}}"
    ));
    s.push_str(&format!(
        "#{id} svg{{font-family:{ff};font-size:{fs};}}#{id} p{{margin:0;}}",
        id = id,
        ff = ff_compact,
        fs = font_size,
    ));
    s.push_str(&format!("#{id} .edge{{stroke-width:3;}}"));

    // Per-section rules — 12 iterations producing section-{-1..10}.
    let theme_str = theme.theme_variant_name();
    let look = "classic"; // mindmap fixtures we cover use the classic look
    for i in 0..12_i32 {
        write_section_block(&mut s, id, i, theme, &theme_str, look);
    }

    // Section-root rules.
    let git0 = theme
        .git0
        .as_deref()
        .unwrap_or("hsl(240, 100%, 46.2745098039%)");
    let git_branch_label0 = theme.git_branch_label0.as_deref().unwrap_or("#ffffff");
    let span_color = if theme_str.contains("redux") {
        theme.node_border.as_deref().unwrap_or("#9370DB")
    } else {
        git_branch_label0
    };

    s.push_str(&format!(
        "#{id} .section-root rect,#{id} .section-root path,#{id} .section-root circle,#{id} .section-root polygon{{fill:{g0};}}",
        id = id,
        g0 = git0,
    ));
    s.push_str(&format!("#{id} .section-root text{{fill:{l};}}", l = git_branch_label0));
    s.push_str(&format!("#{id} .section-root span{{color:{c};}}", c = span_color));

    s.push_str(&format!(
        "#{id} .icon-container{{height:100%;display:flex;justify-content:center;align-items:center;}}"
    ));
    s.push_str(&format!("#{id} .edge{{fill:none;}}"));
    s.push_str(&format!(
        "#{id} .mindmap-node-label{{dy:1em;alignment-baseline:middle;text-anchor:middle;dominant-baseline:middle;text-align:center;}}"
    ));

    let drop_shadow = theme
        .drop_shadow
        .as_deref()
        .unwrap_or("drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))");
    let scoped_drop_shadow = drop_shadow.replace("url(#drop-shadow)", &format!("url({id}-drop-shadow)"));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].mindmap-node{{filter:{ds};}}",
        ds = scoped_drop_shadow,
    ));
    let neo_root_fill = if theme_str.contains("redux") {
        theme.main_bkg.as_deref().unwrap_or("#ECECFF")
    } else {
        git0
    };
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].mindmap-node.section-root rect,#{id} [data-look=\"neo\"].mindmap-node.section-root path,#{id} [data-look=\"neo\"].mindmap-node.section-root circle,#{id} [data-look=\"neo\"].mindmap-node.section-root polygon{{fill:{f};}}",
        f = neo_root_fill,
    ));
    let neo_root_label = if theme_str.contains("redux") {
        theme.node_border.as_deref().unwrap_or("#9370DB").to_string()
    } else {
        c_scale_label(theme, if theme_str == "neutral" { 1 } else { 0 })
    };
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].mindmap-node.section-root .text-inner-tspan{{fill:{l};}}",
        l = neo_root_label,
    ));

    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    s.push_str(&format!("#{id} .node .neo-node{{stroke:{nb};}}", nb = node_border));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].node rect,#{id} [data-look=\"neo\"].cluster rect,#{id} [data-look=\"neo\"].node polygon{{stroke:{nb};filter:{ds};}}",
        nb = node_border, ds = scoped_drop_shadow,
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].node path{{stroke:{nb};stroke-width:1px;}}",
        nb = node_border
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].node .outer-path{{filter:{ds};}}",
        ds = scoped_drop_shadow,
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].node .neo-line path{{stroke:{nb};filter:none;}}",
        nb = node_border
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].node circle{{stroke:{nb};filter:{ds};}}",
        nb = node_border, ds = scoped_drop_shadow,
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].node circle .state-start{{fill:#000000;}}"
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].icon-shape .icon{{fill:{nb};filter:{ds};}}",
        nb = node_border, ds = scoped_drop_shadow,
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].icon-shape .icon-neo path{{stroke:{nb};filter:{ds};}}",
        nb = node_border, ds = scoped_drop_shadow,
    ));
    s.push_str(&format!(
        "#{id} :root{{--mermaid-font-family:{ff};}}",
        ff = ff_compact,
    ));
    let _ = look;

    s.push_str("</style>");
    s
}

/// Emit the per-section CSS block for section index `(i - 1)` (so the
/// outer caller passes `i` from 0..THEME_COLOR_LIMIT, producing
/// `section-{-1..10}`).
fn write_section_block(
    s: &mut String,
    id: &str,
    i: i32,
    theme: &ThemeVariables,
    theme_str: &str,
    _look: &str,
) {
    let sec = i - 1;
    let scale = c_scale(theme, i as usize);
    let scale_label = c_scale_label(theme, i as usize);
    let scale_inv = c_scale_inv(theme, i as usize);
    // sw computation: classic look uses `17 - 3*i`.
    let sw = 17 - 3 * i;

    s.push_str(&format!(
        "#{id} .section-{sec} rect,#{id} .section-{sec} path,#{id} .section-{sec} circle,#{id} .section-{sec} polygon,#{id} .section-{sec} path{{fill:{c};}}",
        c = scale,
    ));
    s.push_str(&format!(
        "#{id} .section-{sec} text{{fill:{l};}}#{id} .section-{sec} span{{color:{l};}}",
        l = scale_label,
    ));
    s.push_str(&format!(
        "#{id} .node-icon-{sec}{{font-size:40px;color:{l};}}",
        l = scale_label,
    ));
    s.push_str(&format!(
        "#{id} .section-edge-{sec}{{stroke:{c};}}",
        c = scale,
    ));
    s.push_str(&format!(
        "#{id} .edge-depth-{sec}{{stroke-width:{sw};}}",
    ));
    s.push_str(&format!(
        "#{id} .section-{sec} line{{stroke:{li};stroke-width:3;}}",
        li = scale_inv,
    ));
    s.push_str(&format!(
        "#{id} .disabled,#{id} .disabled circle,#{id} .disabled text{{fill:lightgray;}}#{id} .disabled text{{fill:#efefef;}}"
    ));

    let stroke_width = theme.stroke_width.unwrap_or(2);
    let neo_fill = if matches!(theme_str, "redux" | "redux-dark" | "neutral") {
        theme.main_bkg.as_deref().unwrap_or("#ECECFF").to_string()
    } else {
        scale.clone()
    };
    let neo_stroke = if matches!(theme_str, "redux" | "redux-dark") {
        theme.node_border.as_deref().unwrap_or("#9370DB").to_string()
    } else {
        scale.clone()
    };
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].mindmap-node.section-{sec} rect,#{id} [data-look=\"neo\"].mindmap-node.section-{sec} path,#{id} [data-look=\"neo\"].mindmap-node.section-{sec} circle,#{id} [data-look=\"neo\"].mindmap-node.section-{sec} polygon{{fill:{f};stroke:{st};stroke-width:{sw}px;}}",
        f = neo_fill, st = neo_stroke, sw = stroke_width,
    ));
    let neo_edge = if theme_str.contains("redux") || theme_str == "neo-dark" {
        theme.node_border.as_deref().unwrap_or("#9370DB").to_string()
    } else {
        scale.clone()
    };
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].section-edge-{sec}{{stroke:{e};}}",
        e = neo_edge,
    ));
    let neo_text = if matches!(theme_str, "redux" | "redux-dark") {
        theme.node_border.as_deref().unwrap_or("#9370DB").to_string()
    } else {
        c_scale_label(theme, if theme_str == "neutral" { 1 } else { i as usize })
    };
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].mindmap-node.section-{sec} text{{fill:{t};}}",
        t = neo_text,
    ));
}

fn c_scale(theme: &ThemeVariables, i: usize) -> String {
    match i {
        0 => theme.c_scale0.clone(),
        1 => theme.c_scale1.clone(),
        2 => theme.c_scale2.clone(),
        3 => theme.c_scale3.clone(),
        4 => theme.c_scale4.clone(),
        5 => theme.c_scale5.clone(),
        6 => theme.c_scale6.clone(),
        7 => theme.c_scale7.clone(),
        8 => theme.c_scale8.clone(),
        9 => theme.c_scale9.clone(),
        10 => theme.c_scale10.clone(),
        11 => theme.c_scale11.clone(),
        _ => None,
    }
    .unwrap_or_default()
}

fn c_scale_label(theme: &ThemeVariables, i: usize) -> String {
    match i {
        0 => theme.c_scale_label0.clone(),
        1 => theme.c_scale_label1.clone(),
        2 => theme.c_scale_label2.clone(),
        3 => theme.c_scale_label3.clone(),
        4 => theme.c_scale_label4.clone(),
        5 => theme.c_scale_label5.clone(),
        6 => theme.c_scale_label6.clone(),
        7 => theme.c_scale_label7.clone(),
        8 => theme.c_scale_label8.clone(),
        9 => theme.c_scale_label9.clone(),
        10 => theme.c_scale_label10.clone(),
        11 => theme.c_scale_label11.clone(),
        _ => None,
    }
    .unwrap_or_default()
}

fn c_scale_inv(theme: &ThemeVariables, i: usize) -> String {
    match i {
        0 => theme.c_scale_inv0.clone(),
        1 => theme.c_scale_inv1.clone(),
        2 => theme.c_scale_inv2.clone(),
        3 => theme.c_scale_inv3.clone(),
        4 => theme.c_scale_inv4.clone(),
        5 => theme.c_scale_inv5.clone(),
        6 => theme.c_scale_inv6.clone(),
        7 => theme.c_scale_inv7.clone(),
        8 => theme.c_scale_inv8.clone(),
        9 => theme.c_scale_inv9.clone(),
        10 => theme.c_scale_inv10.clone(),
        11 => theme.c_scale_inv11.clone(),
        _ => None,
    }
    .unwrap_or_default()
}

trait ThemeName {
    fn theme_variant_name(&self) -> String;
}

impl ThemeName for ThemeVariables {
    fn theme_variant_name(&self) -> String {
        // We don't track the active theme name on the variables struct,
        // so derive from a fingerprint heuristic. The default theme has
        // primary_color "#ECECFF". This is sufficient for the test
        // fixtures we cover.
        match self.primary_color.as_deref() {
            Some("#ECECFF") => "default".to_string(),
            _ => "default".to_string(),
        }
    }
}

/// Emit a SVG `d` attribute matching d3-shape's `curveBasis` for a
/// 3-point line. Numbers are rounded to 3 decimals via the same
/// `Math.round(v * 1000) / 1000` rule d3-path uses when `digits = 3`.
///
/// The expansion for points `[P0, P1, P2]`:
///   M P0 L (5*P0+P1)/6 C (2*P0+P1)/3, (P0+2*P1)/3, (P0+4*P1+P2)/6
///                       C (2*P1+P2)/3, (P1+2*P2)/3, (P1+5*P2)/6
///                       L P2
fn curve_basis_path(ep: EdgePoints) -> String {
    let (x0, y0) = ep.start;
    let (x1, y1) = ep.mid;
    let (x2, y2) = ep.end;
    let mut s = String::with_capacity(160);
    s.push('M');
    s.push_str(&js_round3(x0));
    s.push(',');
    s.push_str(&js_round3(y0));
    s.push('L');
    s.push_str(&js_round3((5.0 * x0 + x1) / 6.0));
    s.push(',');
    s.push_str(&js_round3((5.0 * y0 + y1) / 6.0));
    s.push('C');
    s.push_str(&js_round3((2.0 * x0 + x1) / 3.0));
    s.push(',');
    s.push_str(&js_round3((2.0 * y0 + y1) / 3.0));
    s.push(',');
    s.push_str(&js_round3((x0 + 2.0 * x1) / 3.0));
    s.push(',');
    s.push_str(&js_round3((y0 + 2.0 * y1) / 3.0));
    s.push(',');
    s.push_str(&js_round3((x0 + 4.0 * x1 + x2) / 6.0));
    s.push(',');
    s.push_str(&js_round3((y0 + 4.0 * y1 + y2) / 6.0));
    s.push('C');
    s.push_str(&js_round3((2.0 * x1 + x2) / 3.0));
    s.push(',');
    s.push_str(&js_round3((2.0 * y1 + y2) / 3.0));
    s.push(',');
    s.push_str(&js_round3((x1 + 2.0 * x2) / 3.0));
    s.push(',');
    s.push_str(&js_round3((y1 + 2.0 * y2) / 3.0));
    s.push(',');
    s.push_str(&js_round3((x1 + 5.0 * x2) / 6.0));
    s.push(',');
    s.push_str(&js_round3((y1 + 5.0 * y2) / 6.0));
    s.push('L');
    s.push_str(&js_round3(x2));
    s.push(',');
    s.push_str(&js_round3(y2));
    s
}

/// `Math.round(v * 1000) / 1000` followed by `Number.toString()`.
fn js_round3(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    js_number_to_string(r)
}

/// Encode `[{"x":..,"y":..}, ...]` as `btoa(...)` for a `data-points`
/// attribute. JSON uses full-precision JS Number formatting and the
/// base64 encode is the standard alphabet WITHOUT `=` padding stripped
/// (matches `btoa` output).
fn encode_data_points(ep: EdgePoints) -> String {
    let mut json = String::with_capacity(96);
    json.push('[');
    let pts = [ep.start, ep.mid, ep.end];
    for (i, (x, y)) in pts.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str("{\"x\":");
        json.push_str(&js_number_to_string(*x));
        json.push_str(",\"y\":");
        json.push_str(&js_number_to_string(*y));
        json.push('}');
    }
    json.push(']');
    base64_encode(json.as_bytes())
}

/// Standard base64 encode (the alphabet `btoa` uses) WITH `=` padding.
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let a = input[i] as u32;
        let b = input[i + 1] as u32;
        let c = input[i + 2] as u32;
        let n = (a << 16) | (b << 8) | c;
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
        out.push(CHARS[(n & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let a = input[i] as u32;
        let n = a << 16;
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let a = input[i] as u32;
        let b = input[i + 1] as u32;
        let n = (a << 16) | (b << 8);
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    out
}

fn html_escape(s: &str) -> String {
    // Mirror upstream `markdownToHTML`: `<br/>`, `<br>`, `<br />` are
    // inline HTML elements that marked.lexer passes through verbatim, so
    // they survive `span.html(...)` as real `<br>` elements. Everything
    // else gets standard HTML escaping. Without this, `<br/>` ends up as
    // `&lt;br/&gt;` in our output and the diff breaks at the inner span.
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Try to recognise a `<br>`, `<br/>`, `<br />` (any case).
            if i + 3 <= bytes.len()
                && bytes[i + 1].eq_ignore_ascii_case(&b'b')
                && bytes[i + 2].eq_ignore_ascii_case(&b'r')
            {
                let after = bytes.get(i + 3).copied();
                if matches!(after, Some(b' ') | Some(b'/') | Some(b'>') | Some(b'\t')) {
                    if let Some(rel_end) = bytes[i..].iter().position(|&b| b == b'>') {
                        out.push_str(&s[i..i + rel_end + 1]);
                        i += rel_end + 1;
                        continue;
                    }
                }
            }
        }
        let c = bytes[i];
        match c {
            b'&' => {
                out.push_str("&amp;");
                i += 1;
            }
            b'<' => {
                out.push_str("&lt;");
                i += 1;
            }
            b'>' => {
                out.push_str("&gt;");
                i += 1;
            }
            b'"' => {
                out.push_str("&quot;");
                i += 1;
            }
            _ => {
                let cl = utf8_char_len_first(c);
                out.push_str(&s[i..(i + cl).min(bytes.len())]);
                i += cl;
            }
        }
    }
    out
}

fn utf8_char_len_first(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xC0 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

/// Suppress unused param warnings for the imported types when this
/// module is compiled with `--features=...` that strip the renderer.
#[allow(dead_code)]
fn _unused(_n: &PositionedNode) {}
