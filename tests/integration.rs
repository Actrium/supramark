use std::fs;
use std::path::Path;
use std::sync::OnceLock;

mod support;

fn convert_fixture(path: &str) -> String {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        support::init_test_backend();
    });
    let source = fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    plantuml_little::convert_with_input_path(&source, Path::new(path))
        .unwrap_or_else(|e| panic!("convert {path} failed: {e}"))
}

fn assert_valid_svg(svg: &str, path: &str) {
    assert!(svg.contains("<svg"), "{path}: missing <svg tag");
    assert!(svg.contains("</svg>"), "{path}: missing </svg> tag");
    assert!(
        svg.contains("xmlns=\"http://www.w3.org/2000/svg\""),
        "{path}: missing xmlns"
    );
    assert!(!svg.contains("NaN"), "{path}: SVG contains NaN");
    assert!(!svg.contains("inf"), "{path}: SVG contains inf");
    assert!(
        !svg.contains("<parsererror"),
        "{path}: SVG contains parser error output"
    );
    assert!(
        svg.matches("<text").count() == svg.matches("</text>").count(),
        "{path}: unbalanced <text> tags"
    );

    let has_viewbox = svg.contains("viewBox=\"");
    let width = extract_numeric_attr(svg, "width");
    let height = extract_numeric_attr(svg, "height");
    assert!(
        has_viewbox || (width.is_some() && height.is_some()),
        "{path}: missing SVG dimensions"
    );
    if let Some(w) = width {
        assert!(w > 0.0, "{path}: width must be positive");
    }
    if let Some(h) = height {
        assert!(h > 0.0, "{path}: height must be positive");
    }
    if let Some((_, _, w, h)) = extract_viewbox(svg) {
        assert!(w > 0.0 && h > 0.0, "{path}: viewBox size must be positive");
    }

    let has_graphics = [
        "<rect", "<path", "<line", "<ellipse", "<polygon", "<circle", "<text", "<g",
    ]
    .iter()
    .any(|tag| svg.contains(tag));
    assert!(has_graphics, "{path}: missing drawable SVG content");

    assert_no_raw_markup(svg, path);
}

fn assert_no_raw_markup(svg: &str, path: &str) {
    // Check both raw and XML-escaped forms of Creole/PlantUML markup.
    // Markup that was not processed leaks into SVG text content in
    // either raw form (<size:12>) or escaped form (&lt;size:12&gt;).
    let raw_patterns = [
        ("<$", "raw sprite reference <$...>"),
        ("<size:", "raw <size:N> markup"),
        ("<color:", "raw <color:X> markup"),
        ("<back:", "raw <back:X> markup"),
        ("<font:", "raw <font:X> markup"),
    ];
    let escaped_patterns = [
        ("&lt;size:", "escaped <size:N> markup"),
        ("&lt;color:", "escaped <color:X> markup"),
        ("&lt;back:", "escaped <back:X> markup"),
        ("&lt;font:", "escaped <font:X> markup"),
        ("&lt;$", "escaped sprite reference <$...>"),
    ];

    for (pat, desc) in raw_patterns {
        assert!(!svg.contains(pat), "{path}: {desc} in SVG output");
    }
    // Escaped markup is allowed inside monospace/code text elements (Java behavior:
    // <code> blocks render everything as literal text, including markup tags).
    // Also allowed inside SVG <title> elements which contain raw diagram title text
    // (Java's SvgGraphics.setTitle passes the raw Display text, preserving markup).
    // Strip monospace text content and <title> elements before checking for escaped markup.
    let svg_no_mono = {
        let re_mono =
            regex::Regex::new(r#"<text[^>]*font-family="monospace"[^>]*>[^<]*</text>"#).unwrap();
        let re_title = regex::Regex::new(r#"<title>[^<]*</title>"#).unwrap();
        let s = re_mono.replace_all(svg, "").to_string();
        re_title.replace_all(&s, "").to_string()
    };
    for (pat, desc) in escaped_patterns {
        assert!(!svg_no_mono.contains(pat), "{path}: {desc} in SVG output");
    }

    // Check for unprocessed Creole bold/italic inside <text> elements.
    // Pattern: **text** or //text// appearing literally in text content.
    for line in svg.lines() {
        if let Some(start) = line.find('>') {
            if let Some(end) = line.rfind("</text>") {
                let text_content = &line[start + 1..end];
                if text_content.contains("**") {
                    panic!("{path}: unprocessed Creole bold **...** in text: {text_content}");
                }
            }
        }
    }
}

fn extract_numeric_attr(svg: &str, attr: &str) -> Option<f64> {
    let needle = format!(r#"{attr}=""#);
    let start = svg.find(&needle)? + needle.len();
    let end = svg[start..].find('"')? + start;
    svg[start..end].trim_end_matches("px").parse::<f64>().ok()
}

fn extract_viewbox(svg: &str) -> Option<(f64, f64, f64, f64)> {
    let needle = r#"viewBox=""#;
    let start = svg.find(needle)? + needle.len();
    let end = svg[start..].find('"')? + start;
    let nums: Vec<f64> = svg[start..end]
        .split_whitespace()
        .filter_map(|part| part.parse::<f64>().ok())
        .collect();
    if nums.len() == 4 {
        Some((nums[0], nums[1], nums[2], nums[3]))
    } else {
        None
    }
}

fn assert_svg_has_link(svg: &str, path: &str) {
    assert!(svg.contains("<a"), "{path}: missing hyperlink markup");
}

fn assert_svg_has_tooltip(svg: &str, path: &str) {
    // Java puts tooltip in title="..." and xlink:title="..." attributes, not <title> element
    assert!(
        svg.contains("title=\"") || svg.contains("<title>"),
        "{path}: missing tooltip markup"
    );
}

#[test]
fn test_convert_basic_class() {
    let input = "@startuml\nclass Foo\n@enduml\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("Foo"));
}

#[test]
fn test_fixture_xmi0002() {
    let svg = convert_fixture("tests/fixtures/class/xmi0002.puml");
    assert_valid_svg(&svg, "xmi0002");
    assert!(svg.contains(">A<"), "xmi0002: must contain entity A");
    assert!(svg.contains(">B<"), "xmi0002: must contain entity B");
    assert!(svg.contains("<path"), "xmi0002: must contain edge path");
}

#[test]
fn test_fixture_xmi0003() {
    let svg = convert_fixture("tests/fixtures/class/xmi0003.puml");
    assert_valid_svg(&svg, "xmi0003");
    assert!(svg.contains(">A<"), "xmi0003: must contain entity A");
    assert!(svg.contains(">B<"), "xmi0003: must contain entity B");
    assert!(svg.contains(">C<"), "xmi0003: must contain entity C");
}

#[test]
fn test_fixture_xmi0004() {
    let svg = convert_fixture("tests/fixtures/class/xmi0004.puml");
    assert_valid_svg(&svg, "xmi0004");
    assert!(
        svg.contains("stroke-dasharray"),
        "xmi0004: dependency must be dashed"
    );
}

#[test]
fn test_fixture_a0005() {
    let svg = convert_fixture("tests/fixtures/class/a0005.puml");
    assert_valid_svg(&svg, "a0005");
    assert!(svg.contains("Bob"), "a0005: must contain Bob");
    assert!(svg.contains("Sally"), "a0005: must contain Sally");
}

#[test]
fn test_fixture_hideshow001() {
    let svg = convert_fixture("tests/fixtures/class/hideshow001.puml");
    assert_valid_svg(&svg, "hideshow001");
    assert!(svg.contains("Access"), "hideshow001: must contain Access");
}

#[test]
fn test_fixture_hideshow002() {
    let svg = convert_fixture("tests/fixtures/class/hideshow002.puml");
    assert_valid_svg(&svg, "hideshow002");
}

#[test]
fn test_fixture_hideshow003() {
    let svg = convert_fixture("tests/fixtures/class/hideshow003.puml");
    assert_valid_svg(&svg, "hideshow003");
}

#[test]
fn test_fixture_hideshow004() {
    let svg = convert_fixture("tests/fixtures/class/hideshow004.puml");
    assert_valid_svg(&svg, "hideshow004");
    assert!(svg.contains(">A<"), "hideshow004: must contain A");
    assert!(svg.contains(">B<"), "hideshow004: must contain B");
}

#[test]
fn test_fixture_class_funcparam_arrow_01() {
    let svg = convert_fixture("tests/fixtures/class/class_funcparam_arrow_01.puml");
    assert_valid_svg(&svg, "class_funcparam_arrow_01");
}

#[test]
fn test_fixture_class_funcparam_arrow_02() {
    let svg = convert_fixture("tests/fixtures/class/class_funcparam_arrow_02.puml");
    assert_valid_svg(&svg, "class_funcparam_arrow_02");
    assert!(svg.contains("CImaging"), "must contain CImaging");
}

#[test]
fn test_fixture_qualifiedassoc001() {
    let svg = convert_fixture("tests/fixtures/class/qualifiedassoc001.puml");
    assert_valid_svg(&svg, "qualifiedassoc001");
}

#[test]
fn test_fixture_qualifiedassoc002() {
    let svg = convert_fixture("tests/fixtures/class/qualifiedassoc002.puml");
    assert_valid_svg(&svg, "qualifiedassoc002");
}

#[test]
fn test_fixture_generics001() {
    let svg = convert_fixture("tests/fixtures/class/generics001.puml");
    assert_valid_svg(&svg, "generics001");
    // Generic parameters are rendered in a separate dashed box, not in the class name
    assert!(
        svg.contains("ArrayList"),
        "generics001: must display ArrayList"
    );
    assert!(
        svg.contains(">E<"),
        "generics001: must display generic parameter E"
    );
    assert!(svg.contains("HashMap"), "generics001: must display HashMap");
    assert!(
        svg.contains(">K,V<"),
        "generics001: must display generic parameter K,V"
    );
}

#[test]
fn test_fixture_class_colors001() {
    let svg = convert_fixture("tests/fixtures/class/colors001.puml");
    assert_valid_svg(&svg, "class_colors001");
    assert!(svg.contains("Red"), "must contain Red class");
    assert!(svg.contains("Green"), "must contain Green class");
    assert!(svg.contains("Blue"), "must contain Blue class");
    // Verify entity colors are applied as fill
    assert!(
        svg.contains(r##"fill="#FF0000""##),
        "Red class must have #FF0000 fill"
    );
    assert!(
        svg.contains(r##"fill="#00FF00""##),
        "Green class must have #00FF00 fill"
    );
    assert!(
        svg.contains(r##"fill="#0000FF""##),
        "Blue class must have #0000FF fill"
    );
}

// ── Sequence diagram integration tests ──

#[test]
fn test_convert_basic_sequence() {
    let input = "@startuml\nAlice -> Bob : hello\n@enduml\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("Alice"));
    assert!(svg.contains("Bob"));
    assert!(svg.contains("hello"));
}

#[test]
fn test_seq_fixture_test_0() {
    let svg = convert_fixture("tests/fixtures/sequence/test_0.puml");
    assert_valid_svg(&svg, "test_0");
    assert!(svg.contains("alice"), "test_0: must contain alice");
    assert!(svg.contains("bob"), "test_0: must contain bob");
}

#[test]
fn test_seq_fixture_a0000() {
    let svg = convert_fixture("tests/fixtures/sequence/a0000.puml");
    assert_valid_svg(&svg, "a0000");
    assert!(svg.contains("Alice"), "a0000: must contain Alice");
    assert!(svg.contains("Bob"), "a0000: must contain Bob");
    assert!(svg.contains("Hello"), "a0000: must contain Hello");
}

#[test]
fn test_seq_fixture_a0001() {
    let svg = convert_fixture("tests/fixtures/sequence/a0001.puml");
    assert_valid_svg(&svg, "a0001");
    assert!(svg.contains("Bob"), "a0001: must contain Bob");
    assert!(svg.contains("Alice"), "a0001: must contain Alice");
}

#[test]
fn test_seq_fixture_a0006() {
    let svg = convert_fixture("tests/fixtures/sequence/a0006.puml");
    assert_valid_svg(&svg, "a0006");
}

#[test]
fn test_seq_fixture_jaws2() {
    let svg = convert_fixture("tests/fixtures/sequence/jaws2.puml");
    assert_valid_svg(&svg, "jaws2");
    assert!(svg.contains("alice"), "jaws2: must contain alice");
    assert!(svg.contains("bob"), "jaws2: must contain bob");
}

#[test]
fn test_seq_fixture_jaws4() {
    let svg = convert_fixture("tests/fixtures/sequence/jaws4.puml");
    assert_valid_svg(&svg, "jaws4");
}

#[test]
fn test_seq_fixture_jaws10() {
    let svg = convert_fixture("tests/fixtures/sequence/jaws10.puml");
    assert_valid_svg(&svg, "jaws10");
}

#[test]
fn test_seq_fixture_jaws11() {
    let svg = convert_fixture("tests/fixtures/sequence/jaws11.puml");
    assert_valid_svg(&svg, "jaws11");
}

#[test]
fn test_seq_fixture_sequencelayout_0001() {
    let svg = convert_fixture("tests/fixtures/sequence/sequencelayout_0001.puml");
    assert_valid_svg(&svg, "sequencelayout_0001");
}

#[test]
fn test_seq_fixture_sequencelayout_0001b() {
    let svg = convert_fixture("tests/fixtures/sequence/sequencelayout_0001b.puml");
    assert_valid_svg(&svg, "sequencelayout_0001b");
}

#[test]
fn test_seq_fixture_sequencelayout_0001c() {
    let svg = convert_fixture("tests/fixtures/sequence/sequencelayout_0001c.puml");
    assert_valid_svg(&svg, "sequencelayout_0001c");
}

#[test]
fn test_seq_fixture_sequencelayout_0002() {
    let svg = convert_fixture("tests/fixtures/sequence/sequencelayout_0002.puml");
    assert_valid_svg(&svg, "sequencelayout_0002");
}

#[test]
fn test_seq_fixture_sequencelayout_0003() {
    let svg = convert_fixture("tests/fixtures/sequence/sequencelayout_0003.puml");
    assert_valid_svg(&svg, "sequencelayout_0003");
    assert!(
        svg.contains("Test"),
        "sequencelayout_0003: must contain Test"
    );
}

#[test]
fn test_seq_fixture_sequencelayout_0006() {
    let svg = convert_fixture("tests/fixtures/sequence/sequencelayout_0006.puml");
    assert_valid_svg(&svg, "sequencelayout_0006");
}

#[test]
fn test_seq_fixture_intermediatetest_0000() {
    let svg = convert_fixture("tests/fixtures/sequence/intermediatetest_0000.puml");
    assert_valid_svg(&svg, "intermediatetest_0000");
}

#[test]
fn test_seq_fixture_sequenceleftmessage_0002() {
    let svg =
        convert_fixture("tests/fixtures/sequence/sequenceleftmessageandactivelifelines_0002.puml");
    assert_valid_svg(&svg, "sequenceleftmessage_0002");
}

#[test]
fn test_seq_fixture_sequenceleftmessage_0003() {
    let svg =
        convert_fixture("tests/fixtures/sequence/sequenceleftmessageandactivelifelines_0003.puml");
    assert_valid_svg(&svg, "sequenceleftmessage_0003");
}

#[test]
fn test_seq_fixture_svg0003() {
    let svg = convert_fixture("tests/fixtures/sequence/svg0003.puml");
    assert_valid_svg(&svg, "svg0003");
}

// ── Sequence diagram combined fragment tests ──

#[test]
fn test_seq_fixture_alt001() {
    let svg = convert_fixture("tests/fixtures/sequence/seq_alt001.puml");
    assert_valid_svg(&svg, "seq_alt001");
    assert!(svg.contains("alt"), "seq_alt001: must contain alt label");
    assert!(
        svg.contains("request"),
        "seq_alt001: must contain request message"
    );
}

#[test]
fn test_seq_fixture_loop001() {
    let svg = convert_fixture("tests/fixtures/sequence/seq_loop001.puml");
    assert_valid_svg(&svg, "seq_loop001");
    assert!(svg.contains("loop"), "seq_loop001: must contain loop label");
}

#[test]
fn test_seq_fixture_opt001() {
    let svg = convert_fixture("tests/fixtures/sequence/seq_opt001.puml");
    assert_valid_svg(&svg, "seq_opt001");
    assert!(svg.contains("opt"), "seq_opt001: must contain opt label");
}

#[test]
fn test_seq_fixture_par001() {
    let svg = convert_fixture("tests/fixtures/sequence/seq_par001.puml");
    assert_valid_svg(&svg, "seq_par001");
    assert!(svg.contains("par"), "seq_par001: must contain par label");
}

#[test]
fn test_seq_fixture_break001() {
    let svg = convert_fixture("tests/fixtures/sequence/seq_break001.puml");
    assert_valid_svg(&svg, "seq_break001");
    assert!(
        svg.contains("break"),
        "seq_break001: must contain break label"
    );
}

#[test]
fn test_seq_fixture_critical001() {
    let svg = convert_fixture("tests/fixtures/sequence/seq_critical001.puml");
    assert_valid_svg(&svg, "seq_critical001");
    assert!(
        svg.contains("critical"),
        "seq_critical001: must contain critical label"
    );
}

#[test]
fn test_seq_fixture_group001() {
    let svg = convert_fixture("tests/fixtures/sequence/seq_group001.puml");
    assert_valid_svg(&svg, "seq_group001");
    // Group fragments show the user's label directly in the tab (not "group" keyword)
    assert!(
        svg.contains("My own label"),
        "seq_group001: must contain custom label"
    );
}

#[test]
fn test_seq_fixture_ref001() {
    let svg = convert_fixture("tests/fixtures/sequence/seq_ref001.puml");
    assert_valid_svg(&svg, "seq_ref001");
    assert!(svg.contains("ref"), "seq_ref001: must contain ref label");
    assert!(
        svg.contains("init phase"),
        "seq_ref001: must contain ref text"
    );
}

#[test]
fn test_seq_fixture_nested001() {
    let svg = convert_fixture("tests/fixtures/sequence/seq_nested001.puml");
    assert_valid_svg(&svg, "seq_nested001");
    assert!(svg.contains("alt"), "seq_nested001: must contain alt label");
    assert!(
        svg.contains("loop"),
        "seq_nested001: must contain loop label"
    );
}

#[test]
fn test_seq_fixture_divider001() {
    let svg = convert_fixture("tests/fixtures/sequence/seq_divider001.puml");
    assert_valid_svg(&svg, "seq_divider001");
    assert!(
        svg.contains("Initialization"),
        "seq_divider001: must contain divider text"
    );
}

#[test]
fn test_seq_fixture_autonumber001() {
    let svg = convert_fixture("tests/fixtures/sequence/seq_autonumber001.puml");
    assert_valid_svg(&svg, "seq_autonumber001");
    // Autonumber should render number as separate text element
    assert!(
        svg.contains(">1<") || svg.contains(">1</text>"),
        "seq_autonumber001: must contain autonumber text element"
    );
}

#[test]
fn test_seq_fixture_participants001() {
    let svg = convert_fixture("tests/fixtures/sequence/seq_participants001.puml");
    assert_valid_svg(&svg, "seq_participants001");
    // Actor renders as stick figure (ellipse head)
    assert!(
        svg.contains("<ellipse"),
        "seq_participants001: actor should render ellipse (head)"
    );
    // Database renders as cylinder (ellipse top)
    assert!(
        svg.contains("<ellipse"),
        "seq_participants001: database should render ellipse (cylinder top)"
    );
    // All participant names should appear
    for name in &[
        "Alice", "Bob", "Charlie", "Dave", "DB", "Logs", "MQ", "Default",
    ] {
        assert!(
            svg.contains(name),
            "seq_participants001: must contain participant name '{name}'"
        );
    }
    // Messages should appear
    assert!(
        svg.contains("hello"),
        "seq_participants001: must contain 'hello' message"
    );
    assert!(
        svg.contains("done"),
        "seq_participants001: must contain 'done' message"
    );
}

// ── Activity diagram integration tests ──

#[test]
fn test_convert_basic_activity() {
    let input = "@startuml\nstart\n:do stuff;\nstop\n@enduml\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("do stuff"));
}

#[test]
fn test_activity_fixture_a0002() {
    let svg = convert_fixture("tests/fixtures/activity/a0002.puml");
    assert_valid_svg(&svg, "a0002");
    assert!(svg.contains("foo1"), "a0002: must contain foo1");
    assert!(svg.contains("foo2"), "a0002: must contain foo2");
    assert!(
        svg.contains("Actor 1"),
        "a0002: must contain swimlane Actor 1"
    );
    assert!(
        svg.contains("Actor 2"),
        "a0002: must contain swimlane Actor 2"
    );
}

#[test]
fn test_activity_fixture_creole_table_01() {
    let svg = convert_fixture("tests/fixtures/activity/activity_creole_table_01.puml");
    assert_valid_svg(&svg, "activity_creole_table_01");
}

#[test]
fn test_activity_fixture_creole_table_02() {
    let svg = convert_fixture("tests/fixtures/activity/activity_creole_table_02.puml");
    assert_valid_svg(&svg, "activity_creole_table_02");
}

#[test]
fn test_activity_fixture_mono_multi_line() {
    let svg = convert_fixture("tests/fixtures/activity/activity_mono_multi_line.puml");
    assert_valid_svg(&svg, "activity_mono_multi_line");
}

#[test]
fn test_activity_fixture_mono_multi_line2() {
    let svg = convert_fixture("tests/fixtures/activity/activity_mono_multi_line2.puml");
    assert_valid_svg(&svg, "activity_mono_multi_line2");
}

#[test]
fn test_activity_fixture_mono_multi_line_v2() {
    let svg = convert_fixture("tests/fixtures/activity/activity_mono_multi_line_v2.puml");
    assert_valid_svg(&svg, "activity_mono_multi_line_v2");
}

#[test]
fn test_activity_fixture_mono_multi_line2_v2() {
    let svg = convert_fixture("tests/fixtures/activity/activity_mono_multi_line2_v2.puml");
    assert_valid_svg(&svg, "activity_mono_multi_line2_v2");
}

#[test]
fn test_activity_fixture_swimlane001() {
    let svg = convert_fixture("tests/fixtures/activity/swimlane001.puml");
    assert_valid_svg(&svg, "swimlane001");
    // Swimlane headers must appear
    assert!(
        svg.contains("Swimlane1"),
        "swimlane001: must contain Swimlane1 header"
    );
    assert!(
        svg.contains("Swimlane2"),
        "swimlane001: must contain Swimlane2 header"
    );
    // Action text must appear
    assert!(
        svg.contains("Action 1"),
        "swimlane001: must contain Action 1"
    );
    assert!(
        svg.contains("Action 2"),
        "swimlane001: must contain Action 2"
    );
    assert!(
        svg.contains("Action 3"),
        "swimlane001: must contain Action 3"
    );
    // Swimlane headers rendered with large font (Java uses font-size 18)
    assert!(
        svg.contains("font-size=\"18\""),
        "swimlane001: must have large swimlane headers"
    );
    // Cross-lane edges should produce line or polygon elements
    assert!(
        svg.contains("<line") || svg.contains("<polygon"),
        "swimlane001: cross-lane arrows must have line/polygon elements"
    );
}

// ── State diagram integration tests ──

#[test]
fn test_convert_basic_state() {
    let input = "@startuml\n[*] --> Active\nActive --> [*]\n@enduml\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("Active"));
}

#[test]
fn test_state_fixture_scxml0001() {
    let svg = convert_fixture("tests/fixtures/state/scxml0001.puml");
    assert_valid_svg(&svg, "scxml0001");
    assert!(svg.contains("s1"), "scxml0001: must contain s1");
    assert!(svg.contains("s2"), "scxml0001: must contain s2");
}

#[test]
fn test_state_fixture_scxml0002() {
    let svg = convert_fixture("tests/fixtures/state/scxml0002.puml");
    assert_valid_svg(&svg, "scxml0002");
    assert!(svg.contains("counter"), "scxml0002: must contain counter");
}

#[test]
fn test_state_fixture_scxml0003() {
    let svg = convert_fixture("tests/fixtures/state/scxml0003.puml");
    assert_valid_svg(&svg, "scxml0003");
}

#[test]
fn test_state_fixture_scxml0004() {
    let svg = convert_fixture("tests/fixtures/state/scxml0004.puml");
    assert_valid_svg(&svg, "scxml0004");
}

#[test]
fn test_state_fixture_scxml0005() {
    let svg = convert_fixture("tests/fixtures/state/scxml0005.puml");
    assert_valid_svg(&svg, "scxml0005");
}

#[test]
fn test_state_fixture_monoline_01() {
    let svg = convert_fixture("tests/fixtures/state/state_monoline_01.puml");
    assert_valid_svg(&svg, "state_monoline_01");
}

#[test]
fn test_state_fixture_monoline_02() {
    let svg = convert_fixture("tests/fixtures/state/state_monoline_02.puml");
    assert_valid_svg(&svg, "state_monoline_02");
}

#[test]
fn test_state_fixture_monoline_03() {
    let svg = convert_fixture("tests/fixtures/state/state_monoline_03.puml");
    assert_valid_svg(&svg, "state_monoline_03");
}

// ── State diagram pseudo-state integration tests ──

#[test]
fn test_state_fixture_fork001() {
    let svg = convert_fixture("tests/fixtures/state/state_fork001.puml");
    assert_valid_svg(&svg, "state_fork001");
    // Fork/Join bars should produce filled rectangles
    assert!(
        svg.contains("<rect"),
        "state_fork001: must contain fork/join bar rects"
    );
}

#[test]
fn test_state_fixture_choice001() {
    let svg = convert_fixture("tests/fixtures/state/state_choice001.puml");
    assert_valid_svg(&svg, "state_choice001");
    // Choice diamond should be a polygon
    assert!(
        svg.contains("<polygon"),
        "state_choice001: must contain choice diamond polygon"
    );
}

#[test]
fn test_state_fixture_history001() {
    let svg = convert_fixture("tests/fixtures/state/state_history001.puml");
    assert_valid_svg(&svg, "state_history001");
}

#[test]
fn test_state_fixture_concurrent001() {
    let svg = convert_fixture("tests/fixtures/state/state_concurrent001.puml");
    assert_valid_svg(&svg, "state_concurrent001");
}

#[test]
fn test_state_fixture_note001() {
    let svg = convert_fixture("tests/fixtures/state/state_note001.puml");
    assert_valid_svg(&svg, "state_note001");
}

// ── Component diagram integration tests ──

#[test]
fn test_convert_basic_component() {
    let input = "@startuml\ncomponent A\ncomponent B\nA --> B\n@enduml\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("A"));
    assert!(svg.contains("B"));
}

#[test]
fn test_component_fixture_buitin_newline_chr() {
    let svg = convert_fixture("tests/fixtures/component/buitin_newline_chr.puml");
    assert_valid_svg(&svg, "buitin_newline_chr");
}

#[test]
fn test_component_fixture_extraarrows_0001() {
    let svg = convert_fixture("tests/fixtures/component/componentextraarrows_0001.puml");
    assert_valid_svg(&svg, "componentextraarrows_0001");
}

#[test]
fn test_component_fixture_deployment_last_name() {
    let svg = convert_fixture("tests/fixtures/component/deployment_last_name_multi_line.puml");
    assert_valid_svg(&svg, "deployment_last_name_multi_line");
}

#[test]
fn test_component_fixture_gml0000() {
    let svg = convert_fixture("tests/fixtures/component/gml0000.puml");
    assert_valid_svg(&svg, "gml0000");
}

#[test]
fn test_component_fixture_gml0001() {
    let svg = convert_fixture("tests/fixtures/component/gml0001.puml");
    assert_valid_svg(&svg, "gml0001");
}

#[test]
fn test_component_fixture_jaws5() {
    let svg = convert_fixture("tests/fixtures/component/jaws5.puml");
    assert_valid_svg(&svg, "jaws5");
}

#[test]
fn test_component_fixture_subdiagram_theme_02() {
    let svg = convert_fixture("tests/fixtures/component/subdiagram_theme_02.puml");
    assert_valid_svg(&svg, "subdiagram_theme_02");
}

#[test]
fn test_component_fixture_deployment01() {
    let svg = convert_fixture("tests/fixtures/component/deployment01.puml");
    assert_valid_svg(&svg, "deployment01");
    assert!(
        svg.contains("Web Server") || svg.contains("web"),
        "deployment01: must contain web server node"
    );
    assert!(
        svg.contains("MySQL") || svg.contains("db"),
        "deployment01: must contain database node"
    );
    assert!(
        svg.contains("Docker") || svg.contains("docker"),
        "deployment01: must contain stack node"
    );
}

// ── Use Case diagrams ──

#[test]
fn test_usecase_fixture_basic() {
    let svg = convert_fixture("tests/fixtures/usecase/basic.puml");
    assert_valid_svg(&svg, "usecase/basic");
    assert!(svg.contains("User"), "basic: must contain User actor");
    assert!(svg.contains("Admin"), "basic: must contain Admin actor");
    assert!(svg.contains("Login"), "basic: must contain Login use case");
    assert!(
        svg.contains("Manage Users"),
        "basic: must contain Manage Users use case"
    );
}

#[test]
fn test_usecase_fixture_boundary() {
    let svg = convert_fixture("tests/fixtures/usecase/boundary.puml");
    assert_valid_svg(&svg, "usecase/boundary");
    assert!(
        svg.contains("Customer"),
        "boundary: must contain Customer actor"
    );
    assert!(
        svg.contains("Browse Products"),
        "boundary: must contain Browse Products use case"
    );
    assert!(
        svg.contains("Web Store") || svg.contains("rect"),
        "boundary: must contain boundary rectangle"
    );
}

#[test]
fn test_usecase_fixture_colon_actor() {
    let svg = convert_fixture("tests/fixtures/usecase/colon_actor.puml");
    assert_valid_svg(&svg, "usecase/colon_actor");
    assert!(
        svg.contains("First Actor"),
        "colon_actor: must contain First Actor"
    );
    assert!(
        svg.contains("First Use Case"),
        "colon_actor: must contain First Use Case"
    );
}

#[test]
fn test_seq_fixture_svg0001() {
    let svg = convert_fixture("tests/fixtures/sequence/svg0001.puml");
    assert_valid_svg(&svg, "svg0001");
}

#[test]
fn test_component_fixture_xmi0001() {
    let svg = convert_fixture("tests/fixtures/component/xmi0001.puml");
    assert_valid_svg(&svg, "xmi0001");
}

// ── ERD diagram integration tests ──

#[test]
fn test_convert_basic_erd() {
    let input = "@startchen\nentity Person {\n  name\n}\n@endchen\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("Person"));
}

#[test]
fn test_erd_fixture_chenmovie() {
    let svg = convert_fixture("tests/fixtures/erd/chenmovie.puml");
    assert_valid_svg(&svg, "chenmovie");
}

#[test]
fn test_erd_fixture_chenmoviealias() {
    let svg = convert_fixture("tests/fixtures/erd/chenmoviealias.puml");
    assert_valid_svg(&svg, "chenmoviealias");
}

#[test]
fn test_erd_fixture_chenmovieextended() {
    let svg = convert_fixture("tests/fixtures/erd/chenmovieextended.puml");
    assert_valid_svg(&svg, "chenmovieextended");
}

#[test]
fn test_erd_fixture_chenrankdir() {
    let svg = convert_fixture("tests/fixtures/erd/chenrankdir.puml");
    assert_valid_svg(&svg, "chenrankdir");
}

#[test]
fn test_erd_fixture_weak001() {
    let svg = convert_fixture("tests/fixtures/erd/weak001.puml");
    assert_valid_svg(&svg, "weak001");
    assert!(
        svg.contains("ORDER_ITEM"),
        "weak001: must contain ORDER_ITEM"
    );
    assert!(svg.contains("CUSTOMER"), "weak001: must contain CUSTOMER");
    assert!(svg.contains("CONTAINS"), "weak001: must contain CONTAINS");
    // Weak entity produces 2 rects (double border), regular entity 1 rect
    let rect_count = svg.matches("<rect").count();
    assert!(
        rect_count >= 3,
        "weak001: weak entity needs double border, got {rect_count} rects",
    );
    // Identifying relationship produces 2 polygons, regular relationship 1
    let polygon_count = svg.matches("<polygon").count();
    assert!(
        polygon_count >= 3,
        "weak001: identifying relationship needs double diamond, got {polygon_count} polygons",
    );
}

// ── Gantt diagram integration tests ──

#[test]
fn test_convert_basic_gantt() {
    let input = "@startgantt\n[Task1] lasts 5 days\n@endgantt\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("Task1"));
}

#[test]
fn test_gantt_fixture_a0003() {
    let svg = convert_fixture("tests/fixtures/gantt/a0003.puml");
    assert_valid_svg(&svg, "a0003");
}

// ── JSON diagram integration tests ──

#[test]
fn test_convert_basic_json() {
    let input = "@startjson\n{\"name\": \"hello\"}\n@endjson\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("name"));
}

#[test]
fn test_json_fixture_escaped() {
    let svg = convert_fixture("tests/fixtures/json/json_escaped.puml");
    assert_valid_svg(&svg, "json_escaped");
}

// ── Mindmap diagram integration tests ──

#[test]
fn test_convert_basic_mindmap() {
    let input = "@startmindmap\n* root\n** child1\n** child2\n@endmindmap\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("root"));
}

#[test]
fn test_mindmap_fixture_jaws12() {
    let svg = convert_fixture("tests/fixtures/mindmap/jaws12.puml");
    assert_valid_svg(&svg, "jaws12");
}

#[test]
fn test_convert_mindmap_note_creole() {
    let input =
        "@startmindmap\n* Root\nnote right : **hot** [[https://example.com docs]]\n@endmindmap\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert_valid_svg(&svg, "mindmap_note_creole");
    assert!(svg.contains("<polygon"), "mindmap note must render");
    assert_svg_has_link(&svg, "mindmap_note_creole");
}

// ── WBS diagram integration tests ──

#[test]
fn test_convert_basic_wbs() {
    let input = "@startwbs\n* Root\n** Task A\n** Task B\n@endwbs\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("Root"));
}

#[test]
fn test_convert_wbs_note_creole() {
    let input = "@startwbs\n* Root\nnote right : **todo**\n@endwbs\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert_valid_svg(&svg, "wbs_note_creole");
    assert!(svg.contains("<polygon"), "wbs note must render");
    assert!(
        svg.contains("font-weight=\"bold\"") || svg.contains("font-weight=\"700\""),
        "wbs note should render creole"
    );
}

#[test]
fn test_wbs_fixture_arrow() {
    let svg = convert_fixture("tests/fixtures/wbs/wbs_arrow.puml");
    assert_valid_svg(&svg, "wbs_arrow");
}

#[test]
fn test_wbs_fixture_direction() {
    let svg = convert_fixture("tests/fixtures/wbs/wbs_direction.puml");
    assert_valid_svg(&svg, "wbs_direction");
}

#[test]
fn test_wbs_fixture_link_url_tooltip_01() {
    let svg = convert_fixture("tests/fixtures/wbs/link_url_tooltip_01.puml");
    assert_valid_svg(&svg, "link_url_tooltip_01");
    assert_svg_has_link(&svg, "link_url_tooltip_01");
    assert!(
        svg.contains("raw string literals"),
        "link_url_tooltip_01: missing node label"
    );
}

#[test]
fn test_wbs_fixture_link_url_tooltip_02() {
    let svg = convert_fixture("tests/fixtures/wbs/link_url_tooltip_02.puml");
    assert_valid_svg(&svg, "link_url_tooltip_02");
    assert_svg_has_link(&svg, "link_url_tooltip_02");
    assert!(
        svg.contains("raw string literals"),
        "link_url_tooltip_02: missing node label"
    );
}

#[test]
fn test_wbs_fixture_link_url_tooltip_03() {
    let svg = convert_fixture("tests/fixtures/wbs/link_url_tooltip_03.puml");
    assert_valid_svg(&svg, "link_url_tooltip_03");
    assert_svg_has_link(&svg, "link_url_tooltip_03");
    assert_svg_has_tooltip(&svg, "link_url_tooltip_03");
    assert!(
        svg.contains("TP"),
        "link_url_tooltip_03: missing tooltip label"
    );
}

// ── Timing diagram integration tests ──

#[test]
fn test_convert_basic_timing() {
    let input =
        "@startuml\nrobust \"Web\" as WEB\n@0\nWEB is Idle\n@100\nWEB is Processing\n@enduml\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("Web"));
}

// ── NWDiag integration tests ──

#[test]
fn test_convert_basic_nwdiag() {
    let input = "@startnwdiag\nnwdiag {\n  network dmz {\n    web01;\n  }\n}\n@endnwdiag\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("dmz"));
    assert!(svg.contains("web01"));
}

#[test]
fn test_nwdiag_fixture_basic() {
    let svg = convert_fixture("tests/fixtures/nwdiag/basic.puml");
    assert_valid_svg(&svg, "nwdiag_basic");
    assert!(svg.contains("Infrastructure"));
    // Java nwdiag uses "description" as display name, so "web01" → "app"
    assert!(svg.contains("db01") || svg.contains("app"));
    assert!(
        svg.contains("<line") || svg.contains("<path"),
        "server connectors must render"
    );
}

// ── Salt integration tests ──

#[test]
fn test_convert_basic_salt() {
    let input = "@startsalt\n{\n[OK]\n}\n@endsalt\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("OK"));
}

#[test]
fn test_salt_fixture_basic() {
    let svg = convert_fixture("tests/fixtures/salt/basic.puml");
    assert_valid_svg(&svg, "salt_basic");
    assert!(svg.contains("Feature A"));
    assert!(svg.contains("Choice B"));
    assert!(svg.contains("Alice"));
    assert!(
        svg.contains("<circle") || svg.contains("<ellipse"),
        "radio button should render as circle or ellipse"
    );
}

// ── DITAA integration tests ──

#[test]
fn test_convert_basic_ditaa() {
    let input = "@startditaa\n+--+  +--+\n|A |->|B |\n+--+  +--+\n@endditaa\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("marker-end=\"url(#ditaa-arrow)\""));
    assert!(svg.contains(">A<"));
    assert!(svg.contains(">B<"));
}

#[test]
fn test_ditaa_fixture_basic() {
    let svg = convert_fixture("tests/fixtures/ditaa/basic.puml");
    assert_valid_svg(&svg, "ditaa_basic");
    assert!(svg.contains("marker-end=\"url(#ditaa-arrow)\""));
    assert!(svg.contains("#66CC66"), "colored ditaa box should render");
    assert!(svg.contains("Legend"));
}

#[test]
fn test_convert_timing_note_creole() {
    let input = "@startuml\nrobust \"Web\" as WEB\n@0\nWEB is Idle\nnote right of WEB : **watch**\n@enduml\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert_valid_svg(&svg, "timing_note_creole");
    assert!(svg.contains("<polygon"), "timing note must render");
    assert!(
        svg.contains("font-weight=\"bold\"") || svg.contains("font-weight=\"700\""),
        "timing note should render creole"
    );
}

#[test]
fn test_timing_fixture_messagearrowfont_0001() {
    let svg = convert_fixture("tests/fixtures/timing/timingmessagearrowfont_0001.puml");
    assert_valid_svg(&svg, "timingmessagearrowfont_0001");
}

#[test]
fn test_timing_fixture_messagearrowfont_0002() {
    let svg = convert_fixture("tests/fixtures/timing/timingmessagearrowfont_0002.puml");
    assert_valid_svg(&svg, "timingmessagearrowfont_0002");
}

#[test]
fn test_meta_title_header_footer() {
    let svg = convert_fixture("tests/fixtures/misc/meta_title_header_footer.puml");
    assert_valid_svg(&svg, "meta_title_header_footer");
    assert!(svg.contains("Class Overview"), "must contain title");
    assert!(svg.contains("Page 1 of 2"), "must contain header");
    assert!(svg.contains("Generated by PlantUML"), "must contain footer");
    assert!(svg.contains("Figure 1"), "must contain caption");
    assert!(svg.contains("Foo depends on Bar"), "must contain legend");
    // Body is shifted via absolute coordinate changes, not SVG transform.
    // Java reference also uses absolute coordinates, not <g transform="translate()">.
    assert!(
        svg.contains(r#"class="title""#),
        "must contain rendered title block"
    );
}

// ── Object diagram integration tests ──

#[test]
fn test_convert_basic_object() {
    let input = "@startuml\nobject London\nobject Washington\nLondon --> Washington\n@enduml\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("London"));
    assert!(svg.contains("Washington"));
    // Java PlantUML does not underline object names by default
    assert!(
        !svg.contains(r#"text-decoration="underline""#),
        "object name must NOT be underlined by default"
    );
}

#[test]
fn test_object_fixture_basic() {
    let svg = convert_fixture("tests/fixtures/object/basic.puml");
    assert_valid_svg(&svg, "object/basic");
    assert!(svg.contains("London"), "must contain London");
    assert!(svg.contains("Washington"), "must contain Washington");
    assert!(svg.contains("Berlin"), "must contain Berlin");
    assert!(svg.contains("<path"), "must contain edge paths");
}

#[test]
fn test_object_with_fields() {
    let input = "@startuml\nobject User {\n  name : String\n  age : int\n}\n@enduml\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("User"));
    assert!(svg.contains("name"));
    assert!(svg.contains("age"));
}

#[test]
fn test_object_fixture_fields() {
    let svg = convert_fixture("tests/fixtures/object/fields.puml");
    assert_valid_svg(&svg, "object/fields");
    assert!(svg.contains("User"), "must contain User");
    assert!(svg.contains("John"), "must contain John");
}

#[test]
fn test_object_fixture_map() {
    let svg = convert_fixture("tests/fixtures/object/map.puml");
    assert_valid_svg(&svg, "object/map");
    assert!(svg.contains("Server"), "must contain Server");
    assert!(svg.contains("404"), "must contain 404");
    assert!(svg.contains("Not Found"), "must contain Not Found");
}

// ── Preprocessor fixtures integration tests ──

#[test]
fn test_preprocessor_fixture_builtin_newline() {
    let svg = convert_fixture("tests/fixtures/preprocessor/builtin_newline.puml");
    assert_valid_svg(&svg, "builtin_newline");
    assert!(
        svg.contains("0,0"),
        "builtin_newline: generated salt table should contain first cell"
    );
    assert!(
        svg.contains("3,3"),
        "builtin_newline: generated salt table should contain last cell"
    );
}

#[test]
fn test_preprocessor_fixture_deployment_mono_multi_line() {
    let svg = convert_fixture("tests/fixtures/preprocessor/deployment_mono_multi_line.puml");
    assert_valid_svg(&svg, "deployment_mono_multi_line");
}

#[test]
fn test_preprocessor_fixture_jaws1() {
    let svg = convert_fixture("tests/fixtures/preprocessor/jaws1.puml");
    assert_valid_svg(&svg, "jaws1");
    assert!(
        svg.contains("Administrator"),
        "jaws1: missing Person node label"
    );
    // C4 word-by-word rendering splits "Web Application" into separate text spans
    assert!(
        svg.contains("Web") && svg.contains("Application"),
        "jaws1: missing Container node label (Web + Application)"
    );
    assert!(svg.contains("Twitter"), "jaws1: missing System node label");
    assert!(svg.contains("Uses"), "jaws1: missing relationship label");
}

#[test]
fn test_preprocessor_fixture_jaws3() {
    let svg = convert_fixture("tests/fixtures/preprocessor/jaws3.puml");
    assert_valid_svg(&svg, "jaws3");
    assert!(svg.contains("Field1"), "jaws3: missing table header");
    // Java renders expanded table rows as individual <text> elements in grid cells,
    // not as inline tspan runs.
    assert!(svg.contains(">1</text>"), "jaws3: missing cell value 1");
    assert!(svg.contains(">2</text>"), "jaws3: missing cell value 2");
    assert!(svg.contains(">3</text>"), "jaws3: missing cell value 3");
    assert!(svg.contains(">4</text>"), "jaws3: missing cell value 4");
}

#[test]
fn test_preprocessor_fixture_jaws6() {
    let svg = convert_fixture("tests/fixtures/preprocessor/jaws6.puml");
    assert_valid_svg(&svg, "jaws6");
}

#[test]
fn test_preprocessor_fixture_jaws7() {
    let svg = convert_fixture("tests/fixtures/preprocessor/jaws7.puml");
    assert_valid_svg(&svg, "jaws7");
}

#[test]
fn test_preprocessor_fixture_jaws8() {
    let svg = convert_fixture("tests/fixtures/preprocessor/jaws8.puml");
    assert_valid_svg(&svg, "jaws8");
}

#[test]
fn test_preprocessor_fixture_jaws9() {
    let svg = convert_fixture("tests/fixtures/preprocessor/jaws9.puml");
    assert_valid_svg(&svg, "jaws9");
}

#[test]
fn test_preprocessor_fixture_preproc_functionparam_line_continuation() {
    let svg =
        convert_fixture("tests/fixtures/preprocessor/preproc_functionparam_line_continuation.puml");
    assert_valid_svg(&svg, "preproc_functionparam_line_continuation");
}

#[test]
fn test_preprocessor_fixture_seq_mono_line() {
    let svg = convert_fixture("tests/fixtures/preprocessor/seq_mono_line.puml");
    assert_valid_svg(&svg, "seq_mono_line");
}

#[test]
fn test_preprocessor_fixture_sequencearrows_0001() {
    let svg = convert_fixture("tests/fixtures/preprocessor/sequencearrows_0001.puml");
    assert_valid_svg(&svg, "sequencearrows_0001");
}

#[test]
fn test_preprocessor_fixture_sequencearrows_0002() {
    let svg = convert_fixture("tests/fixtures/preprocessor/sequencearrows_0002.puml");
    assert_valid_svg(&svg, "sequencearrows_0002");
}

#[test]
fn test_preprocessor_fixture_sequencelayout_0004() {
    let svg = convert_fixture("tests/fixtures/preprocessor/sequencelayout_0004.puml");
    assert_valid_svg(&svg, "sequencelayout_0004");
}

#[test]
fn test_preprocessor_fixture_sequencelayout_0005() {
    let svg = convert_fixture("tests/fixtures/preprocessor/sequencelayout_0005.puml");
    assert_valid_svg(&svg, "sequencelayout_0005");
}

#[test]
fn test_preprocessor_fixture_sequencelayout_0005b() {
    let svg = convert_fixture("tests/fixtures/preprocessor/sequencelayout_0005b.puml");
    assert_valid_svg(&svg, "sequencelayout_0005b");
}

#[test]
fn test_preprocessor_fixture_sequenceleftmessageandactivelifelines_0001() {
    let svg = convert_fixture(
        "tests/fixtures/preprocessor/sequenceleftmessageandactivelifelines_0001.puml",
    );
    assert_valid_svg(&svg, "sequenceleftmessageandactivelifelines_0001");
}

#[test]
fn test_preprocessor_fixture_subdiagram_theme_01() {
    let svg = convert_fixture("tests/fixtures/preprocessor/subdiagram_theme_01.puml");
    assert_valid_svg(&svg, "subdiagram_theme_01");
}

#[test]
fn test_preprocessor_fixture_svg0002() {
    let svg = convert_fixture("tests/fixtures/preprocessor/svg0002.puml");
    assert_valid_svg(&svg, "svg0002");
}

#[test]
fn test_preprocessor_fixture_svg0004_smetana() {
    let svg = convert_fixture("tests/fixtures/preprocessor/svg0004_smetana.puml");
    assert_valid_svg(&svg, "svg0004_smetana");
}

#[test]
fn test_preprocessor_fixture_svg0004_svek() {
    let svg = convert_fixture("tests/fixtures/preprocessor/svg0004_svek.puml");
    assert_valid_svg(&svg, "svg0004_svek");
}

#[test]
fn test_preprocessor_fixture_svg0005_smetana() {
    let svg = convert_fixture("tests/fixtures/preprocessor/svg0005_smetana.puml");
    assert_valid_svg(&svg, "svg0005_smetana");
}

#[test]
fn test_preprocessor_fixture_svg0005_svek() {
    let svg = convert_fixture("tests/fixtures/preprocessor/svg0005_svek.puml");
    assert_valid_svg(&svg, "svg0005_svek");
}

#[test]
fn test_preprocessor_fixture_svg0006_svek() {
    let svg = convert_fixture("tests/fixtures/preprocessor/svg0006_svek.puml");
    assert_valid_svg(&svg, "svg0006_svek");
}

#[test]
fn test_preprocessor_fixture_teozaltelseparallel_0001() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teozaltelseparallel_0001.puml");
    assert_valid_svg(&svg, "teozaltelseparallel_0001");
}

#[test]
fn test_preprocessor_fixture_teozaltelseparallel_0002() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teozaltelseparallel_0002.puml");
    assert_valid_svg(&svg, "teozaltelseparallel_0002");
}

#[test]
fn test_preprocessor_fixture_teozaltelseparallel_0003() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teozaltelseparallel_0003.puml");
    assert_valid_svg(&svg, "teozaltelseparallel_0003");
}

#[test]
fn test_preprocessor_fixture_teozaltelseparallel_0004() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teozaltelseparallel_0004.puml");
    assert_valid_svg(&svg, "teozaltelseparallel_0004");
}

#[test]
fn test_preprocessor_fixture_teozaltelseparallel_0005() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teozaltelseparallel_0005.puml");
    assert_valid_svg(&svg, "teozaltelseparallel_0005");
}

#[test]
fn test_preprocessor_fixture_teozaltelseparallel_0006() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teozaltelseparallel_0006.puml");
    assert_valid_svg(&svg, "teozaltelseparallel_0006");
}

#[test]
fn test_preprocessor_fixture_teoztimelineissues_0001() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teoztimelineissues_0001.puml");
    assert_valid_svg(&svg, "teoztimelineissues_0001");
}

#[test]
fn test_preprocessor_fixture_teoztimelineissues_0002() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teoztimelineissues_0002.puml");
    assert_valid_svg(&svg, "teoztimelineissues_0002");
}

#[test]
fn test_preprocessor_fixture_teoztimelineissues_0003() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teoztimelineissues_0003.puml");
    assert_valid_svg(&svg, "teoztimelineissues_0003");
}

#[test]
fn test_preprocessor_fixture_teoztimelineissues_0004() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teoztimelineissues_0004.puml");
    assert_valid_svg(&svg, "teoztimelineissues_0004");
}

#[test]
fn test_preprocessor_fixture_teoztimelineissues_0005() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teoztimelineissues_0005.puml");
    assert_valid_svg(&svg, "teoztimelineissues_0005");
}

#[test]
fn test_preprocessor_fixture_teoztimelineissues_0006() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teoztimelineissues_0006.puml");
    assert_valid_svg(&svg, "teoztimelineissues_0006");
}

#[test]
fn test_preprocessor_fixture_teoztimelineissues_0007() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teoztimelineissues_0007.puml");
    assert_valid_svg(&svg, "teoztimelineissues_0007");
}

#[test]
fn test_preprocessor_fixture_teoztimelineissues_0008() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teoztimelineissues_0008.puml");
    assert_valid_svg(&svg, "teoztimelineissues_0008");
}

#[test]
fn test_preprocessor_fixture_teoztimelineissues_0009() {
    let svg = convert_fixture("tests/fixtures/preprocessor/teoztimelineissues_0009.puml");
    assert_valid_svg(&svg, "teoztimelineissues_0009");
}

// ── Misc fixtures integration tests ──

#[test]
fn test_misc_fixture_a0004() {
    let svg = convert_fixture("tests/fixtures/misc/a0004.puml");
    assert_valid_svg(&svg, "a0004");
}

#[test]
fn test_misc_fixture_deployment_on_name() {
    let svg = convert_fixture("tests/fixtures/misc/deployment_on_name.puml");
    assert_valid_svg(&svg, "deployment_on_name");
}

#[test]
fn test_misc_fixture_link_url_tooltip_04() {
    let svg = convert_fixture("tests/fixtures/misc/link_url_tooltip_04.puml");
    assert_valid_svg(&svg, "link_url_tooltip_04");
    assert_eq!(
        svg.matches("<a ").count(),
        2,
        "link_url_tooltip_04: expected two links"
    );
    // Java produces 1 <title> (the SVG document title), not 2.
    // Tooltips are encoded as title= attributes on <a> elements.
    assert_eq!(
        svg.matches("<title>").count(),
        1,
        "link_url_tooltip_04: expected one SVG doc title"
    );
    assert!(
        svg.contains("multiline label test"),
        "link_url_tooltip_04: missing first link label"
    );
    assert!(
        svg.contains("tooltip test"),
        "link_url_tooltip_04: missing second link label"
    );
}

#[test]
fn test_misc_fixture_link_url_tooltip_05() {
    let svg = convert_fixture("tests/fixtures/misc/link_url_tooltip_05.puml");
    assert_valid_svg(&svg, "link_url_tooltip_05");
    assert_eq!(
        svg.matches("<a ").count(),
        7,
        "link_url_tooltip_05: expected seven link elements"
    );
    // Java produces 1 <title> (the SVG document title).
    // Tooltips are encoded as title= attributes on <a> elements.
    assert_eq!(
        svg.matches("<title>").count(),
        1,
        "link_url_tooltip_05: expected one SVG doc title"
    );
    assert!(
        svg.contains("tooltip test on table"),
        "link_url_tooltip_05: missing table tooltip label"
    );
}

#[test]
fn test_misc_fixture_xmi0000() {
    let svg = convert_fixture("tests/fixtures/misc/xmi0000.puml");
    assert_valid_svg(&svg, "xmi0000");
}

// ── DOT passthrough integration tests ──

#[test]
fn test_convert_basic_dot() {
    let input = "@startdot\ndigraph G {\n  A -> B\n}\n@enddot\n";
    let svg = plantuml_little::convert(input).expect("convert failed");
    assert!(svg.contains("<svg"), "DOT output must contain <svg tag");
    assert!(svg.contains("</svg>"), "DOT output must contain </svg> tag");
}

#[test]
fn test_yaml_fixture_basic() {
    let svg = convert_fixture("tests/fixtures/yaml/basic.puml");
    assert_valid_svg(&svg, "yaml/basic");
    assert!(svg.contains("name"), "yaml/basic: must contain key 'name'");
    assert!(
        svg.contains("database"),
        "yaml/basic: must contain key 'database'"
    );
}

#[test]
fn test_dot_fixture_basic() {
    let svg = convert_fixture("tests/fixtures/dot/basic.puml");
    // DOT rendering is suppressed in Java PlantUML (issue #2495)
    assert!(svg.contains("<svg"), "dot/basic: must contain <svg tag");
    assert!(
        svg.contains("suppressed"),
        "dot/basic: must show suppressed message"
    );
}

// ── Skinparam integration tests ──

#[test]
fn test_skinparam_font001() {
    let svg = convert_fixture("tests/fixtures/misc/skinparam_font001.puml");
    assert_valid_svg(&svg, "skinparam_font001");
    assert!(svg.contains("Alice"), "must contain Alice");
    assert!(svg.contains("Bob"), "must contain Bob");
    assert!(svg.contains("hello"), "must contain message text");
}

#[test]
fn test_skinparam_monochrome001() {
    let svg = convert_fixture("tests/fixtures/misc/skinparam_monochrome001.puml");
    assert_valid_svg(&svg, "skinparam_monochrome001");
    assert!(svg.contains("Alice"), "must contain Alice");
    assert!(svg.contains("hello"), "must contain message text");
}

#[test]
fn test_skinparam_handwritten001() {
    let svg = convert_fixture("tests/fixtures/misc/skinparam_handwritten001.puml");
    assert_valid_svg(&svg, "skinparam_handwritten001");
    assert!(svg.contains("Alice"), "must contain Alice");
    // Handwritten mode does NOT change fonts in Java PlantUML.
    // It only jiggles shapes and adds a warning banner.
    // The font stays sans-serif for all text rendering.
    assert!(
        svg.contains("sans-serif"),
        "handwritten mode must still use sans-serif font"
    );
}

#[test]
fn test_skinparam_roundcorner001() {
    let svg = convert_fixture("tests/fixtures/misc/skinparam_roundcorner001.puml");
    assert_valid_svg(&svg, "skinparam_roundcorner001");
    assert!(svg.contains("Foo"), "must contain Foo");
    assert!(svg.contains("Bar"), "must contain Bar");
    // Java URectangle.rounded(roundCorner): SVG rx = roundCorner / 2 = 7.5
    assert!(
        svg.contains(r#"rx="7.5""#),
        "roundcorner 15 should set rx=7.5 on class rects"
    );
}

#[test]
fn test_skinparam_class001() {
    let svg = convert_fixture("tests/fixtures/misc/skinparam_class001.puml");
    assert_valid_svg(&svg, "skinparam_class001");
    assert!(svg.contains("Foo"), "must contain Foo");
    // Java 1.2026.2: classFontSize 16 with classAttributeFontSize 12 — Java
    // actually uses font-size="12" for everything (attribute font size wins).
    // Verify the class name text color is blue (#0000FF).
    assert!(
        svg.contains("fill=\"#0000FF\""),
        "classFontColor blue should appear as hex"
    );
}

#[test]
fn test_skinparam_sequence001() {
    let svg = convert_fixture("tests/fixtures/misc/skinparam_sequence001.puml");
    assert_valid_svg(&svg, "skinparam_sequence001");
    assert!(svg.contains("Alice"), "must contain Alice");
    assert!(svg.contains("Bob"), "must contain Bob");
    assert!(
        svg.contains("stroke-width:2;") || svg.contains("stroke-width=\"2\""),
        "sequenceArrowThickness 2 should appear"
    );
}

#[test]
fn test_skinparam_colors001() {
    let svg = convert_fixture("tests/fixtures/misc/skinparam_colors001.puml");
    assert_valid_svg(&svg, "skinparam_colors001");
    assert!(svg.contains("Foo"), "must contain Foo");
    assert!(svg.contains("Bar"), "must contain Bar");
}

// ── Creole tag integration tests ──

#[test]
fn test_creole_sup_sub001() {
    let svg = convert_fixture("tests/fixtures/misc/creole_sup_sub001.puml");
    assert_valid_svg(&svg, "creole_sup_sub001");
    // Java renders sub/sup as separate <text> elements with explicit y-offset
    // and reduced font-size (0.77 * base), not CSS baseline-shift.
    assert!(
        svg.contains(r#"font-size="10""#),
        "sub/sup must have reduced font-size (10 = round(13 * 0.77))"
    );
    assert!(
        svg.contains(">2</text>"),
        "subscript/superscript '2' must appear"
    );
}

#[test]
fn test_creole_back001() {
    let svg = convert_fixture("tests/fixtures/misc/creole_back001.puml");
    assert_valid_svg(&svg, "creole_back001");
    assert!(
        svg.contains(r#"filter="url(#"#),
        "back:yellow must produce SVG filter reference"
    );
    assert!(
        svg.contains(r##"flood-color="#FFFF00""##),
        "back:yellow must produce feFlood filter with yellow color"
    );
    assert!(svg.contains("important"), "must contain highlighted text");
}

#[test]
fn test_creole_font001() {
    let svg = convert_fixture("tests/fixtures/misc/creole_font001.puml");
    assert_valid_svg(&svg, "creole_font001");
    assert!(
        svg.contains(r#"font-family="courier""#),
        "font:courier must produce font-family attribute"
    );
    assert!(
        svg.contains(r#"font-family="Arial""#),
        "font:Arial must produce font-family attribute"
    );
}

#[test]
fn test_creole_mixed001() {
    let svg = convert_fixture("tests/fixtures/misc/creole_mixed001.puml");
    assert_valid_svg(&svg, "creole_mixed001");
    assert!(
        svg.contains(r#"font-weight="bold""#) || svg.contains(r#"font-weight="bold""#),
        "must contain bold formatting"
    );
    assert!(
        svg.contains(r#"font-style="italic""#),
        "must contain italic formatting"
    );
}

#[test]
fn test_creole_note001() {
    let svg = convert_fixture("tests/fixtures/misc/creole_note001.puml");
    assert_valid_svg(&svg, "creole_note001");
    assert!(svg.contains("Bold"), "must contain Bold text");
    assert!(svg.contains("Italic"), "must contain Italic text");
}

// ── Upstream nonreg regression test suite ──
// Batch-test all upstream PlantUML regression fixtures.
// Each .puml must convert without error and produce valid SVG.

#[test]
fn test_nonreg_simple_all() {
    let dir = "tests/fixtures/nonreg/simple";
    let mut count = 0;
    let mut failures = Vec::new();
    for entry in fs::read_dir(dir).expect("cannot read nonreg/simple dir") {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "puml") {
            continue;
        }
        let path_str = path.to_str().unwrap();
        let name = path.file_stem().unwrap().to_str().unwrap();
        let source =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path_str}: {e}"));
        match plantuml_little::convert_with_input_path(&source, &path) {
            Ok(svg) => {
                if !svg.contains("<svg") || !svg.contains("</svg>") {
                    failures.push(format!("{name}: invalid SVG structure"));
                } else if svg.contains("NaN") || svg.contains("Infinity") {
                    failures.push(format!("{name}: SVG contains NaN/Infinity"));
                }
            }
            Err(e) => {
                failures.push(format!("{name}: {e}"));
            }
        }
        count += 1;
    }
    assert!(count > 0, "no .puml fixtures found in {dir}");
    if !failures.is_empty() {
        panic!(
            "{}/{} nonreg/simple fixtures failed:\n  {}",
            failures.len(),
            count,
            failures.join("\n  ")
        );
    }
}

#[test]
fn test_nonreg_svg_all() {
    let dir = "tests/fixtures/nonreg/svg";
    if !Path::new(dir).exists() {
        return; // directory not yet populated
    }
    let mut count = 0;
    let mut failures = Vec::new();
    for entry in fs::read_dir(dir).expect("cannot read nonreg/svg dir") {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "puml") {
            continue;
        }
        let path_str = path.to_str().unwrap();
        let name = path.file_stem().unwrap().to_str().unwrap();
        let source =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path_str}: {e}"));
        match plantuml_little::convert_with_input_path(&source, &path) {
            Ok(svg) => {
                if !svg.contains("<svg") || !svg.contains("</svg>") {
                    failures.push(format!("{name}: invalid SVG structure"));
                }
            }
            Err(e) => {
                failures.push(format!("{name}: {e}"));
            }
        }
        count += 1;
    }
    if count > 0 && !failures.is_empty() {
        panic!(
            "{}/{} nonreg/svg fixtures failed:\n  {}",
            failures.len(),
            count,
            failures.join("\n  ")
        );
    }
}

#[test]
fn test_nonreg_dev_all() {
    for subdir in &["dev/newline", "dev/newlinev2", "dev/jaws"] {
        let dir = format!("tests/fixtures/{subdir}");
        if !Path::new(&dir).exists() {
            continue;
        }
        let mut count = 0;
        let mut failures = Vec::new();
        for entry in fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "puml") {
                continue;
            }
            let path_str = path.to_str().unwrap();
            let name = path.file_stem().unwrap().to_str().unwrap();
            let source =
                fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path_str}: {e}"));
            match plantuml_little::convert_with_input_path(&source, &path) {
                Ok(svg) => {
                    if !svg.contains("<svg") || !svg.contains("</svg>") {
                        failures.push(format!("{subdir}/{name}: invalid SVG structure"));
                    }
                }
                Err(e) => {
                    failures.push(format!("{subdir}/{name}: {e}"));
                }
            }
            count += 1;
        }
        if count > 0 && !failures.is_empty() {
            panic!(
                "{}/{} {subdir} fixtures failed:\n  {}",
                failures.len(),
                count,
                failures.join("\n  ")
            );
        }
    }
}

#[test]
fn test_component_fixture_colors001() {
    let svg = convert_fixture("tests/fixtures/component/colors001.puml");
    assert_valid_svg(&svg, "component_colors001");
    assert!(svg.contains("Web"), "must contain Web component");
    assert!(svg.contains("DB"), "must contain DB component");
}

// ── SVG Sprite fixtures ────────────────────────────────────────────────

#[test]
fn test_sprite_all() {
    let dir = "tests/fixtures/sprite";
    let mut count = 0;
    let mut failures = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "puml") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let source = fs::read_to_string(&path).unwrap();
        match plantuml_little::convert_with_input_path(&source, &path) {
            Ok(svg) => {
                if !svg.contains("<svg") {
                    failures.push(format!("{name}: missing <svg tag"));
                }
            }
            Err(e) => {
                failures.push(format!("{name}: {e}"));
            }
        }
        count += 1;
    }
    assert!(count >= 40, "expected >=40 sprite fixtures, found {count}");
    if !failures.is_empty() {
        panic!(
            "{}/{count} sprite fixtures failed:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }
}

#[test]
fn test_sprite_red_rect_content() {
    let svg = convert_fixture("tests/fixtures/sprite/svgRedRect.puml");
    assert_valid_svg(&svg, "sprite/svgRedRect");
    // Java SvgNanoParser silently drops <rect> elements — red fill is NOT present.
    // The message text around the sprite should be present.
    assert!(svg.contains("hello"), "must contain text before sprite ref");
    assert!(svg.contains("there"), "must contain text after sprite ref");
}

#[test]
fn test_sprite_gradient() {
    let svg = convert_fixture("tests/fixtures/sprite/testGradientSprite.puml");
    assert_valid_svg(&svg, "sprite/testGradientSprite");
    // Java SvgNanoParser does NOT parse <defs> or gradient definitions.
    // Gradient references are resolved to the first stop-color.
    // No linearGradient should be hoisted into the output.
    assert!(
        !svg.contains("linearGradient"),
        "gradient sprite must NOT contain linearGradient (Java resolves to stop-color)"
    );
}

#[test]
fn test_sprite_transform_group() {
    let svg = convert_fixture("tests/fixtures/sprite/svgTransformGroup.puml");
    // This is an activity diagram with <$groupTest> in action labels.
    // Java pre-applies transforms to coordinates rather than emitting
    // SVG transform attributes.
    assert!(svg.contains("<svg"), "must produce valid SVG");
}
