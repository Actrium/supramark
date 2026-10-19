// cjk-allow-file: the inputs below are diagram sources under test, not prose.
// The defect being pinned is a byte offset landing inside a multi-byte
// character, so the fixtures have to contain real ones — escaping them would
// keep the same bytes but hide which character sits where, which is the point.
//! A glob pattern in a substituted value must never panic the parser.
//!
//! Same class of bug as the plantuml/mermaid sweep fixed in #248: a byte
//! offset was measured on one string and applied to another.
//! `parse_unquoted_string` tracks `last_pattern_index` as an offset into `sb`,
//! and the substitution branch clears `sb` without resetting that offset — so
//! the next segment (or the final one) cuts the refilled buffer at a stale
//! index. One test per site, plus one pinning the substituted form to its
//! literal equivalent.

use d2_little::{CompileOptions, compile};

/// Renders the source, or panics with the compile error.
fn render(source: &str) -> String {
    match compile(source, &CompileOptions::default()) {
        Ok((_, svg)) => String::from_utf8(svg).expect("svg is valid utf-8"),
        Err(e) => panic!("expected a render, got: {e}"),
    }
}

#[test]
fn parser_pattern_offset_after_substitution_is_ascii_reachable() {
    // parser.rs:1109 — `pat.push(sb[last_pattern_index..])`, applied to the
    // buffer refilled after `${v}` cleared it. The stale offset outruns the
    // new buffer, so this needs no multi-byte character at all.
    let svg = render("vars: {\n  v: hello\n}\nx: a*${v}b*c\n");
    assert!(svg.contains("a*hellob*c"), "label lost fragments:\n{svg}");
}

#[test]
fn parser_pattern_offset_after_substitution_multibyte() {
    // parser.rs:1188 — the final `sv[last_pattern_index..]`, same stale
    // offset, now landing inside a multi-byte character.
    let svg = render("vars: {\n  v: hello\n}\nx: a*${v}日本\n");
    assert!(svg.contains("a*hello日本"), "label lost fragments:\n{svg}");
}

#[test]
fn substituted_pattern_matches_its_literal_equivalent() {
    // `${v}` is plain text substitution: the diagram rendered from the
    // substituted source must be byte-identical to the one rendered from the
    // same source with the value written out literally.
    assert_eq!(
        render("vars: {\n  v: hello\n}\nx: a*${v}b*c\n"),
        render("x: a*hellob*c\n"),
    );
    assert_eq!(
        render("vars: {\n  v: hello\n}\nx: a*${v}日本\n"),
        render("x: a*hello日本\n"),
    );
}

#[test]
fn block_string_keeps_multibyte_text_when_vars_exist() {
    // ir.rs `replace_substitutions_line` rebuilt every line from raw bytes
    // (`bytes[i] as char`), so any multi-byte character turned into mojibake
    // as soon as a `vars` block made substitution scanning run.
    let svg = render("vars: {\n  v: hello\n}\nx: |md\n  日本語テスト ${v}\n|\n");
    assert!(
        svg.contains("日本語テスト"),
        "block string was mangled:\n{svg}"
    );
    assert!(svg.contains("hello"), "substitution did not run:\n{svg}");
}
