//! Invisible label rect — upstream `labelRect.ts`.
//!
//! Used for edge labels and block-diagram labels that need to
//! participate in layout but carry no visible container. Emits a
//! degenerate 0.1×0.1 `<rect>` plus the centred `<text>` (or, for
//! empty labels, a zero-width `<foreignObject>` block matching
//! upstream's `labelHelper` output for label-less nodes).
//!
//! Upstream reference DOM (cypress/flowchart/187 helper):
//!
//! ```text
//! <g class="label edgeLabel" id="…" transform="translate(tx, ty)">
//!   <rect width="0.1" height="0.1"></rect>
//!   <g class="label" style="" transform="translate(0, -8.1484375)">
//!     <rect></rect>
//!     <foreignObject width="0" height="16.296875">
//!       <div style="display: table-cell; white-space: nowrap;
//!                   line-height: 1.5; max-width: 10px; text-align: center;"
//!            xmlns="http://www.w3.org/1999/xhtml">
//!         <span class="nodeLabel "></span>
//!       </div>
//!     </foreignObject>
//!   </g>
//! </g>
//! ```
//!
//! Notes that affect byte-exact output:
//! * The outer `<rect width="0.1" height="0.1"></rect>` MUST be emitted
//!   as paired tags — upstream's d3 `.append("rect")` always serialises
//!   to an opened element rather than a self-closing one.
//! * Empty labels still produce a full `<foreignObject>` body. Upstream's
//!   `labelHelper` does not short-circuit on empty text; the inner
//!   `<span class="nodeLabel "></span>` is what consumers measure.

use super::types::{fmt_num, xml_escape, xml_escape_label};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::render::foreign_object::{render_node_label, LabelOpts};
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let label = node.label.clone().unwrap_or_default();

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="label edgeLabel" id="{id}" transform="translate({tx}, {ty})">"#,
        id = xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    // Outer placeholder rect — emitted in the paired `<rect></rect>` form
    // to match upstream's d3 serialiser. Self-closing here would diverge
    // byte-for-byte from the reference SVG.
    out.push_str(r#"<rect width="0.1" height="0.1"></rect>"#);

    if label.is_empty() {
        // Empty-label helper: upstream's `labelHelper` emits a full
        // `<foreignObject>` body even when the label text is zero
        // characters wide. The width budget for the inner `<div>` is
        // taken from the helper rect (10 px in cypress/flowchart/187),
        // and the inner foreignObject geometry is `width=0` /
        // `height=line_height(14px sans-serif)` ≈ 16.296875.
        let mut opts = LabelOpts::default();
        opts.is_node = true;
        opts.add_background = false;
        opts.wrap_in_p = false;
        // Cyclic self-loop helpers are rendered from an initial 10 px budget,
        // then the DOM pass shrinks the actual shape bbox to 0.1x0.1 before
        // dagre runs. Preserve the 10 px HTML label budget here even though
        // the post-layout helper node now carries the shrunken bbox.
        let helper_width =
            if node.extra.get("synthetic").map(|s| s.as_str()) == Some("cyclic_helper") {
                10.0
            } else {
                node.width.unwrap_or(10.0)
            };
        opts.max_width = helper_width;
        // Match upstream's `<g class="label" style="" transform="translate(0, -8.1484375)">`.
        // The translate is `(0, -line_height/2)` for empty labels — they
        // are not bbox-centred (width=0 ⇒ `-w/2 == 0`). 16.296875 is
        // `line_height("sans-serif", 14px, !bold)` from `font_metrics`.
        let inner_h = 16.296875_f64;
        opts.group_transform = Some(format!("translate(0, {})", fmt_num(-inner_h / 2.0)));
        let inner = render_node_label("", 0.0, inner_h, &opts);
        out.push_str(&inner);
    } else {
        let css_styles = node.css_styles.as_deref().unwrap_or(&[]);
        out.push_str(
            &crate::render::foreign_object::shape_label_block_with_styles(
                &xml_escape_label(&label),
                &crate::render::foreign_object::HtmlLabelFont::default(),
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
    fn label_rect_byte_exact_minimum() {
        let mut n = Node::default();
        n.id = "lr".into();
        n.label = Some("edge".into());
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        assert!(got.contains(r#"class="label edgeLabel""#));
        assert!(got.contains(r#"<rect width="0.1" height="0.1"></rect>"#));
        assert!(got.contains("edge"));
    }

    #[test]
    fn label_rect_empty_label_emits_helper_block() {
        // Upstream `labelHelper` emits a full <foreignObject> body even
        // for empty text — verify we mirror it.
        let mut n = Node::default();
        n.id = "A---A---1".into();
        n.x = Some(100.0);
        n.y = Some(200.0);
        n.width = Some(10.0);
        // n.label left as None → empty.
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        assert!(got.contains(r#"<rect width="0.1" height="0.1"></rect>"#));
        assert!(got.contains(r#"<g class="label" style="" transform="translate(0, -8.1484375)">"#));
        assert!(got.contains(r#"<rect></rect><foreignObject width="0" height="16.296875">"#));
        assert!(got.contains(r#"<span class="nodeLabel "></span>"#));
        assert!(got.contains("max-width: 10px"));
    }
}
