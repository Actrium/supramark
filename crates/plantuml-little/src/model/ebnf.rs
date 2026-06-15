#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct EbnfDiagram {
    pub title: Option<String>,
    pub comment: Option<String>,
    pub rules: Vec<EbnfRule>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct EbnfRule {
    pub name: String,
    pub expr: EbnfExpr,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum EbnfExpr {
    Terminal(String),
    NonTerminal(String),
    Sequence(Vec<EbnfExpr>),
    Alternation(Vec<EbnfExpr>),
    Optional(Box<EbnfExpr>),
    Repetition(Box<EbnfExpr>),
    Group(Box<EbnfExpr>),
    Special(String),
    /// Regex character class group `[abc]`, rendered as dashed box with stacked entries
    RegexGroup(Vec<String>),
    /// Regex OneOrMore with a label (e.g. `{2,3}`), drawn with brace
    RepetitionLabeled(Box<EbnfExpr>, String),
}
