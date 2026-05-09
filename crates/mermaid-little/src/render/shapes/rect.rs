//! Plain rectangle shape.
//!
//! Upstream reference:
//! `packages/mermaid/src/rendering-util/rendering-elements/shapes/squareRect.ts` +
//! `packages/mermaid/src/rendering-util/rendering-elements/shapes/drawRect.ts`.
//!
//! Emits a `<rect>` sized to the node's post-layout width/height
//! anchored at `(-w/2, -h/2)` (upstream convention — the parent `<g>`
//! carries the translate).

use super::types::{
    build_div_style_prefix, build_inline_style, build_label_style, fmt_num, get_node_classes,
};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

/// Draw a plain rectangle shape. Byte-compatible with upstream
/// `squareRect` → `drawRect(options: rx=0, ry=0)`.
pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let x = -w / 2.0;
    let y = -h / 2.0;
    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let label = node.label.clone().unwrap_or_default();
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);

    // When `node.look` is set (e.g. "classic"), emit `data-look` attribute.
    let data_look = match node.look.as_deref() {
        Some(look) if !look.is_empty() => format!(r#" data-look="{}""#, look),
        _ => String::new(),
    };

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}"{data_look} transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = super::types::xml_escape(&id),
        data_look = data_look,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    // Upstream omits rx/ry when both are 0 (D3's .attr() skips
    // attributes whose value is 0 for rect elements). Only emit
    // them when the node has rx/ry set (e.g. round shape).
    let rx_ry = match (node.rx, node.ry) {
        (Some(rx), Some(ry)) if rx > 0.0 && ry > 0.0 => {
            format!(r#" rx="{}" ry="{}""#, fmt_num(rx), fmt_num(ry))
        }
        _ => String::new(),
    };
    // Inline styles from `style <id> …` directives, each decorated with
    // `!important` (matching upstream's applyStyles utility).
    let rect_style = build_inline_style(node.css_styles.as_deref().unwrap_or(&[]));
    out.push_str(&format!(
        r#"<rect class="basic label-container" style="{rect_style}"{rx_ry} x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
        rect_style = rect_style,
        rx_ry = rx_ry,
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    if !label.is_empty() {
        // KaTeX `$$..$$` math block — bypass markdown / FA paths and route
        // the entire label through the embedded KaTeX pipeline. The result
        // is a fully-formed `<div>…</div>` wrapper, so the inner span must
        // skip its `<p>` wrapping (`wrap_in_p: false`).
        let font = crate::render::foreign_object::HtmlLabelFont::default();
        if let Some((katex_html, lw, lh)) =
            crate::render::foreign_object::try_render_katex_label(&label, &font)
        {
            let css = node.css_styles.as_deref().unwrap_or(&[]);
            let lbl_style = build_label_style(css);
            let div_prefix = build_div_style_prefix(css);
            let opts = crate::render::foreign_object::LabelOpts {
                group_style: if lbl_style.is_empty() {
                    Some("")
                } else {
                    Some(&lbl_style)
                },
                label_style: if lbl_style.is_empty() {
                    None
                } else {
                    Some(&lbl_style)
                },
                div_style_prefix: if div_prefix.is_empty() {
                    None
                } else {
                    Some(&div_prefix)
                },
                wrap_in_p: false,
                ..crate::render::foreign_object::LabelOpts::default()
            };
            out.push_str(&crate::render::foreign_object::render_node_label(
                &katex_html,
                lw,
                lh,
                &opts,
            ));
            out.push_str("</g>");
            return Ok(out);
        }
        // HTML foreignObject label matching upstream `labelHelper` /
        // `addHtmlSpan` output. Width/height come from the jsdom-shim
        // measurement (14px sans-serif default).
        //
        // For "string" label types (quoted HTML labels), pass raw without escaping.
        // For "markdown" label types, convert markdown syntax to HTML first.
        // For plain text, XML-escape.
        let is_markdown = node.label_type.as_deref() == Some("markdown");
        let label_content = if is_markdown {
            crate::render::foreign_object::markdown_label_to_html(&label)
        } else {
            // "string" and "text" labels: process embedded HTML tags and escape text chars
            crate::render::foreign_object::string_label_to_html(&label)
        };
        // Apply FA icon substitution before measuring: fa:fa-car → <i ...></i> (zero width).
        let for_measure = crate::render::foreign_object::replace_fa_icons(&label_content);
        let font = crate::render::foreign_object::HtmlLabelFont::default();
        let (lw, lh) = crate::render::foreign_object::measure_html_markup_label(
            &for_measure,
            &font,
            200.0,
            true,
        );
        // Label-specific CSS (color:, font-*) from `style` directives:
        //   - group_style → <g class="label" style="..."> (raw hex)
        //   - label_style → <span style="..."> (raw hex)
        //   - div_style_prefix → <div style="prefix; display: ..."> (rgb-converted)
        let css = node.css_styles.as_deref().unwrap_or(&[]);
        let lbl_style = build_label_style(css);
        let div_prefix = build_div_style_prefix(css);
        let opts = crate::render::foreign_object::LabelOpts {
            extra_span_classes: if is_markdown {
                "markdown-node-label"
            } else {
                ""
            },
            group_style: if lbl_style.is_empty() {
                Some("")
            } else {
                Some(&lbl_style)
            },
            label_style: if lbl_style.is_empty() {
                None
            } else {
                Some(&lbl_style)
            },
            div_style_prefix: if div_prefix.is_empty() {
                None
            } else {
                Some(&div_prefix)
            },
            ..crate::render::foreign_object::LabelOpts::default()
        };
        // Use the FA-processed version for rendering too (contains <i> tags).
        out.push_str(&crate::render::foreign_object::render_node_label(
            &for_measure,
            lw,
            lh,
            &opts,
        ));
    }
    out.push_str("</g>");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_rect_emits_foreign_object_label() {
        let mut n = Node::default();
        n.id = "n1".into();
        n.width = Some(100.0);
        n.height = Some(50.0);
        n.x = Some(60.0);
        n.y = Some(30.0);
        n.label = Some("hi".into());
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.starts_with(
            r#"<g class="node undefined " id="n1" transform="translate(60, 30)"><rect class="basic label-container" style="" x="-50" y="-25" width="100" height="50"></rect>"#
        ));
        assert!(
            got.contains(r#"<foreignObject "#),
            "label should use foreignObject, got:\n{got}"
        );
        assert!(got.contains(r#"<span class="nodeLabel "><p>hi</p></span>"#));
        assert!(got.ends_with("</g></g>"));
    }

    #[test]
    fn rect_without_label_omits_label_block() {
        let mut n = Node::default();
        n.id = "n2".into();
        n.width = Some(20.0);
        n.height = Some(10.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        assert!(got.contains(r#"width="20""#));
        assert!(!got.contains("<text>"));
        assert!(!got.contains("<foreignObject"));
    }
}
