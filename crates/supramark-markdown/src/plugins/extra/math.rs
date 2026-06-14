//! Supramark block math.
//!
//! A `$$` fence alone on a line opens the block; content runs until the next
//! `$$` line. Migrated from the document-level prescan to a first-class block
//! rule so it nests and composes like any other block construct.
use crate::parser::block::{BlockRule, BlockState};
use crate::{MarkdownParser, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct MathBlock {
    pub value: String,
}

impl NodeValue for MathBlock {
    fn to_ast_v2(
        &self,
        node: &Node,
        ctx: &crate::supramark::AstV2Ctx<'_>,
    ) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::MathBlock {
            value: self.value.clone(),
            position: ctx.position(node),
        }])
    }

    fn render(&self, _node: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
        fmt.open("div", &[]);
        fmt.text(&self.value);
        fmt.close("div");
        fmt.cr();
    }
}

pub fn add(md: &mut MarkdownParser) {
    md.block.add_rule::<MathBlockScanner>();
}

#[doc(hidden)]
pub struct MathBlockScanner;

impl MathBlockScanner {
    fn is_fence(state: &BlockState, line: usize) -> bool {
        state.get_line(line).trim() == "$$"
    }
}

impl BlockRule for MathBlockScanner {
    fn check(state: &mut BlockState) -> Option<()> {
        Self::is_fence(state, state.line).then_some(())
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        if !Self::is_fence(state, state.line) {
            return None;
        }

        let mut close = None;
        let mut next_line = state.line + 1;
        while next_line < state.line_max {
            if Self::is_fence(state, next_line) {
                close = Some(next_line);
                break;
            }
            next_line += 1;
        }
        let close = close?;

        let (value, _) = state.get_lines(state.line + 1, close, 0, false);
        let node = Node::new(MathBlock { value });
        Some((node, close - state.line + 1))
    }
}
