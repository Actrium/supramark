#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum FlowDirection {
    North,
    South,
    East,
    West,
}

impl FlowDirection {
    pub fn from_char(c: char) -> Option<Self> {
        match c.to_ascii_lowercase() {
            'n' => Some(Self::North),
            's' => Some(Self::South),
            'e' => Some(Self::East),
            'w' => Some(Self::West),
            _ => None,
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::South => Self::North,
            Self::East => Self::West,
            Self::West => Self::East,
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct FlowNode {
    pub id: String,
    pub label: String,
    pub placement: Option<FlowDirection>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct FlowLink {
    pub from: String,
    pub to: String,
    pub direction: FlowDirection,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct FlowDiagram {
    pub nodes: Vec<FlowNode>,
    pub links: Vec<FlowLink>,
}
