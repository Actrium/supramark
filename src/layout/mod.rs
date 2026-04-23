//! Per-diagram layout — one module per diagram type + shared
//! plumbing. Consumes a [`crate::model::Diagram`], produces a
//! [`DiagramLayout`] the renderer pattern-matches on.

pub mod dagre_bridge;
pub mod gantt;
pub mod intersect;
pub mod packet;
pub mod pie;
pub mod radar;
pub mod routing;
pub mod unified;

/// Dispatch enum — parallel to `model::Diagram`. Each variant holds
/// the post-layout geometry for one diagram kind.
#[derive(Debug, Clone)]
pub enum DiagramLayout {
    Pie(pie::PieLayout),
    Packet(packet::PacketLayout),
    Radar(radar::RadarLayout),
    Ishikawa(ishikawa::IshikawaLayout),
    Journey(journey::JourneyLayout),
    Timeline(timeline::TimelineLayout),
    Quadrant(quadrant::QuadrantLayout),
    Xychart(xychart::XychartLayout),
    Wardley(wardley::WardleyLayout),
    Gantt(gantt::GanttLayout),
    Sankey(sankey::SankeyLayout),
    Treemap(treemap::TreemapLayout),
    Kanban(kanban::KanbanLayout),
    Er(()),
    Requirement(()),
    Class(()),
    State(()),
    Flowchart(()),
    Block(()),
    Mindmap(()),
    Sequence(()),
    C4(()),
    GitGraph(()),
    Architecture(()),
    Venn(()),
}
pub mod block;
pub mod class;
pub mod er;
pub mod flowchart;
pub mod ishikawa;
pub mod journey;
pub mod kanban;
pub mod quadrant;
pub mod requirement;
pub mod sankey;
pub mod state;
pub mod timeline;
pub mod treemap;
pub mod wardley;
pub mod xychart;
