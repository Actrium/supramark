//! Hexagon shape — upstream `hexagon.ts`.
//!
//! Upstream emits the polygon in pre-translate (raw) coordinates and
//! applies `transform="translate(-w/2, h/2)"` on the `<polygon>`
//! itself (string-concat → no space after comma):
//!   m = h / f              (f = 4 default, 3.5 for `neo` look)
//!   points = (m, 0) (w-m, 0) (w, -h/2) (w-m, -h) (m, -h) (0, -h/2)
//! Wider in the middle, two flat edges of width `w - 2m` top/bottom.
//!
//! `node.width` / `node.height` already carry the final visual size
//! (= bbox + 2m + labelPaddingY × labelPaddingX) computed in
//! `measure_vertex_box`, so we feed them straight into the polygon.

use super::types::emit_polygon_node_with_transform;
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let f: f64 = if matches!(node.look.as_deref(), Some("neo")) {
        3.5
    } else {
        4.0
    };
    let m = h / f;
    // Raw upstream points — polygon transform handles centring.
    let pts = [
        (m, 0.0),
        (w - m, 0.0),
        (w, -h / 2.0),
        (w - m, -h),
        (m, -h),
        (0.0, -h / 2.0),
    ];
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
    fn hexagon_points_match_upstream() {
        let mut n = Node::default();
        n.id = "h".into();
        // visual w=100, h=40, m = 40/4 = 10
        n.width = Some(100.0);
        n.height = Some(40.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(
            got.contains(r#"points="10,0 90,0 100,-20 90,-40 10,-40 0,-20""#),
            "got: {}",
            got
        );
        assert!(
            got.contains(r#"transform="translate(-50,20)""#),
            "got: {}",
            got
        );
    }
}
