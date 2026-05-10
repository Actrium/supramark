#![cfg(feature = "metrics-ttf-parser")]
//! gitGraph byte-exact test harness.
//!
//! Runs the fixtures in `tests/ext_fixtures/{cypress,demos}/gitGraph`
//! supported by the current port through the Rust pipeline and diffs
//! against the matching reference SVG.
//!
//! Fixtures requiring features not yet ported are listed in
//! `tests/known_ignored.txt` and skipped here.
//!
//! Compiled only with `metrics-ttf-parser` — byte parity vs upstream
//! Mermaid's reference SVGs only holds when the layout pipeline runs
//! against the static DejaVu fixtures.

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

fn read_known_ignored() -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = base.join("tests/known_ignored.txt");
    if let Ok(text) = fs::read_to_string(&path) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((rel, _)) = line.split_once('\t') {
                let key = rel.trim();
                let key = key.strip_suffix(".mmd").unwrap_or(key);
                set.insert(key.to_string());
            }
        }
    }
    set
}

/// Sweep test that asserts every gitGraph fixture not in `known_ignored.txt`
/// renders byte-exact. Fails the suite on any regression and reports a
/// concise per-fixture summary on failure.
#[test]
fn gitgraph_sweep_all_fixtures() {
    let ignored = read_known_ignored();
    let dirs = [
        "ext_fixtures/cypress/gitGraph",
        "ext_fixtures/demos/gitGraph",
    ];
    let mut total = 0usize;
    let mut pass = 0usize;
    let mut skipped = 0usize;
    let mut failures: Vec<(String, String)> = Vec::new();
    for dir in &dirs {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(dir);
        let mut entries: Vec<_> = fs::read_dir(&base)
            .unwrap_or_else(|e| panic!("reading {:?}: {}", base, e))
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
            let rel = format!("{}/{}", dir, stem);
            total += 1;
            if ignored.contains(&rel) {
                skipped += 1;
                continue;
            }
            let mmd = entry.path();
            let svg = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/reference")
                .join(format!("{}.svg", rel));
            let source = match fs::read_to_string(&mmd) {
                Ok(s) => s,
                Err(e) => {
                    failures.push((rel.clone(), format!("read mmd: {e}")));
                    continue;
                }
            };
            let expected = match fs::read_to_string(&svg) {
                Ok(s) => s,
                Err(_) => {
                    // No reference — treat as skipped (parity-data missing).
                    skipped += 1;
                    continue;
                }
            };
            let id = id_for(&rel);
            match convert_with_id(&source, &id) {
                Ok(got) if got == expected => pass += 1,
                Ok(got) => {
                    let idx = got
                        .bytes()
                        .zip(expected.bytes())
                        .position(|(a, b)| a != b)
                        .unwrap_or(got.len().min(expected.len()));
                    failures.push((
                        rel.clone(),
                        format!(
                            "byte={} got_len={} exp_len={}",
                            idx,
                            got.len(),
                            expected.len()
                        ),
                    ));
                }
                Err(e) => failures.push((rel.clone(), format!("convert: {e}"))),
            }
        }
    }
    println!(
        "gitGraph byte-exact: {pass}/{} passed (skipped={skipped} failures={})",
        total - skipped,
        failures.len()
    );
    if !failures.is_empty() {
        let mut msg = format!(
            "{} gitGraph fixture(s) regressed (pass={}/{}):\n",
            failures.len(),
            pass,
            total - skipped
        );
        for (rel, why) in failures.iter().take(20) {
            msg.push_str(&format!("  {rel}: {why}\n"));
        }
        if failures.len() > 20 {
            msg.push_str(&format!("  ... ({} more)\n", failures.len() - 20));
        }
        panic!("{msg}");
    }
}

// ─── named per-fixture tests (kept for granular regression debugging) ───
//
// The sweep above is the source of truth for "all fixtures pass". The
// individually-named tests below are kept so that a fixture-specific
// regression surfaces as a focused failure rather than a single big
// sweep panic.

#[test]
fn cypress_01() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/01");
}

#[test]
fn cypress_02() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/02");
}

#[test]
fn cypress_03_reverse_highlight() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/03");
}

#[test]
fn cypress_04_tags() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/04");
}

#[test]
fn cypress_05_multi_branch() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/05");
}

#[test]
fn cypress_06_branch_with_merge() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/06");
}

#[test]
fn cypress_07_nested_branches() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/07");
}

#[test]
fn cypress_08_many_branches() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/08");
}

#[test]
fn cypress_09_init_rotate() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/09");
}

#[test]
fn cypress_10_no_rotate() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/10");
}

#[test]
fn cypress_11_cherry_pick() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/11");
}

#[test]
fn cypress_12_cherry_pick_tag() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/12");
}

#[test]
fn cypress_13_cherry_pick_empty_tag() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/13");
}

#[test]
fn cypress_14_cherry_pick_chain() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/14");
}

#[test]
fn cypress_15_long_chain() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/15");
}

#[test]
fn cypress_16_merge_custom() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/16");
}

#[test]
fn cypress_17_frontmatter_title() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/17");
}

#[test]
fn cypress_18_tb() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/18");
}

#[test]
fn cypress_19_tb_branches() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/19");
}

#[test]
fn cypress_20_tb_merge() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/20");
}

#[test]
fn cypress_21_tb_more() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/21");
}

#[test]
fn cypress_22_tb_branch_order() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/22");
}

#[test]
fn cypress_23_tb_more() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/23");
}

#[test]
fn cypress_24_branch_order() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/24");
}

#[test]
fn cypress_25() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/25");
}

#[test]
fn cypress_26() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/26");
}

#[test]
fn cypress_27() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/27");
}

#[test]
fn cypress_28() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/28");
}

#[test]
fn cypress_29() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/29");
}

#[test]
fn cypress_30() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/30");
}

#[test]
fn cypress_31() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/31");
}

#[test]
fn cypress_32() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/32");
}

#[test]
fn cypress_33() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/33");
}

#[test]
fn cypress_34() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/34");
}

#[test]
fn cypress_35() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/35");
}

#[test]
fn cypress_36() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/36");
}

#[test]
fn cypress_37() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/37");
}

#[test]
fn cypress_38() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/38");
}

#[test]
fn cypress_39() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/39");
}

#[test]
fn cypress_40() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/40");
}

#[test]
fn cypress_41() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/41");
}

#[test]
fn cypress_42() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/42");
}

#[test]
fn cypress_43() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/43");
}

#[test]
fn cypress_44() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/44");
}

#[test]
fn cypress_45() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/45");
}

#[test]
fn cypress_46() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/46");
}

#[test]
fn cypress_47() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/47");
}

#[test]
fn cypress_48() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/48");
}

#[test]
fn cypress_49() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/49");
}

#[test]
fn cypress_50() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/50");
}

#[test]
fn cypress_51() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/51");
}

#[test]
fn cypress_52() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/52");
}

#[test]
fn cypress_53() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/53");
}

#[test]
fn cypress_54() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/54");
}

#[test]
fn cypress_55() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/55");
}

#[test]
fn cypress_56() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/56");
}

#[test]
fn cypress_57() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/57");
}

#[test]
fn cypress_58() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/58");
}

#[test]
fn cypress_59() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/59");
}

#[test]
fn cypress_60() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/60");
}

#[test]
fn cypress_61() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/61");
}

#[test]
fn cypress_62() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/62");
}

#[test]
fn cypress_63() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/63");
}

#[test]
fn cypress_64() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/64");
}

#[test]
fn cypress_65() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/65");
}

#[test]
fn cypress_66() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/66");
}

#[test]
fn cypress_67() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/67");
}

#[test]
fn cypress_68() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/68");
}

#[test]
fn cypress_69() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/69");
}

#[test]
fn cypress_70() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/70");
}

#[test]
fn cypress_71() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/71");
}

#[test]
fn cypress_72() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/72");
}

#[test]
fn cypress_73() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/73");
}

#[test]
fn cypress_74() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/74");
}

#[test]
fn cypress_75() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/75");
}

#[test]
fn cypress_76() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/76");
}

#[test]
fn cypress_77() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/77");
}

#[test]
fn cypress_78() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/78");
}

#[test]
fn cypress_79() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/79");
}

#[test]
fn cypress_80() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/80");
}

#[test]
fn cypress_81() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/81");
}

#[test]
fn cypress_82() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/82");
}

#[test]
fn cypress_83() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/83");
}

#[test]
fn cypress_84() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/84");
}

#[test]
fn cypress_85() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/85");
}

#[test]
fn cypress_86() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/86");
}

#[test]
fn cypress_87() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/87");
}

#[test]
fn cypress_88() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/88");
}

#[test]
fn cypress_89() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/89");
}

#[test]
fn cypress_90() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/90");
}

#[test]
fn cypress_91() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/91");
}

#[test]
fn cypress_92() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/92");
}

#[test]
fn cypress_93() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/93");
}

#[test]
fn cypress_94() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/94");
}

#[test]
fn cypress_95() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/95");
}

#[test]
fn cypress_96() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/96");
}

#[test]
fn cypress_97() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/97");
}

#[test]
fn cypress_98() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/98");
}

#[test]
fn cypress_99() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/99");
}

#[test]
fn cypress_100() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/100");
}

#[test]
fn cypress_101_parallel_commits() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/101");
}

#[test]
fn cypress_102() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/102");
}

#[test]
fn cypress_103() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/103");
}

#[test]
fn cypress_104() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/104");
}

#[test]
fn cypress_105_multiline_branch() {
    assert_byte_exact("ext_fixtures/cypress/gitGraph/105");
}

#[test]
fn demo_01() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/01");
}

#[test]
fn demo_02() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/02");
}

#[test]
fn demo_03() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/03");
}

#[test]
fn demo_04() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/04");
}

#[test]
fn demo_05() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/05");
}

#[test]
fn demo_06() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/06");
}

#[test]
fn demo_07() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/07");
}

#[test]
fn demo_08() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/08");
}

#[test]
fn demo_09() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/09");
}

#[test]
fn demo_10() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/10");
}

#[test]
fn demo_11() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/11");
}

#[test]
fn demo_12() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/12");
}

#[test]
fn demo_13() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/13");
}

#[test]
fn demo_14() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/14");
}

#[test]
fn demo_15() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/15");
}

#[test]
fn demo_16() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/16");
}

#[test]
fn demo_17() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/17");
}

#[test]
fn demo_18() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/18");
}

#[test]
fn demo_19() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/19");
}

#[test]
fn demo_20() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/20");
}

#[test]
fn demo_21() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/21");
}

#[test]
fn demo_22() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/22");
}

#[test]
fn demo_23() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/23");
}

#[test]
fn demo_24() {
    assert_byte_exact("ext_fixtures/demos/gitGraph/24");
}
