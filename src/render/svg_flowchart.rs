//! Flowchart SVG renderer. Consumes a `FlowchartDiagram` + its
//! `FlowchartLayout` and emits an SVG string.
//!
//! Upstream reference:
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/flowchart/flowRenderer-v3-unified.ts`
//! + `styles.ts` + `rendering-util/rendering-elements/shapes/*.ts`.
//!
//! Byte-exact parity for flowcharts requires matching (a) dagre's
//! exact float-point layout math with upstream's @dagrejs/dagre, (b)
//! jsdom's font metric assumptions for label measurement, and (c) the
//! precise stylis CSS scoping transform. We reuse the shape registry
//! + markers + edges modules that Wave 3 built, emit structurally
//! correct SVG here, and leave the fine byte-level polish for
//! follow-up iterations.

use crate::error::Result;
use crate::layout::flowchart::FlowchartLayout;
use crate::layout::unified::types::Point;
use crate::layout::unified::{Cluster, Edge as UEdge, Node as UNode};
use crate::model::flowchart::FlowchartDiagram;
use crate::render::edges;
use crate::render::markers;
use crate::render::shapes;
use crate::render::svg_er::fade;
use crate::render::unified_shell;
use crate::theme::css as theme_css;
use crate::theme::ThemeVariables;

/// Compute the viewBox from the layout bounds and rendered node dimensions.
///
/// Upstream uses `svg.getBBox()` after rendering, which returns the
/// tight axis-aligned bounding box of all rendered content. We
/// approximate this by computing bounds from the layout nodes,
/// accounting for shape-specific geometry (e.g. diamonds extend
/// beyond their node center by s/2).
fn compute_viewbox(l: &FlowchartLayout, _inner: &str, padding: f64) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for n in &l.nodes {
        let (Some(x), Some(y)) = (n.x, n.y) else {
            continue;
        };
        let w = n.width.unwrap_or(0.0);
        let h = n.height.unwrap_or(0.0);
        // For diamond shapes, w and h both equal s (the diagonal).
        // The diamond extends s/2 in each direction from center.
        // For rect shapes, w and h are the actual dimensions,
        // extending w/2 and h/2 from center.
        let half_w = w / 2.0;
        let half_h = h / 2.0;
        min_x = min_x.min(x - half_w);
        min_y = min_y.min(y - half_h);
        max_x = max_x.max(x + half_w);
        max_y = max_y.max(y + half_h);
    }

    // Include edge path points in the bounds.
    for e in &l.edges {
        if let Some(points) = &e.points {
            for p in points {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
            }
        }
    }

    // Include edge label positions.
    for e in &l.edges {
        if let (Some(lx), Some(ly)) = (e.label_x, e.label_y) {
            min_x = min_x.min(lx);
            min_y = min_y.min(ly);
            max_x = max_x.max(lx);
            max_y = max_y.max(ly);
        }
    }

    if !min_x.is_finite() {
        return (0.0, 0.0, 1.0, 1.0);
    }

    let vb_x = min_x - padding;
    let vb_y = min_y - padding;
    let vb_w = (max_x - min_x + 2.0 * padding).max(1.0);
    let vb_h = (max_y - min_y + 2.0 * padding).max(1.0);

    (vb_x, vb_y, vb_w, vb_h)
}

/// Render a flowchart diagram as SVG.
pub fn render(
    _d: &FlowchartDiagram,
    l: &FlowchartLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let padding = l.diagram_padding;

    // ── Render inner content first (markers + root group) ──────────
    // We need the rendered content to compute the viewBox accurately,
    // matching upstream's `getBBox()` approach.

    let mut inner = String::new();

    // Seed <g> wrapping markers + root — matches upstream's
    // dagre-unified pipeline behaviour of appending directly into
    // the seed group produced by appendDivSvgG.
    inner.push_str(unified_shell::open_seed_group());
    // Marker defs — emitted as-is (diagram-specific wrapper).
    inner.push_str(&markers::defs(&l.aria_kind, id, theme));

    // Root container — `<g class="root">` with clusters, edgePaths,
    // edgeLabels, and nodes sub-groups.
    inner.push_str(unified_shell::open_root_group());

    // Clusters (subgraphs).
    inner.push_str(&unified_shell::open_layer("clusters"));
    for cluster in &l.clusters {
        if let Some(cnode) = l.nodes.iter().find(|n| n.id == cluster.id && n.is_group) {
            inner.push_str(&render_cluster(cnode, cluster, theme, id));
        }
    }
    inner.push_str(unified_shell::close_layer());

    // Edge paths.
    inner.push_str(&unified_shell::open_layer("edgePaths"));
    for (i, e) in l.edges.iter().enumerate() {
        inner.push_str(&render_edge_path(e, i, id, &l.aria_kind));
    }
    inner.push_str(unified_shell::close_layer());

    // Edge labels.
    inner.push_str(&unified_shell::open_layer("edgeLabels"));
    for e in l.edges.iter() {
        inner.push_str(&render_edge_label(e));
    }
    inner.push_str(unified_shell::close_layer());

    // Nodes.
    inner.push_str(&unified_shell::open_layer("nodes"));
    for n in &l.nodes {
        if n.is_group {
            continue;
        }
        // Prepend SVG id to dom_id — upstream prefixes the stored
        // domId with the diagram's SVG element id at lookup time
        // (see flowDb.lookUpDomId).
        let mut prefixed = n.clone();
        if let Some(did) = &prefixed.dom_id {
            prefixed.dom_id = Some(format!("{svg_id}-{did}", svg_id = id));
        }
        // Dispatch to the shape registry. Unknown shapes fall back to rect.
        let shape_id = prefixed.shape.clone().unwrap_or_else(|| "rect".to_string());
        match shapes::draw(&shape_id, &prefixed, theme) {
            Ok(svg) => inner.push_str(&svg),
            Err(_) => {
                // Fallback: plain rect.
                if let Ok(svg) = shapes::draw("rect", &prefixed, theme) {
                    inner.push_str(&svg);
                }
            }
        }
    }
    inner.push_str(unified_shell::close_layer());

    inner.push_str(unified_shell::close_root_group());
    inner.push_str(unified_shell::close_seed_group());

    // ── Compute viewBox from rendered content ──────────────────────
    // Upstream uses `svg.getBBox()` which returns the actual rendered
    // bounds including shape geometry, edge curves, and label
    // positions. We compute from layout nodes and edges.
    let (vb_x, vb_y, vb_w, vb_h) = compute_viewbox(l, &inner, padding);

    // ── Assemble final SVG ─────────────────────────────────────────
    let mut out = String::new();
    out.push_str(&unified_shell::open_unified_svg(
        id,
        vb_w,
        (vb_x, vb_y, vb_w, vb_h),
        Some("flowchart"),
        &l.aria_kind,
    ));

    // <style> block — shared preamble + flowchart slice + shared tail.
    out.push_str("<style>");
    out.push_str(&theme_css::base_preamble(id, theme));
    out.push_str(&flowchart_specific_css(id, theme));
    out.push_str(&theme_css::neo_look_block(id, theme));
    out.push_str("</style>");

    out.push_str(&inner);

    out.push_str(&unified_shell::emit_defs_shell(id, true, true));
    out.push_str(unified_shell::close_unified_svg());
    Ok(out)
}

fn render_cluster(node: &UNode, _cluster: &Cluster, _theme: &ThemeVariables, svg_id: &str) -> String {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let x = node.x.unwrap_or(0.0);
    let y = node.y.unwrap_or(0.0);
    let base_id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let id = format!("{svg_id}-{base_id}");
    let label = node.label.clone().unwrap_or_default();

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="cluster default" id="{id}" transform="translate({tx}, {ty})">"#,
        id = xml_escape(&id),
        tx = fmt_num(x),
        ty = fmt_num(y),
    ));
    out.push_str(&format!(
        r#"<rect class="label-container" x="{x}" y="{y}" width="{w}" height="{h}"/>"#,
        x = fmt_num(-w / 2.0),
        y = fmt_num(-h / 2.0),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    if !label.is_empty() {
        out.push_str(&format!(
            r#"<g class="cluster-label" transform="translate(0, {})"><foreignObject><div xmlns="http://www.w3.org/1999/xhtml" class="nodeLabel"><span class="nodeLabel">{}</span></div></foreignObject></g>"#,
            fmt_num(-h / 2.0 + 12.0),
            xml_escape(&label),
        ));
    }
    out.push_str("</g>");
    out
}

fn render_edge_path(e: &UEdge, _index: usize, svg_id: &str, aria_kind: &str) -> String {
    let pts: Vec<Point> = e
        .points
        .as_ref()
        .map(|v| v.iter().map(|p| Point { x: p.x, y: p.y }).collect())
        .unwrap_or_default();
    if pts.is_empty() {
        return String::new();
    }
    // Build `d=` via the curve configured on this edge.
    let curve = e.curve.as_deref().unwrap_or("basis");
    let ctype = edges::CurveType::parse(curve).unwrap_or(edges::CurveType::Basis);
    let d_attr = edges::build_path(&pts, ctype);

    let thickness = e.thickness.as_deref().unwrap_or("normal");
    let pattern = e.pattern.as_deref().unwrap_or("solid");
    // Upstream duplicates thickness/pattern classes — see insertEdge in
    // dagre-wrapper index.js. Leading space is intentional (matches upstream).
    let class_attr = format!(
        " edge-thickness-{thickness} edge-pattern-{pattern} edge-thickness-{thickness} edge-pattern-{pattern} flowchart-link"
    );

    // Upstream writes `style=";"` when no explicit edge style is set.
    let style_val = e
        .style
        .as_ref()
        .map(|v| {
            if v.is_empty() || v.iter().all(|s| s.is_empty()) {
                ";".to_string()
            } else {
                v.join(";")
            }
        })
        .unwrap_or_else(|| ";".to_string());

    let edge_id = format!("{svg_id}-{id}", id = e.id.clone());

    // data-points: base64-encoded JSON array of {x, y} objects.
    let data_points_b64 = {
        let mut json = String::from("[");
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                r#"{{"x":{x},"y":{y}}}"#,
                x = fmt_num(p.x),
                y = fmt_num(p.y),
            ));
        }
        json.push(']');
        unified_shell::base64_encode(json.as_bytes())
    };

    let marker_end = match e.arrow_type_end.as_deref() {
        Some("arrow_circle") => {
            format!(r#" marker-end="url(#{svg_id}_{aria_kind}-circleEnd)""#)
        }
        Some("arrow_cross") => {
            format!(r#" marker-end="url(#{svg_id}_{aria_kind}-crossEnd)""#)
        }
        _ => {
            // Default arrow (point) — upstream emits marker-end for
            // arrow_point, arrow, and the None case.
            format!(r#" marker-end="url(#{svg_id}_{aria_kind}-pointEnd)""#)
        }
    };
    let marker_start = match e.arrow_type_start.as_deref() {
        Some("arrow_point") | Some("arrow") => {
            format!(r#" marker-start="url(#{svg_id}_{aria_kind}-pointStart)""#)
        }
        Some("arrow_circle") => {
            format!(r#" marker-start="url(#{svg_id}_{aria_kind}-circleStart)""#)
        }
        Some("arrow_cross") => {
            format!(r#" marker-start="url(#{svg_id}_{aria_kind}-crossStart)""#)
        }
        _ => String::new(),
    };

    format!(
        r#"<path d="{d}" id="{eid}" class="{cls}" style="{st}" data-edge="true" data-et="edge" data-id="{did}" data-points="{b64}" data-look="classic"{me}{ms}></path>"#,
        d = d_attr,
        eid = edge_id,
        cls = class_attr,
        st = style_val,
        did = e.id,
        b64 = data_points_b64,
        me = marker_end,
        ms = marker_start,
    )
}

fn render_edge_label(e: &UEdge) -> String {
    use crate::render::foreign_object::{
        measure_html_label, render_edge_label as fo_edge, HtmlLabelFont, LabelOpts,
    };
    let label_text = e.label.clone().unwrap_or_default();
    let esc = xml_escape(&label_text);
    let is_empty = esc.is_empty();
    // Upstream always measures the label height (even when empty),
    // using the font's line-height. For empty labels, width=0 but
    // height is still the font's line-height.
    let (w, h) = if is_empty {
        let (_, lh) = measure_html_label("X", &HtmlLabelFont::default(), 200.0, true);
        (0.0, lh)
    } else {
        measure_html_label(&esc, &HtmlLabelFont::default(), 200.0, true)
    };
    let lx = e.label_x.unwrap_or(0.0);
    let ly = e.label_y.unwrap_or(0.0);
    let opts = LabelOpts {
        data_id: Some(&e.id),
        group_style: None,
        wrap_in_p: !is_empty,
        ..LabelOpts::default()
    };
    fo_edge(&esc, lx, ly, w, h, opts)
}

/// Build the flowchart-specific CSS slice — a complete port of upstream's
/// `styles.ts` → `getStyles()` output, scoped to `#<id>`. This replaces
/// the former minimal `build_css()` and emits every rule the upstream
/// flowchart CSS template produces after stylis minification.
///
/// The caller sandwiches this between [`theme_css::base_preamble`] and
/// [`theme_css::neo_look_block`] inside the `<style>` block.
fn flowchart_specific_css(id: &str, theme: &ThemeVariables) -> String {
    // Resolve theme variables with upstream defaults.
    let ff_raw = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\", verdana, arial, sans-serif");
    let ff = crate::render::stylis::strip_comma_spaces(ff_raw);
    let node_text_color = theme
        .node_text_color
        .as_deref()
        .or(theme.text_color.as_deref())
        .unwrap_or("#333");
    let title_color = theme
        .title_color
        .as_deref()
        .or(theme.text_color.as_deref())
        .unwrap_or("#333");
    let main_bkg = theme.main_bkg.as_deref().unwrap_or("#ECECFF");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let stroke_width = theme.stroke_width.unwrap_or(1);
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");
    let arrowhead_color = theme
        .arrowhead_color
        .as_deref()
        .unwrap_or("#333333");
    let edge_label_bg = theme
        .edge_label_background
        .as_deref()
        .unwrap_or("rgba(232,232,232, 0.8)");
    let cluster_bkg = theme.cluster_bkg.as_deref().unwrap_or("#ffffde");
    let cluster_border = theme.cluster_border.as_deref().unwrap_or("#aaaa33");
    let tertiary_color = theme
        .tertiary_color
        .as_deref()
        .unwrap_or("hsl(80, 100%, 96.2745098039%)");
    let border2 = theme.border2.as_deref().unwrap_or("#aaaa33");
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let font_family_tooltip = ff.clone();

    // labelBkg: upstream does `fade(options.edgeLabelBackground, 0.5)`.
    let labelbkg_color = fade(edge_label_bg, 0.5);

    let mut css = String::with_capacity(4000);

    // .label { font-family: ...; color: nodeTextColor || textColor; }
    css.push_str(&format!(
        "#{id} .label{{font-family:{ff};color:{ntc};}}",
        ntc = node_text_color,
    ));

    // .cluster-label text { fill: titleColor; }
    css.push_str(&format!(
        "#{id} .cluster-label text{{fill:{tc};}}",
        tc = title_color,
    ));

    // .cluster-label span { color: titleColor; }
    css.push_str(&format!(
        "#{id} .cluster-label span{{color:{tc};}}",
        tc = title_color,
    ));

    // .cluster-label span p { background-color: transparent; }
    css.push_str(&format!(
        "#{id} .cluster-label span p{{background-color:transparent;}}",
    ));

    // .label text, span { fill: nodeTextColor || textColor; color: nodeTextColor || textColor; }
    // Note: stylis expands `.label text,span` → `#id .label text,#id span`
    css.push_str(&format!(
        "#{id} .label text,#{id} span{{fill:{ntc};color:{ntc};}}",
        ntc = node_text_color,
    ));

    // .node rect, .node circle, .node ellipse, .node polygon, .node path
    css.push_str(&format!(
        "#{id} .node rect,#{id} .node circle,#{id} .node ellipse,#{id} .node polygon,#{id} .node path{{fill:{mb};stroke:{nb};stroke-width:{sw}px;}}",
        mb = main_bkg,
        nb = node_border,
        sw = stroke_width,
    ));

    // .rough-node .label text, .node .label text, .image-shape .label, .icon-shape .label
    // { text-anchor: middle; }
    css.push_str(&format!(
        "#{id} .rough-node .label text,#{id} .node .label text,#{id} .image-shape .label,#{id} .icon-shape .label{{text-anchor:middle;}}",
    ));

    // .node .katex path { fill: #000; stroke: #000; stroke-width: 1px; }
    css.push_str(&format!(
        "#{id} .node .katex path{{fill:#000;stroke:#000;stroke-width:1px;}}",
    ));

    // .rough-node .label, .node .label, .image-shape .label, .icon-shape .label
    // { text-align: center; }
    css.push_str(&format!(
        "#{id} .rough-node .label,#{id} .node .label,#{id} .image-shape .label,#{id} .icon-shape .label{{text-align:center;}}",
    ));

    // .node.clickable { cursor: pointer; }
    css.push_str(&format!(
        "#{id} .node.clickable{{cursor:pointer;}}",
    ));

    // .root .anchor path { fill: lineColor !important; stroke-width: 0; stroke: lineColor; }
    css.push_str(&format!(
        "#{id} .root .anchor path{{fill:{lc}!important;stroke-width:0;stroke:{lc};}}",
        lc = line_color,
    ));

    // .arrowheadPath { fill: arrowheadColor; }
    css.push_str(&format!(
        "#{id} .arrowheadPath{{fill:{ac};}}",
        ac = arrowhead_color,
    ));

    // .edgePath .path { stroke: lineColor; stroke-width: strokeWidth ?? 2px; }
    css.push_str(&format!(
        "#{id} .edgePath .path{{stroke:{lc};stroke-width:{sw}px;}}",
        lc = line_color,
        sw = stroke_width,
    ));

    // .flowchart-link { stroke: lineColor; fill: none; }
    css.push_str(&format!(
        "#{id} .flowchart-link{{stroke:{lc};fill:none;}}",
        lc = line_color,
    ));

    // .edgeLabel { background-color: edgeLabelBackground; text-align: center; }
    css.push_str(&format!(
        "#{id} .edgeLabel{{background-color:{ebg};text-align:center;}}",
        ebg = edge_label_bg,
    ));

    // .edgeLabel p { background-color: edgeLabelBackground; }
    css.push_str(&format!(
        "#{id} .edgeLabel p{{background-color:{ebg};}}",
        ebg = edge_label_bg,
    ));

    // .edgeLabel rect { opacity: 0.5; background-color: edgeLabelBackground; fill: edgeLabelBackground; }
    css.push_str(&format!(
        "#{id} .edgeLabel rect{{opacity:0.5;background-color:{ebg};fill:{ebg};}}",
        ebg = edge_label_bg,
    ));

    // .labelBkg { background-color: fade(edgeLabelBackground, 0.5); }
    css.push_str(&format!(
        "#{id} .labelBkg{{background-color:{lbkg};}}",
        lbkg = labelbkg_color,
    ));

    // .cluster rect { fill: clusterBkg; stroke: clusterBorder; stroke-width: 1px; }
    css.push_str(&format!(
        "#{id} .cluster rect{{fill:{cb};stroke:{cbr};stroke-width:1px;}}",
        cb = cluster_bkg,
        cbr = cluster_border,
    ));

    // .cluster text { fill: titleColor; }
    css.push_str(&format!(
        "#{id} .cluster text{{fill:{tc};}}",
        tc = title_color,
    ));

    // .cluster span { color: titleColor; }
    css.push_str(&format!(
        "#{id} .cluster span{{color:{tc};}}",
        tc = title_color,
    ));

    // div.mermaidTooltip
    css.push_str(&format!(
        "#{id} div.mermaidTooltip{{position:absolute;text-align:center;max-width:200px;padding:2px;font-family:{ff_tip};font-size:12px;background:{tc3};border:1px solid {b2};border-radius:2px;pointer-events:none;z-index:100;}}",
        ff_tip = font_family_tooltip,
        tc3 = tertiary_color,
        b2 = border2,
    ));

    // .flowchartTitleText { text-anchor: middle; font-size: 18px; fill: textColor; }
    css.push_str(&format!(
        "#{id} .flowchartTitleText{{text-anchor:middle;font-size:18px;fill:{tc};}}",
        tc = text_color,
    ));

    // rect.text { fill: none; stroke-width: 0; }
    css.push_str(&format!(
        "#{id} rect.text{{fill:none;stroke-width:0;}}",
    ));

    // .icon-shape, .image-shape { background-color: edgeLabelBackground; text-align: center; }
    css.push_str(&format!(
        "#{id} .icon-shape,#{id} .image-shape{{background-color:{ebg};text-align:center;}}",
        ebg = edge_label_bg,
    ));

    // .icon-shape p, .image-shape p { background-color: edgeLabelBackground; padding: 2px; }
    css.push_str(&format!(
        "#{id} .icon-shape p,#{id} .image-shape p{{background-color:{ebg};padding:2px;}}",
        ebg = edge_label_bg,
    ));

    // .icon-shape .label rect, .image-shape .label rect
    css.push_str(&format!(
        "#{id} .icon-shape .label rect,#{id} .image-shape .label rect{{opacity:0.5;background-color:{ebg};fill:{ebg};}}",
        ebg = edge_label_bg,
    ));

    // getIconStyles() — from globalStyles.ts
    css.push_str(&format!(
        "#{id} .label-icon{{display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;}}",
    ));

    css.push_str(&format!(
        "#{id} .node .label-icon path{{fill:currentColor;stroke:revert;stroke-width:revert;}}",
    ));

    css
}

// ─── helpers ────────────────────────────────────────────────────────

fn fmt_num(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.fract() == 0.0 && v.abs() < 1e16 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

fn xml_escape(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::flowchart as fcl;
    use crate::parser::flowchart as fcp;
    use crate::theme;

    #[test]
    fn renders_minimal_svg() {
        let src = "flowchart TD\nA --> B\n";
        let d = fcp::parse(src).unwrap();
        let th = theme::get_theme("default");
        let l = fcl::layout(&d, &th).unwrap();
        let svg = render(&d, &l, &th, "test").unwrap();
        assert!(svg.starts_with("<svg "));
        assert!(svg.contains(r#"aria-roledescription="flowchart-v2""#));
        assert!(svg.contains(r#"class="root""#));
        assert!(svg.contains(r#"class="nodes""#));
        assert!(svg.contains(r#"class="edgePaths""#));
    }

    #[test]
    fn renders_graph_lr_as_flowchart_v1() {
        let src = "graph LR\nA-->B\n";
        let d = fcp::parse(src).unwrap();
        let th = theme::get_theme("default");
        let l = fcl::layout(&d, &th).unwrap();
        let svg = render(&d, &l, &th, "t").unwrap();
        assert!(svg.contains(r#"aria-roledescription="flowchart-v1""#));
    }

    #[test]
    fn renders_subgraph_as_cluster() {
        let src = "flowchart TD\nsubgraph s1 [Title]\nA-->B\nend\n";
        let d = fcp::parse(src).unwrap();
        let th = theme::get_theme("default");
        let l = fcl::layout(&d, &th).unwrap();
        let svg = render(&d, &l, &th, "t").unwrap();
        assert!(svg.contains(r#"class="clusters""#));
        assert!(svg.contains(r#"class="cluster"#));
    }

    #[test]
    fn flowchart_css_contains_all_upstream_rules() {
        let th = theme::get_theme("default");
        let css = flowchart_specific_css("test", &th);
        // Verify all major CSS rules from upstream styles.ts are present.
        assert!(css.contains("#test .label{"), "missing .label rule");
        assert!(css.contains("#test .cluster-label text{"), "missing .cluster-label text");
        assert!(css.contains("#test .cluster-label span{"), "missing .cluster-label span");
        assert!(css.contains("#test .cluster-label span p{"), "missing .cluster-label span p");
        assert!(css.contains("#test .label text,#test span{"), "missing .label text,span");
        assert!(css.contains("#test .node rect,"), "missing .node rect");
        assert!(css.contains("#test .arrowheadPath{"), "missing .arrowheadPath");
        assert!(css.contains("#test .edgePath .path{"), "missing .edgePath .path");
        assert!(css.contains("#test .flowchart-link{"), "missing .flowchart-link");
        assert!(css.contains("#test .edgeLabel{"), "missing .edgeLabel");
        assert!(css.contains("#test .edgeLabel p{"), "missing .edgeLabel p");
        assert!(css.contains("#test .edgeLabel rect{"), "missing .edgeLabel rect");
        assert!(css.contains("#test .labelBkg{"), "missing .labelBkg");
        assert!(css.contains("#test .cluster rect{"), "missing .cluster rect");
        assert!(css.contains("#test .cluster text{"), "missing .cluster text");
        assert!(css.contains("#test .cluster span{"), "missing .cluster span");
        assert!(css.contains("#test div.mermaidTooltip{"), "missing div.mermaidTooltip");
        assert!(css.contains("#test .flowchartTitleText{"), "missing .flowchartTitleText");
        assert!(css.contains("#test rect.text{"), "missing rect.text");
        assert!(css.contains("#test .icon-shape,#test .image-shape{"), "missing icon/image-shape");
        assert!(css.contains("#test .label-icon{"), "missing .label-icon");
        assert!(css.contains("#test .node .label-icon path{"), "missing .node .label-icon path");
    }

    #[test]
    fn flowchart_css_labelbkg_uses_fade() {
        let th = theme::get_theme("default");
        let css = flowchart_specific_css("test", &th);
        // labelBkg should use the faded version of edgeLabelBackground.
        // For default theme, edgeLabelBackground is "rgba(232,232,232, 0.8)"
        // and fade("rgba(232,232,232, 0.8)", 0.5) should produce
        // "rgba(232, 232, 232, 0.5)" (with spaces after commas).
        assert!(
            css.contains("#test .labelBkg{background-color:rgba(232, 232, 232, 0.5);}"),
            "labelBkg should use faded color: got {}",
            css
        );
    }

    /// ID function matching the reference fixture naming convention.
    fn id_for_fixture(rel: &str) -> String {
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
        let d = fcp::parse(source).expect("parse");
        let theme = theme::get_theme("default");
        let l = fcl::layout(&d, &theme).expect("layout");
        super::render(&d, &l, &theme, id).expect("render")
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
        let id = id_for_fixture(rel);
        let got = match std::panic::catch_unwind(|| render_fixture(&source, &id)) {
            Ok(s) => s,
            Err(_) => return false,
        };
        got == expected
    }

    #[test]
    fn byte_exact_sweep() {
        // Walk every cypress + demos flowchart fixture. Fixture 46 is
        // known_ignored (no reference SVG).
        let cypress: Vec<String> = (1..=253u32)
            .filter(|n| *n != 46)
            .map(|n| format!("{:02}", n))
            .collect();
        let demos: Vec<String> = (1..=66u32).map(|n| format!("{:02}", n)).collect();

        let mut pass = 0usize;
        let mut passing: Vec<String> = Vec::new();
        let mut fail_names: Vec<String> = Vec::new();
        for n in &cypress {
            let rel = format!("ext_fixtures/cypress/flowchart/{}", n);
            if check_one(&rel) {
                pass += 1;
                passing.push(rel);
            } else {
                fail_names.push(rel);
            }
        }
        for n in &demos {
            let rel = format!("ext_fixtures/demos/flowchart/{}", n);
            if check_one(&rel) {
                pass += 1;
                passing.push(rel);
            } else {
                fail_names.push(rel);
            }
        }
        let total = cypress.len() + demos.len();
        eprintln!("Flowchart byte-exact: {}/{}", pass, total);
        if pass > 0 {
            eprintln!("Passing ({}): {:?}", passing.len(), passing);
        }
        if pass < total {
            eprintln!(
                "Failing ({}): {:?}",
                fail_names.len(),
                &fail_names[..fail_names.len().min(10)]
            );
        }
        // This test never fails — it reports progress.
    }

    /// Diagnostic: dump our SVG to /tmp for comparison
    #[test]
    #[ignore]
    fn dump_02_svg() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let rel = "ext_fixtures/cypress/flowchart/02";
        let source = std::fs::read_to_string(base.join(format!("tests/{}.mmd", rel))).unwrap();
        let d = fcp::parse(&source).unwrap();
        let theme = theme::get_theme("default");
        let l = fcl::layout(&d, &theme).unwrap();
        let id = id_for_fixture(rel);
        let got = super::render(&d, &l, &theme, &id).unwrap();
        std::fs::write("/tmp/rust_02.svg", &got).unwrap();
        eprintln!("Wrote {} bytes to /tmp/rust_02.svg", got.len());
    }

    /// Diagnostic: probe the first divergence point for a single fixture.
    #[test]
    #[ignore]
    fn diff_probe_02() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let rel = "ext_fixtures/cypress/flowchart/02";
        let source = std::fs::read_to_string(
            base.join(format!("tests/{}.mmd", rel)),
        ).unwrap();
        let expected = std::fs::read_to_string(
            base.join(format!("tests/reference/{}.svg", rel)),
        ).unwrap();
        let d = fcp::parse(&source).unwrap();
        let theme = theme::get_theme("default");
        let l = fcl::layout(&d, &theme).unwrap();
        let id = id_for_fixture(rel);
        let got = super::render(&d, &l, &theme, &id).unwrap();
        let a = got.as_bytes();
        let b = expected.as_bytes();
        let n = a.len().min(b.len());
        let mut i = 0;
        while i < n && a[i] == b[i] { i += 1; }
        if i >= n && a.len() == b.len() {
            eprintln!("BYTE EXACT!");
            return;
        }
        let ctx_lo = i.saturating_sub(80);
        let ctx_hi_a = (i + 200).min(a.len());
        let ctx_hi_b = (i + 200).min(b.len());
        eprintln!("Diverge at byte {} (got={}, want={})", i, a.len(), b.len());
        eprintln!("got [{}..]: {}", ctx_lo, String::from_utf8_lossy(&a[ctx_lo..ctx_hi_a]));
        eprintln!("want[{}..]: {}", ctx_lo, String::from_utf8_lossy(&b[ctx_lo..ctx_hi_b]));
    }
}
