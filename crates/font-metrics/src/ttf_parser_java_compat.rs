//! Java AWT FontMetrics-compatible italic-width adjustment.
//!
//! Wraps an inner [`TtfParserMetrics`] and applies Java AWT's italic-skew
//! horizontal projection: `width += line_height * tan(italic_angle)`.
//!
//! Use this impl when the downstream rendering pipeline is Java AWT-derived
//! (e.g., plantuml's byte-equal Java FontMetrics regression suite) and the
//! upstream values include italic-skew adjustment baked in. Use [`TtfParserMetrics`]
//! directly for raw ttf advance (the canvas-measureText-style baseline).
//!
//! The italic angle is read from the resolved face's `italic_angle()` (degrees,
//! converted to radians for `.tan()`). For DejaVu Sans Oblique this is roughly
//! -8° to -12°. The adjustment uses `abs()` since slant direction doesn't affect
//! horizontal projection magnitude.

use crate::ttf_parser::TtfParserMetrics;
use crate::{Measured, Metrics};

/// `Metrics` impl that wraps `TtfParserMetrics` and applies Java AWT-style
/// italic-skew adjustment. See module docs for when to use.
pub struct TtfParserJavaCompatMetrics<'a> {
    inner: TtfParserMetrics<'a>,
}

impl<'a> TtfParserJavaCompatMetrics<'a> {
    /// Construct from an existing TtfParserMetrics. Wraps it; consumes ownership.
    pub fn new(inner: TtfParserMetrics<'a>) -> Self {
        Self { inner }
    }
}

impl TtfParserJavaCompatMetrics<'static> {
    /// Convenience: wrap [`TtfParserMetrics::default_latin`].
    pub fn default_latin() -> Result<Self, ttf_parser::FaceParsingError> {
        Ok(Self::new(TtfParserMetrics::default_latin()?))
    }
}

impl<'a> Metrics for TtfParserJavaCompatMetrics<'a> {
    fn measure(&self, text: &str, family: &str, size: f64, bold: bool, italic: bool) -> Measured {
        let raw = self.inner.measure(text, family, size, bold, italic);
        if !italic {
            return raw;
        }
        // Java AWT-style adjustment: extra horizontal projection from slant.
        // tan(italic_angle_in_radians) — `ttf-parser` reports the angle in
        // degrees; `Option::None` (no `post` table) means treat as upright.
        let face = self.inner.face_for(family, bold, italic);
        let angle_deg = face.italic_angle().unwrap_or(0.0) as f64;
        let angle_rad = angle_deg.to_radians();
        let line_height = raw.ascent + raw.descent;
        let extra = line_height * angle_rad.tan().abs();
        Measured {
            width: raw.width + extra,
            ascent: raw.ascent,
            descent: raw.descent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn italic_widens_vs_raw_ttf() {
        let raw = TtfParserMetrics::default_latin().expect("raw init");
        let java = TtfParserJavaCompatMetrics::default_latin().expect("java init");
        let raw_italic = raw.measure("Hello", "sans-serif", 14.0, false, true);
        let java_italic = java.measure("Hello", "sans-serif", 14.0, false, true);
        assert!(
            java_italic.width > raw_italic.width,
            "JavaCompat italic should widen vs raw: raw={}, java={}",
            raw_italic.width,
            java_italic.width,
        );
    }

    #[test]
    fn non_italic_matches_raw() {
        // Non-italic queries are pass-through (no Java adjustment applies).
        let raw = TtfParserMetrics::default_latin().expect("raw init");
        let java = TtfParserJavaCompatMetrics::default_latin().expect("java init");
        let r = raw.measure("Hello", "sans-serif", 14.0, false, false);
        let j = java.measure("Hello", "sans-serif", 14.0, false, false);
        assert!((r.width - j.width).abs() < 0.001);
        assert!((r.ascent - j.ascent).abs() < 0.001);
        assert!((r.descent - j.descent).abs() < 0.001);
    }

    #[test]
    fn italic_widening_amount_is_reasonable() {
        // Sanity-check the formula against a hand-computed expectation.
        // For DejaVu Sans Oblique, italic_angle is roughly -9° to -11°;
        // line_height for 14pt is ~16-17px; extra ≈ 16 * tan(9°) ≈ 2.5 px.
        let raw = TtfParserMetrics::default_latin().expect("raw init");
        let java = TtfParserJavaCompatMetrics::default_latin().expect("java init");
        let r = raw.measure("Hello", "sans-serif", 14.0, false, true);
        let j = java.measure("Hello", "sans-serif", 14.0, false, true);
        let extra = j.width - r.width;
        let line_h = r.ascent + r.descent;
        // tolerate a wide range — formula validation, not exact byte-equal
        assert!(
            extra > 0.5 && extra < line_h * 0.3,
            "italic widening should be sane: extra={}, line_h={}",
            extra,
            line_h,
        );
    }
}
