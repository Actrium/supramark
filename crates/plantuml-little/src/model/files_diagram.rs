#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum FilesEntryKind {
    Folder,
    File,
}
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct FilesEntry {
    pub name: String,
    pub kind: FilesEntryKind,
    pub children: Vec<FilesEntry>,
}
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct FilesDiagram {
    pub entries: Vec<FilesEntry>,
}
