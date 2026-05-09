//! Trapezoid shape — upstream `trapezoid.ts`.
//!
//! Upstream emits the polygon in pre-translate (raw) coordinates and
//! applies `transform="translate(-w/2, h/2)"` on the `<polygon>`
//! itself (string-concat → no space after comma):
//!   points = (-3h/6, 0), (w + 3h/6, 0), (w, -h), (0, -h)
//! Wider at the bottom (y=0), narrower at the top.

use super::types::emit_polygon_node_with_transform;
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    // `node.width` carries the *visual* width (= w_base + 2*shear) that
    // dagre saw post-`updateNodeBounds`. Recover the base inner width.
    let h = node.height.unwrap_or(0.0);
    let visual_w = node.width.unwrap_or(0.0);
    let shear = (3.0 * h) / 6.0;
    let w = visual_w - 2.0 * shear;
    // Raw upstream points — the polygon transform handles centring.
    let pts = [(-shear, 0.0), (w + shear, 0.0), (w, -h), (0.0, -h)];
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
    fn trapezoid_points_match_upstream() {
        let mut n = Node::default();
        n.id = "tr".into();
        // visual_w = 100 (= base_w 60 + 2*shear 40)
        n.width = Some(100.0);
        n.height = Some(40.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        // shear=20, base_w=60, raw pts: (-20,0)(80,0)(60,-40)(0,-40), transform=(-30,20)
        assert!(got.contains(r#"points="-20,0 80,0 60,-40 0,-40""#));
        assert!(got.contains(r#"transform="translate(-30,20)""#));
    }
}
