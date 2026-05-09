//! E2E runner for the full dagre/SVG-applicable Go corpus.
//! Each case executes in its own subprocess so crashes/timeouts stay isolated.

#[path = "common/mod.rs"]
mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

const DAGRE_LIMITS_URL: &str = "https://d2lang.com/tour/layouts/#layout-specific-functionality";

#[derive(Debug, serde::Deserialize)]
struct Case {
    family: String,
    name: String,
    #[allow(dead_code)]
    fixture_name: Option<String>,
    script: String,
    expected_kind: String,
    expected_message: Option<String>,
    svg_relpath: Option<String>,
    theme_id: i64,
    dark_theme_id: Option<i64>,
    sketch: bool,
    #[serde(default)]
    use_measured_texts: bool,
    #[serde(default)]
    test_serialization: bool,
    #[allow(dead_code)]
    source: Option<String>,
}

#[derive(Debug)]
enum CaseResult {
    Svg(Vec<u8>),
    CompileError(String),
    FeatureError(String),
}

fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("e2e_dagre_svg_cases.json")).unwrap()
}

fn svg_fixture_path(case: &Case) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("e2e_testdata")
        .join(case.svg_relpath.as_ref().expect("svg_relpath for svg case"))
}

fn is_grid_diagram_like(obj: &d2_little::graph::Object) -> bool {
    obj.grid_rows.is_some() || obj.grid_columns.is_some()
}

fn is_container_for_feature_check(obj: &d2_little::graph::Object) -> bool {
    obj.is_container()
        && obj.class.is_none()
        && obj.sql_table.is_none()
        && !obj
            .shape
            .value
            .eq_ignore_ascii_case(d2_little::target::SHAPE_CLASS)
        && !obj
            .shape
            .value
            .eq_ignore_ascii_case(d2_little::target::SHAPE_SQL_TABLE)
}

fn dagre_feature_support_error(g: &d2_little::graph::Graph) -> Option<String> {
    for edge in &g.edges {
        let src = edge.src;
        let dst = edge.dst;
        let src_obj = &g.objects[src];
        let dst_obj = &g.objects[dst];

        if src_obj.outer_sequence_diagram(g).is_some()
            || dst_obj.outer_sequence_diagram(g).is_some()
        {
            continue;
        }

        if !is_container_for_feature_check(src_obj) && !is_container_for_feature_check(dst_obj) {
            continue;
        }

        if src == dst {
            return Some(format!(
                "Connection \"{}\" is a self loop on a container, but layout engine \"dagre\" does not support this. See {} for more.",
                edge.abs_id(),
                DAGRE_LIMITS_URL
            ));
        }

        if src_obj.is_descendant_of(src, dst, g) || dst_obj.is_descendant_of(dst, src, g) {
            return Some(format!(
                "Connection \"{}\" goes from a container to a descendant, but layout engine \"dagre\" does not support this. See {} for more.",
                edge.abs_id(),
                DAGRE_LIMITS_URL
            ));
        }
    }

    for (i, obj) in g.objects.iter().enumerate() {
        if i == g.root {
            continue;
        }
        if is_container_for_feature_check(obj)
            && !is_grid_diagram_like(obj)
            && (obj.width_attr.is_some() || obj.height_attr.is_some())
        {
            return Some(format!(
                "Object \"{}\" has attribute \"width\" and/or \"height\" set, but layout engine \"dagre\" does not support dimensions set on containers. See {} for more.",
                obj.abs_id(),
                DAGRE_LIMITS_URL
            ));
        }
    }

    None
}

fn execute_case(case: &Case) -> CaseResult {
    let g = match d2_little::compiler::compile("", &case.script) {
        Ok(g) => g,
        Err(e) => return CaseResult::CompileError(e.to_string()),
    };

    if let Some(msg) = dagre_feature_support_error(&g) {
        return CaseResult::FeatureError(msg);
    }

    // The Go harness passes MeasuredTexts only for the measured family.
    // d2-little does not expose that hook yet, so these cases still run through
    // the normal compile path for now; the flag remains in the manifest so the
    // missing plumbing stays visible in the test model.
    let _use_measured_texts = case.use_measured_texts;
    let _test_serialization = case.test_serialization;

    let opts = d2_little::CompileOptions {
        theme_id: Some(case.theme_id),
        dark_theme_id: case.dark_theme_id,
        pad: Some(0),
        sketch: case.sketch,
        ..d2_little::CompileOptions::default()
    };
    match d2_little::compile(&case.script, &opts) {
        Ok((_, svg)) => CaseResult::Svg(svg),
        Err(e) => CaseResult::CompileError(e),
    }
}

fn maybe_run_single_case() -> bool {
    let idx_str = match std::env::var("E2E_CASE_INDEX") {
        Ok(s) => s,
        Err(_) => return false,
    };
    // Tests run inside this child process call into latex::render via the
    // svg_render pipeline; install the rquickjs-backed engine before any
    // case touches a `tex:` block.
    common::latex_engine::install();
    let idx: usize = idx_str.parse().unwrap();
    let all_cases = cases();
    let case = &all_cases[idx];
    match execute_case(case) {
        CaseResult::Svg(svg) => {
            print!("{}", String::from_utf8_lossy(&svg));
            true
        }
        CaseResult::CompileError(msg) => {
            eprint!("COMPILE:{}", msg);
            std::process::exit(2);
        }
        CaseResult::FeatureError(msg) => {
            eprint!("FEATURE:{}", msg);
            std::process::exit(3);
        }
    }
}

fn extract_svg_payload(stdout: &[u8]) -> Option<String> {
    let full = String::from_utf8_lossy(stdout);
    let start = full.find("<?xml")?;
    let end = full.rfind("</svg>")?;
    Some(full[start..end + "</svg>".len()].to_string())
}

fn extract_prefixed_message(bytes: &[u8], prefix: &str) -> Option<String> {
    let s = String::from_utf8_lossy(bytes);
    let idx = s.find(prefix)?;
    let msg = &s[idx + prefix.len()..];
    Some(msg.trim().to_string())
}

fn shorten(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

#[test]
fn e2e_full_dashboard() {
    if maybe_run_single_case() {
        return;
    }

    let all_cases = cases();
    let test_binary = std::env::current_exe().unwrap();

    let mut pass = 0usize;
    let mut diff = 0usize;
    let mut compile_err = 0usize;
    let mut feature_err = 0usize;
    let mut timeout = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (i, case) in all_cases.iter().enumerate() {
        eprint!(
            "[{}/{}] [{}/{}] ... ",
            i + 1,
            all_cases.len(),
            case.family,
            case.name
        );

        let mut child = match Command::new(&test_binary)
            .env("E2E_CASE_INDEX", i.to_string())
            .arg("e2e_full_dashboard")
            .arg("--nocapture")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("SPAWN ERR: {}", e);
                compile_err += 1;
                failures.push(format!("[{}] {}: SPAWN ERR {}", case.family, case.name, e));
                continue;
            }
        };

        let child_stdout = child.stdout.take().unwrap();
        let child_stderr = child.stderr.take().unwrap();

        let stdout_handle = std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = Vec::new();
            let mut r = child_stdout;
            let _ = r.read_to_end(&mut buf);
            buf
        });
        let stderr_handle = std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = Vec::new();
            let mut r = child_stderr;
            let _ = r.read_to_end(&mut buf);
            buf
        });

        let start = std::time::Instant::now();
        let output = loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let stdout = stdout_handle.join().unwrap_or_default();
                    let stderr = stderr_handle.join().unwrap_or_default();
                    break Ok(std::process::Output {
                        status,
                        stdout,
                        stderr,
                    });
                }
                Ok(None) => {
                    if start.elapsed() > std::time::Duration::from_secs(30) {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
                    }
                    std::thread::yield_now();
                }
                Err(e) => break Err(e),
            }
        };

        match output {
            Err(_) => {
                eprintln!("TIMEOUT/CRASH");
                timeout += 1;
                failures.push(format!("[{}] {}: TIMEOUT/CRASH", case.family, case.name));
            }
            Ok(out) if out.status.success() => {
                let svg = match extract_svg_payload(&out.stdout) {
                    Some(svg) => svg,
                    None => {
                        eprintln!("NO SVG");
                        diff += 1;
                        failures.push(format!("[{}] {}: NO SVG", case.family, case.name));
                        continue;
                    }
                };

                match case.expected_kind.as_str() {
                    "svg" => {
                        let expected_path = svg_fixture_path(case);
                        let expected = std::fs::read_to_string(&expected_path).unwrap_or_default();
                        if svg == expected {
                            eprintln!("MATCH");
                            pass += 1;
                        } else {
                            let pos = svg
                                .chars()
                                .zip(expected.chars())
                                .position(|(a, b)| a != b)
                                .unwrap_or(svg.len().min(expected.len()));
                            eprintln!("DIFF@{} ({}b vs {}b)", pos, svg.len(), expected.len());
                            // Dump SVGs when E2E_DUMP_DIR is set
                            if let Ok(dir) = std::env::var("E2E_DUMP_DIR") {
                                let base = format!("{}/{}_{}", dir, case.family, case.name);
                                let _ = std::fs::create_dir_all(&dir);
                                let _ = std::fs::write(format!("{}_got.svg", base), &svg);
                                let _ = std::fs::write(format!("{}_exp.svg", base), &expected);
                            }
                            diff += 1;
                            failures.push(format!(
                                "[{}] {}: DIFF@{} ({}b vs {}b)",
                                case.family,
                                case.name,
                                pos,
                                svg.len(),
                                expected.len()
                            ));
                        }
                    }
                    other => {
                        eprintln!("UNEXPECTED SVG");
                        diff += 1;
                        failures.push(format!(
                            "[{}] {}: expected {}, got SVG",
                            case.family, case.name, other
                        ));
                    }
                }
            }
            Ok(out) => {
                let compile_msg = extract_prefixed_message(&out.stderr, "COMPILE:");
                let feature_msg = extract_prefixed_message(&out.stderr, "FEATURE:");
                match (compile_msg, feature_msg) {
                    (Some(msg), None) => match case.expected_kind.as_str() {
                        "compile_error" => {
                            if case.expected_message.as_deref() == Some(msg.as_str()) {
                                eprintln!("MATCH (compile error)");
                                pass += 1;
                            } else {
                                eprintln!("WRONG COMPILE ERR");
                                diff += 1;
                                failures.push(format!(
                                    "[{}] {}: expected compile {:?}, got {:?}",
                                    case.family,
                                    case.name,
                                    case.expected_message,
                                    shorten(&msg, 160)
                                ));
                            }
                        }
                        _ => {
                            eprintln!("COMPILE ERR");
                            compile_err += 1;
                            failures.push(format!(
                                "[{}] {}: COMPILE {}",
                                case.family,
                                case.name,
                                shorten(&msg, 160)
                            ));
                        }
                    },
                    (None, Some(msg)) => match case.expected_kind.as_str() {
                        "dagre_feature_error" => {
                            if case.expected_message.as_deref() == Some(msg.as_str()) {
                                eprintln!("MATCH (feature error)");
                                pass += 1;
                            } else {
                                eprintln!("WRONG FEATURE ERR");
                                diff += 1;
                                failures.push(format!(
                                    "[{}] {}: expected feature {:?}, got {:?}",
                                    case.family,
                                    case.name,
                                    case.expected_message,
                                    shorten(&msg, 160)
                                ));
                            }
                        }
                        _ => {
                            eprintln!("FEATURE ERR");
                            feature_err += 1;
                            failures.push(format!(
                                "[{}] {}: FEATURE {}",
                                case.family,
                                case.name,
                                shorten(&msg, 160)
                            ));
                        }
                    },
                    _ => {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        eprintln!("UNKNOWN ERR");
                        compile_err += 1;
                        failures.push(format!(
                            "[{}] {}: UNKNOWN {}",
                            case.family,
                            case.name,
                            shorten(&stderr, 160)
                        ));
                    }
                }
            }
        }
    }

    println!("\n========================================");
    println!(" Dagre/SVG Dashboard: {} cases", all_cases.len());
    println!("========================================");
    println!("  MATCH:     {:>3}", pass);
    println!("  DIFF:      {:>3}", diff);
    println!("  COMPILE:   {:>3}", compile_err);
    println!("  FEATURE:   {:>3}", feature_err);
    println!("  TIMEOUT:   {:>3}", timeout);
    println!(
        "  RATE:      {:>5.1}%",
        pass as f64 / all_cases.len() as f64 * 100.0
    );

    if !failures.is_empty() {
        println!("\nFirst 40 failures:");
        for f in failures.iter().take(40) {
            println!("  {}", f);
        }
    }
}

#[test]
fn e2e_manifest_has_expected_cases() {
    let all_cases = cases();
    assert!(!all_cases.is_empty());
    assert!(all_cases.len() >= 300);
    assert!(all_cases.iter().any(|c| c.name == "chaos2"));
    assert!(
        all_cases
            .iter()
            .any(|c| c.expected_kind == "dagre_feature_error")
    );
    assert!(all_cases.iter().any(|c| c.expected_kind == "compile_error"));
}

#[test]
fn e2e_svg_fixtures_exist() {
    for case in cases().iter().filter(|c| c.expected_kind == "svg") {
        let p = svg_fixture_path(case);
        assert!(Path::new(&p).exists(), "missing fixture for {}", case.name);
    }
}
