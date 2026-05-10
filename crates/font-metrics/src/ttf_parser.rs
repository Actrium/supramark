//! Dynamic font metrics via the [`ttf_parser`] crate.
//!
//! Production main path on native and SSR builds (and on wasm hosts
//! that do not provide a measurement callback). The caller supplies
//! TTF byte buffers for whatever fonts the diagram should render
//! with; per-call measurements parse glyph advances from those
//! buffers via `ttf-parser`.
//!
//! plantuml-little / mermaid-little / d2-little select this impl via
//! their `metrics-ttf-parser` feature for both native production
//! builds that want dynamic metrics without browser/RN bridging AND
//! their byte-equal upstream-Java regression suites: a 2026-05-10
//! measurement spike confirmed raw glyph advances from
//! [`TtfParserMetrics::default_latin`] match Java FontMetrics to
//! sub-0.0001 px on the discriminating italic test (raw italic
//! `«archimate-node»` = 128.385742 px vs Java 128.3857 px,
//! delta = 0.000042 px), so no italic-skew wrapper is needed.

use crate::{Measured, Metrics};
use ttf_parser::Face;

/// Behaviour when measuring a character that is not in the font's `cmap`.
///
/// Different upstreams use different conventions for "missing glyph"
/// width, and a single hard-coded choice cannot satisfy both byte-equal
/// regression suites:
///
/// - **`Notdef`** matches Java AWT `FontMetrics`: missing chars use the
///   `.notdef` glyph (gid 0) advance, which is typically the box-shaped
///   placeholder. plantuml-little (port of PlantUML/Java) uses this so
///   strings with chars outside the embedded subset reproduce the
///   upstream Java widths.
/// - **`Space`** matches the historical StaticDejaVu range-table
///   behaviour and Chrome canvas's effective fallback when no system
///   font covers the codepoint. mermaid-little (port of mermaid.js +
///   canvas) uses this to keep its `*_byte_exact.rs` reference suite
///   aligned with the canvas-recorded reference output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingGlyphFallback {
    /// Use the `.notdef` glyph (gid 0) advance. Matches Java AWT.
    Notdef,
    /// Use the space (' ') glyph advance. Matches canvas / StaticDejaVu.
    Space,
}

impl Default for MissingGlyphFallback {
    /// Default to `.notdef`, matching Java AWT — the more common port
    /// source across the current `*-little` family.
    fn default() -> Self {
        Self::Notdef
    }
}

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
    missing_glyph_fallback: MissingGlyphFallback,
}

impl<'a> TtfParserMetrics<'a> {
    /// Construct a [`TtfParserMetrics`] with `sans` as the only
    /// available face. All other faces (bold, italic, mono, mono-bold,
    /// mono-italic, etc.) fall back to `sans` until they're populated
    /// via the builder methods.
    ///
    /// The missing-glyph fallback policy defaults to
    /// [`MissingGlyphFallback::Notdef`]; override with
    /// [`Self::with_missing_glyph_fallback`] if your upstream uses a
    /// different convention (e.g. canvas / StaticDejaVu space-fallback).
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
            missing_glyph_fallback: MissingGlyphFallback::default(),
        })
    }

    /// Override the missing-glyph fallback policy. Returns `self` for
    /// chaining; see [`MissingGlyphFallback`] for the rationale behind
    /// each variant.
    pub fn with_missing_glyph_fallback(mut self, policy: MissingGlyphFallback) -> Self {
        self.missing_glyph_fallback = policy;
        self
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
    /// Latin subset (Sans / Sans-Bold / Sans-Italic / Sans-BoldItalic
    /// + the four Mono variants). Each face is bundled via
    /// `include_bytes!`, so the returned value owns no external buffer
    /// and has `'static` lifetime.
    ///
    /// Codepoint coverage spans Basic Latin, Latin-1 Supplement, Latin
    /// Extended-A/B, IPA Extensions, Spacing Modifier Letters,
    /// Combining Diacriticals, Greek, Cyrillic, General Punctuation,
    /// Super-/Subscripts, Currency, Letterlike Symbols, Number Forms,
    /// Arrows, Math Operators, Misc Technical, Box Drawing, Block
    /// Elements, Geometric Shapes, Misc Symbols, and Dingbats — broad
    /// enough to byte-equal upstream Java/canvas widths on the full
    /// plantuml + mermaid + d2 reference suites without dropping into
    /// the [`MissingGlyphFallback`] path.
    ///
    /// The bundled subsets total ~970 KB across the 8 faces (about 4x
    /// smaller than the full DejaVu set) and are intended as a
    /// zero-config fallback for callers that don't want to source
    /// their own TTFs. For non-Latin scripts (CJK, emoji) or custom
    /// fonts, use [`TtfParserMetrics::from_sans`] with the desired
    /// byte buffer.
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
/// Returns `0.0` for `\n` and `\r`; for unmapped characters, applies the
/// caller-selected [`MissingGlyphFallback`] policy, then falls back to
/// `size * 0.6` if even the chosen fallback glyph has no advance entry.
///
/// See [`MissingGlyphFallback`] for the rationale behind each policy
/// (`.notdef` for Java AWT parity, space for canvas / StaticDejaVu
/// parity).
fn char_advance(face: &Face<'_>, ch: char, size: f64, fallback: MissingGlyphFallback) -> f64 {
    if ch == '\n' || ch == '\r' {
        return 0.0;
    }
    let upem = face.units_per_em() as f64;
    if let Some(gid) = face.glyph_index(ch) {
        if let Some(adv) = face.glyph_hor_advance(gid) {
            return adv as f64 / upem * size;
        }
    }
    let fallback_gid = match fallback {
        MissingGlyphFallback::Notdef => ttf_parser::GlyphId(0),
        MissingGlyphFallback::Space => face.glyph_index(' ').unwrap_or(ttf_parser::GlyphId(0)),
    };
    if let Some(adv) = face.glyph_hor_advance(fallback_gid) {
        return adv as f64 / upem * size;
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
        let fallback = self.missing_glyph_fallback;
        let width: f64 = text.chars().map(|c| char_advance(face, c, size, fallback)).sum();
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
    fn missing_glyph_notdef_fallback() {
        // Default policy is Notdef (Java AWT parity). U+1F600 GRINNING FACE
        // is intentionally outside the embedded Latin/symbols subset and
        // must therefore be measured through the `.notdef` advance, NOT
        // the space advance. For DejaVu Sans @ 14pt the two differ by ~3.95
        // px, so a single byte-precise assertion is enough to lock the
        // policy.
        let m = TtfParserMetrics::default_latin().expect("init");
        let space = m.measure(" ", "sans-serif", 14.0, false, false).width;
        let emoji = m.measure("😀", "sans-serif", 14.0, false, false).width;
        assert!(
            (emoji - space).abs() > 0.01,
            "Notdef fallback expected; got space={space}, emoji={emoji}",
        );
    }

    #[test]
    fn missing_glyph_space_fallback() {
        // Opting into Space policy makes missing-glyph chars (like the
        // grinning-face emoji) measure as space-wide. This is the
        // canvas / StaticDejaVu convention used by mermaid-little.
        let m = TtfParserMetrics::default_latin()
            .expect("init")
            .with_missing_glyph_fallback(MissingGlyphFallback::Space);
        let space = m.measure(" ", "sans-serif", 14.0, false, false).width;
        let emoji = m.measure("😀", "sans-serif", 14.0, false, false).width;
        assert!(
            (emoji - space).abs() < 1e-9,
            "Space fallback expected; got space={space}, emoji={emoji}",
        );
    }

    #[test]
    fn extended_latin_and_symbols_resolve_to_real_glyphs() {
        let m = TtfParserMetrics::default_latin().expect("init");
        let space = m.measure(" ", "sans-serif", 14.0, false, false).width;
        // These chars MUST resolve to non-space-fallback widths after 4b.
        for ch in ['ā', 'ē', '€', '∞', '≤', '—', '…', '★'] {
            let s = ch.to_string();
            let w = m.measure(&s, "sans-serif", 14.0, false, false).width;
            assert!(
                (w - space).abs() > 0.001,
                "char '{}' should have a real glyph width, got space-fallback {}",
                ch, w,
            );
        }
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
