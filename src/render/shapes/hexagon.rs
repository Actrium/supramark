//! Hexagon shape — upstream `hexagon.ts`.
//!
//! Parameterised by `m = h / f` where `f = 4` (default) or `3.5`
//! (`neo` look). Two flat edges of width `w - 2m`, slant-vertices
//! at `(±w/2, 0)`.

use super::types::emit_polygon_node;
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let f: f64 = if matches!(node.look.as_deref(), Some("neo")) { 3.5 } else { 4.0 };
    let m = h / f;
    let pts = [
        (m - w / 2.0, -h / 2.0),
        (w / 2.0 - m, -h / 2.0),
        (w / 2.0, 0.0),
        (w / 2.0 - m, h / 2.0),
        (m - w / 2.0, h / 2.0),
        (-w / 2.0, 0.0),
    ];
    Ok(emit_polygon_node(node, &pts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hexagon_points_match_default_look() {
        let mut n = Node::default();
        n.id = "h".into();
        n.width = Some(100.0);
        n.height = Some(40.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains(r#"points="-40,-20 40,-20 50,0 40,20 -40,20 -50,0""#));
    }
}
