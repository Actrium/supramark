// cjk-allow-file: these are diagram sources under test, not prose. The bug
// being pinned is a byte index landing inside a multi-byte character, so the
// inputs have to contain real ones; an escape would still be the same bytes
// but would hide which character sits where, which is the whole point of each
// case.
//! A non-ASCII identifier must never panic the converter.
//!
//! Scanners walked a `&str` by byte offset and then sliced it as a `str` —
//! `s[..kw.len()]`, `&s[i..]` in an `i += 1` loop, `&s[..10]` behind a
//! `len()` check. Each panics the moment the offset lands inside a multi-byte
//! character, and every line of every diagram reaches at least one of them.
//!
//! One case per fixed site, named after it. Found by mutating the fixture
//! corpus with multi-byte characters at random positions.

/// Renders, or returns a normal error — anything but a panic.
fn assert_no_panic(source: &str, case: &str) {
    if let Ok(svg) = mermaid_little::convert(source) {
        assert!(svg.contains("<svg"), "{case}: no <svg> in output");
    }
}

/// Renders successfully — for inputs that are valid diagrams.
fn assert_renders(source: &str, case: &str) {
    let svg = mermaid_little::convert(source)
        .unwrap_or_else(|e| panic!("{case}: expected a render, got {e}"));
    assert!(svg.contains("<svg"), "{case}: no <svg> in output");
}

#[test]
fn requirement_strip_prefix_ci() {
    // parser/requirement.rs — the 8-byte `acctitle` cuts through the third
    // character of a three-character line.
    for n in 1..=5 {
        let name = "需".repeat(n);
        assert_no_panic(&format!("requirementDiagram\n{name}\n"), &name);
    }
}

#[test]
fn requirement_try_req_kind() {
    // parser/requirement.rs — same shape, keywords `requirement`, `element`…
    for n in 1..=8 {
        let name = "需".repeat(n);
        assert_renders(
            &format!("requirementDiagram\nelement {name} {{\ntype: simple\n}}\n"),
            &format!("element {name}"),
        );
    }
}

#[test]
fn sequence_strip_kw_ci() {
    // parser/sequence.rs
    for n in 1..=5 {
        let name = "用".repeat(n);
        assert_renders(
            &format!("sequenceDiagram\nparticipant {name}\n{name}->>B: hi\n"),
            &format!("participant {name}"),
        );
    }
}

#[test]
fn c4_parser_skips_a_whole_character() {
    // parser/c4.rs — the `advance(1)` for an unrecognised byte left the
    // cursor mid-character, and `read_ident` then sliced from there.
    assert_no_panic("C4Context\n用户", "bare line in C4");
    assert_no_panic("C4Context\ntitle T\n用户\n", "bare line after a title");
}

#[test]
fn journey_frontmatter_scan_steps_whole_characters() {
    // parser/journey.rs `find_line_start` — `&s[i..]` in an `i += 1` loop.
    assert_renders(
        "---\ntitle: 한\n---\njourney\n  section S\n    A: 5: Me\n",
        "journey frontmatter",
    );
}

#[test]
fn shape_hex_colour_is_measured_in_bytes() {
    // render/shapes/types.rs — `len() == 3` counted bytes, so a two-character
    // value reached the three-digit branch.
    assert_renders(
        "flowchart TD\n  A\n  style A color:#한한\n",
        "3-byte colour",
    );
    assert_renders(
        "flowchart TD\n  A\n  style A fill:#가나다\n",
        "hangul colour",
    );
}

#[test]
fn br_tag_body_is_matched_in_bytes() {
    // layout/label_metrics.rs `is_br_tag_body` — `tag[..2]` behind `len() >= 2`.
    assert_renders("flowchart TD\n  A[a<bé>c]\n", "tag body with an accent");
}

#[test]
fn gantt_date_slice_stays_on_a_boundary() {
    // layout/gantt.rs — `&s[..10]` behind `len() < 10`.
    assert_renders(
        "gantt\n  dateFormat YYYY-MM-DD HH:mm\n  section S\n  task :a1, 2024-01-0é 10:00, 1d\n",
        "date with an accent",
    );
}

#[test]
fn a_label_ending_in_fa_is_not_read_past_its_end() {
    // layout/flowchart.rs — with no colon, `prefix_end` is the length and
    // `prefix_end + 1` ran past the end. Pure ASCII, no multi-byte needed.
    for label in ["sofa", "fa", "alfa", "fab", "far"] {
        assert_renders(&format!("flowchart TD\n  A[{label}]\n"), label);
    }
}
