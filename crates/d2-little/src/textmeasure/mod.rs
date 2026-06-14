//! Text measurement for d2 rendering.
//!
//! Hosts the byte-equal Go-upstream engine ([`D2GoEmulationRuler`]), the
//! markdown rendering helper ([`render_markdown`]), the [`default_metrics`]
//! factory, and the d2-internal [`D2Metrics`] sub-trait that the lib.rs
//! layout pipeline dispatches through.
//!
//! d2 layout uses [`D2Metrics`] (an extension of
//! [`font_metrics_core::Metrics`]) so the wasm production path can swap in
//! a [`D2HostMetrics`](d2_host_metrics::D2HostMetrics) backed by the host
//! `canvas.measureText` bridge while native / regression tests keep using
//! [`D2GoEmulationMetrics`] for byte-equal Go output. The d2-specific
//! `line_height_factor` save/restore idiom stays available through the
//! sub-trait without leaking into the cross-crate `Metrics` surface.

use font_metrics_core::Metrics;

use crate::fonts::{Font, FontFamily, FontStyle};

// The byte-equal Go ruler + its Metrics adapter pull in Atlas / freetype
// emulation / fontTools-equivalent code that totals ~2000 LOC and has no
// consumer on wasm (default_d2_metrics() picks D2HostMetrics there).
// Gating these modules out of wasm shrinks the bundle without changing
// any reachable code path.
#[cfg(not(target_arch = "wasm32"))]
pub mod d2_emulation_metrics;
#[cfg(not(target_arch = "wasm32"))]
pub mod d2_go_emulation;
#[cfg(target_arch = "wasm32")]
pub mod d2_host_metrics;

// Markdown walker is target-agnostic — pure traversal over &dyn D2Metrics.
// Both D2GoEmulationMetrics::measure_markdown (native, byte-equal Go) and
// D2HostMetrics::measure_markdown (wasm, host-canvas via caller-side
// composition) drive this same walker; they each save/set/restore their
// own line_height_factor state around the call.
mod markdown;
mod markdown_render;

pub use markdown_render::render_markdown;

#[cfg(not(target_arch = "wasm32"))]
pub use d2_emulation_metrics::D2GoEmulationMetrics;
#[cfg(not(target_arch = "wasm32"))]
pub use d2_go_emulation::D2GoEmulationRuler;
#[cfg(target_arch = "wasm32")]
pub use d2_host_metrics::D2HostMetrics;

// ---------------------------------------------------------------------------
// MarkdownOptions — explicit-shape carrier for measure_markdown arguments.
// Mirrors the existing 4-tuple so future backends (e.g. host wasm) can take
// it as a single value rather than a tower of Options.
// ---------------------------------------------------------------------------

/// Options for [`D2Metrics::measure_markdown`]. Mirrors
/// [`D2GoEmulationRuler::measure_markdown`]'s argument shape.
#[derive(Debug, Clone, Copy, Default)]
pub struct MarkdownOptions {
    pub font_family: Option<FontFamily>,
    pub mono_font_family: Option<FontFamily>,
}

// ---------------------------------------------------------------------------
// D2Metrics — d2-internal extension of font_metrics_core::Metrics.
//
// The cross-crate Metrics trait is intentionally pure (single-line measure
// only). d2's layout pipeline additionally needs:
//   1. `line_height_factor` get/set — stateful, d2-specific (mermaid /
//      plantuml have no equivalent; keeping it here means the cross-crate
//      trait stays uncontaminated).
//   2. d2 `Font` enum convenience accessors — every callsite already has a
//      `Font`, so reshaping to (family_str, bold, italic) on the spot is
//      noisy.
//   3. `measure_markdown` — d2-specific markdown-to-box layout. The native
//      path delegates to the byte-equal Go emulation; the wasm
//      D2HostMetrics adapter currently returns Err pending a separate
//      port.
//
// All stateful methods take `&self` (interior mutability via RefCell /
// Cell in implementors) so callers don't need `&mut`.
// ---------------------------------------------------------------------------

/// d2-internal extension over [`font_metrics_core::Metrics`].
///
/// d2 layout's `set_dimensions` / markdown walker dispatch through this
/// trait. Two implementors:
/// - [`D2GoEmulationMetrics`] — native + byte-equal Go regression path.
/// - [`D2HostMetrics`](d2_host_metrics::D2HostMetrics) — wasm path
///   bridging `canvas.measureText` (target_arch = "wasm32" only).
pub trait D2Metrics: Metrics {
    /// Current line-height factor used when advancing past a `\n`.
    fn line_height_factor(&self) -> f64;
    /// Override the line-height factor. Callers usually save the previous
    /// value and restore it after the temporary override.
    fn set_line_height_factor(&self, value: f64);

    /// Integer-rounded measurement convenience: ceil(width), ceil(height).
    /// Renamed from plain `measure` to avoid shadowing the
    /// [`Metrics::measure`] super-trait method (which has a different,
    /// string-based family signature). All d2 layout callsites use this
    /// name.
    fn measure_text(&self, font: Font, s: &str) -> (i32, i32);
    /// Measure with `bounds_with_dot=true` and the SourceCodePro family.
    fn measure_mono(&self, font: Font, s: &str) -> (i32, i32);
    /// Floating-point measurement: (width, height) without ceil.
    fn measure_precise(&self, font: Font, s: &str) -> (f64, f64);
    /// Width of the space glyph in the supplied font.
    fn space_width(&self, font: Font) -> f64;
    /// CJK width-fallback heuristic: replace measured Latin-fallback width
    /// with `space_width(mono) * unicode_width` for non-1-cell graphemes.
    fn scale_unicode(&self, w: f64, font: Font, s: &str) -> f64;

    /// Measure a markdown blob and return the rendered (width, height).
    /// Native path drives the byte-equal Go emulation walker; wasm path
    /// returns Err (Phase 3 follow-up).
    fn measure_markdown(
        &self,
        md_text: &str,
        opts: MarkdownOptions,
        font_size: i32,
    ) -> Result<(i32, i32), String>;
}

/// Default font size used when measuring markdown content.
pub const MARKDOWN_FONT_SIZE: i32 = crate::fonts::FONT_SIZE_M;

/// Line-height factor used when measuring code blocks (shape: code with
/// language / fenced code). Mirrors Go `textmeasure.CODE_LINE_HEIGHT`.
pub const CODE_LINE_HEIGHT: f64 = 1.3;

const H1_EM: f64 = 2.0;
const H2_EM: f64 = 1.5;
const H3_EM: f64 = 1.25;
const H4_EM: f64 = 1.0;
const H5_EM: f64 = 0.875;
const H6_EM: f64 = 0.85;

/// Construct the default d2 text-measurement engine (the byte-equal
/// reproduction of Go upstream's freetype + Int26_6 path). Native only —
/// wasm builds do not embed the byte-equal Go ruler. For the trait-object
/// equivalent that works on both targets, use [`default_d2_metrics`].
#[cfg(not(target_arch = "wasm32"))]
pub fn default_metrics() -> Result<D2GoEmulationRuler, String> {
    D2GoEmulationRuler::new()
}

/// Construct the default [`D2Metrics`] backend.
///
/// On native targets returns a [`D2GoEmulationMetrics`] (byte-equal Go
/// upstream layout). On wasm targets returns a
/// [`D2HostMetrics`](d2_host_metrics::D2HostMetrics) backed by the host
/// `canvas.measureText` bridge.
pub fn default_d2_metrics() -> Result<Box<dyn D2Metrics>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        Ok(Box::new(d2_host_metrics::D2HostMetrics::new()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(Box::new(D2GoEmulationMetrics::new()?))
    }
}

/// Resolve an HTML header tag (`h1` … `h6`) to its scaled font size.
pub fn header_to_font_size(base_font_size: i32, header: &str) -> i32 {
    match header {
        "h1" => (H1_EM * f64::from(base_font_size)) as i32,
        "h2" => (H2_EM * f64::from(base_font_size)) as i32,
        "h3" => (H3_EM * f64::from(base_font_size)) as i32,
        "h4" => (H4_EM * f64::from(base_font_size)) as i32,
        "h5" => (H5_EM * f64::from(base_font_size)) as i32,
        "h6" => (H6_EM * f64::from(base_font_size)) as i32,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// d2 layout helpers built on top of `font_metrics_core::Metrics`.
//
// Free functions that bridge d2's native `Font` enum to the cross-crate
// `Metrics` trait (`HostCallbackMetrics` on wasm, future ttf-parser
// fallback, ...). Reserved for the wasm production wiring; d2's internal
// layout pipeline still drives `D2GoEmulationRuler` directly because its
// stateful `line_height_factor` cannot be cleanly externalised.
// ---------------------------------------------------------------------------

/// Map a d2 `Font` to (family_str, bold, italic) for trait dispatch.
fn font_to_trait_args(font: Font) -> (&'static str, bool, bool) {
    let family = match font.family {
        FontFamily::SourceSansPro => "Source Sans Pro",
        FontFamily::SourceCodePro => "Source Code Pro",
        FontFamily::HandDrawn => "Fuzzy Bubbles",
    };
    let bold = matches!(font.style, FontStyle::Bold | FontStyle::Semibold);
    let italic = matches!(font.style, FontStyle::Italic);
    (family, bold, italic)
}

/// d2 layout `measure(font, s) -> (i32, i32)` derived from a Metrics backend.
/// Equivalent to `D2GoEmulationRuler::measure(font, s)` when backed by
/// `D2GoEmulationMetrics`.
pub fn d2_measure(metrics: &dyn Metrics, font: Font, s: &str) -> (i32, i32) {
    let (w, h) = d2_measure_precise(metrics, font, s);
    (w.ceil() as i32, h.ceil() as i32)
}

/// d2 layout `measure_mono(font, s) -> (i32, i32)`. Forces SourceCodePro family.
pub fn d2_measure_mono(metrics: &dyn Metrics, font: Font, s: &str) -> (i32, i32) {
    let mono_font = Font {
        family: FontFamily::SourceCodePro,
        style: font.style,
        size: font.size,
    };
    d2_measure(metrics, mono_font, s)
}

/// d2 layout `measure_precise(font, s) -> (f64, f64)` derived from Metrics.
pub fn d2_measure_precise(metrics: &dyn Metrics, font: Font, s: &str) -> (f64, f64) {
    let (family, bold, italic) = font_to_trait_args(font);
    let m = metrics.measure(s, family, font.size as f64, bold, italic);
    (m.width, m.ascent + m.descent)
}

/// d2 layout `space_width(font) -> f64` — width of a single space character.
pub fn d2_space_width(metrics: &dyn Metrics, font: Font) -> f64 {
    let (family, bold, italic) = font_to_trait_args(font);
    metrics
        .measure(" ", family, font.size as f64, bold, italic)
        .width
}

/// d2 layout `scale_unicode` — CJK fallback: replace Latin-fallback width with
/// mono space × cell count. Mirrors `D2GoEmulationRuler::scale_unicode` shape
/// but works via the `Metrics` trait method only.
pub fn d2_scale_unicode(metrics: &dyn Metrics, w: f64, font: Font, s: &str) -> f64 {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;

    let grapheme_count = s.graphemes(true).count();
    if grapheme_count == s.len() {
        return w;
    }

    let (family, bold, italic) = font_to_trait_args(font);
    let size_f = font.size as f64;
    let mono_font = Font {
        family: FontFamily::SourceCodePro,
        style: font.style,
        size: font.size,
    };
    let mono_space = d2_space_width(metrics, mono_font);

    let mut max_w = 0.0_f64;
    for line in s.split('\n') {
        let mut adjusted = metrics.measure(line, family, size_f, bold, italic).width;
        for grapheme in line.graphemes(true) {
            let unicode_w = UnicodeWidthStr::width(grapheme);
            if unicode_w == 1 {
                continue;
            }
            let measured = metrics
                .measure(grapheme, family, size_f, bold, italic)
                .width;
            adjusted -= measured;
            adjusted += mono_space * unicode_w as f64;
        }
        max_w = max_w.max(adjusted);
    }
    max_w
}
