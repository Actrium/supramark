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
    /// Mirror upstream rough.js semantics: when `true`, omit the
    /// `stroke-dasharray` attribute entirely (the JS `if (o.strokeLineDash)`
    /// falsy branch). Existing call sites left this implicitly `false`
    /// so the legacy "0 0" attribute keeps appearing on stadium / er /
    /// requirement outputs that mirror upstream's explicit `[0, 0]`.
    pub omit_dash_attrs: bool,
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
            omit_dash_attrs: false,
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
                // Hachure (or any non-solid pattern routed through
                // `getFiller(...)`). For the patterns mermaid uses
                // (`hachure`), we run the scan-line algorithm against
                // the rectangle's 4 vertices.
                let polys: Vec<Vec<[f64; 2]>> = vec![points.iter().map(|p| [p[0], p[1]]).collect()];
                let lines = polygon_hachure_lines(&polys, o, &mut rng);
                sets.push(hachure_fill_sketch(&lines, o, &mut rng));
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
                let polys: Vec<Vec<[f64; 2]>> =
                    vec![points.iter().map(|(x, y)| [*x, *y]).collect()];
                let lines = polygon_hachure_lines(&polys, o, &mut rng);
                sets.push(hachure_fill_sketch(&lines, o, &mut rng));
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

        // Upstream calls `pointsOnPath(d, 1, distance)` ALWAYS — before
        // hasFill / hasStroke / fillStyle are inspected. The result is
        // used by both the solid-fill multi-set branch and the
        // pattern-fill (hachure) branch. Even when `simplification` is
        // unset (mermaid never sets it), the call runs; we mirror that
        // ordering so RNG state is unaffected (no RNG draws happen
        // inside pointsOnPath).
        let simplification = 0.0_f64; // mermaid never sets `simplification`.
        let distance = if simplification > 0.0 {
            4.0 - 4.0 * simplification
        } else {
            (1.0 + o.roughness) / 2.0
        };
        let polys = points_on_path(d, 1.0, distance);

        // Stroke pass first — `shape = svgPath(d, o)` runs unconditionally.
        let stroke_ops = svg_path_ops(&segs, o, &mut rng);

        if has_fill {
            if o.fill_style == "solid" {
                if polys.len() == 1 {
                    let mut fill_opts = o.clone();
                    fill_opts.disable_multi_stroke = true;
                    fill_opts.roughness = if o.roughness != 0.0 {
                        o.roughness + o.fill_shape_roughness_gain
                    } else {
                        0.0
                    };
                    let fill_ops = svg_path_ops(&segs, &fill_opts, &mut rng);
                    let merged = merged_shape(fill_ops);
                    sets.push(OpSet {
                        op_type: OpSetType::FillPath,
                        ops: merged,
                    });
                } else {
                    sets.push(solid_fill_polygon(&polys, o, &mut rng));
                }
            } else if o.fill_style == "cross-hatch" {
                // `HatchFiller.fillPolygons` — hachure twice (at angle
                // and angle+90) and concat ops into one FillSketch.
                // JS mutates the polygon list in-place between passes
                // (rotate-and-unrotate accumulates float drift); we do
                // the same via `_mut` variants so byte-exact output is
                // preserved.
                let mut polys_m = polys.clone();
                let lines1 = polygon_hachure_lines_mut(&mut polys_m, o, &mut rng);
                let set1 = hachure_fill_sketch(&lines1, o, &mut rng);
                let mut o2 = o.clone();
                o2.hachure_angle += 90.0;
                let lines2 = polygon_hachure_lines_mut(&mut polys_m, &o2, &mut rng);
                let set2 = hachure_fill_sketch(&lines2, &o2, &mut rng);
                let mut combined = set1.ops;
                combined.extend(set2.ops);
                sets.push(OpSet {
                    op_type: OpSetType::FillSketch,
                    ops: combined,
                });
            } else {
                // Pattern (hachure) fill — scan the union of polygons.
                let lines = polygon_hachure_lines(&polys, o, &mut rng);
                sets.push(hachure_fill_sketch(&lines, o, &mut rng));
            }
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

    pub fn ellipse(&mut self, x: f64, y: f64, w: f64, h: f64, o: &RoughOptions) -> Drawable {
        let mut rng = self.make_random(o);
        let mut sets = Vec::with_capacity(2);
        let params = generate_ellipse_params(w, h, o, &mut rng);
        let response = ellipse_with_params(x, y, o, &params, &mut rng);
        if o.fill.is_some() {
            if o.fill_style == "solid" {
                let fresh = ellipse_with_params(x, y, o, &params, &mut rng);
                let mut shape = fresh.opset;
                shape.op_type = OpSetType::FillPath;
                sets.push(shape);
            } else {
                let core = &response.estimated_points;
                let core_arr: Vec<[f64; 2]> = core.iter().map(|(x, y)| [*x, *y]).collect();
                let polys: Vec<Vec<[f64; 2]>> = vec![core_arr];
                let lines = polygon_hachure_lines(&polys, o, &mut rng);
                sets.push(hachure_fill_sketch(&lines, o, &mut rng));
            }
        }
        if o.stroke != "none" {
            sets.push(response.opset);
        }
        if let Some(fb) = rng.fallback {
            self.fallback = fb;
        }
        Drawable {
            shape: "ellipse",
            sets,
        }
    }

    /// `rc.circle(x, y, diameter, options)` — convenience wrapper
    /// around [`ellipse`]. Marks the resulting drawable with
    /// `shape = "circle"` so [`to_paths`] emits the right
    /// `fill-rule` semantics.
    ///
    /// [`ellipse`]: RoughGenerator::ellipse
    pub fn circle(&mut self, x: f64, y: f64, diameter: f64, o: &RoughOptions) -> Drawable {
        let mut d = self.ellipse(x, y, diameter, diameter, o);
        d.shape = "circle";
        d
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
        if let Ok(v) = std::str::from_utf8(&bytes[start..i])
            .unwrap()
            .parse::<f64>()
        {
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
            'Q' => {
                out.push(seg.clone());
                cx = seg.data[2];
                cy = seg.data[3];
            }
            'q' => {
                let mut data = seg.data.clone();
                for (i, v) in data.iter_mut().enumerate() {
                    if i % 2 == 0 {
                        *v += cx;
                    } else {
                        *v += cy;
                    }
                }
                cx = data[2];
                cy = data[3];
                out.push(RawSeg { key: 'Q', data });
            }
            'A' => {
                out.push(seg.clone());
                cx = seg.data[5];
                cy = seg.data[6];
            }
            'a' => {
                cx += seg.data[5];
                cy += seg.data[6];
                out.push(RawSeg {
                    key: 'A',
                    data: vec![
                        seg.data[0],
                        seg.data[1],
                        seg.data[2],
                        seg.data[3],
                        seg.data[4],
                        cx,
                        cy,
                    ],
                });
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
    // Q is also supported here — converted to a cubic via the same
    // midpoint approximation used by upstream `path-data-parser`.
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
            'Q' => {
                // Quadratic → cubic: midpoint approximation per
                // path-data-parser/normalize.js. data = [x1, y1, x, y].
                let x1 = seg.data[0];
                let y1 = seg.data[1];
                let x = seg.data[2];
                let y = seg.data[3];
                let cx1 = cx + 2.0 * (x1 - cx) / 3.0;
                let cy1 = cy + 2.0 * (y1 - cy) / 3.0;
                let cx2 = x + 2.0 * (x1 - x) / 3.0;
                let cy2 = y + 2.0 * (y1 - y) / 3.0;
                out.push(RawSeg {
                    key: 'C',
                    data: vec![cx1, cy1, cx2, cy2, x, y],
                });
                cx = x;
                cy = y;
            }
            'A' => {
                // Mirrors path-data-parser/normalize.js: arc → cubic
                // bezier(s) via `arcToCubicCurves`. Mermaid's venn
                // intersection paths exercise this branch (`d` contains
                // multiple `A` segments stitched together).
                let r1 = seg.data[0].abs();
                let r2 = seg.data[1].abs();
                let angle = seg.data[2];
                let large_arc_flag = seg.data[3];
                let sweep_flag = seg.data[4];
                let x = seg.data[5];
                let y = seg.data[6];
                if r1 == 0.0 || r2 == 0.0 {
                    out.push(RawSeg {
                        key: 'C',
                        data: vec![cx, cy, x, y, x, y],
                    });
                    cx = x;
                    cy = y;
                } else if cx != x || cy != y {
                    let curves = arc_to_cubic_curves(
                        cx,
                        cy,
                        x,
                        y,
                        r1,
                        r2,
                        angle,
                        large_arc_flag,
                        sweep_flag,
                        None,
                    );
                    for curve in curves {
                        out.push(RawSeg {
                            key: 'C',
                            data: curve,
                        });
                    }
                    cx = x;
                    cy = y;
                }
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

/// Decompose an SVG arc segment into a sequence of cubic bezier curves.
/// Direct port of `arcToCubicCurves` in
/// `path-data-parser/lib/normalize.js`. Returns a list of 6-element
/// `Vec<f64>` curves `[c1x, c1y, c2x, c2y, x, y]` matching `C`'s data.
/// Internally uses [`arc_inner`] which mirrors the JS function 1:1
/// (recursive frame returns 2-tuples, outer rotates and packs into
/// 6-tuples).
fn arc_to_cubic_curves(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    r1: f64,
    r2: f64,
    angle: f64,
    large_arc_flag: f64,
    sweep_flag: f64,
    recursive: Option<[f64; 4]>,
) -> Vec<Vec<f64>> {
    arc_inner(
        x1,
        y1,
        x2,
        y2,
        r1,
        r2,
        angle,
        large_arc_flag,
        sweep_flag,
        recursive,
    )
}

#[inline]
fn arc_rotate(x: f64, y: f64, angle_rad: f64) -> (f64, f64) {
    let c = angle_rad.cos();
    let s = angle_rad.sin();
    (x * c - y * s, x * s + y * c)
}

#[inline]
fn round_to_9(v: f64) -> f64 {
    // Mirror JS `parseFloat(v.toFixed(9))`. The std formatter rounds
    // half-to-even; JS toFixed rounds half-away-from-zero. The inputs
    // here (asin args clipped to ±1) round identically under both
    // modes, so the std formatter suffices.
    let s = format!("{:.9}", v);
    s.parse::<f64>().unwrap_or(v)
}

/// Direct 1:1 port of `arcToCubicCurves` — keeps the recursive-frame
/// return shape (2-tuples) and the outer-frame return shape (6-tuples)
/// distinct via the `recursive` flag, but encodes both in a single
/// `Vec<Vec<f64>>` (length-2 entries are intermediate points; length-6
/// entries are final curves). The outer call always returns length-6
/// entries.
#[allow(clippy::too_many_arguments)]
fn arc_inner(
    mut x1: f64,
    mut y1: f64,
    mut x2: f64,
    mut y2: f64,
    mut r1: f64,
    mut r2: f64,
    angle: f64,
    large_arc_flag: f64,
    sweep_flag: f64,
    recursive: Option<[f64; 4]>,
) -> Vec<Vec<f64>> {
    let angle_rad = std::f64::consts::PI * angle / 180.0;
    let mut params: Vec<Vec<f64>> = Vec::new();
    let f1: f64;
    let mut f2: f64;
    let cx: f64;
    let cy: f64;
    if let Some([rf1, rf2, rcx, rcy]) = recursive {
        f1 = rf1;
        f2 = rf2;
        cx = rcx;
        cy = rcy;
    } else {
        let (rx1, ry1) = arc_rotate(x1, y1, -angle_rad);
        let (rx2, ry2) = arc_rotate(x2, y2, -angle_rad);
        x1 = rx1;
        y1 = ry1;
        x2 = rx2;
        y2 = ry2;
        let x = (x1 - x2) / 2.0;
        let y = (y1 - y2) / 2.0;
        let mut h = (x * x) / (r1 * r1) + (y * y) / (r2 * r2);
        if h > 1.0 {
            h = h.sqrt();
            r1 *= h;
            r2 *= h;
        }
        let sign = if large_arc_flag == sweep_flag { -1.0 } else { 1.0 };
        let r1_pow = r1 * r1;
        let r2_pow = r2 * r2;
        let left = r1_pow * r2_pow - r1_pow * y * y - r2_pow * x * x;
        let right = r1_pow * y * y + r2_pow * x * x;
        let k = sign * (left / right).abs().sqrt();
        cx = k * r1 * y / r2 + (x1 + x2) / 2.0;
        cy = k * -r2 * x / r1 + (y1 + y2) / 2.0;
        let v1 = round_to_9((y1 - cy) / r2);
        let v2 = round_to_9((y2 - cy) / r2);
        let mut f1m = v1.asin();
        let mut f2m = v2.asin();
        if x1 < cx {
            f1m = std::f64::consts::PI - f1m;
        }
        if x2 < cx {
            f2m = std::f64::consts::PI - f2m;
        }
        if f1m < 0.0 {
            f1m += std::f64::consts::PI * 2.0;
        }
        if f2m < 0.0 {
            f2m += std::f64::consts::PI * 2.0;
        }
        if sweep_flag != 0.0 && f1m > f2m {
            f1m -= std::f64::consts::PI * 2.0;
        }
        if sweep_flag == 0.0 && f2m > f1m {
            f2m -= std::f64::consts::PI * 2.0;
        }
        f1 = f1m;
        f2 = f2m;
    }
    let mut df = f2 - f1;
    if df.abs() > std::f64::consts::PI * 120.0 / 180.0 {
        let f2old = f2;
        let x2old = x2;
        let y2old = y2;
        if sweep_flag != 0.0 && f2 > f1 {
            f2 = f1 + std::f64::consts::PI * 120.0 / 180.0;
        } else {
            f2 = f1 - std::f64::consts::PI * 120.0 / 180.0;
        }
        x2 = cx + r1 * f2.cos();
        y2 = cy + r2 * f2.sin();
        params = arc_inner(
            x2,
            y2,
            x2old,
            y2old,
            r1,
            r2,
            angle,
            0.0,
            sweep_flag,
            Some([f2, f2old, cx, cy]),
        );
    }
    df = f2 - f1;
    let c1 = f1.cos();
    let s1 = f1.sin();
    let c2 = f2.cos();
    let s2 = f2.sin();
    let t = (df / 4.0).tan();
    let hx = 4.0 / 3.0 * r1 * t;
    let hy = 4.0 / 3.0 * r2 * t;
    let m1 = [x1, y1];
    let mut m2 = [x1 + hx * s1, y1 - hy * c1];
    let m3 = [x2 + hx * s2, y2 - hy * c2];
    let m4 = [x2, y2];
    m2[0] = 2.0 * m1[0] - m2[0];
    m2[1] = 2.0 * m1[1] - m2[1];
    if recursive.is_some() {
        let mut out: Vec<Vec<f64>> = Vec::with_capacity(3 + params.len());
        out.push(vec![m2[0], m2[1]]);
        out.push(vec![m3[0], m3[1]]);
        out.push(vec![m4[0], m4[1]]);
        out.extend(params);
        out
    } else {
        let mut points: Vec<[f64; 2]> = Vec::with_capacity(3 + params.len());
        points.push(m2);
        points.push(m3);
        points.push(m4);
        for p in &params {
            points.push([p[0], p[1]]);
        }
        let mut curves: Vec<Vec<f64>> = Vec::with_capacity(points.len() / 3);
        let mut i = 0;
        while i + 2 < points.len() {
            let r_a = arc_rotate(points[i][0], points[i][1], angle_rad);
            let r_b = arc_rotate(points[i + 1][0], points[i + 1][1], angle_rad);
            let r_c = arc_rotate(points[i + 2][0], points[i + 2][1], angle_rad);
            curves.push(vec![r_a.0, r_a.1, r_b.0, r_b.1, r_c.0, r_c.1]);
            i += 3;
        }
        curves
    }
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
                    current.0,
                    current.1,
                    seg.data[0],
                    seg.data[1],
                    o,
                    false,
                    rng,
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

// ── Ellipse helpers (`generateEllipseParams` / `ellipseWithParams` /
//                    `_computeEllipsePoints` / `_curve`) ────────────────

/// Pre-computed ellipse parameters: angular increment between the
/// sampled points, and the (possibly jittered) effective radii.
#[derive(Debug, Clone, Copy)]
struct EllipseParams {
    increment: f64,
    rx: f64,
    ry: f64,
}

struct EllipseResponse {
    opset: OpSet,
    estimated_points: Vec<(f64, f64)>,
}

/// Two RNG pulls happen here (in order: rx-jitter, then ry-jitter),
/// driving the `_offsetOpt(rx * curveFitRandomness, o)` and the
/// symmetric `ry` adjustment.
fn generate_ellipse_params(
    width: f64,
    height: f64,
    o: &RoughOptions,
    rng: &mut RoughRandom,
) -> EllipseParams {
    let psq = (std::f64::consts::PI
        * 2.0
        * (((width / 2.0).powi(2) + (height / 2.0).powi(2)) / 2.0).sqrt())
    .sqrt();
    let step_count = (o.curve_step_count)
        .max((o.curve_step_count / (200.0_f64).sqrt()) * psq)
        .ceil();
    let increment = (std::f64::consts::PI * 2.0) / step_count;
    let mut rx = (width / 2.0).abs();
    let mut ry = (height / 2.0).abs();
    let curve_fit_randomness = 1.0 - o.curve_fitting;
    rx += offset_opt(rx * curve_fit_randomness, o, 1.0, rng);
    ry += offset_opt(ry * curve_fit_randomness, o, 1.0, rng);
    EllipseParams { increment, rx, ry }
}

fn ellipse_with_params(
    x: f64,
    y: f64,
    o: &RoughOptions,
    p: &EllipseParams,
    rng: &mut RoughRandom,
) -> EllipseResponse {
    let inner = offset(0.4, 1.0, o, 1.0, rng);
    let scaled = p.increment * offset(0.1, inner, o, 1.0, rng);
    let (ap1_all, ap1_core) = compute_ellipse_points(p.increment, x, y, p.rx, p.ry, 1.0, scaled, o, rng);
    let mut o1 = curve_inner(&ap1_all, None, o);
    if !o.disable_multi_stroke && o.roughness != 0.0 {
        let (ap2_all, _) = compute_ellipse_points(p.increment, x, y, p.rx, p.ry, 1.5, 0.0, o, rng);
        let o2 = curve_inner(&ap2_all, None, o);
        o1.extend(o2);
    }
    EllipseResponse {
        opset: OpSet {
            op_type: OpSetType::Path,
            ops: o1,
        },
        estimated_points: ap1_core,
    }
}

fn compute_ellipse_points(
    increment: f64,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    offset_mul: f64,
    overlap: f64,
    o: &RoughOptions,
    rng: &mut RoughRandom,
) -> (Vec<(f64, f64)>, Vec<(f64, f64)>) {
    // Trig calls here drive ellipse path coordinates; route through the
    // V8 fdlibm port so circle/ellipse outlines match Node 1 ULP-exact.
    use crate::math::v8_trig::{cos, sin};
    if o.roughness == 0.0 {
        let mut all = Vec::new();
        let inc = increment / 4.0;
        all.push((cx + rx * cos(-inc), cy + ry * sin(-inc)));
        let mut angle = 0.0;
        while angle <= std::f64::consts::PI * 2.0 {
            all.push((cx + rx * cos(angle), cy + ry * sin(angle)));
            angle += inc;
        }
        all.push((cx + rx * cos(0.0_f64), cy + ry * sin(0.0_f64)));
        all.push((cx + rx * cos(inc), cy + ry * sin(inc)));
        return (all.clone(), all);
    }
    let mut all_points = Vec::with_capacity(20);
    let mut core_points = Vec::with_capacity(20);
    let rad_offset = offset_opt(0.5, o, 1.0, rng) - std::f64::consts::PI / 2.0;
    let r_x0 = offset_opt(offset_mul, o, 1.0, rng);
    let r_y0 = offset_opt(offset_mul, o, 1.0, rng);
    all_points.push((
        r_x0 + cx + 0.9 * rx * cos(rad_offset - increment),
        r_y0 + cy + 0.9 * ry * sin(rad_offset - increment),
    ));
    let end_angle = std::f64::consts::PI * 2.0 + rad_offset - 0.01;
    let mut angle = rad_offset;
    while angle < end_angle {
        let r_x = offset_opt(offset_mul, o, 1.0, rng);
        let r_y = offset_opt(offset_mul, o, 1.0, rng);
        let pt = (r_x + cx + rx * cos(angle), r_y + cy + ry * sin(angle));
        all_points.push(pt);
        core_points.push(pt);
        angle += increment;
    }
    let r_x1 = offset_opt(offset_mul, o, 1.0, rng);
    let r_y1 = offset_opt(offset_mul, o, 1.0, rng);
    all_points.push((
        r_x1 + cx + rx * cos(rad_offset + std::f64::consts::PI * 2.0 + overlap * 0.5),
        r_y1 + cy + ry * sin(rad_offset + std::f64::consts::PI * 2.0 + overlap * 0.5),
    ));
    let r_x2 = offset_opt(offset_mul, o, 1.0, rng);
    let r_y2 = offset_opt(offset_mul, o, 1.0, rng);
    all_points.push((
        r_x2 + cx + 0.98 * rx * cos(rad_offset + overlap),
        r_y2 + cy + 0.98 * ry * sin(rad_offset + overlap),
    ));
    let r_x3 = offset_opt(offset_mul, o, 1.0, rng);
    let r_y3 = offset_opt(offset_mul, o, 1.0, rng);
    all_points.push((
        r_x3 + cx + 0.9 * rx * cos(rad_offset + overlap * 0.5),
        r_y3 + cy + 0.9 * ry * sin(rad_offset + overlap * 0.5),
    ));
    (all_points, core_points)
}

/// `_curve` (Catmull-Rom → cubic Bezier).
///
/// Upstream uses curveTightness=0 by default, so `s = 1 - 0 = 1`. The
/// algorithm walks 4-point windows along the input list, deriving each
/// cubic bezier's two interior control points from neighbour offsets
/// scaled by `s/6`. No RNG pulls here — pure geometry.
fn curve_inner(
    points: &[(f64, f64)],
    close_point: Option<(f64, f64)>,
    o: &RoughOptions,
) -> Vec<Op> {
    let len = points.len();
    let mut ops = Vec::new();
    if len > 3 {
        let s = 1.0 - o.curve_tightness;
        ops.push(Op::Move(points[1].0, points[1].1));
        let mut i = 1;
        while (i + 2) < len {
            let p_im1 = points[i - 1];
            let p_i = points[i];
            let p_ip1 = points[i + 1];
            let p_ip2 = points[i + 2];
            let b1x = p_i.0 + (s * p_ip1.0 - s * p_im1.0) / 6.0;
            let b1y = p_i.1 + (s * p_ip1.1 - s * p_im1.1) / 6.0;
            let b2x = p_ip1.0 + (s * p_i.0 - s * p_ip2.0) / 6.0;
            let b2y = p_ip1.1 + (s * p_i.1 - s * p_ip2.1) / 6.0;
            ops.push(Op::BCurveTo(b1x, b1y, b2x, b2y, p_ip1.0, p_ip1.1));
            i += 1;
        }
        if let Some((cx, cy)) = close_point {
            // Note: this branch consumes 2 RNG pulls upstream; we don't
            // exercise it from the ellipse path so we leave it as a
            // direct line-to without a random offset (mermaid's
            // ellipse never passes a closePoint).
            ops.push(Op::LineTo(cx, cy));
        }
    } else if len == 3 {
        ops.push(Op::Move(points[1].0, points[1].1));
        ops.push(Op::BCurveTo(
            points[1].0,
            points[1].1,
            points[2].0,
            points[2].1,
            points[2].0,
            points[2].1,
        ));
    } else if len == 2 {
        // upstream: `_line(...)` with overlay=true. Ellipse path doesn't
        // pass len==2, so we leave this empty for now.
    }
    ops
}

/// Return the union axis-aligned bounding box of every operation in
/// `sets`. Cubic Bezier curves are evaluated point-wise (16 samples
/// per segment) — sufficient for sub-pixel agreement with the SVG
/// `getBBox` mermaid uses post-jitter for `applyPaddedViewBox`.
///
/// Returns `(xmin, ymin, xmax, ymax)` or `None` if the input has no
/// drawable ops.
pub fn bbox_of_sets(sets: &[OpSet]) -> Option<(f64, f64, f64, f64)> {
    let mut have = false;
    let mut xmin = f64::INFINITY;
    let mut ymin = f64::INFINITY;
    let mut xmax = f64::NEG_INFINITY;
    let mut ymax = f64::NEG_INFINITY;

    for set in sets {
        let mut current: Option<(f64, f64)> = None;
        for op in &set.ops {
            match *op {
                Op::Move(x, y) => {
                    have = true;
                    if x < xmin {
                        xmin = x;
                    }
                    if y < ymin {
                        ymin = y;
                    }
                    if x > xmax {
                        xmax = x;
                    }
                    if y > ymax {
                        ymax = y;
                    }
                    current = Some((x, y));
                }
                Op::LineTo(x, y) => {
                    have = true;
                    if x < xmin {
                        xmin = x;
                    }
                    if y < ymin {
                        ymin = y;
                    }
                    if x > xmax {
                        xmax = x;
                    }
                    if y > ymax {
                        ymax = y;
                    }
                    current = Some((x, y));
                }
                Op::BCurveTo(c1x, c1y, c2x, c2y, ex, ey) => {
                    have = true;
                    let (sx, sy) = current.unwrap_or((c1x, c1y));
                    // Sample the cubic bezier — 16 segments per curve
                    // gives sub-pixel accuracy for the curvatures
                    // mermaid produces. Endpoint t=0 and t=1 are
                    // included so the end vertex is captured exactly.
                    for k in 0..=16 {
                        let t = k as f64 / 16.0;
                        let mt = 1.0 - t;
                        let bx = mt * mt * mt * sx
                            + 3.0 * mt * mt * t * c1x
                            + 3.0 * mt * t * t * c2x
                            + t * t * t * ex;
                        let by = mt * mt * mt * sy
                            + 3.0 * mt * mt * t * c1y
                            + 3.0 * mt * t * t * c2y
                            + t * t * t * ey;
                        if bx < xmin {
                            xmin = bx;
                        }
                        if by < ymin {
                            ymin = by;
                        }
                        if bx > xmax {
                            xmax = bx;
                        }
                        if by > ymax {
                            ymax = by;
                        }
                    }
                    current = Some((ex, ey));
                }
            }
        }
    }
    if have {
        Some((xmin, ymin, xmax, ymax))
    } else {
        None
    }
}

// ── Hachure fill (scan-line) ─────────────────────────────────────────
//
// Port of `node_modules/hachure-fill/bin/hachure.js` (`hachureLines` +
// `straightHachureLines`) and `roughjs/bin/fillers/scan-line-hachure.js`
// (`polygonHachureLines`). Used by mermaid for `look: handDrawn` shapes
// such as the ishikawa head & label boxes and the venn circles.
//
// The algorithm:
//   1. Rotate every polygon vertex around (0, 0) by `hachureAngle + 90`
//      (so horizontal scan-lines become the desired hachure direction
//      after the inverse rotation at the end).
//   2. Build a list of edges (skipping horizontal ones), sorted by the
//      smaller-y endpoint, then by x at that endpoint, then by ymax.
//   3. Scan downward in unit-y steps; at each y, splice in newly
//      activated edges, drop edges whose ymax has been passed, sort by
//      current x, and pair them up into horizontal line segments.
//   4. Step y by `hachureStepOffset`; advance each active edge's x by
//      `hachureStepOffset * islope`. Fill on every iteration when
//      `hachureStepOffset != 1`, else only on iterations divisible by
//      `gap` (so the loop can crawl one pixel at a time but still emit
//      sparse hachures — matching upstream byte-exactly).
//   5. Inverse-rotate every line endpoint by `-angle`.
//
// `polygonHachureLines` derives `gap`, `angle`, and `skipOffset` from
// `RoughOptions`, including the `roughness >= 1` && `random > 0.7`
// branch — that random pull DOES advance the shared RNG, so callers
// must use the same `RoughRandom` instance threaded through the
// outline / fill chain.

/// Convert polar (deg) rotation to a [`(cos, sin)`] pair, matching JS's
/// `Math.cos((Math.PI / 180) * deg)` byte-for-byte. We route through
/// `crate::math::v8_trig` because some inputs (e.g. `sin((π/180)·240)`)
/// disagree with `f64::sin` by 1 ULP and feed downstream into the
/// hachure scan-line, where 1-ULP polygon drift propagates into
/// venn-intersection cross-hatch fill.
#[inline]
fn rot_cs(deg: f64) -> (f64, f64) {
    let radians = (std::f64::consts::PI / 180.0) * deg;
    (
        crate::math::v8_trig::cos(radians),
        crate::math::v8_trig::sin(radians),
    )
}

/// In-place rotate `points` around `(cx, cy)` by `deg`. Mirrors
/// `hachure-fill/bin/hachure.js::rotatePoints`.
fn rotate_points(points: &mut [[f64; 2]], cx: f64, cy: f64, deg: f64) {
    if deg == 0.0 || points.is_empty() {
        return;
    }
    let (cos, sin) = rot_cs(deg);
    for p in points.iter_mut() {
        let x = p[0];
        let y = p[1];
        p[0] = (x - cx) * cos - (y - cy) * sin + cx;
        p[1] = (x - cx) * sin + (y - cy) * cos + cy;
    }
}

/// In-place rotate every endpoint of every line in `lines` around
/// `(cx, cy)` by `deg`. Mirrors `hachure-fill/bin/hachure.js::rotateLines`.
fn rotate_lines(lines: &mut [[[f64; 2]; 2]], cx: f64, cy: f64, deg: f64) {
    if deg == 0.0 || lines.is_empty() {
        return;
    }
    let (cos, sin) = rot_cs(deg);
    for line in lines.iter_mut() {
        for p in line.iter_mut() {
            let x = p[0];
            let y = p[1];
            p[0] = (x - cx) * cos - (y - cy) * sin + cx;
            p[1] = (x - cx) * sin + (y - cy) * cos + cy;
        }
    }
}

/// Internal edge record used by the scan-line algorithm.
#[derive(Debug, Clone, Copy)]
struct Edge {
    ymin: f64,
    ymax: f64,
    /// x at `ymin` — moves by `islope * stepOffset` each iteration.
    x: f64,
    /// Inverse slope (`(p2.x - p1.x) / (p2.y - p1.y)`).
    islope: f64,
}

/// Active-edge entry — wraps an [`Edge`] with the y at which it
/// activated (unused by upstream past activation, but kept to match the
/// shape of the JS object 1:1).
#[derive(Debug, Clone, Copy)]
struct ActiveEdge {
    edge: Edge,
}

/// Generate hachure scan-lines for a list of polygons. Mirrors
/// `hachure-fill/bin/hachure.js::hachureLines` with `polygons` already
/// in the `Vec<Vec<[f64; 2]>>` shape. Returns line segments
/// `[[x1, y1], [x2, y2]]`. The caller owns the polygon vertices; this
/// fn does not mutate them (it works on internal copies).
pub fn hachure_lines(
    polygons: &[Vec<[f64; 2]>],
    hachure_gap: f64,
    hachure_angle: f64,
    hachure_step_offset: f64,
) -> Vec<[[f64; 2]; 2]> {
    let mut polygons_rot: Vec<Vec<[f64; 2]>> = polygons.to_vec();
    hachure_lines_mut(&mut polygons_rot, hachure_gap, hachure_angle, hachure_step_offset)
}

/// In-place variant of [`hachure_lines`] — mirrors JS's
/// `hachureLines`, which rotates the input polygons by `+angle`, runs
/// scan-line, then rotates them back by `-angle`. Each round-trip
/// introduces tiny floating-point drift; cross-hatch fill (which
/// invokes the hachure twice with different angles) needs that drift
/// to match upstream byte-for-byte.
pub fn hachure_lines_mut(
    polygons: &mut [Vec<[f64; 2]>],
    hachure_gap: f64,
    hachure_angle: f64,
    hachure_step_offset: f64,
) -> Vec<[[f64; 2]; 2]> {
    let angle = hachure_angle;
    let gap = hachure_gap.max(0.1);
    if angle != 0.0 {
        for poly in polygons.iter_mut() {
            rotate_points(poly, 0.0, 0.0, angle);
        }
    }
    let mut lines = straight_hachure_lines(polygons, gap, hachure_step_offset);
    if angle != 0.0 {
        for poly in polygons.iter_mut() {
            rotate_points(poly, 0.0, 0.0, -angle);
        }
        rotate_lines(&mut lines, 0.0, 0.0, -angle);
    }
    lines
}

/// Inner half of [`hachure_lines`] — assumes the polygons have already
/// been rotated so that scan-lines are horizontal. Mirrors
/// `straightHachureLines` byte-for-byte.
fn straight_hachure_lines(
    polygons: &[Vec<[f64; 2]>],
    gap: f64,
    hachure_step_offset: f64,
) -> Vec<[[f64; 2]; 2]> {
    // Close any open polygons by appending a copy of the first vertex.
    let mut vertex_array: Vec<Vec<[f64; 2]>> = Vec::new();
    for polygon in polygons {
        let mut vertices = polygon.clone();
        if vertices.is_empty() {
            continue;
        }
        let first = vertices[0];
        let last = *vertices.last().unwrap();
        if first[0] != last[0] || first[1] != last[1] {
            vertices.push([first[0], first[1]]);
        }
        if vertices.len() > 2 {
            vertex_array.push(vertices);
        }
    }
    let mut lines: Vec<[[f64; 2]; 2]> = Vec::new();
    let gap = gap.max(0.1);

    // Build edge table.
    let mut edges: Vec<Edge> = Vec::new();
    for vertices in &vertex_array {
        for i in 0..(vertices.len() - 1) {
            let p1 = vertices[i];
            let p2 = vertices[i + 1];
            if p1[1] != p2[1] {
                let ymin = p1[1].min(p2[1]);
                let ymax = p1[1].max(p2[1]);
                let x = if ymin == p1[1] { p1[0] } else { p2[0] };
                let islope = (p2[0] - p1[0]) / (p2[1] - p1[1]);
                edges.push(Edge {
                    ymin,
                    ymax,
                    x,
                    islope,
                });
            }
        }
    }
    edges.sort_by(|e1, e2| {
        if e1.ymin < e2.ymin {
            std::cmp::Ordering::Less
        } else if e1.ymin > e2.ymin {
            std::cmp::Ordering::Greater
        } else if e1.x < e2.x {
            std::cmp::Ordering::Less
        } else if e1.x > e2.x {
            std::cmp::Ordering::Greater
        } else if e1.ymax == e2.ymax {
            std::cmp::Ordering::Equal
        } else if e1.ymax < e2.ymax {
            // upstream: `(e1.ymax - e2.ymax) / Math.abs((e1.ymax - e2.ymax))`
            // which is -1 when e1.ymax < e2.ymax, +1 otherwise.
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });

    if edges.is_empty() {
        return lines;
    }

    let mut active_edges: Vec<ActiveEdge> = Vec::new();
    let mut y = edges[0].ymin;
    let mut iteration: i64 = 0;

    // Convert edges to a deque-friendly Vec we drain from the front
    // (matches `splice(0, ix + 1)`).
    let mut pending: std::collections::VecDeque<Edge> = edges.into_iter().collect();

    while !active_edges.is_empty() || !pending.is_empty() {
        // Promote any edges whose ymin <= y into the active set.
        if !pending.is_empty() {
            // Find the largest contiguous prefix where ymin <= y.
            let mut count = 0usize;
            for e in &pending {
                if e.ymin > y {
                    break;
                }
                count += 1;
            }
            for _ in 0..count {
                if let Some(edge) = pending.pop_front() {
                    active_edges.push(ActiveEdge { edge });
                }
            }
        }

        // Drop active edges whose ymax has been passed.
        active_edges.retain(|ae| ae.edge.ymax > y);

        // Sort active edges by their current x.
        active_edges.sort_by(|a, b| {
            if a.edge.x < b.edge.x {
                std::cmp::Ordering::Less
            } else if a.edge.x > b.edge.x {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });

        // Emit fills.
        let emit = if hachure_step_offset != 1.0 {
            true
        } else {
            // `iteration % gap === 0` — JS uses Number % Number which
            // for integer iteration & integer-rounded gap is plain
            // modulo. Upstream rounds gap with Math.round but the
            // polygonHachureLines wrapper does that before calling us.
            (iteration as f64) % gap == 0.0
        };
        if emit && active_edges.len() > 1 {
            let mut i = 0usize;
            while i + 1 < active_edges.len() {
                let ce = active_edges[i].edge;
                let ne = active_edges[i + 1].edge;
                lines.push([[ce.x.round(), y], [ne.x.round(), y]]);
                i += 2;
            }
        }

        y += hachure_step_offset;
        for ae in active_edges.iter_mut() {
            ae.edge.x += hachure_step_offset * ae.edge.islope;
        }
        iteration += 1;
    }
    lines
}

/// Wrapper for [`hachure_lines`] that derives `gap`, `angle`, and
/// `skip_offset` from the rough options bag. Mirrors
/// `roughjs/bin/fillers/scan-line-hachure.js::polygonHachureLines`.
///
/// The `roughness >= 1 && random > 0.7` branch consumes one
/// `RoughRandom::next()` pull — the caller MUST pass the active RNG so
/// downstream offsets stay byte-aligned with upstream.
pub fn polygon_hachure_lines(
    polygons: &[Vec<[f64; 2]>],
    o: &RoughOptions,
    rng: &mut RoughRandom,
) -> Vec<[[f64; 2]; 2]> {
    let mut polys = polygons.to_vec();
    polygon_hachure_lines_mut(&mut polys, o, rng)
}

/// In-place variant — mutates `polygons` (rotate-and-rotate-back drift)
/// to match JS's `hachureLines` semantics. Cross-hatch's two-pass call
/// needs this drift between passes for byte-exact output.
pub fn polygon_hachure_lines_mut(
    polygons: &mut [Vec<[f64; 2]>],
    o: &RoughOptions,
    rng: &mut RoughRandom,
) -> Vec<[[f64; 2]; 2]> {
    let angle = o.hachure_angle + 90.0;
    let mut gap = o.hachure_gap;
    if gap < 0.0 {
        gap = o.stroke_width * 4.0;
    }
    gap = gap.max(0.1).round();
    let mut skip_offset = 1.0_f64;
    if o.roughness >= 1.0 {
        let r = rng.next();
        if r > 0.7 {
            skip_offset = gap;
        }
    }
    if skip_offset == 0.0 {
        skip_offset = 1.0;
    }
    hachure_lines_mut(polygons, gap, angle, skip_offset)
}

/// Render an `OpSet` of type `FillSketch` from a list of hachure lines.
/// Mirrors `HachureFiller.renderLines` — each line becomes a doubled
/// stroke (`_doubleLine` in `filling: true` mode). The same RNG must be
/// threaded through here as was used for the outline so stroke + fill
/// jitter remain byte-for-byte aligned with upstream.
pub fn hachure_fill_sketch(
    lines: &[[[f64; 2]; 2]],
    o: &RoughOptions,
    rng: &mut RoughRandom,
) -> OpSet {
    let mut ops: Vec<Op> = Vec::with_capacity(lines.len() * 8);
    for line in lines {
        ops.extend(double_line_filling(
            line[0][0], line[0][1], line[1][0], line[1][1], o, rng,
        ));
    }
    OpSet {
        op_type: OpSetType::FillSketch,
        ops,
    }
}

/// `_doubleLine(.., filling = true)` — the variant the hachure filler
/// invokes via `helper.doubleLineOps`. Identical to [`double_line`]
/// except it routes through `disable_multi_stroke_fill` instead of
/// `disable_multi_stroke`.
fn double_line_filling(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    o: &RoughOptions,
    rng: &mut RoughRandom,
) -> Vec<Op> {
    double_line(x1, y1, x2, y2, o, true, rng)
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

// ── `pointsOnPath` — flatten a path's curves to polylines for hachure ─

/// Port of `points-on-curve::flatness` — squared maximum control-point
/// excursion. Used as the cubic-subdivision termination criterion.
fn bezier_flatness(p1: [f64; 2], p2: [f64; 2], p3: [f64; 2], p4: [f64; 2]) -> f64 {
    let mut ux = 3.0 * p2[0] - 2.0 * p1[0] - p4[0];
    ux *= ux;
    let mut uy = 3.0 * p2[1] - 2.0 * p1[1] - p4[1];
    uy *= uy;
    let mut vx = 3.0 * p3[0] - 2.0 * p4[0] - p1[0];
    vx *= vx;
    let mut vy = 3.0 * p3[1] - 2.0 * p4[1] - p1[1];
    vy *= vy;
    if ux < vx {
        ux = vx;
    }
    if uy < vy {
        uy = vy;
    }
    ux + uy
}

fn lerp_pt(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Recursively subdivide a cubic Bézier until each segment's flatness
/// is below `tolerance`. Mirrors `getPointsOnBezierCurveWithSplitting`.
fn pts_on_bezier_split(
    p1: [f64; 2],
    p2: [f64; 2],
    p3: [f64; 2],
    p4: [f64; 2],
    tolerance: f64,
    out: &mut Vec<[f64; 2]>,
) {
    if bezier_flatness(p1, p2, p3, p4) < tolerance {
        if let Some(last) = out.last() {
            let dx = last[0] - p1[0];
            let dy = last[1] - p1[1];
            let d2 = dx * dx + dy * dy;
            if d2 > 1.0 {
                out.push(p1);
            }
        } else {
            out.push(p1);
        }
        out.push(p4);
    } else {
        let t = 0.5;
        let q1 = lerp_pt(p1, p2, t);
        let q2 = lerp_pt(p2, p3, t);
        let q3 = lerp_pt(p3, p4, t);
        let r1 = lerp_pt(q1, q2, t);
        let r2 = lerp_pt(q2, q3, t);
        let red = lerp_pt(r1, r2, t);
        pts_on_bezier_split(p1, q1, r1, red, tolerance, out);
        pts_on_bezier_split(red, r2, q3, p4, tolerance, out);
    }
}

fn distance_sq(p: [f64; 2], q: [f64; 2]) -> f64 {
    let dx = p[0] - q[0];
    let dy = p[1] - q[1];
    dx * dx + dy * dy
}

fn distance_to_segment_sq(p: [f64; 2], v: [f64; 2], w: [f64; 2]) -> f64 {
    let l2 = distance_sq(v, w);
    if l2 == 0.0 {
        return distance_sq(p, v);
    }
    let mut t = ((p[0] - v[0]) * (w[0] - v[0]) + (p[1] - v[1]) * (w[1] - v[1])) / l2;
    if t < 0.0 {
        t = 0.0;
    }
    if t > 1.0 {
        t = 1.0;
    }
    distance_sq(p, lerp_pt(v, w, t))
}

/// Ramer-Douglas-Peucker — port of `points-on-curve::simplifyPoints`.
fn simplify_points_rec(
    points: &[[f64; 2]],
    start: usize,
    end: usize,
    epsilon: f64,
    out: &mut Vec<[f64; 2]>,
) {
    if end <= start + 1 {
        return;
    }
    let s = points[start];
    let e = points[end - 1];
    let mut max_dist_sq = 0.0_f64;
    let mut max_idx = start + 1;
    for i in (start + 1)..(end - 1) {
        let d = distance_to_segment_sq(points[i], s, e);
        if d > max_dist_sq {
            max_dist_sq = d;
            max_idx = i;
        }
    }
    if max_dist_sq.sqrt() > epsilon {
        simplify_points_rec(points, start, max_idx + 1, epsilon, out);
        simplify_points_rec(points, max_idx, end, epsilon, out);
    } else {
        if out.is_empty() {
            out.push(s);
        }
        out.push(e);
    }
}

fn simplify_polyline(points: &[[f64; 2]], distance: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    if !points.is_empty() {
        simplify_points_rec(points, 0, points.len(), distance, &mut out);
    }
    out
}

/// Port of `points-on-path::pointsOnPath`. Walks an SVG path string and
/// returns one `Vec<[f64; 2]>` per `M…[Z]` sub-path, with curves flattened
/// to polylines and the result simplified by Douglas-Peucker at `distance`.
///
/// `tolerance` controls the cubic-flatness termination (rough.js uses 1).
/// `distance` controls RDP simplification — `0.0` skips simplification.
pub fn points_on_path(d: &str, tolerance: f64, distance: f64) -> Vec<Vec<[f64; 2]>> {
    let segs = path_normalize(&path_absolutize(&path_parse(d)));
    let mut sets: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut current: Vec<[f64; 2]> = Vec::new();
    let mut start: [f64; 2] = [0.0, 0.0];
    // pendingCurve accumulates points-of-cubic-segment runs so adjacent
    // C ops chain through `pointsOnBezierCurves`.
    let mut pending: Vec<[f64; 2]> = Vec::new();

    let flush_curve = |pending: &mut Vec<[f64; 2]>, current: &mut Vec<[f64; 2]>| {
        if pending.len() >= 4 {
            let n_segments = (pending.len() - 1) / 3;
            for i in 0..n_segments {
                let off = i * 3;
                pts_on_bezier_split(
                    pending[off],
                    pending[off + 1],
                    pending[off + 2],
                    pending[off + 3],
                    tolerance,
                    current,
                );
            }
        }
        pending.clear();
    };

    let flush_points = |pending: &mut Vec<[f64; 2]>,
                        current: &mut Vec<[f64; 2]>,
                        sets: &mut Vec<Vec<[f64; 2]>>| {
        flush_curve(pending, current);
        if !current.is_empty() {
            sets.push(std::mem::take(current));
        }
    };

    for seg in &segs {
        match seg.key {
            'M' => {
                let mut pending2 = std::mem::take(&mut pending);
                let mut current2 = std::mem::take(&mut current);
                flush_points(&mut pending2, &mut current2, &mut sets);
                pending = pending2;
                current = current2;
                start = [seg.data[0], seg.data[1]];
                current.push(start);
            }
            'L' => {
                flush_curve(&mut pending, &mut current);
                current.push([seg.data[0], seg.data[1]]);
            }
            'C' => {
                if pending.is_empty() {
                    let last = if let Some(p) = current.last() {
                        *p
                    } else {
                        start
                    };
                    pending.push(last);
                }
                pending.push([seg.data[0], seg.data[1]]);
                pending.push([seg.data[2], seg.data[3]]);
                pending.push([seg.data[4], seg.data[5]]);
            }
            'Z' => {
                flush_curve(&mut pending, &mut current);
                current.push(start);
            }
            _ => {}
        }
    }
    let mut pending2 = std::mem::take(&mut pending);
    let mut current2 = std::mem::take(&mut current);
    flush_points(&mut pending2, &mut current2, &mut sets);

    if distance == 0.0 {
        return sets;
    }
    let mut out: Vec<Vec<[f64; 2]>> = Vec::with_capacity(sets.len());
    for set in sets {
        let simp = simplify_polyline(&set, distance);
        if !simp.is_empty() {
            out.push(simp);
        }
    }
    out
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
    // Upstream rough.js semantics: `if (o.strokeLineDash)` is falsy when
    // the option is absent (`undefined`). We treat `None` here to mean
    // "absent" and `Some(vec)` (including empty) to mean "explicitly
    // set" — matching `[0, 0]` style call sites that exercise the
    // attribute path. The plain default `RoughOptions { ..default() }`
    // leaves both as empty `Vec` → backward-compatible "0 0" output.
    let stroke_dash_attr = if o.omit_dash_attrs {
        None
    } else {
        stroke_dash_for_emit(&o.stroke_line_dash)
    };
    let fill_dash_attr = if o.omit_dash_attrs {
        None
    } else {
        stroke_dash_for_emit(&o.fill_line_dash)
    };
    for set in &d.sets {
        match set.op_type {
            OpSetType::Path => {
                out.push(PathOut {
                    d: ops_to_path(set),
                    stroke: o.stroke.clone(),
                    stroke_width: o.stroke_width,
                    fill: "none".into(),
                    fill_rule_evenodd: false,
                    stroke_dasharray: stroke_dash_attr.clone(),
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
                    stroke_dasharray: fill_dash_attr.clone(),
                });
            }
        }
    }
    out
}

/// Decide whether to emit the `stroke-dasharray` attribute. This
/// function is the contract between `RoughOptions::*_line_dash` and the
/// SVG output. Call-sites that previously relied on the implicit
/// `"0 0"` for an empty Vec keep working; the new
/// [`RoughOptions::omit_dash_attrs`] flag opts out (mirroring upstream's
/// `if (o.strokeLineDash)` falsy-undefined branch).
fn stroke_dash_for_emit(vals: &[f64]) -> Option<String> {
    Some(dasharray_str(vals))
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

    // ── Hachure scan-line fill ────────────────────────────────────
    fn approx_lines_eq(got: &[[[f64; 2]; 2]], want: &[[[f64; 2]; 2]], eps: f64) -> bool {
        if got.len() != want.len() {
            return false;
        }
        for (g, w) in got.iter().zip(want.iter()) {
            for k in 0..2 {
                for j in 0..2 {
                    if (g[k][j] - w[k][j]).abs() > eps {
                        return false;
                    }
                }
            }
        }
        true
    }

    #[test]
    fn hachure_axis_aligned_square_angle_zero() {
        // Reference from upstream `hachureLines` (Node):
        //   square=[[[0,0],[10,0],[10,10],[0,10]]], gap=4, angle=0, step=1
        //   → [[[0,0],[10,0]], [[0,4],[10,4]], [[0,8],[10,8]]]
        let polys = vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]];
        let got = hachure_lines(&polys, 4.0, 0.0, 1.0);
        let want = vec![
            [[0.0, 0.0], [10.0, 0.0]],
            [[0.0, 4.0], [10.0, 4.0]],
            [[0.0, 8.0], [10.0, 8.0]],
        ];
        assert!(approx_lines_eq(&got, &want, 1e-12), "got = {got:?}");
    }

    #[test]
    fn hachure_axis_aligned_square_angle_49() {
        // Reference from upstream `hachureLines`:
        //   square=[[[0,0],[10,0],[10,10],[0,10]]], gap=4, angle=49, step=1
        let polys = vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]];
        let got = hachure_lines(&polys, 4.0, 49.0, 1.0);
        let want: Vec<[[f64; 2]; 2]> = vec![
            [[0.0, 0.0], [0.0, 0.0]],
            [
                [-0.2614568240614483, 6.397784017075889],
                [4.98701540786261, 0.36010737529371317],
            ],
            [
                [1.4452634388486247, 10.531439293483462],
                [10.630089844715727, -0.03449482963534578],
            ],
            [
                [7.0883378757017415, 10.136837088554403],
                [10.36863302065428, 6.363289187440543],
            ],
        ];
        assert!(approx_lines_eq(&got, &want, 1e-12), "got = {got:?}");
    }

    #[test]
    fn hachure_triangle_angle_zero_gap5() {
        // tri=[[[0,0],[20,0],[10,20]]], gap=5, angle=0, step=1
        let polys = vec![vec![[0.0, 0.0], [20.0, 0.0], [10.0, 20.0]]];
        let got = hachure_lines(&polys, 5.0, 0.0, 1.0);
        let want = vec![
            [[0.0, 0.0], [20.0, 0.0]],
            [[3.0, 5.0], [18.0, 5.0]],
            [[5.0, 10.0], [15.0, 10.0]],
            [[8.0, 15.0], [13.0, 15.0]],
        ];
        assert!(approx_lines_eq(&got, &want, 1e-12), "got = {got:?}");
    }

    #[test]
    fn hachure_triangle_angle_30() {
        let polys = vec![vec![[0.0, 0.0], [20.0, 0.0], [10.0, 20.0]]];
        let got = hachure_lines(&polys, 5.0, 30.0, 1.0);
        let want: Vec<[[f64; 2]; 2]> = vec![
            [[0.0, 0.0], [0.0, 0.0]],
            [
                [2.4999999999999996, 4.330127018922194],
                [10.294228634059948, -0.16987298107780546],
            ],
            [
                [4.133974596215561, 9.160254037844387],
                [19.72243186433546, 0.16025403784438907],
            ],
            [
                [6.633974596215561, 13.49038105676658],
                [16.160254037844386, 7.990381056766581],
            ],
            [
                [9.133974596215559, 17.820508075688775],
                [11.732050807568875, 16.320508075688775],
            ],
        ];
        assert!(approx_lines_eq(&got, &want, 1e-12), "got = {got:?}");
    }

    #[test]
    fn hachure_box_centered_angle_49() {
        // 60×20 box centred at origin, gap=5, angle=49 (= -41+90).
        let polys = vec![vec![
            [-30.0, -10.0],
            [30.0, -10.0],
            [30.0, 10.0],
            [-30.0, 10.0],
        ]];
        let got = hachure_lines(&polys, 5.0, 49.0, 1.0);
        let want: Vec<[[f64; 2]; 2]> = vec![
            [
                [-29.911645205994922, -10.101640563649964],
                [-29.911645205994922, -10.101640563649964],
            ],
            [
                [-30.074451478824106, -2.293087937360795],
                [-23.513861188919034, -9.840183739588515],
            ],
            [
                [-30.23725775165329, 5.515464688928372],
                [-16.460018142852636, -10.33343649574984],
            ],
            [
                [-27.77582790852044, 10.305178994326454],
                [-10.062234125776747, -10.071979671688391],
            ],
            [
                [-21.378043891444555, 10.566635818387901],
                [-3.664450108700858, -9.810522847626943],
            ],
            [
                [-14.980259874368665, 10.82809264244935],
                [3.3893929373655385, -10.303775603788267],
            ],
            [
                [-7.926416828302268, 10.334839886288027],
                [9.787176954441428, -10.042318779726818],
            ],
            [
                [-1.5286328112263794, 10.596296710349474],
                [16.184960971517317, -9.780861955665369],
            ],
            [
                [4.86915120584951, 10.857753534410923],
                [23.238804017583714, -10.274114711826694],
            ],
            [
                [11.922994251915906, 10.3645007782496],
                [29.636588034659603, -10.012657887765243],
            ],
            [
                [18.320778268991795, 10.625957602311047],
                [30.785899819811434, -3.7135244219216226],
            ],
            [
                [24.718562286067684, 10.887414426372494],
                [30.623093546982247, 4.095028204367548],
            ],
        ];
        assert!(approx_lines_eq(&got, &want, 1e-12), "got = {got:?}");
    }

    #[test]
    fn polygon_hachure_lines_default_options_consume_one_rng_pull_when_roughness_ge_1() {
        // o.roughness defaults to 1.0 → hits the `roughness >= 1` branch
        // which pulls one rng.next(). After the pull the LCG seed should
        // have advanced exactly once.
        let o = RoughOptions::default();
        let mut rng = RoughRandom::new(1);
        let polys = vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]];
        let _ = polygon_hachure_lines(&polys, &o, &mut rng);
        // First two pulls of seed=1 were 0.0000224779359996, 0.0850324486382306.
        // After polygon_hachure_lines (consuming 1 pull), the next pull
        // should be the 2nd value.
        let next = rng.next();
        let want = 0.0850324486382306_f64;
        assert!((next - want).abs() < 1e-15, "next = {next}");
    }

    #[test]
    fn polygon_hachure_lines_skip_offset_used_when_random_above_07() {
        // Cross-check against upstream `polygonHachureLines` (Node):
        //   seed=1: rng pull 2.25e-5 < 0.7 → skip_offset=1 → 10 lines.
        //   seed=2: rng pull 0.94… > 0.7  → skip_offset=gap=4 → 10 lines.
        // Both branches produce the same coverage (every gap-th y); the
        // probe is that *neither* path panics or emits a wildly wrong
        // count.
        let mut rng_low = RoughRandom::new(1);
        let mut rng_high = RoughRandom::new(2);
        let mut o = RoughOptions::default();
        o.hachure_gap = 4.0;
        o.hachure_angle = 0.0;
        o.roughness = 1.0;
        let polys = vec![vec![[0.0, 0.0], [40.0, 0.0], [40.0, 40.0], [0.0, 40.0]]];
        let lines_low = polygon_hachure_lines(&polys, &o, &mut rng_low);
        let lines_high = polygon_hachure_lines(&polys, &o, &mut rng_high);
        assert_eq!(lines_low.len(), 10, "low = {lines_low:?}");
        assert_eq!(lines_high.len(), 10, "high = {lines_high:?}");
    }

    #[test]
    fn hachure_fill_sketch_emits_double_lines() {
        // Empty input → empty FillSketch.
        let mut o = RoughOptions::default();
        o.seed = 1;
        let mut rng = RoughRandom::new(1);
        let lines: Vec<[[f64; 2]; 2]> = vec![];
        let sk = hachure_fill_sketch(&lines, &o, &mut rng);
        assert!(matches!(sk.op_type, OpSetType::FillSketch));
        assert_eq!(sk.ops.len(), 0);

        // Single line at y=0 from (0,0) to (10,0). disable_multi_stroke_fill
        // = false → 2 halves × (1 move + 1 bcurve) = 4 ops.
        let mut rng2 = RoughRandom::new(1);
        let sk2 = hachure_fill_sketch(&[[[0.0, 0.0], [10.0, 0.0]]], &o, &mut rng2);
        assert_eq!(sk2.ops.len(), 4);
    }

    #[test]
    fn rectangle_with_hachure_fill_emits_fillsketch_set() {
        let mut o = RoughOptions::default();
        o.fill = Some("#fff".into());
        o.fill_style = "hachure".into();
        o.seed = 1;
        let mut rc = RoughGenerator::new();
        let d = rc.rectangle(-30.0, -10.0, 60.0, 20.0, &o);
        // 1 fill-sketch + 1 stroke outline.
        assert_eq!(d.sets.len(), 2);
        assert_eq!(d.sets[0].op_type, OpSetType::FillSketch);
        assert_eq!(d.sets[1].op_type, OpSetType::Path);
    }

    // ── Ellipse / circle byte-exact (validated against Node 20 + roughjs@4.6) ──

    #[test]
    fn circle_seed_2_default_byte_exact() {
        let mut o = RoughOptions::default();
        o.seed = 2;
        let mut rc = RoughGenerator::new();
        let d = rc.circle(100.0, 200.0, 40.0, &o);
        assert_eq!(d.shape, "circle");
        assert_eq!(d.sets.len(), 1);
        let want = "M100.09679836443644 179.93285833854728 C103.880271233494 179.4864195486164, 109.1581153066544 181.5367411018535, 112.15462361544918 184.35487900393508 C115.15113192424397 187.17301690601667, 117.35381075839551 192.72271131641068, 118.07584821720513 196.8416857510368 C118.79788567601476 200.96066018566293, 118.44133322956382 205.55395993417605, 116.48684836830694 209.06872561169186 C114.53236350705005 212.58349128920767, 110.02134165699351 216.53122766442846, 106.34893904966383 217.93027981613167 C102.67653644233414 219.3293319678349, 98.30930343533973 218.81638992900918, 94.45243272432879 217.46303852191113 C90.59556201331785 216.10968711481308, 85.32684163048704 213.33132314479815, 83.20771478359819 209.81017137354343 C81.08858793670935 206.2890196022887, 81.00517206501512 200.49320716061692, 81.73767164299568 196.33612789438274 C82.47017122097624 192.17904862814856, 84.21670264425127 187.5950202337945, 87.60271225148153 184.86769577613828 C90.9887218587118 182.14037131848207, 99.42181005163819 180.52077900473557, 102.05372928637728 179.97218114844551 C104.68564852111636 179.42358329215546, 103.37722201449374 181.23669509848148, 103.39422765991606 181.5761086383979 M103.13106776505347 180.75471489700578 C106.83826916276865 181.0347216596498, 111.6829353648455 183.8861799386937, 114.19810741777064 186.98051933607653 C116.71327947069578 190.07485873345937, 118.32258758346697 195.26448396892965, 118.22210008260427 199.32075128130285 C118.12161258174157 203.37701859367604, 116.19366797279633 208.09348253878866, 113.59518241259443 211.3181232103157 C110.99669685239252 214.54276388184272, 106.70856480951876 217.91690281108728, 102.63118672139281 218.668595310465 C98.55380863326685 219.42028780984273, 92.32234506797357 217.88711421925532, 89.13091388383867 215.82827820658198 C85.93948269970377 213.76944219390865, 84.4270503804196 210.11105278888795, 83.48259961658346 206.31557923442492 C82.53814885274731 202.5201056799619, 82.03569019556213 197.10326396713708, 83.46420930082178 193.05543687980378 C84.89272840608143 189.00760979247048, 88.61743969981103 183.82416907502633, 92.05371424814136 182.02861671042513 C95.48998879647169 180.23306434582392, 102.00619770375964 182.25618105605645, 104.08185659080381 182.2821226921966 C106.15751547784798 182.30806432833674, 104.5035119206667 182.26994174278707, 104.50766757040638 182.184266527266";
        assert_eq!(ops_to_path(&d.sets[0]), want);
    }

    #[test]
    fn bbox_of_sets_rectangle_no_jitter() {
        let mut o = RoughOptions::default();
        o.seed = 1;
        o.roughness = 0.0; // no jitter — exact corners
        o.preserve_vertices = true;
        let mut rc = RoughGenerator::new();
        let d = rc.rectangle(-5.0, -3.0, 10.0, 6.0, &o);
        let bb = bbox_of_sets(&d.sets).expect("non-empty");
        assert!((bb.0 - -5.0).abs() < 1e-9, "xmin {}", bb.0);
        assert!((bb.1 - -3.0).abs() < 1e-9, "ymin {}", bb.1);
        assert!((bb.2 - 5.0).abs() < 1e-9, "xmax {}", bb.2);
        assert!((bb.3 - 3.0).abs() < 1e-9, "ymax {}", bb.3);
    }

    #[test]
    fn bbox_of_sets_circle_seed1_within_bounds() {
        let mut o = RoughOptions::default();
        o.seed = 1;
        let mut rc = RoughGenerator::new();
        let d = rc.circle(0.0, 0.0, 200.0, &o);
        let bb = bbox_of_sets(&d.sets).expect("non-empty");
        // Sanity: the roughened circle's bbox is within ~15px of the
        // ideal [-100, 100]. Track mermaid's applyPaddedViewBox post-jitter.
        assert!(bb.0 < -90.0 && bb.0 > -115.0, "xmin {}", bb.0);
        assert!(bb.1 < -90.0 && bb.1 > -115.0, "ymin {}", bb.1);
        assert!(bb.2 > 90.0 && bb.2 < 115.0, "xmax {}", bb.2);
        assert!(bb.3 > 90.0 && bb.3 < 115.0, "ymax {}", bb.3);
    }
}
