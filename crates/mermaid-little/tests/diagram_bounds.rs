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

/// Node id, vertical extent of its rect, vertical extent of its label.
type NodeExtents = (String, (f64, f64), (f64, f64));

/// Vertical extent of each flowchart node's rect and label foreignObject.
fn node_label_extents(svg: &str) -> Vec<NodeExtents> {
    let doc = roxmltree::Document::parse(svg).expect("valid svg");
    doc.descendants()
        .filter(|n| {
            n.has_tag_name("g") && n.attribute("class").is_some_and(|c| c.starts_with("node "))
        })
        .map(|node| {
            let id = node.attribute("id").unwrap_or_default().to_string();
            let rect = node
                .children()
                .find(|n| n.has_tag_name("rect"))
                .expect("node rect");
            let ry = parse_number(rect.attribute("y").unwrap());
            let rh = parse_number(rect.attribute("height").unwrap());
            let label = node
                .children()
                .find(|n| n.attribute("class") == Some("label"))
                .expect("node label");
            let (_, ly) = translate(label);
            let fo = label
                .descendants()
                .find(|n| n.has_tag_name("foreignObject"))
                .expect("label foreignObject");
            let fh = parse_number(fo.attribute("height").unwrap());
            (id, (ry, ry + rh), (ly, ly + fh))
        })
        .collect()
}

fn assert_node_lines(source: &str, expected: &[(&str, usize)]) {
    let svg = convert_with_id(source, "bounds-lines").expect("render flowchart");
    let nodes = node_label_extents(&svg);
    for (suffix, lines) in expected {
        let (id, (r0, r1), (l0, l1)) = nodes
            .iter()
            .find(|(id, ..)| id.contains(&format!("-flowchart-{suffix}-")))
            .unwrap_or_else(|| panic!("node {suffix} not rendered"));
        assert_eq!(
            l1 - l0,
            24.0 * *lines as f64,
            "{id}: label should be {lines} line(s)"
        );
        assert!(
            *l0 >= *r0 && *l1 <= *r1,
            "{id}: label [{l0}, {l1}] overflows node box [{r0}, {r1}]"
        );
        // The box grows with the label: one-line box + 24px per extra line.
        let one_line_box = 46.296875;
        assert_eq!(
            r1 - r0,
            one_line_box + 24.0 * (*lines as f64 - 1.0),
            "{id}: node box height"
        );
    }
}

/// PR review blocker 1: markdown labels break lines too — `<br/>`, a source
/// newline (rendered as `<br/>`) and blank-line-separated paragraphs (`<p>`
/// blocks) — and the layout box must grow with them, not only the label.
#[test]
fn flowchart_markdown_multiline_labels_fit_inside_their_box() {
    let source = "flowchart TB\n    A[\"`md<br/>br`\"]\n    B[\"`The dog in **the** hog.(1)\nNL`\"]\n    C[\"`para one\n\npara two`\"]\n    D[\"`one **line**`\"]\n";
    assert_node_lines(source, &[("A", 2), ("B", 2), ("C", 2), ("D", 1)]);
}

/// PR review blocker 2: a `<br/>` at the end of a label opens no line box in
/// a browser (`a<br/>` paints one line; checked in headless Chromium), while
/// an empty line in the middle or at the start does count.
#[test]
fn flowchart_trailing_br_opens_no_line() {
    let source = "flowchart TB\n    A[\"trailing<br/>\"]\n    B[\"a<br/><br/>b\"]\n    C[\"<br/>lead\"]\n    D[\"a<br/><br/>\"]\n    E[\"`md trailing<br/>`\"]\n    F[\"x<br class='q'/>y\"]\n";
    assert_node_lines(
        source,
        &[("A", 1), ("B", 3), ("C", 2), ("D", 2), ("E", 1), ("F", 2)],
    );
    let svg = convert_with_id(
        "flowchart TB\n    A -->|\"edge<br/>\"| B\n",
        "bounds-trailing-edge",
    )
    .expect("render");
    let fo = foreign_objects(&svg)
        .into_iter()
        .find(|fo| fo.text.starts_with("edge"))
        .expect("edge label");
    assert_eq!(fo.height, 24.0, "trailing <br/> in an edge label");
}

/// PR review blocker 3: state-diagram notes are multi-line-aware end to end.
/// A `note … end note` block and a literal `<br/>` note both measure one
/// line per painted line, and the note box grows so the label stays inside.
#[test]
fn state_multiline_notes_fit_inside_their_box() {
    let source = "stateDiagram-v2\n    A --> B\n    B --> C\n    note right of A\n        line one\n        line two\n    end note\n    note left of B : x<br/>y<br/>z\n    note right of C : single<br/>\n";
    let svg = convert_with_id(source, "bounds-state-notes").expect("render state");
    let doc = roxmltree::Document::parse(&svg).expect("valid svg");
    let notes: Vec<_> = doc
        .descendants()
        .filter(|n| {
            n.has_tag_name("g")
                && n.attribute("class")
                    .is_some_and(|c| c.contains("statediagram-note"))
        })
        .collect();
    assert_eq!(notes.len(), 3);
    let mut heights = Vec::new();
    for note in notes {
        let id = note.attribute("id").unwrap_or_default();
        // Fill path `M-hw -hh L…`: the box spans [-hh, hh].
        let d = note
            .descendants()
            .find(|n| n.has_tag_name("path"))
            .and_then(|n| n.attribute("d"))
            .expect("note path");
        let hh = -parse_number(d.trim_start_matches('M').split_whitespace().nth(1).unwrap());
        let label = note
            .children()
            .find(|n| {
                n.attribute("class")
                    .is_some_and(|c| c.contains("noteLabel"))
            })
            .expect("note label");
        let (_, ly) = translate(label);
        let fh = parse_number(
            label
                .descendants()
                .find(|n| n.has_tag_name("foreignObject"))
                .and_then(|fo| fo.attribute("height"))
                .expect("note foreignObject height"),
        );
        assert!(
            ly >= -hh && ly + fh <= hh,
            "{id}: label [{ly}, {}] overflows note box [{}, {hh}]",
            ly + fh,
            -hh
        );
        assert_eq!(2.0 * hh, fh + 30.0, "{id}: note box = label + 2*15 padding");
        heights.push(fh);
    }
    heights.sort_by(f64::total_cmp);
    assert_eq!(heights, [24.0, 48.0, 72.0]);
}

/// State-diagram edge labels break at `<br/>` (and at the `\n` the parser
/// stores for it) just like flowchart labels.
#[test]
fn state_multiline_edge_labels_measure_every_line() {
    // A literal `\n` in a state label is not turned into a break by the
    // parser (pre-existing, unrelated to line counting), so S4 stays one line.
    let source = "stateDiagram-v2\n    [*] --> S1\n    S1 --> S2: one<br/>two\n    S1 --> S3: one <br>two<br>three\n    S1 --> S4: one \\ntwo\n    S1 --> S5: one line\n";
    let svg = convert_with_id(source, "bounds-state-edges").expect("render state");
    let mut heights: Vec<f64> = foreign_objects(&svg)
        .into_iter()
        .filter(|fo| fo.text.starts_with("one"))
        .map(|fo| fo.height)
        .collect();
    heights.sort_by(f64::total_cmp);
    assert_eq!(heights, [24.0, 24.0, 48.0, 72.0]);
}
