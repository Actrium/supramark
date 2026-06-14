//! Images
//!
//! `![image](<src> "title")`
//!
//! <https://spec.commonmark.org/0.30/#images>
use crate::generics::inline::full_link;
use crate::{MarkdownIt, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct Image {
    pub url: String,
    pub title: Option<String>,
}

impl NodeValue for Image {
    fn to_ast_v2(&self, node: &Node, ctx: &crate::supramark::AstV2Ctx<'_>) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::Image {
            url: self.url.clone(),
            title: self.title.clone(),
            alt: node.collect_text(),
            position: ctx.position(node),
        }])
    }

    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        let mut attrs = node.attrs.clone();
        attrs.push(("src", self.url.clone()));
        attrs.push(("alt", node.collect_text()));

        if let Some(title) = &self.title {
            attrs.push(("title", title.clone()));
        }

        fmt.self_close("img", &attrs);
    }
}

pub fn add(md: &mut MarkdownIt) {
    full_link::add_prefix::<'!', true>(md, |href, title| {
        Node::new(Image {
            url: href.unwrap_or_default(),
            title,
        })
    });
}
