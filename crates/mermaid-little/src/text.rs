//! Text helpers — CJK double-width classification etc.
//!
//! Vendored from plantuml-little (https://github.com/kookyleo/plantuml-little)
//! at commit b32d6aa, MIT-compatible multi-license.

/// Extract the plain-text content that `markdownToHTML` would produce from a
/// markdown-formatted string, matching what JSDOM's `el.textContent` returns
/// after mermaid renders the label.
///
/// `marked` (the markdown library used by upstream) processes:
/// - `**text**` and `__text__` → `<strong>text</strong>` → textContent = `text`
/// - `*text*` and `_text_` → `<em>text</em>` → textContent = `text`
/// - `` `code` `` → `<code>code</code>` → textContent = `code`
///
/// CommonMark rules for `_` delimiters: a `_` can open emphasis only when NOT
/// immediately preceded by an ASCII alphanumeric character (letter or digit),
/// and can close emphasis only when NOT immediately followed by one. This prevents
/// `word_with_underscores` from being treated as markdown. `*` delimiters are
/// processed unconditionally (as `marked` does).
///
/// HTML entities (e.g. `&lt;&lt;Requirement&gt;&gt;`) are passed through
/// unchanged — they are already the serialized form of the label text
/// and are measured as-is.
///
/// Precedence: process `**` and `__` (two-char markers) before `*` and `_`
/// (single-char) to avoid over-matching.
pub fn markdown_text_content(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut result = String::with_capacity(n);
    let mut i = 0;
    while i < n {
        // *** bold-italic *** — must check before ** to avoid ** consuming the
        // wrong closing marker (e.g. `***X***` would otherwise match `**` open
        // at 0 and closing `**` at 20, leaving inner `*X` with no closing `*`).
        if i + 2 < n && bytes[i] == b'*' && bytes[i + 1] == b'*' && bytes[i + 2] == b'*' {
            if let Some(end) = find_marker_str(s, i + 3, "***") {
                result.push_str(&markdown_text_content(&s[i + 3..end]));
                i = end + 3;
                continue;
            }
        }
        // ** bold ** — * delimiters are always processed
        if i + 1 < n && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            if let Some(end) = find_marker_str(s, i + 2, "**") {
                result.push_str(&markdown_text_content(&s[i + 2..end]));
                i = end + 2;
                continue;
            }
        }
        // __ bold __ — only when NOT preceded by alphanumeric (CommonMark rule)
        if i + 1 < n && bytes[i] == b'_' && bytes[i + 1] == b'_' {
            let preceded_by_alnum = i > 0 && bytes[i - 1].is_ascii_alphanumeric();
            if !preceded_by_alnum {
                if let Some(end) = find_closing_underscore2(s, i + 2) {
                    result.push_str(&markdown_text_content(&s[i + 2..end]));
                    i = end + 2;
                    continue;
                }
            }
            // If __ was rejected (preceded by alnum) or no matching __ was found,
            // output both underscores and skip them so the single-_ path doesn't
            // incorrectly process the second one.
            result.push('_');
            result.push('_');
            i += 2;
            continue;
        }
        // * italic * — * delimiters are always processed
        if bytes[i] == b'*' {
            if let Some(end) = find_marker_str(s, i + 1, "*") {
                if end > i + 1 {
                    result.push_str(&markdown_text_content(&s[i + 1..end]));
                    i = end + 1;
                    continue;
                }
            }
        }
        // _ italic _ — only when NOT preceded by alphanumeric (CommonMark rule).
        // Note: the __ case above already handled and consumed `__` sequences.
        if bytes[i] == b'_' {
            let preceded_by_alnum = i > 0 && bytes[i - 1].is_ascii_alphanumeric();
            if !preceded_by_alnum {
                if let Some(end) = find_closing_underscore1(s, i + 1) {
                    if end > i + 1 {
                        result.push_str(&markdown_text_content(&s[i + 1..end]));
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        // ` code `
        if bytes[i] == b'`' {
            if let Some(end) = find_marker_str(s, i + 1, "`") {
                result.push_str(&s[i + 1..end]);
                i = end + 1;
                continue;
            }
        }
        // Append one byte as a char (safe since we only check ASCII markers)
        // SAFETY: push char-by-char to handle multi-byte UTF-8 correctly
        let ch = s[i..].chars().next().unwrap_or('\0');
        result.push(ch);
        i += ch.len_utf8();
    }
    result
}

/// Find a closing `_` (single) that is not immediately followed by an ASCII
/// alphanumeric (CommonMark close-emphasis rule). Searches from `from` onwards
/// in the byte slice.
fn find_closing_underscore1(s: &str, from: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut pos = from;
    while pos < bytes.len() {
        if bytes[pos] == b'_' {
            // Closing `_` must not be followed by alphanumeric
            let followed_by_alnum = pos + 1 < bytes.len() && bytes[pos + 1].is_ascii_alphanumeric();
            if !followed_by_alnum {
                return Some(pos);
            }
        }
        pos += 1;
    }
    None
}

/// Find a closing `__` (double) where the second `_` is not followed by an
/// ASCII alphanumeric (CommonMark close-emphasis rule).
fn find_closing_underscore2(s: &str, from: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut pos = from;
    while pos + 1 < bytes.len() {
        if bytes[pos] == b'_' && bytes[pos + 1] == b'_' {
            // Closing `__` must not be followed by alphanumeric
            let followed_by_alnum = pos + 2 < bytes.len() && bytes[pos + 2].is_ascii_alphanumeric();
            if !followed_by_alnum {
                return Some(pos);
            }
        }
        pos += 1;
    }
    None
}

fn find_marker_str(s: &str, from: usize, marker: &str) -> Option<usize> {
    s.get(from..)
        .and_then(|tail| tail.find(marker))
        .map(|pos| pos + from)
}

/// Estimate display width of a string, treating CJK characters as double-width.
///
/// ASCII and most Latin characters count as 1 unit; CJK ideographs and
/// fullwidth forms count as 2 units. This gives a much better width
/// estimate than `str::len()` (byte count) for mixed-script text.
pub fn display_width(s: &str) -> usize {
    s.chars().map(|c| if is_cjk(c) { 2 } else { 1 }).sum()
}

/// Convert markdown inline markup to HTML, matching what `marked` (the library
/// used by upstream mermaid) produces for single-line label text.
///
/// Supported constructs (in precedence order):
/// - `**text**` → `<strong>text</strong>`
/// - `__text__` → `<strong>text</strong>` (only at non-alphanumeric boundary)
/// - `*text*` → `<em>text</em>`
/// - `_text_` → `<em>text</em>` (only at non-alphanumeric boundary)
/// - `` `text` `` → `<code>text</code>`
/// - Plain text is passed through unchanged
///
/// This is used to emit the rendered HTML content for `<foreignObject>` spans,
/// so that JSDOM's `el.innerHTML` matches what upstream's `markdownToHTML` produces.
pub fn markdown_to_html(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut result = String::with_capacity(n + 32);
    let mut i = 0;
    while i < n {
        // *** bold-italic *** — must check before ** to avoid wrong closing match
        if i + 2 < n && bytes[i] == b'*' && bytes[i + 1] == b'*' && bytes[i + 2] == b'*' {
            if let Some(end) = find_marker_str(s, i + 3, "***") {
                result.push_str("<em><strong>");
                result.push_str(&markdown_to_html(&s[i + 3..end]));
                result.push_str("</strong></em>");
                i = end + 3;
                continue;
            }
        }
        // ** bold **
        if i + 1 < n && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            if let Some(end) = find_marker_str(s, i + 2, "**") {
                result.push_str("<strong>");
                result.push_str(&markdown_to_html(&s[i + 2..end]));
                result.push_str("</strong>");
                i = end + 2;
                continue;
            }
        }
        // __ bold __ — only when NOT preceded by alphanumeric
        if i + 1 < n && bytes[i] == b'_' && bytes[i + 1] == b'_' {
            let preceded_by_alnum = i > 0 && bytes[i - 1].is_ascii_alphanumeric();
            if !preceded_by_alnum {
                if let Some(end) = find_closing_underscore2(s, i + 2) {
                    result.push_str("<strong>");
                    result.push_str(&markdown_to_html(&s[i + 2..end]));
                    result.push_str("</strong>");
                    i = end + 2;
                    continue;
                }
            }
            // Rejected or no match — output both underscores and skip
            result.push('_');
            result.push('_');
            i += 2;
            continue;
        }
        // * italic *
        if bytes[i] == b'*' {
            if let Some(end) = find_marker_str(s, i + 1, "*") {
                if end > i + 1 {
                    result.push_str("<em>");
                    result.push_str(&markdown_to_html(&s[i + 1..end]));
                    result.push_str("</em>");
                    i = end + 1;
                    continue;
                }
            }
        }
        // _ italic _ — only when NOT preceded by alphanumeric
        if bytes[i] == b'_' {
            let preceded_by_alnum = i > 0 && bytes[i - 1].is_ascii_alphanumeric();
            if !preceded_by_alnum {
                if let Some(end) = find_closing_underscore1(s, i + 1) {
                    if end > i + 1 {
                        result.push_str("<em>");
                        result.push_str(&markdown_to_html(&s[i + 1..end]));
                        result.push_str("</em>");
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        // ` code `
        if bytes[i] == b'`' {
            if let Some(end) = find_marker_str(s, i + 1, "`") {
                result.push_str("<code>");
                result.push_str(&s[i + 1..end]);
                result.push_str("</code>");
                i = end + 1;
                continue;
            }
        }
        // Append char as-is
        let ch = s[i..].chars().next().unwrap_or('\0');
        result.push(ch);
        i += ch.len_utf8();
    }
    result
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
    fn markdown_strip_basic() {
        assert_eq!(markdown_text_content("**bold**"), "bold");
        assert_eq!(markdown_text_content("__bold__"), "bold");
        assert_eq!(markdown_text_content("*italic*"), "italic");
        assert_eq!(markdown_text_content("_italic_"), "italic");
        assert_eq!(markdown_text_content("`code`"), "code");
        assert_eq!(
            markdown_text_content("__my bolded name__"),
            "my bolded name"
        );
        assert_eq!(
            markdown_text_content("*my italicized name*"),
            "my italicized name"
        );
        assert_eq!(
            markdown_text_content("Text: **Bolded text** _italicized text_"),
            "Text: Bolded text italicized text"
        );
        assert_eq!(
            markdown_text_content("Doc Ref: *Italicized* __Bolded__"),
            "Doc Ref: Italicized Bolded"
        );
        assert_eq!(
            markdown_text_content("&lt;&lt;Requirement&gt;&gt;"),
            "&lt;&lt;Requirement&gt;&gt;"
        );
        assert_eq!(markdown_text_content("plain text"), "plain text");
    }

    #[test]
    fn markdown_underscore_word_boundary_rules() {
        // CommonMark: `_` cannot open/close emphasis when adjacent to alphanumeric.
        // Identifiers with underscores must be preserved as-is.
        assert_eq!(
            markdown_text_content("test_entity_name_that_is_extra_long"),
            "test_entity_name_that_is_extra_long"
        );
        // Standalone `_word_` at start of string is processed
        assert_eq!(markdown_text_content("_italic_"), "italic");
        // `_italic_` preceded by space is processed
        assert_eq!(
            markdown_text_content("prefix _italic_ suffix"),
            "prefix italic suffix"
        );
        // `_italic_` preceded by non-alphanumeric (colon) — should process
        assert_eq!(
            markdown_text_content("Text: _italicized text_"),
            "Text: italicized text"
        );
        // word_underscore_word — NOT processed (underscore between alphanumerics)
        assert_eq!(
            markdown_text_content("word_italic_word"),
            "word_italic_word"
        );
        // `__bold__` at start of string is processed
        assert_eq!(markdown_text_content("__bold text__"), "bold text");
        // word__bold__word — NOT processed
        assert_eq!(
            markdown_text_content("word__bold__word"),
            "word__bold__word"
        );
    }

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
