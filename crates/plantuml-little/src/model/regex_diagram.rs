#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct RegexDiagram {
    pub node: RegexNode,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum RegexNode {
    Literal(String),
    CharClass(Vec<String>),
    Concat(Vec<RegexNode>),
    Alternate(Vec<RegexNode>),
    Quantifier {
        inner: Box<RegexNode>,
        min: u32,
        max: Option<u32>,
        label: String,
    },
    Optional(Box<RegexNode>),
    Group(Box<RegexNode>),
}
