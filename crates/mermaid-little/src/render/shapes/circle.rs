//! Circle shape — upstream `circle.ts`.
//!
//! Upstream emits `<circle class="basic label-container" r cx cy>`.
//! Upstream: r = bbox.width / 2 + halfPadding, where halfPadding = node.padding/2.
//! Our dagre node width = bbox.width + node.padding = label_width + padding,
//! so r = (width - padding)/2 + padding/2 = width/2.

use super::types::{fmt_num, get_node_classes, xml_escape, xml_escape_label};
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

    let data_look = match node.look.as_deref() {
        Some(look) if !look.is_empty() => format!(r#" data-look="{}""#, look),
        _ => String::new(),
    };

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}"{data_look} transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = xml_escape(&id),
        data_look = data_look,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    let css_styles = node.css_styles.as_deref().unwrap_or(&[]);
    let circle_style = super::types::build_inline_style(css_styles);
    out.push_str(&format!(
        r#"<circle class="basic label-container" style="{circle_style}" r="{r}" cx="0" cy="0"></circle>"#,
        circle_style = circle_style,
        r = fmt_num(r),
    ));
    if !label.is_empty() {
        out.push_str(
            &crate::render::foreign_object::shape_label_block_with_styles(
                &xml_escape_label(&label),
                &crate::render::foreign_object::HtmlLabelFont::default(),
                css_styles,
            ),
        );
    } else {
        // Upstream's `labelHelper` always emits the `<g class="label">` block
        // (with `<rect>` marker, empty `<foreignObject width="0">`, and an
        // empty `<span class="nodeLabel ">`), even when the source supplies
        // an empty label like `A(( ))`. The label block contributes zero to
        // the visual result but is required for byte-exact parity. See
        // cypress/flowchart/88 + demos/flowchart/30, /31, /61.
        let font = crate::render::foreign_object::HtmlLabelFont::default();
        let (w, h) = crate::render::foreign_object::measure_html_label("", &font, 200.0, true);
        let opts = crate::render::foreign_object::LabelOpts {
            wrap_in_p: false,
            ..crate::render::foreign_object::LabelOpts::default()
        };
        out.push_str(&crate::render::foreign_object::render_node_label(
            "", w, h, &opts,
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
        // width=50 → r = 50/2 = 25
        n.width = Some(50.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains(r#"r="25""#));
    }
}
