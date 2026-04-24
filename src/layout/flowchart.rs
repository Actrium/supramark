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
    /// `aria-roledescription` — derived from the header keyword:
    /// `flowchart-elk`, `flowchart-v2`, or `flowchart-v1`.
    pub aria_kind: String,
}

/// Font sizing defaults (upstream `flowchart.nodePadding=8, ranksep=50, nodesep=50`).
const NODE_PADDING_X: f64 = 8.0;
const NODE_PADDING_Y: f64 = 8.0;
const DEFAULT_FONT_FAMILY: &str = "trebuchet ms,verdana,arial,sans-serif";
/// Upstream's `labelHelper` uses `div.getBoundingClientRect()` on the
/// foreignObject HTML label, which inherits 14 px sans-serif from the
/// SVG root — NOT the theme fontSize (16 px). Using 14 px here makes
/// dagre assign the same node dimensions as upstream.
const LABEL_FONT_SIZE: f64 = 14.0;
/// Upstream `config.flowchart?.padding` default (from config.schema.yaml).
/// Used by shape functions to compute the total node size around the
/// label bounding box:
/// - rect (squareRect): labelPaddingX = padding * 2, labelPaddingY = padding
/// - round (roundedRect): labelPaddingX = padding, labelPaddingY = padding
/// - diamond: s = (labelW + padding) + (labelH + padding)
const FLOWCHART_PADDING: f64 = 15.0;

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
        // Upstream always uses "flowchart-v2" for the aria-roledescription,
        // even for diagrams that start with the `graph` keyword. Only
        // flowchart-elk gets its own label.
        aria_kind: if d.header_keyword == "flowchart-elk" {
            "flowchart-elk".to_string()
        } else {
            "flowchart-v2".to_string()
        },
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

    // Set of subgraph IDs — used to skip vertices that are actually subgraph
    // references (e.g. `B` inside `subgraph A` when `B` is itself a subgraph).
    let subgraph_ids: std::collections::HashSet<&str> =
        d.subgraphs.iter().map(|sg| sg.id.as_str()).collect();

    // Nodes: vertices.
    for v in &d.vertices {
        // Skip vertices whose ID matches a subgraph — they are cluster references,
        // not standalone nodes, and will be rendered as clusters.
        if subgraph_ids.contains(v.id.as_str()) {
            continue;
        }
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
        node.padding = Some(FLOWCHART_PADDING);
        node.look = Some("classic".into());
        node.parent_id = parent_of.get(&v.id).cloned();
        // CSS classes — upstream: `'default ' + vertex.classes.join(' ')`.
        // The trailing space after "default" is intentional — it produces
        // the double-space before the closing quote in `getNodeClasses`.
        let mut classes = String::from("default");
        for cls in &v.classes {
            classes.push(' ');
            classes.push_str(cls);
        }
        // Always append trailing space — even when no extra classes.
        classes.push(' ');
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
        // Upstream cluster DOM id is just the subgraph id — no "flowchart-" prefix.
        // render_cluster prepends the SVG element id when emitting.
        node.dom_id = Some(sg.id.clone());
        node.label = sg.title.as_ref().map(|l| l.text.clone());
        node.shape = Some("rect".into());
        node.width = Some(w);
        node.height = Some(h);
        node.padding = Some(8.0);
        node.is_group = true;
        node.look = Some("classic".into());
        node.dir = sg.dir.map(|d| d.as_str().to_string());
        node.parent_id = parent_of.get(&sg.id).cloned();
        // Cluster CSS class: empty string so render_cluster emits `class="cluster "`.
        node.css_classes = None;
        // `style <subgraph-id> ...` directives land on the matching Vertex (if any)
        // because the parser calls `ensure_vertex` on the id. Apply those styles here.
        if let Some(sv) = d.find_vertex(&sg.id) {
            let merged = collect_styles(sv, &class_map);
            if !merged.is_empty() {
                node.css_styles = Some(merged);
            }
        }
        data.nodes.push(node);
    }

    // Edges. Retarget any edge that points at a subgraph id to the
    // first non-cluster descendant — dagre-rs panics when a compound
    // node is used as an edge endpoint. Upstream mermaid does the
    // equivalent remapping inside `mermaid-graphlib::findNonClusterChild`.
    // Upstream edge IDs use a per-pair counter (see `getEdgeId`):
    //   L_{start}_{end}_0 for the first edge between a pair,
    //   L_{start}_{end}_1 for the second, etc.
    use std::collections::HashMap;
    let mut pair_count: HashMap<(String, String), usize> = HashMap::new();
    for e in &d.edges {
        let start = e.start.clone();
        let end = e.end.clone();
        let counter = *pair_count.entry((start.clone(), end.clone())).and_modify(|c| *c += 1).or_insert(0);
        let mut ue = build_edge(e, d, counter);
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
/// These padding values must match what the upstream shape renderers
/// compute at draw time, so that dagre assigns the correct node
/// dimensions.
fn measure_vertex_box(v: &Vertex) -> (f64, f64) {
    let label = display_label(v);
    let (tw, th) = measure_text(&label);
    // Upstream shape helpers compute total size from the label bbox
    // plus per-shape padding. The `node.padding` config default is 15.
    //
    // squareRect: totalW = bbox.w + padding*4, totalH = bbox.h + padding*2
    //   (labelPaddingX = padding*2, applied twice = padding*4)
    //   (labelPaddingY = padding, applied twice = padding*2)
    // roundedRect: totalW = bbox.w + padding*2, totalH = bbox.h + padding*2
    // diamond: s = (bbox.w + padding) + (bbox.h + padding)
    // hexagon: uses nodePadding directly
    // stadium: wider by label_height
    // cylinder: extra 24 for arcs
    // circle: max(tw,th) + 32
    // doublecircle: max(tw,th) + 48
    let shape = v.shape.as_deref().unwrap_or("rect");
    let p = FLOWCHART_PADDING;
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
            let w = tw + p;
            let h = th + p;
            let s = w + h;
            return (s, s);
        }
        "hexagon" | "hex" => (p * 4.0, p * 2.0),
        "stadium" | "pill" => (th + p * 2.0, p * 2.0),
        "cylinder" | "cyl" => (p * 2.0, p * 2.0 + 24.0),
        "subroutine" => (p * 4.0, p * 2.0),
        "trapezoid" | "trap" | "inv_trapezoid" | "invertedTrapezoid" | "lean_left" | "lean-left"
        | "lean_right" | "lean-right" => (p * 4.0, p * 2.0),
        "round" | "rounded" => (p * 2.0, p * 2.0),
        _ => (p * 4.0, p * 2.0), // rect / squareRect: labelPaddingX = p*2, ×2 sides = p*4
    };
    (tw + pad_x, th + pad_y)
}

/// Strip FontAwesome icon prefixes from a label string before measurement.
/// Upstream replaces `fa:fa-car` with `<i class="fa fa-car"></i>` at render
/// time; the `<i>` element contributes negligible width under the jsdom shim,
/// so we remove those tokens before measuring text width.
fn strip_fa_icons(text: &str) -> String {
    // Match patterns like `fa:fa-car`, `fas:fa-spinner`, `fab:fa-github`, etc.
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("fa") {
        // Check whether this starts a "fa[bklrs]?:fa-<name>" sequence.
        let tail = &rest[pos..];
        // Find the colon.
        let prefix_end = tail.find(':').unwrap_or(tail.len());
        let prefix = &tail[..prefix_end];
        // Valid FA prefixes: fa, fab, fak, fal, far, fas
        let valid_prefix = matches!(prefix, "fa" | "fab" | "fak" | "fal" | "far" | "fas");
        if valid_prefix && tail[prefix_end + 1..].starts_with("fa-") {
            // Consume leading text up to this match.
            out.push_str(&rest[..pos]);
            // Skip past "prefix:fa-name" where name is [a-z0-9-]+.
            let icon_tail = &tail[prefix_end + 1 + 3..]; // after "fa-"
            let icon_end = icon_tail.find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                .unwrap_or(icon_tail.len());
            rest = &rest[pos + prefix_end + 1 + 3 + icon_end..];
        } else {
            // Not a valid FA token — emit up to and including "fa" and move on.
            out.push_str(&rest[..pos + 2]);
            rest = &rest[pos + 2..];
        }
    }
    out.push_str(rest);
    out
}

/// Strip HTML tags from label text for width measurement.
/// Handles `<br>` / `<br/>` as line breaks and tracks bold state via
/// `<strong>` / `<b>` tags. Returns a list of (text, bold) line segments.
fn strip_html_for_measure(s: &str) -> Vec<(String, bool)> {
    let mut lines: Vec<(String, bool)> = vec![(String::new(), false)];
    let mut bold = false;
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Find closing '>'
            let end = s[i..].find('>').map(|n| i + n + 1).unwrap_or(s.len());
            let tag = &s[i + 1..end - 1].trim_start_matches('/').to_ascii_lowercase();
            let tag_lc = tag.trim();
            match tag_lc {
                "br" | "br/" => {
                    lines.push((String::new(), bold));
                }
                "strong" | "b" => bold = true,
                "/strong" | "/b" => bold = false,
                _ => {}
            }
            i = end;
        } else {
            let line = lines.last_mut().unwrap();
            line.0.push(bytes[i] as char);
            i += 1;
        }
    }
    lines
}

/// Measure the overall width/height of the (possibly multi-line) label.
/// Handles HTML labels (containing tags like `<strong>`, `<br/>`) by
/// stripping tags and measuring the visible text content only.
fn measure_text(label: &str) -> (f64, f64) {
    if label.is_empty() {
        return (0.0, LABEL_FONT_SIZE);
    }
    // Strip FA icon tokens first — they render as <i> elements with no width.
    let stripped = strip_fa_icons(label);
    let lh = font_metrics::line_height(DEFAULT_FONT_FAMILY, LABEL_FONT_SIZE, false, false);

    // Check whether the label contains HTML tags.
    if stripped.contains('<') {
        // HTML label — strip tags, treat <br> as line break, <strong>/<b> as bold.
        let segments = strip_html_for_measure(&stripped);
        let line_count = segments.len().max(1) as f64;
        let max_w = segments.iter().map(|(text, bold)| {
            font_metrics::text_width(text, DEFAULT_FONT_FAMILY, LABEL_FONT_SIZE, *bold, false)
        }).fold(0.0f64, f64::max);
        return (max_w, lh * line_count);
    }

    let lines: Vec<&str> = stripped.split("<br/>").flat_map(|s| s.split('\n')).collect();
    let mut max_w = 0.0f64;
    for line in &lines {
        let w = font_metrics::text_width(line, DEFAULT_FONT_FAMILY, LABEL_FONT_SIZE, false, false);
        if w > max_w {
            max_w = w;
        }
    }
    (max_w, lh * lines.len() as f64)
}

fn measure_subgraph_title_box(title: Option<&Label>) -> (f64, f64) {
    let text = title.map(|l| l.text.as_str()).unwrap_or("");
    let (w, h) = measure_text(text);
    (w + 16.0, h + 16.0)
}

/// Measure edge label dimensions to match the foreignObject rendered at runtime.
/// Upstream edge labels use the jsdom default font: sans-serif 14px non-bold,
/// which differs from the node-label font (trebuchet ms 14px).
fn measure_edge_label(text: &str) -> (f64, f64) {
    const EDGE_LABEL_FONT: &str = "sans-serif";
    const EDGE_LABEL_SIZE: f64 = 14.0;
    let h = font_metrics::line_height(EDGE_LABEL_FONT, EDGE_LABEL_SIZE, false, false);
    if text.is_empty() {
        return (0.0, h);
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let mut max_w = 0.0f64;
    for line in &lines {
        let w = font_metrics::text_width(line, EDGE_LABEL_FONT, EDGE_LABEL_SIZE, false, false);
        if w > max_w {
            max_w = w;
        }
    }
    (max_w, h * lines.len() as f64)
}

/// Build a unified::Edge from a model Edge, applying link-style overrides.
/// `pair_counter` is the per-(start,end) duplicate count — 0 for the first
/// edge between a given pair, 1 for the second, etc. (upstream `getEdgeId`).
fn build_edge(e: &ModelEdge, d: &FlowchartDiagram, pair_counter: usize) -> unified::Edge {
    let mut ue = unified::Edge::default();
    ue.id = format!("L_{}_{}_{}", e.start, e.end, pair_counter);
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
    // dagre needs edge label dimensions to reserve space between ranks;
    // labelpos="c" centres the label on the spline (upstream flowchart default).
    ue.labelpos = Some("c".into());
    let label_text = e.label.as_ref().map(|l| l.text.as_str()).unwrap_or("");
    let (lw, lh) = measure_edge_label(label_text);
    ue.extra.insert("label_width".into(), lw.to_string());
    ue.extra.insert("label_height".into(), lh.to_string());

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
