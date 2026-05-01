fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cat = &args[1];
    let stem = &args[2];
    let src = std::fs::read_to_string(format!("tests/ext_fixtures/cypress/{}/{}.mmd", cat, stem)).unwrap();
    let exp = std::fs::read_to_string(format!("tests/reference/ext_fixtures/cypress/{}/{}.svg", cat, stem)).unwrap();
    let id = format!("ref-ext-fixtures-cypress-{}-{}", cat, stem);
    let got = mermaid_little::convert_with_id(&src, &id).unwrap();
    if got == exp { println!("MATCH"); return; }
    let idx = got.bytes().zip(exp.bytes()).position(|(a, b)| a != b).unwrap_or(got.len().min(exp.len()));
    let s = idx.saturating_sub(80);
    let e = (idx+250).min(got.len()).min(exp.len());
    println!("--- byte {} (got len={}, exp len={}) ---", idx, got.len(), exp.len());
    println!("GOT: {}", &got[s..e]);
    println!();
    println!("EXP: {}", &exp[s..e]);
}
