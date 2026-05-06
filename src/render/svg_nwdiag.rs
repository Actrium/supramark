use std::fmt::Write;

use crate::klimt::svg::fmt_coord;
use crate::layout::nwdiag::NwdiagLayout;
use crate::model::nwdiag::NwdiagDiagram;
use crate::render::svg::{ensure_visible_int, write_svg_root_bg};
use crate::style::SkinParams;
use crate::Result;

/// Default fill for network tubes (Java style SName.network BackGroundColor).
const TUBE_FILL: &str = "#E2E2F0";
/// Default stroke for tubes and links (Java style SName.network/arrow LineColor).
const LINE_COLOR: &str = "#181818";
/// Server box fill (Java style SName.server BackGroundColor).
const BOX_FILL: &str = "#F1F1F1";
/// Server box stroke.
const BOX_STROKE: &str = "#181818";
/// Text color.
const TEXT_COLOR: &str = "#000000";

pub fn render_nwdiag(
    _diagram: &NwdiagDiagram,
    layout: &NwdiagLayout,
    skin: &SkinParams,
    body_offset: Option<(f64, f64)>,
) -> Result<String> {
    let mut buf = String::with_capacity(4096);

    let bg = skin.get_or("backgroundcolor", "#FFFFFF");
    let svg_w = ensure_visible_int(layout.width) as f64;
    let svg_h = ensure_visible_int(layout.height) as f64;
    // When body_offset is provided, all coordinates are pre-shifted in f64 so
    // that wrap_with_meta does not round-trip them through the SVG text
    // (format → parse → add → format introduces a ±0.0001 last-digit drift).
    let (bo_x, bo_y) = body_offset.unwrap_or((0.0, 0.0));
    write_svg_root_bg(&mut buf, svg_w, svg_h, "NWDIAG", bg);

    // The <title> element and visible title are handled by wrap_with_meta
    // (from DiagramMeta.title), so we do NOT emit them here.

    buf.push_str("<defs/><g>");

    // --- Network labels ---
    for nl in &layout.net_labels {
        // Name text (font-size=12, left-aligned).
        write!(
            buf,
            r#"<text fill="{}" font-family="sans-serif" font-size="12" lengthAdjust="spacing" textLength="{}" x="{}" y="{}">{}</text>"#,
            TEXT_COLOR,
            fmt_coord(crate::font_metrics::text_width(&nl.name, "SansSerif", 12.0, false, false)),
            fmt_coord(nl.x + bo_x),
            fmt_coord(nl.y + bo_y),
            crate::klimt::svg::xml_escape(&nl.name),
        )
        .unwrap();

        // Address text (font-size=12, left-aligned).
        if let Some(addr) = &nl.address {
            let addr_w = crate::font_metrics::text_width(addr, "SansSerif", 12.0, false, false);
            let _addr_x = nl.x
                + crate::font_metrics::text_width(&nl.name, "SansSerif", 12.0, false, false)
                - addr_w;
            // Actually, the address x is right-aligned to the same right edge as the name.
            // In Java, the entire text block (name+address) is right-aligned.
            // The right edge of the block is at nl.x + name_w.
            // So addr_x = right_edge - addr_w... but wait, the reference shows addr at x=5.
            // Let me re-check.
            //
            // In the reference: "dmz" at x=48.3242, "10.0.0.0/24" at x=5.
            // The label block width = max(25.6055, 68.9297) = 68.9297.
            // deltaX = 68.9297. Labels drawn at (deltaX - dim.getWidth(), y).
            // "dmz" block: dim.getWidth() = 68.9297 (width of entire block, which is max of lines).
            // Wait, no. In Java, the TextBlock containing "dmz\n10.0.0.0/24" has width = max line width = 68.9297.
            // Drawing right-aligned: x = deltaX - block_width = 68.9297 - 68.9297 = 0.
            // But with margin translate: +5 → x_origin = 5.
            //
            // The text within the block: "dmz" is right-aligned within the block.
            // Java's HorizontalAlignment.RIGHT means each line is right-aligned within the block width.
            // So "dmz" (width 25.6055) is drawn at x_offset = 68.9297 - 25.6055 = 43.3242.
            // Absolute: 5 + 43.3242 = 48.3242. ✓ Matches reference.
            // "10.0.0.0/24" (width 68.9297) is drawn at x_offset = 68.9297 - 68.9297 = 0.
            // Absolute: 5 + 0 = 5. ✓ Matches reference.

            // I need to recalculate. The label x I stored is for the name.
            // Let me just compute fresh here.
            // Actually, my layout stores nl.x as the name text x position.
            // For the address, I need to compute it from the block layout.
            // The block left edge = margin + deltaX - block_width = margin + deltaX - max(name_w, addr_w).
            // For right-aligned text within the block:
            //   name_x = block_left + (block_width - name_w)
            //   addr_x = block_left + (block_width - addr_w)

            // Hmm, this means I need to restructure the layout output.
            // For now, let me compute addr_x directly.
            let name_w = crate::font_metrics::text_width(&nl.name, "SansSerif", 12.0, false, false);
            let _block_w = name_w.max(addr_w);
            // nl.x was computed as mx + delta_x - name_w in layout.
            // block_left = mx + delta_x - block_w.
            // addr_x = block_left + (block_w - addr_w) = mx + delta_x - addr_w.
            // Which is: nl.x + name_w - addr_w... no.
            // nl.x = mx + delta_x - name_w. So mx + delta_x = nl.x + name_w.
            let right_edge = nl.x + name_w;
            let actual_addr_x = right_edge - addr_w;

            write!(
                buf,
                r#"<text fill="{}" font-family="sans-serif" font-size="12" lengthAdjust="spacing" textLength="{}" x="{}" y="{}">{}</text>"#,
                TEXT_COLOR,
                fmt_coord(addr_w),
                fmt_coord(actual_addr_x + bo_x),
                fmt_coord(nl.addr_y + bo_y),
                crate::klimt::svg::xml_escape(addr),
            )
            .unwrap();
        }
    }

    // --- Network tubes ---
    for tube in &layout.tubes {
        let fill = tube.color.as_deref().unwrap_or(TUBE_FILL);
        write!(
            buf,
            r#"<rect fill="{}" height="{}" style="stroke:{};stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
            fill,
            tube.height as i32,
            LINE_COLOR,
            fmt_coord(tube.width),
            fmt_coord(tube.x + bo_x),
            fmt_coord(tube.y + bo_y),
        )
        .unwrap();
    }

    // --- Per-server links and address labels (interleaved, matching Java order) ---
    for group in &layout.server_link_groups {
        for item in &group.links_and_labels {
            match item {
                crate::layout::nwdiag::LinkOrLabel::Link(link) => {
                    write!(
                        buf,
                        r#"<path d="M{},{} L{},{}" fill="none" style="stroke:{};stroke-width:1;"/>"#,
                        fmt_coord(link.x + bo_x),
                        fmt_coord(link.y1 + bo_y),
                        fmt_coord(link.x + bo_x),
                        fmt_coord(link.y2 + bo_y),
                        LINE_COLOR,
                    )
                    .unwrap();
                }
                crate::layout::nwdiag::LinkOrLabel::Label(al) => {
                    let text_w =
                        crate::font_metrics::text_width(&al.text, "SansSerif", 11.0, false, false);
                    write!(
                        buf,
                        r#"<text fill="{}" font-family="sans-serif" font-size="11" lengthAdjust="spacing" textLength="{}" x="{}" y="{}">{}</text>"#,
                        TEXT_COLOR,
                        fmt_coord(text_w),
                        fmt_coord(al.x + bo_x),
                        fmt_coord(al.y + bo_y),
                        crate::klimt::svg::xml_escape(&al.text),
                    )
                    .unwrap();
                }
            }
        }
    }

    // --- Server boxes ---
    for sb in &layout.server_boxes {
        write!(
            buf,
            r#"<rect fill="{}" height="{}" style="stroke:{};stroke-width:0.5;" width="{}" x="{}" y="{}"/>"#,
            BOX_FILL,
            fmt_coord(sb.rect_h),
            BOX_STROKE,
            fmt_coord(sb.rect_w),
            fmt_coord(sb.rect_x + bo_x),
            fmt_coord(sb.rect_y + bo_y),
        )
        .unwrap();

        write!(
            buf,
            r#"<text fill="{}" font-family="sans-serif" font-size="12" lengthAdjust="spacing" textLength="{}" x="{}" y="{}">{}</text>"#,
            TEXT_COLOR,
            fmt_coord(crate::font_metrics::text_width(&sb.label, "SansSerif", 12.0, false, false)),
            fmt_coord(sb.text_x + bo_x),
            fmt_coord(sb.text_y + bo_y),
            crate::klimt::svg::xml_escape(&sb.label),
        )
        .unwrap();
    }

    buf.push_str("</g></svg>");
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::nwdiag::layout_nwdiag;
    use crate::model::nwdiag::{Network, NwdiagDiagram, ServerRef};

    fn basic_diagram() -> NwdiagDiagram {
        NwdiagDiagram {
            title: Some("Infrastructure".to_string()),
            networks: vec![
                Network {
                    name: "dmz".to_string(),
                    address: Some("10.0.0.0/24".to_string()),
                    color: None,
                    servers: vec![
                        ServerRef {
                            name: "web01".to_string(),
                            address: Some("10.0.0.10".to_string()),
                            description: Some("frontend".to_string()),
                        },
                        ServerRef {
                            name: "db01".to_string(),
                            address: None,
                            description: None,
                        },
                    ],
                },
                Network {
                    name: "lan".to_string(),
                    address: None,
                    color: None,
                    servers: vec![
                        ServerRef {
                            name: "web01".to_string(),
                            address: None,
                            description: Some("app".to_string()),
                        },
                        ServerRef {
                            name: "app01".to_string(),
                            address: None,
                            description: None,
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn render_contains_svg_root() {
        let d = basic_diagram();
        let layout = layout_nwdiag(&d).unwrap();
        let svg = render_nwdiag(&d, &layout, &SkinParams::default(), None).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("NWDIAG"));
    }

    #[test]
    fn render_contains_server_labels() {
        let d = basic_diagram();
        let layout = layout_nwdiag(&d).unwrap();
        let svg = render_nwdiag(&d, &layout, &SkinParams::default(), None).unwrap();
        // Server boxes should use resolved descriptions.
        assert!(svg.contains(">app<"));
        assert!(svg.contains(">db01<"));
        assert!(svg.contains(">app01<"));
    }

    #[test]
    fn render_contains_network_tubes() {
        let d = basic_diagram();
        let layout = layout_nwdiag(&d).unwrap();
        let svg = render_nwdiag(&d, &layout, &SkinParams::default(), None).unwrap();
        // Two network tubes.
        assert!(svg.contains(&format!(r#"fill="{}""#, TUBE_FILL)));
    }

    #[test]
    fn render_contains_address_label() {
        let d = basic_diagram();
        let layout = layout_nwdiag(&d).unwrap();
        let svg = render_nwdiag(&d, &layout, &SkinParams::default(), None).unwrap();
        assert!(svg.contains("10.0.0.10"));
    }
}
