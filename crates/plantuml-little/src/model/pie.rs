/// A single slice in a pie chart.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct PieSlice {
    pub label: String,
    pub value: f64,
}

/// Pie chart diagram model.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct PieDiagram {
    pub title: Option<String>,
    pub slices: Vec<PieSlice>,
}
