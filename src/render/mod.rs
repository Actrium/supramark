//! SVG rendering — consumes [`crate::layout::DiagramLayout`] and
//! emits an SVG string that is byte-identical to upstream mermaid's
//! output for the same source.

pub mod edges;
pub mod markers;
pub mod shapes;
pub mod svg;
pub mod svg_richtext;
pub mod svg_pie;
pub mod svg_packet;
pub mod svg_radar;
