use mermaid_little::layout::er as layout_er;
use mermaid_little::parser::er as parser_er;
use mermaid_little::render::svg_er;
use mermaid_little::theme::get_theme;
use std::fs;

fn main() {
    let names = ["01"];
    for name in &names {
        let mmd_path = format!("tests/ext_fixtures/demos/er/{}.mmd", name);
        let svg_path = format!("tests/reference/ext_fixtures/demos/er/{}.svg", name);
        let source = fs::read_to_string(&mmd_path).unwrap();
        let expected = fs::read_to_string(&svg_path).unwrap();
        let d = parser_er::parse(&source).unwrap();
        let theme = get_theme("default");
        let l = layout_er::layout(&d, &theme).unwrap();
        let id = format!("ref-ext-fixtures-demos-er-{}", name);
        let got = svg_er::render(&d, &l, &theme, &id).unwrap();
        let a = got.as_bytes();
        let b = expected.as_bytes();
        let n = a.len().min(b.len());
        let mut i = 0;
        while i < n && a[i] == b[i] {
            i += 1;
        }
        if i >= n && a.len() == b.len() {
            println!("ER{} BYTE EXACT!", name);
            continue;
        }
        let ctx_lo = i.saturating_sub(40);
        let ctx_hi_a = (i + 200).min(a.len());
        let ctx_hi_b = (i + 200).min(b.len());
        println!(
            "ER{} diverge at byte {} (got={}, want={})",
            name,
            i,
            a.len(),
            b.len()
        );
        println!(
            "got [{}..]: {}",
            ctx_lo,
            String::from_utf8_lossy(&a[ctx_lo..ctx_hi_a])
        );
        println!(
            "want[{}..]: {}",
            ctx_lo,
            String::from_utf8_lossy(&b[ctx_lo..ctx_hi_b])
        );
    }
}
