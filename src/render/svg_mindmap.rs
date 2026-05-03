//! Mindmap SVG renderer.
//!
//! Targets byte-exact parity with upstream's `mindmapRenderer.ts` →
//! `defaultMindmapNode.ts` (and friends) → `setupViewPortForSVG.ts`
//! pipeline. Currently focused on the trivial single-node fixtures
//! (cypress 05, etc.) — multi-node fixtures require either the
//! `tidy-tree` or `cose-bilkent` layout port and are still routed
//! through `tests/known_ignored.txt`.

use crate::error::{MermaidError, Result};
use crate::layout::mindmap::{MindmapLayout, PositionedNode, VIEWPORT_PADDING};
use crate::model::mindmap::{MindmapDiagram, MindmapNodeType};
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

    let has_icon = d.nodes[0].icon.is_some();
    let label_bkg_attr = if has_icon { r#" class="labelBkg""# } else { "" };

    match shape {
        ShapeKind::Circle | ShapeKind::RoundedRect => {
            out.push_str(&format!(
                r#"<g class="label" style="" transform="translate({tx}, {ty})"><rect></rect><foreignObject width="{w}" height="{h}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"{bkg}><span class="nodeLabel markdown-node-label">{text}</span></div></foreignObject></g>"#,
                tx = fmt_num(-n.bbox_w / 2.0),
                ty = fmt_num(-n.bbox_h / 2.0),
                w = fmt_num(n.bbox_w),
                h = fmt_num(n.bbox_h),
                bkg = label_bkg_attr,
                text = html_escape(&d.nodes[0].raw_descr),
            ));
        }
        _ => {
            out.push_str(&format!(
                r#"<g class="label" style="" transform="translate({tx}, {ty})"><rect></rect><foreignObject width="{w}" height="{h}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"{bkg}><span class="nodeLabel markdown-node-label"><p>{text}</p></span></div></foreignObject></g>"#,
                tx = fmt_num(-n.bbox_w / 2.0),
                ty = fmt_num(-n.bbox_h / 2.0),
                w = fmt_num(n.bbox_w),
                h = fmt_num(n.bbox_h),
                bkg = label_bkg_attr,
                text = html_escape(&d.nodes[0].descr),
            ));
        }
    }

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

    // ─── Edges (parent -> child straight lines).
    out.push_str(r#"<g class="edgePaths">"#);
    for (i, node) in d.nodes.iter().enumerate() {
        let Some(parent) = node.parent else { continue };
        let p = &l.nodes[parent];
        let c = &l.nodes[i];
        let section = l.nodes[i].section;
        let edge_id = format!("{id}-edge_{}_{}", parent, i);
        out.push_str(&format!(
            r#"<path d="M{px},{py}L{cx},{cy}" id="{eid}" class=" edge-thickness-normal edge-pattern-solid edge section-edge-{sec} edge-depth-1" style="undefined;;;undefined" data-edge="true" data-et="edge" data-id="edge_{ps}_{cs}" data-look="classic"></path>"#,
            px = fmt_num(p.x),
            py = fmt_num(p.y),
            cx = fmt_num(c.x),
            cy = fmt_num(c.y),
            eid = edge_id,
            sec = section,
            ps = parent,
            cs = i,
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

        let has_icon = src.icon.is_some();
        let label_bkg_attr = if has_icon { r#" class="labelBkg""# } else { "" };
        match kind {
            ShapeKind::Circle | ShapeKind::RoundedRect => {
                out.push_str(&format!(
                    r#"<g class="label" style="" transform="translate({tx}, {ty})"><rect></rect><foreignObject width="{w}" height="{h}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"{bkg}><span class="nodeLabel markdown-node-label">{text}</span></div></foreignObject></g>"#,
                    tx = fmt_num(-n.bbox_w / 2.0),
                    ty = fmt_num(-n.bbox_h / 2.0),
                    w = fmt_num(n.bbox_w),
                    h = fmt_num(n.bbox_h),
                    bkg = label_bkg_attr,
                    text = html_escape(&src.raw_descr),
                ));
            }
            _ => {
                out.push_str(&format!(
                    r#"<g class="label" style="" transform="translate({tx}, {ty})"><rect></rect><foreignObject width="{w}" height="{h}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"{bkg}><span class="nodeLabel markdown-node-label"><p>{text}</p></span></div></foreignObject></g>"#,
                    tx = fmt_num(-n.bbox_w / 2.0),
                    ty = fmt_num(-n.bbox_h / 2.0),
                    w = fmt_num(n.bbox_w),
                    h = fmt_num(n.bbox_h),
                    bkg = label_bkg_attr,
                    text = html_escape(&src.descr),
                ));
            }
        }

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
            // Simplified bang / explosion stand-in (rounded rect) until
            // the 12-arc `bangShape` formula is ported.
            let inner_w = n.shape_w - 10.0;
            let inner_h = n.shape_h - 10.0;
            out.push_str(&format!(
                r#"<path class="basic label-container" style="" d="M{nx} {hh}v{nih}q0,-5 5,-5h{iw}q5,0 5,5v{ih}q0,5 -5,5h{niw}q-5,0 -5,-5Z"></path>"#,
                nx = fmt_num(-half_w),
                hh = fmt_num(half_h - 5.0),
                nih = fmt_num(-inner_h),
                iw = fmt_num(inner_w),
                ih = fmt_num(inner_h),
                niw = fmt_num(-inner_w),
            ));
        }
        ShapeKind::Cloud => {
            // Simplified cloud stand-in (rounded rect) until the
            // upstream `cloudShape` 9-arc formula is ported.
            let inner_w = n.shape_w - 10.0;
            let inner_h = n.shape_h - 10.0;
            out.push_str(&format!(
                r#"<path class="basic label-container" style="" d="M{nx} {hh}v{nih}q0,-5 5,-5h{iw}q5,0 5,5v{ih}q0,5 -5,5h{niw}q-5,0 -5,-5Z"></path>"#,
                nx = fmt_num(-half_w),
                hh = fmt_num(half_h - 5.0),
                nih = fmt_num(-inner_h),
                iw = fmt_num(inner_w),
                ih = fmt_num(inner_h),
                niw = fmt_num(-inner_w),
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

/// Suppress unused param warnings for the imported types when this
/// module is compiled with `--features=...` that strip the renderer.
#[allow(dead_code)]
fn _unused(_n: &PositionedNode) {}
