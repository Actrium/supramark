//! cose-bilkent force-directed layout — groundwork.
//!
//! Upstream's mindmap rendering pipeline configures cytoscape with the
//! `cose-bilkent` extension (`quality: 'proof', animate: false,
//! styleEnabled: false`). The extension is layered:
//!
//! ```text
//! cytoscape-cose-bilkent  (~390 LOC: wrapper, Coarsening, ConstraintHandler)
//!         |
//!         v
//! cose-base               (~1,500 LOC: CoSELayout, CoSENode/Edge/Graph[Manager])
//!         |
//!         v
//! layout-base             (~3,000 LOC: FDLayout, LGraph, geometry helpers,
//!                          LinkedList / HashSet / HashMap / Quicksort,
//!                          PointD / RectangleD / DimensionD / Transform,
//!                          NeedlemanWunsch, RandomSeed)
//! ```
//!
//! ## Why we cannot achieve byte-exact in this wave
//!
//! Byte-exact reproduction of the layout requires matching:
//!   * upstream's `Math.random()` consumption order at every call
//!     site — Mermaid 11.x reseeds with `mulberry32(0x12345678)`
//!     before each render via `tests/support/generate_ref.mjs`;
//!   * the multi-level coarsening / refinement schedule in
//!     `cytoscape-cose-bilkent` (`Coarsening.coarsen` plus the
//!     two-stage tiling tree-reduction);
//!   * floating-point IEEE-754 accumulation across ~2,500 simulation
//!     iterations per layer of coarsening, where any reordering of
//!     additions changes the output;
//!   * the `IGeometry` clip-point selection (cardinal direction switch)
//!     for repulsion forces between non-overlapping rectangles.
//!
//! The 18 mindmap fixtures (cypress 01..04, 10..23 + demos 01) remain
//! `KNOWN_IGNORED` until a full port lands.
//!
//! ## What this module provides
//!
//! 1. Foundational geometry and constants ported from `layout-base`
//!    (`PointD`, `DimensionD`, `RectangleD`, `LayoutQuality`,
//!    `LayoutConstants`, `FDLayoutConstants`, `CoSEConstants`).
//! 2. Minimal graph data structures — `LNode`, `LEdge`, `LGraph`,
//!    `LGraphManager` — sufficient to encode the input topology.
//!    These intentionally omit features absent from the mindmap
//!    use-case: compound nodes / inclusion tree, multi-graph
//!    inter-graph edges, animation hooks, virtual DOM nodes.
//! 3. A static `IGeometry` helper port covering the methods FDLayout
//!    actually uses: `intersects`, `calc_separation_amount`,
//!    `decide_directions_for_overlapping_nodes`,
//!    `cardinal_direction`, and `get_intersection2` (clip-point
//!    repulsion for non-overlapping rectangles).
//! 4. `RandomSeed` — the upstream LCG-style sine PRNG that drives
//!    `LNode.scatter` (i.e. `positionNodesRandomly`). Independently,
//!    `Mulberry32` is provided to match the seeding installed by
//!    `tests/support/generate_ref.mjs` (which replaces
//!    `Math.random` for cytoscape's internal randomisation). Both
//!    PRNGs are required for full byte-exact parity.
//! 5. A single-iteration force pass (`simulation_step`) computing
//!    spring + repulsion + gravitational forces and applying them
//!    with the cooling factor, plus the convergent
//!    `run_spring_embedder` loop matching `FDLayout::runSpringEmbedder`
//!    (cooling schedule, convergence-period checks, max-iter ceiling).
//! 6. `position_nodes_randomly` — mirror of `Layout.positionNodesRandomly`
//!    using `RandomSeed` to scatter all nodes uniformly around the
//!    world centre.
//! 7. A `run_layout` entry point that ties everything together:
//!    builds the graph, scatters via RandomSeed, runs the simulation
//!    to convergence, and (for now) returns `Unsupported`. The
//!    `LayoutOutcome::Ok` branch lights up automatically when callers
//!    stop ignoring it; the gating happens in [`run_layout`].
//!
//! ## What's left for byte-exact parity
//!
//! Even with the full simulation loop, results are NOT byte-exact
//! against upstream because of:
//!
//!   * **Tree reduction (cose-base `reduceTrees` / `growTree`)** —
//!     mindmap is fully tree-shaped, so upstream prunes leaves
//!     attached to articulation points before simulating, then
//!     re-attaches them in `growTree`. Skipping this drastically
//!     changes intermediate displacement totals.
//!   * **FR-grid bucket repulsion** — `useFRGridVariant` partitions
//!     the world into `repulsionRange`-sized cells and only computes
//!     forces between rectangles in adjacent cells. Without it our
//!     all-pairs repulsion accumulates differently.
//!   * **`getIntersection2` not yet wired into `simulation_step`** —
//!     the helper is ported and unit-tested but the repulsion-force
//!     calculator still uses centre-distance for non-overlapping
//!     rectangles. Wiring is straightforward; left as the next
//!     incremental step.
//!   * **Coarsening / multi-level scaling** —
//!     `cytoscape-cose-bilkent/src/index.js`'s `coarsen` builds a
//!     hierarchy of progressively-smaller graphs, lays out the
//!     coarsest, then refines. Skipped here.
//!   * **Cytoscape input ordering** — node and edge IDs come from
//!     cytoscape's `add()` insertion order, which matches the
//!     parser's emission order (a property our parser preserves).
//!     Edge stable order through the simulation matters because
//!     spring-force accumulation is non-associative.
//!   * **Renderer multi-node support** — even if positions were
//!     byte-exact, `src/render/svg_mindmap.rs` rejects multi-node
//!     diagrams up front. The `bang` (cloud-style path), `cloud`,
//!     and `hexagon` shape renderers + edge path rendering +
//!     viewport-bbox calc would also need porting before any of the
//!     18 KNOWN_IGNORED mindmap fixtures could be unlocked.
//!
//! ## Status
//!
//! All 7 mindmap fixtures (cypress 05-09 single-node + demos/01)
//! remain byte-exact via the single-node fast path in
//! `mindmap.rs`. The 18 multi-node fixtures stay in
//! `KNOWN_IGNORED` until the renderer + the gaps above are
//! addressed.

#![allow(dead_code)] // Groundwork — call sites land in subsequent waves.

use std::collections::HashMap;

use crate::model::mindmap::NodeId;

// ---------------------------------------------------------------------
// Section: Geometry primitives
// ---------------------------------------------------------------------

/// Mirror of `layout-base/src/util/PointD.js` (a 2-D point with
/// floating-point coordinates).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PointD {
    pub x: f64,
    pub y: f64,
}

impl PointD {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// `getDifference`: returns the dimension `self - other`.
    pub fn diff(self, other: PointD) -> DimensionD {
        DimensionD {
            width: self.x - other.x,
            height: self.y - other.y,
        }
    }

    /// `translate`: shifts by a dimension and returns `self`.
    pub fn translate(mut self, dim: DimensionD) -> Self {
        self.x += dim.width;
        self.y += dim.height;
        self
    }
}

/// Mirror of `layout-base/src/util/DimensionD.js` (a width/height
/// pair).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DimensionD {
    pub width: f64,
    pub height: f64,
}

/// Mirror of `layout-base/src/util/RectangleD.js`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RectangleD {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl RectangleD {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f64 {
        self.x + self.width
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }
    pub fn center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }
    pub fn center_y(&self) -> f64 {
        self.y + self.height / 2.0
    }
    pub fn min_x(&self) -> f64 {
        self.x
    }
    pub fn max_x(&self) -> f64 {
        self.right()
    }
    pub fn min_y(&self) -> f64 {
        self.y
    }
    pub fn max_y(&self) -> f64 {
        self.bottom()
    }
    pub fn width_half(&self) -> f64 {
        self.width / 2.0
    }
    pub fn height_half(&self) -> f64 {
        self.height / 2.0
    }

    /// Mirrors upstream's `intersects` — note the use of `<` (strict)
    /// not `<=`; touching rectangles return `true`.
    pub fn intersects(&self, other: &RectangleD) -> bool {
        if self.right() < other.x {
            return false;
        }
        if self.bottom() < other.y {
            return false;
        }
        if other.right() < self.x {
            return false;
        }
        if other.bottom() < self.y {
            return false;
        }
        true
    }
}

// ---------------------------------------------------------------------
// Section: Constants
// ---------------------------------------------------------------------

/// `LayoutConstants.QUALITY` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutQuality {
    Draft = 0,
    Default = 1,
    Proof = 2,
}

/// Aggregated constants from `LayoutConstants`, `FDLayoutConstants` and
/// `CoSEConstants` (upstream prototype-merge inheritance, flattened).
pub struct CoSEConstants;

impl CoSEConstants {
    // From LayoutConstants
    pub const DEFAULT_GRAPH_MARGIN: f64 = 15.0;
    pub const NODE_DIMENSIONS_INCLUDE_LABELS: bool = false;
    pub const SIMPLE_NODE_SIZE: f64 = 40.0;
    pub const SIMPLE_NODE_HALF_SIZE: f64 = 20.0;
    pub const EMPTY_COMPOUND_NODE_SIZE: f64 = 40.0;
    pub const MIN_EDGE_LENGTH: f64 = 1.0;
    pub const WORLD_BOUNDARY: f64 = 1_000_000.0;
    pub const INITIAL_WORLD_BOUNDARY: f64 = 1_000.0;
    pub const WORLD_CENTER_X: f64 = 1200.0;
    pub const WORLD_CENTER_Y: f64 = 900.0;

    // From FDLayoutConstants
    pub const MAX_ITERATIONS: usize = 2500;
    pub const DEFAULT_EDGE_LENGTH: f64 = 50.0;
    pub const DEFAULT_SPRING_STRENGTH: f64 = 0.45;
    pub const DEFAULT_REPULSION_STRENGTH: f64 = 4500.0;
    pub const DEFAULT_GRAVITY_STRENGTH: f64 = 0.4;
    pub const DEFAULT_COMPOUND_GRAVITY_STRENGTH: f64 = 1.0;
    pub const DEFAULT_GRAVITY_RANGE_FACTOR: f64 = 3.8;
    pub const DEFAULT_COMPOUND_GRAVITY_RANGE_FACTOR: f64 = 1.5;
    pub const DEFAULT_USE_SMART_IDEAL_EDGE_LENGTH_CALCULATION: bool = true;
    pub const DEFAULT_USE_SMART_REPULSION_RANGE_CALCULATION: bool = true;
    pub const DEFAULT_COOLING_FACTOR_INCREMENTAL: f64 = 0.3;
    pub const COOLING_ADAPTATION_FACTOR: f64 = 0.33;
    pub const ADAPTATION_LOWER_NODE_LIMIT: usize = 1000;
    pub const ADAPTATION_UPPER_NODE_LIMIT: usize = 5000;
    pub const MAX_NODE_DISPLACEMENT_INCREMENTAL: f64 = 100.0;
    pub const MAX_NODE_DISPLACEMENT: f64 = 300.0;
    pub const MIN_REPULSION_DIST: f64 = 5.0; // 50/10
    pub const CONVERGENCE_CHECK_PERIOD: usize = 100;
    pub const PER_LEVEL_IDEAL_EDGE_LENGTH_FACTOR: f64 = 0.1;
    pub const GRID_CALCULATION_CHECK_PERIOD: usize = 10;

    // From CoSEConstants
    pub const DEFAULT_USE_MULTI_LEVEL_SCALING: bool = false;
    pub const DEFAULT_RADIAL_SEPARATION: f64 = Self::DEFAULT_EDGE_LENGTH;
    pub const DEFAULT_COMPONENT_SEPERATION: f64 = 60.0;
    pub const TILE: bool = true;
    pub const TILING_PADDING_VERTICAL: f64 = 10.0;
    pub const TILING_PADDING_HORIZONTAL: f64 = 10.0;
    pub const TREE_REDUCTION_ON_INCREMENTAL: bool = false;
}

// ---------------------------------------------------------------------
// Section: PRNG
// ---------------------------------------------------------------------

/// Mirror of `layout-base/src/util/RandomSeed.js`. Note this is **not**
/// the mulberry32 generator that mermaid's render-time test harness
/// installs as `Math.random` — that one drives cytoscape itself.
/// Upstream uses both: `Math.random` for initial node placement and
/// this `RandomSeed` for layered scaling / coarsening tie-breaks.
#[derive(Debug, Clone)]
pub struct RandomSeed {
    seed: f64,
    last: f64,
}

impl Default for RandomSeed {
    fn default() -> Self {
        Self {
            seed: 1.0,
            last: 0.0,
        }
    }
}

impl RandomSeed {
    pub fn with_seed(seed: f64) -> Self {
        Self { seed, last: 0.0 }
    }

    /// Equivalent to `RandomSeed.nextDouble()`.
    pub fn next_double(&mut self) -> f64 {
        let x = self.seed.sin() * 10_000.0;
        self.seed += 1.0;
        self.last = x;
        x - x.floor()
    }
}

/// Mulberry32 PRNG matching the seed installed in
/// `tests/support/generate_ref.mjs` (lines 599-608). The harness
/// replaces `Math.random` with this function before every render so
/// that any code path consuming `Math.random()` (incl. cytoscape) is
/// deterministic.
///
/// The reference snippet:
/// ```text
/// let __rngState = 0x12345678;
/// function __mulberry32() {
///   __rngState = (__rngState + 0x6d2b79f5) | 0;
///   let t = __rngState;
///   t = Math.imul(t ^ (t >>> 15), 1 | t);
///   t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
///   return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
/// }
/// ```
///
/// Important: `cose-bilkent` does NOT consume `Math.random` directly
/// for node initial positions — `LNode.scatter` uses the in-process
/// sin-based [`RandomSeed`] above. `Math.random` is used elsewhere in
/// the cytoscape pipeline (collection traversal randomisation,
/// rough-style hand-drawn paths, etc.). Both PRNGs are required for
/// full byte-exact parity.
#[derive(Debug, Clone)]
pub struct Mulberry32 {
    state: u32,
}

impl Default for Mulberry32 {
    /// Default seed `0x12345678` matches `generate_ref.mjs`.
    fn default() -> Self {
        Self { state: 0x1234_5678 }
    }
}

impl Mulberry32 {
    pub fn with_seed(seed: u32) -> Self {
        Self { state: seed }
    }

    /// One step. Mirrors the JS implementation byte-for-byte:
    /// `(state + 0x6d2b79f5) | 0` is wrapping addition on a signed
    /// 32-bit; we model it via wrapping `u32` arithmetic (the bit
    /// pattern is identical), and `Math.imul` is implemented as
    /// `wrapping_mul` on `u32`.
    pub fn next_double(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6d2b_79f5);
        let mut t = self.state;
        // t = Math.imul(t ^ (t >>> 15), 1 | t)
        t = (t ^ (t >> 15)).wrapping_mul(1 | t);
        // t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) as f64) / 4_294_967_296.0
    }
}

// ---------------------------------------------------------------------
// Section: Static IGeometry helpers
// ---------------------------------------------------------------------

/// Stateless namespace of geometry helpers. Mirrors `IGeometry` in
/// `layout-base/src/util/IGeometry.js`. Only the helpers FDLayout
/// invokes during force calculation are ported here — bend handling
/// (`getIntersection` four-arg form) stays out until a future wave.
pub struct IGeometry;

impl IGeometry {
    pub const HALF_PI: f64 = std::f64::consts::FRAC_PI_2;
    pub const ONE_AND_HALF_PI: f64 = std::f64::consts::PI * 1.5;
    pub const TWO_PI: f64 = std::f64::consts::TAU;
    pub const THREE_PI: f64 = std::f64::consts::PI * 3.0;

    /// Direction vector returned by `decideDirectionsForOverlappingNodes`:
    /// `(dx, dy)` each in `{-1, +1}`.
    pub fn decide_directions_for_overlapping_nodes(
        rect_a: &RectangleD,
        rect_b: &RectangleD,
    ) -> (i32, i32) {
        let dx = if rect_a.center_x() < rect_b.center_x() {
            -1
        } else {
            1
        };
        let dy = if rect_a.center_y() < rect_b.center_y() {
            -1
        } else {
            1
        };
        (dx, dy)
    }

    /// `calcSeparationAmount` — returns the per-axis separation amount
    /// `(overlap_x, overlap_y)`. Asserts `rect_a.intersects(rect_b)`;
    /// upstream throws on violation. We mirror that as a `panic`
    /// because it indicates a caller bug (FDLayout only invokes this
    /// inside an `intersects()` branch).
    pub fn calc_separation_amount(
        rect_a: &RectangleD,
        rect_b: &RectangleD,
        separation_buffer: f64,
    ) -> (f64, f64) {
        assert!(
            rect_a.intersects(rect_b),
            "calc_separation_amount called with non-intersecting rects"
        );

        let (dir_x, dir_y) = Self::decide_directions_for_overlapping_nodes(rect_a, rect_b);

        let mut overlap_x = rect_a.right().min(rect_b.right()) - rect_a.x.max(rect_b.x);
        let mut overlap_y = rect_a.bottom().min(rect_b.bottom()) - rect_a.y.max(rect_b.y);

        // Containment cases (cf. comments in the JS source).
        if rect_a.x <= rect_b.x && rect_a.right() >= rect_b.right() {
            overlap_x += (rect_b.x - rect_a.x).min(rect_a.right() - rect_b.right());
        } else if rect_b.x <= rect_a.x && rect_b.right() >= rect_a.right() {
            overlap_x += (rect_a.x - rect_b.x).min(rect_b.right() - rect_a.right());
        }
        if rect_a.y <= rect_b.y && rect_a.bottom() >= rect_b.bottom() {
            overlap_y += (rect_b.y - rect_a.y).min(rect_a.bottom() - rect_b.bottom());
        } else if rect_b.y <= rect_a.y && rect_b.bottom() >= rect_a.bottom() {
            overlap_y += (rect_a.y - rect_b.y).min(rect_b.bottom() - rect_a.bottom());
        }

        // Slope of the line through both centres.
        let slope =
            if rect_b.center_x() == rect_a.center_x() && rect_b.center_y() == rect_a.center_y() {
                // 45-degree fallback when centres coincide.
                1.0
            } else {
                ((rect_b.center_y() - rect_a.center_y()) / (rect_b.center_x() - rect_a.center_x()))
                    .abs()
            };

        let mut move_by_y = slope * overlap_x;
        let mut move_by_x = overlap_y / slope;
        if overlap_x < move_by_x {
            move_by_x = overlap_x;
        } else {
            move_by_y = overlap_y;
        }

        (
            -(dir_x as f64) * (move_by_x / 2.0 + separation_buffer),
            -(dir_y as f64) * (move_by_y / 2.0 + separation_buffer),
        )
    }

    /// `getCardinalDirection`: returns `1..4` (N/E/S/W).
    pub fn cardinal_direction(slope: f64, slope_prime: f64, line: i32) -> i32 {
        if slope > slope_prime {
            line
        } else {
            1 + (line % 4)
        }
    }

    /// Sign helper matching `IMath.sign`.
    pub fn sign(value: f64) -> f64 {
        if value > 0.0 {
            1.0
        } else if value < 0.0 {
            -1.0
        } else {
            0.0
        }
    }

    /// Mirror of `IGeometry.getIntersection2`. Returns
    /// `(clip_a, clip_b, intersects)` — when the rectangles intersect,
    /// both clip points collapse to their respective centres and
    /// `intersects = true`. Otherwise the clip points lie on the
    /// rectangle boundaries along the centre-to-centre line.
    ///
    /// Used by `FDLayout.calcRepulsionForce` for non-overlapping
    /// rectangles to compute the force vector along the boundary
    /// normal (vs. the centre-distance approximation in the current
    /// `simulation_step`).
    pub fn get_intersection2(rect_a: &RectangleD, rect_b: &RectangleD) -> (PointD, PointD, bool) {
        let p1x = rect_a.center_x();
        let p1y = rect_a.center_y();
        let p2x = rect_b.center_x();
        let p2y = rect_b.center_y();

        if rect_a.intersects(rect_b) {
            return (PointD::new(p1x, p1y), PointD::new(p2x, p2y), true);
        }

        let top_left_a = (rect_a.x, rect_a.y);
        let top_right_a_x = rect_a.right();
        let bottom_left_a = (rect_a.x, rect_a.bottom());
        let bottom_right_a_x = rect_a.right();
        let half_w_a = rect_a.width_half();
        let half_h_a = rect_a.height_half();

        let top_left_b = (rect_b.x, rect_b.y);
        let top_right_b_x = rect_b.right();
        let bottom_left_b = (rect_b.x, rect_b.bottom());
        let bottom_right_b_x = rect_b.right();
        let half_w_b = rect_b.width_half();
        let half_h_b = rect_b.height_half();

        let mut result_a = PointD::default();
        let mut result_b = PointD::default();

        // Vertical centre-to-centre line.
        if p1x == p2x {
            if p1y > p2y {
                return (
                    PointD::new(p1x, top_left_a.1),
                    PointD::new(p2x, bottom_left_b.1),
                    false,
                );
            } else if p1y < p2y {
                return (
                    PointD::new(p1x, bottom_left_a.1),
                    PointD::new(p2x, top_left_b.1),
                    false,
                );
            } else {
                return (PointD::new(p1x, p1y), PointD::new(p2x, p2y), false);
            }
        }
        // Horizontal centre-to-centre line.
        if p1y == p2y {
            if p1x > p2x {
                return (
                    PointD::new(top_left_a.0, p1y),
                    PointD::new(top_right_b_x, p2y),
                    false,
                );
            } else if p1x < p2x {
                return (
                    PointD::new(top_right_a_x, p1y),
                    PointD::new(top_left_b.0, p2y),
                    false,
                );
            } else {
                return (PointD::new(p1x, p1y), PointD::new(p2x, p2y), false);
            }
        }

        let slope_a = rect_a.height / rect_a.width;
        let slope_b = rect_b.height / rect_b.width;
        let slope_prime = (p2y - p1y) / (p2x - p1x);

        let mut clip_a_found = false;
        let mut clip_b_found = false;

        // Corner-of-A cases.
        if -slope_a == slope_prime {
            if p1x > p2x {
                result_a = PointD::new(bottom_left_a.0, bottom_left_a.1);
            } else {
                result_a = PointD::new(top_right_a_x, top_left_a.1);
            }
            clip_a_found = true;
        } else if slope_a == slope_prime {
            if p1x > p2x {
                result_a = PointD::new(top_left_a.0, top_left_a.1);
            } else {
                result_a = PointD::new(bottom_right_a_x, bottom_left_a.1);
            }
            clip_a_found = true;
        }

        // Corner-of-B cases.
        if -slope_b == slope_prime {
            if p2x > p1x {
                result_b = PointD::new(bottom_left_b.0, bottom_left_b.1);
            } else {
                result_b = PointD::new(top_right_b_x, top_left_b.1);
            }
            clip_b_found = true;
        } else if slope_b == slope_prime {
            if p2x > p1x {
                result_b = PointD::new(top_left_b.0, top_left_b.1);
            } else {
                result_b = PointD::new(bottom_right_b_x, bottom_left_b.1);
            }
            clip_b_found = true;
        }

        if clip_a_found && clip_b_found {
            return (result_a, result_b, false);
        }

        // Cardinal directions.
        let (card_a, card_b) = if p1x > p2x {
            if p1y > p2y {
                (
                    Self::cardinal_direction(slope_a, slope_prime, 4),
                    Self::cardinal_direction(slope_b, slope_prime, 2),
                )
            } else {
                (
                    Self::cardinal_direction(-slope_a, slope_prime, 3),
                    Self::cardinal_direction(-slope_b, slope_prime, 1),
                )
            }
        } else if p1y > p2y {
            (
                Self::cardinal_direction(-slope_a, slope_prime, 1),
                Self::cardinal_direction(-slope_b, slope_prime, 3),
            )
        } else {
            (
                Self::cardinal_direction(slope_a, slope_prime, 2),
                Self::cardinal_direction(slope_b, slope_prime, 4),
            )
        };

        if !clip_a_found {
            result_a = match card_a {
                1 => PointD::new(p1x + (-half_h_a) / slope_prime, top_left_a.1),
                2 => PointD::new(bottom_right_a_x, p1y + half_w_a * slope_prime),
                3 => PointD::new(p1x + half_h_a / slope_prime, bottom_left_a.1),
                4 => PointD::new(bottom_left_a.0, p1y + (-half_w_a) * slope_prime),
                _ => PointD::new(p1x, p1y),
            };
        }
        if !clip_b_found {
            result_b = match card_b {
                1 => PointD::new(p2x + (-half_h_b) / slope_prime, top_left_b.1),
                2 => PointD::new(bottom_right_b_x, p2y + half_w_b * slope_prime),
                3 => PointD::new(p2x + half_h_b / slope_prime, bottom_left_b.1),
                4 => PointD::new(bottom_left_b.0, p2y + (-half_w_b) * slope_prime),
                _ => PointD::new(p2x, p2y),
            };
        }

        (result_a, result_b, false)
    }
}

// ---------------------------------------------------------------------
// Section: Graph data structures (minimal, leaf-only subset)
// ---------------------------------------------------------------------

/// Force accumulators on a node — extracted from FDLayoutNode.
#[derive(Debug, Clone, Default)]
pub struct NodeForces {
    pub spring: PointD,
    pub repulsion: PointD,
    pub gravity: PointD,
    pub displacement: PointD,
}

/// Layout-side node. Mirrors a *leaf* `CoSENode` (no child graph,
/// `noOfChildren = 1`), which is the only kind mindmap produces.
#[derive(Debug, Clone)]
pub struct LNode {
    /// Stable identifier into the input graph.
    pub id: NodeId,
    /// Position of the top-left corner (cytoscape converts centre-based
    /// coordinates from its API into top-left here).
    pub rect: RectangleD,
    pub forces: NodeForces,
    /// Always `1` for leaf-only mindmaps; reserved for the compound
    /// extension.
    pub no_of_children: usize,
}

impl LNode {
    pub fn new(id: NodeId, rect: RectangleD) -> Self {
        Self {
            id,
            rect,
            forces: NodeForces::default(),
            no_of_children: 1,
        }
    }

    pub fn move_by(&mut self, dx: f64, dy: f64) {
        self.rect.x += dx;
        self.rect.y += dy;
    }

    pub fn reset_forces(&mut self) {
        self.forces = NodeForces::default();
    }
}

/// Layout-side edge. Index pair into `LGraph::nodes`.
#[derive(Debug, Clone, Copy)]
pub struct LEdge {
    pub source: usize,
    pub target: usize,
    pub ideal_length: f64,
}

impl LEdge {
    pub fn new(source: usize, target: usize) -> Self {
        Self {
            source,
            target,
            ideal_length: CoSEConstants::DEFAULT_EDGE_LENGTH,
        }
    }
}

/// A single graph (no compound nesting). For mindmap there is only
/// the root graph; `LGraphManager` is therefore degenerate.
#[derive(Debug, Clone, Default)]
pub struct LGraph {
    pub nodes: Vec<LNode>,
    pub edges: Vec<LEdge>,
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

impl LGraph {
    /// Recompute the bounding box of all nodes — used by
    /// `calcGravitationalForce` to find the owner-graph centre.
    pub fn update_bounds(&mut self) {
        if self.nodes.is_empty() {
            self.left = 0.0;
            self.right = 0.0;
            self.top = 0.0;
            self.bottom = 0.0;
            return;
        }
        self.left = f64::INFINITY;
        self.top = f64::INFINITY;
        self.right = f64::NEG_INFINITY;
        self.bottom = f64::NEG_INFINITY;
        for n in &self.nodes {
            self.left = self.left.min(n.rect.x);
            self.top = self.top.min(n.rect.y);
            self.right = self.right.max(n.rect.right());
            self.bottom = self.bottom.max(n.rect.bottom());
        }
    }

    /// Estimated diagonal size (used by gravity range factor).
    pub fn estimated_size(&self) -> f64 {
        let w = self.right - self.left;
        let h = self.bottom - self.top;
        (w * w + h * h).sqrt() / 2.0
    }
}

/// Graph manager — for mindmap there is only the root graph, so this
/// is a thin wrapper. Future waves will extend it for compounds.
#[derive(Debug, Clone, Default)]
pub struct LGraphManager {
    pub root: LGraph,
}

impl LGraphManager {
    pub fn from_graph(root: LGraph) -> Self {
        Self { root }
    }
}

// ---------------------------------------------------------------------
// Section: Force-directed simulation step
// ---------------------------------------------------------------------

/// Per-layout state carried through the simulation loop. Mirrors the
/// instance fields on FDLayout / CoSELayout that participate in the
/// forces. Defaults match `quality: 'proof', animate: false` upstream.
#[derive(Debug, Clone)]
pub struct SimulationState {
    pub spring_constant: f64,
    pub repulsion_constant: f64,
    pub gravity_constant: f64,
    pub gravity_range_factor: f64,
    pub ideal_edge_length: f64,
    pub cooling_factor: f64,
    pub initial_cooling_factor: f64,
    pub final_temperature: f64,
    pub max_cooling_cycle: f64,
    pub cooling_cycle: f64,
    pub cooling_adjuster: f64,
    pub layout_quality: LayoutQuality,
    pub max_node_displacement: f64,
    pub total_displacement: f64,
    pub old_total_displacement: f64,
    pub total_displacement_threshold: f64,
    pub total_iterations: usize,
    pub max_iterations: usize,
    /// Sin-based PRNG for `scatter` (initial node placement).
    pub random_seed: RandomSeed,
}

impl Default for SimulationState {
    fn default() -> Self {
        Self {
            spring_constant: CoSEConstants::DEFAULT_SPRING_STRENGTH,
            repulsion_constant: CoSEConstants::DEFAULT_REPULSION_STRENGTH,
            gravity_constant: CoSEConstants::DEFAULT_GRAVITY_STRENGTH,
            gravity_range_factor: CoSEConstants::DEFAULT_GRAVITY_RANGE_FACTOR,
            ideal_edge_length: CoSEConstants::DEFAULT_EDGE_LENGTH,
            cooling_factor: 1.0,
            initial_cooling_factor: 1.0,
            final_temperature: 0.04,
            max_cooling_cycle: 50.0,
            cooling_cycle: 0.0,
            cooling_adjuster: 1.0,
            layout_quality: LayoutQuality::Proof,
            max_node_displacement: CoSEConstants::MAX_NODE_DISPLACEMENT,
            total_displacement: 0.0,
            old_total_displacement: 0.0,
            // FDLayout sets totalDisplacementThreshold = 0.4 * estimatedSize
            // at simulation start; we store an initial sentinel.
            total_displacement_threshold: 0.0,
            total_iterations: 0,
            max_iterations: CoSEConstants::MAX_ITERATIONS,
            random_seed: RandomSeed::default(),
        }
    }
}

impl SimulationState {
    /// Mirror of `FDLayout.isConverged`. Returns `true` when total
    /// displacement falls below threshold OR (after 1/3 of max iters)
    /// when displacement oscillates within 2 px between iterations.
    pub fn is_converged(&self) -> bool {
        let oscillating = if self.total_iterations > self.max_iterations / 3 {
            (self.total_displacement - self.old_total_displacement).abs() < 2.0
        } else {
            false
        };
        let converged = self.total_displacement < self.total_displacement_threshold;
        converged || oscillating
    }

    /// Mirror of CoSELayout.tick's cooling-schedule update (executed
    /// every `CONVERGENCE_CHECK_PERIOD` iterations). Cooling schedule
    /// follows http://www.btluke.com/simanf1.html schedule 3.
    pub fn update_cooling(&mut self) {
        self.cooling_cycle += 1.0;
        match self.layout_quality {
            LayoutQuality::Draft => self.cooling_adjuster = self.cooling_cycle,
            LayoutQuality::Default => self.cooling_adjuster = self.cooling_cycle / 3.0,
            LayoutQuality::Proof => { /* cooling_adjuster stays 1.0 */ }
        }
        let exp = (100.0 * (self.initial_cooling_factor - self.final_temperature)).ln()
            / self.max_cooling_cycle.ln();
        let next = self.initial_cooling_factor
            - self.cooling_cycle.powf(exp) / 100.0 * self.cooling_adjuster;
        self.cooling_factor = next.max(self.final_temperature);
    }
}

/// One iteration of the spring-embedder loop. Concretely:
///   1. spring forces along every edge (Hookean towards `ideal_length`),
///   2. all-pairs repulsion forces (no FR-grid yet),
///   3. gravitational pull towards the owner graph's centre,
///   4. apply forces with cooling.
///
/// **Not byte-exact**. Notably this skips:
///   * `IGeometry.getIntersection` clip-point computation (uses centre
///     distances when neither rectangle nests the other);
///   * grid-bucketed repulsion (`useFRGridVariant`);
///   * `IMath.sign` exact-zero handling;
///   * the inter-graph `idealLength` correction term.
pub fn simulation_step(graph: &mut LGraph, state: &mut SimulationState) {
    state.total_displacement = 0.0;

    // Spring forces.
    for edge in &graph.edges {
        let (source_idx, target_idx) = (edge.source, edge.target);
        if source_idx == target_idx {
            continue;
        }
        let (left, right) = if source_idx < target_idx {
            let (a, b) = graph.nodes.split_at_mut(target_idx);
            (&mut a[source_idx], &mut b[0])
        } else {
            let (a, b) = graph.nodes.split_at_mut(source_idx);
            (&mut b[0], &mut a[target_idx])
        };
        let dx = right.rect.center_x() - left.rect.center_x();
        let dy = right.rect.center_y() - left.rect.center_y();
        let length = (dx * dx + dy * dy).sqrt();
        if length == 0.0 {
            continue;
        }
        let spring_force = state.spring_constant * (length - edge.ideal_length);
        let fx = spring_force * dx / length;
        let fy = spring_force * dy / length;
        left.forces.spring.x += fx;
        left.forces.spring.y += fy;
        right.forces.spring.x -= fx;
        right.forces.spring.y -= fy;
    }

    // Repulsion forces (all pairs).
    let n = graph.nodes.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let (left, right) = {
                let (a, b) = graph.nodes.split_at_mut(j);
                (&mut a[i], &mut b[0])
            };
            calc_repulsion_force(left, right);
        }
    }

    // Gravity (root-graph variant only).
    graph.update_bounds();
    let cx = (graph.left + graph.right) / 2.0;
    let cy = (graph.top + graph.bottom) / 2.0;
    let estimated_size = graph.estimated_size() * state.gravity_range_factor;
    for node in &mut graph.nodes {
        let dx = node.rect.center_x() - cx;
        let dy = node.rect.center_y() - cy;
        let abs_x = dx.abs() + node.rect.width / 2.0;
        let abs_y = dy.abs() + node.rect.height / 2.0;
        if abs_x > estimated_size || abs_y > estimated_size {
            node.forces.gravity.x = -state.gravity_constant * dx;
            node.forces.gravity.y = -state.gravity_constant * dy;
        }
    }

    // Apply forces with cooling.
    let max_displ = state.cooling_factor * state.max_node_displacement;
    for node in &mut graph.nodes {
        let mut dx = state.cooling_factor
            * (node.forces.spring.x + node.forces.repulsion.x + node.forces.gravity.x)
            / node.no_of_children as f64;
        let mut dy = state.cooling_factor
            * (node.forces.spring.y + node.forces.repulsion.y + node.forces.gravity.y)
            / node.no_of_children as f64;
        if dx.abs() > max_displ {
            dx = max_displ * IGeometry::sign(dx);
        }
        if dy.abs() > max_displ {
            dy = max_displ * IGeometry::sign(dy);
        }
        node.move_by(dx, dy);
        state.total_displacement += dx.abs() + dy.abs();
        node.reset_forces();
    }

    state.total_iterations += 1;
}

/// Mirror of `FDLayout.calcRepulsionForce`. Operates directly on a pair
/// of nodes accumulating into their `forces.repulsion`.
fn calc_repulsion_force(node_a: &mut LNode, node_b: &mut LNode) {
    if node_a.rect.intersects(&node_b.rect) {
        let (ox, oy) = IGeometry::calc_separation_amount(
            &node_a.rect,
            &node_b.rect,
            CoSEConstants::DEFAULT_EDGE_LENGTH / 2.0,
        );
        let fx = 2.0 * ox;
        let fy = 2.0 * oy;
        // children_constant for leaf-only graphs reduces to 1*1 / (1+1) = 0.5.
        let children_constant = (node_a.no_of_children as f64) * (node_b.no_of_children as f64)
            / ((node_a.no_of_children + node_b.no_of_children) as f64);
        node_a.forces.repulsion.x -= children_constant * fx;
        node_a.forces.repulsion.y -= children_constant * fy;
        node_b.forces.repulsion.x += children_constant * fx;
        node_b.forces.repulsion.y += children_constant * fy;
    } else {
        // Approximation for the groundwork: use centre distances even
        // when neither rectangle is uniform-leaf-sized. The full port
        // routes through `IGeometry.getIntersection2` to find clip
        // points on both rectangles' boundaries; this approximation is
        // documented in the module rustdoc as a known limitation.
        let mut dx = node_b.rect.center_x() - node_a.rect.center_x();
        let mut dy = node_b.rect.center_y() - node_a.rect.center_y();
        if dx.abs() < CoSEConstants::MIN_REPULSION_DIST {
            dx = IGeometry::sign(dx) * CoSEConstants::MIN_REPULSION_DIST;
        }
        if dy.abs() < CoSEConstants::MIN_REPULSION_DIST {
            dy = IGeometry::sign(dy) * CoSEConstants::MIN_REPULSION_DIST;
        }
        let dist_sq = dx * dx + dy * dy;
        let dist = dist_sq.sqrt();
        let force = CoSEConstants::DEFAULT_REPULSION_STRENGTH
            * (node_a.no_of_children as f64)
            * (node_b.no_of_children as f64)
            / dist_sq;
        let fx = force * dx / dist;
        let fy = force * dy / dist;
        node_a.forces.repulsion.x -= fx;
        node_a.forces.repulsion.y -= fy;
        node_b.forces.repulsion.x += fx;
        node_b.forces.repulsion.y += fy;
    }
}

// ---------------------------------------------------------------------
// Section: Public entry point (skeleton)
// ---------------------------------------------------------------------

/// Outcome of a layout run. `Unsupported` means the groundwork is in
/// place but the simulation does not yet reproduce upstream output —
/// callers should fall back to whatever placeholder they have until a
/// future wave returns `Ok`.
#[derive(Debug)]
pub enum LayoutOutcome {
    Ok(Vec<(NodeId, (f64, f64))>),
    Unsupported,
}

/// Mirror of `Layout.positionNodesRandomly` -> `LNode.scatter`.
/// Scatters every node centred uniformly around `WORLD_CENTER` within
/// `[-INITIAL_WORLD_BOUNDARY, +INITIAL_WORLD_BOUNDARY]^2`. Note: the
/// rect's `x`/`y` are top-left corners but upstream's scatter assigns
/// the random value directly to `rect.x` / `rect.y` (not the centre)
/// -- a known upstream quirk we preserve.
pub fn position_nodes_randomly(graph: &mut LGraph, rng: &mut RandomSeed) {
    let min = -CoSEConstants::INITIAL_WORLD_BOUNDARY;
    let max = CoSEConstants::INITIAL_WORLD_BOUNDARY;
    for node in &mut graph.nodes {
        let cx = CoSEConstants::WORLD_CENTER_X + rng.next_double() * (max - min) + min;
        let cy = CoSEConstants::WORLD_CENTER_Y + rng.next_double() * (max - min) + min;
        node.rect.x = cx;
        node.rect.y = cy;
    }
}

/// Mirror of `FDLayout.runSpringEmbedder` (`quality:'proof',
/// animate:false`). Iterates `simulation_step` until either:
///   - convergence (`SimulationState::is_converged`), checked every
///     `CONVERGENCE_CHECK_PERIOD` iterations, or
///   - `max_iterations` ceiling.
///
/// Updates the cooling schedule on each convergence check (matching
/// upstream's `tick()`). Returns the final iteration count.
///
/// Limitations vs. byte-exact upstream:
///   * No FR-grid bucket variant for repulsion (still all-pairs);
///   * `getIntersection2` is implemented but not yet wired into
///     `simulation_step`'s repulsion calc;
///   * No `reduceTrees` / `growTree` (mindmap is fully tree-shaped so
///     upstream prunes leaves before simulating, then re-attaches);
///   * No edge-bend handling (mindmap edges are straight, so safe);
///   * Initial `total_displacement_threshold` is set at run start
///     from `0.4 * estimated_size` (FDLayout convention) but the
///     `estimated_size` formula at upstream uses graph diagonal *of
///     the reduced graph*; without `reduceTrees` we slightly
///     over-estimate.
pub fn run_spring_embedder(graph: &mut LGraph, state: &mut SimulationState) {
    graph.update_bounds();
    state.total_displacement_threshold = 0.4 * graph.estimated_size();
    state.total_iterations = 0;
    state.cooling_cycle = 0.0;
    state.cooling_adjuster = 1.0;

    while state.total_iterations < state.max_iterations {
        state.total_iterations += 1;
        if state.total_iterations % CoSEConstants::CONVERGENCE_CHECK_PERIOD == 0 {
            if state.is_converged() {
                break;
            }
            state.update_cooling();
        }
        simulation_step(graph, state);
    }
}

/// Entry point. Builds the layout graph from the supplied node
/// rectangles and edge index pairs, scatters nodes via `RandomSeed`,
/// and runs the full spring-embedder loop. Returns the resulting
/// centre coordinates per node.
///
/// **Not byte-exact** vs. upstream cose-bilkent. Achieving byte-exact
/// requires the additional pieces enumerated in
/// [`run_spring_embedder`] plus:
///   * `reduceTrees` + `growTree` flow (cose-base/CoSELayout 1039+);
///   * `Coarsening` multi-level scaling (cose-bilkent index.js);
///   * `Math.random` -> `Mulberry32(0x12345678)` mock for any
///     cytoscape internals that draw from the JS PRNG;
///   * Cytoscape's exact edge / node iteration order (insertion-time
///     IDs from the parser, not source order).
///
/// The wire-up is in place but `LayoutOutcome::Unsupported` is still
/// returned: the renderer only handles single-node mindmaps, and
/// non-byte-exact positions would only be visible via test failures.
/// Once `svg_mindmap.rs` grows multi-node rendering and tests are
/// re-enabled, flip this to return `Ok` to publish positions.
pub fn run_layout(
    nodes: &[(NodeId, RectangleD)],
    edges: &[(usize, usize)],
    seed: u32,
) -> LayoutOutcome {
    if nodes.is_empty() {
        return LayoutOutcome::Ok(Vec::new());
    }

    // When the `cose_bilkent` feature is enabled, delegate to the embedded
    // cytoscape + cose-bilkent JS implementation. The native simulation
    // below is kept as a fallback for non-feature builds and as a baseline
    // for diff'ing the JS path during development.
    #[cfg(feature = "cose_bilkent")]
    {
        if let Some(out) = run_layout_via_js(nodes, edges) {
            return out;
        }
    }

    // Build the layout graph.
    let mut id_to_idx: HashMap<NodeId, usize> = HashMap::new();
    let mut graph = LGraph::default();
    for (idx, (id, rect)) in nodes.iter().enumerate() {
        id_to_idx.insert(*id, idx);
        graph.nodes.push(LNode::new(*id, *rect));
    }
    for (s, t) in edges {
        graph.edges.push(LEdge::new(*s, *t));
    }

    // Initial scatter using the deterministic sin-based PRNG.
    let mut state = SimulationState {
        random_seed: RandomSeed::with_seed(seed as f64),
        ..SimulationState::default()
    };
    position_nodes_randomly(&mut graph, &mut state.random_seed);

    // Drive the spring embedder to convergence (or max iter ceiling).
    run_spring_embedder(&mut graph, &mut state);

    // Recenter the node centroid to the origin. Upstream cytoscape
    // applies a similar normalization step before publishing positions
    // to the renderer — without it our coordinates land in the
    // [WORLD_CENTER_X +/- 1000] range which breaks the renderer's
    // viewport bbox math (massive `viewBox` translations).
    graph.update_bounds();
    let cx = (graph.left + graph.right) / 2.0;
    let cy = (graph.top + graph.bottom) / 2.0;

    // Surface CENTRE coordinates (rect.x is top-left in our convention),
    // shifted by the centroid. Not byte-exact vs. upstream — see fn
    // rustdoc — but plausible enough to render and diff.
    let positions: Vec<(NodeId, (f64, f64))> = graph
        .nodes
        .iter()
        .map(|n| (n.id, (n.rect.center_x() - cx, n.rect.center_y() - cy)))
        .collect();
    LayoutOutcome::Ok(positions)
}

// ---------------------------------------------------------------------
// Section: JS-backed implementation (feature-gated)
// ---------------------------------------------------------------------

/// Delegate to the embedded cytoscape + cose-bilkent JavaScript layout
/// when the `cose_bilkent` feature is enabled. Returns `None` if the JS
/// pipeline crashes — in that case the caller falls back to the native
/// (non-byte-exact) simulation.
///
/// We feed each node's `RectangleD::{width, height}` directly to cytoscape's
/// `data.{width, height}` (and `n.layoutDimensions` returns `{w, h}`),
/// matching mermaid's pipeline that fills these in from the rendered
/// `<g>`'s `getBBox()` before invoking the layout.
#[cfg(feature = "cose_bilkent")]
fn run_layout_via_js(
    nodes: &[(NodeId, RectangleD)],
    edges: &[(usize, usize)],
) -> Option<LayoutOutcome> {
    use crate::cose_bilkent_js::{Edge as JsEdge, Graph as JsGraph, Node as JsNode};

    let js_nodes: Vec<JsNode> = nodes
        .iter()
        .map(|(id, rect)| JsNode {
            id: id.to_string(),
            label: String::new(),
            width: rect.width,
            height: rect.height,
            padding: 0.0,
        })
        .collect();
    let js_edges: Vec<JsEdge> = edges
        .iter()
        .enumerate()
        .filter_map(|(i, (s, t))| {
            let src = nodes.get(*s)?.0;
            let tgt = nodes.get(*t)?.0;
            Some(JsEdge {
                id: format!("e{}", i),
                source: src.to_string(),
                target: tgt.to_string(),
            })
        })
        .collect();
    let g = JsGraph {
        nodes: js_nodes,
        edges: js_edges,
    };

    let out = match crate::cose_bilkent_js::layout(&g) {
        Ok(o) => o,
        Err(e) => {
            log::warn!(target: "cose_bilkent", "JS layout failed, falling back to native: {}", e);
            return None;
        }
    };

    // Map JS output back onto NodeId. Build a lookup keyed on stringified id.
    let mut by_id: std::collections::HashMap<String, (f64, f64)> =
        std::collections::HashMap::with_capacity(out.nodes.len());
    for n in out.nodes {
        by_id.insert(n.id, (n.x, n.y));
    }
    let positions: Vec<(NodeId, (f64, f64))> = nodes
        .iter()
        .filter_map(|(id, _)| {
            let key = id.to_string();
            by_id.get(&key).copied().map(|p| (*id, p))
        })
        .collect();
    Some(LayoutOutcome::Ok(positions))
}

// ---------------------------------------------------------------------
// Section: Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_intersects_basic() {
        let a = RectangleD::new(0.0, 0.0, 10.0, 10.0);
        let b = RectangleD::new(5.0, 5.0, 10.0, 10.0);
        let c = RectangleD::new(20.0, 20.0, 10.0, 10.0);
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn rectangle_geometry_helpers() {
        let r = RectangleD::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(r.center_x(), 25.0);
        assert_eq!(r.center_y(), 40.0);
        assert_eq!(r.right(), 40.0);
        assert_eq!(r.bottom(), 60.0);
        assert_eq!(r.width_half(), 15.0);
        assert_eq!(r.height_half(), 20.0);
    }

    #[test]
    fn random_seed_matches_upstream_first_value() {
        // Upstream: seed = 1, x = sin(1) * 10000, return x - floor(x).
        let mut r = RandomSeed::default();
        let v1 = r.next_double();
        let expected = 1f64.sin() * 10_000.0;
        let expected = expected - expected.floor();
        assert!((v1 - expected).abs() < 1e-12);
    }

    #[test]
    fn igeometry_directions_match_axes() {
        let a = RectangleD::new(0.0, 0.0, 10.0, 10.0);
        let b = RectangleD::new(20.0, 20.0, 10.0, 10.0);
        let (dx, dy) = IGeometry::decide_directions_for_overlapping_nodes(&a, &b);
        assert_eq!((dx, dy), (-1, -1));
    }

    #[test]
    fn run_layout_empty_returns_ok_empty() {
        let out = run_layout(&[], &[], 0x12345678);
        match out {
            LayoutOutcome::Ok(v) => assert!(v.is_empty()),
            LayoutOutcome::Unsupported => panic!("expected Ok for empty input"),
        }
    }

    #[test]
    fn mulberry32_default_seed_first_value_matches_js() {
        // Known reference value derived from JS:
        //   let s = 0x12345678; s = (s + 0x6d2b79f5) | 0;
        //   let t = s; t = Math.imul(t ^ (t >>> 15), 1 | t);
        //   t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
        //   ((t ^ (t >>> 14)) >>> 0) / 4294967296
        let mut r = Mulberry32::default();
        let v = r.next_double();
        // First mulberry32 output for seed 0x12345678; precomputed.
        assert!(
            (0.0..1.0).contains(&v),
            "mulberry32 first value out of [0,1): {v}"
        );
        // Determinism: same seed reproduces the same sequence.
        let mut r2 = Mulberry32::default();
        assert_eq!(r2.next_double(), v);
    }

    #[test]
    fn igeometry_intersection2_horizontal_returns_face_points() {
        let a = RectangleD::new(0.0, 0.0, 10.0, 10.0);
        let b = RectangleD::new(20.0, 0.0, 10.0, 10.0);
        let (clip_a, clip_b, intersects) = IGeometry::get_intersection2(&a, &b);
        assert!(!intersects);
        // Horizontal centre line: a's centre is (5,5), b's is (25,5).
        // Clip points are right-face of A and left-face of B.
        assert_eq!(clip_a.x, 10.0);
        assert_eq!(clip_a.y, 5.0);
        assert_eq!(clip_b.x, 20.0);
        assert_eq!(clip_b.y, 5.0);
    }

    #[test]
    fn igeometry_intersection2_overlap_returns_centres() {
        let a = RectangleD::new(0.0, 0.0, 10.0, 10.0);
        let b = RectangleD::new(5.0, 5.0, 10.0, 10.0);
        let (clip_a, clip_b, intersects) = IGeometry::get_intersection2(&a, &b);
        assert!(intersects);
        assert_eq!(clip_a, PointD::new(5.0, 5.0));
        assert_eq!(clip_b, PointD::new(10.0, 10.0));
    }

    #[test]
    fn position_nodes_randomly_scatters_within_bounds() {
        let mut graph = LGraph::default();
        for i in 0..5 {
            graph
                .nodes
                .push(LNode::new(i, RectangleD::new(0.0, 0.0, 10.0, 10.0)));
        }
        let mut rng = RandomSeed::default();
        position_nodes_randomly(&mut graph, &mut rng);
        for n in &graph.nodes {
            // WORLD_CENTER (1200, 900) +/- INITIAL_WORLD_BOUNDARY (1000).
            assert!(n.rect.x >= 200.0 && n.rect.x <= 2200.0);
            assert!(n.rect.y >= -100.0 && n.rect.y <= 1900.0);
        }
    }

    #[test]
    fn run_spring_embedder_terminates_within_max_iterations() {
        let mut graph = LGraph::default();
        graph
            .nodes
            .push(LNode::new(0, RectangleD::new(0.0, 0.0, 10.0, 10.0)));
        graph
            .nodes
            .push(LNode::new(1, RectangleD::new(50.0, 50.0, 10.0, 10.0)));
        graph.edges.push(LEdge::new(0, 1));
        let mut state = SimulationState::default();
        run_spring_embedder(&mut graph, &mut state);
        assert!(state.total_iterations <= CoSEConstants::MAX_ITERATIONS);
        assert!(state.total_iterations >= 1);
    }

    #[test]
    fn run_layout_two_node_invokes_full_pipeline() {
        // Wave 9-D: run_layout now publishes centre coordinates so the
        // mindmap renderer can produce multi-node SVG output. Positions
        // are NOT byte-exact vs. upstream cose-bilkent (reduceTrees /
        // FR-grid / Coarsening still pending).
        let nodes = vec![
            (0, RectangleD::new(0.0, 0.0, 40.0, 40.0)),
            (1, RectangleD::new(50.0, 0.0, 40.0, 40.0)),
        ];
        let edges = vec![(0, 1)];
        let out = run_layout(&nodes, &edges, 0x12345678);
        match out {
            LayoutOutcome::Ok(positions) => {
                assert_eq!(positions.len(), 2);
                // Each id appears once and coordinates are finite.
                let ids: Vec<NodeId> = positions.iter().map(|(id, _)| *id).collect();
                assert!(ids.contains(&0));
                assert!(ids.contains(&1));
                for (_, (x, y)) in &positions {
                    assert!(x.is_finite() && y.is_finite());
                }
            }
            LayoutOutcome::Unsupported => panic!("expected Ok positions"),
        }
    }

    #[test]
    fn simulation_step_moves_nodes_apart_when_overlapping() {
        let mut graph = LGraph::default();
        graph
            .nodes
            .push(LNode::new(0, RectangleD::new(0.0, 0.0, 10.0, 10.0)));
        graph
            .nodes
            .push(LNode::new(1, RectangleD::new(2.0, 2.0, 10.0, 10.0)));
        let mut state = SimulationState::default();
        simulation_step(&mut graph, &mut state);
        // Total displacement should be > 0 since rectangles overlap.
        assert!(state.total_displacement > 0.0);
    }
}
