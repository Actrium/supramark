//! C4 SVG renderer (placeholder).
//!
//! C4 in upstream `mermaid@11.14.0` is a custom layout + custom SVG
//! draw pipeline (~2 945 LOC across `c4Renderer.js`, `svgDraw.js`,
//! `c4Db.js`, the jison grammar, and the styles file). It depends on
//! - per-shape font measurement of label / type / techn / descr,
//! - bound-packing into rows that respect `c4ShapeInRow` /
//!   `c4BoundaryInRow` (with `UpdateLayoutConfig` overrides),
//! - rectangle-edge intersection geometry for relationship endpoints
//!   on both the source and target sides,
//! - a bespoke `<symbol>` library (database, computer, clock) plus
//!   inline base64 PNG sprites for `Person` / `Person_Ext`,
//! - and three different text drawing modes selected by
//!   `conf.textPlacement` (`fo`, `tspan`, `old`).
//!
//! Reaching upstream byte-exact parity is therefore a substantial port
//! that has not been completed yet — see `tests/known_ignored.txt`.
//! This stub returns an `Unsupported` error so the dispatch in
//! `lib.rs::convert_with_id` compiles cleanly and the `c4` arm becomes
//! exhaustive.

use crate::error::MermaidError;
use crate::model::c4::C4Diagram;
use crate::theme::ThemeVariables;

pub fn render(
    _diagram: &C4Diagram,
    _theme: &ThemeVariables,
    _id: &str,
) -> Result<String, MermaidError> {
    Err(MermaidError::Unsupported(
        "c4 renderer not yet implemented — see tests/known_ignored.txt".into(),
    ))
}
