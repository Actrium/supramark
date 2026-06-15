//! End-to-end contract and dispatch tests: lock down the public
//! { engine, kind, data } envelope shape, and verify the registry dispatches by
//! the engine string and that dot/graphviz share an instance.

use supramark_diagram::default_registry;

#[test]
fn registry_dispatches_known_engines() {
    let reg = default_registry();
    assert!(reg.get("mermaid").is_some());
    assert!(reg.get("plantuml").is_some());
    assert!(reg.get("d2").is_some());
    assert!(reg.get("unknown").is_none());
    #[cfg(not(target_arch = "wasm32"))]
    {
        assert!(reg.get("dot").is_some());
        assert!(reg.get("graphviz").is_some());
    }
}

#[test]
fn d2_semantic_envelope_contract() {
    let reg = default_registry();
    let ast = reg.semantic("d2", "a -> b").unwrap().unwrap().unwrap();
    assert_eq!(ast.engine, "d2");
    assert_eq!(ast.kind, "d2");
    let v = serde_json::to_value(&ast).unwrap();
    // The envelope top level is exactly engine / kind / data.
    assert!(v.get("engine").is_some() && v.get("kind").is_some() && v.get("data").is_some());
    assert!(v["data"]["nodes"].is_array());
    assert!(v["data"]["edges"].is_array());
    // Layout coordinates must not leak into the semantics.
    let s = serde_json::to_string(&v).unwrap();
    for k in ["top_left", "topLeft", "\"width\"", "\"height\"", "box_", "route"] {
        assert!(!s.contains(k), "semantic JSON should not contain layout field {k}");
    }
}

#[test]
fn plantuml_semantic_envelope_contract() {
    let reg = default_registry();
    let src = "@startuml\nAlice -> Bob: hi\n@enduml";
    let ast = reg.semantic("plantuml", src).unwrap().unwrap().unwrap();
    assert_eq!(ast.engine, "plantuml");
    assert_eq!(ast.kind, "sequence");
    let v = serde_json::to_value(&ast).unwrap();
    assert!(v.get("data").is_some());
}

#[test]
fn mermaid_semantic_envelope_contract() {
    let reg = default_registry();
    let src = "sequenceDiagram\n    Alice->>Bob: hi";
    let ast = reg.semantic("mermaid", src).unwrap().unwrap().unwrap();
    assert_eq!(ast.engine, "mermaid");
    assert!(
        ["er", "flowchart", "sequence", "class"].contains(&ast.kind.as_str()),
        "kind={} should be one of the four supported kinds",
        ast.kind
    );
}
