//! Thin wrapper around [`font_metrics_core::Metrics`].
//!
//! mermaid-little preserves a few mermaid-specific behaviours on top
//! of the shared metrics impl:
//!
//! - **`text_width` recovery shim** for upstream Mermaid label
//!   builders that cast raw UTF-8 bytes to `char` (the resulting
//!   Latin-1 supplement string would otherwise inflate
//!   `<foreignObject>` widths). Detect+decode is mermaid-specific
//!   so it stays here.
//! - **Vertical metrics ignore bold / italic**: mermaid's reference
//!   output captures DejaVu Sans plain ascender/descender even when
//!   the rendered glyph runs are bold; the `false, false` defaults
//!   are deliberate.
//!
//! Internally each function delegates to a compile-time-selected
//! `&'static dyn Metrics` provider; the active impl is picked by
//! mutually-exclusive Cargo features:
//!
//! - `metrics-ttf-parser` — runtime measurement against a ttf-parser-
//!   parsed font (defaults to font-metrics's embedded DejaVu Latin
//!   subset). Doubles as the byte-exact upstream-Mermaid parity path
//!   for the *_byte_exact.rs reference suites: a 2026-05-10
//!   measurement spike confirmed raw glyph advances from the embedded
//!   DejaVu subset match Java's font stack to sub-0.0001 px on the
//!   discriminating italic test. Replaced the legacy
//!   `metrics-java-compat` slot on 2026-05-10 after the spike showed
//!   the wrapper's italic-skew adjustment was based on a wrong AWT
//!   assumption and over-corrected widths.
//! - `metrics-host-callback` (wasm32 only) — defer measurement to a
//!   JS-side `globalThis.supramark.measureText` bridge so layer-1
//!   layout sees the exact widths the host browser / RN-Skia will
//!   render with.
//! - `metrics-ffi-callback` (non-wasm only) — defer measurement to a
//!   native-side `extern "C"` callback installed via
//!   `supramark_install_metrics_callback`. The React-Native path:
//!   the iOS / Android TurboModule wraps UIFont / Paint / RN-Skia
//!   and registers the C ABI shim once at module init.

use font_metrics_core::Metrics;

/// Compile-time-selected `Metrics` provider. Exactly one of the
/// `metrics-*` Cargo features must be enabled; the dev-dep self-cycle
/// in Cargo.toml selects `metrics-ttf-parser`, which preserves
/// byte-exact upstream-Mermaid parity for the *_byte_exact.rs
/// reference suites (per the 2026-05-10 measurement spike — see module
/// docs).
#[inline]
fn metrics_provider() -> &'static dyn Metrics {
    #[cfg(all(feature = "metrics-host-callback", target_arch = "wasm32"))]
    {
        static M: font_metrics_core::host_callback::HostCallbackMetrics =
            font_metrics_core::host_callback::HostCallbackMetrics;
        return &M;
    }

    #[cfg(all(
        not(all(feature = "metrics-host-callback", target_arch = "wasm32")),
        feature = "metrics-ttf-parser",
    ))]
    {
        use std::sync::OnceLock;
        static M: OnceLock<font_metrics_core::ttf_parser::TtfParserMetrics<'static>> =
            OnceLock::new();
        return M.get_or_init(|| {
            // mermaid-little ports mermaid.js + canvas, whose missing-glyph
            // behaviour is the space-advance fallback (Chrome canvas effective
            // behaviour when no system font covers the codepoint, also matching
            // the historical StaticDejaVu range-table fallback). Override the
            // Java-AWT-flavoured `Notdef` default so byte-equal `*_byte_exact.rs`
            // suites keep matching the canvas-recorded reference output.
            font_metrics_core::ttf_parser::TtfParserMetrics::default_latin()
                .expect("default Latin TTF subset failed to parse")
                .with_missing_glyph_fallback(
                    font_metrics_core::ttf_parser::MissingGlyphFallback::Space,
                )
        });
    }

    #[cfg(all(
        not(all(feature = "metrics-host-callback", target_arch = "wasm32")),
        not(feature = "metrics-ttf-parser"),
        feature = "metrics-ffi-callback",
        not(target_arch = "wasm32"),
    ))]
    {
        static M: font_metrics_core::ffi_callback::FfiCallbackMetrics =
            font_metrics_core::ffi_callback::FfiCallbackMetrics;
        return &M;
    }

    #[cfg(all(
        not(all(feature = "metrics-host-callback", target_arch = "wasm32")),
        not(feature = "metrics-ttf-parser"),
        feature = "metrics-ffi-callback",
        target_arch = "wasm32",
    ))]
    compile_error!("metrics-ffi-callback is for native targets (RN iOS / Android, macOS, Linux); on wasm32 enable metrics-host-callback instead.");

    #[cfg(not(any(
        feature = "metrics-ttf-parser",
        feature = "metrics-host-callback",
        feature = "metrics-ffi-callback",
    )))]
    compile_error!("mermaid-little requires exactly one metrics-* feature; enable metrics-ttf-parser for native byte-exact parity, metrics-host-callback for wasm host-bridge, or metrics-ffi-callback for RN native host-bridge");
}

/// Width of a single character (typographic horizontal advance).
///
/// Computes `glyph_hor_advance / units_per_em * size`, matching
/// Java's `font.getStringBounds(ch, frc).getWidth()` with
/// `FRACTIONALMETRICS_ON`.
pub fn char_width(ch: char, family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    metrics_provider().char_width(ch, family, size, bold, italic)
}

/// Total width of a text string (sum of character advances).
///
/// Robustness shim: certain upstream label-builders cast raw UTF-8
/// bytes to `char` (`b as char`), which mis-decodes multi-byte CJK
/// / emoji sequences into runs of Latin-1 code points
/// (U+0080..U+00FF). DejaVu has glyphs for most of those Latin-1
/// supplements, so the inflated widths leak straight into
/// `<foreignObject>` sizing. Detect that pattern (every non-ASCII
/// char is in U+0080..U+00FF AND those bytes form valid UTF-8) and
/// measure the recovered string instead. Strings that are genuinely
/// Latin-1 with stray accented letters do not round-trip as valid
/// UTF-8, so they are unaffected.
pub fn text_width(text: &str, family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    if let Some(recovered) = recover_mangled_utf8(text) {
        return metrics_provider().text_width(&recovered, family, size, bold, italic);
    }
    metrics_provider().text_width(text, family, size, bold, italic)
}

/// Detect strings that look like UTF-8 bytes mis-cast to `char` and
/// recover.
///
/// Returns `Some(decoded)` only when every non-ASCII char is in
/// U+0080..U+00FF AND treating them as raw bytes yields a valid
/// UTF-8 sequence. This is strict enough that a genuine Latin-1
/// string with a stray accented char is left alone, because a lone
/// `0xE9` byte is not the start of any valid UTF-8 multi-byte
/// sequence.
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

/// Line height = ascent + |descent|. Matches Java's
/// `LineMetrics.getHeight()`.
///
/// mermaid-specific: vertical metrics intentionally ignore
/// bold / italic — see module docs.
pub fn line_height(family: &str, size: f64, _bold: bool, _italic: bool) -> f64 {
    metrics_provider().line_height(family, size, false, false)
}

/// Distance from baseline to top of the tallest glyph
/// (`LineMetrics.getAscent()`).
pub fn ascent(family: &str, size: f64, _bold: bool, _italic: bool) -> f64 {
    metrics_provider().ascent(family, size, false, false)
}

/// Distance from baseline to bottom of the lowest glyph
/// (positive value, `LineMetrics.getDescent()`).
pub fn descent(family: &str, size: f64, _bold: bool, _italic: bool) -> f64 {
    metrics_provider().descent(family, size, false, false)
}

/// OS/2 typographic ascent — used for DOT cluster label dimensions
/// matching Java's `StringBounder.calculateDimension()`.
pub fn typo_ascent(family: &str, size: f64, _bold: bool, _italic: bool) -> f64 {
    metrics_provider().typo_ascent(family, size, false, false)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let w_a = char_width('a', "Monospaced", 13.0, false, false);
        let w_w = char_width('W', "Monospaced", 13.0, false, false);
        assert!((w_a - w_w).abs() < 1e-6, "mono: a={w_a} W={w_w}");
    }

    #[test]
    fn bold_width_differs() {
        let w_plain = char_width('W', "SansSerif", 12.0, false, false);
        let w_bold = char_width('W', "SansSerif", 12.0, true, false);
        assert!(w_bold > w_plain, "bold W should be wider");
    }

    #[test]
    fn family_resolution() {
        let w_mono = char_width('a', "Monospaced", 12.0, false, false);
        let w_monospace = char_width('a', "monospace", 12.0, false, false);
        assert!((w_mono - w_monospace).abs() < 1e-10);
        let w_courier = char_width('a', "Courier", 12.0, false, false);
        assert!((w_mono - w_courier).abs() < 1e-10, "Courier maps to mono");
        let w_courier_new = char_width('a', "Courier New", 12.0, false, false);
        let w_sans = char_width('a', "SansSerif", 12.0, false, false);
        assert!(
            (w_courier_new - w_sans).abs() < 1e-10,
            "Courier New maps to sans"
        );
        let w3 = char_width('a', "SansSerif", 12.0, false, false);
        let w4 = char_width('a', "Dialog", 12.0, false, false);
        assert!((w3 - w4).abs() < 1e-10);
    }

    #[test]
    fn arbitrary_size_works() {
        let h = line_height("SansSerif", 15.0, false, false);
        assert!(h > 0.0);
        assert!((h - (1901.0 + 483.0) / 2048.0 * 15.0).abs() < 1e-6);
    }

    #[test]
    fn text_width_matches_java_reference() {
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
            "italicized text = {}",
            text_width("italicized text", "sans-serif", 14.0, false, false)
        );
        let sum = text_width("Text: ", "sans-serif", 14.0, false, false)
            + text_width("Bolded text", "sans-serif", 14.0, true, false)
            + text_width(" ", "sans-serif", 14.0, false, false)
            + text_width("italicized text", "sans-serif", 14.0, false, false);
        eprintln!("Sum = {}", sum);
    }
}

#[cfg(test)]
mod cjk_recovery_tests {
    use super::*;

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
        let s = cjk_sample();
        let mangled: String = s.bytes().map(|b| b as char).collect();
        let w_mangled = text_width(&mangled, "sans-serif", 14.0, false, false);
        let w_clean = text_width(&s, "sans-serif", 14.0, false, false);
        assert!(
            (w_mangled - w_clean).abs() < 1e-6,
            "mangled={w_mangled}, clean={w_clean}"
        );
    }

    #[test]
    fn genuine_latin1_unaffected() {
        let cafe: String = ['c', 'a', 'f', '\u{00E9}'].iter().collect();
        let w = text_width(&cafe, "sans-serif", 14.0, false, false);
        let expected: f64 = cafe
            .chars()
            .map(|c| char_width(c, "sans-serif", 14.0, false, false))
            .sum();
        assert!(
            (w - expected).abs() < 1e-9,
            "latin-1 perturbed: {w} vs {expected}"
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
        let full = "Important information! You can write\nnotes.";
        let w = text_width(full, "sans-serif", 14.0, false, false);
        let note_w = w + 30.0;
        assert!(
            (note_w - 333.48828125).abs() < 0.01,
            "note_w = {note_w} expected 333.48828125"
        );
    }
}
