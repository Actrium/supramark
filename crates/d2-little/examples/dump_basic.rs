use std::fs;
fn main() {
    let svg = d2_little::d2_to_svg("a -> b").unwrap();
    let svg_str = String::from_utf8_lossy(&svg).to_string();
    let exp = fs::read_to_string("tests/e2e_testdata/sanity/basic/dagre/sketch.exp.svg").unwrap();
    let pos = svg_str
        .chars()
        .zip(exp.chars())
        .position(|(a, b)| a != b)
        .unwrap_or(svg_str.len().min(exp.len()));
    println!("ours len={}, exp len={}", svg_str.len(), exp.len());
    println!("first diff at {}", pos);
    let start = pos.saturating_sub(40);
    let end = (pos + 80).min(svg_str.len()).min(exp.len());
    println!("OURS: {:?}", &svg_str[start..end]);
    println!("EXP : {:?}", &exp[start..end]);
}
