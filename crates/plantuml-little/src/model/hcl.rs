/// A key-value pair in an HCL block.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct HclEntry {
    pub key: String,
    pub value: String,
}

/// HCL (Hashicorp Configuration Language) diagram model.
/// Rendered as a key-value table, similar to JSON tree-table.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct HclDiagram {
    pub entries: Vec<HclEntry>,
}
