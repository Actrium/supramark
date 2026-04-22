//! Per-diagram parsed models. Each diagram has its own submodule
//! holding a plain-data struct — no logic, no rendering.
//!
//! The top-level [`Diagram`] enum is the single dispatch point every
//! downstream stage (layout, render) pattern-matches on. Adding a new
//! diagram type requires a new variant here, which forces exhaustive
//! updates at every match site.

pub mod richtext;

/// Shared metadata every diagram carries — extracted from frontmatter
/// (`---\ntitle: ...\n---`) or directives (`%%{init:...}%%`) by the
/// preprocessor, plus accessibility fields from the diagram body.
#[derive(Debug, Clone, Default)]
pub struct DiagramMeta {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
}

/// One variant per user-facing diagram type in mermaid@11.14.0.
///
/// All variants are currently placeholder unit-likes (wrapping `()`).
/// Wave 1+ replaces each placeholder with a concrete model struct
/// from the corresponding submodule.
#[derive(Debug, Clone)]
pub enum Diagram {
    Pie(()),
    Packet(()),
    Radar(()),
    Ishikawa(()),
    Journey(()),
    Timeline(()),
    Quadrant(()),
    Xychart(()),
    Wardley(()),
    Gantt(()),
    Sankey(()),
    Treemap(()),
    Kanban(()),
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

impl Diagram {
    pub fn kind(&self) -> &'static str {
        match self {
            Diagram::Pie(_) => "pie",
            Diagram::Packet(_) => "packet",
            Diagram::Radar(_) => "radar",
            Diagram::Ishikawa(_) => "ishikawa",
            Diagram::Journey(_) => "journey",
            Diagram::Timeline(_) => "timeline",
            Diagram::Quadrant(_) => "quadrant",
            Diagram::Xychart(_) => "xychart",
            Diagram::Wardley(_) => "wardley",
            Diagram::Gantt(_) => "gantt",
            Diagram::Sankey(_) => "sankey",
            Diagram::Treemap(_) => "treemap",
            Diagram::Kanban(_) => "kanban",
            Diagram::Er(_) => "er",
            Diagram::Requirement(_) => "requirement",
            Diagram::Class(_) => "class",
            Diagram::State(_) => "state",
            Diagram::Flowchart(_) => "flowchart",
            Diagram::Block(_) => "block",
            Diagram::Mindmap(_) => "mindmap",
            Diagram::Sequence(_) => "sequence",
            Diagram::C4(_) => "c4",
            Diagram::GitGraph(_) => "gitGraph",
            Diagram::Architecture(_) => "architecture",
            Diagram::Venn(_) => "venn",
        }
    }
}
