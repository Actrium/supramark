//! Plain rectangle shape.
//!
//! Upstream reference:
//! `packages/mermaid/src/rendering-util/rendering-elements/shapes/squareRect.ts` +
//! `packages/mermaid/src/rendering-util/rendering-elements/shapes/drawRect.ts`.
//!
//! Emits a `<rect>` sized to the node's post-layout width/height
//! anchored at `(-w/2, -h/2)` (upstream convention — the parent `<g>`
//! carries the translate).

use super::types::{fmt_num, get_node_classes};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

/// Draw a plain rectangle shape. Byte-compatible with upstream
/// `squareRect` → `drawRect(options: rx=0, ry=0)`.
pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let x = -w / 2.0;
    let y = -h / 2.0;
    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let label = node.label.clone().unwrap_or_default();
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);

    // When `node.look` is set (e.g. "classic"), emit `data-look` attribute.
    let data_look = match node.look.as_deref() {
        Some(look) if !look.is_empty() => format!(r#" data-look="{}""#, look),
        _ => String::new(),
    };

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}"{data_look} transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = super::types::xml_escape(&id),
        data_look = data_look,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    // Upstream omits rx/ry when both are 0 (D3's .attr() skips
    // attributes whose value is 0 for rect elements). Only emit
    // them when the node has rx/ry set (e.g. round shape).
    let rx_ry = match (node.rx, node.ry) {
        (Some(rx), Some(ry)) if rx > 0.0 && ry > 0.0 => {
            format!(r#" rx="{}" ry="{}""#, fmt_num(rx), fmt_num(ry))
        }
        _ => String::new(),
    };
    out.push_str(&format!(
        r#"<rect class="basic label-container" style=""{rx_ry} x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
        rx_ry = rx_ry,
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    if !label.is_empty() {
        // HTML foreignObject label matching upstream `labelHelper` /
        // `addHtmlSpan` output. Width/height come from the jsdom-shim
        // measurement (14px sans-serif default — see
        // `foreign_object::measure_html_label`).
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
    fn plain_rect_emits_foreign_object_label() {
        let mut n = Node::default();
        n.id = "n1".into();
        n.width = Some(100.0);
        n.height = Some(50.0);
        n.x = Some(60.0);
        n.y = Some(30.0);
        n.label = Some("hi".into());
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.starts_with(
            r#"<g class="node undefined " id="n1" transform="translate(60, 30)"><rect class="basic label-container" style="" x="-50" y="-25" width="100" height="50"></rect>"#
        ));
        assert!(
            got.contains(r#"<foreignObject "#),
            "label should use foreignObject, got:\n{got}"
        );
        assert!(got.contains(r#"<span class="nodeLabel "><p>hi</p></span>"#));
        assert!(got.ends_with("</g></g>"));
    }

    #[test]
    fn rect_without_label_omits_label_block() {
        let mut n = Node::default();
        n.id = "n2".into();
        n.width = Some(20.0);
        n.height = Some(10.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains(r#"width="20""#));
        assert!(!got.contains("<text>"));
        assert!(!got.contains("<foreignObject"));
    }
}
