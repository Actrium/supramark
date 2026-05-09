//! Fork / join bar — upstream `forkJoin.ts`.
//!
//! A filled rectangle 70×10 (TB) or 10×70 (LR) sized by the layout
//! direction. Uses `themeVariables.lineColor` for both fill and
//! stroke.

use super::types::{fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, theme: &ThemeVariables) -> Result<String> {
    let dir = node.dir.as_deref();
    let (w, h) = if matches!(dir, Some("LR")) {
        (
            node.width.unwrap_or(10.0).max(10.0),
            node.height.unwrap_or(70.0).max(70.0),
        )
    } else {
        (
            node.width.unwrap_or(70.0).max(70.0),
            node.height.unwrap_or(10.0).max(10.0),
        )
    };
    let x = -w / 2.0;
    let y = -h / 2.0;
    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let line = theme.line_color.as_deref().unwrap_or("black");

    Ok(format!(
        r#"<g class="{classes}" id="{id}" transform="translate({tx}, {ty})"><rect x="{x}" y="{y}" width="{w}" height="{h}" style="fill:{line};stroke:{line}"/></g>"#,
        classes = classes,
        id = xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(w),
        h = fmt_num(h),
        line = line,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_join_default_tb_dimensions() {
        let mut n = Node::default();
        n.id = "f".into();
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        assert!(got.contains(r#"width="70""#));
        assert!(got.contains(r#"height="10""#));
    }

    #[test]
    fn fork_join_lr_swaps_dimensions() {
        let mut n = Node::default();
        n.id = "f".into();
        n.dir = Some("LR".into());
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        assert!(got.contains(r#"width="10""#));
        assert!(got.contains(r#"height="70""#));
    }
}
