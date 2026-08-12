//! Images
//!
//! `![image](<src> "title")`
//!
//! <https://spec.commonmark.org/0.30/#images>
use once_cell::sync::Lazy;
use regex::Regex;

use crate::generics::inline::full_link;
use crate::{MarkdownParser, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct Image {
    pub url: String,
    pub title: Option<String>,
}

impl NodeValue for Image {
    fn to_ast_v2(
        &self,
        node: &Node,
        ctx: &crate::supramark::AstV2Ctx<'_>,
    ) -> Option<Vec<crate::supramark::SupramarkNode>> {
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

pub fn add(md: &mut MarkdownParser) {
    full_link::add_prefix::<'!', true>(md, |href, title| {
        let url = href.map(|h| sanitize_image_src(&h)).unwrap_or_default();
        Node::new(Image { url, title })
    });
}

/// micromark's `sanitizeUri` with `protocolSource = /^https?$/i`: an image `src`
/// is kept only when it has no scheme (relative URL) or its scheme is `http`/
/// `https`. A colon that lands after a `/`, `?`, or `#` is part of the path,
/// query, or hash — not a scheme — so the URL is treated as relative. Any other
/// scheme (`irc:`, `mailto:`, `data:`, `javascript:`, …) collapses to an empty
/// `src`, matching micromark's "safe by default" for `img[src]`.
fn sanitize_image_src(url: &str) -> String {
    static IMAGE_PROTO_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)^https?$").unwrap());

    let Some(colon) = url.find(':') else {
        return url.to_owned();
    };
    let after_slash = url.find('/').is_some_and(|s| colon > s);
    let after_question = url.find('?').is_some_and(|q| colon > q);
    let after_hash = url.find('#').is_some_and(|h| colon > h);
    if after_slash || after_question || after_hash || IMAGE_PROTO_RE.is_match(&url[..colon]) {
        url.to_owned()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_image_src;

    #[test]
    fn allows_http_https_and_relative() {
        assert_eq!(sanitize_image_src("http://a"), "http://a");
        assert_eq!(sanitize_image_src("https://a"), "https://a");
        assert_eq!(sanitize_image_src("#a"), "#a");
        assert_eq!(sanitize_image_src("?a"), "?a");
        assert_eq!(sanitize_image_src("/a"), "/a");
        assert_eq!(sanitize_image_src("./a"), "./a");
        assert_eq!(sanitize_image_src("../a"), "../a");
    }

    #[test]
    fn allows_colon_in_path_query_hash() {
        assert_eq!(sanitize_image_src("a#b:c"), "a#b:c");
        assert_eq!(sanitize_image_src("a?b:c"), "a?b:c");
        assert_eq!(sanitize_image_src("a/b:c"), "a/b:c");
    }

    #[test]
    fn empties_non_http_schemes() {
        assert_eq!(sanitize_image_src("irc:///help"), "");
        assert_eq!(sanitize_image_src("mailto:a"), "");
        assert_eq!(sanitize_image_src("javascript:alert(1)"), "");
        assert_eq!(sanitize_image_src("data:image/png;base64,x"), "");
    }
}
