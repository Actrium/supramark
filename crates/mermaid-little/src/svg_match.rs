//! Tolerant SVG comparison for byte-exact-ish fixture sweeps.
//!
//! Strict byte equality breaks down once layout/physics computations
//! cross JS engines (quickjs vs V8) — the ~1-ULP drift in `Math.sqrt`
//! et al. flows into single-character coordinate differences that
//! aren't visually meaningful. This module compares two SVG strings
//! treating numeric runs as floats with a tight tolerance, while
//! still requiring exact match on every non-numeric byte.
//!
//! Specifically:
//! 1. Outside numeric tokens — byte-for-byte equality (tags, attribute
//!    names, IDs, classes, base64 alphabet, all strings).
//! 2. Inside a numeric token — both sides parsed as `f64`, compared
//!    with `|a-b| <= max(ABS_TOL, REL_TOL * max(|a|,|b|))`.
//! 3. `data-points="<base64>"` — both sides base64-decoded, then the
//!    decoded JSON strings are matched with the same algorithm
//!    (recurses, since the decoded payload is `[{"x":..,"y":..}]`).
//!
//! Tolerance is `1e-6` absolute / `1e-9` relative — far below
//! sub-pixel for typical SVG coordinates (100-1000 range), but
//! still rejects any meaningful drift.

const ABS_TOL: f64 = 1.0e-6;
const REL_TOL: f64 = 1.0e-9;

/// Compare two SVG strings tolerantly. Returns `true` when every
/// non-numeric byte matches exactly AND every numeric token agrees
/// within tolerance.
pub fn svg_match_tolerant(got: &str, expected: &str) -> bool {
    match_inner(got.as_bytes(), expected.as_bytes())
}

fn match_inner(g: &[u8], e: &[u8]) -> bool {
    let mut gi = 0;
    let mut ei = 0;
    while gi < g.len() && ei < e.len() {
        // data-points="<base64>" — decode then recurse.
        const DP: &[u8] = b"data-points=\"";
        if g[gi..].starts_with(DP) && e[ei..].starts_with(DP) {
            let g_payload_start = gi + DP.len();
            let e_payload_start = ei + DP.len();
            let g_end = match g[g_payload_start..].iter().position(|&b| b == b'"') {
                Some(p) => g_payload_start + p,
                None => return false,
            };
            let e_end = match e[e_payload_start..].iter().position(|&b| b == b'"') {
                Some(p) => e_payload_start + p,
                None => return false,
            };
            let g_b64 = &g[g_payload_start..g_end];
            let e_b64 = &e[e_payload_start..e_end];
            let inner_match = match (base64_decode(g_b64), base64_decode(e_b64)) {
                (Some(gd), Some(ed)) => match_inner(&gd, &ed),
                _ => g_b64 == e_b64,
            };
            if !inner_match {
                return false;
            }
            gi = g_end + 1; // step past closing quote
            ei = e_end + 1;
            continue;
        }

        let g_num = num_token_len(g, gi);
        let e_num = num_token_len(e, ei);
        if g_num > 0 && e_num > 0 {
            // Both at numeric token; compare as floats.
            let gs = std::str::from_utf8(&g[gi..gi + g_num]).unwrap_or("");
            let es = std::str::from_utf8(&e[ei..ei + e_num]).unwrap_or("");
            let gv: f64 = gs.parse().unwrap_or(f64::NAN);
            let ev: f64 = es.parse().unwrap_or(f64::NAN);
            if !numbers_close(gv, ev) {
                return false;
            }
            gi += g_num;
            ei += e_num;
            continue;
        }
        if g[gi] != e[ei] {
            return false;
        }
        gi += 1;
        ei += 1;
    }
    gi == g.len() && ei == e.len()
}

/// Length in bytes of the numeric token starting at `s[i]`, or 0 if
/// no number starts here. Recognises optional `-`, integer / decimal
/// part, and an optional exponent. We only treat `-` as part of the
/// number when followed by a digit or `.<digit>` AND the preceding
/// byte (if any) is not alphanumeric — that avoids capturing
/// e.g. the `-` in `actor-bottom`.
fn num_token_len(s: &[u8], i: usize) -> usize {
    if i >= s.len() {
        return 0;
    }
    let mut j = i;
    if s[j] == b'-' {
        // peek next char must start a number
        let next = match s.get(j + 1) {
            Some(&b) => b,
            None => return 0,
        };
        let next_num = next.is_ascii_digit()
            || (next == b'.' && s.get(j + 2).copied().is_some_and(|c| c.is_ascii_digit()));
        if !next_num {
            return 0;
        }
        // require preceding byte to be a non-alphanumeric (or start of string)
        if i > 0 {
            let prev = s[i - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                return 0;
            }
        }
        j += 1;
    }
    let mut had_digit = false;
    while j < s.len() && s[j].is_ascii_digit() {
        j += 1;
        had_digit = true;
    }
    if j < s.len() && s[j] == b'.' {
        j += 1;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
            had_digit = true;
        }
    }
    if !had_digit {
        return 0;
    }
    if j < s.len() && (s[j] == b'e' || s[j] == b'E') {
        let mut k = j + 1;
        if k < s.len() && (s[k] == b'+' || s[k] == b'-') {
            k += 1;
        }
        let mut exp_digit = false;
        while k < s.len() && s[k].is_ascii_digit() {
            k += 1;
            exp_digit = true;
        }
        if exp_digit {
            j = k;
        }
    }
    j - i
}

fn numbers_close(a: f64, b: f64) -> bool {
    if a.is_nan() || b.is_nan() {
        return false;
    }
    if a == b {
        return true;
    }
    let diff = (a - b).abs();
    let scale = a.abs().max(b.abs());
    diff <= ABS_TOL || diff <= REL_TOL * scale
}

/// Standard base64 decode (RFC 4648 alphabet, padded). Returns
/// `None` on any character outside the alphabet (besides `=`
/// padding) or a length not a multiple of 4.
fn base64_decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut filtered: Vec<u8> = Vec::with_capacity(input.len());
    for &b in input {
        if b == b' ' || b == b'\n' || b == b'\r' || b == b'\t' {
            continue;
        }
        filtered.push(b);
    }
    if filtered.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(filtered.len() / 4 * 3);
    for chunk in filtered.chunks(4) {
        let mut buf = [0u8; 4];
        let mut pad = 0;
        for (i, &c) in chunk.iter().enumerate() {
            buf[i] = match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' => {
                    pad += 1;
                    0
                }
                _ => return None,
            };
        }
        let n = ((buf[0] as u32) << 18)
            | ((buf[1] as u32) << 12)
            | ((buf[2] as u32) << 6)
            | (buf[3] as u32);
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_strings_match() {
        assert!(svg_match_tolerant("<g x=\"1.5\"/>", "<g x=\"1.5\"/>"));
    }

    #[test]
    fn one_ulp_drift_matches() {
        let a = "<g x=\"220.85800183423316\"/>";
        let b = "<g x=\"220.85800183423318\"/>";
        assert!(svg_match_tolerant(a, b));
    }

    #[test]
    fn meaningful_drift_fails() {
        // 0.5 is not within tolerance for values near 100.
        assert!(!svg_match_tolerant(
            "<g x=\"100.0\"/>",
            "<g x=\"100.5\"/>"
        ));
    }

    #[test]
    fn negative_numbers_match() {
        assert!(svg_match_tolerant(
            "viewBox=\"-50 -10 950 455\"",
            "viewBox=\"-50 -10 950 455\""
        ));
    }

    #[test]
    fn dash_in_class_not_treated_as_number() {
        // `actor-1` and `actor-2` differ — the `-1`/`-2` must NOT be
        // parsed as numbers, otherwise the comparison would match.
        assert!(!svg_match_tolerant(
            "class=\"actor-1\"",
            "class=\"actor-2\""
        ));
    }

    #[test]
    fn scientific_notation_matches() {
        assert!(svg_match_tolerant("opacity=\"1e-3\"", "opacity=\"0.001\""));
    }

    #[test]
    fn data_points_base64_drift_matches() {
        // Two different base64 payloads that decode to JSON arrays
        // whose numbers differ by 1 ULP — should match.
        let json_a = r#"[{"x":220.85800183423316,"y":29.7226562}]"#;
        let json_b = r#"[{"x":220.85800183423318,"y":29.7226562}]"#;
        let b64_a = encode_b64(json_a.as_bytes());
        let b64_b = encode_b64(json_b.as_bytes());
        let svg_a = format!("<path data-points=\"{}\" data-look=\"x\"/>", b64_a);
        let svg_b = format!("<path data-points=\"{}\" data-look=\"x\"/>", b64_b);
        assert!(svg_match_tolerant(&svg_a, &svg_b));
    }

    #[test]
    fn structural_differ_fails() {
        assert!(!svg_match_tolerant("<g></g>", "<rect/>"));
    }

    fn encode_b64(input: &[u8]) -> String {
        const CHARS: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        let mut i = 0;
        while i + 3 <= input.len() {
            let n = ((input[i] as u32) << 16)
                | ((input[i + 1] as u32) << 8)
                | (input[i + 2] as u32);
            out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
            out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
            out.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
            out.push(CHARS[(n & 0x3f) as usize] as char);
            i += 3;
        }
        let rem = input.len() - i;
        if rem == 1 {
            let n = (input[i] as u32) << 16;
            out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
            out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        } else if rem == 2 {
            let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
            out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
            out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
            out.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        out
    }
}
