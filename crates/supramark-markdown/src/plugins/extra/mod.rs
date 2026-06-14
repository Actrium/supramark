//! Markdown extensions beyond the CommonMark core, grouped by origin:
//!
//! - GFM: `strikethrough`, `tables`, `linkify` (feature-gated).
//! - Common block extensions (widely adopted across Markdown tooling):
//!   `math` (`$$`), `footnote` definitions, `deflist` (definition lists).
//! - Supramark-specific syntax: the `:::` container and `%%%` input blocks,
//!   plus single-line raw HTML, in `ext`.

pub mod deflist;
pub mod ext;
pub mod footnote;
#[cfg(feature = "linkify")]
pub mod linkify;
pub mod math;
pub mod strikethrough;
pub mod tables;
