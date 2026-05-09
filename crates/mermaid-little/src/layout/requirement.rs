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
use crate::render::foreign_object::{measure_html_label, HtmlLabelFont};
use crate::text::markdown_text_content;
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
    /// Whether the node has `font-weight: bold` applied via classDef/style.
    /// When true, the kind header and body row FOs use bold font metrics.
    pub is_bold: bool,
    /// Whether the name label is measured as bold.
    /// This is true when label_styles_str (from split_node_label_styles) is non-empty,
    /// meaning the name label g style doesn't start with ";" so JSDOM can parse
    /// the "font-weight: bold" appended to it.
    pub name_is_bold: bool,
    /// Raw CSS styles string for the node (used in rendering).
    pub css_styles: Vec<String>,
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

/// Upstream measures labels at 14 px (SVG root default via foreignObject
/// getBoundingClientRect), not the theme fontSize (16 px).
const FONT_SIZE: f64 = 14.0;
/// Font line height for DejaVu Sans at 14 px: (ascender+|descender|)/upem * size.
/// = (1901+483)/2048 * 14 = 16.296875. Used for edge-label FO height.
const LINE_HEIGHT: f64 = (1901.0 + 483.0) / 2048.0 * FONT_SIZE;
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

    let mut edge_labels: Vec<EdgeLabel> = Vec::new();
    for (i, rel) in d.relations.iter().enumerate() {
        let text = edge_text(&rel.kind);
        // measure the label — upstream wraps it inside a foreignObject
        // whose width is max-content.
        let width = text_width(&text, &font_family, FONT_SIZE, false, false);
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
        e.pattern = Some(if is_contains { "normal" } else { "dashed" }.into());
        e.arrow_type_start = Some(
            if is_contains {
                "requirement_contains"
            } else {
                ""
            }
            .into(),
        );
        e.arrow_type_end = Some(if is_contains { "" } else { "requirement_arrow" }.into());
        e.label_type = Some("markdown".into());
        // Pass label dimensions to dagre so it reserves rank space for the
        // edge label (dagre adds label height to ranksep when routing).
        e.extra.insert("label_width".into(), format!("{}", width));
        e.extra.insert("label_height".into(), format!("{}", height));
        data.edges.push(e);
        edge_labels.push(EdgeLabel {
            text,
            width,
            height,
        });
    }

    // Deduplicate edges by ID, keeping only the LAST occurrence — matching
    // dagre's behavior where `graph.setEdge(src, dst, label, name)` with a
    // duplicate `name` overwrites the previous edge. This mirrors how
    // upstream mermaid's dagre pipeline only retains one edge per ID.
    {
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (i, e) in data.edges.iter().enumerate() {
            seen.insert(e.id.clone(), i);
        }
        let mut keep: Vec<usize> = seen.values().cloned().collect();
        keep.sort_unstable();
        let edges_dedup: Vec<_> = keep.iter().map(|&i| data.edges[i].clone()).collect();
        let labels_dedup: Vec<_> = keep.iter().map(|&i| edge_labels[i].clone()).collect();
        data.edges = edges_dedup;
        edge_labels = labels_dedup;
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

fn is_bold(css_styles: &[String]) -> bool {
    css_styles.iter().any(|s| {
        let s_lower = s.to_lowercase();
        s_lower.contains("font-weight") && (s_lower.contains("bold") || s_lower.contains("bolder"))
    })
}

/// Returns true if any css_style maps to a label-type property (color, font-*,
/// text-*, etc.) — i.e. label_styles_str would be non-empty after
/// split_node_label_styles(). Used to determine whether the name label's
/// `<g style="label_styles; font-weight: bold.">` would be parseable by JSDOM
/// (non-empty label styles → no leading `;` → JSDOM detects font-weight:bold).
fn has_label_styles(css_styles: &[String]) -> bool {
    css_styles.iter().any(|s| {
        let key = s.split(':').next().unwrap_or("").trim().to_lowercase();
        matches!(
            key.as_str(),
            "color"
                | "font-size"
                | "font-family"
                | "font-weight"
                | "font-style"
                | "text-decoration"
                | "text-align"
                | "text-transform"
                | "line-height"
                | "letter-spacing"
                | "word-spacing"
                | "text-shadow"
                | "text-overflow"
                | "white-space"
                | "word-wrap"
                | "word-break"
                | "overflow-wrap"
                | "hyphens"
        )
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
    let bold = is_bold(&r.css_styles);
    let name_bold = has_label_styles(&r.css_styles);
    let rows: Vec<&String> = std::iter::once(&kind_header)
        .chain(std::iter::once(&r.name))
        .chain(body.iter())
        .collect();
    let max_width = rows
        .iter()
        .map(|s| text_width(s, font_family, FONT_SIZE, bold, false))
        .fold(0.0_f64, f64::max);
    NodeLabels {
        row_count: rows.len(),
        kind_header,
        name: r.name.clone(),
        body,
        max_width,
        is_bold: bold,
        name_is_bold: name_bold,
        css_styles: r.css_styles.clone(),
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
    let bold = is_bold(&e.css_styles);
    let name_bold = has_label_styles(&e.css_styles);
    let rows: Vec<&String> = std::iter::once(&kind_header)
        .chain(std::iter::once(&e.name))
        .chain(body.iter())
        .collect();
    let max_width = rows
        .iter()
        .map(|s| text_width(s, font_family, FONT_SIZE, bold, false))
        .fold(0.0_f64, f64::max);
    NodeLabels {
        row_count: rows.len(),
        kind_header,
        name: e.name.clone(),
        body,
        max_width,
        is_bold: bold,
        name_is_bold: name_bold,
        css_styles: e.css_styles.clone(),
    }
}

fn box_size(labels: &NodeLabels) -> (f64, f64) {
    // Upstream getBBox() on shapeSvg unions the local bounding boxes of ALL
    // foreignObject children (JSDOM ignores transforms, so each FO is at (0,0)).
    // totalWidth = max(all FO widths) + padding.
    // totalHeight = kind_fo_h + padding.
    //
    // JSDOM measures each FO via boundingClientRectShim which calls:
    //   measureTextBlock(el.textContent, family, size, bold)
    // where:
    //   - textContent strips all HTML tags (so markdown renders to plain text)
    //   - bold is detected via resolveFont walking up to the <g class="label">
    //
    // For kind/body rows: bold = labels.is_bold (label g has label_styles_str
    //   which resolveFont can parse if label_styles_str is non-empty).
    // For name row: bold = labels.name_is_bold (name g has
    //   "label_styles_str; font-weight: bold." — only parseable when non-empty).
    //   When label_styles_str is empty, the style is "; font-weight: bold."
    //   (leading ";") which JSDOM can't parse → bold = false.
    //
    // Text content: markdown markup is stripped to plain text before measuring
    // (e.g. "**bold**" → "bold", "_italic_" → "italic") since JSDOM's
    // measureTextBlock uses el.textContent which strips all HTML tags.
    let kind_font = HtmlLabelFont {
        bold: Some(labels.is_bold),
        ..HtmlLabelFont::default()
    };
    let name_font = HtmlLabelFont {
        bold: Some(labels.name_is_bold),
        ..HtmlLabelFont::default()
    };
    let body_font = HtmlLabelFont {
        bold: Some(labels.is_bold),
        ..HtmlLabelFont::default()
    };
    let (kind_fo_w, kind_fo_h) =
        measure_html_label(&labels.kind_header, &kind_font, f64::INFINITY, false);
    let name_plain = markdown_text_content(&labels.name);
    let (name_fo_w, _) = measure_html_label(&name_plain, &name_font, f64::INFINITY, false);
    let mut max_fo_w = kind_fo_w.max(name_fo_w);
    for row in &labels.body {
        let row_plain = markdown_text_content(row);
        let (row_w, _) = measure_html_label(&row_plain, &body_font, f64::INFINITY, false);
        if row_w > max_fo_w {
            max_fo_w = row_w;
        }
    }
    let width = max_fo_w + BOX_PAD;
    let height = kind_fo_h + BOX_PAD;
    (width, height)
}

fn edge_text(rel: &Relationship) -> String {
    format!("<<{}>>", rel.keyword())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::requirement::{
        Element, Relation, Relationship, Requirement, RequirementDiagram, RequirementKind,
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
