/// DOT passthrough diagram — stores raw DOT source for Graphviz rendering.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct DotDiagram {
    pub source: String,
}
