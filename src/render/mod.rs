//! SVG rendering — consumes [`crate::layout::DiagramLayout`] and
//! emits an SVG string that is byte-identical to upstream mermaid's
//! output for the same source.

pub mod edges;
pub mod foreign_object;
pub mod markers;
pub mod rough;
pub mod shapes;
pub mod stylis;
pub mod svg;
pub mod svg_block;
pub mod svg_c4;
pub mod svg_class;
pub mod svg_er;
pub mod svg_flowchart;
pub mod svg_gantt;
pub mod svg_gitgraph;
pub mod svg_info;
pub mod svg_ishikawa;
pub mod svg_journey;
pub mod svg_kanban;
pub mod svg_mindmap;
pub mod svg_packet;
pub mod svg_pie;
pub mod svg_quadrant;
pub mod svg_radar;
pub mod svg_requirement;
pub mod svg_richtext;
pub mod svg_sankey;
pub mod svg_state;
pub mod svg_timeline;
pub mod svg_treemap;
pub mod svg_venn;
pub mod svg_wardley;
pub mod svg_xychart;
pub mod unified_shell;
