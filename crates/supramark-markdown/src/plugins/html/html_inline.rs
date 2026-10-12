//! HTML inline syntax from CommonMark
//!
//! <https://spec.commonmark.org/0.30/#raw-html>
use super::utils::regexps::*;
use crate::parser::inline::{InlineRule, InlineState};
use crate::{MarkdownParser, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct HtmlInline {
    pub content: String,
}

impl NodeValue for HtmlInline {
    fn to_ast_v2(
        &self,
        node: &Node,
        ctx: &crate::supramark::AstV2Ctx<'_>,
    ) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::Raw {
            format: "html".to_owned(),
            value: self.content.clone(),
            block: false,
            position: ctx.position(node),
        }])
    }

    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        fmt.text_raw(&self.content);
    }
}

pub fn add(md: &mut MarkdownParser) {
    md.inline.add_rule::<HtmlInlineScanner>();
}

#[doc(hidden)]
pub struct HtmlInlineScanner;
impl InlineRule for HtmlInlineScanner {
    const MARKER: char = '<';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        // Check start
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next().unwrap() != '<' {
            return None;
        }

        // Quick fail on second char
        let Some('!' | '?' | '/' | 'A'..='Z' | 'a'..='z') = chars.next() else {
            return None;
        };

        let capture = HTML_TAG_RE
            .captures(&state.src[state.pos..state.pos_max])?
            .get(0)
            .unwrap()
            .as_str();
        let capture_len = capture.len();

        let content = normalize_raw_inline(capture);

        if HTML_LINK_OPEN.is_match(&content) {
            state.link_level += 1;
        } else if HTML_LINK_CLOSE.is_match(&content) {
            state.link_level -= 1;
        }

        let node = Node::new(HtmlInline { content });
        Some((node, capture_len))
    }
}

/// micromark's raw-text HTML inline constructs (comment, processing
/// instruction, declaration, CDATA) consume a line ending plus its
/// following leading whitespace via the `spnl` pattern, but only keep the
/// line ending in the emitted value. Plain open/close tags are unaffected —
/// their attribute whitespace is normalized by the HTML parser during the
/// semantic comparison. Without this normalization a construct like
/// `<?\n    ?>` would round-trip as `<?\n    ?>` instead of micromark's
/// `<?\n?>`.
fn normalize_raw_inline(value: &str) -> String {
    let multi_line = value.starts_with("<!--")
        || value.starts_with("<?")
        || value.starts_with("<![CDATA[")
        || value.starts_with("<!");
    if !multi_line {
        return value.to_owned();
    }
    let mut out = String::with_capacity(value.len());
    let mut lines = value.split('\n');
    if let Some(first) = lines.next() {
        out.push_str(first);
    }
    for line in lines {
        out.push('\n');
        out.push_str(line.trim_start_matches(|c: char| matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{0c}')));
    }
    out
}
