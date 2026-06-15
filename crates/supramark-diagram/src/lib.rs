//! supramark-diagram: a facade assembling the four engines into a ready-to-use
//! [`DiagramRegistry`].
//!
//! See the design in `docs/architecture/diagram-semantic-ast-impl-plan.md`.
//! This crate is the sole "assembly point" depending on both the trait core and
//! the four engines; `supramark-markdown` stays lightweight and depends on no
//! engine, and downstream code that needs semantics holds this crate's registry
//! and parses on demand.

// Re-export core's trait and types so downstream depends on this crate alone.
pub use supramark_diagram_core::{
    DiagramEngine, DiagramError, DiagramRegistry, EngineAst, RenderOutput,
};

use std::sync::Arc;

/// Build the default registry containing all engines.
///
/// - `mermaid` / `plantuml` / `d2` provide a semantic AST; `graphviz` is
///   render-only (semantics to be added in stage four).
/// - The `dot` and `graphviz` keys point at the SAME graphviz engine instance.
/// - graphviz is registered only on native targets (unavailable on wasm).
pub fn default_registry() -> DiagramRegistry {
    let mut reg = DiagramRegistry::new();
    reg.register(["mermaid"], Arc::new(mermaid_little::MermaidEngine));
    reg.register(["plantuml"], Arc::new(plantuml_little::PlantumlEngine));
    reg.register(["d2"], Arc::new(d2_little::D2Engine));
    #[cfg(not(target_arch = "wasm32"))]
    reg.register(
        ["dot", "graphviz"],
        Arc::new(graphviz_anywhere::GraphvizEngine),
    );
    reg
}
