//! Requirement-diagram layout — wraps the shared dagre bridge and
//! computes per-box dimensions (title + id + text + risk + verify
//! rows).
//!
//! Upstream pipeline: `requirementRenderer.ts` calls `render.ts`
//! which dispatches to the dagre layout, which reads `node.width` /
//! `node.height` off the requirement/element DB output. Box sizes
//! aren't actually computed anywhere in the port of
//! `requirementRenderer.ts` — they fall out of the shape's internal
//! measurement (shape `requirementBox` queries each child label's
//! bounding box via `insertElementsForSize`). Here we pre-measure
//! each label with `font_metrics::measure_line_width` and pick the
//! widest row, mirroring what the shape would compute at render time.
//!
//! Output structure:
//!
//! * [`RequirementLayout`] holds the post-layout geometry (dagre
//!   result) plus per-node label row layouts the renderer can
//!   iterate to emit `<foreignObject>` labels in order.

use crate::error::Result;
use crate::font_metrics::text_width;
use crate::layout::dagre_bridge;
use crate::layout::unified::{Edge as UEdge, LayoutData, LayoutResult, Node as UNode};
use crate::model::requirement::{
    Element, Relationship, Requirement, RequirementDiagram, RequirementKind,
};
use crate::theme::ThemeVariables;

/// Per-node pre-measured label rows.
#[derive(Debug, Clone, Default)]
pub struct NodeLabels {
    /// `<<Requirement>>`-style kind header.
    pub kind_header: String,
    /// Requirement name / element name (bold).
    pub name: String,
    /// Body rows (id / text / risk / verification or type / docref).
    pub body: Vec<String>,
    /// Pre-computed max row width (px).
    pub max_width: f64,
    /// Number of rows — header + name + body lines.
    pub row_count: usize,
}

/// Edge-label text (markdown-escaped `<<verb>>`).
#[derive(Debug, Clone, Default)]
pub struct EdgeLabel {
    pub text: String,
    pub width: f64,
    pub height: f64,
}

/// Full post-layout result for a requirement diagram.
#[derive(Debug, Clone, Default)]
pub struct RequirementLayout {
    /// Raw dagre geometry.
    pub graph: LayoutResult,
    /// Parallel to `graph.nodes` — per-node label rows.
    pub node_labels: Vec<NodeLabels>,
    /// Parallel to `graph.edges` — edge-label text + size.
    pub edge_labels: Vec<EdgeLabel>,
    /// Ids of requirement-backed nodes (for CSS class decisions).
    pub requirement_ids: Vec<String>,
    /// Ids of element-backed nodes.
    pub element_ids: Vec<String>,
    /// The direction we fed dagre (TB/BT/LR/RL).
    pub direction: String,
}

const FONT_SIZE: f64 = 16.0;
const LINE_HEIGHT: f64 = FONT_SIZE * 1.5;
/// Padding inside a requirement box (both x and y, upstream's
/// `padding: 20` from `requirementBox.ts`).
const BOX_PAD: f64 = 20.0;

pub fn layout(d: &RequirementDiagram, theme: &ThemeVariables) -> Result<RequirementLayout> {
    let font_family = theme
        .font_family
        .clone()
        .unwrap_or_else(|| "\"trebuchet ms\",verdana,arial,sans-serif".into());

    let mut data = LayoutData::default();
    data.direction = Some(d.direction.clone());
    data.node_spacing = Some(50.0);
    data.rank_spacing = Some(50.0);
    data.diagram_type = Some("requirement".into());
    data.layout_algorithm = Some("dagre".into());

    let mut node_labels = Vec::new();
    let mut requirement_ids = Vec::new();
    let mut element_ids = Vec::new();

    for r in d.requirements() {
        let labels = requirement_labels(r, &font_family);
        let (w, h) = box_size(&labels);
        let mut n = UNode::default();
        n.id = r.name.clone();
        n.label = Some(labels.name.clone());
        n.width = Some(w);
        n.height = Some(h);
        n.padding = Some(BOX_PAD * 3.0); // header area
        n.shape = Some("requirementBox".into());
        n.css_classes = Some(r.classes.join(" "));
        n.css_styles = if r.css_styles.is_empty() {
            None
        } else {
            Some(r.css_styles.clone())
        };
        node_labels.push(labels);
        requirement_ids.push(r.name.clone());
        data.nodes.push(n);
    }
    for e in d.elements() {
        let labels = element_labels(e, &font_family);
        let (w, h) = box_size(&labels);
        let mut n = UNode::default();
        n.id = e.name.clone();
        n.label = Some(labels.name.clone());
        n.width = Some(w);
        n.height = Some(h);
        n.padding = Some(BOX_PAD * 3.0);
        n.shape = Some("requirementBox".into());
        n.css_classes = Some(e.classes.join(" "));
        n.css_styles = if e.css_styles.is_empty() {
            None
        } else {
            Some(e.css_styles.clone())
        };
        node_labels.push(labels);
        element_ids.push(e.name.clone());
        data.nodes.push(n);
    }

    let mut edge_labels = Vec::new();
    for (i, rel) in d.relations.iter().enumerate() {
        let text = edge_text(&rel.kind);
        // measure the label — upstream wraps it inside a foreignObject
        // whose width is max-content.
        let width = text_width(&text, &font_family, FONT_SIZE, false, false) + 4.0;
        let height = LINE_HEIGHT;
        let mut e = UEdge::default();
        e.id = format!("{}-{}-{}", rel.src, rel.dst, 0);
        let _ = i;
        e.start = Some(rel.src.clone());
        e.end = Some(rel.dst.clone());
        e.label = Some(text.clone());
        let is_contains = rel.kind == Relationship::Contains;
        e.classes = Some("relationshipLine".into());
        e.style = Some(vec![
            "fill:none".to_string(),
            if is_contains {
                String::new()
            } else {
                "stroke-dasharray: 10,7".to_string()
            },
        ]);
        e.labelpos = Some("c".into());
        e.thickness = Some("normal".into());
        e.kind = Some("normal".into());
        e.pattern = Some(
            if is_contains {
                "normal"
            } else {
                "dashed"
            }
            .into(),
        );
        e.arrow_type_start = Some(
            if is_contains {
                "requirement_contains"
            } else {
                ""
            }
            .into(),
        );
        e.arrow_type_end = Some(
            if is_contains {
                ""
            } else {
                "requirement_arrow"
            }
            .into(),
        );
        e.label_type = Some("markdown".into());
        data.edges.push(e);
        edge_labels.push(EdgeLabel {
            text,
            width,
            height,
        });
    }

    let graph = dagre_bridge::layout(&data, theme)?;
    Ok(RequirementLayout {
        graph,
        node_labels,
        edge_labels,
        requirement_ids,
        element_ids,
        direction: d.direction.clone(),
    })
}

fn requirement_labels(r: &Requirement, font_family: &str) -> NodeLabels {
    let kind = r
        .kind
        .unwrap_or(RequirementKind::Requirement)
        .label()
        .to_string();
    let mut body = Vec::new();
    if !r.id.is_empty() {
        body.push(format!("ID: {}", r.id));
    }
    if !r.text.is_empty() {
        body.push(format!("Text: {}", r.text));
    }
    if let Some(risk) = r.risk {
        body.push(format!("Risk: {}", risk.label()));
    }
    if let Some(v) = r.verify {
        body.push(format!("Verification: {}", v.label()));
    }

    let kind_header = format!("<<{}>>", kind);
    let rows: Vec<&String> = std::iter::once(&kind_header)
        .chain(std::iter::once(&r.name))
        .chain(body.iter())
        .collect();
    let max_width = rows
        .iter()
        .map(|s| text_width(s, font_family, FONT_SIZE, false, false))
        .fold(0.0_f64, f64::max);
    NodeLabels {
        row_count: rows.len(),
        kind_header,
        name: r.name.clone(),
        body,
        max_width,
    }
}

fn element_labels(e: &Element, font_family: &str) -> NodeLabels {
    let kind = "Element".to_string();
    let mut body = Vec::new();
    if !e.element_type.is_empty() {
        body.push(format!("Type: {}", e.element_type));
    }
    if !e.doc_ref.is_empty() {
        body.push(format!("Doc Ref: {}", e.doc_ref));
    }
    let kind_header = format!("<<{}>>", kind);
    let rows: Vec<&String> = std::iter::once(&kind_header)
        .chain(std::iter::once(&e.name))
        .chain(body.iter())
        .collect();
    let max_width = rows
        .iter()
        .map(|s| text_width(s, font_family, FONT_SIZE, false, false))
        .fold(0.0_f64, f64::max);
    NodeLabels {
        row_count: rows.len(),
        kind_header,
        name: e.name.clone(),
        body,
        max_width,
    }
}

fn box_size(labels: &NodeLabels) -> (f64, f64) {
    let width = labels.max_width + BOX_PAD * 2.0;
    // Header (kind + name) ~= 2 lines + pad; body rows + pad.
    let header_h = BOX_PAD * 2.0 + LINE_HEIGHT * 2.0;
    let body_h = labels.body.len() as f64 * LINE_HEIGHT + BOX_PAD;
    (width, header_h + body_h)
}

fn edge_text(rel: &Relationship) -> String {
    format!("<<{}>>", rel.keyword())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::requirement::{
        Element, Relation, Requirement, RequirementDiagram, Relationship, RequirementKind,
        RiskLevel, VerifyMethod,
    };

    fn req(name: &str, kind: RequirementKind) -> Requirement {
        Requirement {
            name: name.into(),
            kind: Some(kind),
            id: "1".into(),
            text: "hello".into(),
            risk: Some(RiskLevel::Low),
            verify: Some(VerifyMethod::Test),
            classes: vec!["default".into()],
            ..Requirement::default()
        }
    }

    fn elem(name: &str) -> Element {
        Element {
            name: name.into(),
            element_type: "type".into(),
            classes: vec!["default".into()],
            ..Element::default()
        }
    }

    #[test]
    fn lays_out_two_requirement_boxes() {
        let mut d = RequirementDiagram::new();
        let a = req("a", RequirementKind::Requirement);
        let b = req("b", RequirementKind::Functional);
        d.requirement_order.push("a".into());
        d.requirements_map.insert("a".into(), a);
        d.requirement_order.push("b".into());
        d.requirements_map.insert("b".into(), b);
        d.relations.push(Relation {
            kind: Relationship::Contains,
            src: "a".into(),
            dst: "b".into(),
        });
        let theme = ThemeVariables::default();
        let l = layout(&d, &theme).expect("layout");
        assert_eq!(l.graph.nodes.len(), 2);
        assert_eq!(l.graph.edges.len(), 1);
        assert_eq!(l.node_labels.len(), 2);
    }

    #[test]
    fn element_gets_its_own_header() {
        let mut d = RequirementDiagram::new();
        d.requirement_order.push("r".into());
        d.requirements_map
            .insert("r".into(), req("r", RequirementKind::Requirement));
        d.element_order.push("e".into());
        d.elements_map.insert("e".into(), elem("e"));
        let theme = ThemeVariables::default();
        let l = layout(&d, &theme).expect("layout");
        assert_eq!(l.requirement_ids, vec!["r".to_string()]);
        assert_eq!(l.element_ids, vec!["e".to_string()]);
        assert!(l.node_labels[1].kind_header.contains("Element"));
    }
}
