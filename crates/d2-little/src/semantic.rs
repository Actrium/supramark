//! D2 semantic projection: projects the pre-layout `graph::Graph` into a
//! coordinate-independent, stable structure.
//!
//! Design notes: keep only the "semantic" fields (node identity, label, shape,
//! style, parent/child relationships, edge endpoints and labels) and
//! deliberately drop all layout geometry (top_left / width / height / box_ /
//! route etc.). This way, when the source is unchanged and coordinates shift
//! only slightly due to the layout algorithm, the projection stays stable and
//! does not produce spurious diffs.

use serde::Serialize;

use crate::graph::{Edge, Graph, Object, ObjId, Style};

/// Semantic style subset of a single node.
///
/// Only style keys that affect "semantic/visual intent" are recorded, all kept
/// as their raw string values; no coordinates or sizes. A `None` field is
/// skipped during serialization to keep the output compact.
#[derive(Debug, Clone, Default, Serialize)]
pub struct D2Style {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke_dash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<String>,
}

impl D2Style {
    /// Extract the semantic subset from a graph `Style`. All values are the raw
    /// strings from the DSL.
    fn from_style(s: &Style) -> Self {
        let v = |o: &Option<crate::graph::ScalarValue>| o.as_ref().map(|x| x.value.clone());
        D2Style {
            fill: v(&s.fill),
            stroke: v(&s.stroke),
            opacity: v(&s.opacity),
            stroke_width: v(&s.stroke_width),
            stroke_dash: v(&s.stroke_dash),
            border_radius: v(&s.border_radius),
            font_color: v(&s.font_color),
            font_size: v(&s.font_size),
            bold: v(&s.bold),
            italic: v(&s.italic),
            underline: v(&s.underline),
        }
    }

    /// Whether it is fully empty -- used to decide whether to omit the `style`
    /// field in the output.
    fn is_empty(&self) -> bool {
        self.fill.is_none()
            && self.stroke.is_none()
            && self.opacity.is_none()
            && self.stroke_width.is_none()
            && self.stroke_dash.is_none()
            && self.border_radius.is_none()
            && self.font_color.is_none()
            && self.font_size.is_none()
            && self.bold.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
    }
}

/// Semantic projection of a single node.
///
/// Uses `abs_id` (the globally unique absolute path, e.g. `g.a`) as node
/// identity; parent/child relationships are also expressed via `abs_id` rather
/// than internal array indices, making the projection insensitive to internal
/// storage order. Excludes layout coordinates such as top_left / width /
/// height / box_.
#[derive(Debug, Clone, Serialize)]
pub struct D2Node {
    /// Node absolute ID (globally unique path).
    pub id: String,
    /// Node display label; meaningful when it differs from id, by default often
    /// equal to id.
    pub label: String,
    /// Shape (e.g. rectangle / circle etc.), taken as the raw DSL value.
    pub shape: String,
    /// Parent node abs_id; `None` for top-level nodes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// List of direct child abs_ids (in graph order).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    /// Semantic style subset; omitted when empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<D2Style>,
}

/// Semantic projection of a single edge.
///
/// Endpoints are expressed via `abs_id`; excludes layout info such as route
/// (polyline coordinates).
#[derive(Debug, Clone, Serialize)]
pub struct D2Edge {
    /// Source node abs_id.
    pub src: String,
    /// Target node abs_id.
    pub dst: String,
    /// Edge label; empty string when there is no label.
    pub label: String,
}

/// Stable semantic projection of a d2 graph: a node set plus an edge set.
#[derive(Debug, Clone, Serialize)]
pub struct D2Semantic {
    pub nodes: Vec<D2Node>,
    pub edges: Vec<D2Edge>,
}

/// Resolve an ObjId to its abs_id string; out-of-bounds returns an empty string
/// for robustness.
fn abs_id_of(g: &Graph, id: ObjId) -> String {
    g.objects.get(id).map(|o| o.abs_id.clone()).unwrap_or_default()
}

/// Project a single `Object` into a `D2Node` (semantic fields only).
fn project_node(g: &Graph, obj: &Object) -> D2Node {
    let style = D2Style::from_style(&obj.style);
    let children = obj
        .children
        .iter()
        .map(|&c| abs_id_of(g, c))
        .filter(|s| !s.is_empty())
        .collect();
    D2Node {
        id: obj.abs_id.clone(),
        label: obj.label.value.clone(),
        shape: obj.shape.value.clone(),
        parent: obj.parent.and_then(|p| {
            let s = abs_id_of(g, p);
            // The root's abs_id is empty; treat it as "no parent".
            if s.is_empty() { None } else { Some(s) }
        }),
        children,
        style: if style.is_empty() { None } else { Some(style) },
    }
}

/// Project a single `Edge` into a `D2Edge`.
fn project_edge(g: &Graph, e: &Edge) -> D2Edge {
    D2Edge {
        src: abs_id_of(g, e.src),
        dst: abs_id_of(g, e.dst),
        label: e.label.value.clone(),
    }
}

/// Project a pre-layout `Graph` into a coordinate-independent `D2Semantic`.
///
/// Skips the root node at index 0 (its abs_id is empty, serving only as an
/// internal container).
pub fn project(g: &Graph) -> D2Semantic {
    let nodes = g
        .objects
        .iter()
        .filter(|o| !o.abs_id.is_empty())
        .map(|o| project_node(g, o))
        .collect();
    let edges = g.edges.iter().map(|e| project_edge(g, e)).collect();
    D2Semantic { nodes, edges }
}

// ---------------------------------------------------------------------------
// DiagramEngine implementation
// ---------------------------------------------------------------------------

use supramark_diagram_core::{
    DiagramEngine, DiagramError, EngineAst, RenderOutput,
};

/// d2 diagram engine: wires into supramark's unified `DiagramEngine` abstraction.
pub struct D2Engine;

impl DiagramEngine for D2Engine {
    fn id(&self) -> &'static str {
        "d2"
    }

    /// Render to SVG. Reuses [`crate::d2_to_svg`] underneath.
    fn render(&self, source: &str) -> Result<RenderOutput, DiagramError> {
        let bytes = crate::d2_to_svg(source).map_err(|message| DiagramError::Render {
            engine: "d2",
            message,
        })?;
        Ok(RenderOutput::svg(bytes))
    }

    /// Semantic projection: compile to the pre-layout Graph, then project into a
    /// coordinate-independent `D2Semantic`.
    fn semantic(&self, source: &str) -> Result<Option<EngineAst>, DiagramError> {
        let (g, _config) =
            crate::compiler::compile_with_config("", source).map_err(|e| DiagramError::Parse {
                engine: "d2",
                message: format!("{}", e),
            })?;
        let proj = project(&g);
        let ast = EngineAst::new("d2", "d2", &proj)?;
        Ok(Some(ast))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use supramark_diagram_core::DiagramEngine;

    #[test]
    fn semantic_simple_edge_has_nodes_and_edges() {
        let ast = D2Engine
            .semantic("a -> b")
            .expect("semantic should not error")
            .expect("d2 should return Some");
        assert_eq!(ast.engine, "d2");
        assert_eq!(ast.kind, "d2");

        let nodes = ast.data.get("nodes").and_then(|v| v.as_array()).unwrap();
        let edges = ast.data.get("edges").and_then(|v| v.as_array()).unwrap();
        assert!(nodes.len() >= 2, "should project at least the two nodes a and b");
        assert_eq!(edges.len(), 1, "should project a single a -> b edge");

        let edge = &edges[0];
        assert_eq!(edge.get("src").unwrap().as_str().unwrap(), "a");
        assert_eq!(edge.get("dst").unwrap().as_str().unwrap(), "b");
    }

    #[test]
    fn semantic_json_excludes_layout_fields() {
        let ast = D2Engine
            .semantic("a -> b: hello")
            .unwrap()
            .unwrap();
        let json = serde_json::to_string(&ast.data).unwrap();
        // Layout coordinate keys must not appear in the projection.
        for forbidden in ["top_left", "topLeft", "width", "height", "box_", "route"] {
            assert!(
                !json.contains(forbidden),
                "projection JSON should not contain layout key {forbidden}: {json}"
            );
        }
    }
}
