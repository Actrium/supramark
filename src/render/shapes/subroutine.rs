//! Subroutine shape — rectangle with two vertical bars.
//! Upstream: `subroutine.ts`.
//!
//! Upstream emits a 10-vertex polygon (outer box + inner box joined
//! with rewind lines). We emit the same polygon with `FRAME_WIDTH =
//! 8` matching upstream.

use super::types::emit_polygon_node;
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    // Upstream works in un-translated (top-left at 0,0) coords. The
    // outer group's translate(-w/2, h/2) (after upstream's final
    // transform) is baked into the centred vertices below.
    let hw = w / 2.0;
    let hh = h / 2.0;
    let pts = [
        (-hw, hh),
        (hw, hh),
        (hw, -hh),
        (-hw, -hh),
        (-hw, hh),
        (-hw - 8.0, hh),
        (hw + 8.0, hh),
        (hw + 8.0, -hh),
        (-hw - 8.0, -hh),
        (-hw - 8.0, hh),
    ];
    Ok(emit_polygon_node(node, &pts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subroutine_has_ten_vertices() {
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
    }
}
