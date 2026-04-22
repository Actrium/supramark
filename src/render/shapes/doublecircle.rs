//! Double-circle shape — upstream `doubleCircle.ts`.
//!
//! Outer + inner concentric circles; gap is `5` (classic look) or
//! `12` (`neo` look). Inner radius = outer − gap.

use super::types::{fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let outer = node.width.unwrap_or(0.0) / 2.0;
    let gap = if matches!(node.look.as_deref(), Some("neo")) { 12.0 } else { 5.0 };
    let inner = outer - gap;
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
    out.push_str(r#"<g class="basic label-container" style="">"#);
    out.push_str(&format!(
        r#"<circle class="outer-circle" style="" r="{r}" cx="0" cy="0"/>"#,
        r = fmt_num(outer),
    ));
    out.push_str(&format!(
        r#"<circle class="inner-circle" style="" r="{r}" cx="0" cy="0"/>"#,
        r = fmt_num(inner),
    ));
    out.push_str("</g>");
    if !label.is_empty() {
        out.push_str(&format!(
            r#"<g class="label" transform="translate(0, 0)"><text>{l}</text></g>"#,
            l = xml_escape(&label),
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
