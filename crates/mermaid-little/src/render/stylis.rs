//! Minimal stylis CSS minifier port.
//!
//! Upstream: [stylis](https://github.com/thysultan/stylis) (MIT, Sultan
//! Tarimo). Mermaid uses it via `serialize(compile(css), stringify)` to
//! produce the `<style>` block embedded in every rendered SVG.
//!
//! Byte-exact parity with mermaid@11.14.0 is the goal; we only need to
//! cover the subset of CSS features mermaid's theme system actually
//! emits. Concretely that means:
//!
//! * Strip `/* ... */` block comments.
//! * Collapse whitespace between tokens while preserving strings.
//! * Strip single space characters following commas **outside** any
//!   double-quoted region (the load-bearing pass for font-family lists
//!   like `"trebuchet ms", verdana, arial, sans-serif` → `"trebuchet
//!   ms",verdana,arial,sans-serif`).
//! * Leave everything else untouched.
//!
//! A full stylis port (state machine, nesting, `&`-prefix expansion)
//! is out of scope — mermaid's CSS is already hand-written to be flat,
//! so we only need the token-level minification passes.
//!
//! ## Known limitation
//!
//! [`minify`] does **not** preserve spaces inside parenthesised CSS
//! values like `hsl(80, 100%, 95%)` or `rgba(232, 232, 232, 0.8)` —
//! it strips them along with the structural `, ` collapse. Upstream
//! stylis keeps those spaces. The Wave-4 renderers therefore hand-
//! author their CSS literal strings rather than pipe them through
//! `minify`; [`strip_comma_spaces`] is the precise helper most
//! renderers actually want (only affects font-family lists).
//!
//! License attribution: based on stylis (MIT).

/// Token-level minifier. Drops block comments and redundant whitespace
/// around CSS separators while preserving every character inside
/// double-quoted strings. Does **not** re-indent or add newlines —
/// output is single-line.
#[must_use]
pub fn minify(css: &str) -> String {
    let bytes = css.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    // Track previous emitted byte so we can skip redundant whitespace
    // following `{`, `;`, `:`, `,`, `}`.
    let mut prev: u8 = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // Block comment: skip until `*/`.
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        // Double-quoted string — verbatim.
        if c == b'"' {
            out.push('"');
            i += 1;
            while i < bytes.len() {
                let ch = bytes[i];
                out.push(ch as char);
                if ch == b'\\' && i + 1 < bytes.len() {
                    out.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                }
                i += 1;
                if ch == b'"' {
                    break;
                }
            }
            prev = b'"';
            continue;
        }
        // Whitespace: collapse runs, drop after structural punctuation.
        if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
            // Swallow the run.
            let start_prev = prev;
            while i < bytes.len()
                && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n' || bytes[i] == b'\r')
            {
                i += 1;
            }
            if i >= bytes.len() {
                continue;
            }
            let next = bytes[i];
            // Drop whitespace adjacent to structural punctuation
            // (upstream stylis `STRIP` pass).
            if matches!(start_prev, b'{' | b'}' | b';' | b':' | b',' | 0)
                || matches!(next, b'{' | b'}' | b';' | b':' | b',')
            {
                continue;
            }
            out.push(' ');
            prev = b' ';
            continue;
        }
        out.push(c as char);
        prev = c;
        i += 1;
    }
    out
}

/// Scope every top-level CSS rule under `#<scope>`.
///
/// Upstream mermaid wraps the whole `getStyles()` return value with
/// `${svgId}{...}` before handing it to stylis, which then rewrites
/// every nested `&` / bare selector to prefix with the svg id. Our
/// CSS is already flat (no `&` nesting) so the transformation reduces
/// to: for every top-level selector list, emit `#<scope> <selector>`.
///
/// This helper is intentionally narrow — it expects the CSS to already
/// have any `& .foo` / `&-x` patterns expanded by the caller. Mermaid's
/// per-diagram CSS templates use the `& .x` idiom, so the caller
/// should strip the leading `& ` before invoking this function.
#[must_use]
pub fn scope_css(css: &str, scope: &str) -> String {
    // Minimal top-level selector rewrite. We walk the CSS, finding
    // `{` boundaries; for each preceding selector list we prepend
    // `#<scope> ` to each comma-separated selector (unless it already
    // starts with `#<scope>` or is an `@`-rule).
    let mut out = String::with_capacity(css.len() + 64);
    let bytes = css.as_bytes();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut seg_start = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' {
            // Skip string.
            i += 1;
            while i < bytes.len() {
                let ch = bytes[i];
                i += 1;
                if ch == b'\\' && i < bytes.len() {
                    i += 1;
                    continue;
                }
                if ch == b'"' {
                    break;
                }
            }
            continue;
        }
        if c == b'{' {
            if depth == 0 {
                // Selector list at `css[seg_start..i]`.
                let selectors = &css[seg_start..i];
                rewrite_selectors(selectors, scope, &mut out);
                out.push('{');
                seg_start = i + 1;
            }
            // When depth > 0 the `{` is part of a nested body we will
            // flush verbatim on the outer `}`.
            depth += 1;
            i += 1;
            continue;
        }
        if c == b'}' {
            if depth == 1 {
                // Emit body up to here verbatim (includes any nested
                // braces like `@keyframes` inner rules).
                out.push_str(&css[seg_start..i]);
                seg_start = i + 1;
                out.push('}');
            }
            depth -= 1;
            i += 1;
            continue;
        }
        // Nested content: we emit it lazily when the outer `}` arrives,
        // so skip here.
        i += 1;
    }
    // Any trailing chunk outside braces.
    if seg_start < bytes.len() {
        out.push_str(&css[seg_start..]);
    }
    out
}

fn rewrite_selectors(selectors: &str, scope: &str, out: &mut String) {
    let trimmed = selectors.trim();
    if trimmed.is_empty() {
        return;
    }
    // At-rules (`@keyframes`, `@media`, …) pass through unchanged.
    if trimmed.starts_with('@') {
        out.push_str(trimmed);
        return;
    }
    for (i, sel) in trimmed.split(',').enumerate() {
        if i > 0 {
            out.push(',');
        }
        let sel = sel.trim();
        if sel.is_empty() {
            continue;
        }
        // Treat leading `&` as the scope anchor — strip it.
        let sel = sel.strip_prefix('&').unwrap_or(sel).trim_start();
        out.push('#');
        out.push_str(scope);
        if !sel.is_empty() {
            out.push(' ');
            out.push_str(sel);
        }
    }
}

/// Strip single-space after comma outside of double-quoted strings.
/// Useful for normalising font-family lists in theme variables so that
/// the value appears byte-identical to the minified upstream output.
#[must_use]
pub fn strip_comma_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_quote = false;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' {
            in_quote = !in_quote;
            out.push('"');
            i += 1;
            continue;
        }
        if !in_quote && c == b',' {
            out.push(',');
            i += 1;
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_stripped() {
        let css = "a{color:red; /* hi */ fill:blue;}";
        let got = minify(css);
        assert_eq!(got, "a{color:red;fill:blue;}");
    }

    #[test]
    fn whitespace_collapsed() {
        let css = "  a   {  color : red ;  }  ";
        let got = minify(css);
        // Note: whitespace around `:` is dropped as per structural rule.
        assert_eq!(got, "a{color:red;}");
    }

    #[test]
    fn string_preserved() {
        let css = "a{font-family: \"Comic Sans\",   serif;}";
        // minify alone keeps the inner spacing untouched inside the
        // quoted region; the comma-space between quoted tokens is also
        // inside the selector body (outside quotes) so it collapses.
        let got = minify(css);
        // Space between `,` and `serif` is adjacent to `,` so dropped.
        assert_eq!(got, "a{font-family:\"Comic Sans\",serif;}");
    }

    #[test]
    fn scope_prepends_id() {
        let css = ".foo{color:red;}";
        assert_eq!(scope_css(css, "svg-1"), "#svg-1 .foo{color:red;}");
    }

    #[test]
    fn scope_handles_comma_selectors() {
        let css = ".a,.b{color:red;}";
        assert_eq!(scope_css(css, "x"), "#x .a,#x .b{color:red;}");
    }

    #[test]
    fn scope_passes_atrule_through() {
        let css = "@keyframes dash{from{x:0;}to{x:1;}}";
        // Entire atrule body survives the traversal, including its inner braces.
        let got = scope_css(css, "x");
        assert_eq!(got, "@keyframes dash{from{x:0;}to{x:1;}}");
    }

    #[test]
    fn strip_comma_spaces_outside_quotes() {
        let input = r#""trebuchet ms", verdana, arial, sans-serif"#;
        let got = strip_comma_spaces(input);
        assert_eq!(got, r#""trebuchet ms",verdana,arial,sans-serif"#);
    }

    #[test]
    fn strip_comma_spaces_leaves_quoted_spaces_alone() {
        let input = r#""foo, bar", baz"#;
        let got = strip_comma_spaces(input);
        assert_eq!(got, r#""foo, bar",baz"#);
    }

    #[test]
    fn minify_er_entity_block() {
        let css = "
  .entityBox {
    fill: #ECECFF;
    stroke: #9370DB;
  }";
        let got = minify(css);
        assert_eq!(got, ".entityBox{fill:#ECECFF;stroke:#9370DB;}");
    }

    /// Known lossy behavior: our minifier is aggressive and drops
    /// `, ` → `,` even inside `hsl(...)` parens. Upstream stylis
    /// actually preserves those spaces (they appear in reference
    /// SVGs as `hsl(80, 100%, 96.2745098039%)`). This is why the
    /// Wave-4 diagrams hand-author their CSS rather than pipe it
    /// through `minify` — until we grow full value-aware handling
    /// the helper is safe only for CSS that has no parenthesised
    /// comma-separated arguments.
    #[test]
    fn minify_is_aggressive_inside_parens() {
        let css = ".x{fill:hsl(80, 100%, 95%);}";
        let got = minify(css);
        assert_eq!(got, ".x{fill:hsl(80,100%,95%);}");
    }
}
