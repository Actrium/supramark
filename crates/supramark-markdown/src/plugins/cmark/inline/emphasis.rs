//! Emphasis and strong emphasis
//!
//! looks like `*this*` or `__that__`
//!
//! <https://spec.commonmark.org/0.30/#emphasis-and-strong-emphasis>
use crate::generics::inline::emph_pair;
use crate::supramark::{AstV2Ctx, SupramarkNode};
use crate::{MarkdownParser, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct Em {
    pub marker: char,
}

impl NodeValue for Em {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("em", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("em");
    }

    fn to_ast_v2(&self, node: &Node, ctx: &AstV2Ctx<'_>) -> Option<Vec<SupramarkNode>> {
        Some(vec![SupramarkNode::Emphasis {
            children: ctx.map_children(&node.children),
            position: ctx.position(node),
        }])
    }
}

#[derive(Debug)]
pub struct Strong {
    pub marker: char,
}

impl NodeValue for Strong {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("strong", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("strong");
    }

    fn to_ast_v2(&self, node: &Node, ctx: &AstV2Ctx<'_>) -> Option<Vec<SupramarkNode>> {
        Some(vec![SupramarkNode::Strong {
            children: ctx.map_children(&node.children),
            position: ctx.position(node),
        }])
    }
}

pub fn add(md: &mut MarkdownParser) {
    emph_pair::add_with::<'*', 1, true>(md, || Node::new(Em { marker: '*' }));
    emph_pair::add_with::<'_', 1, false>(md, || Node::new(Em { marker: '_' }));
    emph_pair::add_with::<'*', 2, true>(md, || Node::new(Strong { marker: '*' }));
    emph_pair::add_with::<'_', 2, false>(md, || Node::new(Strong { marker: '_' }));
}
