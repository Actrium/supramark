//! Dump the diagram_bytes (the FNV-1a input) for a given script so we can
//! compare against Go's `Diagram.Bytes()` output and find what part of the
//! JSON serialization differs.
//!
//! Usage: cargo run --example dump_hash_bytes -- "<script>"

use std::io::Write;

fn main() {
    let script = std::env::args().nth(1).expect("script");
    let opts = d2_little::CompileOptions {
        pad: Some(0),
        ..d2_little::CompileOptions::default()
    };
    let (diagram, _svg) = d2_little::compile(&script, &opts).expect("compile");

    let bytes = d2_little::target::go_json::diagram_bytes(&diagram);
    let h = diagram.hash_id(None);
    let (tl, br) = diagram.bounding_box();
    eprintln!(
        "len={} hash={} bbox=(({},{}),({},{})) w={} h={}",
        bytes.len(),
        h,
        tl.x,
        tl.y,
        br.x,
        br.y,
        br.x - tl.x,
        br.y - tl.y
    );
    std::io::stdout().write_all(&bytes).unwrap();
}
