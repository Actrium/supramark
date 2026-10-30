//! Random assortment of functions that's used internally to write plugins.

use crate::plugins::cmark::inline::entity::{
    DIGITAL_ENTITY_PATTERN, DIGITAL_RE, NAMED_ENTITY_PATTERN, NAMED_RE,
};
use entities;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;

const UNESCAPE_MD_RE: &str = r##"\\([!"#$%&'()*+,\-./:;<=>?@\[\\\]^_`{|}~])"##;

// `unescape_all` runs over fence info strings and link destinations/titles,
// which are parsed by different rules than inline text but follow the same
// entity grammar (CommonMark §6.2 draws no positional distinction). It
// therefore composes the scanner's own pattern bodies
// (`plugins/cmark/inline/entity.rs` `NAMED_ENTITY_PATTERN` /
// `DIGITAL_ENTITY_PATTERN`) rather than carrying a third copy — issue #234.
// The scanner's `Regex` objects cannot be reused directly: the scanner
// anchors its patterns at the current position (`^`), while this one
// alternates them anywhere in the string. Composing the same bodies keeps
// the grammar single-sourced either way.
static UNESCAPE_ALL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        "{UNESCAPE_MD_RE}|(?i)(?:{NAMED_ENTITY_PATTERN}|{DIGITAL_ENTITY_PATTERN})"
    ))
    .unwrap()
});

#[allow(clippy::manual_range_contains)]
/// Return true if a `code` you got from `&#xHHHH;` entity is a valid charcode.
///
/// Noncharacter handling matches cmark's `houdini_unescape_ent`: noncharacters
/// (U+FDD0–U+FDEF and the last two code points of every plane, U+FFFE/U+FFFF,
/// U+1FFFE/U+1FFFF, ...) are valid Unicode scalar values, so like cmark we keep
/// them verbatim instead of replacing them with U+FFFD.
///
/// This is NOT a full mirror of cmark: cmark replaces only NUL, surrogates
/// (U+D800–U+DFFF) and code points above U+10FFFF with U+FFFD and decodes
/// C0/C1 controls (and DEL, U+007F) as-is. This function additionally rejects
/// those controls — a deliberate markdown-it compatibility deviation, kept
/// because no conformance case exercises them and callers outside the
/// conformance-facing entity decoding rely on the markdown-it escape behavior.
/// ```
/// # use supramark_markdown::common::utils::is_valid_entity_code;
/// assert_eq!(is_valid_entity_code(1), false);
/// assert_eq!(is_valid_entity_code(32), true);
/// assert_eq!(is_valid_entity_code(0xFFFE), true);
/// ```
pub fn is_valid_entity_code(code: u32) -> bool {
    // broken sequence
    if code >= 0xD800 && code <= 0xDFFF {
        return false;
    }
    // control codes
    if code <= 0x08 {
        return false;
    }
    if code == 0x0B {
        return false;
    }
    if code >= 0x0E && code <= 0x1F {
        return false;
    }
    if code >= 0x7F && code <= 0x9F {
        return false;
    }
    // out of range
    if code > 0x10FFFF {
        return false;
    }
    true
}

/// Check if "&xxxx;" string is a valid HTML entity, return character it represents.
/// ```
/// # use supramark_markdown::common::utils::get_entity_from_str;
/// assert_eq!(get_entity_from_str("&amp;"), Some("&"));
/// assert_eq!(get_entity_from_str("&xxx;"), None);
/// ```
pub fn get_entity_from_str(str: &str) -> Option<&'static str> {
    pub static ENTITIES_HASH: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
        let mut mapping = HashMap::new();
        for e in &entities::ENTITIES {
            if e.entity.ends_with(';') {
                mapping.insert(e.entity, e.characters);
            }
        }
        mapping
    });

    ENTITIES_HASH.get(str).copied()
}

/// Decode one `&…;` match produced by `UNESCAPE_ALL_RE`: a named reference
/// through the shared entity table, a numeric reference through the
/// scanner's own `decode_numeric_entity` — so an invalid charcode decodes to
/// `U+FFFD` here exactly as it does in inline text (issue #234). Returns
/// `None` for anything the table does not know and that is not a numeric
/// reference (`&Amp;`, `&ffff;`, ...), leaving the match literal.
fn replace_entity_pattern(str: &str) -> Option<String> {
    if let Some(entity) = get_entity_from_str(str) {
        return Some((*entity).to_owned());
    }
    // Validate the shape with the scanner's own anchored grammar before
    // decoding, so a look-alike that is not a reference (`&#3F;`, `&#xGG;`)
    // stays literal instead of reaching the decoder.
    let captures = DIGITAL_RE.captures(str)?;
    let digits = captures.get(1).unwrap().as_str();
    Some(decode_numeric_entity(digits).to_string())
}

/// Decode the digit body of a numeric character reference (`x41` / `65`,
/// exactly what the `DIGITAL_RE` capture group holds) into its character.
/// Invalid charcodes (`&#0;`, surrogates like `&#xD800;`, out of Unicode
/// range like `&#x110000;`) map to `U+FFFD`. That matches cmark's
/// `houdini_unescape_ent` for NUL, surrogates and code points above
/// U+10FFFF, plus this crate's own additional C0/C1 rejection — see
/// `is_valid_entity_code` for the full deviation note. Noncharacters are
/// valid Unicode scalar values and are kept verbatim (issue #233). Shared
/// by the inline scanner
/// (`entity.rs::parse_digital_entity`), `decode_entity_at_start` (the
/// `RawValueMap` lock-step) and `replace_entity_pattern` (`unescape_all`),
/// so the replacement semantics cannot drift between them either.
#[allow(clippy::from_str_radix_10)]
pub(crate) fn decode_numeric_entity(digits: &str) -> char {
    let code = if digits.starts_with('x') || digits.starts_with('X') {
        u32::from_str_radix(&digits[1..], 16).unwrap()
    } else {
        u32::from_str_radix(digits, 10).unwrap()
    };
    if is_valid_entity_code(code) {
        char::from_u32(code).unwrap()
    } else {
        '\u{FFFD}'
    }
}

/// Decode the entity / numeric character reference that starts at the
/// beginning of `raw`, using the inline entity scanner's own grammar
/// objects (`plugins/cmark/inline/entity.rs` `DIGITAL_RE` / `NAMED_RE` —
/// imported, not copied, so the two can never diverge; issue #234).
/// `&amp;` -> `&`, `&#65;` -> `A`, and an invalid charcode (`&#0;`,
/// `&#xD800;`, `&#x110000;`, ...) -> `U+FFFD` (the scanner's replacement,
/// via `decode_numeric_entity`). Returns the decoded replacement plus the
/// raw byte length consumed, or `None` when `raw` does not start with an
/// entity the scanner would consume (the `&` then stays literal and maps
/// 1:1).
pub(crate) fn decode_entity_at_start(raw: &str) -> Option<(String, usize)> {
    if let Some(captures) = NAMED_RE.captures(raw) {
        let matched = captures.get(0).unwrap();
        let decoded = get_entity_from_str(matched.as_str())?;
        return Some((decoded.to_owned(), matched.as_str().len()));
    }
    let captures = DIGITAL_RE.captures(raw)?;
    let matched = captures.get(0).unwrap();
    let digits = captures.get(1).unwrap().as_str();
    Some((decode_numeric_entity(digits).to_string(), matched.as_str().len()))
}

/// Unescape both entities (`&quot; -> "`) and backslash escapes (`\" -> "`).
///
/// Entity recognition follows the inline scanner's grammar
/// (`plugins/cmark/inline/entity.rs`), so a reference decodes here exactly
/// when it would decode in inline text — same name table, same numeric
/// bounds, same `U+FFFD` replacement for invalid charcodes (issue #234).
/// ```
/// # use supramark_markdown::common::utils::unescape_all;
/// assert_eq!(unescape_all("&amp;"), "&");
/// assert_eq!(unescape_all("\\&"), "&");
/// ```
pub fn unescape_all(str: &str) -> Cow<str> {
    if !str.contains('\\') && !str.contains('&') {
        return Cow::Borrowed(str);
    }

    UNESCAPE_ALL_RE.replace_all(str, |captures: &regex::Captures| {
        let s = captures.get(0).unwrap().as_str();
        if let Some(m) = captures.get(1) {
            // \" -> "
            m.as_str().to_owned()
        } else if let Some(replacement) = replace_entity_pattern(s) {
            // &quot; -> "
            replacement
        } else {
            s.to_owned()
        }
    })
}

/// Escape `" < > &` with corresponding HTML entities;
/// ```
/// # use supramark_markdown::common::utils::escape_html;
/// assert_eq!(escape_html("&\""), "&amp;&quot;");
/// ```
pub fn escape_html(str: &str) -> Cow<str> {
    html_escape::encode_double_quoted_attribute(str)
}

/// Unicode case folding + space normalization, used for for reference labels.
///
/// So that strings equal according to commonmark standard are converted to
/// the same string (lowercase/uppercase differences and spacing go away).
/// ```
/// # use supramark_markdown::common::utils::normalize_reference;
/// assert_eq!(normalize_reference("hello"), normalize_reference("HELLO"));
/// assert_eq!(normalize_reference("a   b"), normalize_reference("a b"));
/// ```
pub fn normalize_reference(str: &str) -> String {
    static SPACE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());

    // Trim and collapse whitespace
    //
    let str = SPACE_RE.replace_all(str.trim(), " ");

    // .toLowerCase().toUpperCase() should get rid of all differences
    // between letter variants.
    //
    // Simple .toLowerCase() doesn't normalize 125 code points correctly,
    // and .toUpperCase doesn't normalize 6 of them (list of exceptions:
    // İ, ϴ, ẞ, Ω, K, Å - those are already uppercased, but have differently
    // uppercased versions).
    //
    // Here's an example showing how it happens. Lets take greek letter omega:
    // uppercase U+0398 (Θ), U+03f4 (ϴ) and lowercase U+03b8 (θ), U+03d1 (ϑ)
    //
    // Unicode entries:
    // 0398;GREEK CAPITAL LETTER THETA;Lu;0;L;;;;;N;;;;03B8;
    // 03B8;GREEK SMALL LETTER THETA;Ll;0;L;;;;;N;;;0398;;0398
    // 03D1;GREEK THETA SYMBOL;Ll;0;L;<compat> 03B8;;;;N;GREEK SMALL LETTER SCRIPT THETA;;0398;;0398
    // 03F4;GREEK CAPITAL THETA SYMBOL;Lu;0;L;<compat> 0398;;;;N;;;;03B8;
    //
    // Case-insensitive comparison should treat all of them as equivalent.
    //
    // But .toLowerCase() doesn't change ϑ (it's already lowercase),
    // and .toUpperCase() doesn't change ϴ (already uppercase).
    //
    // Applying first lower then upper case normalizes any character:
    // '\u0398\u03f4\u03b8\u03d1'.toLowerCase().toUpperCase() === '\u0398\u0398\u0398\u0398'
    //
    // Note: this is equivalent to unicode case folding; unicode normalization
    // is a different step that is not required here.
    //
    // Final result should be uppercased, because it's later stored in an object
    // (this avoid a conflict with Object.prototype members,
    // most notably, `__proto__`)
    //
    str.to_lowercase().to_uppercase()
}

/// Count number of characters since last occurrence of `char`.
///
/// Finds last occurrence of `char` in `source`, returns number of characters from
/// that last occurrence. If char is not found, return number of characters total.
/// ```
/// # use supramark_markdown::common::utils::rfind_and_count;
/// assert_eq!(rfind_and_count("abcde", 'e'), 0);
/// assert_eq!(rfind_and_count("abcde", 'b'), 3);
/// assert_eq!(rfind_and_count("abcde", 'z'), 5);
/// ```
pub fn rfind_and_count(source: &str, char: char) -> usize {
    let mut result = 0;
    for c in source.chars().rev() {
        if c == char {
            break;
        }
        result += 1;
    }
    result
}

/// Calculate number of spaces from `pos` to first non-space character or EOL.
///
/// Tabs are expanded to variable number of spaces with tabstop = 4.
/// Returns relative indent and offset of first non-space character.
/// ```
/// # use supramark_markdown::common::utils::find_indent_of;
/// assert_eq!(find_indent_of("\tfoo", 0), (4, 1));
/// ```
pub fn find_indent_of(line: &str, mut pos: usize) -> (usize, usize) {
    let mut chars = line[pos..].chars();
    let mut indent = 0;

    loop {
        match chars.next() {
            Some('\t') => {
                let bs_count = rfind_and_count(&line[..pos], '\t');
                indent += 4 - bs_count % 4;
                pos += 1;
            }
            Some(' ') => {
                indent += 1;
                pos += 1;
            }
            _ => return (indent, pos),
        }
    }
}

/// Returns trailing whitespace with total length of `indent`.
///
/// Input: a string of characters (presumed whitespaces, can be anything), where each one of
/// them contributes 1 to indent (except for tabs, whose width may vary with tabstop = 4).
///
/// If an indent would split a tab, that tab is replaced with 4 spaces.
///
/// Example: cut_right_whitespace_with_tabstops("\t\t", 6) would return "  \t" (two preceding
/// spaces) because first tab gets expanded to 6 spaces.
/// ```
/// # use supramark_markdown::common::utils::cut_right_whitespace_with_tabstops;
/// assert_eq!(cut_right_whitespace_with_tabstops("\t\t", 6), "  \t");
/// ```
pub fn cut_right_whitespace_with_tabstops(source: &str, indent: i32) -> Cow<str> {
    let (num_spaces, start) = calc_right_whitespace_with_tabstops(source, indent);

    if num_spaces > 0 {
        let mut result = " ".repeat(num_spaces);
        result += &source[start..];
        Cow::Owned(result)
    } else {
        Cow::Borrowed(&source[start..])
    }
}

/// Calculate trailing whitespace with total length of `indent`.
///
/// See [cut_right_whitespace_with_tabstops](cut_right_whitespace_with_tabstops)
/// for algorithm and details.
///
/// Returns number of spaces + number of bytes to cut from the end.
/// ```
/// # use supramark_markdown::common::utils::calc_right_whitespace_with_tabstops;
/// assert_eq!(calc_right_whitespace_with_tabstops("\t\t", 6), (2, 1));
/// ```
pub fn calc_right_whitespace_with_tabstops(source: &str, mut indent: i32) -> (usize, usize) {
    let mut start = source.len();
    let mut chars = source.char_indices().rev();

    while indent > 0 {
        match chars.next() {
            Some((pos, '\t')) => {
                // previous tab is guaranteed to finish at 0 modulo 4,
                // so we can finish counting there
                let indent_from_start = rfind_and_count(&source[..pos], '\t');
                let tab_width = 4 - indent_from_start as i32 % 4;

                if indent < tab_width {
                    return (indent as usize, start);
                }

                indent -= tab_width;
                start = pos;
            }
            Some((pos, _)) => {
                indent -= 1;
                start = pos;
            }
            None => {
                start = 0;
                break;
            }
        }
    }

    (0, start)
}

/// Checks whether a given character should count as punctuation
///
/// used to determine word boundaries, made to match the implementation of
/// `isPunctChar` from the JS library.
/// This is currently implemented as a `match`, but might be simplified as a
/// regex if benchmarking shows this to be beneficient.
pub fn is_punct_char(ch: char) -> bool {
    use unicode_general_category::get_general_category;
    use unicode_general_category::GeneralCategory::*;

    match get_general_category(ch) {
        // P (Punctuation)
        ConnectorPunctuation | DashPunctuation | OpenPunctuation | ClosePunctuation |
        InitialPunctuation | FinalPunctuation | OtherPunctuation |
        // S (Symbol) — CommonMark 0.31.2 treats all Symbol categories as
        // punctuation for emphasis flanking, so currency signs ($, £, €) and
        // math/modifier symbols cannot form intraword emphasis.
        MathSymbol | CurrencySymbol | ModifierSymbol | OtherSymbol => true,

        // L
        UppercaseLetter | LowercaseLetter | TitlecaseLetter | ModifierLetter | OtherLetter |
        // M
        NonspacingMark | SpacingMark | EnclosingMark |
        // N
        DecimalNumber | LetterNumber | OtherNumber |
        // Z
        SpaceSeparator | LineSeparator | ParagraphSeparator |
        // C
        Control | Format | Surrogate | PrivateUse | Unassigned => false
    }
}

#[cfg(test)]
mod tests {
    use super::cut_right_whitespace_with_tabstops as cut_ws;
    use super::decode_entity_at_start;
    use super::find_indent_of;
    use super::replace_entity_pattern;
    use super::rfind_and_count;
    use super::unescape_all;

    #[test]
    fn rfind_and_count_test() {
        assert_eq!(rfind_and_count("", 'b'), 0);
        assert_eq!(rfind_and_count("abcde", 'e'), 0);
        assert_eq!(rfind_and_count("abcde", 'b'), 3);
        assert_eq!(rfind_and_count("abcde", 'z'), 5);
        assert_eq!(rfind_and_count("abcεπ", 'b'), 3);
    }

    #[test]
    fn find_indent_of_simple_test() {
        assert_eq!(find_indent_of("a", 0), (0, 0));
        assert_eq!(find_indent_of(" a", 0), (1, 1));
        assert_eq!(find_indent_of("   a", 0), (3, 3));
        assert_eq!(find_indent_of("    ", 0), (4, 4));
        assert_eq!(find_indent_of("\ta", 0), (4, 1));
        assert_eq!(find_indent_of(" \ta", 0), (4, 2));
        assert_eq!(find_indent_of("  \ta", 0), (4, 3));
        assert_eq!(find_indent_of("   \ta", 0), (4, 4));
        assert_eq!(find_indent_of("    \ta", 0), (8, 5));
    }

    #[test]
    fn find_indent_of_with_offset() {
        assert_eq!(find_indent_of("   a", 2), (1, 3));
        assert_eq!(find_indent_of("    a", 2), (2, 4));
        assert_eq!(find_indent_of("  \ta", 2), (2, 3));
        assert_eq!(find_indent_of("   \ta", 2), (2, 4));
        assert_eq!(find_indent_of("    \ta", 2), (6, 5));
        assert_eq!(find_indent_of("     \ta", 2), (6, 6));
    }

    #[test]
    fn find_indent_of_tabs_test() {
        assert_eq!(find_indent_of("  \t \ta", 1), (7, 5));
        assert_eq!(find_indent_of("  \t \ta", 2), (6, 5));
        assert_eq!(find_indent_of("  \t \ta", 3), (4, 5));
        assert_eq!(find_indent_of("  \t \ta", 4), (3, 5));
    }

    #[test]
    fn cut_ws_simple() {
        assert_eq!(cut_ws("abc", -1), "");
        assert_eq!(cut_ws("abc", 0), "");
        assert_eq!(cut_ws("abc", 1), "c");
        assert_eq!(cut_ws("abc", 2), "bc");
        assert_eq!(cut_ws("abc", 3), "abc");
        assert_eq!(cut_ws("abc", 4), "abc");
    }

    #[test]
    fn cut_ws_unicode() {
        assert_eq!(cut_ws("αβγδ", 1), "δ");
        assert_eq!(cut_ws("αβγδ ", 3), "γδ ");
    }

    #[test]
    fn cut_ws_expands_partial_tabs() {
        assert_eq!(cut_ws("\t", 1), " ");
        assert_eq!(cut_ws("\t", 2), "  ");
        assert_eq!(cut_ws("\t", 3), "   ");
        assert_eq!(cut_ws("\t\t\t", 5), " \t");
        assert_eq!(cut_ws("\t\t\t", 7), "   \t");
    }

    #[test]
    fn cut_ws_retains_full_tabs() {
        assert_eq!(cut_ws("\t\t\t", 4), "\t");
        assert_eq!(cut_ws("\t\t\t", 8), "\t\t");
    }

    #[test]
    fn cut_ws_proper_tabstops() {
        assert_eq!(cut_ws("a\t", 1), " ");
        assert_eq!(cut_ws("a\t", 2), "  ");
        assert_eq!(cut_ws("a\t", 3), "\t");
        assert_eq!(cut_ws("ab\t", 3), "b\t");
        assert_eq!(cut_ws("abc\t", 3), "bc\t");
    }

    #[test]
    fn cut_ws_proper_tabstops_nested() {
        assert_eq!(cut_ws("a\tb\t", 2), "  ");
        assert_eq!(cut_ws("a\tb\t", 3), "\t");
        assert_eq!(cut_ws("a\tb\t", 4), "b\t");
        assert_eq!(cut_ws("a\tb\t", 5), " b\t");
        assert_eq!(cut_ws("a\tb\t", 6), "  b\t");
        assert_eq!(cut_ws("a\tb\t", 7), "\tb\t");
        assert_eq!(cut_ws("a\tb\t", 8), "a\tb\t");
    }

    #[test]
    fn cut_ws_different_tabstops_nested() {
        assert_eq!(cut_ws("abc\tde\tf\tg", 3), "  g");
        assert_eq!(cut_ws("abc\tde\tf\tg", 4), "\tg");
        assert_eq!(cut_ws("abc\tde\tf\tg", 5), "f\tg");
        assert_eq!(cut_ws("abc\tde\tf\tg", 6), " f\tg");
        assert_eq!(cut_ws("abc\tde\tf\tg", 7), "\tf\tg");
        assert_eq!(cut_ws("abc\tde\tf\tg", 9), "de\tf\tg");
        assert_eq!(cut_ws("abc\tde\tf\tg", 10), "\tde\tf\tg");
    }

    #[test]
    fn test_replace_entity_pattern() {
        assert_eq!(replace_entity_pattern("&amp;"), Some("&".into()));
        assert_eq!(replace_entity_pattern("&euro;"), Some("€".into()));
        assert_eq!(replace_entity_pattern("&#8212;"), Some("—".into()));
        assert_eq!(replace_entity_pattern("&#x2014;"), Some("—".into()));
        assert_eq!(replace_entity_pattern("&#X20;"), Some(" ".into()));
        assert_eq!(replace_entity_pattern("&#x3F;"), Some("?".into()));
        assert_eq!(replace_entity_pattern("&ffff;"), None);
        assert_eq!(replace_entity_pattern("&#3F;"), None);
        assert_eq!(replace_entity_pattern("&#xGG;"), None);
    }

    #[test]
    fn unescape_all_matches_scanner_entity_grammar() {
        // `unescape_all` runs over fence info strings and link
        // destinations/titles, but the entity grammar is not positional
        // (CommonMark §6.2): it must accept exactly the `&…;` spans the
        // inline scanner accepts. It composes the scanner's own pattern
        // bodies (entity.rs NAMED_ENTITY_PATTERN / DIGITAL_ENTITY_PATTERN)
        // rather than carrying a copy — issue #234, where the older
        // markdown-it copy had drifted to `{1,8}` and decoded references the
        // scanner leaves literal.
        assert_eq!(unescape_all("&#65;"), "A");
        assert_eq!(unescape_all("&#x41;"), "A");
        assert_eq!(unescape_all("&#X41;"), "A");

        // Past the scanner's bounds (hex {1,6}, decimal {1,7}) the reference
        // stays literal here too, instead of decoding as it used to.
        assert_eq!(unescape_all("&#99999999;"), "&#99999999;");
        assert_eq!(unescape_all("&#x0000041;"), "&#x0000041;");

        // A valid-shape reference with an invalid charcode takes the
        // scanner's replacement rather than staying literal.
        assert_eq!(unescape_all("&#1234567;"), "\u{FFFD}");
        assert_eq!(unescape_all("&#0;"), "\u{FFFD}");
        assert_eq!(unescape_all("&#xD800;"), "\u{FFFD}");

        // Named references are unchanged: HTML5's all-caps aliases are real
        // table entries, while a case variant that is not one stays literal,
        // exactly as the scanner resolves them.
        assert_eq!(unescape_all("&amp;"), "&");
        assert_eq!(unescape_all("&ouml;"), "ö");
        assert_eq!(unescape_all("&AMP;"), "&");
        assert_eq!(unescape_all("&Amp;"), "&Amp;");

        // Names near the scanner's 31-char upper bound decode here too, so
        // a narrowed NAMED_ENTITY_PATTERN cannot pass this test unnoticed.
        assert_eq!(
            unescape_all("&CounterClockwiseContourIntegral;"),
            "\u{2233}"
        );
    }

    #[test]
    fn decode_entity_at_start_grammar_boundaries() {
        // These bounds are the inline scanner's (entity.rs DIGITAL_RE /
        // NAMED_RE, imported — not copied — by decode_entity_at_start):
        // hex {1,6}, decimal {1,7}, named names of 2..=32 chars.

        // Hex: 6 digits is the upper bound; 7+ digits stay literal.
        assert_eq!(decode_entity_at_start("&#x41;"), Some(("A".into(), 6)));
        assert_eq!(
            decode_entity_at_start("&#x10FFEF;"),
            Some(("\u{10FFEF}".into(), 10))
        );
        assert_eq!(decode_entity_at_start("&#x0000041;"), None);

        // Decimal: 7 digits is the upper bound; 8+ digits stay literal.
        assert_eq!(decode_entity_at_start("&#65;"), Some(("A".into(), 5)));
        assert_eq!(
            decode_entity_at_start("&#1234567;"),
            Some(("\u{FFFD}".into(), 10))
        );
        assert_eq!(decode_entity_at_start("&#12345678;"), None);

        // Named: 31 chars is the longest real entity name; a 32-char name
        // still matches NAMED_RE but fails the entity-table lookup (so
        // None, like the scanner), and 33 chars miss the grammar.
        assert_eq!(
            decode_entity_at_start("&CounterClockwiseContourIntegral;"),
            Some(("\u{2233}".into(), 33))
        );
        assert_eq!(decode_entity_at_start(&format!("&{}a;", "a".repeat(31))), None);
        assert_eq!(decode_entity_at_start(&format!("&{}a;", "a".repeat(32))), None);

        // Invalid charcodes decode to U+FFFD (never literal), matching the
        // scanner's replacement.
        assert_eq!(decode_entity_at_start("&#0;"), Some(("\u{FFFD}".into(), 4)));
        assert_eq!(
            decode_entity_at_start("&#xD800;"),
            Some(("\u{FFFD}".into(), 8))
        );
        assert_eq!(
            decode_entity_at_start("&#x110000;"),
            Some(("\u{FFFD}".into(), 10))
        );

        // Noncharacters are valid Unicode scalar values: like cmark's
        // `houdini_unescape_ent`, the scanner and both decoders keep them
        // verbatim instead of replacing them with U+FFFD (issue #233). One
        // case per branch removed by #283: the U+FDD0-U+FDEF block, the
        // last two code points of each plane, and the top of the range.
        assert_eq!(
            decode_entity_at_start("&#xFDD0;"),
            Some(("\u{FDD0}".into(), 8))
        );
        assert_eq!(
            decode_entity_at_start("&#xFFFE;"),
            Some(("\u{FFFE}".into(), 8))
        );
        assert_eq!(
            decode_entity_at_start("&#x10FFFF;"),
            Some(("\u{10FFFF}".into(), 10))
        );
        assert_eq!(
            decode_entity_at_start("&#1114111;"),
            Some(("\u{10FFFF}".into(), 10))
        );
    }

    #[test]
    fn test_unescape_all_simple() {
        assert_eq!(unescape_all("&amp;"), "&");
        assert_eq!(unescape_all("\\&"), "&");
    }

    #[test]
    fn is_punct_char_treats_symbol_categories_as_punct() {
        // CommonMark 0.31.2 treats all Unicode Symbol categories (Math, Currency,
        // Modifier, Other) as punctuation for emphasis flanking, so currency
        // signs and math symbols cannot form intraword emphasis. Letters,
        // digits, and whitespace stay non-punctuation.
        use super::is_punct_char;
        for ch in ['$', '€', '£', '∑', '→', '∞', '☆'] {
            assert!(is_punct_char(ch), "{ch:?} should be punctuation");
        }
        // cjk-allow: next line includes a CJK letter to verify it is treated as a Letter, not punctuation
        for ch in ['a', 'Z', '0', '5', ' ', '中', 'α'] {
            assert!(!is_punct_char(ch), "{ch:?} should NOT be punctuation");
        }
    }

    #[test]
    fn test_unescape_all_xss() {
        assert_eq!(
            unescape_all(r#"javascript&#x3A;alert(1)"#),
            r#"javascript:alert(1)"#
        );

        assert_eq!(
            unescape_all(r#"&#74;avascript:alert(1)"#),
            r#"Javascript:alert(1)"#
        );

        assert_eq!(
            unescape_all(r#"&#x26;#74;avascript:alert(1)"#),
            r#"&#74;avascript:alert(1)"#
        );

        assert_eq!(
            unescape_all(r#"\&#74;avascript:alert(1)"#),
            r#"&#74;avascript:alert(1)"#
        );

        assert_eq!(
            unescape_all(
                r#"&#34;&#62;&#60;script&#62;alert&#40;&#34;xss&#34;&#41;&#60;/script&#62;"#
            ),
            r#""><script>alert("xss")</script>"#
        );
    }
}
