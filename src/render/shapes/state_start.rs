//! state-diagram initial-state marker (filled circle) — upstream
//! `stateStart.ts`. Default width/height: 14.

use super::types::{fmt_num, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(14.0).max(14.0);
    let r = w / 2.0;
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);

    Ok(format!(
        r#"<g class="node default" id="{id}" transform="translate({tx}, {ty})"><circle class="state-start" r="{r}" width="{w}" height="{w}" cx="0" cy="0"/></g>"#,
        id = xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
        r = fmt_num(r),
        w = fmt_num(w),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_start_defaults_to_14() {
        let mut n = Node::default();
        n.id = "ss".into();
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        assert!(got.contains(r#"r="7""#));
        assert!(got.contains(r#"state-start"#));
    }
}
