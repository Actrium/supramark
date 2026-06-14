//! D2GoEmulationMetrics — font_metrics_core::Metrics impl backed by the
//! existing D2GoEmulationRuler (Atlas + Int26_6 byte-equal Go simulation).
//!
//! Lives in d2-little (not font-metrics) because the underlying ruler
//! needs d2-specific FontFamily / FontStyle enums and embedded ttf data
//! that would create a font-metrics → d2-little cyclic dep.

use std::cell::RefCell;

use font_metrics_core::{Measured, Metrics};

use super::D2Metrics;
use super::MarkdownOptions;
use super::d2_go_emulation::D2GoEmulationRuler;
use crate::fonts::{FONT_SIZES, Font, FontFamily, FontStyle};

/// Font-metrics adapter wrapping `D2GoEmulationRuler`. Use this where
/// d2's byte-equal Go layout is required (CLI, byte-equal regression
/// tests). For wasm production where layer-1 = layer-3 matters more
/// than Go parity, use `font_metrics_core::host_callback::HostCallbackMetrics`.
pub struct D2GoEmulationMetrics {
    // RefCell for interior mutability — the Metrics trait takes &self,
    // but the ruler's Atlas cache / dot state require &mut.
    inner: RefCell<D2GoEmulationRuler>,
}

impl D2GoEmulationMetrics {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            inner: RefCell::new(D2GoEmulationRuler::new()?),
        })
    }
}

fn family_from_str(s: &str) -> FontFamily {
    match s.to_lowercase().as_str() {
        "source sans pro" | "sourcesanspro" | "sans-serif" | "sans" => FontFamily::SourceSansPro,
        "source code pro" | "sourcecodepro" | "monospace" | "monospaced" | "courier" => {
            FontFamily::SourceCodePro
        }
        "fuzzy bubbles" | "fuzzybubbles" | "handdrawn" | "hand-drawn" => FontFamily::HandDrawn,
        _ => FontFamily::SourceSansPro,
    }
}

fn style_from(bold: bool, italic: bool) -> FontStyle {
    match (bold, italic) {
        (false, false) => FontStyle::Regular,
        (true, false) => FontStyle::Bold,
        (false, true) => FontStyle::Italic,
        // d2's FontStyle has no BoldItalic — map to Bold; Italic-only tracks
        // only the slant axis. This matches existing d2 lib usage.
        (true, true) => FontStyle::Bold,
    }
}

/// Round an arbitrary size to the closest d2-blessed size constant
/// (XS=13, S=14, M=16, L=20, XL=24, XXL=28, XXXL=32). Other sizes
/// are not in d2's pre-built atlas matrix; the closest is used.
fn round_to_d2_size(size: f64) -> i32 {
    let mut best = FONT_SIZES[0];
    let mut min_diff = (size - best as f64).abs();
    for &s in FONT_SIZES.iter().skip(1) {
        let d = (size - s as f64).abs();
        if d < min_diff {
            min_diff = d;
            best = s;
        }
    }
    best
}

impl D2Metrics for D2GoEmulationMetrics {
    fn line_height_factor(&self) -> f64 {
        self.inner.borrow().line_height_factor
    }

    fn set_line_height_factor(&self, value: f64) {
        self.inner.borrow_mut().line_height_factor = value;
    }

    fn measure_text(&self, font: Font, s: &str) -> (i32, i32) {
        self.inner.borrow_mut().measure(font, s)
    }

    fn measure_mono(&self, font: Font, s: &str) -> (i32, i32) {
        self.inner.borrow_mut().measure_mono(font, s)
    }

    fn measure_precise(&self, font: Font, s: &str) -> (f64, f64) {
        self.inner.borrow_mut().measure_precise(font, s)
    }

    fn space_width(&self, font: Font) -> f64 {
        self.inner.borrow_mut().space_width(font)
    }

    fn scale_unicode(&self, w: f64, font: Font, s: &str) -> f64 {
        self.inner.borrow_mut().scale_unicode(w, font, s)
    }

    fn measure_markdown(
        &self,
        md_text: &str,
        opts: MarkdownOptions,
        font_size: i32,
    ) -> Result<(i32, i32), String> {
        // Save / set per-call state on the underlying ruler, then drop the
        // mutable borrow before invoking the trait-generic walker (which
        // re-borrows the ruler through Self's D2Metrics impl on every
        // measure_precise / space_width / scale_unicode / lh call).
        let (original_lh, original_bounds) = {
            let mut ruler = self.inner.borrow_mut();
            let lh = ruler.line_height_factor;
            let bounds = ruler.bounds_with_dot;
            ruler.bounds_with_dot = true;
            ruler.line_height_factor = super::markdown::MARKDOWN_LINE_HEIGHT;
            (lh, bounds)
        };
        let result = super::markdown::measure_markdown_generic(
            self,
            md_text,
            opts.font_family,
            opts.mono_font_family,
            font_size,
        );
        {
            let mut ruler = self.inner.borrow_mut();
            ruler.line_height_factor = original_lh;
            ruler.bounds_with_dot = original_bounds;
        }
        result
    }
}

impl Metrics for D2GoEmulationMetrics {
    fn measure(&self, text: &str, family: &str, size: f64, bold: bool, italic: bool) -> Measured {
        let font = Font {
            family: family_from_str(family),
            style: style_from(bold, italic),
            size: round_to_d2_size(size),
        };
        let mut ruler = self.inner.borrow_mut();
        let (width, bounds_h) = ruler.measure_precise(font, text);
        let (face_asc, face_desc) = ruler.face_metrics_for(font);
        // Split bounds_h into ascent/descent using the face's natural ratio,
        // so that ascent + descent == bounds_h exactly (preserves byte-equal Go
        // for d2 layout callers that compute h = ascent + descent downstream).
        // This deviates from the text-specific actualBoundingBox semantics that
        // host_callback uses, but D2GoEmulation is d2-internal — its semantic
        // is "what d2 layout expects".
        let face_total = face_asc + face_desc;
        let (ascent, descent) = if face_total > 0.0 {
            let ratio = face_asc / face_total;
            (bounds_h * ratio, bounds_h * (1.0 - ratio))
        } else {
            (bounds_h, 0.0)
        };
        Measured {
            width,
            ascent,
            descent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_matches_ruler_measure_precise() {
        // Sanity: D2GoEmulationMetrics.measure("Hello", "Source Sans Pro", 14, false, false).width
        // must equal D2GoEmulationRuler::new().measure_precise(SourceSansPro.font(14, Regular), "Hello").0
        let metrics = D2GoEmulationMetrics::new().expect("init metrics");
        let m = metrics.measure("Hello", "Source Sans Pro", 14.0, false, false);

        let mut ruler = D2GoEmulationRuler::new().expect("init ruler");
        let font = FontFamily::SourceSansPro.font(14, FontStyle::Regular);
        let (w, h) = ruler.measure_precise(font, "Hello");

        assert!(
            (m.width - w).abs() < 0.001,
            "width mismatch: metrics={}, ruler={}",
            m.width,
            w
        );
        assert!(
            (m.ascent + m.descent - h).abs() < 0.001,
            "height total must equal ruler bounds.h: ascent+descent={}, ruler.h={}",
            m.ascent + m.descent,
            h,
        );
        assert!(m.ascent > 0.0, "ascent should be positive");
        assert!(m.descent >= 0.0, "descent should be non-negative");
    }

    #[test]
    fn family_resolution_handles_common_strings() {
        assert!(matches!(
            family_from_str("Source Sans Pro"),
            FontFamily::SourceSansPro
        ));
        assert!(matches!(
            family_from_str("sans-serif"),
            FontFamily::SourceSansPro
        ));
        assert!(matches!(
            family_from_str("monospace"),
            FontFamily::SourceCodePro
        ));
        assert!(matches!(
            family_from_str("source code pro"),
            FontFamily::SourceCodePro
        ));
        assert!(matches!(
            family_from_str("Fuzzy Bubbles"),
            FontFamily::HandDrawn
        ));
        assert!(matches!(
            family_from_str("nonsense"),
            FontFamily::SourceSansPro
        )); // fallback
    }

    #[test]
    fn round_to_d2_size_picks_closest() {
        assert_eq!(round_to_d2_size(14.0), 14);
        assert_eq!(round_to_d2_size(15.0), 14); // 14 closer than 16 (1 vs 1) — picks 14 (first match)
        assert_eq!(round_to_d2_size(15.5), 16); // 16 closer (0.5 vs 1.5)
        assert_eq!(round_to_d2_size(100.0), 32); // clamps to max
        assert_eq!(round_to_d2_size(1.0), 13); // clamps to min
    }

    /// Phase 3 spike (2026-05-10): validated that ruler.measure_precise on
    /// multi-line text canNOT be exactly reproduced by per-line measure +
    /// caller-side composition, because Go upstream `drawBuf` (lib/textmeasure
    /// /textmeasure.go:282) intentionally leaks `prevR` across `\n` (only
    /// `clear()` resets it; `controlRune('\n')` updates Dot but not prevR).
    /// This produces a ~1 px width drift on certain prev/next char pairs.
    ///
    /// Result: multi-line + lh state stays INSIDE D2GoEmulationMetrics
    /// (RefCell<Ruler>); the Metrics trait stays pure single-call. Phase 3
    /// changes lib.rs signatures to `&dyn Metrics` for plain measure sites
    /// and `&D2GoEmulationMetrics` (concrete) for sites that need to call
    /// `set_line_height_factor`. Plantuml/mermaid trait usage is unaffected
    /// since they never feed `\n`-containing strings to measure.
    ///
    /// Height formula DOES work for caller-side composition:
    ///     bounds.h() = (asc + desc) + (n - 1) * lh_factor * font_size_px
    /// Used by the wasm `D2HostMetrics` adapter where Go byte-equal is
    /// impossible anyway (host canvas uses its own font stack).
    #[test]
    #[ignore = "documentation spike, retained as historical evidence"]
    fn spike_multiline_decomposition() {
        let cases: &[(&str, i32, FontFamily)] = &[
            ("Hello", 14, FontFamily::SourceSansPro),
            ("Hello\nWorld", 14, FontFamily::SourceSansPro),
            ("Hello\nWorld\nFoo", 14, FontFamily::SourceSansPro),
            ("Short\nMuchLonger", 14, FontFamily::SourceSansPro),
            ("MuchLonger\nShort", 14, FontFamily::SourceSansPro),
            ("\u{4E2D}\n\u{6587}", 16, FontFamily::SourceSansPro),
            (
                "\u{4E2D}\u{6587}\u{6DF7}\u{5408}\nABC123",
                16,
                FontFamily::SourceSansPro,
            ),
            (
                "\u{4E00}\u{4E8C}\u{4E09}\n\u{56DB}\u{4E94}\n\u{516D}",
                14,
                FontFamily::SourceCodePro,
            ),
            ("Hello\nWorld", 20, FontFamily::SourceSansPro),
            ("", 14, FontFamily::SourceSansPro),
            ("\n", 14, FontFamily::SourceSansPro),
            ("a\n\nb", 14, FontFamily::SourceSansPro),
        ];
        let lh_factors: &[f64] = &[1.0, 1.3, 1.45, 1.5, 1.25];

        let mut report = String::new();
        let mut all_match = true;

        for &lh in lh_factors {
            for (text, size, family) in cases {
                let font = Font {
                    family: *family,
                    style: FontStyle::Regular,
                    size: *size,
                };

                // Reference: ruler.measure_precise on the multi-line text
                let mut r_multi = D2GoEmulationRuler::new().expect("init multi");
                r_multi.set_line_height_factor(lh);
                let (w_ref, h_ref) = r_multi.measure_precise(font, text);

                // Caller-side composition from per-line measure
                let lines: Vec<&str> = text.split('\n').collect();
                let n = lines.len();

                let mut r_per = D2GoEmulationRuler::new().expect("init per");
                r_per.set_line_height_factor(lh); // shouldn't matter for single-line
                let per_line: Vec<(f64, f64)> = lines
                    .iter()
                    .map(|line| r_per.measure_precise(font, line))
                    .collect();

                let max_w = per_line.iter().map(|(w, _)| *w).fold(0.0_f64, f64::max);

                // single-line height (asc + desc) — same for all lines (same font)
                let single_h = per_line.first().map(|(_, h)| *h).unwrap_or(0.0);

                // line_height_unit = font_size in pixels (atlas.line_height = i2f(scale_26_6))
                let lh_unit = *size as f64;

                let composed_h = if text.is_empty() {
                    // Edge case: empty text → bounds = Rect::zero() → h = 0.0
                    0.0
                } else {
                    single_h + ((n - 1) as f64) * lh * lh_unit
                };

                let w_match = (max_w - w_ref).abs() < 0.001;
                let h_match = (composed_h - h_ref).abs() < 0.001;

                if !w_match || !h_match {
                    all_match = false;
                }

                report.push_str(&format!(
                    "lh={:.2} fam={:?} sz={} text={:?}\n  ref=({:.4},{:.4}) composed=({:.4},{:.4}) per_line={:?} {}\n",
                    lh,
                    family,
                    size,
                    text,
                    w_ref,
                    h_ref,
                    max_w,
                    composed_h,
                    per_line,
                    if w_match && h_match { "OK" } else { "MISMATCH" },
                ));
            }
        }

        eprintln!("{}", report);
        assert!(
            all_match,
            "multi-line decomposition formula does not match ruler output for all cases"
        );
    }
}
