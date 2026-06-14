use crate::MarkdownParser;

pub(super) mod block_parser;

pub use block_parser::BlockParserRule;

pub fn add(md: &mut MarkdownParser) {
    block_parser::add(md);
}
