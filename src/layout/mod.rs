//! Per-diagram layout — one module per diagram type + shared
//! plumbing. Consumes a [`crate::model::Diagram`], produces a
//! [`DiagramLayout`] the renderer pattern-matches on.

pub mod dagre_bridge;
pub mod intersect;
pub mod routing;
pub mod unified;

/// Dispatch enum — parallel to `model::Diagram`. Each variant holds
/// the post-layout geometry for one diagram kind.
#[derive(Debug, Clone)]
pub enum DiagramLayout {
    Pie(()), Packet(()), Radar(()), Ishikawa(()), Journey(()),
    Timeline(()), Quadrant(()), Xychart(()), Wardley(()), Gantt(()),
    Sankey(()), Treemap(()), Kanban(()), Er(()), Requirement(()),
    Class(()), State(()), Flowchart(()), Block(()), Mindmap(()),
    Sequence(()), C4(()), GitGraph(()), Architecture(()), Venn(()),
}
