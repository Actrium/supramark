//! Wires graphviz-anywhere into supramark's unified diagram abstraction.
//!
//! This stage (T0b) is render-only: the Rust side has no DOT semantic structure
//! (DOT parsing happens inside the C FFI), so it does not implement
//! [`DiagramEngine::semantic`] and instead keeps the trait's default
//! implementation returning `Ok(None)`, meaning "this engine does not currently
//! support a semantic AST".

use supramark_diagram_core::{DiagramEngine, DiagramError, RenderOutput};

use crate::{Engine, Format, GraphvizContext};

/// graphviz engine adapter within the supramark diagram system.
///
/// This is a zero-sized type (ZST) holding no state. Note that
/// [`GraphvizContext`] is marked `!Send + !Sync` due to Graphviz's global
/// mutable state and cannot be held across threads, whereas [`DiagramEngine`]
/// requires `Send + Sync`. The adapter therefore does NOT cache a context, but
/// creates and immediately destroys a temporary context inside each
/// [`render`](DiagramEngine::render) call -- its creation, use, and Drop all
/// happen within the same thread stack frame of the same call and never move
/// across threads, so the `!Send/!Sync` constraint is not violated.
/// `GraphvizEngine` itself has no fields and automatically satisfies
/// `Send + Sync`.
#[derive(Debug, Default, Clone, Copy)]
pub struct GraphvizEngine;

impl DiagramEngine for GraphvizEngine {
    fn id(&self) -> &'static str {
        "graphviz"
    }

    fn render(&self, source: &str) -> Result<RenderOutput, DiagramError> {
        // Create a fresh context per call to avoid holding the !Send/!Sync global state.
        let ctx = GraphvizContext::new().map_err(|e| DiagramError::Render {
            engine: "graphviz",
            message: format!("create context failed: {e}"),
        })?;

        // Default layout engine dot + SVG output format.
        let bytes = ctx
            .render(source, Engine::Dot, Format::Svg)
            .map_err(|e| DiagramError::Render {
                engine: "graphviz",
                message: e.to_string(),
            })?;

        Ok(RenderOutput::svg(bytes))
    }

    // semantic: not overridden; uses the trait default implementation (returns Ok(None)).
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_minimal_dot_yields_nonempty_svg() {
        let engine = GraphvizEngine;
        assert_eq!(engine.id(), "graphviz");

        let out = engine
            .render("digraph{a->b}")
            .expect("render minimal dot should succeed");

        assert_eq!(out.mime, "image/svg+xml");
        assert!(!out.bytes.is_empty(), "rendered SVG bytes should be non-empty");
    }

    #[test]
    fn semantic_defaults_to_none() {
        let engine = GraphvizEngine;
        assert!(engine
            .semantic("digraph{a->b}")
            .expect("default semantic must not error")
            .is_none());
    }
}
