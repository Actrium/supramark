use std::fs;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rel = &args[1];
    let out_path = &args[2];
    let mmd = format!("tests/{}.mmd", rel);
    let source = fs::read_to_string(&mmd).unwrap();
    // mirror generate_ref.mjs idForPath: 'ref-' + rel.replace(/[^a-zA-Z0-9]+/g, '-')
    let mut id = String::from("ref-");
    let mut prev_dash = false;
    for c in rel.chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c);
            prev_dash = false;
        } else if !prev_dash {
            id.push('-');
            prev_dash = true;
        }
    }
    if id.ends_with('-') {
        id.pop();
    }
    match mermaid_little::convert_with_id(&source, &id) {
        Ok(s) => fs::write(out_path, &s).unwrap(),
        Err(e) => {
            eprintln!("ERR: {:?}", e);
            std::process::exit(1);
        }
    }
}
