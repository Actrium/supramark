//! State diagram byte-exact test harness.
//!
//! Runs fixtures in `tests/ext_fixtures/cypress/state` through the Rust
//! pipeline and diffs against the matching reference SVG.

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

/// Print a diff summary for all state fixtures (used for manual debugging).
#[test]
#[ignore]
fn sweep_all_state_fixtures() {
    sweep_dir("tests/ext_fixtures/cypress/state", "ext_fixtures/cypress/state");
    sweep_dir("tests/ext_fixtures/demos/state", "ext_fixtures/demos/state");
}

fn sweep_dir(dir_rel: &str, ref_prefix: &str) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir_rel);
    let mut pass = 0;
    let mut fail = 0;
    let mut entries: Vec<_> = fs::read_dir(&base)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in &entries {
        let fname = entry.file_name();
        let name = fname.to_string_lossy();
        if !name.ends_with(".mmd") {
            continue;
        }
        let stem = name.trim_end_matches(".mmd");
        let rel = format!("{}/{}", ref_prefix, stem);
        let id = id_for(&rel);
        let source = fs::read_to_string(entry.path()).unwrap();
        let svg_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/reference")
            .join(format!("{}/{}.svg", ref_prefix, stem));
        let expected = match fs::read_to_string(&svg_path) {
            Ok(s) => s,
            Err(_) => {
                println!("  SKIP {} (no reference)", stem);
                continue;
            }
        };
        match convert_with_id(&source, &id) {
            Ok(got) if got == expected => {
                pass += 1;
                println!("  PASS {}", stem);
            }
            Ok(got) => {
                fail += 1;
                let idx = got
                    .bytes()
                    .zip(expected.bytes())
                    .position(|(a, b)| a != b)
                    .unwrap_or(got.len().min(expected.len()));
                let lo = idx.saturating_sub(30);
                let hi_g = (idx + 80).min(got.len());
                let hi_e = (idx + 80).min(expected.len());
                println!(
                    "  FAIL {} byte={} got_len={} exp_len={}\n    G: ...{}...\n    E: ...{}...",
                    stem,
                    idx,
                    got.len(),
                    expected.len(),
                    &got[lo..hi_g],
                    &expected[lo..hi_e]
                );
            }
            Err(e) => {
                fail += 1;
                println!("  ERR  {} => {}", stem, e);
            }
        }
    }
    println!("\nResult [{}]: {}/{} passed", ref_prefix, pass, pass + fail);
}

#[test]
fn cypress_01() {
    assert_byte_exact("ext_fixtures/cypress/state/01");
}
#[test]
fn cypress_02() {
    assert_byte_exact("ext_fixtures/cypress/state/02");
}
#[test]
fn cypress_03() {
    assert_byte_exact("ext_fixtures/cypress/state/03");
}
#[test]
fn cypress_04() {
    assert_byte_exact("ext_fixtures/cypress/state/04");
}
/// cy/05 is an `info` diagram (unsupported diagram type), not a state diagram.
#[test]
#[ignore]
fn cypress_05() {
    assert_byte_exact("ext_fixtures/cypress/state/05");
}
#[test]
fn cypress_06() {
    assert_byte_exact("ext_fixtures/cypress/state/06");
}
#[test]
fn cypress_07() {
    assert_byte_exact("ext_fixtures/cypress/state/07");
}
#[test]
fn cypress_08() {
    assert_byte_exact("ext_fixtures/cypress/state/08");
}
#[test]
fn cypress_09() {
    assert_byte_exact("ext_fixtures/cypress/state/09");
}
#[test]
fn cypress_10() {
    assert_byte_exact("ext_fixtures/cypress/state/10");
}
#[test]
fn cypress_11() {
    assert_byte_exact("ext_fixtures/cypress/state/11");
}
#[test]
fn cypress_12() {
    assert_byte_exact("ext_fixtures/cypress/state/12");
}
#[test]
fn cypress_13() {
    assert_byte_exact("ext_fixtures/cypress/state/13");
}
#[test]
fn cypress_14() {
    assert_byte_exact("ext_fixtures/cypress/state/14");
}
#[test]
fn cypress_15() {
    assert_byte_exact("ext_fixtures/cypress/state/15");
}

/// Dump diff for one fixture (set FIXTURE env var or default 26).
#[test]
#[ignore]
fn debug_one_fixture() {
    let stem = std::env::var("FIXTURE").unwrap_or_else(|_| "26".to_string());
    let dir = std::env::var("FIXDIR").unwrap_or_else(|_| "cypress".to_string());
    let rel = format!("ext_fixtures/{}/state/{}", dir, stem);
    let mmd = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(format!("{}.mmd", rel));
    let svg = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/reference")
        .join(format!("{}.svg", rel));
    let source = fs::read_to_string(&mmd).unwrap();
    let expected = fs::read_to_string(&svg).unwrap();
    let id = id_for(&rel);
    let got = mermaid_little::convert_with_id(&source, &id).unwrap();
    let outdir = std::path::Path::new("/tmp/state_dump");
    let _ = std::fs::create_dir_all(outdir);
    std::fs::write(outdir.join(format!("{}.got.svg", stem)), &got).unwrap();
    std::fs::write(outdir.join(format!("{}.exp.svg", stem)), &expected).unwrap();
    println!(
        "wrote /tmp/state_dump/{}.{{got,exp}}.svg got_len={} exp_len={}",
        stem,
        got.len(),
        expected.len()
    );
}

/// Print full SVG output for cy/11 for debugging.
#[test]
#[ignore]
fn debug_cy11_output() {
    let source = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ext_fixtures/cypress/state/11.mmd"),
    )
    .unwrap();
    let id = id_for("ext_fixtures/cypress/state/11");
    let got = mermaid_little::convert_with_id(&source, &id).unwrap();
    // Print the cluster and node sections
    let cluster_start = got.find("<g class=\"clusters\">");
    let nodes_end = got.find("</g></g></g></svg>").unwrap_or(got.len());
    if let Some(start) = cluster_start {
        println!("SVG CONTENT:\n{}", &got[start..nodes_end.min(start + 3000)]);
    }
}
