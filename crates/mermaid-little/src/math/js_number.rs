//! `Number.prototype.toString()`-equivalent formatting.
//!
//! Rust's default `Display` for `f64` uses Ryu, which emits the
//! shortest decimal that uniquely round-trips to the input f64. When
//! two candidates of equal length both round-trip, Ryu picks the one
//! with the larger trailing digit; ECMAScript `Number.prototype.toString`
//! instead applies round-half-to-even, preferring the even trailing
//! digit. Example: `557.38873291015625` (= 0x40816b1c20000000) is
//! emitted by Rust as `"557.3887329101563"` and by JS as
//! `"557.3887329101562"` — both parse back to the same f64.
//! That mismatch surfaces in byte-exact comparisons against upstream
//! reference SVGs (cypress/xychart/35 tick label x).
//!
//! Strategy:
//! 1. Use Rust `{}` to get a baseline shortest-roundtrip string.
//! 2. If the trailing digit is odd and the even neighbour parses back
//!    to the same f64, switch to the even-trailing form.
//! 3. Integers and zero take a fast path matching JS (no `.0`).

/// Render `v` like ECMAScript `Number.prototype.toString()`.
///
/// Only the everyday range is handled (`1e-6 <= |v| < 1e21`); outside
/// that we fall back to Rust's default formatter, mirroring the
/// existing `js_num` wrappers in the per-diagram renderers.
pub fn js_number_to_string(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let abs = v.abs();
    if !(1e-6..1e21).contains(&abs) {
        // Scientific-notation territory; the tie ambiguity is unlikely
        // to bite there, so defer to Rust's default for now.
        return format!("{}", v);
    }
    // Integers: no decimal, matching JS's `Number.toString` output.
    if v.fract() == 0.0 && v.is_finite() {
        return format!("{}", v as i64);
    }
    let raw = format!("{}", v);
    // The Ryu / round-half-to-even disagreement only ever affects the
    // final significant digit, and only when v sits at the exact
    // midpoint between two adjacent K-digit decimals (i.e. a true
    // tie). For non-tie values both algorithms pick the same closest
    // decimal, so we must NOT swap there — that would corrupt outputs
    // like `431.69931640625003` (xychart/01) where v really is closer
    // to ...03 than ...02.
    if raw.contains('.') {
        let bytes = raw.as_bytes();
        let last = bytes[bytes.len() - 1];
        if last.is_ascii_digit() && (last - b'0') % 2 == 1 {
            let n = raw.len();
            // Even-trailing candidate below.
            let mut even_below = raw.clone();
            // SAFETY: ASCII byte replaced with ASCII byte.
            unsafe {
                even_below.as_bytes_mut()[n - 1] = last - 1;
            }
            if let Ok(parsed_even) = even_below.parse::<f64>() {
                if parsed_even.to_bits() == v.to_bits() && is_exact_tie(v, &even_below) {
                    return even_below;
                }
            }
        }
    }
    raw
}

/// Returns true iff the exact rational value of `v` equals the exact
/// rational value of `even_below_str + "5"` (the midpoint between
/// `even_below_str` and the K-digit decimal one ULP higher).
///
/// We can't just `parse(midpoint).to_bits() == v.to_bits()` because the
/// midpoint may round to v even when v isn't *exactly* at the midpoint
/// (e.g. xychart/01's `431.69931640625003` is closer to ...03 than to
/// the midpoint, but the midpoint string still parses back to it).
/// So we compare exact rationals via integer arithmetic.
fn is_exact_tie(v: f64, even_below_str: &str) -> bool {
    // Decode v as an exact rational: sig * 2^pow.
    let bits = v.to_bits();
    let raw_exp = ((bits >> 52) & 0x7ff) as i32;
    let raw_mant = bits & ((1u64 << 52) - 1);
    if raw_exp == 0 || raw_exp == 0x7ff {
        // Subnormals / inf / NaN — out of scope here.
        return false;
    }
    let sig: u128 = (1u128 << 52) | raw_mant as u128;
    let pow = raw_exp - 1023 - 52; // v = sig * 2^pow

    // Decode midpoint string `even_below_str + "5"` as m / 10^(k+1),
    // where k is the number of digits after the decimal point in
    // `even_below_str`. The midpoint inherits abs(int_part) and prepends
    // sign separately.
    let s = even_below_str.trim_start_matches('-');
    let dot = match s.find('.') {
        Some(i) => i,
        None => return false,
    };
    let int_part = &s[..dot];
    let frac_part = &s[dot + 1..];
    // Build the midpoint integer: digits = int_part + frac_part + "5".
    let mut digits = String::with_capacity(int_part.len() + frac_part.len() + 1);
    digits.push_str(int_part);
    digits.push_str(frac_part);
    digits.push('5');
    let m: u128 = match digits.parse() {
        Ok(n) => n,
        Err(_) => return false,
    };
    let k = (frac_part.len() as i32) + 1; // # decimal places in midpoint

    // v == m / 10^k  ⇔  sig * 2^pow * 10^k == m
    //              ⇔  sig * 2^(pow + k) * 5^k == m
    // Split by sign of (pow + k):
    let pk = pow + k;
    if pk >= 0 {
        // LHS = sig * 2^pk * 5^k.  Bail on overflow risk.
        let pow5 = match checked_pow_u128(5, k as u32) {
            Some(p) => p,
            None => return false,
        };
        let pow2 = match checked_pow_u128(2, pk as u32) {
            Some(p) => p,
            None => return false,
        };
        let lhs = sig.checked_mul(pow2).and_then(|v| v.checked_mul(pow5));
        match lhs {
            Some(l) => l == m,
            None => false,
        }
    } else {
        // sig * 5^k == m * 2^(-pk)
        let pow5 = match checked_pow_u128(5, k as u32) {
            Some(p) => p,
            None => return false,
        };
        let pow2 = match checked_pow_u128(2, (-pk) as u32) {
            Some(p) => p,
            None => return false,
        };
        let lhs = sig.checked_mul(pow5);
        let rhs = m.checked_mul(pow2);
        match (lhs, rhs) {
            (Some(l), Some(r)) => l == r,
            _ => false,
        }
    }
}

fn checked_pow_u128(base: u128, exp: u32) -> Option<u128> {
    let mut acc: u128 = 1;
    for _ in 0..exp {
        acc = acc.checked_mul(base)?;
    }
    Some(acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xychart_35_tick_position() {
        // From cypress/xychart/35: Rust `{}` gives "557.3887329101563";
        // JS Number.toString gives "557.3887329101562". Both parse back
        // to the same f64. We must produce the even-trailing form.
        let v: f64 = 557.38873291015625;
        assert_eq!(js_number_to_string(v), "557.3887329101562");
    }

    #[test]
    fn integer_no_decimal() {
        assert_eq!(js_number_to_string(42.0), "42");
        assert_eq!(js_number_to_string(-7.0), "-7");
        assert_eq!(js_number_to_string(0.0), "0");
        assert_eq!(js_number_to_string(-0.0), "0");
    }

    #[test]
    fn even_last_digit_unchanged() {
        // Last digit already even: leave alone.
        assert_eq!(js_number_to_string(1.5), "1.5");
        assert_eq!(js_number_to_string(0.125), "0.125");
    }

    #[test]
    fn odd_last_digit_no_alt_keeps() {
        // Odd last digit but no even neighbour round-trips: keep raw.
        assert_eq!(js_number_to_string(1.3), "1.3");
        assert_eq!(js_number_to_string(0.1), "0.1");
        assert_eq!(js_number_to_string(0.7), "0.7");
    }
}
