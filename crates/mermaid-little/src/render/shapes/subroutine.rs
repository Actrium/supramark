//! Subroutine shape — rectangle with two vertical bars.
//! Upstream: `subroutine.ts`.
//!
//! Upstream emits a 10-vertex polygon (outer rect + framing rect joined
//! with rewind lines) using raw top-left-anchored coordinates and
//! applies `transform="translate(-w/2, h/2)"` on the `<polygon>` itself
//! (string-concat → no space after comma):
//!   points = (0,0)(w,0)(w,-h)(0,-h)(0,0)
//!            (-8,0)(w+8,0)(w+8,-h)(-8,-h)(-8,0)
//! `FRAME_WIDTH = 8` matches upstream.

use super::types::emit_polygon_node_with_transform;
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    // `node.width` is the *visual* width post-`updateNodeBounds` —
    // it includes both side frames (each `FRAME_WIDTH = 8`). The
    // upstream raw polygon is built around the inner rect width
    // `w = visual_w - 2 * FRAME_WIDTH` and translated by
    // `(-w/2, h/2)`; the outer rewind extends an additional 8 px on
    // each side, recovering the original visual width.
    let visual_w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let w = visual_w - 16.0;
    let pts = [
        (0.0, 0.0),
        (w, 0.0),
        (w, -h),
        (0.0, -h),
        (0.0, 0.0),
        (-8.0, 0.0),
        (w + 8.0, 0.0),
        (w + 8.0, -h),
        (-8.0, -h),
        (-8.0, 0.0),
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
    fn subroutine_has_ten_vertices() {
        // visual_w = 80 (= base_w 64 + 2 × FRAME_WIDTH 8); h = 40.
        let mut n = Node::default();
        n.id = "sr".into();
        n.width = Some(80.0);
        n.height = Some(40.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        let points = got
            .split(r#"points=""#)
            .nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap();
        assert_eq!(points.split(' ').count(), 10);
        // base_w = 64; the polygon's centring transform uses
        // `(-base_w/2, h/2)` = `(-32, 20)`.
        assert!(got.contains(r#"transform="translate(-32,20)""#));
    }
}
