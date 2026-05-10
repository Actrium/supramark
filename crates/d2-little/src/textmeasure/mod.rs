//! Text measurement for d2 rendering.
//!
//! Hosts the byte-equal Go-upstream engine ([`D2GoEmulationRuler`]), the
//! markdown rendering helper ([`render_markdown`]), and the
//! [`default_metrics`] factory.
//!
//! d2 layout uses the concrete [`D2GoEmulationRuler`] type directly because
//! its `line_height_factor` is a stateful d2-internal concept that other
//! backends cannot cleanly substitute. The [`D2GoEmulationMetrics`] adapter
//! exposes the same engine through the cross-crate
//! [`font_metrics_core::Metrics`] trait, and is reserved for the wasm
//! production wiring (where the host bridge needs to plug into the same
//! abstract surface).

use font_metrics_core::Metrics;

use crate::fonts::{Font, FontFamily, FontStyle};

pub mod d2_emulation_metrics;
pub mod d2_go_emulation;

mod markdown;

pub use d2_emulation_metrics::D2GoEmulationMetrics;
pub use d2_go_emulation::D2GoEmulationRuler;

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
/// reproduction of Go upstream's freetype + Int26_6 path).
pub fn default_metrics() -> Result<D2GoEmulationRuler, String> {
    D2GoEmulationRuler::new()
}

/// Render markdown source to sanitised HTML. No font work involved.
pub fn render_markdown(input: &str) -> Result<String, String> {
    d2_go_emulation::render_markdown(input)
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
