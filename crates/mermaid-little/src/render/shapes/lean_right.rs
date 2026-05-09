//! lean-right parallelogram — upstream `leanRight.ts`.
//!
//! Upstream emits the polygon in pre-translate (raw) coordinates and
//! applies `transform="translate(-w/2, h/2)"` on the `<polygon>`:
//!   points = (-3h/6, 0), (w, 0), (w + 3h/6, -h), (0, -h)

use super::types::emit_polygon_node_with_transform;
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let h = node.height.unwrap_or(0.0);
    let visual_w = node.width.unwrap_or(0.0);
    let shear = (3.0 * h) / 6.0;
    let w = visual_w - 2.0 * shear;
    let pts = [(-shear, 0.0), (w, 0.0), (w + shear, -h), (0.0, -h)];
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
    fn lean_right_polygon_matches_upstream() {
        let mut n = Node::default();
        n.id = "lr".into();
        n.width = Some(100.0); // visual_w (= base 60 + 2*shear 40)
        n.height = Some(40.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        // shear=20, base_w=60: pts (-20,0)(60,0)(80,-40)(0,-40), transform=(-30,20)
        assert!(got.contains(r#"points="-20,0 60,0 80,-40 0,-40""#));
        assert!(got.contains(r#"transform="translate(-30,20)""#));
    }
}
