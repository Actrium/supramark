/// Line style
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum LineStyle {
    Solid,  // --
    Dashed, // ..
}

/// Arrow head type
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum ArrowHead {
    None,          // no arrow
    Arrow,         // >  normal arrow
    Triangle,      // |> hollow triangle (inheritance/implementation)
    Diamond,       // *  filled diamond (composition)
    DiamondHollow, // o  hollow diamond (aggregation)
    Plus,          // +  cross (nested/inner class)
}

/// Relationship between entities
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Link {
    pub uid: Option<String>,
    pub from: String,
    pub to: String,
    pub left_head: ArrowHead,
    pub right_head: ArrowHead,
    pub line_style: LineStyle,
    pub label: Option<String>,
    pub from_label: Option<String>,
    pub to_label: Option<String>,
    pub from_qualifier: Option<String>,
    pub to_qualifier: Option<String>,
    pub source_line: Option<usize>,
    /// Number of dashes/dots in the arrow.  1 = horizontal (LR), >=2 = vertical (TB).
    /// Java: Link.getLength()
    pub arrow_len: usize,
}
