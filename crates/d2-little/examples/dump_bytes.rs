fn main() {
    let input = std::env::args().nth(1).unwrap_or_default();
    let opts = d2_little::CompileOptions {
        pad: Some(0),
        ..d2_little::CompileOptions::default()
    };
    let (diagram, _) = d2_little::compile(&input, &opts).unwrap();
    let bytes = d2_little::target::go_json::diagram_bytes(&diagram);
    println!("{}", String::from_utf8_lossy(&bytes));
    println!("hash: {}", diagram.hash_id(None));
}
