//! Diamond / decision / question shape.
//!
//! Upstream reference:
//! `packages/mermaid/src/rendering-util/rendering-elements/shapes/question.ts`
//! — also referred to as `diamond` in some diagram registries.
//!
//! Geometry: polygon with four vertices top/right/bottom/left,
//! baked centred around origin for drop-in use inside a
//! `translate(x, y)` `<g>`.

use super::types::emit_polygon_node;
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let s = w + h;
    let half = s / 2.0;
    let pts = [
        (0.0, -half),
        (half, 0.0),
        (0.0, half),
        (-half, 0.0),
    ];
    Ok(emit_polygon_node(node, &pts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diamond_points_centre_around_origin() {
        let mut n = Node::default();
        n.id = "q".into();
        n.width = Some(40.0);
        n.height = Some(20.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains(r#"points="0,-30 30,0 0,30 -30,0""#));
    }
}
