#![cfg(feature = "metrics-static-dejavu")]
//! Byte-exact parity tests for the quadrant-chart renderer.
//!
//! The Wave-2 pipeline is called directly (parser → layout → render) so
//! these tests don't rely on `convert_with_id` dispatch from `lib.rs`.
//!
//! Compiled only with `metrics-static-dejavu` — byte parity vs upstream
//! Mermaid's reference SVGs only holds when the layout pipeline runs
//! against the static DejaVu fixtures.

use mermaid_little::layout::quadrant as lay;
use mermaid_little::parser::quadrant as prs;
use mermaid_little::render::svg_quadrant as rnd;
use mermaid_little::theme::get_theme;
use std::fs;
use std::path::PathBuf;

fn id_for(rel: &str) -> String {
    // Mirrors tests/support/generate_ref.mjs::idForPath — run of non
    // alphanumerics collapses to a single '-'.
    let mut id = String::from("ref-");
    let mut last_sep = false;
    for c in rel.chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c);
            last_sep = false;
        } else if !last_sep {
            id.push('-');
            last_sep = true;
        }
    }
    if id.ends_with('-') {
        id.pop();
    }
    id
}

fn render_fixture(source: &str, id: &str) -> String {
    let diagram = prs::parse(source).expect("parse");
    // Pick theme from captured override (falls back to default).
    let theme_name = diagram
        .theme_name
        .clone()
        .unwrap_or_else(|| "default".into());
    let theme = get_theme(&theme_name);
    let laid = lay::layout(&diagram, &theme).expect("layout");
    rnd::render(&diagram, &laid, &theme, id).expect("render")
}

fn check_fixture(rel: &str) {
    let mut mmd = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    mmd.push(format!("tests/{}.mmd", rel));
    let mut svg = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    svg.push(format!("tests/reference/{}.svg", rel));

    let source = fs::read_to_string(&mmd).unwrap_or_else(|e| panic!("read {mmd:?}: {e}"));
    let expected = fs::read_to_string(&svg).unwrap_or_else(|e| panic!("read {svg:?}: {e}"));
    let expected = expected.trim_end_matches('\n');
    let id = id_for(rel);
    let got = render_fixture(&source, &id);

    if got == expected {
        return;
    }
    let got_len = got.len();
    let exp_len = expected.len();
    let mut diff = 0usize;
    for (i, (a, b)) in got.bytes().zip(expected.bytes()).enumerate() {
        if a != b {
            diff = i;
            break;
        }
    }
    let ctx = 160usize;
    let start = diff.saturating_sub(ctx);
    let end_g = (diff + ctx).min(got_len);
    let end_e = (diff + ctx).min(exp_len);
    panic!(
        "byte mismatch for {rel} at byte {diff} (got.len={got_len}, exp.len={exp_len})\nGOT: ...{g}...\nEXP: ...{e}...",
        g = &got[start..end_g],
        e = &expected[start..end_e],
    );
}

#[test]
fn cypress_quadrant_all() {
    for n in 1..=14u32 {
        check_fixture(&format!("ext_fixtures/cypress/quadrant/{:02}", n));
    }
}

#[test]
fn demos_quadrant_all() {
    for n in 1..=2u32 {
        check_fixture(&format!("ext_fixtures/demos/quadrant/{:02}", n));
    }
}
