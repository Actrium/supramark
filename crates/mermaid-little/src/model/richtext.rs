//! Rich-text span types used by label parsers.
//!
//! `TextSpan` enum adapted from plantuml-little
//! (https://github.com/kookyleo/plantuml-little) at commit b32d6aa,
//! MIT-compatible multi-license. Trimmed to the subset mermaid's
//! label markup exercises.

/// A span of inline text with optional formatting.
///
/// Mermaid's label markup is a markdown/HTML-ish subset: `**bold**`,
/// `*italic*`, `` `code` ``, `<br>`, raw hyperlinks, and a handful of
/// inline HTML tags. This enum covers that subset plus the font-family
/// / font-size / color / highlight fragments that upstream mermaid's
/// themes and HTML labels exercise.
#[derive(Debug, Clone, PartialEq)]
pub enum TextSpan {
    /// Plain unformatted text.
    Plain(String),
    /// Bold text: `**bold**` or `<b>bold</b>`.
    Bold(Vec<TextSpan>),
    /// Italic text: `*italic*` or `<i>italic</i>` or `<em>italic</em>`.
    Italic(Vec<TextSpan>),
    /// Underlined text: `<u>underline</u>`.
    Underline(Vec<TextSpan>),
    /// Strikethrough text: `~~strike~~` or `<s>strike</s>`.
    Strikethrough(Vec<TextSpan>),
    /// Monospaced text: `` `code` `` or `<code>code</code>`.
    Monospace(String),
    /// Hard line break (`<br>` / `<br/>`).
    HardBreak,
    /// Colored text (foreground color override).
    Colored {
        color: String,
        content: Vec<TextSpan>,
    },
    /// Background-highlighted text.
    BackHighlight {
        color: String,
        content: Vec<TextSpan>,
    },
    /// Sized text (font-size override, in CSS px-equivalent units).
    Sized { size: f64, content: Vec<TextSpan> },
    /// Subscript text: `<sub>text</sub>`.
    Subscript(Vec<TextSpan>),
    /// Superscript text: `<sup>text</sup>`.
    Superscript(Vec<TextSpan>),
    /// Font family change: `<font face="name">text</font>`.
    FontFamily {
        family: String,
        content: Vec<TextSpan>,
    },
    /// Hyperlink: `[label](url)` or a bare URL.
    Link {
        url: String,
        tooltip: Option<String>,
        label: Option<String>,
    },
}

/// Extract plain-text content from a slice of spans, stripping all formatting.
pub fn plain_text(spans: &[TextSpan]) -> String {
    let mut buf = String::new();
    collect_spans(spans, &mut buf);
    buf
}

fn collect_spans(spans: &[TextSpan], buf: &mut String) {
    for span in spans {
        collect_span(span, buf);
    }
}

fn collect_span(span: &TextSpan, buf: &mut String) {
    match span {
        TextSpan::Plain(s) => buf.push_str(s),
        TextSpan::Monospace(s) => buf.push_str(s),
        TextSpan::HardBreak => buf.push('\n'),
        TextSpan::Bold(inner)
        | TextSpan::Italic(inner)
        | TextSpan::Underline(inner)
        | TextSpan::Strikethrough(inner)
        | TextSpan::Subscript(inner)
        | TextSpan::Superscript(inner) => collect_spans(inner, buf),
        TextSpan::Colored { content, .. }
        | TextSpan::BackHighlight { content, .. }
        | TextSpan::Sized { content, .. }
        | TextSpan::FontFamily { content, .. } => collect_spans(content, buf),
        TextSpan::Link { url, label, .. } => {
            if let Some(lbl) = label {
                buf.push_str(lbl);
            } else if !url.is_empty() {
                buf.push_str(url);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_passthrough() {
        let spans = vec![TextSpan::Plain("hello".into())];
        assert_eq!(plain_text(&spans), "hello");
    }

    #[test]
    fn nested_bold_italic() {
        let spans = vec![
            TextSpan::Bold(vec![TextSpan::Plain("a".into())]),
            TextSpan::Italic(vec![TextSpan::Plain("b".into())]),
        ];
        assert_eq!(plain_text(&spans), "ab");
    }

    #[test]
    fn hard_break_becomes_newline() {
        let spans = vec![
            TextSpan::Plain("a".into()),
            TextSpan::HardBreak,
            TextSpan::Plain("b".into()),
        ];
        assert_eq!(plain_text(&spans), "a\nb");
    }

    #[test]
    fn link_prefers_label_over_url() {
        let spans = vec![TextSpan::Link {
            url: "https://example.com".into(),
            tooltip: None,
            label: Some("example".into()),
        }];
        assert_eq!(plain_text(&spans), "example");
    }

    #[test]
    fn link_falls_back_to_url() {
        let spans = vec![TextSpan::Link {
            url: "https://example.com".into(),
            tooltip: None,
            label: None,
        }];
        assert_eq!(plain_text(&spans), "https://example.com");
    }
}
