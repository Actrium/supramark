//! Supramark footnote definitions: `[^label]: content` on its own line.
//!
//! Migrated from the document-level prescan to a block rule. Definitions stay
//! single-line (matching the previous behaviour); the content is parsed as a
//! paragraph so inline markup and footnote references resolve normally. The
//! rule runs before the link-reference rule so `[^a]:` is not mistaken for a
//! link reference definition.
use crate::parser::block::{BlockRule, BlockState};
use crate::parser::inline::InlineRoot;
use crate::plugins::cmark::block::paragraph::Paragraph;
use crate::plugins::cmark::block::reference::ReferenceScanner;
use crate::{MarkdownParser, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct FootnoteDef {
    pub label: String,
}

impl NodeValue for FootnoteDef {
    fn to_ast_v2(
        &self,
        node: &Node,
        ctx: &crate::supramark::AstV2Ctx<'_>,
    ) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::FootnoteDefinition {
            index: 0,
            identifier: crate::supramark::normalize_footnote_identifier(&self.label),
            label: self.label.clone(),
            children: ctx.map_children(&node.children),
            position: ctx.position(node),
        }])
    }

    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.contents(&node.children);
    }
}

pub fn add(md: &mut MarkdownParser) {
    md.block
        .add_rule::<FootnoteDefScanner>()
        .before::<ReferenceScanner>();
}

#[doc(hidden)]
pub struct FootnoteDefScanner;

impl FootnoteDefScanner {
    /// Parse `[^label]:` at the start of a (indent-trimmed) line, returning the
    /// label and the byte column where the definition content begins.
    fn parse_header(line: &str) -> Option<(String, usize)> {
        let label_rest = line.strip_prefix("[^")?;
        let close = label_rest.find("]:")?;
        let label = &label_rest[..close];
        if label.is_empty() {
            return None;
        }
        let mut content_col = 2 + close + 2;
        let content_rest = line.get(content_col..)?;
        content_col += content_rest.len() - content_rest.trim_start().len();
        Some((label.to_owned(), content_col))
    }
}

impl BlockRule for FootnoteDefScanner {
    fn check(state: &mut BlockState) -> Option<()> {
        if state.line_indent(state.line) > 3 {
            return None;
        }
        Self::parse_header(state.get_line(state.line)).map(|_| ())
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        if state.line_indent(state.line) > 3 {
            return None;
        }
        let line = state.get_line(state.line);
        let (label, content_col) = Self::parse_header(line)?;
        let content = line[content_col..].to_owned();
        let content_offset = state.line_offsets[state.line].first_nonspace + content_col;

        let mut def = Node::new(FootnoteDef { label });
        if !content.is_empty() {
            let mut para = Node::new(Paragraph);
            para.children.push(Node::new(InlineRoot::new(
                content,
                vec![(0, content_offset)],
            )));
            def.children.push(para);
        }
        Some((def, 1))
    }
}
