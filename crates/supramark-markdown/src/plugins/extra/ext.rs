//! Supramark extension blocks, as block rules.
//!
//! `:::name` containers and `%%%name` inputs capture their content verbatim
//! (opaque) and dispatch by name; an unclosed opener yields an Unsupported node
//! carrying a diagnostic. Migrated from the document-level prescan so the whole
//! document parses in one pass.
use crate::parser::block::{BlockRule, BlockState};
use crate::supramark::{build_extension_node, parse_extension_open, ExtensionOpen};
use crate::{MarkdownParser, Node, NodeValue, Renderer};

pub fn add(md: &mut MarkdownParser) {
    md.block.add_rule::<ExtensionScanner>();
}

// ---- :::container / %%%input ----

#[derive(Debug)]
pub struct ExtBlock {
    open: ExtensionOpen,
    value: String,
    closed: bool,
}

impl NodeValue for ExtBlock {
    fn to_ast_v2(
        &self,
        node: &Node,
        ctx: &crate::supramark::AstV2Ctx<'_>,
    ) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![build_extension_node(
            &self.open,
            self.value.clone(),
            ctx.position(node),
            self.closed,
        )])
    }

    fn render(&self, _node: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
    }
}

#[doc(hidden)]
pub struct ExtensionScanner;

impl BlockRule for ExtensionScanner {
    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        let open = parse_extension_open(state.get_line(state.line))?;

        let mut next = state.line + 1;
        let mut close = None;
        while next < state.line_max {
            if state.get_line(next).trim() == open.close_marker {
                close = Some(next);
                break;
            }
            next += 1;
        }

        match close {
            Some(close_line) => {
                let (value, _) = state.get_lines(state.line + 1, close_line, state.blk_indent, false);
                let node = Node::new(ExtBlock {
                    open,
                    value,
                    closed: true,
                });
                Some((node, close_line - state.line + 1))
            }
            None => {
                let start = state.line_offsets[state.line].line_start;
                let value = state.src[start..].to_owned();
                let node = Node::new(ExtBlock {
                    open,
                    value,
                    closed: false,
                });
                Some((node, state.line_max - state.line))
            }
        }
    }
}
