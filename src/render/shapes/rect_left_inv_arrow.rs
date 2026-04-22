//! `rect_left_inv_arrow` — rectangle with a left-pointing notch.
//! Upstream: `rectLeftInvArrow.ts`.
//!
//! Five-vertex polygon:
//!   (x + notch, y), (x, 0), (x + notch, -y), (-x, -y), (-x, y)
//! where `x = -w/2`, `y = -h/2`, `notch = y/2`. Upstream applies a
//! final `translate(-notch/2, 0)` to the outer group, which we bake
//! into the vertices.

use super::types::emit_polygon_node;
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let x = -w / 2.0;
    let y = -h / 2.0;
    let notch = y / 2.0; // upstream keeps sign; y is negative
    // Upstream's pre-translation pts, then apply translate(-notch/2, 0).
    let dx = -notch / 2.0;
    let pts = [
        (x + notch + dx, y),
        (x + dx, 0.0),
        (x + notch + dx, -y),
        (-x + dx, -y),
        (-x + dx, y),
    ];
    Ok(emit_polygon_node(node, &pts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_left_inv_arrow_has_five_points() {
        let mut n = Node::default();
        n.id = "arr".into();
        n.width = Some(80.0);
        n.height = Some(40.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        // Count commas in points attribute (each vertex has one) —
        // 5 vertices == 5 commas.
        let points = got
            .split(r#"points=""#)
            .nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap();
        assert_eq!(points.matches(',').count(), 5);
        assert_eq!(points.split(' ').count(), 5);
    }
}
