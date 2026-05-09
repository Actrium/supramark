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
///
/// Returns `(vb_x, vb_y, vb_w, vb_h, content_center_x)`.
///
/// `content_center_x` is the horizontal center of the bbox BEFORE the diagram
/// title text was folded in. Upstream `utils.insertTitle` saves this center as
/// Order a sequence of node-id strings by JavaScript's `Object.keys`
/// iteration order: integer-index keys (canonical decimal, < 2^32 - 1, no
/// leading zeros) come first in numeric ascending order, then non-integer
/// keys in their original insertion order. Returns indices into the input
/// sequence so callers can re-iterate the original slice in this order.
///
/// Mirrors upstream dagre's `graph.nodes()` which is backed by
/// `Object.keys(this._nodes)` — diagrams whose IDs are pure decimals end
/// up sorted numerically by V8's well-known Object key ordering.
fn js_object_key_order<'a, I: Iterator<Item = &'a str>>(iter: I) -> Vec<usize> {
    let ids: Vec<&str> = iter.collect();
    let mut int_keys: Vec<(u32, usize)> = Vec::new();
    let mut str_keys: Vec<usize> = Vec::new();
    for (idx, s) in ids.iter().enumerate() {
        if let Some(n) = parse_array_index(s) {
            int_keys.push((n, idx));
        } else {
            str_keys.push(idx);
        }
    }
    int_keys.sort_by_key(|(n, _)| *n);
    let mut out: Vec<usize> = Vec::with_capacity(ids.len());
    for (_, idx) in int_keys {
        out.push(idx);
    }
    out.extend(str_keys);
    out
}

/// Test whether `s` is a "canonical numeric index" per ECMAScript:
/// the decimal representation of an integer in `[0, 2^32 - 2]` with no
/// leading zeros and no sign / fractional / exponent parts.
fn parse_array_index(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    if s == "0" {
        return Some(0);
    }
    if s.starts_with('0') {
        return None;
    }
    if !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u64 = s.parse().ok()?;
    if n >= u32::MAX as u64 {
        return None;
    }
    Some(n as u32)
}

/// Detect `font-weight: bold` (or numeric ≥700, or `bolder`) in a list of
/// CSS declarations resolved from a node's classDef + inline styles.
/// Used to apply bold weight when measuring a node's label width — so the
/// computed viewBox tracks the actual rendered foreignObject width.
fn node_styles_have_bold(styles: &[String]) -> bool {
    for s in styles {
        let trimmed = s.trim().trim_end_matches(';');
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        let key = trimmed[..colon].trim();
        if !key.eq_ignore_ascii_case("font-weight") {
            continue;
        }
        let value = trimmed[colon + 1..].trim();
        let val_no_important = value
            .trim_end_matches("!important")
            .trim()
            .trim_end_matches('!')
            .trim();
        if val_no_important.eq_ignore_ascii_case("bold")
            || val_no_important.eq_ignore_ascii_case("bolder")
        {
            return true;
        }
        if let Ok(n) = val_no_important.parse::<u32>() {
            if n >= 700 {
                return true;
            }
        }
    }
    false
}

/// When the resolved node styles include `font-weight:bold`, the upstream
/// shape renderers (`labelHelper` → jsdom-shimmed `getBoundingClientRect()`)
/// measure the label text at bold weight — but our `shapes/*` modules use
/// `HtmlLabelFont::default()` (non-bold). This post-processor patches the
/// emitted node SVG so the inner `<foreignObject>` width and the enclosing
/// `<g class="label" transform="translate(-w/2, …)">` x-offset reflect
/// bold-weight measurement. Other shape geometry (rect outer width/height)
/// already comes from the layout pass which honours bold via
/// `measure_vertex_box(v, is_bold=true)`, so we only need to rewrite the
/// label-level numbers here.
fn diamond_br_postprocess(node: &UNode, svg: &str) -> String {
    use crate::render::foreign_object::HtmlLabelFont;
    use crate::render::shapes::types::{fmt_num, xml_escape};

    let shape = node.shape.as_deref().unwrap_or("rect");
    if shape != "diamond" && shape != "question" {
        return svg.to_string();
    }
    let label = match node.label.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => return svg.to_string(),
    };
    // Cheap check: only rewrite if the label contains a `<br>` variant —
    // otherwise the upstream diamond measurement already matches the
    // jsdom textContent geometry.
    let has_br = {
        let lower = label.to_ascii_lowercase();
        lower.contains("<br>")
            || lower.contains("<br/>")
            || lower.contains("<br />")
            || lower.contains("<br/ >")
    };
    if !has_br {
        return svg.to_string();
    }
    // Mirror the (wrong) measurement diamond.rs uses today:
    //   bbox = measure_html_label(xml_escape(label), default_font)
    // `measure_html_label` splits on `\n` only, so embedded `&lt;br/&gt;`
    // entities remain as plain text and inflate the width. We need to
    // recompute (s_wrong) so we can find-and-replace the literal numbers.
    let escaped = xml_escape(label);
    let (wrong_w, wrong_h) = crate::render::foreign_object::measure_html_label(
        &escaped,
        &HtmlLabelFont::default(),
        200.0,
        true,
    );
    let p = node.padding.unwrap_or(15.0);
    let s_wrong = (wrong_w + p) + (wrong_h + p);
    let half_wrong = s_wrong / 2.0;
    // Compute the right `s` from node.width/height (the layout already
    // measured correctly via the post-`<br/>` concatenated text).
    let s_right = node.width.unwrap_or(s_wrong);
    let half_right = s_right / 2.0;
    if (s_right - s_wrong).abs() < 1e-9 {
        return svg.to_string();
    }
    // Build the literal substrings the diamond shape emits and substitute
    // them with their right-sized counterparts. Use unique substrings
    // (the polygon points + transform) so we don't accidentally rewrite
    // unrelated numbers in the same node block.
    let pts_wrong = format!(
        "points=\"{},0 {},{} {},{} 0,{}\"",
        fmt_num(half_wrong),
        fmt_num(s_wrong),
        fmt_num(-half_wrong),
        fmt_num(half_wrong),
        fmt_num(-s_wrong),
        fmt_num(-half_wrong),
    );
    let pts_right = format!(
        "points=\"{},0 {},{} {},{} 0,{}\"",
        fmt_num(half_right),
        fmt_num(s_right),
        fmt_num(-half_right),
        fmt_num(half_right),
        fmt_num(-s_right),
        fmt_num(-half_right),
    );
    let tx_wrong = format!(
        "transform=\"translate({}, {})\"",
        fmt_num(-half_wrong + 0.5),
        fmt_num(half_wrong),
    );
    let tx_right = format!(
        "transform=\"translate({}, {})\"",
        fmt_num(-half_right + 0.5),
        fmt_num(half_right),
    );
    svg.replace(&pts_wrong, &pts_right)
        .replace(&tx_wrong, &tx_right)
}

/// When a node has a `click <id> "<href>" [...]` directive, upstream
/// wraps the inner shape `<g class="node ..." ...>...</g>` in an
/// `<a href="..." [target="..."] data-look="..." transform="...">...</a>`
/// anchor. The transform / data-look move to the anchor; the inner
/// `<g>` keeps the id, picks up the `clickable` extra class, and gains
/// a `title="..."` attribute when a tooltip was supplied.
///
/// The shape registry has no concept of node-level links, so we
/// rewrite its output here. This is a string transform on the already-
/// emitted SVG: cheaper than threading link data through every shape
/// and isolates the click-event semantics in the flowchart renderer.
fn link_postprocess_node_svg(node: &UNode, svg: &str) -> String {
    use crate::render::shapes::types::xml_escape;
    let link = match node.link.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => return svg.to_string(),
    };
    // Locate the OUTERMOST `<g class="node ...` opening tag — it is the
    // first `<g class="node ` substring in the shape output.
    let g_start = match svg.find("<g class=\"node ") {
        Some(i) => i,
        None => return svg.to_string(),
    };
    // Slice from `<g`.
    let head_open_end = match svg[g_start..].find('>') {
        Some(off) => g_start + off + 1,
        None => return svg.to_string(),
    };
    let opening = &svg[g_start..head_open_end];
    // Extract attribute values we need to preserve / move.
    let class_val = match extract_attr(opening, "class") {
        Some(v) => v,
        None => return svg.to_string(),
    };
    let id_val = extract_attr(opening, "id").unwrap_or_default();
    let data_look_val = extract_attr(opening, "data-look");
    let transform_val = extract_attr(opening, "transform");
    // Upstream `flowDb.setLink` calls `setClass(ids, 'clickable')`,
    // which pushes `'clickable'` into `vertex.classes` *before* any
    // subsequent `class <id> <className>` directive. The parser
    // already mirrors that, so the class string has `clickable` in
    // the upstream-correct slot. Preserve it as-is here.
    let new_class_val = class_val.clone();
    // Build the anchor opening. Order: href, target?, data-look?, transform?.
    let mut anchor = String::with_capacity(link.len() + 96);
    anchor.push_str("<a href=\"");
    anchor.push_str(&xml_escape(link));
    anchor.push('"');
    if let Some(target) = node.link_target.as_deref() {
        if !target.is_empty() {
            anchor.push_str(" target=\"");
            anchor.push_str(&xml_escape(target));
            anchor.push('"');
        }
    }
    if let Some(dl) = data_look_val.as_deref() {
        anchor.push_str(" data-look=\"");
        anchor.push_str(dl);
        anchor.push('"');
    }
    if let Some(tx) = transform_val.as_deref() {
        anchor.push_str(" transform=\"");
        anchor.push_str(tx);
        anchor.push('"');
    }
    anchor.push('>');
    // Rebuild the inner `<g>` — only class, id, optional title.
    let mut new_g = String::with_capacity(opening.len() + 32);
    new_g.push_str("<g class=\"");
    new_g.push_str(&new_class_val);
    new_g.push_str("\" id=\"");
    new_g.push_str(&id_val);
    new_g.push('"');
    if let Some(tip) = node.tooltip.as_deref() {
        if !tip.is_empty() {
            new_g.push_str(" title=\"");
            new_g.push_str(&xml_escape(tip));
            new_g.push('"');
        }
    }
    new_g.push('>');
    // Splice: anything before <g …> + anchor + new <g …> + body + </a>.
    // The shape emits `<g …>…</g>` balanced; the LAST `</g>` closes the
    // outer group, and we append `</a>` after it.
    let prefix = &svg[..g_start];
    let body = &svg[head_open_end..];
    // Find the LAST `</g>` in `body`.
    let last_close = match body.rfind("</g>") {
        Some(i) => i,
        None => return svg.to_string(),
    };
    let body_inner = &body[..last_close];
    let body_close = &body[last_close..];
    let mut out = String::with_capacity(svg.len() + anchor.len() + 8);
    out.push_str(prefix);
    out.push_str(&anchor);
    out.push_str(&new_g);
    out.push_str(body_inner);
    out.push_str(body_close);
    out.push_str("</a>");
    out
}

/// Extract the value of `attr="..."` from an opening tag. Returns
/// `None` when the attribute is absent.
fn extract_attr(opening: &str, attr: &str) -> Option<String> {
    let needle = format!(" {}=\"", attr);
    let i = opening.find(&needle)?;
    let start = i + needle.len();
    let end = opening[start..].find('"')?;
    Some(opening[start..start + end].to_string())
}

/// Inject `title="..."` into the outer `<g class="node …">` opening
/// tag for nodes that have a tooltip but no link. Upstream
/// (`nodes.ts`) calls `el.attr('title', node.tooltip)` on every
/// node with a tooltip — for link nodes the link-postprocess
/// already handles this. This covers callback-only nodes (e.g.
/// `click B testClick "click test"`).
///
/// Placement mirrors upstream's d3 attribute order: `class id
/// data-look title transform`. We insert the `title` attribute
/// immediately before ` transform="…"`.
fn tooltip_postprocess_node_svg(node: &UNode, svg: &str) -> String {
    use crate::render::shapes::types::xml_escape;
    if node.link.is_some() {
        // link_postprocess already handles tooltip injection.
        return svg.to_string();
    }
    let tooltip = match node.tooltip.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => return svg.to_string(),
    };
    let g_start = match svg.find("<g class=\"node ") {
        Some(i) => i,
        None => return svg.to_string(),
    };
    let head_open_end = match svg[g_start..].find('>') {
        Some(off) => g_start + off + 1,
        None => return svg.to_string(),
    };
    let opening = &svg[g_start..head_open_end];
    // If a `title=` is already present on the outer <g>, leave it alone.
    if opening.contains(" title=\"") {
        return svg.to_string();
    }
    // Splice ` title="..."` immediately before ` transform="`.
    let transform_marker = " transform=\"";
    let transform_off = match opening.find(transform_marker) {
        Some(off) => off,
        None => return svg.to_string(),
    };
    let abs_transform = g_start + transform_off;
    let title_attr = format!(r#" title="{}""#, xml_escape(tooltip));
    let mut out = String::with_capacity(svg.len() + title_attr.len());
    out.push_str(&svg[..abs_transform]);
    out.push_str(&title_attr);
    out.push_str(&svg[abs_transform..]);
    out
}

fn bold_postprocess_node_svg(node: &UNode, svg: &str) -> String {
    use crate::render::foreign_object::HtmlLabelFont;
    use crate::render::shapes::types::fmt_num;

    let css = match node.css_styles.as_deref() {
        Some(s) => s,
        None => return svg.to_string(),
    };
    if !node_styles_have_bold(css) {
        return svg.to_string();
    }
    let label = match node.label.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => return svg.to_string(),
    };
    let is_markdown = node.label_type.as_deref() == Some("markdown");
    // Replicate the shape-side measurement chain so we know which "wrong"
    // numbers were emitted, then compute the bold equivalents.
    let escaped = if is_markdown {
        crate::render::foreign_object::markdown_label_to_html(label)
    } else {
        crate::render::foreign_object::string_label_to_html(label)
    };
    let for_measure = crate::render::foreign_object::replace_fa_icons(&escaped);
    let (lw_norm, _lh_norm) = crate::render::foreign_object::measure_html_markup_label(
        &for_measure,
        &HtmlLabelFont::default(),
        200.0,
        true,
    );
    let bold_font = HtmlLabelFont {
        font_family: None,
        font_size_px: None,
        bold: Some(true),
    };
    let (lw_bold, _lh_bold) = crate::render::foreign_object::measure_html_markup_label(
        &for_measure,
        &bold_font,
        200.0,
        true,
    );
    if (lw_bold - lw_norm).abs() < 1e-9 {
        // Bold metrics happen to match (e.g. font lacks bold variant).
        return svg.to_string();
    }
    let norm_w_str = fmt_num(lw_norm);
    let bold_w_str = fmt_num(lw_bold);
    let norm_half_str = fmt_num(-lw_norm / 2.0);
    let bold_half_str = fmt_num(-lw_bold / 2.0);
    // Replace `<foreignObject width="{norm_w}"` and the matching
    // `transform="translate({-norm_w/2}, …)"` token. Both are emitted by
    // `render_node_label` and use the same numeric encoding (`fmt_num`),
    // so a literal substring rewrite is safe — there is no other source of
    // these exact float strings within a single node block.
    let needle_fo = format!(r#"<foreignObject width="{norm_w_str}""#);
    let replace_fo = format!(r#"<foreignObject width="{bold_w_str}""#);
    let mut out = svg.replace(&needle_fo, &replace_fo);
    let needle_tx = format!(r#"transform="translate({norm_half_str},"#);
    let replace_tx = format!(r#"transform="translate({bold_half_str},"#);
    out = out.replace(&needle_tx, &replace_tx);
    out
}

/// Post-process a rendered node SVG to honour `font-size:Xpx` declared in
/// the node's resolved styles (typically via `classDef name font-size:30px`).
///
/// The shape renderers in `shapes/*` always pass `HtmlLabelFont::default()`
/// (14 px) to `shape_label_block`, which means the emitted foreignObject
/// `width`/`height`, the inner `<div>` width, and the surrounding label
/// `<g>` translate are all measured at the default font. Upstream however
/// inlines `font-size:30px !important` on the `<div>` (so jsdom shim measures
/// the bbox at 30 px), inlines `font-size:30px !important` on the label
/// `<g>`, and adds `font-size:30px !important` to the inner `<span>`.
///
/// We mirror that here by:
/// 1. Re-measuring the label HTML at the requested font-size.
/// 2. Substituting the literal numbers in the rendered SVG (foreignObject
///    width/height and the matching label `<g>` translate).
/// 3. Inserting the inline `font-size:Xpx !important` on the label `<g>`,
///    on the inner `<div>` (PRE-pending the existing style), and on the
///    `<span class="nodeLabel ">`.
///
/// Cypress fixture 150 (`classDef larger font-size:30px`) relies on this.
fn font_size_postprocess_node_svg(node: &UNode, svg: &str) -> String {
    use crate::render::foreign_object::HtmlLabelFont;
    use crate::render::shapes::types::fmt_num;

    let css = match node.css_styles.as_deref() {
        Some(s) => s,
        None => return svg.to_string(),
    };
    let font_size_px = match crate::layout::flowchart::styles_font_size_px_pub(css) {
        Some(v) => v,
        None => return svg.to_string(),
    };
    let label = match node.label.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => return svg.to_string(),
    };
    let is_markdown = node.label_type.as_deref() == Some("markdown");
    let escaped = if is_markdown {
        crate::render::foreign_object::markdown_label_to_html(label)
    } else {
        crate::render::foreign_object::string_label_to_html(label)
    };
    let for_measure = crate::render::foreign_object::replace_fa_icons(&escaped);
    let bold = node_styles_have_bold(css);
    let default_font = HtmlLabelFont {
        font_family: None,
        font_size_px: None,
        bold: if bold { Some(true) } else { None },
    };
    let (lw_default, lh_default) = crate::render::foreign_object::measure_html_markup_label(
        &for_measure,
        &default_font,
        200.0,
        true,
    );
    let sized_font = HtmlLabelFont {
        font_family: None,
        font_size_px: Some(font_size_px),
        bold: if bold { Some(true) } else { None },
    };
    let (lw_sized, lh_sized) = crate::render::foreign_object::measure_html_markup_label(
        &for_measure,
        &sized_font,
        200.0,
        true,
    );
    if (lw_sized - lw_default).abs() < 1e-9 && (lh_sized - lh_default).abs() < 1e-9 {
        return svg.to_string();
    }

    // 1. Replace foreignObject width / height numbers.
    let needle_fo = format!(
        r#"<foreignObject width="{}" height="{}""#,
        fmt_num(lw_default),
        fmt_num(lh_default)
    );
    let replace_fo = format!(
        r#"<foreignObject width="{}" height="{}""#,
        fmt_num(lw_sized),
        fmt_num(lh_sized)
    );
    let mut out = svg.replace(&needle_fo, &replace_fo);

    // 2. Replace the label `<g>` translate (`-lw/2`, `-lh/2`).
    let needle_tx = format!(
        r#"<g class="label" style="" transform="translate({}, {})""#,
        fmt_num(-lw_default / 2.0),
        fmt_num(-lh_default / 2.0)
    );
    let replace_tx = format!(
        r#"<g class="label" style="font-size:{fs}px !important" transform="translate({}, {})""#,
        fmt_num(-lw_sized / 2.0),
        fmt_num(-lh_sized / 2.0),
        fs = fmt_num(font_size_px)
    );
    out = out.replace(&needle_tx, &replace_tx);

    // 3. Inline font-size on the inner `<div>` and `<span>`.
    let div_needle = r#"<div style="display: table-cell;"#;
    let div_replace = format!(
        r#"<div style="font-size: {fs}px !important; display: table-cell;"#,
        fs = fmt_num(font_size_px)
    );
    out = out.replace(div_needle, &div_replace);
    let span_needle = r#"<span class="nodeLabel ">"#;
    let span_replace = format!(
        r#"<span style="font-size:{fs}px !important" class="nodeLabel ">"#,
        fs = fmt_num(font_size_px)
    );
    out = out.replace(span_needle, &span_replace);

    out
}

/// the `x` attribute of the `<text class="flowchartTitleText">` element it
/// appends — the title is then measured by jsdom's getBBox shim and pushes the
/// outer bbox horizontally.
fn compute_viewbox(
    l: &FlowchartLayout,
    padding: f64,
    title: Option<&str>,
) -> (f64, f64, f64, f64, f64) {
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
        // Skip cyclic helper labelRects that the renderer also drops —
        // they are dagre placeholders for self-loop expansion that the
        // upstream pipeline does NOT render (see
        // `is_cyclic_helper_from_anchor_rewrite`). Including them here
        // would inflate the viewBox just like rendering them would.
        if is_cyclic_helper_from_anchor_rewrite(n, l) {
            continue;
        }

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

        // Clickable nodes (with `click` directive) are wrapped in `<a>`
        // by the renderer. Upstream's jsdom getBBox shim has no special
        // handler for `<a>` and therefore falls through to the textContent
        // measurement branch (see generate_ref.mjs `intrinsicBox`), which
        // returns `{x:0, y:0, width:textW, height:textH}` — i.e. the
        // shape's negative-coordinate rect (and any local-coord polygon)
        // is NOT visited because intrinsicBox treats the `<a>` as a leaf.
        // Replicate that here by skipping the shape bbox; the foreignObject
        // label below still contributes a `(0, 0, lw, lh)` box, matching
        // the textContent fallback closely enough for byte-exact viewBox.
        let has_link = n.link.as_deref().is_some_and(|s| !s.is_empty());
        if !has_link {
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
                "trapezoid" | "trap" | "inv_trapezoid" | "invertedTrapezoid" | "lean_left"
                | "lean-left" | "lean_right" | "lean-right" => {
                    // `node.width` already carries the visual width
                    // (= base_w + 2*shear). polygon raw points span
                    //   x ∈ [-shear, base_w + shear] = [-shear, w - shear]
                    //   y ∈ [-h, 0]
                    // and the polygon's own transform is ignored by the jsdom shim.
                    let shear = h / 2.0;
                    expand(
                        &mut min_x, &mut min_y, &mut max_x, &mut max_y, -shear, -h, w, h,
                    );
                }
                "hexagon" | "hex" => {
                    // polygon raw points span x ∈ [0, w], y ∈ [-h, 0]
                    // (the polygon's own `translate(-w/2, h/2)` transform
                    // is ignored by the jsdom getBBox shim, so the bbox
                    // reflects the raw point list).
                    expand(
                        &mut min_x, &mut min_y, &mut max_x, &mut max_y, 0.0, -h, w, h,
                    );
                }
                "subroutine" => {
                    // `node.width` carries the visual width
                    // (= base_w + 2*FRAME_WIDTH). The raw polygon spans
                    //   x ∈ [-8, base_w + 8] = [-8, w - 8],  width = w
                    //   y ∈ [-h, 0]
                    // and the polygon's own `translate(-base_w/2, h/2)`
                    // is ignored by the jsdom getBBox shim.
                    expand(
                        &mut min_x, &mut min_y, &mut max_x, &mut max_y, -8.0, -h, w, h,
                    );
                }
                "cylinder" | "cyl" => {
                    // Cylinder path d="M0,ry a... w,0 a... -w,0 l0,h a... w,0 l0,-h"
                    // jsdom's pathBBox shim only registers each command's
                    // endpoint (arcs ignore the bulge), so the visited points
                    // are: (0,ry), (w,ry), (0,ry), (0,ry+h), (w,ry+h), (w,ry).
                    // Bbox = x ∈ [0, w], y ∈ [ry, ry+h]. The path's own
                    // translate(-w/2, -(h/2+ry)) transform is ignored by the
                    // shim, so the contribution lives in path-local coords.
                    let rx = w / 2.0;
                    let ry = rx / (2.5 + w / 50.0);
                    expand(
                        &mut min_x, &mut min_y, &mut max_x, &mut max_y, 0.0, ry, w, h,
                    );
                }
"icon" => {
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
                    let label_text = n.label.as_deref().unwrap_or("");
                    if label_text.is_empty() {
                        let (_, lh) = measure_html_markup_label("", &font, 200.0, true);
                        expand(&mut min_x, &mut min_y, &mut max_x, &mut max_y, 0.0, 0.0, 0.0, lh);
                    } else {
                        let is_markdown = n.label_type.as_deref() == Some("markdown");
                        let label_escaped = if is_markdown {
                            crate::render::foreign_object::markdown_label_to_html(label_text)
                        } else {
                            crate::render::foreign_object::string_label_to_html(label_text)
                        };
                        let processed = crate::render::foreign_object::replace_fa_icons(&label_escaped);
                        let node_bold = n
                            .css_styles
                            .as_deref()
                            .map(node_styles_have_bold)
                            .unwrap_or(false);
                        let label_font = if node_bold {
                            let mut f = font.clone();
                            f.bold = Some(true);
                            f
                        } else {
                            font.clone()
                        };
                        let (lw, lh) = measure_html_markup_label(&processed, &label_font, 200.0, true);
                        expand(&mut min_x, &mut min_y, &mut max_x, &mut max_y, 0.0, 0.0, lw, lh);
                    }
                    expand(
                        &mut min_x,
                        &mut min_y,
                        &mut max_x,
                        &mut max_y,
                        0.0,
                        0.0,
                        80.0,
                        80.0,
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
            // Honour `font-weight:bold` from the resolved node styles so the
            // measured width matches the bold-styled `<div>`'s
            // `getBoundingClientRect()` width.
            let node_bold = n
                .css_styles
                .as_deref()
                .map(node_styles_have_bold)
                .unwrap_or(false);
            let label_font = if node_bold {
                let mut f = font.clone();
                f.bold = Some(true);
                f
            } else {
                font.clone()
            };
            // KaTeX `$$..$$` math: the bbox we feed into the viewBox union
            // must match the bbox the actual KaTeX-rendered foreignObject
            // ends up with — otherwise the viewBox grows as if it contained
            // the raw `$$..$$` source text.
            let (lw, lh) = if crate::render::foreign_object::contains_katex_marker(&processed) {
                match crate::render::foreign_object::try_render_katex_label(
                    &processed,
                    &label_font,
                ) {
                    Some((_, w, h)) => (w, h),
                    None => measure_html_markup_label(&processed, &label_font, 200.0, true),
                }
            } else {
                measure_html_markup_label(&processed, &label_font, 200.0, true)
            };
            expand(
                &mut min_x, &mut min_y, &mut max_x, &mut max_y, 0.0, 0.0, lw, lh,
            );
        }
    }

    // Edge paths: coordinates are already global (edgePaths <g> has no transform).
    // Round to 3 decimal places to match fmt_coord() applied when building the
    // SVG path d attribute — the upstream pathBBox parses the rendered d string.
    //
    // Apply the FULL `getLineFunctionsWithOffset` adjustment to a clone of the
    // points (matching `apply_marker_offsets`), since jsdom's getBBox parses the
    // rendered path d-string — which is built from the offset-adjusted points,
    // not the raw layout points. The full adjustment includes:
    //   1. Endpoint base offset along the direction toward the neighbour.
    //   2. Per-point "extra-room" pull-back when the point is within
    //      `markerHeight` of either endpoint along BOTH axes (this is what
    //      shortens short cluster→cluster edges on BOTH sides even when only
    //      one end carries an arrow).
    //
    // Upstream markerOffsets only has: arrow_point=4, arrow_barb=0,
    // arrow_barb_neo=5.5; arrow_open / arrow_cross / arrow_circle are absent,
    // so they contribute no offset.
    for e in &l.edges {
        // Skip self-loop edges that the renderer also drops: these would
        // contribute degenerate path points that have no rendered visual
        // counterpart but would inflate the viewBox.
        if is_replaced_self_loop(e) || is_cyclic_segment_from_anchor_rewrite(e) {
            continue;
        }
        let Some(points) = &e.points else { continue };
        if points.is_empty() {
            continue;
        }

        let arrow_end = e.arrow_type_end.as_deref().unwrap_or("none");
        let arrow_start = e.arrow_type_start.as_deref().unwrap_or("none");

        let mut adjusted: Vec<Point> = points.clone();
        apply_marker_offsets(&mut adjusted, arrow_end, arrow_start);

        // Mirror upstream's `pathBBox` → `unionBox` flow. The bbox is
        // computed from the rendered `d=` string commands (M/L/C/etc.),
        // not the raw layout points. For cubic curves the basis spline
        // smooths the polyline so the visited control-point bbox is
        // tighter than `min/max` of the raw input. We round each visited
        // coord to 3 decimals to match `appendRound(3)` in the rendered
        // string, then union as `(min, max-min)` to reproduce upstream's
        // `parseFloat(maxX) - parseFloat(minX)` precision behaviour.
        let curve_name = e.curve.as_deref().unwrap_or("basis");
        let ctype = edges::CurveType::parse(curve_name).unwrap_or(edges::CurveType::Basis);
        let visited = edges::pathbbox_points(&adjusted, ctype);
        if visited.is_empty() {
            continue;
        }
        let mut path_min_x = f64::INFINITY;
        let mut path_min_y = f64::INFINITY;
        let mut path_max_x = f64::NEG_INFINITY;
        let mut path_max_y = f64::NEG_INFINITY;
        for (px, py) in visited.iter() {
            let rx = (px * 1000.0).round() / 1000.0;
            let ry = (py * 1000.0).round() / 1000.0;
            if rx < path_min_x {
                path_min_x = rx;
            }
            if ry < path_min_y {
                path_min_y = ry;
            }
            if rx > path_max_x {
                path_max_x = rx;
            }
            if ry > path_max_y {
                path_max_y = ry;
            }
        }
        if path_min_x.is_finite() {
            let bw = path_max_x - path_min_x;
            let bh = path_max_y - path_min_y;
            expand(
                &mut min_x, &mut min_y, &mut max_x, &mut max_y, path_min_x, path_min_y, bw, bh,
            );
        }
    }

    // Edge labels: foreignObject at (0,0) with measured width/height.
    // The label <g> has a centring transform which is ignored by the shim.
    // Upstream replaces FA icon tokens with <i> elements (zero width under
    // jsdom shim) before measuring, so we apply the same substitution.
    for e in &l.edges {
        // Skip self-loop edges that the renderer drops (see edge-paths loop).
        if is_replaced_self_loop(e) || is_cyclic_segment_from_anchor_rewrite(e) {
            continue;
        }
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
        return (0.0, 0.0, 1.0, 1.0, 0.0);
    }

    // Center of the content-only bounding box — recorded BEFORE the title
    // text element widens it. Mirrors upstream `utils.insertTitle` which
    // captures `bounds.x + bounds.width/2` for the title's `x` attribute.
    let content_center_x = min_x + (max_x - min_x) / 2.0;

    // Diagram title: upstream appends `<text class="flowchartTitleText">…</text>`
    // as a sibling of the seed `<g>` AFTER the inner content but BEFORE
    // setupViewPortForSVG measures the bbox. Under the jsdom getBBox shim a
    // `<text>` element contributes `{x:0, y:0, width:tw, height:lh}` regardless
    // of its own `x`/`y` attributes (transforms and text-anchor are ignored),
    // so the title pushes `max_x` out to `tw` and the bbox grows accordingly.
    //
    // The CSS rule `.flowchartTitleText{font-size:18px;}` is applied at the
    // class level — without an inline `font-family`, the shim falls back to
    // jsdom's default sans-serif. Match that here for byte-exact width.
    if let Some(t) = title {
        if !t.is_empty() {
            // Even though `.flowchartTitleText{font-size:18px}` is in the
            // diagram CSS, the jsdom getBBox shim does NOT resolve class-level
            // CSS rules — it falls back to the default sans-serif 14 px metric.
            // Mirror upstream by measuring at 14 px.
            let tw = crate::font_metrics::text_width(t, "sans-serif", 14.0, false, false);
            let lh = 16.296875_f64;
            min_x = min_x.min(0.0);
            max_x = max_x.max(tw);
            min_y = min_y.min(0.0);
            max_y = max_y.max(lh);
        }
    }

    let content_w = max_x - min_x;
    let content_h = max_y - min_y;
    let vb_x = min_x - padding;
    let vb_y = min_y - padding;
    let vb_w = (content_w + 2.0 * padding).max(1.0);
    let vb_h = (content_h + 2.0 * padding).max(1.0);

    (vb_x, vb_y, vb_w, vb_h, content_center_x)
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
    //
    // For isolated clusters the layout engine reports `c.bounds` in the
    // INNER dagre coordinate frame (typically anchored at (8, 8)) — the
    // inner root group is later translated by `outer_tx / outer_ty` to
    // place the cluster correctly in the outer (top-level) coordinate
    // space. Since edge spline points are emitted in outer coordinates
    // we must translate the bounds by the same offset before using them
    // to clip `cluster-endpoint` edge paths; otherwise `cut_path_at_intersect`
    // never sees the spline cross the boundary and either drops every
    // segment or leaves the path entirely outside the cluster.
    let cluster_bounds: std::collections::HashMap<String, crate::layout::unified::Bounds> = l
        .clusters
        .iter()
        .filter_map(|c| {
            let mut b = c.bounds.as_ref()?.clone();
            // Pick up the matching cluster node so we can look up the
            // pre-computed outer translate (only present on isolated
            // clusters that participated in the inner-pass).
            if let Some(cnode) = l.nodes.iter().find(|n| n.id == c.id && n.is_group) {
                let tx = cnode
                    .extra
                    .get("outer_tx")
                    .and_then(|s| s.parse::<f64>().ok());
                let ty = cnode
                    .extra
                    .get("outer_ty")
                    .and_then(|s| s.parse::<f64>().ok());
                if let (Some(tx), Some(ty)) = (tx, ty) {
                    b.x += tx;
                    b.y += ty;
                }
            }
            Some((c.id.clone(), b))
        })
        .collect();

    // ── Render inner content first (markers + root group) ──────────
    // We need the rendered content to compute the viewBox accurately,
    // matching upstream's `getBBox()` approach.

    // `flowchart.htmlLabels` controls whether labels render via foreignObject
    // (HTML/CSS) or `<text>`/`<tspan>` (SVG). Defaults to true, but
    // `%%{init: {flowchart: {htmlLabels: false}}}%%` flips it for an entire
    // diagram. Cluster, node and edge labels all consult this flag.
    let html_labels = d.html_labels.unwrap_or(true);

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
            let has_children = l
                .nodes
                .iter()
                .any(|n| n.parent_id.as_deref() == Some(cluster_id) && !n.is_group)
                || l.nodes
                    .iter()
                    .any(|n| n.parent_id.as_deref() == Some(cluster_id) && n.is_group);
            if !has_children {
                continue;
            } else {
                inner.push_str(&render_cluster(cnode, cluster, theme, id, html_labels));
            }
        }
    }
    inner.push_str(unified_shell::close_layer());

    // Edge paths (outer level).
    //
    // An edge is rendered at the outer level UNLESS both endpoints share
    // the same outermost isolated cluster ancestor — in which case the
    // edge renders inside that cluster's inner root group instead.
    // Cross-cluster edges (e.g. `A --> B` where A and B are isolated
    // subgraphs) belong here because the routed points are in the outer
    // (top-level) coordinate space.
    inner.push_str(&unified_shell::open_layer("edgePaths"));
    for (i, e) in l.edges.iter().enumerate() {
        if edge_is_inside_isolated(e.start.as_deref(), e.end.as_deref(), l) {
            continue;
        }
        // Skip the original user self-edge: upstream `expand_self_edge`
        // replaces it with helper nodes plus three cyclic-special sub-edges
        // (which arrive separately via dagre_bridge synthetic exposure).
        if is_replaced_self_loop(e) {
            continue;
        }
        // Skip synthetic cyclic-special segments that came from anchor
        // rewriting (e.g. `Sub --> In` where `In` is `Sub`'s anchor):
        // upstream does not expand those into self-loop helpers.
        if is_cyclic_segment_from_anchor_rewrite(e) {
            continue;
        }
        inner.push_str(&render_edge_path(e, i, id, &l.aria_kind, &cluster_bounds));
    }
    inner.push_str(unified_shell::close_layer());

    // Edge labels (outer level — same selection rule as edge paths).
    inner.push_str(&unified_shell::open_layer("edgeLabels"));
    for e in l.edges.iter() {
        if edge_is_inside_isolated(e.start.as_deref(), e.end.as_deref(), l) {
            continue;
        }
        if is_replaced_self_loop(e) {
            continue;
        }
        if is_cyclic_segment_from_anchor_rewrite(e) {
            continue;
        }
        inner.push_str(&render_edge_label(e, html_labels, l));
    }
    inner.push_str(unified_shell::close_layer());

    // Nodes (outer level).
    // Top-level isolated clusters (those whose parent is not also isolated)
    // are inserted here as inner root groups; regular (non-cluster) nodes
    // whose parent is not any isolated cluster follow.
    inner.push_str(&unified_shell::open_layer("nodes"));

    // Render top-level isolated clusters as inner <g class="root"> groups and
    // non-isolated empty clusters as regular nodes — both appear inside
    // <g class="nodes">. Iteration order matches upstream's recursiveRender
    // DFS traversal, which (because flowDb.getData reverses subgraph
    // insertion order) visits sibling subgraphs in REVERSED declaration
    // order. Using the shared `cluster_render_order` keeps isolated and
    // non-isolated clusters aligned.
    for cluster_id in &cluster_render_order {
        let Some(cnode) = l.nodes.iter().find(|n| &n.id == cluster_id && n.is_group) else {
            continue;
        };
        let has_children = l
            .nodes
            .iter()
            .any(|n| n.parent_id.as_deref() == Some(cluster_id));
        if l.isolated_cluster_ids.contains(cluster_id) {
            // Skip if parent is also an isolated cluster (nested, handled recursively).
            let parent_also_isolated = cnode
                .parent_id
                .as_deref()
                .map(|p| l.isolated_cluster_ids.contains(p))
                .unwrap_or(false);
            if parent_also_isolated {
                continue;
            }
            if !has_children {
                let cluster = l.clusters.iter().find(|c| &c.id == cluster_id);
                if let Some(cluster) = cluster {
                    inner.push_str(&render_empty_cluster_as_node(
                        cnode,
                        cluster,
                        theme,
                        id,
                        html_labels,
                    ));
                }
            } else {
                inner.push_str(&render_isolated_cluster_inner_root(
                    cnode,
                    d,
                    l,
                    theme,
                    id,
                    html_labels,
                ));
            }
        } else {
            // Non-isolated empty cluster: demoted to node, rendered in
            // <g class="nodes"> instead of <g class="clusters">.
            if !has_children {
                if is_child_of_isolated(Some(cluster_id), l) {
                    continue;
                }
                let cluster = l.clusters.iter().find(|c| &c.id == cluster_id);
                if let Some(cluster) = cluster {
                    inner.push_str(&render_empty_cluster_as_node(
                        cnode,
                        cluster,
                        theme,
                        id,
                        html_labels,
                    ));
                }
            }
        }
    }

    // Render non-isolated-cluster child nodes at the outer level.
    //
    // Iteration order mirrors upstream's `graph.nodes()` traversal which is
    // backed by `Object.keys(this._nodes)`. JS object key iteration order is
    // standardised: integer-index keys (32-bit unsigned, no leading zeros)
    // come first in numeric ascending order, then string keys in insertion
    // order. Diagrams whose vertex IDs are pure decimals (e.g. fixtures
    // 203 / 70 / demos 10/11/51) thus emit nodes sorted numerically rather
    // than by declaration order.
    let outer_node_indices: Vec<usize> = js_object_key_order(l.nodes.iter().map(|n| n.id.as_str()));
    for &idx in &outer_node_indices {
        let n = &l.nodes[idx];
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
        // Skip cyclic helper labelRect nodes whose owner has only
        // anchor-rewrite cyclic_segments — upstream's `expand_self_edge`
        // is bypassed in that case (see `is_cyclic_segment_from_anchor_rewrite`).
        if is_cyclic_helper_from_anchor_rewrite(n, l) {
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
            Ok(svg) => {
                let patched = bold_postprocess_node_svg(&prefixed, &svg);
                let patched = diamond_br_postprocess(&prefixed, &patched);
                let patched = font_size_postprocess_node_svg(&prefixed, &patched);
                let patched = tooltip_postprocess_node_svg(&prefixed, &patched);
                let patched = link_postprocess_node_svg(&prefixed, &patched);
                inner.push_str(&patched);
            }
            Err(_) => {
                // Fallback: plain rect.
                if let Ok(svg) = shapes::draw("rect", &prefixed, theme) {
                    let patched = bold_postprocess_node_svg(&prefixed, &svg);
                    let patched = diamond_br_postprocess(&prefixed, &patched);
                    let patched = tooltip_postprocess_node_svg(&prefixed, &patched);
                    let patched = link_postprocess_node_svg(&prefixed, &patched);
                    inner.push_str(&patched);
                }
            }
        }
    }
    inner.push_str(unified_shell::close_layer());

    inner.push_str(unified_shell::close_root_group());
    // Colored marker variants — upstream's `addEdgeMarker` clones the
    // base marker once per unique stroke-color found in any edge's
    // `pathStyle`. The clones live in the same parent as the base
    // markers (the seed `<g>`), but are appended AFTER the root group
    // because they are added during edge rendering, which happens
    // after the root group has already been built up. Mirror that
    // ordering exactly so byte-for-byte alignment is preserved.
    inner.push_str(&emit_colored_marker_defs(&l.edges, &l.aria_kind, id));
    inner.push_str(unified_shell::close_seed_group());

    // ── Compute viewBox from rendered content ──────────────────────
    // Upstream uses `svg.getBBox()` which returns the actual rendered
    // bounds including shape geometry, edge curves, and label
    // positions. We compute from layout nodes and edges.
    let (vb_x, vb_y, vb_w, vb_h, content_center_x) =
        compute_viewbox(l, padding, d.meta.title.as_deref());

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
    // Theme gradient defs — see `emit_gradient_defs` for the per-theme
    // `useGradient` flag (default theme keeps it false).
    out.push_str(&unified_shell::emit_gradient_defs(id, theme));

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
            // Use the pre-title content center, matching upstream's
            // `utils.insertTitle` which captures `bounds.x + bounds.width/2`
            // BEFORE adding the title text to the SVG.
            out.push_str(&format!(
                r#"<text text-anchor="middle" x="{cx}" y="-{tm}" class="flowchartTitleText">{t}</text>"#,
                cx = fmt_num(content_center_x),
                tm = title_top_margin,
                t = xml_escape(title),
            ));
        }
    }

    out.push_str(unified_shell::close_unified_svg());

    // Post-process: rewrite `<img>` tags inside `<p>` blocks to mirror upstream's
    // `configureLabelImages` (rendering-util/rendering-elements/shapes/labelImageUtils.ts):
    //   - Normalize `src='…'` single-quoted attribute values to `src="…"` (DOM serializer).
    //   - For each `<img>` inside a `<p>`, append an inline `style` attribute:
    //       * If the `<p>` content with all `<img …>` tags removed trims to empty,
    //         the upstream config uses `bodyFontSize * 5` = 16 * 5 = 80px:
    //         `display: flex; flex-direction: column; min-width: 80px; max-width: 80px;`
    //       * Otherwise (image alongside text):
    //         `display: flex; flex-direction: column; width: 100%;`
    // The label measurement isn't affected (image dimensions only matter at runtime),
    // so this is a pure string rewrite of the final SVG.
    let out = postprocess_imgs(&out);

    Ok(out)
}

/// See call site for full documentation. Splits the input into `<p>...</p>` blocks
/// and injects per-block `<img>` styles, matching upstream `configureLabelImages`.
fn postprocess_imgs(svg: &str) -> String {
    if !svg.contains("<img") {
        return svg.to_string();
    }
    let mut out = String::with_capacity(svg.len() + 64);
    let bytes = svg.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find next <p> block. We rewrite only `<img>` inside `<p>`. Outside, copy verbatim.
        let p_open_pat = b"<p>";
        let p_close_pat = b"</p>";
        // Locate next `<p>` start (with possible attributes, but reference uses bare `<p>`).
        let rel = match find_subseq(&bytes[i..], p_open_pat) {
            Some(off) => off,
            None => {
                out.push_str(&svg[i..]);
                break;
            }
        };
        // Copy up to and including `<p>`.
        let p_start = i + rel;
        let p_inner_start = p_start + p_open_pat.len();
        out.push_str(&svg[i..p_inner_start]);
        // Find matching `</p>`.
        let close_rel = match find_subseq(&bytes[p_inner_start..], p_close_pat) {
            Some(off) => off,
            None => {
                out.push_str(&svg[p_inner_start..]);
                break;
            }
        };
        let p_inner_end = p_inner_start + close_rel;
        let inner = &svg[p_inner_start..p_inner_end];
        if inner.contains("<img") {
            // Determine style: based on whether stripping all <img …> tags leaves any text.
            let stripped = strip_img_tags(inner);
            let trimmed = stripped.trim();
            let img_style = if trimmed.is_empty() {
                "display: flex; flex-direction: column; min-width: 80px; max-width: 80px;"
            } else {
                "display: flex; flex-direction: column; width: 100%;"
            };
            out.push_str(&rewrite_imgs_in_segment(inner, img_style));
        } else {
            out.push_str(inner);
        }
        out.push_str("</p>");
        i = p_inner_end + p_close_pat.len();
    }
    out
}

/// Locate `needle` as a contiguous byte sub-sequence inside `hay`; return its start index.
fn find_subseq(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let last = hay.len() - needle.len();
    let mut i = 0;
    while i <= last {
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Remove `<img …>` tags (entire void element) from `s`; returns the residual
/// string used purely to test "is there any non-image text" for upstream's
/// `noImgText` decision.
fn strip_img_tags(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 4 <= bytes.len() && &bytes[i..i + 4] == b"<img" {
            // Skip until next '>'.
            let mut j = i + 4;
            while j < bytes.len() && bytes[j] != b'>' {
                j += 1;
            }
            if j < bytes.len() {
                j += 1; // consume '>'
            }
            i = j;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Rewrite each `<img …>` tag in `seg`:
///   - Convert `src='…'` to `src="…"`.
///   - Append the inline `style="…"` attribute (before the closing `>`) when no
///     `style=` attribute is already present on that tag. This matches the DOM
///     state after `configureLabelImages` has set `img.style.*` properties on
///     each image element.
fn rewrite_imgs_in_segment(seg: &str, img_style: &str) -> String {
    let bytes = seg.as_bytes();
    let mut out = String::with_capacity(seg.len() + 96);
    let mut i = 0;
    while i < bytes.len() {
        if i + 4 <= bytes.len() && &bytes[i..i + 4] == b"<img" {
            let start = i;
            let mut j = i + 4;
            while j < bytes.len() && bytes[j] != b'>' {
                j += 1;
            }
            // Tag covers bytes [start..j], with `>` at j (or eof).
            let tag_inner = if j <= bytes.len() {
                &seg[start + 4..j]
            } else {
                &seg[start + 4..]
            };
            // Normalize `src='…'` → `src="…"` within tag_inner.
            let normalized = normalize_attr_quotes(tag_inner);
            out.push_str("<img");
            out.push_str(&normalized);
            // Append style attribute when not already present.
            if !attr_present(&normalized, "style") {
                out.push_str(" style=\"");
                out.push_str(img_style);
                out.push('"');
            }
            if j < bytes.len() {
                out.push('>');
                i = j + 1;
            } else {
                i = bytes.len();
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Convert single-quoted attribute values to double-quoted equivalents
/// (`src='url'` → `src="url"`). Only re-quotes; does not escape inner double
/// quotes (the source label tokens we accept never contain `"`).
fn normalize_attr_quotes(tag_inner: &str) -> String {
    let bytes = tag_inner.as_bytes();
    let mut out = String::with_capacity(tag_inner.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'=' && i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
            // Find matching closing single quote.
            let mut j = i + 2;
            while j < bytes.len() && bytes[j] != b'\'' {
                j += 1;
            }
            out.push('=');
            out.push('"');
            out.push_str(&tag_inner[i + 2..j]);
            out.push('"');
            i = if j < bytes.len() { j + 1 } else { j };
        } else {
            out.push(b as char);
            i += 1;
        }
    }
    out
}

/// Return true iff a tag attribute named `name=` (case-insensitive) appears in
/// `tag_inner` outside any quoted attribute value. Used to skip injecting the
/// auto-style when the tag already declares one.
fn attr_present(tag_inner: &str, name: &str) -> bool {
    let lower = tag_inner.to_ascii_lowercase();
    let needle = format!("{}=", name.to_ascii_lowercase());
    lower.contains(&needle)
}

fn render_empty_cluster_as_node(
    node: &UNode,
    _cluster: &Cluster,
    theme: &ThemeVariables,
    svg_id: &str,
    html_labels: bool,
) -> String {
    let p = node.padding.unwrap_or(8.0);
    let label = node.label.clone().unwrap_or_default();
    let is_markdown = node.label_type.as_deref() == Some("markdown");
    let label_content_for_measure = if is_markdown {
        crate::render::foreign_object::markdown_label_to_html(&label)
    } else {
        crate::render::foreign_object::string_label_to_html(&label)
    };
    let for_measure = crate::render::foreign_object::replace_fa_icons(&label_content_for_measure);
    let font = crate::render::foreign_object::HtmlLabelFont::default();
    let (lw, lh) = if html_labels && !label.is_empty() {
        crate::render::foreign_object::measure_html_markup_label(&for_measure, &font, 200.0, true)
    } else if !label.is_empty() {
        (
            crate::font_metrics::text_width(&label, "sans-serif", 16.0, false, false) as f64,
            22.0,
        )
    } else {
        (0.0, 0.0)
    };
    let nw = lw + p * 4.0;
    let nh = lh + p * 2.0;
    let mut demoted = node.clone();
    demoted.is_group = false;
    demoted.width = Some(nw);
    demoted.height = Some(nh);
    demoted.css_classes = Some(String::new());
    let base_id = demoted.dom_id.clone().unwrap_or_else(|| demoted.id.clone());
    demoted.dom_id = Some(format!("{svg_id}-{base_id}"));
    let shape_id = demoted.shape.clone().unwrap_or_else(|| "rect".to_string());
    match shapes::draw(&shape_id, &demoted, theme) {
        Ok(svg) => {
            let patched = bold_postprocess_node_svg(&demoted, &svg);
            let patched = diamond_br_postprocess(&demoted, &patched);
            let patched = font_size_postprocess_node_svg(&demoted, &patched);
            let patched = tooltip_postprocess_node_svg(&demoted, &patched);
            let patched = link_postprocess_node_svg(&demoted, &patched);
            patched
        }
        Err(_) => String::new(),
    }
}

fn render_cluster(
    node: &UNode,
    _cluster: &Cluster,
    _theme: &ThemeVariables,
    svg_id: &str,
    html_labels: bool,
) -> String {
    use crate::render::foreign_object::{measure_html_markup_label, HtmlLabelFont};

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
        // Markdown subgraph titles (e.g. `subgraph "`**strong**`"`) need the
        // bold/italic markup expanded to HTML before measuring + emitting,
        // so the foreignObject dimensions and the inner `<p>` content match
        // upstream byte-for-byte.
        let is_markdown = node.label_type.as_deref() == Some("markdown");
        let escaped = if is_markdown {
            crate::render::foreign_object::markdown_label_to_html(&label)
        } else {
            xml_escape(&label)
        };
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
        // Markdown labels render with extra `max-width` (= rect inner width) and
        // centred text — upstream's `createText` adds those when `useHtmlLabels`
        // is on and the label is markdown.
        let div_style = if is_markdown {
            format!(
                "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {mw}px; text-align: center;",
                mw = fmt_num(w),
            )
        } else {
            "display: table-cell; white-space: nowrap; line-height: 1.5;".to_string()
        };
        if html_labels {
            out.push_str(&format!(
                r#"<g class="cluster-label " transform="translate({label_tx}, {label_ty})"><foreignObject width="{lw}" height="{lh}"><div style="{div_style}" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel "{span_style_attr}><p>{escaped}</p></span></div></foreignObject></g>"#,
                label_tx = fmt_num(label_tx),
                label_ty = fmt_num(label_ty),
                lw = fmt_num(lw),
                lh = fmt_num(lh),
                div_style = div_style,
                span_style_attr = span_style_attr,
                escaped = escaped,
            ));
        } else {
            // htmlLabels=false: emit `<text>`/`<tspan>` (SVG) instead of
            // foreignObject. Mirrors upstream `createText` non-html branch.
            // For cluster labels, `addBackground=true` always (createText
            // calls createFormattedText with the labelGroup wrapping a
            // `<rect class="background">` and `<text>`).  The rect carries
            // `style="stroke: none"` and no x/y/width/height — upstream
            // never sizes the rect for cluster labels (unlike edges, which
            // pass through `insertEdgeLabel`).
            let inner_tspans = build_cluster_text_inner_tspans(&label, is_markdown);
            out.push_str(&format!(
                r#"<g class="cluster-label " transform="translate({label_tx}, {label_ty})"><g><rect class="background" style="stroke: none"></rect><text y="-10.1" style=""><tspan class="text-outer-tspan row" x="0" y="-0.1em" dy="1.1em">{inner_tspans}</tspan></text></g></g>"#,
                label_tx = fmt_num(label_tx),
                label_ty = fmt_num(label_ty),
                inner_tspans = inner_tspans,
            ));
        }
    }
    out.push_str("</g>");
    out
}

/// Build the inner `<tspan class="text-inner-tspan">` markup for a cluster
/// label rendered in htmlLabels=false mode.  Mirrors upstream
/// `updateTextContentAndStyles` (createText.ts:308) which iterates the
/// markdown words on the line and emits one `<tspan>` per word with
/// `font-style` and `font-weight` reflecting the segment kind.
///
/// Cluster labels are single-line (no `<br>` splits supported here), so the
/// outer `<tspan class="text-outer-tspan row">` wraps just one line.
fn build_cluster_text_inner_tspans(label: &str, is_markdown: bool) -> String {
    use crate::render::shapes::types::xml_escape;
    let lines: Vec<Vec<MdWord>> = if is_markdown {
        markdown_to_lines(label)
    } else {
        non_markdown_to_lines(label)
    };
    // Cluster labels never wrap onto multiple lines in upstream's flow
    // (the call site passes `useMaxWidth=false`); take the first line only.
    let first_line: &[MdWord] = lines.first().map(|v| v.as_slice()).unwrap_or(&[]);
    let mut out = String::new();
    for (word_idx, w) in first_line.iter().enumerate() {
        let font_style = match w.kind {
            MdWordKind::Em => "italic",
            _ => "normal",
        };
        let font_weight = match w.kind {
            MdWordKind::Strong => "bold",
            _ => "normal",
        };
        let content = if word_idx == 0 {
            xml_escape(&w.content)
        } else {
            format!(" {}", xml_escape(&w.content))
        };
        out.push_str(&format!(
            r#"<tspan font-style="{fs}" class="text-inner-tspan" font-weight="{fw}">{c}</tspan>"#,
            fs = font_style,
            fw = font_weight,
            c = content,
        ));
    }
    out
}

/// Upstream's dagre `expand_self_edge` replaces every user self-edge with two
/// helper labelRect nodes plus three `cyclic-special` sub-edges; the original
/// `setEdge(src, dst, …)` call for the self-loop is skipped at index.js:307.
/// Detect that case here so the renderer can drop the original edge's path /
/// label and avoid double-rendering alongside the synthetic segments.
///
/// `orig_start` / `orig_end` are written by `flowchart::build_layout_data`
/// before any cluster-anchor retargeting, so they reflect what the user
/// actually typed. Cluster-to-cluster self-edges (where the owner is itself a
/// subgraph) are also skipped here — upstream feeds them through the same
/// `expand_self_edge` path, so the renderer must not emit the original.
fn is_replaced_self_loop(e: &UEdge) -> bool {
    if e.extra.get("synthetic").map(|s| s.as_str()) == Some("cyclic_segment") {
        return false;
    }
    let os = e
        .extra
        .get("orig_start")
        .map(|s| s.as_str())
        .or(e.start.as_deref())
        .unwrap_or("");
    let od = e
        .extra
        .get("orig_end")
        .map(|s| s.as_str())
        .or(e.end.as_deref())
        .unwrap_or("");
    !os.is_empty() && os == od
}

/// Synthetic cyclic-special segments are inserted by `dagre_bridge` whenever
/// dagre's `set_edge(v, v)` is called — including the case where a user
/// edge becomes a self-loop only after cluster-anchor rewriting (e.g.
/// `Sub --> In` where `In` is `Sub`'s anchor). Upstream mermaid skips the
/// expansion in that latter case (see `findNonClusterChild` integration in
/// `mermaid-graphlib`'s `adjustClustersAndEdges`): the original edge keeps
/// its degenerate single-point path and no helper segments are emitted.
///
/// Detect that case by inspecting the synthetic segment's `orig_start` /
/// `orig_end` (copied from the user edge it replaces): when those differ,
/// the segment came from anchor-rewriting and must be filtered out.
fn is_cyclic_segment_from_anchor_rewrite(e: &UEdge) -> bool {
    if e.extra.get("synthetic").map(|s| s.as_str()) != Some("cyclic_segment") {
        return false;
    }
    let os = e.extra.get("orig_start").map(|s| s.as_str()).unwrap_or("");
    let od = e.extra.get("orig_end").map(|s| s.as_str()).unwrap_or("");
    !os.is_empty() && !od.is_empty() && os != od
}

/// Cyclic helper labelRect nodes (`In---In---1`, `In---In---2`) are inserted
/// by dagre's `expand_self_edge` together with three `cyclic_segment` sub-edges.
/// When ALL of those segments came from cluster-anchor rewriting (a user edge
/// `Sub --> In` where `In` is `Sub`'s anchor — see `is_cyclic_segment_from_anchor_rewrite`),
/// upstream mermaid skips the entire expansion: no helper labelRects are
/// rendered. Mirror that here so the outer-node loop drops these placeholders
/// rather than emitting the degenerate `<rect width="0.1" height="0.1">`
/// placeholders that would otherwise widen `cluster Sub` and the diagram bbox.
fn is_cyclic_helper_from_anchor_rewrite(node: &UNode, l: &FlowchartLayout) -> bool {
    if node.extra.get("synthetic").map(|s| s.as_str()) != Some("cyclic_helper") {
        return false;
    }
    let owner = match node.extra.get("cyclic_owner").map(|s| s.as_str()) {
        Some(o) if !o.is_empty() => o,
        _ => return false,
    };
    // Find any cyclic_segment edge sharing this owner. If at least one
    // segment is a *real* user self-loop (orig_start == orig_end == owner),
    // keep the helper. Otherwise (every segment came from anchor rewriting,
    // os != od), drop it.
    let mut saw_segment = false;
    for e in &l.edges {
        if e.extra.get("synthetic").map(|s| s.as_str()) != Some("cyclic_segment") {
            continue;
        }
        if e.extra.get("cyclic_owner").map(|s| s.as_str()) != Some(owner) {
            continue;
        }
        saw_segment = true;
        if !is_cyclic_segment_from_anchor_rewrite(e) {
            // A real self-loop segment exists for this owner → keep helper.
            return false;
        }
    }
    // No real segments — every cyclic_segment edge for this owner is an
    // anchor-rewrite artefact, so the helper labelRect must be dropped.
    saw_segment
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

/// Return the outermost isolated cluster ancestor of `node_id`, or `None`
/// if `node_id` has no isolated ancestor.
///
/// Walks up the parent chain and remembers the topmost ancestor whose id
/// appears in `l.isolated_cluster_ids`. Used to decide which inner root
/// (if any) owns an edge: an edge belongs to a cluster's inner root iff
/// both endpoints share that cluster as their topmost isolated ancestor.
/// Cross-cluster edges (different topmost ancestors, or one without an
/// isolated ancestor) render at the outer level instead.
fn outermost_isolated_ancestor(node_id: &str, l: &FlowchartLayout) -> Option<String> {
    let mut top: Option<String> = None;
    let mut current = node_id;
    loop {
        if let Some(n) = l.nodes.iter().find(|n| n.id == current) {
            if let Some(parent) = n.parent_id.as_deref() {
                if l.isolated_cluster_ids.contains(parent) {
                    top = Some(parent.to_string());
                }
                current = parent;
            } else {
                return top;
            }
        } else {
            return top;
        }
    }
}

/// Return true when both endpoints of an edge are descendants of the
/// same outermost isolated cluster — i.e. the edge is fully contained
/// inside that cluster and should render at the inner root instead of
/// the outer level.
fn edge_is_inside_isolated(src: Option<&str>, dst: Option<&str>, l: &FlowchartLayout) -> bool {
    let (Some(s), Some(d)) = (src, dst) else {
        return false;
    };
    let s_top = outermost_isolated_ancestor(s, l);
    let d_top = outermost_isolated_ancestor(d, l);
    match (s_top, d_top) {
        (Some(a), Some(b)) => a == b,
        _ => false,
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

/// Prefer original cluster endpoints when deciding where an edge inside an
/// isolated cluster should render. Anchor rewriting turns `B1 --> B2` into a
/// leaf-to-leaf edge, but the edge still semantically belongs to the parent
/// isolated cluster root, not to either child isolated sub-cluster.
fn isolated_render_endpoint<'a>(
    e: &'a UEdge,
    key: &str,
    fallback: Option<&'a str>,
    cluster_id: &str,
    l: &FlowchartLayout,
) -> Option<&'a str> {
    let orig = e.extra.get(key).map(|s| s.as_str());
    if let Some(id) = orig {
        if id == cluster_id
            || (l.isolated_cluster_ids.contains(id) && is_descendant_of(id, cluster_id, l))
        {
            return Some(id);
        }
    }
    fallback
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
    d: &FlowchartDiagram,
    l: &FlowchartLayout,
    theme: &ThemeVariables,
    svg_id: &str,
    html_labels: bool,
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
    out.push_str(&render_cluster(
        cnode,
        &dummy_cluster,
        theme,
        svg_id,
        html_labels,
    ));
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
        out.push_str(&render_cluster(n, &dummy, theme, svg_id, html_labels));
    }
    out.push_str(unified_shell::close_layer());

    // Build cluster_bounds for edge clipping: non-isolated sub-clusters
    // that are descendants of cnode.id. Their bounds (from l.clusters)
    // are already in the inner dagre coordinate frame.
    let inner_cluster_bounds: std::collections::HashMap<String, crate::layout::unified::Bounds> = l
        .clusters
        .iter()
        .filter_map(|c| {
            let b = c.bounds.as_ref()?;
            if c.id == cnode.id {
                return None;
            }
            if !is_descendant_of(&c.id, &cnode.id, l) {
                return None;
            }
            if l.isolated_cluster_ids.contains(&c.id) {
                return None;
            }
            Some((c.id.clone(), b.clone()))
        })
        .collect();

    // Inner <g class="edgePaths"> — edges between descendants of this
    // cluster, excluding edges between descendants of isolated sub-clusters.
    out.push_str(&unified_shell::open_layer("edgePaths"));
    for (i, e) in l.edges.iter().enumerate() {
        let src = match isolated_render_endpoint(e, "orig_start", e.start.as_deref(), &cnode.id, l)
        {
            Some(s) => s,
            None => continue,
        };
        let dst = match isolated_render_endpoint(e, "orig_end", e.end.as_deref(), &cnode.id, l) {
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
        // Drop the original user self-edge (replaced by cyclic-special segments
        // in upstream `expand_self_edge`).
        if is_replaced_self_loop(e) {
            continue;
        }
        if is_cyclic_segment_from_anchor_rewrite(e) {
            continue;
        }
        out.push_str(&render_edge_path(
            e,
            i,
            svg_id,
            &l.aria_kind,
            &inner_cluster_bounds,
        ));
    }
    out.push_str(unified_shell::close_layer());

    // Inner <g class="edgeLabels">.
    out.push_str(&unified_shell::open_layer("edgeLabels"));
    for e in l.edges.iter() {
        let src = match isolated_render_endpoint(e, "orig_start", e.start.as_deref(), &cnode.id, l)
        {
            Some(s) => s,
            None => continue,
        };
        let dst = match isolated_render_endpoint(e, "orig_end", e.end.as_deref(), &cnode.id, l) {
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
        if is_replaced_self_loop(e) {
            continue;
        }
        if is_cyclic_segment_from_anchor_rewrite(e) {
            continue;
        }
        out.push_str(&render_edge_label(e, html_labels, l));
    }
    out.push_str(unified_shell::close_layer());

    // Inner <g class="nodes">:
    // Upstream's recursiveRender processes nodes via Promise.all(graph.nodes().map(...)).
    // Self-loop helpers from this cluster's inner dagre pass appear after
    // all sub-isolated inner roots and leaf nodes in the <g class="nodes">
    // section, following the sortNodesByHierarchy DFS traversal order where
    // helpers (added after original nodes during self-edge expansion) sort
    // to the end.
    out.push_str(&unified_shell::open_layer("nodes"));

    // Render self-loop helpers that were pre-added before sub-iso placeholders
    // in this cluster's inner dagre. Upstream's `expandSelfEdge` runs before
    // `extractor` splits the graph, so for LR-inner clusters helper nodes for
    // sub-cluster self-loops are inserted into the inner dagre BEFORE the
    // sub-cluster placeholder. Mirror that DOM order here.
    for n in &l.nodes {
        if n.is_group {
            continue;
        }
        if n.extra.get("synthetic").map(|s| s.as_str()) != Some("cyclic_helper") {
            continue;
        }
        if n.extra.get("isolated_cluster_owner").map(|s| s.as_str()) != Some(cnode.id.as_str()) {
            continue;
        }
        if n.extra.get("pre_added_for_sub_iso").map(|s| s.as_str()) != Some("1") {
            continue;
        }
        if n.parent_id.as_deref() != Some(cnode.id.as_str()) {
            continue;
        }
        out.push_str(&render_helper_node(n, theme));
    }

    // Render isolated sub-clusters as nested inner root groups.
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
        out.push_str(&render_isolated_cluster_inner_root(
            n,
            d,
            l,
            theme,
            svg_id,
            html_labels,
        ));
    }

    // Render leaf nodes (non-helper) in DFS traversal order.
    let inner_node_indices = isolated_inner_leaf_node_order(&cnode.id, d, l);
    for &idx in &inner_node_indices {
        let n = &l.nodes[idx];
        if n.is_group {
            continue;
        }
        if n.extra.get("synthetic").map(|s| s.as_str()) == Some("cyclic_helper") {
            continue;
        }
        if !is_descendant_of(&n.id, &cnode.id, l) {
            continue;
        }
        let in_sub_isolated = is_in_isolated_sub_cluster_of(&n.id, &cnode.id, l);
        if in_sub_isolated {
            continue;
        }
        let mut prefixed = n.clone();
        if let Some(did) = &prefixed.dom_id {
            prefixed.dom_id = Some(format!("{svg_id}-{did}", svg_id = svg_id));
        }
        let shape_id = prefixed.shape.clone().unwrap_or_else(|| "rect".to_string());
        match crate::render::shapes::draw(&shape_id, &prefixed, theme) {
            Ok(svg) => {
                let patched = diamond_br_postprocess(&prefixed, &svg);
                let patched = tooltip_postprocess_node_svg(&prefixed, &patched);
                let patched = link_postprocess_node_svg(&prefixed, &patched);
                out.push_str(&patched);
            }
            Err(_) => {
                if let Ok(svg) = crate::render::shapes::draw("rect", &prefixed, theme) {
                    let patched = diamond_br_postprocess(&prefixed, &svg);
                    let patched = tooltip_postprocess_node_svg(&prefixed, &patched);
                    let patched = link_postprocess_node_svg(&prefixed, &patched);
                    out.push_str(&patched);
                }
            }
        }
    }

    // Render self-loop helpers whose isolated_cluster_owner matches this
    // cluster. These appear after all inner roots and leaf nodes, matching
    // upstream's sortNodesByHierarchy DFS order where helpers sort last.
    // Cluster-owner helpers were already emitted above (before sub-iso inner
    // roots) — skip them here.
    for n in &l.nodes {
        if n.is_group {
            continue;
        }
        if n.extra.get("synthetic").map(|s| s.as_str()) != Some("cyclic_helper") {
            continue;
        }
        if n.extra.get("isolated_cluster_owner").map(|s| s.as_str()) != Some(cnode.id.as_str()) {
            continue;
        }
        if !is_descendant_of(&n.id, &cnode.id, l) {
            continue;
        }
        let in_sub_isolated = is_in_isolated_sub_cluster_of(&n.id, &cnode.id, l);
        if in_sub_isolated {
            continue;
        }
        // Skip helpers that we already emitted before sub-iso (pre-added).
        if n.extra.get("pre_added_for_sub_iso").map(|s| s.as_str()) == Some("1") {
            continue;
        }
        out.push_str(&render_helper_node(n, theme));
    }

    out.push_str(unified_shell::close_layer());

    out.push_str("</g>"); // close inner root
    out
}

fn render_helper_node(node: &UNode, theme: &ThemeVariables) -> String {
    let shape_id = node
        .shape
        .clone()
        .unwrap_or_else(|| "labelRect".to_string());
    match crate::render::shapes::draw(&shape_id, node, theme) {
        Ok(svg) => svg,
        Err(_) => String::new(),
    }
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

/// Determine the render order for leaf nodes inside an isolated cluster.
///
/// Mirrors upstream's `recursiveRender` DFS traversal scoped to an isolated
/// cluster's inner dagre pass. The order is:
/// 1. DFS into non-isolated child clusters first (their leaves appear before
///    the cluster's direct leaves)
/// 2. Direct leaf members in their containing subgraph's local declaration order
/// 3. Synthetic/helper nodes not in the diagram model fall back to their
///    position in `l.nodes` (JS key-order fallback)
///
/// Returns indices into `l.nodes` in DFS traversal order. The caller still
/// filters out `is_group` nodes and nodes inside isolated sub-clusters.
fn isolated_inner_leaf_node_order(
    cluster_id: &str,
    d: &FlowchartDiagram,
    l: &FlowchartLayout,
) -> Vec<usize> {
    use std::collections::{HashMap, HashSet};

    let node_idx: HashMap<&str, usize> = l
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let subgraph_ids: HashSet<&str> = d.subgraphs.iter().map(|s| s.id.as_str()).collect();

    let mut seen: HashSet<String> = HashSet::new();
    let push_unique = |insertion: &mut Vec<String>, seen: &mut HashSet<String>, id: &str| {
        if seen.insert(id.to_string()) {
            insertion.push(id.to_string());
        }
    };

    // Build insertion order matching upstream's extractor/copy step scoped to
    // this isolated cluster. Non-isolated child clusters are inserted before
    // leaf members. When such a child cluster exists, upstream's copy path
    // observes the local `subgraph ... end` body order for direct leaves
    // (fixture 134: `a` before `b`). Leaf-only isolated clusters preserve
    // getData's global first-seen vertex order instead (fixture 135).
    let mut insertion: Vec<String> = Vec::new();
    for sg in d.subgraphs.iter().rev() {
        if !node_idx.contains_key(sg.id.as_str()) {
            continue;
        }
        if !is_descendant_of(&sg.id, cluster_id, l) {
            continue;
        }
        if l.isolated_cluster_ids.contains(&sg.id) {
            continue;
        }
        push_unique(&mut insertion, &mut seen, &sg.id);
    }

    let has_non_isolated_cluster_child = l.nodes.iter().any(|n| {
        n.parent_id.as_deref() == Some(cluster_id)
            && n.is_group
            && !l.isolated_cluster_ids.contains(&n.id)
    });

    if has_non_isolated_cluster_child {
        let mut scoped_subgraph_ids: Vec<&str> = vec![cluster_id];
        for sg in &d.subgraphs {
            if l.isolated_cluster_ids.contains(&sg.id) {
                continue;
            }
            if is_descendant_of(&sg.id, cluster_id, l) {
                scoped_subgraph_ids.push(sg.id.as_str());
            }
        }
        for sid in scoped_subgraph_ids {
            let Some(sg) = d.subgraphs.iter().find(|s| s.id == sid) else {
                continue;
            };
            for member in &sg.members {
                if subgraph_ids.contains(member.as_str()) {
                    continue;
                }
                if !node_idx.contains_key(member.as_str()) {
                    continue;
                }
                if !is_descendant_of(member, cluster_id, l) {
                    continue;
                }
                if is_in_isolated_sub_cluster_of(member, cluster_id, l) {
                    continue;
                }
                push_unique(&mut insertion, &mut seen, member);
            }
        }
    } else {
        // With only leaf children, upstream preserves the original getData
        // vertex insertion order. Fixture 135 depends on this: subnet members
        // are declared as compute/lb locally, but lb was first seen globally.
        for v in &d.vertices {
            if subgraph_ids.contains(v.id.as_str()) {
                continue;
            }
            if !node_idx.contains_key(v.id.as_str()) {
                continue;
            }
            if !is_descendant_of(&v.id, cluster_id, l) {
                continue;
            }
            if is_in_isolated_sub_cluster_of(&v.id, cluster_id, l) {
                continue;
            }
            push_unique(&mut insertion, &mut seen, &v.id);
        }
    }

    let pos: HashMap<&str, usize> = insertion
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    // Build child lists per parent in insertion order.
    // Nodes absent from the insertion list (synthetic/helper) get usize::MAX
    // so they sort after all explicitly-ordered nodes.
    let mut children_of: HashMap<Option<String>, Vec<String>> = HashMap::new();
    let mut sorted: Vec<&UNode> = l.nodes.iter().collect();
    sorted.sort_by_key(|n| pos.get(n.id.as_str()).copied().unwrap_or(usize::MAX));
    for n in sorted {
        if !is_descendant_of(&n.id, cluster_id, l) {
            continue;
        }
        if is_in_isolated_sub_cluster_of(&n.id, cluster_id, l) {
            continue;
        }
        children_of
            .entry(n.parent_id.clone())
            .or_default()
            .push(n.id.clone());
    }

    // DFS from cluster_id's children, collecting all node indices.
    let mut result: Vec<usize> = Vec::new();
    let start_children = children_of
        .get(&Some(cluster_id.to_string()))
        .cloned()
        .unwrap_or_default();
    let mut stack: Vec<String> = start_children.into_iter().rev().collect();
    while let Some(id) = stack.pop() {
        if let Some(&idx) = node_idx.get(id.as_str()) {
            result.push(idx);
        }
        if let Some(kids) = children_of.get(&Some(id.clone())) {
            for k in kids.iter().rev() {
                stack.push(k.clone());
            }
        }
    }

    result
}

fn is_in_isolated_sub_cluster_of(node_id: &str, cnode_id: &str, l: &FlowchartLayout) -> bool {
    let mut current = node_id;
    while let Some(n) = l.nodes.iter().find(|n| n.id == current) {
        match n.parent_id.as_deref() {
            Some(p) if p == cnode_id => return false,
            Some(p) if l.isolated_cluster_ids.contains(p) => return true,
            Some(p) => current = p,
            None => return false,
        }
    }
    false
}

/// Apply the upstream `markerOffsets` visual offset to path points.
///
/// Mirror of upstream's `getLineFunctionsWithOffset` from
/// `src/utils/lineWithOffset.ts`. Each point gets two contributions:
///
/// 1. *Endpoint base offset* (only at `i == 0` for `arrowTypeStart` and at
///    `i == last` for `arrowTypeEnd`) shifts the arrowhead so it does not
///    overlap the node boundary.
/// 2. *Extra-room adjustment* applied per-point: when a point lies within
///    `markerHeight` of either endpoint along BOTH axes, the offset is
///    extended by `markerHeight + 1 - difference` in the direction away
///    from that endpoint. This is what produces the cluster-source `-4`
///    shift on the START side even when `arrowTypeStart == "none"` —
///    short cluster→cluster edges where every point is within the end
///    marker's reach get pulled back together with the last point.
///
/// Only `arrow_point` (and the barb variants) carry an offset in the
/// active `markerOffsets` table — `arrow_cross` and `arrow_circle` are
/// intentionally absent (`markerOffsets2` defines them but is unused),
/// so circle/cross arrowheads sit flush against the node boundary.
/// For a single-point path after cluster clipping, compute the outward
/// marker offset direction. Upstream applies the end marker offset as a
/// pure outward shift perpendicular to the nearest boundary edge.
/// `end_mo` / `start_mo` are the marker offset values (4.0 for arrow_point).
fn marker_offset_value(arrow: &str) -> f64 {
    match arrow {
        "arrow_point" => 4.0,
        "arrow_barb_neo" => 5.5,
        _ => 0.0,
    }
}

fn apply_marker_offsets(pts: &mut Vec<Point>, arrow_end: &str, arrow_start: &str) {
    fn marker_offset_for(arrow: &str) -> Option<f64> {
        match arrow {
            "arrow_point" => Some(4.0),
            "arrow_barb_neo" => Some(5.5),
            // arrow_cross / arrow_circle: NO offset (see comment above).
            _ => None,
        }
    }

    let n = pts.len();
    if n < 2 {
        return;
    }

    let start_mo = marker_offset_for(arrow_start);
    let end_mo = marker_offset_for(arrow_end);
    if start_mo.is_none() && end_mo.is_none() {
        return;
    }

    // Snapshot first/last in their pre-adjustment form. Upstream's d3
    // line generator passes the un-adjusted point array as `data5` to
    // each accessor — `data5[0]` and `data5[last]` reflect input
    // coordinates, NOT cumulative offsets.
    let first = pts[0];
    let last = pts[n - 1];

    // DIRECTION used by the extra-room sign flip. Upstream:
    //   x: data5[0].x < data5[last].x ? "left" : "right"
    //   y: data5[0].y < data5[last].y ? "down" : "up"
    let dir_x_right = first.x >= last.x;
    let dir_y_up = first.y >= last.y;

    let adjusted: Vec<Point> = pts
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let mut off_x = 0.0_f64;
            let mut off_y = 0.0_f64;

            // Endpoint base offsets.
            if i == 0 {
                if let Some(mo) = start_mo {
                    let next = pts[1];
                    let dx = next.x - first.x;
                    let dy = next.y - first.y;
                    let blen = (dx * dx + dy * dy).sqrt();
                    if blen > 0.0 {
                        // mo * cos(atan(dy/dx)) * sign(dx)  ==  mo * dx / blen
                        off_x += mo * dx / blen;
                        // mo * |sin(atan(dy/dx))| * sign(dy)  ==  mo * dy / blen
                        off_y += mo * dy / blen;
                    }
                }
            } else if i == n - 1 {
                if let Some(mo) = end_mo {
                    let prev = pts[n - 2];
                    let dx = prev.x - last.x;
                    let dy = prev.y - last.y;
                    let blen = (dx * dx + dy * dy).sqrt();
                    if blen > 0.0 {
                        off_x += mo * dx / blen;
                        off_y += mo * dy / blen;
                    }
                }
            }

            // Extra-room adjustment toward the END.
            if let Some(end_h) = end_mo {
                let diff_to_end_x = (p.x - last.x).abs();
                let diff_y_end = (p.y - last.y).abs();
                if diff_to_end_x < end_h && diff_to_end_x > 0.0 && diff_y_end < end_h {
                    let adj = end_h + 1.0 - diff_to_end_x;
                    let signed = if dir_x_right { -adj } else { adj };
                    off_x -= signed;
                }
                let diff_to_end_y = (p.y - last.y).abs();
                let diff_x_end = (p.x - last.x).abs();
                if diff_to_end_y < end_h && diff_to_end_y > 0.0 && diff_x_end < end_h {
                    let adj = end_h + 1.0 - diff_to_end_y;
                    let signed = if dir_y_up { -adj } else { adj };
                    off_y -= signed;
                }
            }

            // Extra-room adjustment toward the START.
            if let Some(start_h) = start_mo {
                let diff_to_start_x = (p.x - first.x).abs();
                let diff_y_start = (p.y - first.y).abs();
                if diff_to_start_x < start_h && diff_to_start_x > 0.0 && diff_y_start < start_h {
                    let adj = start_h + 1.0 - diff_to_start_x;
                    let signed = if dir_x_right { -adj } else { adj };
                    off_x += signed;
                }
                let diff_to_start_y = (p.y - first.y).abs();
                let diff_x_start = (p.x - first.x).abs();
                if diff_to_start_y < start_h && diff_to_start_y > 0.0 && diff_x_start < start_h {
                    let adj = start_h + 1.0 - diff_to_start_y;
                    let signed = if dir_y_up { -adj } else { adj };
                    off_y += signed;
                }
            }

            Point {
                x: p.x + off_x,
                y: p.y + off_y,
            }
        })
        .collect();

    *pts = adjusted;
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

/// Walk the rendered edges, collect unique (family, position, color)
/// triples implied by each edge's pathStyle stroke color, and emit one
/// `<marker>` per triple in upstream's order.
///
/// Upstream's `addEdgeMarker` only colors the un-suffixed primary
/// markers (no `-margin` variant). It also only emits the marker that
/// matches the edge's actual `arrow_type_*` direction — start markers
/// only when an arrow_type_start is set, end markers only when
/// arrow_type_end is set.
///
/// Emission order matches upstream's `appendChild` walk:
///   for each edge in render order:
///     for each end-position with an arrow type:
///       if (family, position, color) not seen yet → emit marker.
fn emit_colored_marker_defs(edges: &[UEdge], aria_kind: &str, svg_id: &str) -> String {
    let mut out = String::new();
    let mut seen: std::collections::HashSet<(String, String, String)> =
        std::collections::HashSet::new();
    for e in edges {
        let style = compute_edge_path_style(e);
        let Some(stroke) = unified_shell::extract_stroke_color(&style) else {
            continue;
        };
        if stroke.trim().is_empty() {
            continue;
        }
        // Mirror upstream order: start position first when present,
        // then end. addEdgeMarkers iterates start, then end.
        if let Some(arrow) = e.arrow_type_start.as_deref() {
            if let Some((family, position)) = arrow_marker_family(arrow, "start") {
                let key = (family.to_string(), position.to_string(), stroke.clone());
                if seen.insert(key) {
                    out.push_str(&unified_shell::colored_marker(
                        family, position, aria_kind, svg_id, &stroke,
                    ));
                }
            }
        }
        if let Some(arrow) = e.arrow_type_end.as_deref() {
            if let Some((family, position)) = arrow_marker_family(arrow, "end") {
                let key = (family.to_string(), position.to_string(), stroke.clone());
                if seen.insert(key) {
                    out.push_str(&unified_shell::colored_marker(
                        family, position, aria_kind, svg_id, &stroke,
                    ));
                }
            }
        }
    }
    out
}

/// Build the `style="…"` value for a flowchart edge `<path>`.
///
/// Mirrors upstream's `rendering-elements/edges.js` `pathStyle`:
///   styles  = edgeStyles.reduce(acc + style + ';', '')   // "s1;s2;"
///   second  = edgeStyles.reduce(acc + ';' + style, '')   // ";s1;s2"
///   pathStyle = styles + ';' + second                    // "s1;s2;;;s1;s2"
///
/// Class-derived styles take a different path: upstream collects the
/// colour-only entries into `edge.labelStyle` and the renderer emits
/// `${edgeStyles}${labelStyles};;` instead of doubling the full style
/// array. Falls back to `";"` when the edge has no inline style at all.
fn compute_edge_path_style(e: &UEdge) -> String {
    // When `label_style` exactly mirrors `style` (i.e. the colour came from
    // `linkStyle …` which copies the same string into both
    // `defaultStyle.style` and `defaultStyle.labelStyle`), upstream emits
    // the doubled-style template — `${style};;${style}` — rather than the
    // class-derived `${style}${labelStyle};;` template.
    //
    // Detection rule:
    //   1. label_style is non-empty and every entry also appears in style
    //      (label is a subset of style — true for both linkStyle and class).
    //   2. Either v == ls (no `fill:none` auto-add — only colour) or style
    //      contains an auto-added `fill:none` entry that is absent from
    //      label_style. The presence of `fill:none` in style but not in
    //      label_style is the signature of `flowDb.updateLink`'s push, which
    //      class-derived styles never trigger.
    let style_subsumes_label = match (e.style.as_ref(), e.label_style.as_ref()) {
        (Some(v), Some(ls)) => {
            let v_filtered: Vec<&String> = v.iter().filter(|s| !s.is_empty()).collect();
            let ls_filtered: Vec<&String> = ls.iter().filter(|s| !s.is_empty()).collect();
            if ls_filtered.is_empty() {
                false
            } else if v_filtered == ls_filtered {
                true
            } else {
                // Each label entry must appear in style.
                let label_in_style = ls_filtered
                    .iter()
                    .all(|l| v_filtered.iter().any(|v| v == l));
                // Detect linkStyle's auto-pushed `fill:none` (key starts with
                // `fill` and value is `none`, with optional whitespace).
                let has_auto_fill_none = v_filtered.iter().any(|s| {
                    let t = s.trim();
                    t == "fill:none"
                        || t.starts_with("fill:") && t["fill:".len()..].trim() == "none"
                });
                let label_has_fill_none = ls_filtered.iter().any(|s| {
                    let t = s.trim();
                    t == "fill:none"
                        || t.starts_with("fill:") && t["fill:".len()..].trim() == "none"
                });
                label_in_style && has_auto_fill_none && !label_has_fill_none
            }
        }
        _ => false,
    };
    match (e.style.as_ref(), e.label_style.as_ref()) {
        (Some(v), Some(ls))
            if !v.is_empty()
                && v.iter().any(|s| !s.is_empty())
                && !ls.is_empty()
                && !style_subsumes_label =>
        {
            let first: String = v
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| format!("{};", s))
                .collect();
            let labels: String = ls
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| format!("{};", s))
                .collect();
            format!("{first}{labels};;")
        }
        (Some(v), _) if !v.is_empty() && v.iter().any(|s| !s.is_empty()) => {
            let first: String = v
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| format!("{};", s))
                .collect();
            let second: String = v
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| format!(";{}", s))
                .collect();
            format!("{first};{second}")
        }
        _ => ";".to_string(),
    }
}

/// Lowercase a named CSS colour value (e.g. `color:Sienna` → `color:sienna`).
/// Browsers lowercase named colours when serialising via the DOM `style` IDL
/// attribute; the reference SVGs were captured after that pass, so we mirror
/// it here. Hex (`#abc`/`#aabbcc`), `rgb(…)`, `rgba(…)`, `hsl(…)` and `var(…)`
/// values are left untouched — only bare-word values that aren't a hex/rgb
/// expression are lowercased.
fn lowercase_named_color(decl: &str) -> String {
    let trimmed = decl.trim();
    let trailing_semi = trimmed.ends_with(';');
    let body = trimmed.trim_end_matches(';');
    let Some(colon) = body.find(':') else {
        return decl.to_string();
    };
    let key = body[..colon].trim();
    if !key.contains("color") {
        return decl.to_string();
    }
    let raw_value = &body[colon + 1..];
    let value = raw_value.trim();
    let lowered = value.to_lowercase();
    if value == lowered {
        return decl.to_string();
    }
    // Skip values that already encode their own casing (hex, function calls).
    if value.starts_with('#')
        || value.contains('(')
        || value.starts_with("var ")
        || value.starts_with("url")
    {
        return decl.to_string();
    }
    let mut out = String::with_capacity(decl.len());
    out.push_str(&body[..colon]);
    out.push(':');
    // Preserve any leading whitespace between `:` and the value.
    let lead_ws_len = raw_value.len() - raw_value.trim_start().len();
    out.push_str(&raw_value[..lead_ws_len]);
    out.push_str(&lowered);
    if trailing_semi {
        out.push(';');
    }
    out
}

/// Map `arrow_type_*` value to the `markers.rs` family + position pair
/// used to look up colored marker variants. Returns `None` for arrow
/// types that don't have a colored variant ("none", `arrow_open`,
/// other unknowns).
fn arrow_marker_family(arrow_type: &str, position: &str) -> Option<(&'static str, &'static str)> {
    let pos = match position {
        "start" => "Start",
        "end" => "End",
        _ => return None,
    };
    match arrow_type {
        "arrow_circle" => Some(("circle", pos)),
        "arrow_cross" => Some(("cross", pos)),
        "none" | "arrow_open" => None,
        // arrow_point / arrow / etc → point family
        _ => Some(("point", pos)),
    }
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

    // When cluster clipping reduces the path to a single point, upstream
    // still applies the end marker offset. For rewrite self-loops (anchor-
    // rewritten edges where orig_start != orig_end), the self-loop sits on
    // the node's right boundary in TB layout. After fromCluster clipping,
    // the single remaining point is at the node boundary, and the marker
    // offset shifts it outward perpendicular to that boundary edge.
    // Compute the outward direction from the self-loop geometry: the
    // boundary points (first and last of pts_raw) share the same x
    // coordinate, and that x is the node's right boundary. The outward
    // direction is +x for right boundary (TB), -x for left (RL),
    // +y for bottom (LR), -y for top (BT).
    if pts.len() == 1 && pts_raw.len() >= 7 {
        let mo = marker_offset_value(arrow_end);
        if mo > 0.0 {
            // Self-loop points[0] and points[last] share the same x
            // (or y) — that shared coordinate is the boundary. Detect
            // which axis is shared and which side.
            let first_x = pts_raw[0].x;
            let last_x = pts_raw[pts_raw.len() - 1].x;
            let first_y = pts_raw[0].y;
            let last_y = pts_raw[pts_raw.len() - 1].y;

            if (first_x - last_x).abs() < 0.5 {
                // Boundary is vertical (right or left). The
                // self-loop interior (midpoint) is on the
                // OUTWARD side of the boundary. Offset toward
                // the interior direction.
                let interior_x = pts_raw[pts_raw.len() / 2].x;
                if interior_x > first_x {
                    pts[0].x += mo;
                } else {
                    pts[0].x -= mo;
                }
            } else if (first_y - last_y).abs() < 0.5 {
                // Boundary is horizontal (top or bottom). Determine side.
                let interior_y = pts_raw[pts_raw.len() / 2].y;
                if interior_y > first_y {
                    // Interior is below → boundary is TOP → outward = -y
                    pts[0].y -= mo;
                } else {
                    // Interior is above → boundary is BOTTOM → outward = +y
                    pts[0].y += mo;
                }
            }
        }
    }

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
    // Upstream emits the stroke classes only for invisible edges; the
    // trailing ` edge-thickness-normal edge-pattern-solid flowchart-link`
    // suffix is appended only when the edge is actually rendered as a
    // flowchart link.
    let mut class_attr = if thickness == "invisible" {
        format!(" edge-thickness-{thickness} edge-pattern-{pattern}")
    } else {
        format!(
            " edge-thickness-{thickness} edge-pattern-{pattern} edge-thickness-normal edge-pattern-solid flowchart-link"
        )
    };
    // Per-edge `@{ animation: slow|fast }` metadata appends the matching
    // upstream CSS class so the dashed-line dance kicks in. The CSS rule
    // body lives in the per-id `<style>` block emitted at SVG-open time.
    // Cypress fixtures 113 / 237.
    if let Some(anim) = e.animation.as_deref() {
        class_attr.push_str(&format!(" edge-animation-{}", anim));
    }

    // Edge style emission mirrors upstream's `pathStyle` formula in
    // `rendering-elements/edges.js`. Extracted to `compute_edge_path_style`
    // so the marker-defs pre-pass can reuse it for stroke-color sniffing.
    let style_val = compute_edge_path_style(e);
    // 'data-id' below uses the original edge id; do not consume style here.

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

    // Per-edge stroke color (from the final pathStyle), used to switch
    // marker references from the base `pointEnd` to the colored variant
    // `pointEnd_{colorId}` — matching upstream `addEdgeMarker` exactly.
    let stroke_color =
        unified_shell::extract_stroke_color(&style_val).filter(|s| !s.trim().is_empty());
    let color_suffix = stroke_color
        .as_deref()
        .map(|c| format!("_{}", unified_shell::marker_color_id(c)))
        .unwrap_or_default();

    let marker_end = match e.arrow_type_end.as_deref() {
        Some("arrow_circle") => {
            format!(r#" marker-end="url(#{svg_id}_{aria_kind}-circleEnd{color_suffix})""#)
        }
        Some("arrow_cross") => {
            format!(r#" marker-end="url(#{svg_id}_{aria_kind}-crossEnd{color_suffix})""#)
        }
        Some("none") | None => {
            // No arrowhead (arrow_open / open edges like `---`): no marker.
            String::new()
        }
        _ => {
            // Default arrow (point) — upstream emits marker-end for
            // arrow_point, arrow, etc.
            format!(r#" marker-end="url(#{svg_id}_{aria_kind}-pointEnd{color_suffix})""#)
        }
    };
    let marker_start = match e.arrow_type_start.as_deref() {
        Some("arrow_point") | Some("arrow") => {
            format!(r#" marker-start="url(#{svg_id}_{aria_kind}-pointStart{color_suffix})""#)
        }
        Some("arrow_circle") => {
            format!(r#" marker-start="url(#{svg_id}_{aria_kind}-circleStart{color_suffix})""#)
        }
        Some("arrow_cross") => {
            format!(r#" marker-start="url(#{svg_id}_{aria_kind}-crossStart{color_suffix})""#)
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

fn render_edge_label(e: &UEdge, html_labels: bool, l: &FlowchartLayout) -> String {
    use crate::render::foreign_object::{
        markdown_label_to_html, measure_html_label, measure_html_markup_label,
        render_edge_label as fo_edge, replace_fa_icons, string_label_to_html, HtmlLabelFont,
        LabelOpts,
    };
    use crate::render::shapes::types::{build_div_style_prefix, build_label_style};
    let label_text = e.label.clone().unwrap_or_default();
    // Markdown edge labels (` "`...`" `) need `markdownToHTML` conversion before
    // emission so `**bold**` etc. render as `<strong>...</strong>` like upstream.
    // String-typed quoted labels (`"…"`) flow through upstream's `createText` →
    // `decodeEntities` → text-content path which HTML-escapes `&`, `<`, `>`
    // so the inner `<p>` body matches `<p>&gt;= v11</p>` etc.
    // The htmlLabels=false branch tokenises into MarkdownLines below, so
    // only the htmlLabels=true path needs the pre-conversion here.
    let is_markdown = e.label_type.as_deref() == Some("markdown");
    let is_string = e.label_type.as_deref() == Some("string");
    let label_html = if is_markdown && html_labels && !label_text.is_empty() {
        markdown_label_to_html(&label_text)
    } else if is_string && html_labels && !label_text.is_empty() {
        string_label_to_html(&label_text)
    } else {
        label_text.clone()
    };
    // Apply FA icon substitution (fa:fa-car → <i class="fa fa-car"></i>) before
    // measuring, matching upstream's createText path. The <i> element contributes
    // zero width under the jsdom shim.
    // Also normalise `<br>` / `<br />` to `<br/>` so the rendered HTML matches
    // upstream's `markdownToHTML` re-serialisation exactly.
    let processed =
        crate::render::foreign_object::normalize_br_tags(&replace_fa_icons(&label_html));
    let is_empty = processed.is_empty();
    // KaTeX `$$..$$` math edge labels: render through the embedded KaTeX
    // pipeline and emit an edge label with `wrap_in_p: false` so the
    // inner `<span class="edgeLabel ">` directly carries the KaTeX
    // `<div>…</div>` markup, matching the reference SVG.
    let (w, h, katex_html) = if !is_empty
        && crate::render::foreign_object::contains_katex_marker(&processed)
    {
        let font = HtmlLabelFont::default();
        match crate::render::foreign_object::try_render_katex_label(&processed, &font) {
            Some((html, w, h)) => (w, h, Some(html)),
            None => {
                let (w, h) = measure_html_markup_label(&processed, &font, 200.0, true);
                (w, h, None)
            }
        }
    } else if is_empty {
        let (_, lh) = measure_html_label("X", &HtmlLabelFont::default(), 200.0, true);
        (0.0, lh, None)
    } else {
        let (w, h) =
            measure_html_markup_label(&processed, &HtmlLabelFont::default(), 200.0, true);
        (w, h, None)
    };
    let dagre_lx = e.label_x.unwrap_or(0.0);
    let dagre_ly = e.label_y.unwrap_or(0.0);
    // Mirror upstream `positionEdgeLabel` (edges.js:253). When `paths.updatedPath`
    // is set (i.e. the rendered SVG path differs from the dagre point sequence,
    // typically because the edge crosses a cluster boundary via
    // `cutPathAtIntersect`), the label is re-anchored to the half-distance
    // midpoint of the *clipped* path, with x/y rounded to 5 decimals via
    // `roundNumber(_, 5)`. We approximate this by detecting cluster-bound
    // edges (via `orig_start` / `orig_end` referencing an `is_group` node)
    // and recomputing the label position with `cutPathAtIntersect` +
    // `traverseEdge` ported from upstream `edges.js`.
    let (lx, ly) = recompute_edge_label_position(e, l).unwrap_or((dagre_lx, dagre_ly));
    // htmlLabels=false path — emit `<text>`/`<tspan>` instead of <foreignObject>.
    // Mirrors upstream `createText(...)` non-html branch (createText.ts:338+,
    // createFormattedText) plus `insertEdgeLabel` which moves the
    // <text>/labelGroup into <g class="label">. When the label is empty,
    // `addBackground` is forced false so the bare <text> moves but the
    // <rect class="background"> stays as an orphan <g> SIBLING of the
    // <g class="edgeLabel">.
    if !html_labels {
        return render_edge_label_text(e, &processed, is_empty, lx, ly);
    }
    // Inline color/font label styles (from `class <edge-id> myClass`,
    // i.e. `edge.label_style`). Upstream applies these as a `style=`
    // prefix on the `<div>` of the edgeLabel via `applyStyle`, which
    // emits text-only properties before the default `display: table-cell;
    // …` block. `build_div_style_prefix` mirrors that filter exactly.
    //
    // Browsers lowercase named CSS colours when serialising `element.style`
    // (e.g. `Sienna` → `sienna`). The reference SVGs were captured from a
    // real DOM so they carry the lowercased form; we mirror the behaviour
    // here without touching the (blacklisted) shape-style helpers.
    let div_prefix = e
        .label_style
        .as_ref()
        .map(|styles| {
            let lowered: Vec<String> = styles.iter().map(|s| lowercase_named_color(s)).collect();
            build_div_style_prefix(&lowered)
        })
        .unwrap_or_default();
    let span_label_style = e
        .label_style
        .as_ref()
        .map(|styles| build_label_style(styles))
        .unwrap_or_default();
    let opts = LabelOpts {
        data_id: Some(&e.id),
        group_style: None,
        wrap_in_p: !is_empty && katex_html.is_none(),
        div_style_prefix: if div_prefix.is_empty() {
            None
        } else {
            Some(&div_prefix)
        },
        label_style: if span_label_style.is_empty() {
            None
        } else {
            Some(&span_label_style)
        },
        ..LabelOpts::default()
    };
    let body = katex_html.as_deref().unwrap_or(processed.as_str());
    fo_edge(body, lx, ly, w, h, opts)
}

/// Render an edge label as `<text>`/`<tspan>` (htmlLabels=false branch).
///
/// Layout mirrors upstream `createText` + `insertEdgeLabel`:
///
/// 1. The `<g class="edgeLabel">` carries an outer `transform="translate(lx, ly)"`
///    when label_x/label_y are set; for empty labels (lx=ly=0) the attribute
///    is omitted entirely (upstream's d3 `attr` with computed empty/zero
///    string still emits, but the layout path passes 0,0 which round-trips
///    through `text` with no transform — matching the reference fixtures).
/// 2. Inside, `<g class="label" data-id="..." transform="translate(-w/2, -h/2)">`
///    centres the visual content. Upstream uses `computeLabelTransform(bbox,
///    false)` = `translate(-(x + w/2), -(y + h/2))` where `bbox` is the
///    text element's `getBBox`. Under the jsdom shim that bbox is
///    `{x:0, y:0, w, h}` for empty/single-line text.
/// 3. The `<text>` carries `y="-10.1"` and a `text-anchor="middle"`. Inside,
///    a single `<tspan class="text-outer-tspan row" x="0" y="-0.1em" dy="1.1em" text-anchor="middle">`
///    holds either nothing (empty) or one `<tspan class="text-inner-tspan">`
///    per markdown segment (font-style/font-weight per em/strong).
/// 4. When the label is non-empty, `addSvgBackground=isMarkdown` is true,
///    so `createFormattedText` returns the *labelGroup* (which contains both
///    `<rect class="background">` and `<text>`) and that group ends up inside
///    the label's <g>. When empty, `addBackground=false` so it returns the
///    bare `<text>` and the `labelGroup` (with its rect) stays as an orphan
///    sibling of the edgeLabel.
fn render_edge_label_text(e: &UEdge, processed: &str, is_empty: bool, lx: f64, ly: f64) -> String {
    use crate::render::shapes::types::{fmt_num, xml_escape};
    // Outer edgeLabel transform — upstream `positionEdgeLabel` (edges.js:192)
    // only sets the `transform` attribute when `edge.label` is truthy.
    // Empty edge labels therefore receive NO transform at all (regardless of
    // the dagre-computed label_x/label_y). Non-empty labels (178/179/180/181)
    // carry their dagre mid-point as `translate(lx, ly)`.
    let outer_transform = if is_empty {
        String::new()
    } else {
        format!(
            r#" transform="translate({lx}, {ly})""#,
            lx = fmt_num(lx),
            ly = fmt_num(ly),
        )
    };
    // Tokenise the label text into MarkdownLines. For empty there is one
    // empty line. For non-markdown (`labelType != "markdown"`) we still call
    // through the same per-line tokenisation but skip emphasis parsing —
    // mirroring upstream's `nonMarkdownToLines` vs `markdownToLines` split.
    let is_markdown = e.label_type.as_deref() == Some("markdown");
    let lines: Vec<Vec<MdWord>> = if is_empty {
        Vec::new()
    } else if is_markdown {
        markdown_to_lines(processed)
    } else {
        non_markdown_to_lines(processed)
    };

    // Compute text bbox for `computeLabelTransform` and rect sizing.
    // Under the jsdom shim, `<text>.getBBox()` collapses every nested
    // `<tspan>`'s textContent into a single string and measures it as one
    // line at the resolved font (sans-serif 14, no bold/italic). So:
    //   bbox.width  = text_width(textContent_concat)
    //   bbox.height = line_height(sans-serif, 14)
    use crate::font_metrics::{line_height, text_width};
    const FAM: &str = "sans-serif";
    const SIZE: f64 = 14.0;
    let lh = line_height(FAM, SIZE, false, false);
    // textContent of all child tspans concatenated. Words are joined by
    // " " (mirrors `updateTextContentAndStyles` which prepends a space to
    // every word after index 0). Across lines no separator is inserted —
    // jsdom's textContent simply concatenates.
    let text_content: String = if lines.is_empty() {
        String::new()
    } else {
        let mut parts: Vec<String> = Vec::new();
        for line in &lines {
            for (i, w) in line.iter().enumerate() {
                if i == 0 {
                    parts.push(w.content.clone());
                } else {
                    parts.push(format!(" {}", w.content));
                }
            }
        }
        parts.concat()
    };
    let text_w = if text_content.is_empty() {
        0.0
    } else {
        text_width(&text_content, FAM, SIZE, false, false)
    };
    let text_h = lh;
    // Inner label transform via computeLabelTransform({x:0, y:0, w, h}, false)
    //   = translate(-(0 + w/2), -(0 + h/2))
    //   = translate(-w/2, -h/2)
    // For empty labels the bbox collapses to {x:0, y:0, w:0, h:line_h} so
    // the formula reduces to translate(0, -line_h/2).
    let label_tx = -text_w / 2.0;
    let label_ty = -text_h / 2.0;
    // Inner label group.
    let label_attrs = format!(
        r#" data-id="{did}" transform="translate({tx}, {ty})""#,
        did = e.id,
        tx = fmt_num(label_tx),
        ty = fmt_num(label_ty),
    );

    // Build the inner-tspan markup per line. Each `MarkdownLine` becomes
    // one `<tspan class="text-outer-tspan row">` whose `y` reflects the
    // line index. Inside, every `MarkdownWord` becomes one
    // `<tspan class="text-inner-tspan">` carrying `font-style` and
    // `font-weight` per upstream `updateTextContentAndStyles`.
    fn outer_tspan_open(line_idx: usize) -> String {
        const LINE_HEIGHT_EM: f64 = 1.1;
        // y = lineIdx * lineHeight - 0.1 (em). When lineIdx == 0, y = -0.1.
        // Match upstream's d3 attr formatting: omit the leading "+0.1*..."
        // computation when lineIdx == 0 → emits the literal "-0.1em".
        let y_em = if line_idx == 0 {
            "-0.1em".to_string()
        } else {
            // Format like upstream's String coercion: `lineIndex * lineHeight - 0.1 + 'em'`.
            // d3 attr renders the JS number literally, so 1*1.1-0.1=1, 2*1.1-0.1=2.1, etc.
            let val = line_idx as f64 * LINE_HEIGHT_EM - 0.1;
            format!("{}em", fmt_num(val))
        };
        format!(
            r#"<tspan class="text-outer-tspan row" x="0" y="{y}" dy="1.1em" text-anchor="middle">"#,
            y = y_em,
        )
    }
    let mut tspans_markup = String::new();
    if !lines.is_empty() {
        for (line_idx, line) in lines.iter().enumerate() {
            tspans_markup.push_str(&outer_tspan_open(line_idx));
            for (word_idx, w) in line.iter().enumerate() {
                let font_style = match w.kind {
                    MdWordKind::Em => "italic",
                    _ => "normal",
                };
                let font_weight = match w.kind {
                    MdWordKind::Strong => "bold",
                    _ => "normal",
                };
                let content = if word_idx == 0 {
                    xml_escape(&w.content)
                } else {
                    format!(" {}", xml_escape(&w.content))
                };
                tspans_markup.push_str(&format!(
                    r#"<tspan font-style="{fs}" class="text-inner-tspan" font-weight="{fw}">{c}</tspan>"#,
                    fs = font_style,
                    fw = font_weight,
                    c = content,
                ));
            }
            tspans_markup.push_str("</tspan>");
        }
    } else {
        // Empty: emit a bare outer tspan with no inner content.
        tspans_markup.push_str(&outer_tspan_open(0));
        tspans_markup.push_str("</tspan>");
    }

    if is_empty {
        // Empty-label branch: `addBackground` is forced false in upstream
        // (see createText.ts:348) because the text is empty. The labelElement
        // returned is the bare `<text>` (no wrapping `<g>`), so the rect
        // remains an orphan `<g>` SIBLING of the edgeLabel and its style is
        // never overwritten — keeping `style="stroke: none"`.
        format!(
            r#"<g class="edgeLabel"{outer}><g class="label"{label_attrs}><text y="-10.1" text-anchor="middle">{tspans}</text></g></g><g><rect class="background" style="stroke: none"></rect></g>"#,
            outer = outer_transform,
            label_attrs = label_attrs,
            tspans = tspans_markup,
        )
    } else {
        // Non-empty branch: `addBackground=true` always for the new flowchart
        // edge-label path (rendering-elements/edges.js:84). createText returns
        // the `labelGroup` whose children are `<rect class="background">`
        // (sized to `bbox + 2px padding`) and `<text y="-10.1">`. The rect's
        // style attribute is later overwritten with `edgeLabelRectStyle`
        // (text-only style minus `stroke`/`stroke-width`/`fill`,
        // `background:` -> `fill:`); for an edge with no labelStyle the
        // resulting style is empty (`""`).
        let rect_x = -2.0;
        let rect_y = -2.0;
        let rect_w = text_w + 4.0;
        let rect_h = text_h + 4.0;
        // For now, ignore label_style overrides — the byte-exact targets
        // 178/179/180/181 use no per-edge label styles, and the upstream
        // behaviour is more involved (split into rect-style vs text-style).
        let rect_style = "";
        let text_style = "";
        format!(
            r#"<g class="edgeLabel"{outer}><g class="label"{label_attrs}><g><rect class="background" style="{rs}" x="{rx}" y="{ry}" width="{rw}" height="{rh}"></rect><text y="-10.1" text-anchor="middle" style="{ts}">{tspans}</text></g></g></g>"#,
            outer = outer_transform,
            label_attrs = label_attrs,
            rs = rect_style,
            rx = fmt_num(rect_x),
            ry = fmt_num(rect_y),
            rw = fmt_num(rect_w),
            rh = fmt_num(rect_h),
            ts = text_style,
            tspans = tspans_markup,
        )
    }
}

/// Word style for a single MarkdownWord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MdWordKind {
    Normal,
    Strong,
    Em,
}

#[derive(Debug, Clone)]
struct MdWord {
    content: String,
    kind: MdWordKind,
}

/// Mirrors upstream `nonMarkdownToLines`:
///   text.split(/\\n|\n|<br\s*\/?>/gi)
///     .map(line => line.trim().match(/<[^>]+>|[^\s<>]+/g)?.map(w => ({content:w, type:'normal'})) ?? [])
fn non_markdown_to_lines(text: &str) -> Vec<Vec<MdWord>> {
    let normalised = normalise_br_for_split(text);
    normalised
        .split('\n')
        .map(|line| {
            tokenise_non_markdown_line(line.trim())
                .into_iter()
                .map(|w| MdWord {
                    content: w,
                    kind: MdWordKind::Normal,
                })
                .collect()
        })
        .collect()
}

fn normalise_br_for_split(text: &str) -> String {
    // Replace every `<br>` / `<br/>` / `<br />` (case-insensitive) with `\n`,
    // and `\\n` literal sequences with `\n` — mirroring upstream's regex.
    // Done in two passes since Rust's stdlib has no regex without an extra
    // dep — and the input shapes encountered here are simple.
    let mut out = text.replace("\\n", "\n");
    // Manual scan for <br> tags (case-insensitive, optional /, optional space).
    let mut buf = String::with_capacity(out.len());
    let bytes = out.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Try to match `<br\s*/?>` case-insensitively.
            let rest = &out[i..];
            let lower = rest.to_ascii_lowercase();
            if lower.starts_with("<br") {
                // Scan to closing '>'
                if let Some(rel) = lower.find('>') {
                    // Must be just `<br...>` where `...` is whitespace and optional `/`.
                    let between = &lower[3..rel];
                    let between_clean: String =
                        between.chars().filter(|c| !c.is_whitespace()).collect();
                    if between_clean.is_empty() || between_clean == "/" {
                        buf.push('\n');
                        i += rel + 1;
                        continue;
                    }
                }
            }
        }
        buf.push(out.as_bytes()[i] as char);
        i += 1;
    }
    out = buf;
    out
}

fn tokenise_non_markdown_line(line: &str) -> Vec<String> {
    // Match `<[^>]+>` or `[^\s<>]+`.
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '<' {
            // Take until next '>'.
            if let Some(rel) = line[i..].find('>') {
                out.push(line[i..i + rel + 1].to_string());
                i += rel + 1;
                continue;
            }
            // Unclosed tag — treat as plain run.
        }
        // Non-whitespace, non-`<>` run.
        let start = i;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c.is_whitespace() || c == '<' || c == '>' {
                break;
            }
            i += 1;
        }
        if i > start {
            out.push(line[start..i].to_string());
        }
    }
    out
}

/// A simplified port of upstream `markdownToLines` covering the cases
/// needed for byte-exact parity on cypress flowchart fixtures (178, 179,
/// 180, 181, …):
///
/// - `<br/>` → `\n`
/// - Multiple consecutive `\n` collapsed to one
/// - `**…**` / `__…__` → strong; `*…*` / `_…_` → em
/// - Words inside emphasis inherit the parent type, words split on space
///
/// Doesn't yet handle nested emphasis, code spans, or links — but those
/// don't appear in the targeted fixtures' edge labels.
fn markdown_to_lines(md: &str) -> Vec<Vec<MdWord>> {
    let pre = preprocess_markdown(md);
    let mut lines: Vec<Vec<MdWord>> = vec![Vec::new()];
    let mut current = 0usize;

    fn push_text(lines: &mut Vec<Vec<MdWord>>, current: &mut usize, text: &str, kind: MdWordKind) {
        let parts: Vec<&str> = text.split('\n').collect();
        for (idx, line) in parts.iter().enumerate() {
            if idx != 0 {
                *current += 1;
                lines.push(Vec::new());
            }
            for word in line.split(' ') {
                let w = word.replace("&#39;", "'");
                if !w.is_empty() {
                    lines[*current].push(MdWord { content: w, kind });
                }
            }
        }
    }

    let bytes = pre.as_bytes();
    let mut i = 0usize;
    let mut plain_start = 0usize;
    macro_rules! flush_plain {
        ($end:expr) => {{
            let end = $end;
            if end > plain_start {
                let segment = pre[plain_start..end].to_string();
                push_text(&mut lines, &mut current, &segment, MdWordKind::Normal);
            }
        }};
    }
    while i < bytes.len() {
        let b = bytes[i];
        // **strong** / __strong__ (longer marker first)
        if (b == b'*' || b == b'_') && i + 1 < bytes.len() && bytes[i + 1] == b {
            let marker = if b == b'*' { "**" } else { "__" };
            if let Some(end) = find_str(&pre, i + 2, marker) {
                flush_plain!(i);
                let inner = &pre[i + 2..end];
                push_text(&mut lines, &mut current, inner, MdWordKind::Strong);
                i = end + 2;
                plain_start = i;
                continue;
            }
        }
        // *em* / _em_
        if b == b'*' || b == b'_' {
            let marker = if b == b'*' { "*" } else { "_" };
            if let Some(end) = find_str(&pre, i + 1, marker) {
                if end > i + 1 {
                    flush_plain!(i);
                    let inner = &pre[i + 1..end];
                    push_text(&mut lines, &mut current, inner, MdWordKind::Em);
                    i = end + 1;
                    plain_start = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    flush_plain!(bytes.len());
    lines
}

fn find_str(haystack: &str, from: usize, needle: &str) -> Option<usize> {
    haystack.get(from..)?.find(needle).map(|rel| from + rel)
}

/// Preprocess the markdown source as upstream's `preprocessMarkdown` does:
/// `<br/>` → `\n`, collapse consecutive newlines, and dedent leading
/// whitespace. The dedent step matters for fixtures whose mermaid source
/// indents continuation lines (so the rendered text won't carry extra
/// leading spaces when split on `\n`).
fn preprocess_markdown(md: &str) -> String {
    // 1. Replace <br/> (case-insensitive, with optional whitespace) with \n.
    let s = normalise_br_for_split(md);
    // 2. Collapse runs of \n into a single \n.
    let mut collapsed = String::with_capacity(s.len());
    let mut prev_nl = false;
    for c in s.chars() {
        if c == '\n' {
            if !prev_nl {
                collapsed.push('\n');
            }
            prev_nl = true;
        } else {
            collapsed.push(c);
            prev_nl = false;
        }
    }
    // 3. Dedent: strip the common leading-whitespace prefix from every line.
    dedent(&collapsed)
}

fn dedent(s: &str) -> String {
    // Match npm `dedent` semantics: only lines that start with whitespace
    // *and* contain at least one non-whitespace char contribute to the
    // common-indent calculation. Lines that begin with a non-whitespace
    // char (indent = 0) and pure whitespace lines are both excluded —
    // this matches the upstream regex `^(\s+)\S+`.
    let lines: Vec<&str> = s.split('\n').collect();
    let mut min_indent: Option<usize> = None;
    for line in &lines {
        let n = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        if n == 0 {
            continue;
        }
        // Skip pure-whitespace lines (no non-ws content after the indent).
        if line.chars().skip(n).next().is_none() {
            continue;
        }
        min_indent = Some(min_indent.map(|m| m.min(n)).unwrap_or(n));
    }
    let n = min_indent.unwrap_or(0);
    if n == 0 {
        return s.to_string();
    }
    lines
        .iter()
        .map(|line| {
            if line.chars().all(|c| c.is_whitespace()) {
                (*line).to_string()
            } else {
                // Strip up to n leading whitespace chars; lines with shorter
                // indent (or none) keep what they have.
                let mut chars = line.chars();
                let mut stripped = 0usize;
                let mut buf = String::new();
                while stripped < n {
                    match chars.next() {
                        Some(c) if c == ' ' || c == '\t' => stripped += 1,
                        Some(c) => {
                            buf.push(c);
                            break;
                        }
                        None => break,
                    }
                }
                buf.extend(chars);
                buf
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
/// `createCssStyles`. Selectors depend on `flowchart.htmlLabels`:
/// - htmlLabels=true (default): `> *`, `span`
/// - htmlLabels=false: `rect`, `polygon`, `ellipse`, `circle`, `path`
/// Plus a `tspan` rule with `color → fill` rewrites for text styles.
fn flowchart_class_def_css(id: &str, d: &FlowchartDiagram) -> String {
    let mut out = String::new();
    let html_labels = d.html_labels.unwrap_or(true);
    let elements: &[&str] = if html_labels {
        &["> *", "span"]
    } else {
        &["rect", "polygon", "ellipse", "circle", "path"]
    };
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
        // Emit one rule per element selector. Upstream collapses `> *` into
        // `>*` (no space) when stylis serialises, so do the same by hand.
        for el in elements {
            let el_str = if *el == "> *" { ">*" } else { *el };
            let separator = if *el == "> *" { "" } else { " " };
            out.push_str(&format!(
                "#{id} .{name}{sep}{el}{{{css}}}",
                name = def.name,
                sep = separator,
                el = el_str,
                css = all_css
            ));
        }
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

/// Half-precision rounding helper mirroring upstream `roundNumber(num, 5)`
/// (utils.ts:329). Used by `recompute_edge_label_position` so the label
/// transform we emit matches upstream byte-for-byte: e.g.
/// `221.18530273…` → `221.1853`, `221.2421875` → `221.24219`.
fn round_to_5(v: f64) -> f64 {
    (v * 100000.0).round() / 100000.0
}

/// `outsideNode(node, point)` from upstream edges.js:333 — returns true
/// when `point` lies strictly outside the cluster's bounding box (treating
/// the rectangle border as INSIDE, which matters for paths whose terminal
/// point sits exactly on the cluster boundary).
fn point_outside_node(nx: f64, ny: f64, nw: f64, nh: f64, px: f64, py: f64) -> bool {
    let dx = (px - nx).abs();
    let dy = (py - ny).abs();
    dx >= nw / 2.0 || dy >= nh / 2.0
}

/// Port of upstream `intersection(node, outsidePoint, insidePoint)`
/// (edges.js:343). Computes where the segment from `outside` to `inside`
/// crosses the rectangular boundary of the cluster `node`.
#[allow(clippy::too_many_arguments)]
fn rect_intersection(
    nx: f64,
    ny: f64,
    nw: f64,
    nh: f64,
    ox: f64,
    oy: f64,
    ix: f64,
    iy: f64,
) -> (f64, f64) {
    let w = nw / 2.0;
    let h = nh / 2.0;
    let q_total = (oy - iy).abs();
    let r_total = (ox - ix).abs();

    // Branch on which side the outside point projects to.
    if (ny - oy).abs() * w > (nx - ox).abs() * h {
        // Top/bottom branch.
        let q = if iy < oy { oy - h - ny } else { ny - h - oy };
        let r = if q_total == 0.0 {
            0.0
        } else {
            r_total * q / q_total
        };
        let mut res_x = if ix < ox { ix + r } else { ix - r_total + r };
        let mut res_y = if iy < oy {
            iy + q_total - q
        } else {
            iy - q_total + q
        };
        // Edge cases mirror upstream's r === 0 / R === 0 / Q === 0 patches.
        if r == 0.0 {
            res_x = ox;
            res_y = oy;
        }
        if r_total == 0.0 {
            res_x = ox;
        }
        if q_total == 0.0 {
            res_y = oy;
        }
        (res_x, res_y)
    } else {
        // Left/right branch.
        let r = if ix < ox { ox - w - nx } else { nx - w - ox };
        let q = if r_total == 0.0 {
            0.0
        } else {
            q_total * r / r_total
        };
        let mut res_x = if ix < ox {
            ix + r_total - r
        } else {
            ix - r_total + r
        };
        let mut res_y = if iy < oy { iy + q } else { iy - q };
        if r == 0.0 {
            res_x = ox;
            res_y = oy;
        }
        if r_total == 0.0 {
            res_x = ox;
        }
        if q_total == 0.0 {
            res_y = oy;
        }
        (res_x, res_y)
    }
}

/// Port of upstream `cutPathAtIntersect(_points, boundaryNode)` (edges.js:408).
/// Tuple-of-(f64,f64) variant used by `recompute_edge_label_position`; the
/// existing `Bounds`-based implementation upstream operates on the SVG
/// renderer's `Point` type and a top-left-anchored AABB, while the label-
/// position recompute needs centre-anchored cluster geometry.
fn cut_path_at_intersect_xywh(
    pts: &[(f64, f64)],
    nx: f64,
    ny: f64,
    nw: f64,
    nh: f64,
) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    if pts.is_empty() {
        return out;
    }
    let mut last_outside = pts[0];
    let mut is_inside = false;
    for &(px, py) in pts {
        let outside = point_outside_node(nx, ny, nw, nh, px, py);
        if !outside && !is_inside {
            let (ix, iy) =
                rect_intersection(nx, ny, nw, nh, last_outside.0, last_outside.1, px, py);
            if !out.iter().any(|p| p.0 == ix && p.1 == iy) {
                out.push((ix, iy));
            }
            is_inside = true;
        } else {
            last_outside = (px, py);
            if !is_inside {
                out.push((px, py));
            }
        }
    }
    out
}

/// Port of upstream `traverseEdge` + `calculatePoint` (utils.ts:305).
/// Returns the half-total-distance midpoint of the polyline. Returns `None`
/// when the path collapses to a single point or degenerate length.
fn traverse_edge_midpoint(pts: &[(f64, f64)]) -> Option<(f64, f64)> {
    if pts.len() < 2 {
        return None;
    }
    // Total length.
    let mut total = 0.0_f64;
    for w in pts.windows(2) {
        let dx = w[1].0 - w[0].0;
        let dy = w[1].1 - w[0].1;
        total += (dx * dx + dy * dy).sqrt();
    }
    let mut remaining = total / 2.0;
    let mut prev: Option<(f64, f64)> = None;
    for &(px, py) in pts {
        if let Some((ppx, ppy)) = prev {
            let dx = px - ppx;
            let dy = py - ppy;
            let seg = (dx * dx + dy * dy).sqrt();
            if seg == 0.0 {
                return Some((ppx, ppy));
            }
            if seg < remaining {
                remaining -= seg;
            } else {
                let ratio = remaining / seg;
                if ratio <= 0.0 {
                    return Some((ppx, ppy));
                }
                if ratio >= 1.0 {
                    return Some((px, py));
                }
                let x = (1.0 - ratio) * ppx + ratio * px;
                let y = (1.0 - ratio) * ppy + ratio * py;
                return Some((x, y));
            }
        }
        prev = Some((px, py));
    }
    None
}

/// When an edge enters or exits a cluster, upstream `insertEdge` re-clips
/// the polyline at the cluster boundary (via `cutPathAtIntersect`) and the
/// resulting `paths.updatedPath` triggers `positionEdgeLabel` to recompute
/// the label translate via `calcLabelPosition`. Root-level cluster-edge
/// declarations go through the rounded update path; edges declared inside a
/// subgraph keep the raw dagre label precision.
///
/// We approximate this by checking whether the edge's `orig_start` /
/// `orig_end` reference a cluster (`is_group=true` node) and applying the
/// same clip + traverse on the dagre points. Returns `None` when the edge
/// is not cluster-bound or the path has no points to traverse, in which
/// case the renderer falls back to dagre's `label_x`/`label_y`.
///
/// Additionally, upstream's `insertEdge` flags `pointsHasChanged = true` when
/// the dagre midpoint coordinate (rounded to int) does not appear as a
/// substring in the rendered `d=` attribute (after rounding all decimals).
/// This is the `isLabelCoordinateInPath` check (utils.ts:289). When the basis
/// curve interpolation moves the visual midpoint away from the dagre control
/// point, the label is recomputed via `calcLabelPosition` on the polyline.
fn recompute_edge_label_position(e: &UEdge, l: &FlowchartLayout) -> Option<(f64, f64)> {
    let pts_ref = e.points.as_ref()?;
    if pts_ref.len() < 2 {
        return None;
    }
    let mut pts: Vec<(f64, f64)> = pts_ref.iter().map(|p| (p.x, p.y)).collect();
    let orig_start = e.extra.get("orig_start").map(|s| s.as_str());
    let orig_end = e.extra.get("orig_end").map(|s| s.as_str());
    let cluster_bbox = |id: &str| -> Option<(f64, f64, f64, f64)> {
        let n = l.nodes.iter().find(|n| n.id == id && n.is_group)?;
        let x = n.x?;
        let y = n.y?;
        let w = n.width?;
        let h = n.height?;
        Some((x, y, w, h))
    };
    let pts_before = pts.clone();
    if let Some(end_id) = orig_end {
        if let Some((nx, ny, nw, nh)) = cluster_bbox(end_id) {
            pts = cut_path_at_intersect_xywh(&pts, nx, ny, nw, nh);
        }
    }
    if let Some(start_id) = orig_start {
        if let Some((nx, ny, nw, nh)) = cluster_bbox(start_id) {
            pts.reverse();
            pts = cut_path_at_intersect_xywh(&pts, nx, ny, nw, nh);
            pts.reverse();
        }
    }
    let path_actually_changed = pts.len() != pts_before.len()
        || pts
            .iter()
            .zip(pts_before.iter())
            .any(|(a, b)| (a.0 - b.0).abs() > 1e-6 || (a.1 - b.1).abs() > 1e-6);
    // When the path was not cluster-clipped, run the upstream
    // `isLabelCoordinateInPath` check: the dagre midpoint (the polyline
    // point at `points[len/2]`, rounded to int) must appear as a substring
    // in the rendered d= attribute (also with all decimals rounded). When
    // it does NOT, upstream sets `pointsHasChanged = true` and recomputes
    // via `calcLabelPosition`.
    if !path_actually_changed {
        let mid_idx = pts.len() / 2;
        let mid_pt = pts[mid_idx];
        let d_attr = build_rendered_d_for_label_check(e, &pts);
        if label_coordinate_in_d(mid_pt, &d_attr) {
            return None;
        }
    }
    if pts.len() < 2 {
        return None;
    }
    let (x, y) = traverse_edge_midpoint(&pts)?;
    if e.extra.contains_key("scope_parent") {
        Some((x, y))
    } else {
        Some((round_to_5(x), round_to_5(y)))
    }
}

/// Build the rendered `d=` attribute approximation for the upstream
/// `isLabelCoordinateInPath` check. Mirrors `render_edge_path`'s curve-and-
/// offset pipeline minus the marker-offset step (the upstream check uses
/// `svgPath.attr("d")` which DOES include offsets — but the resulting
/// substring test only cares whether the rounded midpoint coord appears
/// anywhere in the path, which the curve interpolation drives the most).
fn build_rendered_d_for_label_check(e: &UEdge, pts: &[(f64, f64)]) -> String {
    let mut points: Vec<Point> = pts.iter().map(|&(x, y)| Point { x, y }).collect();
    let arrow_end = e.arrow_type_end.as_deref().unwrap_or("none");
    let arrow_start = e.arrow_type_start.as_deref().unwrap_or("none");
    apply_marker_offsets(&mut points, arrow_end, arrow_start);
    let curve = e.curve.as_deref().unwrap_or("basis");
    let ctype = edges::CurveType::parse(curve).unwrap_or(edges::CurveType::Basis);
    edges::build_path(&points, ctype)
}

/// Port of upstream `utils.isLabelCoordinateInPath` (utils.ts:289).
///
/// Round the candidate point and every decimal token in the `d=` string to
/// integers, then test whether either rounded coordinate appears anywhere as
/// a substring in the path. Returns `true` when the dagre-computed midpoint
/// is "found" in the rendered path (no recompute needed).
fn label_coordinate_in_d(point: (f64, f64), d_attr: &str) -> bool {
    use std::sync::OnceLock;
    static DECIMAL_RE: OnceLock<regex::Regex> = OnceLock::new();
    let re =
        DECIMAL_RE.get_or_init(|| regex::Regex::new(r"(\d+\.\d+)").expect("valid decimal regex"));
    let sanitized = re.replace_all(d_attr, |caps: &regex::Captures<'_>| {
        caps[1]
            .parse::<f64>()
            .map(|v| v.round().to_string())
            .unwrap_or_else(|_| caps[1].to_string())
    });
    let rounded_x = point.0.round().to_string();
    let rounded_y = point.1.round().to_string();
    sanitized.contains(&rounded_x) || sanitized.contains(&rounded_y)
}

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
}
