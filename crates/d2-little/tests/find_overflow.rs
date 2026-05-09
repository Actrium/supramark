//! Binary search for which test case causes stack overflow.
//! Run with: cargo test -p d2-little --test find_overflow -- --nocapture

fn try_compile(name: &str, script: &str) -> String {
    match d2_little::d2_to_svg(script) {
        Ok(svg) => format!("OK: {} ({} bytes)", name, svg.len()),
        Err(e) => format!("ERR: {}: {}", name, &e[..e.len().min(100)]),
    }
}

#[test]
fn find_overflow_case() {
    let cases_json = include_str!("e2e_cases.json");
    let cases: Vec<serde_json::Value> = serde_json::from_str(cases_json).unwrap();

    // Skip known problematic shapes and test each remaining case
    for (i, case) in cases.iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let script = case["script"].as_str().unwrap();

        // Skip cases known to cause infinite recursion
        // Skip cases that trigger infinite recursion bugs
        let skip_keywords = [
            "sql_table",
            "shape: class",
            "shape: sequence",
            "grid-rows",
            "grid-columns",
            "layers:",
            "scenarios:",
            "steps:",
            "near:",
            "@",
            "d2-config",
            "shape: grid",
        ];
        // Also skip cases with more than 10 edges (dagre bridge overflow)
        let edge_count = script.matches("->").count() + script.matches("<-").count();
        if skip_keywords.iter().any(|kw| script.contains(kw)) || edge_count > 10 {
            eprintln!("[{}/{}] SKIP (complex/large): {}", i + 1, cases.len(), name);
            continue;
        }

        eprintln!("[{}/{}] Testing: {} ...", i + 1, cases.len(), name);
        let result = try_compile(name, script);
        eprintln!("  {}", result);
    }
    eprintln!("Done!");
}
