//! mermaid-little SVG evaluation framework (test-support module).
//!
//! Adapted from selkie (https://github.com/btucker/selkie), MIT license.
//! Originally `src/eval/mod.rs`. Significant trimming and adaptation to
//! mermaid-little's fixture layout and feature set — diagram-specific
//! checks, PNG rendering, sample discovery, reference caching and the
//! `cargo run --bin selkie eval` runner are intentionally not ported.
//!
//! # Usage (from a test file under `tests/`)
//!
//! ```ignore
//! #[path = "eval/mod.rs"]
//! mod eval;
//!
//! use eval::structural_diff;
//!
//! let candidate = mermaid_little::convert(source).unwrap();
//! let reference = std::fs::read_to_string("tests/reference/.../01.svg").unwrap();
//! let diff = structural_diff::compare(&candidate, &reference).unwrap();
//! assert!(diff.is_empty(), "divergence: {}", diff.report_text());
//! ```
//!
//! For a fixture sweep, collect per-fixture `Diff`s into an
//! [`report::EvalReport`] and call [`report::EvalReport::write_all`] to
//! dump `report.{txt,json,html}` into `target/eval/`.
//!
//! # Modules
//!
//! * [`structural_diff`] — roxmltree-based SVG parsing + structural diff.
//! * [`ssim`] — SSIM math for pre-decoded grayscale buffers. Decoding raw
//!   SVGs is stubbed; enable once an `eval-ssim` feature is added.
//! * [`report`] — text / JSON / HTML report emitters.

#![allow(dead_code, unused_imports)]

pub mod report;
pub mod ssim;
pub mod structural_diff;

pub use report::{EvalReport, FixtureReport};
pub use structural_diff::{
    compare, compare_structures, compare_with_config, CheckConfig, Diff, Issue, Level,
    ShapeCounts, SvgStructure,
};
