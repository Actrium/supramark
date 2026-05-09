#![cfg(feature = "metrics-static-dejavu")]
//! Timeline byte-exact test harness (Wave 2).
//!
//! Runs every fixture in `tests/ext_fixtures/{cypress,demos}/timeline`
//! through the Rust pipeline and diffs against the matching reference
//! SVG under `tests/reference/...`. Because the outer `convert_with_id`
//! entry point in `lib.rs` does not yet dispatch to the timeline
//! renderer, this file calls `parse`/`layout`/`render` directly.
//!
//! Compiled only with `metrics-static-dejavu` — byte parity vs upstream
//! Mermaid's reference SVGs only holds when the layout pipeline runs
//! against the static DejaVu fixtures.

use mermaid_little::layout::timeline::layout;
use mermaid_little::parser::timeline::parse;
use mermaid_little::render::svg_timeline::render;
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

    let d = parse(&source).expect("parse");
    let theme_name = d
        .theme_name
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let mut theme = get_theme(&theme_name);
    // Apply cScale overrides from frontmatter / %%init%%.
    for (i, v) in d.theme_overrides.c_scale.iter().enumerate() {
        if let Some(s) = v {
            match i {
                0 => theme.c_scale0 = Some(s.clone()),
                1 => theme.c_scale1 = Some(s.clone()),
                2 => theme.c_scale2 = Some(s.clone()),
                3 => theme.c_scale3 = Some(s.clone()),
                4 => theme.c_scale4 = Some(s.clone()),
                5 => theme.c_scale5 = Some(s.clone()),
                6 => theme.c_scale6 = Some(s.clone()),
                7 => theme.c_scale7 = Some(s.clone()),
                8 => theme.c_scale8 = Some(s.clone()),
                9 => theme.c_scale9 = Some(s.clone()),
                10 => theme.c_scale10 = Some(s.clone()),
                11 => theme.c_scale11 = Some(s.clone()),
                _ => {}
            }
        }
    }
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
        let hi_g = (idx + 80).min(got.len());
        let hi_e = (idx + 80).min(expected.len());
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
    assert_byte_exact("ext_fixtures/cypress/timeline/01");
}
#[test]
fn cypress_02() {
    assert_byte_exact("ext_fixtures/cypress/timeline/02");
}
#[test]
fn cypress_13() {
    assert_byte_exact("ext_fixtures/cypress/timeline/13");
}
#[test]
fn cypress_14() {
    assert_byte_exact("ext_fixtures/cypress/timeline/14");
}
#[test]
fn demo_01() {
    assert_byte_exact("ext_fixtures/demos/timeline/01");
}
#[test]
fn demo_02() {
    assert_byte_exact("ext_fixtures/demos/timeline/02");
}
