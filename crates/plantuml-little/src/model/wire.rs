/// A block (component) in a wire diagram.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct WireBlock {
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub color: Option<String>,
    pub level: usize,
}

/// A vertical link between two blocks.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct WireVLink {
    pub from: String,
    pub to: String,
}

/// Wire diagram model.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct WireDiagram {
    pub blocks: Vec<WireBlock>,
    pub vlinks: Vec<WireVLink>,
}
