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

/// Decode the three XML entities that the flowchart label pipeline emits
/// (via `xml_escape_label`): `&amp;`, `&lt;`, `&gt;`. KaTeX expects raw
/// LaTeX characters — these entities would otherwise be interpreted as the
/// literal letter sequences `amp`, `lt`, `gt` followed by `;`.
fn decode_basic_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        if let Some(semi) = after.find(';') {
            let entity = &after[1..semi];
            match entity {
                "amp" => out.push('&'),
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                _ => {
                    out.push('&');
                    out.push_str(&after[1..=semi]);
                }
            }
            rest = &after[semi + 1..];
        } else {
            out.push_str(after);
            return out;
        }
    }
    out.push_str(rest);
    out
}

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
fn replace_katex_in_line_ex(
    line: &str,
    collapse_double_backslash: bool,
) -> Result<String, RenderError> {
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
    // The label has been XML-escaped by the time it reaches us, so any `&`
    // (case-environment column splits, `\&` literal) inside the LaTeX is now
    // `&amp;`. Same for `<`/`>` (rare in math but possible in `\text`).
    // KaTeX needs the raw character — decode the basic three entities here
    // before handing the body to the JS renderer.
    let decoded = decode_basic_entities(body);
    // Mermaid's KaTeX-input shim — see chunk-7XTQR4JX.mjs:1978:
    //   const inputForKatex = text.replace(/\\\\/g, "\\");
    // i.e. collapse every `\\` to `\`. This converts e.g. `\\ ` (the
    // cases-environment row separator written in .mmd) into `\ ` (KaTeX's
    // thin-space command), which is what the upstream reference SVGs
    // contain.
    let katex_input = if collapse_double_backslash {
        decoded.replace("\\\\", "\\")
    } else {
        decoded
    };
    let katex_html = render::render(&katex_input, true)?;
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
///
/// This entry point applies mermaid's flowchart-specific
/// `\\\\ → \\` collapse (chunk-7XTQR4JX:1978's `inputForKatex`).
/// Sequence diagrams use `render_label_raw` instead because their
/// `drawKatex` call routes directly through `renderKatexSanitized`
/// without the collapse step.
pub fn render_label(text: &str) -> Result<String, RenderError> {
    render_label_inner(text, true)
}

/// Render a label without applying the `\\\\ → \\` collapse. Use this
/// from sequence-renderer paths (note text, message text, loop label),
/// where upstream feeds the raw `$$..$$` body directly to KaTeX.
pub fn render_label_raw(text: &str) -> Result<String, RenderError> {
    render_label_inner(text, false)
}

fn render_label_inner(text: &str, collapse_double_backslash: bool) -> Result<String, RenderError> {
    let mut wrapped = String::with_capacity(text.len() + 64);
    for line in split_line_breaks(text) {
        if has_katex(&line) {
            let inner = replace_katex_in_line_ex(&line, collapse_double_backslash)?;
            wrapped.push_str(
                r#"<div style="display: flex; align-items: center; justify-content: center; white-space: nowrap;">"#,
            );
            wrapped.push_str(&inner);
            wrapped.push_str("</div>");
        } else {
            wrapped.push_str("<div>");
            wrapped.push_str(&line);
            wrapped.push_str("</div>");
        }
    }
    Ok(sanitize::dompurify_equivalent(&wrapped))
}

/// Split `text` on the same `<br\s*\/?>/gi` regex mermaid uses
/// (`lineBreakRegex` in chunk-NCW2MGAP.mjs:6434). This is the
/// per-line splitter `renderKatexUnsanitized` calls before wrapping
/// each segment in its `<div>` (KaTeX-bearing) or plain `<div>` (no
/// math) wrapper.
fn split_line_breaks(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Check for `<br`/`<BR` followed by optional whitespace,
            // optional `/`, then `>`.
            let rest = &text[i..];
            let lower = rest.to_ascii_lowercase();
            if lower.starts_with("<br") {
                // Walk past `<br`.
                let mut j = i + 3;
                // Optional whitespace.
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                // Optional `/`.
                if j < bytes.len() && bytes[j] == b'/' {
                    j += 1;
                    while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                        j += 1;
                    }
                }
                if j < bytes.len() && bytes[j] == b'>' {
                    // Found a `<br>` tag — emit the segment before it.
                    out.push(text[start..i].to_string());
                    i = j + 1;
                    start = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    out.push(text[start..].to_string());
    out
}
