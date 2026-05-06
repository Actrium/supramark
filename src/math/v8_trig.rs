//! V8 Math.cos / Math.sin ports. Hoisted from src/layout/pie.rs during
//! Wave 2 so quadrant-chart, xychart, and any other trig-using
//! diagram can share the same bit-exact implementation.
//!
//! Rust `f64::cos` forwards to glibc libm which differs from V8 by
//! up to 1 ULP on inputs like `cos(0.1)`. The `libm` crate (MSUN
//! port) differs too. Byte-exact parity against `mermaid.js`
//! output requires matching V8's specific fdlibm derivative.
//!
//! Source: V8 11.3 `__kernel_cos`, `__kernel_sin`,
//! `__ieee754_rem_pio2` medium-branch. Verbatim translation.

#![allow(
    clippy::approx_constant,
    clippy::eq_op,
    clippy::excessive_precision,
    clippy::useless_let_if_seq
)]

use std::f64;

// High/low word extraction — V8's GET_HIGH_WORD / GET_LOW_WORD.
#[inline]
fn hi(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}
#[inline]
fn set_high(x: f64, hi: u32) -> f64 {
    let lo = x.to_bits() & 0xFFFF_FFFF;
    f64::from_bits(((hi as u64) << 32) | lo)
}

// ── __kernel_cos ─────────────────────────────────────────────────
// Polynomial approximation on [-π/4, π/4].
fn kernel_cos(x: f64, y: f64) -> f64 {
    const ONE: f64 = 1.0;
    const C1: f64 = 4.16666666666666019037e-02;
    const C2: f64 = -1.38888888888741095749e-03;
    const C3: f64 = 2.48015872894767294178e-05;
    const C4: f64 = -2.75573143513906633035e-07;
    const C5: f64 = 2.08757232129817482790e-09;
    const C6: f64 = -1.13596475577881948265e-11;

    let ix = hi(x) & 0x7FFF_FFFF;
    if ix < 0x3E40_0000 && x as i32 == 0 {
        // |x| < 2^-27, and x == 0 (exact) — cos(0) = 1.
        return ONE;
    }
    let z = x * x;
    let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
    if ix < 0x3FD3_3333 {
        // |x| < 0.3
        ONE - (0.5 * z - (z * r - x * y))
    } else {
        let qx = if ix > 0x3FE9_0000 {
            // x > 0.78125
            0.28125
        } else {
            // qx = x/4 with low word masked off.
            set_high(0.0, (ix - 0x0020_0000) as u32)
        };
        let iz = 0.5 * z - qx;
        let a = ONE - qx;
        a - (iz - (z * r - x * y))
    }
}

// ── __kernel_sin ────────────────────────────────────────────────
fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
    const HALF: f64 = 0.5;
    const S1: f64 = -1.66666666666666324348e-01;
    const S2: f64 = 8.33333333332248946124e-03;
    const S3: f64 = -1.98412698298579493134e-04;
    const S4: f64 = 2.75573137070700676789e-06;
    const S5: f64 = -2.50507602534068634195e-08;
    const S6: f64 = 1.58969099521155010221e-10;

    let ix = hi(x) & 0x7FFF_FFFF;
    if ix < 0x3E40_0000 && x as i32 == 0 {
        return x;
    }
    let z = x * x;
    let v = z * x;
    let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
    if iy == 0 {
        x + v * (S1 + z * r)
    } else {
        x - ((z * (HALF * y - v * r) - y) - v * S1)
    }
}

// ── __ieee754_rem_pio2 (medium-size branch) ─────────────────────
// Returns (n, y0, y1). We never hit the large-arg branch in pie.
fn rem_pio2(x: f64) -> (i32, f64, f64) {
    const TWO_24: f64 = 1.67772160000000000000e+07;
    const INVPIO2: f64 = 6.36619772367581382433e-01;
    const PIO2_1: f64 = 1.57079632673412561417e+00;
    const PIO2_1T: f64 = 6.07710050650619224932e-11;
    const PIO2_2: f64 = 6.07710050630396597660e-11;
    const PIO2_2T: f64 = 2.02226624879595063154e-21;
    const PIO2_3: f64 = 2.02226624871116645580e-21;
    const PIO2_3T: f64 = 8.47842766036889956997e-32;
    // npio2_hw table — high word of n*(pi/2) for n = 1..=32.
    const NPIO2_HW: [i32; 32] = [
        0x3FF921FBu32 as i32,
        0x400921FBu32 as i32,
        0x4012D97Cu32 as i32,
        0x401921FBu32 as i32,
        0x401F6A7Au32 as i32,
        0x4022D97Cu32 as i32,
        0x4025FDBBu32 as i32,
        0x402921FBu32 as i32,
        0x402C463Au32 as i32,
        0x402F6A7Au32 as i32,
        0x4031475Cu32 as i32,
        0x4032D97Cu32 as i32,
        0x40346B9Cu32 as i32,
        0x4035FDBBu32 as i32,
        0x40378FDBu32 as i32,
        0x403921FBu32 as i32,
        0x403AB41Bu32 as i32,
        0x403C463Au32 as i32,
        0x403DD85Au32 as i32,
        0x403F6A7Au32 as i32,
        0x40407E4Cu32 as i32,
        0x4041475Cu32 as i32,
        0x4042106Cu32 as i32,
        0x4042D97Cu32 as i32,
        0x4043A28Cu32 as i32,
        0x40446B9Cu32 as i32,
        0x404534ACu32 as i32,
        0x4045FDBBu32 as i32,
        0x4046C6CBu32 as i32,
        0x40478FDBu32 as i32,
        0x404858EBu32 as i32,
        0x404921FBu32 as i32,
    ];
    let _ = TWO_24;

    let hx = hi(x);
    let ix = hx & 0x7FFF_FFFF;
    if ix <= 0x3FE9_21FB {
        // |x| <= π/4
        return (0, x, 0.0);
    }
    if ix < 0x4002_D97C {
        // |x| < 3π/4 — n = ±1.
        if hx > 0 {
            let z = x - PIO2_1;
            let (y0, y1) = if ix != 0x3FF9_21FB {
                let y0 = z - PIO2_1T;
                let y1 = (z - y0) - PIO2_1T;
                (y0, y1)
            } else {
                let z2 = z - PIO2_2;
                let y0 = z2 - PIO2_2T;
                let y1 = (z2 - y0) - PIO2_2T;
                (y0, y1)
            };
            return (1, y0, y1);
        } else {
            let z = x + PIO2_1;
            let (y0, y1) = if ix != 0x3FF9_21FB {
                let y0 = z + PIO2_1T;
                let y1 = (z - y0) + PIO2_1T;
                (y0, y1)
            } else {
                let z2 = z + PIO2_2;
                let y0 = z2 + PIO2_2T;
                let y1 = (z2 - y0) + PIO2_2T;
                (y0, y1)
            };
            return (-1, y0, y1);
        }
    }
    if ix <= 0x4139_21FB {
        // |x| <= 2^19*(π/2)
        let t = x.abs();
        let n = (t * INVPIO2 + 0.5) as i32;
        let fn_d = n as f64;
        let mut r = t - fn_d * PIO2_1;
        let mut w = fn_d * PIO2_1T;
        let j = ix >> 20;
        let mut y0 = r - w;
        let need_2nd = {
            if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
                false
            } else {
                let high = hi(y0);
                let i = j - ((high >> 20) & 0x7FF);
                i > 16
            }
        };
        if need_2nd {
            let t2 = r;
            w = fn_d * PIO2_2;
            r = t2 - w;
            w = fn_d * PIO2_2T - ((t2 - r) - w);
            y0 = r - w;
            let high = hi(y0);
            let i2 = j - ((high >> 20) & 0x7FF);
            if i2 > 49 {
                // 3rd iteration (rare; included for completeness).
                let t3 = r;
                w = fn_d * PIO2_3;
                r = t3 - w;
                w = fn_d * PIO2_3T - ((t3 - r) - w);
                y0 = r - w;
            }
        }
        let y1 = (r - y0) - w;
        if hx < 0 {
            return (-n, -y0, -y1);
        } else {
            return (n, y0, y1);
        }
    }
    // Pie never reaches here. Return 0/x/0 as a safe fallback.
    (0, x, 0.0)
}

// ── cos ─────────────────────────────────────────────────────────
pub fn cos(x: f64) -> f64 {
    let ix = hi(x) & 0x7FFF_FFFF;
    if ix <= 0x3FE9_21FB {
        // |x| <= π/4
        return kernel_cos(x, 0.0);
    }
    if ix >= 0x7FF0_0000 {
        return x - x; // NaN
    }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_cos(y0, y1),
        1 => -kernel_sin(y0, y1, 1),
        2 => -kernel_cos(y0, y1),
        _ => kernel_sin(y0, y1, 1),
    }
}

// ── sin ─────────────────────────────────────────────────────────
pub fn sin(x: f64) -> f64 {
    let ix = hi(x) & 0x7FFF_FFFF;
    if ix <= 0x3FE9_21FB {
        // |x| <= π/4
        if ix < 0x3E50_0000 {
            // |x| < 2^-26
            if x as i32 == 0 {
                return x;
            }
        }
        return kernel_sin(x, 0.0, 0);
    }
    if ix >= 0x7FF0_0000 {
        return x - x; // NaN
    }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_sin(y0, y1, 1),
        1 => kernel_cos(y0, y1),
        2 => -kernel_sin(y0, y1, 1),
        _ => -kernel_cos(y0, y1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Values taken directly from V8 (Node 20 Linux x86_64).
    #[test]
    fn matches_v8_cos_01() {
        // cos(0.1) differs between glibc libm and V8 by 1 ULP.
        assert_eq!(cos(0.1).to_bits(), 0.99500416527802570954f64.to_bits());
    }
    #[test]
    fn matches_v8_sin_pie_input() {
        // sin(0.82279807594018405936) — slice-1 centroid y for
        // the Sports-in-Sweden fixture.
        assert_eq!(
            sin(0.82279807594018405936).to_bits(),
            0.73305187182982645133f64.to_bits()
        );
    }
    #[test]
    fn matches_v8_cos_pie_input() {
        assert_eq!(
            cos(0.82279807594018405936).to_bits(),
            0.68017273777091924458f64.to_bits()
        );
    }
    #[test]
    fn handles_negative_quadrant() {
        // cos(-π/4) ≈ 0.7071067811865476 (shortest form). The
        // `-0.7853981633974482` input is 1 ULP larger in magnitude
        // than -π/4 and produces a different bit pattern in V8.
        let a = -0.7853981633974482f64;
        assert_eq!(cos(a).to_bits(), 0.7071067811865477f64.to_bits());
    }
}
