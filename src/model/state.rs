//! State-diagram AST.
//!
//! Upstream reference:
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/state/stateDb.ts`
//! (+ `parser/`, `stateRenderer.js`, `stateRenderer-v3-unified.ts`).
//!
//! Two renderer generations ship upstream:
//! * v1 (`stateDiagram`): dagre-d3 legacy path.
//! * v2 (`stateDiagram-v2`): unified-layout pipeline sharing code with
//!   flowchart/class/ER.
//!
//! Both emit the same SVG family (rounded-rect states, `state-start`
//! initial marker, `state-end` final marker, fork/join bars, choice
//! diamonds, composite-state cluster boxes, notes). The AST therefore
//! represents one unified model; the renderer reads `is_v2` when it
//! matters (mostly: cluster look, spacing defaults).

use super::DiagramMeta;

/// A single item in upstream's flat parse order list, used to reproduce
/// the `graphItemCount` dom_id assignment that upstream's `setupGraph()`
/// applies.  Upstream processes state-declarations *and* relations in a
/// single flat pass, incrementing a shared counter after each item.
#[derive(Debug, Clone)]
pub enum ParseItem {
    /// Explicit `state …` declaration — carries the resolved state id.
    StateDecl(String),
    /// `A --> B` transition — carries the index into `transitions`.
    Relation(usize),
    /// `note X of Y` block or `note X of Y : text` — carries the note index
    /// into `notes` and the target state id. Added to items so the note
    /// counter matches upstream's graphItemCount at the time the STMT_STATE
    /// with the note is processed.
    NoteDecl(usize),
}

/// Top-level state-diagram model.
#[derive(Debug, Clone, Default)]
pub struct StateDiagram {
    pub meta: DiagramMeta,
    /// True when the source begins with `stateDiagram-v2`.
    pub is_v2: bool,
    /// Diagram-level direction — `TB` / `BT` / `LR` / `RL`. `None` means
    /// use the upstream default (`TB`).
    pub direction: Option<String>,
    /// Theme name lifted from `%%{init:{theme:"..."}}%%` or frontmatter.
    pub theme_override: Option<String>,
    /// Look variant lifted from frontmatter `config.look` (e.g. `default`,
    /// `classic`, `neo`). When `None` we fall back to upstream default
    /// (`classic`) at render time.
    pub look_override: Option<String>,
    /// All states (including nested); composite children reference a
    /// parent by id.
    pub states: Vec<State>,
    /// Every transition — whether across the top level or inside a
    /// composite, transitions are flat with string endpoints.
    pub transitions: Vec<Transition>,
    /// Free-standing notes attached to a state. Note geometry is
    /// computed by the layout stage, not stored here.
    pub notes: Vec<Note>,
    /// Parse-order item sequence mirroring upstream's `items` array.
    /// Used to reproduce the `graphItemCount`-based dom_id assignment.
    pub items: Vec<ParseItem>,
    /// `classDef`-style style definitions, keyed by class name. Values
    /// are raw CSS fragments.
    pub class_defs: Vec<ClassDef>,
    /// Per-state class applications (`class X highlight` / `state X:::highlight`).
    pub class_applies: Vec<ClassApply>,
}

/// A single state node. Both simple and composite states use this
/// struct; composite states have `children` populated.
#[derive(Debug, Clone, Default)]
pub struct State {
    /// Canonical identifier (unique within the diagram).
    pub id: String,
    /// Rendered label. Defaults to `id` when no explicit label given.
    pub label: Option<String>,
    /// Optional multi-line description — rendered below the title with
    /// a divider line. Lines are the post-`:` body of `State: desc`.
    pub description: Option<Vec<String>>,
    /// Classification — maps to the shape drawer.
    pub kind: StateKind,
    /// Parent id for nested composite states. `None` for top-level.
    pub parent: Option<String>,
    /// Ordered list of child state ids. Only populated on composite
    /// states.
    pub children: Vec<String>,
    /// Direction override inside a composite (`direction TB` line).
    pub direction: Option<String>,
    /// Hidden from main rendering — used by the fake start/end marker
    /// injection when `[*]` appears at the top level.
    pub implicit: bool,
    /// Inline style from `style X fill:...,stroke:...` directive.
    /// Stored as raw CSS (comma-separated properties).
    pub style: Option<String>,
}

/// State variant — drives shape selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StateKind {
    /// Regular state (rounded rect).
    #[default]
    Simple,
    /// `[*]` marker — direction-dependent: when it's the source of an
    /// edge it's a start marker; as target, an end marker. The parser
    /// creates one state per occurrence and tags it with this variant;
    /// the layout stage picks the shape based on edge role.
    StartEnd,
    /// `state X <<fork>>`.
    Fork,
    /// `state X <<join>>`.
    Join,
    /// `state X <<choice>>`.
    Choice,
    /// History marker — `[H]` (shallow) / `[H*]` (deep).
    History,
    /// Deep history marker — `[H*]`.
    HistoryDeep,
    /// Composite state — has child states inside a cluster box.
    Composite,
    /// Note shape (a yellow sticky). Represented as a State so nested
    /// layout naturally handles it, but the renderer uses the note
    /// shape and ignores the label-container styling.
    Note,
    /// Divider — `---` horizontal separator inside a composite state.
    Divider,
}

/// A transition edge — `source --> target [: label]`.
#[derive(Debug, Clone, Default)]
pub struct Transition {
    pub source: String,
    pub target: String,
    /// Multi-line label; lines are split on literal `\n` or `<br/>`.
    pub label: Option<Vec<String>>,
    /// Stylis-minified inline style fragment (optional).
    pub style: Option<String>,
}

/// A note attached to a state (`note right of X`).
#[derive(Debug, Clone, Default)]
pub struct Note {
    /// State the note is anchored to.
    pub target: String,
    /// Placement — `left of` / `right of` / `above` / `below`.
    pub position: NotePosition,
    /// Free text (may contain newlines).
    pub text: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NotePosition {
    #[default]
    RightOf,
    LeftOf,
    Above,
    Below,
}

/// One `classDef NAME css-fragment` entry.
#[derive(Debug, Clone, Default)]
pub struct ClassDef {
    pub name: String,
    pub styles: String,
}

/// One `class X NAME` or `state X:::NAME` entry.
#[derive(Debug, Clone, Default)]
pub struct ClassApply {
    pub state_id: String,
    pub class_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_diagram_is_v1_empty() {
        let d = StateDiagram::default();
        assert!(!d.is_v2);
        assert!(d.states.is_empty());
        assert!(d.transitions.is_empty());
    }

    #[test]
    fn state_kind_default_is_simple() {
        assert_eq!(StateKind::default(), StateKind::Simple);
    }

    #[test]
    fn note_position_default_is_right_of() {
        assert_eq!(NotePosition::default(), NotePosition::RightOf);
    }
}
