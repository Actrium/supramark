//! Byte-exact treemap fixtures — mirrors `wave1_e2e.rs` pattern.

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

fn check(rel: &str) -> Result<(), String> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mmd = base.join("tests").join(format!("{}.mmd", rel));
    let svg = base.join("tests/reference").join(format!("{}.svg", rel));
    let source = fs::read_to_string(&mmd).map_err(|e| format!("{:?}: {e}", mmd))?;
    let expected = fs::read_to_string(&svg).map_err(|e| format!("{:?}: {e}", svg))?;
    let id = id_for(rel);
    let got = convert_with_id(&source, &id).map_err(|e| format!("convert {rel}: {e}"))?;
    if got != expected {
        let byte = got
            .bytes()
            .zip(expected.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(got.len().min(expected.len()));
        let a_end = byte.saturating_add(120).min(got.len());
        let b_end = byte.saturating_add(120).min(expected.len());
        let a_start = byte.saturating_sub(30);
        let b_start = byte.saturating_sub(30);
        return Err(format!(
            "{rel} mismatch at byte {byte}\nGOT: {}\nEXP: {}",
            &got[a_start..a_end],
            &expected[b_start..b_end]
        ));
    }
    Ok(())
}

fn sweep(dirs: &[&str]) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut count = 0usize;
    let mut failures = Vec::new();
    for dir in dirs {
        let full = base.join("tests").join(dir);
        let entries = fs::read_dir(&full).unwrap_or_else(|e| panic!("reading {:?}: {}", full, e));
        let mut names: Vec<String> = Vec::new();
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("mmd") {
                continue;
            }
            names.push(path.file_stem().unwrap().to_str().unwrap().to_string());
        }
        names.sort();
        for stem in names {
            let rel = format!("{}/{}", dir, stem);
            count += 1;
            if let Err(e) = check(&rel) {
                failures.push(e);
            }
        }
    }
    assert!(count > 0, "no fixtures found under {:?}", dirs);
    if !failures.is_empty() {
        let shown = failures.len().min(10);
        let names: Vec<&str> = failures
            .iter()
            .map(|f| f.split(' ').next().unwrap_or(""))
            .collect();
        panic!(
            "{} / {} failures\nall failing: {}\nshowing {} detailed:\n{}",
            failures.len(),
            count,
            names.join(", "),
            shown,
            failures
                .iter()
                .take(shown)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n---\n")
        );
    }
    eprintln!("swept {} fixtures across {:?}", count, dirs);
}

#[test]
fn treemap_all_fixtures() {
    sweep(&["ext_fixtures/cypress/treemap", "ext_fixtures/demos/treemap"]);
}
