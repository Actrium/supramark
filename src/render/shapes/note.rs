//! Note shape — upstream `note.ts`.
//!
//! Plain rect filled with the theme's `noteBkgColor` / stroked with
//! `noteBorderColor`. `label.noteLabel` class is applied to the
//! inner label group.

use super::types::{fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let x = -w / 2.0;
    let y = -h / 2.0;
    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let label = node.label.clone().unwrap_or_default();

    let fill = theme.note_bkg_color.as_deref().unwrap_or("");
    let stroke = theme.note_border_color.as_deref().unwrap_or("");
    let style = format!("fill:{};stroke:{}", fill, stroke);

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}" transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    out.push_str(&format!(
        r#"<rect class="basic label-container outer-path" style="{style}" x="{x}" y="{y}" width="{w}" height="{h}"/>"#,
        style = style,
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    if !label.is_empty() {
        // Upstream emits the noteLabel marker inside a wrapper `<g class="label noteLabel">`.
        // We emit the inner label block ourselves so we can swap the class.
        use crate::render::foreign_object::{measure_html_label, HtmlLabelFont, LabelOpts};
        let esc = xml_escape(&label);
        let (fw, fh) = measure_html_label(&esc, &HtmlLabelFont::default(), 200.0, true);
        let opts = LabelOpts::default();
        // Emit the outer <g class="label noteLabel"> with bbox-centred
        // translate and inlined foreignObject.
        out.push_str(&format!(
            r#"<g class="label noteLabel" transform="translate({tx}, {ty})">"#,
            tx = fmt_num(-fw / 2.0),
            ty = fmt_num(-fh / 2.0),
        ));
        out.push_str("<rect></rect>");
        out.push_str(&crate::render::foreign_object::foreign_object_body(
            &esc, fw, fh, &opts,
        ));
        out.push_str("</g>");
    }
    out.push_str("</g>");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_includes_theme_colors() {
        let mut n = Node::default();
        n.id = "note1".into();
        n.width = Some(80.0);
        n.height = Some(40.0);
        n.label = Some("Heads up".into());
        let mut theme = ThemeVariables::default();
        theme.note_bkg_color = Some("#fff5ad".into());
        theme.note_border_color = Some("#aaaa33".into());
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains("fill:#fff5ad"));
        assert!(got.contains("stroke:#aaaa33"));
        assert!(got.contains("noteLabel"));
    }
}
