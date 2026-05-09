//! Packet layout — bit-field grid geometry.
//!
//! Port of the wrapping + positioning code that lives in upstream's
//! `diagrams/packet/parser.ts::populate` (wrap fields into rows of
//! `bitsPerRow` bits) and `diagrams/packet/renderer.ts::drawWord`
//! (per-block `x` / `width`). Kept numerically identical: every
//! coefficient (`+1` padding, `-paddingX`, `wordY - 2`, …) comes
//! straight from upstream.

use crate::error::Result;
use crate::model::packet::{PacketConfig, PacketDiagram};
use crate::theme::ThemeVariables;

/// Final geometry for a packet diagram: outer SVG dimensions, title
/// placement, plus a list of rendered words (rows) each carrying one
/// or more laid-out blocks.
#[derive(Debug, Clone, Default)]
pub struct PacketLayout {
    pub width: f64,
    pub height: f64,
    pub total_row_height: f64,
    pub title: Option<PacketTitle>,
    pub words: Vec<PacketWord>,
}

#[derive(Debug, Clone)]
pub struct PacketTitle {
    pub text: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct PacketWord {
    pub blocks: Vec<PacketBlock>,
}

#[derive(Debug, Clone)]
pub struct PacketBlock {
    pub start: u32,
    pub end: u32,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Pre-computed label anchor point (block centre).
    pub label_x: f64,
    pub label_y: f64,
    /// Pre-computed byte-number positions (only consumed when
    /// `config.show_bits` is true).
    pub bit_number_y: f64,
    /// Whether start==end — collapses both byte numbers into a single
    /// centred label per upstream's `isSingleBlock` branch.
    pub is_single_block: bool,
}

/// Compute the packet diagram layout.
pub fn layout(d: &PacketDiagram, _theme: &ThemeVariables) -> Result<PacketLayout> {
    let cfg = d.config;
    // Upstream pushes `paddingY += 10` inside `PacketDB.getConfig` when
    // `showBits` is on so the bit numbers have vertical breathing room.
    let effective_padding_y = if cfg.show_bits {
        cfg.padding_y + 10.0
    } else {
        cfg.padding_y
    };
    let total_row_height = cfg.row_height + effective_padding_y;

    // Wrap fields into words according to bitsPerRow.
    let words = wrap_fields_into_words(d, &cfg, total_row_height, effective_padding_y);

    // Overall SVG dimensions mirror `renderer.ts`:
    //   svgHeight = totalRowHeight * (words.length + 1) - (title ? 0 : rowHeight)
    //   svgWidth  = bitWidth * bitsPerRow + 2
    let title_text = d.meta.title.as_deref().unwrap_or("").to_owned();
    let svg_width = cfg.bit_width * f64::from(cfg.bits_per_row) + 2.0;
    let title_allowance = if title_text.is_empty() {
        cfg.row_height
    } else {
        0.0
    };
    let svg_height = total_row_height * (words.len() as f64 + 1.0) - title_allowance;

    // Even an empty title is emitted in the reference SVGs (`<text ...
    // class="packetTitle"></text>`), so we always materialise a
    // PacketTitle node — the renderer treats `text=""` as a valid
    // empty element.
    let title = Some(PacketTitle {
        text: title_text,
        x: svg_width / 2.0,
        y: svg_height - total_row_height / 2.0,
    });

    Ok(PacketLayout {
        width: svg_width,
        height: svg_height,
        total_row_height,
        title,
        words,
    })
}

/// Wrap the contiguous list of fields into words of at most
/// `bitsPerRow` bits. A field that straddles a row boundary is split
/// into `[head_in_row_N, tail_in_row_N+1]`, matching upstream's
/// `getNextFittingBlock` routine.
fn wrap_fields_into_words(
    d: &PacketDiagram,
    cfg: &PacketConfig,
    total_row_height: f64,
    effective_padding_y: f64,
) -> Vec<PacketWord> {
    let bpr = cfg.bits_per_row;
    if bpr == 0 || d.fields.is_empty() {
        return Vec::new();
    }

    // Upstream numbers rows starting at 1. We keep the same semantics
    // because `getNextFittingBlock` compares against `row * bitsPerRow`.
    let mut row: u32 = 1;
    let mut current_word: Vec<(u32, u32, String)> = Vec::new();
    let mut words_raw: Vec<Vec<(u32, u32, String)>> = Vec::new();

    for field in &d.fields {
        let mut start = field.start;
        let end = field.end;
        let label = field.label.clone();

        // Upstream has a defensive `while (word.length <= bitsPerRow + 1)`
        // guard; we rely on the row increment inside the loop instead.
        loop {
            let row_last_bit = row * bpr - 1;
            if end <= row_last_bit {
                current_word.push((start, end, label.clone()));
                if end == row_last_bit {
                    // Word is full — close it and start a new one.
                    words_raw.push(std::mem::take(&mut current_word));
                    row += 1;
                }
                break;
            }
            // The block straddles the boundary — emit the head that
            // fits in this row and continue with the tail.
            let head_end = row_last_bit;
            current_word.push((start, head_end, label.clone()));
            words_raw.push(std::mem::take(&mut current_word));
            row += 1;
            start = row_last_bit + 1;
            // Loop with the remaining range [start..=end].
            if start > end {
                break;
            }
        }
    }
    if !current_word.is_empty() {
        words_raw.push(current_word);
    }

    // Turn the wrapped bit-ranges into drawable blocks.
    words_raw
        .into_iter()
        .enumerate()
        .map(|(row_idx, blocks)| {
            let word_y = row_idx as f64 * total_row_height + effective_padding_y;
            let laid = blocks
                .into_iter()
                .map(|(start, end, label)| {
                    let block_x = f64::from(start % bpr) * cfg.bit_width + 1.0;
                    let width = f64::from(end - start + 1) * cfg.bit_width - cfg.padding_x;
                    let is_single_block = start == end;
                    PacketBlock {
                        start,
                        end,
                        label,
                        x: block_x,
                        y: word_y,
                        width,
                        height: cfg.row_height,
                        label_x: block_x + width / 2.0,
                        label_y: word_y + cfg.row_height / 2.0,
                        bit_number_y: word_y - 2.0,
                        is_single_block,
                    }
                })
                .collect();
            PacketWord { blocks: laid }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::packet::PacketField;
    use crate::theme::get_theme;

    fn make(fields: Vec<(u32, u32, &str)>, title: Option<&str>) -> PacketDiagram {
        let mut d = PacketDiagram::default();
        d.meta.title = title.map(String::from);
        d.fields = fields
            .into_iter()
            .map(|(s, e, l)| PacketField {
                start: s,
                end: e,
                label: l.into(),
            })
            .collect();
        d
    }

    #[test]
    fn single_row_with_title_matches_fixture_01() {
        let d = make(vec![(0, 10, "hello")], Some("Hello world"));
        let l = layout(&d, &get_theme("default")).unwrap();
        assert_eq!(l.width, 1026.0);
        assert_eq!(l.height, 94.0);
        assert_eq!(l.words.len(), 1);
        let b = &l.words[0].blocks[0];
        assert_eq!(b.x, 1.0);
        assert_eq!(b.width, 347.0);
        assert_eq!(b.label_x, 174.5);
        assert_eq!(b.label_y, 31.0);
        assert_eq!(b.bit_number_y, 13.0);
    }

    #[test]
    fn two_single_bits_without_title_matches_fixture_03() {
        let d = make(vec![(0, 0, "h"), (1, 1, "i")], None);
        let l = layout(&d, &get_theme("default")).unwrap();
        assert_eq!(l.height, 62.0);
        assert_eq!(l.words.len(), 1);
        assert_eq!(l.words[0].blocks.len(), 2);
        assert!(l.words[0].blocks[0].is_single_block);
    }

    #[test]
    fn tcp_header_wraps_into_seven_words() {
        let d = make(
            vec![
                (0, 15, "Source Port"),
                (16, 31, "Destination Port"),
                (32, 63, "Sequence Number"),
                (64, 95, "Acknowledgment Number"),
                (96, 99, "Data Offset"),
                (100, 105, "Reserved"),
                (106, 106, "URG"),
                (107, 107, "ACK"),
                (108, 108, "PSH"),
                (109, 109, "RST"),
                (110, 110, "SYN"),
                (111, 111, "FIN"),
                (112, 127, "Window"),
                (128, 143, "Checksum"),
                (144, 159, "Urgent Pointer"),
                (160, 191, "(Options and Padding)"),
                (192, 223, "data"),
            ],
            None,
        );
        let l = layout(&d, &get_theme("default")).unwrap();
        assert_eq!(l.words.len(), 7);
        assert_eq!(l.height, 344.0);
    }
}
