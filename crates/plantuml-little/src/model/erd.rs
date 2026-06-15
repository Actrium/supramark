/// ERD (Chen notation) diagram IR

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ErdAttribute {
    pub name: String,
    pub display_name: Option<String>,
    pub is_key: bool,
    pub is_derived: bool,
    pub is_multi: bool,
    pub attr_type: Option<String>,
    pub children: Vec<ErdAttribute>,
    pub color: Option<String>,
    /// 0-indexed line number in the full source (including @start line).
    pub source_line: usize,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ErdEntity {
    /// Internal identifier (alias if present, otherwise display name)
    pub id: String,
    /// Display name
    pub name: String,
    pub attributes: Vec<ErdAttribute>,
    pub is_weak: bool,
    pub color: Option<String>,
    /// Source declaration order (shared counter with relationships)
    pub source_order: usize,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ErdRelationship {
    /// Internal identifier (alias if present, otherwise display name)
    pub id: String,
    /// Display name
    pub name: String,
    pub attributes: Vec<ErdAttribute>,
    pub is_identifying: bool,
    pub color: Option<String>,
    /// Source declaration order (shared counter with entities)
    pub source_order: usize,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ErdLink {
    pub from: String,
    pub to: String,
    pub cardinality: String,
    pub is_double: bool,
    pub color: Option<String>,
    /// Arrow direction for ISA simple subclass links (`->-` or `-<-`).
    /// `None` for normal links, `Some(true)` for `>` (superset), `Some(false)` for `<` (subset).
    pub isa_arrow: Option<bool>,
    /// Source declaration order (shared counter with entities, relationships, ISAs).
    pub source_order: usize,
    /// 0-indexed line number in the full source (including @start... line).
    pub source_line: usize,
}

/// ISA (specialization/generalization) relationship
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ErdIsa {
    /// Parent entity id
    pub parent: String,
    /// 'd' for disjoint, 'U' for union/overlap
    pub kind: IsaKind,
    /// Child entity ids
    pub children: Vec<String>,
    pub is_double: bool,
    pub color: Option<String>,
    /// Source declaration order (shared counter with entities and relationships)
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum IsaKind {
    Disjoint,
    Union,
}

#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum ErdDirection {
    #[default]
    TopToBottom,
    LeftToRight,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ErdNote {
    pub text: String,
    pub position: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ErdDiagram {
    pub entities: Vec<ErdEntity>,
    pub relationships: Vec<ErdRelationship>,
    pub links: Vec<ErdLink>,
    pub isas: Vec<ErdIsa>,
    pub direction: ErdDirection,
    pub notes: Vec<ErdNote>,
}
