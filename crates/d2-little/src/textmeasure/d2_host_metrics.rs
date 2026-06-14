//! D2HostMetrics — wasm-only [`super::D2Metrics`] adapter backed by
//! [`HostCallbackMetrics`] (the host's `canvas.measureText` bridge).
//!
//! Per the 32bb55ee spike: host canvas measureText has no
//! Go-style `prevR` width-leak across `\n` (every measureText call is
//! independent), so we reproduce d2's multi-line layout entirely caller
//! side:
//!
//! - **width**  = max over per-line `measure_precise(line).width`
//! - **height** = `single_line_h + (n - 1) * line_height_factor *
//!   font_size`
//!
//! This is intentionally a structural emulation rather than a byte-equal
//! reproduction: matching Go's per-character `prevR` carry-over inside
//! `drawBuf` would require a Rust port of the freetype Int26_6 path on
//! top of the host font (which the host doesn't expose). The trade-off
//! is documented in
//! `super::d2_emulation_metrics::tests::spike_multiline_decomposition`:
//! up to ~1 px width drift on certain prev/next char pairs, accepted in
//! exchange for layer-1 = layer-3 consistency in the host's actual font
//! stack.
//!
//! `measure_markdown` drives the same trait-generic walker as the native
//! `D2GoEmulationMetrics::measure_markdown` impl: save the current
//! `line_height_factor`, set it to `MARKDOWN_LINE_HEIGHT`, walk the
//! markdown tree (which calls back into our `measure_precise` /
//! `space_width` / `scale_unicode` / `set_line_height_factor` for each
//! node), then restore the saved factor. The walker handles header /
//! `<pre>` line-height switches via the same per-node save/set/restore
//! pattern the native path uses. Layer-3 SVG rendering goes through the
//! host browser's font stack, matching layer-1 measurement (the same
//! ~1 px upstream-Go drift on multi-line / mixed-width text applies as
//! it does for plain `measure_precise`; see module preface).

#![cfg(target_arch = "wasm32")]

use std::cell::Cell;

use font_metrics_core::{Measured, Metrics, host_callback::HostCallbackMetrics};

use super::{D2Metrics, MarkdownOptions};
use crate::fonts::{Font, FontFamily, FontStyle};

/// Adapter that bridges d2's [`D2Metrics`] surface to the host
/// `canvas.measureText` callback. See module docs for the multi-line
/// semantics.
pub struct D2HostMetrics {
    inner: HostCallbackMetrics,
    line_height_factor: Cell<f64>,
}

impl Default for D2HostMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl D2HostMetrics {
    pub fn new() -> Self {
        Self {
            inner: HostCallbackMetrics,
            line_height_factor: Cell::new(1.0),
        }
    }
}

fn font_to_family_str(family: FontFamily) -> &'static str {
    match family {
        FontFamily::SourceSansPro => "Source Sans Pro",
        FontFamily::SourceCodePro => "Source Code Pro",
        FontFamily::HandDrawn => "Fuzzy Bubbles",
    }
}

fn font_to_bold_italic(font: Font) -> (bool, bool) {
    let bold = matches!(font.style, FontStyle::Bold | FontStyle::Semibold);
    let italic = matches!(font.style, FontStyle::Italic);
    (bold, italic)
}

impl Metrics for D2HostMetrics {
    /// Single-line semantics. Multi-line input is delegated to the
    /// caller-side composition in [`D2Metrics::measure_text`] /
    /// [`D2Metrics::measure_precise`]; for parity with the upstream
    /// `Metrics` contract we treat embedded `\n` as a request to
    /// measure the longest line (width) + the first-line height
    /// (ascent / descent).
    fn measure(&self, text: &str, family: &str, size: f64, bold: bool, italic: bool) -> Measured {
        if !text.contains('\n') {
            return self.inner.measure(text, family, size, bold, italic);
        }
        let mut max_w = 0.0_f64;
        let mut firsts: Option<Measured> = None;
        for (i, line) in text.split('\n').enumerate() {
            let m = self.inner.measure(line, family, size, bold, italic);
            max_w = max_w.max(m.width);
            if i == 0 {
                firsts = Some(m);
            }
        }
        let firsts = firsts.unwrap_or(Measured {
            width: 0.0,
            ascent: size * 0.8,
            descent: size * 0.2,
        });
        Measured {
            width: max_w,
            ascent: firsts.ascent,
            descent: firsts.descent,
        }
    }
}

impl D2Metrics for D2HostMetrics {
    fn line_height_factor(&self) -> f64 {
        self.line_height_factor.get()
    }

    fn set_line_height_factor(&self, value: f64) {
        self.line_height_factor.set(value);
    }

    fn measure_text(&self, font: Font, s: &str) -> (i32, i32) {
        let (w, h) = self.measure_precise(font, s);
        (w.ceil() as i32, h.ceil() as i32)
    }

    fn measure_mono(&self, font: Font, s: &str) -> (i32, i32) {
        // Force SourceCodePro family for mono measurement; matches the
        // d2-emulation contract in
        // `D2GoEmulationRuler::measure_mono`. `bounds_with_dot` semantics
        // (the Go ruler's "extend bounds to current dot" toggle) have no
        // direct host-canvas analog — caller-side composition already
        // accounts for trailing space because measureText includes it.
        let mono = Font {
            family: FontFamily::SourceCodePro,
            style: font.style,
            size: font.size,
        };
        self.measure_text(mono, s)
    }

    fn measure_precise(&self, font: Font, s: &str) -> (f64, f64) {
        let family = font_to_family_str(font.family);
        let (bold, italic) = font_to_bold_italic(font);
        let size_f = font.size as f64;

        if s.is_empty() {
            return (0.0, 0.0);
        }

        let lines: Vec<&str> = s.split('\n').collect();
        let n = lines.len();
        let max_w = lines
            .iter()
            .map(|l| self.inner.measure(l, family, size_f, bold, italic).width)
            .fold(0.0_f64, f64::max);
        let single_h = {
            let m = self.inner.measure(lines[0], family, size_f, bold, italic);
            m.ascent + m.descent
        };
        let composed_h = single_h + ((n - 1) as f64) * self.line_height_factor.get() * size_f;
        (max_w, composed_h)
    }

    fn space_width(&self, font: Font) -> f64 {
        let family = font_to_family_str(font.family);
        let (bold, italic) = font_to_bold_italic(font);
        self.inner
            .measure(" ", family, font.size as f64, bold, italic)
            .width
    }

    fn scale_unicode(&self, w: f64, _font: Font, _s: &str) -> f64 {
        // Host canvas measureText is grapheme-aware (the platform shapers
        // handle CJK / emoji directly), so the d2-emulation CJK
        // fallback heuristic isn't needed. Return the measured width
        // unchanged.
        w
    }

    fn measure_markdown(
        &self,
        md_text: &str,
        opts: MarkdownOptions,
        font_size: i32,
    ) -> Result<(i32, i32), String> {
        // Save / set / restore line_height_factor around the walker. The
        // walker re-borrows it through D2Metrics::set_line_height_factor
        // for header / `<pre>` per-node tweaks (same pattern the native
        // adapter uses). Host canvas has no `bounds_with_dot` analog, so
        // there's nothing to save on that axis.
        let original_lh = self.line_height_factor.get();
        self.line_height_factor
            .set(super::markdown::MARKDOWN_LINE_HEIGHT);
        let result = super::markdown::measure_markdown_generic(
            self,
            md_text,
            opts.font_family,
            opts.mono_font_family,
            font_size,
        );
        self.line_height_factor.set(original_lh);
        result
    }
}
