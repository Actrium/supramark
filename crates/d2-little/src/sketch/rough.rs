//! Pure-Rust port of the subset of rough.js that d2's sketch renderer uses.
//!
//! This module ports the portion of `assets/rough.js` referenced by
//! `d2renderers/d2sketch/sketch.go` (rectangle, ellipse, circle, line, polygon,
//! path, linearPath plus solid/zigzag fill styles) into native Rust. Output
//! matches the rough.js + rquickjs pipeline byte-for-byte (after the 6-digit
//! decimal truncation the Go code applies downstream).
//!
//! Key points:
//! - PRNG: `Rng`, mirrors rough.js `class _`. Uses `i32` seed with
//!   `wrapping_mul(48271)` then `(0x7FFFFFFF & seed) / 2^31`.
//! - Number formatting: `js_num` mimics `Number.prototype.toString()`.
//! - Ops tree: `Op` enum + `OpSet`, rendered to an SVG `d` string by
//!   `opset_to_path`.
//! - Entrypoints `draw_*` return `Vec<RoughPath>`, matching what the rquickjs
//!   oracle extracts from `node.children`.
//!
//! SPDX-License-Identifier: MIT
//!
//! Derived from rough.js — Copyright (c) 2019 Preet Shihn, MIT License.
//! See `src/sketch/LICENSE` for the full MIT text. This file remains under the
//! upstream MIT license, distributed alongside the rest of the crate (MPL-2.0)
//! as permitted by MPL-2.0 §3.3 (Larger Work).

use std::cell::Cell;
use std::fmt::Write as _;

// ---------------------------------------------------------------------------
// PRNG
// ---------------------------------------------------------------------------

/// LCG: `seed = Math.imul(48271, seed)`, then `(0x7FFFFFFF & seed) / 2^31`.
///
/// Seed stays `i32`; `wrapping_mul` matches `Math.imul` behavior bit-for-bit.
#[derive(Debug)]
pub(crate) struct Rng {
    seed: Cell<i32>,
}

impl Rng {
    pub(crate) fn new(seed: i32) -> Self {
        Self {
            seed: Cell::new(seed),
        }
    }
    pub(crate) fn next(&self) -> f64 {
        // In rough.js a zero seed falls back to Math.random(); none of our
        // call sites use seed 0 but guard anyway.
        if self.seed.get() == 0 {
            return 0.0;
        }
        let s = self.seed.get().wrapping_mul(48271);
        self.seed.set(s);
        let masked = (0x7FFF_FFFF & s) as f64;
        masked / 2147483648.0_f64
    }
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// Generator options. Field names mirror rough.js for traceability.
///
/// Defaults come from rough.js `B.defaultOptions` (line 1143).
#[derive(Debug, Clone)]
pub struct Opts {
    pub max_randomness_offset: f64,
    pub roughness: f64,
    pub bowing: f64,
    pub stroke: String,
    pub stroke_width: f64,
    pub curve_tightness: f64,
    pub curve_fitting: f64,
    pub curve_step_count: f64,
    pub fill: Option<String>,
    pub fill_style: String,
    pub fill_weight: f64,
    pub hachure_angle: f64,
    pub hachure_gap: f64,
    pub dash_offset: f64,
    pub dash_gap: f64,
    pub zigzag_offset: f64,
    pub seed: i32,
    /// `roughnessGain` is dynamically adjusted per line segment in rough.js
    /// via direct property writes on the shared options object. Use `Cell`
    /// so we can update through a `&Opts` handle.
    pub(crate) roughness_gain: Cell<f64>,
}

impl Opts {
    pub fn default_base() -> Self {
        Self {
            max_randomness_offset: 2.0,
            roughness: 1.0,
            bowing: 1.0,
            stroke: "#000".into(),
            stroke_width: 1.0,
            curve_tightness: 0.0,
            curve_fitting: 0.95,
            curve_step_count: 9.0,
            fill: None,
            fill_style: "hachure".into(),
            fill_weight: -1.0,
            hachure_angle: -41.0,
            hachure_gap: -1.0,
            dash_offset: -1.0,
            dash_gap: -1.0,
            zigzag_offset: -1.0,
            seed: 0,
            roughness_gain: Cell::new(1.0),
        }
    }
}

/// Fluent builder so lib.rs can construct options without JSON string splicing.
#[derive(Debug, Clone, Default)]
pub struct OptsBuilder {
    pub stroke: Option<String>,
    pub stroke_width: Option<f64>,
    pub fill: Option<String>,
    pub fill_style: Option<String>,
    pub fill_weight: Option<f64>,
    pub hachure_gap: Option<f64>,
    pub bowing: Option<f64>,
    pub seed: Option<i32>,
    pub roughness: Option<f64>,
}

impl OptsBuilder {
    pub fn build(self) -> Opts {
        let mut o = Opts::default_base();
        if let Some(v) = self.stroke {
            o.stroke = v;
        }
        if let Some(v) = self.stroke_width {
            o.stroke_width = v;
        }
        if let Some(v) = self.fill {
            o.fill = Some(v);
        }
        if let Some(v) = self.fill_style {
            o.fill_style = v;
        }
        if let Some(v) = self.fill_weight {
            o.fill_weight = v;
        }
        if let Some(v) = self.hachure_gap {
            o.hachure_gap = v;
        }
        if let Some(v) = self.bowing {
            o.bowing = v;
        }
        if let Some(v) = self.seed {
            o.seed = v;
        }
        if let Some(v) = self.roughness {
            o.roughness = v;
        }
        o
    }
}

// ---------------------------------------------------------------------------
// Ops
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) enum Op {
    Move(f64, f64),
    BCurveTo(f64, f64, f64, f64, f64, f64),
    QCurveTo(f64, f64, f64, f64),
    LineTo(f64, f64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpSetType {
    Path,
    FillPath,
    FillSketch,
}

#[derive(Debug, Clone)]
pub(crate) struct OpSet {
    pub kind: OpSetType,
    pub ops: Vec<Op>,
}

// ---------------------------------------------------------------------------
// Jitter helpers (C / z / T in rough.js)
// ---------------------------------------------------------------------------

fn rand_offset_with_range(lo: f64, hi: f64, rng: &Rng, o: &Opts) -> f64 {
    o.roughness * o.roughness_gain.get() * (rng.next() * (hi - lo) + lo)
}

fn rand_offset(x: f64, rng: &Rng, o: &Opts) -> f64 {
    rand_offset_with_range(-x, x, rng, o)
}

// ---------------------------------------------------------------------------
// Lines (functions A / E / x / m / k)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn line_ops(
    t: f64,
    e: f64,
    s: f64,
    n: f64,
    rng: &Rng,
    opt: &Opts,
    include_start: bool,
    second: bool,
) -> Vec<Op> {
    let h = (t - s).powi(2) + (e - n).powi(2);
    let r = h.sqrt();
    let gain = if r < 200.0 {
        1.0
    } else if r > 500.0 {
        0.4
    } else {
        -0.0016668 * r + 1.233334
    };
    opt.roughness_gain.set(gain);

    let mut c = opt.max_randomness_offset;
    if c * c * 100.0 > h {
        c = r / 10.0;
    }
    let l = c / 2.0;

    let u = 0.2 + 0.2 * rng.next();
    let mut p = (opt.bowing * opt.max_randomness_offset * (n - e)) / 200.0;
    let mut d = (opt.bowing * opt.max_randomness_offset * (t - s)) / 200.0;
    p = rand_offset(p, rng, opt);
    d = rand_offset(d, rng, opt);

    let mut f: Vec<Op> = Vec::with_capacity(2);
    if include_start {
        if second {
            let a = rand_offset(l, rng, opt);
            let b = rand_offset(l, rng, opt);
            f.push(Op::Move(t + a, e + b));
        } else {
            let a = rand_offset(c, rng, opt);
            let b = rand_offset(c, rng, opt);
            f.push(Op::Move(t + a, e + b));
        }
    }

    if second {
        let j1 = rand_offset(l, rng, opt);
        let j2 = rand_offset(l, rng, opt);
        let j3 = rand_offset(l, rng, opt);
        let j4 = rand_offset(l, rng, opt);
        let j5 = rand_offset(l, rng, opt);
        let j6 = rand_offset(l, rng, opt);
        f.push(Op::BCurveTo(
            p + t + (s - t) * u + j1,
            d + e + (n - e) * u + j2,
            p + t + 2.0 * (s - t) * u + j3,
            d + e + 2.0 * (n - e) * u + j4,
            s + j5,
            n + j6,
        ));
    } else {
        let j1 = rand_offset(c, rng, opt);
        let j2 = rand_offset(c, rng, opt);
        let j3 = rand_offset(c, rng, opt);
        let j4 = rand_offset(c, rng, opt);
        let j5 = rand_offset(c, rng, opt);
        let j6 = rand_offset(c, rng, opt);
        f.push(Op::BCurveTo(
            p + t + (s - t) * u + j1,
            d + e + (n - e) * u + j2,
            p + t + 2.0 * (s - t) * u + j3,
            d + e + 2.0 * (n - e) * u + j4,
            s + j5,
            n + j6,
        ));
    }

    f
}

fn double_line_ops(x1: f64, y1: f64, x2: f64, y2: f64, rng: &Rng, o: &Opts) -> Vec<Op> {
    let mut a = line_ops(x1, y1, x2, y2, rng, o, true, false);
    let b = line_ops(x1, y1, x2, y2, rng, o, true, true);
    a.extend(b);
    a
}

/// m(points, closed, opts)
fn linear_path_ops(pts: &[(f64, f64)], closed: bool, rng: &Rng, o: &Opts) -> Vec<Op> {
    let mut ops = Vec::new();
    let n = pts.len();
    if n > 2 {
        for i in 0..n - 1 {
            ops.extend(double_line_ops(
                pts[i].0,
                pts[i].1,
                pts[i + 1].0,
                pts[i + 1].1,
                rng,
                o,
            ));
        }
        if closed {
            ops.extend(double_line_ops(
                pts[n - 1].0,
                pts[n - 1].1,
                pts[0].0,
                pts[0].1,
                rng,
                o,
            ));
        }
    } else if n == 2 {
        ops.extend(double_line_ops(
            pts[0].0, pts[0].1, pts[1].0, pts[1].1, rng, o,
        ));
    }
    ops
}

// ---------------------------------------------------------------------------
// Ellipse (P / w / D / I / R)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct EllipseParams {
    increment: f64,
    rx: f64,
    ry: f64,
}

/// P(width, height, opts) -- advances the PRNG via two rand_offset calls.
fn ellipse_params(width: f64, height: f64, rng: &Rng, o: &Opts) -> EllipseParams {
    let n = (2.0
        * std::f64::consts::PI
        * (((width / 2.0).powi(2) + (height / 2.0).powi(2)) / 2.0).sqrt())
    .sqrt();
    let i = o
        .curve_step_count
        .max(o.curve_step_count / (200.0_f64).sqrt() * n);
    let a = 2.0 * std::f64::consts::PI / i;
    let mut rx = (width / 2.0).abs();
    let mut ry = (height / 2.0).abs();
    let r = 1.0 - o.curve_fitting;
    rx += rand_offset(rx * r, rng, o);
    ry += rand_offset(ry * r, rng, o);
    EllipseParams {
        increment: a,
        rx,
        ry,
    }
}

/// `(curve_points, estimated_points)` returned by `compute_ellipse_points`.
type EllipsePoints = (Vec<(f64, f64)>, Vec<(f64, f64)>);

/// D(...) -> (curve_points, estimated_points) as per rough.js function D.
#[allow(clippy::too_many_arguments)]
fn compute_ellipse_points(
    increment: f64,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    offset_scale: f64,
    overlap: f64,
    rng: &Rng,
    o: &Opts,
) -> EllipsePoints {
    let mut curve: Vec<(f64, f64)> = Vec::new();
    let mut estimated: Vec<(f64, f64)> = Vec::new();
    let l = rand_offset(0.5, rng, o) - std::f64::consts::PI / 2.0;
    curve.push((
        rand_offset(offset_scale, rng, o) + cx + 0.9 * rx * (l - increment).cos(),
        rand_offset(offset_scale, rng, o) + cy + 0.9 * ry * (l - increment).sin(),
    ));
    let mut theta = l;
    while theta < 2.0 * std::f64::consts::PI + l - 0.01 {
        let pt = (
            rand_offset(offset_scale, rng, o) + cx + rx * theta.cos(),
            rand_offset(offset_scale, rng, o) + cy + ry * theta.sin(),
        );
        estimated.push(pt);
        curve.push(pt);
        theta += increment;
    }
    curve.push((
        rand_offset(offset_scale, rng, o)
            + cx
            + rx * (l + 2.0 * std::f64::consts::PI + 0.5 * overlap).cos(),
        rand_offset(offset_scale, rng, o)
            + cy
            + ry * (l + 2.0 * std::f64::consts::PI + 0.5 * overlap).sin(),
    ));
    curve.push((
        rand_offset(offset_scale, rng, o) + cx + 0.98 * rx * (l + overlap).cos(),
        rand_offset(offset_scale, rng, o) + cy + 0.98 * ry * (l + overlap).sin(),
    ));
    curve.push((
        rand_offset(offset_scale, rng, o) + cx + 0.9 * rx * (l + 0.5 * overlap).cos(),
        rand_offset(offset_scale, rng, o) + cy + 0.9 * ry * (l + 0.5 * overlap).sin(),
    ));
    (curve, estimated)
}

/// R(pts, end_point, opts): emit curve-tightness beziers.
fn curve_ops(pts: &[(f64, f64)], end: Option<(f64, f64)>, rng: &Rng, o: &Opts) -> Vec<Op> {
    let n = pts.len();
    let mut i: Vec<Op> = Vec::new();
    if n > 3 {
        let o_tight = 1.0 - o.curve_tightness;
        i.push(Op::Move(pts[1].0, pts[1].1));
        let mut k = 1usize;
        while k + 2 < n {
            let s = pts[k];
            let a1 = (
                s.0 + (o_tight * pts[k + 1].0 - o_tight * pts[k - 1].0) / 6.0,
                s.1 + (o_tight * pts[k + 1].1 - o_tight * pts[k - 1].1) / 6.0,
            );
            let a2 = (
                pts[k + 1].0 + (o_tight * pts[k].0 - o_tight * pts[k + 2].0) / 6.0,
                pts[k + 1].1 + (o_tight * pts[k].1 - o_tight * pts[k + 2].1) / 6.0,
            );
            let a3 = (pts[k + 1].0, pts[k + 1].1);
            i.push(Op::BCurveTo(a1.0, a1.1, a2.0, a2.1, a3.0, a3.1));
            k += 1;
        }
        if let Some(ep) = end {
            let t = o.max_randomness_offset;
            i.push(Op::LineTo(
                ep.0 + rand_offset(t, rng, o),
                ep.1 + rand_offset(t, rng, o),
            ));
        }
    } else if n == 3 {
        i.push(Op::Move(pts[1].0, pts[1].1));
        i.push(Op::BCurveTo(
            pts[1].0, pts[1].1, pts[2].0, pts[2].1, pts[2].0, pts[2].1,
        ));
    } else if n == 2 {
        i.extend(double_line_ops(
            pts[0].0, pts[0].1, pts[1].0, pts[1].1, rng, o,
        ));
    }
    i
}

/// w(cx, cy, opts, params) -> (opset, estimatedPoints)
fn ellipse_path(
    cx: f64,
    cy: f64,
    rng: &Rng,
    o: &Opts,
    params: EllipseParams,
) -> (Vec<Op>, Vec<(f64, f64)>) {
    let overlap = params.increment
        * rand_offset_with_range(0.1, rand_offset_with_range(0.4, 1.0, rng, o), rng, o);
    let (i, a) = compute_ellipse_points(
        params.increment,
        cx,
        cy,
        params.rx,
        params.ry,
        1.0,
        overlap,
        rng,
        o,
    );
    let (o_pts, _) = compute_ellipse_points(
        params.increment,
        cx,
        cy,
        params.rx,
        params.ry,
        1.5,
        0.0,
        rng,
        o,
    );
    let h = curve_ops(&i, None, rng, o);
    let r = curve_ops(&o_pts, None, rng, o);
    let mut ops = h;
    ops.extend(r);
    (ops, a)
}

// ---------------------------------------------------------------------------
// SVG path tokenizer + normalizer
// ---------------------------------------------------------------------------
//
// Ports rough.js class a (tokenize + parseData + processPoints) and class o.
// `segments` holds (key, data, point); `point` mirrors the output of
// rough.js processPoints.

#[derive(Debug, Clone)]
struct Segment {
    key: char,
    data: Vec<f64>,
    point: Option<(f64, f64)>,
}

fn param_count(key: char) -> usize {
    match key {
        'A' | 'a' => 7,
        'C' | 'c' => 6,
        'H' | 'h' => 1,
        'L' | 'l' => 2,
        'M' | 'm' => 2,
        'Q' | 'q' => 4,
        'S' | 's' => 4,
        'T' | 't' => 2,
        'V' | 'v' => 1,
        'Z' | 'z' => 0,
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokType {
    Command,
    Number,
    Eod,
}

#[derive(Debug, Clone)]
struct Token {
    ty: TokType,
    text: String,
}

fn tokenize_path(input: &str) -> Vec<Token> {
    // Match rough.js regexes:
    //   whitespace: /^([ \t\r\n,]+)/
    //   command:    /^([aAcChHlLmMqQsStTvVzZ])/
    //   number:     /^(([-+]?[0-9]+(\.[0-9]*)?|[-+]?\.[0-9]+)([eE][-+]?[0-9]+)?)/
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut out = Vec::<Token>::new();
    while i < bytes.len() {
        let b = bytes[i];
        if matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b',') {
            while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n' | b',') {
                i += 1;
            }
            continue;
        }
        if matches!(
            b,
            b'a' | b'A'
                | b'c'
                | b'C'
                | b'h'
                | b'H'
                | b'l'
                | b'L'
                | b'm'
                | b'M'
                | b'q'
                | b'Q'
                | b's'
                | b'S'
                | b't'
                | b'T'
                | b'v'
                | b'V'
                | b'z'
                | b'Z'
        ) {
            out.push(Token {
                ty: TokType::Command,
                text: (b as char).to_string(),
            });
            i += 1;
            continue;
        }
        let start = i;
        if bytes[i] == b'+' || bytes[i] == b'-' {
            i += 1;
        }
        let mut has_digit_before_dot = false;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            has_digit_before_dot = true;
        }
        let mut has_digit_after_dot = false;
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
                has_digit_after_dot = true;
            }
        }
        if !has_digit_before_dot && !has_digit_after_dot {
            // rough.js returns [] on invalid token.
            return Vec::new();
        }
        if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            let save = i;
            i += 1;
            if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                i += 1;
            }
            let exp_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if exp_start == i {
                i = save;
            }
        }
        let raw = &input[start..i];
        let parsed: f64 = raw.parse().unwrap_or(f64::NAN);
        if parsed.is_nan() {
            continue;
        }
        // Store the canonical JS toString form (rough.js does
        // `${parseFloat(text)}`).
        out.push(Token {
            ty: TokType::Number,
            text: js_num(parsed),
        });
    }
    out.push(Token {
        ty: TokType::Eod,
        text: String::new(),
    });
    out
}

fn parse_path(d_in: &str) -> Vec<Segment> {
    // Preprocessing per rough.js: replace newlines with space, collapse "-\s"
    // sequences to "-". The original regex-based chain is buggy ("/(\s\s)/g"
    // literal match), so mirror the effective behavior.
    let d = {
        let mut out = String::with_capacity(d_in.len());
        let bytes = d_in.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'\n' {
                out.push(' ');
                i += 1;
                continue;
            }
            if b == b'-'
                && i + 1 < bytes.len()
                && matches!(bytes[i + 1], b' ' | b'\t' | b'\r' | b'\n')
            {
                out.push('-');
                i += 2;
                continue;
            }
            out.push(b as char);
            i += 1;
        }
        out
    };

    let tokens = tokenize_path(&d);
    let mut segments: Vec<Segment> = Vec::new();
    let mut idx = 0usize;
    // BOD sentinel: 'X'
    let mut mode: char = 'X';
    while idx < tokens.len() && tokens[idx].ty != TokType::Eod {
        let a = &tokens[idx];
        let h: usize;
        if mode == 'X' {
            let first = a.text.chars().next().unwrap_or(' ');
            if first != 'M' && first != 'm' {
                // rough.js: parseData("M0,0" + t) -- retry with implicit move.
                let mut prefixed = String::from("M0,0");
                prefixed.push_str(&d);
                return parse_path(&prefixed);
            }
            idx += 1;
            h = param_count(first);
            mode = first;
        } else if a.ty == TokType::Number {
            h = param_count(mode);
        } else {
            idx += 1;
            let ch = a.text.chars().next().unwrap_or(' ');
            h = param_count(ch);
            mode = ch;
        }
        if idx + h > tokens.len() {
            break;
        }
        let mut data = Vec::<f64>::with_capacity(h);
        let mut ok = true;
        for tok in &tokens[idx..idx + h] {
            if tok.ty != TokType::Number {
                ok = false;
                break;
            }
            let v: f64 = tok.text.parse().unwrap_or(0.0);
            data.push(v);
        }
        if !ok {
            break;
        }
        segments.push(Segment {
            key: mode,
            data,
            point: None,
        });
        idx += h;
        // Implicit lineto after moveto.
        if mode == 'M' {
            mode = 'L';
        } else if mode == 'm' {
            mode = 'l';
        }
    }
    process_points(&mut segments);
    segments
}

fn process_points(segs: &mut [Segment]) {
    let mut first: Option<(f64, f64)> = None;
    let mut pos: (f64, f64) = (0.0, 0.0);
    for n in segs.iter_mut() {
        let d = &n.data;
        let pt = match n.key {
            'M' | 'L' | 'T' => Some((d[0], d[1])),
            'm' | 'l' | 't' => Some((d[0] + pos.0, d[1] + pos.1)),
            'H' => Some((d[0], pos.1)),
            'h' => Some((d[0] + pos.0, pos.1)),
            'V' => Some((pos.0, d[0])),
            'v' => Some((pos.0, d[0] + pos.1)),
            'z' | 'Z' => first,
            'C' => Some((d[4], d[5])),
            'c' => Some((d[4] + pos.0, d[5] + pos.1)),
            'S' | 'Q' => Some((d[2], d[3])),
            's' | 'q' => Some((d[2] + pos.0, d[3] + pos.1)),
            'A' => Some((d[5], d[6])),
            'a' => Some((d[5] + pos.0, d[6] + pos.1)),
            _ => None,
        };
        n.point = pt;
        if n.key == 'm' || n.key == 'M' {
            first = None;
        }
        if let Some(p) = pt {
            pos = p;
            if first.is_none() {
                first = Some(p);
            }
        }
        if n.key == 'z' || n.key == 'Z' {
            first = None;
        }
    }
}

// ---------------------------------------------------------------------------
// Path renderer helpers
// ---------------------------------------------------------------------------

/// PathState -- mirrors rough.js class o.
#[derive(Debug)]
struct PathState {
    position: (f64, f64),
    first: Option<(f64, f64)>,
    bezier_reflection_point: Option<(f64, f64)>,
    quad_reflection_point: Option<(f64, f64)>,
}

impl PathState {
    fn new() -> Self {
        Self {
            position: (0.0, 0.0),
            first: None,
            bezier_reflection_point: None,
            quad_reflection_point: None,
        }
    }
    fn set_position(&mut self, x: f64, y: f64) {
        self.position = (x, y);
        if self.first.is_none() {
            self.first = Some((x, y));
        }
    }
    fn x(&self) -> f64 {
        self.position.0
    }
    fn y(&self) -> f64 {
        self.position.1
    }
}

/// q(...) -- random bezier curve ops (rough.js function q).
#[allow(clippy::too_many_arguments)]
fn random_bezier(
    t: f64,
    e: f64,
    s: f64,
    n: f64,
    i: f64,
    a: f64,
    state: &mut PathState,
    rng: &Rng,
    opts: &Opts,
) -> Vec<Op> {
    let mut r: Vec<Op> = Vec::new();
    let c0 = opts.max_randomness_offset;
    let c1 = opts.max_randomness_offset + 0.5;
    let c = [c0, c1];
    let mut l = (0.0, 0.0);
    for u in 0..2 {
        if u == 0 {
            r.push(Op::Move(state.x(), state.y()));
        } else {
            r.push(Op::Move(
                state.x() + rand_offset(c[0], rng, opts),
                state.y() + rand_offset(c[0], rng, opts),
            ));
        }
        l = (
            i + rand_offset(c[u], rng, opts),
            a + rand_offset(c[u], rng, opts),
        );
        r.push(Op::BCurveTo(
            t + rand_offset(c[u], rng, opts),
            e + rand_offset(c[u], rng, opts),
            s + rand_offset(c[u], rng, opts),
            n + rand_offset(c[u], rng, opts),
            l.0,
            l.1,
        ));
    }
    state.set_position(l.0, l.1);
    r
}

// ---------------------------------------------------------------------------
// SVG arc -> cubic conversion (class h)
// ---------------------------------------------------------------------------

/// One cubic bezier segment emitted by `ArcConverter::next_segment`:
/// `(control1, control2, end_point)` — each a `(x, y)` pair.
type ArcCubicSegment = ((f64, f64), (f64, f64), (f64, f64));

#[derive(Debug)]
struct ArcConverter {
    num_segs: usize,
    seg_idx: usize,
    rx: f64,
    ry: f64,
    sin_phi: f64,
    cos_phi: f64,
    c: (f64, f64),
    theta: f64,
    delta: f64,
    t: f64,
    from: (f64, f64),
}

impl ArcConverter {
    fn new(
        from: (f64, f64),
        to: (f64, f64),
        rr: (f64, f64),
        angle_deg: f64,
        large_arc: bool,
        sweep: bool,
    ) -> Self {
        let mut out = Self {
            num_segs: 0,
            seg_idx: 0,
            rx: 0.0,
            ry: 0.0,
            sin_phi: 0.0,
            cos_phi: 0.0,
            c: (0.0, 0.0),
            theta: 0.0,
            delta: 0.0,
            t: 0.0,
            from,
        };
        if from.0 == to.0 && from.1 == to.1 {
            return out;
        }
        let o = std::f64::consts::PI / 180.0;
        out.rx = rr.0.abs();
        out.ry = rr.1.abs();
        out.sin_phi = (angle_deg * o).sin();
        out.cos_phi = (angle_deg * o).cos();
        let h = (out.cos_phi * (from.0 - to.0)) / 2.0 + (out.sin_phi * (from.1 - to.1)) / 2.0;
        let r = (-out.sin_phi * (from.0 - to.0)) / 2.0 + (out.cos_phi * (from.1 - to.1)) / 2.0;
        let l =
            out.rx * out.rx * out.ry * out.ry - out.rx * out.rx * r * r - out.ry * out.ry * h * h;
        let c = if l < 0.0 {
            let t = (1.0 - l / (out.rx * out.rx * out.ry * out.ry)).sqrt();
            out.rx *= t;
            out.ry *= t;
            0.0
        } else {
            let sign = if large_arc == sweep { -1.0 } else { 1.0 };
            sign * (l / (out.rx * out.rx * r * r + out.ry * out.ry * h * h)).sqrt()
        };
        let u = (c * out.rx * r) / out.ry;
        let p = (-c * out.ry * h) / out.rx;
        out.c = (
            out.cos_phi * u - out.sin_phi * p + (from.0 + to.0) / 2.0,
            out.sin_phi * u + out.cos_phi * p + (from.1 + to.1) / 2.0,
        );
        out.theta = vec_angle(1.0, 0.0, (h - u) / out.rx, (r - p) / out.ry);
        let mut d = vec_angle(
            (h - u) / out.rx,
            (r - p) / out.ry,
            (-h - u) / out.rx,
            (-r - p) / out.ry,
        );
        if !sweep && d > 0.0 {
            d -= 2.0 * std::f64::consts::PI;
        } else if sweep && d < 0.0 {
            d += 2.0 * std::f64::consts::PI;
        }
        out.num_segs = (d / (std::f64::consts::PI / 2.0)).abs().ceil() as usize;
        out.delta = d / out.num_segs as f64;
        out.t = ((8.0 / 3.0) * (out.delta / 4.0).sin() * (out.delta / 4.0).sin())
            / (out.delta / 2.0).sin();
        out
    }

    fn next_segment(&mut self) -> Option<ArcCubicSegment> {
        if self.seg_idx == self.num_segs {
            return None;
        }
        let t = self.theta.cos();
        let e = self.theta.sin();
        let s = self.theta + self.delta;
        let n = s.cos();
        let i = s.sin();
        let a = (
            self.cos_phi * self.rx * n - self.sin_phi * self.ry * i + self.c.0,
            self.sin_phi * self.rx * n + self.cos_phi * self.ry * i + self.c.1,
        );
        let o = (
            self.from.0 + self.t * (-self.cos_phi * self.rx * e - self.sin_phi * self.ry * t),
            self.from.1 + self.t * (-self.sin_phi * self.rx * e + self.cos_phi * self.ry * t),
        );
        let h = (
            a.0 + self.t * (self.cos_phi * self.rx * i + self.sin_phi * self.ry * n),
            a.1 + self.t * (self.sin_phi * self.rx * i - self.cos_phi * self.ry * n),
        );
        self.theta = s;
        self.from = a;
        self.seg_idx += 1;
        Some((o, h, a))
    }
}

fn vec_angle(t: f64, e: f64, s: f64, n: f64) -> f64 {
    let i = e.atan2(t);
    let a = n.atan2(s);
    if a >= i {
        a - i
    } else {
        2.0 * std::f64::consts::PI - (i - a)
    }
}

// ---------------------------------------------------------------------------
// $(state, seg, prev, opts) -> ops
// ---------------------------------------------------------------------------

fn process_segment(
    state: &mut PathState,
    seg: &Segment,
    prev: Option<&Segment>,
    rng: &Rng,
    n_opts: &Opts,
) -> Vec<Op> {
    let mut i: Vec<Op> = Vec::new();
    match seg.key {
        'M' | 'm' => {
            if seg.data.len() < 2 {
                return i;
            }
            let relative = seg.key == 'm';
            let mut a = seg.data[0];
            let mut o = seg.data[1];
            if relative {
                a += state.x();
                o += state.y();
            }
            let h = n_opts.max_randomness_offset;
            a += rand_offset(h, rng, n_opts);
            o += rand_offset(h, rng, n_opts);
            state.set_position(a, o);
            i.push(Op::Move(a, o));
        }
        'L' | 'l' => {
            if seg.data.len() < 2 {
                return i;
            }
            let relative = seg.key == 'l';
            let mut a = seg.data[0];
            let mut o = seg.data[1];
            if relative {
                a += state.x();
                o += state.y();
            }
            i.extend(double_line_ops(state.x(), state.y(), a, o, rng, n_opts));
            state.set_position(a, o);
        }
        'H' | 'h' => {
            if seg.data.is_empty() {
                return i;
            }
            let mut a = seg.data[0];
            if seg.key == 'h' {
                a += state.x();
            }
            i.extend(double_line_ops(
                state.x(),
                state.y(),
                a,
                state.y(),
                rng,
                n_opts,
            ));
            state.set_position(a, state.y());
        }
        'V' | 'v' => {
            if seg.data.is_empty() {
                return i;
            }
            let mut a = seg.data[0];
            if seg.key == 'v' {
                a += state.y();
            }
            i.extend(double_line_ops(
                state.x(),
                state.y(),
                state.x(),
                a,
                rng,
                n_opts,
            ));
            state.set_position(state.x(), a);
        }
        'Z' | 'z' => {
            if let Some(f) = state.first {
                i.extend(double_line_ops(state.x(), state.y(), f.0, f.1, rng, n_opts));
                state.set_position(f.0, f.1);
                state.first = None;
            }
        }
        'C' | 'c' => {
            if seg.data.len() < 6 {
                return i;
            }
            let rel = seg.key == 'c';
            let mut a = seg.data[0];
            let mut o = seg.data[1];
            let mut h = seg.data[2];
            let mut r = seg.data[3];
            let mut c = seg.data[4];
            let mut l = seg.data[5];
            if rel {
                a += state.x();
                h += state.x();
                c += state.x();
                o += state.y();
                r += state.y();
                l += state.y();
            }
            let u = random_bezier(a, o, h, r, c, l, state, rng, n_opts);
            i.extend(u);
            state.bezier_reflection_point = Some((c + (c - h), l + (l - r)));
        }
        'S' | 's' => {
            if seg.data.len() < 4 {
                return i;
            }
            let rel = seg.key == 's';
            let mut o = seg.data[0];
            let mut h = seg.data[1];
            let mut r = seg.data[2];
            let mut c = seg.data[3];
            if rel {
                o += state.x();
                r += state.x();
                h += state.y();
                c += state.y();
            }
            let (mut l, mut u) = (o, h);
            let p = prev.map(|s| s.key).unwrap_or(' ');
            if matches!(p, 'c' | 'C' | 's' | 'S')
                && let Some(rp) = state.bezier_reflection_point
            {
                l = rp.0;
                u = rp.1;
            }
            let f = random_bezier(l, u, o, h, r, c, state, rng, n_opts);
            i.extend(f);
            state.bezier_reflection_point = Some((r + (r - o), c + (c - h)));
        }
        'Q' | 'q' => {
            if seg.data.len() < 4 {
                return i;
            }
            let rel = seg.key == 'q';
            let mut a = seg.data[0];
            let mut o = seg.data[1];
            let mut h = seg.data[2];
            let mut r = seg.data[3];
            if rel {
                a += state.x();
                h += state.x();
                o += state.y();
                r += state.y();
            }
            let c = 1.0 * (1.0 + 0.2 * n_opts.roughness);
            let l = 1.5 * (1.0 + 0.22 * n_opts.roughness);
            i.push(Op::Move(
                state.x() + rand_offset(c, rng, n_opts),
                state.y() + rand_offset(c, rng, n_opts),
            ));
            let mut u = (
                h + rand_offset(c, rng, n_opts),
                r + rand_offset(c, rng, n_opts),
            );
            i.push(Op::QCurveTo(
                a + rand_offset(c, rng, n_opts),
                o + rand_offset(c, rng, n_opts),
                u.0,
                u.1,
            ));
            i.push(Op::Move(
                state.x() + rand_offset(l, rng, n_opts),
                state.y() + rand_offset(l, rng, n_opts),
            ));
            u = (
                h + rand_offset(l, rng, n_opts),
                r + rand_offset(l, rng, n_opts),
            );
            i.push(Op::QCurveTo(
                a + rand_offset(l, rng, n_opts),
                o + rand_offset(l, rng, n_opts),
                u.0,
                u.1,
            ));
            state.set_position(u.0, u.1);
            state.quad_reflection_point = Some((h + (h - a), r + (r - o)));
        }
        'T' | 't' => {
            if seg.data.len() < 2 {
                return i;
            }
            let rel = seg.key == 't';
            let mut o = seg.data[0];
            let mut h = seg.data[1];
            if rel {
                o += state.x();
                h += state.y();
            }
            let (mut r, mut c) = (o, h);
            let p = prev.map(|s| s.key).unwrap_or(' ');
            if matches!(p, 'q' | 'Q' | 't' | 'T')
                && let Some(qr) = state.quad_reflection_point
            {
                r = qr.0;
                c = qr.1;
            }
            let pp = 1.0 * (1.0 + 0.2 * n_opts.roughness);
            let dd = 1.5 * (1.0 + 0.22 * n_opts.roughness);
            i.push(Op::Move(
                state.x() + rand_offset(pp, rng, n_opts),
                state.y() + rand_offset(pp, rng, n_opts),
            ));
            let mut f = (
                o + rand_offset(pp, rng, n_opts),
                h + rand_offset(pp, rng, n_opts),
            );
            i.push(Op::QCurveTo(
                r + rand_offset(pp, rng, n_opts),
                c + rand_offset(pp, rng, n_opts),
                f.0,
                f.1,
            ));
            i.push(Op::Move(
                state.x() + rand_offset(dd, rng, n_opts),
                state.y() + rand_offset(dd, rng, n_opts),
            ));
            f = (
                o + rand_offset(dd, rng, n_opts),
                h + rand_offset(dd, rng, n_opts),
            );
            i.push(Op::QCurveTo(
                r + rand_offset(dd, rng, n_opts),
                c + rand_offset(dd, rng, n_opts),
                f.0,
                f.1,
            ));
            state.set_position(f.0, f.1);
            state.quad_reflection_point = Some((o + (o - r), h + (h - c)));
        }
        'A' | 'a' => {
            if seg.data.len() < 7 {
                return i;
            }
            let rel = seg.key == 'a';
            let rx = seg.data[0];
            let ry = seg.data[1];
            let angle = seg.data[2];
            let large_arc = seg.data[3];
            let sweep = seg.data[4];
            let mut tx = seg.data[5];
            let mut ty = seg.data[6];
            if rel {
                tx += state.x();
                ty += state.y();
            }
            if tx == state.x() && ty == state.y() {
                return i;
            }
            if rx == 0.0 || ry == 0.0 {
                i.extend(double_line_ops(state.x(), state.y(), tx, ty, rng, n_opts));
                state.set_position(tx, ty);
            } else {
                let mut conv = ArcConverter::new(
                    (state.x(), state.y()),
                    (tx, ty),
                    (rx, ry),
                    angle,
                    large_arc != 0.0,
                    sweep != 0.0,
                );
                while let Some((cp1, cp2, to)) = conv.next_segment() {
                    let a =
                        random_bezier(cp1.0, cp1.1, cp2.0, cp2.1, to.0, to.1, state, rng, n_opts);
                    i.extend(a);
                }
            }
        }
        _ => {}
    }
    i
}

// ---------------------------------------------------------------------------
// Solid fill (S in rough.js)
// ---------------------------------------------------------------------------

fn solid_fill_ops(pts: &[(f64, f64)], rng: &Rng, o: &Opts) -> Vec<Op> {
    let mut ops: Vec<Op> = Vec::new();
    if pts.len() > 2 {
        let n = o.max_randomness_offset;
        ops.push(Op::Move(
            pts[0].0 + rand_offset(n, rng, o),
            pts[0].1 + rand_offset(n, rng, o),
        ));
        for p in &pts[1..] {
            ops.push(Op::LineTo(
                p.0 + rand_offset(n, rng, o),
                p.1 + rand_offset(n, rng, o),
            ));
        }
    }
    ops
}

// ---------------------------------------------------------------------------
// Zigzag fill (class u extends l)
// ---------------------------------------------------------------------------

fn rotate_points(pts: &mut [(f64, f64)], center: (f64, f64), deg: f64) {
    if deg == 0.0 {
        return;
    }
    let a = std::f64::consts::PI / 180.0 * deg;
    let o = a.cos();
    let h = a.sin();
    for p in pts.iter_mut() {
        let (e, s) = (p.0, p.1);
        p.0 = (e - center.0) * o - (s - center.1) * h + center.0;
        p.1 = (e - center.0) * h + (s - center.1) * o + center.1;
    }
}

fn rotate_lines(lines: &mut [[(f64, f64); 2]], center: (f64, f64), deg: f64) {
    if deg == 0.0 {
        return;
    }
    let mut flat: Vec<(f64, f64)> = Vec::with_capacity(lines.len() * 2);
    for l in lines.iter() {
        flat.push(l[0]);
        flat.push(l[1]);
    }
    rotate_points(&mut flat, center, deg);
    for (i, l) in lines.iter_mut().enumerate() {
        l[0] = flat[i * 2];
        l[1] = flat[i * 2 + 1];
    }
}

fn hachure_lines(points_in: &[(f64, f64)], o: &Opts) -> Vec<[(f64, f64); 2]> {
    // Close the polygon.
    let mut pts: Vec<(f64, f64)> = points_in.to_vec();
    if pts.first() != pts.last()
        && let Some(f) = pts.first().copied()
    {
        pts.push(f);
    }
    if pts.len() <= 2 {
        return Vec::new();
    }

    let center = (0.0, 0.0);
    let rot = (o.hachure_angle + 90.0).round();
    rotate_points(&mut pts, center, rot);

    let mut gap = o.hachure_gap;
    if gap < 0.0 {
        gap = 4.0 * o.stroke_width;
    }
    gap = gap.max(0.1);

    #[derive(Clone)]
    struct Edge {
        ymin: f64,
        ymax: f64,
        x: f64,
        islope: f64,
    }
    let mut edges: Vec<Edge> = Vec::new();
    for w in pts.windows(2) {
        let e = w[0];
        let n = w[1];
        if e.1 != n.1 {
            let t = e.1.min(n.1);
            edges.push(Edge {
                ymin: t,
                ymax: e.1.max(n.1),
                x: if t == e.1 { e.0 } else { n.0 },
                islope: (n.0 - e.0) / (n.1 - e.1),
            });
        }
    }
    edges.sort_by(|a, b| {
        a.ymin
            .partial_cmp(&b.ymin)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| {
                if a.ymax == b.ymax {
                    std::cmp::Ordering::Equal
                } else if a.ymax > b.ymax {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Less
                }
            })
    });
    if edges.is_empty() {
        return Vec::new();
    }

    struct Active {
        edge: Edge,
    }
    let mut lines: Vec<[(f64, f64); 2]> = Vec::new();
    let mut active: Vec<Active> = Vec::new();
    let mut y = edges[0].ymin;
    let mut idx = 0usize;

    while !active.is_empty() || idx < edges.len() {
        if idx < edges.len() {
            let mut drained = 0;
            while idx + drained < edges.len() && edges[idx + drained].ymin <= y {
                drained += 1;
            }
            for e in edges.iter().skip(idx).take(drained) {
                active.push(Active { edge: e.clone() });
            }
            idx += drained;
        }
        active.retain(|a| a.edge.ymax > y);
        active.sort_by(|a, b| {
            a.edge
                .x
                .partial_cmp(&b.edge.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if active.len() > 1 {
            let mut i = 0;
            while i + 1 < active.len() {
                let s = &active[i];
                let t = &active[i + 1];
                lines.push([(s.edge.x.round(), y), (t.edge.x.round(), y)]);
                i += 2;
            }
        }
        y += gap;
        for a in active.iter_mut() {
            a.edge.x += gap * a.edge.islope;
        }
    }

    rotate_lines(&mut lines, center, -rot);
    lines
}

/// Zigzag fillPolygon (rough.js class u).
fn zigzag_fill_ops(pts: &[(f64, f64)], rng: &Rng, o: &Opts) -> Vec<Op> {
    let lines = hachure_lines(pts, o);
    let mut ops: Vec<Op> = Vec::new();
    let mut prev: Option<(f64, f64)> = None;
    for l in lines.iter() {
        ops.extend(double_line_ops(l[0].0, l[0].1, l[1].0, l[1].1, rng, o));
        if let Some(p) = prev {
            ops.extend(double_line_ops(p.0, p.1, l[0].0, l[0].1, rng, o));
        }
        prev = Some(l[1]);
    }
    ops
}

// ---------------------------------------------------------------------------
// opsToPath + js_num
// ---------------------------------------------------------------------------

fn opset_to_path(ops: &[Op]) -> String {
    let mut e = String::new();
    for op in ops {
        match op {
            Op::Move(a, b) => {
                write!(&mut e, "M{} {} ", js_num(*a), js_num(*b)).unwrap();
            }
            Op::BCurveTo(a, b, c, d, f, g) => {
                write!(
                    &mut e,
                    "C{} {}, {} {}, {} {} ",
                    js_num(*a),
                    js_num(*b),
                    js_num(*c),
                    js_num(*d),
                    js_num(*f),
                    js_num(*g)
                )
                .unwrap();
            }
            Op::LineTo(a, b) => {
                write!(&mut e, "L{} {} ", js_num(*a), js_num(*b)).unwrap();
            }
            Op::QCurveTo(a, b, c, d) => {
                write!(
                    &mut e,
                    "Q{} {}, {} {} ",
                    js_num(*a),
                    js_num(*b),
                    js_num(*c),
                    js_num(*d)
                )
                .unwrap();
            }
        }
    }
    e.trim().to_string()
}

/// Mimic `Number.prototype.toString()` for f64 values.
pub(crate) fn js_num(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        if x.is_nan() {
            return "NaN".into();
        }
        return if x > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if x.fract() == 0.0 && x.abs() < 1e21 {
        return format!("{}", x as i64);
    }
    format!("{}", x)
}

// ---------------------------------------------------------------------------
// Public draw_* entrypoints
// ---------------------------------------------------------------------------

/// One SVG `<path>` emitted by rough.js SVG renderer.
#[derive(Debug, Clone)]
pub struct RoughPath {
    pub d: String,
    pub stroke: String,
    pub stroke_width: String,
    pub fill: String,
}

fn opset_to_rough_path(op: &OpSet, o: &Opts) -> RoughPath {
    match op.kind {
        OpSetType::Path => RoughPath {
            d: opset_to_path(&op.ops),
            stroke: o.stroke.clone(),
            stroke_width: js_num(o.stroke_width),
            fill: "none".into(),
        },
        OpSetType::FillPath => RoughPath {
            d: opset_to_path(&op.ops),
            stroke: "none".into(),
            stroke_width: "0".into(),
            fill: o.fill.clone().unwrap_or_else(|| "none".into()),
        },
        OpSetType::FillSketch => {
            let mut w = o.fill_weight;
            if w < 0.0 {
                w = o.stroke_width / 2.0;
            }
            RoughPath {
                d: opset_to_path(&op.ops),
                stroke: o.fill.clone().unwrap_or_default(),
                stroke_width: js_num(w),
                fill: "none".into(),
            }
        }
    }
}

fn sets_to_paths(sets: &[OpSet], o: &Opts) -> Vec<RoughPath> {
    sets.iter().map(|s| opset_to_rough_path(s, o)).collect()
}

pub fn draw_rectangle(x: f64, y: f64, w: f64, h: f64, o: &Opts) -> Vec<RoughPath> {
    let rng = Rng::new(o.seed);
    let stroke_ops = linear_path_ops(
        &[(x, y), (x + w, y), (x + w, y + h), (x, y + h)],
        true,
        &rng,
        o,
    );
    let mut sets: Vec<OpSet> = Vec::new();
    if o.fill.is_some() {
        let fill_pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
        if o.fill_style == "solid" {
            sets.push(OpSet {
                kind: OpSetType::FillPath,
                ops: solid_fill_ops(&fill_pts, &rng, o),
            });
        } else if o.fill_style == "zigzag" {
            sets.push(OpSet {
                kind: OpSetType::FillSketch,
                ops: zigzag_fill_ops(&fill_pts, &rng, o),
            });
        }
    }
    if o.stroke != "none" {
        sets.push(OpSet {
            kind: OpSetType::Path,
            ops: stroke_ops,
        });
    }
    sets_to_paths(&sets, o)
}

pub fn draw_ellipse(cx: f64, cy: f64, w: f64, h: f64, o: &Opts) -> Vec<RoughPath> {
    let rng = Rng::new(o.seed);
    let params = ellipse_params(w, h, &rng, o);
    let (stroke_ops, estimated) = ellipse_path(cx, cy, &rng, o, params);
    let mut sets: Vec<OpSet> = Vec::new();
    if o.fill.is_some() {
        if o.fill_style == "solid" {
            // rough.js calls w() again for a solid fill, consuming another
            // full round of PRNG values.
            let (fill_ops, _) = ellipse_path(cx, cy, &rng, o, params);
            sets.push(OpSet {
                kind: OpSetType::FillPath,
                ops: fill_ops,
            });
        } else if o.fill_style == "zigzag" {
            sets.push(OpSet {
                kind: OpSetType::FillSketch,
                ops: zigzag_fill_ops(&estimated, &rng, o),
            });
        }
    }
    if o.stroke != "none" {
        sets.push(OpSet {
            kind: OpSetType::Path,
            ops: stroke_ops,
        });
    }
    sets_to_paths(&sets, o)
}

pub fn draw_circle(cx: f64, cy: f64, diameter: f64, o: &Opts) -> Vec<RoughPath> {
    draw_ellipse(cx, cy, diameter, diameter, o)
}

pub fn draw_line(x1: f64, y1: f64, x2: f64, y2: f64, o: &Opts) -> Vec<RoughPath> {
    let rng = Rng::new(o.seed);
    let ops = double_line_ops(x1, y1, x2, y2, &rng, o);
    sets_to_paths(
        &[OpSet {
            kind: OpSetType::Path,
            ops,
        }],
        o,
    )
}

pub fn draw_polygon(pts: &[(f64, f64)], o: &Opts) -> Vec<RoughPath> {
    let rng = Rng::new(o.seed);
    let stroke_ops = linear_path_ops(pts, true, &rng, o);
    let mut sets: Vec<OpSet> = Vec::new();
    if o.fill.is_some() {
        if o.fill_style == "solid" {
            sets.push(OpSet {
                kind: OpSetType::FillPath,
                ops: solid_fill_ops(pts, &rng, o),
            });
        } else if o.fill_style == "zigzag" {
            sets.push(OpSet {
                kind: OpSetType::FillSketch,
                ops: zigzag_fill_ops(pts, &rng, o),
            });
        }
    }
    if o.stroke != "none" {
        sets.push(OpSet {
            kind: OpSetType::Path,
            ops: stroke_ops,
        });
    }
    sets_to_paths(&sets, o)
}

pub fn draw_linear_path(pts: &[(f64, f64)], o: &Opts) -> Vec<RoughPath> {
    let rng = Rng::new(o.seed);
    let ops = linear_path_ops(pts, false, &rng, o);
    sets_to_paths(
        &[OpSet {
            kind: OpSetType::Path,
            ops,
        }],
        o,
    )
}

pub fn draw_path(d: &str, o: &Opts) -> Vec<RoughPath> {
    let rng = Rng::new(o.seed);
    let segments = parse_path(d);
    let mut state = PathState::new();
    let mut ops: Vec<Op> = Vec::new();
    for (idx, seg) in segments.iter().enumerate() {
        let prev = if idx > 0 {
            Some(&segments[idx - 1])
        } else {
            None
        };
        ops.extend(process_segment(&mut state, seg, prev, &rng, o));
    }

    let mut out: Vec<RoughPath> = Vec::new();
    if o.fill.is_some() && o.fill_style == "solid" {
        // path + solid fill: rough.js emits a path2Dfill, which the SVG
        // renderer draws as path element with d=<original d string>.
        out.push(RoughPath {
            d: d.to_string(),
            stroke: "none".into(),
            stroke_width: "0".into(),
            fill: o.fill.clone().unwrap_or_default(),
        });
    }
    if o.stroke != "none" {
        let set = OpSet {
            kind: OpSetType::Path,
            ops,
        };
        out.push(opset_to_rough_path(&set, o));
    }
    out
}
