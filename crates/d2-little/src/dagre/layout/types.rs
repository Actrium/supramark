use std::collections::HashMap;

/// Options for the layout algorithm.
#[derive(Debug, Clone)]
pub struct LayoutOptions {
    pub rankdir: RankDir,
    pub align: Option<Align>,
    pub nodesep: f64,
    pub edgesep: f64,
    pub ranksep: f64,
    pub marginx: f64,
    pub marginy: f64,
    pub acyclicer: Option<Acyclicer>,
    pub ranker: Ranker,
    pub rank_align: RankAlign,
    /// When the crossing-reduction phase produces multiple sweeps with the
    /// same crossing count, dagre.js v3.0.1-pre keeps the *last* tied
    /// layering, while v0.8.5 (the version bundled by Go d2 v0.7.1) keeps
    /// the *first*. Set this to `true` to match v0.8.5 behavior — required
    /// for byte-identical layouts when interoperating with Go d2.
    pub tie_keep_first: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            rankdir: RankDir::TB,
            align: None,
            nodesep: 50.0,
            edgesep: 20.0,
            ranksep: 50.0,
            marginx: 0.0,
            marginy: 0.0,
            acyclicer: None,
            ranker: Ranker::NetworkSimplex,
            rank_align: RankAlign::Center,
            tie_keep_first: false,
        }
    }
}

/// A 2D point with floating-point coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A self-edge saved during layout for later reinsertion.
#[derive(Debug, Clone)]
pub struct SelfEdge {
    pub e: crate::dagre::graph::Edge,
    pub label: EdgeLabel,
}

/// Direction for rank layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RankDir {
    #[default]
    TB,
    BT,
    LR,
    RL,
}

/// Ranker algorithm selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ranker {
    #[default]
    NetworkSimplex,
    TightTree,
    LongestPath,
}

/// Acyclic algorithm selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acyclicer {
    Greedy,
}

/// Alignment option for rank alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    UL,
    UR,
    DL,
    DR,
}

/// Vertical alignment within each rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RankAlign {
    Top,
    #[default]
    Center,
    Bottom,
}

/// Graph-level label with layout configuration.
#[derive(Debug, Clone)]
pub struct GraphLabel {
    pub width: f64,
    pub height: f64,
    pub compound: bool,
    pub rankdir: RankDir,
    pub align: Option<Align>,
    pub nodesep: f64,
    pub edgesep: f64,
    pub ranksep: f64,
    pub rank_align: RankAlign,
    pub marginx: f64,
    pub marginy: f64,
    pub acyclicer: Option<Acyclicer>,
    pub ranker: Ranker,
    pub nesting_root: Option<String>,
    pub node_rank_factor: Option<f64>,
    pub dummy_chains: Vec<String>,
    pub max_rank: Option<i32>,
    /// See `LayoutOptions::tie_keep_first` — carried on the graph label so
    /// the order phase can read it without threading it through every call.
    pub tie_keep_first: bool,
}

impl Default for GraphLabel {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            compound: false,
            rankdir: RankDir::TB,
            align: None,
            nodesep: 50.0,
            edgesep: 20.0,
            ranksep: 50.0,
            rank_align: RankAlign::Center,
            marginx: 0.0,
            marginy: 0.0,
            acyclicer: None,
            ranker: Ranker::NetworkSimplex,
            nesting_root: None,
            // dagre.js always creates compound layout graph + runs nestingGraph.run()
            // which sets nodeRankFactor=1 minimum. This prevents removeEmptyRanks
            // from collapsing intermediate ranks used for edge label placement.
            node_rank_factor: Some(1.0),
            dummy_chains: Vec::new(),
            max_rank: None,
            tie_keep_first: false,
        }
    }
}

/// Border type for border nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderType {
    Left,
    Right,
}

/// Label position for edge labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelPos {
    Left,
    #[default]
    Center,
    Right,
}

/// Node label with all properties used during layout.
#[derive(Debug, Clone)]
pub struct NodeLabel {
    pub width: f64,
    pub height: f64,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub rank: Option<i32>,
    pub order: Option<usize>,
    pub e: Option<f64>,
    /// Dummy node type: "edge", "border", "edge-label", "edge-proxy", "selfedge", "root"
    pub dummy: Option<String>,
    pub border_type: Option<BorderType>,
    pub border_top: Option<String>,
    pub border_bottom: Option<String>,
    pub border_left: Vec<String>,
    pub border_right: Vec<String>,
    pub min_rank: Option<i32>,
    pub max_rank: Option<i32>,
    pub label: Option<String>,
    pub labelpos: LabelPos,
    pub class: Option<String>,
    pub padding: f64,
    pub padding_x: Option<f64>,
    pub padding_y: Option<f64>,
    pub rx: Option<f64>,
    pub ry: Option<f64>,
    pub shape: Option<String>,
    pub edge_label: Option<Box<EdgeLabel>>,
    pub edge_obj: Option<crate::dagre::graph::Edge>,
    /// Self-edges removed during layout and reinserted later.
    pub self_edges: Vec<SelfEdge>,
    /// For "selfedge" dummy nodes: the original edge descriptor.
    pub self_edge_data_e: Option<crate::dagre::graph::Edge>,
    /// For "selfedge" dummy nodes: the original edge label.
    pub self_edge_data_label: Option<EdgeLabel>,
    /// Extra properties that don't have dedicated fields.
    pub extra: HashMap<String, String>,
}

impl Default for NodeLabel {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            x: None,
            y: None,
            rank: None,
            order: None,
            e: None,
            dummy: None,
            border_type: None,
            border_top: None,
            border_bottom: None,
            border_left: Vec::new(),
            border_right: Vec::new(),
            min_rank: None,
            max_rank: None,
            label: None,
            labelpos: LabelPos::Center,
            class: None,
            padding: 0.0,
            padding_x: None,
            padding_y: None,
            rx: None,
            ry: None,
            shape: None,
            edge_label: None,
            edge_obj: None,
            self_edges: Vec::new(),
            self_edge_data_e: None,
            self_edge_data_label: None,
            extra: HashMap::new(),
        }
    }
}

/// Edge label with all properties used during layout.
#[derive(Debug, Clone)]
pub struct EdgeLabel {
    pub minlen: i32,
    pub weight: i32,
    pub width: f64,
    pub height: f64,
    pub label_offset: f64,
    pub labelpos: LabelPos,
    pub label_rank: Option<f64>,
    pub points: Vec<Point>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub e: Option<f64>,
    pub reversed: bool,
    pub forward_name: Option<String>,
    pub self_edge: bool,
    pub nesting_edge: bool,
    pub cutvalue: Option<i32>,
    pub lim: Option<i32>,
    pub low: Option<i32>,
    pub parent: Option<String>,
    pub edge_label: Option<Box<EdgeLabel>>,
    pub edge_obj: Option<crate::dagre::graph::Edge>,
}

impl Default for EdgeLabel {
    fn default() -> Self {
        Self {
            minlen: 1,
            weight: 1,
            width: 0.0,
            height: 0.0,
            label_offset: 10.0,
            labelpos: LabelPos::Right,
            label_rank: None,
            points: Vec::new(),
            x: None,
            y: None,
            e: None,
            reversed: false,
            forward_name: None,
            self_edge: false,
            nesting_edge: false,
            cutvalue: None,
            lim: None,
            low: None,
            parent: None,
            edge_label: None,
            edge_obj: None,
        }
    }
}
