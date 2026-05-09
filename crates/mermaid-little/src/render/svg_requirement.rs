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
use crate::render::foreign_object::{measure_html_label, HtmlLabelFont};
use crate::render::unified_shell;
use crate::theme::css as theme_css;
use crate::theme::ThemeVariables;

/// Compute SVG viewBox bounds mirroring JSDOM's getBBox shim used by the
/// reference generator.  JSDOM ignores `transform` attributes and unions
/// every element's LOCAL coordinates.  For requirement diagrams that means:
///
/// * Per-node rect at local `(-totalW/2, -totalH/2, totalW, totalH)`.
/// * Per-node foreignObject labels at `(0, 0, fo_w, fo_h)` each.
/// * Edge path points using absolute waypoint coords (no transform on paths).
/// * Edge-label foreignObjects at `(0, 0, el_w, el_h)`.
fn compute_jsdom_bounds(l: &RequirementLayout, title: Option<&str>) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    let mut acc = |x: f64, y: f64, w: f64, h: f64| {
        if min_x > x {
            min_x = x;
        }
        if min_y > y {
            min_y = y;
        }
        if max_x < x + w {
            max_x = x + w;
        }
        if max_y < y + h {
            max_y = y + h;
        }
    };

    for (n, labels) in l.graph.nodes.iter().zip(l.node_labels.iter()) {
        // Kind/body rows: bold when labels.is_bold (label_styles_str has font-weight).
        let kind_font = HtmlLabelFont {
            bold: Some(labels.is_bold),
            ..HtmlLabelFont::default()
        };
        // Name: bold when labels.name_is_bold (label_styles_str is non-empty,
        // so the g style "label_styles; font-weight: bold." is parseable by JSDOM).
        let name_font = HtmlLabelFont {
            bold: Some(labels.name_is_bold),
            ..HtmlLabelFont::default()
        };
        let (kind_fo_w, kind_fo_h) =
            measure_html_label(&labels.kind_header, &kind_font, f64::INFINITY, false);
        // Use layout-computed dimensions (max FO width + padding across all rows).
        let total_w = n.width.unwrap_or(kind_fo_w + 20.0);
        let total_h = n.height.unwrap_or(kind_fo_h + 20.0);

        // Rect at local (-totalW/2, -totalH/2, totalW, totalH)
        acc(-total_w / 2.0, -total_h / 2.0, total_w, total_h);

        // Kind FO at (0, 0, kind_fo_w, kind_fo_h)
        acc(0.0, 0.0, kind_fo_w, kind_fo_h);

        // Name FO at (0, 0, name_fo_w, name_fo_h)
        // JSDOM measures textContent (markdown stripped) of the div.
        let name_plain = crate::text::markdown_text_content(&labels.name);
        let (name_fo_w, name_fo_h) =
            measure_html_label(&name_plain, &name_font, f64::INFINITY, false);
        acc(0.0, 0.0, name_fo_w, name_fo_h);

        // Body row FOs — JSDOM measures textContent (markdown stripped), bold = is_bold.
        let body_font = HtmlLabelFont {
            bold: Some(labels.is_bold),
            ..HtmlLabelFont::default()
        };
        for row in &labels.body {
            let row_plain = crate::text::markdown_text_content(row);
            let (row_fo_w, row_fo_h) =
                measure_html_label(&row_plain, &body_font, f64::INFINITY, false);
            acc(0.0, 0.0, row_fo_w, row_fo_h);
        }

        // Divider line (rough.js `rc.line(hx, lineY, -hx, lineY)`) — only
        // emitted when there are body rows. JSDOM pathBBox includes the
        // path endpoints, which extend below the rect when body rows are present.
        // lineY = hy + kind_fo_h + name_fo_h + gap = -total_h/2 + kind_fo_h + name_fo_h + 20
        if !labels.body.is_empty() {
            let hy = -total_h / 2.0;
            let line_y = hy + kind_fo_h + name_fo_h + 20.0; // gap = 20
            let hx = -total_w / 2.0;
            // Path spans hx...-hx at line_y (rough.js double line)
            acc(hx, line_y, total_w, 0.0);
        }
    }

    for (e, el) in l.graph.edges.iter().zip(l.edge_labels.iter()) {
        // Edge path points are absolute (no transform on path elements).
        // The reference `pathBBox` is computed by parsing the emitted SVG
        // `d` attribute, which uses 3-decimal rounding (d3-path's
        // `.appendRound(3)`). Mirror that rounding here so bounds match.
        let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
        if let Some(pts) = e.points.as_ref() {
            for p in pts {
                acc(r3(p.x), r3(p.y), 0.0, 0.0);
            }
        }
        // Edge label FO at (0, 0, el_w, el_h)
        acc(0.0, 0.0, el.width, el.height);
    }

    // Title text — JSDOM measures it as {x:0, y:0, width:tw, height:lh},
    // ignoring the `x` attribute. The title is added to the SVG AFTER the
    // main content but BEFORE `setupViewPortForSVG`, so it widens the bbox.
    if let Some(t) = title {
        if !t.is_empty() {
            use crate::font_metrics::text_width;
            // Title uses default 14px (JSDOM resolveFont default — no explicit style on <text>).
            let tw = text_width(t, "sans-serif", 14.0, false, false);
            let lh = crate::font_metrics::line_height("sans-serif", 14.0, false, false);
            acc(0.0, 0.0, tw, lh);
        }
    }

    // Fallback if no elements
    if min_x.is_infinite() {
        return (0.0, 0.0, 100.0, 100.0);
    }

    let bounds_x = min_x;
    let bounds_y = min_y;
    let bounds_w = max_x - min_x;
    let bounds_h = max_y - min_y;
    (bounds_x, bounds_y, bounds_w, bounds_h)
}

pub fn render(
    d: &RequirementDiagram,
    l: &RequirementLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let title = d.meta.title.as_deref().unwrap_or("");
    // Compute diagram bounds WITHOUT title for the title element's x position.
    // Upstream's insertTitle appends the <text> BEFORE setupViewPortForSVG, using
    // the pre-title center as the `x` attribute.
    let (pre_bx, _pre_by, pre_bw, _pre_bh) = compute_jsdom_bounds(l, None);
    let title_x = pre_bx + pre_bw / 2.0; // center of diagram (before title)
                                         // Compute viewBox using JSDOM-aware bounds (ignores transforms, unions local coords).
    let (bx, by, bw, bh) =
        compute_jsdom_bounds(l, if title.is_empty() { None } else { Some(title) });
    let pad = 8.0;
    let vb_x = bx - pad;
    let vb_y = by - pad;
    let vb_w = bw + pad * 2.0;
    let vb_h = bh + pad * 2.0;
    let max_w = vb_w.max(1.0);

    let mut out = String::new();
    // Build SVG opening tag. When accTitle/accDescr are present, append
    // aria-describedby and aria-labelledby attributes (upstream's
    // addSVGAccessibilityFields), mirroring the order: desc before title.
    let svg_open = unified_shell::open_unified_svg(
        id,
        max_w,
        (vb_x, vb_y, vb_w, vb_h),
        Some("requirementDiagram"),
        "requirement",
    );
    let a11y_title = d.meta.acc_title.as_deref().unwrap_or("");
    let a11y_descr = d.meta.acc_descr.as_deref().unwrap_or("");
    let has_a11y = !a11y_title.is_empty() || !a11y_descr.is_empty();
    if has_a11y {
        // svg_open ends with `>` — strip it, add aria attrs, re-add `>`.
        let base = svg_open.trim_end_matches('>');
        out.push_str(base);
        if !a11y_descr.is_empty() {
            out.push_str(&format!(r#" aria-describedby="chart-desc-{id}""#));
        }
        if !a11y_title.is_empty() {
            out.push_str(&format!(r#" aria-labelledby="chart-title-{id}""#));
        }
        out.push('>');
        // Emit <title> and <desc> as first children (upstream order: title then desc).
        if !a11y_title.is_empty() {
            out.push_str(&format!(
                r#"<title id="chart-title-{id}">{t}</title>"#,
                t = xml_escape(a11y_title),
            ));
        }
        if !a11y_descr.is_empty() {
            out.push_str(&format!(
                r#"<desc id="chart-desc-{id}">{t}</desc>"#,
                t = xml_escape(a11y_descr),
            ));
        }
    } else {
        out.push_str(&svg_open);
    }
    out.push_str("<style>");
    out.push_str(&theme_css::base_preamble(id, theme));
    out.push_str(&requirement_specific_css(id, theme));
    out.push_str(&theme_css::neo_look_block(id, theme));
    out.push_str("</style>");
    out.push_str(unified_shell::open_seed_group());
    // Markers.
    out.push_str(&marker_defs(id));
    // Root group.
    out.push_str(unified_shell::open_root_group());
    // Upstream emits an empty clusters layer even when no clusters
    // are present — matches the `<g class="root"><g class="clusters"></g>`
    // anchor at the start of every Stratum 3 diagram's reference SVG.
    out.push_str(&unified_shell::open_layer("clusters"));
    out.push_str(unified_shell::close_layer());
    // Edges first — upstream draws them under nodes.
    out.push_str(&unified_shell::open_layer("edgePaths"));
    for (e, _el) in l.graph.edges.iter().zip(l.edge_labels.iter()) {
        out.push_str(&render_edge(id, e));
    }
    out.push_str(unified_shell::close_layer());
    // Edge labels.
    out.push_str(&unified_shell::open_layer("edgeLabels"));
    for (e, el) in l.graph.edges.iter().zip(l.edge_labels.iter()) {
        out.push_str(&render_edge_label(e, el));
    }
    out.push_str(unified_shell::close_layer());
    // Nodes.
    out.push_str(&unified_shell::open_layer("nodes"));
    for (i, (n, labels)) in l.graph.nodes.iter().zip(l.node_labels.iter()).enumerate() {
        out.push_str(&render_node(id, n, labels, i, theme));
    }
    out.push_str(unified_shell::close_layer());
    out.push_str(unified_shell::close_root_group());
    out.push_str(unified_shell::close_seed_group());
    out.push_str(&unified_shell::emit_defs_shell(id, true, true));
    // Optional theme gradient defs — see `emit_gradient_defs`.
    out.push_str(&unified_shell::emit_gradient_defs(id, theme));
    // Diagram title — emitted AFTER the main content (matching upstream's insertTitle
    // which appends the <text> AFTER render() but BEFORE setupViewPortForSVG).
    // When present, the title contributes to the viewBox width (if it's wider than the
    // diagram content). The title uses the diagram center as its `x` anchor.
    if !title.is_empty() {
        let title_top_margin = 25.0_f64;
        out.push_str(&format!(
            r#"<text text-anchor="middle" x="{tx}" y="-{ty}" class="requirementDiagramTitleText">{t}</text>"#,
            tx = fmt_num(title_x),
            ty = fmt_num(title_top_margin),
            t = xml_escape(title),
        ));
    }
    out.push_str(unified_shell::close_unified_svg());
    Ok(out)
}

fn render_node(
    id_prefix: &str,
    n: &UNode,
    labels: &NodeLabels,
    node_index: usize,
    theme: &ThemeVariables,
) -> String {
    use crate::font_metrics::text_width;
    use crate::render::foreign_object::{
        foreign_object_body, measure_html_label, HtmlLabelFont, LabelOpts,
    };

    // Compute node/label styles from css_styles (classDef + inline styles).
    // Upstream `styles2String` splits into nodeStyles (stroke/fill/etc) and
    // labelStyles (color/font-weight/etc), each with `!important` suffix.
    let (node_styles_str, label_styles_str) = split_node_label_styles(&labels.css_styles);

    // Override node border stroke and fill when css_styles specify them.
    let default_node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let default_req_bg = theme.requirement_background.as_deref().unwrap_or("#ECECFF");
    let override_stroke = labels.css_styles.iter().find_map(|s| {
        let key = s.split(':').next().unwrap_or("").trim();
        if key == "stroke" {
            s.splitn(2, ':').nth(1).map(|v| v.trim().to_string())
        } else {
            None
        }
    });
    let override_fill = labels.css_styles.iter().find_map(|s| {
        let key = s.split(':').next().unwrap_or("").trim();
        if key == "fill" {
            s.splitn(2, ':').nth(1).map(|v| v.trim().to_string())
        } else {
            None
        }
    });
    let effective_border = override_stroke.as_deref().unwrap_or(default_node_border);
    let effective_fill = override_fill.as_deref().unwrap_or(default_req_bg);

    // Kind/body: bold from is_bold (font-weight in label_styles_str).
    // Name: bold from name_is_bold (label_styles_str non-empty → JSDOM can parse g style).
    let kind_font = HtmlLabelFont {
        bold: Some(labels.is_bold),
        ..HtmlLabelFont::default()
    };
    let name_font = HtmlLabelFont {
        bold: Some(labels.name_is_bold),
        ..HtmlLabelFont::default()
    };
    let body_font = HtmlLabelFont {
        bold: Some(labels.is_bold),
        ..HtmlLabelFont::default()
    };
    // Upstream getBBox() on shapeSvg unions all FO widths (JSDOM ignores transforms,
    // so each FO is at local origin). totalWidth = max(all FO widths) + padding.
    // totalHeight = kind_fo_h + padding (single line height).
    // We use the pre-computed n.width/n.height from the layout pass which already
    // accounts for the max FO width via box_size().
    let padding = 20.0;
    let gap = 20.0; // space between name bottom and divider
    let (kind_fo_w, kind_fo_h) =
        measure_html_label(&labels.kind_header, &kind_font, f64::INFINITY, false);
    // Use layout-computed dimensions (= max_fo_w + padding, kind_fo_h + padding)
    let total_w = n.width.unwrap_or(kind_fo_w + padding);
    let total_h = n.height.unwrap_or(kind_fo_h + padding);
    let hx = -total_w / 2.0;
    let hy = -total_h / 2.0;

    let x = n.x.unwrap_or(0.0);
    let y = n.y.unwrap_or(0.0);

    let mut s = String::new();

    // data-color-id from borderColorArray (upstream requirementBox.ts line 123-125).
    let data_color_id_attr = if let Some(ref bca) = theme.border_color_array {
        if !bca.is_empty() {
            let color_idx = node_index % bca.len();
            format!(r#" data-color-id="color-{}""#, color_idx)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // Upstream emits `class="node default {extra} "` where extra_classes is the
    // node's CSS classes beyond "default" (e.g., "blue", "bold").
    // Most nodes use `class="node default "` (single trailing space when no extras).
    let css_raw = n.css_classes.as_deref().unwrap_or("");
    let extra_classes: String = css_raw
        .split_whitespace()
        .filter(|c| *c != "default")
        .flat_map(|c| [c, " "])
        .collect();
    s.push_str(&format!(
        r#"<g class="node default {extra_classes}" id="{dom}-{nid}" data-look="classic"{dcid} transform="translate({tx}, {ty})">"#,
        dom = id_prefix,
        nid = xml_escape(&n.id),
        dcid = data_color_id_attr,
        tx = fmt_num(x),
        ty = fmt_num(y),
    ));

    // Inner container — upstream's `rc.rectangle(x, y, totalWidth, totalHeight, options)`
    // with roughness=0, fillStyle='solid', seed=1, stroke=effective_border, strokeWidth=1.3.
    // Produces two paths: fill rect and rough (deterministic bezier) border.
    s.push_str(&format!(
        r#"<g class="basic label-container outer-path" style="{nst}">"#,
        nst = node_styles_str,
    ));
    // Path 1: solid fill rectangle.
    s.push_str(&format!(
        r#"<path d="M{x0} {y0} L{x1} {y0} L{x1} {y1} L{x0} {y1}" stroke="none" stroke-width="0" fill="{fill}"></path>"#,
        x0 = fmt_num(hx),
        x1 = fmt_num(-hx),
        y0 = fmt_num(hy),
        y1 = fmt_num(-hy),
        fill = effective_fill,
    ));
    // Path 2: rough.js double-line rectangle border (roughness=0, seed=1).
    s.push_str(&format!(
        r#"<path d="{d}" stroke="{stroke}" stroke-width="1.3" fill="none" stroke-dasharray="0 0"></path>"#,
        d = rough_rect_path(hx, hy, total_w, total_h),
        stroke = effective_border,
    ));
    s.push_str("</g>");

    // Re-translate logic from upstream requirementBox.ts:
    // Labels placed at original y = -fh/2 + yOffset
    // After re-translate: newY = origY - totalH/2 + padding
    // = (-fh/2 + yOffset) - totalH/2 + padding
    let re_translate = |orig_y: f64| orig_y - total_h / 2.0 + padding;

    // Pre-compute normalized div prefix (JSDOM parses cssText, adds spaces/rgb conversion).
    let div_prefix = normalize_label_styles_for_div(&label_styles_str);
    let div_prefix_opt: Option<&str> = if div_prefix.is_empty() {
        None
    } else {
        Some(&div_prefix)
    };
    let label_style_opt: Option<&str> = if label_styles_str.is_empty() {
        None
    } else {
        Some(&label_styles_str)
    };

    // Kind label — centered, y_offset=0.
    let kind_esc = xml_escape(&labels.kind_header);
    let kind_max_w =
        (text_width(&kind_esc, "sans-serif", 16.0, false, false) + 50.0).round() as i64;
    {
        let kind_orig_y = -kind_fo_h / 2.0; // yOffset=0
        let opts = LabelOpts {
            extra_span_classes: "markdown-node-label",
            max_width: kind_max_w as f64,
            label_style: label_style_opt,
            div_style_prefix: div_prefix_opt,
            group_style: Some(&label_styles_str),
            ..LabelOpts::default()
        };
        s.push_str(&format!(
            r#"<g class="label" style="{lst}" transform="translate({tx}, {ty})">"#,
            lst = label_styles_str,
            tx = fmt_num(-kind_fo_w / 2.0),
            ty = fmt_num(re_translate(kind_orig_y)),
        ));
        s.push_str(&foreign_object_body(&kind_esc, kind_fo_w, kind_fo_h, &opts));
        s.push_str("</g>");
    }

    // Name label — centered, y_offset=kindFoH, bold.
    let name_esc = xml_escape(&labels.name);
    // Upstream passes the label through markdownToHTML which converts inline
    // markdown syntax to HTML tags (e.g. `__bold__` → `<strong>bold</strong>`).
    // Apply markdown-to-HTML on the XML-escaped text so special chars are safe.
    let name_html = crate::text::markdown_to_html(&name_esc);
    // JSDOM measures textContent (markdown stripped) of the name FO.
    let name_plain = crate::text::markdown_text_content(&labels.name);
    let (name_fo_w, name_fo_h) = measure_html_label(&name_plain, &name_font, f64::INFINITY, false);
    let name_max_w =
        (text_width(&name_esc, "sans-serif", 16.0, false, false) + 50.0).round() as i64;
    {
        let name_y_offset = kind_fo_h;
        let name_orig_y = -name_fo_h / 2.0 + name_y_offset;
        // Name label style: label_styles + "; font-weight: bold;"
        let name_label_style_str = if label_styles_str.is_empty() {
            "; font-weight: bold;".to_string()
        } else {
            format!("{}; font-weight: bold;", label_styles_str)
        };
        // Name div prefix: normalized label styles but font-weight loses !important,
        // and font-weight: bold is always present (without !important).
        // Only computed when label_styles_str is non-empty (otherwise div prefix is empty).
        let name_div_prefix = normalize_name_label_div_prefix(&label_styles_str);
        let name_div_opt: Option<&str> = if name_div_prefix.is_empty() {
            None
        } else {
            Some(&name_div_prefix)
        };
        let opts = LabelOpts {
            extra_span_classes: "markdown-node-label",
            max_width: name_max_w as f64,
            label_style: Some(&name_label_style_str),
            div_style_prefix: name_div_opt,
            group_style: Some(&name_label_style_str),
            ..LabelOpts::default()
        };
        s.push_str(&format!(
            r#"<g class="label" style="{nls}" transform="translate({tx}, {ty})">"#,
            nls = name_label_style_str,
            tx = fmt_num(-name_fo_w / 2.0),
            ty = fmt_num(re_translate(name_orig_y)),
        ));
        s.push_str(&foreign_object_body(
            &name_html, name_fo_w, name_fo_h, &opts,
        ));
        s.push_str("</g>");
    }

    // Body rows (id/text/risk/verify or type/docref).
    // Placed after name, offset by gap before body starts.
    // body_y_offset = kindFoH + nameFoH + gap
    let body_start_offset = kind_fo_h + name_fo_h + gap;
    let has_body = !labels.body.is_empty();
    for (i, row) in labels.body.iter().enumerate() {
        let row_esc = xml_escape(row);
        // Upstream renders body rows through markdownToHTML for inline formatting.
        let row_html = crate::text::markdown_to_html(&row_esc);
        // JSDOM measures textContent (markdown stripped) of the body row FO.
        let row_plain = crate::text::markdown_text_content(row);
        let (row_fo_w, row_fo_h) = measure_html_label(&row_plain, &body_font, f64::INFINITY, false);
        let row_max_w =
            (text_width(&row_esc, "sans-serif", 16.0, false, false) + 50.0).round() as i64;
        let row_y_offset = body_start_offset + i as f64 * row_fo_h;
        let row_orig_y = -row_fo_h / 2.0 + row_y_offset;
        // Body labels use left-aligned x: hx + padding/2 = -totalW/2 + 10
        let row_tx = hx + padding / 2.0;
        let opts = LabelOpts {
            extra_span_classes: "markdown-node-label",
            max_width: row_max_w as f64,
            label_style: label_style_opt,
            div_style_prefix: div_prefix_opt,
            group_style: Some(&label_styles_str),
            ..LabelOpts::default()
        };
        s.push_str(&format!(
            r#"<g class="label" style="{lst}" transform="translate({tx}, {ty})">"#,
            lst = label_styles_str,
            tx = fmt_num(row_tx),
            ty = fmt_num(re_translate(row_orig_y)),
        ));
        s.push_str(&foreign_object_body(&row_html, row_fo_w, row_fo_h, &opts));
        s.push_str("</g>");
    }

    // Divider rough line — only when there are body rows.
    // lineY = y + kindFoH + nameFoH + gap = hy + kindFoH + nameFoH + gap
    if has_body {
        let line_y = hy + kind_fo_h + name_fo_h + gap;
        s.push_str(&format!(
            r#"<g class="divider"><path d="{d}" stroke="{stroke}" stroke-width="1.3" fill="none" stroke-dasharray="0 0"></path></g>"#,
            d = rough_hline_path(hx, -hx, line_y),
            stroke = effective_border,
        ));
    }

    s.push_str("</g>");
    s
}

/// Generate rough.js double-line rectangle border path string.
/// roughness=0, seed=1, bowing=1, maxRandomnessOffset=2.
///
/// With roughness=0, all G() offsets = 0. The only randomness is in
/// `p = 0.2 + 0.2 * W(randomizer)` where W uses LCG(seed=1).
fn rough_rect_path(x: f64, y: f64, w: f64, h: f64) -> String {
    // Corners: TL, TR, BR, BL
    let corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    let mut rng = LcgRng::new(1);
    let mut path = String::new();
    for i in 0..4 {
        let (x1, y1) = corners[i];
        let (x2, y2) = corners[(i + 1) % 4];
        // First line segment
        path.push_str(&rough_line_segment(x1, y1, x2, y2, &mut rng, false));
        // Second line segment (double)
        path.push_str(&rough_line_segment(x1, y1, x2, y2, &mut rng, true));
    }
    path.trim_end().to_string()
}

/// Generate rough.js double-line horizontal path for divider.
/// roughness=0, seed=1 (but rng state continues from where rect left off
/// — actually each shape gets its own rough.js instance with seed=1).
fn rough_hline_path(x1: f64, x2: f64, y: f64) -> String {
    let mut rng = LcgRng::new(1);
    let mut path = String::new();
    path.push_str(&rough_line_segment(x1, y, x2, y, &mut rng, false));
    path.push_str(&rough_line_segment(x1, y, x2, y, &mut rng, true));
    path.trim_end().to_string()
}

/// LCG random number generator matching rough.js `p` class.
/// seed = (48271 * seed) & (2^31 - 1); value = seed / 2^31
struct LcgRng {
    seed: u64,
}

impl LcgRng {
    fn new(seed: u64) -> Self {
        Self { seed }
    }
    fn next(&mut self) -> f64 {
        if self.seed == 0 {
            return 0.5; // non-seeded path: use constant (roughness=0 means offset=0 anyway)
        }
        self.seed = (48271u64 * self.seed) & ((1u64 << 31) - 1);
        self.seed as f64 / (1u64 << 31) as f64
    }
}

/// Single rough.js line segment (one half of a double line).
///
/// rough.js `R()` (internal `_line`) makes exactly 11 RNG calls per segment:
///   1. `p = 0.2 + 0.2 * W(o)`  — used for bezier control points
///   2. `G(f, o, c)` = `E(-f, f, o, c)` — bowing offset x (=0 with roughness=0)
///   3. `G(d, o, c)` — bowing offset y (=0)
///   4-5. move op: two `G(u/l, o, c)` calls (=0)
///   6-11. bcurveTo op: six `G(u/l, o, c)` calls (=0)
///
/// All calls after #1 produce 0 with roughness=0, but the RNG state still advances.
/// We must consume all 11 to keep subsequent segments in sync with upstream.
fn rough_line_segment(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    rng: &mut LcgRng,
    _is_second: bool,
) -> String {
    // Call 1: p = 0.2 + 0.2 * W(o)
    let p = 0.2 + 0.2 * rng.next();
    // Calls 2-11: G() / E() calls that return 0 with roughness=0 but must advance RNG
    for _ in 0..10 {
        rng.next();
    }
    // With roughness=0, all offsets are zero.
    // Bezier: Cx1+(x2-x1)*p, y1+(y2-y1)*p, x1+2*(x2-x1)*p, y1+2*(y2-y1)*p, x2, y2
    let cx1 = x1 + (x2 - x1) * p;
    let cy1 = y1 + (y2 - y1) * p;
    let cx2 = x1 + 2.0 * (x2 - x1) * p;
    let cy2 = y1 + 2.0 * (y2 - y1) * p;
    // rough.js serialises cubic beziers as "M x1 y1 C cx1 cy1, cx2 cy2, x2 y2 "
    // (space within each control-point pair, comma between pairs).
    format!(
        "M{x1} {y1} C{cx1} {cy1}, {cx2} {cy2}, {x2} {y2} ",
        x1 = fmt_num(x1),
        y1 = fmt_num(y1),
        cx1 = fmt_num(cx1),
        cy1 = fmt_num(cy1),
        cx2 = fmt_num(cx2),
        cy2 = fmt_num(cy2),
        x2 = fmt_num(x2),
        y2 = fmt_num(y2),
    )
}

/// Format a number with d3's appendRound(3) — at most 3 decimal places,
/// no trailing zeros (e.g. 86.947265625 → "86.947", 77.0 → "77").
fn fmt_r3(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let s = format!("{:.3}", r);
    // Strip trailing zeros and optional trailing dot
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

/// Build an SVG path string for an edge using d3's curveBasis algorithm.
///
/// D3's `curveBasis` on points [p0, p1, ..., pN] produces B-spline paths.
/// For the typical requirement-diagram 3-point edge [p0, p1, p2]:
///   M p0
///   L (5p0+p1)/6
///   C (2p0+p1)/3, (p0+2p1)/3, (p0+4p1+p2)/6
///   C (2p1+p2)/3, (p1+2p2)/3, (p1+4p2+p2)/6
///   L p2
///
/// All coordinates are rounded to 3 decimal places (d3-path's appendRound(3)).
fn edge_path_basis(pts: &[crate::layout::unified::Point]) -> String {
    let n = pts.len();
    if n == 0 {
        return String::new();
    }
    if n == 1 {
        return format!("M{},{}", fmt_r3(pts[0].x), fmt_r3(pts[0].y));
    }

    // d3 curveBasis state machine
    // _x0, _y0 = point two steps back; _x1, _y1 = one step back
    let mut d = String::new();
    let mut x0 = f64::NAN;
    let mut y0 = f64::NAN;
    let mut x1 = f64::NAN;
    let mut y1 = f64::NAN;
    let mut point_idx = 0usize; // 0=first, 1=second, 2+=subsequent

    // Helper: bezierCurveTo
    let bcurve_to = |d: &mut String, x0: f64, y0: f64, x1: f64, y1: f64, x: f64, y: f64| {
        let cp1x = (2.0 * x0 + x1) / 3.0;
        let cp1y = (2.0 * y0 + y1) / 3.0;
        let cp2x = (x0 + 2.0 * x1) / 3.0;
        let cp2y = (y0 + 2.0 * y1) / 3.0;
        let ex = (x0 + 4.0 * x1 + x) / 6.0;
        let ey = (y0 + 4.0 * y1 + y) / 6.0;
        d.push_str(&format!(
            "C{},{},{},{},{},{}",
            fmt_r3(cp1x),
            fmt_r3(cp1y),
            fmt_r3(cp2x),
            fmt_r3(cp2y),
            fmt_r3(ex),
            fmt_r3(ey)
        ));
    };

    for (i, p) in pts.iter().enumerate() {
        let (x, y) = (p.x, p.y);
        match point_idx {
            0 => {
                // case 0: moveTo
                d.push_str(&format!("M{},{}", fmt_r3(x), fmt_r3(y)));
                point_idx = 1;
            }
            1 => {
                // case 1: just save, no output
                point_idx = 2;
            }
            2 => {
                // case 2: lineTo (5x0+x1)/6, then falls through to bezierCurveTo
                let lx = (5.0 * x0 + x1) / 6.0;
                let ly = (5.0 * y0 + y1) / 6.0;
                d.push_str(&format!("L{},{}", fmt_r3(lx), fmt_r3(ly)));
                bcurve_to(&mut d, x0, y0, x1, y1, x, y);
                point_idx = 3;
            }
            _ => {
                // default: just bezierCurveTo
                bcurve_to(&mut d, x0, y0, x1, y1, x, y);
            }
        }
        // Advance state: x0 = old x1, x1 = current x
        if i > 0 {
            x0 = x1;
            y0 = y1;
        }
        x1 = x;
        y1 = y;
        let _ = i; // silence unused warning
    }

    // lineEnd
    match point_idx {
        3 | _ if point_idx >= 3 => {
            // case 3: point(_x1, _y1) then falls to case 2: lineTo(_x1, _y1)
            bcurve_to(&mut d, x0, y0, x1, y1, x1, y1);
            d.push_str(&format!("L{},{}", fmt_r3(x1), fmt_r3(y1)));
        }
        2 => {
            d.push_str(&format!("L{},{}", fmt_r3(x1), fmt_r3(y1)));
        }
        _ => {}
    }

    d
}

fn render_edge(id_prefix: &str, e: &UEdge) -> String {
    let points = match e.points.as_ref() {
        Some(p) if p.len() >= 2 => p,
        _ => return String::new(),
    };
    let d = edge_path_basis(points);
    let pattern_cls = match e.pattern.as_deref() {
        Some("dashed") => "edge-pattern-dashed",
        _ => "edge-pattern-solid",
    };
    let classes = format!(" edge-thickness-normal {} relationshipLine", pattern_cls,);

    // Build base64 data-points from the edge's point array.
    let data_points_b64 = {
        let mut json = String::from("[");
        for (i, p) in points.iter().enumerate() {
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

    // Build the edge data-id from source-target-index.
    // Use start/end (set by layout) falling back to source/target (post-bridge).
    let e_src = e.start.as_deref().or(e.source.as_deref()).unwrap_or("");
    let e_dst = e.end.as_deref().or(e.target.as_deref()).unwrap_or("");
    let edge_data_id = format!("{}-{}-0", xml_escape(e_src), xml_escape(e_dst));

    // Upstream `insertEdge` computes `pathStyle` as:
    //   styles = edge.style.reduce((acc, s) => acc + s + ';', '')
    //   pathStyle = styles + ';' + edge.style.reduce((acc, s) => acc + ';' + s, '')
    // No cssCompiledStyles for requirement edges (stylesFromClasses = "").
    let style = {
        let edge_styles = e.style.as_ref().map(|v| v.as_slice()).unwrap_or(&[]);
        // styles = each item + ";"
        let styles: String = edge_styles
            .iter()
            .fold(String::new(), |acc, s| acc + s + ";");
        // reduce2 = each item prepended with ";"
        let reduce2: String = edge_styles
            .iter()
            .fold(String::new(), |acc, s| acc + ";" + s);
        format!("{};{}", styles, reduce2)
    };
    let mut marker_attrs = String::new();
    if e.arrow_type_start
        .as_deref()
        .map_or(false, |s| !s.is_empty())
    {
        marker_attrs.push_str(&format!(
            r#" marker-start="url(#{id}_requirement-requirement_containsStart)""#,
            id = id_prefix,
        ));
    }
    if e.arrow_type_end.as_deref().map_or(false, |s| !s.is_empty()) {
        marker_attrs.push_str(&format!(
            r#" marker-end="url(#{id}_requirement-requirement_arrowEnd)""#,
            id = id_prefix,
        ));
    }
    format!(
        r#"<path d="{d}" id="{id_prefix}-{eid}" class="{cls}" style="{st}" data-edge="true" data-et="edge" data-id="{did}" data-points="{b64}" data-look="classic"{ma}></path>"#,
        d = d,
        eid = xml_escape(&e.id),
        cls = &classes,
        st = style,
        did = edge_data_id,
        b64 = data_points_b64,
        ma = marker_attrs,
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
    let e_src = e.start.as_deref().or(e.source.as_deref()).unwrap_or("");
    let e_dst = e.end.as_deref().or(e.target.as_deref()).unwrap_or("");
    let data_id = format!("{}-{}-0", xml_escape(e_src), xml_escape(e_dst));
    let opts = LabelOpts {
        data_id: Some(&data_id),
        group_style: None,
        ..LabelOpts::default()
    };
    fo_edge(&esc, x, y, el.width, el.height, opts)
}

fn marker_defs(id_prefix: &str) -> String {
    // Two markers — circle-cross (contains start) + arrow (end).
    // The arrow path uses upstream's literal newline-indented format to
    // match the reference SVG byte-for-byte.
    format!(
        concat!(
            r#"<defs><marker id="{id}_requirement-requirement_containsStart" refX="0" refY="10" markerWidth="20" markerHeight="20" orient="auto"><g><circle cx="10" cy="10" r="9" fill="none"></circle><line x1="1" x2="19" y1="10" y2="10"></line><line y1="1" y2="19" x1="10" x2="10"></line></g></marker></defs>"#,
            r#"<defs><marker id="{id}_requirement-requirement_arrowEnd" refX="20" refY="10" markerWidth="20" markerHeight="20" orient="auto"><path d="M0,0"#,
            "\n      L20,10\n      M20,10\n      L0,20\"></path></marker></defs>",
        ),
        id = id_prefix,
    )
}

/// Requirement-diagram-specific CSS — port of upstream `styles.js`.
///
/// Sits between the shared `base_preamble` and `neo_look_block` in the
/// `<style>` block. Covers:
///
/// * `genColor()` — `[data-color-id="color-N"]` rules for each colour
///   in the `cScalePeer` / `cScale` arrays (only emitted when
///   `borderColorArray` is non-empty; the default theme has none).
/// * `marker` fill/stroke (re-emitted here because upstream's
///   `styles.js` repeats them after the preamble).
/// * `marker.cross` stroke.
/// * `svg` font-family/font-size (also repeated).
/// * `.reqBox`, `.reqTitle`/`.reqLabel`, `.reqLabelBox`,
///   `.req-title-line`, `.relationshipLine`, `.relationshipLabel`,
///   `.edgeLabel`, `.divider`, `.label`, `.labelBkg`.
fn requirement_specific_css(id: &str, theme: &ThemeVariables) -> String {
    let rel_color = theme.relation_color.as_deref().unwrap_or("#333333");
    let req_bg = theme.requirement_background.as_deref().unwrap_or("#ECECFF");
    let req_border = theme
        .requirement_border_color
        .as_deref()
        .unwrap_or("hsl(240, 60%, 86.2745098039%)");
    let req_border_size = theme.requirement_border_size.as_deref().unwrap_or("1");
    let req_text = theme.requirement_text_color.as_deref().unwrap_or("#131300");
    let rel_label_bg = theme
        .relation_label_background
        .as_deref()
        .unwrap_or("rgba(232,232,232, 0.8)");
    let rel_label_color = theme.relation_label_color.as_deref().unwrap_or("black");
    let ff_raw = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\", verdana, arial, sans-serif");
    let ff = crate::render::stylis::strip_comma_spaces(ff_raw);
    let font_size = theme.font_size.as_deref().unwrap_or("16px");
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let node_text_color = theme.node_text_color.as_deref().unwrap_or(text_color);
    let edge_label_bg = theme
        .edge_label_background
        .as_deref()
        .unwrap_or("rgba(232,232,232, 0.8)");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");

    // Build borderColorArray / bkgColorArray from theme variables,
    // matching upstream's genColor() logic. Only emit [data-color-id]
    // rules when borderColorArray is explicitly set and non-empty
    // (the default theme does NOT set it).
    let border_color_array = theme.border_color_array.as_ref();
    let bkg_color_array = theme.bkg_color_array.as_ref();
    let theme_color_limit = border_color_array.map(|a| a.len()).unwrap_or(0);

    let mut css = String::with_capacity(6000);

    // genColor() — [data-color-id] rules (only when borderColorArray
    // is non-empty, matching upstream's early-return guard).
    if let Some(bca) = border_color_array {
        if !bca.is_empty() {
            let look = "classic";
            for i in 0..theme_color_limit {
                let border_color = &bca[i];
                let bkg_fill = bkg_color_array
                    .and_then(|a| a.get(i))
                    .map(|s| s.as_str())
                    .unwrap_or("");
                css.push_str(&format!(
                    r#"#{id} [data-look="{look}"][data-color-id="color-{i}"].node path{{stroke:{bc};fill:{bf};}}"#,
                    bc = border_color,
                    bf = bkg_fill,
                ));
                css.push_str(&format!(
                    r#"#{id} [data-look="{look}"][data-color-id="color-{i}"].node rect{{stroke:{bc};fill:{bf};}}"#,
                    bc = border_color,
                    bf = bkg_fill,
                ));
            }
        }
    }

    // marker (repeated from preamble — upstream styles.js emits these)
    css.push_str(&format!(
        "#{id} marker{{fill:{rc};stroke:{rc};}}",
        rc = rel_color,
    ));
    css.push_str(&format!(
        "#{id} marker.cross{{stroke:{lc};}}",
        lc = theme.line_color.as_deref().unwrap_or("#333333"),
    ));

    // svg (repeated from preamble)
    css.push_str(&format!(
        "#{id} svg{{font-family:{ff};font-size:{fs};}}",
        fs = font_size,
    ));

    // .reqBox
    css.push_str(&format!(
        "#{id} .reqBox{{fill:{rb};fill-opacity:1.0;stroke:{br};stroke-width:{bs};}}",
        rb = req_bg,
        br = req_border,
        bs = req_border_size,
    ));
    // .reqTitle, .reqLabel
    css.push_str(&format!(
        "#{id} .reqTitle,#{id} .reqLabel{{fill:{rt};}}",
        rt = req_text,
    ));
    // .reqLabelBox
    css.push_str(&format!(
        "#{id} .reqLabelBox{{fill:{rlb};fill-opacity:1.0;}}",
        rlb = rel_label_bg,
    ));
    // .req-title-line
    css.push_str(&format!(
        "#{id} .req-title-line{{stroke:{br};stroke-width:{bs};}}",
        br = req_border,
        bs = req_border_size,
    ));
    // .relationshipLine — stroke-width is 1px for classic look
    css.push_str(&format!(
        "#{id} .relationshipLine{{stroke:{rc};stroke-width:1px;}}",
        rc = rel_color,
    ));
    // .relationshipLabel
    css.push_str(&format!(
        "#{id} .relationshipLabel{{fill:{rlc};}}",
        rlc = rel_label_color,
    ));
    // .edgeLabel
    css.push_str(&format!(
        "#{id} .edgeLabel{{background-color:{elb};}}",
        elb = edge_label_bg,
    ));
    // .edgeLabel .label rect
    css.push_str(&format!(
        "#{id} .edgeLabel .label rect{{fill:{elb};}}",
        elb = edge_label_bg,
    ));
    // .edgeLabel .label text
    css.push_str(&format!(
        "#{id} .edgeLabel .label text{{fill:{rlc};}}",
        rlc = rel_label_color,
    ));
    // .divider
    css.push_str(&format!(
        "#{id} .divider{{stroke:{nb};stroke-width:1;}}",
        nb = node_border,
    ));
    // .label
    css.push_str(&format!(
        "#{id} .label{{font-family:{ff};color:{ntc};}}",
        ntc = node_text_color,
    ));
    // .label text,span
    css.push_str(&format!(
        "#{id} .label text,#{id} span{{fill:{ntc};color:{ntc};}}",
        ntc = node_text_color,
    ));
    // .labelBkg — uses requirementEdgeLabelBackground if set, else edgeLabelBackground
    let label_bkg_bg = theme
        .requirement_edge_label_background
        .as_deref()
        .unwrap_or(edge_label_bg);
    css.push_str(&format!(
        "#{id} .labelBkg{{background-color:{lbb};}}",
        lbb = label_bkg_bg,
    ));

    css
}

/// Convert a hex color string (#rrggbb or #rgb) to `rgb(r, g, b)` format,
/// matching JSDOM's CSS parsing normalization. Returns the input unchanged if
/// it's not a hex color.
fn hex_to_rgb(s: &str) -> String {
    let s = s.trim();
    if !s.starts_with('#') {
        return s.to_string();
    }
    let hex = &s[1..];
    let (r, g, b) = if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok();
        let g = u8::from_str_radix(&hex[2..4], 16).ok();
        let b = u8::from_str_radix(&hex[4..6], 16).ok();
        match (r, g, b) {
            (Some(r), Some(g), Some(b)) => (r, g, b),
            _ => return s.to_string(),
        }
    } else if hex.len() == 3 {
        let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok();
        let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok();
        let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok();
        match (r, g, b) {
            (Some(r), Some(g), Some(b)) => (r, g, b),
            _ => return s.to_string(),
        }
    } else {
        return s.to_string();
    };
    format!("rgb({}, {}, {})", r, g, b)
}

/// Compute the div style prefix for the NAME label.
///
/// The name label div gets `applyStyle(div, labelStyle + '; font-weight: bold;')`.
/// JSDOM normalizes this by:
/// 1. Processing `font-weight:bold !important` → `font-weight: bold` (plain)
///    when followed by `; font-weight: bold` in the same style string.
/// 2. Other properties keep their `!important`.
/// 3. All values are normalized (hex→rgb, spaces after colons).
///
/// When label_styles is empty, returns `""` (no div prefix — the name g's style
/// starts with `;` which JSDOM can't parse, so no extra styles appear in the div).
fn normalize_name_label_div_prefix(label_styles: &str) -> String {
    if label_styles.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    let mut has_font_weight = false;
    for item in label_styles.split(';') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        // item format: "key:value !important" (from split_node_label_styles)
        let (key_val, _priority) = if let Some(rest) = item.strip_suffix(" !important") {
            (rest, " !important")
        } else {
            (item, "")
        };
        if let Some(colon_pos) = key_val.find(':') {
            let key = key_val[..colon_pos].trim();
            let val = key_val[colon_pos + 1..].trim();
            if key == "font-weight" {
                // font-weight: always plain bold (no !important) in the name div
                parts.push(format!("font-weight: {}", val));
                has_font_weight = true;
            } else {
                // Normalize color values, keep !important
                let normalized_val = if key == "color"
                    || key == "background-color"
                    || key == "border-color"
                    || key == "fill"
                    || key == "stroke"
                {
                    hex_to_rgb(val)
                } else {
                    val.to_string()
                };
                parts.push(format!("{}: {} !important", key, normalized_val));
            }
        }
    }
    if !has_font_weight {
        parts.push("font-weight: bold".to_string());
    }
    if parts.is_empty() {
        String::new()
    } else {
        parts.join("; ") + "; "
    }
}

/// Normalize CSS label styles for a div element — JSDOM parses the CSS text
/// and re-normalizes it: adds spaces around `:`, converts hex colors to rgb.
/// This matches what JSDOM produces when `div.attr('style', labelStyles)` is
/// called followed by `.style('display', ...)` etc.
fn normalize_label_styles_for_div(label_styles: &str) -> String {
    if label_styles.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    for item in label_styles.split(';') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        // item format: "key:value !important" or "key:value"
        let (key_val, priority) = if let Some(rest) = item.strip_suffix(" !important") {
            (rest, " !important")
        } else {
            (item, "")
        };
        if let Some(colon_pos) = key_val.find(':') {
            let key = key_val[..colon_pos].trim();
            let val = key_val[colon_pos + 1..].trim();
            // Convert hex colors to rgb
            let normalized_val = if key == "color"
                || key == "background-color"
                || key == "border-color"
                || key == "fill"
                || key == "stroke"
            {
                hex_to_rgb(val)
            } else {
                val.to_string()
            };
            parts.push(format!("{}: {}{}", key, normalized_val, priority));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        // JSDOM adds trailing "; " separator before "display: table-cell"
        parts.join("; ") + "; "
    }
}

/// CSS properties that apply to the label text (vs. the node border/fill).
/// Matches upstream's `isLabelStyle` in `handDrawnShapeStyles.ts`.
fn is_label_style(key: &str) -> bool {
    matches!(
        key,
        "color"
            | "font-size"
            | "font-family"
            | "font-weight"
            | "font-style"
            | "text-decoration"
            | "text-align"
            | "text-transform"
            | "line-height"
            | "letter-spacing"
            | "word-spacing"
            | "text-shadow"
            | "text-overflow"
            | "white-space"
            | "word-wrap"
            | "word-break"
            | "overflow-wrap"
            | "hyphens"
    )
}

/// Split css_styles into (node_styles, label_styles) matching upstream's
/// `styles2String()`. Each style gets ` !important` appended.
/// Upstream's `styles2Map` splits on `:` and re-joins without spaces, so
/// `"font-weight: bold"` → `"font-weight:bold !important"`.
fn split_node_label_styles(css_styles: &[String]) -> (String, String) {
    let mut node_parts: Vec<String> = Vec::new();
    let mut label_parts: Vec<String> = Vec::new();
    for s in css_styles {
        let mut iter = s.splitn(2, ':');
        let key = iter.next().unwrap_or("").trim();
        let val = iter.next().map(|v| v.trim()).unwrap_or("");
        // Normalize: key:value (no spaces around colon)
        let normalized = format!("{}:{}", key, val);
        if is_label_style(key) {
            label_parts.push(format!("{} !important", normalized));
        } else {
            node_parts.push(format!("{} !important", normalized));
        }
    }
    (node_parts.join(";"), label_parts.join(";"))
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

    /// Byte-exact sweep across the 44 fixtures. This stays non-failing
    /// and reports the current pass count so follow-up waves can ratchet
    /// the requirement renderer upward without rewriting the harness.
    #[test]
    fn byte_exact_sweep_reports_progress() {
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
                        eprintln!("[requirement-MATCH] {}", stem);
                    } else {
                        let prefix = got
                            .bytes()
                            .zip(expected.bytes())
                            .take_while(|(a, b)| a == b)
                            .count();
                        eprintln!(
                            "[requirement-DIFF] {} got={} exp={} prefix={}",
                            stem,
                            got.len(),
                            expected.len(),
                            prefix
                        );
                        if prefix < expected.len() {
                            let exp_ctx =
                                &expected[prefix..std::cmp::min(prefix + 80, expected.len())];
                            let got_ctx = &got[prefix..std::cmp::min(prefix + 80, got.len())];
                            eprintln!("  exp: {:?}", exp_ctx);
                            eprintln!("  got: {:?}", got_ctx);
                        }
                    }
                }
            }
        }
        eprintln!("[requirement] byte-exact={}/{}", matched, total);
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

    /// Diagnostic: reports shell+preamble alignment for one fixture.
    /// Wave 3.5's unified-shell work aims to make the `<svg><style>`
    /// prefix match the reference byte-for-byte, independent of any
    /// layout differences in the diagram body.
    #[test]
    fn dump_requirement_01_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let rel = "ext_fixtures/cypress/requirement/01";
        let Ok(mmd) = std::fs::read_to_string(base.join(format!("tests/{}.mmd", rel))) else {
            return;
        };
        let Ok(exp) = std::fs::read_to_string(base.join(format!("tests/reference/{}.svg", rel)))
        else {
            return;
        };
        let id = "ref-ext-fixtures-cypress-requirement-01";
        let Ok(d) = parse(&mmd) else { return };
        let theme = get_theme("default");
        let Ok(l) = layout(&d, &theme) else { return };
        let Ok(got) = render(&d, &l, &theme, id) else {
            return;
        };
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "[requirement-diag] got={} exp={} prefix={}",
            got.len(),
            exp.len(),
            prefix
        );

        // Compare CSS sections
        let got_style_start = got.find("<style>").map(|i| i + 7).unwrap_or(0);
        let got_style_end = got.find("</style>").unwrap_or(got.len());
        let got_css = &got[got_style_start..got_style_end];

        let exp_style_start = exp.find("<style>").map(|i| i + 7).unwrap_or(0);
        let exp_style_end = exp.find("</style>").unwrap_or(exp.len());
        let exp_css = &exp[exp_style_start..exp_style_end];

        let css_prefix = got_css
            .bytes()
            .zip(exp_css.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "[requirement-css-diag] got_css={} exp_css={} prefix={}",
            got_css.len(),
            exp_css.len(),
            css_prefix
        );

        // Dump our output to /tmp for comparison
        let _ = std::fs::write("/tmp/got_req01.svg", &got);

        // Dump node dimensions
        for (i, n) in l.graph.nodes.iter().enumerate() {
            eprintln!(
                "Node {}: id={} x={:?} y={:?} w={:?} h={:?}",
                i, n.id, n.x, n.y, n.width, n.height
            );
        }
        for (i, e) in l.graph.edges.iter().enumerate() {
            eprintln!("Edge {}: id={} points={:?}", i, e.id, e.points);
        }
    }

    #[test]
    fn dump_requirement_07_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let rel = "ext_fixtures/cypress/requirement/07";
        let Ok(mmd) = std::fs::read_to_string(base.join(format!("tests/{}.mmd", rel))) else {
            return;
        };
        let Ok(exp) = std::fs::read_to_string(base.join(format!("tests/reference/{}.svg", rel)))
        else {
            return;
        };
        let id = "ref-ext-fixtures-cypress-requirement-07";
        let Ok(d) = parse(&mmd) else { return };
        let theme = get_theme("default");
        let Ok(l) = layout(&d, &theme) else { return };
        let Ok(got) = render(&d, &l, &theme, id) else {
            return;
        };
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "[req07-diag] got={} exp={} prefix={}",
            got.len(),
            exp.len(),
            prefix
        );
        let _ = std::fs::write("/tmp/got_req07.svg", &got);
        if got != exp {
            // Find first difference
            let gb: Vec<u8> = got.bytes().collect();
            let eb: Vec<u8> = exp.bytes().collect();
            let ctx_start = if prefix > 30 { prefix - 30 } else { 0 };
            let got_ctx =
                String::from_utf8_lossy(&gb[ctx_start..std::cmp::min(prefix + 100, gb.len())]);
            let exp_ctx =
                String::from_utf8_lossy(&eb[ctx_start..std::cmp::min(prefix + 100, eb.len())]);
            eprintln!("[req07-diff-got] ...{}...", got_ctx);
            eprintln!("[req07-diff-exp] ...{}...", exp_ctx);
        }
        for (i, n) in l.graph.nodes.iter().enumerate() {
            eprintln!(
                "Node07 {}: id={} x={:?} y={:?} w={:?} h={:?}",
                i, n.id, n.x, n.y, n.width, n.height
            );
        }
    }

    fn dump_fixture(name: &str, id_suffix: &str) {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let rel = format!("ext_fixtures/cypress/requirement/{}", name);
        let Ok(mmd) = std::fs::read_to_string(base.join(format!("tests/{}.mmd", rel))) else {
            return;
        };
        let Ok(exp) = std::fs::read_to_string(base.join(format!("tests/reference/{}.svg", rel)))
        else {
            return;
        };
        let id = format!("ref-ext-fixtures-cypress-requirement-{}", id_suffix);
        let Ok(d) = parse(&mmd) else { return };
        let theme = get_theme("default");
        let Ok(l) = layout(&d, &theme) else { return };
        let Ok(got) = render(&d, &l, &theme, &id) else {
            return;
        };
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "[req{}-diag] got={} exp={} prefix={}",
            name,
            got.len(),
            exp.len(),
            prefix
        );
        let _ = std::fs::write(format!("/tmp/got_req{}.svg", name), &got);
        let _ = std::fs::write(format!("/tmp/exp_req{}.svg", name), &exp);
        if got != exp {
            let gb: Vec<u8> = got.bytes().collect();
            let eb: Vec<u8> = exp.bytes().collect();
            let ctx_start = if prefix > 60 { prefix - 60 } else { 0 };
            let got_ctx =
                String::from_utf8_lossy(&gb[ctx_start..std::cmp::min(prefix + 200, gb.len())]);
            let exp_ctx =
                String::from_utf8_lossy(&eb[ctx_start..std::cmp::min(prefix + 200, eb.len())]);
            eprintln!("[req{}-diff-got] ...{}...", name, got_ctx);
            eprintln!("[req{}-diff-exp] ...{}...", name, exp_ctx);
        }
    }

    #[test]
    fn dump_fixtures_for_analysis() {
        dump_fixture("27", "27");
        dump_fixture("28", "28");
        dump_fixture("13", "13");
        dump_fixture("29", "29");
        dump_fixture("34", "34");
        dump_fixture("35", "35");
        dump_fixture("36", "36");
        dump_fixture("38", "38");
        dump_fixture("40", "40");
        dump_fixture("14", "14");
        dump_fixture("15", "15");
        dump_fixture("16", "16");
        dump_fixture("43", "43");
        dump_fixture("24", "24");
        dump_fixture("25", "25");
        dump_fixture("20", "20");
        dump_fixture("21", "21");
        dump_fixture("27", "27");
    }

    #[test]
    fn debug_measure_html_labels() {
        use crate::render::foreign_object::{measure_html_label, HtmlLabelFont};
        let font_plain = HtmlLabelFont {
            bold: Some(false),
            ..HtmlLabelFont::default()
        };
        let font_bold = HtmlLabelFont {
            bold: Some(true),
            ..HtmlLabelFont::default()
        };
        let labels = [
            ("Text: **Bolded text** _italicized text_", 219.830078125),
            ("Risk: High", 70.3623046875),
            ("Verification: Test", 118.86328125),
            ("ID: 1", 32.9833984375),
            ("__my bolded name__", 0.0), // unknown expected
            ("*my italicized name*", 0.0),
            ("Type: **Bolded type** _italicized type_", 0.0),
            ("Doc Ref: *Italicized* __Bolded__", 0.0),
            ("test_entity_name_that_is_extra_long", 255.7802734375),
            ("my bolded name", 0.0),
            ("my italicized name", 0.0),
        ];
        for (text, exp_w) in &labels {
            let (wp, hp) = measure_html_label(text, &font_plain, f64::INFINITY, false);
            let (wb, hb) = measure_html_label(text, &font_bold, f64::INFINITY, false);
            eprintln!(
                "label={:50} plain={:20} bold={:20} h={:10} exp={}",
                text, wp, wb, hp, exp_w
            );
        }
    }
}
