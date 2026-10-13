// cjk-allow-file: these are diagram sources under test, not prose. The bug
// being pinned is a byte index landing inside a multi-byte character, so the
// inputs have to contain real ones; an escape would still be the same bytes
// but would hide which character sits where, which is the whole point of each
// case.
//! A non-ASCII identifier must never panic the converter.
//!
//! Scanners walked a `&str` by byte offset and then sliced it as a `str` —
//! `line[..prefix.len()]`, `line[pos..pos + 2]`, `pos += 1`, an offset
//! measured on `to_lowercase()` and applied to the original. Each panics the
//! moment the offset lands inside a multi-byte character, and every line of
//! every diagram reaches at least one of them.
//!
//! One case per fixed site, named after it. Found by mutating the fixture
//! corpus with multi-byte characters at random positions.

use std::sync::OnceLock;

mod support;

fn convert(source: &str) -> plantuml_little::Result<String> {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        support::init_test_backend();
    });
    plantuml_little::convert(source)
}

/// Renders, or returns a normal error — anything but a panic.
fn assert_no_panic(source: &str, case: &str) {
    if let Ok(svg) = convert(source) {
        assert!(svg.contains("<svg"), "{case}: no <svg> in output");
    }
}

/// Renders successfully — for inputs that are valid diagrams.
fn assert_renders(source: &str, case: &str) {
    let svg = convert(source).unwrap_or_else(|e| panic!("{case}: expected a render, got {e}"));
    assert!(svg.contains("<svg"), "{case}: no <svg> in output");
}

#[test]
fn preproc_strip_directive_prefix() {
    // preproc/mod.rs — the reported case. `!define ` is 8 bytes and the
    // second character of the name starts at byte 6.
    for decl in ["actor 用户", "participant 用户", "class 类名", "state 状态"] {
        assert_renders(&format!("@startuml\n{decl}\n@enduml\n"), decl);
    }
    // The offending index depends on how far into the line the character
    // sits, so walk it past every prefix length in use.
    for pad in 0..12 {
        let line = format!("{}用户 -> B : hi", "a".repeat(pad));
        assert_renders(&format!("@startuml\n{line}\n@enduml\n"), &line);
    }
}

#[test]
fn sequence_strip_participant_keyword() {
    // parser/sequence.rs — `create`/`destroy` hand it a bare name and the
    // 5-byte `actor` cuts through it.
    for n in 1..=4 {
        let name = "用".repeat(n);
        assert_renders(
            &format!("@startuml\nA -> B : x\ndestroy {name}\n@enduml\n"),
            &format!("destroy {name}"),
        );
        assert_renders(
            &format!("@startuml\ncreate {name}\nA -> {name} : x\n@enduml\n"),
            &format!("create {name}"),
        );
    }
}

#[test]
fn graphviz_creole_width_steps_whole_characters() {
    // layout/graphviz.rs `measure_creole_line_width` — stepped one byte at a
    // time while peeking two bytes ahead as a `str`. Reached through a class
    // diagram's edge label, which graphviz measures for creole markup.
    for label in [
        "this iés\\non several\\nlines",
        "**中文**",
        "//中文//",
        "中文**粗体**中文",
        "<size:14>中文</size>",
        "中<size:14>文</size>",
    ] {
        assert_renders(
            &format!("@startuml\nclass cl1\nclass cl2\ncl1 -- cl2 : {label}\n@enduml\n"),
            label,
        );
    }
}

#[test]
fn activity_creole_prefix_is_matched_in_bytes() {
    // layout/activity.rs `starts_with_ci`, reached from `wrap_note_text`.
    // A note only wraps when a style sets `MaximumWidth`, and the scan runs
    // on each flushed line looking for an unclosed creole tag.
    let words = ["中文标签"; 8].join(" ");
    for body in [words.clone(), format!("<b>{words}"), format!("a {words}")] {
        let src = format!(
            "@startuml\n<style>\nactivityDiagram {{\n  note {{\n    MaximumWidth 100\n  }}\n}}\n\
             </style>\nstart\n:step;\nnote right\n  {body}\nend note\nstop\n@enduml\n"
        );
        assert_renders(&src, "wrapped note");
    }
}

#[test]
fn component_arrow_scan_is_matched_in_bytes() {
    // parser/component.rs `find_arrow_start_forward` — walks back a byte at a
    // time looking for `up` / `down` / `left` / `right`.
    // Reached through a port-to-port arrow whose middle is non-ASCII.
    assert_renders(
        "@startuml\ncomponent c1 {\n  portout p1\n}\ncomponent c2 {\n  portin p2\n}\n\
         p1 -한글-> p2\n@enduml\n",
        "arrow with a non-ASCII direction word",
    );
    for line in [
        "[组件] --> [其他]",
        "[组件] -down-> [其他]",
        "组件 --> 其他",
    ] {
        assert_renders(&format!("@startuml\n{line}\n@enduml\n"), line);
    }
}

#[test]
fn sprite_element_scanners_step_whole_characters() {
    // render/svg_sprite.rs — two scanners advanced one byte past a character
    // they did not recognise, and `parse_element` compared `&s[gt - 1..gt + 1]`.
    let sprite = concat!(
        "@startuml\n",
        "sprite $foo <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\">",
        "<circle r=\"5\" é></circle></svg>\n",
        "Alice -> Bob : <$foo>\n",
        "@enduml\n"
    );
    assert_no_panic(sprite, "sprite with a non-ASCII attribute");
}

#[test]
fn preproc_word_boundary_steps_whole_characters() {
    // preproc/expr.rs — `replace_word_boundary` and its sibling
    // `find_whole_word` both advanced by `abs_pos + 1` past a needle that can
    // begin with a multi-byte character.
    assert_renders(
        "@startuml\n!define Ä Alice\naÄ -> Bob : hi\n@enduml\n",
        "non-ASCII macro name",
    );
    assert_renders(
        "@startuml\n!define Ä(x) x\naÄ(1) -> Bob : hi\n@enduml\n",
        "non-ASCII parameterised macro name",
    );
}

#[test]
fn builtin_colour_helpers_require_hex_digits() {
    // preproc/builtins.rs `parse_color_hex` — `len() == 6` counted bytes.
    assert_no_panic(
        "@startuml\n!$c = %darken(\"aébcd\", 20)\nAlice -> Bob : hi\n@enduml\n",
        "darken with a non-ASCII colour",
    );
}

#[test]
fn sequence_fill_colour_is_measured_in_bytes() {
    // render/svg_sequence.rs `resolve_fill_attrs` — `len() == 9` counted bytes.
    assert_renders(
        "@startuml\nparticipant Alice #ABCDEéZ\nAlice -> Bob : hi\n@enduml\n",
        "9-byte participant colour",
    );
}

#[test]
fn component_fill_colour_is_measured_in_bytes() {
    // render/svg_component.rs `parse_hex_color`.
    assert_renders(
        "@startuml\nskinparam componentBackgroundColor #aébcd\n[A] --> [B]\n@enduml\n",
        "non-ASCII skinparam colour",
    );
}

#[test]
fn ditaa_colour_code_stays_on_a_boundary() {
    // layout/ditaa.rs `remove_color_codes` — `&trimmed[1..4]` behind a
    // `len() >= 4` byte check.
    assert_no_panic(
        "@startuml\n@startditaa\n+--------+\n| cé€xxx |\n+--------+\n@endditaa\n@enduml\n",
        "ditaa colour code",
    );
}

#[test]
fn state_note_direction_folds_ascii_only() {
    // parser/state.rs — the prefix length was measured on `to_lowercase()`,
    // which can change a string's byte length, and then indexed the original.
    assert_renders(
        "@startuml\nstate A\nnote right of ẞ : hello\n@enduml\n",
        "note target that folds to two bytes",
    );
}

#[test]
fn sequence_fragment_keyword_folds_ascii_only() {
    // parser/sequence.rs — same idiom, with U+212A KELVIN SIGN folding to a
    // one-byte `k`.
    // Written as an escape so it cannot be mistaken for, or silently
    // normalised into, an ASCII `K`: U+212A folds to a one-byte `k` under
    // `to_lowercase`, which is what shifted every later offset.
    assert_no_panic(
        "@startuml\nAlice -> Bob : hi\nbrea\u{212A} oops\nend\n@enduml\n",
        "fragment keyword with a Kelvin sign",
    );
}

#[test]
fn an_ebnf_comment_needs_both_delimiters() {
    // parser/ebnf.rs — `(*)` satisfies both ends at once and gave a reversed
    // range. Pure ASCII, no multi-byte needed.
    assert_no_panic("@startebnf\n(*)\n@endebnf\n", "(*) alone");
    assert_no_panic("@startebnf\n(**)\n@endebnf\n", "(**) alone");
}
