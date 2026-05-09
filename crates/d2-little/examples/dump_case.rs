//! Dump the first byte difference for an arbitrary e2e case.
//!
//! Usage: cargo run --example dump_case -- <category> <name> <script>
//! e.g.   cargo run --example dump_case -- stable bold-mono "$(cat - << 'D2'
//!        not bold mono.style.font: mono
//!        not bold mono.style.bold: false
//!        bold mono.style.font: mono
//!        D2
//!        )"

use std::fs;

fn main() {
    let mut args = std::env::args().skip(1);
    let category = args.next().expect("category");
    let name = args.next().expect("name");
    let script = args.next().expect("script");
    let theme_id: Option<i64> = args.next().and_then(|v| v.parse().ok());

    let opts = d2_little::CompileOptions {
        pad: Some(0),
        theme_id,
        ..d2_little::CompileOptions::default()
    };
    let svg = match d2_little::compile(&script, &opts) {
        Ok((diagram, s)) => {
            if std::env::var("DUMP_DIAG").is_ok() {
                for shape in &diagram.shapes {
                    eprintln!(
                        "SHAPE id={} type={} label={:?} w={} h={} labelW={} labelH={}",
                        shape.id,
                        shape.type_,
                        shape.text.label,
                        shape.width,
                        shape.height,
                        shape.text.label_width,
                        shape.text.label_height
                    );
                }
            }
            s
        }
        Err(e) => {
            eprintln!("ERR: {}", e);
            std::process::exit(1);
        }
    };
    let svg_str = String::from_utf8_lossy(&svg).to_string();
    let exp_path = format!(
        "tests/e2e_testdata/{}/{}/dagre/sketch.exp.svg",
        category, name
    );
    let exp = fs::read_to_string(&exp_path).expect("read fixture");

    // Dump both SVGs to /tmp for external diffing tools.
    let _ = fs::write("/tmp/ours.svg", &svg_str);
    let _ = fs::write("/tmp/exp.svg", &exp);

    let pos = svg_str
        .chars()
        .zip(exp.chars())
        .position(|(a, b)| a != b)
        .unwrap_or(svg_str.len().min(exp.len()));
    println!("ours len={}, exp len={}", svg_str.len(), exp.len());
    println!("first diff at {}", pos);

    let start = pos.saturating_sub(80);
    let end_o = (pos + 120).min(svg_str.len());
    let end_e = (pos + 120).min(exp.len());
    println!("OURS: {:?}", &svg_str[start..end_o]);
    println!("EXP : {:?}", &exp[start..end_e]);
}
