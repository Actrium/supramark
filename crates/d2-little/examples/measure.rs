//! Quick comparison of our text measurement against Go's.
//!
//! Run: cargo run -p d2-textmeasure --example measure
use d2_little::fonts::{Font, FontFamily, FontStyle};
use d2_little::textmeasure::Ruler;

fn main() {
    let mut r = Ruler::new().unwrap();
    for label in &["hello", "not bold mono", "bold mono"] {
        for style in [FontStyle::Regular, FontStyle::Bold, FontStyle::Italic] {
            let f = Font::new(FontFamily::SourceCodePro, style, 16);
            let (w, h) = r.measure(f, label);
            println!("mono {:?} {:?}: w={} h={}", style, label, w, h);
        }
    }
}
