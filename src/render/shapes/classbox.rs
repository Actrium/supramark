//! Class-diagram class box — upstream `classBox.ts`.
//!
//! Three-section rectangle: name header, members area, methods area,
//! with two dividers. Upstream mirrors this layout through
//! `textHelper` which composes three `<g>` sub-groups and auto-sizes
//! them. Our port takes the already-laid-out `width` / `height` from
//! `Node` and computes divider y-coordinates from the member counts
//! that the diagram adapter records in `node.description[0]` (member
//! count) and `node.description[1]` (method count).

use super::types::{fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let x = -w / 2.0;
    let y = -h / 2.0;
    // Three equally-sized bands as a fallback; the adapter should set
    // `node.description = vec![header_h, members_h, methods_h]` for
    // byte-accurate layout.
    let (band1, band2) = if let Some(descr) = &node.description {
        let b1: f64 = descr
            .first()
            .and_then(|s| s.parse().ok())
            .unwrap_or(h / 3.0);
        let b2: f64 = descr.get(1).and_then(|s| s.parse().ok()).unwrap_or(h / 3.0);
        (b1, b2)
    } else {
        (h / 3.0, h / 3.0)
    };

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
    // Dividers.
    let y1 = y + band1;
    let y2 = y + band1 + band2;
    out.push_str(&format!(
        r#"<line class="divider" x1="{x}" x2="{rx}" y1="{y1}" y2="{y1}"/>"#,
        x = fmt_num(x),
        rx = fmt_num(-x),
        y1 = fmt_num(y1),
    ));
    out.push_str(&format!(
        r#"<line class="divider" x1="{x}" x2="{rx}" y1="{y2}" y2="{y2}"/>"#,
        x = fmt_num(x),
        rx = fmt_num(-x),
        y2 = fmt_num(y2),
    ));
    if let Some(label) = &node.label {
        // Class-title label uses the markdown-node-label span class per
        // upstream `classBox.ts` + `labelHelper` when the config sets
        // markdown rendering. Emit a foreignObject at the centre of the
        // title band.
        use crate::render::foreign_object::{
            foreign_object_body, measure_html_label, HtmlLabelFont, LabelOpts,
        };
        let esc = xml_escape(label);
        let (fw, fh) = measure_html_label(&esc, &HtmlLabelFont::default(), 200.0, true);
        let ly = y + band1 / 2.0;
        let opts = LabelOpts {
            extra_span_classes: "markdown-node-label",
            ..LabelOpts::default()
        };
        out.push_str(&format!(
            r#"<g class="label" transform="translate({tx}, {ty})">"#,
            tx = fmt_num(-fw / 2.0),
            ty = fmt_num(ly - fh / 2.0),
        ));
        out.push_str(&foreign_object_body(&esc, fw, fh, &opts));
        out.push_str("</g>");
    }
    out.push_str("</g>");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classbox_has_two_dividers() {
        let mut n = Node::default();
        n.id = "cls".into();
        n.width = Some(150.0);
        n.height = Some(120.0);
        n.label = Some("MyClass".into());
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        assert_eq!(got.matches("divider").count(), 2);
        assert!(got.contains("MyClass"));
    }

    /// Structural parity: classBox titles should use `<foreignObject>` +
    /// `<span class="nodeLabel markdown-node-label">` now that the
    /// label stack is wired through.
    #[test]
    fn classbox_title_uses_foreign_object() {
        let mut n = Node::default();
        n.id = "cls".into();
        n.width = Some(120.0);
        n.height = Some(90.0);
        n.label = Some("Animal".into());
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        assert!(
            got.contains(r#"<foreignObject width="#),
            "title should be wrapped in foreignObject, got:\n{got}"
        );
        assert!(
            got.contains(r#"<span class="nodeLabel markdown-node-label">"#),
            "title span should have markdown-node-label class"
        );
        assert!(got.contains("<p>Animal</p>"));
    }
}
