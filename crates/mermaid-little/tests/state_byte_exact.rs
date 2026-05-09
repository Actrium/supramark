//! State diagram byte-exact test harness.
//!
//! Runs fixtures in `tests/ext_fixtures/cypress/state` through the Rust
//! pipeline and diffs against the matching reference SVG.

use mermaid_little::convert_with_id;
use std::fs;
use std::path::PathBuf;

fn id_for(rel: &str) -> String {
    let mut id = String::from("ref-");
    let mut last_was_sep = false;
    for c in rel.chars() {
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
    id
}

#[track_caller]
fn assert_byte_exact(rel: &str) {
    let mut mmd = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    mmd.push("tests");
    mmd.push(format!("{}.mmd", rel));
    let mut svg = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    svg.push("tests/reference");
    svg.push(format!("{}.svg", rel));

    let source = fs::read_to_string(&mmd).unwrap_or_else(|e| panic!("reading {:?}: {}", mmd, e));
    let expected = fs::read_to_string(&svg).unwrap_or_else(|e| panic!("reading {:?}: {}", svg, e));
    let id = id_for(rel);
    let got = convert_with_id(&source, &id).unwrap_or_else(|e| panic!("convert {}: {}", rel, e));

    if got == expected {
        return;
    }
    let idx = got
        .bytes()
        .zip(expected.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(got.len().min(expected.len()));
    let lo = idx.saturating_sub(60);
    let hi_g = (idx + 200).min(got.len());
    let hi_e = (idx + 200).min(expected.len());
    panic!(
        "mismatch in {} at byte {} (got_len={} exp_len={})\n GOT: ...{}...\n EXP: ...{}...\n",
        rel,
        idx,
        got.len(),
        expected.len(),
        &got[lo..hi_g],
        &expected[lo..hi_e],
    );
}

/// Print a diff summary for all state fixtures (used for manual debugging).
#[test]
#[ignore]
fn sweep_all_state_fixtures() {
    sweep_dir(
        "tests/ext_fixtures/cypress/state",
        "ext_fixtures/cypress/state",
    );
    sweep_dir("tests/ext_fixtures/demos/state", "ext_fixtures/demos/state");
}

fn sweep_dir(dir_rel: &str, ref_prefix: &str) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir_rel);
    let mut pass = 0;
    let mut fail = 0;
    let mut entries: Vec<_> = fs::read_dir(&base)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in &entries {
        let fname = entry.file_name();
        let name = fname.to_string_lossy();
        if !name.ends_with(".mmd") {
            continue;
        }
        let stem = name.trim_end_matches(".mmd");
        let rel = format!("{}/{}", ref_prefix, stem);
        let id = id_for(&rel);
        let source = fs::read_to_string(entry.path()).unwrap();
        let svg_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/reference")
            .join(format!("{}/{}.svg", ref_prefix, stem));
        let expected = match fs::read_to_string(&svg_path) {
            Ok(s) => s,
            Err(_) => {
                println!("  SKIP {} (no reference)", stem);
                continue;
            }
        };
        match convert_with_id(&source, &id) {
            Ok(got) if got == expected => {
                pass += 1;
                println!("  PASS {}", stem);
            }
            Ok(got) => {
                fail += 1;
                let idx = got
                    .bytes()
                    .zip(expected.bytes())
                    .position(|(a, b)| a != b)
                    .unwrap_or(got.len().min(expected.len()));
                let lo = idx.saturating_sub(30);
                let hi_g = (idx + 80).min(got.len());
                let hi_e = (idx + 80).min(expected.len());
                println!(
                    "  FAIL {} byte={} got_len={} exp_len={}\n    G: ...{}...\n    E: ...{}...",
                    stem,
                    idx,
                    got.len(),
                    expected.len(),
                    &got[lo..hi_g],
                    &expected[lo..hi_e]
                );
            }
            Err(e) => {
                fail += 1;
                println!("  ERR  {} => {}", stem, e);
            }
        }
    }
    println!("\nResult [{}]: {}/{} passed", ref_prefix, pass, pass + fail);
}

#[test]
fn cypress_01() {
    assert_byte_exact("ext_fixtures/cypress/state/01");
}
#[test]
fn cypress_02() {
    assert_byte_exact("ext_fixtures/cypress/state/02");
}
#[test]
fn cypress_03() {
    assert_byte_exact("ext_fixtures/cypress/state/03");
}
#[test]
fn cypress_04() {
    assert_byte_exact("ext_fixtures/cypress/state/04");
}
#[test]
fn cypress_05() {
    assert_byte_exact("ext_fixtures/cypress/state/05");
}
#[test]
fn cypress_06() {
    assert_byte_exact("ext_fixtures/cypress/state/06");
}
#[test]
fn cypress_07() {
    assert_byte_exact("ext_fixtures/cypress/state/07");
}
#[test]
fn cypress_08() {
    assert_byte_exact("ext_fixtures/cypress/state/08");
}
#[test]
fn cypress_09() {
    assert_byte_exact("ext_fixtures/cypress/state/09");
}
#[test]
fn cypress_10() {
    assert_byte_exact("ext_fixtures/cypress/state/10");
}
#[test]
fn cypress_11() {
    assert_byte_exact("ext_fixtures/cypress/state/11");
}
#[test]
fn cypress_12() {
    assert_byte_exact("ext_fixtures/cypress/state/12");
}
#[test]
fn cypress_13() {
    assert_byte_exact("ext_fixtures/cypress/state/13");
}
#[test]
fn cypress_14() {
    assert_byte_exact("ext_fixtures/cypress/state/14");
}
#[test]
fn cypress_15() {
    assert_byte_exact("ext_fixtures/cypress/state/15");
}

/// Nested isolated cluster: `state PilotCockpit { state Parent { C } }`.
/// Exercises the dagre_bridge fix that propagates `outer_tx/outer_ty` to a
/// nested isolated cluster (whose parent is itself isolated). Without it
/// Parent's inner-root `<g>` ended up at translate(0, 0). Also exercises
/// the depth-toggled `statediagram-cluster-alt` class on Parent (depth 2).
#[test]
fn cypress_25() {
    assert_byte_exact("ext_fixtures/cypress/state/25");
}

/// Same nested-isolated-cluster shape as cypress/25 but using the v1
/// `stateDiagram` keyword (no `-v2` suffix).
#[test]
fn cypress_67() {
    assert_byte_exact("ext_fixtures/cypress/state/67");
}

/// Composite state with a single leaf child, default rankdir TB so the
/// inner pass runs LR.  Exercises the leaf-only-LR upstream-alignment
/// post-process inside `dagre_bridge::layout_isolated_cluster`.
#[test]
fn cypress_30() {
    assert_byte_exact("ext_fixtures/cypress/state/30");
}

/// Same shape as `cypress/30` but using the `stateDiagram` (v1) keyword.
/// Confirms the leaf-only-LR fix applies regardless of state grammar.
#[test]
fn cypress_68() {
    assert_byte_exact("ext_fixtures/cypress/state/68");
}

/// Composite state whose title label ("Long state name 2") is wider than
/// its inner-graph bbox. Exercises the post-layout `expand_cluster_width_for_label`
/// pass: when the label demands more horizontal room than dagre allocated,
/// the cluster outer rect grows symmetrically around `cluster.x` and the
/// renderer's viewbox includes the cluster-label foreignObject local bbox.
#[test]
fn cypress_28() {
    assert_byte_exact("ext_fixtures/cypress/state/28");
}

/// Composite state with concurrent-region `--` separators. Exercises the
/// parser's docTranslator pass that partitions the parent's children into
/// divider-cluster wrappers (one per chunk), the layout's per-cluster `dir`
/// default (`TB`) so disconnected divider siblings flow horizontally inside
/// the parent, and the renderer's dashed-rect divider cluster shape.
///
/// Byte-exactness depends on deterministic iteration over the `sub_isolated`
/// HashMap inside `dagre_bridge::layout_isolated_cluster`; the bridge now
/// sorts sub-cluster ids before feeding them to dagre, so sibling divider
/// slot positions are stable across runs.
#[test]
fn cypress_44() {
    assert_byte_exact("ext_fixtures/cypress/state/44");
}

/// `[*] --> TV` outer transition with a `state TV { … }` composite child.
/// Exercises the parser's scope-prefixed `[*]` ids (outer `root_start`
/// vs inner `TV_start`/`TV_end`) AND the dagre_bridge isolated cluster
/// fork-widening so the outer pass places `root_start` at the correct
/// column under TV. v2 grammar.
#[test]
fn cypress_22() {
    assert_byte_exact("ext_fixtures/cypress/state/22");
}

/// Same shape as cypress/22 with `stateDiagram` (v1) keyword.
#[test]
fn cypress_64() {
    assert_byte_exact("ext_fixtures/cypress/state/64");
}

/// Two sibling composite states (`state A {…}` / `state C {…}`), each with
/// its own `direction`. Both are top-level isolated clusters. Byte-exactness
/// requires that the renderer emits the inner `<g class="root">` wrappers in
/// source-declaration order (A before C). The fix iterates `result.nodes`
/// (insertion-ordered) and filters by `isolated_cluster_ids` instead of
/// iterating the HashSet directly.
#[test]
fn cypress_33() {
    assert_byte_exact("ext_fixtures/cypress/state/33");
}

/// Two composite states S1 and S2 with cross-boundary edges (S1→S2 and
/// sub1→sub4). Both edges get anchor-rewritten to the same (sub1, sub4)
/// pair in the outer dagre, making them parallel multiedges. Byte-exactness
/// requires that the dagre geometric binding order matches upstream: the
/// non-rewritten leaf edge (sub1→sub4) must be added to dagre before the
/// anchor-rewritten cluster edge (S1→S2 → sub1→sub4), mirroring upstream's
/// `adjustClustersAndEdges` which removes and re-adds cluster edges at the
/// end of the edge list. The fix stable-partitions edges in
/// `build_graph_filtered_ex` so anchor-rewritten edges come last.
#[test]
fn cypress_34() {
    assert_byte_exact("ext_fixtures/cypress/state/34");
}

/// Single TB column of ten composite states whose long titles
/// (`StateN_____________`) widen each cluster's outer rect via
/// `expand_cluster_width_for_label`. Frontmatter declares `look: default`
/// (custom override) which the renderer forwards to every `data-look="…"`
/// attribute on cluster / node / edge elements.
///
/// Byte-exactness requires:
/// 1. Parser lifting `config.look` from frontmatter into
///    `StateDiagram::look_override` and the renderer threading that value
///    through every `data-look` slot via `with_look(...)`.
/// 2. The post-widen outer-shift in `expand_cluster_width_for_label`:
///    when every top-level cluster widens by the same amount, the entire
///    outer-level layout (top-level leaf nodes + outer edge points) gets
///    shifted by `delta - LABEL_PADDING` to mirror upstream's pre-layout
///    widening. Without this every outer edge anchored to root_start
///    sits ~13px left of where mermaid-js puts it.
#[test]
fn cypress_47() {
    assert_byte_exact("ext_fixtures/cypress/state/47");
}

/// Dump diff for one fixture (set FIXTURE env var or default 26).
#[test]
#[ignore]
fn debug_one_fixture() {
    let stem = std::env::var("FIXTURE").unwrap_or_else(|_| "26".to_string());
    let dir = std::env::var("FIXDIR").unwrap_or_else(|_| "cypress".to_string());
    let rel = format!("ext_fixtures/{}/state/{}", dir, stem);
    let mmd = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(format!("{}.mmd", rel));
    let svg = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/reference")
        .join(format!("{}.svg", rel));
    let source = fs::read_to_string(&mmd).unwrap();
    let expected = fs::read_to_string(&svg).unwrap();
    let id = id_for(&rel);
    let got = mermaid_little::convert_with_id(&source, &id).unwrap();
    let outdir = std::path::Path::new("/tmp/state_dump");
    let _ = std::fs::create_dir_all(outdir);
    std::fs::write(outdir.join(format!("{}.got.svg", stem)), &got).unwrap();
    std::fs::write(outdir.join(format!("{}.exp.svg", stem)), &expected).unwrap();
    println!(
        "wrote /tmp/state_dump/{}.{{got,exp}}.svg got_len={} exp_len={}",
        stem,
        got.len(),
        expected.len()
    );
}

/// Print full SVG output for cy/11 for debugging.
#[test]
#[ignore]
fn debug_cy11_output() {
    let source = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ext_fixtures/cypress/state/11.mmd"),
    )
    .unwrap();
    let id = id_for("ext_fixtures/cypress/state/11");
    let got = mermaid_little::convert_with_id(&source, &id).unwrap();
    // Print the cluster and node sections
    let cluster_start = got.find("<g class=\"clusters\">");
    let nodes_end = got.find("</g></g></g></svg>").unwrap_or(got.len());
    if let Some(start) = cluster_start {
        println!("SVG CONTENT:\n{}", &got[start..nodes_end.min(start + 3000)]);
    }
}
