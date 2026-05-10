//! Dynamic font metrics via the [`ttf_parser`] crate.
//!
//! Production main path on native and SSR builds (and on wasm hosts
//! that do not provide a measurement callback). The caller supplies
//! TTF byte buffers for whatever fonts the diagram should render
//! with; per-call measurements parse glyph advances from those
//! buffers via `ttf-parser`.
//!
//! # Status
//!
//! Skeleton — the type and trait impl are in place so the
//! [`crate::Metrics`] trait has at least one always-on
//! implementation, but the methods currently return placeholder
//! values. Production wiring (default-DejaVu embedded subset, family
//! resolution table, kerning fallback) is filled in by a follow-up
//! pass — tracked on the same milestone as the
//! `host-callback`-bridge wiring.
//!
//! Once the implementation is complete, plantuml-little / mermaid-
//! little / d2-little will switch their main code paths from the
//! current static-tables route to this one. The static tables stay
//! around as a `static-fixtures` test-only build for upstream-byte-
//! equal regression tests.

use crate::{Measured, Metrics};
use ttf_parser::Face;

/// Dynamic [`Metrics`] backed by `ttf-parser`.
///
/// Holds parsed faces for sans / sans-bold / mono / mono-bold (with
/// italic / serif variants added as needed). The lifetime parameter
/// ties each face to the TTF byte buffer the caller passed in;
/// typically a `'static` buffer obtained via `include_bytes!()` or
/// loaded once at host init and pinned for the program lifetime.
pub struct TtfParserMetrics<'a> {
    sans: Face<'a>,
    sans_bold: Option<Face<'a>>,
    sans_italic: Option<Face<'a>>,
    sans_bold_italic: Option<Face<'a>>,
    mono: Option<Face<'a>>,
    mono_bold: Option<Face<'a>>,
    mono_italic: Option<Face<'a>>,
    mono_bold_italic: Option<Face<'a>>,
}

impl<'a> TtfParserMetrics<'a> {
    /// Construct a [`TtfParserMetrics`] with `sans` as the only
    /// available face. All other faces (bold, italic, mono, mono-bold,
    /// mono-italic, etc.) fall back to `sans` until they're populated
    /// via the builder methods.
    pub fn from_sans(sans_ttf: &'a [u8]) -> Result<Self, ttf_parser::FaceParsingError> {
        Ok(Self {
            sans: Face::parse(sans_ttf, 0)?,
            sans_bold: None,
            sans_italic: None,
            sans_bold_italic: None,
            mono: None,
            mono_bold: None,
            mono_italic: None,
            mono_bold_italic: None,
        })
    }

    /// Set the bold sans face. Returns `self` for chaining.
    pub fn with_sans_bold(mut self, ttf: &'a [u8]) -> Result<Self, ttf_parser::FaceParsingError> {
        self.sans_bold = Some(Face::parse(ttf, 0)?);
        Ok(self)
    }

    /// Set the italic sans face. Returns `self` for chaining.
    pub fn with_sans_italic(mut self, ttf: &'a [u8]) -> Result<Self, ttf_parser::FaceParsingError> {
        self.sans_italic = Some(Face::parse(ttf, 0)?);
        Ok(self)
    }

    /// Set the bold-italic sans face. Returns `self` for chaining.
    pub fn with_sans_bold_italic(
        mut self,
        ttf: &'a [u8],
    ) -> Result<Self, ttf_parser::FaceParsingError> {
        self.sans_bold_italic = Some(Face::parse(ttf, 0)?);
        Ok(self)
    }

    /// Set the mono face. Returns `self` for chaining.
    pub fn with_mono(mut self, ttf: &'a [u8]) -> Result<Self, ttf_parser::FaceParsingError> {
        self.mono = Some(Face::parse(ttf, 0)?);
        Ok(self)
    }

    /// Set the bold mono face. Returns `self` for chaining.
    pub fn with_mono_bold(mut self, ttf: &'a [u8]) -> Result<Self, ttf_parser::FaceParsingError> {
        self.mono_bold = Some(Face::parse(ttf, 0)?);
        Ok(self)
    }

    /// Set the italic mono face. Returns `self` for chaining.
    pub fn with_mono_italic(mut self, ttf: &'a [u8]) -> Result<Self, ttf_parser::FaceParsingError> {
        self.mono_italic = Some(Face::parse(ttf, 0)?);
        Ok(self)
    }

    /// Set the bold-italic mono face. Returns `self` for chaining.
    pub fn with_mono_bold_italic(
        mut self,
        ttf: &'a [u8],
    ) -> Result<Self, ttf_parser::FaceParsingError> {
        self.mono_bold_italic = Some(Face::parse(ttf, 0)?);
        Ok(self)
    }

    fn pick_face(&self, family: &str, bold: bool, italic: bool) -> &Face<'a> {
        let primary = family.split(',').next().unwrap_or(family).trim().to_lowercase();
        let is_mono = primary == "monospaced" || primary == "monospace" || primary == "courier";
        match (is_mono, bold, italic) {
            (true, true, true) => self
                .mono_bold_italic
                .as_ref()
                .or(self.mono_italic.as_ref())
                .or(self.mono_bold.as_ref())
                .or(self.mono.as_ref())
                .unwrap_or(&self.sans),
            (true, true, false) => self
                .mono_bold
                .as_ref()
                .or(self.mono.as_ref())
                .unwrap_or(&self.sans),
            (true, false, true) => self
                .mono_italic
                .as_ref()
                .or(self.mono.as_ref())
                .unwrap_or(&self.sans),
            (true, false, false) => self.mono.as_ref().unwrap_or(&self.sans),
            (false, true, true) => self
                .sans_bold_italic
                .as_ref()
                .or(self.sans_italic.as_ref())
                .or(self.sans_bold.as_ref())
                .unwrap_or(&self.sans),
            (false, true, false) => self.sans_bold.as_ref().unwrap_or(&self.sans),
            (false, false, true) => self.sans_italic.as_ref().unwrap_or(&self.sans),
            (false, false, false) => &self.sans,
        }
    }
}

impl TtfParserMetrics<'static> {
    /// Construct a [`TtfParserMetrics`] backed by an embedded DejaVu
    /// Latin subset (Sans / Sans-Bold / Mono / Mono-Bold), covering
    /// U+0020-U+007F and U+00A0-U+00FF. Each face is bundled via
    /// `include_bytes!`, so the returned value owns no external buffer
    /// and has `'static` lifetime.
    ///
    /// The subset weighs roughly 130 KB total (about 5x smaller than
    /// the full DejaVu set) and is intended as a zero-config fallback
    /// for callers that don't want to source their own TTFs. For
    /// non-Latin scripts or custom fonts, use
    /// [`TtfParserMetrics::from_sans`] with the desired byte buffer.
    ///
    /// The DejaVu fonts are released under the Bitstream Vera Fonts
    /// Copyright + Public Domain dual licence; see
    /// `crates/font-metrics/assets/` and the repo-root `REUSE.toml`
    /// for attribution.
    pub fn default_latin() -> Result<Self, ttf_parser::FaceParsingError> {
        const SANS: &[u8] = include_bytes!("../assets/dejavu-sans-latin.ttf");
        const SANS_BOLD: &[u8] = include_bytes!("../assets/dejavu-sans-bold-latin.ttf");
        const SANS_ITALIC: &[u8] = include_bytes!("../assets/dejavu-sans-italic-latin.ttf");
        const SANS_BOLD_ITALIC: &[u8] =
            include_bytes!("../assets/dejavu-sans-bolditalic-latin.ttf");
        const MONO: &[u8] = include_bytes!("../assets/dejavu-mono-latin.ttf");
        const MONO_BOLD: &[u8] = include_bytes!("../assets/dejavu-mono-bold-latin.ttf");
        const MONO_ITALIC: &[u8] = include_bytes!("../assets/dejavu-mono-italic-latin.ttf");
        const MONO_BOLD_ITALIC: &[u8] =
            include_bytes!("../assets/dejavu-mono-bolditalic-latin.ttf");
        Self::from_sans(SANS)?
            .with_sans_bold(SANS_BOLD)?
            .with_sans_italic(SANS_ITALIC)?
            .with_sans_bold_italic(SANS_BOLD_ITALIC)?
            .with_mono(MONO)?
            .with_mono_bold(MONO_BOLD)?
            .with_mono_italic(MONO_ITALIC)?
            .with_mono_bold_italic(MONO_BOLD_ITALIC)
    }
}

/// Glyph advance for a single character on a resolved face, in user units.
///
/// Returns `0.0` for `\n` and `\r`; falls back to the space advance for
/// unmapped glyphs, then to `size * 0.6` if the face lacks a space glyph.
fn char_advance(face: &Face<'_>, ch: char, size: f64) -> f64 {
    if ch == '\n' || ch == '\r' {
        return 0.0;
    }
    let upem = face.units_per_em() as f64;
    if let Some(gid) = face.glyph_index(ch) {
        if let Some(adv) = face.glyph_hor_advance(gid) {
            return adv as f64 / upem * size;
        }
    }
    if let Some(gid) = face.glyph_index(' ') {
        if let Some(adv) = face.glyph_hor_advance(gid) {
            return adv as f64 / upem * size;
        }
    }
    size * 0.6
}

impl<'a> Metrics for TtfParserMetrics<'a> {
    /// Single source of truth: computes width + ascent + descent
    /// directly from face data. Going through the trait helpers would
    /// recurse — they default-impl back to `measure`.
    fn measure(&self, text: &str, family: &str, size: f64, bold: bool, italic: bool) -> Measured {
        let face = self.pick_face(family, bold, italic);
        let upem = face.units_per_em() as f64;
        let asc = face.ascender() as f64 / upem * size;
        let desc = face.descender().unsigned_abs() as f64 / upem * size;
        let width: f64 = text.chars().map(|c| char_advance(face, c, size)).sum();
        Measured {
            width,
            ascent: asc,
            descent: desc,
        }
    }

    /// Override: `ttf_parser::Face::typographic_ascender()` reads
    /// `OS/2.sTypoAscent` when present and may differ from
    /// `hhea.ascent`. The default impl (which equals `ascent`) would
    /// lose that distinction.
    fn typo_ascent(&self, family: &str, size: f64, bold: bool, italic: bool) -> f64 {
        let face = self.pick_face(family, bold, italic);
        let typo = face.typographic_ascender().unwrap_or_else(|| face.ascender());
        typo as f64 / face.units_per_em() as f64 * size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_latin_basic_smoke() {
        let m = TtfParserMetrics::default_latin().expect("Latin TTF parse");
        let w = m.text_width("Hello", "sans-serif", 14.0, false, false);
        assert!(w > 20.0 && w < 50.0, "expected ~31px, got {}", w);
        let h = m.line_height("sans-serif", 14.0, false, false);
        assert!(h > 12.0 && h < 22.0, "expected ~16px, got {}", h);
    }

    #[test]
    fn italic_returns_distinct_metrics() {
        let m = TtfParserMetrics::default_latin().expect("init");
        // DejaVu Oblique faces share horizontal advances with their upright
        // siblings (they only slant glyphs), so we cannot rely on width
        // differences. Instead, prove that pick_face truly resolves to a
        // distinct italic face by querying its `italic_angle()` — the upright
        // face reports 0.0, the oblique face reports a non-zero slant. This
        // catches the regression where italic queries fell back to the
        // upright face and returned non-oblique metrics.
        let plain_face = m.pick_face("sans-serif", false, false);
        let italic_face = m.pick_face("sans-serif", false, true);
        let plain_angle = plain_face.italic_angle().unwrap_or(0.0);
        let italic_angle = italic_face.italic_angle().unwrap_or(0.0);
        assert_eq!(
            plain_angle, 0.0,
            "upright sans face should have zero italic angle, got {plain_angle}",
        );
        assert!(
            italic_angle.abs() > 0.001,
            "italic sans face should have non-zero italic angle, got {italic_angle}",
        );
    }
}
