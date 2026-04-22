//! HSL-space color math — lighten/darken/saturate/hue-rotate.
//!
//! Adapted from mermaid-rs-renderer (https://github.com/1jehuang/mermaid-rs-renderer),
//! MIT license. Replaces upstream mermaid's `khroma` JS dependency.

/// Parse a CSS-ish color string into HSL triple `(h, s, l)` where
/// `h` is degrees in `[0, 360)`, `s` and `l` are percentages in
/// `[0, 100]`.
///
/// Accepts `hsl(...)` / `hsla(...)` syntax and `#rgb` / `#rrggbb` /
/// `#rrggbbaa` hex. Returns `None` on any parse failure.
pub fn parse_color_to_hsl(color: &str) -> Option<(f32, f32, f32)> {
    let color = color.trim();
    if let Some(hsl) = parse_hsl(color) {
        return Some(hsl);
    }
    let rgb = parse_hex(color)?;
    Some(rgb_to_hsl(rgb.0, rgb.1, rgb.2))
}

/// Adjust a color in HSL space by additive deltas.
///
/// `delta_h` rotates hue (degrees, wraps modulo 360), `delta_s`
/// shifts saturation, `delta_l` shifts lightness — both clamped to
/// `[0, 100]`. Returns a formatted `hsl(...)` string so the result
/// round-trips through [`parse_color_to_hsl`]. If the input is
/// unparseable, returns it unchanged.
pub fn adjust_color(color: &str, delta_h: f32, delta_s: f32, delta_l: f32) -> String {
    let Some((h, s, l)) = parse_color_to_hsl(color) else {
        return color.to_string();
    };
    let mut h = h + delta_h;
    if h < 0.0 {
        h = (h % 360.0) + 360.0;
    } else if h >= 360.0 {
        h %= 360.0;
    }
    let s = (s + delta_s).clamp(0.0, 100.0);
    let l = (l + delta_l).clamp(0.0, 100.0);
    format!("hsl({:.10}, {:.10}%, {:.10}%)", h, s, l)
}

/// Parse a `hsl(h, s%, l%)` or `hsla(h, s%, l%, a)` string into
/// `(h, s, l)` (alpha is discarded). Whitespace-tolerant, accepts
/// either `hsl` or `hsla` prefixes case-insensitively.
pub fn parse_hsl(value: &str) -> Option<(f32, f32, f32)> {
    let value = value.trim();
    let open = value.find('(')?;
    let close = value.rfind(')')?;
    let prefix = value[..open].trim().to_ascii_lowercase();
    if prefix != "hsl" && prefix != "hsla" {
        return None;
    }
    let inner = &value[open + 1..close];
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() < 3 {
        return None;
    }
    let h = parts[0].trim().parse::<f32>().ok()?;
    let s = parts[1].trim().trim_end_matches('%').parse::<f32>().ok()?;
    let l = parts[2].trim().trim_end_matches('%').parse::<f32>().ok()?;
    Some((h, s, l))
}

/// Parse a hex color (`#rgb`, `#rrggbb`, or `#rrggbbaa`) into RGB
/// components in `[0.0, 1.0]`. Alpha channel, if present, is
/// discarded. Returns `None` on any parse failure.
pub fn parse_hex(value: &str) -> Option<(f32, f32, f32)> {
    let hex = value.strip_prefix('#')?;
    let digits = match hex.len() {
        3 => {
            let mut expanded = String::new();
            for ch in hex.chars() {
                expanded.push(ch);
                expanded.push(ch);
            }
            expanded
        }
        6 => hex.to_string(),
        8 => hex[..6].to_string(),
        _ => return None,
    };
    let r = u8::from_str_radix(&digits[0..2], 16).ok()?;
    let g = u8::from_str_radix(&digits[2..4], 16).ok()?;
    let b = u8::from_str_radix(&digits[4..6], 16).ok()?;
    Some((r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0))
}

/// Convert RGB components (each in `[0.0, 1.0]`) to HSL.
/// `h` in degrees `[0, 360)`, `s` and `l` in percent `[0, 100]`.
pub fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let mut h = 0.0;
    let l = (max + min) / 2.0;
    let d = max - min;
    let s = if d == 0.0 {
        0.0
    } else {
        d / (1.0 - (2.0 * l - 1.0).abs())
    };
    if d != 0.0 {
        if max == r {
            h = ((g - b) / d) % 6.0;
        } else if max == g {
            h = (b - r) / d + 2.0;
        } else {
            h = (r - g) / d + 4.0;
        }
        h *= 60.0;
        if h < 0.0 {
            h += 360.0;
        }
    }
    (h, s * 100.0, l * 100.0)
}

/// Convert HSL (`h` in `[0, 360)`, `s`/`l` in `[0, 100]`) into linear
/// RGB components in `[0.0, 1.0]`.
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let s = (s / 100.0).clamp(0.0, 1.0);
    let l = (l / 100.0).clamp(0.0, 1.0);
    if s == 0.0 {
        return (l, l, l);
    }
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hh = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hh % 2.0 - 1.0).abs());
    let (r1, g1, b1) = if hh < 1.0 {
        (c, x, 0.0)
    } else if hh < 2.0 {
        (x, c, 0.0)
    } else if hh < 3.0 {
        (0.0, c, x)
    } else if hh < 4.0 {
        (0.0, x, c)
    } else if hh < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = l - c / 2.0;
    (r1 + m, g1 + m, b1 + m)
}

/// Format an HSL triple as a `#rrggbb` hex string. Values are
/// clamped/wrapped the same way [`hsl_to_rgb`] handles them.
pub fn hsl_to_hex(h: f32, s: f32, l: f32) -> String {
    let (r, g, b) = hsl_to_rgb(h, s, l);
    let r = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn parse_hex_long() {
        let rgb = parse_hex("#ff8040").unwrap();
        assert!(approx_eq(rgb.0, 1.0, 1e-3));
        assert!(approx_eq(rgb.1, 128.0 / 255.0, 1e-3));
        assert!(approx_eq(rgb.2, 64.0 / 255.0, 1e-3));
    }

    #[test]
    fn parse_hex_short_expands() {
        // #abc == #aabbcc
        let short = parse_hex("#abc").unwrap();
        let long = parse_hex("#aabbcc").unwrap();
        assert!(approx_eq(short.0, long.0, 1e-6));
        assert!(approx_eq(short.1, long.1, 1e-6));
        assert!(approx_eq(short.2, long.2, 1e-6));
    }

    #[test]
    fn parse_hex_rgba_discards_alpha() {
        let a = parse_hex("#112233ff").unwrap();
        let b = parse_hex("#112233").unwrap();
        assert!(approx_eq(a.0, b.0, 1e-6));
        assert!(approx_eq(a.1, b.1, 1e-6));
        assert!(approx_eq(a.2, b.2, 1e-6));
    }

    #[test]
    fn parse_hex_rejects_bad_input() {
        assert!(parse_hex("ff0000").is_none()); // missing '#'
        assert!(parse_hex("#ggg").is_none());
        assert!(parse_hex("#1234").is_none());
    }

    #[test]
    fn parse_hsl_basic() {
        let hsl = parse_hsl("hsl(120, 50%, 40%)").unwrap();
        assert!(approx_eq(hsl.0, 120.0, 1e-3));
        assert!(approx_eq(hsl.1, 50.0, 1e-3));
        assert!(approx_eq(hsl.2, 40.0, 1e-3));
    }

    #[test]
    fn parse_color_delegates() {
        assert!(parse_color_to_hsl("#ff0000").is_some());
        assert!(parse_color_to_hsl("hsl(0, 100%, 50%)").is_some());
        assert!(parse_color_to_hsl("not a color").is_none());
    }

    #[test]
    fn rgb_to_hsl_red() {
        let hsl = rgb_to_hsl(1.0, 0.0, 0.0);
        assert!(approx_eq(hsl.0, 0.0, 1e-3));
        assert!(approx_eq(hsl.1, 100.0, 1e-3));
        assert!(approx_eq(hsl.2, 50.0, 1e-3));
    }

    #[test]
    fn hsl_to_rgb_red_roundtrip() {
        let (r, g, b) = hsl_to_rgb(0.0, 100.0, 50.0);
        assert!(approx_eq(r, 1.0, 1e-3));
        assert!(approx_eq(g, 0.0, 1e-3));
        assert!(approx_eq(b, 0.0, 1e-3));
    }

    #[test]
    fn hsl_to_hex_red() {
        assert_eq!(hsl_to_hex(0.0, 100.0, 50.0), "#ff0000");
    }

    #[test]
    fn hsl_to_hex_grayscale() {
        assert_eq!(hsl_to_hex(0.0, 0.0, 100.0), "#ffffff");
        assert_eq!(hsl_to_hex(0.0, 0.0, 0.0), "#000000");
    }

    #[test]
    fn adjust_color_lighten() {
        // Lightening #000 by +50 lightness should give mid-gray.
        let out = adjust_color("#000000", 0.0, 0.0, 50.0);
        let (_, _, l) = parse_color_to_hsl(&out).unwrap();
        assert!(approx_eq(l, 50.0, 1e-3));
    }

    #[test]
    fn adjust_color_hue_wrap() {
        // Rotate red (h=0) by +400 -> same as +40.
        let out = adjust_color("#ff0000", 400.0, 0.0, 0.0);
        let (h, _, _) = parse_color_to_hsl(&out).unwrap();
        assert!(approx_eq(h, 40.0, 1e-3));
    }

    #[test]
    fn adjust_color_passthrough_on_bad_input() {
        assert_eq!(adjust_color("garbage", 0.0, 0.0, 0.0), "garbage");
    }
}
