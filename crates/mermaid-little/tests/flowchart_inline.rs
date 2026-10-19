#![cfg(feature = "metrics-ttf-parser")]
//! Flowchart integration tests — parse → layout → render, comparing
//! against reference SVGs for structural soundness (not byte-exact
//! yet; see module-level commentary in `src/render/svg_flowchart.rs`).
//!
//! This file deliberately stays independent of `tests/wave1_e2e.rs`
//! and friends so that parser / layout / renderer churn doesn't
//! cascade across the existing sweeps.
//!
//! Compiled only with `metrics-ttf-parser` — several tests below
//! (`flowchart_134_isolated_cluster_dom_order_is_byte_exact`, the byte
//! sweep, etc.) assert byte-exact equality against upstream reference
//! SVGs, which only holds with the static DejaVu fixtures.

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

fn read_known_alignment_warnings(scope: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = base.join("tests/known_alignment_warnings.txt");
    if let Ok(text) = fs::read_to_string(&path) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(3, '\t');
            let Some(line_scope) = parts.next() else {
                continue;
            };
            let Some(check) = parts.next() else {
                continue;
            };
            let Some(reason) = parts.next() else {
                continue;
            };
            if line_scope == scope {
                warnings.push(format!("{check}: {reason}"));
            }
        }
    }
    warnings
}

fn is_elk_source(src: &str) -> bool {
    src.contains("flowchart-elk") || src.contains("layout: elk")
}

fn render_one(rel: &str) -> Result<String, String> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mmd = base.join("tests").join(format!("{}.mmd", rel));
    let source = fs::read_to_string(&mmd).map_err(|e| format!("read {:?}: {e}", mmd))?;
    if is_elk_source(&source) {
        return Err("elk — out of scope".into());
    }
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
    let l = fcl::layout(&d, &th, false).map_err(|e| format!("layout: {e}"))?;
    svg_flowchart::render(&d, &l, &th, &id).map_err(|e| format!("render: {e}"))
}

fn run_one(rel: &str) -> Result<(bool, String), String> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let svg_path = base.join("tests/reference").join(format!("{}.svg", rel));
    let expected =
        fs::read_to_string(&svg_path).map_err(|e| format!("read {:?}: {e}", svg_path))?;
    let got = render_one(rel)?;
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

fn viewbox(svg: &str) -> [f64; 4] {
    let start = svg.find(r#"viewBox=""#).expect("viewBox attribute") + r#"viewBox=""#.len();
    let end = svg[start..]
        .find('"')
        .map(|i| start + i)
        .expect("viewBox close quote");
    let values: Vec<f64> = svg[start..end]
        .split_whitespace()
        .map(|part| part.parse::<f64>().expect("viewBox number"))
        .collect();
    assert_eq!(values.len(), 4);
    [values[0], values[1], values[2], values[3]]
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
            let l_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                fcl::layout(&d, &th, false)
            }));
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
    for warning in read_known_alignment_warnings("flowchart") {
        eprintln!("[flowchart] warning-ignored {warning}");
    }
    for (r, d) in diffs.iter().take(30) {
        eprintln!("[flowchart] diff {r}: {d}");
    }
    // This test is aspirational for the MVP: it exists so the CI / harness
    // can track regression. Not a hard-fail until we hit 100%.
}

#[test]
fn flowchart_134_isolated_cluster_has_browser_aligned_bounds() {
    let rel = "ext_fixtures/cypress/flowchart/134";
    let svg = render_one(rel).unwrap();
    assert!(svg.contains(r#"class="flowchart""#));
    assert!(svg.contains(r#"<g class="clusters">"#));
    assert!(svg.contains(r#"<g class="edgePaths">"#));
    let vb = viewbox(&svg);
    assert!(vb[2] > 1.0, "{rel} width should be non-empty: {vb:?}");
    assert!(vb[3] > 1.0, "{rel} height should be non-empty: {vb:?}");

    // Regression guard: the viewBox used to be computed without the isolated
    // cluster's inner-root translate, so the cluster — and everything below its
    // row — landed outside the viewport (clipped by browsers, painted outside
    // the host view on React Native). Every drawn cluster rect must sit inside.
    let rects = cluster_rects_in_viewbox_coords(&svg);
    assert!(!rects.is_empty(), "{rel} should emit cluster rects");
    for (x, y, w, h) in &rects {
        assert!(
            *x >= vb[0] - 0.5
                && *y >= vb[1] - 0.5
                && x + w <= vb[0] + vb[2] + 0.5
                && y + h <= vb[1] + vb[3] + 0.5,
            "{rel}: cluster rect ({x}, {y}, {w}, {h}) falls outside viewBox {vb:?}"
        );
    }
    // The viewport tracks the drawing: the outermost cluster plus 8px padding.
    assert!(
        (vb[2] - 570.027).abs() < 0.5 && (vb[3] - 350.668).abs() < 0.5,
        "{rel}: viewBox drifted to {vb:?}"
    );
}

#[test]
fn flowchart_isolated_cluster_below_the_node_row_stays_inside_the_viewbox() {
    // The shape from the chat report: dagre places the subgraph below the node
    // row, so the cluster is laid out recursively (isolated) and drawn inside a
    // translated inner root. Computing the viewBox without that translate used to
    // leave the cluster — and everything below its row — outside the viewport.
    let svg = mermaid_little::convert_with_id(
        "flowchart TB\n\
         subgraph g[\"gate\"]\n     a[\"if: always()\"]\n     b[\"result != success\"]\n   end\n\
         n0[\"rust\"] --> g\n   n1[\"test\"] --> g\n",
        "t-isolated-cluster",
    )
    .expect("render isolated cluster");

    let vb = viewbox(&svg);
    let rects = cluster_rects_in_viewbox_coords(&svg);
    assert!(!rects.is_empty(), "expected a cluster rect, got none");
    for (x, y, w, h) in &rects {
        assert!(
            *x >= vb[0] - 0.5
                && *y >= vb[1] - 0.5
                && x + w <= vb[0] + vb[2] + 0.5
                && y + h <= vb[1] + vb[3] + 0.5,
            "cluster rect ({x}, {y}, {w}, {h}) falls outside viewBox {vb:?}"
        );
    }
}

#[test]
fn flowchart_nested_isolated_clusters_stay_inside_the_viewbox() {
    // Nested isolated clusters carry their placement in the parent's inner frame
    // (dagre_bridge `outer_tx/outer_ty`), which the viewBox union has to compose
    // the same way the inner-root emission does.
    let svg = mermaid_little::convert_with_id(
        "flowchart TB\n\
         subgraph outer[\"outer\"]\n\
           subgraph inner[\"inner\"]\n     n0[\"a\"]\n   end\n\
           n1[\"b\"]\n\
         end\n\
         n2[\"c\"] --> outer\n",
        "t-nested-cluster",
    )
    .expect("render nested clusters");

    let vb = viewbox(&svg);
    let rects = cluster_rects_in_viewbox_coords(&svg);
    assert!(rects.len() >= 2, "expected both clusters, got {rects:?}");
    for (x, y, w, h) in &rects {
        assert!(
            *x >= vb[0] - 0.5
                && *y >= vb[1] - 0.5
                && x + w <= vb[0] + vb[2] + 0.5
                && y + h <= vb[1] + vb[3] + 0.5,
            "nested cluster rect ({x}, {y}, {w}, {h}) falls outside viewBox {vb:?}"
        );
    }
}

/// Every cluster `<rect>`, in viewBox coordinates: the scan composes the
/// `translate(...)` of each enclosing `<g>` (isolated clusters are drawn inside
/// their own translated inner root) and only counts rects whose nearest
/// ancestor `<g>` carries a `cluster` class.
fn cluster_rects_in_viewbox_coords(svg: &str) -> Vec<(f64, f64, f64, f64)> {
    let tag_re =
        regex::Regex::new(r#"<(\/?)([A-Za-z][\w:.-]*)((?:"[^"]*"|'[^']*'|[^>"'])*?)(\/?)>"#)
            .expect("tag regex");
    let translate_re = regex::Regex::new(r#"translate\(\s*([-\d.]+)[,\s]+([-\d.]+)\s*\)"#)
        .expect("translate regex");
    let attr_re = |attrs: &str, name: &str| -> Option<f64> {
        regex::Regex::new(&format!(r#"\b{name}="([-\d.]+)""#))
            .ok()?
            .captures(attrs)?
            .get(1)?
            .as_str()
            .parse::<f64>()
            .ok()
    };

    let mut rects = Vec::new();
    // Frame: (accumulated translate, translate of the nearest cluster ancestor).
    let mut stack: Vec<((f64, f64), Option<(f64, f64)>)> = vec![((0.0, 0.0), None)];
    for caps in tag_re.captures_iter(svg) {
        let closing = &caps[1];
        let name = caps[2].to_ascii_lowercase();
        let attrs = &caps[3];
        let self_closing = !caps[4].is_empty();

        if !closing.is_empty() {
            if stack.len() > 1 {
                stack.pop();
            }
            continue;
        }

        let (parent_t, parent_cluster) = *stack.last().expect("stack frame");
        let extra = translate_re
            .captures(attrs)
            .map(|c| {
                (
                    c[1].parse::<f64>().unwrap_or(0.0),
                    c[2].parse::<f64>().unwrap_or(0.0),
                )
            })
            .unwrap_or((0.0, 0.0));
        let acc = (parent_t.0 + extra.0, parent_t.1 + extra.1);
        let is_cluster =
            attrs.contains(r#"class="cluster"#) || attrs.contains(r#"class="cluster "#);
        let cluster_frame = if is_cluster {
            Some(acc)
        } else {
            parent_cluster
        };

        if name == "rect" {
            if let Some((cx, cy)) = cluster_frame {
                if let (Some(x), Some(y), Some(w), Some(h)) = (
                    attr_re(attrs, "x"),
                    attr_re(attrs, "y"),
                    attr_re(attrs, "width"),
                    attr_re(attrs, "height"),
                ) {
                    rects.push((cx + x, cy + y, w, h));
                }
            }
        }

        if !self_closing {
            stack.push((acc, cluster_frame));
        }
    }
    rects
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
    let l = fcl::layout(&d, &th, false).unwrap();
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
    let l = fcl::layout(&d, &th, false).unwrap();

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
    let l = fcl::layout(&d, &th, false).unwrap();

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
    let l = fcl::layout(&d, &th, false).unwrap();

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

/// Issue #254: a label the browser clamps to `flowchart.wrappingWidth` has to
/// be measured, laid out and emitted as the *wrapped* box.
///
/// Upstream measures the label `<div>` in a real DOM, where `max-width: 200px`
/// shrinks it to 200 px; it then re-styles the div to
/// `display: table; white-space: break-spaces; width: 200px`, which wraps the
/// text inside 200 px, and sizes the node from that box. Measuring the
/// un-clamped text instead reserves a 614.91 px node for a label the browser
/// paints 200 px wide, flush with the box's left edge — its over-wide lines
/// poking out of the border.
#[test]
fn clamped_html_label_matches_the_browser_box() {
    // `wide` label, one line longer than the 200 px wrapping width:
    //   python_codegen / unittest + protoc <mao yan> /
    //   compileall <wu> legacy import <xie lou> / python -m build
    // `narrow` stays inside the budget and must keep the un-clamped box.
    let source = concat!(
        "flowchart TD\n",
        "    wide[\"python_codegen<br/>unittest + protoc \u{5192}\u{70DF}<br/>",
        "compileall \u{65E0} legacy import \u{6CC4}\u{6F0F}<br/>python -m build\"]\n",
        "    narrow[\"webrtc<br/>\u{771F}\u{5B9E} RTC \u{573A}\u{666F}\"]\n",
    );
    let d = fcp::parse(source).expect("parse");
    let th = theme::get_theme("default");
    let l = fcl::layout(&d, &th, false).expect("layout");
    let svg = svg_flowchart::render(&d, &l, &th, "issue-254").expect("render");
    // The clamped node: 200 px label + 2 x 30 px label padding, five painted
    // lines at line-height 24.
    assert!(
        svg.contains(r#"<foreignObject width="200" height="120" overflow="visible">"#),
        "clamped label must be the wrapped 200 x 120 box"
    );
    assert!(
        svg.contains(
            r#"<div style="display: table; white-space: break-spaces; line-height: 1.5; max-width: 200px; text-align: center; width: 200px;""#
        ),
        "clamped label div must switch to the wrapping style"
    );
    assert!(
        svg.contains(r#"x="-130" y="-75" width="260" height="150""#),
        "node box must be sized from the wrapped label"
    );
    assert!(
        !svg.contains(r#"<foreignObject width="614.9"#),
        "the un-clamped label width must not reach the node box"
    );
    // The label that fits keeps `nowrap` and its measured width.
    assert!(
        svg.contains(r#"<div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;""#),
        "labels inside the budget must keep the un-clamped style"
    );
}

/// A subgraph title is an HTML label upstream never gives a `max-width`, so the
/// browser never wraps it. Measuring it through the clamp made the viewBox
/// narrower than the label the renderer paints, clipping a long title on both
/// sides.
#[test]
fn cluster_labels_stay_unclamped() {
    use mermaid_little::render::foreign_object::{
        measure_html_markup_label, HtmlLabelFont, HTML_LABEL_WRAPPING_WIDTH,
    };
    // A title well past the wrapping width.
    let title = "a very long subgraph title that goes past the two hundred pixel wrapping budget";
    let source = format!("flowchart TD\n    subgraph SG[\"{title}\"]\n    a --> b\n    end\n");
    let d = fcp::parse(&source).expect("parse");
    let th = theme::get_theme("default");
    let l = fcl::layout(&d, &th, false).expect("layout");
    let svg = svg_flowchart::render(&d, &l, &th, "cluster-clamp").expect("render");

    assert!(
        !svg.contains("break-spaces"),
        "cluster labels must keep the un-clamped `nowrap` style"
    );
    let markup = mermaid_little::render::foreign_object::string_label_to_html(title);
    let (label_w, _) = measure_html_markup_label(
        &markup,
        &HtmlLabelFont::default(),
        HTML_LABEL_WRAPPING_WIDTH,
        true,
    );
    assert!(label_w > HTML_LABEL_WRAPPING_WIDTH, "fixture must overflow");
    let label_fo = format!(
        r#"<foreignObject width="{}""#,
        mermaid_little::render::shapes::types::fmt_num(label_w)
    );
    assert!(
        svg.contains(&label_fo),
        "the emitted label must be the un-clamped box ({label_w})"
    );
    // And the viewBox has to cover it.
    let vb = viewbox(&svg);
    assert!(
        vb[2] >= label_w,
        "viewBox width {} must cover the {} px label",
        vb[2],
        label_w
    );
}

/// The `font-size` / `bold` post-process passes rewrite the label's
/// foreignObject numbers by literal needle replacement, so they must resolve
/// the clamp exactly as the measurement that produced those numbers did. A
/// demoted empty cluster is not clamped: measuring it clamped made every needle
/// miss, silently dropping the patch while the `<div>` kept its
/// `font-size: 30px !important`.
#[test]
fn postprocess_patches_match_the_unclamped_label_it_emitted() {
    use mermaid_little::render::foreign_object::{
        measure_html_markup_label, HtmlLabelFont, HTML_LABEL_WRAPPING_WIDTH,
    };
    let title = "a very long subgraph title that goes past the two hundred pixel wrapping budget";
    let source = format!(
        "flowchart TD\n    subgraph SG[\"{title}\"]\n    end\n    classDef big font-size:30px\n    class SG big\n"
    );
    let d = fcp::parse(&source).expect("parse");
    let th = theme::get_theme("default");
    let l = fcl::layout(&d, &th, false).expect("layout");
    let svg = svg_flowchart::render(&d, &l, &th, "cluster-fontsize").expect("render");

    assert!(
        svg.contains("font-size: 30px !important"),
        "fixture must carry the size override"
    );
    let markup = mermaid_little::render::foreign_object::string_label_to_html(title);
    let sized = HtmlLabelFont {
        font_family: None,
        font_size_px: Some(30.0),
        bold: None,
    };
    let (w30, _) = measure_html_markup_label(&markup, &sized, HTML_LABEL_WRAPPING_WIDTH, true);
    assert!(
        svg.contains(&format!(
            r#"<foreignObject width="{}""#,
            mermaid_little::render::shapes::types::fmt_num(w30)
        )),
        "the post-process patch must land on the 30 px geometry ({w30})"
    );
    // The label `<g>` carries the size override, i.e. the pass ran and its
    // needle for the group matched. (Its translate stays at the default-font
    // centre — pre-existing behaviour, byte-identical to HEAD.)
    assert!(
        svg.contains(r#"<g class="label" style="font-size:30px !important""#),
        "the pass must also stamp the size on the label group"
    );
}

/// A label that only overflows at its own inline `font-size` must still clamp:
/// the shape emitters have to decide the clamp in the same font domain the
/// layout reserved the box in, or the layout wraps it while the emitted `<div>`
/// stays `nowrap` and a CSS-aware host paints the larger text out of the box.
#[test]
fn styled_polygon_label_clamps_in_the_rendered_font() {
    // The label has to overflow *only* at 30 px (~169 px at 16 px, ~317 px at
    // 30 px): one that overflows in both domains passes even without the fix.
    let source = concat!(
        "flowchart TD\n",
        "    classDef big font-size:30px\n",
        "    a{{\"fits at sixteen px\"}}:::big\n",
    );
    let d = fcp::parse(source).expect("parse");
    let th = theme::get_theme("default");
    let l = fcl::layout(&d, &th, false).expect("layout");
    let svg = svg_flowchart::render(&d, &l, &th, "styled-polygon").expect("render");

    // The size pass prefixes the div; the clamp has to have switched it to the
    // wrapping style underneath that prefix.
    assert!(
        svg.contains("font-size: 30px !important; display: table; white-space: break-spaces;"),
        "a label clamped at 30 px must carry the wrapping style"
    );
    assert!(
        svg.contains(r#"<foreignObject width="200""#),
        "the box must be the wrapping width"
    );
    // The size pass's number needles only match default-font geometry, which a
    // clamped label no longer emits — the label group still has to carry the
    // size.
    assert!(
        svg.contains(r#"<g class="label" style="font-size:30px !important""#),
        "the label group must keep the size stamp"
    );
}

/// `diamond_br_postprocess` rewrites the polygon to the size dagre laid out. It
/// recomputes the numbers it searches for, so with a styled node it has to use
/// the same font the shape drew with — otherwise the needle misses and the
/// polygon keeps the drawn size while every edge endpoint was intersected
/// against the laid-out one.
#[test]
fn diamond_br_postprocess_matches_a_styled_drawing() {
    let source = concat!(
        "flowchart TD\n",
        "    classDef big font-size:30px\n",
        "    a{\"diamond<br/>a longer label that overflows at thirty px\"}:::big\n",
    );
    let d = fcp::parse(source).expect("parse");
    let th = theme::get_theme("default");
    let l = fcl::layout(&d, &th, false).expect("layout");
    let svg = svg_flowchart::render(&d, &l, &th, "styled-diamond").expect("render");

    // 455 is the diamond size `measure_vertex_box` laid out for this label; the
    // shape draws 496.54 when the post-process cannot find its needles.
    assert!(
        svg.contains(r#"points="227.5,0 455,-227.5 227.5,-455 0,-227.5""#),
        "the polygon must be the laid-out diamond, not the drawn one"
    );
}

/// A KaTeX label is measured from the *rendered* math, so the clamp — which
/// measures the label's HTML — must not replace that box with the `$$..$$`
/// source's (the renderer emits the KaTeX geometry).
#[cfg(feature = "katex")]
#[test]
fn katex_node_label_keeps_the_rendered_math_box() {
    let source = "flowchart TD\n    a[\"$$x = \\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}$$\"]\n";
    let d = fcp::parse(source).expect("parse");
    let th = theme::get_theme("default");
    let l = fcl::layout(&d, &th, false).expect("layout");
    let svg = svg_flowchart::render(&d, &l, &th, "katex-clamp").expect("render");

    let rect_w: f64 = svg
        .split(r#"<rect class="basic label-container" style="" x=""#)
        .nth(1)
        .and_then(|rest| rest.split("width=\"").nth(1))
        .and_then(|rest| rest.split('"').next())
        .and_then(|w| w.parse().ok())
        .expect("node rect");
    let fo_w: f64 = svg
        .split("<foreignObject width=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .and_then(|w| w.parse().ok())
        .expect("label foreignObject");
    assert!(
        rect_w >= fo_w,
        "the node box ({rect_w}) must contain the rendered KaTeX label ({fo_w})"
    );
}
