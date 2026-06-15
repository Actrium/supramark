//! Salt (mockup) diagram model.
//!
//! Mirrors Java PlantUML's `net.sourceforge.plantuml.salt` package. Salt text is
//! parsed into a grid of cells, where each cell holds one UI element (button,
//! text field, checkbox, etc.). The grid uses the same row/column semantics as
//! Java's `Positionner2`: cells advance by column on `|` and by row on newline.

/// A parsed salt diagram. The root element is always a pyramid (grid) since
/// Java requires salt content to be wrapped in `{ ... }`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct SaltDiagram {
    pub root: SaltElement,
    /// True when salt is embedded inside `@startuml` (inline), false for
    /// `@startsalt`. Retained as parsing metadata; both modes share the
    /// same `data-diagram-type="SALT"` SVG header in Java PlantUML.
    pub is_inline: bool,
}

/// A single UI element inside the salt grid.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum SaltElement {
    /// Plain text label. Java: `ElementText`.
    Text(String),
    /// Button. Java: `ElementButton`. Text is the inside of `[...]`.
    Button(String),
    /// Text input field. Java: `ElementTextField`. Text is the inside of `"..."`.
    TextField(String),
    /// Checkbox with label and on/off state. Java: `ElementRadioCheckbox` (radio=false).
    Checkbox { label: String, checked: bool },
    /// Radio button with label and on/off state. Java: `ElementRadioCheckbox` (radio=true).
    Radio { label: String, selected: bool },
    /// A nested pyramid (grid). Java: `ElementPyramid`.
    Pyramid(SaltPyramid),
}

/// A pyramid (grid) of salt cells. Java: `ElementPyramid`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct SaltPyramid {
    /// Cells in the order they were added by the positioner. Each cell has its
    /// own row/col coordinates, and may span multiple rows/columns via `mergeLeft`.
    pub cells: Vec<SaltCell>,
    /// Total rows used by the positioner. Always `max(cell.max_row) + 1`.
    pub rows: usize,
    /// Total cols used by the positioner.
    pub cols: usize,
    /// Drawing strategy for grid lines. Determined by the block header (`{`, `{#`, etc.).
    pub strategy: TableStrategy,
}

/// A single cell in a salt pyramid. Holds its element plus its row/col span.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct SaltCell {
    pub min_row: usize,
    pub max_row: usize,
    pub min_col: usize,
    pub max_col: usize,
    pub element: SaltElement,
}

impl SaltCell {
    pub fn new(row: usize, col: usize, element: SaltElement) -> Self {
        Self {
            min_row: row,
            max_row: row,
            min_col: col,
            max_col: col,
            element,
        }
    }
}

/// Table grid line drawing strategy. Mirrors Java `TableStrategy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum TableStrategy {
    /// No grid lines. Used for plain `{`.
    DrawNone,
    /// Horizontal + vertical + outside. Used for `{#`.
    DrawAll,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_new_defaults_to_single_cell_span() {
        let cell = SaltCell::new(2, 3, SaltElement::Text("hi".into()));
        assert_eq!(cell.min_row, 2);
        assert_eq!(cell.max_row, 2);
        assert_eq!(cell.min_col, 3);
        assert_eq!(cell.max_col, 3);
    }
}
