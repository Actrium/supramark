//! ER-diagram entity box — upstream `erBox.ts`.
//!
//! Layout: one header row (entity name) plus one row per attribute.
//! Attribute rows alternate fill between `rowEven` and `rowOdd`.
//! Upstream handles up to 4 columns (type / name / keys / comment);
//! the adapter must supply column widths via
//! `node.description` (a column-count × 1 vec of newline-joined
//! attribute strings) for byte-accurate layout.
//!
//! For Wave 4 we emit the container + a single horizontal divider
//! under the name row. Column dividers and per-row fills are
//! deferred to Wave 5 when the ER adapter lands.

use super::types::{fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let x = -w / 2.0;
    let y = -h / 2.0;
    let header_h = node.padding.unwrap_or(24.0);

    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let border = theme.node_border.as_deref().unwrap_or("");

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}" transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    out.push_str(&format!(
        r#"<rect class="outer-path" style="stroke:{b}" x="{x}" y="{y}" width="{w}" height="{h}"/>"#,
        b = border,
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    let ly = y + header_h;
    out.push_str(&format!(
        r#"<line class="divider" x1="{x}" x2="{rx}" y1="{ly}" y2="{ly}"/>"#,
        x = fmt_num(x),
        rx = fmt_num(-x),
        ly = fmt_num(ly),
    ));
    if let Some(label) = &node.label {
        out.push_str(&format!(
            r#"<g class="label name" transform="translate(0, {ny})"><text>{t}</text></g>"#,
            ny = fmt_num(y + header_h / 2.0),
            t = xml_escape(label),
        ));
    }
    out.push_str("</g>");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erbox_has_name_divider() {
        let mut n = Node::default();
        n.id = "e".into();
        n.width = Some(200.0);
        n.height = Some(80.0);
        n.padding = Some(24.0);
        n.label = Some("Customer".into());
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        // y=-40, header=24 → divider at -16
        assert!(got.contains(r#"y1="-16""#));
        assert!(got.contains("Customer"));
    }
}
