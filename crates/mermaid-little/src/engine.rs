//! Implementation of `supramark-diagram-core::DiagramEngine` for the mermaid engine.
//!
//! - `render`: a thin wrapper over the existing [`crate::convert_with_id`],
//!   mapping `MermaidError` into the unified [`DiagramError::Render`], with the
//!   output returned as SVG bytes.
//! - `semantic`: returns `Ok(Some(EngineAst))` only for the four diagram kinds
//!   with semantics wired up (er / flowchart / sequence / class), and `Ok(None)`
//!   otherwise; syntax errors are mapped into [`DiagramError::Parse`].

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use supramark_diagram_core::{DiagramEngine, DiagramError, EngineAst, RenderOutput};

#[cfg(feature = "semantic-serde")]
use crate::semantic::{self, SemanticError};

/// Stable engine identifier.
const ENGINE_ID: &str = "mermaid";

/// mermaid engine unit struct -- no internal state, can be `Arc::new`'d and
/// registered directly.
#[derive(Debug, Default, Clone, Copy)]
pub struct MermaidEngine;

impl MermaidEngine {
    /// Derive a stable `<svg id>` from the source content.
    ///
    /// `convert_with_id` uses this id as the root `<svg id>` and threads it
    /// through CSS selectors; to keep the render of the same source stable, it
    /// uses a hash of the source rather than a random/incrementing value.
    fn stable_id(source: &str) -> String {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        format!("mermaid-{:x}", hasher.finish())
    }
}

impl DiagramEngine for MermaidEngine {
    fn id(&self) -> &'static str {
        ENGINE_ID
    }

    fn render(&self, source: &str) -> Result<RenderOutput, DiagramError> {
        let id = Self::stable_id(source);
        let svg = crate::convert_with_id(source, &id).map_err(|e| DiagramError::Render {
            engine: ENGINE_ID,
            message: e.to_string(),
        })?;
        Ok(RenderOutput::svg(svg.into_bytes()))
    }

    fn semantic(&self, source: &str) -> Result<Option<EngineAst>, DiagramError> {
        // The semantic AST depends on `MermaidAst: Serialize`, whose derive is
        // gated by the feature `semantic-serde`. When disabled, this engine is
        // render-only and semantics uniformly fall back to `Ok(None)` (aligned
        // with core's default implementation semantics).
        #[cfg(not(feature = "semantic-serde"))]
        {
            let _ = source;
            Ok(None)
        }
        #[cfg(feature = "semantic-serde")]
        {
            match semantic::parse_semantic(source) {
                Ok(ast) => {
                    let kind = ast.kind_str();
                    // EngineAst::new internally serializes the semantic struct via
                    // serde_json::to_value; failures are mapped to
                    // DiagramError::Parse (see core impl).
                    let env = EngineAst::new(ENGINE_ID, kind, &ast)?;
                    Ok(Some(env))
                }
                // This diagram kind does not currently support a semantic AST -> Ok(None).
                Err(SemanticError::Unsupported(_)) => Ok(None),
                // Supported kind but parsing failed (syntax error) -> mapped to Parse.
                Err(SemanticError::Parse(e)) => Err(DiagramError::Parse {
                    engine: ENGINE_ID,
                    message: e.to_string(),
                }),
            }
        }
    }
}

#[cfg(all(test, feature = "semantic-serde", feature = "metrics-ttf-parser"))]
mod tests {
    use super::*;
    use supramark_diagram_core::DiagramEngine;

    // One minimal source per supported kind, asserting semantic yields Some,
    // that the envelope engine/kind are correct, and that data contains the
    // expected node info.

    #[test]
    fn semantic_er_has_entities() {
        let src = "erDiagram\n    CUSTOMER ||--o{ ORDER : places\n";
        let env = MermaidEngine.semantic(src).unwrap().expect("er should have semantics");
        assert_eq!(env.engine, "mermaid");
        assert_eq!(env.kind, "er");
        let data = serde_json::to_value(&env).unwrap();
        // ErDiagram is nested under the Er variant; an externally-tagged enum looks like {"Er": {...}}.
        let er = &data["data"]["Er"];
        let keys = er["entity_keys"].as_array().unwrap();
        assert!(keys.iter().any(|k| k == "CUSTOMER"));
        assert!(keys.iter().any(|k| k == "ORDER"));
    }

    #[test]
    fn semantic_flowchart_has_vertices() {
        let src = "flowchart TD\n    A[Start] --> B[End]\n";
        let env = MermaidEngine
            .semantic(src)
            .unwrap()
            .expect("flowchart should have semantics");
        assert_eq!(env.kind, "flowchart");
        let data = serde_json::to_value(&env).unwrap();
        let fc = &data["data"]["Flowchart"];
        let ids: Vec<String> = fc["vertices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap().to_string())
            .collect();
        assert!(ids.contains(&"A".to_string()));
        assert!(ids.contains(&"B".to_string()));
        assert_eq!(fc["edges"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn semantic_sequence_has_actors() {
        let src = "sequenceDiagram\n    Alice->>Bob: Hello\n";
        let env = MermaidEngine
            .semantic(src)
            .unwrap()
            .expect("sequence should have semantics");
        assert_eq!(env.kind, "sequence");
        let data = serde_json::to_value(&env).unwrap();
        let seq = &data["data"]["Sequence"];
        let actor_ids: Vec<String> = seq["actors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["id"].as_str().unwrap().to_string())
            .collect();
        assert!(actor_ids.contains(&"Alice".to_string()));
        assert!(actor_ids.contains(&"Bob".to_string()));
    }

    #[test]
    fn semantic_class_has_classes() {
        let src = "classDiagram\n    class Animal\n    Animal <|-- Dog\n";
        let env = MermaidEngine
            .semantic(src)
            .unwrap()
            .expect("class should have semantics");
        assert_eq!(env.kind, "class");
        let data = serde_json::to_value(&env).unwrap();
        let cls = &data["data"]["Class"];
        let ids: Vec<String> = cls["classes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap().to_string())
            .collect();
        assert!(ids.contains(&"Animal".to_string()));
        assert!(ids.contains(&"Dog".to_string()));
    }

    #[test]
    fn semantic_unsupported_kind_is_none() {
        // pie has no semantics wired up -> Ok(None), the render-only path still works.
        let src = "pie\n    \"A\" : 10\n    \"B\" : 20\n";
        assert!(MermaidEngine.semantic(src).unwrap().is_none());
    }

    #[test]
    fn render_produces_svg_bytes() {
        let src = "flowchart TD\n    A --> B\n";
        let out = MermaidEngine.render(src).unwrap();
        assert_eq!(out.mime, "image/svg+xml");
        let svg = String::from_utf8(out.bytes).unwrap();
        assert!(svg.contains("<svg"));
    }
}
