//! Rectangle with title — upstream `rectWithTitle.ts`.
//!
//! Port scope: emits the outer rect + divider line at the title
//! baseline + two label slots (title, description). Label layout is
//! simplified relative to upstream (no bounding-box round-trips);
//! byte-exact for the container + divider line; label tspan
//! placement is deferred to the future richer label helper.

use super::types::{fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let x = -w / 2.0;
    let y = -h / 2.0;
    // Title height: upstream measures via DOM; here we take
    // `node.padding`'s implied band. Fall back to `h/3` if no better
    // info available — matches visual convention for composite states.
    let title_h = node.padding.unwrap_or(h / 3.0);

    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let title = node.label.clone().unwrap_or_default();
    let descr = node
        .description
        .as_ref()
        .map(|d| d.join("\n"))
        .unwrap_or_default();

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}" transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    out.push_str(&format!(
        r#"<rect class="outer title-state" style="" x="{x}" y="{y}" width="{w}" height="{h}"/>"#,
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    let ly = y + title_h;
    out.push_str(&format!(
        r#"<line class="divider" x1="{x1}" x2="{x2}" y1="{ly}" y2="{ly}"/>"#,
        x1 = fmt_num(x),
        x2 = fmt_num(-x),
        ly = fmt_num(ly),
    ));
    use crate::render::foreign_object::{
        foreign_object_body, measure_html_label, HtmlLabelFont, LabelOpts,
    };
    if !title.is_empty() {
        let esc = xml_escape(&title);
        let (fw, fh) = measure_html_label(&esc, &HtmlLabelFont::default(), 200.0, true);
        let cy = y + title_h / 2.0;
        let opts = LabelOpts::default();
        out.push_str(&format!(
            r#"<g class="label" transform="translate({tx}, {ty})">"#,
            tx = fmt_num(-fw / 2.0),
            ty = fmt_num(cy - fh / 2.0),
        ));
        out.push_str("<rect></rect>");
        out.push_str(&foreign_object_body(&esc, fw, fh, &opts));
        out.push_str("</g>");
    }
    if !descr.is_empty() {
        let esc = xml_escape(&descr);
        let (fw, fh) = measure_html_label(&esc, &HtmlLabelFont::default(), 200.0, true);
        let cy = ly + (h - title_h) / 2.0;
        let opts = LabelOpts::default();
        out.push_str(&format!(
            r#"<g class="label" transform="translate({tx}, {ty})">"#,
            tx = fmt_num(-fw / 2.0),
            ty = fmt_num(cy - fh / 2.0),
        ));
        out.push_str("<rect></rect>");
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
    fn rect_with_title_has_divider() {
        let mut n = Node::default();
        n.id = "rt".into();
        n.width = Some(120.0);
        n.height = Some(60.0);
        n.padding = Some(20.0);
        n.label = Some("Title".into());
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        // divider at y = -30 + 20 = -10
        assert!(got.contains(r#"<line class="divider" x1="-60" x2="60" y1="-10" y2="-10"/>"#));
    }
}
