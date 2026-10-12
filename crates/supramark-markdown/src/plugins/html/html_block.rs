//! HTML block syntax from CommonMark
//!
//! <https://spec.commonmark.org/0.30/#html-blocks>
use once_cell::sync::Lazy;
use regex::Regex;

use super::utils::blocks::*;
use super::utils::regexps::*;
use crate::parser::block::{BlockRule, BlockState};
use crate::{MarkdownParser, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct HtmlBlock {
    pub content: String,
}

impl NodeValue for HtmlBlock {
    fn to_ast_v2(
        &self,
        node: &Node,
        ctx: &crate::supramark::AstV2Ctx<'_>,
    ) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::Raw {
            format: "html".to_owned(),
            value: self.content.clone(),
            block: true,
            position: ctx.position(node),
        }])
    }

    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
        fmt.text_raw(&self.content);
        fmt.cr();
    }
}

pub fn add(md: &mut MarkdownParser) {
    md.block.add_rule::<HtmlBlockScanner>();
}

struct HTMLSequence {
    open: Regex,
    close: Regex,
    can_terminate_paragraph: bool,
    /// Whether a blank line ends the block. CommonMark §4.6: types 1–5 end
    /// only at their specific close condition (running through blank lines);
    /// types 6–7 end at a blank line.
    ends_at_blank: bool,
}

impl HTMLSequence {
    pub fn new(open: Regex, close: Regex, can_terminate_paragraph: bool, ends_at_blank: bool) -> Self {
        Self {
            open,
            close,
            can_terminate_paragraph,
            ends_at_blank,
        }
    }
}

// An array of opening and corresponding closing sequences for html tags,
// last argument defines whether it can terminate a paragraph or not
//
static HTML_SEQUENCES: Lazy<[HTMLSequence; 7]> = Lazy::new(|| {
    let block_names = HTML_BLOCKS.join("|");
    let open_close_tag_re = HTML_OPEN_CLOSE_TAG_RE.as_str();

    [
        HTMLSequence::new(
            Regex::new(r#"(?i)^<(script|pre|style|textarea)(\s|>|$)"#).unwrap(),
            Regex::new(r#"(?i)</(script|pre|style|textarea)>"#).unwrap(),
            true,
            false,
        ),
        HTMLSequence::new(
            Regex::new(r#"^<!--"#).unwrap(),
            Regex::new(r#"-->"#).unwrap(),
            true,
            false,
        ),
        HTMLSequence::new(
            Regex::new(r#"^<\?"#).unwrap(),
            Regex::new(r#"\?>"#).unwrap(),
            true,
            false,
        ),
        HTMLSequence::new(
            Regex::new(r#"^<![A-Za-z]"#).unwrap(),
            Regex::new(r#">"#).unwrap(),
            true,
            false,
        ),
        HTMLSequence::new(
            Regex::new(r#"^<!\[CDATA\["#).unwrap(),
            Regex::new(r#"\]\]>"#).unwrap(),
            true,
            false,
        ),
        HTMLSequence::new(
            Regex::new(&format!("(?i)^</?({block_names})(\\s|/?>|$)")).unwrap(),
            Regex::new(r#"^$"#).unwrap(),
            true,
            true,
        ),
        HTMLSequence::new(
            Regex::new(&format!("{open_close_tag_re}\\s*$")).unwrap(),
            Regex::new(r#"^$"#).unwrap(),
            false,
            true,
        ),
    ]
});

#[doc(hidden)]
pub struct HtmlBlockScanner;

impl HtmlBlockScanner {
    fn get_sequence(state: &mut BlockState) -> Option<&'static HTMLSequence> {
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }

        let line_text = state.get_line(state.line);
        let Some('<') = line_text.chars().next() else {
            return None;
        };

        let mut sequence = None;
        for seq in HTML_SEQUENCES.iter() {
            if seq.open.is_match(line_text) {
                sequence = Some(seq);
                break;
            }
        }

        sequence
    }
}

impl BlockRule for HtmlBlockScanner {
    fn check(state: &mut BlockState) -> Option<()> {
        let sequence = Self::get_sequence(state)?;
        if !sequence.can_terminate_paragraph {
            return None;
        }
        // `get_sequence` only matches type 1–6 start conditions. Type 1 names
        // (script/pre/style/textarea) are absent from the type 6 name list, so
        // a closing tag like `</pre>` matches no sequence and never interrupts
        // a paragraph — it stays inline, matching cmark. A type 6 closing tag
        // (`</div>`, `</td>`, …) matches and interrupts normally.
        Some(())
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        let sequence = Self::get_sequence(state)?;

        let line_text = state.get_line(state.line);
        let start_line = state.line;
        let mut next_line = state.line + 1;

        // If we are here - we detected HTML block.
        // Let's roll down till block end.
        if !sequence.close.is_match(line_text) {
            while next_line < state.line_max {
                // HTML blocks never allow lazy continuation. A non-blank line
                // that has dropped out of the enclosing container (e.g. lost
                // the blockquote `>` marker) ends the block for every type.
                // `line_indent < 0` catches this (the line's content sits
                // before `blk_indent`); `is_empty` excludes genuine blank
                // lines, whose effect is governed by the next guard.
                if !state.is_empty(next_line) && state.line_indent(next_line) < 0 {
                    break;
                }
                // Types 6–7 end at a blank line; types 1–5 run through blank
                // lines until their specific close condition.
                if sequence.ends_at_blank && state.line_indent(next_line) < 0 {
                    break;
                }

                let line_text = state.get_line(next_line);

                if sequence.close.is_match(line_text) {
                    if !line_text.is_empty() {
                        next_line += 1;
                    }
                    break;
                }

                next_line += 1;
            }
        }

        // micromark includes a trailing line ending in an HTML block's raw
        // content only when one is actually present in the source — i.e. when
        // the block's last line is followed by more input, not when it sits at
        // document EOF. `line_end` points at the line's newline byte (or at
        // `src.len()` for the final, newline-less line), so `line_end < src.len()`
        // distinguishes "newline present" from "EOF".
        let last_line = next_line - 1;
        let keep_last_lf = state.line_offsets[last_line].line_end < state.src.len();
        let (content, _) = state.get_lines(start_line, next_line, state.blk_indent, keep_last_lf);
        let node = Node::new(HtmlBlock { content });
        Some((node, next_line - state.line))
    }
}
