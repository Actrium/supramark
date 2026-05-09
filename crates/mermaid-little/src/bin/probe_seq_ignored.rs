// Probe: render every known_ignored sequence fixture, compute diff_at vs
// reference, sort by diff_at descending. Largest diff_at fixtures render
// most of the SVG before diverging — usually one tiny feature away.
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

struct Row {
    rel: String,
    diff_at: usize,
    got_len: usize,
    exp_len: usize,
    got_ctx: String,
    exp_ctx: String,
    note: String,
}

fn main() {
    let text = fs::read_to_string("tests/known_ignored.txt").expect("read");
    let mut rows: Vec<Row> = Vec::new();
    let mut errors: Vec<(String, String)> = Vec::new();
    let mut passing: Vec<String> = Vec::new();

    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let stem = line.split('\t').next().unwrap_or(line).trim();
        if !stem.contains("/sequence/") {
            continue;
        }
        let note = line
            .split('\t')
            .nth(1)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let rel = stem.trim_end_matches(".mmd");
        let mmd_path = PathBuf::from("tests").join(format!("{}.mmd", rel));
        let svg_path = PathBuf::from("tests/reference").join(format!("{}.svg", rel));
        let Ok(source) = fs::read_to_string(&mmd_path) else {
            continue;
        };
        let Ok(expected) = fs::read_to_string(&svg_path) else {
            continue;
        };
        let id = id_for(rel);
        let got = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mermaid_little::convert_with_id(&source, &id)
        })) {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                errors.push((rel.to_string(), format!("err: {}", e)));
                continue;
            }
            Err(_) => {
                errors.push((rel.to_string(), "panic".into()));
                continue;
            }
        };
        if got == expected {
            passing.push(rel.to_string());
            continue;
        }
        let diff_at = got
            .bytes()
            .zip(expected.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| got.len().min(expected.len()));
        let lo = diff_at.saturating_sub(60);
        let hi_g = (diff_at + 200).min(got.len());
        let hi_e = (diff_at + 200).min(expected.len());
        let got_ctx = got
            .get(lo..hi_g)
            .unwrap_or("")
            .replace('\n', "\\n")
            .to_string();
        let exp_ctx = expected
            .get(lo..hi_e)
            .unwrap_or("")
            .replace('\n', "\\n")
            .to_string();
        rows.push(Row {
            rel: rel.to_string(),
            diff_at,
            got_len: got.len(),
            exp_len: expected.len(),
            got_ctx,
            exp_ctx,
            note,
        });
    }

    // Sort by diff_at DESCENDING (largest = closest to byte-exact).
    rows.sort_by(|a, b| b.diff_at.cmp(&a.diff_at));

    println!(
        "== seq probe: {} ignored fixtures (sorted by diff_at desc) ==",
        rows.len()
    );
    if !passing.is_empty() {
        println!("\n!! UNEXPECTED PASSING (false positives in known_ignored):");
        for r in &passing {
            println!("  {}", r);
        }
    }
    if !errors.is_empty() {
        println!("\n!! ERROR/PANIC:");
        for (r, e) in &errors {
            println!("  {} -- {}", r, e);
        }
    }

    let take = std::env::var("SEQ_PROBE_TAKE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(40);
    println!(
        "\n-- TOP {} (LARGEST DIFFS = closest to byte-exact) --",
        take.min(rows.len())
    );
    for r in rows.iter().take(take) {
        println!(
            "diff_at={:>5} (g={} e={}) {}",
            r.diff_at, r.got_len, r.exp_len, r.rel
        );
        println!("  note: {}", r.note);
        println!("  GOT: ...{}...", r.got_ctx);
        println!("  EXP: ...{}...", r.exp_ctx);
    }
}
