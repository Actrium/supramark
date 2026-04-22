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
use crate::render::unified_shell;
use crate::theme::css as theme_css;
use crate::theme::ThemeVariables;

/// Render a flowchart diagram as SVG.
pub fn render(
    d: &FlowchartDiagram,
    l: &FlowchartLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::new();

    // Compute viewBox with padding.
    let padding = l.diagram_padding;
    let vb_x = l.bounds.x - padding;
    let vb_y = l.bounds.y - padding;
    let vb_w = (l.bounds.width + 2.0 * padding).max(1.0);
    let vb_h = (l.bounds.height + 2.0 * padding).max(1.0);

    // SVG root — canonical attribute order via the unified shell.
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
    out.push_str(&build_css(id, theme, d));
    out.push_str(&theme_css::neo_look_block(id, theme));
    out.push_str("</style>");

    // Seed <g> wrapping markers + root — matches upstream's
    // dagre-unified pipeline behaviour of appending directly into
    // the seed group produced by appendDivSvgG.
    out.push_str(unified_shell::open_seed_group());
    // Marker defs — emitted as-is (diagram-specific wrapper).
    out.push_str(&markers::defs(&l.aria_kind, id, theme));

    // Root container — `<g class="root">` with clusters, edgePaths,
    // edgeLabels, and nodes sub-groups.
    out.push_str(unified_shell::open_root_group());

    // Clusters (subgraphs).
    out.push_str(&unified_shell::open_layer("clusters"));
    for cluster in &l.clusters {
        if let Some(cnode) = l.nodes.iter().find(|n| n.id == cluster.id && n.is_group) {
            out.push_str(&render_cluster(cnode, cluster, theme));
        }
    }
    out.push_str(unified_shell::close_layer());

    // Edge paths.
    out.push_str(&unified_shell::open_layer("edgePaths"));
    for (i, e) in l.edges.iter().enumerate() {
        out.push_str(&render_edge_path(e, i, id, &l.aria_kind));
    }
    out.push_str(unified_shell::close_layer());

    // Edge labels.
    out.push_str(&unified_shell::open_layer("edgeLabels"));
    for e in l.edges.iter() {
        out.push_str(&render_edge_label(e));
    }
    out.push_str(unified_shell::close_layer());

    // Nodes.
    out.push_str(&unified_shell::open_layer("nodes"));
    for n in &l.nodes {
        if n.is_group {
            continue;
        }
        // Dispatch to the shape registry. Unknown shapes fall back to rect.
        let shape_id = n.shape.clone().unwrap_or_else(|| "rect".to_string());
        match shapes::draw(&shape_id, n, theme) {
            Ok(svg) => out.push_str(&svg),
            Err(_) => {
                // Fallback: plain rect.
                if let Ok(svg) = shapes::draw("rect", n, theme) {
                    out.push_str(&svg);
                }
            }
        }
    }
    out.push_str(unified_shell::close_layer());

    out.push_str(unified_shell::close_root_group());
    out.push_str(unified_shell::close_seed_group());
    out.push_str(&unified_shell::emit_defs_shell(id, true, true));
    out.push_str(unified_shell::close_unified_svg());
    Ok(out)
}

fn render_cluster(node: &UNode, _cluster: &Cluster, _theme: &ThemeVariables) -> String {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let x = node.x.unwrap_or(0.0);
    let y = node.y.unwrap_or(0.0);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
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

fn render_edge_path(e: &UEdge, index: usize, svg_id: &str, aria_kind: &str) -> String {
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
    let class_attr = format!(
        " edge-thickness-{thickness} edge-pattern-{pattern} edge-thickness-{thickness} edge-pattern-{pattern} flowchart-link"
    );

    let style = e.style.as_ref().map(|v| v.join(";")).unwrap_or_default();
    let edge_id = format!("{svg_id}-{id}", id = e.id.clone(),);
    let marker_end = match e.arrow_type_end.as_deref() {
        Some("arrow_point") | Some("arrow") | None => {
            format!(" marker-end=\"url(#{svg_id}_{aria_kind}-pointEnd)\"",)
        }
        Some("arrow_circle") => {
            format!(" marker-end=\"url(#{svg_id}_{aria_kind}-circleEnd)\"",)
        }
        Some("arrow_cross") => {
            format!(" marker-end=\"url(#{svg_id}_{aria_kind}-crossEnd)\"",)
        }
        _ => String::new(),
    };
    let marker_start = match e.arrow_type_start.as_deref() {
        Some("arrow_point") | Some("arrow") => {
            format!(" marker-start=\"url(#{svg_id}_{aria_kind}-pointStart)\"",)
        }
        Some("arrow_circle") => {
            format!(" marker-start=\"url(#{svg_id}_{aria_kind}-circleStart)\"",)
        }
        Some("arrow_cross") => {
            format!(" marker-start=\"url(#{svg_id}_{aria_kind}-crossStart)\"",)
        }
        _ => String::new(),
    };
    let _ = index;
    format!(
        r#"<path d="{d}" id="{eid}" class="{cls}" style="{st};" data-look="classic"{ms}{me}></path>"#,
        d = d_attr,
        eid = edge_id,
        cls = class_attr.trim(),
        st = style,
        ms = marker_start,
        me = marker_end,
    )
}

fn render_edge_label(e: &UEdge) -> String {
    use crate::render::foreign_object::{
        measure_html_label, render_edge_label as fo_edge, HtmlLabelFont, LabelOpts,
    };
    let label_text = e.label.clone().unwrap_or_default();
    let esc = xml_escape(&label_text);
    let (w, h) = if esc.is_empty() {
        (0.0, 0.0)
    } else {
        measure_html_label(&esc, &HtmlLabelFont::default(), 200.0, true)
    };
    let lx = e.label_x.unwrap_or(0.0);
    let ly = e.label_y.unwrap_or(0.0);
    let opts = LabelOpts {
        data_id: Some(&e.id),
        group_style: None,
        wrap_in_p: !esc.is_empty(),
        ..LabelOpts::default()
    };
    fo_edge(&esc, lx, ly, w, h, opts)
}

/// Build the CSS `<style>` block — a minimal subset of upstream's
/// `styles.ts` → `flowchart` output, scoped to `#<id>`. Real upstream
/// CSS is ~3KB of class rules; we emit a compact subset focused on the
/// structural classes our renderer outputs.
fn build_css(id: &str, theme: &ThemeVariables, _d: &FlowchartDiagram) -> String {
    let primary = theme.primary_color.as_deref().unwrap_or("#ECECFF");
    let primary_border = theme.primary_border_color.as_deref().unwrap_or("#9370DB");
    let line = theme
        .line_color
        .as_deref()
        .or(theme.arrowhead_color.as_deref())
        .unwrap_or("#333333");
    let font_family = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\",verdana,arial,sans-serif");
    let font_size = theme.font_size.as_deref().unwrap_or("16px");
    let txt_color = theme
        .node_text_color
        .as_deref()
        .or(theme.text_color.as_deref())
        .unwrap_or("#333");

    format!(
        "#{id}{{font-family:{ff};font-size:{fs};fill:{fill};}}\
#{id} .marker{{fill:{line};stroke:{line};}}\
#{id} .marker.cross{{stroke:{line};}}\
#{id} .label{{font-family:{ff};color:{txt};}}\
#{id} .node rect,#{id} .node circle,#{id} .node ellipse,#{id} .node polygon,#{id} .node path{{fill:{primary};stroke:{primary_border};stroke-width:1px;}}\
#{id} .flowchart-link{{stroke:{line};fill:none;}}\
#{id} .edgeLabel{{background-color:rgba(232,232,232,0.8);text-align:center;}}\
#{id} .edgeLabel p{{background-color:rgba(232,232,232,0.8);}}\
#{id} .edgeLabel rect{{opacity:0.5;}}\
#{id} .cluster rect{{fill:#ffffde;stroke:#aaaa33;stroke-width:1px;}}\
#{id} .edge-thickness-normal{{stroke-width:1px;}}\
#{id} .edge-thickness-thick{{stroke-width:3.5px;}}\
#{id} .edge-pattern-solid{{stroke-dasharray:0;}}\
#{id} .edge-pattern-dashed{{stroke-dasharray:3;}}\
#{id} .edge-pattern-dotted{{stroke-dasharray:2;}}",
        id = id,
        ff = font_family,
        fs = font_size,
        fill = txt_color,
        line = line,
        primary = primary,
        primary_border = primary_border,
        txt = txt_color,
    )
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
}
