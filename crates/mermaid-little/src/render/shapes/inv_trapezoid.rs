//! Inverted trapezoid shape — upstream `invertedTrapezoid.ts`.
//!
//! Upstream emits the polygon in pre-translate (raw) coordinates and
//! applies `transform="translate(-w/2, h/2)"` on the `<polygon>`:
//!   points = (0, 0), (w, 0), (w + 3h/6, -h), (-3h/6, -h)
//! Narrower at the bottom, wider at the top.

use super::types::emit_polygon_node_with_transform;
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let h = node.height.unwrap_or(0.0);
    let visual_w = node.width.unwrap_or(0.0);
    let shear = (3.0 * h) / 6.0;
    let w = visual_w - 2.0 * shear;
    let pts = [(0.0, 0.0), (w, 0.0), (w + shear, -h), (-shear, -h)];
    Ok(emit_polygon_node_with_transform(
        node,
        &pts,
        -w / 2.0,
        h / 2.0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inv_trapezoid_points_match_upstream() {
        let mut n = Node::default();
        n.id = "itr".into();
        n.width = Some(100.0); // visual_w (= base 60 + 2*shear 40)
        n.height = Some(40.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        // shear=20, base_w=60: pts (0,0)(60,0)(80,-40)(-20,-40), transform=(-30,20)
        assert!(got.contains(r#"points="0,0 60,0 80,-40 -20,-40""#));
        assert!(got.contains(r#"transform="translate(-30,20)""#));
    }
}
