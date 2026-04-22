//! Cylinder / database shape — upstream `cylinder.ts`.

use super::types::{create_cylinder_path_d, fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    // Upstream: rx = w/2; ry = rx / (2.5 + w/50)
    let rx = w / 2.0;
    let ry = rx / (2.5 + w / 50.0);
    let d = create_cylinder_path_d(0.0, 0.0, w, h, rx, ry);

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
    // Upstream wraps in transform translate(-w/2, -(h/2+ry)).
    let ox = -w / 2.0;
    let oy = -(h / 2.0 + ry);
    out.push_str(&format!(
        r#"<path d="{d}" class="basic label-container outer-path" style="" label-offset-y="{ly}" transform="translate({ox}, {oy})"/>"#,
        d = d,
        ly = fmt_num(ry),
        ox = fmt_num(ox),
        oy = fmt_num(oy),
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
    fn cylinder_path_byte_exact() {
        let mut n = Node::default();
        n.id = "db".into();
        n.width = Some(50.0);
        n.height = Some(40.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        // rx=25, ry=25/3.5 ≈ 7.142857142857143
        assert!(got.contains(r#"d="M0,"#));
        assert!(got.contains(r#"a25,"#));
    }
}
