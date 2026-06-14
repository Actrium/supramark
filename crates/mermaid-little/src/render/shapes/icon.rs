use super::types::fmt_num;
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

const ICON_SIZE: f64 = 48.0;
const ICON_SVG_PAD: f64 = 16.0;

fn icon_pack_fill(icon_val: &str) -> &str {
    if icon_val.starts_with("aws:") {
        "#087ebf"
    } else {
        "#087ebf"
    }
}

pub fn draw(node: &Node, theme: &ThemeVariables) -> Result<String> {
    let h = node.height.unwrap_or(ICON_SIZE + 16.296875);
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let css = node.css_classes.as_deref().unwrap_or("undefined");
    let classes = format!("icon-shape {}", css.trim_end());
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());

    let data_look = match node.look.as_deref() {
        Some(look) if !look.is_empty() => format!(r#" data-look="{}""#, look),
        _ => String::new(),
    };

    let label_text = node.label.as_deref().unwrap_or("");
    let has_label = !label_text.is_empty();
    let is_markdown = node.label_type.as_deref() == Some("markdown");

    let label_content = if has_label {
        if is_markdown {
            crate::render::foreign_object::markdown_label_to_html(label_text)
        } else {
            crate::render::foreign_object::string_label_to_html(label_text)
        }
    } else {
        String::new()
    };
    let for_measure = if has_label {
        crate::render::foreign_object::replace_fa_icons(&label_content)
    } else {
        String::new()
    };
    let font = crate::render::foreign_object::HtmlLabelFont::default();
    let (lw, lh) = if has_label {
        crate::render::foreign_object::measure_html_markup_label(&for_measure, &font, 200.0, true)
    } else {
        (
            0.0,
            crate::render::foreign_object::measure_html_markup_label("", &font, 200.0, true).1,
        )
    };

    let gap = if has_label { 8.0 } else { 0.0 };
    let icon_center_y = -(gap + lh) / 2.0;

    let bounding_w = if lw > ICON_SIZE { lw } else { ICON_SIZE };

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}"{data_look} transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = super::types::xml_escape(&id),
        data_look = data_look,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));

    out.push_str(&format!(
        r#"<g transform="translate(0,{icy})"><path d="M-24 -24 L24 -24 L24 24 L-24 24" stroke="none" stroke-width="0" fill="none"></path></g>"#,
        icy = fmt_num(icon_center_y),
    ));

    let label_translate_x = if has_label && lw > 0.0 {
        fmt_num(-lw / 2.0)
    } else {
        "0".to_string()
    };
    let label_translate_y = fmt_num((ICON_SIZE + gap - lh) / 2.0);

    // Body wrap: upstream `createText` (used by `labelHelper`) routes through
    // `addHtmlLabel`, which always wraps the rendered HTML in `<p>…</p>` when
    // the label is non-empty. Empty labels produce `<span …></span>` with no
    // inner body — matching cypress/116 and cypress/117 (no-label fixtures).
    let inner_html = if has_label {
        format!("<p>{}</p>", for_measure)
    } else {
        String::new()
    };
    out.push_str(&format!(
        r#"<g class="label" style="" transform="translate({ltx},{lty})"><rect></rect><foreignObject width="{lw}" height="{lh}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><span class="nodeLabel {md_cls}">{label_html}</span></div></foreignObject></g>"#,
        ltx = label_translate_x,
        lty = label_translate_y,
        lw = fmt_num(lw),
        lh = fmt_num(lh),
        md_cls = if is_markdown && has_label {
            "markdown-node-label"
        } else {
            ""
        },
        label_html = inner_html,
    ));

    let bw_half = fmt_num(bounding_w / 2.0);
    let bh_half = fmt_num(h / 2.0);
    out.push_str(&format!(
        r#"<g><path d="M-{bw2} -{bh2} L{bw2} -{bh2} L{bw2} {bh2} L-{bw2} {bh2}" stroke="none" stroke-width="0" fill="transparent"></path></g>"#,
        bw2 = bw_half,
        bh2 = bh_half,
    ));

    let icon_val = node.icon.as_deref().unwrap_or("");
    let icon_fill = icon_pack_fill(icon_val);
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let icon_svg_tx = fmt_num(-(ICON_SIZE / 2.0 + ICON_SVG_PAD));
    let icon_svg_ty = fmt_num(-(h / 2.0 + ICON_SVG_PAD));

    out.push_str(&format!(
        r#"<g transform="translate({itx},{ity})" style="color: {nb};"><g><svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 80 80"><g><rect width="80" height="80" style="fill: {icf}; stroke-width: 0px;"></rect><text transform="translate(21.16 64.67)" style="fill: #fff; font-family: ArialMT, Arial; font-size: 67.75px;"><tspan x="0" y="0">?</tspan></text></g></svg></g></g>"#,
        itx = icon_svg_tx,
        ity = icon_svg_ty,
        nb = node_border,
        icf = icon_fill,
    ));

    out.push_str("</g>");
    Ok(out)
}
