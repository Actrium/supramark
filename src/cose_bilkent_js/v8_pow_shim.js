// fdlibm-based Math.pow — verbatim port of V8 src/base/ieee754.cc::pow.
//
// Original fdlibm copyright (BSD-style, Apache-2 compatible):
// ====================================================
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
// Permission to use, copy, modify, and distribute this software is freely
// granted, provided that this notice is preserved.
// ====================================================
// Modifications by Google (V8 project authors, 2016) — same license terms.
//
// Why: QuickJS's host libm (glibc on Linux x86-64) disagrees with V8's
// fdlibm-based MathPow by up to 1 ULP on inputs like `pow(15, 1.4179...)`.
// IEEE-754 does NOT mandate correctly-rounded pow, so this divergence is
// legal. cose-bilkent's cooling schedule amplifies that 1 ULP into a >250 px
// layout delta. Patching Math.pow with V8's exact implementation collapses
// the delta to zero. Endianness: assumes little-endian host (x86-64 / aarch64
// LE) — verified at install time by an endian self-check.

(function () {
  var __buf = new ArrayBuffer(8);
  var __f64 = new Float64Array(__buf);
  var __u32 = new Uint32Array(__buf);

  // Endian self-check: on LE hosts, after f64[0]=2.0, the IEEE-754 high word
  // is 0x40000000 and the low word is 0x00000000; u32[1] holds the high.
  __f64[0] = 2.0;
  if (__u32[1] !== 0x40000000 || __u32[0] !== 0) {
    throw new Error('v8_pow_shim: unexpected endianness (need LE)');
  }

  function HI(x) { __f64[0] = x; return __u32[1]; }
  function LO(x) { __f64[0] = x; return __u32[0]; }
  // Returns the (hi, lo) words as a 2-element representation. We use the
  // same shared buffer pattern as fdlibm's union — but JS forces us to
  // re-load. To match the C macro semantics for `EXTRACT_WORDS(j, i, z)`
  // we read both fields after a single store.
  function EXTRACT(z) { __f64[0] = z; return [__u32[1] | 0, __u32[0] >>> 0]; }
  // SET_HIGH_WORD(d, v): replace the high 32 bits of d with v.
  function SET_HI(x, v) { __f64[0] = x; __u32[1] = v >>> 0; return __f64[0]; }
  // SET_LOW_WORD(d, v): replace the low 32 bits of d with v.
  function SET_LO(x, v) { __f64[0] = x; __u32[0] = v >>> 0; return __f64[0]; }
  // GET_HIGH_WORD: same as HI; kept as a separate name to mirror the C
  // source.
  function GET_HI(x) { __f64[0] = x; return __u32[1] | 0; }

  // scalbn(x, n): x * 2^n, mirroring fdlibm. cose-bilkent never reaches the
  // subnormal-output path (cooling factor never decays that far), but we
  // implement it for completeness and oracle-equality on edge cases.
  var two54 = 1.80143985094819840000e+16;     // 0x4350000000000000
  var twom54 = 5.55111512312578270212e-17;    // 0x3C90000000000000
  var hugeVal = 1.0e300;
  var tinyVal = 1.0e-300;

  function scalbn(x, n) {
    var hx = HI(x) | 0;
    var lx = LO(x) >>> 0;
    var k = (hx & 0x7ff00000) >> 20;        // extract exponent
    if (k === 0) {                           // 0 or subnormal x
      if ((lx | (hx & 0x7fffffff)) === 0) return x; // +-0
      x = x * two54;
      hx = HI(x) | 0;
      k = ((hx & 0x7ff00000) >> 20) - 54;
      if (n < -50000) return tinyVal * x;    // underflow
    }
    if (k === 0x7ff) return x + x;           // NaN or Inf
    k = k + n;
    if (k > 0x7fe) return hugeVal * copysign(hugeVal, x); // overflow
    if (k > 0) {                             // normal result
      return SET_HI(x, (hx & 0x800fffff) | (k << 20));
    }
    if (k <= -54) {
      if (n > 50000) return hugeVal * copysign(hugeVal, x); // overflow
      return tinyVal * copysign(tinyVal, x); // underflow
    }
    k += 54;                                 // subnormal result
    x = SET_HI(x, (hx & 0x800fffff) | (k << 20));
    return x * twom54;
  }

  function copysign(x, y) {
    return SET_HI(x, (HI(x) & 0x7fffffff) | (HI(y) & 0x80000000));
  }

  // Constants (verbatim from V8 ieee754.cc::pow).
  var bp0 = 1.0, bp1 = 1.5;
  var dp_h0 = 0.0, dp_h1 = 5.84962487220764160156e-01; // 0x3FE2B803,0x40000000
  var dp_l0 = 0.0, dp_l1 = 1.35003920212974897128e-08; // 0x3E4CFDEB,0x43CFD006
  var zero = 0.0, one = 1.0, two = 2.0;
  var two53 = 9007199254740992.0;
  var L1 = 5.99999999999994648725e-01;
  var L2 = 4.28571428578550184252e-01;
  var L3 = 3.33333329818377432918e-01;
  var L4 = 2.72728123808534006489e-01;
  var L5 = 2.30660745775561754067e-01;
  var L6 = 2.06975017800338417784e-01;
  var P1 = 1.66666666666666019037e-01;
  var P2 = -2.77777777770155933842e-03;
  var P3 = 6.61375632143793436117e-05;
  var P4 = -1.65339022054652515390e-06;
  var P5 = 4.13813679705723846039e-08;
  var lg2   = 6.93147180559945286227e-01;
  var lg2_h = 6.93147182464599609375e-01;
  var lg2_l = -1.90465429995776804525e-09;
  var ovt   = 8.0085662595372944372e-0017;
  var cp    = 9.61796693925975554329e-01;
  var cp_h  = 9.61796700954437255859e-01;
  var cp_l  = -7.02846165095275826516e-09;
  var ivln2   = 1.44269504088896338700e+00;
  var ivln2_h = 1.44269502162933349609e+00;
  var ivln2_l = 1.92596299112661746887e-08;

  function fdlibmPow(x, y) {
    // 1) operand normalisation — see V8 lines around `EXTRACT_WORDS`.
    var hx = HI(x) | 0, lx = LO(x) >>> 0;
    var hy = HI(y) | 0, ly = LO(y) >>> 0;
    var ix = hx & 0x7fffffff;
    var iy = hy & 0x7fffffff;

    // y == 0 -> 1.0
    if ((iy | ly) === 0) return one;

    // NaN propagation
    if (ix > 0x7ff00000 || ((ix === 0x7ff00000) && (lx !== 0)) ||
        iy > 0x7ff00000 || ((iy === 0x7ff00000) && (ly !== 0))) {
      return x + y;
    }

    // Determine if y is an odd / even integer when x < 0.
    var yisint = 0, k, j;
    if (hx < 0) {
      if (iy >= 0x43400000) {
        yisint = 2; // even integer y
      } else if (iy >= 0x3ff00000) {
        k = (iy >> 20) - 0x3ff;
        if (k > 20) {
          j = ly >>> (52 - k);
          if (((j << (52 - k)) >>> 0) === ly) yisint = 2 - (j & 1);
        } else if (ly === 0) {
          j = iy >> (20 - k);
          if (((j << (20 - k)) | 0) === iy) yisint = 2 - (j & 1);
        }
      }
    }

    // Special values of y.
    if (ly === 0) {
      if (iy === 0x7ff00000) {
        if (((ix - 0x3ff00000) | lx) === 0) return y - y; // 1**+-inf = NaN
        if (ix >= 0x3ff00000) return (hy >= 0) ? y : zero;
        return (hy < 0) ? -y : zero;
      }
      if (iy === 0x3ff00000) return (hy < 0) ? one / x : x;
      if (hy === 0x40000000) return x * x;
      if (hy === 0x3fe00000) {
        if (hx >= 0) return Math.sqrt(x); // sqrt is correctly-rounded; same in V8 and QuickJS
      }
    }

    var ax = Math.abs(x);

    // Special values of x: +-0, +-inf, +-1.
    if (lx === 0) {
      if (ix === 0x7ff00000 || ix === 0 || ix === 0x3ff00000) {
        var z = ax;
        if (hy < 0) z = one / z;
        if (hx < 0) {
          if (((ix - 0x3ff00000) | yisint) === 0) {
            z = (z - z) / (z - z); // (-1)**non-int = NaN
          } else if (yisint === 1) {
            z = -z;
          }
        }
        return z;
      }
    }

    var n = (hx >> 31) + 1;
    if ((n | yisint) === 0) return (x - x) / (x - x); // (x<0)**(non-int) = NaN

    var s = one;
    if ((n | (yisint - 1)) === 0) s = -one;

    var t, u, v, w, t1, t2, p_h, p_l, y1, r, z_h, z_l;

    // |y| huge
    if (iy > 0x41e00000) {
      if (iy > 0x43f00000) {
        if (ix <= 0x3fefffff) return (hy < 0) ? hugeVal * hugeVal : tinyVal * tinyVal;
        if (ix >= 0x3ff00000) return (hy > 0) ? hugeVal * hugeVal : tinyVal * tinyVal;
      }
      if (ix < 0x3fefffff) return (hy < 0) ? s * hugeVal * hugeVal : s * tinyVal * tinyVal;
      if (ix > 0x3ff00000) return (hy > 0) ? s * hugeVal * hugeVal : s * tinyVal * tinyVal;
      // |1-x| tiny: log(x) by Taylor
      t = ax - one;
      w = (t * t) * (0.5 - t * (0.3333333333333333333333 - t * 0.25));
      u = ivln2_h * t;
      v = t * ivln2_l - w * ivln2;
      t1 = u + v;
      t1 = SET_LO(t1, 0);
      t2 = v - (t1 - u);
    } else {
      var ss, s2, s_h, s_l, t_h, t_l;
      n = 0;
      if (ix < 0x00100000) {
        ax = ax * two53;
        n -= 53;
        ix = HI(ax) | 0;
      }
      n += ((ix) >> 20) - 0x3ff;
      j = ix & 0x000fffff;
      ix = j | 0x3ff00000;
      var kk;
      if (j <= 0x3988E) {
        kk = 0;
      } else if (j < 0xBB67A) {
        kk = 1;
      } else {
        kk = 0;
        n += 1;
        ix -= 0x00100000;
      }
      ax = SET_HI(ax, ix);
      var bpk = (kk === 0) ? bp0 : bp1;
      var dp_hk = (kk === 0) ? dp_h0 : dp_h1;
      var dp_lk = (kk === 0) ? dp_l0 : dp_l1;

      u = ax - bpk;
      v = one / (ax + bpk);
      ss = u * v;
      s_h = ss;
      s_h = SET_LO(s_h, 0);
      t_h = zero;
      t_h = SET_HI(t_h, ((ix >> 1) | 0x20000000) + 0x00080000 + (kk << 18));
      t_l = ax - (t_h - bpk);
      s_l = v * ((u - s_h * t_h) - s_h * t_l);

      s2 = ss * ss;
      r = s2 * s2 *
          (L1 + s2 * (L2 + s2 * (L3 + s2 * (L4 + s2 * (L5 + s2 * L6)))));
      r += s_l * (s_h + ss);
      s2 = s_h * s_h;
      t_h = 3.0 + s2 + r;
      t_h = SET_LO(t_h, 0);
      t_l = r - ((t_h - 3.0) - s2);

      u = s_h * t_h;
      v = s_l * t_h + t_l * ss;

      p_h = u + v;
      p_h = SET_LO(p_h, 0);
      p_l = v - (p_h - u);
      z_h = cp_h * p_h;
      z_l = cp_l * p_h + p_l * cp + dp_lk;

      t = n;
      t1 = (((z_h + z_l) + dp_hk) + t);
      t1 = SET_LO(t1, 0);
      t2 = z_l - (((t1 - t) - dp_hk) - z_h);
    }

    // (y1 + y2) * (t1 + t2)
    y1 = y;
    y1 = SET_LO(y1, 0);
    p_l = (y - y1) * t1 + y * t2;
    p_h = y1 * t1;
    var zfinal = p_l + p_h;
    var hl = EXTRACT(zfinal);
    var jh = hl[0], il = hl[1];
    if (jh >= 0x40900000) {
      if (((jh - 0x40900000) | il) !== 0) return s * hugeVal * hugeVal;
      if (p_l + ovt > zfinal - p_h) return s * hugeVal * hugeVal;
    } else if ((jh & 0x7fffffff) >= 0x4090cc00) {
      if (((jh - 0xc090cc00) | il) !== 0) return s * tinyVal * tinyVal;
      if (p_l <= zfinal - p_h) return s * tinyVal * tinyVal;
    }

    // 2^(p_h + p_l)
    var ifinal = jh & 0x7fffffff;
    k = (ifinal >> 20) - 0x3ff;
    var nfinal = 0;
    if (ifinal > 0x3fe00000) {
      nfinal = jh + (0x00100000 >> (k + 1));
      k = ((nfinal & 0x7fffffff) >> 20) - 0x3ff;
      t = zero;
      t = SET_HI(t, nfinal & ~(0x000fffff >> k));
      nfinal = ((nfinal & 0x000fffff) | 0x00100000) >> (20 - k);
      if (jh < 0) nfinal = -nfinal;
      p_h -= t;
    }
    t = p_l + p_h;
    t = SET_LO(t, 0);
    u = t * lg2_h;
    v = (p_l - (t - p_h)) * lg2 + t * lg2_l;
    zfinal = u + v;
    w = v - (zfinal - u);
    t = zfinal * zfinal;
    t1 = zfinal - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    r = (zfinal * t1) / ((t1 - two) - (w + zfinal * w));
    zfinal = one - (r - zfinal);
    var jh2 = GET_HI(zfinal);
    // Add (n << 20) as a 32-bit signed-to-the-exponent adjustment.
    // V8: `j += static_cast<int>(static_cast<uint32_t>(n) << 20);`
    jh2 = (jh2 + (((nfinal >>> 0) << 20) >>> 0)) | 0;
    if ((jh2 >> 20) <= 0) {
      zfinal = scalbn(zfinal, nfinal);
    } else {
      var tmp = GET_HI(zfinal);
      zfinal = SET_HI(zfinal, (tmp + (((nfinal >>> 0) << 20) >>> 0)) | 0);
    }
    return s * zfinal;
  }

  // Replace Math.pow. Length is 2 — JS's Math.pow accepts (base, exp).
  globalThis.Math.pow = fdlibmPow;
})();
