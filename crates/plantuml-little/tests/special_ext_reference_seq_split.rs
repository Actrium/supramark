#![cfg(feature = "metrics-static-dejavu")]
// Compiled only with `metrics-static-dejavu` — this suite spawns Java
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
        .join("sequence")
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

    // Normalize participant IDs (ent####)
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
    for (old_id, new_id) in &ent_map {
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

// ── sequence split cases ──

#[test]
#[ignore = "diagnostic: open half-arrow V-line direction (\\\\, //)"]
fn ext_ref_seq_half_arrow_open() {
    assert_ext_reference_case(
        "seq_half_arrow_open",
        "seq_ext_half_arrow_open.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: self-message half-arrow (filled + open)"]
fn ext_ref_seq_self_half_arrow() {
    assert_ext_reference_case(
        "seq_self_half_arrow",
        "seq_ext_self_half_arrow.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: self-message + note width"]
fn ext_ref_seq_self_note_width() {
    assert_ext_reference_case(
        "seq_self_note_width",
        "seq_ext_self_note_width.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: filled half-arrow polygon shape (3-point vs 4-point)"]
fn ext_ref_seq_half_arrow_filled() {
    assert_ext_reference_case(
        "seq_half_arrow_filled",
        "seq_ext_half_arrow_filled.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: teoz opt/alt nested fragment parallel with self-msg"]
fn ext_ref_seq_teoz_opt_alt_par() {
    assert_ext_reference_case(
        "seq_teoz_opt_alt_par",
        "seq_ext_teoz_opt_alt_par.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: teoz self-message parallel with nested fragment"]
fn ext_ref_seq_teoz_self_par_nested() {
    assert_ext_reference_case(
        "seq_teoz_self_par_nested",
        "seq_ext_teoz_self_par_nested.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: teoz simplest parallel fragment pair"]
fn ext_ref_seq_teoz_par_frag_simple() {
    assert_ext_reference_case(
        "seq_teoz_par_frag_simple",
        "seq_ext_teoz_par_frag_simple.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: teoz parallel fragment pairs"]
fn ext_ref_seq_teoz_par_frag() {
    assert_ext_reference_case(
        "seq_teoz_par_frag",
        "seq_ext_teoz_par_frag.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: teoz alt followed by parallel self-message"]
fn ext_ref_seq_teoz_alt_par_self() {
    assert_ext_reference_case(
        "seq_teoz_alt_par_self",
        "seq_ext_teoz_alt_par_self.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: teoz alt/else/par fragment height"]
fn ext_ref_seq_teoz_alt_simple() {
    assert_ext_reference_case(
        "seq_teoz_alt_simple",
        "seq_ext_teoz_alt_simple.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: right self-message no activation"]
fn ext_ref_seq_right_self_noact() {
    assert_ext_reference_case(
        "seq_right_self_noact",
        "seq_ext_right_self_noact.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: right self-message simple"]
fn ext_ref_seq_right_self_simple() {
    assert_ext_reference_case(
        "seq_right_self_simple",
        "seq_ext_right_self_simple.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: left self-message with msg-attached activation"]
fn ext_ref_seq_left_self_msgact() {
    assert_ext_reference_case(
        "seq_left_self_msgact",
        "seq_ext_left_self_msgact.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: left self-message simple"]
fn ext_ref_seq_left_self_simple() {
    assert_ext_reference_case(
        "seq_left_self_simple",
        "seq_ext_left_self_simple.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: left self-message with activation"]
fn ext_ref_seq_left_msg_active() {
    assert_ext_reference_case(
        "seq_left_msg_active",
        "seq_ext_left_msg_active.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: teoz simple group with two messages"]
fn ext_ref_seq_teoz_group_simple() {
    assert_ext_reference_case(
        "seq_teoz_group_simple",
        "seq_ext_teoz_group_simple.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: teoz timeline groups with activations"]
fn ext_ref_seq_teoz_timeline_0009() {
    assert_ext_reference_case(
        "seq_teoz_timeline_0009",
        "seq_ext_teoz_timeline_0009.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: teoz timeline arrow decorations and note over"]
fn ext_ref_seq_teoz_timeline_0007() {
    assert_ext_reference_case(
        "seq_teoz_timeline_0007",
        "seq_ext_teoz_timeline_0007.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: teoz timeline notes with hidden and activate"]
fn ext_ref_seq_teoz_timeline_0004() {
    assert_ext_reference_case(
        "seq_teoz_timeline_0004",
        "seq_ext_teoz_timeline_0004.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}

#[test]
#[ignore = "diagnostic: teoz timeline parallel messages with activate/deactivate"]
fn ext_ref_seq_teoz_timeline_0002() {
    assert_ext_reference_case(
        "seq_teoz_timeline_0002",
        "seq_ext_teoz_timeline_0002.puml",
        CompareProfile {
            normalize_ids: true,
            strip_source_line: true,
        },
    );
}
