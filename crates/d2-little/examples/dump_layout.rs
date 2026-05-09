//! Dump the post-layout positions of all shapes for a given script. Used to
//! diagnose dagre layout-order issues.
//!
//! Usage: cargo run --example dump_layout -- "<script>"

fn main() {
    let script = std::env::args().nth(1).expect("script");
    let opts = d2_little::CompileOptions {
        pad: Some(0),
        ..d2_little::CompileOptions::default()
    };
    let (diagram, _) = d2_little::compile(&script, &opts).expect("compile");

    println!("shapes:");
    for s in &diagram.shapes {
        println!(
            "  id={:<10} pos=({:>4},{:>4}) size=({:>3}x{:>3})",
            s.id, s.pos.x, s.pos.y, s.width, s.height
        );
    }
    println!("connections:");
    for c in &diagram.connections {
        println!(
            "  {:<6} -> {:<6} src_arrow={:?} route={:?}",
            c.src,
            c.dst,
            c.src_arrow,
            c.route.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>()
        );
    }
}
