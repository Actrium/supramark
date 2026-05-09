use std::fs;
use std::path::PathBuf;

fn id_for(rel: &str) -> String {
    let mut id = String::from("ref-");
    let mut last_sep = false;
    for c in rel.chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c);
            last_sep = false;
        } else if !last_sep {
            id.push('-');
            last_sep = true;
        }
    }
    while id.ends_with('-') {
        id.pop();
    }
    id
}

fn is_elk(src: &str) -> bool {
    src.contains("flowchart-elk") || src.contains("layout: elk")
}

fn read_known_ignored() -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    let path = PathBuf::from("tests/known_ignored.txt");
    if let Ok(text) = fs::read_to_string(&path) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let stem = line.split('\t').next().unwrap_or(line).trim();
            let stem = stem.trim_end_matches(".mmd");
            set.insert(stem.to_string());
        }
    }
    set
}

fn main() {
    let ignored = read_known_ignored();
    let mut categories: Vec<(String, Vec<(String, bool, String)>)> = Vec::new();
    let mut by_cat: std::collections::BTreeMap<String, Vec<(String, bool, String)>> =
        std::collections::BTreeMap::new();

    let bases = ["tests/ext_fixtures/cypress", "tests/ext_fixtures/demos"];
    for base in &bases {
        let basep = PathBuf::from(base);
        let cats = match fs::read_dir(&basep) {
            Ok(it) => it,
            Err(_) => continue,
        };
        for cat in cats.filter_map(|e| e.ok()) {
            if !cat.path().is_dir() {
                continue;
            }
            let cat_name = cat.file_name().to_string_lossy().to_string();
            let key = format!("{}/{}", basep.file_name().unwrap().to_string_lossy(), cat_name);
            let key = format!(
                "{}/{}",
                basep
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                cat_name
            );
            let _ = key;
            let cat_key = format!(
                "{}/{}",
                basep.file_name().unwrap().to_string_lossy(),
                cat_name
            );
            let _ = cat_key;
            let cat_disp = format!(
                "{}/{}",
                basep.file_name().unwrap().to_string_lossy(),
                cat_name
            );
            let mut entries: Vec<_> = match fs::read_dir(cat.path()) {
                Ok(it) => it.filter_map(|e| e.ok()).collect(),
                Err(_) => continue,
            };
            entries.sort_by_key(|e| e.file_name());
            for entry in &entries {
                let fname = entry.file_name();
                let fname = fname.to_string_lossy().to_string();
                if !fname.ends_with(".mmd") {
                    continue;
                }
                let stem = fname.trim_end_matches(".mmd").to_string();
                let rel = format!("ext_fixtures/{}/{}", cat_disp, stem);
                if ignored.contains(&rel) {
                    continue;
                }
                let mmd_path = entry.path();
                let source = match fs::read_to_string(&mmd_path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if is_elk(&source) {
                    continue;
                }
                let svg_path = PathBuf::from("tests/reference").join(format!("{}.svg", rel));
                let expected = match fs::read_to_string(&svg_path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let id = id_for(&rel);
                let got = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    mermaid_little::convert_with_id(&source, &id)
                })) {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => {
                        by_cat
                            .entry(cat_disp.clone())
                            .or_default()
                            .push((stem, false, format!("error: {}", e)));
                        continue;
                    }
                    Err(_) => {
                        by_cat
                            .entry(cat_disp.clone())
                            .or_default()
                            .push((stem, false, "panic".into()));
                        continue;
                    }
                };
                let pass = mermaid_little::svg_match::svg_match_tolerant(&got, &expected);
                let detail = if pass {
                    String::new()
                } else {
                    let idx = got
                        .bytes()
                        .zip(expected.bytes())
                        .position(|(a, b)| a != b)
                        .unwrap_or(got.len().min(expected.len()));
                    format!("byte={} got={} exp={}", idx, got.len(), expected.len())
                };
                by_cat
                    .entry(cat_disp.clone())
                    .or_default()
                    .push((stem, pass, detail));
            }
        }
    }

    let only_failing = std::env::args().any(|a| a == "--only-failing");
    let mut total_p = 0usize;
    let mut total_t = 0usize;
    for (cat, items) in &by_cat {
        let p = items.iter().filter(|x| x.1).count();
        let t = items.len();
        total_p += p;
        total_t += t;
        println!("{}: {}/{}", cat, p, t);
        if only_failing {
            for (stem, pass, detail) in items {
                if !pass {
                    println!("  FAIL {} {}", stem, detail);
                }
            }
        }
    }
    println!("==== TOTAL: {}/{} ====", total_p, total_t);
    let _ = categories;
}
