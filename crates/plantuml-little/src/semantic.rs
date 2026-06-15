//! Unified diagram-engine adapter: wraps plantuml-little's existing
//! `convert` (render) and `parser::parse` (semantic parse) into the
//! [`DiagramEngine`] trait defined by `supramark-diagram-core`.
//!
//! - `render`: thin wrapper over [`crate::convert`], errors normalized to
//!   [`DiagramError::Render`].
//! - `semantic`: parses into the unified [`Diagram`] enum, uses the current
//!   variant name as `kind`, and serializes the whole semantic tree into
//!   `EngineAst.data`.
//!
//! Note: semantic serialization relies on the model types deriving
//! `serde::Serialize`, which is gated by the `semantic-serde` feature (see
//! Cargo.toml). The concrete `semantic` impl is therefore compiled only when
//! that feature is on; otherwise it falls back to the trait default
//! (`Ok(None)`, "semantic not supported"), so the engine still works as a
//! render-only engine without the feature.

use supramark_diagram_core::{DiagramEngine, DiagramError, RenderOutput};
#[cfg(feature = "semantic-serde")]
use supramark_diagram_core::EngineAst;

/// plantuml engine instance. Stateless; can be registered directly via
/// `Arc::new(PlantumlEngine)` into a `DiagramRegistry`.
pub struct PlantumlEngine;

impl DiagramEngine for PlantumlEngine {
    fn id(&self) -> &'static str {
        "plantuml"
    }

    fn render(&self, source: &str) -> Result<RenderOutput, DiagramError> {
        match crate::convert(source) {
            Ok(svg) => Ok(RenderOutput::svg(svg.into_bytes())),
            Err(e) => Err(DiagramError::Render {
                engine: "plantuml",
                message: e.to_string(),
            }),
        }
    }

    #[cfg(feature = "semantic-serde")]
    fn semantic(&self, source: &str) -> Result<Option<EngineAst>, DiagramError> {
        // Parse failure is normalized to Parse ("syntax error"); on success we
        // take the variant name as kind and serialize the whole Diagram tree
        // into data.
        let diagram = crate::parser::parse(source).map_err(|e| DiagramError::Parse {
            engine: "plantuml",
            message: e.to_string(),
        })?;
        let kind = diagram_kind(&diagram);
        let ast = EngineAst::new("plantuml", kind, &diagram)?;
        Ok(Some(ast))
    }
}

/// Returns the stable lowercase kind string for the current [`Diagram`]
/// variant.
///
/// Alias variants are kept distinct by intent: `Yaml` reuses `JsonDiagram`
/// but reports kind `"yaml"`; `Latex` / `Def` reuse `MathDiagram` but report
/// `"latex"` / `"def"` respectively, so downstream can tell them apart.
#[cfg(feature = "semantic-serde")]
fn diagram_kind(d: &crate::model::diagram::Diagram) -> &'static str {
    use crate::model::diagram::Diagram::*;
    match d {
        Bpm(_) => "bpm",
        Class(_) => "class",
        Sequence(_) => "sequence",
        Activity(_) => "activity",
        State(_) => "state",
        Component(_) => "component",
        Board(_) => "board",
        Chart(_) => "chart",
        Chronology(_) => "chronology",
        Ditaa(_) => "ditaa",
        Erd(_) => "erd",
        Files(_) => "files",
        Flow(_) => "flow",
        Gantt(_) => "gantt",
        Hcl(_) => "hcl",
        Json(_) => "json",
        Mindmap(_) => "mindmap",
        Nwdiag(_) => "nwdiag",
        Pie(_) => "pie",
        Salt(_) => "salt",
        Timing(_) => "timing",
        Wbs(_) => "wbs",
        Yaml(_) => "yaml",
        Dot(_) => "dot",
        UseCase(_) => "usecase",
        Packet(_) => "packet",
        Git(_) => "git",
        Regex(_) => "regex",
        Ebnf(_) => "ebnf",
        Wire(_) => "wire",
        Math(_) => "math",
        Latex(_) => "latex",
        Creole(_) => "creole",
        Def(_) => "def",
    }
}

#[cfg(all(test, feature = "semantic-serde"))]
mod tests {
    use super::*;

    #[test]
    fn semantic_sequence_emits_some_with_kind() {
        let src = "@startuml\nAlice -> Bob: hello\n@enduml\n";
        let ast = PlantumlEngine
            .semantic(src)
            .expect("should parse")
            .expect("should be Some");
        assert_eq!(ast.engine, "plantuml");
        assert_eq!(ast.kind, "sequence");
        assert!(!ast.data.is_null(), "data should be non-null");
    }

    #[test]
    fn semantic_class_emits_some_with_kind() {
        let src = "@startuml\nclass Foo {\n  +bar(): int\n}\n@enduml\n";
        let ast = PlantumlEngine
            .semantic(src)
            .expect("should parse")
            .expect("should be Some");
        assert_eq!(ast.engine, "plantuml");
        assert_eq!(ast.kind, "class");
        assert!(!ast.data.is_null(), "data should be non-null");
    }
}
