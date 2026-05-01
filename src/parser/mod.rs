//! Per-diagram text-to-model parsers. Dispatch happens in
//! [`crate::detect`]; each submodule here owns one diagram kind.

pub mod block;
pub mod c4;
pub mod class;
pub mod common;
pub mod er;
pub mod flowchart;
pub mod gantt;
pub mod ishikawa;
pub mod journey;
pub mod kanban;
pub mod packet;
pub mod pie;
pub mod quadrant;
pub mod radar;
pub mod requirement;
pub mod richtext;
pub mod sankey;
pub mod state;
pub mod timeline;
pub mod treemap;
pub mod wardley;
pub mod xychart;
