//! Sequence-diagram parsed model.
//!
//! Upstream reference:
//!   `packages/mermaid/src/diagrams/sequence/sequenceDb.ts`
//!   `packages/mermaid/src/diagrams/sequence/parser/sequenceDiagram.jison`
//!
//! Sequence is the largest single-diagram implementation in upstream
//! mermaid (~8 kLOC including parser + db + renderer + svgDraw). This
//! file only defines the parsed model — every variant maps directly to
//! a grammar production. Layout and render walk the same struct.
//!
//! NOTE: this scaffold deliberately covers more than the first-cut
//! parser handles. Unimplemented branches are listed in
//! `tests/known_ignored.txt` until the matching parser/layout/render
//! work lands.

use crate::model::DiagramMeta;

/// Visual archetype for an actor box. Mirrors upstream's
/// `actor.type` field — see `svgDraw.drawActor` switch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum ActorType {
    /// Default rectangle with text. `participant` keyword.
    #[default]
    Participant,
    /// Stick-figure. `actor` keyword OR explicit `@{ "type": "actor" }`.
    Actor,
    /// Bracketed sides (`<<` `>>`) — UML boundary.
    Boundary,
    /// Circle with hat — UML control.
    Control,
    /// Rounded rectangle with bottom line — UML entity.
    Entity,
    /// Stacked-cylinder — UML database.
    Database,
    /// Multi-rect — UML collections.
    Collections,
    /// Open-rect — UML queue.
    Queue,
}

/// One actor / participant column. Position in `actors` is the
/// declaration order — upstream uses the same ordering for the
/// initial X-coordinate sweep.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Actor {
    /// Identifier used in messages (e.g. `Alice` in `Alice->>Bob`).
    pub id: String,
    /// Display label — defaults to `id` if no `as <label>` was given.
    pub description: String,
    /// Visual type — `participant` by default.
    pub actor_type: ActorType,
    /// `box` group this actor belongs to (None = no box).
    pub box_index: Option<usize>,
    /// Was this actor materialised by a `create participant` later in
    /// the source? Affects initial render-vs-create-message ordering.
    pub created: bool,
    /// Was this actor `destroy`ed mid-diagram? Affects lifeline length.
    pub destroyed: bool,
    /// `wrap:` prefix on the description — when true, the renderer pre-
    /// wraps the description text via `wrap_label` before measuring.
    /// Mirrors upstream `addActor`'s `description.wrap` field, fed by
    /// `parseMessage`'s `extractWrap`.
    pub wrap: bool,
    /// Popup-menu entries collected from `link <actor>: <name> @ <url>`
    /// and `links <actor>: {...}` directives. Insertion order is the
    /// source order — upstream `svgDraw.popupMenu` walks `Object.entries`
    /// in the same order.
    pub links: Vec<(String, String)>,
    /// Custom CSS class name from a `properties <actor>: {"class": ...}`
    /// directive. When set, the actor's main rect uses this class (with
    /// the standard `actor-top`/`actor-bottom` suffix appended) and a
    /// custom fill (`#EDF2AE` for known service-actor classes).
    pub class_name: Option<String>,
}

/// Arrow-token classification — matches upstream's `LINETYPE`
/// constants in `messageHelper.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum ArrowType {
    /// `->` solid line, no arrow.
    SolidLine,
    /// `-->` dashed line, no arrow.
    DottedLine,
    /// `->>` solid line, filled arrow.
    SolidArrow,
    /// `-->>` dashed line, filled arrow.
    DottedArrow,
    /// `-x` solid line, cross.
    SolidCross,
    /// `--x` dashed line, cross.
    DottedCross,
    /// `-)` solid line, open arrow (async).
    SolidPoint,
    /// `--)` dashed line, open arrow (async).
    DottedPoint,
    /// Bidirectional variants `<<->>` `<<-->>`.
    BiSolid,
    BiDotted,
    /// Forward half-arrows. Mirrors upstream's `LINETYPE.SOLID_TOP / _BOTTOM`
    /// and `STICK_TOP / _BOTTOM`. Source token examples:
    ///   `-|\` SolidTop      (filled triangle, lower half)
    ///   `-|/` SolidBottom   (filled triangle, upper half)
    ///   `-\\` StickTop      (stick line, lower half)
    ///   `-//` StickBottom   (stick line, upper half)
    /// The `--` dotted variants append `Dotted`.
    SolidTop,
    SolidBottom,
    StickTop,
    StickBottom,
    SolidTopDotted,
    SolidBottomDotted,
    StickTopDotted,
    StickBottomDotted,
    /// Reverse half-arrows — head appears at the source actor instead of
    /// the destination. Mirrors upstream's `LINETYPE.SOLID_ARROW_TOP_REVERSE`
    /// family. Source token examples:
    ///   `/|-` SolidTopReverse,  `\|-` SolidBottomReverse,
    ///   `//-` StickTopReverse,  `\\-` StickBottomReverse.
    /// The `--` dotted variants (`/|--`, `\|--`, `//--`, `\\--`)
    /// append `Dotted`.
    SolidTopReverse,
    SolidBottomReverse,
    StickTopReverse,
    StickBottomReverse,
    SolidTopReverseDotted,
    SolidBottomReverseDotted,
    StickTopReverseDotted,
    StickBottomReverseDotted,
}

/// Central-connection marker style. Mirrors upstream's
/// `LINETYPE.CENTRAL_CONNECTION{,_REVERSE,_DUAL}`. Set by the
/// `()` token next to one or both actors in a message line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum CentralConnection {
    /// `actor signal '()' actor` — circle drawn at destination.
    AtTo,
    /// `actor '()' signal actor` — circle drawn at source.
    AtFrom,
    /// `actor '()' signal '()' actor` — circles at both ends.
    Dual,
}

/// One message between two actors.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Message {
    pub from: String,
    pub to: String,
    pub text: String,
    pub arrow: Option<ArrowType>,
    /// Set when the source line had `+` after the arrow (auto-activate).
    pub activate: bool,
    /// Set when the source line had `-` after the arrow (auto-deactivate).
    pub deactivate: bool,
    /// Set by `wrap:` prefix on the message text.
    pub wrap: bool,
    /// `()` central-connection marker (visualised as small circles
    /// at one or both ends of the message line).
    pub central_connection: Option<CentralConnection>,
}

/// Note placement relative to its anchor actor(s).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum NotePlacement {
    LeftOf,
    RightOf,
    Over,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Note {
    pub placement_actors: Vec<String>,
    pub placement: Option<NotePlacement>,
    pub text: String,
    pub wrap: bool,
}

/// One arm of an `alt`/`else`/`else`/`end` block. The first arm
/// holds the `alt` text; subsequent arms hold the `else <text>` or
/// no-text.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct AltBranch {
    pub label: String,
    pub items: Vec<DiagramItem>,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ParBranch {
    pub label: String,
    pub items: Vec<DiagramItem>,
}

/// Optional grouping container for actors — `box <colour> <label> ... end`.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ActorBox {
    pub fill: Option<String>,
    pub label: String,
    pub actors: Vec<String>,
}

/// One element in the linear stream of "things that happen". The
/// renderer walks this Vec to emit message lines, notes, loops, etc.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum DiagramItem {
    Message(Message),
    Note(Note),
    /// `loop <label> ... end`
    Loop {
        label: String,
        items: Vec<DiagramItem>,
    },
    /// `alt <label> ... else <label2> ... end`
    Alt {
        branches: Vec<AltBranch>,
    },
    /// `opt <label> ... end`
    Opt {
        label: String,
        items: Vec<DiagramItem>,
    },
    /// `par <label> ... and <label2> ... end`
    Par {
        branches: Vec<ParBranch>,
    },
    /// `critical <label> ... option <label2> ... end`
    Critical {
        branches: Vec<AltBranch>,
    },
    /// `break <label> ... end`
    Break {
        label: String,
        items: Vec<DiagramItem>,
    },
    /// `rect rgb(r,g,b) ... end` — coloured background block.
    Rect {
        fill: String,
        items: Vec<DiagramItem>,
    },
    /// `activate <actor>` — explicit lifeline activation.
    Activate(String),
    /// `deactivate <actor>` — explicit lifeline deactivation.
    Deactivate(String),
    /// `create participant <id>`
    Create(Actor),
    /// `destroy <actor>`
    Destroy(String),
    /// `autonumber` / `autonumber <start>` / `autonumber <start> <step>`
    /// / `autonumber off`. Mirrors upstream `LINETYPE.AUTONUMBER` —
    /// each occurrence consumes one item-id slot and (de)activates
    /// numeric prefixes on subsequent message lines.
    Autonumber {
        start: Option<i64>,
        step: Option<i64>,
        visible: bool,
    },
}

/// Per-diagram config consumed from `%%{init}%%` / frontmatter.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct SequenceConfig {
    pub diagram_margin_x: f64,
    pub diagram_margin_y: f64,
    pub actor_margin: f64,
    pub width: f64,
    pub height: f64,
    pub box_margin: f64,
    pub box_text_margin: f64,
    pub note_margin: f64,
    pub message_margin: f64,
    pub message_align: String,
    pub mirror_actors: bool,
    pub force_menus: bool,
    pub bottom_margin_adj: f64,
    pub right_angles: bool,
    pub show_sequence_numbers: bool,
    pub actor_font_size: i64,
    pub actor_font_family: String,
    pub actor_font_weight: i64,
    pub note_font_size: i64,
    pub note_font_family: String,
    pub note_font_weight: i64,
    pub note_align: String,
    pub message_font_size: i64,
    pub message_font_family: String,
    pub message_font_weight: i64,
    pub wrap: bool,
    pub wrap_padding: f64,
    pub label_box_width: f64,
    pub label_box_height: f64,
    pub hide_unused_participants: bool,
}

impl Default for SequenceConfig {
    fn default() -> Self {
        // Defaults match upstream `defaultConfig.ts#sequence`.
        SequenceConfig {
            diagram_margin_x: 50.0,
            diagram_margin_y: 10.0,
            actor_margin: 50.0,
            width: 150.0,
            height: 65.0,
            box_margin: 10.0,
            box_text_margin: 5.0,
            note_margin: 10.0,
            message_margin: 35.0,
            message_align: "center".into(),
            mirror_actors: true,
            force_menus: false,
            bottom_margin_adj: 1.0,
            right_angles: false,
            show_sequence_numbers: false,
            actor_font_size: 14,
            actor_font_family: "\"Open Sans\", sans-serif".into(),
            actor_font_weight: 400,
            note_font_size: 14,
            note_font_family: "\"trebuchet ms\", verdana, arial, sans-serif".into(),
            note_font_weight: 400,
            note_align: "center".into(),
            message_font_size: 16,
            message_font_family: "\"trebuchet ms\", verdana, arial".into(),
            message_font_weight: 400,
            wrap: false,
            wrap_padding: 10.0,
            label_box_width: 50.0,
            label_box_height: 20.0,
            hide_unused_participants: false,
        }
    }
}

/// Parsed sequence diagram — a flat actor list plus the linear stream
/// of items in declaration order.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct SequenceDiagram {
    pub meta: DiagramMeta,
    pub title: Option<String>,
    pub actors: Vec<Actor>,
    pub boxes: Vec<ActorBox>,
    pub items: Vec<DiagramItem>,
    pub config: SequenceConfig,
    /// Optional theme override lifted from frontmatter / init directive.
    pub theme_name: Option<String>,
}
