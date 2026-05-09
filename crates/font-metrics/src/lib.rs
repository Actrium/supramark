//! Font metrics abstraction for the `*-little` family of port crates.
//!
//! # Why a trait
//!
//! plantuml-little / mermaid-little / d2-little all need to know
//! "given this string + this font + this size, how wide is the
//! resulting rendered text?" to lay out SVG diagrams. There are
//! three legitimate ways to answer that, and they belong on
//! different code paths:
//!
//! 1. **`TtfParserMetrics`** — parse a caller-supplied TTF buffer
//!    with `ttf-parser` and compute glyph advances at runtime. The
//!    production main path on native / SSR / wasm hosts that have no
//!    text-measurement bridge.
//!
//! 2. **`HostCallbackMetrics`** — defer measurement to a JS-side
//!    callback (e.g. `canvas.measureText` in browsers, RN-Skia
//!    `SkiaText.measureText` on React Native). The production main
//!    path inside a wasm host that has a real text renderer:
//!    measuring with the very font the host will render with
//!    eliminates Layer 1 / Layer 3 drift that no static table can
//!    fix.
//!
//! 3. **`StaticDejaVuMetrics`** (feature `static-fixtures`) —
//!    pre-computed range tables that match Java FontMetrics on
//!    DejaVu Sans / Mono / Serif byte-exactly. Used **only** by the
//!    upstream-byte-equal regression tests in plantuml-little and
//!    mermaid-little to verify the port still matches Java's output.
//!    Production code should not depend on it; the Java-flavoured
//!    numbers diverge from any browser's actual rendering anyway.
//!
//! # Layer 1 / 2 / 3 architecture (for context)
//!
//! - **Layer 1**: in-wasm layout (this crate). Needs metrics to
//!   compute box sizes, line wrap, alignment.
//! - **Layer 2**: emitted SVG, with `font-family="..."` strings.
//! - **Layer 3**: host renderer (browser, RN-svg, ImageMagick) that
//!   actually rasterises the SVG, using whatever font the host's OS
//!   has installed.
//!
//! `TtfParserMetrics` and `HostCallbackMetrics` solve the same
//! problem from opposite ends: the former lets layer 1 use a known
//! font that we ship; the latter lets layer 1 ask layer 3 directly.
//! Pick the one that matches your host environment.
//!
//! See `docs/architecture/SHARED_FONT_METRICS.md` (TODO) for the
//! full design rationale.

#![cfg_attr(docsrs, feature(doc_cfg))]

/// Measurement of a text run in a given font, in CSS pixels.
///
/// Mirrors the host canvas API's `measureText` return shape:
/// `width` from `TextMetrics.width`, `ascent` from
/// `actualBoundingBoxAscent`, `descent` from `actualBoundingBoxDescent`.
/// Other backends derive these from glyph metrics.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Measured {
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
}

/// Font metrics consumed by SVG diagram layout.
///
/// All measurements are in the same "user units" the size argument
/// is expressed in: if you pass `size = 14.0` for "14 px font",
/// `text_width` returns pixels.
///
/// `family` is matched case-insensitively after stripping any
/// CSS-style fallback list (`"Foo, sans-serif"` → `"Foo"`). Each
/// implementation chooses how to map family names to actual faces;
/// see the implementation docs for the resolution table.
pub trait Metrics {
    /// Measure a single line of text in the given font.
    ///
    /// This is the host's single source of truth: width + ascent + descent
    /// from one call. The 6 historical helpers (`text_width`, `line_height`,
    /// `ascent`, `descent`, etc.) are derivable from this and will be
    /// removed once caller code migrates (R2 onward).
    ///
    /// Pre-condition: `text` should not contain newlines — multi-line
    /// layout is the caller's responsibility (split + per-line measure).
    fn measure(&self, text: &str, family: &str, size: f64, bold: bool, italic: bool) -> Measured;

    /// Width of a single character (typographic horizontal advance).
    /// Returns `0.0` for `\n` and `\r`.
    fn char_width(&self, ch: char, family: &str, size: f64, bold: bool, italic: bool) -> f64;

    /// Total width of a text string, summed character-by-character.
    fn text_width(&self, text: &str, family: &str, size: f64, bold: bool, italic: bool) -> f64;

    /// Line height — typically `ascent + |descent|` for fonts whose
    /// `hhea.lineGap` is zero (DejaVu, most browser-installed fonts).
    fn line_height(&self, family: &str, size: f64, bold: bool, italic: bool) -> f64;

    /// Distance from baseline to top of the tallest glyph.
    fn ascent(&self, family: &str, size: f64, bold: bool, italic: bool) -> f64;

    /// Distance from baseline to bottom of the lowest glyph
    /// (positive value).
    fn descent(&self, family: &str, size: f64, bold: bool, italic: bool) -> f64;

    /// OS/2 typographic ascent (`OS/2.sTypoAscent`). Some
    /// upstream-Java diagram families use this instead of the hhea
    /// ascent for their text-block height calculations.
    fn typo_ascent(&self, family: &str, size: f64, bold: bool, italic: bool) -> f64;
}

#[cfg(feature = "static-fixtures")]
#[cfg_attr(docsrs, doc(cfg(feature = "static-fixtures")))]
pub mod static_dejavu;

pub mod ttf_parser;

#[cfg(target_arch = "wasm32")]
#[cfg_attr(docsrs, doc(cfg(target_arch = "wasm32")))]
pub mod host_callback;
