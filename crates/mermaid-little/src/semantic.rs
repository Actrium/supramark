//! Semantic AST aggregation layer: converges each diagram's strongly typed
//! model into one aggregate enum [`MermaidAst`] dispatched by
//! [`detect::DiagramKind`], and provides the source-to-enum parse entry point
//! [`parse_semantic`].
//!
//! Design notes:
//! - The aggregate enum's `serde::Serialize` is gated by the crate feature
//!   `semantic-serde`, consistent with the `cfg_attr(... derive(serde::Serialize))`
//!   on `model/*`; the module still compiles fine without the feature (it just
//!   cannot be serialized).
//! - Currently only four diagram kinds are supported: er / flowchart / sequence
//!   / class. Other kinds make [`parse_semantic`] return
//!   [`SemanticError::Unsupported`], on which the upper layer lets
//!   `DiagramEngine::semantic` fall back to `Ok(None)`.

use crate::detect::{self, DiagramKind};
use crate::error::MermaidError;
use crate::model;
use crate::parser;
use crate::preprocess;

/// Aggregate semantic AST: each variant holds the strongly typed model for the
/// corresponding diagram kind.
///
/// The variant set is extended on demand -- it currently covers only the four
/// kinds already wired into `parse_semantic`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum MermaidAst {
    /// Entity-relationship diagram.
    Er(model::er::ErDiagram),
    /// Flowchart.
    Flowchart(model::flowchart::FlowchartDiagram),
    /// Sequence diagram.
    Sequence(model::sequence::SequenceDiagram),
    /// Class diagram.
    Class(model::class::ClassDiagram),
}

impl MermaidAst {
    /// The public diagram kind string for this AST, aligned with
    /// `EngineAst::kind`. Uses the diagram kind name rather than
    /// `DiagramKind::id` (here the two happen to coincide).
    pub fn kind_str(&self) -> &'static str {
        match self {
            MermaidAst::Er(_) => "er",
            MermaidAst::Flowchart(_) => "flowchart",
            MermaidAst::Sequence(_) => "sequence",
            MermaidAst::Class(_) => "class",
        }
    }
}

/// Error for `parse_semantic`: either underlying parsing failed (syntax error),
/// or this diagram kind does not currently support a semantic AST.
#[derive(Debug)]
pub enum SemanticError {
    /// The underlying parser errored (syntax error etc.); the original
    /// [`MermaidError`] is passed through.
    Parse(MermaidError),
    /// This diagram kind does not currently support a semantic AST; carries the
    /// detected kind name.
    Unsupported(&'static str),
}

impl From<MermaidError> for SemanticError {
    fn from(e: MermaidError) -> Self {
        SemanticError::Parse(e)
    }
}

/// Source -> aggregate semantic AST.
///
/// First [`detect::detect`] the diagram kind, then call the corresponding
/// `parser::xxx::parse`. Only the four supported kinds return `Ok`; the rest
/// return [`SemanticError::Unsupported`].
pub fn parse_semantic(source: &str) -> Result<MermaidAst, SemanticError> {
    // Preprocess before detection: detect's regexes anchor to line start with
    // `^`, so a frontmatter/directive that is not stripped would cause
    // misdetection (matching how `convert_with_id_inner`'s detect takes
    // `cleaned_source`). The parser still receives the raw source -- each parser
    // extracts frontmatter / `%%{init}%%` on its own.
    let cleaned = preprocess::preprocess(source)?.cleaned_source;
    let kind = detect::detect(&cleaned);
    match kind {
        DiagramKind::Er => Ok(MermaidAst::Er(parser::er::parse(source)?)),
        DiagramKind::Flowchart => Ok(MermaidAst::Flowchart(parser::flowchart::parse(source)?)),
        DiagramKind::Sequence => Ok(MermaidAst::Sequence(parser::sequence::parse(source)?)),
        DiagramKind::Class => Ok(MermaidAst::Class(parser::class::parse(source)?)),
        other => Err(SemanticError::Unsupported(other.id())),
    }
}
