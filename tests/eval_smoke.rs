//! Smoke test for the `tests/eval/` framework.
//!
//! Ensures the shared eval module compiles and its core primitives behave
//! sanely on a trivial SVG pair. Individual diagram tests (Phase 4+) will
//! also `#[path = "eval/mod.rs"] mod eval;` and call into it.

#[path = "eval/mod.rs"]
mod eval;

use eval::structural_diff;

const SVG_A: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
    <g class="node"><rect x="0" y="0" width="10" height="10"/><text>A</text></g>
    <g class="edge"><path d="M0 0L10 10" stroke="#000"/></g>
</svg>"##;

const SVG_A_PLUS_NODE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
    <g class="node"><rect x="0" y="0" width="10" height="10"/><text>A</text></g>
    <g class="node"><rect x="0" y="0" width="10" height="10"/><text>B</text></g>
    <g class="edge"><path d="M0 0L10 10" stroke="#000"/></g>
</svg>"##;

#[test]
fn identical_svgs_diff_is_empty() {
    let diff = structural_diff::compare(SVG_A, SVG_A).unwrap();
    assert!(diff.is_empty(), "expected empty: {}", diff.report_text());
}

#[test]
fn extra_node_raises_error() {
    let diff = structural_diff::compare(SVG_A_PLUS_NODE, SVG_A).unwrap();
    assert!(diff.has_errors());
    assert!(diff.errors().any(|i| i.check == "node_count"));
}

/// Parses a real reference SVG to sanity-check roxmltree handles
/// mermaid.js's output and our counters give non-zero results.
/// Ignored by default; run via `cargo test --test eval_smoke -- --ignored`.
#[test]
#[ignore]
fn parses_real_reference_svg() {
    let path = "tests/reference/fixtures/flowchart/01.svg";
    let svg = std::fs::read_to_string(path).expect("reference svg missing");
    let s = eval::structural_diff::SvgStructure::from_svg(&svg).unwrap();
    assert!(s.width > 0.0);
    assert!(s.height > 0.0);
    assert!(s.node_count > 0, "expected nodes, got: {:#?}", s);
    assert!(s.marker_count > 0);

    // Self-compare yields no diff.
    let diff = structural_diff::compare(&svg, &svg).unwrap();
    assert!(
        diff.is_empty(),
        "self-diff non-empty: {}",
        diff.report_text()
    );
}

#[test]
fn report_emitters_run() {
    use eval::report::{EvalReport, FixtureReport};

    let diff = structural_diff::compare(SVG_A_PLUS_NODE, SVG_A).unwrap();
    let mut report = EvalReport::new();
    report.push(FixtureReport::new("smoke/extra-node", diff).with_type("flowchart"));

    let text = report.text_summary();
    assert!(text.contains("Parity"));
    let json = report.to_json();
    assert!(json.starts_with('{') && json.ends_with('}'));
    assert!(json.contains("\"status\":\"error\""));
    let html = report.to_html();
    assert!(html.contains("<table"));
    assert!(html.contains("status-error"));
}
