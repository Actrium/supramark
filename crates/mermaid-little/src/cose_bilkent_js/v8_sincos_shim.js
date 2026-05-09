// fdlibm-based Math.sin and Math.cos — verbatim port of V8 src/base/ieee754.cc
// (sin, cos, __kernel_sin, __kernel_cos, __ieee754_rem_pio2, __kernel_rem_pio2).
//
// Original fdlibm copyright (BSD-style, Apache-2 compatible):
// ====================================================
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
// Permission to use, copy, modify, and distribute this software is freely
// granted, provided that this notice is preserved.
// ====================================================
// Modifications by Google (V8 project authors, 2016) — same license terms.
//
// Why: QuickJS's host libm sin/cos disagree with V8's fdlibm-based versions
// by ~1 ULP on inputs like cos(0.1), sin(2.5), sin(18.0). cose-bilkent
// uses Math.sin(seed++) for its RandomSeed RNG; even a single 1 ULP drift in
// the initial scatter cascades into 1 ULP shifts in 4 of 16 final node
// positions for fixture cypress/mindmap/23. Patching to V8's fdlibm sin/cos
// closes the residual byte-exactness gap.
//
// Endianness: assumes little-endian (x86-64 / aarch64 LE) — verified at
// install time by the pow shim's endian self-check, which we rely on
// having run before this file.

(function () {
  var __buf = new ArrayBuffer(8);
  var __f64 = new Float64Array(__buf);
  var __u32 = new Uint32Array(__buf);

  function HI(x) { __f64[0] = x; return __u32[1] | 0; }
  function LO(x) { __f64[0] = x; return __u32[0] >>> 0; }
  function SET_HI(x, v) { __f64[0] = x; __u32[1] = v >>> 0; return __f64[0]; }
  function SET_LO(x, v) { __f64[0] = x; __u32[0] = v >>> 0; return __f64[0]; }
  function INSERT(hi, lo) { __u32[1] = hi >>> 0; __u32[0] = lo >>> 0; return __f64[0]; }

  // scalbn(x, n): x * 2^n, fdlibm port. cos/sin only call scalbn from
  // __kernel_rem_pio2 with one and integer-q0 arguments, so the simple
  // path is sufficient — but we keep the full algorithm.
  var two54 = 1.80143985094819840000e+16;
  var twom54 = 5.55111512312578270212e-17;
  var hugeVal = 1.0e300;
  var tinyVal = 1.0e-300;
  function copysign(x, y) {
    return SET_HI(x, (HI(x) & 0x7fffffff) | (HI(y) & 0x80000000));
  }
  function scalbn(x, n) {
    var hx = HI(x), lx = LO(x);
    var k = (hx & 0x7ff00000) >> 20;
    if (k === 0) {
      if ((lx | (hx & 0x7fffffff)) === 0) return x;
      x = x * two54;
      hx = HI(x);
      k = ((hx & 0x7ff00000) >> 20) - 54;
      if (n < -50000) return tinyVal * x;
    }
    if (k === 0x7ff) return x + x;
    k = k + n;
    if (k > 0x7fe) return hugeVal * copysign(hugeVal, x);
    if (k > 0) return SET_HI(x, (hx & 0x800fffff) | (k << 20));
    if (k <= -54) {
      if (n > 50000) return hugeVal * copysign(hugeVal, x);
      return tinyVal * copysign(tinyVal, x);
    }
    k += 54;
    x = SET_HI(x, (hx & 0x800fffff) | (k << 20));
    return x * twom54;
  }

  // Constants used by both kernels.
  var __KS_half = 0.5;
  var __KS_S1 = -1.66666666666666324348e-01;
  var __KS_S2 = 8.33333333332248946124e-03;
  var __KS_S3 = -1.98412698298579493134e-04;
  var __KS_S4 = 2.75573137070700676789e-06;
  var __KS_S5 = -2.50507602534068634195e-08;
  var __KS_S6 = 1.58969099521155010221e-10;
  var __KC_one = 1.0;
  var __KC_C1 = 4.16666666666666019037e-02;
  var __KC_C2 = -1.38888888888741095749e-03;
  var __KC_C3 = 2.48015872894767294178e-05;
  var __KC_C4 = -2.75573143513906633035e-07;
  var __KC_C5 = 2.08757232129817482790e-09;
  var __KC_C6 = -1.13596475577881948265e-11;

  // __kernel_sin — input |x| <= pi/4. iy: 0 if y is 0 (one-arg form).
  function kernel_sin(x, y, iy) {
    var ix = HI(x) & 0x7fffffff;
    if (ix < 0x3e400000) {
      if ((x | 0) === 0) return x; // tiny x → x
    }
    var z = x * x;
    var v = z * x;
    var r = __KS_S2 + z * (__KS_S3 + z * (__KS_S4 + z * (__KS_S5 + z * __KS_S6)));
    if (iy === 0) {
      return x + v * (__KS_S1 + z * r);
    }
    return x - ((z * (__KS_half * y - v * r) - y) - v * __KS_S1);
  }

  // __kernel_cos — input |x| <= pi/4.
  function kernel_cos(x, y) {
    var ix = HI(x) & 0x7fffffff;
    if (ix < 0x3e400000) {
      if ((x | 0) === 0) return __KC_one;
    }
    var z = x * x;
    var r = z * (__KC_C1 + z * (__KC_C2 + z * (__KC_C3 + z * (__KC_C4 + z * (__KC_C5 + z * __KC_C6)))));
    if (ix < 0x3fd33333) {
      return __KC_one - (0.5 * z - (z * r - x * y));
    }
    var qx;
    if (ix > 0x3fe90000) {
      qx = 0.28125;
    } else {
      qx = INSERT(ix - 0x00200000, 0);
    }
    var iz = 0.5 * z - qx;
    var a = __KC_one - qx;
    return a - (iz - (z * r - x * y));
  }

  // 396 hex digits of 2/pi from fdlibm — same data as V8's two_over_pi[].
  var two_over_pi = [
    0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62, 0x95993C,
    0x439041, 0xFE5163, 0xABDEBB, 0xC561B7, 0x246E3A, 0x424DD2, 0xE00649,
    0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129, 0xA73EE8, 0x8235F5, 0x2EBB44,
    0x84E99C, 0x7026B4, 0x5F7E41, 0x3991D6, 0x398353, 0x39F49C, 0x845F8B,
    0xBDF928, 0x3B1FF8, 0x97FFDE, 0x05980F, 0xEF2F11, 0x8B5A0A, 0x6D1F6D,
    0x367ECF, 0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5,
    0xF17B3D, 0x0739F7, 0x8A5292, 0xEA6BFB, 0x5FB11F, 0x8D5D08, 0x560330,
    0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3, 0x91615E, 0xE61B08,
    0x659985, 0x5F14A0, 0x68408D, 0xFFD880, 0x4D7327, 0x310606, 0x1556CA,
    0x73A8C9, 0x60E27B, 0xC08C6B
  ];
  var npio2_hw = [
    0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C,
    0x4025FDBB, 0x402921FB, 0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C,
    0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB, 0x403AB41B, 0x403C463A,
    0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x4041475C, 0x4042106C, 0x4042D97C,
    0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB,
    0x404858EB, 0x404921FB
  ];
  var __RP_zero = 0.0;
  var __RP_half = 0.5;
  var __RP_two24 = 1.67772160000000000000e+07;
  var __RP_invpio2 = 6.36619772367581382433e-01;
  var __RP_pio2_1 = 1.57079632673412561417e+00;
  var __RP_pio2_1t = 6.07710050650619224932e-11;
  var __RP_pio2_2 = 6.07710050630396597660e-11;
  var __RP_pio2_2t = 2.02226624879595063154e-21;
  var __RP_pio2_3 = 2.02226624871116645580e-21;
  var __RP_pio2_3t = 8.47842766036889956997e-32;

  var __KRP_init_jk = [2, 3, 4, 6];
  var __KRP_PIo2 = [
    1.57079625129699707031e+00,
    7.54978941586159635335e-08,
    5.39030252995776476554e-15,
    3.28200341580791294123e-22,
    1.27065575308067607349e-29,
    1.22933308981111328932e-36,
    2.73370053816464559624e-44,
    2.16741683877804819444e-51
  ];
  var __KRP_zero = 0.0;
  var __KRP_one = 1.0;
  var __KRP_two24 = 1.67772160000000000000e+07;
  var __KRP_twon24 = 5.96046447753906250000e-08;

  // __kernel_rem_pio2(x[], y[], e0, nx, prec, ipio2[])
  // Returns n & 7. Mutates y[].
  function kernel_rem_pio2(x, y, e0, nx, prec, ipio2) {
    var jz, jx, jv, jp, jk, carry, n, i, j, k, m, q0, ih;
    var iq = new Array(20);
    var f = new Array(20);
    var fq = new Array(20);
    var q = new Array(20);
    var z, fw;

    jk = __KRP_init_jk[prec];
    jp = jk;
    jx = nx - 1;
    jv = ((e0 - 3) / 24) | 0;
    if (jv < 0) jv = 0;
    q0 = e0 - 24 * (jv + 1);

    j = jv - jx;
    m = jx + jk;
    for (i = 0; i <= m; i++, j++) {
      f[i] = (j < 0) ? __KRP_zero : ipio2[j];
    }

    for (i = 0; i <= jk; i++) {
      fw = 0.0;
      for (j = 0; j <= jx; j++) fw += x[j] * f[jx + i - j];
      q[i] = fw;
    }

    jz = jk;

    // recompute label
    while (true) {
      // distill q[] into iq[] reversingly
      i = 0; j = jz; z = q[jz];
      for (; j > 0; i++, j--) {
        fw = (__KRP_twon24 * z) | 0;
        iq[i] = (z - __KRP_two24 * fw) | 0;
        z = q[j - 1] + fw;
      }

      // compute n
      z = scalbn(z, q0);
      z -= 8.0 * Math.floor(z * 0.125);
      n = z | 0;
      z -= n;
      ih = 0;
      if (q0 > 0) {
        i = (iq[jz - 1] >> (24 - q0));
        n += i;
        iq[jz - 1] -= i << (24 - q0);
        ih = iq[jz - 1] >> (23 - q0);
      } else if (q0 === 0) {
        ih = iq[jz - 1] >> 23;
      } else if (z >= 0.5) {
        ih = 2;
      }

      if (ih > 0) {
        n += 1;
        carry = 0;
        for (i = 0; i < jz; i++) {
          j = iq[i];
          if (carry === 0) {
            if (j !== 0) {
              carry = 1;
              iq[i] = 0x1000000 - j;
            }
          } else {
            iq[i] = 0xFFFFFF - j;
          }
        }
        if (q0 > 0) {
          switch (q0) {
            case 1: iq[jz - 1] &= 0x7FFFFF; break;
            case 2: iq[jz - 1] &= 0x3FFFFF; break;
          }
        }
        if (ih === 2) {
          z = __KRP_one - z;
          if (carry !== 0) z -= scalbn(__KRP_one, q0);
        }
      }

      // check if recomputation is needed
      if (z === __KRP_zero) {
        j = 0;
        for (i = jz - 1; i >= jk; i--) j |= iq[i];
        if (j === 0) {
          for (k = 1; jk >= k && iq[jk - k] === 0; k++) {}
          for (i = jz + 1; i <= jz + k; i++) {
            f[jx + i] = ipio2[jv + i];
            fw = 0.0;
            for (j = 0; j <= jx; j++) fw += x[j] * f[jx + i - j];
            q[i] = fw;
          }
          jz += k;
          continue; // recompute
        }
      }
      break;
    }

    // chop off zero terms
    if (z === 0.0) {
      jz -= 1;
      q0 -= 24;
      while (iq[jz] === 0) {
        jz--;
        q0 -= 24;
      }
    } else {
      z = scalbn(z, -q0);
      if (z >= __KRP_two24) {
        fw = (__KRP_twon24 * z) | 0;
        iq[jz] = (z - __KRP_two24 * fw) | 0;
        jz += 1;
        q0 += 24;
        iq[jz] = fw;
      } else {
        iq[jz] = z | 0;
      }
    }

    fw = scalbn(__KRP_one, q0);
    for (i = jz; i >= 0; i--) {
      q[i] = fw * iq[i];
      fw *= __KRP_twon24;
    }

    for (i = jz; i >= 0; i--) {
      fw = 0.0;
      for (k = 0; k <= jp && k <= jz - i; k++) fw += __KRP_PIo2[k] * q[i + k];
      fq[jz - i] = fw;
    }

    switch (prec) {
      case 0:
        fw = 0.0;
        for (i = jz; i >= 0; i--) fw += fq[i];
        y[0] = (ih === 0) ? fw : -fw;
        break;
      case 1:
      case 2:
        fw = 0.0;
        for (i = jz; i >= 0; i--) fw += fq[i];
        y[0] = (ih === 0) ? fw : -fw;
        fw = fq[0] - fw;
        for (i = 1; i <= jz; i++) fw += fq[i];
        y[1] = (ih === 0) ? fw : -fw;
        break;
      case 3:
        for (i = jz; i > 0; i--) {
          fw = fq[i - 1] + fq[i];
          fq[i] += fq[i - 1] - fw;
          fq[i - 1] = fw;
        }
        for (i = jz; i > 1; i--) {
          fw = fq[i - 1] + fq[i];
          fq[i] += fq[i - 1] - fw;
          fq[i - 1] = fw;
        }
        fw = 0.0;
        for (i = jz; i >= 2; i--) fw += fq[i];
        if (ih === 0) {
          y[0] = fq[0]; y[1] = fq[1]; y[2] = fw;
        } else {
          y[0] = -fq[0]; y[1] = -fq[1]; y[2] = -fw;
        }
    }
    return n & 7;
  }

  // __ieee754_rem_pio2(x, y[]) — y is 2-element output array.
  function ieee754_rem_pio2(x, y) {
    var z = 0;
    var hx = HI(x);
    var ix = hx & 0x7fffffff;
    var t, w, r, fn, e0, i, j, n, low;

    if (ix <= 0x3fe921fb) {
      y[0] = x; y[1] = 0; return 0;
    }
    if (ix < 0x4002d97c) {
      if (hx > 0) {
        z = x - __RP_pio2_1;
        if (ix !== 0x3ff921fb) {
          y[0] = z - __RP_pio2_1t;
          y[1] = (z - y[0]) - __RP_pio2_1t;
        } else {
          z -= __RP_pio2_2;
          y[0] = z - __RP_pio2_2t;
          y[1] = (z - y[0]) - __RP_pio2_2t;
        }
        return 1;
      }
      z = x + __RP_pio2_1;
      if (ix !== 0x3ff921fb) {
        y[0] = z + __RP_pio2_1t;
        y[1] = (z - y[0]) + __RP_pio2_1t;
      } else {
        z += __RP_pio2_2;
        y[0] = z + __RP_pio2_2t;
        y[1] = (z - y[0]) + __RP_pio2_2t;
      }
      return -1;
    }
    if (ix <= 0x413921fb) {
      t = Math.abs(x);
      n = (t * __RP_invpio2 + __RP_half) | 0;
      fn = n;
      r = t - fn * __RP_pio2_1;
      w = fn * __RP_pio2_1t;
      if (n < 32 && ix !== npio2_hw[n - 1]) {
        y[0] = r - w;
      } else {
        j = ix >> 20;
        y[0] = r - w;
        var high = HI(y[0]);
        i = j - ((high >> 20) & 0x7ff);
        if (i > 16) {
          t = r;
          w = fn * __RP_pio2_2;
          r = t - w;
          w = fn * __RP_pio2_2t - ((t - r) - w);
          y[0] = r - w;
          high = HI(y[0]);
          i = j - ((high >> 20) & 0x7ff);
          if (i > 49) {
            t = r;
            w = fn * __RP_pio2_3;
            r = t - w;
            w = fn * __RP_pio2_3t - ((t - r) - w);
            y[0] = r - w;
          }
        }
      }
      y[1] = (r - y[0]) - w;
      if (hx < 0) {
        y[0] = -y[0]; y[1] = -y[1]; return -n;
      }
      return n;
    }
    if (ix >= 0x7ff00000) {
      y[0] = y[1] = x - x;
      return 0;
    }
    // set z = scalbn(|x|,ilogb(x)-23)
    low = LO(x);
    z = SET_LO(z, low);
    e0 = (ix >> 20) - 1046;
    z = SET_HI(z, ix - ((e0 >>> 0) << 20));
    var tx = [0, 0, 0];
    for (i = 0; i < 2; i++) {
      tx[i] = z | 0;
      z = (z - tx[i]) * __RP_two24;
    }
    tx[2] = z;
    var nx = 3;
    while (tx[nx - 1] === __RP_zero) nx--;
    n = kernel_rem_pio2(tx, y, e0, nx, 2, two_over_pi);
    if (hx < 0) {
      y[0] = -y[0]; y[1] = -y[1]; return -n;
    }
    return n;
  }

  function fdlibmCos(x) {
    var y = [0, 0];
    var z = 0.0;
    var ix = HI(x) & 0x7fffffff;
    if (ix <= 0x3fe921fb) {
      return kernel_cos(x, z);
    }
    if (ix >= 0x7ff00000) return x - x;
    var n = ieee754_rem_pio2(x, y);
    switch (n & 3) {
      case 0: return kernel_cos(y[0], y[1]);
      case 1: return -kernel_sin(y[0], y[1], 1);
      case 2: return -kernel_cos(y[0], y[1]);
      default: return kernel_sin(y[0], y[1], 1);
    }
  }

  function fdlibmSin(x) {
    var y = [0, 0];
    var z = 0.0;
    var ix = HI(x) & 0x7fffffff;
    if (ix <= 0x3fe921fb) {
      return kernel_sin(x, z, 0);
    }
    if (ix >= 0x7ff00000) return x - x;
    var n = ieee754_rem_pio2(x, y);
    switch (n & 3) {
      case 0: return kernel_sin(y[0], y[1], 1);
      case 1: return kernel_cos(y[0], y[1]);
      case 2: return -kernel_sin(y[0], y[1], 1);
      default: return -kernel_cos(y[0], y[1]);
    }
  }

  globalThis.Math.sin = fdlibmSin;
  globalThis.Math.cos = fdlibmCos;
})();
