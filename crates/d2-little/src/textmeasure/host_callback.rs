//! HostCallbackRuler — TextMetrics impl that defers to the host's
//! globalThis.supramark.measureText bridge via font_metrics_core::HostCallbackMetrics.
//!
//! Wasm-only. The module is gated at the `mod` declaration in
//! `super::mod`; no inner `#![cfg]` is needed (and adding one would
//! trip `clippy::duplicated_attributes`). Selector wiring lives in 3c.

use super::TextMetrics;
use crate::fonts::{Font, FontFamily, FontStyle};
use font_metrics_core::Metrics;
use font_metrics_core::host_callback::HostCallbackMetrics;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Text-measurement backend that dispatches every measurement through the
/// host-supplied `globalThis.supramark.measureText` bridge.
///
/// The struct is intentionally small: the wrapped [`HostCallbackMetrics`]
/// is stateless and zero-cost; the only owned state is the line-height
/// factor (matching [`crate::textmeasure::d2_go_emulation::D2GoEmulationRuler`]'s
/// shape so the two impls are interchangeable through the `TextMetrics`
/// trait).
pub struct HostCallbackRuler {
    line_height_factor: f64,
    inner: HostCallbackMetrics,
}

impl HostCallbackRuler {
    pub fn new() -> Self {
        Self {
            line_height_factor: 1.0,
            inner: HostCallbackMetrics,
        }
    }
}

impl Default for HostCallbackRuler {
    fn default() -> Self {
        Self::new()
    }
}

fn family_str(family: FontFamily) -> &'static str {
    match family {
        FontFamily::SourceSansPro => "Source Sans Pro",
        FontFamily::SourceCodePro => "Source Code Pro",
        FontFamily::HandDrawn => "Fuzzy Bubbles",
    }
}

fn is_bold(style: FontStyle) -> bool {
    matches!(style, FontStyle::Bold | FontStyle::Semibold)
}

fn is_italic(style: FontStyle) -> bool {
    matches!(style, FontStyle::Italic)
}

impl TextMetrics for HostCallbackRuler {
    fn measure(&mut self, font: Font, s: &str) -> (i32, i32) {
        let (w, h) = self.measure_precise(font, s);
        (w.ceil() as i32, h.ceil() as i32)
    }

    fn measure_mono(&mut self, font: Font, s: &str) -> (i32, i32) {
        let mono_font = Font {
            family: FontFamily::SourceCodePro,
            style: font.style,
            size: font.size,
        };
        self.measure(mono_font, s)
    }

    fn measure_precise(&mut self, font: Font, s: &str) -> (f64, f64) {
        let family = family_str(font.family);
        let size = font.size as f64;
        let bold = is_bold(font.style);
        let italic = is_italic(font.style);

        let line_count = s.split('\n').count().max(1);
        let widest = s
            .split('\n')
            .map(|line| self.inner.text_width(line, family, size, bold, italic))
            .fold(0.0_f64, f64::max);

        let line_h = self.inner.line_height(family, size, bold, italic) * self.line_height_factor;
        let width = self.scale_unicode(widest, font, s);
        let height = line_h * line_count as f64;
        (width, height)
    }

    fn space_width(&mut self, font: Font) -> f64 {
        let family = family_str(font.family);
        let size = font.size as f64;
        let bold = is_bold(font.style);
        let italic = is_italic(font.style);
        self.inner.text_width(" ", family, size, bold, italic)
    }

    fn scale_unicode(&mut self, w: f64, font: Font, s: &str) -> f64 {
        // CJK width-fallback heuristic: replace the host's Latin-fallback
        // width with `mono_space * unicode_width(grapheme)` for any
        // non-1-cell grapheme. Mirrors the d2 Go upstream logic that
        // `D2GoEmulationRuler::scale_unicode` already encodes (without the
        // Atlas state machine — the host bridge already returned a
        // host-true width, but we still need the per-grapheme adjustment
        // when the host's font is Latin-only Source Sans Pro and the
        // glyph it picked for a wide CJK codepoint is the typical narrow
        // .notdef / fallback-square shape).
        let grapheme_count = s.graphemes(true).count();
        if grapheme_count == s.len() {
            return w;
        }

        let mono_font = Font {
            family: FontFamily::SourceCodePro,
            style: font.style,
            size: font.size,
        };
        let mono_space = self.space_width(mono_font);

        let family = family_str(font.family);
        let size = font.size as f64;
        let bold = is_bold(font.style);
        let italic = is_italic(font.style);

        let mut max_w = 0.0_f64;
        for line in s.split('\n') {
            let mut adjusted = self.inner.text_width(line, family, size, bold, italic);
            for grapheme in line.graphemes(true) {
                let unicode_w = UnicodeWidthStr::width(grapheme);
                if unicode_w == 1 {
                    continue;
                }
                let measured = self.inner.text_width(grapheme, family, size, bold, italic);
                adjusted -= measured;
                adjusted += mono_space * unicode_w as f64;
            }
            max_w = max_w.max(adjusted);
        }
        max_w
    }

    fn line_height_factor(&self) -> f64 {
        self.line_height_factor
    }

    fn set_line_height_factor(&mut self, value: f64) {
        self.line_height_factor = value;
    }

    // measure_markdown: defaults to Err from the trait. 3b.3 will port
    // the markdown layout to be backend-agnostic; for now, callers using
    // HostCallbackRuler with markdown shapes get a clear error.
}
