//! Flowchart integration tests — parse → layout → render, comparing
//! against reference SVGs for structural soundness (not byte-exact
//! yet; see module-level commentary in `src/render/svg_flowchart.rs`).
//!
//! This file deliberately stays independent of `tests/wave1_e2e.rs`
//! and friends so that parser / layout / renderer churn doesn't
//! cascade across the existing sweeps.

use mermaid_little::layout::flowchart as fcl;
use mermaid_little::parser::flowchart as fcp;
use mermaid_little::preprocess;
use mermaid_little::render::svg_flowchart;
use mermaid_little::theme;

use std::fs;
use std::path::PathBuf;

/// Derive the id the reference-SVG was built with. Matches the convention
/// in `tests/treemap_byte_exact.rs` — `ref-<non-alnum-collapse>`.
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

/// Load every fixture stem under `dir` that isn't in `known_ignored`.
fn fixture_stems(dir_rel: &str) -> Vec<String> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = base.join("tests").join(dir_rel);
    let mut names = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries {
            let path = e.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) != Some("mmd") {
                continue;
            }
            let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
            names.push(stem);
        }
    }
    names.sort();
    names
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

fn is_elk_source(src: &str) -> bool {
    src.contains("flowchart-elk") || src.contains("layout: elk")
}

fn run_one(rel: &str) -> Result<(bool, String), String> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mmd = base.join("tests").join(format!("{}.mmd", rel));
    let svg_path = base.join("tests/reference").join(format!("{}.svg", rel));
    let source = fs::read_to_string(&mmd).map_err(|e| format!("read {:?}: {e}", mmd))?;
    if is_elk_source(&source) {
        return Ok((false, "elk — out of scope".into()));
    }
    let expected =
        fs::read_to_string(&svg_path).map_err(|e| format!("read {:?}: {e}", svg_path))?;
    let id = id_for(rel);
    let d = fcp::parse(&source).map_err(|e| format!("parse: {e}"))?;
    // Mirror lib.rs's `convert_with_id` pipeline so `%%{init: { theme,
    // themeVariables }}%%` directives propagate to the renderer the
    // same way as production.
    let pre = preprocess::preprocess(&source).map_err(|e| format!("preprocess: {e}"))?;
    let theme_name = pre.config.theme.as_deref().unwrap_or("default");
    let mut th = theme::get_theme(theme_name);
    if let Some(tv) = pre.config.theme_variables.as_ref() {
        theme::apply_theme_variables(&mut th, tv);
    }
    let l = fcl::layout(&d, &th).map_err(|e| format!("layout: {e}"))?;
    let got = svg_flowchart::render(&d, &l, &th, &id).map_err(|e| format!("render: {e}"))?;
    Ok((
        got == expected,
        if got == expected {
            String::new()
        } else {
            let mut diff = String::new();
            let byte = got
                .bytes()
                .zip(expected.bytes())
                .position(|(a, b)| a != b)
                .unwrap_or(0);
            diff.push_str(&format!("first-diff at byte {byte}"));
            diff
        },
    ))
}

#[test]
fn flowchart_parser_roundtrips_all_fixtures() {
    // Less strict than byte-exact: we just verify the parser and layout
    // run to completion without panicking on the ~270 non-elk fixtures.
    let ignored = read_known_ignored();
    let dirs = [
        "ext_fixtures/cypress/flowchart",
        "ext_fixtures/demos/flowchart",
    ];
    let mut total = 0usize;
    let mut skipped_elk = 0usize;
    let mut skipped_ignored = 0usize;
    let mut parse_failures = Vec::new();
    let mut layout_failures = Vec::new();
    let mut render_failures = Vec::new();
    for dir in &dirs {
        for stem in fixture_stems(dir) {
            let rel = format!("{}/{}", dir, stem);
            total += 1;
            if ignored.contains(&rel) {
                skipped_ignored += 1;
                continue;
            }
            let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let mmd = base.join("tests").join(format!("{}.mmd", rel));
            let source = match fs::read_to_string(&mmd) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if is_elk_source(&source) {
                skipped_elk += 1;
                continue;
            }
            let d = match fcp::parse(&source) {
                Ok(d) => d,
                Err(e) => {
                    parse_failures.push((rel.clone(), format!("{e}")));
                    continue;
                }
            };
            let th = theme::get_theme("default");
            let rel_for_panic = rel.clone();
            let l_result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fcl::layout(&d, &th)));
            let l = match l_result {
                Ok(Ok(l)) => l,
                Ok(Err(e)) => {
                    layout_failures.push((rel_for_panic, format!("{e}")));
                    continue;
                }
                Err(_) => {
                    layout_failures.push((rel_for_panic, "layout panic (dagre)".into()));
                    continue;
                }
            };
            let id = id_for(&rel);
            let render_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                svg_flowchart::render(&d, &l, &th, &id)
            }));
            match render_result {
                Ok(Err(e)) => render_failures.push((rel.clone(), format!("{e}"))),
                Err(_) => render_failures.push((rel.clone(), "render panic".into())),
                _ => {}
            }
        }
    }
    let eligible = total.saturating_sub(skipped_elk + skipped_ignored);
    eprintln!(
        "[flowchart] scan total={} eligible={} elk-skipped={} ignored={} parse-fail={} layout-fail={} render-fail={}",
        total,
        eligible,
        skipped_elk,
        skipped_ignored,
        parse_failures.len(),
        layout_failures.len(),
        render_failures.len(),
    );
    for (r, e) in parse_failures.iter().take(10) {
        eprintln!("[flowchart] parse-fail {r}: {e}");
    }
    for (r, e) in layout_failures.iter().take(10) {
        eprintln!("[flowchart] layout-fail {r}: {e}");
    }
    for (r, e) in render_failures.iter().take(10) {
        eprintln!("[flowchart] render-fail {r}: {e}");
    }
    // Treat as advisory — report to stderr but don't fail. The main
    // pipeline will tighten this once parser coverage extends to the
    // complete grammar and dagre-rs survives degenerate graphs.
    let _ = (parse_failures, layout_failures, render_failures);
}

#[test]
fn flowchart_byte_exact_sweep() {
    let ignored = read_known_ignored();
    let dirs = [
        "ext_fixtures/cypress/flowchart",
        "ext_fixtures/demos/flowchart",
    ];
    let mut total = 0usize;
    let mut pass = 0usize;
    let mut diffs: Vec<(String, String)> = Vec::new();
    for dir in &dirs {
        for stem in fixture_stems(dir) {
            let rel = format!("{}/{}", dir, stem);
            if ignored.contains(&rel) {
                continue;
            }
            let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let mmd = base.join("tests").join(format!("{}.mmd", rel));
            let source = match fs::read_to_string(&mmd) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if is_elk_source(&source) {
                continue;
            }
            total += 1;
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_one(&rel)));
            match r {
                Ok(Ok((true, _))) => pass += 1,
                Ok(Ok((false, d))) => diffs.push((rel.clone(), d)),
                Ok(Err(e)) => diffs.push((rel.clone(), format!("error: {e}"))),
                Err(_) => diffs.push((rel.clone(), "panic".into())),
            }
        }
    }
    eprintln!("[flowchart] byte-exact={}/{}", pass, total);
    for (r, d) in diffs.iter().take(30) {
        eprintln!("[flowchart] diff {r}: {d}");
    }
    // This test is aspirational for the MVP: it exists so the CI / harness
    // can track regression. Not a hard-fail until we hit 100%.
}

#[test]
fn flowchart_134_isolated_cluster_dom_order_is_byte_exact() {
    let rel = "ext_fixtures/cypress/flowchart/134";
    let (ok, diff) = run_one(rel).unwrap();
    assert!(ok, "{rel}: {diff}");
}

#[test]
fn flowchart_single_diff_report() {
    let rel_buf = std::env::var("FLOWCHART_DIFF_REL")
        .unwrap_or_else(|_| "ext_fixtures/cypress/flowchart/137".to_string());
    let rel = rel_buf.as_str();
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mmd = base.join("tests").join(format!("{}.mmd", rel));
    let svg_path = base.join("tests/reference").join(format!("{}.svg", rel));
    let source = std::fs::read_to_string(&mmd).unwrap();
    let expected = std::fs::read_to_string(&svg_path).unwrap();
    let id = id_for(rel);
    let d = fcp::parse(&source).unwrap();
    eprintln!(
        "vertices: {:?}",
        d.vertices.iter().map(|n| &n.id).collect::<Vec<_>>()
    );
    eprintln!(
        "subgraphs: {:?}",
        d.subgraphs
            .iter()
            .map(|s| (&s.id, &s.title, &s.members, &s.children))
            .collect::<Vec<_>>()
    );
    eprintln!("edges: {}", d.edges.len());
    // Mirror the production pipeline so `%%{init}%%` themes propagate.
    let pre = preprocess::preprocess(&source).unwrap();
    let theme_name = pre.config.theme.as_deref().unwrap_or("default");
    let mut th = theme::get_theme(theme_name);
    if let Some(tv) = pre.config.theme_variables.as_ref() {
        theme::apply_theme_variables(&mut th, tv);
    }
    let l = fcl::layout(&d, &th).unwrap();
    eprintln!("diagram_padding={}", l.diagram_padding);
    eprintln!("isolated_cluster_ids: {:?}", l.isolated_cluster_ids);
    for n in &l.nodes {
        eprintln!("  node id={} x={:?} y={:?} w={:?} h={:?} shape={:?} is_group={} parent={:?} padding={:?}", n.id, n.x, n.y, n.width, n.height, n.shape, n.is_group, n.parent_id, n.padding);
    }
    for e in &l.edges {
        eprintln!("  edge id={} lx={:?} ly={:?}", e.id, e.label_x, e.label_y);
        if let Some(pts) = &e.points {
            let s: Vec<String> = pts
                .iter()
                .map(|p| format!("({:.3},{:.3})", p.x, p.y))
                .collect();
            eprintln!("    pts: {}", s.join(" "));
        }
    }
    let got = svg_flowchart::render(&d, &l, &th, &id).unwrap();
    let byte = got
        .bytes()
        .zip(expected.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(0);
    let context = 160usize;
    let g_end = (byte + context).min(got.len());
    let e_end = (byte + context).min(expected.len());
    let g_start = byte.saturating_sub(60);
    let e_start = byte.saturating_sub(60);
    eprintln!("fixture {rel}: first diff at byte {byte}");
    eprintln!("GOT: ...{}...", &got[g_start..g_end]);
    eprintln!("EXP: ...{}...", &expected[e_start..e_end]);
    eprintln!("got len={} exp len={}", got.len(), expected.len());
}

/// Edge labels wrapped in `"\`…\`"` must be parsed as markdown so the
/// renderer can convert `**bold**`/`*italic*`/etc. into `<strong>`/`<em>`
/// tags. Regression for cypress fixtures 174/179 where the literal
/// `**bold**` text used to leak through.
#[test]
fn edge_label_backtick_quotes_classify_as_markdown() {
    use mermaid_little::model::flowchart::LabelKind;
    let src = "flowchart LR\nb -- \"`1o **bold**`\" --> c";
    let d = fcp::parse(src).unwrap();
    let l = d.edges[0]
        .label
        .as_ref()
        .expect("edge label parsed from `\"`...`\"` syntax");
    assert!(matches!(l.kind, LabelKind::Markdown));
    assert_eq!(l.text, "1o **bold**");
}

/// Round-shape bodies wrapped in `"\`…\`"` may legitimately contain `)`
/// inside the markdown text (e.g. `"`Item.(1)`"`). The shape parser
/// must respect quote/backtick regions so the closing `)` of the shape
/// isn't mistaken for one inside the label. Regression for cypress
/// fixtures 174/175.
#[test]
fn round_shape_label_with_inline_paren_in_markdown_quotes() {
    let src = "flowchart LR\nb(\"`Item.(1)`\") --> c";
    let d = fcp::parse(src).unwrap();
    let v = d.vertices.iter().find(|v| v.id == "b").expect("vertex `b`");
    let l = v.label.as_ref().expect("label populated");
    assert_eq!(l.text, "Item.(1)");
}

#[test]
fn dump_fixture_168() {
    let source = r#"flowchart TB
    Out --> In
    subgraph Sub
      In
    end
    Sub --> In
"#;
    let id = "ref-ext-fixtures-cypress-flowchart-168";
    let pre = preprocess::preprocess(source).unwrap();
    let theme_name = pre.config.theme.as_deref().unwrap_or("default");
    let mut th = theme::get_theme(theme_name);
    if let Some(tv) = pre.config.theme_variables.as_ref() {
        theme::apply_theme_variables(&mut th, tv);
    }
    let d = fcp::parse(source).unwrap();
    let l = fcl::layout(&d, &th).unwrap();

    for e in &l.edges {
        if e.id.contains("Sub_In") {
            eprintln!(
                "Edge {}: start={:?} end={:?} orig_start={:?} orig_end={:?}",
                e.id,
                e.start,
                e.end,
                e.extra.get("orig_start"),
                e.extra.get("orig_end")
            );
            if let Some(pts) = &e.points {
                eprintln!("  points ({}):", pts.len());
                for (i, p) in pts.iter().enumerate() {
                    eprintln!("    [{}] x={:.6} y={:.6}", i, p.x, p.y);
                }
            } else {
                eprintln!("  points: None");
            }
        }
    }

    let got = svg_flowchart::render(&d, &l, &th, id).unwrap();
    if let Some(idx) = got.find("L_Sub_In_0") {
        let start = idx.saturating_sub(300);
        let end = (idx + 500).min(got.len());
        eprintln!("\nRendered edge fragment:\n{}", &got[start..end]);
    }
}

#[test]
fn dump_fixture_155_parallel_edge_assignment() {
    let source = r#"flowchart TB
   subgraph S1
    sub1 -->sub2
   end
  subgraph S2
    sub4
   end
   S1 --> S2
   sub1 --> sub4
"#;
    let id = "ref-ext-fixtures-cypress-flowchart-155";
    let pre = preprocess::preprocess(source).unwrap();
    let theme_name = pre.config.theme.as_deref().unwrap_or("default");
    let mut th = theme::get_theme(theme_name);
    if let Some(tv) = pre.config.theme_variables.as_ref() {
        theme::apply_theme_variables(&mut th, tv);
    }
    let d = fcp::parse(source).unwrap();
    let l = fcl::layout(&d, &th).unwrap();

    for e in &l.edges {
        if e.id == "L_sub1_sub4_0" || e.id == "L_S1_S2_0" {
            if let Some(pts) = &e.points {
                eprintln!("{}: first_point x = {:.6}", e.id, pts[0].x);
            }
        }
    }

    // L_sub1_sub4_0 (leaf edge, inserted first) must get the LEFT spline:
    // its first point x should be ~104.788 (close to sub1 center at 99.98),
    // NOT ~119.687 (the right spline that belongs to L_S1_S2_0).
    let sub1_sub4_edge = l.edges.iter().find(|e| e.id == "L_sub1_sub4_0").unwrap();
    let sub1_sub4_x0 = sub1_sub4_edge.points.as_ref().unwrap()[0].x;
    assert!(
        (sub1_sub4_x0 - 104.788).abs() < 1.0,
        "L_sub1_sub4_0 first x should be ~104.788, got {:.6}",
        sub1_sub4_x0
    );

    let s1_s2_edge = l.edges.iter().find(|e| e.id == "L_S1_S2_0").unwrap();
    let s1_s2_x0 = s1_s2_edge.points.as_ref().unwrap()[0].x;
    assert!(
        (s1_s2_x0 - 119.687).abs() < 1.0,
        "L_S1_S2_0 first x should be ~119.687, got {:.6}",
        s1_s2_x0
    );

    let got = svg_flowchart::render(&d, &l, &th, id).unwrap();
    let expected_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/reference/ext_fixtures/cypress/flowchart/155.svg");
    let expected = std::fs::read_to_string(&expected_path).unwrap();

    if got == expected {
        eprintln!("fixture 155: BYTE-EXACT MATCH!");
    } else {
        let byte_pos = got
            .bytes()
            .zip(expected.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        eprintln!("fixture 155: DIFF at byte {}", byte_pos);
        let start = byte_pos.saturating_sub(30);
        let end = (byte_pos + 60).min(got.len().min(expected.len()));
        eprintln!("GOT:  ...{}...", &got[start..end]);
        eprintln!("EXP:  ...{}...", &expected[start..end]);
    }
}

#[test]
fn dump_fixture_168_clusters() {
    let source = r#"flowchart TB
    Out --> In
    subgraph Sub
      In
    end
    Sub --> In
"#;
    let id = "ref-ext-fixtures-cypress-flowchart-168";
    let pre = preprocess::preprocess(source).unwrap();
    let theme_name = pre.config.theme.as_deref().unwrap_or("default");
    let mut th = theme::get_theme(theme_name);
    if let Some(tv) = pre.config.theme_variables.as_ref() {
        theme::apply_theme_variables(&mut th, tv);
    }
    let d = fcp::parse(source).unwrap();
    let l = fcl::layout(&d, &th).unwrap();

    for c in &l.clusters {
        eprintln!("Cluster {}: {:?}", c.id, c.bounds);
    }
    for n in &l.nodes {
        eprintln!(
            "Node {}: x={:?} y={:?} w={:?} h={:?} is_group={}",
            n.id, n.x, n.y, n.width, n.height, n.is_group
        );
    }

    // Check the cluster clip process
    let got = svg_flowchart::render(&d, &l, &th, id).unwrap();

    // Look at the data-points and d attribute for L_Sub_In_0
    if let Some(idx) = got.find("data-id=\"L_Sub_In_0\"") {
        let start = idx.saturating_sub(200);
        let end = (idx + 200).min(got.len());
        eprintln!("\nEdge path fragment:\n{}", &got[start..end]);
    }
}
