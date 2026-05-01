//! rough.js port — byte-exact output parity with upstream's
//! `roughjs/bin/generator.js` + `renderer.js` + `math.js` as vendored
//! by mermaid 11.14.0 (rough.js 4.6.x). Scope is intentionally narrow:
//! only the code paths mermaid actually exercises when emitting default
//! and hand-drawn shapes (`rectangle`, `polygon`, `line`, `path`,
//! `circle`), and only the fills that appear in those paths
//! (`solid` + `hachure`).
//!
//! Determinism contract
//! --------------------
//! Upstream rough.js uses a Lehmer LCG seeded from the `seed` option:
//!
//! ```text
//! seed' = Math.imul(48271, seed)          // 32-bit signed multiply
//! val   = seed' & 0x7fff_ffff             // keep low 31 bits
//! r     = val / 2**31                     // normalised [0, 1)
//! ```
//!
//! When `seed` is `0` (the falsy branch of `Random.next`) it falls back
//! to `Math.random()`. Mermaid's reference harness overrides
//! `Math.random` with a fixed-seed **mulberry32** starting from state
//! `0x12345678`, reset per render. Both PRNGs are implemented here so
//! downstream callers can reproduce either path bit-for-bit.
//!
//! Float formatting
//! ----------------
//! Both roughjs (`opsToPath`) and mermaid emit control-point coordinates
//! via JavaScript `Number#toString`. Rust's default `{}` for `f64` uses
//! the same shortest-round-trip (Grisu/Ryu-family) algorithm and
//! matches byte-for-byte for every value we ever produce — see the
//! extensive tests at the bottom of this file.
//!
//! Original JS implementation: MIT © 2019 Preet Shihn
//! <https://github.com/rough-stuff/rough>. This module is a Rust port
//! of the subset mermaid uses; the original license travels with it.

// ── PRNG implementations ──────────────────────────────────────────────

/// Upstream roughjs LCG (`math.js::Random`). 31-bit Lehmer generator
/// with multiplier 48271 — identical to Numerical Recipes' `ran0` but
/// with JavaScript's `Math.imul` coercing to signed i32 on each step.
///
/// The seed is kept as a signed i32 to match upstream exactly — JS's
/// `Math.imul(48271, seed)` returns an i32, assigned back to
/// `this.seed`, and the subsequent `& 0x7fff_ffff` treats the i32 as
/// a 32-bit bitfield. We use `u32` internally for unsigned bit ops and
/// cast through i32 at the multiply step.
#[derive(Debug, Clone, Copy)]
pub struct RoughRandom {
    /// Current seed state, stored in the JS-equivalent signed i32
    /// representation (cast to u32 for bitwise math).
    seed: i32,
    /// `true` when the options seed was `0` / missing — in that case
    /// upstream falls through to `Math.random()`. Our test harness
    /// replaces `Math.random` with a mulberry32 over a fixed initial
    /// state; we thread a separately supplied `Mulberry32` in via
    /// [`RoughRandom::with_fallback`]. Not setting one leaves `next`
    /// returning `0.0` for the seed-0 case (only exercised by
    /// hand-drawn look paths that always set a non-zero seed, so the
    /// fallback is rarely meaningful in practice).
    fallback: Option<Mulberry32>,
}

impl RoughRandom {
    /// Build a generator seeded to the given i32 value. Seed of zero
    /// is the "falsy" branch — consult [`with_fallback`] to supply the
    /// Math.random replacement mermaid's test harness installs.
    ///
    /// [`with_fallback`]: RoughRandom::with_fallback
    pub fn new(seed: i32) -> Self {
        Self {
            seed,
            fallback: None,
        }
    }

    /// Attach a mulberry32 fallback. When the LCG's seed is 0, each
    /// `next()` call instead delegates to this generator — mirroring
    /// upstream's `Math.random()` branch for the seed-0 case.
    pub fn with_fallback(mut self, rng: Mulberry32) -> Self {
        self.fallback = Some(rng);
        self
    }

    /// Return the next `[0, 1)` float. Advances the internal LCG state
    /// when `seed != 0`; otherwise pulls from the mulberry32 fallback
    /// (or returns `0.0` when no fallback is attached).
    pub fn next(&mut self) -> f64 {
        if self.seed != 0 {
            // Math.imul(48271, seed) — 32-bit signed multiply.
            let product = (self.seed as i64 * 48271i64) as i32;
            self.seed = product;
            let val = (product as u32) & 0x7fff_ffff;
            val as f64 / 2147483648.0_f64 // 2^31
        } else if let Some(fb) = self.fallback.as_mut() {
            fb.next()
        } else {
            0.0
        }
    }
}

/// Mulberry32 PRNG — the `Math.random` replacement used by mermaid's
/// reference generator shim (`tests/support/generate_ref.mjs`). Exposed
/// as a standalone primitive so downstream code can seed / reset it
/// per render just like the shim does (`__rngState = 0x12345678;`).
#[derive(Debug, Clone, Copy)]
pub struct Mulberry32 {
    state: u32,
}

impl Mulberry32 {
    /// Mermaid shim default: `0x12345678`.
    pub const MERMAID_SHIM_SEED: u32 = 0x1234_5678;

    /// Construct with the given starting state.
    pub fn new(state: u32) -> Self {
        Self { state }
    }

    /// Reset back to `0x12345678` — the per-render reset the mermaid
    /// shim performs inside `renderOne()`.
    pub fn reset_mermaid_default(&mut self) {
        self.state = Self::MERMAID_SHIM_SEED;
    }

    /// Produce the next `[0, 1)` float and advance the state. This is
    /// an exact transliteration of the upstream shim's `__mulberry32`.
    pub fn next(&mut self) -> f64 {
        // state = (state + 0x6d2b79f5) | 0;  (wrap to i32)
        self.state = self.state.wrapping_add(0x6d2b79f5);
        let mut t: u32 = self.state;
        // t = Math.imul(t ^ (t >>> 15), 1 | t);
        t = (t ^ (t >> 15)).wrapping_mul(1 | t);
        // t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        // ((t ^ (t >>> 14)) >>> 0) / 4294967296
        (t ^ (t >> 14)) as f64 / 4_294_967_296.0_f64
    }
}

// ── Options + default values ──────────────────────────────────────────

/// roughjs option bundle. Field names track upstream 1:1 — even when a
/// field is unused in mermaid's call pattern, it's present so future
/// features can be wired up without churning this struct's shape.
#[derive(Debug, Clone)]
pub struct RoughOptions {
    pub max_randomness_offset: f64,
    pub roughness: f64,
    pub bowing: f64,
    pub stroke: String,
    pub stroke_width: f64,
    pub curve_tightness: f64,
    pub curve_fitting: f64,
    pub curve_step_count: f64,
    /// `"hachure" | "solid" | "zigzag" | "cross-hatch" | "dots" | "dashed" | "zigzag-line"`.
    pub fill_style: String,
    pub fill_weight: f64,
    pub hachure_angle: f64,
    pub hachure_gap: f64,
    pub dash_offset: f64,
    pub dash_gap: f64,
    pub zigzag_offset: f64,
    pub seed: i32,
    pub disable_multi_stroke: bool,
    pub disable_multi_stroke_fill: bool,
    pub preserve_vertices: bool,
    pub fill_shape_roughness_gain: f64,
    /// `None` ⇒ no fill; `Some(color)` ⇒ emit fill sub-path.
    pub fill: Option<String>,
    /// `fill_line_dash` / `stroke_line_dash` — emitted as
    /// `stroke-dasharray="a b"` on the stroke path.
    pub fill_line_dash: Vec<f64>,
    pub stroke_line_dash: Vec<f64>,
}

impl Default for RoughOptions {
    /// Upstream `RoughGenerator.defaultOptions`, verbatim.
    fn default() -> Self {
        Self {
            max_randomness_offset: 2.0,
            roughness: 1.0,
            bowing: 1.0,
            stroke: "#000".into(),
            stroke_width: 1.0,
            curve_tightness: 0.0,
            curve_fitting: 0.95,
            curve_step_count: 9.0,
            fill_style: "hachure".into(),
            fill_weight: -1.0,
            hachure_angle: -41.0,
            hachure_gap: -1.0,
            dash_offset: -1.0,
            dash_gap: -1.0,
            zigzag_offset: -1.0,
            seed: 0,
            disable_multi_stroke: false,
            disable_multi_stroke_fill: false,
            preserve_vertices: false,
            fill_shape_roughness_gain: 0.8,
            fill: None,
            fill_line_dash: Vec::new(),
            stroke_line_dash: Vec::new(),
        }
    }
}

// ── Intermediate representation: Op + OpSet ───────────────────────────

/// A single drawing op, matching upstream's `{op, data}` objects.
#[derive(Debug, Clone)]
pub enum Op {
    Move(f64, f64),
    LineTo(f64, f64),
    BCurveTo(f64, f64, f64, f64, f64, f64),
}

/// Type tag on an [`OpSet`]. Matches upstream's 3 drawing types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpSetType {
    Path,
    FillPath,
    FillSketch,
}

/// A grouped drawing — one of `rectangle`'s "fill + outline" halves,
/// or a standalone polygon. Translated to an SVG `<path>` via
/// [`ops_to_path`].
#[derive(Debug, Clone)]
pub struct OpSet {
    pub op_type: OpSetType,
    pub ops: Vec<Op>,
}

// ── The driver: translate a high-level shape to a list of OpSets ──────

/// One fully-populated rough.js drawable — exactly what mermaid's
/// shape code receives from `rc.rectangle(...)` / `rc.polygon(...)` /
/// `rc.path(...)`.
#[derive(Debug, Clone)]
pub struct Drawable {
    pub shape: &'static str,
    pub sets: Vec<OpSet>,
}

/// High-level entry points — mirror `RoughGenerator`'s `rectangle` /
/// `polygon` / `line` / `path` methods. Each allocates a fresh LCG
/// (and, optionally, a mulberry32 fallback attached by the caller).
pub struct RoughGenerator {
    /// Mermaid-shim compatible mulberry32, used as the Math.random
    /// fallback when an option bag specifies `seed: 0`. Callers that
    /// want a deterministic fallback pre-seed this; default `new()`
    /// starts at `0x12345678` (the mermaid shim's initial state) and
    /// is *not* reset between shape calls — matching the shim's
    /// per-render reset semantics.
    pub fallback: Mulberry32,
}

impl Default for RoughGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl RoughGenerator {
    /// Build a fresh generator with a mermaid-shim-compatible
    /// mulberry32 fallback at state `0x12345678`.
    pub fn new() -> Self {
        Self {
            fallback: Mulberry32::new(Mulberry32::MERMAID_SHIM_SEED),
        }
    }

    /// Reset the mulberry32 fallback to the mermaid-shim default state.
    /// Matches `__rngState = 0x12345678;` inside `renderOne()`.
    pub fn reset_fallback(&mut self) {
        self.fallback.reset_mermaid_default();
    }

    /// Produce a `RoughRandom` carrying the option-bag's seed plus the
    /// current fallback state. The fallback is shared by value — each
    /// `make_random` call snapshots the mulberry32; callers that
    /// mutate the RNG are expected to thread state back via
    /// [`RoughGenerator::fallback`].
    fn make_random(&self, o: &RoughOptions) -> RoughRandom {
        RoughRandom::new(o.seed).with_fallback(self.fallback)
    }

    /// `rc.rectangle(x, y, w, h, options)` — same order as upstream:
    /// fill half first (mutates RNG), then outline half. The emitted
    /// SVG places the solid fill *before* the stroke on `<path>` order.
    pub fn rectangle(&mut self, x: f64, y: f64, w: f64, h: f64, o: &RoughOptions) -> Drawable {
        let mut rng = self.make_random(o);
        let mut sets = Vec::with_capacity(2);
        let outline = rectangle_outline(x, y, w, h, o, &mut rng);
        if o.fill.is_some() {
            let points = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
            if o.fill_style == "solid" {
                sets.push(solid_fill_polygon(&[&points[..]], o, &mut rng));
            } else {
                // Hachure is out of scope for the attribute-bearing ER
                // box (which always runs with fillStyle=solid). Fall
                // back to producing the same polygon as the solid case
                // to keep downstream integrations non-panicking; a
                // future wave can port `scan-line-hachure.js`.
                sets.push(solid_fill_polygon(&[&points[..]], o, &mut rng));
            }
        }
        if o.stroke != "none" {
            sets.push(outline);
        }
        // Persist any mulberry32 advancement (only relevant when
        // seed == 0, but harmless otherwise — the LCG path doesn't
        // touch the fallback).
        if let Some(fb) = rng.fallback {
            self.fallback = fb;
        }
        Drawable {
            shape: "rectangle",
            sets,
        }
    }

    /// `rc.polygon(points, options)` — used by ER's divider degenerate
    /// rects. Points are `(x, y)` pairs; the polygon is closed.
    pub fn polygon(&mut self, points: &[(f64, f64)], o: &RoughOptions) -> Drawable {
        let mut rng = self.make_random(o);
        let mut sets = Vec::with_capacity(2);
        let outline = linear_path(points, true, o, &mut rng);
        if o.fill.is_some() {
            if o.fill_style == "solid" {
                // Mimic the JS: `solidFillPolygon([points], o)`.
                sets.push(solid_fill_points(&[points], o, &mut rng));
            } else {
                sets.push(solid_fill_points(&[points], o, &mut rng));
            }
        }
        if o.stroke != "none" {
            sets.push(outline);
        }
        if let Some(fb) = rng.fallback {
            self.fallback = fb;
        }
        Drawable {
            shape: "polygon",
            sets,
        }
    }

    /// `rc.line(x1, y1, x2, y2, options)`.
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, o: &RoughOptions) -> Drawable {
        let mut rng = self.make_random(o);
        let sets = vec![OpSet {
            op_type: OpSetType::Path,
            ops: double_line(x1, y1, x2, y2, o, false, &mut rng),
        }];
        if let Some(fb) = rng.fallback {
            self.fallback = fb;
        }
        Drawable {
            shape: "line",
            sets,
        }
    }

    /// `rc.path(d, options)` — accepts an SVG path `d` string. Mirrors
    /// upstream `RoughGenerator.path` for the M/L/Z subset that mermaid
    /// shapes (stadium, etc.) feed in via `createPathFromPoints`.
    ///
    /// Critical detail: upstream allocates one `Random` instance the
    /// first time `_o(options)` is materialised on the caller's option
    /// bag, then `Object.assign({}, o, {...})` for the fill copies the
    /// `randomizer` reference. Both calls share the same RNG state.
    /// We mirror that by running `svg_path` once for the stroke first
    /// (consuming N reads), then `svg_path` again for the fill — both
    /// against the same `RoughRandom`.
    ///
    /// Path order matches upstream:
    ///   1. fillPath (after `_mergedShape`) when `hasFill && fillStyle == 'solid'`.
    ///   2. stroke path.
    /// The `_o` order of operations in upstream is:
    ///   `shape = svgPath(d, o)`     ← stroke path, runs first, advances RNG
    ///   `fillShape = svgPath(d, fillOpts)` ← fill path, RNG continues
    /// then `paths.push(fill)`, `paths.push(stroke)`. Output emit order
    /// is `[fill, stroke]`. We follow that exact ordering.
    pub fn path(&mut self, d: &str, o: &RoughOptions) -> Drawable {
        let mut rng = self.make_random(o);
        let mut sets: Vec<OpSet> = Vec::with_capacity(2);

        let segs = path_normalize(&path_absolutize(&path_parse(d)));
        let has_fill = match &o.fill {
            None => false,
            Some(s) => s != "none" && s != "transparent",
        };
        let has_stroke = o.stroke != "none";

        // Stroke pass first — same as upstream. Even if hasStroke is
        // false, the RNG is still advanced by upstream because
        // `shape = svgPath(d, o)` runs unconditionally before the
        // hasFill / hasStroke checks.
        let stroke_ops = svg_path_ops(&segs, o, &mut rng);

        // Fill pass — fillShape uses {disableMultiStroke: true,
        // roughness: o.roughness ? o.roughness + fill_shape_roughness_gain : 0}.
        if has_fill {
            let mut fill_opts = o.clone();
            fill_opts.disable_multi_stroke = true;
            fill_opts.roughness = if o.roughness != 0.0 {
                o.roughness + o.fill_shape_roughness_gain
            } else {
                0.0
            };
            let fill_ops = svg_path_ops(&segs, &fill_opts, &mut rng);
            // _mergedShape: keep first op, drop subsequent moves.
            let merged = merged_shape(fill_ops);
            sets.push(OpSet {
                op_type: OpSetType::FillPath,
                ops: merged,
            });
        }

        if has_stroke {
            sets.push(OpSet {
                op_type: OpSetType::Path,
                ops: stroke_ops,
            });
        }

        if let Some(fb) = rng.fallback {
            self.fallback = fb;
        }
        Drawable {
            shape: "path",
            sets,
        }
    }
}

/// Filter ops: keep first op, drop subsequent `Move` ops. Mirrors
/// upstream `_mergedShape`.
fn merged_shape(ops: Vec<Op>) -> Vec<Op> {
    let mut out: Vec<Op> = Vec::with_capacity(ops.len());
    for (i, op) in ops.into_iter().enumerate() {
        if i == 0 {
            out.push(op);
            continue;
        }
        if matches!(op, Op::Move(_, _)) {
            continue;
        }
        out.push(op);
    }
    out
}

// ── SVG path d parser (M/L/H/V/Z subset) ─────────────────────────────
//
// Mermaid only ever feeds rough.path() strings of the form
//   "M x,y L x,y L x,y ... Z"
// (from `createPathFromPoints`). To stay tractable and byte-exact we
// implement just the M / L / H / V / Z commands plus their relative
// counterparts. Anything more exotic (curves, arcs) is silently
// dropped — the caller has fallback paths for those shapes.

#[derive(Debug, Clone)]
struct RawSeg {
    key: char,
    data: Vec<f64>,
}

fn path_parse(d: &str) -> Vec<RawSeg> {
    // Tokenize: commands | numbers | whitespace/comma.
    let bytes = d.as_bytes();
    let mut tokens: Vec<(char, Option<f64>)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == ' ' || c == ',' || c == '\t' || c == '\n' || c == '\r' {
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() && "MmLlHhVvZzCcSsQqTtAa".contains(c) {
            tokens.push((c, None));
            i += 1;
            continue;
        }
        // Parse number.
        let start = i;
        if bytes[i] == b'-' || bytes[i] == b'+' {
            i += 1;
        }
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            i += 1;
            if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
                i += 1;
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if start == i {
            // Unknown char — skip.
            i += 1;
            continue;
        }
        if let Ok(v) = std::str::from_utf8(&bytes[start..i]).unwrap().parse::<f64>() {
            tokens.push(('#', Some(v)));
        }
    }

    let params_count = |k: char| -> usize {
        match k {
            'M' | 'm' | 'L' | 'l' | 'T' | 't' => 2,
            'H' | 'h' | 'V' | 'v' => 1,
            'C' | 'c' => 6,
            'S' | 's' | 'Q' | 'q' => 4,
            'A' | 'a' => 7,
            'Z' | 'z' => 0,
            _ => 0,
        }
    };

    let mut segs = Vec::new();
    let mut idx = 0;
    let mut mode = '\0';
    while idx < tokens.len() {
        let t = tokens[idx];
        if t.0 != '#' {
            mode = t.0;
            idx += 1;
            if mode == 'Z' || mode == 'z' {
                segs.push(RawSeg {
                    key: mode,
                    data: vec![],
                });
                continue;
            }
        }
        let need = params_count(mode);
        if idx + need > tokens.len() {
            break;
        }
        let mut data = Vec::with_capacity(need);
        for j in 0..need {
            if let (_, Some(v)) = tokens[idx + j] {
                data.push(v);
            } else {
                return segs;
            }
        }
        idx += need;
        segs.push(RawSeg { key: mode, data });
        // Per SVG spec, after an M, subsequent implicit numbers are L.
        if mode == 'M' {
            mode = 'L';
        } else if mode == 'm' {
            mode = 'l';
        }
    }
    segs
}

fn path_absolutize(segs: &[RawSeg]) -> Vec<RawSeg> {
    let mut out = Vec::with_capacity(segs.len());
    let mut cx = 0.0_f64;
    let mut cy = 0.0_f64;
    let mut subx = 0.0_f64;
    let mut suby = 0.0_f64;
    for seg in segs {
        match seg.key {
            'M' => {
                out.push(seg.clone());
                cx = seg.data[0];
                cy = seg.data[1];
                subx = cx;
                suby = cy;
            }
            'm' => {
                cx += seg.data[0];
                cy += seg.data[1];
                out.push(RawSeg {
                    key: 'M',
                    data: vec![cx, cy],
                });
                subx = cx;
                suby = cy;
            }
            'L' => {
                out.push(seg.clone());
                cx = seg.data[0];
                cy = seg.data[1];
            }
            'l' => {
                cx += seg.data[0];
                cy += seg.data[1];
                out.push(RawSeg {
                    key: 'L',
                    data: vec![cx, cy],
                });
            }
            'H' => {
                out.push(seg.clone());
                cx = seg.data[0];
            }
            'h' => {
                cx += seg.data[0];
                out.push(RawSeg {
                    key: 'H',
                    data: vec![cx],
                });
            }
            'V' => {
                out.push(seg.clone());
                cy = seg.data[0];
            }
            'v' => {
                cy += seg.data[0];
                out.push(RawSeg {
                    key: 'V',
                    data: vec![cy],
                });
            }
            'C' => {
                out.push(seg.clone());
                cx = seg.data[4];
                cy = seg.data[5];
            }
            'c' => {
                let mut data = seg.data.clone();
                for (i, v) in data.iter_mut().enumerate() {
                    if i % 2 == 0 {
                        *v += cx;
                    } else {
                        *v += cy;
                    }
                }
                cx = data[4];
                cy = data[5];
                out.push(RawSeg { key: 'C', data });
            }
            'Z' | 'z' => {
                out.push(RawSeg {
                    key: 'Z',
                    data: vec![],
                });
                cx = subx;
                cy = suby;
            }
            _ => {
                // Unsupported command — pass through. Mermaid stadium
                // does not exercise this branch.
                out.push(seg.clone());
            }
        }
    }
    out
}

fn path_normalize(segs: &[RawSeg]) -> Vec<RawSeg> {
    // For our M/L/H/V/Z subset, normalize collapses H and V into L.
    let mut out: Vec<RawSeg> = Vec::with_capacity(segs.len());
    let mut cx = 0.0_f64;
    let mut cy = 0.0_f64;
    let mut subx = 0.0_f64;
    let mut suby = 0.0_f64;
    for seg in segs {
        match seg.key {
            'M' => {
                out.push(seg.clone());
                cx = seg.data[0];
                cy = seg.data[1];
                subx = cx;
                suby = cy;
            }
            'L' => {
                out.push(seg.clone());
                cx = seg.data[0];
                cy = seg.data[1];
            }
            'H' => {
                cx = seg.data[0];
                out.push(RawSeg {
                    key: 'L',
                    data: vec![cx, cy],
                });
            }
            'V' => {
                cy = seg.data[0];
                out.push(RawSeg {
                    key: 'L',
                    data: vec![cx, cy],
                });
            }
            'C' => {
                out.push(seg.clone());
                cx = seg.data[4];
                cy = seg.data[5];
            }
            'Z' => {
                out.push(seg.clone());
                cx = subx;
                cy = suby;
            }
            _ => {
                out.push(seg.clone());
            }
        }
    }
    out
}

/// Mirrors upstream `svgPath(path, o)` from `renderer.js`.
fn svg_path_ops(segs: &[RawSeg], o: &RoughOptions, rng: &mut RoughRandom) -> Vec<Op> {
    let mut ops: Vec<Op> = Vec::new();
    let mut first = (0.0_f64, 0.0_f64);
    let mut current = (0.0_f64, 0.0_f64);
    for seg in segs {
        match seg.key {
            'M' => {
                current = (seg.data[0], seg.data[1]);
                first = current;
            }
            'L' => {
                ops.extend(double_line(
                    current.0, current.1, seg.data[0], seg.data[1], o, false, rng,
                ));
                current = (seg.data[0], seg.data[1]);
            }
            'C' => {
                let x1 = seg.data[0];
                let y1 = seg.data[1];
                let x2 = seg.data[2];
                let y2 = seg.data[3];
                let x = seg.data[4];
                let y = seg.data[5];
                ops.extend(bezier_to(x1, y1, x2, y2, x, y, current, o, rng));
                current = (x, y);
            }
            'Z' => {
                ops.extend(double_line(
                    current.0, current.1, first.0, first.1, o, false, rng,
                ));
                current = first;
            }
            _ => {}
        }
    }
    ops
}

/// Mirrors upstream `_bezierTo`. Used by `svg_path_ops` for `C`
/// segments. Stadium doesn't hit this in its M/L/Z form, but other
/// shapes use it via `path()`.
fn bezier_to(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    x: f64,
    y: f64,
    current: (f64, f64),
    o: &RoughOptions,
    rng: &mut RoughRandom,
) -> Vec<Op> {
    let mut ops = Vec::with_capacity(4);
    let max_off = if o.max_randomness_offset == 0.0 {
        1.0
    } else {
        o.max_randomness_offset
    };
    let ros = [max_off, max_off + 0.3];
    let iterations = if o.disable_multi_stroke { 1 } else { 2 };
    let preserve = o.preserve_vertices;
    for i in 0..iterations {
        let move_x;
        let move_y;
        if i == 0 {
            move_x = current.0;
            move_y = current.1;
        } else {
            move_x = current.0
                + if preserve {
                    0.0
                } else {
                    offset_opt(ros[0], o, 1.0, rng)
                };
            move_y = current.1
                + if preserve {
                    0.0
                } else {
                    offset_opt(ros[0], o, 1.0, rng)
                };
        }
        ops.push(Op::Move(move_x, move_y));
        let f0;
        let f1;
        if preserve {
            f0 = x;
            f1 = y;
        } else {
            f0 = x + offset_opt(ros[i], o, 1.0, rng);
            f1 = y + offset_opt(ros[i], o, 1.0, rng);
        }
        let cx1 = x1 + offset_opt(ros[i], o, 1.0, rng);
        let cy1 = y1 + offset_opt(ros[i], o, 1.0, rng);
        let cx2 = x2 + offset_opt(ros[i], o, 1.0, rng);
        let cy2 = y2 + offset_opt(ros[i], o, 1.0, rng);
        ops.push(Op::BCurveTo(cx1, cy1, cx2, cy2, f0, f1));
    }
    ops
}

// ── `rectangle(x, y, w, h, o)` from renderer.js — pure outline ─────────
fn rectangle_outline(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    o: &RoughOptions,
    rng: &mut RoughRandom,
) -> OpSet {
    let points = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    linear_path(&points, true, o, rng)
}

// ── `linearPath(points, close, o)` from renderer.js ───────────────────
fn linear_path(
    points: &[(f64, f64)],
    close: bool,
    o: &RoughOptions,
    rng: &mut RoughRandom,
) -> OpSet {
    let len = points.len();
    if len > 2 {
        let mut ops = Vec::with_capacity(len * 22);
        for i in 0..(len - 1) {
            ops.extend(double_line(
                points[i].0,
                points[i].1,
                points[i + 1].0,
                points[i + 1].1,
                o,
                false,
                rng,
            ));
        }
        if close {
            ops.extend(double_line(
                points[len - 1].0,
                points[len - 1].1,
                points[0].0,
                points[0].1,
                o,
                false,
                rng,
            ));
        }
        OpSet {
            op_type: OpSetType::Path,
            ops,
        }
    } else if len == 2 {
        OpSet {
            op_type: OpSetType::Path,
            ops: double_line(
                points[0].0,
                points[0].1,
                points[1].0,
                points[1].1,
                o,
                false,
                rng,
            ),
        }
    } else {
        OpSet {
            op_type: OpSetType::Path,
            ops: Vec::new(),
        }
    }
}

// ── `_doubleLine` / `_line` from renderer.js ──────────────────────────
fn double_line(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    o: &RoughOptions,
    filling: bool,
    rng: &mut RoughRandom,
) -> Vec<Op> {
    let single_stroke = if filling {
        o.disable_multi_stroke_fill
    } else {
        o.disable_multi_stroke
    };
    let mut ops = line_ops(x1, y1, x2, y2, o, true, false, rng);
    if single_stroke {
        return ops;
    }
    ops.extend(line_ops(x1, y1, x2, y2, o, true, true, rng));
    ops
}

fn line_ops(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    o: &RoughOptions,
    do_move: bool,
    overlay: bool,
    rng: &mut RoughRandom,
) -> Vec<Op> {
    let length_sq = (x1 - x2).powi(2) + (y1 - y2).powi(2);
    let length = length_sq.sqrt();
    let roughness_gain = if length < 200.0 {
        1.0
    } else if length > 500.0 {
        0.4
    } else {
        -0.0016668 * length + 1.233334
    };

    let mut offset = o.max_randomness_offset;
    if offset * offset * 100.0 > length_sq {
        offset = length / 10.0;
    }
    let half_offset = offset / 2.0;

    // ── RNG call 1: divergePoint ─────────────────────────────────────
    let r1 = rng.next();
    let diverge_point = 0.2 + r1 * 0.2;

    // ── RNG call 2, 3: midDispX, midDispY ────────────────────────────
    let mid_disp_x_pre = o.bowing * o.max_randomness_offset * (y2 - y1) / 200.0;
    let mid_disp_y_pre = o.bowing * o.max_randomness_offset * (x1 - x2) / 200.0;
    let mid_disp_x = offset_opt(mid_disp_x_pre, o, roughness_gain, rng);
    let mid_disp_y = offset_opt(mid_disp_y_pre, o, roughness_gain, rng);

    let preserve_vertices = o.preserve_vertices;
    let mut ops = Vec::with_capacity(2);

    if do_move {
        if overlay {
            // Both components pull from the half-offset range.
            let mx = if preserve_vertices {
                0.0
            } else {
                offset_opt(half_offset, o, roughness_gain, rng)
            };
            let my = if preserve_vertices {
                0.0
            } else {
                offset_opt(half_offset, o, roughness_gain, rng)
            };
            ops.push(Op::Move(x1 + mx, y1 + my));
        } else {
            let mx = if preserve_vertices {
                0.0
            } else {
                offset_opt(offset, o, roughness_gain, rng)
            };
            let my = if preserve_vertices {
                0.0
            } else {
                offset_opt(offset, o, roughness_gain, rng)
            };
            ops.push(Op::Move(x1 + mx, y1 + my));
        }
    }

    // bcurveTo. Note: each of the 6 random reads is a distinct
    // rng.next() call. We inline the offset_opt call order exactly.
    let (c1x, c1y, c2x, c2y, ex, ey) = if overlay {
        let c1x_r = offset_opt(half_offset, o, roughness_gain, rng);
        let c1y_r = offset_opt(half_offset, o, roughness_gain, rng);
        let c2x_r = offset_opt(half_offset, o, roughness_gain, rng);
        let c2y_r = offset_opt(half_offset, o, roughness_gain, rng);
        let ex_r = if preserve_vertices {
            0.0
        } else {
            offset_opt(half_offset, o, roughness_gain, rng)
        };
        let ey_r = if preserve_vertices {
            0.0
        } else {
            offset_opt(half_offset, o, roughness_gain, rng)
        };
        (
            mid_disp_x + x1 + (x2 - x1) * diverge_point + c1x_r,
            mid_disp_y + y1 + (y2 - y1) * diverge_point + c1y_r,
            mid_disp_x + x1 + 2.0 * (x2 - x1) * diverge_point + c2x_r,
            mid_disp_y + y1 + 2.0 * (y2 - y1) * diverge_point + c2y_r,
            x2 + ex_r,
            y2 + ey_r,
        )
    } else {
        let c1x_r = offset_opt(offset, o, roughness_gain, rng);
        let c1y_r = offset_opt(offset, o, roughness_gain, rng);
        let c2x_r = offset_opt(offset, o, roughness_gain, rng);
        let c2y_r = offset_opt(offset, o, roughness_gain, rng);
        let ex_r = if preserve_vertices {
            0.0
        } else {
            offset_opt(offset, o, roughness_gain, rng)
        };
        let ey_r = if preserve_vertices {
            0.0
        } else {
            offset_opt(offset, o, roughness_gain, rng)
        };
        (
            mid_disp_x + x1 + (x2 - x1) * diverge_point + c1x_r,
            mid_disp_y + y1 + (y2 - y1) * diverge_point + c1y_r,
            mid_disp_x + x1 + 2.0 * (x2 - x1) * diverge_point + c2x_r,
            mid_disp_y + y1 + 2.0 * (y2 - y1) * diverge_point + c2y_r,
            x2 + ex_r,
            y2 + ey_r,
        )
    };
    ops.push(Op::BCurveTo(c1x, c1y, c2x, c2y, ex, ey));
    ops
}

/// `_offset(min, max, ops, rg) = ops.roughness * rg * (random*(max-min)+min)`.
fn offset(min_v: f64, max_v: f64, o: &RoughOptions, rg: f64, rng: &mut RoughRandom) -> f64 {
    o.roughness * rg * (rng.next() * (max_v - min_v) + min_v)
}

/// `_offsetOpt(x, ops, rg) = _offset(-x, x, ops, rg)`.
fn offset_opt(x: f64, o: &RoughOptions, rg: f64, rng: &mut RoughRandom) -> f64 {
    offset(-x, x, o, rg, rng)
}

// ── `solidFillPolygon` — fills from maxRandomnessOffset ──────────────
fn solid_fill_polygon<P: AsRef<[[f64; 2]]>>(
    polys: &[P],
    o: &RoughOptions,
    rng: &mut RoughRandom,
) -> OpSet {
    let offset_raw = o.max_randomness_offset;
    let mut ops = Vec::new();
    for poly in polys {
        let points = poly.as_ref();
        if !points.is_empty() && points.len() > 2 {
            // move
            let mx = points[0][0] + offset_opt(offset_raw, o, 1.0, rng);
            let my = points[0][1] + offset_opt(offset_raw, o, 1.0, rng);
            ops.push(Op::Move(mx, my));
            for p in &points[1..] {
                let lx = p[0] + offset_opt(offset_raw, o, 1.0, rng);
                let ly = p[1] + offset_opt(offset_raw, o, 1.0, rng);
                ops.push(Op::LineTo(lx, ly));
            }
        }
    }
    OpSet {
        op_type: OpSetType::FillPath,
        ops,
    }
}

/// Variant of [`solid_fill_polygon`] accepting `&[(f64, f64)]` — used
/// by [`RoughGenerator::polygon`] where the caller already has the
/// input as a flat points slice.
fn solid_fill_points(polys: &[&[(f64, f64)]], o: &RoughOptions, rng: &mut RoughRandom) -> OpSet {
    let offset_raw = o.max_randomness_offset;
    let mut ops = Vec::new();
    for poly in polys {
        let points = *poly;
        if !points.is_empty() && points.len() > 2 {
            let mx = points[0].0 + offset_opt(offset_raw, o, 1.0, rng);
            let my = points[0].1 + offset_opt(offset_raw, o, 1.0, rng);
            ops.push(Op::Move(mx, my));
            for p in &points[1..] {
                let lx = p.0 + offset_opt(offset_raw, o, 1.0, rng);
                let ly = p.1 + offset_opt(offset_raw, o, 1.0, rng);
                ops.push(Op::LineTo(lx, ly));
            }
        }
    }
    OpSet {
        op_type: OpSetType::FillPath,
        ops,
    }
}

// ── `opsToPath` — turn a sequence of ops into a `d` attribute ─────────

/// Render a single [`OpSet`]'s ops to the SVG `d` attribute string —
/// mirrors `RoughGenerator.opsToPath(drawing, fixedDecimals=undefined)`.
/// Trailing whitespace is trimmed (upstream calls `.trim()` on the
/// accumulated string).
pub fn ops_to_path(set: &OpSet) -> String {
    let mut s = String::with_capacity(set.ops.len() * 32);
    for op in &set.ops {
        match op {
            Op::Move(x, y) => {
                s.push('M');
                s.push_str(&fmt_num(*x));
                s.push(' ');
                s.push_str(&fmt_num(*y));
                s.push(' ');
            }
            Op::LineTo(x, y) => {
                s.push('L');
                s.push_str(&fmt_num(*x));
                s.push(' ');
                s.push_str(&fmt_num(*y));
                s.push(' ');
            }
            Op::BCurveTo(c1x, c1y, c2x, c2y, ex, ey) => {
                s.push('C');
                s.push_str(&fmt_num(*c1x));
                s.push(' ');
                s.push_str(&fmt_num(*c1y));
                s.push_str(", ");
                s.push_str(&fmt_num(*c2x));
                s.push(' ');
                s.push_str(&fmt_num(*c2y));
                s.push_str(", ");
                s.push_str(&fmt_num(*ex));
                s.push(' ');
                s.push_str(&fmt_num(*ey));
                s.push(' ');
            }
        }
    }
    // Strip trailing spaces — `path.trim()` in upstream.
    while s.ends_with(' ') {
        s.pop();
    }
    s
}

// ── Path assembly (SVG `<path>` elements, one per OpSet) ──────────────

/// One emitted `<path>` element — analogous to upstream's `toPaths`
/// return values. Callers wrap these in the appropriate parent `<g>`.
#[derive(Debug, Clone)]
pub struct PathOut {
    pub d: String,
    pub stroke: String,
    pub stroke_width: f64,
    pub fill: String,
    /// For `fillPath` drawings, upstream adds `fill-rule="evenodd"`.
    /// We carry that flag here so the SVG generator can emit the attr.
    pub fill_rule_evenodd: bool,
    /// Emitted as `stroke-dasharray="a b"` on stroke paths only.
    pub stroke_dasharray: Option<String>,
}

/// Convert a [`Drawable`] into one or more emitable paths. Upstream's
/// `RoughSVG.draw` is the reference; we include the subtle bit that
/// `fill-rule="evenodd"` gets emitted only when the source shape was
/// `"polygon"` or `"curve"` — `rectangle` fills do **not** carry
/// `fill-rule`, matching `roughjs/bin/svg.js` exactly.
pub fn to_paths(d: &Drawable, o: &RoughOptions) -> Vec<PathOut> {
    let emit_fill_rule = d.shape == "polygon" || d.shape == "curve";
    let mut out = Vec::with_capacity(d.sets.len());
    for set in &d.sets {
        match set.op_type {
            OpSetType::Path => {
                out.push(PathOut {
                    d: ops_to_path(set),
                    stroke: o.stroke.clone(),
                    stroke_width: o.stroke_width,
                    fill: "none".into(),
                    fill_rule_evenodd: false,
                    stroke_dasharray: Some(dasharray_str(&o.stroke_line_dash)),
                });
            }
            OpSetType::FillPath => {
                out.push(PathOut {
                    d: ops_to_path(set),
                    stroke: "none".into(),
                    stroke_width: 0.0,
                    fill: o.fill.clone().unwrap_or_else(|| "none".into()),
                    fill_rule_evenodd: emit_fill_rule,
                    stroke_dasharray: None,
                });
            }
            OpSetType::FillSketch => {
                // Only reached for hachure (out of scope for MVP).
                let fweight = if o.fill_weight < 0.0 {
                    o.stroke_width / 2.0
                } else {
                    o.fill_weight
                };
                out.push(PathOut {
                    d: ops_to_path(set),
                    stroke: o.fill.clone().unwrap_or_else(|| "none".into()),
                    stroke_width: fweight,
                    fill: "none".into(),
                    fill_rule_evenodd: false,
                    stroke_dasharray: Some(dasharray_str(&o.fill_line_dash)),
                });
            }
        }
    }
    out
}

fn dasharray_str(vals: &[f64]) -> String {
    if vals.is_empty() {
        "0 0".to_string()
    } else {
        let mut s = String::new();
        for (i, v) in vals.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            s.push_str(&fmt_num(*v));
        }
        s
    }
}

// ── Number formatting (shared with the rest of the render module) ────

/// JavaScript `Number#toString` equivalent. `-0.0` becomes `"0"`;
/// finite integers print without a decimal point; every other value
/// relies on Rust's default shortest-round-trip f64 `Display`.
pub fn fmt_num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    format!("{}", v)
}

// ── Emit a full `<path>` element from a [`PathOut`] ───────────────────

/// Serialise a single [`PathOut`] to its `<path …></path>` form.
///
/// Attribute order matches what roughjs' SVG path emits byte-for-byte,
/// which depends on the original drawing type:
///
/// * **stroke path** (type `path`) →
///   `d, stroke, stroke-width, fill, stroke-dasharray`.
/// * **rectangle / arc fill** (type `fillPath`, source shape NOT
///   polygon/curve) → `d, stroke, stroke-width, fill`.
/// * **polygon / curve / path fill** (type `fillPath`, source shape
///   IS polygon/curve) → `d, stroke, stroke-width, fill, fill-rule`.
pub fn path_out_to_svg(p: &PathOut) -> String {
    match (&p.stroke_dasharray, p.fill_rule_evenodd) {
        (Some(da), _) => {
            // Stroke path (has stroke-dasharray) — fill-rule never set here.
            format!(
                r#"<path d="{d}" stroke="{s}" stroke-width="{sw}" fill="{f}" stroke-dasharray="{da}"></path>"#,
                d = p.d,
                s = p.stroke,
                sw = fmt_num(p.stroke_width),
                f = p.fill,
                da = da,
            )
        }
        (None, true) => {
            // Polygon / curve / path fill — carries fill-rule.
            format!(
                r#"<path d="{d}" stroke="{s}" stroke-width="{sw}" fill="{f}" fill-rule="evenodd"></path>"#,
                d = p.d,
                s = p.stroke,
                sw = fmt_num(p.stroke_width),
                f = p.fill,
            )
        }
        (None, false) => {
            // Rectangle / arc fill — no dasharray, no fill-rule.
            format!(
                r#"<path d="{d}" stroke="{s}" stroke-width="{sw}" fill="{f}"></path>"#,
                d = p.d,
                s = p.stroke,
                sw = fmt_num(p.stroke_width),
                f = p.fill,
            )
        }
    }
}

// ── Tests — the byte-exact probe battery ──────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── PRNG math ──────────────────────────────────────────────────
    #[test]
    fn lcg_seed_1_matches_upstream_first_20() {
        let mut r = RoughRandom::new(1);
        let expected: [f64; 8] = [
            0.0000224779359996_f64,
            0.0850324486382306,
            0.6013282160274684,
            0.7143158619292080,
            0.7409711848013103,
            0.4200615440495312,
            0.7907928149215877,
            0.3599690799601376,
        ];
        for (i, e) in expected.iter().enumerate() {
            let got = r.next();
            assert!((got - e).abs() < 1e-15, "idx {i}: got {got} want {e}");
        }
    }

    #[test]
    fn lcg_seed_0_no_fallback_returns_zeros() {
        let mut r = RoughRandom::new(0);
        assert_eq!(r.next(), 0.0);
        assert_eq!(r.next(), 0.0);
    }

    #[test]
    fn lcg_seed_0_with_fallback_pulls_from_mulberry() {
        let mut r = RoughRandom::new(0).with_fallback(Mulberry32::new(1));
        let mut m = Mulberry32::new(1);
        // Consumed values should match.
        assert_eq!(r.next(), m.next());
        assert_eq!(r.next(), m.next());
    }

    #[test]
    fn mulberry32_mermaid_shim_first_values() {
        // Cross-check against the mermaid shim implementation: seed
        // 0x12345678, first few pulls.
        let mut m = Mulberry32::new(Mulberry32::MERMAID_SHIM_SEED);
        // Reference values generated by running the exact JS function
        // from tests/support/generate_ref.mjs in a Node shell:
        //   let s=0x12345678;
        //   function r(){ s=(s+0x6d2b79f5)|0; let t=s;
        //     t=Math.imul(t^(t>>>15),1|t);
        //     t=(t+Math.imul(t^(t>>>7),61|t))^t;
        //     return ((t^(t>>>14))>>>0)/4294967296; }
        //   [r(), r(), r(), r()]
        // → [0.10615200875326991, 0.941276284167543,
        //    0.9398706152569503, 0.2338848018553108]
        let out: Vec<f64> = (0..4).map(|_| m.next()).collect();
        let want = [
            0.10615200875326991,
            0.941276284167543,
            0.9398706152569503,
            0.2338848018553108,
        ];
        for (g, w) in out.iter().zip(want.iter()) {
            assert!((g - w).abs() < 1e-15, "got {g} want {w}");
        }
    }

    // ── Number formatting ─────────────────────────────────────────
    #[test]
    fn fmt_num_matches_js_to_string() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-50.01428957260195, "-50.01428957260195"),
            (-16.670180707703906, "-16.670180707703906"),
            (0.20000449558719993, "0.20000449558719993"),
            (-35.046925, "-35.046925"),
            (4.9286609375, "4.9286609375"),
            (83.3583984375, "83.3583984375"),
        ];
        for (v, s) in cases {
            assert_eq!(&fmt_num(*v), s, "{v}");
        }
    }

    // ── Rectangle outline — the fixture-12 outer-path case ────────
    //
    // cypress/er/12 outer rect: x = -83.3583984375, y = -70.09375,
    //   w = 166.716796875, h = 140.1875
    // seed = 1 (handDrawnSeed), roughness = 0, fillStyle = "solid",
    // fill = mainBkg ("#ECECFF"), stroke = nodeBorder ("#9370DB"),
    // strokeWidth = 1.3, strokeLineDash = [0, 0].
    //
    // Reference fill path:
    //   M-83.3583984375 -70.09375 L83.3583984375 -70.09375
    //   L83.3583984375 70.09375 L-83.3583984375 70.09375
    // Reference stroke path:
    //   M-83.3583984375 -70.09375
    //     C-50.01428957260195 -70.09375,
    //      -16.670180707703906 -70.09375,
    //       83.3583984375 -70.09375
    //     M-83.3583984375 -70.09375
    //     C-46.45532004618162 -70.09375,
    //       -9.552241654863238 -70.09375,
    //        83.3583984375 -70.09375
    //     ...
    fn er12_options() -> RoughOptions {
        let mut o = RoughOptions::default();
        o.roughness = 0.0;
        o.fill_style = "solid".into();
        o.fill = Some("#ECECFF".into());
        o.stroke = "#9370DB".into();
        o.stroke_width = 1.3;
        o.seed = 1;
        o.stroke_line_dash = vec![0.0, 0.0];
        o.fill_line_dash = vec![0.0, 0.0];
        o
    }

    #[test]
    fn rectangle_fixture_er12_outer_byte_exact() {
        let o = er12_options();
        let mut rc = RoughGenerator::new();
        let d = rc.rectangle(-83.3583984375, -70.09375, 166.716796875, 140.1875, &o);
        let paths = to_paths(&d, &o);
        // Expect 2 paths: fill (FillPath) first, stroke (Path) second.
        assert_eq!(paths.len(), 2, "rectangle fill+stroke");
        let fill = &paths[0];
        let stroke = &paths[1];

        let want_fill = "M-83.3583984375 -70.09375 L83.3583984375 -70.09375 L83.3583984375 70.09375 L-83.3583984375 70.09375";
        assert_eq!(fill.d, want_fill);

        let want_stroke = "M-83.3583984375 -70.09375 C-50.01428957260195 -70.09375, -16.670180707703906 -70.09375, 83.3583984375 -70.09375 M-83.3583984375 -70.09375 C-46.45532004618162 -70.09375, -9.552241654863238 -70.09375, 83.3583984375 -70.09375 M83.3583984375 -70.09375 C83.3583984375 -39.60264543436351, 83.3583984375 -9.111540868727019, 83.3583984375 70.09375 M83.3583984375 -70.09375 C83.3583984375 -32.46802012564731, 83.3583984375 5.157709748705386, 83.3583984375 70.09375 M83.3583984375 70.09375 C47.40217398189689 70.09375, 11.445949526293774 70.09375, -83.3583984375 70.09375 M83.3583984375 70.09375 C20.68745670016233 70.09375, -41.98348503717534 70.09375, -83.3583984375 70.09375 M-83.3583984375 70.09375 C-83.3583984375 29.886668778507733, -83.3583984375 -10.320412442984534, -83.3583984375 -70.09375 M-83.3583984375 70.09375 C-83.3583984375 19.09826630631578, -83.3583984375 -31.897217387368443, -83.3583984375 -70.09375";
        assert_eq!(stroke.d, want_stroke);
    }

    // ── Divider polygon — the fixture-12 degenerate rect divider ─
    //
    // cypress/er/12 first divider: polygon of 4 points forming a
    //   1e-4-thick horizontal strip at y = -35.046875 ± 5e-5.
    //   Points come from `lineToPolygon(x, y1, x2, y1, thickness)`
    //   upstream — for a horizontal line:
    //     [{-83.3583984375, -35.046925}, {-83.3583984375, -35.046825},
    //      { 83.3583984375, -35.046825}, { 83.3583984375, -35.046925}]
    //   (the -y points first, +y second, mirrored).
    //
    // Reference fill path:
    //   M-83.3583984375 -35.046925 L-83.3583984375 -35.046825
    //   L83.3583984375 -35.046825 L83.3583984375 -35.046925
    #[test]
    fn polygon_fixture_er12_divider_byte_exact() {
        let o = er12_options();
        let pts = [
            (-83.3583984375, -35.046925),
            (-83.3583984375, -35.046825),
            (83.3583984375, -35.046825),
            (83.3583984375, -35.046925),
        ];
        let mut rc = RoughGenerator::new();
        let d = rc.polygon(&pts, &o);
        let paths = to_paths(&d, &o);
        assert_eq!(paths.len(), 2);
        let fill = &paths[0];
        let stroke = &paths[1];

        let want_fill = "M-83.3583984375 -35.046925 L-83.3583984375 -35.046825 L83.3583984375 -35.046825 L83.3583984375 -35.046925";
        assert_eq!(fill.d, want_fill);

        let want_stroke = "M-83.3583984375 -35.046925 C-83.3583984375 -35.04690499955044, -83.3583984375 -35.04688499910088, -83.3583984375 -35.046825 M-83.3583984375 -35.046925 C-83.3583984375 -35.04690286481082, -83.3583984375 -35.04688072962163, -83.3583984375 -35.046825 M-83.3583984375 -35.046825 C-47.097110616805544 -35.046825, -10.835822796111088 -35.046825, 83.3583984375 -35.046825 M-83.3583984375 -35.046825 C-38.612317904384874 -35.046825, 6.133762628730253 -35.046825, 83.3583984375 -35.046825 M83.3583984375 -35.046825 C83.3583984375 -35.04684656724765, 83.3583984375 -35.0468681344953, 83.3583984375 -35.046925 M83.3583984375 -35.046825 C83.3583984375 -35.0468625912583, 83.3583984375 -35.046900182516595, 83.3583984375 -35.046925 M83.3583984375 -35.046925 C35.54246768090506 -35.046925, -12.273463075689875 -35.046925, -83.3583984375 -35.046925 M83.3583984375 -35.046925 C22.712451427229276 -35.046925, -37.93349558304145 -35.046925, -83.3583984375 -35.046925";
        assert_eq!(stroke.d, want_stroke);
    }

    #[test]
    fn polygon_fixture_er12_vertical_divider_byte_exact() {
        // Second divider — vertical, x-centred at 4.9287109375 (approx),
        // y from -35.046875 to 70.09375. Fixture gives fill:
        //   M4.9286609375 -35.046875 L4.9287609375 -35.046875
        //   L4.9287609375 70.09375 L4.9286609375 70.09375
        let o = er12_options();
        let pts = [
            (4.9286609375, -35.046875),
            (4.9287609375, -35.046875),
            (4.9287609375, 70.09375),
            (4.9286609375, 70.09375),
        ];
        let mut rc = RoughGenerator::new();
        let d = rc.polygon(&pts, &o);
        let paths = to_paths(&d, &o);
        assert_eq!(paths.len(), 2);
        let fill = &paths[0];
        let stroke = &paths[1];

        assert_eq!(
            fill.d,
            "M4.9286609375 -35.046875 L4.9287609375 -35.046875 L4.9287609375 70.09375 L4.9286609375 70.09375"
        );

        let want_stroke = "M4.9286609375 -35.046875 C4.928680937949559 -35.046875, 4.928700938399118 -35.046875, 4.9287609375 -35.046875 M4.9286609375 -35.046875 C4.928683072689185 -35.046875, 4.9287052078783695 -35.046875, 4.9287609375 -35.046875 M4.9287609375 -35.046875 C4.9287609375 -12.178546575772632, 4.9287609375 10.689781848454736, 4.9287609375 70.09375 M4.9287609375 -35.046875 C4.9287609375 -6.827577594235478, 4.9287609375 21.391719811529043, 4.9287609375 70.09375 M4.9287609375 70.09375 C4.92873937025235 70.09375, 4.928717803004701 70.09375, 4.9286609375 70.09375 M4.9287609375 70.09375 C4.9287233462417035 70.09375, 4.928685754983406 70.09375, 4.9286609375 70.09375 M4.9286609375 70.09375 C4.9286609375 39.938439083880795, 4.9286609375 9.783128167761596, 4.9286609375 -35.046875 M4.9286609375 70.09375 C4.9286609375 31.84713722973683, 4.9286609375 -6.399475540526339, 4.9286609375 -35.046875";
        assert_eq!(stroke.d, want_stroke);
    }

    // ── Path element serialisation ────────────────────────────────
    #[test]
    fn path_out_to_svg_fill_attr_order_rectangle() {
        // Rectangle fill: no fill-rule (rough.js' svg.js only emits
        // fill-rule for polygon/curve drawables).
        let o = er12_options();
        let mut rc = RoughGenerator::new();
        let d = rc.rectangle(-10.0, -10.0, 20.0, 20.0, &o);
        let paths = to_paths(&d, &o);
        let fill_svg = path_out_to_svg(&paths[0]);
        assert!(fill_svg.starts_with("<path d=\""));
        assert!(fill_svg.contains("stroke=\"none\""));
        assert!(fill_svg.contains("stroke-width=\"0\""));
        assert!(fill_svg.contains("fill=\"#ECECFF\""));
        assert!(!fill_svg.contains("fill-rule"));
    }

    #[test]
    fn path_out_to_svg_fill_attr_order_polygon() {
        // Polygon fill: DOES emit fill-rule="evenodd".
        let o = er12_options();
        let mut rc = RoughGenerator::new();
        let d = rc.polygon(
            &[(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)],
            &o,
        );
        let paths = to_paths(&d, &o);
        let fill_svg = path_out_to_svg(&paths[0]);
        assert!(fill_svg.contains("fill-rule=\"evenodd\""));
    }

    #[test]
    fn path_out_to_svg_stroke_attr_order() {
        let o = er12_options();
        let mut rc = RoughGenerator::new();
        let d = rc.rectangle(-10.0, -10.0, 20.0, 20.0, &o);
        let paths = to_paths(&d, &o);
        let stroke_svg = path_out_to_svg(&paths[1]);
        assert!(stroke_svg.contains("stroke=\"#9370DB\""));
        assert!(stroke_svg.contains("stroke-width=\"1.3\""));
        assert!(stroke_svg.contains("fill=\"none\""));
        assert!(stroke_svg.contains("stroke-dasharray=\"0 0\""));
    }

    // ── Generator state isolation ─────────────────────────────────
    #[test]
    fn rng_state_is_per_call_not_shared() {
        // Each call to rc.rectangle builds a fresh RoughRandom from
        // the option bag's seed — two consecutive calls with the same
        // options must produce identical output.
        let o = er12_options();
        let mut rc = RoughGenerator::new();
        let d1 = rc.rectangle(-10.0, -10.0, 20.0, 20.0, &o);
        let d2 = rc.rectangle(-10.0, -10.0, 20.0, 20.0, &o);
        let p1 = to_paths(&d1, &o);
        let p2 = to_paths(&d2, &o);
        assert_eq!(p1.len(), p2.len());
        for (a, b) in p1.iter().zip(p2.iter()) {
            assert_eq!(a.d, b.d);
        }
    }

    // ── Simple rectangle without fill — single outline path ──────
    #[test]
    fn rectangle_no_fill_emits_single_stroke_path() {
        let mut o = RoughOptions::default();
        o.roughness = 0.0;
        o.fill = None;
        o.seed = 1;
        let mut rc = RoughGenerator::new();
        let d = rc.rectangle(0.0, 0.0, 10.0, 10.0, &o);
        let paths = to_paths(&d, &o);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].fill, "none");
    }

    // ── Preserve vertices: move/end points align exactly ─────────
    #[test]
    fn preserve_vertices_keeps_exact_endpoints() {
        let mut o = RoughOptions::default();
        o.roughness = 0.0; // zero offsets already exact, but belt-and-braces
        o.preserve_vertices = true;
        o.seed = 42;
        let mut rc = RoughGenerator::new();
        let d = rc.line(0.0, 0.0, 100.0, 0.0, &o);
        let paths = to_paths(&d, &o);
        // First op in the first (and only) set must be a Move at (0,0).
        match &d.sets[0].ops[0] {
            Op::Move(x, y) => {
                assert_eq!(*x, 0.0);
                assert_eq!(*y, 0.0);
            }
            _ => panic!("expected Move first"),
        }
        // Last bcurve's endpoint must be (100, 0).
        let last = d.sets[0].ops.last().unwrap();
        match last {
            Op::BCurveTo(_, _, _, _, ex, ey) => {
                assert_eq!(*ex, 100.0);
                assert_eq!(*ey, 0.0);
            }
            _ => panic!("expected BCurveTo last"),
        }
        // And the path emitted string contains the exact endpoints.
        assert!(paths[0].d.contains("100 0"));
    }

    // ── Regression: ensure double-line consumes exactly 11 RNG
    //   pulls per half (non-overlay, move, no-preserve). ───────────
    #[test]
    fn line_ops_non_overlay_consumes_11_rng_pulls() {
        let mut o = RoughOptions::default();
        o.seed = 1;
        o.disable_multi_stroke = true; // take only the non-overlay half
        let mut rng = RoughRandom::new(1);
        let _ = line_ops(0.0, 0.0, 100.0, 0.0, &o, true, false, &mut rng);
        // After 11 pulls from seed=1, seed should equal the 11th LCG
        // iterate (computed in the lcg_seed_1 test above).
        // Quickest check: compare next() to the 12th reference value.
        // 12th r-value in our reference table (0-indexed 11): r12.
        let expect_r12 = 0.1067594592459500_f64;
        assert!((rng.next() - expect_r12).abs() < 1e-15);
    }
}
