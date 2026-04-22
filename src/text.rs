//! Text helpers — CJK double-width classification etc.
//!
//! Vendored from plantuml-little (https://github.com/kookyleo/plantuml-little)
//! at commit b32d6aa, MIT-compatible multi-license.

/// Estimate display width of a string, treating CJK characters as double-width.
///
/// ASCII and most Latin characters count as 1 unit; CJK ideographs and
/// fullwidth forms count as 2 units. This gives a much better width
/// estimate than `str::len()` (byte count) for mixed-script text.
pub fn display_width(s: &str) -> usize {
    s.chars().map(|c| if is_cjk(c) { 2 } else { 1 }).sum()
}

/// Returns `true` if the character is a CJK ideograph or fullwidth form
/// that typically occupies two columns in a monospace font.
pub fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{2E80}'..='\u{9FFF}'   | // CJK Radicals, Kangxi, Ideographic, Kana, Bopomofo, Hangul, CJK Unified
        '\u{F900}'..='\u{FAFF}'   | // CJK Compatibility Ideographs
        '\u{FE30}'..='\u{FE4F}'   | // CJK Compatibility Forms
        '\u{FF01}'..='\u{FF60}'   | // Fullwidth Latin, Punctuation, Katakana
        '\u{FFE0}'..='\u{FFE6}'   | // Fullwidth Signs (cent, pound, yen, etc.)
        '\u{20000}'..='\u{2FA1F}'   // CJK Unified Ideographs Extension B and beyond
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_only() {
        assert_eq!(display_width("hello"), 5);
    }

    #[test]
    fn empty_string() {
        assert_eq!(display_width(""), 0);
    }

    #[test]
    fn single_ascii() {
        assert_eq!(display_width("A"), 1);
    }

    #[test]
    fn cjk_chinese() {
        // 3 CJK ideographs (U+4F60 U+597D U+4E16) -> display width 6
        assert_eq!(display_width("\u{4F60}\u{597D}\u{4E16}"), 6);
    }

    #[test]
    fn cjk_japanese_hiragana_katakana() {
        // Hiragana/Katakana are in 0x3040..0x30FF, within the 0x2E80..0x9FFF range.
        // Hiragana a/i/u (U+3042 U+3044 U+3046)
        assert_eq!(display_width("\u{3042}\u{3044}\u{3046}"), 6);
        // Katakana a/i/u (U+30A2 U+30A4 U+30A6)
        assert_eq!(display_width("\u{30A2}\u{30A4}\u{30A6}"), 6);
    }

    #[test]
    fn cjk_korean() {
        // Hangul syllables are in 0xAC00..0xD7AF, outside the current range
        // but Hangul Jamo compatibility (0x3130..0x318F) is within 0x2E80..0x9FFF.
        // Full Hangul syllables are U+AC00..U+D7AF which is NOT in our range
        // — they are handled separately in East Asian Width but for simplicity
        // we only cover the most common CJK blocks. This test documents the behavior.
        // Hangul compatibility Jamo (U+3131 U+3134 U+3137) ARE in range:
        assert_eq!(display_width("\u{3131}\u{3134}\u{3137}"), 6);
    }

    #[test]
    fn mixed_ascii_cjk() {
        // "Hello" + two CJK ideographs = 5 ASCII + 2*2 CJK = 9
        assert_eq!(display_width("Hello\u{4E16}\u{754C}"), 9);
    }

    #[test]
    fn fullwidth_forms() {
        // Fullwidth 'A' is U+FF21
        assert_eq!(display_width("\u{FF21}"), 2);
        // Fullwidth '!' is U+FF01
        assert_eq!(display_width("\u{FF01}"), 2);
    }

    #[test]
    fn cjk_compatibility_ideographs() {
        // U+F900 is CJK Compatibility Ideograph
        assert_eq!(display_width("\u{F900}"), 2);
    }

    #[test]
    fn cjk_extension_b() {
        // U+20000 is CJK Unified Ideographs Extension B
        assert_eq!(display_width("\u{20000}"), 2);
    }

    #[test]
    fn latin_accented() {
        // Accented Latin chars are NOT CJK, should be width 1.
        // "caf" + U+00E9 (e with acute) = 4
        assert_eq!(display_width("caf\u{00E9}"), 4);
    }

    #[test]
    fn emoji_basic() {
        // Basic emoji (U+1F600) is outside CJK ranges, treated as width 1.
        // True terminal-width emoji handling would need a full Unicode East Asian Width
        // table, but for our monospace-font SVG layout this is acceptable.
        assert_eq!(display_width("\u{1F600}"), 1);
    }

    #[test]
    fn mixed_multiline_single_line() {
        // display_width works on a single line; callers split by newline first.
        // But if called with newline chars, '\n' is width 1 (non-CJK).
        assert_eq!(display_width("ab\ncd"), 5); // 'a','b','\n','c','d'
    }

    #[test]
    fn fullwidth_yen_sign() {
        // U+FFE5 (Fullwidth Yen Sign)
        assert_eq!(display_width("\u{FFE5}"), 2);
    }

    #[test]
    fn cjk_radicals() {
        // U+2E80 (CJK Radical Repeat)
        assert_eq!(display_width("\u{2E80}"), 2);
    }

    #[test]
    fn long_mixed_string() {
        // "Test" + 2 CJK + "Demo" + 2 CJK = 4 + 4 + 4 + 4 = 16
        assert_eq!(
            display_width("Test\u{6D4B}\u{8BD5}Demo\u{6F14}\u{793A}"),
            16
        );
    }
}
