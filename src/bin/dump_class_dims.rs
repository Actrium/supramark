//! Print every class-layout node's geometry for one fixture.
//!
//! Usage:
//!   cargo run --bin dump_class_dims -- ext_fixtures/cypress/class/01

use mermaid_little::layout::class::layout as class_layout;
use mermaid_little::parser::class::parse;
use mermaid_little::theme::get_theme;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: dump_class_dims <fixture_rel>");
        eprintln!("  e.g.  dump_class_dims ext_fixtures/cypress/class/01");
        std::process::exit(1);
    }
    let rel = &args[1];
    let mmd = std::fs::read_to_string(format!("tests/{}.mmd", rel)).expect("read .mmd");
    let d = parse(&mmd).expect("parse");
    let theme = get_theme("default");
    let l = class_layout(&d, &theme).expect("layout");
    for n in &l.unified.nodes {
        println!(
            "node id={} shape={:?} w={:?} h={:?} x={:?} y={:?} group={} parent={:?}",
            n.id, n.shape, n.width, n.height, n.x, n.y, n.is_group, n.parent_id
        );
    }
    for e in &l.unified.edges {
        println!(
            "edge id={} src={:?} tgt={:?} pts={:?}",
            e.id, e.source, e.target, e.points
        );
    }
    println!("bounds={:?}", l.unified.bounds);
}
