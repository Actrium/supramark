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

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut err = 0usize;
    let mut no_ref = 0usize;
    let mut total = 0usize;
    let mut passing: Vec<String> = Vec::new();
    let mut failing: Vec<String> = Vec::new();
    let mut erroring: Vec<(String, String)> = Vec::new();
    for sub in ["cypress", "demos"] {
        let dir = manifest
            .join("tests/ext_fixtures")
            .join(sub)
            .join("gitGraph");
        let mut entries: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in &entries {
            let fname = entry.file_name();
            let name = fname.to_string_lossy();
            if !name.ends_with(".mmd") {
                continue;
            }
            total += 1;
            let stem = name.trim_end_matches(".mmd");
            let rel = format!("ext_fixtures/{}/gitGraph/{}", sub, stem);
            let svg_path = manifest
                .join("tests/reference")
                .join(format!("{}.svg", rel));
            let source = fs::read_to_string(entry.path()).unwrap();
            let id = id_for(&rel);
            match mermaid_little::convert_with_id(&source, &id) {
                Ok(got) => match fs::read_to_string(&svg_path) {
                    Ok(expected) => {
                        if got == expected {
                            pass += 1;
                            passing.push(rel);
                        } else {
                            fail += 1;
                            failing.push(rel);
                        }
                    }
                    Err(_) => {
                        no_ref += 1;
                    }
                },
                Err(e) => {
                    err += 1;
                    erroring.push((rel, e.to_string()));
                }
            }
        }
    }
    println!("=== gitGraph sweep ===");
    println!("PASS: {}", pass);
    println!("FAIL: {}", fail);
    println!("ERR : {}", err);
    println!("NO_REF: {}", no_ref);
    println!("Total: {}", total);
    println!("\n--- PASSING ---");
    for p in &passing {
        println!("  {}", p);
    }
    println!("\n--- FAILING ---");
    for p in &failing {
        println!("  {}", p);
    }
    println!("\n--- ERRORING ---");
    for (p, e) in &erroring {
        println!("  {}: {}", p, e);
    }
}
