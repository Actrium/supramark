#![cfg(feature = "metrics-ttf-parser")]

use mermaid_little::convert_with_id;
use mermaid_little::render::foreign_object::{measure_html_markup_label, HtmlLabelFont};

#[derive(Debug)]
struct ForeignObject {
    text: String,
    width: f64,
    height: f64,
}

fn parse_number(value: &str) -> f64 {
    value
        .parse::<f64>()
        .unwrap_or_else(|e| panic!("parse float {value:?}: {e}"))
}

fn viewbox(svg: &str) -> [f64; 4] {
    let doc = roxmltree::Document::parse(svg).expect("valid svg");
    let svg_node = doc
        .descendants()
        .find(|n| n.has_tag_name("svg"))
        .expect("svg root");
    let value = svg_node.attribute("viewBox").expect("viewBox");
    let parts: Vec<f64> = value.split_whitespace().map(parse_number).collect();
    assert_eq!(parts.len(), 4, "viewBox should have four numbers");
    [parts[0], parts[1], parts[2], parts[3]]
}

fn foreign_objects(svg: &str) -> Vec<ForeignObject> {
    let doc = roxmltree::Document::parse(svg).expect("valid svg");
    doc.descendants()
        .filter(|n| n.has_tag_name("foreignObject"))
        .map(|node| ForeignObject {
            text: {
                let p_text = node
                    .descendants()
                    .filter(|n| n.has_tag_name("p"))
                    .filter_map(|n| n.text())
                    .collect::<String>();
                let text = if p_text.is_empty() {
                    node.descendants()
                        .filter_map(|n| n.text())
                        .collect::<String>()
                } else {
                    p_text
                };
                text.trim().to_string()
            },
            width: parse_number(node.attribute("width").unwrap_or("0")),
            height: parse_number(node.attribute("height").unwrap_or("0")),
        })
        .collect()
}

#[test]
fn flowchart_html_labels_have_browser_line_height_and_bounds() {
    let source = r#"flowchart TD
    A[Write Markdown] --> B[Render Preview]
    B --> C{Review}
    C -->|Pass| D[Publish]
    C -->|Fail| A
"#;
    let svg = convert_with_id(source, "bounds-flowchart").expect("render flowchart");
    let vb = viewbox(&svg);
    assert_eq!(vb[0], 0.0);
    assert_eq!(vb[1], 0.0);
    assert!(vb[2] > 250.0, "flowchart viewBox is too narrow: {vb:?}");
    assert!(
        vb[3] > 410.0,
        "flowchart viewBox does not include the bottom node: {vb:?}"
    );

    let labels = foreign_objects(&svg);
    let non_empty: Vec<_> = labels.iter().filter(|fo| !fo.text.is_empty()).collect();
    assert_eq!(non_empty.len(), 6);
    for label in non_empty {
        assert!(
            label.height >= 24.0,
            "HTML label {:?} has clipped height {}",
            label.text,
            label.height
        );
        assert!(
            label.width > 0.0,
            "HTML label {:?} has no width",
            label.text
        );
    }

    for label in labels.iter().filter(|fo| fo.text.is_empty()) {
        assert_eq!(
            label.height, 0.0,
            "empty edge labels should not affect bounds"
        );
    }
}

#[test]
fn flowchart_edge_labels_have_readability_padding() {
    let source = r#"flowchart TD
    A[Write Markdown] --> B[Render Preview]
    B --> C{Review}
    C -->|Pass| D[Publish]
    C -->|Fail| A
"#;
    let svg = convert_with_id(source, "bounds-flowchart-padding").expect("render flowchart");
    let labels = foreign_objects(&svg);
    let font = HtmlLabelFont::default();
    let seen: Vec<_> = labels.iter().map(|fo| fo.text.as_str()).collect();

    for edge_label in ["Pass", "Fail"] {
        let label = labels
            .iter()
            .find(|fo| fo.text == edge_label)
            .unwrap_or_else(|| panic!("edge label {edge_label:?} should render; saw {seen:?}"));
        let (text_width, _) = measure_html_markup_label(edge_label, &font, 200.0, true);
        let expected = text_width + 8.0;
        assert!(
            (label.width - expected).abs() < 1e-9,
            "edge label {edge_label:?} width {} should include 4px left/right padding over measured text width {text_width}",
            label.width
        );
    }
}

#[test]
fn class_diagram_labels_have_browser_line_height_and_empty_edge_label_zero_height() {
    let source = r#"classDiagram
    class Workspace {
      +string id
      +string root
      +open()
      +status()
    }
    class MarkdownFile {
      +string path
      +render()
    }
    Workspace "1" --> "*" MarkdownFile
"#;
    let svg = convert_with_id(source, "bounds-class").expect("render class diagram");
    let vb = viewbox(&svg);
    assert_eq!(vb[0], 0.0);
    assert_eq!(vb[1], 0.0);
    assert!(vb[2] > 145.0, "class viewBox is too narrow: {vb:?}");
    assert!(vb[3] >= 400.0, "class viewBox is too short: {vb:?}");

    let labels = foreign_objects(&svg);
    let empty = labels
        .iter()
        .find(|fo| fo.text.is_empty())
        .expect("empty relation edge label placeholder");
    assert_eq!(empty.height, 0.0);

    for label in labels.iter().filter(|fo| !fo.text.is_empty()) {
        assert!(
            label.height >= 21.0,
            "class label {:?} has clipped height {}",
            label.text,
            label.height
        );
        assert!(
            label.width > 0.0,
            "class label {:?} has no width",
            label.text
        );
    }
}

#[test]
fn class_namespace_labels_do_not_use_jsdom_fixture_height() {
    let source = r#"classDiagram
namespace WorkspaceLayer {
  class Workspace
}
"#;
    let svg = convert_with_id(source, "bounds-class-namespace").expect("render class namespace");
    let labels = foreign_objects(&svg);
    let namespace = labels
        .iter()
        .find(|fo| fo.text.contains("WorkspaceLayer"))
        .expect("namespace label");
    assert!(
        namespace.height >= 21.0,
        "namespace label uses clipped fixture height {}",
        namespace.height
    );
}

/// `translate(x, y)` of an element's `transform` attribute.
fn translate(node: roxmltree::Node) -> (f64, f64) {
    let t = node.attribute("transform").expect("transform");
    let inner = t
        .trim_start_matches("translate(")
        .trim_end_matches(')')
        .replace(',', " ");
    let parts: Vec<f64> = inner.split_whitespace().map(parse_number).collect();
    (parts[0], parts[1])
}

/// markon #97: a `<br/>` breaks the label line in the browser, so the node
/// box must be tall enough for every line. The label used to be measured as
/// one line, and the second line painted below the node's bottom edge.
#[test]
fn flowchart_multiline_node_labels_fit_inside_their_box() {
    let source = "flowchart TB\n    A[\"line1<br/>line2\"]\n    B[\"one<br>two<br />three\"]\n    C[\"\u{5458}\u{5DE5}\u{8BBE}\u{5907}<br/>Mac / Windows\"]\n    D[single]\n";
    let svg = convert_with_id(source, "bounds-multiline").expect("render flowchart");
    let doc = roxmltree::Document::parse(&svg).expect("valid svg");
    let nodes: Vec<_> = doc
        .descendants()
        .filter(|n| {
            n.has_tag_name("g") && n.attribute("class").is_some_and(|c| c.starts_with("node "))
        })
        .collect();
    assert_eq!(nodes.len(), 4);
    for node in nodes {
        let id = node.attribute("id").unwrap_or_default();
        let rect = node
            .children()
            .find(|n| n.has_tag_name("rect"))
            .expect("node rect");
        let rect_y = parse_number(rect.attribute("y").unwrap());
        let rect_h = parse_number(rect.attribute("height").unwrap());
        let label = node
            .children()
            .find(|n| n.attribute("class") == Some("label"))
            .expect("node label");
        let (_, label_y) = translate(label);
        let fo = label
            .descendants()
            .find(|n| n.has_tag_name("foreignObject"))
            .expect("label foreignObject");
        let fo_h = parse_number(fo.attribute("height").unwrap());
        let lines = fo.descendants().filter(|n| n.has_tag_name("br")).count() + 1;
        assert_eq!(
            fo_h,
            24.0 * lines as f64,
            "{id}: label height for {lines} lines"
        );
        assert!(
            label_y >= rect_y && label_y + fo_h <= rect_y + rect_h,
            "{id}: label [{label_y}, {}] overflows node box [{rect_y}, {}]",
            label_y + fo_h,
            rect_y + rect_h
        );
    }
}

/// markon #97: multi-line edge labels are sized (and reserved in layout) for
/// every line, not just the first.
#[test]
fn flowchart_multiline_edge_labels_measure_every_line() {
    let single = convert_with_id("flowchart TB\n    A -->|\"one\"| B\n", "bounds-edge-1")
        .expect("render single-line edge label");
    let multi = convert_with_id(
        "flowchart TB\n    A -->|\"one<br/>two\"| B\n",
        "bounds-edge-2",
    )
    .expect("render multi-line edge label");
    let edge_fo_h = |svg: &str| {
        foreign_objects(svg)
            .into_iter()
            .find(|fo| fo.text.starts_with("one"))
            .expect("edge label")
            .height
    };
    assert_eq!(edge_fo_h(&single), 24.0);
    assert_eq!(edge_fo_h(&multi), 48.0);
    // The extra line is reserved in the layout: the diagram grows by it.
    let grow = viewbox(&multi)[3] - viewbox(&single)[3];
    assert!(
        grow >= 24.0,
        "layout reserved only {grow}px for the extra line"
    );
}
