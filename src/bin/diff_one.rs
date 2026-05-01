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
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: diff_one <kind> <num>");
        eprintln!("  e.g. diff_one gitGraph 01");
        std::process::exit(1);
    }
    let kind = &args[1];
    let num = &args[2];
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    for sub in ["cypress", "demos"] {
        let rel = format!("ext_fixtures/{}/{}/{}", sub, kind, num);
        let mmd_path = manifest.join("tests").join(format!("{}.mmd", rel));
        if !mmd_path.exists() {
            continue;
        }
        let svg_path = manifest
            .join("tests/reference")
            .join(format!("{}.svg", rel));
        let source = fs::read_to_string(&mmd_path).unwrap();
        let id = id_for(&rel);
        match mermaid_little::convert_with_id(&source, &id) {
            Ok(got) => {
                if let Ok(expected) = fs::read_to_string(&svg_path) {
                    if got == expected {
                        println!("PASS {}", rel);
                    } else {
                        let idx = got
                            .bytes()
                            .zip(expected.bytes())
                            .position(|(a, b)| a != b)
                            .unwrap_or(got.len().min(expected.len()));
                        let lo = idx.saturating_sub(80);
                        let hi_g = (idx + 200).min(got.len());
                        let hi_e = (idx + 200).min(expected.len());
                        println!("FAIL {} byte {} (got_len={} exp_len={})", rel, idx, got.len(), expected.len());
                        println!(" GOT: ...{}...", &got[lo..hi_g]);
                        println!(" EXP: ...{}...", &expected[lo..hi_e]);
                    }
                } else {
                    println!("NO_REF {}", rel);
                }
            }
            Err(e) => println!("ERR  {}: {}", rel, e),
        }
    }
}
