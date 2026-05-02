//! Khroma-compatible color math — port of the small subset of
//! `khroma@2.1.0` used by mermaid's `theme-*.js` `updateColors()`.
//!
//! What lives here:
//!
//! * Parse `#rgb` / `#rrggbb` / `#rrggbbaa` / `rgb(...)` / `rgba(...)` /
//!   `hsl(...)` / `hsla(...)` into a [`Channels`] container.
//! * `adjust(color, deltas)` — the workhorse used by mermaid to derive
//!   `secondaryColor`, `tertiaryColor`, the `cScale*` palette etc.
//! * `lighten(color, n)` / `darken(color, n)` / `mkBorder(col, dark)` —
//!   thin wrappers around `adjust_channel(... 'l' ...)`.
//!
//! Output stringification mirrors khroma exactly:
//!
//! * If the result kept type `RGB` and `r/g/b` are all integers and
//!   alpha is 1, emit `#rrggbb`.
//! * Otherwise if type is `RGB`, emit `rgb(r, g, b)` / `rgba(r, g, b, a)`
//!   with the JS-style 10-digit `Math.round` shortener.
//! * If type is `HSL`, emit `hsl(h, s%, l%)` / `hsla(...)` likewise.
//!
//! Adapted from mermaid-rs-renderer (https://github.com/1jehuang/mermaid-rs-renderer),
//! MIT license. Replaces upstream mermaid's `khroma` JS dependency.

/// Tiny subset of `khroma`'s `Channels` enum — tracks which space the
/// last write touched so [`stringify`] can pick the right printer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    Rgb,
    Hsl,
}

/// Parsed-and-mutable color, mirroring khroma's `Channels` class.
///
/// Only one of `(r, g, b)` or `(h, s, l)` is guaranteed to be live at
/// any moment; the other set is computed on demand by `ensure_*`.
#[derive(Debug, Clone)]
pub struct Channels {
    /// In `[0, 255]`.
    r: f64,
    /// In `[0, 255]`.
    g: f64,
    /// In `[0, 255]`.
    b: f64,
    /// In `[0, 360)` (modulo).
    h: f64,
    /// In `[0, 100]`.
    s: f64,
    /// In `[0, 100]`.
    l: f64,
    /// In `[0, 1]`.
    a: f64,
    /// `true` once `r`, `g`, `b` are populated.
    have_rgb: bool,
    /// `true` once `h`, `s`, `l` are populated.
    have_hsl: bool,
    /// Tracks "last setter wins" — controls [`stringify`]'s pick.
    space: ColorSpace,
}

impl Channels {
    fn from_rgb(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self {
            r,
            g,
            b,
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a,
            have_rgb: true,
            have_hsl: false,
            space: ColorSpace::Rgb,
        }
    }
    fn from_hsl(h: f64, s: f64, l: f64, a: f64) -> Self {
        Self {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            h,
            s,
            l,
            a,
            have_rgb: false,
            have_hsl: true,
            space: ColorSpace::Hsl,
        }
    }

    fn ensure_hsl(&mut self) {
        if self.have_hsl {
            return;
        }
        let (h, s, l) = rgb_to_hsl_full(self.r, self.g, self.b);
        self.h = h;
        self.s = s;
        self.l = l;
        self.have_hsl = true;
    }
    fn ensure_rgb(&mut self) {
        if self.have_rgb {
            return;
        }
        let (r, g, b) = hsl_to_rgb_full(self.h, self.s, self.l);
        self.r = r;
        self.g = g;
        self.b = b;
        self.have_rgb = true;
    }

    /// Equivalent of `get h` in khroma — converts on demand.
    pub fn get_h(&mut self) -> f64 {
        if !self.have_hsl {
            self.ensure_hsl();
        }
        self.h
    }
    pub fn get_s(&mut self) -> f64 {
        if !self.have_hsl {
            self.ensure_hsl();
        }
        self.s
    }
    pub fn get_l(&mut self) -> f64 {
        if !self.have_hsl {
            self.ensure_hsl();
        }
        self.l
    }
    pub fn get_r(&mut self) -> f64 {
        if !self.have_rgb {
            self.ensure_rgb();
        }
        self.r
    }
    pub fn get_g(&mut self) -> f64 {
        if !self.have_rgb {
            self.ensure_rgb();
        }
        self.g
    }
    pub fn get_b(&mut self) -> f64 {
        if !self.have_rgb {
            self.ensure_rgb();
        }
        self.b
    }
    pub fn get_a(&self) -> f64 {
        self.a
    }

    /// Mirror of khroma's `set h(...)` — switches space to HSL,
    /// invalidates the RGB cache, and stores the clamped value.
    pub fn set_h(&mut self, v: f64) {
        if !self.have_hsl {
            self.ensure_hsl();
        }
        self.h = clamp_h(v);
        self.have_rgb = false;
        self.space = ColorSpace::Hsl;
    }
    pub fn set_s(&mut self, v: f64) {
        if !self.have_hsl {
            self.ensure_hsl();
        }
        self.s = clamp_s(v);
        self.have_rgb = false;
        self.space = ColorSpace::Hsl;
    }
    pub fn set_l(&mut self, v: f64) {
        if !self.have_hsl {
            self.ensure_hsl();
        }
        self.l = clamp_l(v);
        self.have_rgb = false;
        self.space = ColorSpace::Hsl;
    }
    pub fn set_r(&mut self, v: f64) {
        if !self.have_rgb {
            self.ensure_rgb();
        }
        self.r = clamp_rgb(v);
        self.have_hsl = false;
        self.space = ColorSpace::Rgb;
    }
    pub fn set_g(&mut self, v: f64) {
        if !self.have_rgb {
            self.ensure_rgb();
        }
        self.g = clamp_rgb(v);
        self.have_hsl = false;
        self.space = ColorSpace::Rgb;
    }
    pub fn set_b(&mut self, v: f64) {
        if !self.have_rgb {
            self.ensure_rgb();
        }
        self.b = clamp_rgb(v);
        self.have_hsl = false;
        self.space = ColorSpace::Rgb;
    }
    pub fn set_a(&mut self, v: f64) {
        self.a = clamp_a(v);
    }
}

/// Parse any color khroma understands; returns `None` if the input is
/// not recognised. Recognised formats:
///
/// * `#rgb`, `#rrggbb`, `#rrggbbaa` (case-insensitive)
/// * `rgb(r, g, b)`, `rgba(r, g, b, a)` — components decimal or `%`
/// * `hsl(h, s%, l%)`, `hsla(h, s%, l%, a)`
pub fn parse(color: &str) -> Option<Channels> {
    let s = color.trim();
    if s.is_empty() {
        return None;
    }
    // Match khroma's char-code dispatch order: hex first (`#...`),
    // then `r..` (rgb), then `h..` (hsl).
    if s.starts_with('#') {
        return parse_hex_full(s);
    }
    let head = s.as_bytes()[0].to_ascii_lowercase();
    if head == b'r' {
        return parse_rgb_full(s);
    }
    if head == b'h' {
        return parse_hsl_full(s);
    }
    None
}

fn parse_hex_full(s: &str) -> Option<Channels> {
    let hex = s.strip_prefix('#')?;
    let (r, g, b, a) = match hex.len() {
        3 => {
            let bytes = hex.as_bytes();
            let r = u8::from_str_radix(&format!("{0}{0}", bytes[0] as char), 16).ok()?;
            let g = u8::from_str_radix(&format!("{0}{0}", bytes[1] as char), 16).ok()?;
            let b = u8::from_str_radix(&format!("{0}{0}", bytes[2] as char), 16).ok()?;
            (r, g, b, 255u8)
        }
        4 => {
            let bytes = hex.as_bytes();
            let r = u8::from_str_radix(&format!("{0}{0}", bytes[0] as char), 16).ok()?;
            let g = u8::from_str_radix(&format!("{0}{0}", bytes[1] as char), 16).ok()?;
            let b = u8::from_str_radix(&format!("{0}{0}", bytes[2] as char), 16).ok()?;
            let a = u8::from_str_radix(&format!("{0}{0}", bytes[3] as char), 16).ok()?;
            (r, g, b, a)
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b, 255)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            (r, g, b, a)
        }
        _ => return None,
    };
    Some(Channels::from_rgb(
        r as f64,
        g as f64,
        b as f64,
        a as f64 / 255.0,
    ))
}

fn parse_rgb_full(s: &str) -> Option<Channels> {
    // Accept `rgb(r, g, b)` and `rgba(r, g, b, a)`. Components may
    // optionally be `%` (in which case we scale by 2.55 for r/g/b).
    let (open, close) = (s.find('(')?, s.rfind(')')?);
    if close <= open {
        return None;
    }
    let prefix = s[..open].trim().to_ascii_lowercase();
    if prefix != "rgb" && prefix != "rgba" {
        return None;
    }
    let inner = &s[open + 1..close];
    let parts: Vec<&str> = inner.split([',', '/']).collect();
    let parse_chan = |p: &str| -> Option<(f64, bool)> {
        let p = p.trim();
        if let Some(stripped) = p.strip_suffix('%') {
            stripped.trim().parse::<f64>().ok().map(|v| (v, true))
        } else {
            p.parse::<f64>().ok().map(|v| (v, false))
        }
    };
    if parts.len() < 3 {
        return None;
    }
    let (r_raw, r_pct) = parse_chan(parts[0])?;
    let (g_raw, g_pct) = parse_chan(parts[1])?;
    let (b_raw, b_pct) = parse_chan(parts[2])?;
    let r = clamp_rgb(if r_pct { r_raw * 2.55 } else { r_raw });
    let g = clamp_rgb(if g_pct { g_raw * 2.55 } else { g_raw });
    let b = clamp_rgb(if b_pct { b_raw * 2.55 } else { b_raw });
    let a = if parts.len() >= 4 {
        let (a_raw, a_pct) = parse_chan(parts[3])?;
        clamp_a(if a_pct { a_raw / 100.0 } else { a_raw })
    } else {
        1.0
    };
    Some(Channels::from_rgb(r, g, b, a))
}

fn parse_hsl_full(s: &str) -> Option<Channels> {
    let (open, close) = (s.find('(')?, s.rfind(')')?);
    if close <= open {
        return None;
    }
    let prefix = s[..open].trim().to_ascii_lowercase();
    if prefix != "hsl" && prefix != "hsla" {
        return None;
    }
    let inner = &s[open + 1..close];
    let parts: Vec<&str> = inner.split([',', '/']).collect();
    if parts.len() < 3 {
        return None;
    }
    // Hue: accept plain number or `<n>deg|grad|rad|turn`.
    let h_raw = parts[0].trim();
    let h = parse_hue_unit(h_raw)?;
    let s_val = parts[1].trim().trim_end_matches('%').parse::<f64>().ok()?;
    let l_val = parts[2].trim().trim_end_matches('%').parse::<f64>().ok()?;
    let a = if parts.len() >= 4 {
        let p = parts[3].trim();
        if let Some(stripped) = p.strip_suffix('%') {
            stripped.trim().parse::<f64>().ok().map(|v| v / 100.0)?
        } else {
            p.parse::<f64>().ok()?
        }
    } else {
        1.0
    };
    Some(Channels::from_hsl(
        clamp_h(h),
        clamp_s(s_val),
        clamp_l(l_val),
        clamp_a(a),
    ))
}

fn parse_hue_unit(raw: &str) -> Option<f64> {
    let lo = raw.to_ascii_lowercase();
    if let Some(stripped) = lo.strip_suffix("deg") {
        return stripped.trim().parse::<f64>().ok();
    }
    if let Some(stripped) = lo.strip_suffix("grad") {
        return stripped.trim().parse::<f64>().ok().map(|v| v * 0.9);
    }
    if let Some(stripped) = lo.strip_suffix("rad") {
        return stripped
            .trim()
            .parse::<f64>()
            .ok()
            .map(|v| v * 180.0 / std::f64::consts::PI);
    }
    if let Some(stripped) = lo.strip_suffix("turn") {
        return stripped.trim().parse::<f64>().ok().map(|v| v * 360.0);
    }
    raw.parse::<f64>().ok()
}

#[inline]
fn clamp_rgb(v: f64) -> f64 {
    if v >= 255.0 {
        255.0
    } else if v < 0.0 {
        0.0
    } else {
        v
    }
}
#[inline]
fn clamp_s(v: f64) -> f64 {
    if v >= 100.0 {
        100.0
    } else if v < 0.0 {
        0.0
    } else {
        v
    }
}
#[inline]
fn clamp_l(v: f64) -> f64 {
    if v >= 100.0 {
        100.0
    } else if v < 0.0 {
        0.0
    } else {
        v
    }
}
#[inline]
fn clamp_a(v: f64) -> f64 {
    if v >= 1.0 {
        1.0
    } else if v < 0.0 {
        0.0
    } else {
        v
    }
}
#[inline]
fn clamp_h(v: f64) -> f64 {
    let r = v % 360.0;
    if r < 0.0 {
        r + 360.0
    } else {
        r
    }
}

fn rgb_to_hsl_full(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    // Khroma's per-channel converter — we compute h, s, l in one shot.
    let rr = r / 255.0;
    let gg = g / 255.0;
    let bb = b / 255.0;
    let max = rr.max(gg.max(bb));
    let min = rr.min(gg.min(bb));
    let l = (max + min) / 2.0;
    if (max - min).abs() < f64::EPSILON {
        return (0.0, 0.0, l * 100.0); // achromatic
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - rr).abs() < f64::EPSILON {
        ((gg - bb) / d) + (if gg < bb { 6.0 } else { 0.0 })
    } else if (max - gg).abs() < f64::EPSILON {
        ((bb - rr) / d) + 2.0
    } else {
        ((rr - gg) / d) + 4.0
    } * 60.0;
    (h, s * 100.0, l * 100.0)
}

fn hsl_to_rgb_full(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        let v = l * 2.55;
        return (v, v, v);
    }
    let h = (h % 360.0 + 360.0) % 360.0 / 360.0;
    let s = s / 100.0;
    let l = l / 100.0;
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        (l + s) - (l * s)
    };
    let p = 2.0 * l - q;
    fn hue2rgb(p: f64, q: f64, mut t: f64) -> f64 {
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
    (
        hue2rgb(p, q, h + 1.0 / 3.0) * 255.0,
        hue2rgb(p, q, h) * 255.0,
        hue2rgb(p, q, h - 1.0 / 3.0) * 255.0,
    )
}

/// Render a number the way `String(num)` would in JavaScript after
/// `Math.round(n * 1e10) / 1e10` — i.e., 10-digit rounding with
/// trailing zeros stripped, and no trailing `.0` for integers.
fn js_round_str(n: f64) -> String {
    // Replicate `Math.round(n * 1e10) / 1e10`.
    // JS `Math.round` rounds half-towards positive infinity.
    let scaled = n * 1.0e10;
    let rounded = (scaled + 0.5).floor();
    let v = rounded / 1.0e10;
    // Special-case NaN / Infinity to mirror JS `String(NaN) == "NaN"`.
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    // Print with 10 fractional digits, then trim trailing zeros and a
    // dangling decimal point. This matches JS `String(num)` for the
    // numeric range we care about (theme HSL coords).
    let s = format!("{:.10}", v);
    if s.contains('.') {
        let trimmed = s.trim_end_matches('0').trim_end_matches('.');
        if trimmed.is_empty() || trimmed == "-" {
            "0".into()
        } else {
            trimmed.to_string()
        }
    } else {
        s
    }
}

/// Khroma's `Color.stringify` — picks `#hex` / `rgb()` / `rgba()` /
/// `hsl()` / `hsla()` based on which space was last touched.
pub fn stringify(c: &mut Channels) -> String {
    match c.space {
        ColorSpace::Hsl => {
            let h = c.get_h();
            let s = c.get_s();
            let l = c.get_l();
            let a = c.get_a();
            if a < 1.0 {
                format!(
                    "hsla({}, {}%, {}%, {})",
                    js_round_str(h),
                    js_round_str(s),
                    js_round_str(l),
                    a
                )
            } else {
                format!(
                    "hsl({}, {}%, {}%)",
                    js_round_str(h),
                    js_round_str(s),
                    js_round_str(l)
                )
            }
        }
        ColorSpace::Rgb => {
            let r = c.get_r();
            let g = c.get_g();
            let b = c.get_b();
            let a = c.get_a();
            // Khroma's stringify: if alpha < 1 OR any of r/g/b is non-integer,
            // emit `rgb(...)` / `rgba(...)`; otherwise `#hex`.
            let all_int = r.fract() == 0.0 && g.fract() == 0.0 && b.fract() == 0.0;
            if a < 1.0 {
                format!(
                    "rgba({}, {}, {}, {})",
                    js_round_str(r),
                    js_round_str(g),
                    js_round_str(b),
                    js_round_str(a)
                )
            } else if !all_int {
                format!(
                    "rgb({}, {}, {})",
                    js_round_str(r),
                    js_round_str(g),
                    js_round_str(b)
                )
            } else {
                format!("#{:02x}{:02x}{:02x}", r as u8, g as u8, b as u8)
            }
        }
    }
}

/// Khroma's `change(color, channels)` — set every requested channel
/// (clamping per-channel) and re-stringify.
fn change(color: &str, deltas: &[(char, f64)]) -> String {
    let Some(mut ch) = parse(color) else {
        return color.to_string();
    };
    for (c, v) in deltas {
        match c {
            'r' => ch.set_r(*v),
            'g' => ch.set_g(*v),
            'b' => ch.set_b(*v),
            'h' => ch.set_h(*v),
            's' => ch.set_s(*v),
            'l' => ch.set_l(*v),
            'a' => ch.set_a(*v),
            _ => {}
        }
    }
    stringify(&mut ch)
}

/// Khroma's `adjust(color, channels)` — additive deltas in HSL/RGB/A
/// space. Skips channels whose delta is `0` (mirrors `if (!channels[c]) continue`).
pub fn adjust(color: &str, deltas: &[(char, f64)]) -> String {
    let Some(mut ch) = parse(color) else {
        return color.to_string();
    };
    let mut applied: Vec<(char, f64)> = Vec::with_capacity(deltas.len());
    for (c, delta) in deltas {
        if *delta == 0.0 {
            continue;
        }
        let cur = match c {
            'r' => ch.get_r(),
            'g' => ch.get_g(),
            'b' => ch.get_b(),
            'h' => ch.get_h(),
            's' => ch.get_s(),
            'l' => ch.get_l(),
            'a' => ch.get_a(),
            _ => continue,
        };
        applied.push((*c, cur + delta));
    }
    if applied.is_empty() {
        return stringify(&mut ch);
    }
    change(color, &applied)
}

/// Convenience wrapper matching upstream's `mkBorder(col, darkMode)`.
pub fn mk_border(color: &str, dark_mode: bool) -> String {
    if dark_mode {
        adjust(color, &[('s', -40.0), ('l', 10.0)])
    } else {
        adjust(color, &[('s', -40.0), ('l', -10.0)])
    }
}

/// Khroma's `lighten(color, n)` — `+n` on the L channel.
pub fn lighten(color: &str, amount: f64) -> String {
    adjust_channel(color, 'l', amount)
}

/// Khroma's `isDark(color)` — `luminance(color) < 0.5`.
///
/// Mirrors `khroma/dist/methods/luminance.js`:
///   `luminance = 0.2126·toLinear(r) + 0.7152·toLinear(g) + 0.0722·toLinear(b)`
/// where `toLinear(c)` is sRGB→linear gamma decoding (channels in 0..255).
/// Named keywords other than `white` / `black` fall back to false (light)
/// — the only Mermaid theme background that isn't hex / rgb / hsl is
/// `default` / `forest`'s `"white"`.
pub fn is_dark(color: &str) -> bool {
    let s = color.trim();
    let mut ch = match parse(s) {
        Some(c) => c,
        None => {
            // Minimal keyword shim for the values shipped by built-in themes.
            return matches!(s.to_ascii_lowercase().as_str(), "black");
        }
    };
    let lin = |v: f64| -> f64 {
        let n = v / 255.0;
        if v > 0.03928 {
            ((n + 0.055) / 1.055).powf(2.4)
        } else {
            n / 12.92
        }
    };
    let lum = 0.2126 * lin(ch.get_r()) + 0.7152 * lin(ch.get_g()) + 0.0722 * lin(ch.get_b());
    // Khroma rounds to 10 fractional digits before the < 0.5 compare.
    let lum = (lum * 1e10).round() / 1e10;
    lum < 0.5
}

/// Khroma's `darken(color, n)` — `-n` on the L channel.
pub fn darken(color: &str, amount: f64) -> String {
    adjust_channel(color, 'l', -amount)
}

/// Khroma's `transparentize(color, amount)` — `-amount` on the A channel.
pub fn transparentize(color: &str, amount: f64) -> String {
    adjust_channel(color, 'a', -amount)
}

/// Khroma's `adjust_channel(color, channel, delta)` — additive delta on
/// a single named channel, then re-stringify.
pub fn adjust_channel(color: &str, channel: char, delta: f64) -> String {
    if delta == 0.0 {
        return color.to_string();
    }
    adjust(color, &[(channel, delta)])
}

/// Khroma's `invert(color, weight=100)` — flip the RGB channels and
/// optionally mix back. We only use weight=100 (full inversion).
pub fn invert(color: &str) -> String {
    let Some(mut ch) = parse(color) else {
        return color.to_string();
    };
    let r = 255.0 - ch.get_r();
    let g = 255.0 - ch.get_g();
    let b = 255.0 - ch.get_b();
    ch.set_r(r);
    ch.set_g(g);
    ch.set_b(b);
    stringify(&mut ch)
}

// ─── Legacy compatibility shims ─────────────────────────────────────
//
// The renderer modules (e.g. `svg_kanban.rs`) reach into this module
// for HSL parsing. Keep the old function signatures so we don't have
// to touch them.

/// Parse a `hsl(h, s%, l%)` / `hsla(h, s%, l%, a)` string, discarding
/// alpha. Returns `(h, s, l)` as `f32` for legacy callers.
pub fn parse_hsl(value: &str) -> Option<(f32, f32, f32)> {
    let ch = parse_hsl_full(value)?;
    Some((ch.h as f32, ch.s as f32, ch.l as f32))
}

/// Parse a hex color into RGB components in `[0.0, 1.0]`.
pub fn parse_hex(value: &str) -> Option<(f32, f32, f32)> {
    let ch = parse_hex_full(value)?;
    Some((
        ch.r as f32 / 255.0,
        ch.g as f32 / 255.0,
        ch.b as f32 / 255.0,
    ))
}

/// Parse any color and return its HSL coordinates.
pub fn parse_color_to_hsl(color: &str) -> Option<(f32, f32, f32)> {
    let mut ch = parse(color)?;
    Some((ch.get_h() as f32, ch.get_s() as f32, ch.get_l() as f32))
}

/// Convert RGB (`[0.0, 1.0]`) to HSL (h in `[0,360)`, s/l in `[0,100]`).
pub fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let (h, s, l) = rgb_to_hsl_full(r as f64 * 255.0, g as f64 * 255.0, b as f64 * 255.0);
    (h as f32, s as f32, l as f32)
}

/// Convert HSL to linear RGB (`[0.0, 1.0]`).
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let (r, g, b) = hsl_to_rgb_full(h as f64, s as f64, l as f64);
    (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

/// Render an HSL triple as `#rrggbb`.
pub fn hsl_to_hex(h: f32, s: f32, l: f32) -> String {
    let (r, g, b) = hsl_to_rgb(h, s, l);
    let r = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// Legacy helper kept for the `state` / older callers. Adjust a color
/// in HSL space by additive deltas and emit `hsl(...)`.
pub fn adjust_color(color: &str, delta_h: f32, delta_s: f32, delta_l: f32) -> String {
    adjust(
        color,
        &[
            ('h', delta_h as f64),
            ('s', delta_s as f64),
            ('l', delta_l as f64),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn parse_hex_long() {
        let rgb = parse_hex("#ff8040").unwrap();
        assert!(approx(rgb.0, 1.0, 1e-3));
        assert!(approx(rgb.1, 128.0 / 255.0, 1e-3));
        assert!(approx(rgb.2, 64.0 / 255.0, 1e-3));
    }

    #[test]
    fn parse_hex_short_expands() {
        let short = parse_hex("#abc").unwrap();
        let long = parse_hex("#aabbcc").unwrap();
        assert!(approx(short.0, long.0, 1e-6));
        assert!(approx(short.1, long.1, 1e-6));
        assert!(approx(short.2, long.2, 1e-6));
    }

    #[test]
    fn parse_hsl_basic() {
        let hsl = parse_hsl("hsl(120, 50%, 40%)").unwrap();
        assert!(approx(hsl.0, 120.0, 1e-3));
        assert!(approx(hsl.1, 50.0, 1e-3));
        assert!(approx(hsl.2, 40.0, 1e-3));
    }

    #[test]
    fn rgb_to_hsl_red() {
        let hsl = rgb_to_hsl(1.0, 0.0, 0.0);
        assert!(approx(hsl.0, 0.0, 1e-3));
        assert!(approx(hsl.1, 100.0, 1e-3));
        assert!(approx(hsl.2, 50.0, 1e-3));
    }

    #[test]
    fn hsl_to_rgb_red_roundtrip() {
        let (r, g, b) = hsl_to_rgb(0.0, 100.0, 50.0);
        assert!(approx(r, 1.0, 1e-3));
        assert!(approx(g, 0.0, 1e-3));
        assert!(approx(b, 0.0, 1e-3));
    }

    #[test]
    fn hsl_to_hex_red() {
        assert_eq!(hsl_to_hex(0.0, 100.0, 50.0), "#ff0000");
    }

    #[test]
    fn adjust_matches_khroma_tertiary() {
        // primaryColor=#411d4e adjusted by {h: 180, l: 5} should
        // produce the exact hsl string upstream embeds in the
        // `.error-icon{fill:...}` rule of fixture 109.
        let out = adjust("#411d4e", &[('h', 180.0), ('l', 5.0)]);
        assert_eq!(out, "hsl(104.0816326531, 45.7943925234%, 25.9803921569%)");
    }

    #[test]
    fn mkborder_dark_mode_matches_khroma() {
        // tertiaryBorderColor for fixture 109:
        //   mkBorder(tertiaryColor, darkMode=true)
        //     = adjust(tertiaryColor, {s: -40, l: 10})
        // tertiaryColor = hsl(104.0816..., 45.794..., 25.980...).
        // Result must be hsl(104.0816326531, 5.7943925234%, 35.9803921569%).
        let tertiary = adjust("#411d4e", &[('h', 180.0), ('l', 5.0)]);
        let border = mk_border(&tertiary, true);
        assert_eq!(border, "hsl(104.0816326531, 5.7943925234%, 35.9803921569%)");
    }

    #[test]
    fn adjust_color_lighten_legacy() {
        // The legacy `adjust_color` used by older callers must keep
        // returning a parseable result.
        let out = adjust_color("#000000", 0.0, 0.0, 50.0);
        let (_, _, l) = parse_color_to_hsl(&out).unwrap();
        assert!(approx(l, 50.0, 1e-3));
    }

    #[test]
    fn adjust_color_passthrough_on_bad_input() {
        assert_eq!(adjust_color("garbage", 0.0, 0.0, 0.0), "garbage");
    }

    #[test]
    fn invert_white_is_black() {
        assert_eq!(invert("#ffffff"), "#000000");
    }

    #[test]
    fn lighten_red_increases_l() {
        // hsl(0, 100%, 50%) + 10 in lightness -> hsl(0, 100%, 60%).
        let out = lighten("#ff0000", 10.0);
        assert_eq!(out, "hsl(0, 100%, 60%)");
    }
}
