//! Rounded rectangle shape.
//!
//! Upstream reference:
//! `packages/mermaid/src/rendering-util/rendering-elements/shapes/roundedRect.ts` +
//! `packages/mermaid/src/rendering-util/rendering-elements/shapes/labelRect.ts`
//! (both funnel into `drawRect` with `rx`/`ry` > 0).
//!
//! Upstream's radius source: `themeVariables.radius ?? 5` — here we
//! take `node.rx` if set, otherwise default to `5` (matching the
//! stable-v11 default when no theme override is supplied).

use super::types::{fmt_num, get_node_classes};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let x = -w / 2.0;
    let y = -h / 2.0;
    let r = node.rx.unwrap_or(5.0);
    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let label = node.label.clone().unwrap_or_default();

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}" transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = super::types::xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    out.push_str(&format!(
        r#"<rect class="basic label-container" style="" rx="{r}" ry="{r}" x="{x}" y="{y}" width="{w}" height="{h}"/>"#,
        r = fmt_num(r),
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    if !label.is_empty() {
        out.push_str(&crate::render::foreign_object::shape_label_block(
            &super::types::xml_escape(&label),
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
    fn rounded_rect_emits_radius() {
        let mut n = Node::default();
        n.id = "n".into();
        n.width = Some(80.0);
        n.height = Some(40.0);
        n.rx = Some(5.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains(r#"rx="5""#));
        assert!(got.contains(r#"ry="5""#));
        assert!(got.contains(r#"x="-40""#));
    }
}
