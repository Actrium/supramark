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
    if id.ends_with('-') { id.pop(); }
    id
}

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for num in ["07", "08"] {
        let rel = format!("ext_fixtures/demos/sequence/{}", num);
        let mmd_path = manifest.join("tests").join(format!("{}.mmd", rel));
        let source = fs::read_to_string(&mmd_path).unwrap();
        let id = id_for(&rel);
        match mermaid_little::convert_with_id(&source, &id) {
            Ok(svg) => {
                let out = format!("/tmp/mermaid_diff/our_{}.svg", num);
                fs::write(&out, svg).unwrap();
                eprintln!("Wrote {}", out);
            }
            Err(e) => eprintln!("ERR {}: {}", rel, e),
        }
    }
}
