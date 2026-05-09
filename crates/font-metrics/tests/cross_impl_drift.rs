//! Cross-implementation drift tolerance test.
//!
//! Asserts that `StaticDejaVuMetrics` (offline-baked Java-parity range
//! tables) and `TtfParserMetrics::default_latin()` (runtime ttf-parser
//! over the embedded DejaVu Latin subset) agree to within a small
//! pixel bound on Latin stimuli. Both impls back onto the same DejaVu
//! upstream, so any future drift between them is a regression worth
//! catching.

#![cfg(feature = "static-fixtures")]

use font_metrics::static_dejavu::StaticDejaVuMetrics;
use font_metrics::ttf_parser::TtfParserMetrics;
use font_metrics::Metrics;

const SIZES: &[f64] = &[10.0, 12.0, 14.0, 18.0];
const HORIZONTAL_TOL: f64 = 1.5;
const VERTICAL_TOL: f64 = 2.5;

fn check(label: &str, family: &str, size: f64, tol: f64, st: f64, dy: f64) {
    let drift = (st - dy).abs();
    assert!(
        drift <= tol,
        "{label} family={family} size={size} static={st:.4} dynamic={dy:.4} drift={drift:.4} tol={tol}"
    );
}

#[test]
fn static_vs_ttf_parser_latin_drift_within_bounds() {
    let s = StaticDejaVuMetrics;
    let d = TtfParserMetrics::default_latin().expect("embedded DejaVu Latin parses");

    for &size in SIZES {
        let sans = "Sans";
        let mono = "Monospaced";
        let hello = "Hello, world!";

        check(
            "char_width('A',Sans)",
            sans,
            size,
            HORIZONTAL_TOL,
            s.char_width('A', sans, size, false, false),
            d.char_width('A', sans, size, false, false),
        );
        check(
            "char_width('g',Sans)",
            sans,
            size,
            HORIZONTAL_TOL,
            s.char_width('g', sans, size, false, false),
            d.char_width('g', sans, size, false, false),
        );
        check(
            "char_width('M',Monospaced)",
            mono,
            size,
            HORIZONTAL_TOL,
            s.char_width('M', mono, size, false, false),
            d.char_width('M', mono, size, false, false),
        );
        check(
            "text_width('Hello, world!',Sans)",
            sans,
            size,
            HORIZONTAL_TOL,
            s.text_width(hello, sans, size, false, false),
            d.text_width(hello, sans, size, false, false),
        );
        check(
            "text_width('Hello, world!',Sans,bold)",
            sans,
            size,
            HORIZONTAL_TOL,
            s.text_width(hello, sans, size, true, false),
            d.text_width(hello, sans, size, true, false),
        );
        check(
            "line_height(Sans)",
            sans,
            size,
            VERTICAL_TOL,
            s.line_height(sans, size, false, false),
            d.line_height(sans, size, false, false),
        );
        check(
            "ascent(Sans)",
            sans,
            size,
            VERTICAL_TOL,
            s.ascent(sans, size, false, false),
            d.ascent(sans, size, false, false),
        );
        check(
            "descent(Sans)",
            sans,
            size,
            VERTICAL_TOL,
            s.descent(sans, size, false, false),
            d.descent(sans, size, false, false),
        );
    }
}
