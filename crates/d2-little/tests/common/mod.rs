//! Shared helpers across integration test files.  Each test file opts in
//! with `#[path = "common/mod.rs"] mod common;` (Rust gives every test file
//! its own crate, so we share via `path`).

pub mod latex_engine;
