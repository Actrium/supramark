/// A task node in a board diagram (Kanban-style).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct BoardTask {
    pub label: String,
    pub level: usize,
    pub children: Vec<BoardTask>,
}

/// Board diagram model.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct BoardDiagram {
    pub tasks: Vec<BoardTask>,
}
