//! Dagre adapter — glue between our `unified::LayoutData` and the
//! `dagre` crate (`/ext/dagre`).
//!
//! Upstream references:
//! * `rendering-util/layout-algorithms/dagre/index.ts`        (379 LoC)
//! * `rendering-util/layout-algorithms/dagre/mermaid-graphlib.ts` (413 LoC)
//!
//! Responsibilities:
//! 1. Populate a `dagre::graph::Graph<NodeLabel, EdgeLabel>` from our
//!    `LayoutData` — compound when any node has `parent_id`, simple
//!    otherwise.
//! 2. Self-edges get expanded into two helper nodes + three stitched
//!    edges, matching `index.ts`'s handling (lines 308-364 upstream).
//!    For Wave 3 P0 we keep the expansion simple — the rendered self-loop
//!    geometry refinement happens in `routing.rs`.
//! 3. Run `dagre::layout(&mut g, opts)`.
//! 4. Copy post-layout coordinates back to a fresh `LayoutResult`.
//!
//! ## Recursive cluster layout (isolated subgraphs)
//!
//! Upstream mermaid v11 uses a recursive rendering strategy for clusters
//! that have no edges crossing their boundary ("isolated clusters").
//! The `adjustClustersAndEdges` / `extractor` functions in
//! `mermaid-graphlib.ts` extract these clusters into a separate inner
//! graph with `rankdir` flipped (TB → LR) and `ranksep += 25`.
//! Then `recursiveRender` runs dagre on the inner graph with the
//! cluster as a compound parent, and the resulting dimensions feed
//! back as the cluster leaf node size for the outer dagre pass.
//!
//! We replicate this behaviour here:
//! - Isolated clusters are detected by checking whether any edge
//!   crosses the cluster boundary.
//! - For each isolated cluster, an inner compound dagre is run with
//!   the perpendicular direction and ranksep + 25.
//! - The inner dagre result gives the cluster's w/h and the final
//!   positions of its children.
//! - The outer dagre is then run with the cluster as a simple
//!   (non-compound) leaf node of those computed dimensions.

use crate::error::Result;
use crate::layout::routing;
use crate::layout::unified::{Bounds, Cluster, Edge, LayoutData, LayoutResult, Node, Point};
use crate::theme::ThemeVariables;

use dagre::graph::{Graph, GraphOptions};
use dagre::layout::types::{EdgeLabel, LabelPos, LayoutOptions, NodeLabel, RankDir};

/// Default node box size when a diagram failed to size-measure its label
/// before handing us a `LayoutData`. Matches upstream's fallback where
/// `node.width / node.height` default to 0 and dagre treats them as
/// point-sized — which degenerates to coincident coords and is rarely
/// what a renderer wants, so we nudge to something sensible.
const DEFAULT_NODE_WIDTH: f64 = 80.0;
const DEFAULT_NODE_HEIGHT: f64 = 40.0;

/// Parse upstream's `rankdir` strings — "TB" / "BT" / "LR" / "RL".
/// Upstream also accepts the flowchart aliases "TD" (= "TB") and the
/// lowercase spellings; we cover those too.
fn parse_rankdir(s: Option<&str>) -> RankDir {
    match s.map(str::trim).map(str::to_ascii_uppercase).as_deref() {
        Some("BT") => RankDir::BT,
        Some("LR") => RankDir::LR,
        Some("RL") => RankDir::RL,
        // "TB" and "TD" and the absent case all map to top-bottom.
        _ => RankDir::TB,
    }
}

/// Determine whether the graph has any parent-child relationships — if
/// yes dagre must run in compound mode.
fn is_compound(data: &LayoutData) -> bool {
    data.nodes.iter().any(|n| n.parent_id.is_some())
}

/// Build the layout options from `LayoutData` + defaults. Mirrors
/// upstream `index.ts` lines 272-291's `.setGraph({...})` call.
fn build_layout_options(data: &LayoutData) -> LayoutOptions {
    LayoutOptions {
        rankdir: parse_rankdir(data.direction.as_deref()),
        nodesep: data.node_spacing.unwrap_or(50.0),
        ranksep: data.rank_spacing.unwrap_or(50.0),
        // Upstream hard-codes these to 8 at the top-level graph.
        marginx: 8.0,
        marginy: 8.0,
        // dagre-d3-es v7.0.14 (the version mermaid uses) does NOT update
        // the best layering when the crossing count is tied — it keeps the
        // first one. The newer @dagrejs/dagre (v3.0.1-pre) replaces best
        // on ties. Our dagre-rs defaults to the newer behavior
        // (`tie_keep_first = false`), but mermaid's actual dagre behaves
        // like `tie_keep_first = true`. This flag is the single biggest
        // source of coordinate divergence for multi-rank graphs with
        // multiple nodes per rank (e.g. ER/03, flowchart with branches).
        tie_keep_first: true,
        ..LayoutOptions::default()
    }
}

/// Build a dagre NodeLabel populated with just the fields dagre cares
/// about (width/height/labelpos/padding). Shape/label rendering fields
/// are carried outside dagre — we re-attach them from `LayoutData` when
/// building the `LayoutResult`.
fn make_node_label(node: &Node) -> NodeLabel {
    NodeLabel {
        width: node.width.unwrap_or(DEFAULT_NODE_WIDTH),
        height: node.height.unwrap_or(DEFAULT_NODE_HEIGHT),
        label: node.label.clone(),
        padding: node.padding.unwrap_or(0.0),
        padding_x: node.label_padding_x,
        padding_y: node.label_padding_y,
        rx: node.rx,
        ry: node.ry,
        shape: node.shape.clone(),
        class: node.css_classes.clone(),
        ..NodeLabel::default()
    }
}

/// Build a dagre EdgeLabel. Only a handful of fields feed into dagre's
/// layout proper (`minlen`, `weight`, `width`, `height`, `labelpos`);
/// everything else rides back on the user-facing `Edge`.
fn make_edge_label(edge: &Edge) -> EdgeLabel {
    // Diagrams (ER in particular) stash `label_width` / `label_height`
    // into `Edge::extra` before calling the bridge so dagre reserves a
    // rank row for the edge label. Fall back to 0 — unchanged default
    // behaviour — when the keys are absent.
    let w = edge
        .extra
        .get("label_width")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let h = edge
        .extra
        .get("label_height")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    // labelpos — upstream `Edge.labelpos` is `'l' | 'r' | 'c'`; default 'r'
    // in dagre, but mermaid's flowchart / ER renderers use 'c' (centre) so
    // the edge label sits ON the spline, not offset to the side.
    let labelpos = match edge.labelpos.as_deref() {
        Some("l") => LabelPos::Left,
        Some("c") => LabelPos::Center,
        Some("r") => LabelPos::Right,
        _ => LabelPos::Right,
    };
    EdgeLabel {
        minlen: edge.minlen.unwrap_or(1),
        weight: 1,
        width: w,
        height: h,
        labelpos,
        ..EdgeLabel::default()
    }
}

/// Resolve an edge's source node id. Upstream uses `edge.start` for
/// flowchart and `edge.source` for newer diagrams — we accept whichever
/// is populated, preferring `start` to match the dagre/index.ts call
/// site (`graph.setEdge(edge.start, edge.end, ...)`).
fn edge_source<'a>(e: &'a Edge) -> Option<&'a str> {
    e.start.as_deref().or(e.source.as_deref())
}

/// Symmetric to [`edge_source`].
fn edge_target<'a>(e: &'a Edge) -> Option<&'a str> {
    e.end.as_deref().or(e.target.as_deref())
}

/// Populate a dagre graph from a `LayoutData`. Self-edges are expanded
/// using the upstream pattern (two label-rect helper nodes + three
/// stitched edges).
fn build_graph(data: &LayoutData) -> Graph<NodeLabel, EdgeLabel> {
    build_graph_filtered(data, &std::collections::HashSet::new())
}

/// Variant of `build_graph` that skips nodes whose ids are in `excluded`.
/// Used by the outer pass when isolated clusters have pre-computed layouts
/// and their children are excluded from the outer dagre graph.
fn build_graph_filtered<'a>(
    data: &LayoutData,
    excluded: &std::collections::HashSet<&'a str>,
) -> Graph<NodeLabel, EdgeLabel> {
    build_graph_filtered_ex(data, excluded, &std::collections::HashMap::new())
}

/// Like `build_graph_filtered` but also skips edges whose BOTH effective
/// endpoints are inside the same isolated cluster. Used by the outer compound
/// pass to drop intra-cluster edges (a→b, i→f) that would confuse dagre rank
/// computation while still keeping cluster children as compound children for
/// bounding-box purposes.
///
/// `isolated_descendants`: for each isolated cluster id, the set of all
/// descendant node ids (not including the cluster itself).
fn build_graph_filtered_ex<'a>(
    data: &LayoutData,
    excluded: &std::collections::HashSet<&'a str>,
    isolated_descendants: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> Graph<NodeLabel, EdgeLabel> {
    let opts = GraphOptions {
        directed: true,
        multigraph: true,
        compound: data.nodes.iter().any(|n| {
            n.parent_id.is_some()
                && !excluded.contains(n.id.as_str())
                && !excluded.contains(n.parent_id.as_deref().unwrap_or(""))
        }),
    };
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(opts);

    for node in &data.nodes {
        if excluded.contains(node.id.as_str()) {
            continue;
        }
        g.set_node(node.id.clone(), Some(make_node_label(node)));
    }
    if g.is_compound() {
        for node in &data.nodes {
            if excluded.contains(node.id.as_str()) {
                continue;
            }
            if let Some(parent) = node.parent_id.as_deref() {
                if !excluded.contains(parent) {
                    g.set_parent(&node.id, Some(parent));
                }
            }
        }
    }

    // Collect the set of all isolated cluster ids for quick lookup.
    let isolated_cluster_ids: std::collections::HashSet<&str> =
        isolated_descendants.keys().map(|s| s.as_str()).collect();

    for edge in &data.edges {
        // When an edge was originally cluster-to-cluster (orig_start/orig_end
        // are both cluster ids present in the graph), use those original ids.
        // This ensures that after isolated-cluster retargeting (A→B becomes
        // a→i, and a,i are excluded), the cluster super-nodes A and B still
        // receive the edge in the outer graph.
        let orig_src = edge.extra.get("orig_start").map(|s| s.as_str());
        let orig_dst = edge.extra.get("orig_end").map(|s| s.as_str());
        let (effective_src, effective_dst): (&str, &str) =
            if let (Some(os), Some(od)) = (orig_src, orig_dst) {
                if g.has_node(os) && g.has_node(od) {
                    // Both original cluster endpoints are in the outer graph:
                    // restore the cluster-level edge.
                    (os, od)
                } else {
                    let (Some(src), Some(dst)) = (edge_source(edge), edge_target(edge)) else {
                        log::warn!(
                            "dagre_bridge: edge '{}' missing start/end (source/target); skipped",
                            edge.id
                        );
                        continue;
                    };
                    (src, dst)
                }
            } else {
                let (Some(src), Some(dst)) = (edge_source(edge), edge_target(edge)) else {
                    log::warn!(
                        "dagre_bridge: edge '{}' missing start/end (source/target); skipped",
                        edge.id
                    );
                    continue;
                };
                (src, dst)
            };

        // Skip edges where either endpoint is excluded (and not restored above).
        if excluded.contains(effective_src) || excluded.contains(effective_dst) {
            continue;
        }

        // Skip intra-isolated-cluster edges (both endpoints inside the same
        // isolated cluster). These edges are already accounted for by the inner
        // dagre pass; including them in the outer compound graph would cause
        // dagre-rs to try to rank the cluster children against each other
        // alongside the cluster-level A→B edge, which is unsupported.
        // EXCEPTION: if both effective endpoints are isolated cluster ids
        // themselves (e.g. effective_src=="A", effective_dst=="B"), keep the edge.
        let both_are_iso_clusters = isolated_cluster_ids.contains(effective_src)
            && isolated_cluster_ids.contains(effective_dst);
        if !both_are_iso_clusters {
            // Check if both endpoints are descendants of the same isolated cluster.
            let mut same_iso = false;
            for (_iso_id, desc) in isolated_descendants {
                let src_in = desc.contains(effective_src);
                let dst_in = desc.contains(effective_dst);
                if src_in && dst_in {
                    same_iso = true;
                    break;
                }
            }
            if same_iso {
                continue;
            }
        }

        if effective_src == effective_dst {
            // Self-edge expansion — see upstream index.ts:308-364.
            expand_self_edge(&mut g, edge, effective_src);
        } else {
            let name = if edge.id.is_empty() {
                None
            } else {
                Some(edge.id.as_str())
            };
            g.set_edge(effective_src, effective_dst, Some(make_edge_label(edge)), name);
        }
    }

    g
}

/// Insert two helper nodes and three edges so dagre has something to
/// rank for a self-edge. Port of upstream `index.ts:308-364`, trimmed
/// to the ranking essentials — visual self-loop smoothing is the job
/// of `routing::smooth_self_loop` later.
fn expand_self_edge(g: &mut Graph<NodeLabel, EdgeLabel>, edge: &Edge, node_id: &str) {
    let sid1 = format!("{node_id}---{node_id}---1");
    let sid2 = format!("{node_id}---{node_id}---2");

    let helper = || NodeLabel {
        width: 10.0,
        height: 10.0,
        label: Some(String::new()),
        padding: 0.0,
        shape: Some("labelRect".to_string()),
        class: None,
        ..NodeLabel::default()
    };
    g.set_node(sid1.clone(), Some(helper()));
    g.set_node(sid2.clone(), Some(helper()));

    // Mirror parent-id when inside a cluster.
    if g.is_compound() {
        if let Some(parent) = g.parent(node_id).map(|s| s.to_string()) {
            g.set_parent(&sid1, Some(&parent));
            g.set_parent(&sid2, Some(&parent));
        }
    }

    let base_label = make_edge_label(edge);
    g.set_edge(
        node_id,
        &sid1,
        Some(base_label.clone()),
        Some(&format!("{node_id}-cyclic-special-0")),
    );
    g.set_edge(
        &sid1,
        &sid2,
        Some(base_label.clone()),
        Some(&format!("{node_id}-cyclic-special-1")),
    );
    g.set_edge(
        &sid2,
        node_id,
        Some(base_label),
        Some(&format!("{node_id}-cyclic-special-2")),
    );
}

/// Pull post-layout coordinates out of `g` and paint them back onto a
/// fresh copy of `data.nodes`, preserving original index order.
fn collect_nodes(data: &LayoutData, g: &Graph<NodeLabel, EdgeLabel>) -> Vec<Node> {
    data.nodes
        .iter()
        .map(|orig| {
            let mut out = orig.clone();
            if let Some(lbl) = g.node(&orig.id) {
                out.x = lbl.x;
                out.y = lbl.y;
                // Dagre may have widened a compound node while packing
                // children — honour the updated width/height.
                out.width = Some(lbl.width);
                out.height = Some(lbl.height);
            }
            out
        })
        .collect()
}

/// Pull post-layout edge spline points + label centres.
fn collect_edges(data: &LayoutData, g: &Graph<NodeLabel, EdgeLabel>) -> Vec<Edge> {
    data.edges
        .iter()
        .map(|orig| {
            let mut out = orig.clone();
            let (Some(src_raw), Some(dst_raw)) = (edge_source(orig), edge_target(orig)) else {
                return out;
            };
            if src_raw == dst_raw {
                // Self-edges were expanded; leave routing to
                // `routing::smooth_self_loop` which regenerates them
                // from the node bounds rather than from the helper chain.
                return out;
            }
            // Edges whose effective endpoints were remapped to cluster ids
            // (e.g. A→B from orig_start/orig_end) are stored in `g` under
            // the cluster ids, not the raw retargeted endpoints.
            let orig_src = orig.extra.get("orig_start").map(|s| s.as_str());
            let orig_dst = orig.extra.get("orig_end").map(|s| s.as_str());
            let (eff_src, eff_dst) = if let (Some(os), Some(od)) = (orig_src, orig_dst) {
                if g.has_node(os) && g.has_node(od) { (os, od) } else { (src_raw, dst_raw) }
            } else {
                (src_raw, dst_raw)
            };
            let name = if orig.id.is_empty() {
                None
            } else {
                Some(orig.id.as_str())
            };
            if let Some(lbl) = g.edge(eff_src, eff_dst, name) {
                out.points = Some(
                    lbl.points
                        .iter()
                        .map(|p| Point { x: p.x, y: p.y })
                        .collect(),
                );
                out.label_x = lbl.x;
                out.label_y = lbl.y;
            }
            out
        })
        .collect()
}

/// Derive cluster metadata from compound-node bounds.
fn collect_clusters(nodes: &[Node]) -> Vec<Cluster> {
    nodes
        .iter()
        .filter(|n| n.is_group)
        .map(|n| Cluster {
            id: n.id.clone(),
            representative: None,
            bounds: match (n.x, n.y, n.width, n.height) {
                (Some(x), Some(y), Some(w), Some(h)) => Some(Bounds {
                    x: x - w / 2.0,
                    y: y - h / 2.0,
                    width: w,
                    height: h,
                }),
                _ => None,
            },
        })
        .collect()
}

/// Compute a tight AABB over all post-layout nodes + edge spline points.
fn compute_bounds(nodes: &[Node], edges: &[Edge]) -> Bounds {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for n in nodes {
        let (Some(x), Some(y)) = (n.x, n.y) else {
            continue;
        };
        let w = n.width.unwrap_or(0.0);
        let h = n.height.unwrap_or(0.0);
        min_x = min_x.min(x - w / 2.0);
        min_y = min_y.min(y - h / 2.0);
        max_x = max_x.max(x + w / 2.0);
        max_y = max_y.max(y + h / 2.0);
    }
    for e in edges {
        let Some(points) = e.points.as_ref() else {
            continue;
        };
        for p in points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
    }

    if !min_x.is_finite() {
        return Bounds::default();
    }
    Bounds {
        x: min_x,
        y: min_y,
        width: (max_x - min_x).max(0.0),
        height: (max_y - min_y).max(0.0),
    }
}

/// Return the opposite `RankDir` — used when building the inner cluster
/// graph. Upstream: `extractor` line 339:
///   `let dir = graphSettings.rankdir === 'TB' ? 'LR' : 'TB'`
fn opposite_rankdir(rd: RankDir) -> RankDir {
    match rd {
        RankDir::TB => RankDir::LR,
        RankDir::BT => RankDir::RL,
        RankDir::LR => RankDir::TB,
        RankDir::RL => RankDir::BT,
    }
}

/// Collect the set of top-level (root) cluster ids in `data` — clusters
/// whose own `parent_id` is `None` or whose parent is not itself a cluster.
fn root_cluster_ids(data: &LayoutData) -> Vec<String> {
    use std::collections::HashSet;
    let cluster_ids: HashSet<&str> = data
        .nodes
        .iter()
        .filter(|n| n.is_group)
        .map(|n| n.id.as_str())
        .collect();
    data.nodes
        .iter()
        .filter(|n| {
            n.is_group
                && !n
                    .parent_id
                    .as_deref()
                    .map(|p| cluster_ids.contains(p))
                    .unwrap_or(false)
        })
        .map(|n| n.id.clone())
        .collect()
}

/// Return the direct children (non-cluster node ids) of a given cluster.
fn direct_children<'a>(cluster_id: &str, data: &'a LayoutData) -> Vec<&'a Node> {
    data.nodes
        .iter()
        .filter(|n| n.parent_id.as_deref() == Some(cluster_id) && !n.is_group)
        .collect()
}

/// Check if a cluster is "isolated" — none of its (transitively) contained
/// nodes appear as an endpoint of an edge that connects outside the cluster.
///
/// Upstream: `adjustClustersAndEdges` calls `extractor` only for clusters
/// where every edge with a member source/target also has the other endpoint
/// inside the same cluster subtree.
fn is_isolated_cluster(cluster_id: &str, data: &LayoutData) -> bool {
    use std::collections::HashSet;
    // Gather all node ids that are (directly or transitively) inside this cluster.
    let mut members: HashSet<&str> = HashSet::new();
    // BFS/DFS: include direct children and children of sub-clusters.
    let mut queue: Vec<&str> = vec![cluster_id];
    while let Some(cid) = queue.pop() {
        members.insert(cid);
        for n in &data.nodes {
            if n.parent_id.as_deref() == Some(cid) {
                members.insert(n.id.as_str());
                if n.is_group {
                    queue.push(n.id.as_str());
                }
            }
        }
    }
    // Collect the set of all cluster ids for the orig_start/orig_end check below.
    let cluster_ids: HashSet<&str> = data.nodes.iter()
        .filter(|n| n.is_group)
        .map(|n| n.id.as_str())
        .collect();
    // Check every edge: if one endpoint is in `members` and the other is not,
    // the cluster is NOT isolated.
    // IMPORTANT: use orig_start/orig_end (pre-retargeting) when available so
    // that cluster-to-cluster edges (e.g. "A --> B") do not falsely appear to
    // cross the boundary.  An edge whose BOTH original endpoints are cluster ids
    // represents a super-node connection that does not penetrate either cluster's
    // interior, so it must not prevent either cluster from being isolated.
    for edge in &data.edges {
        let orig_src = edge.extra.get("orig_start").map(|s| s.as_str());
        let orig_dst = edge.extra.get("orig_end").map(|s| s.as_str());
        // If the original endpoints are available AND both are cluster ids,
        // this is a cluster-to-cluster edge — skip the boundary check.
        if let (Some(os), Some(od)) = (orig_src, orig_dst) {
            if cluster_ids.contains(os) && cluster_ids.contains(od) {
                continue;
            }
        }
        // Use original endpoints for the boundary check when available;
        // fall back to post-retarget endpoints.
        let src = orig_src.or_else(|| edge_source(edge));
        let dst = orig_dst.or_else(|| edge_target(edge));
        let src_in = src.map(|s| members.contains(s)).unwrap_or(false);
        let dst_in = dst.map(|s| members.contains(s)).unwrap_or(false);
        if src_in != dst_in {
            return false;
        }
    }
    true
}

/// Result of an inner compound dagre pass for one isolated cluster.
/// Recursive: isolated sub-clusters within this cluster have their own
/// `InnerLayout` entries in `sub_isolated`.
struct InnerLayout {
    /// The cluster's computed width (from the compound dagre).
    cluster_width: f64,
    /// The cluster's computed height.
    cluster_height: f64,
    /// Inner dagre x-position of the cluster compound node (its center).
    /// Used as the cluster's x/cx in the inner render (for the cluster rect).
    inner_x: f64,
    /// Inner dagre y-position of the cluster compound node (its center).
    inner_y: f64,
    /// Outer dagre leaf node width for this cluster.
    ///
    /// Upstream mermaid renders the isolated cluster inner SVG, then calls
    /// `updateNodeBounds(node, el)` which sets `node.width = getBBox(el).width`.
    /// The getBBox is computed WITHOUT applying any transforms (jsdom shim
    /// behavior), so it is the union of:
    ///   - The cluster rect local coords: [8, 8+cluster_width]
    ///   - Each leaf node's local rect: [-w/2, +w/2]
    ///
    /// Result: `bbox_width = (8 + cluster_width) + max(node.width/2)`
    bbox_width: f64,
    /// Outer dagre leaf node height: `(8 + cluster_height) + max(node.height/2)`.
    bbox_height: f64,
    /// Post-layout (x, y) for ALL direct children (leaf nodes and
    /// sub-cluster leaf representations).  For isolated sub-clusters
    /// the position is what O's inner dagre assigned; actual children of
    /// isolated sub-clusters are carried in their own `InnerLayout`.
    /// Value: (x, y, w, h) as returned by the inner dagre.
    child_positions: std::collections::HashMap<String, (f64, f64, f64, f64)>,
    /// Recursive inner layouts for sub-clusters that are isolated within
    /// this cluster's context.  Keyed by sub-cluster id.
    sub_isolated: std::collections::HashMap<String, InnerLayout>,
}

/// Collect every node id that is (directly or transitively) inside
/// `cluster_id` in `data`.  The cluster itself is not included.
fn all_descendants(cluster_id: &str, data: &LayoutData) -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    let mut members: HashSet<String> = HashSet::new();
    let mut queue: Vec<&str> = vec![cluster_id];
    while let Some(cid) = queue.pop() {
        for n in &data.nodes {
            if n.parent_id.as_deref() == Some(cid) {
                members.insert(n.id.clone());
                if n.is_group {
                    queue.push(n.id.as_str());
                }
            }
        }
    }
    members
}

/// Check whether a sub-cluster is isolated *within the context of its parent
/// cluster* — i.e. no edge crosses the sub-cluster boundary when only looking
/// at edges that are entirely contained within `parent_members`.
fn is_isolated_within(
    sub_cluster_id: &str,
    parent_members: &std::collections::HashSet<String>,
    data: &LayoutData,
) -> bool {
    // All nodes that are descendants of sub_cluster_id (within the parent).
    let sub_members: std::collections::HashSet<&str> = {
        let mut m = std::collections::HashSet::new();
        m.insert(sub_cluster_id);
        let mut queue = vec![sub_cluster_id];
        while let Some(cid) = queue.pop() {
            for n in &data.nodes {
                if n.parent_id.as_deref() == Some(cid)
                    && parent_members.contains(n.id.as_str())
                {
                    m.insert(n.id.as_str());
                    if n.is_group {
                        queue.push(n.id.as_str());
                    }
                }
            }
        }
        m
    };

    // Collect cluster ids for the orig-endpoint cluster-to-cluster skip below.
    let cluster_ids: std::collections::HashSet<&str> = data.nodes.iter()
        .filter(|n| n.is_group)
        .map(|n| n.id.as_str())
        .collect();

    for edge in &data.edges {
        // Use original pre-retarget endpoints when available.
        let orig_src = edge.extra.get("orig_start").map(|s| s.as_str());
        let orig_dst = edge.extra.get("orig_end").map(|s| s.as_str());
        // Cluster-to-cluster edges do not cross interior boundaries.
        if let (Some(os), Some(od)) = (orig_src, orig_dst) {
            if cluster_ids.contains(os) && cluster_ids.contains(od) {
                continue;
            }
        }
        let src = match orig_src.or_else(|| edge_source(edge)) {
            Some(s) => s,
            None => continue,
        };
        let dst = match orig_dst.or_else(|| edge_target(edge)) {
            Some(s) => s,
            None => continue,
        };
        // Only look at edges both of whose endpoints are in parent_members.
        if !parent_members.contains(src) || !parent_members.contains(dst) {
            continue;
        }
        // If exactly one endpoint is in sub_members the cluster is not isolated.
        let src_in = sub_members.contains(src);
        let dst_in = sub_members.contains(dst);
        if src_in != dst_in {
            return false;
        }
    }
    true
}

/// Run an inner compound dagre for an isolated cluster, recursively handling
/// any isolated sub-clusters within it.
///
/// Mirrors upstream `extractor` + `recursiveRender`:
/// - For each direct cluster-child that is isolated within this context:
///   recursively compute its inner layout (using `inner_rankdir` as outer
///   for the next level, `inner_ranksep` as outer_ranksep).
/// - Non-isolated cluster-children participate as compound nodes.
/// - Leaf children participate as leaf nodes.
/// - `rankdir` = opposite of `outer_rankdir`.
/// - `ranksep` = `outer_ranksep + 25`.
fn layout_isolated_cluster(
    cluster_id: &str,
    data: &LayoutData,
    outer_rankdir: RankDir,
    outer_ranksep: f64,
) -> InnerLayout {
    let inner_rankdir = opposite_rankdir(outer_rankdir);
    let inner_ranksep = outer_ranksep + 25.0;
    let inner_nodesep = data.node_spacing.unwrap_or(50.0);

    // All nodes that live inside this cluster (transitively).
    let descendants = all_descendants(cluster_id, data);

    // Direct children: both leaf nodes and cluster children.
    let leaf_children: Vec<&Node> = data
        .nodes
        .iter()
        .filter(|n| n.parent_id.as_deref() == Some(cluster_id) && !n.is_group)
        .collect();
    let cluster_children: Vec<&Node> = data
        .nodes
        .iter()
        .filter(|n| n.parent_id.as_deref() == Some(cluster_id) && n.is_group)
        .collect();

    // For each direct cluster-child, decide: isolated or compound?
    let mut sub_isolated: std::collections::HashMap<String, InnerLayout> =
        std::collections::HashMap::new();
    let mut non_isolated_cluster_children: Vec<&Node> = Vec::new();

    // The "parent members" for isolation checks = descendants + cluster_id itself.
    let mut parent_members = descendants.clone();
    parent_members.insert(cluster_id.to_string());

    for cc in &cluster_children {
        if is_isolated_within(&cc.id, &parent_members, data) {
            let inner = layout_isolated_cluster(
                &cc.id,
                data,
                inner_rankdir,
                inner_ranksep,
            );
            sub_isolated.insert(cc.id.clone(), inner);
        } else {
            non_isolated_cluster_children.push(cc);
        }
    }

    // Build the inner dagre graph.
    let opts = GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    };
    let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(opts);

    // Add the cluster node itself as compound root.
    g.set_node(cluster_id.to_string(), Some(NodeLabel {
        width: 0.0,
        height: 0.0,
        ..NodeLabel::default()
    }));

    // Add leaf children.
    for child in &leaf_children {
        g.set_node(child.id.clone(), Some(make_node_label(child)));
        g.set_parent(&child.id, Some(cluster_id));
    }

    // Add isolated sub-clusters as opaque leaf nodes with pre-computed dims.
    for (cid, inner) in &sub_isolated {
        let lbl = NodeLabel {
            width: inner.cluster_width,
            height: inner.cluster_height,
            ..NodeLabel::default()
        };
        g.set_node(cid.clone(), Some(lbl));
        g.set_parent(cid, Some(cluster_id));
    }

    // Add non-isolated cluster children as compound sub-graphs.
    // Each such cluster and all its descendants go in as compound nodes.
    for cc in &non_isolated_cluster_children {
        // Add the cluster node itself.
        g.set_node(cc.id.clone(), Some(NodeLabel {
            width: 0.0,
            height: 0.0,
            ..NodeLabel::default()
        }));
        g.set_parent(&cc.id, Some(cluster_id));
        // Recursively add all descendants of this non-isolated cluster.
        let mut stack: Vec<&str> = vec![cc.id.as_str()];
        while let Some(pid) = stack.pop() {
            for n in &data.nodes {
                if n.parent_id.as_deref() == Some(pid) {
                    if n.is_group {
                        g.set_node(n.id.clone(), Some(NodeLabel {
                            width: 0.0,
                            height: 0.0,
                            ..NodeLabel::default()
                        }));
                        g.set_parent(&n.id, Some(pid));
                        stack.push(n.id.as_str());
                    } else {
                        g.set_node(n.id.clone(), Some(make_node_label(n)));
                        g.set_parent(&n.id, Some(pid));
                    }
                }
            }
        }
    }

    // Collect all node ids that are in this dagre graph (excluding the
    // cluster_id itself for edge purposes).
    let graph_node_ids: std::collections::HashSet<String> = {
        let mut s = std::collections::HashSet::new();
        for lc in &leaf_children { s.insert(lc.id.clone()); }
        for cid in sub_isolated.keys() { s.insert(cid.clone()); }
        for cc in &non_isolated_cluster_children {
            s.insert(cc.id.clone());
            let desc = all_descendants(&cc.id, data);
            s.extend(desc);
        }
        s
    };

    // Add edges whose both endpoints are in graph_node_ids.
    for edge in &data.edges {
        let src = match edge_source(edge) {
            Some(s) => s,
            None => continue,
        };
        let dst = match edge_target(edge) {
            Some(s) => s,
            None => continue,
        };
        if !graph_node_ids.contains(src) || !graph_node_ids.contains(dst) {
            continue;
        }
        if src == dst {
            expand_self_edge(&mut g, edge, src);
        } else {
            let name = if edge.id.is_empty() {
                None
            } else {
                Some(edge.id.as_str())
            };
            g.set_edge(src, dst, Some(make_edge_label(edge)), name);
        }
    }

    let inner_opts = LayoutOptions {
        rankdir: inner_rankdir,
        nodesep: inner_nodesep,
        ranksep: inner_ranksep,
        marginx: 8.0,
        marginy: 8.0,
        tie_keep_first: true,
        ..LayoutOptions::default()
    };
    dagre::layout(&mut g, Some(inner_opts));

    // Read back cluster dimensions (the cluster node's rect, excluding inner margins).
    let (cluster_width, cluster_height, inner_x, inner_y) = if let Some(lbl) = g.node(cluster_id) {
        (lbl.width, lbl.height, lbl.x.unwrap_or(0.0), lbl.y.unwrap_or(0.0))
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    // Read back positions (and dagre-computed dimensions) for all nodes in this dagre.
    let mut child_positions = std::collections::HashMap::new();
    for child in &leaf_children {
        if let Some(lbl) = g.node(&child.id) {
            child_positions.insert(
                child.id.clone(),
                (lbl.x.unwrap_or(0.0), lbl.y.unwrap_or(0.0), lbl.width, lbl.height),
            );
        }
    }
    for cid in sub_isolated.keys() {
        if let Some(lbl) = g.node(cid) {
            child_positions.insert(
                cid.clone(),
                (lbl.x.unwrap_or(0.0), lbl.y.unwrap_or(0.0), lbl.width, lbl.height),
            );
        }
    }
    for cc in &non_isolated_cluster_children {
        if let Some(lbl) = g.node(&cc.id) {
            child_positions.insert(
                cc.id.clone(),
                (lbl.x.unwrap_or(0.0), lbl.y.unwrap_or(0.0), lbl.width, lbl.height),
            );
        }
        // Also collect positions for all descendants of non-isolated clusters.
        let mut stack: Vec<&str> = vec![cc.id.as_str()];
        while let Some(pid) = stack.pop() {
            for n in &data.nodes {
                if n.parent_id.as_deref() == Some(pid) {
                    if let Some(lbl) = g.node(&n.id) {
                        child_positions.insert(
                            n.id.clone(),
                            (lbl.x.unwrap_or(0.0), lbl.y.unwrap_or(0.0), lbl.width, lbl.height),
                        );
                    }
                    if n.is_group {
                        stack.push(n.id.as_str());
                    }
                }
            }
        }
    }

    // Compute the outer dagre leaf node dimensions: these match the bounding box
    // that upstream's `updateNodeBounds(node, el)` would compute from the inner
    // rendered SVG via `el.getBBox()`. The jsdom shim returns the union of all
    // child elements' local bounding boxes WITHOUT applying any transforms.
    //
    // Union bbox components (in the inner dagre coordinate space):
    //   - Cluster rect: [8, 8+cluster_width] × [8, 8+cluster_height]  (absolute)
    //   - Each leaf node's local rect: [-w/2, w/2] × [-h/2, h/2]  (transforms ignored)
    //   - Label foreignObjects: [0, lw] × [0, lh]  (transforms ignored; lw small for short labels)
    //   - Edge paths: absolute inner dagre coords (within the cluster rect range)
    //
    // The label foreignObjects typically have small widths (single char labels) that
    // do not exceed the cluster rect's right edge. Edge paths fall within the rect.
    // So the binding constraints are the cluster rect right and the node rects left:
    //   bbox_width  = (8 + cluster_width)  - min(0, -max_half_node_w)
    //              = (8 + cluster_width)  + max_half_node_w
    //   bbox_height = (8 + cluster_height) + max_half_node_h
    let inner_margin = 8.0; // inner dagre marginx/marginy
    let max_half_node_w = {
        let mut v: f64 = 0.0;
        for child in &leaf_children {
            if let Some(lbl) = g.node(&child.id) {
                v = v.max(lbl.width / 2.0);
            }
        }
        // Sub-isolated clusters appear as leaf nodes with preset dims.
        for (cid, _) in &sub_isolated {
            if let Some(lbl) = g.node(cid) {
                v = v.max(lbl.width / 2.0);
            }
        }
        // Non-isolated cluster children — use the compound bbox from dagre.
        for cc in &non_isolated_cluster_children {
            if let Some(lbl) = g.node(&cc.id) {
                v = v.max(lbl.width / 2.0);
            }
        }
        v
    };
    let max_half_node_h = {
        let mut v: f64 = 0.0;
        for child in &leaf_children {
            if let Some(lbl) = g.node(&child.id) {
                v = v.max(lbl.height / 2.0);
            }
        }
        for (cid, _) in &sub_isolated {
            if let Some(lbl) = g.node(cid) {
                v = v.max(lbl.height / 2.0);
            }
        }
        for cc in &non_isolated_cluster_children {
            if let Some(lbl) = g.node(&cc.id) {
                v = v.max(lbl.height / 2.0);
            }
        }
        v
    };
    let bbox_width  = inner_margin + cluster_width  + max_half_node_w;
    let bbox_height = inner_margin + cluster_height + max_half_node_h;

    log::debug!(
        "dagre_bridge: inner layout for isolated cluster '{}': cluster_w={}, cluster_h={}, \
         inner_x={}, inner_y={}, bbox_w={}, bbox_h={}, sub_isolated={:?}",
        cluster_id, cluster_width, cluster_height,
        inner_x, inner_y, bbox_width, bbox_height,
        sub_isolated.keys().collect::<Vec<_>>(),
    );

    InnerLayout {
        cluster_width,
        cluster_height,
        inner_x,
        inner_y,
        bbox_width,
        bbox_height,
        child_positions,
        sub_isolated,
    }
}

/// Public entry — run the dagre layout on a `LayoutData`, return the
/// geometry. Upstream analogue: `render.ts::render` + `dagre/index.ts::render`.
pub fn layout(data: &LayoutData, _theme: &ThemeVariables) -> Result<LayoutResult> {
    // Degenerate shortcut: empty graph — dagre handles it, but bypass to
    // save the pipeline overhead and keep the tests snappy.
    if data.nodes.is_empty() {
        return Ok(LayoutResult::default());
    }

    log::debug!(
        "dagre_bridge: laying out {} node(s), {} edge(s), compound={}",
        data.nodes.len(),
        data.edges.len(),
        is_compound(data)
    );

    let outer_rankdir = parse_rankdir(data.direction.as_deref());
    let outer_ranksep = data.rank_spacing.unwrap_or(50.0);

    // --- Isolated cluster pre-pass -------------------------------------------
    // For each root-level cluster that has no cross-boundary edges, run an
    // inner compound dagre (opposite rankdir, ranksep+25) to compute the
    // cluster's final dimensions and child positions.
    // The cluster then enters the outer dagre as a plain (non-compound) leaf
    // node with those pre-computed dimensions.
    let root_clusters = root_cluster_ids(data);
    let mut isolated_layouts: std::collections::HashMap<String, InnerLayout> =
        std::collections::HashMap::new();
    for cid in &root_clusters {
        if is_isolated_cluster(cid, data) {
            let inner = layout_isolated_cluster(cid, data, outer_rankdir, outer_ranksep);
            isolated_layouts.insert(cid.clone(), inner);
        }
    }

    // Collect ALL isolated cluster ids (top-level and nested) and their
    // positions from the recursive inner layouts.
    // Also build a flat map: node_id → (x, y) for all nodes inside any
    // isolated cluster at any nesting level.
    let mut all_isolated_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    // Maps node_id → (x, y, w, h) as computed by the inner dagre.
    let mut inner_positions: std::collections::HashMap<String, (f64, f64, f64, f64)> =
        std::collections::HashMap::new();

    fn collect_inner(
        inner: &InnerLayout,
        cluster_id: &str,
        all_iso: &mut std::collections::HashSet<String>,
        positions: &mut std::collections::HashMap<String, (f64, f64, f64, f64)>,
    ) {
        all_iso.insert(cluster_id.to_string());
        for (id, &(cx, cy, w, h)) in &inner.child_positions {
            positions.insert(id.clone(), (cx, cy, w, h));
        }
        for (sub_id, sub_inner) in &inner.sub_isolated {
            collect_inner(sub_inner, sub_id, all_iso, positions);
        }
    }

    for (cid, inner) in &isolated_layouts {
        collect_inner(inner, cid, &mut all_isolated_ids, &mut inner_positions);
    }

    // --- Outer dagre (simple leaf mode) --------------------------------------
    // Isolated clusters are excluded from the outer graph as compound parents;
    // instead each top-level isolated cluster enters the outer dagre as a plain
    // leaf node whose dimensions are the inner render's bounding box.
    //
    // This matches upstream mermaid's `recursiveRender` → `updateNodeBounds`
    // pattern: after the inner cluster is rendered, `node.width/height` are
    // overwritten with `getBBox(innerEl).width/height`, and the outer dagre
    // receives a simple leaf node of those dimensions.
    //
    // All descendants of isolated clusters are excluded from the outer graph
    // since they are already placed by the inner dagre pass.
    let mut excluded_node_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for cid in isolated_layouts.keys() {
        for desc_id in all_descendants(cid, data) {
            excluded_node_ids.insert(desc_id);
        }
    }

    // Build an outer LayoutData where each top-level isolated cluster is
    // represented as a leaf node with bbox_width/bbox_height, and its children
    // are excluded.
    let mut outer_nodes: Vec<Node> = Vec::new();
    for n in &data.nodes {
        if excluded_node_ids.contains(&n.id) {
            // Descendant of isolated cluster — handled by inner dagre.
            continue;
        }
        if let Some(il) = isolated_layouts.get(&n.id) {
            // Top-level isolated cluster: replace with a bbox-sized leaf node.
            let mut leaf = n.clone();
            leaf.width = Some(il.bbox_width);
            leaf.height = Some(il.bbox_height);
            leaf.parent_id = None; // no longer a compound parent in outer
            leaf.is_group = false;
            outer_nodes.push(leaf);
        } else {
            outer_nodes.push(n.clone());
        }
    }
    let outer_data = crate::layout::unified::LayoutData {
        nodes: outer_nodes,
        ..data.clone()
    };

    let excluded_refs: std::collections::HashSet<&str> =
        excluded_node_ids.iter().map(|s| s.as_str()).collect();
    let mut g = build_graph_filtered(
        &outer_data,
        &excluded_refs,
    );
    let opts = build_layout_options(&outer_data);
    dagre::layout(&mut g, Some(opts));

    // Helper: find InnerLayout for any cluster id (at any nesting level).
    fn find_inner_layout<'a>(
        cid: &str,
        map: &'a std::collections::HashMap<String, InnerLayout>,
    ) -> Option<&'a InnerLayout> {
        if let Some(il) = map.get(cid) {
            return Some(il);
        }
        for il in map.values() {
            if let found @ Some(_) = find_inner_layout(cid, &il.sub_isolated) {
                return found;
            }
        }
        None
    }

    // --- Collect results ------------------------------------------------------
    // is_isolated_descendant: a node that is inside an isolated cluster (but
    // is NOT the cluster itself) — its final position comes from the inner
    // dagre pass, not the outer one.
    let is_isolated_descendant = |node: &Node| -> bool {
        inner_positions.contains_key(node.id.as_str())
            && !all_isolated_ids.contains(&node.id)
    };

    let cluster_padding = 8.0_f64; // upstream insertCluster padding

    let mut nodes: Vec<Node> = data
        .nodes
        .iter()
        .map(|orig| {
            let mut out = orig.clone();
            if is_isolated_descendant(orig) {
                // Leaf node (or non-top-level cluster) inside an isolated cluster:
                // position comes from the inner dagre pass.
                if let Some(&(cx, cy, w, h)) = inner_positions.get(&orig.id) {
                    out.x = Some(cx);
                    out.y = Some(cy);
                    // Use dagre-computed dimensions for cluster-children that are
                    // themselves compound (their sizes are resolved by the inner pass).
                    if orig.is_group {
                        out.width = Some(w);
                        out.height = Some(h);
                    }
                    // Keep leaf node's original (label-measured) dimensions.
                }
            } else if all_isolated_ids.contains(&orig.id) {
                // Isolated cluster (top-level or nested).
                if let Some(il) = find_inner_layout(&orig.id, &isolated_layouts) {
                    // Set cluster's x/y to its inner dagre center position.
                    // The render uses cnode.x/y to draw the cluster rect, so we
                    // store the inner dagre compound-center here.
                    out.x = Some(il.inner_x);
                    out.y = Some(il.inner_y);
                    out.width = Some(il.cluster_width);
                    out.height = Some(il.cluster_height);
                }
                // For top-level isolated clusters, compute the outer translate
                // (tx, ty) from the outer dagre position and store in extra.
                // Downstream render uses these directly as the <g transform="translate">.
                //
                // Formula (upstream positionNode with diff = -padding = -8):
                //   tx = outer_x + diff - bbox_width/2 = outer_x - 8 - bbox_w/2
                //   ty = outer_y - bbox_h/2 - padding
                let parent_is_isolated = orig.parent_id.as_deref()
                    .map(|p| all_isolated_ids.contains(p))
                    .unwrap_or(false);
                if !parent_is_isolated {
                    if let Some(lbl) = g.node(&orig.id) {
                        if let Some(il) = find_inner_layout(&orig.id, &isolated_layouts) {
                            let outer_x = lbl.x.unwrap_or(0.0);
                            let outer_y = lbl.y.unwrap_or(0.0);
                            let tx = outer_x - cluster_padding - il.bbox_width / 2.0;
                            let ty = outer_y - il.bbox_height / 2.0 - cluster_padding;
                            out.extra.insert("outer_tx".to_string(), tx.to_string());
                            out.extra.insert("outer_ty".to_string(), ty.to_string());
                            log::debug!(
                                "dagre_bridge: isolated cluster '{}' outer_x={} outer_y={} \
                                 bbox_w={} bbox_h={} → tx={} ty={}",
                                orig.id, outer_x, outer_y,
                                il.bbox_width, il.bbox_height, tx, ty
                            );
                        }
                    }
                }
                out.is_group = orig.is_group;
                out.parent_id = orig.parent_id.clone();
            } else if let Some(lbl) = g.node(&orig.id) {
                out.x = lbl.x;
                out.y = lbl.y;
                out.width = Some(lbl.width);
                out.height = Some(lbl.height);
            }
            out
        })
        .collect();

    let edges_pre = collect_edges(data, &g);
    let edges = routing::refine_edges(&nodes, &edges_pre);
    let clusters = collect_clusters(&nodes);
    let bounds = compute_bounds(&nodes, &edges);

    Ok(LayoutResult {
        nodes,
        edges,
        clusters,
        bounds,
        isolated_cluster_ids: all_isolated_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::unified::{Edge, LayoutData, Node};
    use crate::theme::ThemeVariables;

    fn two_node_graph() -> LayoutData {
        let mut a = Node::default();
        a.id = "a".into();
        a.label = Some("A".into());
        a.width = Some(60.0);
        a.height = Some(30.0);

        let mut b = Node::default();
        b.id = "b".into();
        b.label = Some("B".into());
        b.width = Some(60.0);
        b.height = Some(30.0);

        let mut e = Edge::default();
        e.id = "e1".into();
        e.start = Some("a".into());
        e.end = Some("b".into());

        LayoutData {
            nodes: vec![a, b],
            edges: vec![e],
            direction: Some("TB".into()),
            ..LayoutData::default()
        }
    }

    #[test]
    fn two_node_pipeline_assigns_coordinates() {
        let data = two_node_graph();
        let theme = ThemeVariables::default();
        let out = layout(&data, &theme).expect("layout");

        assert_eq!(out.nodes.len(), 2);
        let a = &out.nodes[0];
        let b = &out.nodes[1];
        assert!(a.x.is_some() && a.y.is_some(), "a coords populated");
        assert!(b.x.is_some() && b.y.is_some(), "b coords populated");

        // TB layout: `b` is below `a` (larger y).
        assert!(
            b.y.unwrap() > a.y.unwrap(),
            "TB means target below source: a.y={:?} b.y={:?}",
            a.y,
            b.y
        );
        // And roughly centre-aligned on x (same width, no siblings).
        assert!(
            (a.x.unwrap() - b.x.unwrap()).abs() < 1e-6,
            "TB centres x: a.x={:?} b.x={:?}",
            a.x,
            b.x
        );

        // The edge should have waypoints connecting the two centres.
        let edge = &out.edges[0];
        let points = edge.points.as_ref().expect("edge points set");
        assert!(points.len() >= 2, "at least endpoints on the spline");
        let first = points.first().unwrap();
        let last = points.last().unwrap();
        assert!(first.y < last.y, "edge points go from A toward B downward");
    }

    #[test]
    fn empty_graph_returns_empty_result() {
        let data = LayoutData::default();
        let theme = ThemeVariables::default();
        let out = layout(&data, &theme).expect("empty");
        assert!(out.nodes.is_empty());
        assert!(out.edges.is_empty());
        assert!(out.clusters.is_empty());
    }

    #[test]
    fn lr_direction_orients_horizontally() {
        let mut data = two_node_graph();
        data.direction = Some("LR".into());
        let theme = ThemeVariables::default();
        let out = layout(&data, &theme).expect("layout");

        let a = &out.nodes[0];
        let b = &out.nodes[1];
        assert!(
            b.x.unwrap() > a.x.unwrap(),
            "LR means target right of source: a.x={:?} b.x={:?}",
            a.x,
            b.x
        );
        assert!(
            (a.y.unwrap() - b.y.unwrap()).abs() < 1e-6,
            "LR centres y: a.y={:?} b.y={:?}",
            a.y,
            b.y
        );
    }

    #[test]
    fn missing_endpoints_skip_gracefully() {
        let mut a = Node::default();
        a.id = "a".into();
        a.width = Some(40.0);
        a.height = Some(20.0);

        let bogus = Edge {
            id: "bad".into(),
            ..Edge::default()
        };

        let data = LayoutData {
            nodes: vec![a],
            edges: vec![bogus],
            ..LayoutData::default()
        };
        let theme = ThemeVariables::default();
        // Must not panic; the unmapped edge should just be carried
        // through without points.
        let out = layout(&data, &theme).expect("layout");
        assert_eq!(out.edges.len(), 1);
        assert!(out.edges[0].points.is_none());
    }
}
