//! Flowchart (the `flowchart` / `graph` diagram) — AST.
//!
//! Upstream reference:
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/flowchart/`
//! (`flow.jison` grammar + `flowDb.ts` state).
//!
//! The model captures everything the parser extracts from the source
//! text, prior to layout/render. It mirrors the upstream `flowDb`
//! fields field-for-field where feasible.

use crate::model::DiagramMeta;
use std::collections::BTreeMap;

/// Graph direction. Upstream accepts `TB / TD / BT / LR / RL` plus
/// ASCII-arrow aliases `> < ^ v`. We normalise to the four canonical
/// forms — `TD` is an alias for `TB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum Direction {
    #[default]
    TB,
    BT,
    LR,
    RL,
}

impl Direction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Direction::TB => "TB",
            Direction::BT => "BT",
            Direction::LR => "LR",
            Direction::RL => "RL",
        }
    }

    /// Parse upstream strings including aliases.
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        match t {
            "TB" | "TD" | "v" => Some(Self::TB),
            "BT" | "^" => Some(Self::BT),
            "LR" | ">" => Some(Self::LR),
            "RL" | "<" => Some(Self::RL),
            _ => None,
        }
    }
}

/// Label text — carried with its parsing mode so renderers know
/// whether the content is plain text, a quoted string, or markdown.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Label {
    pub text: String,
    pub kind: LabelKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum LabelKind {
    #[default]
    Text,
    String,
    Markdown,
}

impl Label {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            text: s.into(),
            kind: LabelKind::Text,
        }
    }
    pub fn string(s: impl Into<String>) -> Self {
        Self {
            text: s.into(),
            kind: LabelKind::String,
        }
    }
    pub fn markdown(s: impl Into<String>) -> Self {
        Self {
            text: s.into(),
            kind: LabelKind::Markdown,
        }
    }
}

/// A single vertex (a.k.a. node) declared in the source.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Vertex {
    pub id: String,
    /// Empty label means "use the id as label" — upstream behaviour.
    pub label: Option<Label>,
    /// Shape string — e.g. `rect / round / stadium / diamond / ...`.
    /// `None` means the vertex was first seen without shape syntax
    /// (default `rect`).
    pub shape: Option<String>,
    /// Inline `style id fill:#...` declarations. One entry per decl.
    pub styles: Vec<String>,
    /// `class id :::cls` assignments (single-colon form via `class`
    /// statement).
    pub classes: Vec<String>,
    /// `click` event.
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub tooltip: Option<String>,
    pub callback: Option<String>,
    pub callback_args: Option<String>,
    /// Extra properties from `[|field:value|label]` vertex variant.
    pub props: BTreeMap<String, String>,
    /// YAML-style `@{shape: circle, label: "..."}` data — captured as
    /// raw text for the renderer to reinterpret.
    pub shape_data: Option<String>,
    /// Declaration order — used to break ties in rendering / id-suffix.
    pub order: usize,
}

/// Edge stroke style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum EdgeStroke {
    #[default]
    Normal,
    Thick,
    Dotted,
    Invisible,
}

/// Edge arrow-head type (both ends).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum ArrowType {
    #[default]
    None,
    Arrow,
    Circle,
    Cross,
    Point,
}

/// An edge between two (or more) vertices. Flowchart allows `A & B --> C`
/// which produces `[A-C, B-C]`; the parser expands such groups to
/// individual `Edge` entries.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Edge {
    pub id: Option<String>,
    pub start: String,
    pub end: String,
    pub stroke: EdgeStroke,
    pub length: usize,
    pub arrow_end: ArrowType,
    pub arrow_start: ArrowType,
    pub label: Option<Label>,
    /// Edge declaration index — used for `linkStyle N stroke:...` lookup.
    pub index: usize,
    /// CSS class names applied via `class <edge-id> <className>` directives.
    /// Resolved against `class_defs` at layout time to populate inline styles
    /// — mirrors how upstream's `applyClassesToEdge` walks each edge's
    /// `cssClasses` array.
    pub classes: Vec<String>,
    /// Per-edge curve type from `@{ curve: <type> }` metadata.
    pub curve: Option<String>,
    /// Per-edge animation type from `@{ animation: <slow|fast> }` metadata.
    /// Maps to upstream's `edge-animation-slow` / `edge-animation-fast` CSS
    /// classes that drive the dashed-line dance animation.
    pub animation: Option<String>,
    /// Innermost subgraph active where the edge statement was declared.
    /// `None` means the edge was declared at diagram root.
    pub scope: Option<String>,
}

/// A subgraph (cluster) declaration.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Subgraph {
    pub id: String,
    pub title: Option<Label>,
    /// Member vertex-ids (those directly inside this subgraph).
    pub members: Vec<String>,
    /// Nested sub-subgraph ids.
    pub children: Vec<String>,
    /// Direction override inside this subgraph.
    pub dir: Option<Direction>,
    /// Declaration order — first-seen.
    pub order: usize,
    /// Class names applied via `class <subgraph-id> <name>` directive.
    /// Mirrors `Vertex::classes` for clusters: at render-time these expand
    /// into the cluster `<g>`'s `class="cluster <name1> <name2>"` attribute
    /// and resolve to inline `style=` via `classDef`.
    pub classes: Vec<String>,
}

/// `classDef name fill:#red,stroke:#000` — set of style props for a class.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ClassDef {
    pub name: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

/// `linkStyle N stroke:#red,...`, or `linkStyle default ...`.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct LinkStyle {
    /// Which edge indices this rule applies to. Empty = default.
    pub indices: Vec<usize>,
    pub is_default: bool,
    pub styles: Vec<String>,
    pub interpolate: Option<String>,
}

/// Top-level flowchart AST.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct FlowchartDiagram {
    pub meta: DiagramMeta,
    /// Direction declared on the header line (`flowchart TD`).
    pub direction: Direction,
    /// Theme name lifted from the frontmatter (`config.theme`) or an
    /// embedded `%%{init: {"theme":"..."}}%%` directive.
    pub theme_override: Option<String>,
    /// Optional `%%{init: {"themeVariables": {...}}}%%` content, JSON.
    pub theme_variables_raw: Option<String>,
    /// Effective `flowchart.htmlLabels` from frontmatter / `%%{init}%%`.
    /// `None` keeps the upstream default (`true`); `Some(false)` switches
    /// CSS selectors and label rendering to the non-HTML shape path.
    pub html_labels: Option<bool>,
    /// Effective `flowchart.nodeSpacing` (dagre `nodesep`) from frontmatter
    /// / `%%{init}%%`. `None` keeps the upstream default (50).
    pub node_spacing: Option<u32>,
    /// Effective `flowchart.rankSpacing` (dagre `ranksep`) from frontmatter
    /// / `%%{init}%%`. `None` keeps the upstream default (50).
    pub rank_spacing: Option<u32>,
    /// Effective `flowchart.curve` from frontmatter / `%%{init}%%`.
    /// `None` keeps the upstream default ("basis").
    pub curve: Option<String>,
    /// Effective `flowchart.inheritDir` from frontmatter / `%%{init}%%`.
    /// When `true`, subgraphs without an explicit `direction` keyword
    /// inherit the outer direction instead of flipping.
    pub inherit_dir: bool,
    /// All vertices declared, in source order. Duplicates coalesce —
    /// the first declaration wins, subsequent refs just need to exist.
    pub vertices: Vec<Vertex>,
    /// All edges, including those from `&`-groups (expanded to 1:1).
    pub edges: Vec<Edge>,
    /// Subgraph definitions, in source order.
    pub subgraphs: Vec<Subgraph>,
    /// Class definitions keyed by name.
    pub class_defs: Vec<ClassDef>,
    /// Per-edge overrides.
    pub link_styles: Vec<LinkStyle>,
    /// Did the source use `flowchart` (v2) or `graph` (v1)? Impacts
    /// `aria-roledescription` (`flowchart-v2` vs `flowchart-v1`).
    pub is_v2: bool,
    /// The raw header keyword: `"flowchart"`, `"flowchart-elk"`, or `"graph"`.
    /// Used by the renderer to determine `aria-roledescription` and marker IDs.
    pub header_keyword: String,
}

impl FlowchartDiagram {
    pub fn find_vertex(&self, id: &str) -> Option<&Vertex> {
        self.vertices.iter().find(|v| v.id == id)
    }
    pub fn find_vertex_mut(&mut self, id: &str) -> Option<&mut Vertex> {
        self.vertices.iter_mut().find(|v| v.id == id)
    }
    pub fn find_subgraph(&self, id: &str) -> Option<&Subgraph> {
        self.subgraphs.iter().find(|s| s.id == id)
    }
    pub fn find_subgraph_mut(&mut self, id: &str) -> Option<&mut Subgraph> {
        self.subgraphs.iter_mut().find(|s| s.id == id)
    }
}
