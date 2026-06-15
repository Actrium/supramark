/// Hyperlink data extracted from `[[...]]` syntax in PlantUML.
///
/// Supports these forms:
/// - `[[url]]` — simple link
/// - `[[url label]]` — link with display label
/// - `[[url{tooltip}]]` — link with tooltip
/// - `[[url{tooltip} label]]` — link with tooltip and label
/// - `[[{tooltip} label]]` — tooltip-only annotation
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Hyperlink {
    pub url: String,
    pub tooltip: Option<String>,
    pub label: Option<String>,
}

/// Parse a `[[...]]` hyperlink at the start of `input`.
///
/// Returns `Some((Hyperlink, remaining))` on success, or `None` if
/// the input does not begin with `[[`.
pub fn parse_hyperlink(input: &str) -> Option<(Hyperlink, &str)> {
    let s = input.strip_prefix("[[")?;

    // Find the matching `]]`, respecting `{…}` for tooltips.
    let close_idx = find_closing_brackets(s)?;
    let inner = &s[..close_idx];
    let remaining = &s[close_idx + 2..];

    if inner.is_empty() {
        return None;
    }

    let (url, tooltip, label) = parse_inner(inner);
    if url.is_empty() && tooltip.is_none() && label.is_none() {
        return None;
    }

    Some((
        Hyperlink {
            url: url.to_string(),
            tooltip: tooltip.map(std::string::ToString::to_string),
            label: label.map(std::string::ToString::to_string),
        },
        remaining,
    ))
}

/// Extract all `[[...]]` hyperlinks from a text string.
///
/// Returns the text with link markers removed and a list of extracted links.
pub fn extract_hyperlinks(text: &str) -> (String, Vec<Hyperlink>) {
    let mut cleaned = String::with_capacity(text.len());
    let mut links = Vec::new();
    let mut rest = text;

    while !rest.is_empty() {
        if let Some(start) = rest.find("[[") {
            cleaned.push_str(&rest[..start]);
            if let Some((link, after)) = parse_hyperlink(&rest[start..]) {
                // Insert the label (or url) as visible text replacement
                if let Some(ref label) = link.label {
                    cleaned.push_str(label);
                } else if !link.url.is_empty() {
                    cleaned.push_str(&link.url);
                }
                links.push(link);
                rest = after;
            } else {
                // Not a valid link — keep the literal `[[`
                cleaned.push_str("[[");
                rest = &rest[start + 2..];
            }
        } else {
            cleaned.push_str(rest);
            break;
        }
    }

    (cleaned, links)
}

// ── Internal helpers ────────────────────────────────────────────────

/// Find the index of the closing `]]` inside the content after `[[`.
fn find_closing_brackets(s: &str) -> Option<usize> {
    let mut i = 0;
    let bytes = s.as_bytes();
    let len = bytes.len();

    while i + 1 < len {
        if bytes[i] == b']' && bytes[i + 1] == b']' {
            return Some(i);
        }
        // Java: [[...]] must not span lines — newline breaks the link
        if bytes[i] == b'\n' {
            return None;
        }
        // Skip over `{…}` blocks so that `}` inside tooltips doesn't confuse us.
        if bytes[i] == b'{' {
            i += 1;
            while i < len && bytes[i] != b'}' {
                i += 1;
            }
            // advance past the closing `}`
            if i < len {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    None
}

/// Parse the inner content of `[[…]]` into (url, tooltip, label).
///
/// Formats (matched in order, following Java `UrlBuilder`):
///   `"url"` + optional `{tooltip}` + optional `label`  (quoted URL)
///   `{tooltip}`                                         (tooltip only)
///   `{tooltip} label`                                   (tooltip + label)
///   `url {tooltip}`                                     (link + tooltip, no label)
///   `url` + optional `{tooltip}` + optional `label`     (unquoted URL)
///
/// Java's `UrlBuilder` regex (`[^\\[\\]]*`) preserves trailing whitespace
/// inside the captured label, so the surrounding atom layout reserves space
/// for it even though `DriverTextSvg` later trims it before SVG rendering.
/// We mirror that by leaving trailing whitespace on the label here; render
/// code is responsible for trimming the visible glyphs.
fn parse_inner(inner: &str) -> (&str, Option<&str>, Option<&str>) {
    // Only strip leading whitespace; trailing whitespace must be preserved
    // so that callers can reproduce Java's atom width allocation (the label
    // width is computed including trailing spaces, even though those spaces
    // are stripped from the rendered glyph string by `DriverTextSvg`).
    let trimmed = inner.trim_start();

    // 1. Quoted URL: "url" {tooltip}? label?
    if let Some(after_open_quote) = trimmed.strip_prefix('"') {
        if let Some(close_quote) = after_open_quote.find('"') {
            let url = &after_open_quote[..close_quote];
            let after_quote = after_open_quote[close_quote + 1..].trim_start();
            // Check for optional {tooltip}
            if let Some(after_brace_open) = after_quote.strip_prefix('{') {
                if let Some(brace_end) = after_brace_open.find('}') {
                    let tooltip = &after_brace_open[..brace_end];
                    let after_brace = after_brace_open[brace_end + 1..].trim_start();
                    let label = if after_brace.is_empty() {
                        None
                    } else {
                        Some(after_brace)
                    };
                    let tooltip = if tooltip.is_empty() {
                        None
                    } else {
                        Some(tooltip)
                    };
                    return (url, tooltip, label);
                }
            }
            // No tooltip — check for label
            let label = if after_quote.is_empty() {
                None
            } else {
                Some(after_quote)
            };
            return (url, None, label);
        }
    }

    // 2. Tooltip only: {tooltip}
    // 3. Tooltip + label: {tooltip} label
    if let Some(after_open) = trimmed.strip_prefix('{') {
        if let Some(brace_end) = after_open.find('}') {
            let tooltip = &after_open[..brace_end];
            let after_brace = after_open[brace_end + 1..].trim_start();
            let label = if after_brace.is_empty() {
                None
            } else {
                Some(after_brace)
            };
            let tooltip = if tooltip.is_empty() {
                None
            } else {
                Some(tooltip)
            };
            return ("", tooltip, label);
        }
    }

    // 4 & 5. Check for tooltip `{…}` after URL
    if let Some(brace_start) = trimmed.find('{') {
        let url = trimmed[..brace_start].trim();
        let after_url = &trimmed[brace_start..];

        if let Some(brace_end) = after_url.find('}') {
            let tooltip = &after_url[1..brace_end];
            let after_brace = after_url[brace_end + 1..].trim_start();
            let label = if after_brace.is_empty() {
                None
            } else {
                Some(after_brace)
            };
            let tooltip = if tooltip.is_empty() {
                None
            } else {
                Some(tooltip)
            };
            return (url, tooltip, label);
        }
    }

    // No tooltip — check for label (first whitespace after url)
    if let Some(space_idx) = trimmed.find(|c: char| c.is_whitespace()) {
        let url = &trimmed[..space_idx];
        let label = trimmed[space_idx..].trim_start();
        let label = if label.is_empty() { None } else { Some(label) };
        (url, None, label)
    } else {
        (trimmed, None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_url() {
        let input = "[[https://example.com]]rest";
        let (link, remaining) = parse_hyperlink(input).expect("should parse");
        assert_eq!(link.url, "https://example.com");
        assert_eq!(link.tooltip, None);
        assert_eq!(link.label, None);
        assert_eq!(remaining, "rest");
    }

    #[test]
    fn parse_url_with_label() {
        let input = "[[https://example.com Example Site]]";
        let (link, remaining) = parse_hyperlink(input).expect("should parse");
        assert_eq!(link.url, "https://example.com");
        assert_eq!(link.label, Some("Example Site".into()));
        assert_eq!(link.tooltip, None);
        assert_eq!(remaining, "");
    }

    #[test]
    fn parse_url_with_tooltip() {
        let input = "[[https://example.com{Visit our site}]]";
        let (link, _) = parse_hyperlink(input).expect("should parse");
        assert_eq!(link.url, "https://example.com");
        assert_eq!(link.tooltip, Some("Visit our site".into()));
        assert_eq!(link.label, None);
    }

    #[test]
    fn parse_url_with_tooltip_and_label() {
        let input = "[[https://example.com{Visit our site} Example]]trailing";
        let (link, remaining) = parse_hyperlink(input).expect("should parse");
        assert_eq!(link.url, "https://example.com");
        assert_eq!(link.tooltip, Some("Visit our site".into()));
        assert_eq!(link.label, Some("Example".into()));
        assert_eq!(remaining, "trailing");
    }

    #[test]
    fn parse_no_link_returns_none() {
        assert!(parse_hyperlink("plain text").is_none());
        assert!(parse_hyperlink("[[]]").is_none());
        assert!(parse_hyperlink("[single]").is_none());
    }

    #[test]
    fn parse_tooltip_only_with_label() {
        let input = "[[{hover text} Visible]]";
        let (link, remaining) = parse_hyperlink(input).expect("should parse");
        assert_eq!(link.url, "");
        assert_eq!(link.tooltip, Some("hover text".into()));
        assert_eq!(link.label, Some("Visible".into()));
        assert_eq!(remaining, "");
    }

    #[test]
    fn extract_text_with_one_link() {
        let text = "Click [[https://example.com here]] to visit";
        let (cleaned, links) = extract_hyperlinks(text);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://example.com");
        assert_eq!(links[0].label, Some("here".into()));
        assert_eq!(cleaned, "Click here to visit");
    }

    #[test]
    fn extract_text_with_multiple_links() {
        let text = "See [[https://a.com A]] and [[https://b.com B]]";
        let (cleaned, links) = extract_hyperlinks(text);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].url, "https://a.com");
        assert_eq!(links[1].url, "https://b.com");
        assert_eq!(cleaned, "See A and B");
    }

    #[test]
    fn extract_text_with_no_links() {
        let text = "plain text without links";
        let (cleaned, links) = extract_hyperlinks(text);
        assert!(links.is_empty());
        assert_eq!(cleaned, text);
    }

    #[test]
    fn parse_url_only_no_label_shows_url_in_cleaned() {
        let text = "Go to [[https://example.com]] now";
        let (cleaned, links) = extract_hyperlinks(text);
        assert_eq!(links.len(), 1);
        assert_eq!(cleaned, "Go to https://example.com now");
    }

    #[test]
    fn parse_label_with_multiple_words() {
        let input = "[[https://example.com Click Here Now]]";
        let (link, _) = parse_hyperlink(input).expect("should parse");
        assert_eq!(link.url, "https://example.com");
        assert_eq!(link.label, Some("Click Here Now".into()));
    }

    #[test]
    fn tooltip_with_special_chars() {
        let input = "[[https://x.com{A & B info}]]";
        let (link, _) = parse_hyperlink(input).expect("should parse");
        assert_eq!(link.url, "https://x.com");
        assert_eq!(link.tooltip, Some("A & B info".into()));
    }

    #[test]
    fn cleaned_text_uses_label_for_tooltip_only() {
        let text = "See [[{hover} label]]";
        let (cleaned, links) = extract_hyperlinks(text);
        assert_eq!(cleaned, "See label");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "");
        assert_eq!(links[0].tooltip.as_deref(), Some("hover"));
    }
}
