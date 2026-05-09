//! Unified error type for mermaid-little.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MermaidError {
    #[error("unsupported diagram type: {0}")]
    Unsupported(String),
    #[error("parse error at line {line}, col {col}: {message}")]
    Parse {
        line: usize,
        col: usize,
        message: String,
    },
    #[error("config error: {0}")]
    Config(String),
    #[error("render error: {0}")]
    Render(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, MermaidError>;
