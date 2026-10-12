//! Line breaks
//!
//! Processes EOL (`\n`, soft and hard breaks).
//!
//!  - <https://spec.commonmark.org/0.30/#hard-line-breaks>
//!  - <https://spec.commonmark.org/0.30/#soft-line-breaks>
use crate::parser::inline::{InlineRule, InlineState};
use crate::{MarkdownParser, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct Hardbreak;

impl NodeValue for Hardbreak {
    fn to_ast_v2(
        &self,
        node: &Node,
        ctx: &crate::supramark::AstV2Ctx<'_>,
    ) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::Break {
            position: ctx.position(node),
        }])
    }

    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        fmt.self_close("br", &[]);
        fmt.cr();
    }
}

#[derive(Debug)]
pub struct Softbreak;

impl NodeValue for Softbreak {
    fn to_ast_v2(
        &self,
        node: &Node,
        ctx: &crate::supramark::AstV2Ctx<'_>,
    ) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::Text {
            value: "\n".to_owned(),
            position: ctx.position(node),
        }])
    }

    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
    }
}

pub fn add(md: &mut MarkdownParser) {
    md.inline.add_rule::<NewlineScanner>();
}

#[doc(hidden)]
pub struct NewlineScanner;
impl InlineRule for NewlineScanner {
    const MARKER: char = '\n';

    fn check(state: &mut InlineState) -> Option<usize> {
        // check rule is required because run() modifies trailing text
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next().unwrap() != '\n' {
            return None;
        }
        Some(1)
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();

        if chars.next().unwrap() != '\n' {
            return None;
        }

        let mut pos = state.pos;
        pos += 1;

        // skip leading whitespaces from next line
        while let Some(' ' | '\t') = chars.next() {
            pos += 1;
        }

        // Trailing line suffix = the maximal run of spaces/tabs at the end of
        // the preceding text. A hard break requires 2+ trailing spaces, and a
        // tab anywhere in that suffix makes it a soft break (matching
        // micromark's mixed-line-suffix handling). The whole suffix is popped
        // so neither the spaces nor a mixed-in tab leak into the output text.
        let trailing_text = state.trailing_text_get();
        let mut suffix_len = 0;
        let mut suffix_spaces = 0;
        let mut suffix_has_tab = false;
        for ch in trailing_text.chars().rev() {
            match ch {
                ' ' => {
                    suffix_len += 1;
                    suffix_spaces += 1;
                }
                '\t' => {
                    suffix_len += 1;
                    suffix_has_tab = true;
                }
                _ => break,
            }
        }

        state.trailing_text_pop(suffix_len);

        let node = if suffix_spaces >= 2 && !suffix_has_tab {
            Node::new(Hardbreak)
        } else {
            Node::new(Softbreak)
        };

        state.pos -= suffix_len; // backtrack to include the suffix in source maps
        Some((node, pos - state.pos))
    }
}
