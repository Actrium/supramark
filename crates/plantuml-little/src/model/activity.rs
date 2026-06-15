/// Activity diagram node kind
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum ActivityNodeKind {
    /// Start node
    Start,
    /// Stop node
    Stop,
    /// End node (end)
    End,
    /// Action `:text;`
    Action,
    /// Conditional branch
    If,
    /// Merge (endif)
    Merge,
    /// Fork branch
    Fork,
    /// Fork merge
    ForkEnd,
    /// Detach separator `====`
    Detach,
}

/// Note position in activity diagram
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum NotePosition {
    Left,
    Right,
}

/// Activity diagram event
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum ActivityEvent {
    /// start
    Start,
    /// stop / end
    Stop,
    /// Action node `:text;`
    Action { text: String },
    /// Conditional branch
    If {
        condition: String,
        then_label: String,
    },
    /// Else-if branch
    ElseIf { condition: String, label: String },
    /// else
    Else { label: String },
    /// endif
    EndIf,
    /// While loop
    While { condition: String, label: String },
    /// endwhile
    EndWhile { label: String },
    /// repeat
    Repeat,
    /// repeat while
    RepeatWhile {
        condition: String,
        is_text: Option<String>,
        not_text: Option<String>,
    },
    /// fork
    Fork,
    /// fork again
    ForkAgain,
    /// end fork
    EndFork,
    /// Swimlane switch
    Swimlane { name: String },
    /// Note
    Note {
        position: NotePosition,
        text: String,
    },
    /// Floating note
    FloatingNote {
        position: NotePosition,
        text: String,
    },
    /// detach
    Detach,
    /// label NAME — marks a goto target (no visual element)
    Label { name: String },
    /// goto NAME — jumps to a label
    Goto { name: String },
    /// backward:text; — action rendered on the repeat loop-back path
    Backward { text: String },
    /// break — exits the enclosing repeat loop
    Break,
    /// Synchronization bar (old-style `===NAME===`)
    SyncBar(String),
    /// Incoming convergence to an existing sync bar (old-style target)
    GotoSyncBar(String),
    /// Resume layout from a sync bar (old-style source in arrow)
    ResumeFromSyncBar(String),
}

/// Old-style activity node kind.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum OldActivityNodeKind {
    Start,
    End,
    Action,
    Branch,
    SyncBar,
}

/// Old-style activity node metadata mirroring Java `activitydiagram.ActivityDiagram`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct OldActivityNode {
    pub id: String,
    pub uid: String,
    pub qualified_name: String,
    pub kind: OldActivityNodeKind,
    pub text: String,
}

/// Old-style activity edge metadata mirroring Java `Link` creation order.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct OldActivityLink {
    pub uid: String,
    pub from_id: String,
    pub to_id: String,
    pub label: Option<String>,
    pub head_label: Option<String>,
    pub source_line: usize,
    pub length: u32,
}

/// Old-style activity graph reconstructed during parsing.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct OldActivityGraph {
    pub nodes: Vec<OldActivityNode>,
    pub links: Vec<OldActivityLink>,
}

/// Activity diagram IR
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ActivityDiagram {
    pub events: Vec<ActivityEvent>,
    pub swimlanes: Vec<String>,
    pub direction: super::diagram::Direction,
    /// Maximum width for note text wrapping (from `<style>` MaximumWidth).
    pub note_max_width: Option<f64>,
    /// True when the diagram uses old-style `(*)` / `===` activity syntax.
    pub is_old_style: bool,
    /// Old-style activity graphs are Graphviz-backed rather than sequential.
    pub old_graph: Option<OldActivityGraph>,
}
