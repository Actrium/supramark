//! Per-diagram text-to-model parsers. Dispatch happens in
//! [`crate::detect`]; each submodule here owns one diagram kind.

pub mod common;
pub mod ishikawa;
pub mod journey;
pub mod kanban;
pub mod packet;
pub mod pie;
pub mod quadrant;
pub mod radar;
pub mod richtext;
pub mod sankey;
pub mod timeline;
pub mod treemap;
pub mod wardley;
pub mod xychart;
