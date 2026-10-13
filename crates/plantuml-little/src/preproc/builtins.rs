//! Builtin function helpers (color, date, theme/stdlib listing).

use std::fs;
use std::path::Path;

pub(super) fn parse_int_like(value: &str) -> i64 {
    let trimmed = value.trim();
    let mut out = String::new();
    let mut started = false;
    for ch in trimmed.chars() {
        if !started && (ch == '+' || ch == '-' || ch.is_ascii_digit()) {
            out.push(ch);
            started = true;
        } else if started && ch.is_ascii_digit() {
            out.push(ch);
        } else if started {
            break;
        }
    }
    out.parse::<i64>().unwrap_or(0)
}

pub(super) fn parse_hex_like(value: &str) -> i64 {
    let trimmed = value.trim();
    let trimmed = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .or_else(|| trimmed.strip_prefix('#'))
        .unwrap_or(trimmed);
    i64::from_str_radix(trimmed, 16).unwrap_or(0)
}

pub(super) fn format_string_array(values: &[String]) -> String {
    let parts: Vec<String> = values
        .iter()
        .map(|value| format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    format!("[{}]", parts.join(", "))
}

/// Parse a hex color string (with or without `#` prefix) into RGB.
pub(super) fn parse_color_hex(color: &str) -> Option<(u8, u8, u8)> {
    let hex = color.trim().trim_start_matches('#');
    // `len()` counts bytes, so a non-ASCII value can reach a 3- or 6-byte
    // branch and be sliced mid-character. Only hex digits can parse anyway.
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        None
    } else if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some((r, g, b))
    } else if hex.len() == 3 {
        let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
        let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
        let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
        Some((r, g, b))
    } else {
        None
    }
}

/// Convert HSL (h: 0..360, s: 0..1, l: 0..1) to RGB.
pub(super) fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let h_norm = h / 360.0;
    let r = hue_to_rgb(p, q, h_norm + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h_norm);
    let b = hue_to_rgb(p, q, h_norm - 1.0 / 3.0);
    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

pub(super) fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 1.0 / 2.0 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

/// Simple pseudo-random number using system time and process id as entropy.
pub(super) fn simple_random(min: i64, max: i64) -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    // Mix with process id and thread id for additional entropy
    let pid = std::process::id() as u64;
    let mixed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(pid ^ 1442695040888963407);
    let range = if max > min { (max - min + 1) as u64 } else { 1 };
    min + (mixed % range) as i64
}

/// Format current date/time using Java-style format patterns.
pub(super) fn format_java_date(pattern: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = duration.as_secs() as i64;

    // Convert unix timestamp to date components (UTC)
    let days = total_secs / 86400;
    let time_of_day = total_secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Civil date from unix days (algorithm from Howard Hinnant)
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    // Apply Java-style format substitutions
    let mut result = pattern.to_string();
    // Handle quoted literals (e.g. 'T')
    let mut processed = String::new();
    let mut chars = result.chars().peekable();
    let mut unquoted = String::new();
    while let Some(ch) = chars.next() {
        if ch == '\'' {
            // Mark quoted chars with a placeholder
            unquoted.push('\x01');
            let mut quoted_text = String::new();
            while let Some(&next) = chars.peek() {
                if next == '\'' {
                    chars.next();
                    break;
                }
                quoted_text.push(next);
                chars.next();
            }
            processed.push_str(&quoted_text);
        } else {
            unquoted.push(ch);
            processed.push(ch);
        }
    }

    // Now substitute in `unquoted` and rebuild with the quoted literals
    result = unquoted;
    result = result.replace("yyyy", &format!("{year:04}"));
    result = result.replace("yy", &format!("{:02}", year % 100));
    result = result.replace("MM", &format!("{m:02}"));
    result = result.replace("dd", &format!("{d:02}"));
    result = result.replace("HH", &format!("{hours:02}"));
    result = result.replace("mm", &format!("{minutes:02}"));
    result = result.replace("ss", &format!("{seconds:02}"));

    // Rebuild with quoted literals
    let mut final_result = String::new();
    let mut pi = 0;
    let processed_chars: Vec<char> = processed.chars().collect();
    for ch in result.chars() {
        if ch == '\x01' {
            // Emit the quoted literal
            if pi < processed_chars.len() {
                let pc = processed_chars[pi];
                pi += 1;
                if pc != '\x01' {
                    final_result.push(pc);
                }
            }
        } else {
            final_result.push(ch);
            // Advance pi past non-placeholder chars
            if pi < processed_chars.len() {
                pi += 1;
            }
        }
    }

    if final_result.is_empty() {
        result
    } else {
        final_result
    }
}

pub(super) fn list_themes() -> Vec<String> {
    let theme_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/themes");
    let Ok(entries) = fs::read_dir(theme_root) else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Some(stripped) = name
            .strip_prefix("puml-theme-")
            .and_then(|s| s.strip_suffix(".puml"))
        {
            names.push(stripped.to_string());
        }
    }
    names.sort();
    names
}

pub(super) fn list_stdlib_names() -> Vec<String> {
    let stdlib_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    let Ok(entries) = fs::read_dir(stdlib_root) else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.is_dir() {
            names.push(name.to_string());
        } else if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            names.push(stem.to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}
