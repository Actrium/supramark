//! Pie diagram parsed model.
//!
//! Upstream: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/pie/
//! Grammar (langium): /ext/mermaid-official-stable-v11.14.0/packages/parser/src/language/pie/pie.langium
//!
//! The parser preserves insertion order — upstream's `pieDb.addSection` uses a
//! Map whose first-write ordering mermaid itself depends on for slice/legend
//! sequencing. Duplicate labels are silently dropped (first value wins).

use crate::model::DiagramMeta;

/// Parsed pie chart.
#[derive(Debug, Clone, Default)]
pub struct PieDiagram {
    pub meta: DiagramMeta,
    /// `pie showData` header toggle — renders slice values `[N]` inside legend labels.
    pub show_data: bool,
    /// Insertion-ordered slices.
    pub slices: Vec<PieSlice>,
    /// `%%{init: {pie: {textPosition}}}%%` — label radius fraction (default 0.75).
    pub text_position: f64,
    /// `%%{init: {themeVariables: {pieOuterStrokeWidth}}}%%` — raw CSS length (default `"2px"`).
    pub outer_stroke_width: String,
}

/// One slice: (label, value). Insertion order is preserved.
#[derive(Debug, Clone)]
pub struct PieSlice {
    pub label: String,
    pub value: f64,
}
