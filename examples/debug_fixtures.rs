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
    if id.ends_with('-') { id.pop(); }
    id
}

fn main() {
    env_logger::init();
    let base = PathBuf::from("/ext/mermaid");
    for rel in ["ext_fixtures/cypress/flowchart/135", "ext_fixtures/cypress/flowchart/137", "ext_fixtures/cypress/flowchart/169"] {
        let mmd = base.join("tests").join(format!("{}.mmd", rel));
        let source = fs::read_to_string(&mmd).unwrap();
        
        let d = mermaid_little::parser::flowchart::parse(&source).unwrap();
        println!("=== {} ===", rel);
        println!("  direction: {:?}", d.direction);
        println!("  subgraphs: {:?}", d.subgraphs.iter().map(|s| format!("{}(dir={:?}, members={:?}, children={:?})", s.id, s.dir, s.members, s.children)).collect::<Vec<_>>());
        println!("  edges: {:?}", d.edges.iter().map(|e| format!("{}→{}", e.start, e.end)).collect::<Vec<_>>());
        
        let pre = mermaid_little::preprocess::preprocess(&source).unwrap();
        let theme_name = pre.config.theme.as_deref().unwrap_or("default");
        let mut th = mermaid_little::theme::get_theme(theme_name);
        if let Some(tv) = pre.config.theme_variables.as_ref() {
            mermaid_little::theme::apply_theme_variables(&mut th, tv);
        }
        
        let l = mermaid_little::layout::flowchart::layout(&d, &th).unwrap();
        
        for c in &l.clusters {
            println!("  cluster '{}' bounds: {:?}", c.id, c.bounds);
        }
        for n in &l.nodes {
            if n.is_group {
                println!("  group_node '{}' x={:?} y={:?} w={:?} h={:?} outer_tx={} outer_ty={}",
                    n.id, n.x, n.y, n.width, n.height,
                    n.extra.get("outer_tx").unwrap_or(&"-".to_string()),
                    n.extra.get("outer_ty").unwrap_or(&"-".to_string()));
            }
        }
        
        let id = id_for(rel);
        let got = mermaid_little::render::svg_flowchart::render(&d, &l, &th, &id).unwrap();
        
        if let Some(start) = got.find("viewBox=\"") {
            let start = start + 9;
            if let Some(end) = got[start..].find("\"") {
                println!("  OUR viewBox: {}", &got[start..start+end]);
            }
        }
        
        let ref_path = base.join("tests/reference").join(format!("{}.svg", rel));
        let expected = fs::read_to_string(&ref_path).unwrap();
        if let Some(start) = expected.find("viewBox=\"") {
            let start = start + 9;
            if let Some(end) = expected[start..].find("\"") {
                println!("  REF viewBox: {}", &expected[start..start+end]);
            }
        }
        
        let out_path = base.join("tests/debug_output").join(format!("{}.svg", rel.replace("/", "_")));
        fs::create_dir_all(base.join("tests/debug_output")).unwrap();
        fs::write(&out_path, &got).unwrap();
    }
}
