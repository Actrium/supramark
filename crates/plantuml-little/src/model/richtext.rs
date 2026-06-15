/// Rich text model for Creole markup used in PlantUML labels, notes, and descriptions.
/// A span of inline text with optional formatting.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum TextSpan {
    /// Plain unformatted text.
    Plain(String),
    /// Bold text: `**bold**` or `<b>bold</b>`.
    Bold(Vec<TextSpan>),
    /// Italic text: `//italic//` or `<i>italic</i>`.
    Italic(Vec<TextSpan>),
    /// Underlined text: `__underline__` or `<u>underline</u>`.
    Underline(Vec<TextSpan>),
    /// Underlined text with a specific underline color: `<u:blue>text</u>`.
    UnderlineColored {
        color: String,
        content: Vec<TextSpan>,
    },
    /// Strikethrough text: `~~strike~~` or `<s>strike</s>`.
    Strikethrough(Vec<TextSpan>),
    /// Monospaced text: `""mono""`.
    Monospace(String),
    /// Colored text: `<color:red>text</color>`.
    Colored {
        color: String,
        content: Vec<TextSpan>,
    },
    /// Sized text: `<size:18>text</size>`.
    Sized { size: f64, content: Vec<TextSpan> },
    /// Subscript text: `<sub>text</sub>`.
    Subscript(Vec<TextSpan>),
    /// Superscript text: `<sup>text</sup>`.
    Superscript(Vec<TextSpan>),
    /// Background-highlighted text: `<back:color>text</back>`.
    BackHighlight {
        color: String,
        content: Vec<TextSpan>,
    },
    /// Font family change: `<font:name>text</font>`.
    FontFamily {
        family: String,
        content: Vec<TextSpan>,
    },
    /// Hyperlink: `[[url]]`, `[[url label]]`, or `[[url{tooltip} label]]`.
    Link {
        url: String,
        tooltip: Option<String>,
        label: Option<String>,
    },
    /// Inline SVG sprite reference: `<$name>`, optionally with scale/color params.
    InlineSvg {
        name: String,
        scale: Option<f64>,
        color: Option<String>,
    },
    /// OpenIconic icon: `<&name>`, optionally with scale/color.
    /// Renders as inline SVG `<path>` elements.
    OpenIcon {
        name: String,
        scale: f64,
        color: Option<String>,
    },
    /// Inline image: `<img:url>`, optionally with scale.
    /// Renders as `<image>` element with the image data.
    Image { url: String, scale: f64 },
}

/// A block-level rich text element.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum RichText {
    /// A single line consisting of inline spans.
    Line(Vec<TextSpan>),
    /// Multiple lines or blocks.
    Block(Vec<RichText>),
    /// Bullet list (`* item`).
    BulletList(Vec<RichText>),
    /// Numbered list (`# item`).
    NumberedList(Vec<RichText>),
    /// Table with optional header row and data rows.
    ///
    /// Every cell is a `Vec<Vec<TextSpan>>` — an ordered list of lines
    /// where each line is a span sequence.  Java `StripeTable` splits
    /// cell content on `U+E100` (the Jaws newline placeholder), so a
    /// single-line cell is `vec![vec![spans]]` while a multi-line cell
    /// stacks its lines vertically.
    Table {
        headers: Vec<Vec<Vec<TextSpan>>>,
        rows: Vec<Vec<Vec<Vec<TextSpan>>>>,
    },
    /// Horizontal rule (`----`).
    HorizontalRule,
    /// Creole section title: a horizontal line with the title text centered
    /// between a left half-line and a right half-line, drawn as two thin
    /// strokes (at y and y+2) matching Java's `UHorizontalLine` with style
    /// `=`.  Source pattern: `^==([^=]*)==$` (no `=` allowed in the title).
    SectionTitle(Vec<TextSpan>),
}

/// Extract plain text content from a `RichText` tree, stripping all formatting.
pub fn plain_text(rich: &RichText) -> String {
    let mut buf = String::new();
    collect_rich_text(rich, &mut buf);
    buf
}

fn collect_rich_text(rich: &RichText, buf: &mut String) {
    match rich {
        RichText::Line(spans) => {
            collect_spans(spans, buf);
        }
        RichText::Block(items) => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    buf.push('\n');
                }
                collect_rich_text(item, buf);
            }
        }
        RichText::BulletList(items) | RichText::NumberedList(items) => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    buf.push('\n');
                }
                collect_rich_text(item, buf);
            }
        }
        RichText::Table { headers, rows } => {
            // `headers` is a single header row of multi-line cells.
            if !headers.is_empty() {
                for (j, cell) in headers.iter().enumerate() {
                    if j > 0 {
                        buf.push('\t');
                    }
                    for (li, line) in cell.iter().enumerate() {
                        if li > 0 {
                            buf.push(' ');
                        }
                        collect_spans(line, buf);
                    }
                }
            }
            for row in rows {
                if !headers.is_empty() || !buf.is_empty() {
                    buf.push('\n');
                }
                for (j, cell) in row.iter().enumerate() {
                    if j > 0 {
                        buf.push('\t');
                    }
                    for (li, line) in cell.iter().enumerate() {
                        if li > 0 {
                            buf.push(' ');
                        }
                        collect_spans(line, buf);
                    }
                }
            }
        }
        RichText::HorizontalRule => {
            buf.push_str("----");
        }
        RichText::SectionTitle(spans) => {
            collect_spans(spans, buf);
        }
    }
}

fn collect_spans(spans: &[TextSpan], buf: &mut String) {
    for span in spans {
        collect_span(span, buf);
    }
}

fn collect_span(span: &TextSpan, buf: &mut String) {
    match span {
        TextSpan::Plain(s) => buf.push_str(s),
        TextSpan::Bold(inner)
        | TextSpan::Italic(inner)
        | TextSpan::Underline(inner)
        | TextSpan::Strikethrough(inner)
        | TextSpan::Subscript(inner)
        | TextSpan::Superscript(inner) => collect_spans(inner, buf),
        TextSpan::Monospace(s) => buf.push_str(s),
        TextSpan::UnderlineColored { content, .. }
        | TextSpan::Colored { content, .. }
        | TextSpan::Sized { content, .. }
        | TextSpan::BackHighlight { content, .. }
        | TextSpan::FontFamily { content, .. } => {
            collect_spans(content, buf);
        }
        TextSpan::Link { url, label, .. } => {
            if let Some(lbl) = label {
                buf.push_str(lbl);
            } else if !url.is_empty() {
                buf.push_str(url);
            }
        }
        TextSpan::InlineSvg { .. } => {}
        TextSpan::OpenIcon { .. } => {}
        TextSpan::Image { .. } => {}
    }
}
