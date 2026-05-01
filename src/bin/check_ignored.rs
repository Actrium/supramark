// Check whether any known_ignored fixtures now actually pass byte-exact.
// Surfaces false positives (entries that should be removed from the list).
use std::fs;
use std::path::PathBuf;

fn id_for(rel: &str) -> String {
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
    while id.ends_with('-') {
        id.pop();
    }
    id
}

fn main() {
    let text = fs::read_to_string("tests/known_ignored.txt").expect("read");
    let mut now_passing = vec![];
    let mut still_failing = 0usize;
    let mut no_reference = vec![];
    let mut errors = vec![];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let stem = line.split('\t').next().unwrap_or(line).trim();
        let rel = stem.trim_end_matches(".mmd");
        let mmd_path = PathBuf::from("tests").join(format!("{}.mmd", rel));
        let svg_path = PathBuf::from("tests/reference").join(format!("{}.svg", rel));
        let source = match fs::read_to_string(&mmd_path) {
            Ok(s) => s,
            Err(_) => {
                no_reference.push(rel.to_string());
                continue;
            }
        };
        let expected = match fs::read_to_string(&svg_path) {
            Ok(s) => s,
            Err(_) => {
                no_reference.push(rel.to_string());
                continue;
            }
        };
        let id = id_for(rel);
        let got = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mermaid_little::convert_with_id(&source, &id)
        })) {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                errors.push(format!("{}: {}", rel, e));
                continue;
            }
            Err(_) => {
                errors.push(format!("{}: panic", rel));
                continue;
            }
        };
        if got == expected {
            now_passing.push(rel.to_string());
        } else {
            still_failing += 1;
        }
    }
    println!("now_passing (false positives): {}", now_passing.len());
    for rel in &now_passing {
        println!("  {}", rel);
    }
    println!("\nstill_failing: {}", still_failing);
    println!("no_reference (mmd or svg missing): {}", no_reference.len());
    for rel in &no_reference {
        println!("  {}", rel);
    }
    println!("errored (parse/render error or panic): {}", errors.len());
    for e in errors.iter().take(20) {
        println!("  {}", e);
    }
}
