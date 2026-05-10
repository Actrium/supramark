#![cfg(feature = "metrics-ttf-parser")]
//! Journey byte-exact test harness (Wave 2).
//!
//! Runs every fixture in `tests/ext_fixtures/{cypress,demos}/journey`
//! through the Rust pipeline and diffs against the matching reference
//! SVG. Because `convert_with_id` in `lib.rs` does not yet dispatch to
//! the journey renderer, this file calls `parse`/`layout`/`render`
//! directly.
//!
//! Compiled only with `metrics-ttf-parser` — byte parity vs upstream
//! Mermaid's reference SVGs only holds when the layout pipeline runs
//! against the static DejaVu fixtures.

use mermaid_little::layout::journey::layout;
use mermaid_little::parser::journey::parse;
use mermaid_little::render::svg_journey::render;
use mermaid_little::theme::get_theme;
use std::fs;
use std::path::PathBuf;

fn id_for(rel: &str) -> String {
    let mut id = String::from("ref-");
    let mut last_was_sep = false;
    for c in rel.chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c);
            last_was_sep = false;
        } else if !last_was_sep {
            id.push('-');
            last_was_sep = true;
        }
    }
    if id.ends_with('-') {
        id.pop();
    }
    id
}

fn run(rel: &str) -> (String, String) {
    let mut mmd = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    mmd.push("tests");
    mmd.push(format!("{}.mmd", rel));
    let mut svg = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    svg.push("tests/reference");
    svg.push(format!("{}.svg", rel));

    let source = fs::read_to_string(&mmd).unwrap_or_else(|e| panic!("reading {:?}: {}", mmd, e));
    let expected = fs::read_to_string(&svg).unwrap_or_else(|e| panic!("reading {:?}: {}", svg, e));
    let id = id_for(rel);
    let theme = get_theme("default");
    let d = parse(&source).expect("parse");
    let l = layout(&d, &theme).expect("layout");
    let got = render(&d, &l, &theme, &id).expect("render");
    (got, expected)
}

#[track_caller]
fn assert_byte_exact(rel: &str) {
    let (got, expected) = run(rel);
    if got != expected {
        let idx = got
            .bytes()
            .zip(expected.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(got.len().min(expected.len()));
        let lo = idx.saturating_sub(60);
        let hi_g = (idx + 120).min(got.len());
        let hi_e = (idx + 120).min(expected.len());
        panic!(
            "mismatch in {} at byte {}\n GOT: ...{}...\n EXP: ...{}...\n",
            rel,
            idx,
            &got[lo..hi_g],
            &expected[lo..hi_e],
        );
    }
}

#[test]
fn cypress_01() {
    assert_byte_exact("ext_fixtures/cypress/journey/01");
}
#[test]
fn cypress_02() {
    assert_byte_exact("ext_fixtures/cypress/journey/02");
}
#[test]
fn cypress_03() {
    assert_byte_exact("ext_fixtures/cypress/journey/03");
}
#[test]
fn cypress_04() {
    assert_byte_exact("ext_fixtures/cypress/journey/04");
}
#[test]
fn cypress_05() {
    assert_byte_exact("ext_fixtures/cypress/journey/05");
}
#[test]
fn cypress_06() {
    assert_byte_exact("ext_fixtures/cypress/journey/06");
}
#[test]
fn cypress_07() {
    assert_byte_exact("ext_fixtures/cypress/journey/07");
}
#[test]
fn cypress_08() {
    assert_byte_exact("ext_fixtures/cypress/journey/08");
}
#[test]
fn cypress_09() {
    assert_byte_exact("ext_fixtures/cypress/journey/09");
}
#[test]
fn cypress_10() {
    assert_byte_exact("ext_fixtures/cypress/journey/10");
}
#[test]
fn demo_01() {
    assert_byte_exact("ext_fixtures/demos/journey/01");
}
