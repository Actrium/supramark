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

/// A `<` that opens no tag is literal text in a markdown label, so it must
/// still be measured: the node box may not shrink below the label it holds.
/// (Wrapping paragraphs in `<p>` for line counting once made the width
/// helper swallow everything from the `<` to the next `>`.)
#[test]
fn flowchart_markdown_labels_measure_literal_less_than() {
    // `<` + letter has no `>` of its own: the markdown path must not borrow
    // one from markup it generated itself.
    let source = "flowchart TB\n    A[\"`a < b`\"]\n    B[\"`< b`\"]\n    C[\"`a < b < c`\"]\n    D[\"`a < b`\"]\n    E[\"a < b\"]\n    F[\"`count<max`\"]\n    G[\"`if i<n then loop again`\"]\n    H[\"`x<y and y<z`\"]\n";
    let svg = convert_with_id(source, "bounds-md-lt").expect("render flowchart");
    let doc = roxmltree::Document::parse(&svg).expect("valid svg");
    for node in doc.descendants().filter(|n| {
        n.has_tag_name("g") && n.attribute("class").is_some_and(|c| c.starts_with("node "))
    }) {
        let id = node.attribute("id").unwrap_or_default();
        let rect_w = parse_number(
            node.children()
                .find(|n| n.has_tag_name("rect"))
                .and_then(|r| r.attribute("width"))
                .expect("node rect width"),
        );
        let fo_w = parse_number(
            node.descendants()
                .find(|n| n.has_tag_name("foreignObject"))
                .and_then(|fo| fo.attribute("width"))
                .expect("label width"),
        );
        assert!(
            rect_w >= fo_w,
            "{id}: box {rect_w} is narrower than its label {fo_w}"
        );
        // Literal `<` text is measured, so no label degenerates to the
        // empty-label box (padding only).
        assert!(rect_w > 60.0, "{id}: box {rect_w} looks unmeasured");
    }
    // The markdown and plain spellings of the same text measure alike.
    let widths: Vec<f64> = doc
        .descendants()
        .filter(|n| n.has_tag_name("rect") && n.attribute("class") == Some("basic label-container"))
        .map(|r| parse_number(r.attribute("width").unwrap()))
        .collect();
    assert_eq!(widths[0], widths[3], "two markdown `a < b` labels");
    assert_eq!(widths[0], widths[4], "markdown vs plain `a < b`");
    assert!(widths[2] > widths[0], "`a < b < c` is wider than `a < b`");
    // A markdown label measures like its plain spelling: the text after a
    // literal `<` counts towards the width.
    let plain = convert_with_id(
        "flowchart TB\n    F[\"count<max\"]\n    G[\"if i<n then loop again\"]\n    H[\"x<y and y<z\"]\n",
        "bounds-md-lt-plain",
    )
    .expect("render flowchart");
    // Scanned as text, not parsed: a plain string label emits `<max` as a raw
    // tag, so the document is not well-formed XML (pre-existing).
    let plain_widths: Vec<f64> = plain
        .split(r#"<rect class="basic label-container""#)
        .skip(1)
        .map(|chunk| {
            let at = chunk.find(r#" width=""#).expect("rect width") + 8;
            let end = chunk[at..].find('"').expect("rect width end") + at;
            parse_number(&chunk[at..end])
        })
        .collect();
    assert_eq!(
        &widths[5..8],
        &plain_widths[..],
        "markdown vs plain `<` labels"
    );
}

/// A markdown label whose paragraph ends with a literal `<` + letter must
/// still split at the paragraph break: the label paints two lines, so the box
/// has to hold two.
#[test]
fn flowchart_markdown_paragraphs_split_after_literal_less_than() {
    let source = "flowchart TB\n    A[\"`a<b\n\nsecond paragraph`\"]\n";
    assert_node_lines(source, &[("A", 2)]);
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

/// Rect width of every node, in document order, read as text (a label may
/// emit markup that is not well-formed XML).
fn rect_widths(svg: &str) -> Vec<f64> {
    svg.split(r#"<rect class="basic label-container""#)
        .skip(1)
        .map(|chunk| {
            let at = chunk.find(r#" width=""#).expect("rect width") + 8;
            let end = chunk[at..].find('"').expect("rect width end") + at;
            parse_number(&chunk[at..end])
        })
        .collect()
}

/// An FA icon token ends at its line break: `fa:fa-car<br/>Longer text` is an
/// icon plus a second line, not an icon whose name ran into that line. The
/// icon tokens must therefore be stripped before the lines are joined for the
/// width measurement.
#[test]
fn flowchart_fa_icon_before_a_line_break_keeps_the_next_line() {
    let source = "flowchart TB\n    A[\"fa:fa-car<br/>Longer text here\"]\n    B[\"`fa:fa-car<br/>Longer text here`\"]\n    C[\"Car fa:fa-car<br/>Longer text here\"]\n    D[\"fa:fa-car<br/>x\"]\n";
    let svg = convert_with_id(source, "bounds-fa").expect("render flowchart");
    let widths = rect_widths(&svg);
    let nodes = node_label_extents(&svg);
    for (i, (id, (r0, r1), (l0, l1))) in nodes.iter().enumerate() {
        let fo_w = parse_number(
            svg.split(r#"<foreignObject width=""#)
                .nth(i + 1)
                .and_then(|c| c.split('"').next())
                .expect("label width"),
        );
        assert!(
            widths[i] >= fo_w,
            "{id}: box {} is narrower than its label {fo_w}",
            widths[i]
        );
        assert_eq!(l1 - l0, 48.0, "{id}: icon plus one text line");
        assert!(*l0 >= *r0 && *l1 <= *r1, "{id}: label outside the box");
    }
    // Both spellings of the same label measure alike, and the text after the
    // break still counts (the whole label, not just "Longer" onwards).
    assert_eq!(widths[0], widths[1], "markdown vs plain FA label");
    assert!(
        widths[0] > 170.0,
        "box {} lost the text after the break",
        widths[0]
    );
    assert!(widths[2] > widths[0], "a leading word widens the box");
}

/// An FA icon alone on the second line paints no line box of its own (the
/// emitted `<i>` is empty), so layout must agree with the renderer: one line.
#[test]
fn flowchart_fa_icon_alone_on_a_line_agrees_with_the_renderer() {
    let svg = convert_with_id(
        "flowchart TB\n    A[\"a<br/>fa:fa-car\"]\n    B[\"fa:fa-car<br/>fa:fa-bus\"]\n",
        "bounds-fa-alone",
    )
    .expect("render flowchart");
    for (id, (r0, r1), (l0, l1)) in node_label_extents(&svg) {
        assert_eq!(l1 - l0, 24.0, "{id}: icons paint no line of their own");
        assert_eq!(r1 - r0, 46.296875, "{id}: one-line box");
    }
    assert_eq!(rect_widths(&svg)[1], 60.0, "two icons measure as empty");
}

/// Where a single-cluster flowchart puts its pieces: the cluster rect, the
/// title's `foreignObject`, and the top edge of the first node.
#[derive(Debug)]
struct ClusterTitleGeom {
    rect_y: f64,
    rect_h: f64,
    title_h: f64,
    node_top: f64,
}

fn cluster_title_geom(svg: &str) -> ClusterTitleGeom {
    let doc = roxmltree::Document::parse(svg).expect("valid svg");
    let cluster = doc
        .descendants()
        .find(|n| {
            n.has_tag_name("g")
                && n.attribute("class")
                    .is_some_and(|c| c == "cluster " || c.starts_with("cluster "))
        })
        .expect("cluster group");
    let rect = cluster
        .descendants()
        .find(|n| n.has_tag_name("rect"))
        .expect("cluster rect");
    let title = cluster
        .descendants()
        .find(|n| n.has_tag_name("foreignObject"))
        .expect("cluster title");
    let node = doc
        .descendants()
        .find(|n| {
            n.has_tag_name("g")
                && n.attribute("class")
                    .is_some_and(|c| c == "node" || c.starts_with("node "))
        })
        .expect("node group");
    let transform = node.attribute("transform").expect("node transform");
    let cy: f64 = transform
        .trim_start_matches("translate(")
        .trim_end_matches(')')
        .split(',')
        .nth(1)
        .expect("node centre y")
        .trim()
        .parse()
        .expect("parse node centre y");
    let node_rect = node
        .descendants()
        .find(|n| n.has_tag_name("rect"))
        .expect("node rect");
    ClusterTitleGeom {
        rect_y: parse_number(rect.attribute("y").unwrap_or("0")),
        rect_h: parse_number(rect.attribute("height").unwrap_or("0")),
        title_h: parse_number(title.attribute("height").unwrap_or("0")),
        node_top: cy + parse_number(node_rect.attribute("y").unwrap_or("0")),
    }
}

/// `#241`: a `<br/>` in a subgraph title paints a second line, so the cluster
/// box has to grow by one line per extra line and its contents have to move
/// down with it — before this the second line was drawn on top of the first
/// row of nodes (dagre sizes a cluster from its border nodes, and the title is
/// no part of that).
#[test]
fn flowchart_cluster_title_lines_get_their_own_band() {
    let one_line = convert_with_id(
        "flowchart TB\n    subgraph S [\"Sub\"]\n        A\n    end\n",
        "cluster-title-one",
    )
    .expect("render one-line title");
    let two_lines = convert_with_id(
        "flowchart TB\n    subgraph S [\"Sub<br/>title\"]\n        A\n    end\n",
        "cluster-title-two",
    )
    .expect("render two-line title");

    let one = cluster_title_geom(&one_line);
    let two = cluster_title_geom(&two_lines);

    assert_eq!(one.title_h, 24.0, "a one-line title paints one line");
    assert_eq!(two.title_h, 48.0, "`<br/>` paints a second title line");
    assert_eq!(
        two.rect_y, one.rect_y,
        "the box grows downwards, not upwards"
    );
    assert_eq!(
        two.rect_h - one.rect_h,
        24.0,
        "the box gains exactly the second title line"
    );
    assert_eq!(
        two.node_top - one.node_top,
        24.0,
        "the contents move down by the line the title gained"
    );
    assert!(
        two.node_top >= two.rect_y + two.title_h,
        "the second title line ({}) must clear the first row of nodes ({}): \
         title bottom {} vs node top {}",
        two.title_h,
        two.node_top - two.rect_y,
        two.rect_y + two.title_h,
        two.node_top,
    );
}

/// The band a title needs is a property of the lines it paints, so a title
/// that stays on one line — and a cluster with no title at all — must keep the
/// geometry they had before `#241`, which is what every existing fixture
/// exercises.
#[test]
fn flowchart_one_line_cluster_titles_keep_their_geometry() {
    let bare = convert_with_id(
        "flowchart TB\n    subgraph S\n        A\n    end\n",
        "cluster-bare",
    )
    .expect("render untitled cluster");
    let titled = convert_with_id(
        "flowchart TB\n    subgraph S [\"Sub\"]\n        A\n    end\n",
        "cluster-titled",
    )
    .expect("render one-line title");

    let without = cluster_title_geom(&bare);
    let with = cluster_title_geom(&titled);
    assert_eq!(
        with.rect_h, without.rect_h,
        "a one-line title asks for no room"
    );
    assert_eq!(
        with.node_top, without.node_top,
        "a one-line title moves nothing"
    );
}

/// x and y coordinates of the first edge's `data-points` payload — the layout's
/// own points, before the renderer shortens the path for the arrow marker.
fn edge_data_points(svg: &str) -> (Vec<f64>, Vec<f64>) {
    let doc = roxmltree::Document::parse(svg).expect("valid svg");
    let payload = doc
        .descendants()
        .find_map(|n| {
            let class = n.attribute("class")?;
            if class.contains("flowchart-link") {
                n.attribute("data-points")
            } else {
                None
            }
        })
        .expect("edge data-points");
    let text = String::from_utf8(decode_base64(payload)).expect("base64 is utf-8");
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for pair in text.split('{').skip(1) {
        for (key, out) in [("\"x\":", &mut xs), ("\"y\":", &mut ys)] {
            let Some(at) = pair.find(key) else { continue };
            let rest = &pair[at + key.len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .unwrap_or(rest.len());
            out.push(rest[..end].parse().expect("coordinate"));
        }
    }
    (xs, ys)
}

/// Minimal base64 decoder for the `data-points` payload, which upstream writes
/// as `btoa(JSON.stringify(points))`.
fn decode_base64(input: &str) -> Vec<u8> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &c) in TABLE.iter().enumerate() {
        lookup[c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0;
    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        let value = lookup[byte as usize];
        if value == 255 {
            continue;
        }
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}

/// Numbers in an SVG path `d` string, in order — `M77.8,79.3L77.8,83.5C…`
/// yields x, y, x, y, … so odd indices are the y coordinates.
fn path_numbers(d: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let mut current = String::new();
    for c in d.chars() {
        if c.is_ascii_digit() || c == '.' || (c == '-' && current.is_empty()) {
            current.push(c);
        } else if !current.is_empty() {
            out.push(current.parse().unwrap_or(0.0));
            current.clear();
        }
    }
    if !current.is_empty() {
        out.push(current.parse().unwrap_or(0.0));
    }
    out
}

/// y coordinates of the first flowchart edge path, in path order.
fn edge_path_ys(svg: &str) -> Vec<f64> {
    let doc = roxmltree::Document::parse(svg).expect("valid svg");
    let d = doc
        .descendants()
        .find(|n| {
            n.has_tag_name("path")
                && n.attribute("class")
                    .is_some_and(|c| c.contains("flowchart-link"))
        })
        .and_then(|n| n.attribute("d"))
        .expect("flowchart edge path");
    path_numbers(d).into_iter().skip(1).step_by(2).collect()
}

/// y of the first edge label's group, if the diagram has one.
fn edge_label_y(svg: &str) -> Option<f64> {
    let doc = roxmltree::Document::parse(svg).expect("valid svg");
    let transform = doc
        .descendants()
        .find(|n| n.has_tag_name("g") && n.attribute("class").is_some_and(|c| c == "edgeLabel"))?
        .attribute("transform")?;
    transform
        .trim_start_matches("translate(")
        .trim_end_matches(')')
        .split(',')
        .nth(1)?
        .trim()
        .parse()
        .ok()
}

/// An edge inside a titled cluster moves as one piece. Two failure modes this
/// guards, both found in review: judging each point by the cluster rect it
/// falls in tears the spline at the boundary (a point just outside keeps its
/// old y while its neighbours move a whole line), and `refine_edges` places the
/// label before the move, so a label left behind floats a line above its edge.
#[test]
fn flowchart_cluster_title_moves_edges_and_labels_as_one_piece() {
    let source = |title: &str| {
        format!(
            "flowchart TB\n    subgraph S [\"{title}\"]\n        A -->|edge label| B\n    end\n"
        )
    };
    let one_line = convert_with_id(&source("Sub"), "cluster-edge-one").expect("render");
    let two_lines = convert_with_id(&source("Sub<br/>title"), "cluster-edge-two").expect("render");

    let before = edge_path_ys(&one_line);
    let after = edge_path_ys(&two_lines);
    assert_eq!(before.len(), after.len(), "same path shape");
    assert!(before.len() >= 2, "edge has a path");
    for (a, b) in before.iter().zip(&after) {
        assert!(
            (b - a - 24.0).abs() < 1e-6,
            "every path point shifts by the title's line: {a} -> {b}"
        );
    }

    let label_before = edge_label_y(&one_line).expect("label");
    let label_after = edge_label_y(&two_lines).expect("label");
    assert!(
        (label_after - label_before - 24.0).abs() < 1e-6,
        "the label rides with the edge: {label_before} -> {label_after}"
    );
}

/// An edge crossing a cluster boundary shifts smoothly: the amount is
/// interpolated along the path, so neighbouring points never diverge by a whole
/// line. The per-point rect rule this replaced moved one point 24px while the
/// next stayed put, folding the spline into a 1px kink.
#[test]
fn flowchart_cluster_title_keeps_cross_cluster_edges_smooth() {
    let source = |first: &str, second: &str| {
        format!(
            "flowchart TB\n    subgraph S1 [\"{first}\"]\n        A\n    end\n    subgraph S2 [\"{second}\"]\n        B\n    end\n    A --> B\n"
        )
    };
    let flat = convert_with_id(&source("One", "Three"), "cluster-cross-flat").expect("render");
    let titled =
        convert_with_id(&source("One<br/>Two", "Three<br/>Four"), "cluster-cross").expect("render");

    let before = edge_path_ys(&flat);
    let after = edge_path_ys(&titled);
    assert_eq!(before.len(), after.len(), "same path shape");
    assert!(before.len() >= 3, "edge has a routed path");

    let shifts: Vec<f64> = before.iter().zip(&after).map(|(a, b)| b - a).collect();
    assert!(
        shifts.iter().any(|s| s.abs() > 1.0),
        "the edge moved with its clusters, got {shifts:?}"
    );
    for w in shifts.windows(2) {
        assert!(
            (w[1] - w[0]).abs() < 12.0,
            "the shift varies smoothly along the path (half a line at most \
             between neighbours), got {shifts:?}"
        );
    }
}

/// The edge endpoints land exactly on the node boundaries they attach to — the
/// invariant every downstream clip depends on (`node.intersect`, the
/// cluster-endpoint cut, subroutine's polygon rounding). The endpoints take
/// their node's shift verbatim rather than the interpolated value, so it holds
/// after the move too.
///
/// The edge leaves the cluster, so the two endpoints carry *different* shifts.
/// That is what lets this tell a per-endpoint assignment from an interpolated
/// one — and from no movement at all — which an edge that stays inside a single
/// cluster cannot: there both endpoints share a shift, so every candidate
/// produces the same numbers. Read from `data-points`, not from `d`: the
/// rendered path is shortened by the arrow marker's offset, so only the layout's
/// own points sit on the boundary.
#[test]
fn flowchart_cluster_title_keeps_edge_endpoints_on_their_nodes() {
    let svg = convert_with_id(
        "flowchart TB\n    subgraph S [\"One<br/>Two\"]\n        A\n    end\n    B\n    A --> B\n",
        "cluster-endpoints",
    )
    .expect("render");

    let doc = roxmltree::Document::parse(&svg).expect("valid svg");
    let vertical_extent = |id_fragment: &str| -> (f64, f64) {
        let node = doc
            .descendants()
            .find(|n| {
                n.has_tag_name("g") && n.attribute("id").is_some_and(|id| id.contains(id_fragment))
            })
            .unwrap_or_else(|| panic!("node {id_fragment}"));
        let (_, cy) = translate(node);
        let rect = node
            .children()
            .find(|c| c.has_tag_name("rect"))
            .expect("node rect");
        let ry = parse_number(rect.attribute("y").unwrap());
        let rh = parse_number(rect.attribute("height").unwrap());
        (cy + ry, cy + ry + rh)
    };
    let (_, a_bottom) = vertical_extent("-flowchart-A-");
    let (b_top, _) = vertical_extent("-flowchart-B-");

    let (_, ys) = edge_data_points(&svg);
    let last = *ys.last().unwrap();
    assert!(
        (ys[0] - a_bottom).abs() < 1e-6,
        "the start sits on A's bottom edge: {} vs {a_bottom}",
        ys[0]
    );
    assert!(
        (last - b_top).abs() < 1e-6,
        "the end sits on B's top edge: {last} vs {b_top}"
    );
}

/// Distance from `point` to the polyline `path`.
fn distance_to_path(point: (f64, f64), path: &[(f64, f64)]) -> f64 {
    let mut best = f64::INFINITY;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len_sq = dx * dx + dy * dy;
        let t = if len_sq > 0.0 {
            (((point.0 - a.0) * dx + (point.1 - a.1) * dy) / len_sq).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (qx, qy) = (a.0 + dx * t, a.1 + dy * t);
        best = best.min(((point.0 - qx).powi(2) + (point.1 - qy).powi(2)).sqrt());
    }
    best
}

/// The edge label's centre, from its `<g class="edgeLabel">` translate.
fn edge_label_point(svg: &str) -> Option<(f64, f64)> {
    let doc = roxmltree::Document::parse(svg).expect("valid svg");
    let transform = doc
        .descendants()
        .find(|n| n.has_tag_name("g") && n.attribute("class").is_some_and(|c| c == "edgeLabel"))?
        .attribute("transform")?;
    let inner = transform
        .trim_start_matches("translate(")
        .trim_end_matches(')');
    let mut parts = inner.split(',');
    let x = parts.next()?.trim().parse().ok()?;
    let y = parts.next()?.trim().parse().ok()?;
    Some((x, y))
}

/// A label on an edge that crosses a cluster boundary rides the same
/// interpolation its path does. Asserting only that the label sits on the edge
/// is not enough: this edge is a straight line, so a label that never moved
/// still lies on the moved path. The only quantity that separates a real
/// interpolation from a stale position is how far the label travelled — one
/// endpoint's cluster grows and the other does not, so the label moves a
/// fraction of the line rather than all of it or none of it.
#[test]
fn flowchart_cluster_title_interpolates_cross_cluster_edge_labels() {
    let source = |title: &str| {
        format!(
            "flowchart TB\n    subgraph S [\"{title}\"]\n        A\n    end\n    B\n    A -->|edge label| B\n"
        )
    };
    let flat = convert_with_id(&source("One"), "cluster-cross-label-flat").expect("render");
    let titled = convert_with_id(&source("One<br/>Two"), "cluster-cross-label").expect("render");

    let before = edge_label_point(&flat).expect("label before");
    let after = edge_label_point(&titled).expect("label after");
    let shift = after.1 - before.1;
    assert!(
        shift > 0.0 && shift < 24.0,
        "the label moves with the path but not with one endpoint alone: {shift}px"
    );

    let (xs, ys) = edge_data_points(&titled);
    let path: Vec<(f64, f64)> = xs.iter().zip(&ys).map(|(x, y)| (*x, *y)).collect();
    let distance = distance_to_path(after, &path);
    assert!(
        distance < 0.5,
        "and it still sits on its edge: {distance}px away from the path"
    );
}

/// Geometry of a single-composite-state diagram: the cluster's outer rect, its
/// title box, and the first node's centre.
#[derive(Debug)]
struct StateTitleGeom {
    outer_y: f64,
    outer_h: f64,
    title_h: f64,
    first_node_y: f64,
}

fn state_cluster_geometry(svg: &str) -> StateTitleGeom {
    let doc = roxmltree::Document::parse(svg).expect("valid svg");
    let outer = doc
        .descendants()
        .find(|n| n.has_tag_name("rect") && n.attribute("class") == Some("outer"))
        .expect("cluster outer rect");
    let title = doc
        .descendants()
        .find(|n| n.has_tag_name("foreignObject"))
        .expect("cluster title");
    let node = doc
        .descendants()
        .find(|n| {
            n.has_tag_name("g")
                && n.attribute("class")
                    .is_some_and(|c| c == "node" || c.starts_with("node "))
        })
        .expect("node group");
    StateTitleGeom {
        outer_y: parse_number(outer.attribute("y").unwrap_or("0")),
        outer_h: parse_number(outer.attribute("height").unwrap_or("0")),
        title_h: parse_number(title.attribute("height").unwrap_or("0")),
        first_node_y: translate(node).1,
    }
}

/// A `<br/>` in a composite state's title gets the same treatment a flowchart
/// subgraph title gets: it paints a second line, and the cluster grows to keep
/// that line clear of its contents. Upstream 11.14 leaves the box alone and
/// lets the line land on the first node (measured in Chromium: it covers about
/// 11.5px of it) — this port deliberately does not reproduce that, the same
/// call it makes for flowchart titles.
#[test]
fn state_composite_title_lines_get_their_own_band() {
    let render = |title: &str, id: &str| {
        convert_with_id(
            &format!("stateDiagram-v2\n    state \"{title}\" as S {{\n        A --> B\n    }}\n"),
            id,
        )
        .expect("render state diagram")
    };
    let one = state_cluster_geometry(&render("Sub", "state-title-one"));
    let two = state_cluster_geometry(&render("Sub<br/>title", "state-title-two"));

    assert_eq!(one.title_h, 24.0, "a one-line title paints one line");
    assert_eq!(two.title_h, 48.0, "`<br/>` paints a second title line");
    assert_eq!(two.outer_y, one.outer_y, "the box grows downwards");
    assert_eq!(
        two.outer_h - one.outer_h,
        24.0,
        "the box gains exactly the second title line"
    );
    assert_eq!(
        two.first_node_y - one.first_node_y,
        24.0,
        "the contents move down with it"
    );
    // 22.5 is half the node box's height in this diagram (the box is 45 tall).
    assert!(
        two.first_node_y - 22.5 >= two.outer_y + two.title_h,
        "the second title line clears the first row of nodes"
    );
}
