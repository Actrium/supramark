#[test]
fn gen_fixture_svg() {
    let cases = [
        ("ext_fixtures/cypress/sequence/76", "76"),
        ("ext_fixtures/demos/sequence/05", "05"),
        ("ext_fixtures/demos/sequence/07", "07"),
        ("ext_fixtures/demos/sequence/08", "08"),
    ];
    for (rel, num) in &cases {
        let mmd = format!("tests/{}.mmd", rel);
        let source = std::fs::read_to_string(&mmd).unwrap();
        let mut id = String::from("ref-");
        let mut last = false;
        for c in rel.chars() {
            if c.is_ascii_alphanumeric() {
                id.push(c);
                last = false;
            } else if !last {
                id.push('-');
                last = true;
            }
        }
        while id.ends_with('-') {
            id.pop();
        }
        match mermaid_little::convert_with_id(&source, &id) {
            Ok(svg) => {
                let out = std::env::temp_dir().join(format!("our_{}.svg", num));
                std::fs::write(&out, &svg).unwrap();
                eprintln!("{}: {} bytes -> {}", num, svg.len(), out.display());
            }
            Err(e) => eprintln!("ERR {}: {}", num, e),
        }
    }
}
