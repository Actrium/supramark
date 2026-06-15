/// Participant kind
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum ParticipantKind {
    #[default]
    Default,
    Actor,
    Boundary,
    Control,
    Entity,
    Database,
    Collections,
    Queue,
}

/// Participant
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Participant {
    pub name: String,
    pub display_name: Option<String>,
    pub kind: ParticipantKind,
    pub color: Option<String>,
    /// Source line number (0-based) for data-source-line attribute
    pub source_line: Option<usize>,
    /// URL from `[[url text]]` link markup in the display name.
    /// Used for Java-compatible `<title>` encoding in lifelines.
    pub link_url: Option<String>,
}

/// Message arrow style
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum SeqArrowStyle {
    #[default]
    Solid, // ->
    Dashed, // -->
    Dotted, // ..>  (rarely used but reserved)
}

/// Message arrow head
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum SeqArrowHead {
    #[default]
    Filled, // > — filled diamond arrowhead (4 points)
    FilledHalfTop,    // \\ on right (or / on left) — filled top-half triangle
    FilledHalfBottom, // / on right (or \\ on left) — filled bottom-half triangle
    Open,             // >> or << — open V-shaped head (2 lines)
    HalfTop,          // // — open half-arrow, upper line only
    HalfBottom,       // \\ — open half-arrow, lower line only
}

/// Message direction
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum SeqDirection {
    LeftToRight, // ->
    RightToLeft, // <-
}

/// A single message
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Message {
    pub from: String,
    pub to: String,
    pub text: String,
    pub arrow_style: SeqArrowStyle,
    pub arrow_head: SeqArrowHead,
    pub direction: SeqDirection,
    /// Optional arrow color, e.g. `[#blue]->` stores `"blue"`
    pub color: Option<String>,
    /// Source line number (0-based) for data-source-line attribute
    pub source_line: Option<usize>,
    /// Circle decoration on the "from" end of the arrow (o->)
    pub circle_from: bool,
    /// Circle decoration on the "to" end of the arrow (->o)
    pub circle_to: bool,
    /// Cross (X) decoration on the "from" end of the arrow (x->)
    pub cross_from: bool,
    /// Cross (X) decoration on the "to" end of the arrow (->x)
    pub cross_to: bool,
    /// Teoz parallel message (& prefix): shares y position with previous tile
    pub parallel: bool,
    /// Java isReverseDefine: controls self-message constraint direction
    pub is_reverse_define: bool,
    /// Hidden arrow: `-[hidden]->` — occupies vertical space but is not drawn
    pub hidden: bool,
    /// Bidirectional arrow: `<->` — arrowheads at both ends
    pub bidirectional: bool,
    /// Short gate arrow (?->) as opposed to full boundary ([->).
    /// Gate arrows use text-width-based span; boundary arrows extend to diagram edges.
    pub is_short_gate: bool,
}

/// Combined fragment kind
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum FragmentKind {
    Alt,
    Loop,
    Opt,
    Par,
    Break,
    Critical,
    Group,
}

impl FragmentKind {
    /// Return the display label for this fragment kind
    pub fn label(&self) -> &'static str {
        match self {
            FragmentKind::Alt => "alt",
            FragmentKind::Loop => "loop",
            FragmentKind::Opt => "opt",
            FragmentKind::Par => "par",
            FragmentKind::Break => "break",
            FragmentKind::Critical => "critical",
            FragmentKind::Group => "group",
        }
    }
}

/// Events in a sequence diagram (in chronological order)
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum SeqEvent {
    Message(Message),
    Activate(String, Option<String>),
    Deactivate(String),
    Destroy(String),
    NoteRight {
        participant: String,
        text: String,
        /// True when the note uses `& note` parallel syntax
        parallel: bool,
        /// Optional background color (e.g. "#FF0000" from `note right #red`)
        color: Option<String>,
    },
    NoteLeft {
        participant: String,
        text: String,
        /// True when the note uses `& note` parallel syntax
        parallel: bool,
        /// Optional background color
        color: Option<String>,
    },
    NoteOver {
        participants: Vec<String>,
        text: String,
        /// True when the note uses `& note` parallel syntax
        parallel: bool,
    },
    /// Legacy group start (kept for backward compatibility, maps to Fragment)
    GroupStart {
        label: Option<String>,
    },
    /// Legacy group end
    GroupEnd,
    /// Combined fragment start (alt, loop, opt, par, break, critical, group)
    FragmentStart {
        kind: FragmentKind,
        label: String,
        /// Teoz parallel fragment (& prefix): shares y position with previous tile
        parallel: bool,
        /// Background color from `#color` prefix (e.g., `group #ffa Label`)
        color: Option<String>,
    },
    /// Fragment separator (else within alt/par)
    FragmentSeparator {
        label: String,
    },
    /// Fragment end
    FragmentEnd,
    /// Reference over participants
    Ref {
        participants: Vec<String>,
        label: String,
    },
    Divider {
        text: Option<String>,
    },
    Delay {
        text: Option<String>,
    },
    /// Explicit spacing: ||| or || N ||
    Spacing {
        pixels: u32,
    },
    /// Auto-numbering control
    AutoNumber {
        start: Option<u32>,
    },
}

/// Sequence diagram IR
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct SequenceDiagram {
    pub participants: Vec<Participant>,
    pub events: Vec<SeqEvent>,
    /// Whether `!pragma teoz true` was set (parallel message rendering)
    pub teoz_mode: bool,
    /// Whether `hide footbox` was set (hide tail participant boxes)
    pub hide_footbox: bool,
    /// Shadow offset from skin directive (e.g. `skin rose` sets 4.0)
    pub delta_shadow: f64,
    /// Event indices of inline life events (from message `++`/`--` syntax).
    pub inline_life_events: Vec<usize>,
    /// Source text hash seed for deterministic handwritten rendering.
    /// Java: `StringUtils.seed(source.getPlainString("\n"))`
    pub source_seed: i64,
}
