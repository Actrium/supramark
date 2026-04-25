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
    /// IDs of clusters that were laid out via the recursive inner-layout
    /// algorithm (isolated clusters — no cross-boundary edges).
    /// These are rendered as inner `<g class="root">` groups inside
    /// the outer `<g class="nodes">` section, not in `<g class="clusters">`.
    pub isolated_cluster_ids: std::collections::HashSet<String>,
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
    let LayoutResult {
        nodes,
        edges,
        clusters,
        bounds,
        isolated_cluster_ids,
    } = unified::layout(&layout_data, "dagre", theme)?;

    // Dagre's `assign_node_intersects` always uses `intersect_rect`, which
    // produces a point on the node's axis-aligned bounding box. Upstream
    // mermaid instead calls each shape's `intersect()` callback — for the
    // diamond/question shape this is `intersectPolygon` against the actual
    // polygon vertices. Recompute the entry/exit point for diamond endpoints
    // here so the rendered path matches upstream byte-for-byte.
    let mut edges = edges;
    fix_diamond_edge_endpoints(&mut edges, &nodes);

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
        isolated_cluster_ids,
    })
}

/// Replace the first/last edge waypoint with the diamond polygon intersection
/// when the corresponding endpoint node has shape "diamond". Mirrors upstream
/// `intersectPolygon` for question/diamond shapes; dagre-rs only ever uses
/// `intersect_rect` and so produces a point on the bounding box, which then
/// disagrees with mermaid's reference SVG by ~25 px on the lower-left edge.
fn fix_diamond_edge_endpoints(edges: &mut [unified::Edge], nodes: &[unified::Node]) {
    use crate::layout::unified::types::Point;
    // Build a quick lookup of node id → (cx, cy, s) for diamond nodes.
    let mut diamond_info: BTreeMap<&str, (f64, f64, f64)> = BTreeMap::new();
    for n in nodes {
        if n.shape.as_deref() == Some("diamond") {
            let cx = n.x.unwrap_or(0.0);
            let cy = n.y.unwrap_or(0.0);
            // Layout stores width = height = s (full diagonal length).
            let s = n.width.unwrap_or(0.0);
            diamond_info.insert(n.id.as_str(), (cx, cy, s));
        }
    }
    if diamond_info.is_empty() {
        return;
    }

    // Diamond polygon vertices in absolute coords. Upstream's
    // `intersectPolygon` recomputes positions from `node.x/y/width/height`
    // and the polygon's `minX/minY`, IGNORING the render-time +0.5 px
    // translation that the SVG `<polygon transform>` carries. Mirror that
    // here so we land on the same intersection coordinates.
    let polygon_for = |cx: f64, cy: f64, s: f64| -> [(f64, f64); 4] {
        let half = s / 2.0;
        [
            (cx, cy + half),         // bottom
            (cx + half, cy),         // right
            (cx, cy - half),         // top
            (cx - half, cy),         // left
        ]
    };

    for e in edges.iter_mut() {
        let Some(points) = e.points.as_mut() else {
            continue;
        };
        if points.len() < 2 {
            continue;
        }
        // Start endpoint (anchor follows `start` field which may have been
        // retargeted from a cluster — the actual leaf node is what matters).
        if let Some(start_id) = e.start.as_deref() {
            if let Some(&(cx, cy, s)) = diamond_info.get(start_id) {
                let poly = polygon_for(cx, cy, s);
                let next = points[1];
                if let Some(p) = polygon_intersection((cx, cy), (next.x, next.y), &poly) {
                    // Upstream `question.ts::calcIntersect` subtracts (0.5, 0.5)
                    // from the raw polygon intersection ("Adjusted result").
                    points[0] = Point { x: p.0 - 0.5, y: p.1 - 0.5 };
                }
            }
        }
        if let Some(end_id) = e.end.as_deref() {
            if let Some(&(cx, cy, s)) = diamond_info.get(end_id) {
                let poly = polygon_for(cx, cy, s);
                let n = points.len();
                let prev = points[n - 2];
                if let Some(p) = polygon_intersection((cx, cy), (prev.x, prev.y), &poly) {
                    points[n - 1] = Point { x: p.0 - 0.5, y: p.1 - 0.5 };
                }
            }
        }
    }
}

/// Mirror of upstream `intersectPolygon(node, polyPoints, point)` from
/// `rendering-util/rendering-elements/intersect/intersect-polygon.js`.
///
/// Returns the polygon-edge intersection nearest to `target`, or `None` when
/// no segment intersects the line `(center → target)`. The polygon points
/// are already in absolute coordinates (caller did the `left/top` shift).
///
/// Implementation faithfully reproduces upstream's segment-segment test
/// (`intersect-line.js`) including the +/- offset rounding trick on the
/// numerator, which materially affects the last bit of the f64 result and
/// is required for byte-exact `data-points` parity.
fn polygon_intersection(
    center: (f64, f64),
    target: (f64, f64),
    poly: &[(f64, f64)],
) -> Option<(f64, f64)> {
    let mut hits: Vec<(f64, f64)> = Vec::with_capacity(poly.len());
    for i in 0..poly.len() {
        let p1 = poly[i];
        let p2 = poly[(i + 1) % poly.len()];
        if let Some(p) = intersect_line(center, target, p1, p2) {
            hits.push(p);
        }
    }
    if hits.is_empty() {
        return None;
    }
    if hits.len() > 1 {
        hits.sort_by(|a, b| {
            let da = (a.0 - target.0).powi(2) + (a.1 - target.1).powi(2);
            let db = (b.0 - target.0).powi(2) + (b.1 - target.1).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    Some(hits[0])
}

/// Mirror of upstream `intersectLine(p1, p2, q1, q2)`. Returns the
/// intersection of two line *segments* or `None` if they don't intersect.
fn intersect_line(
    p1: (f64, f64),
    p2: (f64, f64),
    q1: (f64, f64),
    q2: (f64, f64),
) -> Option<(f64, f64)> {
    let a1 = p2.1 - p1.1;
    let b1 = p1.0 - p2.0;
    let c1 = p2.0 * p1.1 - p1.0 * p2.1;
    let r3 = a1 * q1.0 + b1 * q1.1 + c1;
    let r4 = a1 * q2.0 + b1 * q2.1 + c1;
    if r3 != 0.0 && r4 != 0.0 && r3 * r4 > 0.0 {
        return None;
    }
    let a2 = q2.1 - q1.1;
    let b2 = q1.0 - q2.0;
    let c2 = q2.0 * q1.1 - q1.0 * q2.1;
    let r1 = a2 * p1.0 + b2 * p1.1 + c2;
    let r2 = a2 * p2.0 + b2 * p2.1 + c2;
    let epsilon = 1e-6_f64;
    if r1.abs() < epsilon && r2.abs() < epsilon && r1 * r2 > 0.0 {
        return None;
    }
    let denom = a1 * b2 - a2 * b1;
    if denom == 0.0 {
        return None;
    }
    let offset = (denom / 2.0).abs();
    let num_x = b1 * c2 - b2 * c1;
    let x = if num_x < 0.0 {
        (num_x - offset) / denom
    } else {
        (num_x + offset) / denom
    };
    let num_y = a2 * c1 - a1 * c2;
    let y = if num_y < 0.0 {
        (num_y - offset) / denom
    } else {
        (num_y + offset) / denom
    };
    Some((x, y))
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
        // `"default "` has a trailing space; when classes are appended via
        // join(' '), the result is `"default dark"` (no trailing space) for
        // one class, or `"default "` (trailing space) when the list is empty.
        // The shape renderer then formats `"node {cssClasses} "` which
        // produces `"node default  "` (double space) when no extra classes,
        // and `"node default dark "` (one trailing space) when "dark" is last.
        let classes = if v.classes.is_empty() {
            "default ".to_string()
        } else {
            format!("default {}", v.classes.join(" "))
        };
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
    // Two-pass insertion: edges with leaf endpoints first, then edges that
    // originally pointed at a cluster (and were retargeted to a leaf). This
    // matches upstream mermaid's traversal order where cluster-endpoint edges
    // are processed in a follow-up pass — and crucially it matches how dagre
    // resolves parallel-edge ordering between the same (src, dst) pair.
    let mut leaf_edges: Vec<unified::Edge> = Vec::new();
    let mut cluster_edges: Vec<unified::Edge> = Vec::new();
    for e in &d.edges {
        let start = e.start.clone();
        let end = e.end.clone();
        let counter = *pair_count
            .entry((start.clone(), end.clone()))
            .and_modify(|c| *c += 1)
            .or_insert(0);
        let mut ue = build_edge(e, d, counter);
        // Record original endpoints before retargeting so the isolation check
        // in dagre_bridge can test against the pre-retarget cluster IDs.
        ue.extra.insert("orig_start".into(), e.start.clone());
        ue.extra.insert("orig_end".into(), e.end.clone());
        let touched_cluster = d.find_subgraph(&e.start).is_some()
            || d.find_subgraph(&e.end).is_some();
        retarget_cluster_endpoints(&mut ue, d);
        if touched_cluster {
            cluster_edges.push(ue);
        } else {
            leaf_edges.push(ue);
        }
    }
    data.edges.extend(leaf_edges);
    data.edges.extend(cluster_edges);

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

/// Strip markdown syntax markers from a label to get the plain text that
/// jsdom `textContent` would return after markdown→HTML conversion.
///
/// Markdown `**bold**` → `<strong>bold</strong>` → textContent `bold`.
/// Markdown `*italic*` → `<em>italic</em>` → textContent `italic`.
/// HTML tags like `<br>` embedded in markdown are stripped by textContent.
/// The `\n` → `<br/>` → stripped. Result: plain text, single line.
fn strip_markdown_for_measure(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let bytes = label.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'*' {
            // Skip `**` or `*` markers
            if i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                i += 2; // skip **
            } else {
                i += 1; // skip *
            }
        } else if bytes[i] == b'_' {
            // Skip `__` or `_` markers
            if i + 1 < bytes.len() && bytes[i + 1] == b'_' {
                i += 2;
            } else {
                i += 1;
            }
        } else if bytes[i] == b'`' {
            i += 1; // skip backtick (inline code marker)
        } else if bytes[i] == b'<' {
            // HTML tag embedded in markdown: skip to '>'
            if let Some(rel_end) = label[i..].find('>') {
                i += rel_end + 1; // skip the tag
            } else {
                // Bare '<' with no '>' — treat as literal
                out.push('<');
                i += 1;
            }
        } else if bytes[i] == b'\n' {
            // \n → <br/> in HTML → stripped by textContent
            i += 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Measure a vertex's bounding box including its intrinsic shape padding.
/// These padding values must match what the upstream shape renderers
/// compute at draw time, so that dagre assigns the correct node
/// dimensions.
fn measure_vertex_box(v: &Vertex) -> (f64, f64) {
    let label = display_label(v);
    // For markdown labels, the `**bold**` syntax is rendered as HTML and
    // textContent strips the markers — measure the plain-text equivalent.
    let is_markdown = v
        .label
        .as_ref()
        .map(|l| l.kind == LabelKind::Markdown)
        .unwrap_or(false);
    let measure_label = if is_markdown {
        strip_markdown_for_measure(&label)
    } else {
        label.clone()
    };
    let (tw, th) = measure_text(&measure_label);
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
            // Upstream circle.ts: r = bbox.width/2 + halfPadding
            // halfPadding = node.padding/2 = p/2
            // d = 2*r = bbox.width + node.padding = tw + p
            // Uses label WIDTH only (not max(tw,th)) to match upstream bbox.width.
            let d = tw + p;
            return (d, d);
        }
        "doublecircle" => {
            // Upstream: r = bbox.width/2 + labelPadding*2 (look="neo") or + halfPadding*3
            // Using approximate: d = tw + p*2 (matching observed behavior)
            let d = tw + p * 2.0;
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
        "trapezoid" | "trap" | "inv_trapezoid" | "invertedTrapezoid" | "lean_left"
        | "lean-left" | "lean_right" | "lean-right" => (p * 4.0, p * 2.0),
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
            let icon_end = icon_tail
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
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
///
/// Mirrors jsdom's `textContent` semantics: ALL HTML tags (including `<br>`)
/// are stripped, text nodes are concatenated, and only `\n` (actual newline
/// characters) create new measurement lines. This matches how the upstream
/// getBBox shim calls `measureTextBlock(el.textContent, ...)`.
///
/// Bold state is tracked for accurate width: `<strong>`/`<b>` toggles bold.
/// Strip HTML tags from `s` to get the plain text as jsdom `textContent` would.
///
/// `textContent` strips ALL HTML tags (including `<br>`, `<strong>`, etc.)
/// and returns a single concatenated plain text string. Bold markup is NOT
/// accounted for in textContent — it returns plain text regardless.
///
/// `\n` was already converted to `<br/>` in HTML before measurement;
/// textContent strips `<br/>` — so `\n` contributes nothing to the text.
///
/// Returns a single-element vec with the concatenated plain text and bold=false.
fn strip_html_for_measure(s: &str) -> Vec<(String, bool)> {
    let mut text = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Try to find closing '>'. If not found, treat '<' as literal text.
            if let Some(rel_end) = s[i..].find('>') {
                i += rel_end + 1; // skip entire tag
            } else {
                text.push('<');
                i += 1;
            }
        } else if bytes[i] == b'\n' {
            // \n → <br/> in HTML → stripped by textContent
            i += 1;
        } else {
            text.push(bytes[i] as char);
            i += 1;
        }
    }
    vec![(text, false)]
}

/// Measure the overall width/height of the (possibly multi-line) label.
///
/// Upstream mermaid measures node labels via `measureTextBlock` which puts
/// the rendered HTML into a jsdom `<div>` and then reads `el.textContent`.
/// `textContent` strips ALL HTML tags (including `<br/>`) and returns the
/// concatenated plain text — which never contains `\n` since `\n` in the
/// original label was already converted to `<br/>` before measurement.
/// Therefore the measured block is always exactly ONE line, regardless of
/// how many `<br/>` or `\n` appear in the source label.
///
/// Width is the width of the concatenated plain text (with bold spans
/// measured at bold weight). Height is always one `line_height`.
fn measure_text(label: &str) -> (f64, f64) {
    if label.is_empty() {
        return (0.0, LABEL_FONT_SIZE);
    }
    // Strip FA icon tokens first — they render as <i> elements with no width.
    let stripped = strip_fa_icons(label);
    let lh = font_metrics::line_height(DEFAULT_FONT_FAMILY, LABEL_FONT_SIZE, false, false);

    // strip_html_for_measure strips all HTML tags (including <br>) and
    // returns segments split on \n. Since jsdom textContent strips <br/>
    // and the original \n was converted to <br/> before measurement, we
    // treat the whole label as ONE line: sum all segment widths.
    let segments = strip_html_for_measure(&stripped);
    let total_w: f64 = segments
        .iter()
        .map(|(text, bold)| {
            font_metrics::text_width(text, DEFAULT_FONT_FAMILY, LABEL_FONT_SIZE, *bold, false)
        })
        .sum();
    (total_w, lh)
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
fn collect_styles<'a>(v: &'a Vertex, class_map: &BTreeMap<&'a str, &'a ClassDef>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // Upstream: getCompiledStyles(["default", "node", ...vertex.classes])
    // So we look up the "default" and "node" classDefs in addition to the
    // vertex's own classes.
    for builtin in &["default", "node"] {
        if let Some(cd) = class_map.get(*builtin) {
            out.extend(cd.styles.iter().cloned());
        }
    }
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
