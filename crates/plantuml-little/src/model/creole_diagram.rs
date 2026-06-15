/// An element in a creole diagram.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum CreoleElement {
    /// Heading: `= Title` (level 1), `== Subtitle` (level 2), etc.
    Heading { text: String, level: usize },
    /// Bullet list item: `* item`.
    Bullet { text: String, level: usize },
    /// Plain text.
    Text(String),
}

/// Creole diagram model.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct CreoleDiagram {
    pub elements: Vec<CreoleElement>,
}
