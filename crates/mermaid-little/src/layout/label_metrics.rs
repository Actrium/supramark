//! Shared CJK-aware label width estimation.
//!
//! The byte-exact jsdom shim (`measure_html_markup_label` /
//! `font_metrics::text_width`) measures ASCII width accurately but
//! **under-measures CJK glyphs to ~40%** of their real render width — it has
//! no CJK fallback-font metrics. A 4-ideograph label measures ~28px via the shim but
//! renders ~72px in a real browser. Any consumer that needs the label's
//! *visual* width — reserving layout space, or detecting collisions — must
//! floor the shim measurement with a per-glyph character estimate, or it will
//! under-reserve / fail to see overlaps.
//!
//! Never loosen the shim itself: it drives foreignObject sizing, which must
//! stay byte-exact with upstream `mermaid@11.14.0`. The floor is applied only
//! by opt-in consumers on top of the shim result.

/// One em at the rendered edge-label font size (`HTML_LABEL_FONT_SIZE = 16`),
/// i.e. what the browser actually paints. Layout reservation uses this so dagre
/// reserves the real on-screen width rather than the shim's under-estimate.
const EM: f64 = 16.0;

/// Best estimate of `text`'s rendered width, never below the shim `measured`
/// value. Latin glyphs estimate to ~0.56em (close to the shim, so pure-Latin
/// labels are effectively unchanged); CJK and fullwidth glyphs estimate to
/// ~1em, correcting the shim's under-measurement.
pub fn cjk_aware_label_width(text: &str, measured: f64) -> f64 {
    let per_glyph: f64 = text
        .chars()
        .map(|c| if is_wide_glyph(c) { EM } else { EM * 0.56 })
        .sum();
    measured.max(per_glyph)
}

/// Whether a glyph renders ~1em wide and is thus under-measured by the shim.
/// Used both inside [`cjk_aware_label_width`] and to cheaply gate the floor on
/// "label contains at least one wide glyph", so pure-Latin diagrams are not
/// perturbed.
///
/// Delegates to the Unicode East Asian Width property (`W` and `F` both report
/// width 2) rather than a hand-rolled range list. Hand-rolled ranges got this
/// wrong in both directions: Hangul syllables (`U+AC00..`), CJK Ext B+
/// (`U+20000..`), Bopomofo and the compatibility blocks were all missing, so a
/// Korean label got no floor at all, while `U+FF00..=U+FFEF` swallowed the
/// *halfwidth* forms (`U+FF61..=U+FF9F`) and over-reserved them 2x.
///
/// Note this is deliberately **not** [`crate::text::is_cjk`]: that one feeds
/// `display_width` for class-diagram member sizing, which is byte-exact with
/// upstream and must keep its own range list.
pub fn is_wide_glyph(c: char) -> bool {
    unicode_width::UnicodeWidthChar::width(c) == Some(2)
}

/// The marker-free plain text a label actually paints — the same string
/// `measure_edge_label` / `measure_html_markup_label` and the browser measure.
/// Markdown labels go through `markdown_label_to_html` first (so `**x**` →
/// `<strong>x</strong>`), then [`strip_html_for_measurement`] removes every
/// tag and decodes entities.
///
/// Feed this — not the raw markup — into [`cjk_aware_label_width`]: the
/// per-glyph estimate must run over painted glyphs only, or markup characters
/// (`<br/>`, `**`, `&amp;`, `<i class="fa …">`) get billed ~0.56em each and the
/// width floor re-inflates past the real painted width, re-creating the
/// off-centre appearance issue #93's fix exists to remove.
pub fn edge_label_plain_text(text: &str, is_markdown: bool) -> String {
    let measure_text = if is_markdown {
        crate::render::foreign_object::markdown_label_to_html(text)
    } else {
        text.to_string()
    };
    strip_html_for_measurement(&measure_text)
}

/// Split label markup at line breaks (`<br>` variants, any case, and `\n`)
/// and reduce each line to its painted plain text via
/// [`strip_html_for_measurement`]. Always returns at least one line.
pub fn plain_text_lines(s: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            lines.push(strip_html_for_measurement(&s[start..i]));
            i += 1;
            start = i;
            continue;
        }
        if bytes[i] == b'<' {
            if let Some(rel_end) = s[i..].find('>') {
                let inner = s[i + 1..i + rel_end].trim_end_matches('/').trim();
                if inner.eq_ignore_ascii_case("br") {
                    lines.push(strip_html_for_measurement(&s[start..i]));
                    i += rel_end + 1;
                    start = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    lines.push(strip_html_for_measurement(&s[start..]));
    lines
}

/// Strip HTML tags and decode common entities to mirror jsdom's `textContent`
/// for label width measurement. A `<` only starts a tag when followed by an
/// ASCII letter or `/letter` (so `<<`, `<1`, `<!` stay literal); entities
/// decode to their character; inline `\n` is dropped (textContent whitespace
/// collapsed by the single-line measure shim).
pub fn strip_html_for_measurement(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let next = bytes.get(i + 1).copied();
            let is_tag_start = match next {
                Some(c) if c.is_ascii_alphabetic() => true,
                Some(b'/') => bytes
                    .get(i + 2)
                    .map(|c| c.is_ascii_alphabetic())
                    .unwrap_or(false),
                _ => false,
            };
            if is_tag_start {
                if let Some(rel_end) = s[i..].find('>') {
                    i += rel_end + 1;
                    continue;
                }
            }
            out.push('<');
            i += 1;
        } else if bytes[i] == b'&' {
            if let Some(semi_rel) = s[i..].find(';') {
                let entity = &s[i + 1..i + semi_rel];
                let ch = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some('\u{00A0}'),
                    _ => None,
                };
                if let Some(c) = ch {
                    out.push(c);
                    i += semi_rel + 1;
                    continue;
                }
            }
            out.push('&');
            i += 1;
        } else if bytes[i] == b'\n' {
            i += 1;
        } else {
            let mut len = 1usize;
            while len < 4 && i + len < bytes.len() && (bytes[i + len] & 0xC0) == 0x80 {
                len += 1;
            }
            out.push_str(&s[i..i + len]);
            i += len;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // CJK literals are spelled as `\u{...}` escapes throughout: repo convention
    // keeps code files ASCII-only. The trailing comment names each glyph.
    const ZHANG: char = '\u{8D26}'; // CJK ideograph
    const HIRAGANA_A: char = '\u{3042}';
    const KATAKANA_KA: char = '\u{30AB}';
    const FULLWIDTH_A: char = '\u{FF21}';
    const IDEOGRAPHIC_SPACE: char = '\u{3000}';
    /// "account sync" in Chinese: 4 ideographs.
    const ZHANG_HU_TONG_BU: &str = "\u{8D26}\u{6237}\u{540C}\u{6B65}";
    /// "hello" in Korean: 5 Hangul syllables.
    const ANNYEONGHASEYO: &str = "\u{C548}\u{B155}\u{D558}\u{C138}\u{C694}";

    #[test]
    fn wide_glyph_detects_cjk_and_fullwidth() {
        // CJK ideographs + kana + fullwidth latin/space all render ~1em.
        assert!(is_wide_glyph(ZHANG));
        assert!(is_wide_glyph(HIRAGANA_A));
        assert!(is_wide_glyph(KATAKANA_KA));
        assert!(is_wide_glyph(FULLWIDTH_A));
        assert!(is_wide_glyph(IDEOGRAPHIC_SPACE));
        // ASCII / Latin-1 render narrower and must not trigger the floor.
        assert!(!is_wide_glyph('A'));
        assert!(!is_wide_glyph(' '));
        assert!(!is_wide_glyph('-'));
        assert!(!is_wide_glyph('\u{E9}')); // e-acute
    }

    #[test]
    fn wide_glyph_covers_scripts_the_range_list_missed() {
        assert!(is_wide_glyph('\u{AC00}')); // Hangul syllable
        assert!(is_wide_glyph('\u{1100}')); // Hangul Jamo
        assert!(is_wide_glyph('\u{3131}')); // Hangul compatibility Jamo
        assert!(is_wide_glyph('\u{20000}')); // CJK Ext B
        assert!(is_wide_glyph('\u{3105}')); // Bopomofo
        assert!(is_wide_glyph('\u{3231}')); // Enclosed CJK
    }

    #[test]
    fn halfwidth_forms_are_not_wide() {
        // U+FF61..=U+FF9F are *halfwidth*; the old 0xFF00..=0xFFEF range
        // over-reserved them at 2x their painted width.
        assert!(!is_wide_glyph('\u{FF71}')); // halfwidth katakana KA-row A
        assert!(!is_wide_glyph('\u{FF61}')); // halfwidth ideographic full stop
    }

    #[test]
    fn korean_label_now_gets_a_floor() {
        // 5 Hangul syllables -> 5 x EM = 80. The range-list version matched no
        // character here and returned the shim value untouched.
        assert_eq!(cjk_aware_label_width(ANNYEONGHASEYO, 44.8), 80.0);
    }

    #[test]
    fn cjk_floors_to_one_em_per_glyph() {
        // 4 CJK glyphs -> 4 x EM = 64; shim (~28) is below, so floor wins.
        assert_eq!(cjk_aware_label_width(ZHANG_HU_TONG_BU, 28.0), 64.0);
    }

    #[test]
    fn cjk_keeps_shim_when_already_wider() {
        // If the shim measured wider than the per-glyph estimate, keep the shim.
        assert_eq!(cjk_aware_label_width(ZHANG_HU_TONG_BU, 80.0), 80.0);
    }

    #[test]
    fn latin_estimates_below_cjk_floor() {
        // Latin glyphs estimate ~0.56em; "ok" -> 2 x 16 x 0.56 = 17.92.
        // (Call sites gate on is_wide_glyph, so pure-Latin labels never hit
        // this path; this pins the coefficient for mixed labels.)
        assert!((cjk_aware_label_width("ok", 12.0) - 17.92).abs() < 1e-9);
    }

    #[test]
    fn mixed_takes_max_of_estimate_and_shim() {
        let mixed = format!("A{ZHANG}");
        // 0.56em + 1em = 24.96; shim 10 -> 24.96.
        assert!((cjk_aware_label_width(&mixed, 10.0) - 24.96).abs() < 1e-9);
        // Same estimate, but shim already larger -> keep shim.
        assert_eq!(cjk_aware_label_width(&mixed, 40.0), 40.0);
    }

    #[test]
    fn empty_returns_shim() {
        assert_eq!(cjk_aware_label_width("", 5.0), 5.0);
    }

    #[test]
    fn plain_lines_split_on_every_br_form_and_newline() {
        assert_eq!(
            plain_text_lines("a<br/>b<BR>c<br />d\ne"),
            ["a", "b", "c", "d", "e"]
        );
        assert_eq!(plain_text_lines("<b>x</b> &amp; y"), ["x & y"]);
        assert_eq!(plain_text_lines(""), [""]);
    }

    #[test]
    fn strip_removes_tags_and_decodes_entities() {
        assert_eq!(strip_html_for_measurement("a<b>b</b>c"), "abc");
        assert_eq!(strip_html_for_measurement("x<br/>y"), "xy");
        assert_eq!(strip_html_for_measurement("a&amp;b"), "a&b");
        assert_eq!(strip_html_for_measurement("1<2"), "1<2"); // literal '<', not a tag
        assert_eq!(strip_html_for_measurement("a\nb"), "ab"); // inline newline dropped
    }

    #[test]
    fn plain_text_strips_markup_before_width_floor() {
        // Markdown bold wraps the 4 ideographs — the floor must see the glyphs
        // only, not the `**` markers (which would bill 2 × 0.56em extra and
        // re-inflate the box past the painted width).
        let bold = format!("**{ZHANG_HU_TONG_BU}**");
        assert_eq!(edge_label_plain_text(&bold, true), ZHANG_HU_TONG_BU);
        // A `<br/>` between glyphs is a tag, not 5 painted chars.
        let br = "\u{8D26}\u{6237}<br/>\u{540C}\u{6B65}";
        assert_eq!(edge_label_plain_text(br, false), ZHANG_HU_TONG_BU);
        // `&amp;` decodes to a single `&`, so the entity isn't billed as 5 chars.
        let ent = "\u{8D26}\u{6237}&amp;\u{540C}\u{6B65}";
        assert_eq!(
            edge_label_plain_text(ent, false),
            "\u{8D26}\u{6237}&\u{540C}\u{6B65}"
        );
    }
}
