//! JSON serialization and deserialization for graphs.
//!
//! Uses serde for (de)serialization. Node and edge labels must implement
//! Serialize/Deserialize.

use super::{Graph, GraphOptions};
use serde::{Deserialize, Serialize};

/// JSON representation of a graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonGraph<N, E, G> {
    pub options: JsonGraphOptions,
    pub nodes: Vec<JsonNode<N>>,
    pub edges: Vec<JsonEdge<E>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<G>,
}

/// Graph options for JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonGraphOptions {
    pub directed: bool,
    pub multigraph: bool,
    pub compound: bool,
}

/// JSON representation of a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonNode<N> {
    pub v: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<N>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

/// JSON representation of an edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonEdge<E> {
    pub v: String,
    pub w: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<E>,
}

/// Serialize a graph to its JSON representation.
pub fn graph_to_json<N: Clone + Serialize, E: Clone + Serialize, G: Clone + Serialize>(
    g: &Graph<N, E>,
    graph_label: Option<&G>,
) -> JsonGraph<N, E, G> {
    let nodes = g
        .nodes()
        .iter()
        .map(|v| {
            let value = g.node(v).cloned();
            let parent = if g.is_compound() {
                g.parent(v).map(|p| p.to_string())
            } else {
                None
            };
            JsonNode {
                v: v.clone(),
                value,
                parent,
            }
        })
        .collect();

    let edges = g
        .edges()
        .iter()
        .map(|e| {
            let value = g.edge(&e.v, &e.w, e.name.as_deref()).cloned();
            JsonEdge {
                v: e.v.clone(),
                w: e.w.clone(),
                name: e.name.clone(),
                value,
            }
        })
        .collect();

    JsonGraph {
        options: JsonGraphOptions {
            directed: g.is_directed(),
            multigraph: g.is_multigraph(),
            compound: g.is_compound(),
        },
        nodes,
        edges,
        value: graph_label.cloned(),
    }
}

/// Deserialize a graph from its JSON representation.
pub fn graph_from_json<N: Clone, E: Clone, G>(
    json: JsonGraph<N, E, G>,
) -> (Graph<N, E>, Option<G>) {
    let mut g = Graph::with_options(GraphOptions {
        directed: json.options.directed,
        multigraph: json.options.multigraph,
        compound: json.options.compound,
    });

    // First pass: create nodes
    for node in &json.nodes {
        g.set_node(node.v.clone(), node.value.clone());
    }

    // Second pass: set parents (compound graphs)
    if json.options.compound {
        for node in &json.nodes {
            if let Some(parent) = &node.parent {
                g.set_parent(&node.v, Some(parent.as_str()));
            }
        }
    }

    // Create edges
    for edge in json.edges {
        g.set_edge(edge.v, edge.w, edge.value, edge.name.as_deref());
    }

    (g, json.value)
}
