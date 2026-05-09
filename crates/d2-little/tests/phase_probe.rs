//! Focused phase probe for a single e2e case.
//! Run with:
//!   PHASE_CASE_INDEX=12 cargo test -p d2-little --test phase_probe probe_case -- --ignored --nocapture

#[derive(Debug, serde::Deserialize)]
struct Case {
    family: String,
    name: String,
    script: String,
    theme_id: i64,
}

fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("e2e_dagre_svg_cases.json")).unwrap()
}

fn parent_chain_has_cycle(g: &d2_little::graph::Graph, start: usize) -> bool {
    let mut seen = std::collections::HashSet::new();
    let mut cur = Some(start);
    while let Some(id) = cur {
        if !seen.insert(id) {
            return true;
        }
        cur = g.objects[id].parent;
    }
    false
}

#[test]
#[ignore]
fn probe_case() {
    let idx: usize = std::env::var("PHASE_CASE_INDEX")
        .expect("PHASE_CASE_INDEX")
        .parse()
        .expect("valid PHASE_CASE_INDEX");
    let case = &cases()[idx];
    eprintln!("case[{idx}] {}/{}", case.family, case.name);

    let (ast_map, parse_err) = d2_little::parser::parse("", &case.script);
    assert!(parse_err.is_none(), "parse failed: {parse_err:?}");
    eprintln!("parse ok");

    let ir_map = d2_little::ir::compile(&ast_map).expect("ir");
    eprintln!(
        "ir ok: fields={}, edges={}",
        ir_map.fields.len(),
        ir_map.edges.len()
    );

    let mut g = d2_little::compiler::compile("", &case.script).expect("compiler");
    eprintln!(
        "compiler ok: objects={}, edges={}",
        g.objects.len(),
        g.edges.len()
    );

    for (i, obj) in g.objects.iter().enumerate() {
        if obj.parent == Some(i) {
            eprintln!("self-parent: {i} {}", obj.abs_id());
        }
        if obj.children_array.contains(&i) {
            eprintln!("self-child: {i} {}", obj.abs_id());
        }
        if parent_chain_has_cycle(&g, i) {
            eprintln!("parent-cycle: {i} {}", obj.abs_id());
        }
    }

    for (i, obj) in g.objects.iter().enumerate() {
        if i == g.root
            || !obj.children_array.is_empty()
            || obj.grid_rows.is_some()
            || obj.grid_columns.is_some()
        {
            eprintln!(
                "obj[{i}] id={} parent={:?} shape={} kids={} grid_rows={:?} grid_cols={:?} label={:?}",
                obj.abs_id(),
                obj.parent,
                obj.shape.value,
                obj.children_array.len(),
                obj.grid_rows.as_ref().map(|v| v.value.as_str()),
                obj.grid_columns.as_ref().map(|v| v.value.as_str()),
                obj.label.value,
            );
        }
    }

    if let Some(theme) = d2_little::themes::catalog::find(case.theme_id) {
        g.theme = Some(theme.clone());
    }

    let mut ruler = d2_little::textmeasure::Ruler::new().expect("ruler");
    d2_little::set_dimensions(&mut g, &mut ruler).expect("set_dimensions");
    eprintln!("set_dimensions ok");

    d2_little::dagre_layout::layout(&mut g, None).expect("layout");
    eprintln!("layout ok");

    if std::env::var("PHASE_DUMP_DIAGRAM").as_deref() == Ok("1") {
        let diagram = d2_little::exporter::export(&g, None, None).expect("export");
        for s in &diagram.shapes {
            eprintln!(
                "shape id={:?} type={} x={} y={} w={} h={} label={:?} label_w={} label_h={} font={} size={} lang={}",
                s.id,
                s.type_,
                s.pos.x,
                s.pos.y,
                s.width,
                s.height,
                s.text.label,
                s.text.label_width,
                s.text.label_height,
                s.text.font_family,
                s.text.font_size,
                s.text.language,
                // label position is the critical clue for outside-label routing bugs.
                // Keep it in the dump so route mismatches can be tied back to placement.
                // Exporter copies this directly from the graph object.
            );
            eprintln!(
                "shape-meta id={:?} label_pos={:?} icon_pos={:?}",
                s.id, s.label_position, s.icon_position,
            );
        }
        for c in &diagram.connections {
            eprintln!(
                "conn id={:?} src={:?} dst={:?} label={:?} label_w={} label_h={} route={:?}",
                c.id, c.src, c.dst, c.text.label, c.text.label_width, c.text.label_height, c.route,
            );
            if let Some(ref l) = c.src_label {
                eprintln!("  src_label: {:?} color={:?}", l.label, l.color);
            }
            if let Some(ref l) = c.dst_label {
                eprintln!("  dst_label: {:?} color={:?}", l.label, l.color);
            }
        }
    }

    let svg = d2_little::d2_to_svg(&case.script).expect("d2_to_svg");
    eprintln!("svg ok: {} bytes", svg.len());
}

#[test]
#[ignore]
fn hash_probe() {
    let script = std::env::var("HASH_SCRIPT").unwrap_or_else(|_| "a -> b".to_string());
    let theme: Option<i64> = std::env::var("HASH_THEME")
        .ok()
        .and_then(|v| v.parse().ok());
    eprintln!("script: {script:?}, theme: {theme:?}");
    let opts = d2_little::CompileOptions {
        theme_id: theme,
        pad: Some(0),
        ..d2_little::CompileOptions::default()
    };
    let (diagram, _svg) = d2_little::compile(&script, &opts).expect("compile");
    let bytes = d2_little::target::go_json::diagram_bytes(&diagram);
    let hash = diagram.hash_id(None);
    eprintln!("HashID: {hash}");
    eprintln!("Bytes len: {}", bytes.len());
    eprintln!("FULL: {}", String::from_utf8_lossy(&bytes));
}
