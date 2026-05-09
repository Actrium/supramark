//! TTF to WOFF conversion and font subsetting.
//!
//! Ported from Go:
//! - `lib/font/font.go` (sfnt2woff)
//! - `lib/font/subsetFont.go` (UTF8CutFont)

mod subset;
mod woff;

pub use subset::utf8_cut_font;
pub use woff::sfnt2woff;
