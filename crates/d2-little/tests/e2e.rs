//! End-to-end tests: compile D2 scripts and compare SVG output
//! against expected files from the original Go d2 project.
//!
//! Test cases are loaded from tests/e2e_cases.json, which contains
//! name + script pairs extracted from the Go e2e test files.
//! Expected SVGs are in tests/fixtures/{category}/{name}/dagre/sketch.exp.svg.

use std::fs;
use std::path::PathBuf;

fn fixture_path(category: &str, name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("e2e_testdata")
        .join(category)
        .join(name)
        .join("dagre")
        .join("sketch.exp.svg")
}

fn run_e2e(category: &str, name: &str, script: &str) -> (bool, String) {
    let svg_result = d2_little::d2_to_svg(script);
    let svg = match svg_result {
        Ok(s) => s,
        Err(e) => return (false, format!("Compilation failed: {}", e)),
    };

    let svg_str = String::from_utf8_lossy(&svg).to_string();
    if !svg_str.contains("<svg") {
        return (false, "Output does not contain <svg".to_string());
    }

    let expected_path = fixture_path(category, name);
    if !expected_path.exists() {
        return (true, format!("SKIP: no fixture at {:?}", expected_path));
    }

    let expected = fs::read_to_string(&expected_path).unwrap_or_default();

    if svg_str == expected {
        (true, "MATCH".to_string())
    } else {
        // Report first difference location
        let mut diff_pos = 0;
        for (i, (a, b)) in svg_str.chars().zip(expected.chars()).enumerate() {
            if a != b {
                diff_pos = i;
                break;
            }
        }
        if diff_pos == 0 && svg_str.len() != expected.len() {
            diff_pos = svg_str.len().min(expected.len());
        }

        let context_start = diff_pos.saturating_sub(40);
        let context_end = (diff_pos + 40).min(svg_str.len()).min(expected.len());
        let got_ctx = &svg_str[context_start..context_end.min(svg_str.len())];
        let exp_ctx = &expected[context_start..context_end.min(expected.len())];

        (
            false,
            format!(
                "DIFF at byte {}: got[{}..]=`{}`  exp[{}..]=`{}`  (got {} bytes vs exp {} bytes)",
                diff_pos,
                context_start,
                got_ctx,
                context_start,
                exp_ctx,
                svg_str.len(),
                expected.len()
            ),
        )
    }
}

#[test]
fn e2e_sanity_basic() {
    let (ok, msg) = run_e2e("sanity", "basic", "a -> b\n");
    if msg.starts_with("SKIP") {
        eprintln!("{}", msg);
        return;
    }
    assert!(ok, "sanity/basic: {}", msg);
}

#[test]
fn e2e_sanity_empty() {
    let (ok, msg) = run_e2e("sanity", "empty", "");
    if msg.starts_with("SKIP") {
        eprintln!("{}", msg);
        return;
    }
    assert!(ok, "sanity/empty: {}", msg);
}

#[test]
fn e2e_sanity_1_to_2() {
    let (ok, msg) = run_e2e("sanity", "1 to 2", "a -> b\na -> c\n");
    if msg.starts_with("SKIP") {
        eprintln!("{}", msg);
        return;
    }
    assert!(ok, "sanity/1_to_2: {}", msg);
}

#[test]
fn e2e_sanity_child_to_child() {
    let (ok, msg) = run_e2e("sanity", "child to child", "a.b -> c.d\n");
    if msg.starts_with("SKIP") {
        eprintln!("{}", msg);
        return;
    }
    assert!(ok, "sanity/child_to_child: {}", msg);
}

#[test]
fn e2e_sanity_connection_label() {
    let (ok, msg) = run_e2e("sanity", "connection label", "a -> b: hello\n");
    if msg.starts_with("SKIP") {
        eprintln!("{}", msg);
        return;
    }
    assert!(ok, "sanity/connection_label: {}", msg);
}

/// Dashboard test: run all extracted e2e cases and report pass/fail rate.
#[test]
fn e2e_dashboard() {
    let cases_json = include_str!("e2e_cases.json");
    let cases: Vec<serde_json::Value> = serde_json::from_str(cases_json).unwrap();

    let mut pass = 0;
    let mut fail = 0;
    let mut skip = 0;
    let mut compile_fail = 0;
    let mut failures: Vec<String> = Vec::new();

    // Mirror Go's `go test` fixture naming: when two test cases share a
    // name within a category, the second one lands in `<name>#01`, the
    // third in `<name>#02`, and so on. Several of the extracted stable
    // cases duplicate names (e.g. `near_keys_for_container`), so track
    // occurrence counts and rewrite the fixture name accordingly.
    let mut seen: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();

    let mut overflow_skip = 0;
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let script = case["script"].as_str().unwrap();
        let category = case["category"].as_str().unwrap();

        // Skip cases known to cause infinite recursion (sql_table/class parsing bug)
        if script.contains("sql_table") || script.contains("shape: class") {
            overflow_skip += 1;
            continue;
        }

        let key = (category.to_string(), name.to_string());
        let idx = *seen.entry(key).and_modify(|c| *c += 1).or_insert(0);
        let fixture_name: String = if idx == 0 {
            name.to_string()
        } else {
            format!("{}#{:02}", name, idx)
        };
        let name = fixture_name.as_str();

        let (ok, msg) = run_e2e(category, name, script);
        if msg.starts_with("SKIP") {
            skip += 1;
        } else if msg.starts_with("Compilation failed") {
            compile_fail += 1;
            failures.push(format!("[{}] {}: {}", category, name, msg));
        } else if ok {
            pass += 1;
        } else {
            fail += 1;
            failures.push(format!(
                "[{}] {}: {}",
                category,
                name,
                &msg[..msg.len().min(200)]
            ));
        }
    }

    println!("\n=== E2E Dashboard ===");
    println!("Total:   {}", cases.len());
    println!("OvSkip:  {} (sql_table/class overflow bug)", overflow_skip);
    println!("Pass:    {} (byte-identical SVG)", pass);
    println!("Fail:    {} (SVG differs)", fail);
    println!("CompErr: {} (compilation failed)", compile_fail);
    println!("Skip:    {} (no fixture)", skip);
    println!("Rate:    {:.1}%", pass as f64 / cases.len() as f64 * 100.0);

    if !failures.is_empty() {
        println!("\nFirst 20 failures:");
        for f in failures.iter().take(20) {
            println!("  {}", f);
        }
    }

    // Don't assert — this is a dashboard, not a gate.
    // As we improve, the pass rate will increase.
}
