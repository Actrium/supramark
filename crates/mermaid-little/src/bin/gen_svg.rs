use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: gen_svg <fixture_rel> <output_path>");
        std::process::exit(1);
    }
    let rel = &args[1];
    let out_path = &args[2];
    let base = std::path::PathBuf::from(".");
    let mmd = base.join("tests").join(format!("{}.mmd", rel));
    let source = fs::read_to_string(&mmd).unwrap();
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
    let got = mermaid_little::convert_with_id(&source, &id).unwrap();
    fs::write(out_path, &got).unwrap();
}
