//! Raw html syntax (block and inline), part of CommonMark standard.
//!
//! This feature is separated from cmark because it is unsafe to enable by
//! default (due to lack of any kind of html sanitization).
//!
//! You can enable it if you're:
//!  - looking for strict CommonMark compatibility
//!  - only have trusted input (i.e. writing markdown yourself)
//!  - or took some care to sanitize html yourself
//!
//! ```rust
//! let md = &mut supramark_markdown::MarkdownParser::new();
//! supramark_markdown::plugins::cmark::add(md);
//! supramark_markdown::plugins::html::add(md);
//!
//! let html = md.parse("hello<br>world").render();
//! assert_eq!(html.trim(), r#"<p>hello<br>world</p>"#);
//! ```

pub mod html_block;
pub mod html_inline;
mod utils;

use crate::MarkdownParser;

pub fn add(md: &mut MarkdownParser) {
    html_inline::add(md);
    html_block::add(md);
}
