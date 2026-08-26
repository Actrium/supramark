use supramark_markdown::{
    parse, parse_with_options, DiagnosticSeverity, ExtensionMode, ParseOptions, SupramarkNode,
    TableAlign,
};

#[test]
fn public_api_outputs_ast_v2_with_positions() {
    // cjk-allow: multi-byte CJK + emoji source needed to exercise utf16-offset math
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
fn public_api_parses_diagram_meta_into_object() {
    let ast = parse("```mermaid theme=dark zoom=3 wide title=\"hi\"\ngraph TD; A-->B;\n```\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Diagram { engine, meta, .. } = &children[0] else {
        panic!("expected diagram, got {children:?}");
    };
    assert_eq!(engine, "mermaid");
    let meta = meta.as_ref().expect("diagram meta object");
    assert_eq!(meta["theme"], serde_json::json!("dark"));
    assert_eq!(meta["zoom"], serde_json::json!("3"));
    assert_eq!(meta["wide"], serde_json::json!(true));
    assert_eq!(meta["title"], serde_json::json!("hi"));
}

#[test]
fn public_api_omits_empty_diagram_meta() {
    let ast = parse(
        "```mermaid
graph TD; A-->B;
```
",
    );
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Diagram { meta, .. } = &children[0] else {
        panic!("expected diagram");
    };
    assert!(meta.is_none());
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
    // cmark-gfm's tasklist scan consumes the `[x]`/`[ ]` marker plus its
    // trailing separator whitespace (ext_scanners.re `spacechar+`); the text
    // node carries the item text with no leading whitespace, and the single
    // space in `<input ... /> Done` comes from the renderer's literal, not the
    // text node.
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

// GFM autolink extension (cmark-gfm 0.29 conformance, spec-0621..0631 +
// extensions-0019). Bare www./scheme-URL/email text is linkified into a Link
// node whose child text is the raw matched substring, mirroring cmark-gfm's
// postprocess pass. See issue #144.
fn first_link(nodes: &[SupramarkNode]) -> (&str, &str) {
    match &nodes[0] {
        SupramarkNode::Paragraph { children, .. } => first_link(children),
        SupramarkNode::Link { url, children, .. } => {
            let SupramarkNode::Text { value, .. } = &children[0] else {
                panic!("expected text child, got {:?}", children);
            };
            (url, value)
        }
        _ => panic!("expected link, got {:?}", nodes[0]),
    }
}

#[test]
fn gfm_autolink_www_basic() {
    // spec-0621: `www.commonmark.org` -> <a href="http://www.commonmark.org">www.commonmark.org</a>
    let ast = parse("www.commonmark.org\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let (url, text) = first_link(&children);
    assert_eq!(url, "http://www.commonmark.org");
    assert_eq!(text, "www.commonmark.org");
}

#[test]
fn gfm_autolink_www_paren_balancing() {
    // spec-0624: parens inside the path are kept, a trailing unmatched `)` is stripped.
    let ast = parse("www.google.com/search?q=Markup+(business))\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let (url, _text) = first_link(&children);
    assert_eq!(url, "http://www.google.com/search?q=Markup+(business)");
}

#[test]
fn gfm_autolink_www_lt_truncation() {
    // spec-0627: `www.commonmark.org/he<lp` -> URL ends at `<`.
    let ast = parse("www.commonmark.org/he<lp\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let (url, _text) = first_link(&children);
    assert_eq!(url, "http://www.commonmark.org/he");
}

#[test]
fn gfm_autolink_scheme_url() {
    // spec-0628: bare http://, https://, ftp:// URLs are linkified.
    let ast = parse("https://encrypted.google.com/search?q=Markup+(business)\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let (url, _text) = first_link(&children);
    assert_eq!(
        url,
        "https://encrypted.google.com/search?q=Markup+(business)"
    );
}

#[test]
fn gfm_autolink_email_basic() {
    // spec-0629: `foo@bar.baz` -> mailto: link.
    let ast = parse("foo@bar.baz\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let (url, text) = first_link(&children);
    assert_eq!(url, "mailto:foo@bar.baz");
    assert_eq!(text, "foo@bar.baz");
}

#[test]
fn gfm_autolink_email_trailing_punct_stripped() {
    // spec-0631: trailing `.`, `-`, `_` after the domain are not part of the link.
    let ast = parse("foo@bar.baz/_test_link\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let (url, _text) = first_link(&children);
    assert_eq!(url, "mailto:foo@bar.baz");
}

#[test]
fn gfm_autolink_does_not_linkify_inside_link() {
    // Text already inside a markdown link must not be re-linkified (cmark-gfm
    // postprocess skips CMARK_NODE_LINK). `[x](http://a.b)` keeps the link text
    // `x` verbatim.
    let ast = parse("[click www.example.com](http://x.y)\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Paragraph { children, .. } = &children[0] else {
        panic!("expected paragraph");
    };
    let SupramarkNode::Link { children: link_children, .. } = &children[0] else {
        panic!("expected link, got {:?}", children[0]);
    };
    // The link text is the single literal `click www.example.com` — no nested
    // autolink child.
    assert!(link_children.iter().all(|c| !matches!(
        c,
        SupramarkNode::Link { .. }
    )));
}

#[test]
fn gfm_autolink_does_not_linkify_inside_code_span() {
    // cmark-gfm's autolink postprocess only walks text nodes, never the
    // content of a code span. `` `<http://foo.bar.`baz>` `` keeps the URL
    // literal inside the code span (spec-0355).
    let ast = parse("`<http://foo.bar.`baz>`\n");
    let para = paragraph_children(ast);
    let SupramarkNode::InlineCode { value, .. } = &para[0] else {
        panic!("expected inline code, got {:?}", para[0]);
    };
    assert_eq!(value, "<http://foo.bar.");
    // Trailing literal text is also untouched.
    let SupramarkNode::Text { value, .. } = &para[1] else {
        panic!("expected trailing text, got {:?}", para[1]);
    };
    assert_eq!(value, "baz>`");
}

#[test]
fn gfm_autolink_does_not_linkify_image_alt() {
    // cmark-gfm leaves URLs inside an image's alt text literal — the alt is
    // collected as a plain string attribute, not rendered content. The GFM
    // autolink postprocess must not enter image children (extensions-0019).
    let ast = parse("![http://inline.com/image](http://inline.com/image)\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Paragraph { children, .. } = &children[0] else {
        panic!("expected paragraph");
    };
    let SupramarkNode::Image { url, alt, .. } = &children[0] else {
        panic!("expected image, got {:?}", children[0]);
    };
    assert_eq!(url, "http://inline.com/image");
    assert_eq!(alt, "http://inline.com/image");
}

#[test]
fn gfm_autolink_option_disables_bare_url_linkification() {
    // The CommonMark conformance suite parses with the GFM autolink extension
    // disabled so bare URLs stay literal (CommonMark spec has no bare-URL
    // autolink). `parse` (default) linkifies; `parse_with_options` with
    // `gfm_autolink: false` does not.
    let default_ast = parse("https://example.com\n");
    let SupramarkNode::Root { children, .. } = default_ast else {
        panic!("expected root");
    };
    let SupramarkNode::Paragraph { children, .. } = &children[0] else {
        panic!("expected paragraph");
    };
    assert!(
        matches!(children[0], SupramarkNode::Link { .. }),
        "default profile should autolink bare URLs"
    );

    let mut options = ParseOptions::default();
    options.gfm_autolink = false;
    let ast = parse_with_options("https://example.com\n", options);
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Paragraph { children, .. } = &children[0] else {
        panic!("expected paragraph");
    };
    let SupramarkNode::Text { value, .. } = &children[0] else {
        panic!("expected literal text, got {:?}", children[0]);
    };
    assert_eq!(value, "https://example.com");
}

#[test]
fn wikilink_option_enables_wikilink_parsing() {
    // Inverted mirror of the gfm_autolink gate above: WikiLink is OFF by
    // default (`[[...]]` is not CommonMark/GFM syntax), and the option turns
    // it on. Default parsing must stay byte-identical to the CommonMark/GFM
    // profiles.
    let default_ast = parse("[[Project Plan]]\n");
    let SupramarkNode::Root { children, .. } = default_ast else {
        panic!("expected root");
    };
    let SupramarkNode::Paragraph { children, .. } = &children[0] else {
        panic!("expected paragraph");
    };
    assert!(
        matches!(&children[0], SupramarkNode::Text { value, .. } if value == "[[Project Plan]]"),
        "default options must leave [[...]] as text, got {:?}",
        children[0]
    );

    let mut options = ParseOptions::default();
    options.wikilink = true;
    let ast = parse_with_options("[[Project Plan]]\n", options);
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Paragraph { children, .. } = &children[0] else {
        panic!("expected paragraph");
    };
    assert!(
        matches!(
            &children[0],
            SupramarkNode::WikiLink { target, section: None, label: None, .. }
                if target == "Project Plan"
        ),
        "option on should produce a wiki_link, got {:?}",
        children[0]
    );
}

#[test]
fn wikilink_serializes_tagged_node_omitting_absent_fields() {
    let mut options = ParseOptions::default();
    options.wikilink = true;
    let json = serde_json::to_string(&parse_with_options("[[a|b]]\n", options)).unwrap();
    assert!(json.contains(r#""type":"wiki_link""#), "{json}");
    assert!(json.contains(r#""target":"a""#), "{json}");
    assert!(json.contains(r#""label":"b""#), "{json}");
    assert!(!json.contains("section"), "absent section must not serialize: {json}");

    let json = serde_json::to_string(&parse_with_options("[[a]]\n", options)).unwrap();
    assert!(json.contains(r#""type":"wiki_link""#), "{json}");
    assert!(
        !json.contains("label") && !json.contains("section"),
        "absent label/section must not serialize: {json}"
    );
}

#[test]
fn wikilink_option_defers_to_inline_math() {
    // Regression for the PR-208 review: with the wikilink option on, `$[[foo]]$`
    // must stay one math_inline — the WikiLink scanner declines a `[[` that
    // sits inside an open math span so the text post-pass still claims `$…$`.
    let mut options = ParseOptions::default();
    options.wikilink = true;
    let ast = parse_with_options("pre $[[foo]]$ post\n", options);
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Paragraph { children, .. } = &children[0] else {
        panic!("expected paragraph");
    };
    let math = children
        .iter()
        .find_map(|n| match n {
            SupramarkNode::MathInline { value, .. } => Some(value.clone()),
            _ => None,
        })
        .expect("math_inline must survive the wikilink option");
    assert_eq!(math, "[[foo]]");
    assert!(
        children
            .iter()
            .all(|n| !matches!(n, SupramarkNode::WikiLink { .. })),
        "no wiki_link inside a math span: {children:?}"
    );
}

// GFM strikethrough (cmark-gfm 0.29 conformance, extensions-0018). Both `~x~`
// and `~~x~~` produce a single <del>; runs of 3+ tildes and mismatched lengths
// stay literal. See issue #144.
fn paragraph_children(ast: SupramarkNode) -> Vec<SupramarkNode> {
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::Paragraph { children, .. } = &children[0] else {
        panic!("expected paragraph, got {:?}", children[0]);
    };
    children.clone()
}

#[test]
fn gfm_strikethrough_single_tilde() {
    // extensions-0018: `~one~` -> <del>one</del>
    let para = paragraph_children(parse("~one~\n"));
    assert!(matches!(
        &para[0],
        SupramarkNode::Delete { children, .. } if matches!(&children[0], SupramarkNode::Text { value, .. } if value == "one")
    ));
}

#[test]
fn gfm_strikethrough_double_tilde_single_del() {
    // extensions-0018: `~~two~~` -> a single <del>two</del> (not nested).
    let para = paragraph_children(parse("~~two~~\n"));
    let SupramarkNode::Delete { children, .. } = &para[0] else {
        panic!("expected delete, got {:?}", para[0]);
    };
    assert!(matches!(
        &children[0],
        SupramarkNode::Text { value, .. } if value == "two"
    ));
    // No nested Delete.
    assert!(children.iter().all(|c| !matches!(c, SupramarkNode::Delete { .. })));
}

#[test]
fn gfm_strikethrough_three_tildes_literal() {
    // extensions-0018: a run of 3+ tildes is not a delimiter — it stays literal.
    // (Wrapped in a paragraph so the leading `~~~` isn't read as a code fence.)
    let para = paragraph_children(parse("x ~~~three~~~ y\n"));
    // No Delete node formed; the tildes survive as text.
    assert!(para.iter().all(|c| !matches!(c, SupramarkNode::Delete { .. })));
    let joined: String = para
        .iter()
        .filter_map(|c| match c {
            SupramarkNode::Text { value, .. } => Some(value.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(joined, "x ~~~three~~~ y");
}

#[test]
fn gfm_strikethrough_mismatched_lengths_literal() {
    // extensions-0018: `~mismatch~~` -> literal (opener and closer must have
    // equal length).
    let para = paragraph_children(parse("~mismatch~~\n"));
    let SupramarkNode::Text { value, .. } = &para[0] else {
        panic!("expected literal text, got {:?}", para[0]);
    };
    assert_eq!(value, "~mismatch~~");
}

#[test]
fn gfm_strikethrough_mixed_single_and_double() {
    // extensions-0018: `~one~ ~~two~~` -> <del>one</del> <del>two</del>
    let para = paragraph_children(parse("~one~ ~~two~~\n"));
    let dels: Vec<_> = para
        .iter()
        .filter_map(|c| match c {
            SupramarkNode::Delete { children, .. } => Some(children.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(dels.len(), 2, "expected two <del>, got {para:?}");
}

#[test]
fn gfm_strikethrough_inner_tilde_preserved() {
    // extensions-0018: `~is ~ legit~` -> <del>is ~ legit</del>. The middle `~`
    // is flanked by spaces (neither open nor close) so it stays literal inside.
    let para = paragraph_children(parse("~is ~ legit~\n"));
    let SupramarkNode::Delete { children, .. } = &para[0] else {
        panic!("expected delete, got {:?}", para[0]);
    };
    let SupramarkNode::Text { value, .. } = &children[0] else {
        panic!("expected text child, got {:?}", children);
    };
    assert_eq!(value, "is ~ legit");
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
    // An HTML block owns its line terminator. CommonMark 0.31.2 defines the
    // expected output for `<div>Hello</div>\n` as the source lines verbatim,
    // newline included, which is what `html_block.rs` now emits (700cc57e) and
    // what the 652/652 conformance run depends on. This assertion encoded the
    // pre-#121 behaviour and was not updated with it.
    assert_eq!(value, "<div>Hello</div>\n");
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
    // Trailing newline retained, same rule as the single-line case above.
    assert_eq!(value, "<div>\n  <p>x</p>\n</div>\n");
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
    assert!(
        has_inline_raw,
        "expected inline raw html, got {paragraph:?}"
    );
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
fn public_api_inline_math_widehat_escaped_braces_207() {
    // Regression for #207: inline math whose TeX source contains cmark
    // backslash-escaped punctuation (`\{`, `\}`) must still parse as a
    // math_inline node — not collapse to literal text — and its value must
    // preserve the raw escapes so the TeX engine receives `\{0, ?, 1\}` (a
    // literal set), not `{0, ?, 1}`. Covers both `\widehat{\rho}` and
    // `\widehat\rho` spellings, and both inline `$…$` and block `$$…$$`.
    let inline_cases: &[(&str, &str)] = &[
        ("$\\widehat{\\rho}=1$", "\\widehat{\\rho}=1"),
        ("$\\widehat\\rho=1$", "\\widehat\\rho=1"),
        (
            "text $\\widehat{\\rho}_{\\Gamma,q}(a,s) \\in \\{0, ?, 1\\}$ text",
            "\\widehat{\\rho}_{\\Gamma,q}(a,s) \\in \\{0, ?, 1\\}",
        ),
    ];
    for (input, expected) in inline_cases {
        let ast = parse(input);
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root for {input}");
        };
        let SupramarkNode::Paragraph { children: paragraph, .. } = &children[0] else {
            panic!("expected paragraph for {input}");
        };
        let value = paragraph
            .iter()
            .find_map(|n| match n {
                SupramarkNode::MathInline { value, .. } => Some(value.as_str()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected a math_inline node for {input}"));
        assert_eq!(value, *expected, "inline math value for {input}");
    }

    // Block form: escaped braces inside `$$…$$` already worked, but assert the
    // value preserves them so a future change cannot silently strip `\{`.
    let ast = parse("$$\n\\widehat{\\rho}_{\\Gamma,q}(a,s) \\in \\{0, ?, 1\\}\n$$");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::MathBlock { value, .. } = &children[0] else {
        panic!("expected math_block");
    };
    assert_eq!(value, "\\widehat{\\rho}_{\\Gamma,q}(a,s) \\in \\{0, ?, 1\\}");
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
        SupramarkNode::FootnoteReference { index, label, identifier, .. }
            if *index == 1 && label == "a" && identifier == "a"
    ));
    assert!(matches!(
        &children[1],
        SupramarkNode::FootnoteDefinition { index, label, identifier, .. }
            if *index == 1 && label == "a" && identifier == "a"
    ));
}

#[test]
fn public_api_associates_footnotes_by_normalized_identifier() {
    // Reference label "My Note" and definition label "my  note" differ in case
    // and whitespace but normalize to the same identifier, so they must share an
    // index and stay associated.
    let ast = parse("Text[^My Note].\n\n[^my  note]: Body.");
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

    let SupramarkNode::FootnoteReference {
        index: ref_index,
        label: ref_label,
        identifier: ref_id,
        ..
    } = &paragraph[1]
    else {
        panic!("expected footnote reference, got {:?}", paragraph[1]);
    };
    let SupramarkNode::FootnoteDefinition {
        index: def_index,
        label: def_label,
        identifier: def_id,
        ..
    } = &children[1]
    else {
        panic!("expected footnote definition, got {:?}", children[1]);
    };

    assert_eq!(ref_label, "My Note");
    assert_eq!(def_label, "my  note");
    assert_eq!(ref_id, "my note");
    assert_eq!(def_id, "my note");
    assert_eq!(ref_index, def_index);
    assert_ne!(*ref_index, 0);
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
    let SupramarkNode::List {
        children: items, ..
    } = &children[0]
    else {
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
    let SupramarkNode::List {
        children: items, ..
    } = &children[0]
    else {
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

#[test]
fn footnote_definition_absorbs_indented_continuation() {
    // cmark-gfm footnote definitions are containers: lines indented >= 4 past
    // the definition base become block children (blockquote, indented code,
    // paragraph). Regression guard for cmark-gfm extensions-0023.
    let src = "[^a]:\n    > quoted.\n\n        code line\n\n    para line.\n";
    let ast = parse(src);
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::FootnoteDefinition { children, label, .. } = &children[0] else {
        panic!("expected footnote definition, got {:?}", &children[0]);
    };
    assert_eq!(label, "a");
    // blockquote, code, paragraph — in source order.
    assert!(matches!(children[0], SupramarkNode::Blockquote { .. }));
    assert!(matches!(children[1], SupramarkNode::Code { .. }));
    assert!(matches!(children[2], SupramarkNode::Paragraph { .. }));
    if let SupramarkNode::Code { value, .. } = &children[1] {
        assert_eq!(value, "code line\n");
    }
    // The indented continuation must NOT leak as a root-level code block.
    assert!(
        children.len() == 3,
        "footnote def should own all continuation, got {children:?}"
    );
}

#[test]
fn footnote_definition_continuation_ends_at_new_block() {
    // A non-indented line that opens a new block (another definition) ends the
    // current definition's continuation; it is not absorbed.
    let ast = parse("[^a]: first\n\n[^b]: second.\n");
    let SupramarkNode::Root { children, .. } = ast else {
        panic!("expected root");
    };
    let SupramarkNode::FootnoteDefinition { label: a, children: a_children, .. } = &children[0]
    else {
        panic!("expected first definition, got {:?}", &children[0]);
    };
    assert_eq!(a, "a");
    assert_eq!(a_children.len(), 1, "first def should have one paragraph child");
    let SupramarkNode::FootnoteDefinition { label: b, .. } = &children[1] else {
        panic!("expected second definition, got {:?}", &children[1]);
    };
    assert_eq!(b, "b");
}

#[test]
fn public_api_maps_emphasis_spanning_an_escape_inside_a_table_cell() {
    // The table cell scanner hands the inline parser the *unescaped* cell content (`_x|y_`),
    // which is shorter than the source range it came from (`_x\|y_`). Emphasis matching used to
    // measure the token in source coordinates and apply it to content coordinates, which
    // underflowed and aborted the parse. Regression guard for cmark-gfm's "Embedded pipes" case.
    let source = "| a |\n| - |\n| _x\\|y_ |\n";
    let ast = parse(source);

    let SupramarkNode::Root { children, .. } = &ast else {
        panic!("expected root");
    };
    let SupramarkNode::Table { children, .. } = &children[0] else {
        panic!("expected table, got {:?}", &children[0]);
    };
    let SupramarkNode::TableRow { children, .. } = &children[1] else {
        panic!("expected body row, got {:?}", &children[1]);
    };
    let SupramarkNode::TableCell { children, .. } = &children[0] else {
        panic!("expected cell, got {:?}", &children[0]);
    };
    let SupramarkNode::Emphasis {
        children, position, ..
    } = &children[0]
    else {
        panic!("expected emphasis, got {:?}", &children[0]);
    };

    // The emphasis must be mapped back onto the escaped source text, not the unescaped content.
    let position = position.as_ref().expect("emphasis position");
    assert_eq!(
        &source[position.start.byte_offset..position.end.byte_offset],
        "_x\\|y_"
    );

    let SupramarkNode::Text { value, .. } = &children[0] else {
        panic!("expected text, got {:?}", &children[0]);
    };
    assert_eq!(value, "x|y");
}
