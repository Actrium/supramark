//! Stadium / pill shape — upstream `stadium.ts`.
//!
//! A rounded rectangle whose corner radius equals half its height;
//! degenerates to a circle when `w == h`. Upstream generates it as a
//! high-point-count path (`generateCirclePoints(..., 50, ...)`) and
//! draws via `rough.path`. We take the analytic path form via
//! [`types::create_stadium_path_d`] since the non-handDrawn path is
//! byte-exact with the RoughJS output when roughness is zero.

use super::types::{create_stadium_path_d, fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let d = create_stadium_path_d(-w / 2.0, -h / 2.0, w, h);

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
        r#"<path class="basic label-container outer-path" style="" d="{d}"/>"#,
        d = d,
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
    fn stadium_path_byte_exact() {
        let mut n = Node::default();
        n.id = "s".into();
        n.width = Some(80.0);
        n.height = Some(20.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        // r = 10; path anchored at (-40, -10) 80×20
        assert!(got.contains(r#"d="M -30 -10 H 30 A 10 10 0 0 1 40 0 H -40 A 10 10 0 0 1 30 10 H -30 A 10 10 0 0 1 -40 0 Z""#));
    }
}
