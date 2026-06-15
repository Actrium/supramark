/// A single event on a chronology timeline.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ChronologyEvent {
    pub date: String,
    pub label: String,
}

/// Chronology diagram model.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ChronologyDiagram {
    pub events: Vec<ChronologyEvent>,
}
