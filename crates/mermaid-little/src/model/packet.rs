//! Packet diagram parsed model.
//!
//! Upstream: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/packet/
//! Grammar (langium): /ext/mermaid-official-stable-v11.14.0/packages/parser/src/language/packet/packet.langium

use crate::model::DiagramMeta;

/// Top-level packet diagram model — metadata + an ordered list of bit
/// fields + the layout-affecting config block extracted from the
/// frontmatter (or defaults when unset).
#[derive(Debug, Clone, Default)]
pub struct PacketDiagram {
    pub meta: DiagramMeta,
    pub fields: Vec<PacketField>,
    pub config: PacketConfig,
}

/// A single contiguous bit-range entry as written in the source text
/// (`start-end: "label"` or `start: "label"` or `+bits: "label"`).
#[derive(Debug, Clone)]
pub struct PacketField {
    pub start: u32,
    pub end: u32,
    pub label: String,
}

/// User-visible config knobs for the packet renderer. Defaults match
/// upstream `PacketDiagramConfig` (see config.schema.yaml §PacketDiagramConfig).
#[derive(Debug, Clone, Copy)]
pub struct PacketConfig {
    pub row_height: f64,
    pub bit_width: f64,
    pub bits_per_row: u32,
    pub show_bits: bool,
    pub padding_x: f64,
    pub padding_y: f64,
}

impl Default for PacketConfig {
    fn default() -> Self {
        Self {
            row_height: 32.0,
            bit_width: 32.0,
            bits_per_row: 32,
            show_bits: true,
            padding_x: 5.0,
            padding_y: 5.0,
        }
    }
}
