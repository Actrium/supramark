//! Static DejaVu range-table metrics. **Test fixture only.**
//!
//! Pre-computed range tables giving byte-exact Java FontMetrics output
//! for DejaVu Sans / Mono / Serif × plain/bold/italic/bold-italic.
//! Generated offline by `crates/plantuml-little/tools/gen_font_data.py`
//! from the upstream DejaVu TTFs.
//!
//! This module is **gated behind the `static-fixtures` feature** and
//! is only meant to be enabled in dev-dependencies of consumers that
//! run upstream-byte-equal regression tests. Production code paths
//! should use [`crate::ttf_parser::TtfParserMetrics`] or
//! [`crate::host_callback::HostCallbackMetrics`] so layout reflects
//! the fonts the host actually renders with.
//!
//! # Provenance
//!
//! The range tables are a derived work of the DejaVu fonts (Bitstream
//! Vera + Public Domain dual licence). Downstream consumers that
//! redistribute the compiled tables should preserve the DejaVu
//! attribution — see `REUSE.toml` and the upstream notice at
//! <https://dejavu-fonts.github.io/>.

pub mod font_data;

use crate::{Measured, Metrics};
use font_data::{
    FontMeta, DEJAVU_MONO, DEJAVU_MONO_BOLD, DEJAVU_MONO_BOLD_OBLIQUE, DEJAVU_MONO_OBLIQUE,
    DEJAVU_SANS, DEJAVU_SANS_BOLD, DEJAVU_SANS_BOLD_OBLIQUE, DEJAVU_SANS_OBLIQUE, DEJAVU_SERIF,
    DEJAVU_SERIF_BOLD, DEJAVU_SERIF_BOLD_ITALIC, DEJAVU_SERIF_ITALIC,
};

// ── Font family resolution ──────────────────────────────────────────────

/// Logical font family bucket recognised by the static-DejaVu impl.
#[derive(Copy, Clone, Eq, PartialEq)]
enum FamilyKind {
    Sans,
    Mono,
    Serif,
}

fn family_kind(family: &str) -> FamilyKind {
    let primary = family.split(',').next().unwrap_or(family).trim();
    let p = primary.to_lowercase();
    if p == "monospaced" || p == "monospace" || p == "courier" {
        FamilyKind::Mono
    } else if p == "serif" {
        // Only Java logical name "Serif" resolves to DejaVu Serif. Physical
        // serifs ("Times", "Times New Roman", "Georgia") aren't installed on
        // the Java reference machine and fall back to Dialog/Sans there.
        FamilyKind::Serif
    } else {
        FamilyKind::Sans
    }
}

/// Map a logical font family name + bold/italic flags to a canonical face.
///
/// Java logical fonts: `"SansSerif"` / `"Dialog"` → DejaVu Sans;
/// `"Monospaced"` / `"Courier"` → DejaVu Sans Mono; `"Serif"` →
/// DejaVu Serif. Physical fonts not installed on the reference
/// machine (e.g. `"Courier New"`, `"Arial"`) fall back to Dialog
/// (sans-serif) — that's what Java AWT does on the reference setup.
///
/// CSS-style font-family lists like `"Courier New, monospace"` are
/// resolved by the **primary** (first) name, matching Java AWT's
/// font-lookup order.
///
/// Italic variants resolve to the corresponding `*-Oblique.ttf`
/// (Sans / Mono) or `*-Italic.ttf` (Serif) faces. DejaVu ships real
/// italic glyphs whose horizontal advances differ slightly from
/// plain; reference SVGs are generated on systems where Java has the
/// italic faces available, so byte-exact comparison requires
/// matching that path here.
fn resolve_face(family: &str, bold: bool, italic: bool) -> &'static FontMeta {
    match (family_kind(family), bold, italic) {
        (FamilyKind::Mono, false, false) => &DEJAVU_MONO,
        (FamilyKind::Mono, true, false) => &DEJAVU_MONO_BOLD,
        (FamilyKind::Mono, false, true) => &DEJAVU_MONO_OBLIQUE,
        (FamilyKind::Mono, true, true) => &DEJAVU_MONO_BOLD_OBLIQUE,
        (FamilyKind::Sans, false, false) => &DEJAVU_SANS,
        (FamilyKind::Sans, true, false) => &DEJAVU_SANS_BOLD,
        (FamilyKind::Sans, false, true) => &DEJAVU_SANS_OBLIQUE,
        (FamilyKind::Sans, true, true) => &DEJAVU_SANS_BOLD_OBLIQUE,
        (FamilyKind::Serif, false, false) => &DEJAVU_SERIF,
        (FamilyKind::Serif, true, false) => &DEJAVU_SERIF_BOLD,
        (FamilyKind::Serif, false, true) => &DEJAVU_SERIF_ITALIC,
        (FamilyKind::Serif, true, true) => &DEJAVU_SERIF_BOLD_ITALIC,
    }
}

// ── Metrics implementation ───────────────────────────────────────────────

/// Byte-exact Java-FontMetrics-compatible measurement using
/// pre-extracted DejaVu range tables.
///
/// Stateless / zero-cost: an instance just dispatches to module-level
/// `static` lookup tables. Cheap to construct anywhere a
/// `&dyn Metrics` is needed.
#[derive(Debug, Clone, Copy, Default)]
pub struct StaticDejaVuMetrics;

/// Glyph advance for a single character on a resolved face, in user units.
///
/// Computes `glyph_hor_advance / units_per_em * size`, matching Java's
/// `font.getStringBounds(ch, frc).getWidth()` with `FRACTIONALMETRICS_ON`.
/// Returns `0.0` for `\n` and `\r`. Falls back to the space advance for
/// unmapped characters, then to `size * 0.6` if the face lacks a space
/// glyph (should not happen with DejaVu).
fn char_advance(face: &FontMeta, ch: char, size: f64) -> f64 {
    if ch == '\n' || ch == '\r' {
        return 0.0;
    }
    let upem = face.units_per_em as f64;
    if let Some(adv) = face.glyph_advance(ch as u32) {
        return adv as f64 / upem * size;
    }
    if let Some(sp_adv) = face.glyph_advance(' ' as u32) {
        return sp_adv as f64 / upem * size;
    }
    size * 0.6
}

impl Metrics for StaticDejaVuMetrics {
    /// Single source of truth: computes width + ascent + descent
    /// directly from face data, without going through the helper
    /// methods (which themselves derive from `measure` via default
    /// impls — calling them here would recurse).
    ///
    /// Vertical metrics are face-dependent — DejaVu Sans / Mono share
    /// `asc=1901, desc=-483` across plain and bold, but DejaVu Serif
    /// Bold / BoldItalic raise the ascender to 1923. Java picks the
    /// value from the actual rendered face, so we do too.
    fn measure(&self, text: &str, family: &str, size: f64, bold: bool, italic: bool) -> Measured {
        let face = resolve_face(family, bold, italic);
        let upem = face.units_per_em as f64;
        let asc = face.ascender as f64 / upem * size;
        let desc = face.descender.unsigned_abs() as f64 / upem * size;
        let width: f64 = text.chars().map(|c| char_advance(face, c, size)).sum();
        Measured {
            width,
            ascent: asc,
            descent: desc,
        }
    }

    /// Override: DejaVu's pre-extracted tables expose `OS/2.sTypoAscent`
    /// as a separate field that differs from `hhea.ascent` on some
    /// faces; the default impl (which equals `ascent`) would lose that
    /// distinction and break Java byte-equal regressions.
    fn typo_ascent(&self, family: &str, size: f64, bold: bool, italic: bool) -> f64 {
        let face = resolve_face(family, bold, italic);
        face.typo_ascender as f64 / face.units_per_em as f64 * size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Java ground truth (FRACTIONALMETRICS_ON, DejaVu Sans):
    //   SansSerif 12 PLAIN: ascent=11.1386718750 descent=2.8300781250 height=13.9687500000
    //   SansSerif 13 PLAIN: ascent=12.0668945313 descent=3.0659179688 height=15.1328125000
    //   SansSerif 18 PLAIN: ascent=16.7080078125 descent=4.2451171875 height=20.9531250000
    //   charW('W') at 12 = 11.8652343750
    //   width('foo1') at 12 = 26.5429687500

    const M: StaticDejaVuMetrics = StaticDejaVuMetrics;

    #[test]
    fn ascent_matches_java() {
        assert!((M.ascent("SansSerif", 12.0, false, false) - 11.1386718750).abs() < 1e-6);
        assert!((M.ascent("SansSerif", 13.0, false, false) - 12.0668945313).abs() < 1e-6);
        assert!((M.ascent("SansSerif", 18.0, false, false) - 16.7080078125).abs() < 1e-6);
    }

    #[test]
    fn descent_matches_java() {
        assert!((M.descent("SansSerif", 12.0, false, false) - 2.8300781250).abs() < 1e-6);
    }

    #[test]
    fn line_height_matches_java() {
        assert!((M.line_height("SansSerif", 12.0, false, false) - 13.9687500000).abs() < 1e-6);
        assert!((M.line_height("SansSerif", 13.0, false, false) - 15.1328125000).abs() < 1e-6);
        assert!((M.line_height("SansSerif", 18.0, false, false) - 20.9531250000).abs() < 1e-6);
    }

    #[test]
    fn char_width_w_matches_java() {
        assert!((M.char_width('W', "SansSerif", 12.0, false, false) - 11.8652343750).abs() < 1e-6);
    }

    #[test]
    fn text_width_foo1_matches_java() {
        assert!((M.text_width("foo1", "SansSerif", 12.0, false, false) - 26.5429687500).abs() < 1e-4);
    }

    #[test]
    fn monospaced_metrics() {
        let w_a = M.char_width('a', "Monospaced", 13.0, false, false);
        let w_w = M.char_width('W', "Monospaced", 13.0, false, false);
        assert!((w_a - w_w).abs() < 1e-6);
    }

    #[test]
    fn bold_width_differs() {
        let w_plain = M.char_width('W', "SansSerif", 12.0, false, false);
        let w_bold = M.char_width('W', "SansSerif", 12.0, true, false);
        assert!(w_bold > w_plain);
    }

    #[test]
    fn italic_uses_oblique_metrics() {
        // DejaVu Sans Oblique has different per-glyph advances than
        // the plain face for several glyphs. Java ground truth at 14pt:
        //   getStringBounds("«archimate-node»")             = 128.3857
        //   getStringBounds("«archimate-business-process»") = 213.6230
        let w1 = M.text_width("«archimate-node»", "SansSerif", 14.0, false, true);
        let w2 = M.text_width("«archimate-business-process»", "SansSerif", 14.0, false, true);
        assert!((w1 - 128.3857).abs() < 0.01);
        assert!((w2 - 213.6230).abs() < 0.01);

        // Plain face still returns its own (smaller) advances.
        let p1 = M.text_width("«archimate-node»", "SansSerif", 14.0, false, false);
        assert!((p1 - 128.2354).abs() < 0.01);
        assert!(w1 > p1);
    }

    #[test]
    fn family_resolution() {
        let w_mono = M.char_width('a', "Monospaced", 12.0, false, false);
        let w_monospace = M.char_width('a', "monospace", 12.0, false, false);
        assert!((w_mono - w_monospace).abs() < 1e-10);
        // "Courier" (Java logical, no "New") maps to Monospaced.
        let w_courier = M.char_width('a', "Courier", 12.0, false, false);
        assert!((w_mono - w_courier).abs() < 1e-10);
        // "Courier New" is a physical font absent from the reference
        // machine → Java Dialog fallback → sans-serif.
        let w_courier_new = M.char_width('a', "Courier New", 12.0, false, false);
        let w_sans = M.char_width('a', "SansSerif", 12.0, false, false);
        assert!((w_courier_new - w_sans).abs() < 1e-10);
        // "SansSerif", "Dialog", "Arial" all resolve to sans.
        let w3 = M.char_width('a', "SansSerif", 12.0, false, false);
        let w4 = M.char_width('a', "Dialog", 12.0, false, false);
        assert!((w3 - w4).abs() < 1e-10);
    }

    #[test]
    fn arbitrary_size_works() {
        let h = M.line_height("SansSerif", 15.0, false, false);
        assert!(h > 0.0);
        assert!((h - (1901.0 + 483.0) / 2048.0 * 15.0).abs() < 1e-6);
    }

    #[test]
    fn text_width_matches_java_reference() {
        // Verify text_width matches Java's getStringBounds on a
        // representative cross-section.
        let cases: &[(&str, f64, bool, f64)] = &[
            ("Alice", 14.0, false, 33.667),
            ("Bob", 14.0, false, 27.0566),
            ("Hello", 13.0, false, 32.9507),
            ("Test", 14.0, false, 29.9482),
            ("Grouping messages", 13.0, true, 144.5869),
            ("Swimlane1", 18.0, false, 98.6484),
            ("Action 1", 12.0, false, 49.2422),
        ];
        for (text, size, bold, java_w) in cases {
            let ours = M.text_width(text, "SansSerif", *size, *bold, false);
            assert!((ours - java_w).abs() < 0.001, "text=\"{text}\" size={size} bold={bold}: ours={ours:.4} java={java_w:.4}");
        }
    }
}
