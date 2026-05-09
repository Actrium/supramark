//! Diamond / decision / question shape.
//!
//! Upstream reference:
//! `packages/mermaid/src/rendering-util/rendering-elements/shapes/question.ts`
//! + `insertPolygonShape.ts`.
//!
//! Upstream geometry:
//! - `s = labelWidth + padding + labelHeight + padding`
//! - Points are in the "top-right-bottom-left" format anchored at
//!   top-left: `{s/2, 0}`, `{s, -s/2}`, `{s/2, -s}`, `{0, -s/2}`.
//! - The `<polygon>` carries class `label-container` (NOT `basic
//!   label-container`) and a `transform="translate(-s/2+0.5, s/2)"`.
//! - The closing tag is `></polygon>`.

use super::types::{build_inline_style, fmt_num, get_node_classes, xml_escape, xml_escape_label};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let label = node.label.clone().unwrap_or_default();
    // Re-measure the label to get the bbox dimensions, matching upstream's
    // `bbox = labelHelper(...)`. The layout stores `s` as both width and
    // height for dagre, so we can't use node.width/height directly.
    //
    // Upstream's `labelHelper` runs `createText` on the label which
    // expands FontAwesome icon references (`fa:fa-car`) into
    // `<i class="fa fa-car"></i>` markup BEFORE the bbox is measured.
    // Mirror that so a label like `fa:fa-code Text` is measured as
    // " Text" (the rendered textContent), not as the literal 15-char
    // source string. Cypress fixture 57 / 117 / 118 hits this on a
    // diamond node `I{"fa:fa-code Text"}`.
    let (bbox_w, bbox_h) = if label.is_empty() {
        (0.0, 0.0)
    } else {
        let escaped = super::types::xml_escape_label(&label);
        let for_measure = crate::render::foreign_object::replace_fa_icons(&escaped);
        // `measure_html_label` does not strip HTML tags — it would
        // measure the literal `<i class="fa fa-code"></i>` characters.
        // When the FA-icon expansion produced a non-trivial change
        // (`<i …>` markup), fall through to `measure_html_markup_label`
        // which strips tags via the textContent rule. For plain labels
        // (no `<i>` injected) keep `measure_html_label` for the
        // existing markdown-marker / `\n` semantics.
        if for_measure.contains("<i ") {
            crate::render::foreign_object::measure_html_markup_label(
                &for_measure,
                &crate::render::foreign_object::HtmlLabelFont::default(),
                200.0,
                true,
            )
        } else {
            crate::render::foreign_object::measure_html_label(
                &for_measure,
                &crate::render::foreign_object::HtmlLabelFont::default(),
                200.0,
                true,
            )
        }
    };
    let p = node.padding.unwrap_or(15.0);
    let w = bbox_w + p;
    let h = bbox_h + p;
    let s = w + h;
    let half = s / 2.0;

    // Upstream question.ts points array.
    let pts = [(half, 0.0), (s, -half), (half, -s), (0.0, -half)];
    let pts_attr = pts
        .iter()
        .map(|(x, y)| format!("{},{}", fmt_num(*x), fmt_num(*y)))
        .collect::<Vec<_>>()
        .join(" ");

    // Upstream applies adjustment=0.5 to the x translation.
    let tx = -half + 0.5;
    let ty = half;

    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let label = node.label.clone().unwrap_or_default();
    let data_look = match node.look.as_deref() {
        Some(look) if !look.is_empty() => format!(r#" data-look="{}""#, look),
        _ => String::new(),
    };
    let node_tx = node.x.unwrap_or(0.0);
    let node_ty = node.y.unwrap_or(0.0);

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}"{data_look} transform="translate({node_tx}, {node_ty})">"#,
        classes = classes,
        id = xml_escape(&id),
        data_look = data_look,
        node_tx = fmt_num(node_tx),
        node_ty = fmt_num(node_ty),
    ));
    // Upstream insertPolygonShape: class="label-container", not "basic label-container".
    // Upstream's question.ts then conditionally appends `style="..."` from
    // `nodeStyles` (built via `styles2String` from the node's style directive),
    // but ONLY when non-empty. Mirror that with `build_inline_style`.
    let node_style = build_inline_style(node.css_styles.as_deref().unwrap_or(&[]));
    let style_attr = if node_style.is_empty() {
        String::new()
    } else {
        format!(r#" style="{}""#, node_style)
    };
    out.push_str(&format!(
        r#"<polygon points="{pts_attr}" class="label-container" transform="translate({tx}, {ty})"{style_attr}></polygon>"#,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    if !label.is_empty() {
        let css_styles = node.css_styles.as_deref().unwrap_or(&[]);
        out.push_str(
            &crate::render::foreign_object::shape_label_block_with_styles(
                &xml_escape_label(&label),
                &crate::render::foreign_object::HtmlLabelFont::default(),
                css_styles,
            ),
        );
    }
    out.push_str("</g>");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diamond_points_match_upstream_format() {
        let mut n = Node::default();
        n.id = "q".into();
        n.label = Some("test".into());
        n.padding = Some(15.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        // The diamond should use label measurement, not node.width/height.
        // class should be "label-container" not "basic label-container"
        assert!(got.contains(r#"class="label-container""#));
        assert!(!got.contains(r#"class="basic label-container""#));
        // Should have transform with adjustment=0.5
        assert!(got.contains(r#"transform="translate"#));
    }
}
