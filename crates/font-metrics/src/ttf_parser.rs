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
    mono: Option<Face<'a>>,
    mono_bold: Option<Face<'a>>,
}

impl<'a> TtfParserMetrics<'a> {
    /// Construct a [`TtfParserMetrics`] with `sans` as the only
    /// available face. All other faces (bold, mono, mono-bold)
    /// fall back to `sans` until they're populated via the builder
    /// methods.
    pub fn from_sans(sans_ttf: &'a [u8]) -> Result<Self, ttf_parser::FaceParsingError> {
        Ok(Self {
            sans: Face::parse(sans_ttf, 0)?,
            sans_bold: None,
            mono: None,
            mono_bold: None,
        })
    }

    /// Set the bold sans face. Returns `self` for chaining.
    pub fn with_sans_bold(mut self, ttf: &'a [u8]) -> Result<Self, ttf_parser::FaceParsingError> {
        self.sans_bold = Some(Face::parse(ttf, 0)?);
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

    fn pick_face(&self, family: &str, bold: bool) -> &Face<'a> {
        let primary = family.split(',').next().unwrap_or(family).trim().to_lowercase();
        let is_mono = primary == "monospaced" || primary == "monospace" || primary == "courier";
        match (is_mono, bold) {
            (true, true) => self.mono_bold.as_ref().or(self.mono.as_ref()).unwrap_or(&self.sans),
            (true, false) => self.mono.as_ref().unwrap_or(&self.sans),
            (false, true) => self.sans_bold.as_ref().unwrap_or(&self.sans),
            (false, false) => &self.sans,
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
        const MONO: &[u8] = include_bytes!("../assets/dejavu-mono-latin.ttf");
        const MONO_BOLD: &[u8] = include_bytes!("../assets/dejavu-mono-bold-latin.ttf");
        Self::from_sans(SANS)?
            .with_sans_bold(SANS_BOLD)?
            .with_mono(MONO)?
            .with_mono_bold(MONO_BOLD)
    }
}

impl<'a> Metrics for TtfParserMetrics<'a> {
    fn measure(&self, text: &str, family: &str, size: f64, bold: bool, italic: bool) -> Measured {
        Measured {
            width: self.text_width(text, family, size, bold, italic),
            ascent: self.ascent(family, size, bold, italic),
            descent: self.descent(family, size, bold, italic),
        }
    }

    fn char_width(&self, ch: char, family: &str, size: f64, bold: bool, _italic: bool) -> f64 {
        if ch == '\n' || ch == '\r' {
            return 0.0;
        }
        let face = self.pick_face(family, bold);
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

    fn text_width(&self, text: &str, family: &str, size: f64, bold: bool, italic: bool) -> f64 {
        text.chars()
            .map(|c| self.char_width(c, family, size, bold, italic))
            .sum()
    }

    fn line_height(&self, family: &str, size: f64, bold: bool, _italic: bool) -> f64 {
        let face = self.pick_face(family, bold);
        let upem = face.units_per_em() as f64;
        let asc = face.ascender() as f64;
        let desc = face.descender().unsigned_abs() as f64;
        (asc + desc) / upem * size
    }

    fn ascent(&self, family: &str, size: f64, bold: bool, _italic: bool) -> f64 {
        let face = self.pick_face(family, bold);
        face.ascender() as f64 / face.units_per_em() as f64 * size
    }

    fn descent(&self, family: &str, size: f64, bold: bool, _italic: bool) -> f64 {
        let face = self.pick_face(family, bold);
        face.descender().unsigned_abs() as f64 / face.units_per_em() as f64 * size
    }

    fn typo_ascent(&self, family: &str, size: f64, bold: bool, italic: bool) -> f64 {
        // ttf-parser's typographic_ascender() reads OS/2.sTypoAscent
        // when present; falls back to hhea.ascent otherwise.
        let face = self.pick_face(family, bold);
        let typo = face.typographic_ascender().unwrap_or_else(|| face.ascender());
        let _ = italic;
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
}
