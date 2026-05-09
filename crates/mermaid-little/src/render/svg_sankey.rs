//! Sankey SVG renderer — byte-exact parity with upstream
//! mermaid@11.14.0's output for every fixture in
//! tests/ext_fixtures/{cypress,demos}/sankey.
//!
//! Upstream renderer:
//! /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/sankey/sankeyRenderer.ts
//!
//! ## Byte-exact considerations
//! * `viewBox` is computed from the jsdom `getBBox` shim (see
//!   tests/support/generate_ref.mjs lines 460-550). That shim ignores
//!   SVG transforms — only each element's *intrinsic* geometry (rect
//!   attrs, path `d` anchors + control points, text `{0,0,w,h}`) is
//!   considered. We replicate that identically here.
//! * All numeric attrs go through [`js_num`] to match JS's
//!   `Number.prototype.toString()` (scientific form outside `[1e-6, 1e21)`).
//! * `Uid` counters start at 0 fresh per render. After the `K` nodes,
//!   the next `K+1` is the first `linearGradient-*` id.
//!
//! Portions adapted from mermaid-rs-renderer
//! (<https://github.com/1jehuang/mermaid-rs-renderer>, MIT license) —
//! only for the ordinal-palette trick and the high-level structural
//! scaffolding. All layout math and attribute ordering is
//! independently ported from upstream mermaid + d3-sankey.

use crate::error::Result;
use crate::layout::sankey::{SankeyLayout, NODE_WIDTH};
use crate::model::sankey::{LinkColor, SankeyDiagram};
use crate::theme::ThemeVariables;

pub fn render(
    d: &SankeyDiagram,
    l: &SankeyLayout,
    _theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(16384);

    // ------------------------------------------------------------------
    // Compute bbox exactly like the jsdom shim would.
    // ------------------------------------------------------------------
    let bbox = compute_bbox(d, l);
    let use_max_width = d.config.use_max_width;

    // Attribute order: id, width, xmlns, style | height + width, viewBox, role, aria-roledescription.
    out.push_str(&format!(r#"<svg id="{id}""#));
    if use_max_width {
        out.push_str(&format!(
            r#" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: {w}px;""#,
            w = js_num(bbox.2),
        ));
    } else {
        out.push_str(&format!(
            r#" xmlns="http://www.w3.org/2000/svg" height="{h}" width="{w}""#,
            h = js_num(bbox.3),
            w = js_num(bbox.2),
        ));
    }
    out.push_str(&format!(
        r#" viewBox="{vx} {vy} {vw} {vh}" role="graphics-document document" aria-roledescription="sankey">"#,
        vx = js_num(bbox.0),
        vy = js_num(bbox.1),
        vw = js_num(bbox.2),
        vh = js_num(bbox.3),
    ));

    // ------------------------------------------------------------------
    // <style>
    // ------------------------------------------------------------------
    out.push_str(&build_style_block(id));

    // ------------------------------------------------------------------
    // Anchor <g></g> (upstream always appends an empty group first).
    // ------------------------------------------------------------------
    out.push_str("<g></g>");

    // ------------------------------------------------------------------
    // <g class="nodes"> with per-node <rect>.
    // ------------------------------------------------------------------
    out.push_str(r#"<g class="nodes">"#);
    for (i, node) in l.graph.nodes.iter().enumerate() {
        let uid = i + 1;
        let x0 = node.x0;
        let y0 = node.y0;
        out.push_str(&format!(
            r#"<g class="node" id="node-{uid}" transform="translate({x},{y})" x="{x}" y="{y}"><rect height="{h}" width="{w}" fill="{color}"></rect></g>"#,
            x = js_num(x0),
            y = js_num(y0),
            h = js_num(node.y1 - node.y0),
            w = js_num(node.x1 - node.x0),
            color = &l.node_colors[i],
        ));
    }
    out.push_str("</g>");

    // ------------------------------------------------------------------
    // <g class="node-labels">
    // ------------------------------------------------------------------
    out.push_str(r#"<g class="node-labels" font-size="14">"#);
    let half_width = l.width / 2.0;
    let dy_str = if d.config.show_values {
        "0em"
    } else {
        "0.35em"
    };
    for (i, node) in l.graph.nodes.iter().enumerate() {
        let id_text = &d.nodes[i];
        let text_body = if d.config.show_values {
            let rounded = (node.value * 100.0).round() / 100.0;
            format!(
                "{id}\n{p}{v}{s}",
                id = id_text,
                p = d.config.prefix,
                v = js_num(rounded),
                s = d.config.suffix
            )
        } else {
            id_text.clone()
        };
        let (x, anchor) = if node.x0 < half_width {
            (node.x1 + 6.0, "start")
        } else {
            (node.x0 - 6.0, "end")
        };
        let y = (node.y1 + node.y0) / 2.0;
        out.push_str(&format!(
            r#"<text x="{x}" y="{y}" dy="{dy}" text-anchor="{a}">{t}</text>"#,
            x = js_num(x),
            y = js_num(y),
            dy = dy_str,
            a = anchor,
            t = html_escape(&text_body),
        ));
    }
    out.push_str("</g>");

    // ------------------------------------------------------------------
    // <g class="links">
    // ------------------------------------------------------------------
    out.push_str(r#"<g class="links" fill="none" stroke-opacity="0.5">"#);

    let node_count = l.graph.nodes.len();
    let color_for_link =
        |color_mode: &LinkColor, source_i: usize, target_i: usize, uid: usize| -> String {
            match color_mode {
                LinkColor::Gradient => format!("url(#linearGradient-{uid})"),
                LinkColor::Source => l.node_colors[source_i].clone(),
                LinkColor::Target => l.node_colors[target_i].clone(),
                LinkColor::Custom(c) => c.clone(),
            }
        };

    for (li, link) in l.graph.links.iter().enumerate() {
        let uid = node_count + li + 1;
        let source_node = &l.graph.nodes[link.source];
        let target_node = &l.graph.nodes[link.target];
        let x0 = source_node.x1;
        let x1 = target_node.x0;

        out.push_str(r#"<g class="link" style="mix-blend-mode: multiply;">"#);
        if matches!(d.config.link_color, LinkColor::Gradient) {
            out.push_str(&format!(
                r#"<linearGradient id="linearGradient-{uid}" gradientUnits="userSpaceOnUse" x1="{x1}" x2="{x2}"><stop offset="0%" stop-color="{c1}"></stop><stop offset="100%" stop-color="{c2}"></stop></linearGradient>"#,
                uid = uid,
                x1 = js_num(x0),
                x2 = js_num(x1),
                c1 = &l.node_colors[link.source],
                c2 = &l.node_colors[link.target],
            ));
        }
        let path = sankey_link_horizontal(x0, link.y0, x1, link.y1);
        let stroke = color_for_link(&d.config.link_color, link.source, link.target, uid);
        let width = link.width.max(1.0);
        out.push_str(&format!(
            r#"<path d="{d}" stroke="{s}" stroke-width="{w}"></path>"#,
            d = path,
            s = stroke,
            w = js_num(width),
        ));
        out.push_str("</g>");
    }
    out.push_str("</g>");

    out.push_str("</svg>");
    Ok(out)
}

// -------------------------------------------------------------------------------------------------
// Geometry helpers.
// -------------------------------------------------------------------------------------------------

/// Generate `sankeyLinkHorizontal()` path — mirrors d3-shape's
/// `linkHorizontal()` with source = (d.source.x1, d.y0) and target =
/// (d.target.x0, d.y1). The emitted path is
///     M sx sy C (sx+tx)/2 sy, (sx+tx)/2 ty, tx ty
/// d3-shape renders this via its cubic bezier builder which yields the
/// same canonical string.
fn sankey_link_horizontal(sx: f64, sy: f64, tx: f64, ty: f64) -> String {
    // d3-shape's link.x() / link.y() default to identity; the curve is
    // `curveBumpX` which places both control points at the midpoint x
    // of source/target, matching upstream output.
    let mid = (sx + tx) / 2.0;
    format!(
        "M{sx},{sy}C{mid},{sy},{mid},{ty},{tx},{ty}",
        sx = js_num(sx),
        sy = js_num(sy),
        mid = js_num(mid),
        ty = js_num(ty),
        tx = js_num(tx),
    )
}

/// Compute `getBBox()`-style bbox across every rendered child (rect,
/// path, text), mirroring the jsdom shim in tests/support/generate_ref.mjs.
fn compute_bbox(d: &SankeyDiagram, l: &SankeyLayout) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut has_box = false;

    // Skip empty boxes (width==0 && height==0 ignored by shim).
    let mut add = |x: f64, y: f64, w: f64, h: f64| {
        if w == 0.0 && h == 0.0 {
            return;
        }
        has_box = true;
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

    // <g class="nodes"> rects — intrinsic box `(0, 0, NODE_WIDTH, h)`.
    for node in &l.graph.nodes {
        let h = node.y1 - node.y0;
        add(0.0, 0.0, NODE_WIDTH, h);
    }

    // <g class="node-labels"> texts — (0, 0, measured_w, measured_h).
    let half_width = l.width / 2.0;
    for (i, node) in l.graph.nodes.iter().enumerate() {
        let text = if d.config.show_values {
            let rounded = (node.value * 100.0).round() / 100.0;
            format!(
                "{id}\n{p}{v}{s}",
                id = &d.nodes[i],
                p = d.config.prefix,
                v = js_num(rounded),
                s = d.config.suffix
            )
        } else {
            d.nodes[i].clone()
        };
        let (w, h) = measure_text_block(&text);
        let _ = (node, half_width);
        add(0.0, 0.0, w, h);
    }

    // <g class="links"> paths — `pathBBox` of d attribute (anchors +
    // control points, ignoring stroke width). linearGradient elements
    // are non-visible, skipped.
    for link in &l.graph.links {
        let sx = l.graph.nodes[link.source].x1;
        let tx = l.graph.nodes[link.target].x0;
        let mid = (sx + tx) / 2.0;
        // M sx sy → anchor (sx, sy)
        // C mid sy, mid ty, tx ty → control points (mid, sy), (mid, ty)
        //     and final anchor (tx, ty).
        // pathBBox includes every anchor + control point.
        let mut xs = [sx, mid, mid, tx];
        let mut ys = [link.y0, link.y0, link.y1, link.y1];
        let min_lx = xs.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_lx = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min_ly = ys.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_ly = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        add(min_lx, min_ly, max_lx - min_lx, max_ly - min_ly);
        let _ = (&mut xs, &mut ys); // silence unused-mut warnings
    }

    if !has_box {
        return (0.0, 0.0, 0.0, 0.0);
    }
    (min_x, min_y, max_x - min_x, max_y - min_y)
}

/// Multi-line text measurement matching the Node-side `measureTextBlock`
/// in tests/support/font_metrics.mjs.
fn measure_text_block(text: &str) -> (f64, f64) {
    let family = "sans-serif";
    let size = 14.0_f64;
    let bold = false;
    let italic = false;

    let mut max_w = 0.0_f64;
    let mut line_count = 0usize;
    for line in text.split('\n') {
        let w = crate::font_metrics::text_width(line, family, size, bold, italic);
        if w > max_w {
            max_w = w;
        }
        line_count += 1;
    }
    let lh = crate::font_metrics::line_height(family, size, bold, italic);
    (max_w, lh * line_count as f64)
}

// -------------------------------------------------------------------------------------------------
// <style> block — verbatim from upstream's generated CSS for the sankey
// diagram. We build the string with the id prefixed exactly once per
// rule.
// -------------------------------------------------------------------------------------------------

fn build_style_block(id: &str) -> String {
    let mut s = String::with_capacity(4096);
    s.push_str("<style>");
    // Root-scope font family/size/fill.
    s.push_str(&format!(
        r#"#{id}{{font-family:"trebuchet ms",verdana,arial,sans-serif;font-size:16px;fill:#333;}}"#
    ));
    s.push_str("@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}");
    s.push_str("@keyframes dash{to{stroke-dashoffset:0;}}");
    s.push_str(&format!(
        r#"#{id} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}"#
    ));
    s.push_str(&format!(
        r#"#{id} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}"#
    ));
    s.push_str(&format!(r#"#{id} .error-icon{{fill:#552222;}}"#));
    s.push_str(&format!(
        r#"#{id} .error-text{{fill:#552222;stroke:#552222;}}"#
    ));
    s.push_str(&format!(
        r#"#{id} .edge-thickness-normal{{stroke-width:1px;}}"#
    ));
    s.push_str(&format!(
        r#"#{id} .edge-thickness-thick{{stroke-width:3.5px;}}"#
    ));
    s.push_str(&format!(
        r#"#{id} .edge-pattern-solid{{stroke-dasharray:0;}}"#
    ));
    s.push_str(&format!(
        r#"#{id} .edge-thickness-invisible{{stroke-width:0;fill:none;}}"#
    ));
    s.push_str(&format!(
        r#"#{id} .edge-pattern-dashed{{stroke-dasharray:3;}}"#
    ));
    s.push_str(&format!(
        r#"#{id} .edge-pattern-dotted{{stroke-dasharray:2;}}"#
    ));
    s.push_str(&format!(r#"#{id} .marker{{fill:#333333;stroke:#333333;}}"#));
    s.push_str(&format!(r#"#{id} .marker.cross{{stroke:#333333;}}"#));
    s.push_str(&format!(
        r#"#{id} svg{{font-family:"trebuchet ms",verdana,arial,sans-serif;font-size:16px;}}"#
    ));
    s.push_str(&format!(r#"#{id} p{{margin:0;}}"#));
    s.push_str(&format!(
        r#"#{id} .label{{font-family:"trebuchet ms",verdana,arial,sans-serif;}}"#
    ));
    s.push_str(&format!(r#"#{id} .node .neo-node{{stroke:#9370DB;}}"#));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node rect,#{id} [data-look="neo"].cluster rect,#{id} [data-look="neo"].node polygon{{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}"#
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node path{{stroke:#9370DB;stroke-width:1px;}}"#
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node .outer-path{{filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}"#
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node .neo-line path{{stroke:#9370DB;filter:none;}}"#
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle{{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}"#
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle .state-start{{fill:#000000;}}"#
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon{{fill:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}"#
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon-neo path{{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}"#
    ));
    s.push_str(&format!(
        r#"#{id} :root{{--mermaid-font-family:"trebuchet ms",verdana,arial,sans-serif;}}"#
    ));
    s.push_str("</style>");
    s
}

// -------------------------------------------------------------------------------------------------
// Number formatting — identical to src/render/svg_radar.rs::js_num.
// -------------------------------------------------------------------------------------------------

fn js_num(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let abs = v.abs();
    if !(1e-6..1e21).contains(&abs) {
        let s = format!("{:e}", v);
        if let Some(e_pos) = s.find('e') {
            let exp = &s[e_pos + 1..];
            if !exp.starts_with('-') {
                let mut fixed = String::with_capacity(s.len() + 1);
                fixed.push_str(&s[..=e_pos]);
                fixed.push('+');
                fixed.push_str(exp);
                return fixed;
            }
        }
        s
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
            _ => out.push(c),
        }
    }
    out
}

// -------------------------------------------------------------------------------------------------
// Tests — byte-exact parity.
// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::sankey as layout_mod;
    use crate::parser::sankey as parser_mod;
    use crate::theme::get_theme;

    fn render_fixture(source: &str, id: &str) -> String {
        let diagram = parser_mod::parse(source).expect("parse");
        let theme = get_theme("default");
        let lay = layout_mod::layout(&diagram, &theme).expect("layout");
        super::render(&diagram, &lay, &theme, id).expect("render")
    }

    fn check_fixture(source_path: &str, reference_path: &str, id: &str) {
        let source = std::fs::read_to_string(source_path).expect("source");
        let reference = std::fs::read_to_string(reference_path).expect("reference");
        let got = render_fixture(&source, id);
        if got != reference.trim_end_matches('\n') {
            let got_len = got.len();
            let ref_len = reference.len();
            let mut diff_at = 0;
            for (i, (a, b)) in got.bytes().zip(reference.bytes()).enumerate() {
                if a != b {
                    diff_at = i;
                    break;
                }
            }
            let ctx = 160usize;
            let start = diff_at.saturating_sub(ctx);
            let end_got = (diff_at + ctx).min(got_len);
            let end_ref = (diff_at + ctx).min(ref_len);
            panic!(
                "byte mismatch for {source_path} at byte {diff_at}\n  got: ...{g}...\n  ref: ...{r}...",
                g = &got[start..end_got],
                r = &reference[start..end_ref],
            );
        }
    }

    #[test]
    fn cypress_sankey_01() {
        check_fixture(
            "tests/ext_fixtures/cypress/sankey/01.mmd",
            "tests/reference/ext_fixtures/cypress/sankey/01.svg",
            "ref-ext-fixtures-cypress-sankey-01",
        );
    }

    #[test]
    fn demos_sankey_01() {
        check_fixture(
            "tests/ext_fixtures/demos/sankey/01.mmd",
            "tests/reference/ext_fixtures/demos/sankey/01.svg",
            "ref-ext-fixtures-demos-sankey-01",
        );
    }

    #[test]
    fn demos_sankey_02() {
        check_fixture(
            "tests/ext_fixtures/demos/sankey/02.mmd",
            "tests/reference/ext_fixtures/demos/sankey/02.svg",
            "ref-ext-fixtures-demos-sankey-02",
        );
    }
}
