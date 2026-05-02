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
//!    `cardinal_direction`.
//! 4. `RandomSeed` — the upstream LCG-style sine PRNG. This stays
//!    separate from the rendering-time mulberry32 PRNG (which seeds
//!    cytoscape's `Math.random` mock); both are required for full
//!    byte-exact parity.
//! 5. A single-iteration force pass (`simulation_step`) computing
//!    spring + repulsion + gravitational forces and applying them with
//!    the cooling factor.
//! 6. A `run_layout` skeleton: builds the data structures, executes
//!    one iteration's worth of forces, and returns `Unsupported`.
//!    **Not byte-exact**; future waves will:
//!      * loop until convergence (`isConverged` + max-iter cap),
//!      * port the FR grid variant for repulsion range,
//!      * port `Coarsening` (multi-level scaling) and the tiling
//!        tree-reduction stage,
//!      * align node-iteration order with cytoscape's insertion order.
//!
//! ## What's left for next wave
//!
//! Roughly in priority order:
//!   * Full `FDLayout::runSpringEmbedder` loop (iteration over all
//!     edges/nodes until `isConverged()`), ported to Rust.
//!   * `CoSELayout`-specific extensions:
//!     `multiLevelScaling`/`Coarsening` (cytoscape-cose-bilkent
//!     `index.js` `coarsen`), incremental layout flag, tree reduction
//!     for the leaves of a star / chain.
//!   * `Tiler` / `Tiling` for compound nodes (not strictly needed for
//!     mindmap — every node is a leaf — but required for `class` and
//!     `flowchart` to share the same layout backend.)
//!   * Wire the mulberry32 seed (`0x12345678`) into a `Math.random`
//!     replacement that drives `LGraph.position` initial randomisation
//!     and `cytoscape.add` ordering.
//!   * Replace `mindmap.rs`'s multi-node placeholder with a real call
//!     into `run_layout`.
//!   * Re-run the 18 KNOWN_IGNORED mindmap fixtures and compare.

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
        let slope = if rect_b.center_x() == rect_a.center_x()
            && rect_b.center_y() == rect_a.center_y()
        {
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
            -1.0 * dir_x as f64 * (move_by_x / 2.0 + separation_buffer),
            -1.0 * dir_y as f64 * (move_by_y / 2.0 + separation_buffer),
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
    pub max_node_displacement: f64,
    pub total_displacement: f64,
    pub total_iterations: usize,
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
            max_node_displacement: CoSEConstants::MAX_NODE_DISPLACEMENT,
            total_displacement: 0.0,
            total_iterations: 0,
        }
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

/// Skeleton entry point. Populates an `LGraph` from the supplied node
/// rectangles and edge index pairs, runs a single simulation step
/// purely to exercise the data path, and returns `Unsupported`. Future
/// waves will replace the body with the full `runSpringEmbedder` loop.
///
/// The `seed` parameter is stored for future use (drives the
/// `RandomSeed` and, eventually, a `Math.random` mock matching mermaid
/// 11.x's `mulberry32(0x12345678)` reseed in
/// `tests/support/generate_ref.mjs`).
pub fn run_layout(
    nodes: &[(NodeId, RectangleD)],
    edges: &[(usize, usize)],
    _seed: u32,
) -> LayoutOutcome {
    if nodes.is_empty() {
        return LayoutOutcome::Ok(Vec::new());
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

    // Smoke-test one iteration so the data path stays exercised; this
    // is *not* the byte-exact output and is discarded.
    let mut state = SimulationState::default();
    simulation_step(&mut graph, &mut state);

    // Until the full simulation loop is ported, mark unsupported.
    let _ = graph;
    LayoutOutcome::Unsupported
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
    fn run_layout_two_node_returns_unsupported_for_now() {
        let nodes = vec![
            (0, RectangleD::new(0.0, 0.0, 10.0, 10.0)),
            (1, RectangleD::new(50.0, 0.0, 10.0, 10.0)),
        ];
        let edges = vec![(0, 1)];
        let out = run_layout(&nodes, &edges, 0x12345678);
        assert!(matches!(out, LayoutOutcome::Unsupported));
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
