//! Pure-Rust equivalent of mermaid's KaTeX post-processing pipeline.
//!
//! Mermaid post-processes `katex.renderToString` output via:
//!
//! ```js
//! markup
//!   .replace(/\n/g, ' ')
//!   .replace(/<annotation.*<\/annotation>/g, '')
//! ```
//!
//! followed by `DOMPurify.sanitize(markup, { FORBID_TAGS: ['style'] })` with
//! mermaid's defaults (`securityLevel: 'loose'`). DOMPurify parses the markup
//! through an HTML5 DOM and re-serialises, producing several cosmetic byte-level
//! transformations that we mirror here without pulling in a DOM:
//!
//! 1. Strip `<semantics>` / `</semantics>` tags (preserving children) — the
//!    tag is not on the DOMPurify default MathML allow-list.
//! 2. Self-closing SVG/HTML tags (`<path .../>`, `<line .../>`, `<svg .../>`,
//!    `<rect .../>`) are re-serialised with explicit closers (`<path ...></path>`).
//!    HTML5 parsers don't recognise XML self-closing on non-void elements
//!    inside HTML content (which is what `<foreignObject>`'s subtree becomes
//!    after mermaid splices KaTeX into its `<div>`).
//! 3. Literal U+0020 inside `<mtext>` content normalises to `&nbsp;`.
//!
//! The implementation is a tiny purpose-built tokenizer rather than a full
//! HTML parser — KaTeX's output is a closed, well-formed subset and we only
//! need to surface those three transformations.

/// Apply mermaid's full post-processing pipeline to KaTeX markup. Kept for
/// callers and tests that want a one-shot replay of every step.
pub fn sanitize(markup: &str) -> String {
    let s1 = drop_newlines(markup);
    let s2 = strip_annotation(&s1);
    dompurify_equivalent(&s2)
}

/// Public wrapper for `strip_annotation` so the per-line replacer in
/// `mod.rs` can call it without re-exporting every internal helper.
pub fn strip_annotation_pub(s: &str) -> String {
    strip_annotation(s)
}

/// Mermaid's outer `DOMPurify.sanitize(..., { FORBID_TAGS: ['style'] })`
/// equivalent. Run on the assembled markup that wraps KaTeX output in
/// the per-line `<div>` blocks.
pub fn dompurify_equivalent(html: &str) -> String {
    let s1 = strip_semantics(html);
    let s2 = expand_self_closing(&s1);
    nbsp_to_entity(&s2)
}

fn drop_newlines(s: &str) -> String {
    s.replace('\n', " ")
}

/// Mirror mermaid's `replace(/<annotation.*<\/annotation>/g, '')`. The regex
/// is greedy and single-line — `.*` doesn't cross `\n`, but `drop_newlines`
/// already collapsed the input to one line, so the greedy match consumes
/// from the first `<annotation` to the LAST `</annotation>`. KaTeX emits a
/// single annotation per expression, but be safe and find them by scanning
/// for the last close tag.
fn strip_annotation(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("<annotation") {
        out.push_str(&rest[..start]);
        // Greedy: match through the LAST </annotation> in the remainder.
        let after = &rest[start..];
        let close = match after.rfind("</annotation>") {
            Some(c) => c + "</annotation>".len(),
            None => {
                out.push_str(after);
                return out;
            }
        };
        rest = &after[close..];
    }
    out.push_str(rest);
    out
}

/// Strip `<semantics>` and `</semantics>` tags (no attributes — KaTeX never
/// puts any on `<semantics>`), preserving inner content.
fn strip_semantics(s: &str) -> String {
    s.replace("<semantics>", "").replace("</semantics>", "")
}

/// Expand XML-style self-closing tags `<X .../>` that DOMPurify's HTML5
/// re-serialiser would write as `<X ...></X>`. Apply only to non-void
/// element names that KaTeX emits inside its SVG decorations:
/// `path`, `line`, `svg`, `rect`, `g`, `polygon`, `polyline`, `circle`,
/// `ellipse`, `text`, `tspan`, `defs`, `clipPath`, `use`. Void HTML
/// elements like `<br/>` stay untouched.
fn expand_self_closing(s: &str) -> String {
    const TAGS: &[&str] = &[
        "path", "line", "svg", "rect", "g", "polygon", "polyline", "circle", "ellipse",
        "text", "tspan", "defs", "clipPath", "use",
    ];
    let mut out = String::with_capacity(s.len() + 16);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Copy verbatim up to the next `<`. We work in byte indices for
        // O(1) lookups — `<` is ASCII so it always lands on a char
        // boundary, and the slice between `<` markers is also boundary
        // safe (no UTF-8 sequence is split).
        let lt = match s[i..].find('<') {
            Some(c) => i + c,
            None => {
                out.push_str(&s[i..]);
                break;
            }
        };
        out.push_str(&s[i..lt]);
        // Scan for the matching `>` of this tag.
        let close = match s[lt..].find('>') {
            Some(c) => lt + c,
            None => {
                out.push_str(&s[lt..]);
                break;
            }
        };
        let tag = &s[lt..=close];
        if tag.ends_with("/>") {
            // Read tag name after '<'. KaTeX never emits namespaced tags
            // with `:` here, but allow it defensively.
            let name_start = lt + 1;
            let mut name_end = name_start;
            while name_end < close && {
                let b = bytes[name_end];
                b.is_ascii_alphanumeric() || b == b':' || b == b'-'
            } {
                name_end += 1;
            }
            let name = &s[name_start..name_end];
            if TAGS.contains(&name) {
                // Replace `... />` with `...></name>`.
                out.push_str(&s[lt..close - 1]);
                out.push('>');
                out.push_str("</");
                out.push_str(name);
                out.push('>');
                i = close + 1;
                continue;
            }
        }
        out.push_str(tag);
        i = close + 1;
    }
    out
}

/// Replace literal NBSP (U+00A0) with the `&nbsp;` named entity. KaTeX
/// emits NBSP as a raw two-byte UTF-8 sequence (`0xC2 0xA0`) wherever a
/// non-breaking space is needed — inside `<mtext>` (for `\text{…}` content),
/// inside the `<span class="mord">` text-mode siblings, and inside
/// `<span class="mspace">` thin/medium/thick spaces. When DOMPurify
/// re-serialises the parsed DOM, every NBSP code point becomes `&nbsp;`,
/// regardless of which element it sits in.
///
/// ASCII U+0020 spaces are passed through unchanged — DOMPurify's HTML
/// serialiser only entity-escapes NBSP (and a couple of other control
/// chars KaTeX never emits).
///
/// We do not need to skip attribute values: KaTeX's output uses ASCII-only
/// class names, style values, and other attributes — there is never an
/// NBSP character on the attribute side.
fn nbsp_to_entity(s: &str) -> String {
    s.replace('\u{00A0}', "&nbsp;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_newlines_collapses_to_space() {
        assert_eq!(drop_newlines("a\nb\n"), "a b ");
    }

    #[test]
    fn strip_annotation_removes_block() {
        let s = "<math>X<annotation encoding=\"application/x-tex\">\\alpha</annotation>Y</math>";
        assert_eq!(strip_annotation(s), "<math>XY</math>");
    }

    #[test]
    fn strip_semantics_unwraps() {
        let s = "<math><semantics><mrow></mrow></semantics></math>";
        assert_eq!(strip_semantics(s), "<math><mrow></mrow></math>");
    }

    #[test]
    fn self_closing_path_expanded() {
        let s = r#"<svg><path d="M0,0L1,1"/></svg>"#;
        assert_eq!(
            expand_self_closing(s),
            r#"<svg><path d="M0,0L1,1"></path></svg>"#
        );
    }

    #[test]
    fn self_closing_void_br_kept() {
        let s = r#"<p>x<br/>y</p>"#;
        // `<br>` is HTML void, mermaid emits it as `<br/>` by convention —
        // DOMPurify preserves it. Our expander only touches the SVG list.
        assert_eq!(expand_self_closing(s), s);
    }

    #[test]
    fn nbsp_byte_to_entity() {
        // KaTeX emits NBSP (U+00A0) anywhere it needs a non-breaking space —
        // inside <mtext>, inside <span class="mord">, etc.
        let s = "<mrow><mtext>if\u{00A0}</mtext><mi>b</mi><span>x\u{00A0}y</span></mrow>";
        assert_eq!(
            nbsp_to_entity(s),
            "<mrow><mtext>if&nbsp;</mtext><mi>b</mi><span>x&nbsp;y</span></mrow>"
        );
    }

    #[test]
    fn ascii_space_kept() {
        // ASCII U+0020 stays as-is — DOMPurify only escapes NBSP.
        let s = "<mtext>a b</mtext>";
        assert_eq!(nbsp_to_entity(s), "<mtext>a b</mtext>");
    }

    #[test]
    fn full_pipeline_alpha() {
        // Real KaTeX 0.16.45 output for \alpha (single-line; \n already
        // would have been dropped by the time we see it in production).
        let raw = r#"<span class="katex-display"><span class="katex"><span class="katex-mathml"><math xmlns="http://www.w3.org/1998/Math/MathML" display="block"><semantics><mrow><mi>α</mi></mrow><annotation encoding="application/x-tex">\alpha</annotation></semantics></math></span><span class="katex-html"></span></span></span>"#;
        let got = sanitize(raw);
        assert!(!got.contains("<semantics>"), "semantics not stripped: {}", got);
        assert!(!got.contains("<annotation"), "annotation not stripped: {}", got);
    }
}
