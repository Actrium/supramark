//! Invisible label rect — upstream `labelRect.ts`.
//!
//! Used for edge labels and block-diagram labels that need to
//! participate in layout but carry no visible container. Emits a
//! degenerate 0.1×0.1 `<rect>` plus the centred `<text>`.

use super::types::{fmt_num, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let label = node.label.clone().unwrap_or_default();

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="label edgeLabel" id="{id}" transform="translate({tx}, {ty})">"#,
        id = xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    out.push_str(r#"<rect width="0.1" height="0.1"/>"#);
    if !label.is_empty() {
        out.push_str(&crate::render::foreign_object::shape_label_block(
            &xml_escape(&label),
            &crate::render::foreign_object::HtmlLabelFont::default(),
        ));
    }
    out.push_str("</g>");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_rect_byte_exact_minimum() {
        let mut n = Node::default();
        n.id = "lr".into();
        n.label = Some("edge".into());
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        assert!(got.contains(r#"class="label edgeLabel""#));
        assert!(got.contains(r#"<rect width="0.1" height="0.1"/>"#));
        assert!(got.contains("edge"));
    }
}
