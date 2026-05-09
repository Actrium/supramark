//! Shared static font metric tables for the `*-little` family of Rust
//! ports (plantuml-little, mermaid-little).
//!
//! # Why this crate exists
//!
//! plantuml-little and mermaid-little both need to know "given this
//! string + this font + this size, what's the bounding box?" to lay
//! out SVG diagrams whose text geometry must match the upstream
//! Java / JS reference output. They both solve this with the same
//! approach: ship pre-computed range tables generated offline from
//! DejaVu TTFs (via `tools/gen_font_data.py`).
//!
//! Until 2026-05-09 mermaid-little carried a hand-vendored copy of
//! plantuml-little's range tables (`font_data.rs`, 9.6K LOC) anchored
//! to plantuml's commit `b32d6aa`. Future plantuml-side font fixes
//! would not flow through automatically. This crate centralises the
//! tables so both sister projects depend on a single source of truth.
//!
//! # What's in here today (Phase 1)
//!
//! - [`font_data`]: the 22K LOC of static `FontMeta` definitions
//!   covering DejaVu Sans / Sans Mono / Serif in plain + bold + italic
//!   + bold-italic combinations. Verbatim move from
//!   plantuml-little/src/font_data.rs.
//!
//! # What's coming (Phase 2 / 3)
//!
//! A `Metrics` trait + multiple implementations
//! (`StaticDejaVuMetrics`, `TtfParserMetrics`, `HostCallbackMetrics`)
//! so callers can pick a measurement strategy at runtime. The static
//! tables here become one such implementation and serve double-duty
//! as the byte-exact upstream-parity test fixture.

pub mod font_data;
