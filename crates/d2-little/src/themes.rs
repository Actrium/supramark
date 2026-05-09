//! d2-themes: theme definitions and SVG element builder for d2 diagrams.
//!
//! Ported from Go `d2themes/d2themes.go`, `d2themes/element.go`,
//! and `d2themes/d2themescatalog/*.go`.

// ---------------------------------------------------------------------------
// Neutral presets
// ---------------------------------------------------------------------------

/// Neutral colors from darkest (N1) to lightest (N7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neutral {
    pub n1: &'static str,
    pub n2: &'static str,
    pub n3: &'static str,
    pub n4: &'static str,
    pub n5: &'static str,
    pub n6: &'static str,
    pub n7: &'static str,
}

pub const COOL_NEUTRAL: Neutral = Neutral {
    n1: "#0A0F25",
    n2: "#676C7E",
    n3: "#9499AB",
    n4: "#CFD2DD",
    n5: "#DEE1EB",
    n6: "#EEF1F8",
    n7: "#FFFFFF",
};

pub const WARM_NEUTRAL: Neutral = Neutral {
    n1: "#170206",
    n2: "#535152",
    n3: "#787777",
    n4: "#CCCACA",
    n5: "#DFDCDC",
    n6: "#ECEBEB",
    n7: "#FFFFFF",
};

pub const DARK_NEUTRAL: Neutral = Neutral {
    n1: "#F4F6FA",
    n2: "#BBBEC9",
    n3: "#868A96",
    n4: "#676D7D",
    n5: "#3A3D49",
    n6: "#191C28",
    n7: "#000410",
};

pub const DARK_MAUVE_NEUTRAL: Neutral = Neutral {
    n1: "#CDD6F4",
    n2: "#BAC2DE",
    n3: "#A6ADC8",
    n4: "#585B70",
    n5: "#45475A",
    n6: "#313244",
    n7: "#1E1E2E",
};

// ---------------------------------------------------------------------------
// ColorPalette
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorPalette {
    pub neutrals: Neutral,

    // Base colors: used for containers
    pub b1: &'static str,
    pub b2: &'static str,
    pub b3: &'static str,
    pub b4: &'static str,
    pub b5: &'static str,
    pub b6: &'static str,

    // Alternative colors A
    pub aa2: &'static str,
    pub aa4: &'static str,
    pub aa5: &'static str,

    // Alternative colors B
    pub ab4: &'static str,
    pub ab5: &'static str,
}

// ---------------------------------------------------------------------------
// SpecialRules
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpecialRules {
    pub mono: bool,
    pub no_corner_radius: bool,
    pub outer_container_double_border: bool,
    pub container_dots: bool,
    pub caps_lock: bool,
    pub c4: bool,
    pub all_paper: bool,
}

// ---------------------------------------------------------------------------
// ThemeOverrides
// ---------------------------------------------------------------------------

/// User-specified color overrides for a theme.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeOverrides {
    pub n1: Option<String>,
    pub n2: Option<String>,
    pub n3: Option<String>,
    pub n4: Option<String>,
    pub n5: Option<String>,
    pub n6: Option<String>,
    pub n7: Option<String>,
    pub b1: Option<String>,
    pub b2: Option<String>,
    pub b3: Option<String>,
    pub b4: Option<String>,
    pub b5: Option<String>,
    pub b6: Option<String>,
    pub aa2: Option<String>,
    pub aa4: Option<String>,
    pub aa5: Option<String>,
    pub ab4: Option<String>,
    pub ab5: Option<String>,
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub id: i64,
    pub name: &'static str,
    pub colors: ColorPalette,
    pub special_rules: SpecialRules,
}

impl Theme {
    pub fn is_dark(&self) -> bool {
        self.id >= 200 && self.id < 300
    }

    /// Apply user overrides to the theme colors.
    ///
    /// Because the catalog themes use `&'static str`, we need owned `String`s
    /// when overriding.  This method returns a new [`OwnedColorPalette`] with
    /// overrides applied, leaving `self` unchanged.
    pub fn apply_overrides(&self, overrides: &ThemeOverrides) -> OwnedColorPalette {
        let mut p = OwnedColorPalette::from(&self.colors);

        macro_rules! apply {
            ($field:ident) => {
                if let Some(ref v) = overrides.$field {
                    p.$field = v.clone();
                }
            };
        }

        apply!(n1);
        apply!(n2);
        apply!(n3);
        apply!(n4);
        apply!(n5);
        apply!(n6);
        apply!(n7);
        apply!(b1);
        apply!(b2);
        apply!(b3);
        apply!(b4);
        apply!(b5);
        apply!(b6);
        apply!(aa2);
        apply!(aa4);
        apply!(aa5);
        apply!(ab4);
        apply!(ab5);

        p
    }
}

// ---------------------------------------------------------------------------
// ResolvedTheme
// ---------------------------------------------------------------------------

/// A theme value suitable for inline SVG color resolution.
///
/// Catalog themes can be used directly, while themes with runtime overrides
/// need an owned palette so inline `fill`/`stroke` attributes match the CSS
/// rules generated from those same overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTheme {
    Catalog(Theme),
    Overridden {
        is_dark: bool,
        colors: OwnedColorPalette,
    },
}

impl ResolvedTheme {
    pub fn from_theme(theme: &Theme, overrides: Option<&ThemeOverrides>) -> Self {
        match overrides {
            Some(ov) => Self::Overridden {
                is_dark: theme.is_dark(),
                colors: theme.apply_overrides(ov),
            },
            None => Self::Catalog(theme.clone()),
        }
    }

    pub fn is_dark(&self) -> bool {
        match self {
            Self::Catalog(theme) => theme.is_dark(),
            Self::Overridden { is_dark, .. } => *is_dark,
        }
    }

    pub fn resolve_color<'a>(&'a self, code: &'a str) -> &'a str {
        match self {
            Self::Catalog(theme) => resolve_theme_color(theme, code),
            Self::Overridden { colors, .. } => resolve_owned_color(colors, code),
        }
    }
}

// ---------------------------------------------------------------------------
// OwnedColorPalette  (for runtime overrides)
// ---------------------------------------------------------------------------

/// A color palette that owns its strings, produced by [`Theme::apply_overrides`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedColorPalette {
    pub n1: String,
    pub n2: String,
    pub n3: String,
    pub n4: String,
    pub n5: String,
    pub n6: String,
    pub n7: String,
    pub b1: String,
    pub b2: String,
    pub b3: String,
    pub b4: String,
    pub b5: String,
    pub b6: String,
    pub aa2: String,
    pub aa4: String,
    pub aa5: String,
    pub ab4: String,
    pub ab5: String,
}

impl From<&ColorPalette> for OwnedColorPalette {
    fn from(cp: &ColorPalette) -> Self {
        Self {
            n1: cp.neutrals.n1.to_owned(),
            n2: cp.neutrals.n2.to_owned(),
            n3: cp.neutrals.n3.to_owned(),
            n4: cp.neutrals.n4.to_owned(),
            n5: cp.neutrals.n5.to_owned(),
            n6: cp.neutrals.n6.to_owned(),
            n7: cp.neutrals.n7.to_owned(),
            b1: cp.b1.to_owned(),
            b2: cp.b2.to_owned(),
            b3: cp.b3.to_owned(),
            b4: cp.b4.to_owned(),
            b5: cp.b5.to_owned(),
            b6: cp.b6.to_owned(),
            aa2: cp.aa2.to_owned(),
            aa4: cp.aa4.to_owned(),
            aa5: cp.aa5.to_owned(),
            ab4: cp.ab4.to_owned(),
            ab5: cp.ab5.to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Theme color resolution
// ---------------------------------------------------------------------------

/// Resolve a theme color code (e.g. "N1") to its hex value.
///
/// If `code` is not a theme color, it is returned unchanged.
pub fn resolve_theme_color<'a>(theme: &'a Theme, code: &'a str) -> &'a str {
    if !crate::color::is_theme_color(code) {
        return code;
    }
    match code {
        "N1" => theme.colors.neutrals.n1,
        "N2" => theme.colors.neutrals.n2,
        "N3" => theme.colors.neutrals.n3,
        "N4" => theme.colors.neutrals.n4,
        "N5" => theme.colors.neutrals.n5,
        "N6" => theme.colors.neutrals.n6,
        "N7" => theme.colors.neutrals.n7,
        "B1" => theme.colors.b1,
        "B2" => theme.colors.b2,
        "B3" => theme.colors.b3,
        "B4" => theme.colors.b4,
        "B5" => theme.colors.b5,
        "B6" => theme.colors.b6,
        "AA2" => theme.colors.aa2,
        "AA4" => theme.colors.aa4,
        "AA5" => theme.colors.aa5,
        "AB4" => theme.colors.ab4,
        "AB5" => theme.colors.ab5,
        _ => "",
    }
}

/// Resolve a theme color code against an [`OwnedColorPalette`].
pub fn resolve_owned_color<'a>(palette: &'a OwnedColorPalette, code: &'a str) -> &'a str {
    if !crate::color::is_theme_color(code) {
        return code;
    }
    match code {
        "N1" => &palette.n1,
        "N2" => &palette.n2,
        "N3" => &palette.n3,
        "N4" => &palette.n4,
        "N5" => &palette.n5,
        "N6" => &palette.n6,
        "N7" => &palette.n7,
        "B1" => &palette.b1,
        "B2" => &palette.b2,
        "B3" => &palette.b3,
        "B4" => &palette.b4,
        "B5" => &palette.b5,
        "B6" => &palette.b6,
        "AA2" => &palette.aa2,
        "AA4" => &palette.aa4,
        "AA5" => &palette.aa5,
        "AB4" => &palette.ab4,
        "AB5" => &palette.ab5,
        _ => "",
    }
}

// ---------------------------------------------------------------------------
// ThemableElement  (SVG element builder with theme color resolution)
// ---------------------------------------------------------------------------

/// SVG element builder that resolves theme colors at render time.
///
/// Ported from Go `d2themes/element.go`.
#[derive(Debug, Clone)]
pub struct ThemableElement {
    tag: String,

    pub x: Option<f64>,
    pub x1: Option<f64>,
    pub x2: Option<f64>,
    pub y: Option<f64>,
    pub y1: Option<f64>,
    pub y2: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub r: Option<f64>,
    pub rx: Option<f64>,
    pub ry: Option<f64>,
    pub cx: Option<f64>,
    pub cy: Option<f64>,

    pub d: String,
    pub mask: String,
    pub points: String,
    pub transform: String,
    pub href: String,
    pub xmlns: String,

    pub fill: String,
    pub stroke: String,
    pub stroke_dash_array: String,
    pub background_color: String,
    pub color: String,

    pub class_name: String,
    pub style: String,
    pub attributes: String,

    pub content: String,
    pub clip_path: String,

    pub fill_pattern: String,

    /// When set, theme colors are resolved inline instead of via CSS class.
    inline_theme: Option<ResolvedTheme>,
}

impl ThemableElement {
    pub fn new(tag: &str, inline_theme: Option<&ResolvedTheme>) -> Self {
        let xmlns = if tag == "div" {
            "http://www.w3.org/1999/xhtml".to_owned()
        } else {
            String::new()
        };
        Self {
            tag: tag.to_owned(),
            x: None,
            x1: None,
            x2: None,
            y: None,
            y1: None,
            y2: None,
            width: None,
            height: None,
            r: None,
            rx: None,
            ry: None,
            cx: None,
            cy: None,
            d: String::new(),
            mask: String::new(),
            points: String::new(),
            transform: String::new(),
            href: String::new(),
            xmlns,
            fill: String::new(),
            stroke: String::new(),
            stroke_dash_array: String::new(),
            background_color: String::new(),
            color: String::new(),
            class_name: String::new(),
            style: String::new(),
            attributes: String::new(),
            content: String::new(),
            clip_path: String::new(),
            fill_pattern: String::new(),
            inline_theme: inline_theme.cloned(),
        }
    }

    pub fn copy(&self) -> Self {
        self.clone()
    }

    pub fn set_translate(&mut self, x: f64, y: f64) {
        self.transform = format!("translate({x:.6} {y:.6})");
    }

    pub fn set_mask_url(&mut self, url: &str) {
        self.mask = format!("url(#{url})");
    }

    /// Render the element to an SVG/XML string.
    pub fn render(&self) -> String {
        let mut out = format!("<{}", self.tag);

        // href has to be at the top for the img bundler to detect <image> tags correctly
        if !self.href.is_empty() {
            out += &format!(r#" href="{}""#, self.href);
        }
        if let Some(v) = self.x {
            out += &format!(r#" x="{v:.6}""#);
        }
        if let Some(v) = self.x1 {
            out += &format!(r#" x1="{v:.6}""#);
        }
        if let Some(v) = self.x2 {
            out += &format!(r#" x2="{v:.6}""#);
        }
        if let Some(v) = self.y {
            out += &format!(r#" y="{v:.6}""#);
        }
        if let Some(v) = self.y1 {
            out += &format!(r#" y1="{v:.6}""#);
        }
        if let Some(v) = self.y2 {
            out += &format!(r#" y2="{v:.6}""#);
        }
        if let Some(v) = self.width {
            out += &format!(r#" width="{v:.6}""#);
        }
        if let Some(v) = self.height {
            out += &format!(r#" height="{v:.6}""#);
        }
        if let Some(v) = self.r {
            out += &format!(r#" r="{v:.6}""#);
        }
        // Match Go d2themes element.go: skip rx when it equals MaxFloat64,
        // which the d2svg renderer uses as a sentinel meaning "no border
        // radius set" (so that SHAPE_RECTANGLE renders without rx).
        if let Some(rx) = self.rx
            && rx != f64::MAX
        {
            let w = self.width.unwrap_or(f64::MAX);
            let h = self.height.unwrap_or(f64::MAX);
            out += &format!(r#" rx="{:.6}""#, calculate_axis_radius(rx, w, h));
        }
        if let Some(ry) = self.ry
            && ry != f64::MAX
        {
            let w = self.width.unwrap_or(f64::MAX);
            let h = self.height.unwrap_or(f64::MAX);
            out += &format!(r#" ry="{:.6}""#, calculate_axis_radius(ry, w, h));
        }
        if let Some(v) = self.cx {
            out += &format!(r#" cx="{v:.6}""#);
        }
        if let Some(v) = self.cy {
            out += &format!(r#" cy="{v:.6}""#);
        }
        if !self.stroke_dash_array.is_empty() {
            out += &format!(r#" stroke-dasharray="{}""#, self.stroke_dash_array);
        }
        if !self.d.is_empty() {
            out += &format!(r#" d="{}""#, self.d);
        }
        if !self.mask.is_empty() {
            out += &format!(r#" mask="{}""#, self.mask);
        }
        if !self.points.is_empty() {
            out += &format!(r#" points="{}""#, self.points);
        }
        if !self.transform.is_empty() {
            out += &format!(r#" transform="{}""#, self.transform);
        }
        if !self.xmlns.is_empty() {
            out += &format!(r#" xmlns="{}""#, self.xmlns);
        }

        let mut class = self.class_name.clone();
        let style = self.style.clone();

        // Add class {property}-{theme color} if the color is from a theme
        if crate::color::is_theme_color(&self.stroke) {
            class += &format!(" stroke-{}", self.stroke);
            if let Some(ref theme) = self.inline_theme {
                out += &format!(r#" stroke="{}""#, theme.resolve_color(&self.stroke));
            }
        } else if !self.stroke.is_empty() {
            let s = if crate::color::is_gradient(&self.stroke) {
                format!("url('#{}')", crate::color::unique_gradient_id(&self.stroke))
            } else {
                self.stroke.clone()
            };
            out += &format!(r#" stroke="{s}""#);
        }

        if crate::color::is_theme_color(&self.fill) {
            class += &format!(" fill-{}", self.fill);
            if let Some(ref theme) = self.inline_theme {
                out += &format!(r#" fill="{}""#, theme.resolve_color(&self.fill));
            }
        } else if !self.fill.is_empty() {
            let s = if crate::color::is_gradient(&self.fill) {
                format!("url('#{}')", crate::color::unique_gradient_id(&self.fill))
            } else {
                self.fill.clone()
            };
            out += &format!(r#" fill="{s}""#);
        }

        if crate::color::is_theme_color(&self.background_color) {
            class += &format!(" background-color-{}", self.background_color);
            if let Some(ref theme) = self.inline_theme {
                out += &format!(
                    r#" background-color="{}""#,
                    theme.resolve_color(&self.background_color)
                );
            }
        } else if !self.background_color.is_empty() {
            out += &format!(r#" background-color="{}""#, self.background_color);
        }

        if crate::color::is_theme_color(&self.color) {
            class += &format!(" color-{}", self.color);
            if let Some(ref theme) = self.inline_theme {
                out += &format!(r#" color="{}""#, theme.resolve_color(&self.color));
            }
        } else if !self.color.is_empty() {
            out += &format!(r#" color="{}""#, self.color);
        }

        if !class.is_empty() {
            out += &format!(r#" class="{class}""#);
        }
        if !style.is_empty() {
            out += &format!(r#" style="{style}""#);
        }
        if !self.attributes.is_empty() {
            out += &format!(" {}", self.attributes);
        }

        if !self.clip_path.is_empty() {
            out += &format!(r#" clip-path="url(#{})""#, self.clip_path);
        }

        if !self.content.is_empty() {
            return format!("{out}>{}</{}>", self.content, self.tag);
        }

        out += " />";

        if !self.fill_pattern.is_empty() && self.fill_pattern != "none" {
            let mut pattern_el = self.copy();
            pattern_el.fill.clear();
            pattern_el.stroke.clear();
            pattern_el.background_color.clear();
            pattern_el.color.clear();
            pattern_el.class_name = format!("{}-overlay", self.fill_pattern);
            pattern_el.fill_pattern.clear();
            out += &pattern_el.render();
        }

        out
    }
}

fn calculate_axis_radius(border_radius: f64, width: f64, height: f64) -> f64 {
    let min_side = width.min(height);
    let max_radius = min_side / 2.0;
    border_radius.min(max_radius)
}

// ===========================================================================
// Theme Catalog
// ===========================================================================

pub mod catalog {
    use super::*;

    // -- Terminal custom neutrals --

    const TERMINAL_NEUTRAL: Neutral = Neutral {
        n1: "#000410",
        n2: "#0000B8",
        n3: "#9499AB",
        n4: "#CFD2DD",
        n5: "#C3DEF3",
        n6: "#EEF1F8",
        n7: "#FFFFFF",
    };

    const TERMINAL_GRAYSCALE_NEUTRAL: Neutral = Neutral {
        n1: "#000410",
        n2: "#000410",
        n3: "#9499AB",
        n4: "#FFFFFF",
        n5: "#FFFFFF",
        n6: "#EEF1F8",
        n7: "#FFFFFF",
    };

    const ORIGAMI_NEUTRAL: Neutral = Neutral {
        n1: "#170206",
        n2: "#6F0019",
        n3: "#FFFFFF",
        n4: "#E07088",
        n5: "#D2B098",
        n6: "#FFFFFF",
        n7: "#FFFFFF",
    };

    // -----------------------------------------------------------------------
    // Light themes
    // -----------------------------------------------------------------------

    pub const NEUTRAL_DEFAULT: Theme = Theme {
        id: 0,
        name: "Neutral Default",
        colors: ColorPalette {
            neutrals: COOL_NEUTRAL,
            b1: "#0D32B2",
            b2: "#0D32B2",
            b3: "#E3E9FD",
            b4: "#E3E9FD",
            b5: "#EDF0FD",
            b6: "#F7F8FE",
            aa2: "#4A6FF3",
            aa4: "#EDF0FD",
            aa5: "#F7F8FE",
            ab4: "#EDF0FD",
            ab5: "#F7F8FE",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const NEUTRAL_GREY: Theme = Theme {
        id: 1,
        name: "Neutral Grey",
        colors: ColorPalette {
            neutrals: COOL_NEUTRAL,
            b1: "#0A0F25",
            b2: "#676C7E",
            b3: "#9499AB",
            b4: "#CFD2DD",
            b5: "#DEE1EB",
            b6: "#EEF1F8",
            aa2: "#676C7E",
            aa4: "#CFD2DD",
            aa5: "#DEE1EB",
            ab4: "#CFD2DD",
            ab5: "#DEE1EB",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const FLAGSHIP_TERRASTRUCT: Theme = Theme {
        id: 3,
        name: "Flagship Terrastruct",
        colors: ColorPalette {
            neutrals: COOL_NEUTRAL,
            b1: "#000E3D",
            b2: "#234CDA",
            b3: "#6B8AFB",
            b4: "#A6B8F8",
            b5: "#D2DBFD",
            b6: "#E7EAFF",
            aa2: "#5829DC",
            aa4: "#B4AEF8",
            aa5: "#E4DBFF",
            ab4: "#7FDBF8",
            ab5: "#C3F0FF",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const COOL_CLASSICS: Theme = Theme {
        id: 4,
        name: "Cool Classics",
        colors: ColorPalette {
            neutrals: COOL_NEUTRAL,
            b1: "#000536",
            b2: "#0F66B7",
            b3: "#4393DD",
            b4: "#87BFF3",
            b5: "#BCDDFB",
            b6: "#E5F3FF",
            aa2: "#076F6F",
            aa4: "#77DEDE",
            aa5: "#C3F8F8",
            ab4: "#C1A2F3",
            ab5: "#DACEFB",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const MIXED_BERRY_BLUE: Theme = Theme {
        id: 5,
        name: "Mixed Berry Blue",
        colors: ColorPalette {
            neutrals: COOL_NEUTRAL,
            b1: "#000536",
            b2: "#0F66B7",
            b3: "#4393DD",
            b4: "#87BFF3",
            b5: "#BCDDFB",
            b6: "#E5F3FF",
            aa2: "#7639C5",
            aa4: "#C1A2F3",
            aa5: "#DACEFB",
            ab4: "#EA99C6",
            ab5: "#FFDEF1",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const GRAPE_SODA: Theme = Theme {
        id: 6,
        name: "Grape Soda",
        colors: ColorPalette {
            neutrals: COOL_NEUTRAL,
            b1: "#170034",
            b2: "#7639C5",
            b3: "#8F70D1",
            b4: "#C1A2F3",
            b5: "#DACEFB",
            b6: "#F2EDFF",
            aa2: "#0F66B7",
            aa4: "#87BFF3",
            aa5: "#BCDDFB",
            ab4: "#EA99C6",
            ab5: "#FFDAEF",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const AUBERGINE: Theme = Theme {
        id: 7,
        name: "Aubergine",
        colors: ColorPalette {
            neutrals: COOL_NEUTRAL,
            b1: "#170034",
            b2: "#7639C5",
            b3: "#8F70D1",
            b4: "#D0B9F5",
            b5: "#E7DEFF",
            b6: "#F4F0FF",
            aa2: "#0F66B7",
            aa4: "#87BFF3",
            aa5: "#BCDDFB",
            ab4: "#92E3E3",
            ab5: "#D7F5F5",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const COLORBLIND_CLEAR: Theme = Theme {
        id: 8,
        name: "Colorblind Clear",
        colors: ColorPalette {
            neutrals: COOL_NEUTRAL,
            b1: "#010E31",
            b2: "#173688",
            b3: "#5679D4",
            b4: "#84A1EC",
            b5: "#C8D6F9",
            b6: "#E5EDFF",
            aa2: "#048E63",
            aa4: "#A6E2D0",
            aa5: "#CAF2E6",
            ab4: "#FFDA90",
            ab5: "#FFF0D1",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const VANILLA_NITRO_COLA: Theme = Theme {
        id: 100,
        name: "Vanilla Nitro Cola",
        colors: ColorPalette {
            neutrals: WARM_NEUTRAL,
            b1: "#1E1303",
            b2: "#55452F",
            b3: "#9A876C",
            b4: "#C9B9A1",
            b5: "#E9DBCA",
            b6: "#FAF1E6",
            aa2: "#D35F0A",
            aa4: "#FABA8A",
            aa5: "#FFE0C7",
            ab4: "#84A1EC",
            ab5: "#D5E0FD",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const ORANGE_CREAMSICLE: Theme = Theme {
        id: 101,
        name: "Orange Creamsicle",
        colors: ColorPalette {
            neutrals: WARM_NEUTRAL,
            b1: "#311602",
            b2: "#D35F0A",
            b3: "#F18F47",
            b4: "#FABA8A",
            b5: "#FFE0C7",
            b6: "#FFF6EF",
            aa2: "#13A477",
            aa4: "#A6E2D0",
            aa5: "#CAF2E6",
            ab4: "#FEEC8C",
            ab5: "#FFF8CF",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const SHIRLEY_TEMPLE: Theme = Theme {
        id: 102,
        name: "Shirley Temple",
        colors: ColorPalette {
            neutrals: WARM_NEUTRAL,
            b1: "#31021D",
            b2: "#9B1A48",
            b3: "#D2517F",
            b4: "#EA99B6",
            b5: "#FFDAE7",
            b6: "#FCEDF2",
            aa2: "#D35F0A",
            aa4: "#FABA8A",
            aa5: "#FFE0C7",
            ab4: "#FFE767",
            ab5: "#FFF2AA",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const EARTH_TONES: Theme = Theme {
        id: 103,
        name: "Earth Tones",
        colors: ColorPalette {
            neutrals: WARM_NEUTRAL,
            b1: "#1E1303",
            b2: "#55452F",
            b3: "#9A876C",
            b4: "#C9B9A1",
            b5: "#E9DBCA",
            b6: "#FAF1E6",
            aa2: "#D35F0A",
            aa4: "#FABA8A",
            aa5: "#FFE0C7",
            ab4: "#FFE767",
            ab5: "#FFF2AA",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const EVERGLADE_GREEN: Theme = Theme {
        id: 104,
        name: "Everglade Green",
        colors: ColorPalette {
            neutrals: WARM_NEUTRAL,
            b1: "#023324",
            b2: "#048E63",
            b3: "#49BC99",
            b4: "#A6E2D0",
            b5: "#CAF2E6",
            b6: "#EBFDF7",
            aa2: "#D35F0A",
            aa4: "#FABA8A",
            aa5: "#FFE0C7",
            ab4: "#C9B9A1",
            ab5: "#E9DBCA",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const BUTTERED_TOAST: Theme = Theme {
        id: 105,
        name: "Buttered Toast",
        colors: ColorPalette {
            neutrals: WARM_NEUTRAL,
            b1: "#312102",
            b2: "#DF9C18",
            b3: "#FDC659",
            b4: "#FFDA90",
            b5: "#FFF0D1",
            b6: "#FFF7E7",
            aa2: "#55452F",
            aa4: "#C9B9A1",
            aa5: "#E9DBCA",
            ab4: "#FABA8A",
            ab5: "#FFE0C7",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const TERMINAL: Theme = Theme {
        id: 300,
        name: "Terminal",
        colors: ColorPalette {
            neutrals: TERMINAL_NEUTRAL,
            b1: "#000410",
            b2: "#0000E4",
            b3: "#5AA4DC",
            b4: "#E7E9EE",
            b5: "#F5F6F9",
            b6: "#FFFFFF",
            aa2: "#008566",
            aa4: "#45BBA5",
            aa5: "#7ACCBD",
            ab4: "#F1C759",
            ab5: "#F9E088",
        },
        special_rules: SpecialRules {
            mono: true,
            no_corner_radius: true,
            outer_container_double_border: true,
            container_dots: true,
            caps_lock: true,
            c4: false,
            all_paper: false,
        },
    };

    pub const TERMINAL_GRAYSCALE: Theme = Theme {
        id: 301,
        name: "Terminal Grayscale",
        colors: ColorPalette {
            neutrals: TERMINAL_GRAYSCALE_NEUTRAL,
            b1: "#000410",
            b2: "#000410",
            b3: "#FFFFFF",
            b4: "#E7E9EE",
            b5: "#F5F6F9",
            b6: "#FFFFFF",
            aa2: "#6D7284",
            aa4: "#F5F6F9",
            aa5: "#FFFFFF",
            ab4: "#F5F6F9",
            ab5: "#FFFFFF",
        },
        special_rules: SpecialRules {
            mono: true,
            no_corner_radius: true,
            outer_container_double_border: true,
            container_dots: true,
            caps_lock: true,
            c4: false,
            all_paper: false,
        },
    };

    pub const ORIGAMI: Theme = Theme {
        id: 302,
        name: "Origami",
        colors: ColorPalette {
            neutrals: ORIGAMI_NEUTRAL,
            b1: "#170206",
            b2: "#A62543",
            b3: "#E07088",
            b4: "#F3E0D2",
            b5: "#FAF1E6",
            b6: "#FFFBF8",
            aa2: "#0A4EA6",
            aa4: "#3182CD",
            aa5: "#68A8E4",
            ab4: "#E07088",
            ab5: "#F19CAE",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: true,
            outer_container_double_border: true,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: true,
        },
    };

    pub const C4: Theme = Theme {
        id: 303,
        name: "C4",
        colors: ColorPalette {
            neutrals: Neutral {
                n1: "#0f5eaa",
                n2: "#707070",
                n3: "#FFFFFF",
                n4: "#073b6f",
                n5: "#999999",
                n6: "#FFFFFF",
                n7: "#FFFFFF",
            },
            b1: "#073b6f",
            b2: "#08427b",
            b3: "#3c7fc0",
            b4: "#438dd5",
            b5: "#8a8a8a",
            b6: "#999999",
            aa2: "#0f5eaa",
            aa4: "#707070",
            aa5: "#f5f5f5",
            ab4: "#e1e1e1",
            ab5: "#f0f0f0",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: true,
            all_paper: false,
        },
    };

    // -----------------------------------------------------------------------
    // Dark themes
    // -----------------------------------------------------------------------

    pub const DARK_MAUVE: Theme = Theme {
        id: 200,
        name: "Dark Mauve",
        colors: ColorPalette {
            neutrals: DARK_MAUVE_NEUTRAL,
            b1: "#CBA6f7",
            b2: "#CBA6f7",
            b3: "#6C7086",
            b4: "#585B70",
            b5: "#45475A",
            b6: "#313244",
            aa2: "#f38BA8",
            aa4: "#45475A",
            aa5: "#313244",
            ab4: "#45475A",
            ab5: "#313244",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    pub const DARK_FLAGSHIP_TERRASTRUCT: Theme = Theme {
        id: 201,
        name: "Dark Flagship Terrastruct",
        colors: ColorPalette {
            neutrals: DARK_NEUTRAL,
            b1: "#F4F6FA",
            b2: "#6B8AFB",
            b3: "#3733E9",
            b4: "#070B67",
            b5: "#0B1197",
            b6: "#3733E9",
            aa2: "#8B5DEE",
            aa4: "#4918B1",
            aa5: "#7240DD",
            ab4: "#00607C",
            ab5: "#01799D",
        },
        special_rules: SpecialRules {
            mono: false,
            no_corner_radius: false,
            outer_container_double_border: false,
            container_dots: false,
            caps_lock: false,
            c4: false,
            all_paper: false,
        },
    };

    // -----------------------------------------------------------------------
    // Catalog arrays and lookup
    // -----------------------------------------------------------------------

    pub const LIGHT_CATALOG: &[&Theme] = &[
        &NEUTRAL_DEFAULT,
        &NEUTRAL_GREY,
        &FLAGSHIP_TERRASTRUCT,
        &COOL_CLASSICS,
        &MIXED_BERRY_BLUE,
        &GRAPE_SODA,
        &AUBERGINE,
        &COLORBLIND_CLEAR,
        &VANILLA_NITRO_COLA,
        &ORANGE_CREAMSICLE,
        &SHIRLEY_TEMPLE,
        &EARTH_TONES,
        &EVERGLADE_GREEN,
        &BUTTERED_TOAST,
        &TERMINAL,
        &TERMINAL_GRAYSCALE,
        &ORIGAMI,
        &C4,
    ];

    pub const DARK_CATALOG: &[&Theme] = &[&DARK_MAUVE, &DARK_FLAGSHIP_TERRASTRUCT];

    /// Find a theme by its numeric ID. Returns `None` if not found.
    pub fn find(id: i64) -> Option<&'static Theme> {
        for t in LIGHT_CATALOG {
            if t.id == id {
                return Some(t);
            }
        }
        DARK_CATALOG.iter().find(|&t| t.id == id).map(|v| v as _)
    }

    /// Build a human-readable catalog listing for CLI usage.
    pub fn cli_string() -> String {
        let mut s = String::from("Light:\n");
        for t in LIGHT_CATALOG {
            s += &format!("- {}: {}\n", t.name, t.id);
        }
        s += "Dark:\n";
        for t in DARK_CATALOG {
            s += &format!("- {}: {}\n", t.name, t.id);
        }
        s
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_theme_color_known() {
        let theme = &catalog::NEUTRAL_DEFAULT;
        assert_eq!(resolve_theme_color(theme, "N1"), "#0A0F25");
        assert_eq!(resolve_theme_color(theme, "N7"), "#FFFFFF");
        assert_eq!(resolve_theme_color(theme, "B1"), "#0D32B2");
        assert_eq!(resolve_theme_color(theme, "AA2"), "#4A6FF3");
        assert_eq!(resolve_theme_color(theme, "AB4"), "#EDF0FD");
    }

    #[test]
    fn resolve_theme_color_passthrough() {
        let theme = &catalog::NEUTRAL_DEFAULT;
        assert_eq!(resolve_theme_color(theme, "#FF0000"), "#FF0000");
        assert_eq!(resolve_theme_color(theme, "red"), "red");
    }

    #[test]
    fn resolve_theme_color_unknown_code() {
        let theme = &catalog::NEUTRAL_DEFAULT;
        assert_eq!(resolve_theme_color(theme, "ZZ9"), "ZZ9");
    }

    #[test]
    fn theme_is_dark() {
        assert!(!catalog::NEUTRAL_DEFAULT.is_dark());
        assert!(!catalog::TERMINAL.is_dark());
        assert!(catalog::DARK_MAUVE.is_dark());
        assert!(catalog::DARK_FLAGSHIP_TERRASTRUCT.is_dark());
    }

    #[test]
    fn catalog_find() {
        assert_eq!(catalog::find(0).unwrap().name, "Neutral Default");
        assert_eq!(catalog::find(200).unwrap().name, "Dark Mauve");
        assert!(catalog::find(9999).is_none());
    }

    #[test]
    fn catalog_counts() {
        assert_eq!(catalog::LIGHT_CATALOG.len(), 18);
        assert_eq!(catalog::DARK_CATALOG.len(), 2);
    }

    #[test]
    fn themable_element_self_closing() {
        let el = ThemableElement::new("circle", None);
        assert_eq!(el.render(), "<circle />");
    }

    #[test]
    fn themable_element_with_content() {
        let mut el = ThemableElement::new("text", None);
        el.content = "Hello".to_owned();
        assert_eq!(el.render(), "<text>Hello</text>");
    }

    #[test]
    fn themable_element_with_theme_stroke() {
        let resolved = ResolvedTheme::from_theme(&catalog::NEUTRAL_DEFAULT, None);
        let mut el = ThemableElement::new("rect", Some(&resolved));
        el.stroke = "N1".to_owned();
        el.x = Some(10.0);
        el.y = Some(20.0);
        let rendered = el.render();
        // Should contain inline resolved stroke and a CSS class
        assert!(
            rendered.contains(r##"stroke="#0A0F25""##),
            "rendered: {rendered}"
        );
        assert!(rendered.contains("stroke-N1"), "rendered: {rendered}");
    }

    #[test]
    fn themable_element_non_theme_fill() {
        let mut el = ThemableElement::new("rect", None);
        el.fill = "#FF0000".to_owned();
        let rendered = el.render();
        assert!(
            rendered.contains(r##"fill="#FF0000""##),
            "rendered: {rendered}"
        );
        // Should not contain any class for theme colors
        assert!(!rendered.contains("fill-"), "rendered: {rendered}");
    }

    #[test]
    fn themable_element_div_has_xmlns() {
        let el = ThemableElement::new("div", None);
        let rendered = el.render();
        assert!(
            rendered.contains(r#"xmlns="http://www.w3.org/1999/xhtml""#),
            "rendered: {rendered}"
        );
    }

    #[test]
    fn apply_overrides() {
        let theme = &catalog::NEUTRAL_DEFAULT;
        let overrides = ThemeOverrides {
            n1: Some("#111111".to_owned()),
            b1: Some("#222222".to_owned()),
            ..Default::default()
        };
        let palette = theme.apply_overrides(&overrides);
        assert_eq!(palette.n1, "#111111");
        assert_eq!(palette.b1, "#222222");
        // Unchanged values should be from original theme
        assert_eq!(palette.n7, "#FFFFFF");
        assert_eq!(palette.b2, "#0D32B2");
    }

    #[test]
    fn cli_string_not_empty() {
        let s = catalog::cli_string();
        assert!(s.contains("Neutral Default"));
        assert!(s.contains("Dark Mauve"));
        assert!(s.contains("Light:"));
        assert!(s.contains("Dark:"));
    }
}
