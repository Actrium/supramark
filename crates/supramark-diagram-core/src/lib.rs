//! supramark-diagram-core: the unified trait and public AST envelope for the
//! four diagram engines.
//!
//! See the design in `docs/architecture/diagram-semantic-ast.md` (§0 decision
//! log) and `diagram-semantic-ast-impl-plan.md`. This crate has zero engine
//! dependencies, so the four engine crates depend on it in reverse and impl
//! [`DiagramEngine`], avoiding a dependency cycle.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;

/// Render output: unified into bytes; SVG text is carried as UTF-8 bytes,
/// leaving room for future formats such as png.
#[derive(Debug, Clone)]
pub struct RenderOutput {
    /// MIME type, e.g. `image/svg+xml`.
    pub mime: &'static str,
    /// Output bytes.
    pub bytes: Vec<u8>,
}

impl RenderOutput {
    /// Convenience constructor for an SVG output.
    pub fn svg(bytes: Vec<u8>) -> Self {
        Self { mime: "image/svg+xml", bytes }
    }
}

/// Public unified semantic AST envelope: across the boundary (serialized to
/// TS / AST v2) it is always `{ engine, kind, data }` (design §0, decision 1).
/// Each engine uses a strongly typed semantic struct internally and converges
/// it into `data` via `serde_json::to_value` in its impl.
#[derive(Debug, Clone, Serialize)]
pub struct EngineAst {
    /// Stable engine identifier: "mermaid" / "plantuml" / "d2" / "graphviz".
    pub engine: &'static str,
    /// Diagram kind discriminant: e.g. d2's "d2", mermaid's "flowchart",
    /// plantuml's "sequence". Downstream picks a diff strategy by `engine` +
    /// `kind`, falling back to source fallback for an unknown kind.
    pub kind: String,
    /// The engine's semantic struct serialized to JSON; its shape evolves with
    /// engine / kind / version.
    pub data: serde_json::Value,
}

impl EngineAst {
    /// Build the envelope from a strongly typed semantic struct; a serialization
    /// failure is turned into a [`DiagramError::Parse`] semantic error.
    pub fn new<T: Serialize>(
        engine: &'static str,
        kind: impl Into<String>,
        data: &T,
    ) -> Result<Self, DiagramError> {
        let data = serde_json::to_value(data).map_err(|e| DiagramError::Parse {
            engine,
            message: format!("serialize semantic failed: {e}"),
        })?;
        Ok(Self { engine, kind: kind.into(), data })
    }
}

/// Unified error: carries the engine name + a discrete category + the original
/// message.
#[derive(Debug, thiserror::Error)]
pub enum DiagramError {
    /// Semantic parsing failed (syntax error etc.).
    #[error("{engine}: parse failed: {message}")]
    Parse { engine: &'static str, message: String },
    /// Rendering failed.
    #[error("{engine}: render failed: {message}")]
    Render { engine: &'static str, message: String },
    /// This engine does not support a semantic AST (e.g. graphviz stage one).
    #[error("{engine}: semantic AST not supported")]
    SemanticUnsupported { engine: &'static str },
}

/// The unified shape of a diagram engine. Object-safe, registerable as
/// `Box<dyn DiagramEngine>`.
pub trait DiagramEngine: Send + Sync {
    /// Stable engine identifier, aligned with markdown's `engine` field.
    fn id(&self) -> &'static str;

    /// source -> render output (a thin wrapper over the existing capability,
    /// always required).
    fn render(&self, source: &str) -> Result<RenderOutput, DiagramError>;

    /// source -> public semantic AST envelope.
    ///
    /// - `Ok(None)`: this engine / diagram kind does not currently support
    ///   semantics (e.g. graphviz stage one).
    /// - `Err(..)`: supported but parsing failed (syntax error).
    ///
    /// The default implementation returns `Ok(None)`, letting a new engine wire
    /// in render-only at zero cost.
    fn semantic(&self, source: &str) -> Result<Option<EngineAst>, DiagramError> {
        let _ = source;
        Ok(None)
    }
}

/// Engine registry: dispatches by the `engine` string.
///
/// Supports pointing multiple keys at the same instance (e.g. `dot` and
/// `graphviz` sharing a single graphviz engine).
#[derive(Default, Clone)]
pub struct DiagramRegistry {
    engines: HashMap<&'static str, Arc<dyn DiagramEngine>>,
}

impl DiagramRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self { engines: HashMap::new() }
    }

    /// Register the same engine instance under one or more keys.
    pub fn register<I>(&mut self, keys: I, engine: Arc<dyn DiagramEngine>)
    where
        I: IntoIterator<Item = &'static str>,
    {
        for k in keys {
            self.engines.insert(k, Arc::clone(&engine));
        }
    }

    /// Get an engine by its id.
    pub fn get(&self, engine_id: &str) -> Option<&dyn DiagramEngine> {
        self.engines.get(engine_id).map(|a| a.as_ref())
    }

    /// Convenience: render directly; returns `None` if the engine is not registered.
    pub fn render(
        &self,
        engine_id: &str,
        source: &str,
    ) -> Option<Result<RenderOutput, DiagramError>> {
        self.get(engine_id).map(|e| e.render(source))
    }

    /// Convenience: get semantics directly; returns `None` if the engine is not registered.
    pub fn semantic(
        &self,
        engine_id: &str,
        source: &str,
    ) -> Option<Result<Option<EngineAst>, DiagramError>> {
        self.get(engine_id).map(|e| e.semantic(source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyEngine;
    impl DiagramEngine for DummyEngine {
        fn id(&self) -> &'static str {
            "dot"
        }
        fn render(&self, _source: &str) -> Result<RenderOutput, DiagramError> {
            Ok(RenderOutput::svg(b"<svg/>".to_vec()))
        }
    }

    #[test]
    fn registry_shares_instance_across_keys() {
        let mut reg = DiagramRegistry::new();
        reg.register(["dot", "graphviz"], Arc::new(DummyEngine));
        assert!(reg.get("dot").is_some());
        assert!(reg.get("graphviz").is_some());
        assert!(reg.get("unknown").is_none());
        // dot / graphviz hit the same instance, so render behaves identically.
        let a = reg.render("dot", "x").unwrap().unwrap();
        let b = reg.render("graphviz", "x").unwrap().unwrap();
        assert_eq!(a.bytes, b.bytes);
    }

    #[test]
    fn default_semantic_is_none() {
        let eng = DummyEngine;
        assert!(eng.semantic("x").unwrap().is_none());
    }

    #[test]
    fn engine_ast_envelope_shape() {
        let ast = EngineAst::new("d2", "d2", &serde_json::json!({"nodes": []})).unwrap();
        let v = serde_json::to_value(&ast).unwrap();
        assert_eq!(v["engine"], "d2");
        assert_eq!(v["kind"], "d2");
        assert!(v["data"]["nodes"].is_array());
    }
}
