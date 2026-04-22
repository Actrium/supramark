use mermaid_little::convert_with_id;
use std::fs;
use std::path::PathBuf;

fn check(rel: &str) {
    let mut mmd = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    mmd.push("tests");
    mmd.push(format!("{}.mmd", rel));
    let mut svg = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    svg.push("tests/reference");
    svg.push(format!("{}.svg", rel));

    let source = fs::read_to_string(&mmd).unwrap_or_else(|e| panic!("reading {:?}: {}", mmd, e));
    let expected = fs::read_to_string(&svg).unwrap_or_else(|e| panic!("reading {:?}: {}", svg, e));
    // Match tests/support/generate_ref.mjs::idForPath — runs of non-
    // alphanumeric chars collapse to a single '-'.
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
    if id.ends_with('-') { id.pop(); }
    let got = convert_with_id(&source, &id).unwrap_or_else(|e| panic!("convert {}: {}", rel, e));
    if got != expected {
        let byte = got.bytes().zip(expected.bytes()).position(|(a, b)| a != b).unwrap_or(got.len().min(expected.len()));
        panic!("mismatch on {} at byte {}\nGOT: {}\nEXP: {}", rel, byte, &got[byte.saturating_sub(30)..byte.saturating_add(60).min(got.len())], &expected[byte.saturating_sub(30)..byte.saturating_add(60).min(expected.len())]);
    }
}

fn sweep(dirs: &[&str]) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut count = 0;
    for dir in dirs {
        let full = base.join("tests").join(dir);
        let entries = fs::read_dir(&full).unwrap_or_else(|e| panic!("reading {:?}: {}", full, e));
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("mmd") {
                continue;
            }
            let stem = path.file_stem().unwrap().to_str().unwrap();
            let rel = format!("{}/{}", dir, stem);
            check(&rel);
            count += 1;
        }
    }
    assert!(count > 0, "no fixtures found under {:?}", dirs);
    eprintln!("swept {} fixtures across {:?}", count, dirs);
}

#[test]
fn pie_all_fixtures() {
    sweep(&[
        "fixtures/pie",
        "ext_fixtures/demos/pie",
        "ext_fixtures/cypress/pie",
    ]);
}

#[test]
fn packet_all_fixtures() {
    sweep(&["ext_fixtures/cypress/packet"]);
}

#[test]
fn radar_all_fixtures() {
    sweep(&["ext_fixtures/cypress/radar", "ext_fixtures/demos/radar"]);
}
