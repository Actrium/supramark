//! lean-left parallelogram — upstream `leanLeft.ts`.
//!
//! Upstream pre-translate points:
//!   (0, 0), (w + (3h)/6, 0), (w, -h), (-(3h)/6, -h)
//! Outer group carries `translate(-w/2, h/2)`.

use super::types::emit_polygon_node;
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let shear = (3.0 * h) / 6.0;
    let pts = [
        (-w / 2.0, h / 2.0),
        (w + shear - w / 2.0, h / 2.0),
        (w - w / 2.0, -h / 2.0),
        (-shear - w / 2.0, -h / 2.0),
    ];
    Ok(emit_polygon_node(node, &pts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lean_left_polygon_matches_upstream() {
        let mut n = Node::default();
        n.id = "ll".into();
        n.width = Some(60.0);
        n.height = Some(40.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains(r#"points="-30,20 50,20 30,-20 -50,-20""#));
    }
}
