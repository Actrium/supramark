//! state-diagram final-state marker (ring + filled inner) —
//! upstream `stateEnd.ts`.

use super::types::{fmt_num, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(14.0).max(14.0);
    let outer = w / 2.0;
    let inner = (w * 5.0) / 14.0;
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let line = theme.line_color.as_deref().unwrap_or("");
    let nborder = theme.node_border.as_deref().unwrap_or("");

    Ok(format!(
        r#"<g class="node default" id="{id}" transform="translate({tx}, {ty})"><g class="outer-path"><circle r="{r}" cx="0" cy="0" style="stroke:{line};stroke-width:2;fill:none"/><circle r="{ri}" cx="0" cy="0" style="fill:{nb};stroke:{nb};stroke-width:2"/></g></g>"#,
        id = xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
        r = fmt_num(outer),
        ri = fmt_num(inner),
        line = line,
        nb = nborder,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_end_radii_match_upstream_ratio() {
        let mut n = Node::default();
        n.id = "se".into();
        n.width = Some(14.0);
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        // outer=7, inner=5
        assert!(got.contains(r#"r="7""#));
        assert!(got.contains(r#"r="5""#));
    }
}
