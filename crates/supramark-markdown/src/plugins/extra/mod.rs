//! Markdown extensions from GFM used by Supramark.
//!
//!  - strikethrough (`~~xxx~~`)
//!  - tables
//!  - linkify (convert `http://example.com` to a link; feature-gated)
//!  - code block highlighting using `syntect` (feature-gated)

#[cfg(feature = "linkify")]
pub mod linkify;
pub mod strikethrough;
#[cfg(feature = "syntect")]
pub mod syntect;
pub mod tables;

use crate::MarkdownParser;

/// Enable the GFM-style extras Supramark ships with.
pub fn add(md: &mut MarkdownParser) {
    strikethrough::add(md);
    tables::add(md);
    #[cfg(feature = "linkify")]
    linkify::add(md);
    #[cfg(feature = "syntect")]
    syntect::add(md);
}
