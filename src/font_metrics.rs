//! Font metrics computed from pre-extracted static DejaVu font data.
//!
//! Uses [`crate::font_data`] lookup tables instead of runtime TTF parsing.
//! Font metric values match Java PlantUML exactly (same font files, same math:
//! `raw_units / units_per_em * size`).

use crate::font_data::{
    FontMeta, DEJAVU_MONO, DEJAVU_MONO_BOLD, DEJAVU_MONO_BOLD_OBLIQUE, DEJAVU_MONO_OBLIQUE,
    DEJAVU_SANS, DEJAVU_SANS_BOLD, DEJAVU_SANS_BOLD_OBLIQUE, DEJAVU_SANS_OBLIQUE, DEJAVU_SERIF,
    DEJAVU_SERIF_BOLD, DEJAVU_SERIF_BOLD_ITALIC, DEJAVU_SERIF_ITALIC,
};

// ── Font family resolution ──────────────────────────────────────────────

/// Logical font family bucket.
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

/// Map a logical font family name to a canonical key.
/// Java logical fonts: "SansSerif"/"Dialog"→ DejaVu Sans, "Monospaced"/"Courier"→ DejaVu Sans Mono,
/// "Serif"→ DejaVu Serif (used by creole headings).
/// Physical fonts not installed on the reference machine (e.g. "Courier New", "Arial")
/// fall back to Dialog (sans-serif) in Java AWT.
/// For CSS `font-family` lists like "Courier New,monospace", we resolve based on
/// the PRIMARY (first) name — Java AWT uses the first name for font lookup.
///
/// Italic variants resolve to the corresponding `*-Oblique.ttf` (Sans/Mono) or
/// `*-Italic.ttf` (Serif) faces — DejaVu ships real italic glyphs whose
/// horizontal advances differ slightly from plain. Reference SVGs are
/// generated on systems where Java has the italic faces available, so we
/// follow the same path here for byte-exact comparison.
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

// ── Public API (signatures preserved from previous implementation) ───────

/// Width of a single character in the given font configuration.
///
/// Computes `glyph_hor_advance / units_per_em * size`, matching Java's
/// `font.getStringBounds(ch, frc).getWidth()` with `FRACTIONALMETRICS_ON`.
pub fn char_width(ch: char, family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    if ch == '\n' || ch == '\r' {
        return 0.0;
    }
    let face = resolve_face(family, bold, italic);
    let upem = face.units_per_em as f64;
    if let Some(adv) = face.glyph_advance(ch as u32) {
        return adv as f64 / upem * size;
    }
    // Fallback: use space advance for unmapped characters
    if let Some(sp_adv) = face.glyph_advance(' ' as u32) {
        return sp_adv as f64 / upem * size;
    }
    size * 0.6 // last-resort fallback
}

/// Total width of a text string (sum of character advances).
pub fn text_width(text: &str, family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    text.chars()
        .map(|c| char_width(c, family, size, bold, italic))
        .sum()
}

/// Line height = ascent + |descent| (leading is 0 for DejaVu fonts).
///
/// Matches Java's `LineMetrics.getHeight()`.
///
/// Vertical metrics are face-dependent — DejaVu Sans/Mono share asc=1901/desc=-483
/// across plain and bold, but DejaVu Serif Bold/BoldItalic raise the ascender to
/// 1923. Java picks the value from the actual rendered face, so we do too.
pub fn line_height(family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    let face = resolve_face(family, bold, italic);
    let upem = face.units_per_em as f64;
    let asc = face.ascender as f64; // positive (hhea.ascender)
    let desc = face.descender.unsigned_abs() as f64; // make positive
    (asc + desc) / upem * size
}

/// Font ascent (baseline to top of tallest glyph).
///
/// Matches Java's `LineMetrics.getAscent()`.
pub fn ascent(family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    let face = resolve_face(family, bold, italic);
    face.ascender as f64 / face.units_per_em as f64 * size
}

/// Font descent (baseline to bottom of lowest glyph).
///
/// Matches Java's `LineMetrics.getDescent()` (positive value).
pub fn descent(family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    let face = resolve_face(family, bold, italic);
    face.descender.unsigned_abs() as f64 / face.units_per_em as f64 * size
}

/// OS/2 typographic ascent. Used for DOT cluster label dimensions which match
/// Java's `StringBounder.calculateDimension()` text block height.
pub fn typo_ascent(family: &str, size: f64, bold: bool, italic: bool) -> f64 {
    let face = resolve_face(family, bold, italic);
    let upem = face.units_per_em as f64;
    let typo_asc = face.typo_ascender as f64;
    typo_asc / upem * size
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Java ground truth (FRACTIONALMETRICS_ON, DejaVu Sans):
    // SansSerif 12 PLAIN: ascent=11.1386718750 descent=2.8300781250 height=13.9687500000
    // SansSerif 13 PLAIN: ascent=12.0668945313 descent=3.0659179688 height=15.1328125000
    // SansSerif 18 PLAIN: ascent=16.7080078125 descent=4.2451171875 height=20.9531250000
    // charW('W') at 12 = 11.8652343750
    // width('foo1') at 12 = 26.5429687500

    #[test]
    fn ascent_matches_java() {
        let a12 = ascent("SansSerif", 12.0, false, false);
        let a13 = ascent("SansSerif", 13.0, false, false);
        let a18 = ascent("SansSerif", 18.0, false, false);
        assert!((a12 - 11.1386718750).abs() < 1e-6, "a12={a12}");
        assert!((a13 - 12.0668945313).abs() < 1e-6, "a13={a13}");
        assert!((a18 - 16.7080078125).abs() < 1e-6, "a18={a18}");
    }

    #[test]
    fn descent_matches_java() {
        let d12 = descent("SansSerif", 12.0, false, false);
        assert!((d12 - 2.8300781250).abs() < 1e-6, "d12={d12}");
    }

    #[test]
    fn line_height_matches_java() {
        let h12 = line_height("SansSerif", 12.0, false, false);
        let h13 = line_height("SansSerif", 13.0, false, false);
        let h18 = line_height("SansSerif", 18.0, false, false);
        assert!((h12 - 13.9687500000).abs() < 1e-6, "h12={h12}");
        assert!((h13 - 15.1328125000).abs() < 1e-6, "h13={h13}");
        assert!((h18 - 20.9531250000).abs() < 1e-6, "h18={h18}");
    }

    #[test]
    fn char_width_w_matches_java() {
        let w = char_width('W', "SansSerif", 12.0, false, false);
        assert!((w - 11.8652343750).abs() < 1e-6, "W width={w}");
    }

    #[test]
    fn text_width_foo1_matches_java() {
        let w = text_width("foo1", "SansSerif", 12.0, false, false);
        assert!((w - 26.5429687500).abs() < 1e-4, "foo1 width={w}");
    }

    #[test]
    fn monospaced_metrics() {
        // All monospaced chars should have equal advance width
        let w_a = char_width('a', "Monospaced", 13.0, false, false);
        let w_w = char_width('W', "Monospaced", 13.0, false, false);
        assert!(
            (w_a - w_w).abs() < 1e-6,
            "mono: a={w_a} W={w_w} should be equal"
        );
    }

    #[test]
    fn bold_width_differs() {
        let w_plain = char_width('W', "SansSerif", 12.0, false, false);
        let w_bold = char_width('W', "SansSerif", 12.0, true, false);
        assert!(w_bold > w_plain, "bold W should be wider");
    }

    #[test]
    fn italic_uses_oblique_metrics() {
        // DejaVu Sans Oblique has different per-glyph advances than the plain
        // face for several glyphs (real italic, not synthetic shear). Java
        // PlantUML on systems with the Oblique TTF returns the Oblique
        // measurements from `getStringBounds`; we follow the same path so
        // reference SVGs compare byte-exact.
        //
        // Java ground truth (FRACTIONALMETRICS_ON, DejaVu Sans Oblique 14pt):
        //   getStringBounds("«archimate-node»")            = 128.3857
        //   getStringBounds("«archimate-business-process»") = 213.6230
        let w1 = text_width("«archimate-node»", "SansSerif", 14.0, false, true);
        let w2 = text_width(
            "«archimate-business-process»",
            "SansSerif",
            14.0,
            false,
            true,
        );
        assert!((w1 - 128.3857).abs() < 0.01, "italic w1={w1}");
        assert!((w2 - 213.6230).abs() < 0.01, "italic w2={w2}");

        // Plain face still returns its own (smaller) advances.
        let p1 = text_width("«archimate-node»", "SansSerif", 14.0, false, false);
        assert!((p1 - 128.2354).abs() < 0.01, "plain w1={p1}");
        assert!(w1 > p1, "italic should differ from plain");
    }

    #[test]
    fn family_resolution() {
        // "Monospaced" (Java logical name) resolves to mono font
        let w_mono = char_width('a', "Monospaced", 12.0, false, false);
        let w_monospace = char_width('a', "monospace", 12.0, false, false);
        assert!((w_mono - w_monospace).abs() < 1e-10);
        // "Courier" (Java logical font, no "New") maps to Monospaced
        let w_courier = char_width('a', "Courier", 12.0, false, false);
        assert!((w_mono - w_courier).abs() < 1e-10, "Courier maps to mono");
        // "Courier New" is a physical font not installed on reference machine
        // → Java Dialog fallback → sans-serif
        let w_courier_new = char_width('a', "Courier New", 12.0, false, false);
        let w_sans = char_width('a', "SansSerif", 12.0, false, false);
        assert!(
            (w_courier_new - w_sans).abs() < 1e-10,
            "Courier New maps to sans (Dialog fallback)"
        );
        // "SansSerif", "Dialog", "Arial" all resolve to sans font
        let w3 = char_width('a', "SansSerif", 12.0, false, false);
        let w4 = char_width('a', "Dialog", 12.0, false, false);
        assert!((w3 - w4).abs() < 1e-10);
    }

    #[test]
    fn arbitrary_size_works() {
        // Size 15 was not in the old lookup table — runtime computation handles any size
        let h = line_height("SansSerif", 15.0, false, false);
        assert!(h > 0.0);
        assert!((h - (1901.0 + 483.0) / 2048.0 * 15.0).abs() < 1e-6);
    }

    #[test]
    fn text_width_matches_java_reference() {
        // Verify text_width matches Java PlantUML's getStringBounds for various strings
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
            let our_w = text_width(text, "SansSerif", *size, *bold, false);
            assert!(
                (our_w - java_w).abs() < 0.001,
                "text_width(\"{text}\", size={size}, bold={bold}): ours={our_w:.4}, java={java_w:.4}"
            );
        }
    }
}
