//! Packet SVG renderer.
//!
//! Emits a byte-identical SVG to upstream mermaid@11.14.0 for the
//! packet diagram kind. Upstream composes the final CSS by passing
//! the per-diagram `styles(options)` fragment through stylis with the
//! root selector `#<svgId>`; the resulting minified CSS is appended
//! verbatim inside a `<style>` element. We skip the stylis round-trip
//! and use the post-serialisation CSS as a fixed template with a
//! single `{{ID}}` placeholder.
//!
//! The structural output (empty title group, per-word group, rect +
//! label + byte numbers in the upstream order) mirrors
//! `diagrams/packet/renderer.ts` line-for-line.

use crate::error::Result;
use crate::layout::packet::{PacketBlock, PacketLayout, PacketWord};
use crate::model::packet::PacketDiagram;
use crate::render::svg::{close_svg, open_svg, ViewBox};
use crate::theme::ThemeVariables;

/// Fully-minified CSS template — the single-byte-for-byte exact output
/// stylis produces for the default theme + packet styles bundle.
/// Substituting `{{ID}}` for the target SVG id yields the reference
/// `<style>` body.
const PACKET_CSS_TEMPLATE: &str = concat!(
    "#{{ID}}{font-family:\"trebuchet ms\",verdana,arial,sans-serif;font-size:16px;fill:#333;}",
    "@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}",
    "@keyframes dash{to{stroke-dashoffset:0;}}",
    "#{{ID}} .edge-animation-slow{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}",
    "#{{ID}} .edge-animation-fast{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}",
    "#{{ID}} .error-icon{fill:#552222;}",
    "#{{ID}} .error-text{fill:#552222;stroke:#552222;}",
    "#{{ID}} .edge-thickness-normal{stroke-width:1px;}",
    "#{{ID}} .edge-thickness-thick{stroke-width:3.5px;}",
    "#{{ID}} .edge-pattern-solid{stroke-dasharray:0;}",
    "#{{ID}} .edge-thickness-invisible{stroke-width:0;fill:none;}",
    "#{{ID}} .edge-pattern-dashed{stroke-dasharray:3;}",
    "#{{ID}} .edge-pattern-dotted{stroke-dasharray:2;}",
    "#{{ID}} .marker{fill:#333333;stroke:#333333;}",
    "#{{ID}} .marker.cross{stroke:#333333;}",
    "#{{ID}} svg{font-family:\"trebuchet ms\",verdana,arial,sans-serif;font-size:16px;}",
    "#{{ID}} p{margin:0;}",
    "#{{ID}} .packetByte{font-size:10px;}",
    "#{{ID}} .packetByte.start{fill:black;}",
    "#{{ID}} .packetByte.end{fill:black;}",
    "#{{ID}} .packetLabel{fill:black;font-size:12px;}",
    "#{{ID}} .packetTitle{fill:black;font-size:14px;}",
    "#{{ID}} .packetBlock{stroke:black;stroke-width:1;fill:#efefef;}",
    "#{{ID}} .node .neo-node{stroke:#9370DB;}",
    "#{{ID}} [data-look=\"neo\"].node rect,#{{ID}} [data-look=\"neo\"].cluster rect,#{{ID}} [data-look=\"neo\"].node polygon{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}",
    "#{{ID}} [data-look=\"neo\"].node path{stroke:#9370DB;stroke-width:1px;}",
    "#{{ID}} [data-look=\"neo\"].node .outer-path{filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}",
    "#{{ID}} [data-look=\"neo\"].node .neo-line path{stroke:#9370DB;filter:none;}",
    "#{{ID}} [data-look=\"neo\"].node circle{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}",
    "#{{ID}} [data-look=\"neo\"].node circle .state-start{fill:#000000;}",
    "#{{ID}} [data-look=\"neo\"].icon-shape .icon{fill:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}",
    "#{{ID}} [data-look=\"neo\"].icon-shape .icon-neo path{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}",
    "#{{ID}} :root{--mermaid-font-family:\"trebuchet ms\",verdana,arial,sans-serif;}",
);

/// Produce the byte-exact packet SVG.
pub fn render(
    d: &PacketDiagram,
    l: &PacketLayout,
    _theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let vb = ViewBox {
        min_x: 0.0,
        min_y: 0.0,
        width: l.width,
        height: l.height,
    };
    let mut out = String::with_capacity(
        PACKET_CSS_TEMPLATE.len() + 512 + 200 * l.words.iter().map(|w| w.blocks.len()).sum::<usize>(),
    );

    out.push_str(&open_svg(id, "packet", vb));
    out.push_str("<style>");
    out.push_str(&PACKET_CSS_TEMPLATE.replace("{{ID}}", id));
    out.push_str("</style>");

    // Initial empty group — upstream's `appendDivSvgG` always seeds the
    // SVG with one, before any diagram renders into it.
    out.push_str("<g></g>");

    // Per-word groups.
    for word in &l.words {
        out.push_str("<g>");
        render_word(&mut out, word, d.config.show_bits);
        out.push_str("</g>");
    }

    // Title (emitted even when empty — the reference SVGs keep the
    // placeholder `<text ... class="packetTitle"></text>`).
    if let Some(title) = &l.title {
        out.push_str(&format!(
            r#"<text x="{x}" y="{y}" dominant-baseline="middle" text-anchor="middle" class="packetTitle">{text}</text>"#,
            x = fmt_num(title.x),
            y = fmt_num(title.y),
            text = escape_text(&title.text),
        ));
    }

    out.push_str(close_svg());
    Ok(out)
}

/// Render the geometry for a single word (row).
fn render_word(out: &mut String, word: &PacketWord, show_bits: bool) {
    for block in &word.blocks {
        render_block_rect(out, block);
        render_block_label(out, block);
        if show_bits {
            render_byte_numbers(out, block);
        }
    }
}

fn render_block_rect(out: &mut String, b: &PacketBlock) {
    out.push_str(&format!(
        r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" class="packetBlock"></rect>"#,
        x = fmt_num(b.x),
        y = fmt_num(b.y),
        w = fmt_num(b.width),
        h = fmt_num(b.height),
    ));
}

fn render_block_label(out: &mut String, b: &PacketBlock) {
    out.push_str(&format!(
        r#"<text x="{x}" y="{y}" class="packetLabel" dominant-baseline="middle" text-anchor="middle">{text}</text>"#,
        x = fmt_num(b.label_x),
        y = fmt_num(b.label_y),
        text = escape_text(&b.label),
    ));
}

fn render_byte_numbers(out: &mut String, b: &PacketBlock) {
    // Mirrors upstream's two branches: single-bit block renders one
    // centred number, otherwise render start (left-anchored) and end
    // (right-anchored).
    if b.is_single_block {
        let centre_x = b.x + b.width / 2.0;
        out.push_str(&format!(
            r#"<text x="{x}" y="{y}" class="packetByte start" dominant-baseline="auto" text-anchor="middle">{text}</text>"#,
            x = fmt_num(centre_x),
            y = fmt_num(b.bit_number_y),
            text = b.start,
        ));
        return;
    }
    out.push_str(&format!(
        r#"<text x="{x}" y="{y}" class="packetByte start" dominant-baseline="auto" text-anchor="start">{text}</text>"#,
        x = fmt_num(b.x),
        y = fmt_num(b.bit_number_y),
        text = b.start,
    ));
    out.push_str(&format!(
        r#"<text x="{x}" y="{y}" class="packetByte end" dominant-baseline="auto" text-anchor="end">{text}</text>"#,
        x = fmt_num(b.x + b.width),
        y = fmt_num(b.bit_number_y),
        text = b.end,
    ));
}

/// Format a number the way upstream's SVG attributes end up serialised:
/// integer-valued floats drop the `.0`, otherwise use the default float
/// formatting. Matches the `svg::fmt` helper — duplicated here to keep
/// the module standalone.
fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Minimal XML text escaper — only the five characters that would
/// break surrounding SVG markup. The upstream renderer sets the label
/// via D3's `.text(...)` which performs identical escaping.
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::packet::layout as packet_layout;
    use crate::parser::packet::parse as packet_parse;
    use crate::theme::get_theme;
    use std::fs;

    fn run_fixture(n: &str) {
        let src = fs::read_to_string(format!(
            "tests/ext_fixtures/cypress/packet/{n}.mmd"
        ))
        .unwrap();
        let expected = fs::read_to_string(format!(
            "tests/reference/ext_fixtures/cypress/packet/{n}.svg"
        ))
        .unwrap();
        let theme = get_theme("default");
        let d = packet_parse(&src).unwrap();
        let l = packet_layout(&d, &theme).unwrap();
        let id = format!("ref-ext-fixtures-cypress-packet-{n}");
        let got = render(&d, &l, &theme, &id).unwrap();
        let expected = expected.trim_end_matches('\n');
        if got != expected {
            // Print a short context to help debug byte mismatches.
            for (i, (a, b)) in got.chars().zip(expected.chars()).enumerate() {
                if a != b {
                    let lo = i.saturating_sub(40);
                    eprintln!(
                        "diff at byte {i}:\n got: ...{}\n exp: ...{}",
                        &got[lo..(i + 40).min(got.len())],
                        &expected[lo..(i + 40).min(expected.len())],
                    );
                    break;
                }
            }
            if got.len() != expected.len() {
                eprintln!("length: got={} expected={}", got.len(), expected.len());
            }
        }
        assert_eq!(got, expected, "fixture {n} mismatched");
    }

    #[test]
    fn packet_fixture_01_byte_exact() {
        run_fixture("01");
    }

    #[test]
    fn packet_fixture_02_byte_exact() {
        run_fixture("02");
    }

    #[test]
    fn packet_fixture_03_byte_exact() {
        run_fixture("03");
    }

    #[test]
    fn packet_fixture_04_byte_exact() {
        run_fixture("04");
    }

    #[test]
    fn packet_fixture_05_byte_exact() {
        run_fixture("05");
    }
}
