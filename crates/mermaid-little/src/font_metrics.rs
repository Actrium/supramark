//! Font metrics computed from pre-extracted static DejaVu font data.
//!
//! Uses [`crate::font_data`] lookup tables instead of runtime TTF parsing.
//!
//! Vendored from the sister project
//! [plantuml-little](https://github.com/kookyleo/plantuml-little) at commit
//! `b32d6aa`, under its MIT-compatible multi-license. Mermaid has no runtime
//! Java dependency, but uses the same DejaVu TTF files, so the same glyph
//! advance math yields byte-exact geometry on this side of the pipeline too.

use crate::font_data::{FontMeta, DEJAVU_MONO, DEJAVU_MONO_BOLD, DEJAVU_SANS, DEJAVU_SANS_BOLD};

// ── Font family resolution ──────────────────────────────────────────────

/// Map a logical font family name to a canonical key.
/// Java logical fonts: "SansSerif"/"Dialog"→ DejaVu Sans, "Monospaced"/"Courier"→ DejaVu Sans Mono.
/// Physical fonts not installed on the reference machine (e.g. "Courier New", "Arial")
/// fall back to Dialog (sans-serif) in Java AWT.
/// For CSS `font-family` lists like "Courier New,monospace", we resolve based on
/// the PRIMARY (first) name — Java AWT uses the first name for font lookup.
fn resolve_face(family: &str, bold: bool) -> &'static FontMeta {
    // Use the first name in a CSS comma-separated font-family list
    let primary = family.split(',').next().unwrap_or(family).trim();
    let p = primary.to_lowercase();
    // Java logical font "Monospaced" and its alias "Courier" (without "New") map to mono.
    // CSS generic "monospace" also maps to mono.
    // "Courier New" is a physical font — uninstalled on reference machine → Dialog fallback.
    let is_mono = p == "monospaced" || p == "monospace" || p == "courier";
    if is_mono {
        if bold {
            &DEJAVU_MONO_BOLD
        } else {
            &DEJAVU_MONO
        }
    } else if bold {
        &DEJAVU_SANS_BOLD
    } else {
        &DEJAVU_SANS
    }
}

// ── Public API (signatures preserved from previous implementation) ───────

/// Width of a single character in the given font configuration.
///
/// Computes `glyph_hor_advance / units_per_em * size`, matching Java's
/// `font.getStringBounds(ch, frc).getWidth()` with `FRACTIONALMETRICS_ON`.
pub fn char_width(ch: char, family: &str, size: f64, bold: bool, _italic: bool) -> f64 {
    if ch == '\n' || ch == '\r' {
        return 0.0;
    }
    let face = resolve_face(family, bold);
    let upem = face.units_per_em as f64;
    if let Some(adv) = face.glyph_advance(ch as u32) {
        return adv as f64 / upem * size;
    }
    // Fallback: use space advance for unmapped characters
    if let Some(sp_adv) = face.glyph_advance(' ' as u32) {
        return sp_adv as f64 / upem * size;
    }
    size * 0.6 // last-resort fallback
}

/// Total width of a text string (sum of character advances).
///
/// Robustness shim: certain upstream label-builders cast raw UTF-8 bytes to
/// `char` (`b as char`), which mis-decodes multi-byte CJK / emoji sequences
/// into runs of Latin-1 code points (U+0080..U+00FF). DejaVu has glyphs for
/// most of those Latin-1 supplements, so the inflated widths leak straight
/// into `<foreignObject>` sizing. Detect that pattern (every non-ASCII char
/// is in U+0080..U+00FF AND those bytes form valid UTF-8) and measure the
/// recovered string instead. Strings that are genuinely Latin-1 with stray
/// accented letters do not round-trip as valid UTF-8, so they are
/// unaffected.
pub fn text_width(text: &str, family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    if let Some(recovered) = recover_mangled_utf8(text) {
        return recovered
            .chars()
            .map(|c| char_width(c, family, size, bold, italic))
            .sum();
    }
    text.chars()
        .map(|c| char_width(c, family, size, bold, italic))
        .sum()
}

/// Detect strings that look like UTF-8 bytes mis-cast to `char` and recover.
///
/// Returns `Some(decoded)` only when every non-ASCII char is in U+0080..U+00FF
/// AND treating them as raw bytes yields a valid UTF-8 sequence. This is
/// strict enough that a genuine Latin-1 string with a stray accented char is
/// left alone, because a lone `0xE9` byte is not the start of any valid UTF-8
/// multi-byte sequence.
fn recover_mangled_utf8(text: &str) -> Option<String> {
    let mut has_high = false;
    for ch in text.chars() {
        let cp = ch as u32;
        if cp > 0xFF {
            return None; // genuine non-Latin-1 char — string isn't mangled
        }
        if cp > 0x7F {
            has_high = true;
        }
    }
    if !has_high {
        return None;
    }
    let bytes: Vec<u8> = text.chars().map(|c| c as u8).collect();
    std::str::from_utf8(&bytes).ok().map(|s| s.to_string())
}

/// Line height = ascent + |descent| (leading is 0 for DejaVu fonts).
///
/// Matches Java's `LineMetrics.getHeight()`.
pub fn line_height(family: &str, size: f64, _bold: bool, _italic: bool) -> f64 {
    let face = resolve_face(family, false); // vertical metrics are style-independent
    let upem = face.units_per_em as f64;
    let asc = face.ascender as f64; // positive (hhea.ascender)
    let desc = face.descender.unsigned_abs() as f64; // make positive
    (asc + desc) / upem * size
}

/// Font ascent (baseline to top of tallest glyph).
///
/// Matches Java's `LineMetrics.getAscent()`.
pub fn ascent(family: &str, size: f64, _bold: bool, _italic: bool) -> f64 {
    let face = resolve_face(family, false);
    face.ascender as f64 / face.units_per_em as f64 * size
}

/// Font descent (baseline to bottom of lowest glyph).
///
/// Matches Java's `LineMetrics.getDescent()` (positive value).
pub fn descent(family: &str, size: f64, _bold: bool, _italic: bool) -> f64 {
    let face = resolve_face(family, false);
    face.descender.unsigned_abs() as f64 / face.units_per_em as f64 * size
}

/// OS/2 typographic ascent. Used for DOT cluster label dimensions which match
/// Java's `StringBounder.calculateDimension()` text block height.
pub fn typo_ascent(family: &str, size: f64, _bold: bool, _italic: bool) -> f64 {
    let face = resolve_face(family, false);
    let upem = face.units_per_em as f64;
    let typo_asc = face.typo_ascender as f64;
    typo_asc / upem * size
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Java ground truth (FRACTIONALMETRICS_ON, DejaVu Sans):
    // SansSerif 12 PLAIN: ascent=11.1386718750 descent=2.8300781250 height=13.9687500000
    // SansSerif 13 PLAIN: ascent=12.0668945313 descent=3.0659179688 height=15.1328125000
    // SansSerif 18 PLAIN: ascent=16.7080078125 descent=4.2451171875 height=20.9531250000
    // charW('W') at 12 = 11.8652343750
    // width('foo1') at 12 = 26.5429687500

    #[test]
    fn ascent_matches_java() {
        let a12 = ascent("SansSerif", 12.0, false, false);
        let a13 = ascent("SansSerif", 13.0, false, false);
        let a18 = ascent("SansSerif", 18.0, false, false);
        assert!((a12 - 11.1386718750).abs() < 1e-6, "a12={a12}");
        assert!((a13 - 12.0668945313).abs() < 1e-6, "a13={a13}");
        assert!((a18 - 16.7080078125).abs() < 1e-6, "a18={a18}");
    }

    #[test]
    fn descent_matches_java() {
        let d12 = descent("SansSerif", 12.0, false, false);
        assert!((d12 - 2.8300781250).abs() < 1e-6, "d12={d12}");
    }

    #[test]
    fn line_height_matches_java() {
        let h12 = line_height("SansSerif", 12.0, false, false);
        let h13 = line_height("SansSerif", 13.0, false, false);
        let h18 = line_height("SansSerif", 18.0, false, false);
        assert!((h12 - 13.9687500000).abs() < 1e-6, "h12={h12}");
        assert!((h13 - 15.1328125000).abs() < 1e-6, "h13={h13}");
        assert!((h18 - 20.9531250000).abs() < 1e-6, "h18={h18}");
    }

    #[test]
    fn char_width_w_matches_java() {
        let w = char_width('W', "SansSerif", 12.0, false, false);
        assert!((w - 11.8652343750).abs() < 1e-6, "W width={w}");
    }

    #[test]
    fn text_width_foo1_matches_java() {
        let w = text_width("foo1", "SansSerif", 12.0, false, false);
        assert!((w - 26.5429687500).abs() < 1e-4, "foo1 width={w}");
    }

    #[test]
    fn monospaced_metrics() {
        // All monospaced chars should have equal advance width
        let w_a = char_width('a', "Monospaced", 13.0, false, false);
        let w_w = char_width('W', "Monospaced", 13.0, false, false);
        assert!(
            (w_a - w_w).abs() < 1e-6,
            "mono: a={w_a} W={w_w} should be equal"
        );
    }

    #[test]
    fn bold_width_differs() {
        let w_plain = char_width('W', "SansSerif", 12.0, false, false);
        let w_bold = char_width('W', "SansSerif", 12.0, true, false);
        assert!(w_bold > w_plain, "bold W should be wider");
    }

    #[test]
    fn family_resolution() {
        // "Monospaced" (Java logical name) resolves to mono font
        let w_mono = char_width('a', "Monospaced", 12.0, false, false);
        let w_monospace = char_width('a', "monospace", 12.0, false, false);
        assert!((w_mono - w_monospace).abs() < 1e-10);
        // "Courier" (Java logical font, no "New") maps to Monospaced
        let w_courier = char_width('a', "Courier", 12.0, false, false);
        assert!((w_mono - w_courier).abs() < 1e-10, "Courier maps to mono");
        // "Courier New" is a physical font not installed on reference machine
        // → Java Dialog fallback → sans-serif
        let w_courier_new = char_width('a', "Courier New", 12.0, false, false);
        let w_sans = char_width('a', "SansSerif", 12.0, false, false);
        assert!(
            (w_courier_new - w_sans).abs() < 1e-10,
            "Courier New maps to sans (Dialog fallback)"
        );
        // "SansSerif", "Dialog", "Arial" all resolve to sans font
        let w3 = char_width('a', "SansSerif", 12.0, false, false);
        let w4 = char_width('a', "Dialog", 12.0, false, false);
        assert!((w3 - w4).abs() < 1e-10);
    }

    #[test]
    fn arbitrary_size_works() {
        // Size 15 was not in the old lookup table — runtime computation handles any size
        let h = line_height("SansSerif", 15.0, false, false);
        assert!(h > 0.0);
        assert!((h - (1901.0 + 483.0) / 2048.0 * 15.0).abs() < 1e-6);
    }

    #[test]
    fn text_width_matches_java_reference() {
        // Verify text_width matches Java PlantUML's getStringBounds for various strings
        let cases: &[(&str, f64, bool, f64)] = &[
            ("Alice", 14.0, false, 33.667),
            ("Bob", 14.0, false, 27.0566),
            ("Hello", 13.0, false, 32.9507),
            ("Test", 14.0, false, 29.9482),
            ("Grouping messages", 13.0, true, 144.5869),
            ("Swimlane1", 18.0, false, 98.6484),
            ("Action 1", 12.0, false, 49.2422),
        ];
        for (text, size, bold, java_w) in cases {
            let our_w = text_width(text, "SansSerif", *size, *bold, false);
            assert!(
                (our_w - java_w).abs() < 0.001,
                "text_width(\"{text}\", size={size}, bold={bold}): ours={our_w:.4}, java={java_w:.4}"
            );
        }
    }

    #[test]
    fn measure_requirement_labels() {
        use crate::render::foreign_object::{measure_html_label, HtmlLabelFont};
        let labels = [
            "<<Requirement>>",
            "&lt;&lt;Requirement&gt;&gt;",
            "test_req",
            "ID: 1",
            "Text: the test text.",
            "Risk: High",
            "Verification: Test",
            "<<Element>>",
            "&lt;&lt;Element&gt;&gt;",
            "test_entity",
            "Type: simulation",
        ];
        let font = HtmlLabelFont::default();
        for l in &labels {
            let (w, _h) = measure_html_label(l, &font, 200.0, true);
            let w16 = text_width(l, "sans-serif", 16.0, false, false);
            eprintln!(
                "label={:40} fo_w={:20} w16={:20} w16+50={}",
                l,
                w,
                w16,
                w16 + 50.0
            );
        }
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;
    #[test]
    fn measure_crash_bold() {
        let normal = text_width("Crash", "sans-serif", 14.0, false, false);
        let bold = text_width("Crash", "sans-serif", 14.0, true, false);
        eprintln!("Crash normal={} bold={}", normal, bold);
        let normal_b = text_width("B", "sans-serif", 14.0, false, false);
        let bold_b = text_width("B", "sans-serif", 14.0, true, false);
        eprintln!("B normal={} bold={}", normal_b, bold_b);
    }

    #[test]
    fn measure_markdown_segments() {
        eprintln!(
            "Text: = {}",
            text_width("Text: ", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "Bolded text (bold) = {}",
            text_width("Bolded text", "sans-serif", 14.0, true, false)
        );
        eprintln!(
            "  (space) = {}",
            text_width(" ", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "italicized text = {}",
            text_width("italicized text", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "Sum = {}",
            text_width("Text: ", "sans-serif", 14.0, false, false)
                + text_width("Bolded text", "sans-serif", 14.0, true, false)
                + text_width(" ", "sans-serif", 14.0, false, false)
                + text_width("italicized text", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "my bolded name (bold) = {}",
            text_width("my bolded name", "sans-serif", 14.0, true, false)
        );
        eprintln!(
            "my italicized name (plain) = {}",
            text_width("my italicized name", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "Bolded type (bold) = {}",
            text_width("Bolded type", "sans-serif", 14.0, true, false)
        );
        eprintln!(
            "italicized type (plain) = {}",
            text_width("italicized type", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "Type: Bolded type italicized type = {}",
            text_width("Type: ", "sans-serif", 14.0, false, false)
                + text_width("Bolded type", "sans-serif", 14.0, true, false)
                + text_width(" ", "sans-serif", 14.0, false, false)
                + text_width("italicized type", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "Italicized (plain) = {}",
            text_width("Italicized", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "Bolded (bold) = {}",
            text_width("Bolded", "sans-serif", 14.0, true, false)
        );
        eprintln!(
            "Doc Ref: Italicized Bolded = {}",
            text_width("Doc Ref: ", "sans-serif", 14.0, false, false)
                + text_width("Italicized", "sans-serif", 14.0, false, false)
                + text_width(" ", "sans-serif", 14.0, false, false)
                + text_width("Bolded", "sans-serif", 14.0, true, false)
        );
        // Plain widths for comparison
        eprintln!(
            "my bolded name (PLAIN) = {}",
            text_width("my bolded name", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "Type: Bolded type italicized type (PLAIN all) = {}",
            text_width(
                "Type: Bolded type italicized type",
                "sans-serif",
                14.0,
                false,
                false
            )
        );
        eprintln!(
            "Doc Ref: Italicized Bolded (PLAIN all) = {}",
            text_width(
                "Doc Ref: Italicized Bolded",
                "sans-serif",
                14.0,
                false,
                false
            )
        );
        // Reference expected widths
        eprintln!("Ref: 118.2548828125 (my bolded name)");
        eprintln!("Ref: 230.0224609375 (Type: **Bolded type** _italicized type_)");
        eprintln!("Ref: 179.23828125 (Doc Ref: *Italicized* __Bolded__)");

        // test_entity (bold) name
        eprintln!(
            "test_entity (PLAIN) = {}",
            text_width("test_entity", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "test_entity (BOLD) = {}",
            text_width("test_entity", "sans-serif", 14.0, true, false)
        );
        eprintln!("Ref: 84.984375 (test_entity name FO width from fixture 38)");
        eprintln!(
            "sys_req (PLAIN) = {}",
            text_width("sys_req", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "sys_req (BOLD) = {}",
            text_width("sys_req", "sans-serif", 14.0, true, false)
        );
        eprintln!(
            "test_req (PLAIN) = {}",
            text_width("test_req", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "test_req (BOLD) = {}",
            text_width("test_req", "sans-serif", 14.0, true, false)
        );
        eprintln!("Ref fixture 38 test_req name: 64.6337890625");
        eprintln!(
            "<<Requirement>> (PLAIN) = {}",
            text_width("<<Requirement>>", "sans-serif", 14.0, false, false)
        );
        eprintln!(
            "<<Requirement>> (BOLD) = {}",
            text_width("<<Requirement>>", "sans-serif", 14.0, true, false)
        );
        // Fixture 34: all bold
        eprintln!(
            "<<Element>> (BOLD) = {}",
            text_width("<<Element>>", "sans-serif", 14.0, true, false)
        );
        eprintln!(
            "ID: 1 (BOLD) = {}",
            text_width("ID: 1", "sans-serif", 14.0, true, false)
        );
        eprintln!(
            "Text: the test text. (BOLD) = {}",
            text_width("Text: the test text.", "sans-serif", 14.0, true, false)
        );
        eprintln!(
            "Risk: High (BOLD) = {}",
            text_width("Risk: High", "sans-serif", 14.0, true, false)
        );
        eprintln!(
            "Verification: Test (BOLD) = {}",
            text_width("Verification: Test", "sans-serif", 14.0, true, false)
        );
        eprintln!(
            "Type: simulation (BOLD) = {}",
            text_width("Type: simulation", "sans-serif", 14.0, true, false)
        );
    }
}

#[cfg(test)]
mod cjk_recovery_tests {
    use super::*;

    // Sample CJK text built via codepoints to keep this source file ASCII.
    // Equivalent to U+63D0 U+4EA4 U+7533 U+8BF7 (a 4-char Chinese label).
    fn cjk_sample() -> String {
        ['\u{63D0}', '\u{4EA4}', '\u{7533}', '\u{8BF7}']
            .iter()
            .collect()
    }

    #[test]
    fn cjk_string_measured_correctly() {
        // 4 CJK chars, each falls back to space advance (~4.45 @ 14pt sans).
        let s = cjk_sample();
        let w = text_width(&s, "sans-serif", 14.0, false, false);
        assert!((w - 17.80078125).abs() < 1e-6, "cjk width = {w}");
    }

    #[test]
    fn mangled_utf8_recovered() {
        // Simulate the upstream bug: `b as char` over UTF-8 bytes of the CJK
        // sample. Direct char-by-char measurement would be ~80px (Latin-1
        // supplement glyphs); the recovery shim should restore the real width.
        let s = cjk_sample();
        let mangled: String = s.bytes().map(|b| b as char).collect();
        let w_mangled = text_width(&mangled, "sans-serif", 14.0, false, false);
        let w_clean = text_width(&s, "sans-serif", 14.0, false, false);
        assert!(
            (w_mangled - w_clean).abs() < 1e-6,
            "mangled utf8 not recovered: mangled={w_mangled}, clean={w_clean}"
        );
    }

    #[test]
    fn genuine_latin1_unaffected() {
        // "cafe" with U+00E9 is a real Rust &str; should NOT be re-decoded.
        // (A lone 0xE9 byte is not a valid UTF-8 start, so the recovery shim
        // declines to touch it.)
        let cafe: String = ['c', 'a', 'f', '\u{00E9}'].iter().collect();
        let w = text_width(&cafe, "sans-serif", 14.0, false, false);
        let expected: f64 = cafe
            .chars()
            .map(|c| char_width(c, "sans-serif", 14.0, false, false))
            .sum();
        assert!(
            (w - expected).abs() < 1e-9,
            "latin-1 string perturbed: {w} vs {expected}"
        );
    }

    #[test]
    fn pure_ascii_unaffected() {
        let w = text_width("Hello", "sans-serif", 14.0, false, false);
        assert!(w > 0.0);
        let direct: f64 = "Hello"
            .chars()
            .map(|c| char_width(c, "sans-serif", 14.0, false, false))
            .sum();
        assert!((w - direct).abs() < 1e-9);
    }
}

#[cfg(test)]
mod debug_note_width {
    use super::*;
    #[test]
    fn note_text_width_cy11() {
        // Upstream: jsdom textContent concatenates lines (strips <br/>).
        // \n has zero advance so text_width with \n gives concatenated width.
        let full = "Important information! You can write\nnotes.";
        let w = text_width(full, "sans-serif", 14.0, false, false);
        let note_w = w + 30.0; // + 2*15 padding
                               // Expected: (303.48828125) + 30 = 333.48828125
        assert!(
            (note_w - 333.48828125).abs() < 0.01,
            "note_w = {} but expected 333.48828125",
            note_w
        );
    }
}
