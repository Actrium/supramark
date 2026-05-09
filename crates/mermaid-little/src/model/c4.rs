//! C4 model: parsed but pre-layout representation of a C4 diagram.
//!
//! Mirrors the upstream `c4Db.js` data structures. C4 diagrams are
//! built from C4 macros (`Person`, `System`, `Container`, `Component`,
//! `Boundary`, `Rel*`, etc.) and carry mostly textual data plus
//! parent-boundary relationships.

use crate::model::DiagramMeta;

/// One of the five C4 diagram subtypes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C4Subtype {
    Context,
    Container,
    Component,
    Dynamic,
    Deployment,
}

impl C4Subtype {
    pub fn as_str(&self) -> &'static str {
        match self {
            C4Subtype::Context => "C4Context",
            C4Subtype::Container => "C4Container",
            C4Subtype::Component => "C4Component",
            C4Subtype::Dynamic => "C4Dynamic",
            C4Subtype::Deployment => "C4Deployment",
        }
    }
}

/// A textual block — label, type, descr, techn.
#[derive(Debug, Clone, Default)]
pub struct C4Text {
    pub text: String,
}

/// A Person/System/Container/Component shape.
#[derive(Debug, Clone)]
pub struct C4Shape {
    /// Discriminator (`person`, `system`, `system_db`, `container`,
    /// `component_queue`, `external_system`, …).
    pub type_c4_shape: String,
    pub alias: String,
    pub label: C4Text,
    pub descr: C4Text,
    pub techn: C4Text,
    pub sprite: Option<String>,
    pub tags: Option<String>,
    pub link: Option<String>,
    pub parent_boundary: String,
    pub bg_color: Option<String>,
    pub font_color: Option<String>,
    pub border_color: Option<String>,
    pub shadowing: Option<String>,
    pub shape: Option<String>,
    pub legend_text: Option<String>,
    pub legend_sprite: Option<String>,
    pub wrap: bool,
}

/// A boundary or deployment node.
#[derive(Debug, Clone)]
pub struct C4Boundary {
    pub alias: String,
    pub label: C4Text,
    /// `system`, `container`, `node`, `ENTERPRISE`, `SYSTEM`,
    /// `CONTAINER`, or a user-supplied string.
    pub b_type: C4Text,
    pub descr: C4Text,
    pub tags: Option<String>,
    pub link: Option<String>,
    pub parent_boundary: String,
    /// `node`, `nodeL`, `nodeR` for deployment nodes; `None` for
    /// regular boundaries.
    pub node_type: Option<String>,
    pub bg_color: Option<String>,
    pub font_color: Option<String>,
    pub border_color: Option<String>,
    pub wrap: bool,
}

/// A relationship between two shapes.
#[derive(Debug, Clone)]
pub struct C4Rel {
    /// `rel`, `birel`, `rel_u`, `rel_d`, `rel_l`, `rel_r`, `rel_b`.
    pub rel_type: String,
    pub from: String,
    pub to: String,
    pub label: C4Text,
    pub techn: C4Text,
    pub descr: C4Text,
    pub sprite: Option<String>,
    pub tags: Option<String>,
    pub link: Option<String>,
    pub text_color: Option<String>,
    pub line_color: Option<String>,
    pub offset_x: Option<i32>,
    pub offset_y: Option<i32>,
    pub wrap: bool,
}

/// Parsed C4 diagram.
#[derive(Debug, Clone)]
pub struct C4Diagram {
    pub subtype: C4Subtype,
    pub meta: DiagramMeta,
    /// Boundaries in declaration order, with the synthetic `global`
    /// root prepended (alias=="global", parent_boundary=="").
    pub boundaries: Vec<C4Boundary>,
    /// Shapes in declaration order.
    pub shapes: Vec<C4Shape>,
    /// Relationships in declaration order.
    pub rels: Vec<C4Rel>,
    /// `UpdateLayoutConfig` overrides — None means inherit defaults
    /// (4 / 2).
    pub c4_shape_in_row: Option<u32>,
    pub c4_boundary_in_row: Option<u32>,
}

impl Default for C4Diagram {
    fn default() -> Self {
        Self {
            subtype: C4Subtype::Context,
            meta: DiagramMeta::default(),
            boundaries: vec![C4Boundary {
                alias: "global".into(),
                label: C4Text { text: "global".into() },
                b_type: C4Text { text: "global".into() },
                descr: C4Text::default(),
                tags: None,
                link: None,
                parent_boundary: String::new(),
                node_type: None,
                bg_color: None,
                font_color: None,
                border_color: None,
                wrap: false,
            }],
            shapes: Vec::new(),
            rels: Vec::new(),
            c4_shape_in_row: None,
            c4_boundary_in_row: None,
        }
    }
}
