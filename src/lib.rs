//! mermaid-little — pure-Rust reimplementation of Mermaid, targeting
//! byte-exact SVG output parity with upstream `mermaid@11.14.0`.
//!
//! This crate is in the scaffolding phase — no diagram types are
//! implemented yet. See `FEATURES.md` for the support matrix and
//! execution plan.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MermaidError {
    #[error("unsupported diagram type: {0}")]
    Unsupported(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("render error: {0}")]
    Render(String),
}

/// Convert mermaid source text (`.mmd`) into SVG.
///
/// Returns `MermaidError::Unsupported` for every input until individual
/// diagram types are wired up.
pub fn convert(_source: &str) -> Result<String, MermaidError> {
    Err(MermaidError::Unsupported(
        "mermaid-little is scaffolding — no diagram types wired yet".into(),
    ))
}
