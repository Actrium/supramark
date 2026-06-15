/// Math/Latex diagram model (monospace text fallback).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct MathDiagram {
    /// The formula text (single line).
    pub formula: String,
}
