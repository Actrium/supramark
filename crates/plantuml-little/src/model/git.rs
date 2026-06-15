// Git log diagram model — visualizes a git commit graph.
//
// Input format:
//   `* main`
//   `** feature1`
//   `** feature2`
//
// Asterisks indicate depth/nesting level.

/// A single node in the git commit graph.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct GitNode {
    /// Depth level (number of asterisks, 1-based).
    pub depth: usize,
    /// Label text.
    pub label: String,
    /// Index in the original list (for ordering).
    pub index: usize,
}

/// The git log diagram model.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct GitDiagram {
    /// Ordered list of commit nodes.
    pub nodes: Vec<GitNode>,
}
