// for bragging rights
#![forbid(unsafe_code)]
//
// useful asserts that's off by default
#![warn(clippy::manual_assert)]
#![warn(clippy::semicolon_if_nothing_returned)]
//
// these are often intentionally not collapsed for readability
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
//
// these are intentional in bevy systems: nobody is directly calling those,
// so extra arguments don't decrease readability
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
//
// just a style choice that clippy has no business complaining about
#![allow(clippy::uninlined_format_args)]

#[allow(
    dead_code,
    unused_assignments,
    unused_imports,
    mismatched_lifetime_syntaxes
)]
mod common;
#[allow(
    dead_code,
    unused_assignments,
    unused_imports,
    mismatched_lifetime_syntaxes
)]
mod generics;
#[allow(
    dead_code,
    unused_assignments,
    unused_imports,
    mismatched_lifetime_syntaxes
)]
mod parser;
#[allow(
    dead_code,
    unused_assignments,
    unused_imports,
    mismatched_lifetime_syntaxes
)]
mod plugins;
mod supramark;

pub(crate) use parser::main::MarkdownParser;
pub(crate) use parser::node::{Node, NodeValue};
pub(crate) use parser::renderer::Renderer;
pub use supramark::{
    parse, Diagnostic, DiagnosticSeverity, ExtensionMode, ParserInfo, SourcePoint, SourcePosition,
    SupramarkNode, TableAlign,
};
