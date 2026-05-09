//! Dump the d2 IR for a given script for debugging compile order issues.

fn main() {
    let script = std::env::args().nth(1).expect("script");
    let (ast_map, parse_err) = d2_little::parser::parse("", &script);
    if let Some(e) = parse_err {
        eprintln!("parse error: {}", e);
        std::process::exit(1);
    }
    let ir_map = d2_little::ir::compile(&ast_map).expect("ir compile");
    print_map(&ir_map, 0);
}

fn print_map(m: &d2_little::ir::Map, depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{}Fields:", indent);
    for f in &m.fields {
        println!(
            "{}  - {} (primary={:?})",
            indent,
            f.name,
            f.primary.as_ref().map(|p| p.scalar_string())
        );
        if let Some(map) = f.map() {
            print_map(map, depth + 2);
        }
    }
    println!("{}Edges:", indent);
    for e in &m.edges {
        println!(
            "{}  - {} -> {}",
            indent,
            e.id.src_path.join("."),
            e.id.dst_path.join("."),
        );
    }
}
