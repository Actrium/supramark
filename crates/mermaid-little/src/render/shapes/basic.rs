//! Basic shape placeholder used by block diagrams — upstream blocks
//! render with `shape: 'rect'` by default and fall back to `'round'`
//! when explicit radius styling is applied.
//!
//! This module is an explicit alias for `rect::draw` so block
//! adapters that use the literal string `"basic"` resolve cleanly.

use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, theme: &ThemeVariables) -> Result<String> {
    super::rect::draw(node, theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_delegates_to_rect() {
        let mut n = Node::default();
        n.id = "b".into();
        n.width = Some(40.0);
        n.height = Some(20.0);
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        assert!(got.contains(r#"class="basic label-container""#));
    }
}
