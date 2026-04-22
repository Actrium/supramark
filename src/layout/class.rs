//! Class diagram layout — populates `unified::LayoutData` from a
//! parsed [`ClassDiagram`] and runs it through the shared dagre bridge.
//!
//! Upstream references:
//! * `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/class/classDb.ts` (`getData`)
//! * `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/class/shapeUtil.ts`
//! * `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/class/classRenderer-v3-unified.ts`
//!
//! Status
//! ------
//! This is the scaffolding stage. We emit a correct `LayoutData`
//! structure (nodes / edges / clusters populated, markers registered)
//! and hand it to the unified dagre bridge. The **byte-exact** pixel
//! coordinates still depend on:
//!
//! 1. correct node width/height derived from text measurement — the
//!    shared `font_metrics` path applied to each member/method line,
//!    then the classBox stacked-band sum;
//! 2. the v3 classBox shape emitter (see `render/shapes/classbox.rs`);
//! 3. label-bbox measurement for edge labels and multiplicity stubs.
//!
//! The renderer layer (`render/svg_class.rs`) consumes [`ClassLayout`]
//! to produce final SVG. Until both sides are complete for byte-exact
//! fidelity we keep the shape of the API stable so downstream work can
//! progress.

use crate::error::Result;
use crate::font_metrics;
use crate::layout::unified::types::{Edge, LayoutData, LayoutResult, Node};
use crate::layout::unified::render as unified_render;
use crate::model::class::{ClassDiagram, ClassNode, LineType, RelationEnd};
use crate::theme::ThemeVariables;

/// Output of the class layout pass.
#[derive(Debug, Clone)]
pub struct ClassLayout {
    /// Post-layout node + edge geometry from dagre.
    pub unified: LayoutResult,
    /// Mirror of the input — lets the renderer look up style / labels
    /// without re-traversing the model.
    pub data: LayoutData,
    /// Viewbox — computed by the renderer using the unified bounds
    /// plus a uniform padding. Mirrors upstream's `setupViewPortForSVG`
    /// which insets by `padding = 8`.
    pub viewbox_x: f64,
    pub viewbox_y: f64,
    pub viewbox_w: f64,
    pub viewbox_h: f64,
}

/// Default padding — matches upstream `classRenderer-v3-unified.ts`
/// which calls `setupViewPortForSVG(svg, padding=8)`.
const VIEWBOX_PADDING: f64 = 8.0;

/// Public entry point.
pub fn layout(d: &ClassDiagram, theme: &ThemeVariables) -> Result<ClassLayout> {
    let data = build_layout_data(d, theme);
    let result = unified_render::layout(
        &data,
        data.layout_algorithm.as_deref().unwrap_or("dagre"),
        theme,
    )?;

    // Derive viewbox. Upstream's `setupViewPortForSVG` grows the tight
    // bounding box by `padding` on every side.
    let b = result.bounds;
    let vx = b.x - VIEWBOX_PADDING;
    let vy = b.y - VIEWBOX_PADDING;
    let vw = b.width + 2.0 * VIEWBOX_PADDING;
    let vh = b.height + 2.0 * VIEWBOX_PADDING;

    Ok(ClassLayout {
        unified: result,
        data,
        viewbox_x: vx,
        viewbox_y: vy,
        viewbox_w: vw,
        viewbox_h: vh,
    })
}

/// Build the `LayoutData` sent to dagre. Mirrors upstream
/// `classDb.getData`.
fn build_layout_data(d: &ClassDiagram, _theme: &ThemeVariables) -> LayoutData {
    let mut data = LayoutData {
        diagram_type: Some("classDiagram".to_string()),
        direction: d.direction.clone().or_else(|| Some("TB".into())),
        node_spacing: Some(50.0),
        rank_spacing: Some(50.0),
        markers: vec![
            "aggregation".into(),
            "extension".into(),
            "composition".into(),
            "dependency".into(),
            "lollipop".into(),
        ],
        layout_algorithm: Some("dagre".into()),
        ..LayoutData::default()
    };

    // Cluster nodes first, then class nodes. Dagre wants parents before
    // children for compound graphs.
    for ns in &d.namespaces {
        data.nodes.push(cluster_node(ns));
    }
    for c in &d.classes {
        data.nodes.push(class_to_node(c));
    }
    // Notes become their own nodes with a dashed border — upstream's
    // `getData` emits them with `shape: 'note'` and wires a special
    // relation to the target class.
    for n in &d.notes {
        let mut note = Node::default();
        note.id = n.id.clone();
        note.label = Some(n.text.clone());
        note.shape = Some("note".into());
        note.css_classes = Some("note".into());
        note.parent_id = n.parent.clone();
        // Rough text-width estimate — font_metrics expects a font
        // spec; class uses the default 16px Trebuchet (same as pie).
        let (w, h) = measure_multiline(&n.text, 16.0);
        note.width = Some((w + 20.0).max(60.0));
        note.height = Some((h + 20.0).max(30.0));
        data.nodes.push(note);
        if !n.class_id.is_empty() {
            // Invisible edge so dagre keeps them close.
            let mut e = Edge::default();
            e.id = format!("edgeNote_{}_{}", n.class_id, n.id);
            e.source = Some(n.class_id.clone());
            e.target = Some(n.id.clone());
            e.classes = Some("relation".into());
            e.thickness = Some("invisible".into());
            data.edges.push(e);
        }
    }

    // Relation edges.
    for (i, r) in d.relations.iter().enumerate() {
        let mut e = Edge::default();
        e.id = format!("id_{}_{}_{}", r.id1, r.id2, i + 1);
        e.source = Some(r.id1.clone());
        e.target = Some(r.id2.clone());
        e.label = if r.title.is_empty() {
            None
        } else {
            Some(r.title.clone())
        };
        e.arrow_type_start = Some(end_marker_name(r.end1));
        e.arrow_type_end = Some(end_marker_name(r.end2));
        e.pattern = Some(match r.line {
            LineType::Solid => "solid".into(),
            LineType::Dotted => "dashed".into(),
        });
        e.thickness = Some("normal".into());
        e.classes = Some("relation".into());
        e.start_label_right = if r.title1.is_empty() {
            None
        } else {
            Some(r.title1.clone())
        };
        e.end_label_left = if r.title2.is_empty() {
            None
        } else {
            Some(r.title2.clone())
        };
        e.curve = Some("basis".into());
        e.look = Some("classic".into());
        data.edges.push(e);
    }

    data
}

fn cluster_node(ns: &crate::model::class::Namespace) -> Node {
    let mut n = Node::default();
    n.id = ns.id.clone();
    n.dom_id = Some(ns.dom_id.clone());
    n.label = Some(ns.id.clone());
    n.is_group = true;
    n.shape = Some("rect".into());
    n.css_classes = Some("namespace".into());
    n
}

fn class_to_node(c: &ClassNode) -> Node {
    let mut n = Node::default();
    n.id = c.id.clone();
    n.dom_id = Some(c.dom_id.clone());
    n.label = Some(c.label.clone());
    n.shape = Some("classBox".into());
    n.css_classes = Some(
        std::iter::once("default").chain(c.css_classes.iter().map(String::as_str)).collect::<Vec<_>>().join(" "),
    );
    n.parent_id = c.parent.clone();
    n.look = Some("classic".into());

    // Width/height — approximate by summing member-line widths.
    let (w, h) = estimate_classbox_dimensions(c);
    n.width = Some(w);
    n.height = Some(h);

    // Carry member/method text through so the shape emitter can pick
    // them up. `description` is the unified-types field we reuse.
    let mut description = Vec::new();
    for m in &c.members {
        description.push(m.text.clone());
    }
    description.push("__SEP__".into()); // marker between members and methods
    for m in &c.methods {
        description.push(m.text.clone());
    }
    n.description = Some(description);
    n
}

fn estimate_classbox_dimensions(c: &ClassNode) -> (f64, f64) {
    let font = 16.0;
    let family = "trebuchet ms,verdana,arial,sans-serif";
    // Header row: label + optional generic + annotations.
    let mut max_w: f64 = font_metrics::text_width(&c.label, family, font, true, false);
    for a in &c.annotations {
        let aw = font_metrics::text_width(&format!("<<{}>>", a), family, font, false, false);
        max_w = max_w.max(aw);
    }
    let header_h = 38.0;
    let row_h = 24.0;

    for m in &c.members {
        let w = font_metrics::text_width(&m.text, family, font, false, false);
        max_w = max_w.max(w);
    }
    for m in &c.methods {
        let w = font_metrics::text_width(&m.text, family, font, false, false);
        max_w = max_w.max(w);
    }

    let members_h = (c.members.len() as f64 * row_h).max(row_h);
    let methods_h = (c.methods.len() as f64 * row_h).max(row_h);
    let total_h = header_h + members_h + methods_h;
    let total_w = max_w + 24.0;
    (total_w.max(80.0), total_h)
}

fn end_marker_name(end: RelationEnd) -> String {
    match end {
        RelationEnd::None => String::new(),
        RelationEnd::Aggregation => "aggregation".into(),
        RelationEnd::Extension => "extension".into(),
        RelationEnd::Composition => "composition".into(),
        RelationEnd::Dependency => "dependency".into(),
        RelationEnd::Lollipop => "lollipop".into(),
    }
}

/// Crude multi-line measurement helper — counts lines, picks the
/// longest, and hands back (width, height). Kept in a standalone fn so
/// note-node sizing stays consistent with the upstream approach.
fn measure_multiline(text: &str, font: f64) -> (f64, f64) {
    let family = "trebuchet ms,verdana,arial,sans-serif";
    let lines: Vec<&str> = text.split('\n').collect();
    let mut w: f64 = 0.0;
    for line in &lines {
        let lw = font_metrics::text_width(line, family, font, false, false);
        if lw > w {
            w = lw;
        }
    }
    let h = lines.len() as f64 * (font * 1.4);
    (w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    #[test]
    fn layout_runs_on_simple_diagram() {
        let src = "classDiagram\nA <|-- B\n";
        let d = parser::class::parse(src).unwrap();
        let theme = ThemeVariables::default();
        let l = layout(&d, &theme).unwrap();
        assert_eq!(l.unified.nodes.len(), 2);
        assert_eq!(l.unified.edges.len(), 1);
    }

    #[test]
    fn layout_populates_markers() {
        let src = "classDiagram\nA o-- B\n";
        let d = parser::class::parse(src).unwrap();
        let theme = ThemeVariables::default();
        let l = layout(&d, &theme).unwrap();
        assert!(l.data.markers.iter().any(|m| m == "aggregation"));
    }
}
