//! Sequence-diagram SVG render (scaffold).
//!
//! Upstream reference: `packages/mermaid/src/diagrams/sequence/svgDraw.ts`
//!
//! Emits a minimum-viable SVG that places each actor box at its layout
//! X column with a vertical lifeline, and one line per message. This
//! is NOT byte-exact with upstream — every sequence fixture stays in
//! `tests/known_ignored.txt` until the full svgDraw port lands.

use crate::error::Result;
use crate::layout::sequence::SequenceLayout;
use crate::model::sequence::SequenceDiagram;
use crate::theme::ThemeVariables;

type Theme = ThemeVariables;

pub fn render(
    _d: &SequenceDiagram,
    l: &SequenceLayout,
    _theme: &Theme,
    id: &str,
) -> Result<String> {
    let mut s = String::new();
    let vb_x = l.view_box_x;
    let vb_y = l.view_box_y;
    let vb_w = l.width + 100.0;
    let vb_h = l.height + 50.0;

    s.push_str(&format!(
        "<svg id=\"{}\" width=\"100%\" xmlns=\"http://www.w3.org/2000/svg\" \
         viewBox=\"{:.0} {:.0} {:.0} {:.0}\" \
         role=\"graphics-document document\" aria-roledescription=\"sequence\">",
        id, vb_x, vb_y, vb_w, vb_h
    ));

    // Top + bottom actor boxes + lifelines.
    for a in &l.actors {
        s.push_str(&format!(
            "<g><rect x=\"{:.0}\" y=\"0\" width=\"{:.0}\" height=\"{:.0}\" \
             fill=\"#eaeaea\" stroke=\"#666\" rx=\"3\" ry=\"3\" class=\"actor actor-top\" name=\"{}\"></rect>\
             <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
             dominant-baseline=\"central\" alignment-baseline=\"central\" class=\"actor actor-box\">\
             <tspan x=\"{:.1}\" dy=\"0\">{}</tspan></text></g>",
            a.x,
            a.width,
            a.height,
            escape(&a.id),
            a.x + a.width / 2.0,
            a.height / 2.0,
            a.x + a.width / 2.0,
            escape(&a.description),
        ));
        // lifeline
        s.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.0}\" x2=\"{:.1}\" y2=\"{:.0}\" \
             class=\"actor-line\" stroke-width=\"0.5px\" stroke=\"#999\"></line>",
            a.x + a.width / 2.0,
            a.height,
            a.x + a.width / 2.0,
            l.height - a.height,
        ));
        // bottom box
        s.push_str(&format!(
            "<g><rect x=\"{:.0}\" y=\"{:.1}\" width=\"{:.0}\" height=\"{:.0}\" \
             fill=\"#eaeaea\" stroke=\"#666\" rx=\"3\" ry=\"3\" class=\"actor actor-bottom\" name=\"{}\"></rect>\
             <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
             dominant-baseline=\"central\" alignment-baseline=\"central\" class=\"actor actor-box\">\
             <tspan x=\"{:.1}\" dy=\"0\">{}</tspan></text></g>",
            a.x,
            l.height - a.height,
            a.width,
            a.height,
            escape(&a.id),
            a.x + a.width / 2.0,
            l.height - a.height / 2.0,
            a.x + a.width / 2.0,
            escape(&a.description),
        ));
    }

    // Messages: one solid line + label.
    for m in &l.messages {
        let from_x = l
            .actors
            .iter()
            .find(|a| a.id == m.from)
            .map(|a| a.x + a.width / 2.0)
            .unwrap_or(0.0);
        let to_x = l
            .actors
            .iter()
            .find(|a| a.id == m.to)
            .map(|a| a.x + a.width / 2.0)
            .unwrap_or(0.0);
        s.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" class=\"messageLine0\" \
             stroke-width=\"1.5\" stroke=\"#333\"></line>",
            from_x, m.y, to_x, m.y,
        ));
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" class=\"messageText\">{}</text>",
            (from_x + to_x) / 2.0,
            m.y - 5.0,
            escape(&m.text),
        ));
    }

    s.push_str("</svg>");
    Ok(s)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
