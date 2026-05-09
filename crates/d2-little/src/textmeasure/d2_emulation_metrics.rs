//! D2GoEmulationMetrics — font_metrics_core::Metrics impl backed by the
//! existing D2GoEmulationRuler (Atlas + Int26_6 byte-equal Go simulation).
//!
//! Lives in d2-little (not font-metrics) because the underlying ruler
//! needs d2-specific FontFamily / FontStyle enums and embedded ttf data
//! that would create a font-metrics → d2-little cyclic dep.

use std::cell::RefCell;

use font_metrics_core::{Measured, Metrics};

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

impl Metrics for D2GoEmulationMetrics {
    fn measure(&self, text: &str, family: &str, size: f64, bold: bool, italic: bool) -> Measured {
        let font = Font {
            family: family_from_str(family),
            style: style_from(bold, italic),
            size: round_to_d2_size(size),
        };
        let mut ruler = self.inner.borrow_mut();
        let (width, _height) = ruler.measure_precise(font, text);
        let (ascent, descent) = ruler.face_metrics_for(font);
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
        let (w, _h) = ruler.measure_precise(font, "Hello");

        assert!(
            (m.width - w).abs() < 0.001,
            "width mismatch: metrics={}, ruler={}",
            m.width,
            w
        );
        assert!(m.ascent > 0.0, "ascent should be positive");
        assert!(m.descent > 0.0, "descent should be positive");
        assert!((m.ascent + m.descent) > 0.0, "line height should be positive");
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
}
