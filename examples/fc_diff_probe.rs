use mermaid_little::layout::flowchart as layout_fc;
use mermaid_little::parser::flowchart as parser_fc;
use mermaid_little::render::svg_flowchart;
use mermaid_little::theme::get_theme;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut names: Vec<(String, String)> = Vec::new();
    if args.is_empty() {
        names.push(("ext_fixtures/cypress/flowchart".into(), "134".into()));
    } else if args[0] == "all" {
        for dir in ["ext_fixtures/cypress/flowchart", "ext_fixtures/demos/flowchart"] {
            let mut entries: Vec<String> = std::fs::read_dir(format!("tests/{}", dir))
                .unwrap()
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let p = e.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("mmd") {
                        Some(p.file_stem().unwrap().to_str().unwrap().to_string())
                    } else {
                        None
                    }
                })
                .collect();
            entries.sort();
            for stem in entries {
                names.push((dir.into(), stem));
            }
        }
    } else {
        for n in args {
            names.push(("ext_fixtures/cypress/flowchart".into(), n));
        }
    }
    for (dir, name) in &names {
        let mmd_path = format!("tests/{}/{}.mmd", dir, name);
        let svg_path = format!("tests/reference/{}/{}.svg", dir, name);
        let source = match fs::read_to_string(&mmd_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if source.trim_start().starts_with("flowchart-elk") {
            continue;
        }
        let expected = match fs::read_to_string(&svg_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let d = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parser_fc::parse(&source))) {
            Ok(Ok(d)) => d,
            _ => { println!("FC{} parse fail", name); continue; }
        };
        let theme = get_theme("default");
        let l = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| layout_fc::layout(&d, &theme))) {
            Ok(Ok(l)) => l,
            _ => { println!("FC{} layout fail", name); continue; }
        };
        let mut id = String::from("ref-");
        let mut last_was_sep = false;
        for c in format!("{}/{}", dir, name).chars() {
            if c.is_ascii_alphanumeric() {
                id.push(c);
                last_was_sep = false;
            } else if !last_was_sep {
                id.push('-');
                last_was_sep = true;
            }
        }
        if id.ends_with('-') {
            id.pop();
        }
        let got = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| svg_flowchart::render(&d, &l, &theme, &id))) {
            Ok(Ok(s)) => s,
            _ => { println!("FC{} render fail", name); continue; }
        };
        let a = got.as_bytes();
        let b = expected.as_bytes();
        let n = a.len().min(b.len());
        let mut i = 0;
        while i < n && a[i] == b[i] {
            i += 1;
        }
        if i >= n && a.len() == b.len() {
            // skip silent matches
            continue;
        }
        // For categorisation: print only the byte # plus 80 chars context
        let ctx_lo = i.saturating_sub(20);
        let ctx_hi_a = (i + 100).min(a.len());
        let ctx_hi_b = (i + 100).min(b.len());
        println!(
            "FC {}/{} byte={} got_len={} want_len={}",
            dir, name, i, a.len(), b.len()
        );
        println!("  got : {}", String::from_utf8_lossy(&a[ctx_lo..ctx_hi_a]).replace('\n', "\\n"));
        println!("  want: {}", String::from_utf8_lossy(&b[ctx_lo..ctx_hi_b]).replace('\n', "\\n"));
    }
}
