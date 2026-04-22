//! Per-diagram text-to-model parsers. Dispatch happens in
//! [`crate::detect`]; each submodule here owns one diagram kind.

pub mod common;
pub mod richtext;
