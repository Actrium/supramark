//! Port of upstream `rendering-util/types.ts` (209 LoC).
//!
//! Upstream reference:
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/rendering-util/types.ts`
//!
//! This module mirrors the TypeScript shape field-for-field. Every TS
//! optional (`foo?:`) becomes `Option<T>`; every TS required field becomes
//! a required Rust field. The goal is one-to-one fidelity so Stratum 3
//! (er / requirement / class / state / flowchart / block) model-to-layout
//! adapters can populate a `LayoutData` from their parsed model with a
//! minimum of translation.
//!
//! `[key: string]: any` TS escape hatches are represented by `extra:
//! BTreeMap<String, String>` — only string-valued extras are carried.
//! Non-string extras were never relied on by upstream's dagre path.

use std::collections::BTreeMap;

/// Markdown word type — `normal | strong | em`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownWordType {
    Normal,
    Strong,
    Em,
}

/// One word in a markdown line. Mirrors upstream `MarkdownWord`.
#[derive(Debug, Clone)]
pub struct MarkdownWord {
    pub content: String,
    pub kind: MarkdownWordType,
}

/// A line of markdown words. Mirrors upstream `MarkdownLine = MarkdownWord[]`.
pub type MarkdownLine = Vec<MarkdownWord>;

/// A simple 2-D point. Duplicated from upstream `types.ts` (`Point`) to
/// avoid pulling it in from `src/layout/intersect::Point` (which is
/// `(f32, f32)`; upstream uses `{x, y}` with `number` which is f64).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Axis-aligned bounding box. Mirrors upstream `Bounds` from
/// `packages/mermaid/src/types.ts`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Group/cluster children — `NodeChildren = Node[]`.
pub type NodeChildren = Vec<Node>;

/// Pos discriminator for asset-bearing shapes. TS: `pos?: 't' | 'b'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetPos {
    /// 't' — asset above label.
    Top,
    /// 'b' — asset below label.
    Bottom,
}

/// Constraint toggle — TS: `constraint?: 'on' | 'off'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    On,
    Off,
}

/// Unified node. Matches TS `Node = ClusterNode | NonClusterNode`.
///
/// Upstream splits clusters and non-clusters by the `isGroup` discriminant;
/// here we keep them in one struct with `is_group: bool` because every
/// field is otherwise shared and the `shape` value space overlaps in
/// practice (string-typed in upstream's runtime graphlib payload).
///
/// Fields are grouped by the comment sections in `types.ts` for easier
/// cross-referencing against the TS source.
#[derive(Debug, Clone, Default)]
pub struct Node {
    // --- BaseNode common fields -----------------------------------------
    pub id: String,
    pub label: Option<String>,
    pub description: Option<Vec<String>>,
    pub parent_id: Option<String>,
    /// Note-specific: 'left of', 'right of', etc.
    pub position: Option<String>,
    pub css_styles: Option<Vec<String>>,
    pub css_compiled_styles: Option<Vec<String>>,
    pub css_classes: Option<String>,
    pub label_style: Option<String>,

    // --- Flowchart-specific ---------------------------------------------
    pub label_type: Option<String>,

    pub dom_id: Option<String>,
    /// Only relevant for `is_group == true` — composite-state / subgraph dir.
    pub dir: Option<String>,
    pub have_callback: Option<bool>,
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub tooltip: Option<String>,
    pub padding: Option<f64>,
    pub is_group: bool,
    pub width: Option<f64>,
    pub height: Option<f64>,

    // --- Shape geometry -------------------------------------------------
    pub rx: Option<f64>,
    pub ry: Option<f64>,

    pub use_html_labels: Option<bool>,
    pub center_label: Option<bool>,

    // --- Node style properties ------------------------------------------
    pub background_color: Option<String>,
    pub border_color: Option<String>,
    pub border_style: Option<String>,
    pub border_width: Option<f64>,
    pub label_text_color: Option<String>,
    pub label_padding_x: Option<f64>,
    pub label_padding_y: Option<f64>,

    // --- Post-layout coordinates ----------------------------------------
    pub x: Option<f64>,
    pub y: Option<f64>,

    pub look: Option<String>,
    pub icon: Option<String>,
    pub pos: Option<AssetPos>,
    pub img: Option<String>,
    pub asset_width: Option<f64>,
    pub asset_height: Option<f64>,
    pub default_width: Option<f64>,
    pub image_aspect_ratio: Option<f64>,
    pub constraint: Option<Constraint>,
    pub children: Option<NodeChildren>,
    pub node_id: Option<String>,
    pub level: Option<i64>,
    pub descr: Option<String>,
    /// TS uses `type?: number` for certain diagram families (e.g. kanban);
    /// kept as an i64 rather than collapsed into the variant-string `shape`.
    pub kind: Option<i64>,
    pub radius: Option<f64>,
    pub taper: Option<f64>,
    pub stroke: Option<String>,
    pub color_index: Option<i64>,

    /// ShapeID / ClusterShapeID. Upstream's type is a union of string
    /// literals (dozens of shape IDs); we keep it as a free-form string
    /// because the shape registry validates at render time.
    pub shape: Option<String>,

    /// Post-layout bounding box of the rendered label — needed by dagre
    /// cluster padding math. Not in `types.ts` but used throughout
    /// `index.js` as `labelBBox`.
    pub label_bbox: Option<Bounds>,

    /// Cluster "y offset" computed during recursive-render.
    pub offset_y: Option<f64>,
    /// Height delta assigned to compound nodes after child layout
    /// (`node.diff` in upstream).
    pub diff: Option<f64>,

    /// Everything not yet promoted to a typed field — string-valued
    /// escape hatch for diagram-specific extras.
    pub extra: BTreeMap<String, String>,
}

/// Edge. Matches TS `Edge` (`types.ts` lines 108-149).
#[derive(Debug, Clone, Default)]
pub struct Edge {
    pub id: String,
    pub label: Option<String>,
    pub classes: Option<String>,
    pub style: Option<Vec<String>>,
    pub animate: Option<bool>,
    /// `'fast' | 'slow'`.
    pub animation: Option<String>,

    // --- Common to Flowchart + State ------------------------------------
    pub arrowhead: Option<String>,
    pub arrowhead_style: Option<String>,
    pub arrow_type_end: Option<String>,
    pub arrow_type_start: Option<String>,
    pub css_compiled_styles: Option<Vec<String>>,

    // --- Flowchart-specific ---------------------------------------------
    pub default_interpolate: Option<String>,
    pub end: Option<String>,
    pub interpolate: Option<String>,
    pub label_type: Option<String>,
    pub length: Option<f64>,
    pub start: Option<String>,
    pub stroke: Option<String>,
    pub text: Option<String>,
    /// TS `type?: string` — not to be confused with `Node::kind`.
    pub kind: Option<String>,

    // --- Class Diagram specific -----------------------------------------
    pub start_label_right: Option<String>,
    pub end_label_left: Option<String>,

    // --- Rendering-specific ---------------------------------------------
    pub curve: Option<String>,
    pub labelpos: Option<String>,
    pub label_style: Option<Vec<String>>,
    pub minlen: Option<i32>,
    pub pattern: Option<String>,
    /// `'normal' | 'thick' | 'invisible' | 'dotted'`.
    pub thickness: Option<String>,
    pub look: Option<String>,
    pub is_user_defined_id: Option<bool>,
    /// Post-layout spline waypoints. Populated by dagre.
    pub points: Option<Vec<Point>>,
    pub parent_id: Option<String>,
    pub dir: Option<String>,

    /// Graphlib source/target. Upstream's dagre adapter reads `start` /
    /// `end` for flowchart and `source` / `target` for newer paths. We
    /// carry both; the bridge uses whichever is set.
    pub source: Option<String>,
    pub target: Option<String>,
    pub depth: Option<i32>,

    /// Post-layout label centre (set by dagre's edge-label placement).
    pub label_x: Option<f64>,
    pub label_y: Option<f64>,

    /// Extras — as with `Node::extra`.
    pub extra: BTreeMap<String, String>,
}

/// A cluster/group shadow — useful when a layout wants to expose a
/// dedicated cluster map rather than inspecting `Node::is_group`. This is
/// NOT in `types.ts` but mirrors the `clusterDb` shape built at runtime
/// inside `mermaid-graphlib.js`.
#[derive(Debug, Clone, Default)]
pub struct Cluster {
    pub id: String,
    /// The non-cluster child dagre routes edges through — upstream's
    /// `findNonClusterChild` result.
    pub representative: Option<String>,
    /// Post-layout bounds.
    pub bounds: Option<Bounds>,
}

/// Mirrors `RectOptions`. Kept for future shape work.
#[derive(Debug, Clone, Copy)]
pub struct RectOptions {
    pub rx: f64,
    pub ry: f64,
    pub label_padding_x: f64,
    pub label_padding_y: f64,
}

/// Mirrors `MindmapOptions`.
#[derive(Debug, Clone, Copy)]
pub struct MindmapOptions {
    pub padding: f64,
}

/// Mirrors `ShapeRenderOptions`. `config` is implicit in our pipeline —
/// theme + config live beside layout, not embedded.
#[derive(Debug, Clone)]
pub struct ShapeRenderOptions {
    pub dir: Option<String>,
    pub padding: Option<f64>,
}

/// Available layout engines. `dagre-wrapper` is a historical alias dagre;
/// `elk` and the others are upstream-only for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMethod {
    Dagre,
    DagreWrapper,
    Elk,
    Neato,
    Dot,
    Circo,
    Fdp,
    Osage,
    Grid,
}

impl LayoutMethod {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "dagre" => Some(Self::Dagre),
            "dagre-wrapper" => Some(Self::DagreWrapper),
            "elk" => Some(Self::Elk),
            "neato" => Some(Self::Neato),
            "dot" => Some(Self::Dot),
            "circo" => Some(Self::Circo),
            "fdp" => Some(Self::Fdp),
            "osage" => Some(Self::Osage),
            "grid" => Some(Self::Grid),
            _ => None,
        }
    }
}

/// The unified input to every layout engine. Ports upstream
/// `LayoutData` (`types.ts` line 169).
///
/// Fields not in `types.ts` but read off `data4Layout` inside `dagre/index.js`:
///
/// * `direction` — graph-level `rankdir`;
/// * `node_spacing` / `rank_spacing` — fallbacks for when
///   `config.flowchart.*Spacing` is absent;
/// * `markers` — list of marker IDs to register up-front;
/// * `diagram_type` — `"flowchart" | "classDiagram" | ...`;
/// * `layout_algorithm` — e.g. `"dagre"`.
#[derive(Debug, Clone, Default)]
pub struct LayoutData {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub diagram_id: Option<String>,

    // Extras pulled off `data4Layout` at render-time (not on the base TS
    // `LayoutData`, but set by every diagram's `getData` call).
    pub diagram_type: Option<String>,
    pub direction: Option<String>,
    pub node_spacing: Option<f64>,
    pub rank_spacing: Option<f64>,
    pub markers: Vec<String>,
    pub layout_algorithm: Option<String>,

    /// Diagram-family extras. Upstream uses `[key: string]: any` — we
    /// restrict to string-valued extras.
    pub extra: BTreeMap<String, String>,
}

/// Output of a layout run. Not in `types.ts` (upstream mutates the
/// input), but we keep input immutable and return a new struct so the
/// Rust API stays borrow-checker-friendly.
#[derive(Debug, Clone, Default)]
pub struct LayoutResult {
    /// Post-layout nodes. Same order as `LayoutData::nodes`.
    pub nodes: Vec<Node>,
    /// Post-layout edges. Same order as `LayoutData::edges`.
    pub edges: Vec<Edge>,
    /// Cluster bounds (derived from post-layout compound nodes).
    pub clusters: Vec<Cluster>,
    /// Overall graph bounds (tight AABB around all nodes + edge points).
    pub bounds: Bounds,
    /// IDs of clusters that were laid out via the recursive inner-layout
    /// algorithm (no cross-boundary edges). These clusters are rendered as
    /// inner `<g class="root">` wrappers in the `<g class="nodes">` section,
    /// rather than as entries in the `<g class="clusters">` section.
    pub isolated_cluster_ids: std::collections::HashSet<String>,
}

/// Post-layout render-data envelope. Matches TS `RenderData` shape, kept
/// here so Stratum 3 renderers can hand a uniform `items` list to SVG
/// emission code. Not used by the dagre bridge itself.
#[derive(Debug, Clone, Default)]
pub struct RenderData {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub clusters: Vec<Cluster>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_default_is_not_a_group() {
        let n = Node::default();
        assert!(!n.is_group);
        assert!(n.x.is_none());
        assert!(n.label.is_none());
    }

    #[test]
    fn layout_method_parse_round_trips_known_names() {
        assert_eq!(LayoutMethod::parse("dagre"), Some(LayoutMethod::Dagre));
        assert_eq!(LayoutMethod::parse("elk"), Some(LayoutMethod::Elk));
        assert_eq!(LayoutMethod::parse("handdrawn"), None);
    }

    #[test]
    fn extra_survives_clone() {
        let mut d = LayoutData::default();
        d.extra.insert("foo".into(), "bar".into());
        let c = d.clone();
        assert_eq!(c.extra.get("foo").map(String::as_str), Some("bar"));
    }
}
