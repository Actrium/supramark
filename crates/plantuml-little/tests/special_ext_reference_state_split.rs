#![cfg(feature = "metrics-ttf-parser")]
// Compiled only with `metrics-ttf-parser` — this suite spawns Java
// PlantUML and diffs its SVG bytes against the Rust output. Java's
// FontMetrics maps to the static DejaVu fixtures; any other metrics
// impl would diverge by construction.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_path(name: &str) -> PathBuf {
    repo_root()
        .join("tests")
        .join("ext_fixtures")
        .join("state")
        .join(name)
}

fn find_java_jar() -> PathBuf {
    let libs_dir = Path::new("/ext/plantuml/plantuml/build/libs");
    let mut jars: Vec<PathBuf> = fs::read_dir(libs_dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", libs_dir.display()))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            name.starts_with("plantuml-")
                && name.ends_with(".jar")
                && !name.contains("-sources")
                && !name.contains("-javadoc")
        })
        .collect();
    jars.sort();
    jars.pop()
        .unwrap_or_else(|| panic!("no Java PlantUML jar found in {}", libs_dir.display()))
}

fn render_rust(fixture: &Path) -> String {
    let source = fs::read_to_string(fixture)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", fixture.display()));
    plantuml_little::convert_with_input_path(&source, fixture)
        .unwrap_or_else(|e| panic!("Rust render failed for {}: {e}", fixture.display()))
}

fn render_java(fixture: &Path) -> String {
    let source = fs::read_to_string(fixture)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", fixture.display()));
    let jar = find_java_jar();
    let mut child = Command::new("java")
        .arg("-jar")
        .arg(&jar)
        .arg("-tsvg")
        .arg("-pipe")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("cannot spawn java for {}: {e}", fixture.display()));
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .expect("java stdin unavailable")
        .write_all(source.as_bytes())
        .unwrap_or_else(|e| panic!("cannot write java stdin for {}: {e}", fixture.display()));
    let output = child
        .wait_with_output()
        .unwrap_or_else(|e| panic!("cannot wait for java for {}: {e}", fixture.display()));
    if !output.status.success() {
        panic!(
            "Java render failed for {} with status {:?}\nstderr:\n{}",
            fixture.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout).unwrap_or_else(|e| {
        panic!(
            "java stdout was not valid UTF-8 for {}: {e}",
            fixture.display()
        )
    })
}

fn strip_plantuml_src_pi(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find("<?plantuml-src ") {
        if let Some(end) = result[start..].find("?>") {
            result.replace_range(start..start + end + 2, "");
        } else {
            break;
        }
    }
    result
}

fn normalize_entity_link_ids(s: &str) -> String {
    use std::collections::HashMap;

    let mut result = s.to_string();

    let mut ent_map: HashMap<String, String> = HashMap::new();
    let mut ent_counter = 0usize;
    {
        let mut pos = 0;
        while let Some(idx) = result[pos..].find("id=\"ent") {
            let abs = pos + idx + 4;
            if let Some(end) = result[abs..].find('"') {
                let old_id = result[abs..abs + end].to_string();
                if let std::collections::hash_map::Entry::Vacant(e) = ent_map.entry(old_id) {
                    e.insert(format!("__e{}__", ent_counter));
                    ent_counter += 1;
                }
                pos = abs + end + 1;
            } else {
                break;
            }
        }
    }

    let mut lnk_map: HashMap<String, String> = HashMap::new();
    let mut lnk_counter = 0usize;
    {
        let mut pos = 0;
        while let Some(idx) = result[pos..].find("id=\"lnk") {
            let abs = pos + idx + 4;
            if let Some(end) = result[abs..].find('"') {
                let old_id = result[abs..abs + end].to_string();
                if let std::collections::hash_map::Entry::Vacant(e) = lnk_map.entry(old_id) {
                    e.insert(format!("__l{}__", lnk_counter));
                    lnk_counter += 1;
                }
                pos = abs + end + 1;
            } else {
                break;
            }
        }
    }

    for (old_id, new_id) in &ent_map {
        result = result.replace(&format!("id=\"{old_id}\""), &format!("id=\"{new_id}\""));
        result = result.replace(
            &format!("data-entity-1=\"{old_id}\""),
            &format!("data-entity-1=\"{new_id}\""),
        );
        result = result.replace(
            &format!("data-entity-2=\"{old_id}\""),
            &format!("data-entity-2=\"{new_id}\""),
        );
    }

    for (old_id, new_id) in &lnk_map {
        result = result.replace(&format!("id=\"{old_id}\""), &format!("id=\"{new_id}\""));
    }

    result
}

fn strip_source_line_attrs(s: &str) -> String {
    let re = regex::Regex::new(r#" data-source-line="[^"]*""#).unwrap();
    re.replace_all(s, "").to_string()
}

fn find_first_diff(a: &str, b: &str) -> (usize, usize, String) {
    let mut line = 1;
    let mut col = 1;
    for (i, (ca, cb)) in a.chars().zip(b.chars()).enumerate() {
        if ca != cb {
            let context_a = &a[i.saturating_sub(80)..a.len().min(i + 80)];
            let context_b = &b[i.saturating_sub(80)..b.len().min(i + 80)];
            return (
                line,
                col,
                format!(
                    "expected: ...{}...\nactual:   ...{}...",
                    context_b, context_a
                ),
            );
        }
        if ca == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    let la = a.len();
    let lb = b.len();
    (
        line,
        col,
        format!("length differs: actual={la}, expected={lb}"),
    )
}

#[derive(Clone, Copy)]
struct CompareProfile {
    normalize_ids: bool,
    strip_source_line: bool,
}

fn canonicalize(svg: &str, profile: CompareProfile) -> String {
    let mut out = strip_plantuml_src_pi(svg);
    if profile.strip_source_line {
        out = strip_source_line_attrs(&out);
    }
    if profile.normalize_ids {
        out = normalize_entity_link_ids(&out);
    }
    out
}

fn write_case_artifacts(
    case: &str,
    rust_svg: &str,
    java_svg: &str,
    rust_cmp: &str,
    java_cmp: &str,
) {
    let out_dir = repo_root()
        .join("tmp_debug")
        .join("special-ext-ref")
        .join(case);
    fs::create_dir_all(&out_dir)
        .unwrap_or_else(|e| panic!("cannot create {}: {e}", out_dir.display()));
    fs::write(out_dir.join("rust.raw.svg"), rust_svg)
        .unwrap_or_else(|e| panic!("cannot write rust.raw.svg for {case}: {e}"));
    fs::write(out_dir.join("java.raw.svg"), java_svg)
        .unwrap_or_else(|e| panic!("cannot write java.raw.svg for {case}: {e}"));
    fs::write(out_dir.join("rust.canonical.svg"), rust_cmp)
        .unwrap_or_else(|e| panic!("cannot write rust.canonical.svg for {case}: {e}"));
    fs::write(out_dir.join("java.canonical.svg"), java_cmp)
        .unwrap_or_else(|e| panic!("cannot write java.canonical.svg for {case}: {e}"));
}

fn assert_ext_reference_case(case: &str, fixture_name: &str, profile: CompareProfile) {
    let fixture = fixture_path(fixture_name);
    let rust_svg = render_rust(&fixture);
    let java_svg = render_java(&fixture);
    let rust_cmp = canonicalize(&rust_svg, profile);
    let java_cmp = canonicalize(&java_svg, profile);
    write_case_artifacts(case, &rust_svg, &java_svg, &rust_cmp, &java_cmp);
    if rust_cmp != java_cmp {
        let (line, col, ctx) = find_first_diff(&rust_cmp, &java_cmp);
        panic!(
            "{case}: {} differs from Java reference at line {line} col {col}\n{ctx}",
            fixture.display()
        );
    }
}

#[test]
#[ignore = "diagnostic external raw-reference test; geometry only after ID/source-line normalization"]
fn ext_ref_state_final_y_only() {
    assert_ext_reference_case(
        "state_final_y_only",
        "state_ext_final_y_only.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic external raw-reference test; order only after ID/source-line normalization"]
fn ext_ref_state_order_only() {
    assert_ext_reference_case(
        "state_order_only",
        "state_ext_order_no_final.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic external raw-reference test; ID allocation only after source-line stripping"]
fn ext_ref_state_id_only() {
    assert_ext_reference_case(
        "state_id_only",
        "state_ext_id_self_only.puml",
        CompareProfile {
            normalize_ids: false,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic external raw-reference test; source-line only after ID normalization"]
fn ext_ref_state_source_line_only() {
    assert_ext_reference_case(
        "state_source_line_only",
        "state_ext_source_line_only.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: false,
        },
    );
}

// ── state_note001 split cases ──

#[test]
#[ignore = "diagnostic: composite + start entity output order"]
fn ext_ref_state_order_composite_start() {
    assert_ext_reference_case(
        "state_order_composite_start",
        "state_ext_order_composite_start.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: note body path shape (A0,0 arcs)"]
fn ext_ref_state_note_path() {
    assert_ext_reference_case(
        "state_note_path",
        "state_ext_note_path.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: choice diagram edge Bezier control points"]
fn ext_ref_state_choice_bezier() {
    assert_ext_reference_case(
        "state_choice_bezier",
        "state_ext_choice_bezier.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: multiline note body path shape"]
fn ext_ref_state_note_multiline() {
    assert_ext_reference_case(
        "state_note_multiline",
        "state_ext_note_multiline.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: empty composite state overhead height"]
fn ext_ref_state_composite_empty() {
    assert_ext_reference_case(
        "state_composite_empty",
        "state_ext_composite_empty.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: composite state with edge labels height"]
fn ext_ref_state_composite_with_label() {
    assert_ext_reference_case(
        "state_composite_with_label",
        "state_ext_composite_with_label.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: fork/join bar height with composite"]
fn ext_ref_state_fork_composite() {
    assert_ext_reference_case(
        "state_fork_composite",
        "state_ext_fork_composite.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: fork/join bar height"]
fn ext_ref_state_fork_simple() {
    assert_ext_reference_case(
        "state_fork_simple",
        "state_ext_fork_simple.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: history cluster height in outer layout"]
fn ext_ref_state_history_simple() {
    assert_ext_reference_case(
        "state_history_simple",
        "state_ext_history_simple.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: concurrent region height (stacked with separator)"]
fn ext_ref_state_concurrent_simple() {
    assert_ext_reference_case(
        "state_concurrent_simple",
        "state_ext_concurrent_simple.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}
