//! Code spans
//!
//! `` `looks like this` ``
//!
//! <https://spec.commonmark.org/0.30/#code-span>
use crate::generics::inline::code_pair;
use crate::{MarkdownIt, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct CodeInline {
    pub marker: char,
    pub marker_len: usize,
}

impl NodeValue for CodeInline {
    fn to_ast_v2(&self, node: &Node, ctx: &crate::supramark::AstV2Ctx<'_>) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::InlineCode {
            value: node.collect_text(),
            position: ctx.position(node),
        }])
    }

    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("code", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("code");
    }
}

pub fn add(md: &mut MarkdownIt) {
    code_pair::add_with::<'`'>(md, |len| {
        Node::new(CodeInline {
            marker: '`',
            marker_len: len,
        })
    });
}
