use std::fs;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (name, cat) = (&args[1], &args[2]);
    let mmd_path = format!("tests/ext_fixtures/{}/sequence/{}.mmd", cat, name);
    let source = fs::read_to_string(&mmd_path).unwrap();
    let rel = format!("ext_fixtures/{}/sequence/{}", cat, name);
    let mut id = String::from("ref-");
    let mut last_sep = false;
    for c in rel.chars() {
        if c.is_ascii_alphanumeric() { id.push(c); last_sep = false; }
        else if !last_sep { id.push('-'); last_sep = true; }
    }
    while id.ends_with('-') { id.pop(); }
    let got = mermaid_little::convert_with_id(&source, &id).unwrap();
    let ref_path = format!("tests/reference/ext_fixtures/{}/sequence/{}.svg", cat, name);
    let exp = fs::read_to_string(&ref_path).unwrap();
    let got_b = got.as_bytes(); let exp_b = exp.as_bytes();
    if got == exp { println!("EXACT MATCH"); return; }
    println!("DIFF got={} exp={}", got.len(), exp.len());
    let min_len = got_b.len().min(exp_b.len());
    let mut count = 0;
    let mut i = 0;
    while i < min_len && count < 10 {
        if got_b[i] != exp_b[i] {
            let start = i.saturating_sub(30);
            let end = (i + 40).min(min_len);
            println!("  byte {}: got={:?} exp={:?}", i,
                String::from_utf8_lossy(&got_b[start..end.min(got_b.len())]),
                String::from_utf8_lossy(&exp_b[start..end.min(exp_b.len())]));
            i += 1; count += 1;
        } else { i += 1; }
    }
}
