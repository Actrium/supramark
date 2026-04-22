//! Circle shape — upstream `circle.ts`.
//!
//! Upstream emits `<circle class="basic label-container" r cx cy>`.
//! Radius comes from `width / 2` (post-layout).

use super::types::{fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let r = node.width.unwrap_or(0.0) / 2.0;
    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let label = node.label.clone().unwrap_or_default();

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}" transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    out.push_str(&format!(
        r#"<circle class="basic label-container" style="" r="{r}" cx="0" cy="0"/>"#,
        r = fmt_num(r),
    ));
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
    fn circle_emits_correct_radius() {
        let mut n = Node::default();
        n.id = "c".into();
        n.width = Some(50.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains(r#"r="25""#));
    }
}
