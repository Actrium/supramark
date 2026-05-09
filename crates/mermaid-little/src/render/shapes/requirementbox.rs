//! Requirement-diagram box — upstream `requirementBox.ts`.
//!
//! Rect with internal divider separating `<<type>>` + name header
//! from body rows (id / text / risk / verification).

use super::types::{fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let x = -w / 2.0;
    let y = -h / 2.0;
    // Upstream's padding/gap constants: both `20`. Header height
    // ≈ two label lines (type + name) + gap → ~ 2*20 + gap.
    let header_h = node.padding.unwrap_or(60.0);

    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}" transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    out.push_str(&format!(
        r#"<rect class="basic label-container outer-path" style="" x="{x}" y="{y}" width="{w}" height="{h}"/>"#,
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    let ly = y + header_h;
    if ly < y + h {
        out.push_str(&format!(
            r#"<line class="divider" x1="{x}" x2="{rx}" y1="{ly}" y2="{ly}"/>"#,
            x = fmt_num(x),
            rx = fmt_num(-x),
            ly = fmt_num(ly),
        ));
    }
    if let Some(label) = &node.label {
        out.push_str(&format!(
            r#"<g class="label" transform="translate(0, {ly})"><text>{t}</text></g>"#,
            ly = fmt_num(y + 20.0),
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
    fn requirementbox_has_divider_when_body_present() {
        let mut n = Node::default();
        n.id = "r".into();
        n.width = Some(200.0);
        n.height = Some(160.0);
        n.padding = Some(60.0);
        n.label = Some("Req1".into());
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        assert!(got.contains("divider"));
    }
}
