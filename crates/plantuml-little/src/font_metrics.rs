//! Thin wrapper around [`font_metrics_core::Metrics`].
//!
//! The 6 free functions (`char_width`, `text_width`, `line_height`,
//! `ascent`, `descent`, `typo_ascent`) preserve their pre-extraction
//! signatures so the ~150 call sites scattered through `layout/`,
//! `render/`, `skin/` continue to compile unchanged. Internally each
//! delegates to a compile-time-selected `&'static dyn Metrics`
//! provider; the active impl is picked by mutually-exclusive Cargo
//! features:
//!
//! - `metrics-ttf-parser` — runtime measurement against a ttf-parser-
//!   parsed font (defaults to font-metrics's embedded DejaVu Latin
//!   subset). Doubles as the byte-exact Java FontMetrics parity path
//!   for the 268+ reference SVG snapshot tests: a 2026-05-10
//!   measurement spike confirmed raw glyph advances from the embedded
//!   DejaVu subset match Java's font stack to sub-0.0001 px on the
//!   discriminating italic test (`«archimate-node»` italic =
//!   128.385742 px vs Java 128.3857 px). Replaced the legacy
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
/// byte-exact Java FontMetrics parity for the 268+ reference SVG
/// tests (per the 2026-05-10 measurement spike — see module docs).
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
            font_metrics_core::ttf_parser::TtfParserMetrics::default_latin()
                .expect("default Latin TTF subset failed to parse")
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
    compile_error!("plantuml-little requires exactly one metrics-* feature; enable metrics-ttf-parser for native byte-exact Java parity, metrics-host-callback for wasm host-bridge, or metrics-ffi-callback for RN native host-bridge");
}

/// Width of a single character (typographic horizontal advance).
pub fn char_width(ch: char, family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    metrics_provider().char_width(ch, family, size, bold, italic)
}

/// Total width of a text string (sum of per-character advances).
pub fn text_width(text: &str, family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    metrics_provider().text_width(text, family, size, bold, italic)
}

/// Line height = ascent + |descent|. Matches Java's
/// `LineMetrics.getHeight()`.
pub fn line_height(family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    metrics_provider().line_height(family, size, bold, italic)
}

/// Distance from baseline to top of the tallest glyph
/// (`LineMetrics.getAscent()`).
pub fn ascent(family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    metrics_provider().ascent(family, size, bold, italic)
}

/// Distance from baseline to bottom of the lowest glyph
/// (positive value, `LineMetrics.getDescent()`).
pub fn descent(family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    metrics_provider().descent(family, size, bold, italic)
}

/// OS/2 typographic ascent. Used by DOT cluster label dimensions
/// which match Java's `StringBounder.calculateDimension()` text
/// block height.
pub fn typo_ascent(family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    metrics_provider().typo_ascent(family, size, bold, italic)
}

#[cfg(test)]
mod tests {
    //! Sanity checks that the wrapper still pipes through to the
    //! shared `TtfParserMetrics` impl. The `TtfParserMetrics` unit
    //! tests live alongside the impl in `font-metrics/src/ttf_parser.rs`.

    use super::*;

    #[test]
    fn ascent_matches_java() {
        assert!((ascent("SansSerif", 12.0, false, false) - 11.1386718750).abs() < 1e-6);
    }

    #[test]
    fn descent_matches_java() {
        assert!((descent("SansSerif", 12.0, false, false) - 2.8300781250).abs() < 1e-6);
    }

    #[test]
    fn line_height_matches_java() {
        assert!((line_height("SansSerif", 12.0, false, false) - 13.9687500000).abs() < 1e-6);
    }

    #[test]
    fn char_width_w_matches_java() {
        assert!((char_width('W', "SansSerif", 12.0, false, false) - 11.8652343750).abs() < 1e-6);
    }

    #[test]
    fn text_width_foo1_matches_java() {
        assert!((text_width("foo1", "SansSerif", 12.0, false, false) - 26.5429687500).abs() < 1e-4);
    }

    #[test]
    fn monospaced_metrics() {
        let w_a = char_width('a', "Monospaced", 13.0, false, false);
        let w_w = char_width('W', "Monospaced", 13.0, false, false);
        assert!((w_a - w_w).abs() < 1e-6);
    }

    #[test]
    fn bold_width_differs() {
        let w_plain = char_width('W', "SansSerif", 12.0, false, false);
        let w_bold = char_width('W', "SansSerif", 12.0, true, false);
        assert!(w_bold > w_plain);
    }

    #[test]
    fn italic_uses_oblique_metrics() {
        let w1 = text_width("«archimate-node»", "SansSerif", 14.0, false, true);
        assert!((w1 - 128.3857).abs() < 0.01);
        let p1 = text_width("«archimate-node»", "SansSerif", 14.0, false, false);
        assert!(w1 > p1);
    }

    #[test]
    fn family_resolution() {
        let w_mono = char_width('a', "Monospaced", 12.0, false, false);
        let w_courier = char_width('a', "Courier", 12.0, false, false);
        assert!((w_mono - w_courier).abs() < 1e-10);
        let w_courier_new = char_width('a', "Courier New", 12.0, false, false);
        let w_sans = char_width('a', "SansSerif", 12.0, false, false);
        assert!((w_courier_new - w_sans).abs() < 1e-10);
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
            let ours = text_width(text, "SansSerif", *size, *bold, false);
            assert!((ours - java_w).abs() < 0.001, "text=\"{text}\" ours={ours:.4} java={java_w:.4}");
        }
    }
}
