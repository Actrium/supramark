//! Double-circle shape — upstream `doubleCircle.ts`.
//!
//! Outer + inner concentric circles; gap is `5` (classic look) or
//! `12` (`neo` look). Inner radius = outer − gap.

use super::types::{build_inline_style, fmt_num, get_node_classes, xml_escape, xml_escape_label};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let outer = node.width.unwrap_or(0.0) / 2.0;
    let gap = if matches!(node.look.as_deref(), Some("neo")) {
        12.0
    } else {
        5.0
    };
    let inner = outer - gap;
    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let label = node.label.clone().unwrap_or_default();

    let data_look = match node.look.as_deref() {
        Some(look) if !look.is_empty() => format!(r#" data-look="{}""#, look),
        _ => String::new(),
    };

    let css_styles = node.css_styles.as_deref().unwrap_or(&[]);
    let container_style = build_inline_style(css_styles);

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}"{data_look} transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = xml_escape(&id),
        data_look = data_look,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    out.push_str(&format!(
        r#"<g class="basic label-container" style="{container_style}">"#,
        container_style = container_style,
    ));
    out.push_str(&format!(
        r#"<circle class="outer-circle" style="{container_style}" r="{r}" cx="0" cy="0"></circle>"#,
        container_style = container_style,
        r = fmt_num(outer),
    ));
    out.push_str(&format!(
        r#"<circle class="inner-circle" style="{container_style}" r="{r}" cx="0" cy="0"></circle>"#,
        container_style = container_style,
        r = fmt_num(inner),
    ));
    out.push_str("</g>");
    if !label.is_empty() {
        out.push_str(
            &crate::render::foreign_object::shape_label_block_with_styles(
                &xml_escape_label(&label),
                &crate::render::foreign_object::HtmlLabelFont::default(),
                css_styles,
            ),
        );
    } else {
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
    fn double_circle_classic_gap() {
        let mut n = Node::default();
        n.id = "d".into();
        n.width = Some(60.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains(r#"r="30""#));
        assert!(got.contains(r#"r="25""#));
    }

    #[test]
    fn double_circle_neo_gap() {
        let mut n = Node::default();
        n.id = "d".into();
        n.width = Some(60.0);
        n.look = Some("neo".into());
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains(r#"r="30""#));
        assert!(got.contains(r#"r="18""#));
    }
}
