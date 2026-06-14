use supramark_markdown::{parse, DiagnosticSeverity, ExtensionMode, SupramarkNode, TableAlign};

#[test]
fn public_api_outputs_ast_v2_with_positions() {
    let ast = parse("# 标题 😄\n\nHello **世界** and `code`.");
    let SupramarkNode::Root {
        ast_version,
        children,
        diagnostics,
        position,
        ..
    } = ast
    else {
        panic!("expected root");
    };

    assert_eq!(ast_version, 2);
    assert!(diagnostics.is_empty());
    assert!(position.is_some());
    assert_eq!(children.len(), 2);

    let SupramarkNode::Paragraph { children, .. } = &children[1] else {
        panic!("expected paragraph");
    };
    let SupramarkNode::Strong { position, .. } = &children[1] else {
        panic!("expected strong");
    };

    let position = position.as_ref().expect("strong node position");
    assert_eq!(position.start.byte_offset, 21);
    assert_eq!(position.start.utf16_offset, 15);
}

#[test]
fn public_api_serializes_diagrams_and_tables() {
    let ast = parse("```mermaid\ngraph TD; A-->B;\n```\n\n| A | B |\n|:-|--:|\n| 1 | 2 |\n");
    let json = serde_json::to_string(&ast).expect("serialize ast");

    assert!(json.contains(r#""type":"diagram""#));
    assert!(json.contains(r#""engine":"mermaid""#));
    assert!(json.contains(r#""type":"table""#));

    let ast: SupramarkNode = serde_json::from_str(&json).expect("deserialize ast");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Table { align, .. } = &children[1] else {
        panic!("expected table");
    };

    assert_eq!(
        align,
        &vec![Some(TableAlign::Left), Some(TableAlign::Right)]
    );
}

#[test]
fn public_api_omits_absent_optional_fields() {
    let json = serde_json::to_string(&parse("- plain item\n")).expect("serialize ast");

    assert!(json.contains(r#""type":"list_item""#));
    assert!(!json.contains(r#""checked":null"#));
    assert!(!json.contains(r#""start":null"#));
    assert!(!json.contains(r#""position":null"#));
}

#[test]
fn public_api_maps_task_list_items() {
    let ast = parse("- [x] Done\n- [ ] Todo\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::List { children, .. } = &children[0] else {
        panic!("expected list");
    };
    let SupramarkNode::ListItem {
        checked: first_checked,
        children: first_children,
        ..
    } = &children[0]
    else {
        panic!("expected first task item");
    };
    let SupramarkNode::ListItem {
        checked: second_checked,
        children: second_children,
        ..
    } = &children[1]
    else {
        panic!("expected second task item");
    };

    assert_eq!(*first_checked, Some(true));
    assert_eq!(*second_checked, Some(false));
    assert_eq!(first_text(first_children), "Done");
    assert_eq!(first_text(second_children), "Todo");
}

fn first_text(nodes: &[SupramarkNode]) -> &str {
    match &nodes[0] {
        SupramarkNode::Paragraph { children, .. } => first_text(children),
        SupramarkNode::Text { value, .. } => value,
        _ => panic!("expected text"),
    }
}

#[test]
fn public_api_maps_opaque_map_container() {
    let source =
        "before\n\n:::map\ncenter: [34.05, -118.24]\nzoom: 12\nmarker:\n  lat: 34.05\n  lng: -118.24\n:::\n\nafter";
    let ast = parse(source);
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };

    assert_eq!(children.len(), 3);
    let SupramarkNode::Container {
        name,
        mode,
        value,
        data,
        children: container_children,
        position,
        ..
    } = &children[1]
    else {
        panic!("expected map container");
    };

    assert_eq!(name, "map");
    assert_eq!(*mode, ExtensionMode::Opaque);
    assert!(container_children.is_empty());
    assert_eq!(
        value.as_deref(),
        Some("center: [34.05, -118.24]\nzoom: 12\nmarker:\n  lat: 34.05\n  lng: -118.24")
    );
    assert_eq!(
        data.as_ref()
            .and_then(|data| data.pointer("/markers/0/lat")),
        Some(&serde_json::json!(34.05))
    );
    assert!(position.is_some());

    let SupramarkNode::Paragraph { position, .. } = &children[2] else {
        panic!("expected trailing paragraph");
    };
    let position = position.as_ref().expect("trailing paragraph position");
    assert_eq!(position.start.byte_offset, source.find("after").unwrap());
}

#[test]
fn public_api_maps_opaque_input_block() {
    let ast = parse("%%%form user\nname: Leo\n%%%\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Input {
        name,
        mode,
        params,
        value,
        children,
        ..
    } = &children[0]
    else {
        panic!("expected input");
    };

    assert_eq!(name, "form");
    assert_eq!(*mode, ExtensionMode::Opaque);
    assert_eq!(params.as_deref(), Some("user"));
    assert_eq!(value.as_deref(), Some("name: Leo"));
    assert!(children.is_empty());
}

#[test]
fn public_api_maps_vison_container_data() {
    let ast = parse(":::vison\n{ \"version\": \"1\", \"type\": \"text\" }\n:::\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Container { name, data, .. } = &children[0] else {
        panic!("expected container");
    };

    assert_eq!(name, "vison");
    assert_eq!(
        data.as_ref().and_then(|data| data.pointer("/spec/type")),
        Some(&serde_json::json!("text"))
    );
    assert!(data
        .as_ref()
        .and_then(|data| data.pointer("/source"))
        .is_some());
}

#[test]
fn public_api_keeps_vison_parse_errors() {
    let ast = parse(":::vison\n{ invalid json\n:::\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Container { data, .. } = &children[0] else {
        panic!("expected container");
    };

    assert!(data
        .as_ref()
        .and_then(|data| data.pointer("/parseError"))
        .is_some());
    assert!(data
        .as_ref()
        .and_then(|data| data.pointer("/spec"))
        .is_none());
}

#[test]
fn public_api_maps_html_container_data() {
    let ast = parse(":::html\n<div>Hello</div>\n:::\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Container { data, .. } = &children[0] else {
        panic!("expected container");
    };

    assert_eq!(
        data.as_ref().and_then(|data| data.pointer("/html")),
        Some(&serde_json::json!("<div>Hello</div>"))
    );
}

#[test]
fn public_api_maps_weather_container_data() {
    let ast = parse(":::weather yaml\nlocation: Beijing\nunits: metric\n:::\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Container {
        name, params, data, ..
    } = &children[0]
    else {
        panic!("expected container");
    };

    assert_eq!(name, "weather");
    assert_eq!(params.as_deref(), Some("yaml"));
    assert_eq!(
        data.as_ref().and_then(|data| data.pointer("/format")),
        Some(&serde_json::json!("yaml"))
    );
    assert_eq!(
        data.as_ref().and_then(|data| data.pointer("/location")),
        Some(&serde_json::json!("Beijing"))
    );
    assert_eq!(
        data.as_ref().and_then(|data| data.pointer("/units")),
        Some(&serde_json::json!("metric"))
    );
}

#[test]
fn public_api_preserves_raw_html_blocks() {
    let ast = parse("<div>Hello</div>\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Raw {
        format,
        value,
        block,
        ..
    } = &children[0]
    else {
        panic!("expected raw html");
    };

    assert_eq!(format, "html");
    assert_eq!(value, "<div>Hello</div>");
    assert!(*block);
}

#[test]
fn public_api_preserves_multiline_raw_html_blocks() {
    let ast = parse("<div>\n  <p>x</p>\n</div>\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Raw {
        format,
        value,
        block,
        ..
    } = &children[0]
    else {
        panic!("expected raw html block, got {children:?}");
    };

    assert_eq!(format, "html");
    assert_eq!(value, "<div>\n  <p>x</p>\n</div>");
    assert!(*block);
}

#[test]
fn public_api_preserves_inline_raw_html() {
    let ast = parse("text <span>x</span> y\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Paragraph {
        children: paragraph,
        ..
    } = &children[0]
    else {
        panic!("expected paragraph, got {children:?}");
    };

    let has_inline_raw = paragraph.iter().any(|node| {
        matches!(
            node,
            SupramarkNode::Raw { format, value, block, .. }
                if format == "html" && value == "<span>" && !block
        )
    });
    assert!(has_inline_raw, "expected inline raw html, got {paragraph:?}");
}

#[test]
fn public_api_reports_unclosed_extension_blocks() {
    let ast = parse(":::map\ncenter: [0, 0]\n");
    let SupramarkNode::Root {
        diagnostics,
        children,
        ..
    } = ast
    else {
        panic!("expected root");
    };

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Error);
    let SupramarkNode::Unsupported {
        syntax,
        reason,
        diagnostics: node_diagnostics,
        ..
    } = &children[0]
    else {
        panic!("expected unsupported");
    };

    assert_eq!(syntax, "container");
    assert_eq!(reason, "missing closing marker");
    assert_eq!(node_diagnostics.len(), 1);
}

#[test]
fn public_api_maps_math_blocks() {
    let ast = parse("$$\nE = mc^2\n$$\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::MathBlock {
        value, position, ..
    } = &children[0]
    else {
        panic!("expected math block");
    };

    assert_eq!(value, "E = mc^2");
    assert!(position.is_some());
}

#[test]
fn public_api_maps_inline_math() {
    let ast = parse("Energy: $E = mc^2$.");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Paragraph {
        children: paragraph,
        ..
    } = &children[0]
    else {
        panic!("expected paragraph");
    };

    assert!(matches!(
        &paragraph[1],
        SupramarkNode::MathInline { value, .. } if value == "E = mc^2"
    ));
}

#[test]
fn public_api_maps_footnotes() {
    let ast = parse("Text[^a].\n\n[^a]: Footnote.");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Paragraph {
        children: paragraph,
        ..
    } = &children[0]
    else {
        panic!("expected paragraph");
    };

    assert!(matches!(
        &paragraph[1],
        SupramarkNode::FootnoteReference { index, label, .. } if *index == 1 && label == "a"
    ));
    assert!(matches!(
        &children[1],
        SupramarkNode::FootnoteDefinition { index, label, .. } if *index == 1 && label == "a"
    ));
}

#[test]
fn public_api_replaces_emoji_shortcodes_without_breaking_unicode_list_items() {
    let ast = parse("- :smile: :joy: :wink:\n- :rocket: :tada: :warning:\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::List { children, .. } = &children[0] else {
        panic!("expected list");
    };

    let SupramarkNode::ListItem {
        children: first_item,
        ..
    } = &children[0]
    else {
        panic!("expected first item");
    };
    let SupramarkNode::ListItem {
        children: second_item,
        ..
    } = &children[1]
    else {
        panic!("expected second item");
    };

    assert_eq!(first_text(first_item), "😄 😂 😉");
    assert_eq!(first_text(second_item), "🚀 🎉 ⚠️");
}

#[test]
fn public_api_maps_definition_lists_with_v2_children() {
    let source = "Term\n:   Definition\n\nAfter";
    let ast = parse(source);
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };

    assert_eq!(children.len(), 2);
    let SupramarkNode::DefinitionList {
        children: items, ..
    } = &children[0]
    else {
        panic!("expected definition list");
    };
    let SupramarkNode::DefinitionItem {
        children: item_children,
        ..
    } = &items[0]
    else {
        panic!("expected definition item");
    };
    let SupramarkNode::DefinitionTerm {
        children: term_children,
        ..
    } = &item_children[0]
    else {
        panic!("expected definition term");
    };
    let SupramarkNode::DefinitionDescription {
        children: description_children,
        ..
    } = &item_children[1]
    else {
        panic!("expected definition description");
    };

    assert_eq!(first_text(term_children), "Term");
    let SupramarkNode::Paragraph {
        children: paragraph_children,
        ..
    } = &description_children[0]
    else {
        panic!("expected description paragraph");
    };
    assert_eq!(first_text(paragraph_children), "Definition");

    let SupramarkNode::Paragraph { position, .. } = &children[1] else {
        panic!("expected trailing paragraph");
    };
    assert_eq!(
        position.as_ref().map(|position| position.start.byte_offset),
        Some(source.find("After").unwrap())
    );
}

// --- nesting regression tests: extension blocks now compose inside other
// block constructs (they were top-level only under the old prescan) ---

#[test]
fn nests_math_block_inside_list_item() {
    let ast = parse("- item\n\n  $$\n  E=mc^2\n  $$\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::List { children: items, .. } = &children[0] else {
        panic!("expected list");
    };
    let SupramarkNode::ListItem { children: item, .. } = &items[0] else {
        panic!("expected list item");
    };
    let SupramarkNode::MathBlock { value, .. } = &item[1] else {
        panic!("expected math block nested in list item, got {item:?}");
    };
    assert_eq!(value, "E=mc^2");
}

#[test]
fn nests_container_inside_list_item() {
    let ast = parse("- item\n\n  :::map\n  center: [0,0]\n  :::\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::List { children: items, .. } = &children[0] else {
        panic!("expected list");
    };
    let SupramarkNode::ListItem { children: item, .. } = &items[0] else {
        panic!("expected list item");
    };
    let SupramarkNode::Container { name, value, .. } = &item[1] else {
        panic!("expected container nested in list item, got {item:?}");
    };
    assert_eq!(name, "map");
    assert_eq!(value.as_deref(), Some("center: [0,0]"));
}

#[test]
fn nests_math_block_inside_blockquote() {
    let ast = parse("> $$\n> E=mc^2\n> $$\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Blockquote { children: bq, .. } = &children[0] else {
        panic!("expected blockquote");
    };
    let SupramarkNode::MathBlock { value, .. } = &bq[0] else {
        panic!("expected math block nested in blockquote, got {bq:?}");
    };
    assert_eq!(value, "E=mc^2");
}

#[test]
fn nests_footnote_definition_inside_blockquote() {
    let ast = parse("> [^a]: note\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Blockquote { children: bq, .. } = &children[0] else {
        panic!("expected blockquote");
    };
    let SupramarkNode::FootnoteDefinition { label, .. } = &bq[0] else {
        panic!("expected footnote definition nested in blockquote, got {bq:?}");
    };
    assert_eq!(label, "a");
}
