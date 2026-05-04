//! KaTeX integration — byte-exact LaTeX math rendering.
//!
//! Mermaid pipes every `$$..$$` segment through KaTeX (with `forceLegacyMathML`
//! we drive the same code path even in jsdom). To stay byte-exact with the
//! reference SVGs, we embed `katex.min.js` and run it inside QuickJS via
//! `rquickjs`. KaTeX's `renderToString` is SSR-safe — it never reaches for
//! `document` / `window` — so no DOM polyfill is required.
//!
//! After KaTeX renders, mermaid post-processes the markup:
//!
//! 1. `text.replace(/\n/g, ' ')`
//! 2. `text.replace(/<annotation.*<\/annotation>/g, '')`
//! 3. `DOMPurify.sanitize(text, { FORBID_TAGS: ['style'] })` — re-parses the
//!    HTML through DOMPurify and re-serialises, producing cosmetic differences
//!    (strips `<semantics>`, expands self-closing SVG tags, normalises NBSP
//!    inside `<mtext>`). We replicate the equivalent transformations in pure
//!    Rust without pulling in a DOM — see `sanitize.rs`.
//!
//! Feature-gated under `katex`. Without the feature the module is absent and
//! the renderer falls back to the literal `MathML is unsupported in this
//! environment.` placeholder produced by mermaid's jsdom default config.

pub mod render;
pub mod sanitize;

pub use render::{render, RenderError};
pub use sanitize::sanitize;

/// Mermaid's `katexRegex` — `/\$\$(.*)\$\$/g`. The capture is greedy, but
/// in mermaid's per-line replace path each line carries at most one
/// `$$..$$` segment that we want to expand. We intentionally use a manual
/// finder rather than the `regex` crate's lookahead (none needed here)
/// to keep this hot path simple.
fn has_katex(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'$' && bytes[i + 1] == b'$' {
            // need a closing `$$` later in the same line
            for j in (i + 2)..bytes.len() - 1 {
                if bytes[j] == b'$' && bytes[j + 1] == b'$' {
                    return true;
                }
            }
            return false;
        }
        i += 1;
    }
    false
}

/// Expand each `$$..$$` occurrence in `line` by running KaTeX. Mermaid's
/// regex is *greedy* (`/\$\$(.*)\$\$/g`); applied per line that means a
/// line like `$$a$$ + $$b$$` collapses both pairs into one match
/// containing `a$$ + $$b`. We mirror that by always matching from the
/// first `$$` to the *last* `$$` on the line.
fn replace_katex_in_line(line: &str) -> Result<String, RenderError> {
    let bytes = line.as_bytes();
    let mut first = None;
    let mut last = None;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'$' && bytes[i + 1] == b'$' {
            if first.is_none() {
                first = Some(i);
            }
            last = Some(i);
            i += 2;
        } else {
            i += 1;
        }
    }
    let (Some(start), Some(end)) = (first, last) else {
        return Ok(line.to_owned());
    };
    if end <= start + 2 {
        // `$$$$` — empty math, leave as-is rather than feeding KaTeX an
        // empty string that throws ParseError.
        return Ok(line.to_owned());
    }
    let prefix = &line[..start];
    let body = &line[start + 2..end];
    let suffix = &line[end + 2..];
    let katex_html = render::render(body, true)?;
    // Mermaid post-processes KaTeX output before splicing it back in:
    // ```
    // .replace(/\n/g, ' ').replace(/<annotation.*<\/annotation>/g, '')
    // ```
    // We bake those into `sanitize()` (alongside the DOMPurify equivalent),
    // so do it inline here too — the outer `sanitize` call below sees a
    // mix of KaTeX HTML and the mermaid `<div>` wrapper, and applying the
    // post-process to the KaTeX HTML before splicing matches mermaid's
    // ordering exactly.
    let mut cleaned = katex_html.replace('\n', " ");
    cleaned = sanitize::strip_annotation_pub(&cleaned);
    Ok(format!("{}{}{}", prefix, cleaned, suffix))
}

/// Render a label string the same way mermaid's `renderKatexSanitized`
/// does for `forceLegacyMathML: true` + `securityLevel: 'loose'`:
///
/// 1. Split on `\n`. Wrap each line in a `<div>` (or a flex `<div>` when
///    the line contains `$$..$$`).
/// 2. Replace each `$$..$$` with `katex.renderToString(.., displayMode:true,
///    output:'htmlAndMathml')`, then drop newlines and strip `<annotation>`.
/// 3. Join the wrapped lines and run the whole string through our
///    DOMPurify equivalent.
pub fn render_label(text: &str) -> Result<String, RenderError> {
    let mut wrapped = String::with_capacity(text.len() + 64);
    for line in text.split('\n') {
        if has_katex(line) {
            let inner = replace_katex_in_line(line)?;
            wrapped.push_str(
                r#"<div style="display: flex; align-items: center; justify-content: center; white-space: nowrap;">"#,
            );
            wrapped.push_str(&inner);
            wrapped.push_str("</div>");
        } else {
            wrapped.push_str("<div>");
            wrapped.push_str(line);
            wrapped.push_str("</div>");
        }
    }
    Ok(sanitize::dompurify_equivalent(&wrapped))
}
