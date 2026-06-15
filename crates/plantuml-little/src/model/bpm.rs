/// BPM element type — matches Java BpmElementType.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum BpmElementType {
    Start,
    End,
    Merge,
    DockedEvent,
}

/// A single element in the BPM grid.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct BpmElement {
    pub id: Option<String>,
    pub element_type: BpmElementType,
    pub label: Option<String>,
    /// Connector lines to draw (N/S/E/W).
    pub connectors: Vec<Where>,
}

/// Direction for connector lines on BPM elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum Where {
    North,
    South,
    East,
    West,
}

/// BPM event — matches Java BpmEvent hierarchy.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum BpmEvent {
    Add(BpmElement),
    Resume(String),
    Goto(String),
}

/// BPM diagram model.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct BpmDiagram {
    pub events: Vec<BpmEvent>,
}
