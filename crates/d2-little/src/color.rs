use std::collections::HashMap;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Theme color constants
// ---------------------------------------------------------------------------

/// Foreground color.
pub const N1: &str = "N1";
pub const N2: &str = "N2";
pub const N3: &str = "N3";
pub const N4: &str = "N4";
pub const N5: &str = "N5";
pub const N6: &str = "N6";
/// Background color.
pub const N7: &str = "N7";

/// Base colors (used for containers).
pub const B1: &str = "B1";
pub const B2: &str = "B2";
pub const B3: &str = "B3";
pub const B4: &str = "B4";
pub const B5: &str = "B5";
pub const B6: &str = "B6";

/// Alternative colors A.
pub const AA2: &str = "AA2";
pub const AA4: &str = "AA4";
pub const AA5: &str = "AA5";

/// Alternative colors B.
pub const AB4: &str = "AB4";
pub const AB5: &str = "AB5";

// ---------------------------------------------------------------------------
// Theme color detection
// ---------------------------------------------------------------------------

/// Returns `true` when `color` is a d2 theme color token (N1-N7, B1-B6, AA2/AA4/AA5, AB4/AB5).
pub fn is_theme_color(color: &str) -> bool {
    matches!(
        color,
        "N1" | "N2"
            | "N3"
            | "N4"
            | "N5"
            | "N6"
            | "N7"
            | "B1"
            | "B2"
            | "B3"
            | "B4"
            | "B5"
            | "B6"
            | "AA2"
            | "AA4"
            | "AA5"
            | "AB4"
            | "AB5"
    )
}

// ---------------------------------------------------------------------------
// RGB helpers
// ---------------------------------------------------------------------------

/// Simple RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    pub fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    /// Perceived brightness test (HSP color model).
    pub fn is_light(&self) -> bool {
        let r = self.red as f64;
        let g = self.green as f64;
        let b = self.blue as f64;
        let hsp = (0.299 * r * r + 0.587 * g * g + 0.114 * b * b).sqrt();
        hsp > 130.0
    }

    /// Convert to `#RRGGBB` hex string.
    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }
}

/// Parse a `#RRGGBB` or `#RGB` hex string into [`Rgb`].
pub fn hex_to_rgb(hex: &str) -> Result<Rgb, String> {
    let hex = hex
        .strip_prefix('#')
        .ok_or_else(|| format!("cannot parse hex color {hex}"))?;
    match hex.len() {
        6 => {
            let val =
                u32::from_str_radix(hex, 16).map_err(|e| format!("invalid hex color: {e}"))?;
            Ok(Rgb {
                red: (val >> 16) as u8,
                green: ((val >> 8) & 0xFF) as u8,
                blue: (val & 0xFF) as u8,
            })
        }
        3 => {
            let val =
                u32::from_str_radix(hex, 16).map_err(|e| format!("invalid hex color: {e}"))?;
            let r = ((val >> 8) & 0xF) as u8;
            let g = ((val >> 4) & 0xF) as u8;
            let b = (val & 0xF) as u8;
            Ok(Rgb {
                red: r << 4 | r,
                green: g << 4 | g,
                blue: b << 4 | b,
            })
        }
        _ => Err(format!("cannot parse hex color #{hex}")),
    }
}

// ---------------------------------------------------------------------------
// CSS named color map  (W3C css-color-4 / SVG named colors)
// ---------------------------------------------------------------------------

/// Map of CSS named color names (lowercase) to their `(R, G, B)` values.
pub static NAMED_RGB_MAP: LazyLock<HashMap<&'static str, [u8; 3]>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("aliceblue", [240, 248, 255]);
    m.insert("antiquewhite", [250, 235, 215]);
    m.insert("aqua", [0, 255, 255]);
    m.insert("aquamarine", [127, 255, 212]);
    m.insert("azure", [240, 255, 255]);
    m.insert("beige", [245, 245, 220]);
    m.insert("bisque", [255, 228, 196]);
    m.insert("black", [0, 0, 0]);
    m.insert("blanchedalmond", [255, 235, 205]);
    m.insert("blue", [0, 0, 255]);
    m.insert("blueviolet", [138, 43, 226]);
    m.insert("brown", [165, 42, 42]);
    m.insert("burlywood", [222, 184, 135]);
    m.insert("cadetblue", [95, 158, 160]);
    m.insert("chartreuse", [127, 255, 0]);
    m.insert("chocolate", [210, 105, 30]);
    m.insert("coral", [255, 127, 80]);
    m.insert("cornflowerblue", [100, 149, 237]);
    m.insert("cornsilk", [255, 248, 220]);
    m.insert("crimson", [220, 20, 60]);
    m.insert("cyan", [0, 255, 255]);
    m.insert("darkblue", [0, 0, 139]);
    m.insert("darkcyan", [0, 139, 139]);
    m.insert("darkgoldenrod", [184, 134, 11]);
    m.insert("darkgray", [169, 169, 169]);
    m.insert("darkgreen", [0, 100, 0]);
    m.insert("darkgrey", [169, 169, 169]);
    m.insert("darkkhaki", [189, 183, 107]);
    m.insert("darkmagenta", [139, 0, 139]);
    m.insert("darkolivegreen", [85, 107, 47]);
    m.insert("darkorange", [255, 140, 0]);
    m.insert("darkorchid", [153, 50, 204]);
    m.insert("darkred", [139, 0, 0]);
    m.insert("darksalmon", [233, 150, 122]);
    m.insert("darkseagreen", [143, 188, 143]);
    m.insert("darkslateblue", [72, 61, 139]);
    m.insert("darkslategray", [47, 79, 79]);
    m.insert("darkslategrey", [47, 79, 79]);
    m.insert("darkturquoise", [0, 206, 209]);
    m.insert("darkviolet", [148, 0, 211]);
    m.insert("deeppink", [255, 20, 147]);
    m.insert("deepskyblue", [0, 191, 255]);
    m.insert("dimgray", [105, 105, 105]);
    m.insert("dimgrey", [105, 105, 105]);
    m.insert("dodgerblue", [30, 144, 255]);
    m.insert("firebrick", [178, 34, 34]);
    m.insert("floralwhite", [255, 250, 240]);
    m.insert("forestgreen", [34, 139, 34]);
    m.insert("fuchsia", [255, 0, 255]);
    m.insert("gainsboro", [220, 220, 220]);
    m.insert("ghostwhite", [248, 248, 255]);
    m.insert("gold", [255, 215, 0]);
    m.insert("goldenrod", [218, 165, 32]);
    m.insert("gray", [128, 128, 128]);
    m.insert("green", [0, 128, 0]);
    m.insert("greenyellow", [173, 255, 47]);
    m.insert("grey", [128, 128, 128]);
    m.insert("honeydew", [240, 255, 240]);
    m.insert("hotpink", [255, 105, 180]);
    m.insert("indianred", [205, 92, 92]);
    m.insert("indigo", [75, 0, 130]);
    m.insert("ivory", [255, 255, 240]);
    m.insert("khaki", [240, 230, 140]);
    m.insert("lavender", [230, 230, 250]);
    m.insert("lavenderblush", [255, 240, 245]);
    m.insert("lawngreen", [124, 252, 0]);
    m.insert("lemonchiffon", [255, 250, 205]);
    m.insert("lightblue", [173, 216, 230]);
    m.insert("lightcoral", [240, 128, 128]);
    m.insert("lightcyan", [224, 255, 255]);
    m.insert("lightgoldenrodyellow", [250, 250, 210]);
    m.insert("lightgray", [211, 211, 211]);
    m.insert("lightgreen", [144, 238, 144]);
    m.insert("lightgrey", [211, 211, 211]);
    m.insert("lightpink", [255, 182, 193]);
    m.insert("lightsalmon", [255, 160, 122]);
    m.insert("lightseagreen", [32, 178, 170]);
    m.insert("lightskyblue", [135, 206, 250]);
    m.insert("lightslategray", [119, 136, 153]);
    m.insert("lightslategrey", [119, 136, 153]);
    m.insert("lightsteelblue", [176, 196, 222]);
    m.insert("lightyellow", [255, 255, 224]);
    m.insert("lime", [0, 255, 0]);
    m.insert("limegreen", [50, 205, 50]);
    m.insert("linen", [250, 240, 230]);
    m.insert("magenta", [255, 0, 255]);
    m.insert("maroon", [128, 0, 0]);
    m.insert("mediumaquamarine", [102, 205, 170]);
    m.insert("mediumblue", [0, 0, 205]);
    m.insert("mediumorchid", [186, 85, 211]);
    m.insert("mediumpurple", [147, 112, 219]);
    m.insert("mediumseagreen", [60, 179, 113]);
    m.insert("mediumslateblue", [123, 104, 238]);
    m.insert("mediumspringgreen", [0, 250, 154]);
    m.insert("mediumturquoise", [72, 209, 204]);
    m.insert("mediumvioletred", [199, 21, 133]);
    m.insert("midnightblue", [25, 25, 112]);
    m.insert("mintcream", [245, 255, 250]);
    m.insert("mistyrose", [255, 228, 225]);
    m.insert("moccasin", [255, 228, 181]);
    m.insert("navajowhite", [255, 222, 173]);
    m.insert("navy", [0, 0, 128]);
    m.insert("oldlace", [253, 245, 230]);
    m.insert("olive", [128, 128, 0]);
    m.insert("olivedrab", [107, 142, 35]);
    m.insert("orange", [255, 165, 0]);
    m.insert("orangered", [255, 69, 0]);
    m.insert("orchid", [218, 112, 214]);
    m.insert("palegoldenrod", [238, 232, 170]);
    m.insert("palegreen", [152, 251, 152]);
    m.insert("paleturquoise", [175, 238, 238]);
    m.insert("palevioletred", [219, 112, 147]);
    m.insert("papayawhip", [255, 239, 213]);
    m.insert("peachpuff", [255, 218, 185]);
    m.insert("peru", [205, 133, 63]);
    m.insert("pink", [255, 192, 203]);
    m.insert("plum", [221, 160, 221]);
    m.insert("powderblue", [176, 224, 230]);
    m.insert("purple", [128, 0, 128]);
    m.insert("rebeccapurple", [102, 51, 153]);
    m.insert("red", [255, 0, 0]);
    m.insert("rosybrown", [188, 143, 143]);
    m.insert("royalblue", [65, 105, 225]);
    m.insert("saddlebrown", [139, 69, 19]);
    m.insert("salmon", [250, 128, 114]);
    m.insert("sandybrown", [244, 164, 96]);
    m.insert("seagreen", [46, 139, 87]);
    m.insert("seashell", [255, 245, 238]);
    m.insert("sienna", [160, 82, 45]);
    m.insert("silver", [192, 192, 192]);
    m.insert("skyblue", [135, 206, 235]);
    m.insert("slateblue", [106, 90, 205]);
    m.insert("slategray", [112, 128, 144]);
    m.insert("slategrey", [112, 128, 144]);
    m.insert("snow", [255, 250, 250]);
    m.insert("springgreen", [0, 255, 127]);
    m.insert("steelblue", [70, 130, 180]);
    m.insert("tan", [210, 180, 140]);
    m.insert("teal", [0, 128, 128]);
    m.insert("thistle", [216, 191, 216]);
    m.insert("tomato", [255, 99, 71]);
    m.insert("turquoise", [64, 224, 208]);
    m.insert("violet", [238, 130, 238]);
    m.insert("wheat", [245, 222, 179]);
    m.insert("white", [255, 255, 255]);
    m.insert("whitesmoke", [245, 245, 245]);
    m.insert("yellow", [255, 255, 0]);
    m.insert("yellowgreen", [154, 205, 50]);
    m
});

/// All CSS named colors accepted by d2 (lowercase).
pub static NAMED_COLORS: &[&str] = &[
    "currentcolor",
    "transparent",
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgrey",
    "darkgreen",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "grey",
    "green",
    "greenyellow",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgrey",
    "lightgreen",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "turquoise",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
];

/// Look up a CSS named color and return its [`Rgb`]. Returns `Rgb(0,0,0)` if not found.
pub fn name_to_rgb(name: &str) -> Rgb {
    match NAMED_RGB_MAP.get(name.to_lowercase().as_str()) {
        Some(rgb) => Rgb::new(rgb[0], rgb[1], rgb[2]),
        None => Rgb::new(0, 0, 0),
    }
}

// ---------------------------------------------------------------------------
// Color hex regex
// ---------------------------------------------------------------------------

/// Check if a string is a valid `#RRGGBB` or `#RGB` hex color.
pub fn is_color_hex(color: &str) -> bool {
    let Some(hex) = color.strip_prefix('#') else {
        return false;
    };
    match hex.len() {
        3 | 6 => hex.chars().all(|c| c.is_ascii_hexdigit()),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// CSS color parsing (inline, no external crate)
// ---------------------------------------------------------------------------

/// Parse a CSS color string (hex, named, rgb(), rgba(), hsl(), hsla())
/// into `(R, G, B)` floating-point values in 0.0..1.0.
///
/// Returns `Err` if the color cannot be parsed.
pub fn parse_css_color(color: &str) -> Result<(f64, f64, f64), String> {
    let color = color.trim();

    // hex
    if let Some(hex) = color.strip_prefix('#') {
        return parse_hex_to_floats(hex);
    }

    // named
    let lower = color.to_lowercase();
    if let Some(rgb) = NAMED_RGB_MAP.get(lower.as_str()) {
        return Ok((
            rgb[0] as f64 / 255.0,
            rgb[1] as f64 / 255.0,
            rgb[2] as f64 / 255.0,
        ));
    }

    // rgb(r,g,b) / rgba(r,g,b,a)
    if let Some(inner) = strip_func(&lower, "rgba") {
        return parse_rgb_func(inner);
    }
    if let Some(inner) = strip_func(&lower, "rgb") {
        return parse_rgb_func(inner);
    }

    // hsl(h,s%,l%) / hsla(h,s%,l%,a)
    if let Some(inner) = strip_func(&lower, "hsla") {
        return parse_hsl_func(inner);
    }
    if let Some(inner) = strip_func(&lower, "hsl") {
        return parse_hsl_func(inner);
    }

    Err(format!("cannot parse color \"{color}\""))
}

fn strip_func<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let s = s.strip_prefix(name)?;
    let s = s.strip_prefix('(')?;
    let s = s.strip_suffix(')')?;
    Some(s)
}

fn parse_hex_to_floats(hex: &str) -> Result<(f64, f64, f64), String> {
    match hex.len() {
        3 => {
            let val =
                u32::from_str_radix(hex, 16).map_err(|e| format!("invalid hex color: {e}"))?;
            let r = ((val >> 8) & 0xF) as u8;
            let g = ((val >> 4) & 0xF) as u8;
            let b = (val & 0xF) as u8;
            Ok((
                (r << 4 | r) as f64 / 255.0,
                (g << 4 | g) as f64 / 255.0,
                (b << 4 | b) as f64 / 255.0,
            ))
        }
        6 => {
            let val =
                u32::from_str_radix(hex, 16).map_err(|e| format!("invalid hex color: {e}"))?;
            Ok((
                ((val >> 16) & 0xFF) as f64 / 255.0,
                ((val >> 8) & 0xFF) as f64 / 255.0,
                (val & 0xFF) as f64 / 255.0,
            ))
        }
        8 => {
            // #RRGGBBAA - ignore alpha
            let val =
                u32::from_str_radix(hex, 16).map_err(|e| format!("invalid hex color: {e}"))?;
            Ok((
                ((val >> 24) & 0xFF) as f64 / 255.0,
                ((val >> 16) & 0xFF) as f64 / 255.0,
                ((val >> 8) & 0xFF) as f64 / 255.0,
            ))
        }
        _ => Err(format!("invalid hex color length: {hex}")),
    }
}

fn parse_rgb_func(inner: &str) -> Result<(f64, f64, f64), String> {
    // Accept both comma-separated and space-separated, with optional / alpha
    let inner = inner.replace([',', '/'], " ");
    let parts: Vec<&str> = inner.split_whitespace().collect();
    if parts.len() < 3 {
        return Err("rgb() requires at least 3 values".to_string());
    }
    let r = parse_component(parts[0], 255.0)?;
    let g = parse_component(parts[1], 255.0)?;
    let b = parse_component(parts[2], 255.0)?;
    Ok((r, g, b))
}

fn parse_hsl_func(inner: &str) -> Result<(f64, f64, f64), String> {
    let inner = inner.replace([',', '/'], " ");
    let parts: Vec<&str> = inner.split_whitespace().collect();
    if parts.len() < 3 {
        return Err("hsl() requires at least 3 values".to_string());
    }
    let h: f64 = parts[0]
        .trim_end_matches("deg")
        .parse()
        .map_err(|e| format!("invalid hue: {e}"))?;
    let s = parse_percent(parts[1])?;
    let l = parse_percent(parts[2])?;
    let (r, g, b) = hsl_to_rgb(h / 360.0, s, l);
    Ok((r, g, b))
}

fn parse_component(s: &str, max: f64) -> Result<f64, String> {
    if let Some(pct) = s.strip_suffix('%') {
        let v: f64 = pct.parse().map_err(|e| format!("invalid number: {e}"))?;
        Ok((v / 100.0).clamp(0.0, 1.0))
    } else {
        let v: f64 = s.parse().map_err(|e| format!("invalid number: {e}"))?;
        Ok((v / max).clamp(0.0, 1.0))
    }
}

fn parse_percent(s: &str) -> Result<f64, String> {
    let s = s.trim_end_matches('%');
    let v: f64 = s.parse().map_err(|e| format!("invalid percent: {e}"))?;
    Ok((v / 100.0).clamp(0.0, 1.0))
}

/// Convert HSL (all in 0..1) to RGB (all in 0..1).
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    (r, g, b)
}

fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

/// Convert RGB floats (0..1) to `#rrggbb` hex.
fn rgb_floats_to_hex(r: f64, g: f64, b: f64) -> String {
    let r = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

// ---------------------------------------------------------------------------
// Darken
// ---------------------------------------------------------------------------

/// Darken a color by one step.
///
/// For theme colors, returns the next-darker theme color.
/// For CSS colors, decreases HSL luminance by 10%.
pub fn darken(color: &str) -> Result<String, String> {
    if is_theme_color(color) {
        return darken_theme(color);
    }
    darken_css(color)
}

fn darken_theme(color: &str) -> Result<String, String> {
    let bytes = color.as_bytes();
    match bytes[0] {
        b'B' => match bytes[1] {
            b'1' | b'2' => Ok(B1.to_string()),
            b'3' => Ok(B2.to_string()),
            b'4' => Ok(B3.to_string()),
            b'5' => Ok(B4.to_string()),
            b'6' => Ok(B5.to_string()),
            _ => Err(format!("invalid color \"{color}\"")),
        },
        b'N' => match bytes[1] {
            b'1' | b'2' => Ok(N1.to_string()),
            b'3' => Ok(N2.to_string()),
            b'4' => Ok(N3.to_string()),
            b'5' => Ok(N4.to_string()),
            b'6' => Ok(N5.to_string()),
            b'7' => Ok(N6.to_string()),
            _ => Err(format!("invalid color \"{color}\"")),
        },
        b'A' => {
            if bytes.len() < 3 {
                return Err(format!("invalid color \"{color}\""));
            }
            match (bytes[1], bytes[2]) {
                (b'A', b'2') | (b'A', b'4') => Ok(AA2.to_string()),
                (b'A', b'5') => Ok(AA4.to_string()),
                (b'B', b'4') => Ok(AB4.to_string()),
                (b'B', b'5') => Ok(AB5.to_string()),
                _ => Err(format!("invalid color \"{color}\"")),
            }
        }
        _ => Err(format!("invalid color \"{color}\"")),
    }
}

fn darken_css(color: &str) -> Result<String, String> {
    let (r, g, b) = parse_css_color(color)?;
    let (h, s, l) = rgb_to_hsl(r, g, b);
    // decrease luminance by 10%
    let new_l = (l - 0.1).max(0.0);
    let (nr, ng, nb) = hsl_to_rgb(h, s, new_l);
    Ok(rgb_floats_to_hex(nr, ng, nb))
}

/// Convert RGB (0..1) to HSL (h in 0..1, s in 0..1, l in 0..1).
fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < f64::EPSILON {
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if (max - r).abs() < f64::EPSILON {
        let mut h = (g - b) / d;
        if g < b {
            h += 6.0;
        }
        h
    } else if (max - g).abs() < f64::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };

    (h / 6.0, s, l)
}

// ---------------------------------------------------------------------------
// Luminance
// ---------------------------------------------------------------------------

/// Calculate perceived luminance (0.0..1.0) of a CSS color.
///
/// Uses the formula `0.299*R + 0.587*G + 0.114*B` where R, G, B are in 0..1.
pub fn luminance(color: &str) -> Result<f64, String> {
    let (r, g, b) = parse_css_color(color)?;
    Ok(0.299 * r + 0.587 * g + 0.114 * b)
}

/// Luminance category for a color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuminanceCategory {
    Bright,
    Normal,
    Dark,
    Darker,
}

impl LuminanceCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bright => "bright",
            Self::Normal => "normal",
            Self::Dark => "dark",
            Self::Darker => "darker",
        }
    }
}

impl std::fmt::Display for LuminanceCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Categorize a color by its luminance.
///
/// Gradient URLs (`url('#grad-...')`) are always `Normal`.
pub fn luminance_category(color: &str) -> Result<LuminanceCategory, String> {
    if is_url_gradient_id(color) {
        return Ok(LuminanceCategory::Normal);
    }
    let l = luminance(color)?;
    Ok(if l >= 0.88 {
        LuminanceCategory::Bright
    } else if l >= 0.55 {
        LuminanceCategory::Normal
    } else if l >= 0.30 {
        LuminanceCategory::Dark
    } else {
        LuminanceCategory::Darker
    })
}

// ---------------------------------------------------------------------------
// Gradient support
// ---------------------------------------------------------------------------

/// A parsed CSS gradient.
#[derive(Debug, Clone)]
pub struct Gradient {
    /// `"linear"` or `"radial"`.
    pub gradient_type: String,
    /// Direction string (e.g. `"to right"`, `"180deg"`, `"circle"`).
    pub direction: String,
    /// Color stops.
    pub color_stops: Vec<ColorStop>,
    /// Unique ID derived from the CSS source text.
    pub id: String,
}

/// A single color stop in a gradient.
#[derive(Debug, Clone)]
pub struct ColorStop {
    pub color: String,
    pub position: String,
}

/// Parse a CSS gradient string into a [`Gradient`].
pub fn parse_gradient(css: &str) -> Result<Gradient, String> {
    let css = css.trim();

    let (gradient_type, params) = if let Some(rest) = css.strip_prefix("linear-gradient(") {
        (
            "linear",
            rest.strip_suffix(')').ok_or("invalid gradient syntax")?,
        )
    } else if let Some(rest) = css.strip_prefix("radial-gradient(") {
        (
            "radial",
            rest.strip_suffix(')').ok_or("invalid gradient syntax")?,
        )
    } else {
        return Err("invalid gradient syntax".to_string());
    };

    let param_list = split_params(params);
    if param_list.is_empty() {
        return Err("no parameters in gradient".to_string());
    }

    let first = param_list[0].trim();
    let mut gradient = Gradient {
        gradient_type: gradient_type.to_string(),
        direction: String::new(),
        color_stops: Vec::new(),
        id: String::new(),
    };

    let has_direction = (gradient_type == "linear"
        && (first.ends_with("deg") || first.starts_with("to ")))
        || (gradient_type == "radial" && (first == "circle" || first == "ellipse"));
    if has_direction {
        gradient.direction = first.to_string();
        let stops = &param_list[1..];
        if stops.is_empty() {
            return Err("no color stops in gradient".to_string());
        }
        gradient.color_stops = parse_color_stops(stops);
    } else {
        gradient.color_stops = parse_color_stops(&param_list);
    }

    gradient.id = unique_gradient_id(css);
    Ok(gradient)
}

fn split_params(params: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut nesting = 0i32;

    for ch in params.chars() {
        match ch {
            ',' if nesting == 0 => {
                parts.push(buf.clone());
                buf.clear();
                continue;
            }
            '(' => nesting += 1,
            ')' if nesting > 0 => nesting -= 1,
            _ => {}
        }
        buf.push(ch);
    }
    if !buf.is_empty() {
        parts.push(buf);
    }
    parts
}

fn parse_color_stops(params: &[String]) -> Vec<ColorStop> {
    let mut stops = Vec::new();
    for p in params {
        let p = p.trim();
        let parts: Vec<&str> = p.split_whitespace().collect();
        match parts.len() {
            1 => stops.push(ColorStop {
                color: parts[0].to_string(),
                position: String::new(),
            }),
            2 => stops.push(ColorStop {
                color: parts[0].to_string(),
                position: parts[1].to_string(),
            }),
            _ => continue,
        }
    }
    stops
}

/// Generate a unique gradient ID from its CSS source text using SHA-1.
pub fn unique_gradient_id(css_gradient: &str) -> String {
    let digest = sha1_hex(css_gradient.as_bytes());
    format!("grad-{digest}")
}

/// Minimal SHA-1 implementation (only used for gradient IDs).
fn sha1_hex(data: &[u8]) -> String {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let bit_len = (data.len() as u64) * 8;
    // pre-processing: pad message
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // process each 512-bit (64-byte) block
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);

        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    format!("{:08x}{:08x}{:08x}{:08x}{:08x}", h0, h1, h2, h3, h4)
}

/// Convert a gradient to SVG markup.
pub fn gradient_to_svg(gradient: &Gradient) -> String {
    match gradient.gradient_type.as_str() {
        "linear" => linear_gradient_to_svg(gradient),
        "radial" => radial_gradient_to_svg(gradient),
        _ => String::new(),
    }
}

fn linear_gradient_to_svg(gradient: &Gradient) -> String {
    let (x1, y1, x2, y2) = parse_linear_gradient_direction(&gradient.direction);
    let mut sb = String::new();
    sb.push_str(&format!(
        "<linearGradient id=\"{}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">",
        gradient.id, x1, y1, x2, y2
    ));
    sb.push('\n');

    let total = gradient.color_stops.len();
    for (i, cs) in gradient.color_stops.iter().enumerate() {
        let offset = if cs.position.is_empty() {
            let v = i as f64 / (total - 1) as f64 * 100.0;
            format!("{:.2}%", v)
        } else {
            cs.position.clone()
        };
        sb.push_str(&format!(
            "<stop offset=\"{}\" stop-color=\"{}\" />",
            offset, cs.color
        ));
        sb.push('\n');
    }
    sb.push_str("</linearGradient>");
    sb
}

fn parse_linear_gradient_direction(direction: &str) -> (String, String, String, String) {
    let direction = direction.trim();
    if let Some(dir) = direction.strip_prefix("to ") {
        let parts: Vec<&str> = dir.split_whitespace().collect();
        let (mut x_start, mut y_start) = ("50%".to_string(), "50%".to_string());
        let (mut x_end, mut y_end) = ("50%".to_string(), "50%".to_string());
        let mut x_set = false;
        let mut y_set = false;

        for part in &parts {
            match *part {
                "left" => {
                    x_start = "100%".to_string();
                    x_end = "0%".to_string();
                    x_set = true;
                }
                "right" => {
                    x_start = "0%".to_string();
                    x_end = "100%".to_string();
                    x_set = true;
                }
                "top" => {
                    y_start = "100%".to_string();
                    y_end = "0%".to_string();
                    y_set = true;
                }
                "bottom" => {
                    y_start = "0%".to_string();
                    y_end = "100%".to_string();
                    y_set = true;
                }
                _ => {}
            }
        }
        if !x_set {
            x_start = "50%".to_string();
            x_end = "50%".to_string();
        }
        if !y_set {
            y_start = "50%".to_string();
            y_end = "50%".to_string();
        }
        (x_start, y_start, x_end, y_end)
    } else if let Some(angle_str) = direction.strip_suffix("deg") {
        if let Ok(angle) = angle_str.trim().parse::<f64>() {
            let svg_angle = (90.0 - angle) * (std::f64::consts::PI / 180.0);
            let x1 = 50.0;
            let y1 = 50.0;
            let x2 = x1 + 50.0 * svg_angle.cos();
            let y2 = y1 + 50.0 * svg_angle.sin();
            (
                format!("{:.2}%", x1),
                format!("{:.2}%", y1),
                format!("{:.2}%", x2),
                format!("{:.2}%", y2),
            )
        } else {
            (
                "0%".to_string(),
                "0%".to_string(),
                "0%".to_string(),
                "100%".to_string(),
            )
        }
    } else {
        (
            "0%".to_string(),
            "0%".to_string(),
            "0%".to_string(),
            "100%".to_string(),
        )
    }
}

fn radial_gradient_to_svg(gradient: &Gradient) -> String {
    let mut sb = String::new();
    sb.push_str(&format!("<radialGradient id=\"{}\">", gradient.id));
    sb.push('\n');

    let total = gradient.color_stops.len();
    for (i, cs) in gradient.color_stops.iter().enumerate() {
        let offset = if cs.position.is_empty() {
            let v = i as f64 / (total - 1) as f64 * 100.0;
            format!("{:.2}%", v)
        } else {
            cs.position.clone()
        };
        sb.push_str(&format!(
            "<stop offset=\"{}\" stop-color=\"{}\" />",
            offset, cs.color
        ));
        sb.push('\n');
    }
    sb.push_str("</radialGradient>");
    sb
}

/// Check if a string matches the gradient CSS syntax.
pub fn is_gradient(color: &str) -> bool {
    (color.starts_with("linear-gradient(") || color.starts_with("radial-gradient("))
        && color.ends_with(')')
}

/// Check if a string matches the `url('#grad-<sha1>')` format.
pub fn is_url_gradient_id(color: &str) -> bool {
    let Some(rest) = color.strip_prefix("url('#grad-") else {
        return false;
    };
    let Some(hex) = rest.strip_suffix("')") else {
        return false;
    };
    hex.len() == 40 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

// ---------------------------------------------------------------------------
// Color validation
// ---------------------------------------------------------------------------

/// Validate whether a string is a valid d2 color (named, hex, or gradient).
pub fn valid_color(color: &str) -> bool {
    if is_gradient(color) {
        let Ok(gradient) = parse_gradient(color) else {
            return false;
        };
        for cs in &gradient.color_stops {
            if parse_css_color(&cs.color).is_err() {
                return false;
            }
        }
        true
    } else {
        NAMED_COLORS.contains(&color.to_lowercase().as_str()) || is_color_hex(color)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_theme_color() {
        assert!(is_theme_color("N1"));
        assert!(is_theme_color("B6"));
        assert!(is_theme_color("AA2"));
        assert!(is_theme_color("AB5"));
        assert!(!is_theme_color("X1"));
        assert!(!is_theme_color("N8"));
        assert!(!is_theme_color("red"));
    }

    #[test]
    fn test_hex_to_rgb() {
        let rgb = hex_to_rgb("#FF0000").unwrap();
        assert_eq!(rgb, Rgb::new(255, 0, 0));

        let rgb = hex_to_rgb("#0f0").unwrap();
        assert_eq!(rgb, Rgb::new(0, 255, 0));

        assert!(hex_to_rgb("invalid").is_err());
    }

    #[test]
    fn test_rgb_is_light() {
        assert!(Rgb::new(255, 255, 255).is_light());
        assert!(!Rgb::new(0, 0, 0).is_light());
    }

    #[test]
    fn test_name_to_rgb() {
        let rgb = name_to_rgb("red");
        assert_eq!(rgb, Rgb::new(255, 0, 0));

        let rgb = name_to_rgb("cornflowerblue");
        assert_eq!(rgb, Rgb::new(100, 149, 237));

        // unknown returns black
        let rgb = name_to_rgb("notacolor");
        assert_eq!(rgb, Rgb::new(0, 0, 0));
    }

    #[test]
    fn test_is_color_hex() {
        assert!(is_color_hex("#FF0000"));
        assert!(is_color_hex("#abc"));
        assert!(!is_color_hex("FF0000"));
        assert!(!is_color_hex("#GGGGGG"));
        assert!(!is_color_hex("#12345"));
    }

    #[test]
    fn test_darken_theme() {
        assert_eq!(darken("B1").unwrap(), "B1");
        assert_eq!(darken("B3").unwrap(), "B2");
        assert_eq!(darken("N7").unwrap(), "N6");
        assert_eq!(darken("AA5").unwrap(), "AA4");
    }

    #[test]
    fn test_darken_css() {
        let result = darken("#808080").unwrap();
        // gray darkened by 10% luminance
        assert!(result.starts_with('#'));
        assert_eq!(result.len(), 7);
    }

    #[test]
    fn test_luminance() {
        let l = luminance("#FFFFFF").unwrap();
        assert!((l - 1.0).abs() < 0.01);

        let l = luminance("#000000").unwrap();
        assert!(l.abs() < 0.01);
    }

    #[test]
    fn test_luminance_category() {
        assert_eq!(
            luminance_category("#FFFFFF").unwrap(),
            LuminanceCategory::Bright
        );
        assert_eq!(
            luminance_category("#000000").unwrap(),
            LuminanceCategory::Darker
        );
    }

    #[test]
    fn test_luminance_category_gradient_url() {
        let url = "url('#grad-da39a3ee5e6b4b0d3255bfef95601890afd80709')";
        assert_eq!(luminance_category(url).unwrap(), LuminanceCategory::Normal);
    }

    #[test]
    fn test_is_gradient() {
        assert!(is_gradient("linear-gradient(to right, red, blue)"));
        assert!(is_gradient("radial-gradient(circle, red, blue)"));
        assert!(!is_gradient("red"));
        assert!(!is_gradient("#FF0000"));
    }

    #[test]
    fn test_parse_gradient_linear() {
        let g = parse_gradient("linear-gradient(to right, red, blue)").unwrap();
        assert_eq!(g.gradient_type, "linear");
        assert_eq!(g.direction, "to right");
        assert_eq!(g.color_stops.len(), 2);
        assert_eq!(g.color_stops[0].color, "red");
        assert_eq!(g.color_stops[1].color, "blue");
        assert!(g.id.starts_with("grad-"));
    }

    #[test]
    fn test_parse_gradient_radial() {
        let g = parse_gradient("radial-gradient(circle, red 0%, blue 100%)").unwrap();
        assert_eq!(g.gradient_type, "radial");
        assert_eq!(g.direction, "circle");
        assert_eq!(g.color_stops.len(), 2);
        assert_eq!(g.color_stops[0].position, "0%");
    }

    #[test]
    fn test_is_url_gradient_id() {
        assert!(is_url_gradient_id(
            "url('#grad-da39a3ee5e6b4b0d3255bfef95601890afd80709')"
        ));
        assert!(!is_url_gradient_id("url('#grad-short')"));
        assert!(!is_url_gradient_id("red"));
    }

    #[test]
    fn test_valid_color() {
        assert!(valid_color("#FF0000"));
        assert!(valid_color("#abc"));
        assert!(valid_color("red"));
        assert!(valid_color("transparent"));
        assert!(valid_color("linear-gradient(to right, red, blue)"));
        assert!(!valid_color("notacolor"));
    }

    #[test]
    fn test_sha1_hex() {
        // SHA-1 of empty string
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        // SHA-1 of "abc"
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn test_gradient_to_svg_linear() {
        let g = parse_gradient("linear-gradient(to right, red, blue)").unwrap();
        let svg = gradient_to_svg(&g);
        assert!(svg.contains("<linearGradient"));
        assert!(svg.contains("</linearGradient>"));
        assert!(svg.contains("stop-color=\"red\""));
        assert!(svg.contains("stop-color=\"blue\""));
    }

    #[test]
    fn test_gradient_to_svg_radial() {
        let g = parse_gradient("radial-gradient(circle, red, blue)").unwrap();
        let svg = gradient_to_svg(&g);
        assert!(svg.contains("<radialGradient"));
        assert!(svg.contains("</radialGradient>"));
    }

    #[test]
    fn test_parse_css_color_hex() {
        let (r, g, b) = parse_css_color("#ff0000").unwrap();
        assert!((r - 1.0).abs() < 0.01);
        assert!(g.abs() < 0.01);
        assert!(b.abs() < 0.01);
    }

    #[test]
    fn test_parse_css_color_named() {
        let (r, g, b) = parse_css_color("white").unwrap();
        assert!((r - 1.0).abs() < 0.01);
        assert!((g - 1.0).abs() < 0.01);
        assert!((b - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_css_color_rgb_func() {
        let (r, g, b) = parse_css_color("rgb(255, 0, 0)").unwrap();
        assert!((r - 1.0).abs() < 0.01);
        assert!(g.abs() < 0.01);
        assert!(b.abs() < 0.01);
    }

    #[test]
    fn test_parse_css_color_hsl_func() {
        let (r, g, b) = parse_css_color("hsl(0, 100%, 50%)").unwrap();
        assert!((r - 1.0).abs() < 0.01);
        assert!(g.abs() < 0.01);
        assert!(b.abs() < 0.01);
    }

    #[test]
    fn test_rgb_to_hsl_roundtrip() {
        let (h, s, l) = rgb_to_hsl(1.0, 0.0, 0.0);
        let (r, g, b) = hsl_to_rgb(h, s, l);
        assert!((r - 1.0).abs() < 0.01);
        assert!(g.abs() < 0.01);
        assert!(b.abs() < 0.01);
    }
}
