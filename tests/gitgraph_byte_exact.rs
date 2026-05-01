//! gitGraph byte-exact test harness.
//!
//! Runs the fixtures in `tests/ext_fixtures/{cypress,demos}/gitGraph`
//! supported by the current minimal port through the Rust pipeline
//! and diffs against the matching reference SVG.
//!
//! Fixtures requiring features not yet ported (multi-branch, merge,
//! cherry-pick, TB/BT orientation, custom mainBranchName) are listed
//! in `tests/known_ignored.txt` and skipped here.

use mermaid_little::convert_with_id;
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

#[track_caller]
fn assert_byte_exact(rel: &str) {
    let mut mmd = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    mmd.push("tests");
    mmd.push(format!("{}.mmd", rel));
    let mut svg = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    svg.push("tests/reference");
    svg.push(format!("{}.svg", rel));

    let source = fs::read_to_string(&mmd).unwrap_or_else(|e| panic!("reading {:?}: {}", mmd, e));
    let expected = fs::read_to_string(&svg).unwrap_or_else(|e| panic!("reading {:?}: {}", svg, e));
    let id = id_for(rel);
    let got = convert_with_id(&source, &id).unwrap_or_else(|e| panic!("convert {}: {}", rel, e));

    if got == expected {
        return;
    }
    let idx = got
        .bytes()
        .zip(expected.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(got.len().min(expected.len()));
    let lo = idx.saturating_sub(60);
    let hi_g = (idx + 200).min(got.len());
    let hi_e = (idx + 200).min(expected.len());
    panic!(
        "mismatch in {} at byte {} (got_len={} exp_len={})\n GOT: ...{}...\n EXP: ...{}...\n",
        rel,
        idx,
        got.len(),
        expected.len(),
        &got[lo..hi_g],
        &expected[lo..hi_e],
    );
}

#[test]
fn cypress_01() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/01");
}

#[test]
fn cypress_02() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/02");
}

#[test]
fn cypress_03_reverse_highlight() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/03");
}

#[test]
fn cypress_04_tags() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/04");
}

#[test]
fn cypress_09_init_rotate() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/09");
}

#[test]
fn cypress_10_no_rotate() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/10");
}

#[test]
fn cypress_17_frontmatter_title() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/17");
}
