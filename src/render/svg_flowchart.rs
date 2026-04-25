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

/// Compute the viewBox matching the upstream jsdom `getBBox()` shim.
///
/// The reference generator patches `SVGElement.prototype.getBBox` with an
/// implementation that computes the union of all descendant elements' local
/// bounding boxes WITHOUT applying any `transform` attributes. This means:
///
/// - A `<rect x="-w/2" y="-h/2" ...>` inside a `<g transform="translate(cx, cy)">`
///   contributes `{x: -w/2, y: -h/2, w, h}` — not the global position.
/// - A diamond `<polygon points="s/2,0 s,-s/2 s/2,-s 0,-s/2">` contributes
///   `{x: 0, y: -s, w: s, h: s}` (the polygon's own translate is also ignored).
/// - Edge `<path>` elements live in a group with no transform, so their
///   coordinates ARE global dagre coordinates.
/// - Edge-label `<foreignObject width="w" height="h">` contributes `{x:0, y:0, w, h}`
///   because the outer label `<g>` has a centring transform that is ignored.
///
/// `viewBox = "${union.x - p} ${union.y - p} ${union.w + 2p} ${union.h + 2p}"`
fn compute_viewbox(l: &FlowchartLayout, padding: f64) -> (f64, f64, f64, f64) {
    use crate::render::foreign_object::{
        measure_html_label, measure_html_markup_label, HtmlLabelFont,
    };

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    let expand = |min_x: &mut f64,
                  min_y: &mut f64,
                  max_x: &mut f64,
                  max_y: &mut f64,
                  bx: f64,
                  by: f64,
                  bw: f64,
                  bh: f64| {
        if bw == 0.0 && bh == 0.0 {
            return;
        }
        if bx < *min_x {
            *min_x = bx;
        }
        if by < *min_y {
            *min_y = by;
        }
        if bx + bw > *max_x {
            *max_x = bx + bw;
        }
        if by + bh > *max_y {
            *max_y = by + bh;
        }
    };

    // Font for foreignObject label measurement.
    let font = HtmlLabelFont::default();

    // Node local bboxes (transform ignored — matching jsdom shim).
    for n in &l.nodes {
        let w = n.width.unwrap_or(0.0);
        let h = n.height.unwrap_or(0.0);

        if n.is_group {
            // Cluster (subgraph) rect: the cluster <g> has NO transform in upstream SVG;
            // the rect uses absolute coordinates x = node.x - w/2, y = node.y - h/2.
            // jsdom shim reads these absolute values directly.
            let cx = n.x.unwrap_or(0.0);
            let cy = n.y.unwrap_or(0.0);
            let rx = cx - w / 2.0;
            let ry = cy - h / 2.0;
            expand(&mut min_x, &mut min_y, &mut max_x, &mut max_y, rx, ry, w, h);
            // Cluster label foreignObject: cluster-label <g> has transform (ignored).
            // foreignObject sits at (0,0) relative to the cluster-label <g>.
            let label_text = n.label.as_deref().unwrap_or("");
            if !label_text.is_empty() {
                let processed = crate::render::foreign_object::replace_fa_icons(label_text);
                let (lw, lh) = measure_html_markup_label(&processed, &font, 200.0, true);
                expand(
                    &mut min_x, &mut min_y, &mut max_x, &mut max_y, 0.0, 0.0, lw, lh,
                );
            }
            continue;
        }

        let shape = n.shape.as_deref().unwrap_or("rect");
        match shape {
            "diamond" | "question" => {
                // polygon points="s/2,0 s,-s/2 s/2,-s 0,-s/2"
                // polygon has its own transform="translate(-s/2+0.5, s/2)" which is ignored.
                // polyBBox of points = {x:0, y:-s, w:s, h:s}
                let s = w; // w == h == s for diamond
                expand(
                    &mut min_x, &mut min_y, &mut max_x, &mut max_y, 0.0, -s, s, s,
                );
            }
            "circle" | "circ" => {
                // Upstream circle.ts: r = w/2 (with corrected sizing: d = label_w + padding).
                // The <circle r> in local coords → bbox {x:-r, y:-r, w:2r, h:2r}.
                // jsdom shim ignores transform, uses this local bbox.
                let r = w / 2.0;
                expand(
                    &mut min_x,
                    &mut min_y,
                    &mut max_x,
                    &mut max_y,
                    -r,
                    -r,
                    2.0 * r,
                    2.0 * r,
                );
            }
            _ => {
                // rect/round/stadium/etc.: rect x=-w/2, y=-h/2
                expand(
                    &mut min_x,
                    &mut min_y,
                    &mut max_x,
                    &mut max_y,
                    -w / 2.0,
                    -h / 2.0,
                    w,
                    h,
                );
            }
        }
        // Node label foreignObject: at (0,0) in local coords (label <g> transform ignored).
        // The jsdom shim's getBBox treats each foreignObject as {x:0, y:0, w, h}.
        let label_text = n.label.as_deref().unwrap_or("");
        if !label_text.is_empty() {
            let is_markdown = n.label_type.as_deref() == Some("markdown");
            let label_escaped = if is_markdown {
                crate::render::foreign_object::markdown_label_to_html(label_text)
            } else {
                crate::render::foreign_object::string_label_to_html(label_text)
            };
            let processed = crate::render::foreign_object::replace_fa_icons(&label_escaped);
            let (lw, lh) = measure_html_markup_label(&processed, &font, 200.0, true);
            expand(
                &mut min_x, &mut min_y, &mut max_x, &mut max_y, 0.0, 0.0, lw, lh,
            );
        }
    }

    // Edge paths: coordinates are already global (edgePaths <g> has no transform).
    // Round to 3 decimal places to match fmt_coord() applied when building the
    // SVG path d attribute — the upstream pathBBox parses the rendered d string.
    //
    // Additionally apply the marker visual offset (`markerOffsets.arrow_point = 4`)
    // to the path endpoints, matching upstream's `getLineFunctionsWithOffset`.
    // The offset shortens the path at arrow ends so the arrowhead doesn't
    // overlap the node boundary. For a vertical edge going downward:
    //   last_y_adjusted = last_y - 4
    // For a diagonal edge, it is proportional to sin(angle).
    //
    // Upstream markerOffsets only has: arrow_point=4, arrow_barb=0, arrow_barb_neo=5.5.
    // arrow_open (edges without arrowheads, e.g. `---`) is NOT present, so NO offset
    // is applied for those edges.
    const ARROW_POINT_OFFSET: f64 = 4.0;
    for e in &l.edges {
        let Some(points) = &e.points else { continue };
        let n = points.len();
        if n == 0 {
            continue;
        }

        let arrow_end = e.arrow_type_end.as_deref().unwrap_or("none");
        let arrow_start = e.arrow_type_start.as_deref().unwrap_or("none");
        // Only arrow_point carries a non-zero offset in upstream's markerOffsets.
        let end_offset = if arrow_end == "arrow_point" {
            ARROW_POINT_OFFSET
        } else {
            0.0
        };
        let start_offset = if arrow_start == "arrow_point" {
            ARROW_POINT_OFFSET
        } else {
            0.0
        };

        for (i, p) in points.iter().enumerate() {
            let (mut px, mut py) = (p.x, p.y);

            // Apply arrow_point end offset to the last point.
            // Upstream getLineFunctionsWithOffset: offset applied along the
            // backward direction vector from endpoint toward prev point.
            //   angle = atan2(bdy, bdx)
            //   px += cos(angle) * offset * sign_x
            //   py += sin(angle) * offset * sign_y
            if i == n - 1 && n >= 2 && end_offset > 0.0 {
                let prev = &points[n - 2];
                let bdx = prev.x - px;
                let bdy = prev.y - py;
                let blen = (bdx * bdx + bdy * bdy).sqrt();
                if blen > 0.0 {
                    let cos_a = bdx / blen;
                    let sin_a = bdy / blen;
                    px += end_offset * cos_a;
                    py += end_offset * sin_a;
                }
            }
            // Apply arrow_point start offset to the first point.
            if i == 0 && n >= 2 && start_offset > 0.0 {
                let next = &points[1];
                let fdx = next.x - px;
                let fdy = next.y - py;
                let flen = (fdx * fdx + fdy * fdy).sqrt();
                if flen > 0.0 {
                    let cos_a = fdx / flen;
                    let sin_a = fdy / flen;
                    px += start_offset * cos_a;
                    py += start_offset * sin_a;
                }
            }

            let rx = (px * 1000.0).round() / 1000.0;
            let ry = (py * 1000.0).round() / 1000.0;
            if rx < min_x {
                min_x = rx;
            }
            if ry < min_y {
                min_y = ry;
            }
            if rx > max_x {
                max_x = rx;
            }
            if ry > max_y {
                max_y = ry;
            }
        }
    }

    // Edge labels: foreignObject at (0,0) with measured width/height.
    // The label <g> has a centring transform which is ignored by the shim.
    // Upstream replaces FA icon tokens with <i> elements (zero width under
    // jsdom shim) before measuring, so we apply the same substitution.
    for e in &l.edges {
        let label_text = e.label.as_deref().unwrap_or("");
        let is_empty = label_text.is_empty();
        let (lw, lh) = if is_empty {
            let (_, h) = measure_html_label("X", &font, 200.0, true);
            (0.0, h)
        } else {
            // replace_fa_icons converts `fa:fa-car` → `<i class="fa fa-car"></i>`
            // which measure_html_markup_label strips as a zero-width HTML tag.
            let processed = crate::render::foreign_object::replace_fa_icons(label_text);
            measure_html_markup_label(&processed, &font, 200.0, true)
        };
        // foreignObject x=0, y=0 (no transform applied)
        expand(
            &mut min_x, &mut min_y, &mut max_x, &mut max_y, 0.0, 0.0, lw, lh,
        );
    }

    if !min_x.is_finite() {
        return (0.0, 0.0, 1.0, 1.0);
    }

    let content_w = max_x - min_x;
    let content_h = max_y - min_y;
    let vb_x = min_x - padding;
    let vb_y = min_y - padding;
    let vb_w = (content_w + 2.0 * padding).max(1.0);
    let vb_h = (content_h + 2.0 * padding).max(1.0);

    (vb_x, vb_y, vb_w, vb_h)
}

/// Render a flowchart diagram as SVG.
pub fn render(
    d: &FlowchartDiagram,
    l: &FlowchartLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let padding = l.diagram_padding;

    // Build cluster bounds map for cluster-endpoint edge clipping.
    // Keys are the original cluster IDs; values are the AABB bounds.
    let cluster_bounds: std::collections::HashMap<String, crate::layout::unified::Bounds> = l
        .clusters
        .iter()
        .filter_map(|c| c.bounds.as_ref().map(|b| (c.id.clone(), b.clone())))
        .collect();

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
    // Isolated clusters are rendered inside <g class="nodes"> instead (as
    // inner <g class="root"> groups — see below). Only non-isolated clusters
    // that are NOT descendants of isolated clusters appear here.
    //
    // Cluster rendering order matches upstream: `recursiveRender` walks the
    // dagre graph via `sortNodesByHierarchy` (DFS from `g.children()` in
    // insertion order), and inserts each cluster as it is visited.
    // Upstream `flowDb.getData()` pushes subgraphs in REVERSED declaration
    // order before vertex nodes, so the top-level graph children begin with
    // the last-declared subgraph and end with the first-declared one.
    inner.push_str(&unified_shell::open_layer("clusters"));
    let cluster_render_order = upstream_cluster_render_order(d, l);
    for cluster_id in &cluster_render_order {
        if l.isolated_cluster_ids.contains(cluster_id) {
            continue;
        }
        // Skip clusters that are descendants of any isolated cluster.
        if is_child_of_isolated(Some(cluster_id), l) {
            continue;
        }
        let Some(cluster) = l.clusters.iter().find(|c| &c.id == cluster_id) else {
            continue;
        };
        if let Some(cnode) = l.nodes.iter().find(|n| &n.id == cluster_id && n.is_group) {
            inner.push_str(&render_cluster(cnode, cluster, theme, id));
        }
    }
    inner.push_str(unified_shell::close_layer());

    // Edge paths (outer level — only non-isolated-cluster edges).
    inner.push_str(&unified_shell::open_layer("edgePaths"));
    for (i, e) in l.edges.iter().enumerate() {
        // Skip edges whose endpoints are children of an isolated cluster —
        // those are rendered in the inner root group below.
        let src_isolated = is_child_of_isolated(e.start.as_deref(), l);
        let dst_isolated = is_child_of_isolated(e.end.as_deref(), l);
        if src_isolated || dst_isolated {
            continue;
        }
        inner.push_str(&render_edge_path(e, i, id, &l.aria_kind, &cluster_bounds));
    }
    inner.push_str(unified_shell::close_layer());

    // Edge labels (outer level).
    inner.push_str(&unified_shell::open_layer("edgeLabels"));
    for e in l.edges.iter() {
        let src_isolated = is_child_of_isolated(e.start.as_deref(), l);
        let dst_isolated = is_child_of_isolated(e.end.as_deref(), l);
        if src_isolated || dst_isolated {
            continue;
        }
        inner.push_str(&render_edge_label(e));
    }
    inner.push_str(unified_shell::close_layer());

    // Nodes (outer level).
    // Top-level isolated clusters (those whose parent is not also isolated)
    // are inserted here as inner root groups; regular (non-cluster) nodes
    // whose parent is not any isolated cluster follow.
    inner.push_str(&unified_shell::open_layer("nodes"));

    // Render top-level isolated clusters as inner <g class="root"> groups.
    // "Top-level" = isolated cluster whose parent is NOT also isolated.
    for cluster_id in &l.isolated_cluster_ids {
        if let Some(cnode) = l.nodes.iter().find(|n| &n.id == cluster_id && n.is_group) {
            // Skip if parent is also an isolated cluster (nested, handled recursively).
            let parent_also_isolated = cnode
                .parent_id
                .as_deref()
                .map(|p| l.isolated_cluster_ids.contains(p))
                .unwrap_or(false);
            if parent_also_isolated {
                continue;
            }
            inner.push_str(&render_isolated_cluster_inner_root(cnode, l, theme, id));
        }
    }

    // Render non-isolated-cluster child nodes at the outer level.
    for n in &l.nodes {
        if n.is_group {
            continue;
        }
        // Skip descendants of any isolated cluster.
        if is_child_of_isolated(Some(&n.id), l) {
            continue;
        }
        // Compatibility: also skip if direct parent is isolated.
        if let Some(parent) = n.parent_id.as_deref() {
            if l.isolated_cluster_ids.contains(parent) {
                continue;
            }
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
    let (vb_x, vb_y, vb_w, vb_h) = compute_viewbox(l, padding);

    // ── Assemble final SVG ─────────────────────────────────────────
    let acc_title = d.meta.acc_title.as_deref();
    let acc_descr = d.meta.acc_descr.as_deref();

    let mut out = String::new();
    out.push_str(&unified_shell::open_unified_svg_with_a11y(
        id,
        vb_w,
        (vb_x, vb_y, vb_w, vb_h),
        Some("flowchart"),
        &l.aria_kind,
        acc_descr.is_some(),
        acc_title.is_some(),
    ));

    // Accessibility title/desc elements (if present in diagram source).
    out.push_str(&unified_shell::emit_a11y_elements(id, acc_title, acc_descr));

    // <style> block — shared preamble + flowchart slice + shared tail +
    // classDef rules emitted after :root (upstream `utils.insertClass`).
    out.push_str("<style>");
    out.push_str(&theme_css::base_preamble(id, theme));
    out.push_str(&flowchart_specific_css(id, theme));
    out.push_str(&theme_css::neo_look_block(id, theme));
    out.push_str(&flowchart_class_def_css(id, d));
    out.push_str("</style>");

    out.push_str(&inner);

    out.push_str(&unified_shell::emit_defs_shell(id, true, true));

    // Diagram title — upstream's `utils.insertTitle` appends a centered
    // `<text class="flowchartTitleText">` above the diagram when the
    // frontmatter (or directive) supplied a `title:`. The element is
    // appended to the SVG element AFTER the inner content but BEFORE
    // setupViewPortForSVG measures bbox, so it ends up as a sibling of
    // the seed group (and any drop-shadow defs). Position uses the
    // pre-title bbox's horizontal center, which here equals the
    // viewBox center because the title's text-anchor is `middle` and
    // its width is bounded by the diagram (so it does not push bbox).
    if let Some(title) = d.meta.title.as_deref() {
        if !title.is_empty() {
            let title_top_margin = 25.0_f64;
            let cx = vb_x + vb_w / 2.0;
            out.push_str(&format!(
                r#"<text text-anchor="middle" x="{cx}" y="-{tm}" class="flowchartTitleText">{t}</text>"#,
                cx = cx,
                tm = title_top_margin,
                t = xml_escape(title),
            ));
        }
    }

    out.push_str(unified_shell::close_unified_svg());
    Ok(out)
}

fn render_cluster(
    node: &UNode,
    _cluster: &Cluster,
    _theme: &ThemeVariables,
    svg_id: &str,
) -> String {
    use crate::render::foreign_object::{
        measure_html_label, measure_html_markup_label, HtmlLabelFont,
    };

    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let cx = node.x.unwrap_or(0.0);
    let cy = node.y.unwrap_or(0.0);
    // Absolute rect coordinates — cluster <g> has no transform in upstream SVG.
    let rx = cx - w / 2.0;
    let ry = cy - h / 2.0;
    let base_id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let cluster_id = format!("{svg_id}-{base_id}");
    let label = node.label.clone().unwrap_or_default();

    // data-look attribute from node.look.
    let data_look = match node.look.as_deref() {
        Some(look) if !look.is_empty() => format!(r#" data-look="{}""#, look),
        _ => String::new(),
    };

    // CSS class: "cluster <css_classes>" — look class not prepended here.
    // Upstream emits `class="cluster "` (with trailing space when no extra class).
    let extra_classes = node.css_classes.as_deref().unwrap_or("");
    let class_attr = if extra_classes.is_empty() {
        "cluster ".to_string()
    } else {
        format!("cluster {}", extra_classes)
    };

    // Inline style for rect from css_styles.
    let css_styles = node.css_styles.as_deref().unwrap_or(&[]);
    let rect_style = crate::render::shapes::types::build_inline_style(css_styles);

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{class_attr}" id="{id}"{data_look}>"#,
        class_attr = class_attr,
        id = xml_escape(&cluster_id),
        data_look = data_look,
    ));
    out.push_str(&format!(
        r#"<rect style="{rect_style}" x="{rx}" y="{ry}" width="{w}" height="{h}"></rect>"#,
        rect_style = rect_style,
        rx = fmt_num(rx),
        ry = fmt_num(ry),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    if !label.is_empty() {
        // Measure label for foreignObject dimensions.
        let font = HtmlLabelFont::default();
        let escaped = xml_escape(&label);
        let (lw, lh) = measure_html_markup_label(&escaped, &font, 200.0, true);
        // cluster-label translate: center label horizontally at cx, vertically at top of rect (ry).
        let label_tx = cx - lw / 2.0;
        let label_ty = ry;
        // Label-style (color, font-*) extracted from css_styles — applied to the span.
        let label_style = crate::render::shapes::types::build_label_style(css_styles);
        let span_style_attr = if label_style.is_empty() {
            String::new()
        } else {
            format!(r#" style="{label_style}""#)
        };
        out.push_str(&format!(
            r#"<g class="cluster-label " transform="translate({label_tx}, {label_ty})"><foreignObject width="{lw}" height="{lh}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel "{span_style_attr}><p>{escaped}</p></span></div></foreignObject></g>"#,
            label_tx = fmt_num(label_tx),
            label_ty = fmt_num(label_ty),
            lw = fmt_num(lw),
            lh = fmt_num(lh),
            span_style_attr = span_style_attr,
            escaped = escaped,
        ));
    }
    out.push_str("</g>");
    out
}

/// Return true if `node_id` has any ancestor that is an isolated cluster.
/// Used to skip nodes/edges at the outer render level that belong inside
/// an isolated cluster's inner root group.
fn is_child_of_isolated(node_id: Option<&str>, l: &FlowchartLayout) -> bool {
    let id = match node_id {
        Some(s) => s,
        None => return false,
    };
    // Walk up the parent chain.
    let mut current = id;
    loop {
        if let Some(n) = l.nodes.iter().find(|n| n.id == current) {
            if let Some(parent) = n.parent_id.as_deref() {
                if l.isolated_cluster_ids.contains(parent) {
                    return true;
                }
                current = parent;
            } else {
                return false;
            }
        } else {
            return false;
        }
    }
}

/// Return true if `node_id` is a descendant of `cluster_id` (transitively).
fn is_descendant_of(node_id: &str, cluster_id: &str, l: &FlowchartLayout) -> bool {
    let mut current = node_id;
    loop {
        if let Some(n) = l.nodes.iter().find(|n| n.id == current) {
            if let Some(parent) = n.parent_id.as_deref() {
                if parent == cluster_id {
                    return true;
                }
                current = parent;
            } else {
                return false;
            }
        } else {
            return false;
        }
    }
}

/// Compute the cluster rendering order matching upstream mermaid's
/// `recursiveRender` traversal.
///
/// Upstream `flowDb.getData()` (in flowchart/flowDb.ts) pushes subgraphs in
/// REVERSED declaration order before vertex nodes. The dagre/index.js then
/// runs `data4Layout.nodes.forEach(n => graph.setNode(n.id, ...))`, so the
/// graphlib insertion order is `[reverse_subgraphs..., vertices...]`.
///
/// During rendering, `sortNodesByHierarchy(graph)` walks `graph.children()`
/// (top-level nodes in insertion order) DFS, pulling each node's children
/// (also in insertion order). The cluster (subgraph) ids encountered during
/// this walk define the order in which `<g class="cluster">` elements are
/// emitted.
///
/// Mirroring that traversal here is what gives us byte-exact cluster order
/// without disturbing dagre's geometry pass.
fn upstream_cluster_render_order(d: &FlowchartDiagram, l: &FlowchartLayout) -> Vec<String> {
    use std::collections::HashSet;

    // Build the upstream-style insertion list:
    //   reversed subgraphs (clusters), then vertices in declaration order.
    // We restrict to ids that exist in `l.nodes` to stay aligned with the
    // post-layout state.
    let layout_ids: HashSet<&str> = l.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut insertion: Vec<String> = Vec::new();
    for sg in d.subgraphs.iter().rev() {
        if layout_ids.contains(sg.id.as_str()) {
            insertion.push(sg.id.clone());
        }
    }
    let subgraph_ids: HashSet<&str> = d.subgraphs.iter().map(|s| s.id.as_str()).collect();
    for v in &d.vertices {
        // Vertices with the same id as a subgraph are cluster references — skip
        // (matches the filter in `flowchart::build_layout_data`).
        if subgraph_ids.contains(v.id.as_str()) {
            continue;
        }
        if layout_ids.contains(v.id.as_str()) {
            insertion.push(v.id.clone());
        }
    }

    // Map id -> insertion index (used for ordering siblings).
    let pos: std::collections::HashMap<&str, usize> = insertion
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    // Build child lists per parent in insertion order.
    let mut children_of: std::collections::HashMap<Option<String>, Vec<String>> =
        std::collections::HashMap::new();
    let mut sorted_nodes: Vec<&UNode> = l.nodes.iter().collect();
    sorted_nodes.sort_by_key(|n| pos.get(n.id.as_str()).copied().unwrap_or(usize::MAX));
    for n in sorted_nodes {
        children_of
            .entry(n.parent_id.clone())
            .or_default()
            .push(n.id.clone());
    }

    // DFS from top-level (parent = None), collecting cluster ids.
    let mut order: Vec<String> = Vec::new();
    let mut stack: Vec<String> = children_of
        .get(&None)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .rev()
        .collect();
    while let Some(id) = stack.pop() {
        // Emit this id if it is a cluster (group).
        if l.nodes.iter().any(|n| n.id == id && n.is_group) {
            order.push(id.clone());
        }
        if let Some(kids) = children_of.get(&Some(id.clone())) {
            for k in kids.iter().rev() {
                stack.push(k.clone());
            }
        }
    }
    order
}

/// Render an isolated cluster as an inner `<g class="root" transform="...">` group.
///
/// Upstream mermaid's `recursiveRender` wraps an isolated cluster's inner
/// layout in a `<g class="root">` placed in the outer `<g class="nodes">` section.
/// The `transform="translate(tx, ty)"` is pre-computed by the layout engine
/// (`dagre_bridge`) using the `positionNode` formula:
///
///   tx = outer_x + diff - bbox_w/2   (diff = -padding = -8)
///   ty = outer_y - bbox_h/2 - padding
///
/// The pre-computed values are stored in `cnode.extra["outer_tx"]` and
/// `cnode.extra["outer_ty"]`. When absent (no outer dagre pass, e.g. nested
/// isolated clusters handled by parent), fall back to the classic formula
/// using inner dagre coords: `tx = cx - padding - cluster_w/2`.
///
/// `cnode.x/y/width/height` always hold the INNER dagre coords (cluster center
/// and cluster rect dimensions) used by `render_cluster` for the cluster rect.
///
/// This function is recursive: isolated sub-clusters within `cnode` are
/// themselves rendered as nested inner root groups.
fn render_isolated_cluster_inner_root(
    cnode: &UNode,
    l: &FlowchartLayout,
    theme: &ThemeVariables,
    svg_id: &str,
) -> String {
    // Retrieve pre-computed outer translate from the layout engine.
    let tx = cnode
        .extra
        .get("outer_tx")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or_else(|| {
            // Fallback: classic formula using inner dagre coords.
            let cx = cnode.x.unwrap_or(0.0);
            let w = cnode.width.unwrap_or(0.0);
            let padding = cnode.padding.unwrap_or(8.0);
            cx - padding - w / 2.0
        });
    let ty = cnode
        .extra
        .get("outer_ty")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or_else(|| {
            let cy = cnode.y.unwrap_or(0.0);
            let h = cnode.height.unwrap_or(0.0);
            let padding = cnode.padding.unwrap_or(8.0);
            cy - h / 2.0 - padding
        });

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="root" transform="translate({tx}, {ty})">"#,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));

    // Inner <g class="clusters"> — this cluster's own rect plus
    // child cluster rects that are NOT themselves isolated.
    out.push_str(&unified_shell::open_layer("clusters"));
    // This cluster's own rect.
    let dummy_cluster = crate::layout::unified::Cluster {
        id: cnode.id.clone(),
        representative: None,
        bounds: None,
    };
    out.push_str(&render_cluster(cnode, &dummy_cluster, theme, svg_id));
    // Non-isolated child cluster rects (direct children only).
    for n in &l.nodes {
        if !n.is_group {
            continue;
        }
        if n.parent_id.as_deref() != Some(cnode.id.as_str()) {
            continue;
        }
        if l.isolated_cluster_ids.contains(&n.id) {
            continue; // isolated sub-clusters get their own inner root
        }
        let dummy = crate::layout::unified::Cluster {
            id: n.id.clone(),
            representative: None,
            bounds: None,
        };
        out.push_str(&render_cluster(n, &dummy, theme, svg_id));
    }
    out.push_str(unified_shell::close_layer());

    // Inner <g class="edgePaths"> — edges between descendants of this
    // cluster, excluding edges between descendants of isolated sub-clusters.
    out.push_str(&unified_shell::open_layer("edgePaths"));
    for (i, e) in l.edges.iter().enumerate() {
        let src = match e.start.as_deref() {
            Some(s) => s,
            None => continue,
        };
        let dst = match e.end.as_deref() {
            Some(s) => s,
            None => continue,
        };
        // Both endpoints must be descendants of this cluster.
        if !is_descendant_of(src, &cnode.id, l) || !is_descendant_of(dst, &cnode.id, l) {
            continue;
        }
        // Skip if BOTH endpoints are inside the same isolated sub-cluster
        // (those edges are handled in the sub-cluster's inner root).
        // cnode.id is itself isolated, so exclude it from the "sub-isolated" check.
        let src_in_sub_iso = l
            .nodes
            .iter()
            .find(|n| n.id == src)
            .and_then(|n| n.parent_id.as_deref())
            .map(|p| p != cnode.id.as_str() && l.isolated_cluster_ids.contains(p))
            .unwrap_or(false);
        let dst_in_sub_iso = l
            .nodes
            .iter()
            .find(|n| n.id == dst)
            .and_then(|n| n.parent_id.as_deref())
            .map(|p| p != cnode.id.as_str() && l.isolated_cluster_ids.contains(p))
            .unwrap_or(false);
        if src_in_sub_iso || dst_in_sub_iso {
            continue;
        }
        out.push_str(&render_edge_path(
            e,
            i,
            svg_id,
            &l.aria_kind,
            &std::collections::HashMap::new(),
        ));
    }
    out.push_str(unified_shell::close_layer());

    // Inner <g class="edgeLabels">.
    out.push_str(&unified_shell::open_layer("edgeLabels"));
    for e in l.edges.iter() {
        let src = match e.start.as_deref() {
            Some(s) => s,
            None => continue,
        };
        let dst = match e.end.as_deref() {
            Some(s) => s,
            None => continue,
        };
        if !is_descendant_of(src, &cnode.id, l) || !is_descendant_of(dst, &cnode.id, l) {
            continue;
        }
        let src_in_sub_iso = l
            .nodes
            .iter()
            .find(|n| n.id == src)
            .and_then(|n| n.parent_id.as_deref())
            .map(|p| p != cnode.id.as_str() && l.isolated_cluster_ids.contains(p))
            .unwrap_or(false);
        let dst_in_sub_iso = l
            .nodes
            .iter()
            .find(|n| n.id == dst)
            .and_then(|n| n.parent_id.as_deref())
            .map(|p| p != cnode.id.as_str() && l.isolated_cluster_ids.contains(p))
            .unwrap_or(false);
        if src_in_sub_iso || dst_in_sub_iso {
            continue;
        }
        out.push_str(&render_edge_label(e));
    }
    out.push_str(unified_shell::close_layer());

    // Inner <g class="nodes">:
    // 1. Isolated sub-clusters as nested inner roots.
    // 2. Direct leaf nodes.
    out.push_str(&unified_shell::open_layer("nodes"));

    // Render isolated sub-clusters (those whose parent is cnode.id and
    // that are in isolated_cluster_ids).
    for n in &l.nodes {
        if !n.is_group {
            continue;
        }
        if n.parent_id.as_deref() != Some(cnode.id.as_str()) {
            continue;
        }
        if !l.isolated_cluster_ids.contains(&n.id) {
            continue;
        }
        out.push_str(&render_isolated_cluster_inner_root(n, l, theme, svg_id));
    }

    // Render direct leaf nodes.
    for n in &l.nodes {
        if n.is_group {
            continue;
        }
        // Only direct children of this cluster, or children of non-isolated sub-clusters.
        // We emit nodes whose parent is in the subtree of this cluster but NOT
        // in any isolated sub-cluster.
        if !is_descendant_of(&n.id, &cnode.id, l) {
            continue;
        }
        // Skip if the node is inside an isolated sub-cluster of cnode
        // (those are handled recursively in sub-cluster's inner root).
        // Note: cnode.id itself is in isolated_cluster_ids, so we must
        // exclude it from the "sub-isolated" check.
        let in_sub_isolated = n
            .parent_id
            .as_deref()
            .map(|p| p != cnode.id.as_str() && l.isolated_cluster_ids.contains(p))
            .unwrap_or(false);
        if in_sub_isolated {
            continue;
        }
        let mut prefixed = n.clone();
        if let Some(did) = &prefixed.dom_id {
            prefixed.dom_id = Some(format!("{svg_id}-{did}", svg_id = svg_id));
        }
        let shape_id = prefixed.shape.clone().unwrap_or_else(|| "rect".to_string());
        match crate::render::shapes::draw(&shape_id, &prefixed, theme) {
            Ok(svg) => out.push_str(&svg),
            Err(_) => {
                if let Ok(svg) = crate::render::shapes::draw("rect", &prefixed, theme) {
                    out.push_str(&svg);
                }
            }
        }
    }
    out.push_str(unified_shell::close_layer());

    out.push_str("</g>"); // close inner root
    out
}

/// Return true if `node_id`'s parent is `cluster_id`.
fn node_parent_is(node_id: Option<&str>, cluster_id: &str, l: &FlowchartLayout) -> bool {
    let id = match node_id {
        Some(s) => s,
        None => return false,
    };
    l.nodes
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.parent_id.as_deref())
        .map(|p| p == cluster_id)
        .unwrap_or(false)
}

/// Apply the upstream `markerOffsets` visual offset to path endpoints.
///
/// Upstream's `getLineFunctionsWithOffset` adjusts the first/last path
/// point by `markerOffset * sin(angle)` so the arrowhead doesn't overlap
/// the node boundary. `arrow_point` has offset 4, `arrow_cross/circle` 12.5.
fn apply_marker_offsets(pts: &mut Vec<Point>, arrow_end: &str, arrow_start: &str) {
    fn marker_offset_for(arrow: &str) -> Option<f64> {
        match arrow {
            "arrow_point" => Some(4.0),
            "arrow_cross" | "arrow_circle" => Some(12.5),
            _ => None,
        }
    }

    let n = pts.len();
    if n < 2 {
        return;
    }

    // End offset: applied to last point.
    // Upstream calculateDeltaAndAngle(last, prev): deltaX = prev.x - last.x.
    // x_offset = mo * cos(atan(dy/dx)) * sign(deltaX) = mo * deltaX/len
    // y_offset = mo * |sin(atan(dy/dx))| * sign(deltaY) = mo * |deltaY|/len * sign(deltaY)
    if let Some(mo) = marker_offset_for(arrow_end) {
        let last = pts[n - 1];
        let prev = pts[n - 2];
        let dx = prev.x - last.x;
        let dy = prev.y - last.y;
        let blen = (dx * dx + dy * dy).sqrt();
        if blen > 0.0 {
            // x: mo * cos(atan(dy/dx)) * sign(dx) = mo * |dx|/len * sign(dx) = mo * dx/len
            pts[n - 1].x += mo * dx / blen;
            // y: mo * |sin| * sign(dy) = mo * |dy|/len * sign(dy) = mo * |dy|/len * (dy/|dy|)
            pts[n - 1].y += mo * dy.abs() / blen * if dy >= 0.0 { 1.0 } else { -1.0 };
        }
    }

    // Start offset: applied to first point.
    // calculateDeltaAndAngle(first, second): deltaX = second.x - first.x.
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

/// Mirror of upstream mermaid's `outsideNode(node, point)`. The
/// `boundaryNode` is treated as a centred AABB — its origin is the
/// rectangle centre `(node.x, node.y)` and the half-extents are
/// `node.width / 2` and `node.height / 2`. Returns `true` when the
/// point lies strictly outside (or exactly on) the rectangle.
fn outside_node(b: &crate::layout::unified::Bounds, p: Point) -> bool {
    // Bounds in our type system stores (x_left, y_top, w, h). Convert to
    // the centre-anchored form used by upstream `outsideNode`.
    let cx = b.x + b.width / 2.0;
    let cy = b.y + b.height / 2.0;
    let dx = (p.x - cx).abs();
    let dy = (p.y - cy).abs();
    let w = b.width / 2.0;
    let h = b.height / 2.0;
    dx >= w || dy >= h
}

/// Mirror of upstream mermaid's `intersection(boundaryNode, outsidePoint, insidePoint)`.
/// Returns the point on the rectangle's border crossed by the segment
/// between `outside` (must be outside the AABB) and `inside` (must be
/// inside the AABB).
///
/// The original implementation lives in `dagre-wrapper/edges.js`; the
/// algorithm is a custom box-line solver, NOT a generic Liang–Barsky
/// segment-clip. Reproducing it byte-for-byte is required for parity.
fn intersection_box_line(
    b: &crate::layout::unified::Bounds,
    outside: Point,
    inside: Point,
) -> Point {
    let node_x = b.x + b.width / 2.0;
    let node_y = b.y + b.height / 2.0;
    let w = b.width / 2.0;
    let h = b.height / 2.0;

    let dx = (node_x - inside.x).abs();
    // Pre-compute `r` for the side branch fallback (only used in `else` arm).
    let _r_side = if inside.x < outside.x { w - dx } else { w + dx };
    let q_total = (outside.y - inside.y).abs();
    let r_total = (outside.x - inside.x).abs();

    if (node_y - outside.y).abs() * w > (node_x - outside.x).abs() * h {
        // Top/bottom branch.
        let q = if inside.y < outside.y {
            outside.y - h - node_y
        } else {
            node_y - h - outside.y
        };
        let r = r_total * q / q_total;
        let mut res_x = if inside.x < outside.x {
            inside.x + r
        } else {
            inside.x - r_total + r
        };
        let mut res_y = if inside.y < outside.y {
            inside.y + q_total - q
        } else {
            inside.y - q_total + q
        };
        // Edge-case overrides in upstream.
        if r == 0.0 {
            res_x = outside.x;
            res_y = outside.y;
        }
        if r_total == 0.0 {
            res_x = outside.x;
        }
        if q_total == 0.0 {
            res_y = outside.y;
        }
        Point { x: res_x, y: res_y }
    } else {
        // Side branch.
        let r = if inside.x < outside.x {
            outside.x - w - node_x
        } else {
            node_x - w - outside.x
        };
        let q = q_total * r / r_total;
        let mut res_x = if inside.x < outside.x {
            inside.x + r_total - r
        } else {
            inside.x - r_total + r
        };
        let mut res_y = if inside.y < outside.y {
            inside.y + q
        } else {
            inside.y - q
        };
        if r == 0.0 {
            res_x = outside.x;
            res_y = outside.y;
        }
        if r_total == 0.0 {
            res_x = outside.x;
        }
        if q_total == 0.0 {
            res_y = outside.y;
        }
        Point { x: res_x, y: res_y }
    }
}

/// Mirror of upstream mermaid's `cutPathAtIntersect(_points, boundaryNode)`
/// from `dagre-wrapper/edges.js`. Walks the polyline from the first
/// point; when a point transitions from outside the boundary node to
/// inside, the previous outside point is replaced by the segment-vs-
/// rectangle intersection as produced by `intersection_box_line`.
/// Subsequent inside points are dropped.
fn cut_path_at_intersect(
    points: &[Point],
    boundary: &crate::layout::unified::Bounds,
) -> Vec<Point> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    let mut last_outside = points[0];
    let mut is_inside = false;
    for &p in points {
        let outside = outside_node(boundary, p);
        if !outside && !is_inside {
            // Outside → inside transition: append the boundary intersection.
            let inter = intersection_box_line(boundary, last_outside, p);
            // Upstream skips duplicates by exact-equality on (x, y).
            if !out
                .iter()
                .any(|q| (q.x - inter.x).abs() == 0.0 && (q.y - inter.y).abs() == 0.0)
            {
                out.push(inter);
            }
            is_inside = true;
        } else if outside {
            last_outside = p;
            if !is_inside {
                out.push(p);
            }
        }
        // (`!outside && is_inside`: stay-inside — drop the point entirely.)
    }
    out
}

fn render_edge_path(
    e: &UEdge,
    _index: usize,
    svg_id: &str,
    aria_kind: &str,
    cluster_bounds: &std::collections::HashMap<String, crate::layout::unified::Bounds>,
) -> String {
    // `pts` used for data-points (original dagre coordinates, matching upstream
    // `pointsStr` = btoa(JSON.stringify(points)) which precedes the offset step).
    let pts_raw: Vec<Point> = e
        .points
        .as_ref()
        .map(|v| v.iter().map(|p| Point { x: p.x, y: p.y }).collect())
        .unwrap_or_default();
    if pts_raw.is_empty() {
        return String::new();
    }
    // Clip rendered path at cluster boundaries when the original edge endpoint
    // was a cluster. The `data-points` attribute keeps the full path (pts_raw),
    // but the visual `d=` path is clipped to the cluster border.
    // Upstream renders the edge path stopping at the cluster rect boundary —
    // and crucially performs the clip BEFORE applying marker visual offsets,
    // so the arrow head sits inside the cluster border by `markerOffset`px.
    let mut pts = pts_raw.clone();
    let orig_dst = e
        .extra
        .get("orig_end")
        .map(|s| s.as_str())
        .unwrap_or_else(|| e.end.as_deref().unwrap_or(""));
    let orig_src = e
        .extra
        .get("orig_start")
        .map(|s| s.as_str())
        .unwrap_or_else(|| e.start.as_deref().unwrap_or(""));
    if let Some(bounds) = cluster_bounds.get(orig_dst) {
        pts = cut_path_at_intersect(&pts, bounds);
    }
    if let Some(bounds) = cluster_bounds.get(orig_src) {
        pts.reverse();
        pts = cut_path_at_intersect(&pts, bounds);
        pts.reverse();
    }

    // Apply marker visual offsets to get the rendered path points.
    let arrow_end = e.arrow_type_end.as_deref().unwrap_or("none");
    let arrow_start = e.arrow_type_start.as_deref().unwrap_or("none");
    apply_marker_offsets(&mut pts, arrow_end, arrow_start);

    // Build `d=` via the curve configured on this edge (offset-adjusted pts).
    let curve = e.curve.as_deref().unwrap_or("basis");
    let ctype = edges::CurveType::parse(curve).unwrap_or(edges::CurveType::Basis);
    let d_attr = edges::build_path(&pts, ctype);

    let thickness = e.thickness.as_deref().unwrap_or("normal");
    let pattern = e.pattern.as_deref().unwrap_or("solid");
    // Upstream class format: `" {strokeClasses} {edge.classes}"` where
    //   strokeClasses = "edge-thickness-{t} edge-pattern-{p}" (from edge's stroke type)
    //   edge.classes   = "edge-thickness-normal edge-pattern-solid flowchart-link" (always,
    //                    unless invisible).
    // Leading space is intentional (upstream emits `" " + strokeClasses + edge.classes`).
    let class_attr = if thickness == "invisible" {
        String::new()
    } else {
        format!(
            " edge-thickness-{thickness} edge-pattern-{pattern} edge-thickness-normal edge-pattern-solid flowchart-link"
        )
    };

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
    // Upstream encodes raw dagre points BEFORE marker offset adjustments.
    let data_points_b64 = {
        let mut json = String::from("[");
        for (i, p) in pts_raw.iter().enumerate() {
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
        Some("none") | None => {
            // No arrowhead (arrow_open / open edges like `---`): no marker.
            String::new()
        }
        _ => {
            // Default arrow (point) — upstream emits marker-end for
            // arrow_point, arrow, etc.
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
        r#"<path d="{d}" id="{eid}" class="{cls}" style="{st}" data-edge="true" data-et="edge" data-id="{did}" data-points="{b64}" data-look="classic"{ms}{me}></path>"#,
        d = d_attr,
        eid = edge_id,
        cls = class_attr,
        st = style_val,
        did = e.id,
        b64 = data_points_b64,
        ms = marker_start,
        me = marker_end,
    )
}

fn render_edge_label(e: &UEdge) -> String {
    use crate::render::foreign_object::{
        measure_html_label, measure_html_markup_label, render_edge_label as fo_edge,
        replace_fa_icons, HtmlLabelFont, LabelOpts,
    };
    let label_text = e.label.clone().unwrap_or_default();
    // Apply FA icon substitution (fa:fa-car → <i class="fa fa-car"></i>) before
    // measuring, matching upstream's createText path. The <i> element contributes
    // zero width under the jsdom shim.
    let processed = replace_fa_icons(&label_text);
    let is_empty = processed.is_empty();
    // Upstream always measures the label height (even when empty),
    // using the font's line-height. For empty labels, width=0 but
    // height is still the font's line-height.
    let (w, h) = if is_empty {
        let (_, lh) = measure_html_label("X", &HtmlLabelFont::default(), 200.0, true);
        (0.0, lh)
    } else {
        measure_html_markup_label(&processed, &HtmlLabelFont::default(), 200.0, true)
    };
    let lx = e.label_x.unwrap_or(0.0);
    let ly = e.label_y.unwrap_or(0.0);
    let opts = LabelOpts {
        data_id: Some(&e.id),
        group_style: None,
        wrap_in_p: !is_empty,
        ..LabelOpts::default()
    };
    fo_edge(&processed, lx, ly, w, h, opts)
}

/// Build the flowchart-specific CSS slice — a complete port of upstream's
/// `styles.ts` → `getStyles()` output, scoped to `#<id>`. This replaces
/// the former minimal `build_css()` and emits every rule the upstream
/// flowchart CSS template produces after stylis minification.
///
/// The caller sandwiches this between [`theme_css::base_preamble`] and
/// [`theme_css::neo_look_block`] inside the `<style>` block.
/// Generate CSS rules for `classDef` directives.
///
/// Mirrors upstream `utils.insertClass(svg, classDef, diagramId)` which calls
/// `createCssStyles` to produce:
/// - `#id .name>*{<all styles>!important;}`
/// - `#id .name span{<all styles>!important;}`
/// - `#id .name tspan{<color→fill styles>!important;}`
fn flowchart_class_def_css(id: &str, d: &FlowchartDiagram) -> String {
    let mut out = String::new();
    for def in &d.class_defs {
        // Build the "all styles" block.
        let mut all_props: Vec<String> = Vec::new();
        let mut text_props: Vec<String> = Vec::new();
        for style in &def.styles {
            let style = style.trim().trim_end_matches(';');
            if style.is_empty() {
                continue;
            }
            if let Some(colon) = style.find(':') {
                let key = style[..colon].trim();
                let val = style[colon + 1..].trim();
                all_props.push(format!("{}:{}!important;", key, val));
                // tspan only: properties whose key contains "color"
                if key.contains("color") {
                    // upstream: replace "fill" → "bgFill" then "color" → "fill"
                    let new_key = key.replace("fill", "bgFill").replace("color", "fill");
                    text_props.push(format!("{}:{}!important;", new_key, val));
                }
            } else {
                all_props.push(format!("{}!important;", style));
            }
        }
        if all_props.is_empty() {
            continue;
        }
        let all_css: String = all_props.join("");
        // >* and span rules use all styles
        out.push_str(&format!(
            "#{id} .{name}>*{{{css}}}",
            name = def.name,
            css = all_css
        ));
        out.push_str(&format!(
            "#{id} .{name} span{{{css}}}",
            name = def.name,
            css = all_css
        ));
        // tspan rule uses only text (color) styles
        if !text_props.is_empty() {
            let text_css: String = text_props.join("");
            out.push_str(&format!(
                "#{id} .{name} tspan{{{css}}}",
                name = def.name,
                css = text_css
            ));
        }
    }
    out
}

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
    let arrowhead_color = theme.arrowhead_color.as_deref().unwrap_or("#333333");
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
    css.push_str(&format!("#{id} .node.clickable{{cursor:pointer;}}",));

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
    css.push_str(&format!("#{id} rect.text{{fill:none;stroke-width:0;}}",));

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
        // Upstream uses "flowchart-v2" for all non-ELK flowchart/graph diagrams.
        let src = "graph LR\nA-->B\n";
        let d = fcp::parse(src).unwrap();
        let th = theme::get_theme("default");
        let l = fcl::layout(&d, &th).unwrap();
        let svg = render(&d, &l, &th, "t").unwrap();
        assert!(svg.contains(r#"aria-roledescription="flowchart-v2""#));
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
        assert!(
            css.contains("#test .cluster-label text{"),
            "missing .cluster-label text"
        );
        assert!(
            css.contains("#test .cluster-label span{"),
            "missing .cluster-label span"
        );
        assert!(
            css.contains("#test .cluster-label span p{"),
            "missing .cluster-label span p"
        );
        assert!(
            css.contains("#test .label text,#test span{"),
            "missing .label text,span"
        );
        assert!(css.contains("#test .node rect,"), "missing .node rect");
        assert!(
            css.contains("#test .arrowheadPath{"),
            "missing .arrowheadPath"
        );
        assert!(
            css.contains("#test .edgePath .path{"),
            "missing .edgePath .path"
        );
        assert!(
            css.contains("#test .flowchart-link{"),
            "missing .flowchart-link"
        );
        assert!(css.contains("#test .edgeLabel{"), "missing .edgeLabel");
        assert!(css.contains("#test .edgeLabel p{"), "missing .edgeLabel p");
        assert!(
            css.contains("#test .edgeLabel rect{"),
            "missing .edgeLabel rect"
        );
        assert!(css.contains("#test .labelBkg{"), "missing .labelBkg");
        assert!(
            css.contains("#test .cluster rect{"),
            "missing .cluster rect"
        );
        assert!(
            css.contains("#test .cluster text{"),
            "missing .cluster text"
        );
        assert!(
            css.contains("#test .cluster span{"),
            "missing .cluster span"
        );
        assert!(
            css.contains("#test div.mermaidTooltip{"),
            "missing div.mermaidTooltip"
        );
        assert!(
            css.contains("#test .flowchartTitleText{"),
            "missing .flowchartTitleText"
        );
        assert!(css.contains("#test rect.text{"), "missing rect.text");
        assert!(
            css.contains("#test .icon-shape,#test .image-shape{"),
            "missing icon/image-shape"
        );
        assert!(css.contains("#test .label-icon{"), "missing .label-icon");
        assert!(
            css.contains("#test .node .label-icon path{"),
            "missing .node .label-icon path"
        );
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
        let source = std::fs::read_to_string(base.join(format!("tests/{}.mmd", rel))).unwrap();
        let expected =
            std::fs::read_to_string(base.join(format!("tests/reference/{}.svg", rel))).unwrap();
        let d = fcp::parse(&source).unwrap();
        let theme = theme::get_theme("default");
        let l = fcl::layout(&d, &theme).unwrap();
        let id = id_for_fixture(rel);
        let got = super::render(&d, &l, &theme, &id).unwrap();
        let a = got.as_bytes();
        let b = expected.as_bytes();
        let n = a.len().min(b.len());
        let mut i = 0;
        while i < n && a[i] == b[i] {
            i += 1;
        }
        if i >= n && a.len() == b.len() {
            eprintln!("BYTE EXACT!");
            return;
        }
        let ctx_lo = i.saturating_sub(80);
        let ctx_hi_a = (i + 200).min(a.len());
        let ctx_hi_b = (i + 200).min(b.len());
        eprintln!("Diverge at byte {} (got={}, want={})", i, a.len(), b.len());
        eprintln!(
            "got [{}..]: {}",
            ctx_lo,
            String::from_utf8_lossy(&a[ctx_lo..ctx_hi_a])
        );
        eprintln!(
            "want[{}..]: {}",
            ctx_lo,
            String::from_utf8_lossy(&b[ctx_lo..ctx_hi_b])
        );
    }
}
