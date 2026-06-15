#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ChartSeries {
    pub label: String,
    pub values: Vec<f64>,
    pub series_type: ChartSeriesType,
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum ChartSeriesType {
    Bar,
}
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ChartDiagram {
    pub x_labels: Vec<String>,
    pub series: Vec<ChartSeries>,
    pub x_title: Option<String>,
    pub y_title: Option<String>,
}
