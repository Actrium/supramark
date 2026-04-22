//! Flowchart layout — converts a `FlowchartDiagram` AST into a
//! `LayoutData` envelope, hands it to the dagre bridge, and packages
//! the result (nodes + edges + clusters + bounds) into a
//! `FlowchartLayout` struct the renderer can consume.
//!
//! Upstream reference:
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/flowchart/flowRenderer-v3-unified.ts`
//! — which calls `getData()` to build a `data4Layout`, runs
//! `layoutRenderer.render()`, and yields nodes/edges with coordinates.

use crate::error::Result;
use crate::font_metrics;
use crate::layout::unified::{self, Bounds, LayoutData, LayoutResult};
use crate::model::flowchart::{
    ArrowType, ClassDef, Edge as ModelEdge, EdgeStroke, FlowchartDiagram, Label, LabelKind,
    LinkStyle, Vertex,
};
use crate::theme::ThemeVariables;
use std::collections::BTreeMap;

/// Post-layout result.
#[derive(Debug, Clone, Default)]
pub struct FlowchartLayout {
    /// Post-layout nodes (unified::Node).
    pub nodes: Vec<unified::Node>,
    /// Post-layout edges (unified::Edge).
    pub edges: Vec<unified::Edge>,
    /// Post-layout cluster bounds.
    pub clusters: Vec<unified::Cluster>,
    /// Tight AABB over the graph.
    pub bounds: Bounds,
    /// Padding applied around the bounds for the viewBox.
    pub diagram_padding: f64,
    /// `aria-roledescription` — `flowchart-v2` or `flowchart-v1`.
    pub aria_kind: &'static str,
}

/// Font sizing defaults (upstream `flowchart.nodePadding=8, ranksep=50, nodesep=50`).
const NODE_PADDING_X: f64 = 8.0;
const NODE_PADDING_Y: f64 = 8.0;
const DEFAULT_FONT_FAMILY: &str = "trebuchet ms,verdana,arial,sans-serif";
const DEFAULT_FONT_SIZE: f64 = 16.0;

/// Lay out a flowchart diagram. Uses dagre for the graph geometry.
pub fn layout(d: &FlowchartDiagram, theme: &ThemeVariables) -> Result<FlowchartLayout> {
    let layout_data = build_layout_data(d);
    let LayoutResult { nodes, edges, clusters, bounds } =
        unified::layout(&layout_data, "dagre", theme)?;

    Ok(FlowchartLayout {
        nodes,
        edges,
        clusters,
        bounds,
        diagram_padding: 8.0,
        aria_kind: if d.is_v2 { "flowchart-v2" } else { "flowchart-v1" },
    })
}

/// Build a unified `LayoutData` from a flowchart AST.
fn build_layout_data(d: &FlowchartDiagram) -> LayoutData {
    let mut data = LayoutData::default();
    data.diagram_type = Some("flowchart-v2".into());
    data.direction = Some(d.direction.as_str().into());
    data.node_spacing = Some(50.0);
    data.rank_spacing = Some(50.0);
    data.layout_algorithm = Some("dagre".into());

    // Class-def lookup for inline CSS.
    let class_map: BTreeMap<&str, &ClassDef> =
        d.class_defs.iter().map(|c| (c.name.as_str(), c)).collect();

    // Build a parent-id map from subgraph membership.
    let mut parent_of: BTreeMap<String, String> = BTreeMap::new();
    for sg in &d.subgraphs {
        for child in &sg.children {
            parent_of.insert(child.clone(), sg.id.clone());
        }
        for m in &sg.members {
            parent_of.insert(m.clone(), sg.id.clone());
        }
    }

    // Nodes: vertices.
    for v in &d.vertices {
        let shape_id = canon_shape(v.shape.as_deref().unwrap_or("rect"));
        let (w, h) = measure_vertex_box(v);
        let label_text = display_label(v);
        let mut node = unified::Node::default();
        node.id = v.id.clone();
        node.dom_id = Some(flowchart_dom_id(&v.id, v.order));
        node.label = Some(label_text.clone());
        node.label_type = Some(label_kind_string(v.label.as_ref()).to_string());
        node.shape = Some(shape_id.to_string());
        node.width = Some(w);
        node.height = Some(h);
        node.padding = Some(NODE_PADDING_Y.max(NODE_PADDING_X));
        node.look = Some("classic".into());
        node.parent_id = parent_of.get(&v.id).cloned();
        // CSS classes — use the flowchart convention: `"default " + classes`.
        let mut classes = String::from("default");
        for cls in &v.classes {
            classes.push(' ');
            classes.push_str(cls);
        }
        node.css_classes = Some(classes);
        // Inline styles.
        let merged_styles = collect_styles(v, &class_map);
        if !merged_styles.is_empty() {
            node.css_styles = Some(merged_styles);
        }
        node.link = v.link.clone();
        node.link_target = v.link_target.clone();
        node.tooltip = v.tooltip.clone();
        if v.callback.is_some() {
            node.have_callback = Some(true);
        }
        // Rectangle radii (only set for `round`).
        if shape_id == "round" {
            node.rx = Some(5.0);
            node.ry = Some(5.0);
        }
        data.nodes.push(node);
    }

    // Subgraph cluster nodes.
    for sg in &d.subgraphs {
        let (w, h) = measure_subgraph_title_box(sg.title.as_ref());
        let mut node = unified::Node::default();
        node.id = sg.id.clone();
        node.dom_id = Some(format!("flowchart-{}", sg.id));
        node.label = sg.title.as_ref().map(|l| l.text.clone());
        node.shape = Some("rect".into());
        node.width = Some(w);
        node.height = Some(h);
        node.padding = Some(8.0);
        node.is_group = true;
        node.look = Some("classic".into());
        node.dir = sg.dir.map(|d| d.as_str().to_string());
        node.parent_id = parent_of.get(&sg.id).cloned();
        node.css_classes = Some("default".into());
        data.nodes.push(node);
    }

    // Edges. Retarget any edge that points at a subgraph id to the
    // first non-cluster descendant — dagre-rs panics when a compound
    // node is used as an edge endpoint. Upstream mermaid does the
    // equivalent remapping inside `mermaid-graphlib::findNonClusterChild`.
    for e in &d.edges {
        let mut ue = build_edge(e, d);
        retarget_cluster_endpoints(&mut ue, d);
        data.edges.push(ue);
    }

    data
}

fn retarget_cluster_endpoints(ue: &mut unified::Edge, d: &FlowchartDiagram) {
    if let Some(sid) = ue.start.clone() {
        if d.find_subgraph(&sid).is_some() {
            if let Some(child) = first_non_cluster_descendant(&sid, d) {
                ue.start = Some(child);
            }
        }
    }
    if let Some(sid) = ue.end.clone() {
        if d.find_subgraph(&sid).is_some() {
            if let Some(child) = first_non_cluster_descendant(&sid, d) {
                ue.end = Some(child);
            }
        }
    }
}

fn first_non_cluster_descendant(sid: &str, d: &FlowchartDiagram) -> Option<String> {
    let sg = d.find_subgraph(sid)?;
    for m in &sg.members {
        // `members` only holds vertex ids (parser didn't add subgraphs
        // as members), but double-check.
        if d.find_vertex(m).is_some() {
            return Some(m.clone());
        }
    }
    for child in &sg.children {
        if let Some(x) = first_non_cluster_descendant(child, d) {
            return Some(x);
        }
    }
    None
}

/// Map upstream shape aliases to the shape registry's canonical ids.
fn canon_shape(s: &str) -> &'static str {
    match s {
        "square" | "rect" => "rect",
        "round" | "rounded" => "round",
        "stadium" | "pill" => "stadium",
        "subroutine" => "subroutine",
        "cylinder" | "cyl" => "cylinder",
        "circle" | "circ" => "circle",
        "doublecircle" => "doublecircle",
        "ellipse" => "ellipse",
        "diamond" | "question" => "diamond",
        "hexagon" | "hex" => "hexagon",
        "lean_right" | "lean-right" => "lean_right",
        "lean_left" | "lean-left" => "lean_left",
        "trapezoid" | "trap" => "trapezoid",
        "inv_trapezoid" | "invertedTrapezoid" => "inv_trapezoid",
        "odd" => "rect_left_inv_arrow",
        "note" => "note",
        _ => "rect",
    }
}

fn display_label(v: &Vertex) -> String {
    match v.label.as_ref() {
        Some(l) if !l.text.is_empty() => l.text.clone(),
        _ => v.id.clone(),
    }
}

fn label_kind_string(l: Option<&Label>) -> &'static str {
    match l.map(|l| l.kind) {
        Some(LabelKind::Markdown) => "markdown",
        Some(LabelKind::String) => "string",
        _ => "text",
    }
}

/// Measure a vertex's bounding box including its intrinsic shape padding.
fn measure_vertex_box(v: &Vertex) -> (f64, f64) {
    let label = display_label(v);
    let (tw, th) = measure_text(&label);
    // Upstream shape helpers apply their own padding on top of the
    // label bbox. For a rough, common baseline:
    //   rect: +16, +16
    //   round: +16, +16 (rx/ry adds little)
    //   stadium: +label_height, +0  (wider to fit the semicircle caps)
    //   diamond: label_w*√2, label_h*√2  (then outer octagonal)
    //   hexagon: tw + m_x, h + pad
    //   cylinder: +16, +24 (top/bottom arcs)
    //   circle: max(tw, th) + 32
    //   doublecircle: max(tw, th) + 48
    let shape = v.shape.as_deref().unwrap_or("rect");
    let (pad_x, pad_y) = match shape {
        "circle" | "circ" => {
            let d = tw.max(th) + 32.0;
            return (d, d);
        }
        "doublecircle" => {
            let d = tw.max(th) + 48.0;
            return (d, d);
        }
        "diamond" | "question" => {
            let s = (tw.powi(2) + th.powi(2)).sqrt() + 24.0;
            return (s, s);
        }
        "hexagon" | "hex" => (24.0, 8.0),
        "stadium" | "pill" => (th + 16.0, 16.0),
        "cylinder" | "cyl" => (16.0, 24.0),
        "subroutine" => (32.0, 16.0),
        "trapezoid" | "trap" | "inv_trapezoid" | "invertedTrapezoid" | "lean_left" | "lean-left"
        | "lean_right" | "lean-right" => (32.0, 16.0),
        _ => (16.0, 16.0),
    };
    (tw + pad_x, th + pad_y)
}

/// Measure the overall width/height of the (possibly multi-line) label.
fn measure_text(label: &str) -> (f64, f64) {
    if label.is_empty() {
        return (0.0, DEFAULT_FONT_SIZE);
    }
    let lines: Vec<&str> = label.split("<br/>").flat_map(|s| s.split('\n')).collect();
    let mut max_w = 0.0f64;
    for line in &lines {
        let w = font_metrics::text_width(line, DEFAULT_FONT_FAMILY, DEFAULT_FONT_SIZE, false, false);
        if w > max_w {
            max_w = w;
        }
    }
    let lh = font_metrics::line_height(DEFAULT_FONT_FAMILY, DEFAULT_FONT_SIZE, false, false);
    (max_w, lh * lines.len() as f64)
}

fn measure_subgraph_title_box(title: Option<&Label>) -> (f64, f64) {
    let text = title.map(|l| l.text.as_str()).unwrap_or("");
    let (w, h) = measure_text(text);
    (w + 16.0, h + 16.0)
}

/// Build a unified::Edge from a model Edge, applying link-style overrides.
fn build_edge(e: &ModelEdge, d: &FlowchartDiagram) -> unified::Edge {
    let mut ue = unified::Edge::default();
    ue.id = format!("L_{}_{}_{}", e.start, e.end, e.index);
    ue.start = Some(e.start.clone());
    ue.end = Some(e.end.clone());
    ue.minlen = Some(e.length as i32);
    ue.label = e.label.as_ref().map(|l| l.text.clone());
    ue.label_type = Some(label_kind_string(e.label.as_ref()).to_string());
    ue.arrow_type_end = Some(arrow_kind_string(e.arrow_end).to_string());
    ue.arrow_type_start = Some(arrow_kind_string(e.arrow_start).to_string());
    let (thickness, pattern) = stroke_descriptor(e.stroke);
    ue.thickness = Some(thickness.into());
    ue.pattern = Some(pattern.into());
    ue.stroke = Some(thickness.into());
    ue.interpolate = Some("basis".into());
    ue.curve = Some("basis".into());

    // Apply link-style overrides.
    let mut applied_styles: Vec<String> = Vec::new();
    let mut interpolate: Option<String> = None;
    for ls in &d.link_styles {
        if apply_link_style(ls, e.index) {
            for s in &ls.styles {
                applied_styles.push(s.clone());
            }
            if let Some(i) = &ls.interpolate {
                interpolate = Some(i.clone());
            }
        }
    }
    if !applied_styles.is_empty() {
        ue.style = Some(applied_styles);
    }
    if let Some(i) = interpolate {
        ue.interpolate = Some(i.clone());
        ue.curve = Some(i);
    }
    ue.look = Some("classic".into());
    ue
}

fn apply_link_style(ls: &LinkStyle, idx: usize) -> bool {
    ls.is_default || ls.indices.iter().any(|&i| i == idx)
}

fn arrow_kind_string(a: ArrowType) -> &'static str {
    match a {
        ArrowType::None => "none",
        ArrowType::Arrow => "arrow_point",
        ArrowType::Circle => "arrow_circle",
        ArrowType::Cross => "arrow_cross",
        ArrowType::Point => "arrow_point",
    }
}

fn stroke_descriptor(s: EdgeStroke) -> (&'static str, &'static str) {
    match s {
        EdgeStroke::Normal => ("normal", "solid"),
        EdgeStroke::Thick => ("thick", "solid"),
        EdgeStroke::Dotted => ("normal", "dotted"),
        EdgeStroke::Invisible => ("invisible", "solid"),
    }
}

/// Compose styles from classDef + inline styles. Returns `Vec<String>`
/// of `"key:value"` entries.
fn collect_styles<'a>(
    v: &'a Vertex,
    class_map: &BTreeMap<&'a str, &'a ClassDef>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for cls in &v.classes {
        if let Some(cd) = class_map.get(cls.as_str()) {
            out.extend(cd.styles.iter().cloned());
        }
    }
    out.extend(v.styles.iter().cloned());
    out
}

/// Compose the DOM id mermaid uses for a flowchart node:
/// `flowchart-<id>-<order>`. Upstream dedupes and coalesces this on
/// per-render basis — the order int is globally unique.
fn flowchart_dom_id(id: &str, order: usize) -> String {
    format!("flowchart-{}-{}", id, order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::flowchart as fcp;

    #[test]
    fn layout_minimal_two_node_graph() {
        let src = "flowchart TD\nA --> B\n";
        let d = fcp::parse(src).unwrap();
        let theme = ThemeVariables::default();
        let l = layout(&d, &theme).unwrap();
        assert_eq!(l.nodes.len(), 2);
        assert_eq!(l.edges.len(), 1);
        let a = l.nodes.iter().find(|n| n.id == "A").unwrap();
        assert!(a.x.is_some() && a.y.is_some());
    }

    #[test]
    fn layout_subgraph_creates_cluster() {
        let src = "flowchart TD\nsubgraph s1 [Title]\n  A-->B\nend\nA-->C\n";
        let d = fcp::parse(src).unwrap();
        let theme = ThemeVariables::default();
        let l = layout(&d, &theme).unwrap();
        assert!(l.clusters.iter().any(|c| c.id == "s1"));
        // members must have their parent_id set
        let a = l.nodes.iter().find(|n| n.id == "A").unwrap();
        assert_eq!(a.parent_id.as_deref(), Some("s1"));
    }

    #[test]
    fn layout_lr_direction_flows_horizontally() {
        let src = "flowchart LR\nA-->B\n";
        let d = fcp::parse(src).unwrap();
        let theme = ThemeVariables::default();
        let l = layout(&d, &theme).unwrap();
        let a = l.nodes.iter().find(|n| n.id == "A").unwrap();
        let b = l.nodes.iter().find(|n| n.id == "B").unwrap();
        assert!(b.x.unwrap() > a.x.unwrap());
    }
}
