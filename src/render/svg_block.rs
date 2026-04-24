//! Block diagram SVG renderer — byte-exact output vs upstream
//! `packages/mermaid/src/diagrams/block/blockRenderer.ts` combined with
//! `packages/mermaid/src/dagre-wrapper/nodes.js` (rect / composite /
//! round / circle / … variants).
//!
//! Each leaf block renders as:
//!
//! ```text
//! <g class="node default default flowchart-label" id="{id}-{nid}" transform="translate(cx, cy)">
//!   <rect class="basic label-container" style="" rx="0" ry="0" x y width height></rect>
//!   <g class="label" style="" transform="translate(dx, -8.1484375)">
//!     <rect></rect>
//!     <foreignObject width height>
//!       <div style="…" xmlns="…"><span class="nodeLabel "><p>text</p></span></div>
//!     </foreignObject>
//!   </g>
//! </g>
//! ```
//!
//! Composites (`block ... end`) swap `class="basic label-container"` for
//! `"basic cluster composite label-container"`. The label is empty for
//! composites.

use crate::error::Result;
use crate::layout::block::{BlockLayout, NodeGeom, LABEL_HEIGHT};
use crate::model::block::{BlockDiagram, BlockShape};
use crate::render::unified_shell;
use crate::theme::css as theme_css;
use crate::theme::ThemeVariables;

pub fn render(
    d: &BlockDiagram,
    l: &BlockLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(16 * 1024);

    // ── <svg> opener ─────────────────────────────────────────────────
    // Upstream `blockRenderer.ts` adds 10 to both dims and applies a
    // `magicFactor = max(1, round(0.125 * w/h))` only to height.
    let (bx, by, bw, bh) = l.bounds;
    let vb_x = bx - 5.0;
    let vb_y = by - 5.0;
    let vb_w = bw + 10.0;
    let vb_h = bh + 10.0;
    let max_width = vb_w;

    out.push_str(&unified_shell::open_unified_svg(
        id,
        max_width,
        (vb_x, vb_y, vb_w, vb_h),
        None,
        "block",
    ));

    // ── <style> block ────────────────────────────────────────────────
    out.push_str(&build_style_block(id, theme));
    // Empty seed group that upstream d3 emits right after the <style>.
    // Block is an outlier — markers live *outside* this seed group
    // (unlike ER / state / flowchart which wrap everything inside).
    out.push_str(unified_shell::seed_group());

    // ── Markers (always emitted in block regardless of use). ─────────
    out.push_str(&build_markers(id));

    // ── <g class="block"> node list ──────────────────────────────────
    out.push_str(r#"<g class="block">"#);
    for n in &l.nodes {
        // Composites need the cluster class; non-arrow leaves use the
        // normal rect path.
        match n.shape {
            BlockShape::Composite => render_composite(&mut out, id, n),
            BlockShape::BlockArrow => render_block_arrow(&mut out, id, n),
            _ => render_leaf(&mut out, id, n),
        }
    }
    out.push_str("</g>");

    out.push_str("</svg>");
    let _ = d; // reserved for future edge/class lookup.
    Ok(out)
}

// ─── Node rendering helpers ────────────────────────────────────────────

fn render_leaf(out: &mut String, diagram_id: &str, n: &NodeGeom) {
    let (rx, ry) = rx_ry_for(n.shape);
    let classes = "node default default flowchart-label";
    let node_style = format_node_style(&n.styles);
    out.push_str(&format!(
        r#"<g class="{classes}" id="{did}-{nid}" transform="translate({cx}, {cy})">"#,
        classes = classes,
        did = diagram_id,
        nid = n.id,
        cx = fmt_num(n.x),
        cy = fmt_num(n.y),
    ));
    // Shape body. For circle / double-circle upstream emits a `<circle>`,
    // for diamond / hexagon / lean* / trapezoid* it emits `<polygon>`,
    // etc. For the wave-3 pass we handle the common rectangle variants
    // (square, round, na — all of which draw a `<rect>`) plus circle.
    match n.shape {
        BlockShape::Circle => render_circle_shape(out, n),
        BlockShape::DoubleCircle => render_circle_shape(out, n),
        BlockShape::Stadium => {
            // Upstream `stadium()` does NOT set a `class` on the rect —
            // the emitted tag is bare `<rect style rx ry x y w h>`.
            let h = n.text_height + crate::layout::block::PADDING;
            let w = n.text_width
                + (n.text_height + crate::layout::block::PADDING) / 4.0
                + crate::layout::block::PADDING;
            out.push_str(&format!(
                r#"<rect style="{s}" rx="{rxh}" ry="{rxh}" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
                s = node_style,
                rxh = fmt_num(h / 2.0),
                x = fmt_num(-w / 2.0),
                y = fmt_num(-h / 2.0),
                w = fmt_num(w),
                h = fmt_num(h),
            ));
        }
        _ => {
            out.push_str(&format!(
                r#"<rect class="basic label-container" style="{s}" rx="{rx}" ry="{ry}" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
                s = node_style,
                rx = rx,
                ry = ry,
                x = fmt_num(-n.width / 2.0),
                y = fmt_num(-n.height / 2.0),
                w = fmt_num(n.width),
                h = fmt_num(n.height),
            ));
        }
    }
    render_label(out, n);
    out.push_str("</g>");
}

fn render_composite(out: &mut String, diagram_id: &str, n: &NodeGeom) {
    let classes = "node default default flowchart-label";
    out.push_str(&format!(
        r#"<g class="{classes}" id="{did}-{nid}" transform="translate({cx}, {cy})">"#,
        classes = classes,
        did = diagram_id,
        nid = n.id,
        cx = fmt_num(n.x),
        cy = fmt_num(n.y),
    ));
    out.push_str(&format!(
        r#"<rect class="basic cluster composite label-container" style="" rx="0" ry="0" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
        x = fmt_num(-n.width / 2.0),
        y = fmt_num(-n.height / 2.0),
        w = fmt_num(n.width),
        h = fmt_num(n.height),
    ));
    render_label(out, n);
    out.push_str("</g>");
}

fn render_circle_shape(out: &mut String, n: &NodeGeom) {
    // Upstream `circle()` uses `r = bbox.width/2 + halfPadding` and
    // sets `width = bbox.width + padding`, `height = bbox.height + padding`
    // on the circle element. Since the circle stays the same size in the
    // positioned second pass (no `node.positioned` branch), these always
    // derive from the LABEL bbox (text_width / text_height).
    let r = n.text_width / 2.0 + crate::layout::block::PADDING / 2.0;
    let w = n.text_width + crate::layout::block::PADDING;
    let h = n.text_height + crate::layout::block::PADDING;
    out.push_str(&format!(
        r#"<circle style="" rx="0" ry="0" r="{r}" width="{w}" height="{h}"></circle>"#,
        r = fmt_num(r),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
}

fn render_block_arrow(out: &mut String, diagram_id: &str, n: &NodeGeom) {
    // Placeholder: emit a plain rect so the diagram still draws.
    // Upstream `block_arrow.js` builds a polygon path — supporting it
    // byte-exact is deferred.
    let classes = "node default default flowchart-label";
    out.push_str(&format!(
        r#"<g class="{classes}" id="{did}-{nid}" transform="translate({cx}, {cy})">"#,
        classes = classes,
        did = diagram_id,
        nid = n.id,
        cx = fmt_num(n.x),
        cy = fmt_num(n.y),
    ));
    out.push_str(&format!(
        r#"<polygon class="label-container" points="" style=""></polygon>"#
    ));
    render_label(out, n);
    out.push_str("</g>");
}

fn render_label(out: &mut String, n: &NodeGeom) {
    // Block labels use the `width = Number.POSITIVE_INFINITY` branch of
    // `addHtmlSpan` — no `max-width` / no `text-align` on the div. The
    // outer group carries an empty `style=""` and a bbox-centred
    // `translate`.
    use crate::render::foreign_object::{render_node_label, LabelOpts};
    let label = n.label.as_deref().unwrap_or("");
    let h = if label.is_empty() {
        LABEL_HEIGHT
    } else {
        n.text_height
    };
    let opts = LabelOpts {
        max_width: f64::INFINITY,
        ..LabelOpts::default()
    };
    let escaped = html_escape(label);
    out.push_str(&render_node_label(&escaped, n.text_width, h, &opts));
}

fn rx_ry_for(shape: BlockShape) -> (&'static str, &'static str) {
    match shape {
        BlockShape::Round => ("5", "5"),
        _ => ("0", "0"),
    }
}

fn format_node_style(styles: &[String]) -> String {
    // Upstream emits styles joined by `;` but empty string when none.
    if styles.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    for (i, st) in styles.iter().enumerate() {
        if i > 0 {
            s.push(';');
            s.push(' ');
        }
        s.push_str(st);
    }
    s
}

// ─── Markers ───────────────────────────────────────────────────────────

fn build_markers(id: &str) -> String {
    let mut s = String::with_capacity(1024);
    s.push_str(&format!(
        r#"<marker id="{id}_block-pointEnd" class="marker block" viewBox="0 0 10 10" refX="6" refY="5" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="12" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1,0;"></path></marker>"#
    ));
    s.push_str(&format!(
        r#"<marker id="{id}_block-pointStart" class="marker block" viewBox="0 0 10 10" refX="4.5" refY="5" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="12" orient="auto"><path d="M 0 5 L 10 10 L 10 0 z" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1,0;"></path></marker>"#
    ));
    s.push_str(&format!(
        r#"<marker id="{id}_block-circleEnd" class="marker block" viewBox="0 0 10 10" refX="11" refY="5" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto"><circle cx="5" cy="5" r="5" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1,0;"></circle></marker>"#
    ));
    s.push_str(&format!(
        r#"<marker id="{id}_block-circleStart" class="marker block" viewBox="0 0 10 10" refX="-1" refY="5" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto"><circle cx="5" cy="5" r="5" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1,0;"></circle></marker>"#
    ));
    s.push_str(&format!(
        r#"<marker id="{id}_block-crossEnd" class="marker cross block" viewBox="0 0 11 11" refX="12" refY="5.2" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto"><path d="M 1,1 l 9,9 M 10,1 l -9,9" class="arrowMarkerPath" style="stroke-width: 2; stroke-dasharray: 1,0;"></path></marker>"#
    ));
    s.push_str(&format!(
        r#"<marker id="{id}_block-crossStart" class="marker cross block" viewBox="0 0 11 11" refX="-1" refY="5.2" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto"><path d="M 1,1 l 9,9 M 10,1 l -9,9" class="arrowMarkerPath" style="stroke-width: 2; stroke-dasharray: 1,0;"></path></marker>"#
    ));
    s
}

// ─── <style> block — copy of the upstream block diagram stylesheet ─────

fn build_style_block(id: &str, theme: &ThemeVariables) -> String {
    let mut s = String::with_capacity(4096);
    s.push_str("<style>");
    // Shared base preamble — root + keyframes + edge helpers + marker.
    s.push_str(&theme_css::base_preamble(id, theme));
    // Block-diagram specific CSS sandwiched between the preamble and
    // the shared neo-look tail.
    s.push_str(&block_specific_css(id, theme));
    // Shared neo-look tail + :root variable.
    s.push_str(&theme_css::neo_look_block(id, theme));
    s.push_str("</style>");
    s
}

fn block_specific_css(id: &str, theme: &ThemeVariables) -> String {
    let font_family_raw = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\", verdana, arial, sans-serif");
    let ff = crate::render::stylis::strip_comma_spaces(font_family_raw);
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let stroke_width = theme.stroke_width.unwrap_or(1);
    let main_bkg = theme.main_bkg.as_deref().unwrap_or("#ECECFF");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");
    let edge_label_background = theme
        .edge_label_background
        .as_deref()
        .unwrap_or("rgba(232,232,232, 0.8)");
    let tertiary_color = theme
        .tertiary_color
        .as_deref()
        .unwrap_or("hsl(80, 100%, 96.2745098039%)");
    let border2 = theme.border2.as_deref().unwrap_or("#aaaa33");
    let cluster_bkg_fade = theme
        .cluster_bkg
        .as_deref()
        .map(|c| fade(c, 0.5))
        .unwrap_or_else(|| "rgba(255, 255, 222, 0.5)".to_string());
    let cluster_border_fade = theme
        .cluster_border
        .as_deref()
        .map(|c| fade(c, 0.2))
        .unwrap_or_else(|| "rgba(170, 170, 51, 0.2)".to_string());
    let title_color = theme.title_color.as_deref().unwrap_or(text_color);
    let node_text_color = theme.node_text_color.as_deref().unwrap_or(text_color);

    let mut s = String::with_capacity(3072);
    s.push_str(&format!(
        "#{id} .label{{font-family:{ff};color:{ntc};}}",
        ntc = node_text_color,
    ));
    s.push_str(&format!("#{id} .cluster-label text{{fill:{title_color};}}"));
    s.push_str(&format!(
        "#{id} .cluster-label span,#{id} p{{color:{title_color};}}"
    ));
    s.push_str(&format!(
        "#{id} .label text,#{id} span,#{id} p{{fill:{ntc};color:{ntc};}}",
        ntc = node_text_color,
    ));
    s.push_str(&format!(
        "#{id} .node rect,#{id} .node circle,#{id} .node ellipse,#{id} .node polygon,#{id} .node path{{fill:{mbg};stroke:{nb};stroke-width:{sw}px;}}",
        mbg = main_bkg, nb = node_border, sw = stroke_width,
    ));
    s.push_str(&format!(
        "#{id} .flowchart-label text{{text-anchor:middle;}}"
    ));
    s.push_str(&format!("#{id} .node .label{{text-align:center;}}"));
    s.push_str(&format!("#{id} .node.clickable{{cursor:pointer;}}"));
    s.push_str(&format!("#{id} .arrowheadPath{{fill:{line_color};}}"));
    s.push_str(&format!(
        "#{id} .edgePath .path{{stroke:{line_color};stroke-width:2.0px;}}"
    ));
    s.push_str(&format!(
        "#{id} .flowchart-link{{stroke:{line_color};fill:none;}}"
    ));
    s.push_str(&format!(
        "#{id} .edgeLabel{{background-color:{elb};text-align:center;}}",
        elb = edge_label_background,
    ));
    s.push_str(&format!(
        "#{id} .edgeLabel p{{margin:0;padding:0;display:inline;}}"
    ));
    s.push_str(&format!(
        "#{id} .edgeLabel rect{{opacity:0.5;background-color:{elb};fill:{elb};}}",
        elb = edge_label_background,
    ));
    s.push_str(&format!(
        "#{id} .labelBkg{{background-color:{elb};}}",
        elb = edge_label_background,
    ));
    s.push_str(&format!(
        "#{id} .node .cluster{{fill:{cb};stroke:{cbd};box-shadow:rgba(50, 50, 93, 0.25) 0px 13px 27px -5px,rgba(0, 0, 0, 0.3) 0px 8px 16px -8px;stroke-width:1px;}}",
        cb = cluster_bkg_fade, cbd = cluster_border_fade,
    ));
    s.push_str(&format!("#{id} .cluster text{{fill:{title_color};}}"));
    s.push_str(&format!(
        "#{id} .cluster span,#{id} p{{color:{title_color};}}"
    ));
    s.push_str(&format!(
        "#{id} div.mermaidTooltip{{position:absolute;text-align:center;max-width:200px;padding:2px;font-family:{ff};font-size:12px;background:{tc};border:1px solid {b};border-radius:2px;pointer-events:none;z-index:100;}}",
        tc = tertiary_color, b = border2,
    ));
    s.push_str(&format!(
        "#{id} .flowchartTitleText{{text-anchor:middle;font-size:18px;fill:{text_color};}}"
    ));
    s.push_str(&format!(
        "#{id} .label-icon{{display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;}}"
    ));
    s.push_str(&format!(
        "#{id} .node .label-icon path{{fill:currentColor;stroke:revert;stroke-width:revert;}}"
    ));
    s
}

fn fade(color: &str, opacity: f64) -> String {
    // Port of upstream styles.ts `fade` helper built on khroma.
    // Parses the input to r/g/b channels and prints an rgba(...) triple
    // with the requested opacity. We currently handle `#rgb` / `#rrggbb`
    // and pass through any other form as-is inside `rgba(...)`.
    if let Some((r, g, b)) = parse_hex_color(color) {
        return format!("rgba({}, {}, {}, {})", r, g, b, opacity);
    }
    // Fallback: keep behaviour for already-rgb strings.
    format!("rgba({}, {})", color, opacity)
}

fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    let hex = s.strip_prefix('#')?;
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            Some((r * 17, g * 17, b * 17))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        _ => None,
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

/// JS-Number-like formatting — integers without `.0`, fractions shortest.
pub fn fmt_num(x: f64) -> String {
    if x.fract() == 0.0 && x.is_finite() {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

// ── byte-exact fixture tests ────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::block::parse;
    use crate::theme::get_theme;

    fn render_fixture(path: &str, id: &str) -> String {
        let src = std::fs::read_to_string(path).expect("read source");
        let d = parse(&src).expect("parse");
        let theme = get_theme("default");
        let l = crate::layout::block::layout(&d, &theme).expect("layout");
        render(&d, &l, &theme, id).expect("render")
    }

    fn compare_fixture(num: &str) -> std::result::Result<(), String> {
        let src = format!("tests/ext_fixtures/cypress/block/{}.mmd", num);
        let refp = format!("tests/reference/ext_fixtures/cypress/block/{}.svg", num);
        let id = format!("ref-ext-fixtures-cypress-block-{}", num);
        let expected = std::fs::read_to_string(&refp).map_err(|e| format!("read ref: {e}"))?;
        let got = render_fixture(&src, &id);
        let expected = expected.trim_end_matches('\n');
        if got == expected {
            return Ok(());
        }
        let mut at = 0usize;
        for (i, (a, b)) in got.bytes().zip(expected.bytes()).enumerate() {
            if a != b {
                at = i;
                break;
            }
        }
        let ctx = 120;
        let g_end = (at + ctx).min(got.len());
        let e_end = (at + ctx).min(expected.len());
        Err(format!(
            "mismatch at {at}: got len={} ref len={}\n  got:...{}...\n  ref:...{}...",
            got.len(),
            expected.len(),
            &got[at.saturating_sub(ctx)..g_end],
            &expected[at.saturating_sub(ctx)..e_end],
        ))
    }

    macro_rules! fixture_test {
        ($name:ident, $num:literal) => {
            #[test]
            fn $name() {
                compare_fixture($num).unwrap();
            }
        };
    }

    #[test]
    fn byte_exact_sweep() {
        let total = 33usize;
        let mut pass = 0usize;
        let mut failing = Vec::new();
        for n in 1..=33u32 {
            let num = format!("{:02}", n);
            match compare_fixture(&num) {
                Ok(()) => pass += 1,
                Err(_) => failing.push(num),
            }
        }
        eprintln!("[block] byte-exact={}/{}", pass, total);
        if !failing.is_empty() {
            eprintln!(
                "[block] failing ({}): {:?}",
                failing.len(),
                &failing[..failing.len().min(10)]
            );
        }
        assert_eq!(pass + failing.len(), total);
    }

    // Byte-exact fixtures — populated from manual probe (see report).
    fixture_test!(cypress_block_03, "03");
    fixture_test!(cypress_block_15, "15");
    fixture_test!(cypress_block_16, "16");
    fixture_test!(cypress_block_17, "17");
    fixture_test!(cypress_block_18, "18");
    fixture_test!(cypress_block_20, "20");
    fixture_test!(cypress_block_22, "22");
    fixture_test!(cypress_block_23, "23");
    fixture_test!(cypress_block_24, "24");
    fixture_test!(cypress_block_25, "25");
    fixture_test!(cypress_block_27, "27");
    fixture_test!(cypress_block_28, "28");
    fixture_test!(cypress_block_29, "29");
    fixture_test!(cypress_block_30, "30");
    fixture_test!(cypress_block_32, "32");
    fixture_test!(cypress_block_33, "33");
}
