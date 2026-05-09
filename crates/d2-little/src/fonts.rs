//! Font family definitions and embedded TTF data for d2 renderers.
//!
//! Ported from Go `d2renderers/d2fonts/d2fonts_common.go` and
//! `d2renderers/d2fonts/d2fonts_embed.go`.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

// ---------------------------------------------------------------------------
// Font size constants
// ---------------------------------------------------------------------------

pub const FONT_SIZE_XS: i32 = 13;
pub const FONT_SIZE_S: i32 = 14;
pub const FONT_SIZE_M: i32 = 16;
pub const FONT_SIZE_L: i32 = 20;
pub const FONT_SIZE_XL: i32 = 24;
pub const FONT_SIZE_XXL: i32 = 28;
pub const FONT_SIZE_XXXL: i32 = 32;

pub const FONT_SIZES: &[i32] = &[
    FONT_SIZE_XS,
    FONT_SIZE_S,
    FONT_SIZE_M,
    FONT_SIZE_L,
    FONT_SIZE_XL,
    FONT_SIZE_XXL,
    FONT_SIZE_XXXL,
];

// ---------------------------------------------------------------------------
// FontFamily
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontFamily {
    SourceSansPro,
    SourceCodePro,
    /// Sketch / hand-drawn font (FuzzyBubbles), enabled when sketch=true.
    HandDrawn,
}

impl FontFamily {
    pub fn font(self, size: i32, style: FontStyle) -> Font {
        Font {
            family: self,
            style,
            size,
        }
    }
}

impl fmt::Display for FontFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontFamily::SourceSansPro => write!(f, "SourceSansPro"),
            FontFamily::SourceCodePro => write!(f, "SourceCodePro"),
            FontFamily::HandDrawn => write!(f, "HandDrawn"),
        }
    }
}

pub const FONT_FAMILIES: &[FontFamily] = &[
    FontFamily::SourceSansPro,
    FontFamily::SourceCodePro,
    FontFamily::HandDrawn,
];

// ---------------------------------------------------------------------------
// FontStyle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Regular,
    Bold,
    Italic,
    Semibold,
}

impl fmt::Display for FontStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontStyle::Regular => write!(f, "regular"),
            FontStyle::Bold => write!(f, "bold"),
            FontStyle::Italic => write!(f, "italic"),
            FontStyle::Semibold => write!(f, "semibold"),
        }
    }
}

pub const FONT_STYLES: &[FontStyle] = &[
    FontStyle::Regular,
    FontStyle::Bold,
    FontStyle::Semibold,
    FontStyle::Italic,
];

// ---------------------------------------------------------------------------
// Font
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Font {
    pub family: FontFamily,
    pub style: FontStyle,
    pub size: i32,
}

impl Font {
    pub fn new(family: FontFamily, style: FontStyle, size: i32) -> Self {
        Self {
            family,
            style,
            size,
        }
    }

    /// Return the encoded (base64 WOFF data-URI) subset font for the given corpus.
    pub fn get_encoded_subset(&self, corpus: &str) -> String {
        // Deduplicate characters
        let mut seen = std::collections::HashSet::new();
        let unique: String = corpus.chars().filter(|c| seen.insert(*c)).collect();

        let face = lookup_font_face(self.family, self.style);
        let mut font_buf = face.to_vec();

        if let Some(subset) = crate::font::utf8_cut_font(&font_buf, &unique) {
            font_buf = subset;
        }

        match crate::font::sfnt2woff(&font_buf) {
            Ok(woff) => {
                format!(
                    "data:application/font-woff;base64,{}",
                    BASE64_STANDARD.encode(&woff)
                )
            }
            Err(_) => {
                // Fall back to full font encoding
                let woff = crate::font::sfnt2woff(face).unwrap_or_default();
                format!(
                    "data:application/font-woff;base64,{}",
                    BASE64_STANDARD.encode(&woff)
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Embedded TTF font data
// ---------------------------------------------------------------------------

// SourceSansPro
static SOURCE_SANS_PRO_REGULAR: &[u8] = include_bytes!("../ttf/SourceSansPro-Regular.ttf");
static SOURCE_SANS_PRO_BOLD: &[u8] = include_bytes!("../ttf/SourceSansPro-Bold.ttf");
static SOURCE_SANS_PRO_SEMIBOLD: &[u8] = include_bytes!("../ttf/SourceSansPro-Semibold.ttf");
static SOURCE_SANS_PRO_ITALIC: &[u8] = include_bytes!("../ttf/SourceSansPro-Italic.ttf");

// SourceCodePro
static SOURCE_CODE_PRO_REGULAR: &[u8] = include_bytes!("../ttf/SourceCodePro-Regular.ttf");
static SOURCE_CODE_PRO_BOLD: &[u8] = include_bytes!("../ttf/SourceCodePro-Bold.ttf");
static SOURCE_CODE_PRO_SEMIBOLD: &[u8] = include_bytes!("../ttf/SourceCodePro-Semibold.ttf");
static SOURCE_CODE_PRO_ITALIC: &[u8] = include_bytes!("../ttf/SourceCodePro-Italic.ttf");

// HandDrawn = FuzzyBubbles.  Go reuses regular for italic and bold for
// semibold because FuzzyBubbles ships only two cuts.  Mirror that.
static FUZZY_BUBBLES_REGULAR: &[u8] = include_bytes!("../ttf/FuzzyBubbles-Regular.ttf");
static FUZZY_BUBBLES_BOLD: &[u8] = include_bytes!("../ttf/FuzzyBubbles-Bold.ttf");

/// Look up the raw TTF bytes for a given font family + style.
pub fn lookup_font_face(family: FontFamily, style: FontStyle) -> &'static [u8] {
    match (family, style) {
        (FontFamily::SourceSansPro, FontStyle::Regular) => SOURCE_SANS_PRO_REGULAR,
        (FontFamily::SourceSansPro, FontStyle::Bold) => SOURCE_SANS_PRO_BOLD,
        (FontFamily::SourceSansPro, FontStyle::Semibold) => SOURCE_SANS_PRO_SEMIBOLD,
        (FontFamily::SourceSansPro, FontStyle::Italic) => SOURCE_SANS_PRO_ITALIC,
        (FontFamily::SourceCodePro, FontStyle::Regular) => SOURCE_CODE_PRO_REGULAR,
        (FontFamily::SourceCodePro, FontStyle::Bold) => SOURCE_CODE_PRO_BOLD,
        (FontFamily::SourceCodePro, FontStyle::Semibold) => SOURCE_CODE_PRO_SEMIBOLD,
        (FontFamily::SourceCodePro, FontStyle::Italic) => SOURCE_CODE_PRO_ITALIC,
        (FontFamily::HandDrawn, FontStyle::Regular) => FUZZY_BUBBLES_REGULAR,
        (FontFamily::HandDrawn, FontStyle::Italic) => FUZZY_BUBBLES_REGULAR,
        (FontFamily::HandDrawn, FontStyle::Bold) => FUZZY_BUBBLES_BOLD,
        (FontFamily::HandDrawn, FontStyle::Semibold) => FUZZY_BUBBLES_BOLD,
    }
}

/// Return true if the font face exists for the given key (always true for
/// built-in families, but the API mirrors the Go `FontFaces.Lookup` pattern).
pub fn has_font_face(family: FontFamily, style: FontStyle) -> bool {
    // All built-in combinations exist
    let _ = (family, style);
    true
}

// ---------------------------------------------------------------------------
// D2_FONT_TO_FAMILY mapping
// ---------------------------------------------------------------------------

/// Map from d2 logical font name to FontFamily.
pub fn d2_font_to_family(name: &str) -> Option<FontFamily> {
    match name {
        "default" => Some(FontFamily::SourceSansPro),
        "mono" => Some(FontFamily::SourceCodePro),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_face_lookup() {
        let face = lookup_font_face(FontFamily::SourceSansPro, FontStyle::Regular);
        assert!(!face.is_empty());
        // TTF files start with version 0x00010000
        assert_eq!(&face[0..4], &[0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn test_font_sizes() {
        assert_eq!(FONT_SIZES.len(), 7);
        assert_eq!(FONT_SIZES[0], 13);
        assert_eq!(FONT_SIZES[6], 32);
    }

    #[test]
    fn test_d2_font_to_family() {
        assert_eq!(
            d2_font_to_family("default"),
            Some(FontFamily::SourceSansPro)
        );
        assert_eq!(d2_font_to_family("mono"), Some(FontFamily::SourceCodePro));
        assert_eq!(d2_font_to_family("unknown"), None);
    }

    #[test]
    fn test_sfnt2woff_with_real_font() {
        let ttf = lookup_font_face(FontFamily::SourceSansPro, FontStyle::Regular);
        let woff = crate::font::sfnt2woff(ttf).expect("sfnt2woff should succeed");
        // WOFF starts with magic 0x774F4646
        assert_eq!(&woff[0..4], &[0x77, 0x4F, 0x46, 0x46]);
        // WOFF should be smaller than TTF (compression)
        assert!(
            woff.len() < ttf.len(),
            "WOFF ({}) should be smaller than TTF ({})",
            woff.len(),
            ttf.len()
        );
    }

    #[test]
    fn test_utf8_cut_font() {
        let ttf = lookup_font_face(FontFamily::SourceSansPro, FontStyle::Regular);
        let subset = crate::font::utf8_cut_font(ttf, "Hello");
        assert!(subset.is_some(), "subsetting should succeed");
        let subset = subset.unwrap();
        // Subset should be smaller than the original
        assert!(
            subset.len() < ttf.len(),
            "subset ({}) should be smaller than original ({})",
            subset.len(),
            ttf.len()
        );
        // Subset should still be a valid TTF (starts with 0x00010000)
        assert_eq!(&subset[0..4], &[0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn test_get_encoded_subset() {
        let font = Font::new(FontFamily::SourceSansPro, FontStyle::Regular, FONT_SIZE_M);
        let encoded = font.get_encoded_subset("Test");
        assert!(encoded.starts_with("data:application/font-woff;base64,"));
        assert!(encoded.len() > 40); // should have actual base64 data
    }
}
