//! mermaid-little — pure-Rust reimplementation of Mermaid, targeting
//! byte-exact SVG output parity with upstream `mermaid@11.14.0`.
//!
//! Wave 0 lands foundations: module tree, Diagram/DiagramLayout enums
//! with placeholders for all 25 diagram types, config + preprocess +
//! detect pipeline, theme variants, eval harness. No diagram types
//! render yet — each Wave 1+ milestone fills in one or more variants.
//!
//! Licensing: core crate is MIT. Portions vendored from sister
//! projects (plantuml-little, dagre-rs, selkie, mmdr, mmdflux) are
//! marked with per-file attribution blocks.

pub mod config;
pub mod detect;
pub mod error;
pub mod font_data;
pub mod font_metrics;
pub mod layout;
pub mod model;
pub mod parser;
pub mod preprocess;
pub mod render;
pub mod text;
pub mod theme;

pub use error::MermaidError;

/// Convert mermaid source text (`.mmd`) into SVG.
///
/// Returns `MermaidError::Unsupported` for every input until individual
/// diagram types are wired up.
pub fn convert(source: &str) -> Result<String, MermaidError> {
    let _preprocessed = preprocess::preprocess(source)?;
    Err(MermaidError::Unsupported(
        "mermaid-little is in Wave 0 — diagram renderers arrive in Wave 1+".into(),
    ))
}
