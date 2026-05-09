//! Port of upstream `rendering-util/render.ts` (146 LoC).
//!
//! Upstream reference:
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/rendering-util/render.ts`
//!
//! In upstream TypeScript this file:
//!
//! 1. Registers layout algorithm loaders (dagre by default, elk / cose-bilkent
//!    when feature-flagged);
//! 2. Injects SVG-level defs (drop-shadow filters, gradient stops);
//! 3. Dispatches to `layoutRenderer.render(...)`.
//!
//! For the Wave 3 P0 port we keep (1) and (3) — the SVG-defs stage is
//! renderer-layer, not layout-layer, and lives in `src/render/` once the
//! Stratum 3 diagram work starts. The Rust analogue is therefore:
//!
//! * `layout(...)` — pure-layout dispatcher returning geometry only.
//! * `registered_algorithms()` — introspection for CLI / tests.

use crate::error::{MermaidError, Result};
use crate::theme::ThemeVariables;

use super::super::dagre_bridge;
use super::types::{LayoutData, LayoutResult};

/// Fallback layout algorithm when the requested one is absent or unknown.
/// Matches upstream `getRegisteredLayoutAlgorithm`'s default.
pub const DEFAULT_ALGORITHM: &str = "dagre";

/// Pure-layout dispatcher. Mirrors upstream `render.ts::render`'s layout
/// half: picks a registered engine, hands it the `LayoutData`, returns
/// the resulting geometry.
///
/// `algorithm` is matched case-insensitively against the known-engine
/// list. When it fails to match, we fall back to [`DEFAULT_ALGORITHM`]
/// with a warning log (upstream does the same via `log.warn`).
pub fn layout(data: &LayoutData, algorithm: &str, theme: &ThemeVariables) -> Result<LayoutResult> {
    let requested = algorithm.trim().to_ascii_lowercase();
    let effective = match requested.as_str() {
        // `dagre-wrapper` is the pre-v11 alias for the dagre engine.
        "" | "dagre" | "dagre-wrapper" => "dagre",
        other => {
            log::warn!("unknown layout algorithm '{other}', falling back to '{DEFAULT_ALGORITHM}'");
            DEFAULT_ALGORITHM
        }
    };

    match effective {
        "dagre" => dagre_bridge::layout(data, theme),
        // Every other registered name is a stub slot today — report it
        // cleanly so Stratum 3 wiring doesn't swallow the mis-config.
        other => Err(MermaidError::Render(format!(
            "layout algorithm '{other}' is registered but not implemented"
        ))),
    }
}

/// Introspection — names callers can pass to [`layout`].
pub fn registered_algorithms() -> &'static [&'static str] {
    &["dagre", "dagre-wrapper"]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeVariables;

    #[test]
    fn unknown_algorithm_falls_back_to_dagre() {
        let data = LayoutData::default();
        let theme = ThemeVariables::default();
        // Empty graph must still succeed under the fallback path.
        let out = layout(&data, "elk", &theme).expect("fallback to dagre");
        assert!(out.nodes.is_empty());
        assert!(out.edges.is_empty());
    }

    #[test]
    fn empty_algorithm_defaults_to_dagre() {
        let data = LayoutData::default();
        let theme = ThemeVariables::default();
        let _ = layout(&data, "", &theme).expect("default dispatch");
    }

    #[test]
    fn registered_includes_dagre() {
        assert!(registered_algorithms().contains(&"dagre"));
    }
}
