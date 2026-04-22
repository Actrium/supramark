//! Requirement-diagram SVG renderer.
//!
//! Upstream reference:
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/requirement/requirementRenderer.ts`
//! (which in turn delegates to the unified dagre renderer at
//! `rendering-util/render.ts` + the `requirementBox` shape).
//!
//! Byte-exact parity with upstream is out of scope for Wave 4's
//! foundational pass — replicating mermaid's HTML `<foreignObject>`
//! label stack, drop-shadow filter defs, d3-interpolated rough-basis
//! rectangle paths and markdown `<p>`-wrapped spans faithfully is a
//! multi-wave effort. This renderer emits a *structural* SVG that
//! contains:
//!
//! 1. The `<svg>` shell + mermaid stylesheet (scoped under `#id`).
//! 2. Edge-marker defs (contains-circle-cross start + arrow end).
//! 3. One `<g class="node">` per requirement / element with nested
//!    `<rect>` + divider + label rows (text-only, no foreignObject).
//! 4. One `<g class="edgePath">` + `<g class="edgeLabel">` per
//!    relationship.
//!
//! That's enough for consumers to see the diagram content. The
//! byte-exact effort is tracked in
//! `PROGRESS.zh.md` under "requirement pending".

use crate::error::Result;
use crate::layout::requirement::{EdgeLabel, NodeLabels, RequirementLayout};
use crate::layout::unified::{Edge as UEdge, Node as UNode};
use crate::model::requirement::RequirementDiagram;
use crate::theme::ThemeVariables;

pub fn render(
    _d: &RequirementDiagram,
    l: &RequirementLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let b = &l.graph.bounds;
    // Pad the viewBox so boxes aren't flush with the edge.
    let pad = 8.0;
    let vb_x = b.x - pad;
    let vb_y = b.y - pad;
    let vb_w = b.width + pad * 2.0;
    let vb_h = b.height + pad * 2.0;
    let max_w = vb_w.max(1.0);

    let mut out = String::new();
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" class="requirementDiagram" style="max-width: {mw}px;" viewBox="{vx} {vy} {vw} {vh}" role="graphics-document document" aria-roledescription="requirement">"#,
        id = id,
        mw = fmt_num(max_w),
        vx = fmt_num(vb_x),
        vy = fmt_num(vb_y),
        vw = fmt_num(vb_w),
        vh = fmt_num(vb_h),
    ));
    out.push_str("<style>");
    out.push_str(&stylesheet(id, theme));
    out.push_str("</style>");
    out.push_str("<g></g>");
    // Markers.
    out.push_str(&marker_defs(id));
    // Root group.
    out.push_str(r#"<g class="root">"#);
    // Edges first — upstream draws them under nodes.
    out.push_str(r#"<g class="edgePaths">"#);
    for (e, _el) in l.graph.edges.iter().zip(l.edge_labels.iter()) {
        out.push_str(&render_edge(id, e));
    }
    out.push_str("</g>");
    // Edge labels.
    out.push_str(r#"<g class="edgeLabels">"#);
    for (e, el) in l.graph.edges.iter().zip(l.edge_labels.iter()) {
        out.push_str(&render_edge_label(e, el));
    }
    out.push_str("</g>");
    // Nodes.
    out.push_str(r#"<g class="nodes">"#);
    for (n, labels) in l.graph.nodes.iter().zip(l.node_labels.iter()) {
        out.push_str(&render_node(id, n, labels));
    }
    out.push_str("</g>");
    out.push_str("</g>");
    out.push_str("</svg>");
    Ok(out)
}

fn render_node(id_prefix: &str, n: &UNode, labels: &NodeLabels) -> String {
    let x = n.x.unwrap_or(0.0);
    let y = n.y.unwrap_or(0.0);
    let w = n.width.unwrap_or(0.0);
    let h = n.height.unwrap_or(0.0);
    let hx = -w / 2.0;
    let hy = -h / 2.0;
    let header_h = 20.0 + 16.0 * 2.0; // pad + 2 label rows
    let divider_y = hy + header_h;
    let mut s = String::new();
    let classes = format!("node default {}", n.css_classes.as_deref().unwrap_or(""));
    s.push_str(&format!(
        r#"<g class="{cls}" id="{dom}-{nid}" transform="translate({tx}, {ty})">"#,
        cls = classes.trim(),
        dom = id_prefix,
        nid = xml_escape(&n.id),
        tx = fmt_num(x),
        ty = fmt_num(y),
    ));
    s.push_str(&format!(
        r#"<rect class="reqBox" x="{x}" y="{y}" width="{w}" height="{h}"/>"#,
        x = fmt_num(hx),
        y = fmt_num(hy),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    // Divider line between header and body.
    s.push_str(&format!(
        r#"<line class="req-title-line" x1="{x1}" x2="{x2}" y1="{y}" y2="{y}"/>"#,
        x1 = fmt_num(hx),
        x2 = fmt_num(-hx),
        y = fmt_num(divider_y),
    ));
    // Header rows (kind, name) — upstream uses foreignObject HTML
    // labels (`markdown-node-label` class) centred in the header band.
    use crate::render::foreign_object::{
        foreign_object_body, measure_html_label, HtmlLabelFont, LabelOpts,
    };
    let line_h = 24.0;
    let mut ty = hy + 20.0 + 12.0;
    // Kind header — "<<Requirement>>" etc.
    {
        let esc = xml_escape(&labels.kind_header);
        let (fw, fh) = measure_html_label(&esc, &HtmlLabelFont::default(), 280.0, true);
        let opts = LabelOpts {
            extra_span_classes: "markdown-node-label",
            max_width: 280.0,
            ..LabelOpts::default()
        };
        s.push_str(&format!(
            r#"<g class="label" style="" transform="translate({tx}, {ty2})">"#,
            tx = fmt_num(-fw / 2.0),
            ty2 = fmt_num(ty - fh / 2.0),
        ));
        s.push_str(&foreign_object_body(&esc, fw, fh, &opts));
        s.push_str("</g>");
    }
    ty += line_h;
    // Name — bold.
    {
        let esc = xml_escape(&labels.name);
        let (fw, fh) = measure_html_label(&esc, &HtmlLabelFont::default(), 280.0, true);
        let opts = LabelOpts {
            extra_span_classes: "markdown-node-label",
            max_width: 280.0,
            label_style: Some(""),
            ..LabelOpts::default()
        };
        s.push_str(&format!(
            r#"<g class="label" style="; font-weight: bold;" transform="translate({tx}, {ty2})">"#,
            tx = fmt_num(-fw / 2.0),
            ty2 = fmt_num(ty - fh / 2.0),
        ));
        s.push_str(&foreign_object_body(&esc, fw, fh, &opts));
        s.push_str("</g>");
    }
    // Body rows — left-aligned, each wrapped in foreignObject too.
    let body_x = hx + 10.0;
    let mut by = divider_y + 20.0;
    for row in &labels.body {
        let esc = xml_escape(row);
        let (fw, fh) = measure_html_label(&esc, &HtmlLabelFont::default(), 280.0, true);
        let opts = LabelOpts {
            extra_span_classes: "markdown-node-label",
            max_width: 280.0,
            ..LabelOpts::default()
        };
        s.push_str(&format!(
            r#"<g class="label" style="" transform="translate({tx}, {ty2})">"#,
            tx = fmt_num(body_x),
            ty2 = fmt_num(by - fh / 2.0),
        ));
        s.push_str(&foreign_object_body(&esc, fw, fh, &opts));
        s.push_str("</g>");
        by += line_h;
    }
    s.push_str("</g>");
    s
}

fn render_edge(id_prefix: &str, e: &UEdge) -> String {
    let points = match e.points.as_ref() {
        Some(p) if p.len() >= 2 => p,
        _ => return String::new(),
    };
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        d.push_str(if i == 0 { "M" } else { "L" });
        d.push_str(&fmt_num(p.x));
        d.push(',');
        d.push_str(&fmt_num(p.y));
    }
    let pattern_cls = match e.pattern.as_deref() {
        Some("dashed") => " edge-pattern-dashed",
        _ => "",
    };
    let classes = format!(
        "edge-thickness-normal{} {}",
        pattern_cls,
        e.classes.as_deref().unwrap_or("")
    );
    let style = e.style.as_ref().map(|v| v.join(";")).unwrap_or_default();
    let mut attrs = String::new();
    if e.arrow_type_start
        .as_deref()
        .map_or(false, |s| !s.is_empty())
    {
        attrs.push_str(&format!(
            r#" marker-start="url(#{id}_requirement-requirement_containsStart)""#,
            id = id_prefix,
        ));
    }
    if e.arrow_type_end.as_deref().map_or(false, |s| !s.is_empty()) {
        attrs.push_str(&format!(
            r#" marker-end="url(#{id}_requirement-requirement_arrowEnd)""#,
            id = id_prefix,
        ));
    }
    format!(
        r#"<path d="{d}" id="{id}-{eid}" class="{cls}" style="{st}"{attrs}/>"#,
        d = d,
        id = id_prefix,
        eid = xml_escape(&e.id),
        cls = classes.trim(),
        st = style,
        attrs = attrs,
    )
}

fn render_edge_label(e: &UEdge, el: &EdgeLabel) -> String {
    // Requirement edge labels wrap text like "<<satisfies>>" in a
    // foreignObject span with labelBkg class — matching upstream's
    // `insertEdgeLabel` + `addHtmlSpan(addBackground=true)`. The `<<`
    // and `>>` angle brackets are HTML-escaped to `&lt;&lt;…&gt;&gt;`.
    use crate::render::foreign_object::{render_edge_label as fo_edge, LabelOpts};
    let x = e.label_x.unwrap_or(0.0);
    let y = e.label_y.unwrap_or(0.0);
    let esc = xml_escape(&el.text);
    let data_id = format!(
        "{}-{}-0",
        xml_escape(e.source.as_deref().unwrap_or("")),
        xml_escape(e.target.as_deref().unwrap_or("")),
    );
    let opts = LabelOpts {
        data_id: Some(&data_id),
        group_style: None,
        ..LabelOpts::default()
    };
    fo_edge(&esc, x, y, el.width, el.height, opts)
}

fn marker_defs(id_prefix: &str) -> String {
    // Two markers — circle-cross (contains start) + arrow (end).
    format!(
        r#"<defs><marker id="{id}_requirement-requirement_containsStart" refX="0" refY="10" markerWidth="20" markerHeight="20" orient="auto"><g><circle cx="10" cy="10" r="9" fill="none"></circle><line x1="1" x2="19" y1="10" y2="10"></line><line y1="1" y2="19" x1="10" x2="10"></line></g></marker></defs><defs><marker id="{id}_requirement-requirement_arrowEnd" refX="20" refY="10" markerWidth="20" markerHeight="20" orient="auto"><path d="M0,0 L20,10 M20,10 L0,20"></path></marker></defs>"#,
        id = id_prefix,
    )
}

fn stylesheet(id: &str, theme: &ThemeVariables) -> String {
    let rel_color = theme
        .relation_color
        .clone()
        .unwrap_or_else(|| "#333333".into());
    let req_bg = theme
        .requirement_background
        .clone()
        .unwrap_or_else(|| "#ECECFF".into());
    let req_border = theme
        .requirement_border_color
        .clone()
        .unwrap_or_else(|| "hsl(240, 60%, 86.2745098039%)".into());
    let req_border_size = theme
        .requirement_border_size
        .clone()
        .unwrap_or_else(|| "1".into());
    let req_text = theme
        .requirement_text_color
        .clone()
        .unwrap_or_else(|| "#131300".into());
    let rel_label_bg = theme
        .relation_label_background
        .clone()
        .unwrap_or_else(|| "rgba(232,232,232, 0.8)".into());
    let rel_label_color = theme
        .relation_label_color
        .clone()
        .unwrap_or_else(|| "black".into());
    let font_family = theme
        .font_family
        .clone()
        .unwrap_or_else(|| "\"trebuchet ms\",verdana,arial,sans-serif".into());
    let font_size = theme.font_size.clone().unwrap_or_else(|| "16px".into());
    let text_color = theme.text_color.clone().unwrap_or_else(|| "#333".into());
    let edge_label_bg = theme
        .edge_label_background
        .clone()
        .unwrap_or_else(|| "rgba(232,232,232, 0.8)".into());
    let node_border = theme
        .node_border
        .clone()
        .unwrap_or_else(|| "#9370DB".into());

    format!(
        concat!(
            "#{id}{{font-family:{ff};font-size:{fs};fill:{tc};}}",
            "#{id} svg{{font-family:{ff};font-size:{fs};}}",
            "#{id} .reqBox{{fill:{rb};fill-opacity:1.0;stroke:{br};stroke-width:{bs};}}",
            "#{id} .reqTitle,#{id} .reqLabel{{fill:{rt};}}",
            "#{id} .reqLabelBox{{fill:{rlb};fill-opacity:1.0;}}",
            "#{id} .req-title-line{{stroke:{br};stroke-width:{bs};}}",
            "#{id} .relationshipLine{{stroke:{rc};stroke-width:1px;}}",
            "#{id} .relationshipLabel{{fill:{rlc};}}",
            "#{id} .edgeLabel{{background-color:{elb};}}",
            "#{id} .edgeLabel .label rect{{fill:{elb};}}",
            "#{id} .edgeLabel .label text{{fill:{rlc};}}",
            "#{id} .divider{{stroke:{nb};stroke-width:1;}}",
            "#{id} .labelBkg{{background-color:{elb};}}",
        ),
        id = id,
        ff = font_family,
        fs = font_size,
        tc = text_color,
        rb = req_bg,
        br = req_border,
        bs = req_border_size,
        rt = req_text,
        rlb = rel_label_bg,
        rc = rel_color,
        rlc = rel_label_color,
        elb = edge_label_bg,
        nb = node_border,
    )
}

fn fmt_num(v: f64) -> String {
    // d3-friendly short form — strip trailing zeros while preserving
    // enough precision to round-trip in tests.
    if v == 0.0 {
        return "0".into();
    }
    let s = format!("{}", v);
    s
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::requirement::layout;
    use crate::parser::requirement::parse;
    use crate::theme::get_theme;

    #[test]
    fn renders_minimal_diagram() {
        let src = "requirementDiagram\n\
                   requirement r { id: 1\n text: hi\n risk: low\n verifymethod: test\n }\n\
                   element e { type: thing\n }\n\
                   e - satisfies -> r\n";
        let d = parse(src).expect("parse");
        let theme = get_theme("default");
        let l = layout(&d, &theme).expect("layout");
        let svg = render(&d, &l, &theme, "req-test").expect("render");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains(r#"class="requirementDiagram""#));
        assert!(svg.contains("reqBox"));
        assert!(svg.contains("&lt;&lt;Requirement&gt;&gt;"));
        assert!(svg.contains("&lt;&lt;Element&gt;&gt;"));
        assert!(svg.contains("&lt;&lt;satisfies&gt;&gt;"));
        assert!(svg.contains("marker-end"));
        // No byte-exact match expected — this is a structural stub.
    }

    /// Byte-exact sweep across the 44 fixtures. We don't expect any
    /// to match today — the assertion here just records progress for
    /// future waves.
    #[test]
    fn byte_exact_sweep_reports_zero_matches() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let dirs = [
            "tests/ext_fixtures/cypress/requirement",
            "tests/ext_fixtures/demos/requirement",
        ];
        let mut total = 0usize;
        let mut matched = 0usize;
        for dir in dirs {
            let full = base.join(dir);
            let Ok(entries) = std::fs::read_dir(&full) else {
                continue;
            };
            for entry in entries {
                let Ok(entry) = entry else { continue };
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("mmd") {
                    continue;
                }
                let stem = path.file_stem().unwrap().to_str().unwrap();
                let ref_path = base.join(format!(
                    "tests/reference/{}/{}.svg",
                    dir.trim_start_matches("tests/"),
                    stem
                ));
                let Ok(src) = std::fs::read_to_string(&path) else {
                    continue;
                };
                total += 1;
                let mut id = String::from("ref-");
                let rel = format!("{}/{}", dir.trim_start_matches("tests/"), stem);
                let mut last_was_sep = false;
                for c in rel.chars() {
                    if c.is_ascii_alphanumeric() {
                        id.push(c);
                        last_was_sep = false;
                    } else if !last_was_sep {
                        id.push('-');
                        last_was_sep = true;
                    }
                }
                if id.ends_with('-') {
                    id.pop();
                }
                let Ok(d) = parse(&src) else {
                    continue;
                };
                let theme = get_theme("default");
                let Ok(l) = layout(&d, &theme) else {
                    continue;
                };
                let Ok(got) = render(&d, &l, &theme, &id) else {
                    continue;
                };
                if let Ok(expected) = std::fs::read_to_string(&ref_path) {
                    if got == expected {
                        matched += 1;
                    }
                }
            }
        }
        eprintln!(
            "requirement sweep: {}/{} byte-exact (structural port, not targeting byte-match this wave)",
            matched, total
        );
        // Byte-exact parity tracked as Wave 5 work; assert total>0 so
        // regressions in fixture discovery are caught.
        assert!(total > 0, "no fixtures discovered — check test data paths");
    }

    /// Structural parity: after wiring foreignObject, requirement node
    /// + edge labels should use the HTML label stack rather than bare
    /// `<text>` elements — matching upstream's
    /// `<foreignObject><div>...<span class="nodeLabel markdown-node-label">`.
    #[test]
    fn node_and_edge_labels_use_foreign_object() {
        let src = std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/ext_fixtures/demos/requirement/01.mmd"),
        )
        .expect("read fixture");
        let d = parse(&src).expect("parse");
        let theme = get_theme("default");
        let l = layout(&d, &theme).expect("layout");
        let svg = render(&d, &l, &theme, "structural-test").expect("render");
        // Node labels — kind header and name:
        assert!(
            svg.contains(r#"<span class="nodeLabel markdown-node-label">"#),
            "node labels should use markdown-node-label span"
        );
        assert!(
            svg.contains(r#"<foreignObject width="#),
            "node labels should be wrapped in foreignObject"
        );
        // Edge labels — "<<satisfies>>" etc.
        assert!(
            svg.contains(r#"<span class="edgeLabel ">"#),
            "edge labels should use edgeLabel span"
        );
        assert!(
            svg.contains(r#"class="labelBkg""#),
            "edge foreignObject div should carry labelBkg class"
        );
    }
}
