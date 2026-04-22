//! Ellipse / circle-with-independent-radii shape. Used by some
//! diagram families (not flowchart-native — see `circle` and
//! `double_circle` for those).
//!
//! Upstream: there is no standalone `ellipse.ts` in the shape
//! library (circles are handled by `circle.ts`). This module serves
//! as the entry point for the `"ellipse"` registry key which is
//! consumed by state-diagram "circle" nodes via the unified pipeline.

use super::types::{fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let rx = node.rx.unwrap_or_else(|| node.width.unwrap_or(0.0) / 2.0);
    let ry = node.ry.unwrap_or_else(|| node.height.unwrap_or(0.0) / 2.0);
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
        r#"<ellipse class="basic label-container" style="" rx="{rx}" ry="{ry}" cx="0" cy="0"/>"#,
        rx = fmt_num(rx),
        ry = fmt_num(ry),
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
    fn ellipse_byte_exact() {
        let mut n = Node::default();
        n.id = "e".into();
        n.rx = Some(30.0);
        n.ry = Some(20.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains(r#"rx="30""#));
        assert!(got.contains(r#"ry="20""#));
        assert!(got.contains(r#"cx="0""#));
    }
}
