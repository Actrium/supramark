//! Strikethrough syntax (like `~~this~~`)
use crate::generics::inline::emph_pair;
use crate::{MarkdownIt, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct Strikethrough {
    pub marker: char,
}

impl NodeValue for Strikethrough {
    fn to_ast_v2(&self, node: &Node, ctx: &crate::supramark::AstV2Ctx<'_>) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::Delete {
            children: ctx.map_children(&node.children),
            position: ctx.position(node),
        }])
    }

    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("s", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("s");
    }
}

pub fn add(md: &mut MarkdownIt) {
    emph_pair::add_with::<'~', 2, true>(md, || Node::new(Strikethrough { marker: '~' }));
}
