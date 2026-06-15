/// Pseudo-state kind for special state nodes.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum StateKind {
    /// Regular state (default).
    #[default]
    Normal,
    /// Fork bar — synchronization split.
    Fork,
    /// Join bar — synchronization merge.
    Join,
    /// Choice pseudo-state (diamond).
    Choice,
    /// End pseudo-state (`<<end>>` stereotype).
    End,
    /// Shallow history (H).
    History,
    /// Deep history (H*).
    DeepHistory,
    /// Entry point pseudo-state.
    EntryPoint,
    /// Exit point pseudo-state.
    ExitPoint,
}

/// State in a state diagram
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct State {
    /// State name (display name)
    pub name: String,
    /// State ID (used for references)
    pub id: String,
    /// Description lines
    pub description: Vec<String>,
    /// Stereotype (e.g. `<<inputPin>>`)
    pub stereotype: Option<String>,
    /// Child states (composite state)
    pub children: Vec<State>,
    /// Whether this is a special state `[*]`
    pub is_special: bool,
    /// Pseudo-state kind (fork, join, choice, history, etc.)
    pub kind: StateKind,
    /// Concurrent regions within a composite state.
    /// Each region is a list of child states.
    /// If non-empty, `children` holds the first region and `regions` holds additional regions.
    pub regions: Vec<Vec<State>>,
    /// Source line number (0-based) where this state was first defined/referenced.
    pub source_line: Option<usize>,
    /// Physical source line (0-based) of the first explicit declaration/description
    /// that defines this state during Java's parser pass 1.
    pub explicit_source_line: Option<usize>,
}

/// State transition
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Transition {
    /// Source state ID
    pub from: String,
    /// Target state ID
    pub to: String,
    /// Transition label
    pub label: String,
    /// Arrow style: `->` (solid) or `-->` (dashed) -- both rendered as solid in state diagrams
    pub dashed: bool,
    /// Arrow length = number of dashes in the arrow symbol.
    /// `->` = 1, `-->` = 2, `--->` = 3, etc.
    /// Java uses this as graphviz `minlen = length - 1`.
    pub length: usize,
    /// Source line number (0-based) where this transition was defined.
    pub source_line: Option<usize>,
}

/// Note
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct StateNote {
    /// Note alias
    pub alias: Option<String>,
    /// Internal entity id/qualified name used for rendered note entities.
    pub entity_id: Option<String>,
    /// Note text
    pub text: String,
    /// Relative position: left/right/top/bottom for notes attached to a state.
    pub position: String,
    /// Target state id for attached notes.
    pub target: Option<String>,
    /// Source line number (1-based) for data-source-line attribute.
    pub source_line: Option<usize>,
}

/// State diagram IR
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct StateDiagram {
    /// All top-level states
    pub states: Vec<State>,
    /// All transitions
    pub transitions: Vec<Transition>,
    /// Notes
    pub notes: Vec<StateNote>,
    /// Layout direction
    pub direction: super::diagram::Direction,
}
