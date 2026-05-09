//! Note shape — upstream `note.ts`.
//!
//! Renders with rough.js roughness=0, fillStyle='solid', seed=1 (from
//! mermaid's `handDrawnSeed:1` config). The shape emits two SVG paths:
//! 1. Fill path: plain straight-line rectangle (L commands).
//! 2. Stroke path: 8 cubic bezier segments (4 sides × 2 strokes each).
//!
//! The bezier control points are computed from a deterministic LCG PRNG
//! (rough.js seed=1). Since roughness=0 all random offsets are zero, so
//! the control point formula reduces to:
//!   cp1 = start + (end - start) * p
//!   cp2 = start + 2 * (end - start) * p
//! where `p = 0.2 + 0.2 * W(o)` and W(o) is the seeded PRNG.
//!
//! The p values are constant for all notes (seed resets to 1 each call):
//!   Side 1 (top) Stroke 1: P1 = 0.20000449558719993
//!   Side 1 (top) Stroke 2: P2 = 0.22135189184919002
//!   Side 2 (right) Stroke 1: P3 = 0.21750230630859735
//!   Side 2 (right) Stroke 2: P4 = 0.26839575478807093
//!   Side 3 (bottom) Stroke 1: P5 = 0.21567247649654747
//!   Side 3 (bottom) Stroke 2: P6 = 0.37591258296743035
//!   Side 4 (left) Stroke 1: P7 = 0.28680931767448786
//!   Side 4 (left) Stroke 2: P8 = 0.36376626798883083

use super::types::{fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

/// Normalize note HTML text for the foreignObject body.
///
/// Converts `\n` line separators to `<br/>` and normalizes all `<br>`
/// tag variants (`<br>`, `<br/>`, `<br />`, `<br\t/>` etc.) to `<br/>`.
/// The result is suitable as HTML content inside `<p>...</p>`.
fn normalize_note_html(text: &str) -> String {
    // Replace \n with <br/> first, then normalize all <br...> variants.
    let with_br = text.replace('\n', "<br/>");
    // Normalize <br ...> variants using a simple scan.
    let mut out = String::with_capacity(with_br.len());
    let chars: Vec<char> = with_br.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' && i + 2 < chars.len() && chars[i + 1] == 'b' && chars[i + 2] == 'r' {
            // Found `<br` — skip ahead to the closing `>`
            let start = i;
            let mut j = i + 3;
            while j < chars.len() && chars[j] != '>' {
                j += 1;
            }
            if j < chars.len() && chars[j] == '>' {
                // This is a <br...> tag — emit normalized <br/>
                out.push_str("<br/>");
                i = j + 1;
                continue;
            }
            // No closing `>` found — emit as-is
            out.push(chars[start]);
            i = start + 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Deterministic control-point fractions from rough.js LCG (seed=1).
const P: [f64; 8] = [
    0.20000449558719993, // side1 stroke1
    0.22135189184919002, // side1 stroke2
    0.21750230630859735, // side2 stroke1
    0.26839575478807093, // side2 stroke2
    0.21567247649654747, // side3 stroke1
    0.37591258296743035, // side3 stroke2
    0.28680931767448786, // side4 stroke1
    0.36376626798883083, // side4 stroke2
];

/// Build the rough.js stroke path for a rectangle with given half-width (hw)
/// and half-height (hh), centered at origin.
///
/// The path consists of 8 M…C segments (4 sides × 2 strokes each):
/// - Top    (stroke1, stroke2): horizontal from (-hw,-hh) to (hw,-hh)
/// - Right  (stroke1, stroke2): vertical from (hw,-hh) to (hw,hh)
/// - Bottom (stroke1, stroke2): horizontal from (hw,hh) to (-hw,hh)
/// - Left   (stroke1, stroke2): vertical from (-hw,hh) to (-hw,-hh)
fn rough_rect_stroke_path(hw: f64, hh: f64) -> String {
    let mut out = String::new();
    // Side 1 top: (-hw,-hh) → (hw,-hh)  (horizontal, Δy=0)
    for stroke in 0..2usize {
        let p = P[stroke];
        let cp1x = -hw + 2.0 * hw * p;
        let cp2x = -hw + 4.0 * hw * p;
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&format!(
            "M{mx} {my} C{cp1x} {cp1y}, {cp2x} {cp2y}, {ex} {ey}",
            mx = fmt_num(-hw),
            my = fmt_num(-hh),
            cp1x = cp1x,
            cp1y = fmt_num(-hh),
            cp2x = cp2x,
            cp2y = fmt_num(-hh),
            ex = fmt_num(hw),
            ey = fmt_num(-hh),
        ));
    }
    // Side 2 right: (hw,-hh) → (hw,hh)  (vertical, Δx=0)
    for stroke in 0..2usize {
        let p = P[2 + stroke];
        let cp1y = -hh + 2.0 * hh * p;
        let cp2y = -hh + 4.0 * hh * p;
        out.push(' ');
        out.push_str(&format!(
            "M{mx} {my} C{cp1x} {cp1y}, {cp2x} {cp2y}, {ex} {ey}",
            mx = fmt_num(hw),
            my = fmt_num(-hh),
            cp1x = fmt_num(hw),
            cp1y = cp1y,
            cp2x = fmt_num(hw),
            cp2y = cp2y,
            ex = fmt_num(hw),
            ey = fmt_num(hh),
        ));
    }
    // Side 3 bottom: (hw,hh) → (-hw,hh)  (horizontal, Δy=0)
    for stroke in 0..2usize {
        let p = P[4 + stroke];
        // start=hw, end=-hw, (end-start)=-2*hw
        let cp1x = hw + (-2.0 * hw) * p;
        let cp2x = hw + 2.0 * (-2.0 * hw) * p;
        out.push(' ');
        out.push_str(&format!(
            "M{mx} {my} C{cp1x} {cp1y}, {cp2x} {cp2y}, {ex} {ey}",
            mx = fmt_num(hw),
            my = fmt_num(hh),
            cp1x = cp1x,
            cp1y = fmt_num(hh),
            cp2x = cp2x,
            cp2y = fmt_num(hh),
            ex = fmt_num(-hw),
            ey = fmt_num(hh),
        ));
    }
    // Side 4 left: (-hw,hh) → (-hw,-hh)  (vertical, Δx=0)
    for stroke in 0..2usize {
        let p = P[6 + stroke];
        // start=-hh+2*hh=hh (y direction: from hh to -hh), (end-start)=-2*hh
        let cp1y = hh + (-2.0 * hh) * p;
        let cp2y = hh + 2.0 * (-2.0 * hh) * p;
        out.push(' ');
        out.push_str(&format!(
            "M{mx} {my} C{cp1x} {cp1y}, {cp2x} {cp2y}, {ex} {ey}",
            mx = fmt_num(-hw),
            my = fmt_num(hh),
            cp1x = fmt_num(-hw),
            cp1y = cp1y,
            cp2x = fmt_num(-hw),
            cp2y = cp2y,
            ex = fmt_num(-hw),
            ey = fmt_num(-hh),
        ));
    }
    out
}

pub fn draw(node: &Node, theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let hw = w / 2.0;
    let hh = h / 2.0;
    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let label = node.label.clone().unwrap_or_default();

    let fill = theme.note_bkg_color.as_deref().unwrap_or("#fff5ad");
    let stroke = theme.note_border_color.as_deref().unwrap_or("#aaaa33");

    let data_look = match node.look.as_deref() {
        Some(look) if !look.is_empty() => format!(r#" data-look="{}""#, look),
        _ => String::new(),
    };

    // Fill path: straight-line rectangle (roughness=0 solid fill = plain polygon).
    let fill_path = format!(
        "M{x1} {y1} L{x2} {y2} L{x3} {y3} L{x4} {y4}",
        x1 = fmt_num(-hw),
        y1 = fmt_num(-hh),
        x2 = fmt_num(hw),
        y2 = fmt_num(-hh),
        x3 = fmt_num(hw),
        y3 = fmt_num(hh),
        x4 = fmt_num(-hw),
        y4 = fmt_num(hh),
    );

    // Stroke path: rough.js bezier rectangle.
    let stroke_path = rough_rect_stroke_path(hw, hh);

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}"{data_look} transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = xml_escape(&id),
        data_look = data_look,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    out.push_str(r#"<g class="basic label-container outer-path">"#);
    // Fill path first (fillPath type in rough.js).
    out.push_str(&format!(
        r#"<path d="{d}" stroke="none" stroke-width="0" fill="{fill}" style=""></path>"#,
        d = fill_path,
        fill = fill,
    ));
    // Stroke path second (path type in rough.js).
    out.push_str(&format!(
        r#"<path d="{d}" stroke="{stroke}" stroke-width="1.3" fill="none" stroke-dasharray="0 0" style=""></path>"#,
        d = stroke_path,
        stroke = stroke,
    ));
    out.push_str("</g>");

    if !label.is_empty() {
        use crate::render::foreign_object::{measure_html_markup_label, HtmlLabelFont, LabelOpts};
        // Upstream: `\n` is pre-replaced with `<br/>` before innerHTML-assign,
        // so jsdom textContent returns the label as a SINGLE logical line.
        // `measure_html_markup_label` mirrors that — it strips `<br>` and any
        // other tag, decodes entities, and returns one line-height.
        let (fw, fh) = measure_html_markup_label(&label, &HtmlLabelFont::default(), 200.0, true);
        // Build the FO HTML body:
        // 1. Normalize all `<br>` variants to `<br/>`.
        // 2. Replace `\n` (multi-line note separator) with `<br/>`.
        // Note: we do NOT xml_escape the whole label because `<br/>` must
        // remain as actual HTML tags. Other chars that need escaping
        // (like `&`) can be handled below if needed, but mermaid note text
        // typically doesn't contain unencoded HTML-unsafe chars.
        let html_label = normalize_note_html(&label);
        let opts = LabelOpts {
            extra_span_classes: "markdown-node-label",
            ..LabelOpts::default()
        };
        // Emit <g class="label noteLabel" style="" transform="translate(-fw/2, -fh/2)">
        out.push_str(&format!(
            r#"<g class="label noteLabel" style="" transform="translate({tx}, {ty})">"#,
            tx = fmt_num(-fw / 2.0),
            ty = fmt_num(-fh / 2.0),
        ));
        out.push_str("<rect></rect>");
        out.push_str(&crate::render::foreign_object::foreign_object_body(
            &html_label,
            fw,
            fh,
            &opts,
        ));
        out.push_str("</g>");
    }
    out.push_str("</g>");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_includes_theme_colors() {
        let mut n = Node::default();
        n.id = "note1".into();
        n.width = Some(80.0);
        n.height = Some(40.0);
        n.label = Some("Heads up".into());
        let mut theme = ThemeVariables::default();
        theme.note_bkg_color = Some("#fff5ad".into());
        theme.note_border_color = Some("#aaaa33".into());
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains("fill=\"#fff5ad\""), "fill color missing");
        assert!(got.contains("stroke=\"#aaaa33\""), "stroke color missing");
        assert!(got.contains("noteLabel"));
    }

    #[test]
    fn note_stroke_path_matches_reference_cy11() {
        // cy/11: note text "Important information! You can write\nnotes."
        // Note node half-width hw=166.744140625, half-height hh=23.1484375
        let hw = 166.744140625_f64;
        let hh = 23.1484375_f64;
        let path = rough_rect_stroke_path(hw, hh);
        // Reference: first segment from cy/11
        assert!(
            path.contains("-100.04498514935149"),
            "cp1x of top side stroke1 mismatch; path={:?}",
            &path[..path.find(' ').unwrap_or(path.len()).min(200)]
        );
        assert!(
            path.contains("-33.34582967370298"),
            "cp2x of top side stroke1 mismatch"
        );
    }
}
