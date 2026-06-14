//! Indented code block
//!
//! Parses anything indented with 4 spaces.
//!
//! <https://spec.commonmark.org/0.30/#indented-code-block>
use crate::parser::block::{BlockRule, BlockState};
use crate::{MarkdownParser, Node, NodeValue, Renderer};

const CODE_INDENT: i32 = 4;

#[derive(Debug)]
pub struct CodeBlock {
    pub content: String,
}

impl NodeValue for CodeBlock {
    fn to_ast_v2(&self, node: &Node, ctx: &crate::supramark::AstV2Ctx<'_>) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::Code {
            value: self.content.clone(),
            lang: None,
            meta: None,
            position: ctx.position(node),
        }])
    }

    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
        fmt.open("pre", &[]);
        fmt.open("code", &node.attrs);
        fmt.text(&self.content);
        fmt.close("code");
        fmt.close("pre");
        fmt.cr();
    }
}

pub fn add(md: &mut MarkdownParser) {
    md.block.add_rule::<CodeScanner>();
    md.max_indent = CODE_INDENT;
}

#[doc(hidden)]
pub struct CodeScanner;
impl BlockRule for CodeScanner {
    fn check(_: &mut BlockState) -> Option<()> {
        None
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        if state.line_indent(state.line) < CODE_INDENT {
            return None;
        }

        let mut next_line = state.line + 1;
        let mut last = next_line;

        while next_line < state.line_max {
            if state.is_empty(next_line) {
                next_line += 1;
                continue;
            }

            if state.line_indent(next_line) >= CODE_INDENT {
                next_line += 1;
                last = next_line;
                continue;
            }

            break;
        }

        let (mut content, _mapping) = state.get_lines(
            state.line,
            last,
            CODE_INDENT as usize + state.blk_indent,
            false,
        );
        content += "\n";

        let node = Node::new(CodeBlock { content });
        //node.srcmap = state.get_map_from_offsets(mapping[0].1, state.line_offsets[last - 1].line_end);

        Some((node, last - state.line))
    }
}
