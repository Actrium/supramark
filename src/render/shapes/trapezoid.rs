//! Trapezoid shape — upstream `trapezoid.ts`.
//!
//! Upstream pre-translate points:
//!   (-3h/6, 0), (w + 3h/6, 0), (w, -h), (0, -h)
//! Wider at the bottom (y=0), narrower at the top.

use super::types::emit_polygon_node;
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let shear = (3.0 * h) / 6.0;
    let pts = [
        (-shear - w / 2.0, h / 2.0),
        (w + shear - w / 2.0, h / 2.0),
        (w - w / 2.0, -h / 2.0),
        (-w / 2.0, -h / 2.0),
    ];
    Ok(emit_polygon_node(node, &pts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trapezoid_points_match_upstream() {
        let mut n = Node::default();
        n.id = "tr".into();
        n.width = Some(60.0);
        n.height = Some(40.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        // shear=20 → (-50,20) (50,20) (30,-20) (-30,-20)
        assert!(got.contains(r#"points="-50,20 50,20 30,-20 -30,-20""#));
    }
}
