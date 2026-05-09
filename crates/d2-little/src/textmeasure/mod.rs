//! Text measurement for d2 rendering.
//!
//! This module hosts the [`TextMetrics`] trait — the abstract interface every
//! text-measurement backend must implement — together with the public
//! markdown helpers ([`render_markdown`], [`measure_markdown`]) and the
//! [`default_metrics`] factory that hands out the byte-equal Go-upstream
//! engine ([`D2GoEmulationRuler`]).
//!
//! Future backends (host callbacks, ttf-parser fallback) will live next to
//! [`d2_go_emulation`] and slot into the same trait without touching call
//! sites in `lib.rs` / `target.rs` / `svg_render`.

use std::any::Any;

use crate::fonts::Font;

pub mod d2_go_emulation;

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

/// Abstract text-measurement interface used by the d2 pipeline.
///
/// The trait carries an [`Any`] supertrait so the markdown helper layer
/// (currently only the byte-equal Go engine) can downcast to its concrete
/// implementation. Once additional backends are introduced, the markdown
/// helpers will move to a trait-only contract.
pub trait TextMetrics: Any {
    fn measure(&mut self, font: Font, s: &str) -> (i32, i32);
    fn measure_mono(&mut self, font: Font, s: &str) -> (i32, i32);
    fn measure_precise(&mut self, font: Font, s: &str) -> (f64, f64);
    fn line_height_factor(&self) -> f64;
    fn set_line_height_factor(&mut self, value: f64);
}

/// Construct the default text-measurement backend (currently the byte-equal
/// reproduction of Go upstream's freetype + Int26_6 path).
pub fn default_metrics() -> Result<Box<dyn TextMetrics>, String> {
    Ok(Box::new(D2GoEmulationRuler::new()?))
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

/// Measure a markdown blob and return the rendered (width, height) in pixels.
///
/// The measurement currently dispatches to [`D2GoEmulationRuler`] only.
/// Other backends will be plumbed in as the trait grows.
pub fn measure_markdown(
    md_text: &str,
    metrics: &mut dyn TextMetrics,
    font_family: Option<crate::fonts::FontFamily>,
    mono_font_family: Option<crate::fonts::FontFamily>,
    font_size: i32,
) -> Result<(i32, i32), String> {
    let any_ref: &mut dyn Any = metrics;
    let ruler = any_ref
        .downcast_mut::<D2GoEmulationRuler>()
        .ok_or_else(|| {
            "measure_markdown currently requires D2GoEmulationRuler; future \
             backends will register their own markdown path"
                .to_string()
        })?;
    d2_go_emulation::measure_markdown_inner(
        md_text,
        ruler,
        font_family,
        mono_font_family,
        font_size,
    )
}
