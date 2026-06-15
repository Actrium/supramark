// Packet diagram model — visualizes network packet structure (bit fields).
//
// Input format:
//   `0-15: Source Port`
//   `16-31: Destination Port`
//   `32-63: Sequence Number`
//
// Each field specifies a bit range and a label.

/// A single field in the packet diagram.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct PacketField {
    /// Start bit (inclusive).
    pub start: u32,
    /// End bit (inclusive).
    pub end: u32,
    /// Label for this field.
    pub label: String,
}

/// The packet diagram model.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct PacketDiagram {
    /// Ordered list of fields.
    pub fields: Vec<PacketField>,
    /// Bits per row (default 32 for standard network packet headers).
    pub bits_per_row: u32,
}
