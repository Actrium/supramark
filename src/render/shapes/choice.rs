//! Choice diamond — upstream `choice.ts`. 28-unit rhombus.

use super::types::{create_path_from_points, fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::{Node, Point};
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let s = node.width.unwrap_or(28.0).max(28.0);
    let pts = [
        Point { x: 0.0, y: s / 2.0 },
        Point { x: s / 2.0, y: 0.0 },
        Point { x: 0.0, y: -s / 2.0 },
        Point { x: -s / 2.0, y: 0.0 },
    ];
    let d = create_path_from_points(&pts);

    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);

    Ok(format!(
        r#"<g class="{classes}" id="{id}" transform="translate({tx}, {ty})"><path class="basic label-container" d="{d}"/></g>"#,
        classes = classes,
        id = xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
        d = d,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_byte_exact_default() {
        let mut n = Node::default();
        n.id = "c".into();
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        // 28 → points: (0,14) (14,0) (0,-14) (-14,0)
        assert!(got.contains(r#"d="M0,14 L14,0 L0,-14 L-14,0 Z""#));
    }
}
