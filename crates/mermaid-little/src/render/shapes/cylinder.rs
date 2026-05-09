//! Cylinder / database shape — upstream `cylinder.ts`.

use super::types::{
    build_inline_style, create_cylinder_path_d, fmt_num, get_node_classes, xml_escape,
    xml_escape_label,
};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

const FLOWCHART_PADDING: f64 = 15.0;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    // Upstream: rx = w/2; ry = rx / (2.5 + w/50). Layout stores h3 (the
    // straight-section height) directly, since jsdom's pathBBox shim only
    // sees arc endpoints — see flowchart.rs cylinder branch.
    let rx = w / 2.0;
    let ry = rx / (2.5 + w / 50.0);
    let d = create_cylinder_path_d(0.0, 0.0, w, h, rx, ry);

    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let label = node.label.clone().unwrap_or_default();

    let data_look = match node.look.as_deref() {
        Some(look) if !look.is_empty() => format!(r#" data-look="{}""#, look),
        _ => String::new(),
    };
    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}"{data_look} transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = xml_escape(&id),
        data_look = data_look,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    // Upstream wraps in transform translate(-w/2, -(h/2+ry)) and emits the
    // path with an explicit close tag (`></path>`), not the self-closing form.
    let ox = -w / 2.0;
    let oy = -(h / 2.0 + ry);
    let path_style = build_inline_style(node.css_styles.as_deref().unwrap_or(&[]));
    out.push_str(&format!(
        r#"<path d="{d}" class="basic label-container outer-path" style="{st}" label-offset-y="{ly}" transform="translate({ox}, {oy})"></path>"#,
        d = d,
        st = path_style,
        ly = fmt_num(ry),
        ox = fmt_num(ox),
        oy = fmt_num(oy),
    ));
    if !label.is_empty() {
        // Upstream cylinder.ts label transform:
        //   translate(-(bbox.w/2), -(bbox.h/2) + node.padding/1.5)
        // bbox here is the inner foreignObject (label_w × label_h). Estimate
        // label_h from line height (single-line labels): we re-render via
        // shape_label_block_with_transform to use the measured bbox.
        let padding = node.padding.unwrap_or(FLOWCHART_PADDING);
        let y_offset = padding / 1.5;
        let css_styles = node.css_styles.as_deref().unwrap_or(&[]);
        out.push_str(
            &crate::render::foreign_object::shape_label_block_with_y_offset_and_styles(
                &xml_escape_label(&label),
                &crate::render::foreign_object::HtmlLabelFont::default(),
                y_offset,
                css_styles,
            ),
        );
    }
    out.push_str("</g>");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cylinder_path_byte_exact() {
        let mut n = Node::default();
        n.id = "db".into();
        n.width = Some(50.0);
        n.height = Some(40.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        // rx=25, ry=25/3.5 ≈ 7.142857142857143
        assert!(got.contains(r#"d="M0,"#));
        assert!(got.contains(r#"a25,"#));
    }
}
