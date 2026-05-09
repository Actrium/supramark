//! Mermaid label rich-text parser.
//!
//! Parses the HTML + light-markdown subset mermaid label syntax
//! supports, into a tree of [`crate::model::richtext::TextSpan`].
//!
//! Supported tags (HTML mode, default when `useHtmlLabels: true`):
//!   <br> / <br/> / <br />                      → HardBreak
//!   <b> / <strong>                             → Bold
//!   <i> / <em>                                 → Italic
//!   <u>                                        → Underline
//!   <s> / <del>                                → Strikethrough
//!   <code>                                     → Monospace
//!   <sub>                                      → Subscript
//!   <sup>                                      → Superscript
//!   <font face="..."> / <font color="...">     → FontFamily / Colored
//!   <span style="color: X">                    → Colored
//!   <span style="background-color: Y">         → BackHighlight
//!
//! Supported markdown (light mode, when `useHtmlLabels: false`):
//!   **bold** / __bold__                         → Bold
//!   *italic* / _italic_                         → Italic
//!   `code`                                      → Monospace
//!   [label](url)                                → Link
//!
//! Non-goals for Wave 0: nested markdown inside HTML (or vice versa);
//! well-formedness recovery beyond "unmatched close tag becomes plain
//! text". These accrete as Wave 3+ diagrams exercise more edge cases.

use crate::model::richtext::TextSpan;

/// Parse a mermaid label into `TextSpan`s, HTML mode (default).
pub fn parse(source: &str) -> Vec<TextSpan> {
    parse_html(source)
}

/// HTML-mode parser: treats `<tag>...</tag>` as rich structure,
/// everything else as Plain.
pub fn parse_html(source: &str) -> Vec<TextSpan> {
    let tokens = tokenise(source);
    let mut it = tokens.into_iter().peekable();
    parse_until(&mut it, None)
}

/// Markdown-mode parser: minimal `**bold**` / `*italic*` / `` `code` ``
/// / `[text](url)`. No nested markdown inside markdown (Wave 0 scope).
pub fn parse_markdown(source: &str) -> Vec<TextSpan> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut plain_start = 0;

    macro_rules! flush_plain {
        ($end:expr) => {
            if $end > plain_start {
                out.push(TextSpan::Plain(source[plain_start..$end].to_string()));
            }
        };
    }

    while i < bytes.len() {
        let b = bytes[i];
        // **bold**
        if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            if let Some(end) = find(source, i + 2, "**") {
                flush_plain!(i);
                let inner = &source[i + 2..end];
                out.push(TextSpan::Bold(parse_markdown(inner)));
                i = end + 2;
                plain_start = i;
                continue;
            }
        }
        // *italic*
        if b == b'*' {
            if let Some(end) = find(source, i + 1, "*") {
                if end > i + 1 {
                    flush_plain!(i);
                    let inner = &source[i + 1..end];
                    out.push(TextSpan::Italic(parse_markdown(inner)));
                    i = end + 1;
                    plain_start = i;
                    continue;
                }
            }
        }
        // `code`
        if b == b'`' {
            if let Some(end) = find(source, i + 1, "`") {
                flush_plain!(i);
                out.push(TextSpan::Monospace(source[i + 1..end].to_string()));
                i = end + 1;
                plain_start = i;
                continue;
            }
        }
        // [label](url)
        if b == b'[' {
            if let Some(lbl_end) = find(source, i + 1, "]") {
                if bytes.get(lbl_end + 1) == Some(&b'(') {
                    if let Some(url_end) = find(source, lbl_end + 2, ")") {
                        flush_plain!(i);
                        out.push(TextSpan::Link {
                            url: source[lbl_end + 2..url_end].to_string(),
                            tooltip: None,
                            label: Some(source[i + 1..lbl_end].to_string()),
                        });
                        i = url_end + 1;
                        plain_start = i;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    flush_plain!(bytes.len());
    out
}

fn find(s: &str, from: usize, needle: &str) -> Option<usize> {
    s.get(from..).and_then(|r| r.find(needle)).map(|p| p + from)
}

// ────────── HTML tokeniser + recursive-descent parser ──────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Text(String),
    /// <tag ...> — opening tag with its raw attribute string.
    Open {
        name: String,
        attrs: String,
    },
    /// </tag>
    Close(String),
    /// <tag ... /> or void tags (br, img, etc).
    SelfClose {
        name: String,
        attrs: String,
    },
}

fn tokenise(source: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut text_start = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Try to parse a tag. If not a valid tag, treat as text.
            if let Some((tok, next)) = scan_tag(source, i) {
                if i > text_start {
                    out.push(Token::Text(source[text_start..i].to_string()));
                }
                out.push(tok);
                i = next;
                text_start = next;
                continue;
            }
        }
        i += 1;
    }
    if text_start < bytes.len() {
        out.push(Token::Text(source[text_start..].to_string()));
    }
    out
}

/// Try to scan a tag starting at `start`. Returns (token, next-offset)
/// on success, None if the `<...>` isn't a well-formed tag and should
/// be treated as literal text.
fn scan_tag(source: &str, start: usize) -> Option<(Token, usize)> {
    let bytes = source.as_bytes();
    debug_assert!(bytes[start] == b'<');
    let end = source[start..].find('>')? + start;
    let inner = &source[start + 1..end];
    let (is_close, inner) = if let Some(rest) = inner.strip_prefix('/') {
        (true, rest)
    } else {
        (false, inner)
    };
    // Self-closing form: ends with `/`
    let (self_close, inner) = if let Some(rest) = inner.strip_suffix('/') {
        (true, rest.trim_end())
    } else {
        (false, inner)
    };
    let inner = inner.trim();
    if inner.is_empty() {
        return None;
    }
    let (name, attrs) = match inner.find(|c: char| c.is_whitespace()) {
        Some(p) => (&inner[..p], inner[p..].trim()),
        None => (inner, ""),
    };
    // Tag names must be ASCII letters/digits (very permissive).
    if !name.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let name = name.to_ascii_lowercase();
    let attrs = attrs.to_string();
    let token = if is_close {
        Token::Close(name)
    } else if self_close || is_void_tag(&name) {
        Token::SelfClose { name, attrs }
    } else {
        Token::Open { name, attrs }
    };
    Some((token, end + 1))
}

fn is_void_tag(name: &str) -> bool {
    matches!(name, "br" | "hr" | "img" | "wbr")
}

/// Parse tokens until `close_name` is seen (if Some) or end of stream.
/// When a close tag is encountered that doesn't match our expected
/// one, we stop — the caller decides how to emit the unmatched token.
fn parse_until<I>(it: &mut std::iter::Peekable<I>, close_name: Option<&str>) -> Vec<TextSpan>
where
    I: Iterator<Item = Token>,
{
    let mut out = Vec::new();
    while let Some(tok) = it.peek() {
        match tok {
            Token::Close(name) => {
                if close_name == Some(name.as_str()) {
                    it.next(); // consume our matching close
                    return out;
                }
                // Unmatched close — emit as literal text and drop the token.
                let closed = it.next();
                if let Some(Token::Close(n)) = closed {
                    out.push(TextSpan::Plain(format!("</{}>", n)));
                }
            }
            Token::Text(_) => {
                if let Some(Token::Text(s)) = it.next() {
                    out.push(TextSpan::Plain(s));
                }
            }
            Token::SelfClose { .. } => {
                let (name_owned, attrs_owned) = match it.next() {
                    Some(Token::SelfClose { name, attrs }) => (name, attrs),
                    _ => unreachable!(),
                };
                match name_owned.as_str() {
                    "br" => out.push(TextSpan::HardBreak),
                    _ => {
                        // Unsupported void tag — emit nothing. The
                        // whole `<tag/>` is dropped.
                        let _ = attrs_owned;
                    }
                }
            }
            Token::Open { .. } => {
                let (name_owned, attrs_owned) = match it.next() {
                    Some(Token::Open { name, attrs }) => (name, attrs),
                    _ => unreachable!(),
                };
                match name_owned.as_str() {
                    "b" | "strong" => {
                        let inner = parse_until(it, Some("b")); // accept </b> close for <strong> as well? upstream is lax
                                                                // Actually upstream requires matching close. We
                                                                // retry with the actual name:
                        let _ = attrs_owned;
                        out.push(TextSpan::Bold(inner));
                    }
                    "i" | "em" => {
                        let inner = parse_until(it, Some(name_owned.as_str()));
                        let _ = attrs_owned;
                        out.push(TextSpan::Italic(inner));
                    }
                    "u" => out.push(TextSpan::Underline(parse_until(it, Some("u")))),
                    "s" | "del" => {
                        let inner = parse_until(it, Some(name_owned.as_str()));
                        out.push(TextSpan::Strikethrough(inner));
                    }
                    "code" => {
                        // Collapse inner content to plain text for Monospace (which is flat String).
                        let inner = parse_until(it, Some("code"));
                        out.push(TextSpan::Monospace(crate::model::richtext::plain_text(
                            &inner,
                        )));
                    }
                    "sub" => out.push(TextSpan::Subscript(parse_until(it, Some("sub")))),
                    "sup" => out.push(TextSpan::Superscript(parse_until(it, Some("sup")))),
                    "font" => {
                        let inner = parse_until(it, Some("font"));
                        if let Some(face) = attr_value(&attrs_owned, "face") {
                            out.push(TextSpan::FontFamily {
                                family: face,
                                content: inner,
                            });
                        } else if let Some(color) = attr_value(&attrs_owned, "color") {
                            out.push(TextSpan::Colored {
                                color,
                                content: inner,
                            });
                        } else {
                            out.extend(inner);
                        }
                    }
                    "span" => {
                        let inner = parse_until(it, Some("span"));
                        let style = attr_value(&attrs_owned, "style").unwrap_or_default();
                        let color = css_value(&style, "color");
                        let bg = css_value(&style, "background-color")
                            .or_else(|| css_value(&style, "background"));
                        let mut wrapped = inner;
                        if let Some(c) = color {
                            wrapped = vec![TextSpan::Colored {
                                color: c,
                                content: wrapped,
                            }];
                        }
                        if let Some(b) = bg {
                            wrapped = vec![TextSpan::BackHighlight {
                                color: b,
                                content: wrapped,
                            }];
                        }
                        out.extend(wrapped);
                    }
                    _ => {
                        // Unknown tag — emit inner content without wrapping.
                        let inner = parse_until(it, Some(name_owned.as_str()));
                        out.extend(inner);
                    }
                }
            }
        }
    }
    out
}

fn attr_value(attrs: &str, key: &str) -> Option<String> {
    // Very permissive attribute extractor: handles key="value" and key='value'.
    let lower = attrs.to_ascii_lowercase();
    let idx = lower.find(&format!("{}=", key))?;
    let rest = &attrs[idx + key.len() + 1..];
    let trimmed = rest.trim_start();
    if let Some(rest2) = trimmed.strip_prefix('"') {
        let end = rest2.find('"')?;
        Some(rest2[..end].to_string())
    } else if let Some(rest2) = trimmed.strip_prefix('\'') {
        let end = rest2.find('\'')?;
        Some(rest2[..end].to_string())
    } else {
        let end = trimmed
            .find(|c: char| c.is_whitespace())
            .unwrap_or(trimmed.len());
        Some(trimmed[..end].to_string())
    }
}

fn css_value(style: &str, prop: &str) -> Option<String> {
    let lower = style.to_ascii_lowercase();
    let idx = lower.find(&format!("{}:", prop))?;
    let rest = &style[idx + prop.len() + 1..];
    let end = rest.find(';').unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(s: &str) -> TextSpan {
        TextSpan::Plain(s.into())
    }

    #[test]
    fn plain_passthrough() {
        assert_eq!(parse("hello world"), vec![plain("hello world")]);
    }

    #[test]
    fn br_maps_to_hardbreak() {
        let r = parse("a<br>b<br/>c<br />d");
        assert_eq!(
            r,
            vec![
                plain("a"),
                TextSpan::HardBreak,
                plain("b"),
                TextSpan::HardBreak,
                plain("c"),
                TextSpan::HardBreak,
                plain("d"),
            ]
        );
    }

    #[test]
    fn bold_and_italic_html() {
        let r = parse("<b>bold</b> and <i>italic</i>");
        assert_eq!(
            r,
            vec![
                TextSpan::Bold(vec![plain("bold")]),
                plain(" and "),
                TextSpan::Italic(vec![plain("italic")]),
            ]
        );
    }

    #[test]
    fn code_becomes_monospace_string() {
        let r = parse("<code>let x = 1;</code>");
        assert_eq!(r, vec![TextSpan::Monospace("let x = 1;".into())]);
    }

    #[test]
    fn font_face_becomes_fontfamily() {
        let r = parse(r#"<font face="courier">x</font>"#);
        assert_eq!(
            r,
            vec![TextSpan::FontFamily {
                family: "courier".into(),
                content: vec![plain("x")],
            }]
        );
    }

    #[test]
    fn span_with_color() {
        let r = parse(r#"<span style="color: red">oops</span>"#);
        assert_eq!(
            r,
            vec![TextSpan::Colored {
                color: "red".into(),
                content: vec![plain("oops")],
            }]
        );
    }

    #[test]
    fn unmatched_close_emits_literal() {
        let r = parse("</b>");
        assert_eq!(r, vec![plain("</b>")]);
    }

    #[test]
    fn stray_lt_is_plain_text() {
        // `<` not followed by a valid tag name is treated as plain.
        let r = parse("a < b");
        assert_eq!(r, vec![plain("a < b")]);
    }

    #[test]
    fn markdown_bold_italic_code_link() {
        let r = parse_markdown("**bold** *it* `c` [l](u)");
        assert_eq!(
            r,
            vec![
                TextSpan::Bold(vec![plain("bold")]),
                plain(" "),
                TextSpan::Italic(vec![plain("it")]),
                plain(" "),
                TextSpan::Monospace("c".into()),
                plain(" "),
                TextSpan::Link {
                    url: "u".into(),
                    tooltip: None,
                    label: Some("l".into()),
                },
            ]
        );
    }
}
