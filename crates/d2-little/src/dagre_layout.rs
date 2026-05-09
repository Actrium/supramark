//! d2-dagre-layout: bridge between d2-graph and the dagre-rs layout engine.
//!
//! Ported from Go `d2layouts/d2dagrelayout/layout.go`.

use std::collections::{HashMap, HashSet};

use crate::geo::{Point, Segment};
use crate::graph::{Graph, ObjId};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Minimum rank separation (used in post-processing adjustments).
#[allow(dead_code)]
const MIN_RANK_SEP: i32 = 60;
const EDGE_LABEL_GAP: i32 = 20;
const CONSTANT_NEAR_PAD: f64 = 20.0;
/// Default padding around container contents.
#[allow(dead_code)]
const DEFAULT_PADDING: f64 = 30.0;
/// Minimum spacing between elements.
#[allow(dead_code)]
const MIN_SPACING: f64 = 10.0;

// ---------------------------------------------------------------------------
// ConfigurableOpts
// ---------------------------------------------------------------------------

/// User-configurable layout options.
#[derive(Debug, Clone)]
pub struct ConfigurableOpts {
    pub node_sep: i32,
    pub edge_sep: i32,
}

impl Default for ConfigurableOpts {
    fn default() -> Self {
        Self {
            node_sep: 60,
            edge_sep: 20,
        }
    }
}

// ---------------------------------------------------------------------------
// ObjectMapper
// ---------------------------------------------------------------------------

/// Maps between d2 object indices and dagre node string IDs.
struct ObjectMapper {
    obj_to_id: HashMap<ObjId, String>,
    id_to_obj: HashMap<String, ObjId>,
    counter: usize,
}

impl ObjectMapper {
    fn new() -> Self {
        Self {
            obj_to_id: HashMap::new(),
            id_to_obj: HashMap::new(),
            counter: 0,
        }
    }

    fn register(&mut self, obj_id: ObjId) {
        let dagre_id = self.counter.to_string();
        self.obj_to_id.insert(obj_id, dagre_id.clone());
        self.id_to_obj.insert(dagre_id, obj_id);
        self.counter += 1;
    }

    fn to_dagre_id(&self, obj_id: ObjId) -> &str {
        &self.obj_to_id[&obj_id]
    }

    fn dagre_id(&self, obj_id: ObjId) -> Option<&str> {
        self.obj_to_id.get(&obj_id).map(String::as_str)
    }

    #[allow(dead_code)]
    fn to_obj_id(&self, dagre_id: &str) -> ObjId {
        self.id_to_obj[dagre_id]
    }
}

// ---------------------------------------------------------------------------
// Edge endpoint resolution (container -> descendant)
// ---------------------------------------------------------------------------

/// Find the effective endpoints for an edge, routing through containers.
/// dagre cannot handle edges to containers, so we route to leaf descendants.
fn get_edge_endpoints(g: &Graph, edge_idx: usize) -> (ObjId, ObjId) {
    let edge = &g.edges[edge_idx];
    let mut src = edge.src;
    let mut dst = edge.dst;

    // Route container edges to their first/last child
    while g.objects[src].is_container()
        && g.objects[src].class.is_none()
        && g.objects[src].sql_table.is_none()
    {
        src = get_longest_edge_chain_tail(g, src);
    }
    while g.objects[dst].is_container()
        && g.objects[dst].class.is_none()
        && g.objects[dst].sql_table.is_none()
    {
        dst = get_longest_edge_chain_head(g, dst);
    }

    // For reverse arrows (b <- a), swap endpoints
    if edge.src_arrow && !edge.dst_arrow {
        std::mem::swap(&mut src, &mut dst);
    }
    (src, dst)
}

/// Check if `obj` is equal to or a descendant of `container`.
fn in_container(obj_id: ObjId, container_id: ObjId, g: &Graph) -> Option<ObjId> {
    in_container_depth(obj_id, container_id, g, 0)
}

fn in_container_depth(
    obj_id: ObjId,
    container_id: ObjId,
    g: &Graph,
    depth: usize,
) -> Option<ObjId> {
    if depth > 100 {
        return None; // prevent infinite recursion from cyclic parent pointers
    }
    if obj_id == container_id {
        return Some(obj_id);
    }
    if g.objects[obj_id].parent == Some(container_id) {
        return Some(obj_id);
    }
    if let Some(parent) = g.objects[obj_id].parent {
        return in_container_depth(parent, container_id, g, depth + 1);
    }
    None
}

/// Get the head of the longest edge chain in a container (first child in chain).
fn get_longest_edge_chain_head(g: &Graph, container: ObjId) -> ObjId {
    let children = &g.objects[container].children_array;
    if children.is_empty() {
        return container;
    }

    let mut rank: HashMap<ObjId, i32> = HashMap::new();
    let mut chain_length: HashMap<ObjId, i32> = HashMap::new();

    for &child in children {
        let mut is_head = true;
        for edge in &g.edges {
            if in_container(edge.src, container, g).is_some()
                && in_container(edge.dst, child, g).is_some()
            {
                is_head = false;
                break;
            }
        }
        if !is_head {
            continue;
        }
        rank.insert(child, 1);
        chain_length.insert(child, 1);

        // BFS to find chain length
        let mut queue = vec![child];
        let mut visited = std::collections::HashSet::new();
        while let Some(curr) = queue.first().copied() {
            queue.remove(0);
            if !visited.insert(curr) {
                continue;
            }
            for edge in &g.edges {
                let dst_child = in_container(edge.dst, container, g);
                if dst_child == Some(curr) {
                    continue;
                }
                if let Some(dc) = dst_child
                    && in_container(edge.src, curr, g).is_some()
                {
                    let new_rank = rank.get(&curr).copied().unwrap_or(0) + 1;
                    if new_rank > rank.get(&dc).copied().unwrap_or(0) {
                        rank.insert(dc, new_rank);
                        let cl = chain_length.entry(child).or_insert(0);
                        *cl = (*cl).max(new_rank);
                    }
                    queue.push(dc);
                }
            }
        }
    }

    let max_chain = children
        .iter()
        .filter_map(|c| chain_length.get(c))
        .copied()
        .max()
        .unwrap_or(0);

    let heads: Vec<ObjId> = children
        .iter()
        .filter(|&&c| {
            rank.get(&c).copied().unwrap_or(0) == 1
                && chain_length.get(&c).copied().unwrap_or(0) == max_chain
        })
        .copied()
        .collect();

    if !heads.is_empty() {
        heads[heads.len() / 2]
    } else {
        children[0]
    }
}

/// Get the tail of the longest edge chain in a container.
fn get_longest_edge_chain_tail(g: &Graph, container: ObjId) -> ObjId {
    let children = &g.objects[container].children_array;
    if children.is_empty() {
        return container;
    }

    let mut rank: HashMap<ObjId, i32> = HashMap::new();

    for &child in children {
        let mut is_head = true;
        for edge in &g.edges {
            if in_container(edge.src, container, g).is_some()
                && in_container(edge.dst, child, g).is_some()
            {
                is_head = false;
                break;
            }
        }
        if !is_head {
            continue;
        }
        rank.insert(child, 1);

        // BFS
        let mut queue = vec![child];
        let mut visited = std::collections::HashSet::new();
        while let Some(curr) = queue.first().copied() {
            queue.remove(0);
            if !visited.insert(curr) {
                continue;
            }
            for edge in &g.edges {
                let dst_child = in_container(edge.dst, container, g);
                if dst_child == Some(curr) {
                    continue;
                }
                if let Some(dc) = dst_child
                    && in_container(edge.src, curr, g).is_some()
                {
                    let new_rank = rank.get(&curr).copied().unwrap_or(0) + 1;
                    let old = rank.get(&dc).copied().unwrap_or(0);
                    rank.insert(dc, old.max(new_rank));
                    queue.push(dc);
                }
            }
        }
    }

    let max_rank = children
        .iter()
        .filter_map(|c| rank.get(c))
        .copied()
        .max()
        .unwrap_or(0);

    let tails: Vec<ObjId> = children
        .iter()
        .filter(|&&c| rank.get(&c).copied().unwrap_or(0) == max_rank)
        .copied()
        .collect();

    if !tails.is_empty() {
        tails[tails.len() / 2]
    } else {
        children[0]
    }
}

// ---------------------------------------------------------------------------
// Label/icon positioning
// ---------------------------------------------------------------------------

/// Set default label and icon positions for an object.
fn position_labels_icons(obj: &mut crate::graph::Object) {
    if obj.icon.is_some() && obj.icon_position.is_none() {
        if !obj.children_array.is_empty() {
            obj.icon_position = Some("OUTSIDE_TOP_LEFT".to_owned());
            if obj.label_position.is_none() {
                obj.label_position = Some("OUTSIDE_TOP_RIGHT".to_owned());
                return;
            }
        } else if obj.sql_table.is_some() || obj.class.is_some() || !obj.language.is_empty() {
            obj.icon_position = Some("OUTSIDE_TOP_LEFT".to_owned());
        } else {
            obj.icon_position = Some("INSIDE_MIDDLE_CENTER".to_owned());
        }
    }

    if obj.has_label() && obj.label_position.is_none() {
        if !obj.children_array.is_empty() {
            obj.label_position = Some("OUTSIDE_TOP_CENTER".to_owned());
        } else if obj.has_outside_bottom_label() {
            obj.label_position = Some("OUTSIDE_BOTTOM_CENTER".to_owned());
        } else if obj.icon.is_some() {
            obj.label_position = Some("INSIDE_TOP_CENTER".to_owned());
        } else {
            obj.label_position = Some("INSIDE_MIDDLE_CENTER".to_owned());
        }

        if (obj.label_dimensions.width as f64) > obj.width
            || (obj.label_dimensions.height as f64) > obj.height
        {
            if !obj.children_array.is_empty() {
                obj.label_position = Some("OUTSIDE_TOP_CENTER".to_owned());
            } else {
                obj.label_position = Some("OUTSIDE_BOTTOM_CENTER".to_owned());
            }
        }
    }
}

#[derive(Clone)]
struct ConstantNearSubgraph {
    root_orig: ObjId,
    internal_edge_indices: Vec<usize>,
    external_edge_indices: Vec<usize>,
    orig_to_temp: HashMap<ObjId, ObjId>,
    temp: Graph,
}

fn is_constant_near_key(near_key: Option<&str>) -> bool {
    matches!(
        near_key,
        Some(
            "top-left"
                | "top-center"
                | "top-right"
                | "center-left"
                | "center-right"
                | "bottom-left"
                | "bottom-center"
                | "bottom-right"
        )
    )
}

fn collect_subtree_ids(g: &Graph, root: ObjId, out: &mut HashSet<ObjId>) {
    if !out.insert(root) {
        return;
    }
    for &child in &g.objects[root].children_array {
        collect_subtree_ids(g, child, out);
    }
}

fn build_constant_near_subgraphs(
    g: &Graph,
) -> (Vec<ConstantNearSubgraph>, HashSet<ObjId>, HashSet<usize>) {
    let mut subgraphs = Vec::new();
    let mut excluded_objects = HashSet::new();
    let mut excluded_edges = HashSet::new();
    let root_children = g.objects[g.root].children_array.clone();

    for root_obj in root_children {
        if !is_constant_near_key(g.objects[root_obj].near_key.as_deref()) {
            continue;
        }

        let mut subtree = HashSet::new();
        collect_subtree_ids(g, root_obj, &mut subtree);

        let mut temp = Graph::new();
        temp.theme = g.theme.clone();
        // Mirror Go ExtractSubgraph (line 379): nestedGraph.Root.Attributes
        // = container.Attributes — the constant-near subgraph's virtual
        // root inherits the near container's attributes (most notably
        // Direction) so a `direction: right` on the near container is seen
        // by dagre layout when it reads `root_obj().direction`.
        temp.objects[temp.root].direction = g.objects[root_obj].direction.clone();

        let mut subtree_ids: Vec<ObjId> = subtree.iter().copied().collect();
        subtree_ids.sort_unstable();

        let mut orig_to_temp = HashMap::new();
        for orig_id in &subtree_ids {
            let mut cloned = g.objects[*orig_id].clone();
            cloned.children.clear();
            cloned.children_array.clear();
            cloned.parent = if *orig_id == root_obj {
                cloned.near_key = None;
                Some(temp.root)
            } else {
                g.objects[*orig_id]
                    .parent
                    .and_then(|parent| orig_to_temp.get(&parent).copied())
            };
            let temp_id = temp.add_object(cloned);
            orig_to_temp.insert(*orig_id, temp_id);
        }

        let mut internal_edge_indices = Vec::new();
        let mut external_edge_indices = Vec::new();
        for (edge_idx, edge) in g.edges.iter().enumerate() {
            let src_in = subtree.contains(&edge.src);
            let dst_in = subtree.contains(&edge.dst);
            if src_in && dst_in {
                let mut cloned = edge.clone();
                cloned.src = orig_to_temp[&edge.src];
                cloned.dst = orig_to_temp[&edge.dst];
                temp.add_edge(cloned);
                internal_edge_indices.push(edge_idx);
            } else if src_in || dst_in {
                external_edge_indices.push(edge_idx);
            }
        }

        excluded_objects.extend(subtree.iter().copied());
        excluded_edges.extend(internal_edge_indices.iter().copied());
        excluded_edges.extend(external_edge_indices.iter().copied());
        subgraphs.push(ConstantNearSubgraph {
            root_orig: root_obj,
            internal_edge_indices,
            external_edge_indices,
            orig_to_temp,
            temp,
        });
    }

    (subgraphs, excluded_objects, excluded_edges)
}

fn constant_near_root(g: &Graph, obj_id: ObjId) -> Option<ObjId> {
    let mut current = Some(obj_id);
    while let Some(id) = current {
        if is_constant_near_key(g.objects[id].near_key.as_deref()) {
            return Some(id);
        }
        current = g.objects[id].parent;
    }
    None
}

fn constant_near_bounding_box(g: &Graph, placed_constant_nears: &HashSet<ObjId>) -> (Point, Point) {
    if g.objects.len() <= 1 {
        return (Point::new(0.0, 0.0), Point::new(0.0, 0.0));
    }

    let mut x1 = f64::INFINITY;
    let mut y1 = f64::INFINITY;
    let mut x2 = f64::NEG_INFINITY;
    let mut y2 = f64::NEG_INFINITY;

    // Mirrors Go d2near.boundingBox: it iterates every object in the graph
    // and uses each object's absolute TopLeft. In d2-little the dagre
    // pipeline has not yet shifted grid/sequence-diagram descendants into
    // their container's coordinate frame at this point (that happens in
    // d2-lib AFTER `layout_with_exclude` returns), so descendants' top_left
    // is still cell-local. Including those would corrupt the bbox. Restrict
    // to top-level (root-children) objects: their boxes already encompass
    // all descendants in absolute coords, so the resulting bbox matches Go.
    let top_level: HashSet<ObjId> = g.objects[g.root].children_array.iter().copied().collect();
    for obj_id in 0..g.objects.len() {
        if obj_id == g.root {
            continue;
        }
        let obj = &g.objects[obj_id];
        if let Some(near_key) = obj.near_key.as_deref() {
            if !placed_constant_nears.contains(&obj_id) {
                continue;
            }
            match near_key {
                "top-center" | "bottom-center" => {
                    x1 = x1.min(obj.top_left.x);
                    x2 = x2.max(obj.top_left.x + obj.width);
                }
                "center-left" | "center-right" => {
                    y1 = y1.min(obj.top_left.y);
                    y2 = y2.max(obj.top_left.y + obj.height);
                }
                _ => {}
            }
            continue;
        }
        if constant_near_root(g, obj_id).is_some() {
            continue;
        }
        if !top_level.contains(&obj_id) {
            // Descendants' positions are not yet absolute; their parent's
            // box (counted above) already bounds them.
            continue;
        }

        x1 = x1.min(obj.top_left.x);
        y1 = y1.min(obj.top_left.y);
        x2 = x2.max(obj.top_left.x + obj.width);
        y2 = y2.max(obj.top_left.y + obj.height);

        if obj.has_label()
            && obj
                .label_position
                .as_deref()
                .is_some_and(|pos| pos.contains("OUTSIDE"))
            && let Some(label_tl) = obj.get_label_top_left()
        {
            x1 = x1.min(label_tl.x);
            y1 = y1.min(label_tl.y);
            x2 = x2.max(label_tl.x + obj.label_dimensions.width as f64);
            y2 = y2.max(label_tl.y + obj.label_dimensions.height as f64);
        }
    }

    for edge in &g.edges {
        if constant_near_root(g, edge.src).is_some() || constant_near_root(g, edge.dst).is_some() {
            continue;
        }
        for point in &edge.route {
            x1 = x1.min(point.x);
            y1 = y1.min(point.y);
            x2 = x2.max(point.x);
            y2 = y2.max(point.y);
        }
    }

    if x1.is_infinite() && x2.is_infinite() {
        x1 = 0.0;
        x2 = 0.0;
    }
    if y1.is_infinite() && y2.is_infinite() {
        y1 = 0.0;
        y2 = 0.0;
    }

    (Point::new(x1, y1), Point::new(x2, y2))
}

fn place_constant_near(
    obj: &crate::graph::Object,
    near_key: &str,
    g: &Graph,
    placed_constant_nears: &HashSet<ObjId>,
) -> Point {
    let (tl, br) = constant_near_bounding_box(g, placed_constant_nears);
    let w = br.x - tl.x;
    let h = br.y - tl.y;

    let (mut x, mut y) = match near_key {
        "top-left" => (
            tl.x - obj.width - CONSTANT_NEAR_PAD,
            tl.y - obj.height - CONSTANT_NEAR_PAD,
        ),
        "top-center" => (
            tl.x + w / 2.0 - obj.width / 2.0,
            tl.y - obj.height - CONSTANT_NEAR_PAD,
        ),
        "top-right" => (
            br.x + CONSTANT_NEAR_PAD,
            tl.y - obj.height - CONSTANT_NEAR_PAD,
        ),
        "center-left" => (
            tl.x - obj.width - CONSTANT_NEAR_PAD,
            tl.y + h / 2.0 - obj.height / 2.0,
        ),
        "center-right" => (br.x + CONSTANT_NEAR_PAD, tl.y + h / 2.0 - obj.height / 2.0),
        "bottom-left" => (
            tl.x - obj.width - CONSTANT_NEAR_PAD,
            br.y + CONSTANT_NEAR_PAD,
        ),
        "bottom-center" => (br.x - w / 2.0 - obj.width / 2.0, br.y + CONSTANT_NEAR_PAD),
        "bottom-right" => (br.x + CONSTANT_NEAR_PAD, br.y + CONSTANT_NEAR_PAD),
        _ => (obj.top_left.x, obj.top_left.y),
    };

    if let Some(label_position) = obj.label_position.as_deref()
        && !label_position.contains("INSIDE")
    {
        if label_position.contains("_TOP_") {
            if near_key.contains("bottom") {
                y += obj.label_dimensions.height as f64;
            }
        } else if label_position.contains("_LEFT_") {
            if near_key.contains("right") {
                x += obj.label_dimensions.width as f64;
            }
        } else if label_position.contains("_RIGHT_") {
            if near_key.contains("left") {
                x -= obj.label_dimensions.width as f64;
            }
        } else if label_position.contains("_BOTTOM_") && near_key.contains("top") {
            y -= obj.label_dimensions.height as f64;
        }
    }

    Point::new(x, y)
}

fn apply_constant_near_subgraphs(
    g: &mut Graph,
    subgraphs: &mut [ConstantNearSubgraph],
    opts: &ConfigurableOpts,
) -> Result<Vec<usize>, String> {
    let ordered_groups = [
        ["top-center", "bottom-center"].as_slice(),
        ["center-left", "center-right"].as_slice(),
        ["top-left", "top-right", "bottom-left", "bottom-right"].as_slice(),
    ];
    let mut placed_constant_nears = HashSet::new();

    for near_group in ordered_groups {
        for subgraph in subgraphs.iter_mut() {
            let near_key = g.objects[subgraph.root_orig]
                .near_key
                .as_deref()
                .unwrap_or_default();
            if !near_group.contains(&near_key) {
                continue;
            }

            layout(&mut subgraph.temp, Some(opts))?;
            let temp_root = subgraph.orig_to_temp[&subgraph.root_orig];
            let mut placed_root = subgraph.temp.objects[temp_root].clone();
            placed_root.near_key = g.objects[subgraph.root_orig].near_key.clone();
            let placement = place_constant_near(
                &placed_root,
                placed_root.near_key.as_deref().unwrap_or_default(),
                g,
                &placed_constant_nears,
            );
            let current_top_left = subgraph.temp.objects[temp_root].top_left;
            let dx = placement.x - current_top_left.x;
            let dy = placement.y - current_top_left.y;

            for obj in subgraph.temp.objects.iter_mut().skip(1) {
                obj.top_left.x += dx;
                obj.top_left.y += dy;
                obj.update_box();
            }
            for edge in &mut subgraph.temp.edges {
                for point in &mut edge.route {
                    point.x += dx;
                    point.y += dy;
                }
            }

            for (orig_id, temp_id) in &subgraph.orig_to_temp {
                let temp_obj = &subgraph.temp.objects[*temp_id];
                let orig_obj = &mut g.objects[*orig_id];
                orig_obj.top_left = temp_obj.top_left;
                orig_obj.width = temp_obj.width;
                orig_obj.height = temp_obj.height;
                orig_obj.box_ = temp_obj.box_;
                orig_obj.label_position = temp_obj.label_position.clone();
                orig_obj.icon_position = temp_obj.icon_position.clone();
            }

            for (edge_idx, temp_edge) in subgraph
                .internal_edge_indices
                .iter()
                .copied()
                .zip(subgraph.temp.edges.iter())
            {
                g.edges[edge_idx].route = temp_edge.route.clone();
                g.edges[edge_idx].is_curve = temp_edge.is_curve;
                g.edges[edge_idx].label_position = temp_edge.label_position.clone();
            }

            placed_constant_nears.insert(subgraph.root_orig);
        }
    }

    let mut routed_external_edges = HashSet::new();
    let mut deferred_external_edges: Vec<usize> = Vec::new();
    for subgraph in subgraphs.iter() {
        for &edge_idx in &subgraph.external_edge_indices {
            if !routed_external_edges.insert(edge_idx) {
                continue;
            }
            // External edges that touch a grid descendant whose absolute
            // position depends on the post-dagre grid shift (performed by
            // the caller in d2-lib) cannot be safely routed here; defer
            // them and let the caller invoke `route_deferred_constant_near_external_edges`
            // once the descendants are at their final absolute positions.
            // For non-grid-descendant edges we route in-place as before.
            let edge = &g.edges[edge_idx];
            let needs_defer =
                touches_grid_descendant(g, edge.src) || touches_grid_descendant(g, edge.dst);
            if needs_defer {
                deferred_external_edges.push(edge_idx);
            } else {
                route_constant_near_external_edge(g, edge_idx);
            }
        }
    }

    Ok(deferred_external_edges)
}

/// True if `obj_id` lies inside (at any depth) a grid-diagram container.
/// d2-lib's grid pre-layout stores grid-descendant top_lefts in cell-local
/// coordinates; their absolute positions are only realised by the post-
/// dagre grid shift. Routing edges that touch such descendants requires
/// running after that shift.
fn touches_grid_descendant(g: &Graph, obj_id: ObjId) -> bool {
    let mut cur = g.objects[obj_id].parent;
    while let Some(pid) = cur {
        if pid == g.root {
            break;
        }
        if g.objects[pid].is_grid_diagram() {
            return true;
        }
        cur = g.objects[pid].parent;
    }
    false
}

/// Route external constant-near edges that were deferred from
/// `apply_constant_near_subgraphs` because they touch grid descendants.
/// Call this after the caller has performed any post-dagre grid-cell
/// shifting so src/dst objects are at their final absolute positions.
pub fn route_deferred_constant_near_external_edges(g: &mut Graph, edges: &[usize]) {
    for &edge_idx in edges {
        route_constant_near_external_edge(g, edge_idx);
    }
}

fn route_constant_near_external_edge(g: &mut Graph, edge_idx: usize) {
    if edge_idx >= g.edges.len() {
        return;
    }

    let src_id = g.edges[edge_idx].src;
    let dst_id = g.edges[edge_idx].dst;
    let src = g.objects[src_id].center();
    let dst = g.objects[dst_id].center();
    let mut points = vec![src, dst];

    let (new_start, new_end) = {
        let edge = &g.edges[edge_idx];
        edge.trace_to_shape(&points, 0, 1, g)
    };
    points = points[new_start..=new_end].to_vec();

    if points.len() >= 2 {
        let last = points.len() - 1;
        let src_box = g.objects[src_id].box_;
        let dst_box = g.objects[dst_id].box_;

        // Mirror Go `Edge.TraceToShape`: fully resolve the src side (bbox
        // clip + non-rect perimeter snap) BEFORE constructing the dst's
        // `ending_segment`. This keeps the dst clip aligned with the
        // perimeter-snapped `points[0]` rather than the stale bbox hit — a
        // 0.5 px drift otherwise creeps in whenever `TraceToShapeBorder`
        // moves the src endpoint off the bbox corner.
        let starting_segment = Segment::new(points[1], points[0]);
        if let Some(p) = src_box.intersections(&starting_segment).first().copied() {
            points[0] = p;
        }
        if !g.objects[src_id].is_rectangular_shape() {
            let bbox = crate::geo::Box2D::new(
                g.objects[src_id].top_left,
                g.objects[src_id].width,
                g.objects[src_id].height,
            );
            let traced =
                trace_to_shape_border_with_box(&g.objects[src_id], bbox, points[0], points[1]);
            points[0] = traced;
        }

        let ending_segment = Segment::new(points[last - 1], points[last]);
        if let Some(p) = dst_box.intersections(&ending_segment).first().copied() {
            points[last] = p;
        }
        if !g.objects[dst_id].is_rectangular_shape() {
            let last = points.len() - 1;
            let bbox = crate::geo::Box2D::new(
                g.objects[dst_id].top_left,
                g.objects[dst_id].width,
                g.objects[dst_id].height,
            );
            let traced = trace_to_shape_border_with_box(
                &g.objects[dst_id],
                bbox,
                points[last],
                points[last - 1],
            );
            points[last] = traced;
        }
    }

    g.edges[edge_idx].route = points;
    g.edges[edge_idx].is_curve = false;
    if !g.edges[edge_idx].label.value.is_empty() {
        g.edges[edge_idx].label_position = Some("INSIDE_MIDDLE_CENTER".to_owned());
    }
}

// ---------------------------------------------------------------------------
// Layout entry point
// ---------------------------------------------------------------------------

/// Run the dagre layout algorithm on a d2 graph.
///
/// This builds a dagre graph from d2 objects and edges, runs the layout,
/// then reads back positions and routes.
pub fn layout(g: &mut Graph, opts: Option<&ConfigurableOpts>) -> Result<(), String> {
    layout_with_exclude(g, opts, &std::collections::HashSet::new()).map(|_| ())
}

/// Layout with an explicit set of object IDs to exclude from dagre.
///
/// Returns a list of constant-near external edge indices whose routing was
/// deferred because their src or dst lies inside a grid container. The
/// caller (typically d2-lib) must invoke
/// [`route_deferred_constant_near_external_edges`] after performing the
/// post-dagre grid-cell shifts so the final absolute coordinates are used.
pub fn layout_with_exclude(
    g: &mut Graph,
    opts: Option<&ConfigurableOpts>,
    extra_excluded: &std::collections::HashSet<usize>,
) -> Result<Vec<usize>, String> {
    let default_opts = ConfigurableOpts::default();
    let opts = opts.unwrap_or(&default_opts);
    let (mut constant_near_subgraphs, mut excluded_objects, excluded_edges) =
        build_constant_near_subgraphs(g);
    excluded_objects.extend(extra_excluded);

    // Determine direction
    let root_direction = g.root_obj().direction.value.clone();
    let is_horizontal = matches!(root_direction.as_str(), "right" | "left");
    let rankdir = match root_direction.as_str() {
        "right" => crate::dagre::RankDir::LR,
        "left" => crate::dagre::RankDir::RL,
        "up" => crate::dagre::RankDir::BT,
        _ => crate::dagre::RankDir::TB,
    };

    // Position labels and icons
    for i in 0..g.objects.len() {
        if i == g.root {
            continue;
        }
        // Borrow-safe: copy needed data, modify object
        position_labels_icons(&mut g.objects[i]);
    }

    // Compute max label dimensions for rank separation
    let mut max_label_width = 0;
    let mut max_label_height = 0;
    for (edge_idx, edge) in g.edges.iter().enumerate() {
        if excluded_edges.contains(&edge_idx) {
            continue;
        }
        max_label_width = max_label_width.max(edge.label_dimensions.width);
        max_label_height = max_label_height.max(edge.label_dimensions.height);
    }

    let ranksep = if !is_horizontal {
        100i32.max(max_label_height + 40)
    } else {
        100i32.max(max_label_width + 40)
    };

    // Build dagre graph
    let graph_opts = crate::dagre::graph::GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    };
    let mut dagre_g =
        crate::dagre::graph::Graph::<crate::dagre::NodeLabel, crate::dagre::EdgeLabel>::with_options(graph_opts);

    // Set graph-level label
    dagre_g.set_graph_label(crate::dagre::GraphLabel {
        compound: true,
        rankdir,
        nodesep: opts.node_sep as f64,
        edgesep: opts.edge_sep as f64,
        ranksep: ranksep as f64,
        ..Default::default()
    });

    // Register all objects with the mapper.
    // Skip class/sql_table children that have been absorbed into their
    // parent shape (marked with a sentinel in `compile_class_shape` /
    // `compile_sql_table_shape`). Those objects must not participate in
    // layout or they'll be laid out as independent nodes and throw the
    // bounding box off.
    let mut mapper = ObjectMapper::new();
    let obj_ids: Vec<ObjId> = (0..g.objects.len())
        .filter(|&i| {
            i != g.root
                && !excluded_objects.contains(&i)
                && g.objects[i].shape.value != "__d2_class_field_removed__"
                && g.objects[i].shape.value != "__d2_seq_nested_removed__"
        })
        .collect();
    for &obj_id in &obj_ids {
        mapper.register(obj_id);
    }

    // Add nodes
    for &obj_id in &obj_ids {
        let obj = &g.objects[obj_id];
        let dagre_id = mapper.to_dagre_id(obj_id).to_owned();
        // Match Go's `int(obj.Width)` truncation when feeding into dagre.
        // Without this, fractional widths from sequence-diagram containers
        // pass through dagre and emerge ceil'd one pixel wider than Go.
        dagre_g.set_node(
            dagre_id,
            Some(crate::dagre::NodeLabel {
                width: (obj.width as i64) as f64,
                height: (obj.height as i64) as f64,
                ..Default::default()
            }),
        );
    }

    // Set parents for compound graph
    for &obj_id in &obj_ids {
        let mut parent = g.objects[obj_id].parent;
        while let Some(parent_id) = parent {
            if parent_id == g.root {
                break;
            }
            if let Some(parent_dagre) = mapper.dagre_id(parent_id) {
                let child_dagre = mapper.to_dagre_id(obj_id).to_owned();
                dagre_g.set_parent(&child_dagre, Some(parent_dagre));
                break;
            }
            parent = g.objects[parent_id].parent;
        }
    }

    // Add edges
    // Collect edge endpoint data first (immutable borrow)
    let edge_data: Vec<(usize, ObjId, ObjId, i32, i32, String)> = (0..g.edges.len())
        .filter(|ei| !excluded_edges.contains(ei))
        // Skip edges whose src/dst are excluded from layout
        .filter(|&ei| {
            let src = g.edges[ei].src;
            let dst = g.edges[ei].dst;
            !excluded_objects.contains(&src) && !excluded_objects.contains(&dst)
        })
        .filter_map(|ei| {
            let (src, dst) = get_edge_endpoints(g, ei);
            if mapper.dagre_id(src).is_none() || mapper.dagre_id(dst).is_none() {
                return None;
            }
            let edge = &g.edges[ei];
            let mut width = edge.label_dimensions.width;
            let mut height = edge.label_dimensions.height;

            // Count parallel edges for gap spacing
            let num_parallel = (0..g.edges.len())
                .filter(|ei2| !excluded_edges.contains(ei2))
                .filter(|&ei2| {
                    let (s2, d2) = get_edge_endpoints(g, ei2);
                    (s2 == src && d2 == dst) || (s2 == dst && d2 == src)
                })
                .count();

            if num_parallel > 1 {
                match root_direction.as_str() {
                    "left" | "right" => height += EDGE_LABEL_GAP,
                    _ => width += EDGE_LABEL_GAP,
                }
            }

            let abs_id = edge.abs_id.clone();
            Some((ei, src, dst, width, height, abs_id))
        })
        .collect();

    for (_, src, dst, width, height, abs_id) in &edge_data {
        let src_dagre = mapper.to_dagre_id(*src).to_owned();
        let dst_dagre = mapper.to_dagre_id(*dst).to_owned();
        dagre_g.set_edge(
            src_dagre,
            dst_dagre,
            Some(crate::dagre::EdgeLabel {
                width: *width as f64,
                height: *height as f64,
                labelpos: crate::dagre::layout::types::LabelPos::Center,
                ..Default::default()
            }),
            Some(abs_id.as_str()),
        );
    }

    // Run layout. `tie_keep_first = true` makes dagre-rs match dagre.js
    // v0.8.5 (the version Go d2 v0.7.1 bundles), which is critical for
    // byte-identical SVG output: without it, the order phase mirrors x
    // coordinates for any rank with no crossings.
    let layout_opts = crate::dagre::LayoutOptions {
        rankdir,
        nodesep: opts.node_sep as f64,
        edgesep: opts.edge_sep as f64,
        ranksep: ranksep as f64,
        tie_keep_first: true,
        ..Default::default()
    };
    crate::dagre::layout(&mut dagre_g, Some(layout_opts));

    // Read back node positions
    for &obj_id in &obj_ids {
        let dagre_id = mapper.to_dagre_id(obj_id);
        if let Some(node_label) = dagre_g.node(dagre_id)
            && let (Some(cx), Some(cy)) = (node_label.x, node_label.y)
        {
            let w = node_label.width;
            let h = node_label.height;
            let obj = &mut g.objects[obj_id];
            // dagre gives center coordinates; convert to top-left
            obj.top_left = Point::new((cx - w / 2.0).round(), (cy - h / 2.0).round());
            obj.width = w.ceil();
            obj.height = h.ceil();
            obj.update_box();
        }
    }

    // Read back edge routes.
    //
    // CRITICAL: do not iterate `dagre_g.edges()` by position index here.
    // dagre.js's `layout(g)` operates on an internal *copy* of the graph
    // (`buildLayoutGraph(g)` → `runLayout(layoutGraph)` →
    // `updateInputGraph(g, layoutGraph)`), so its caller queries
    // `g.edges()[i]` on the untouched input and gets edges back in the
    // original insertion order. dagre-rs, however, runs the full
    // pipeline on the caller's graph directly. The `acyclic::run` /
    // `acyclic::undo` pair reverses back-edges via `remove_edge` +
    // `set_edge`, which drops the edge's slot in the internal
    // `edge_order` vector and re-appends the restored edge at the end
    // of the list. After layout, position-based indexing therefore maps
    // route[i] to edge_data[i] for the forward edges but drifts for any
    // back-edge: the route slot that used to belong to edge_data[k] is
    // now occupied by a later edge, and the back-edge lands at the end.
    //
    // This showed up in `constant_near_title`: `unfavorable -> poll the
    // people` is a back edge, so its route was shuffled onto
    // `results -> favorable`, and the true route of that edge rotated
    // onto `favorable -> will of the people`, etc. The diagram_bytes
    // hash then diverged even though every node coordinate matched.
    //
    // Fix: iterate our own `edge_data` in insertion order and look up
    // each dagre edge by (src_dagre_id, dst_dagre_id, abs_id). After
    // `reverse_points_for_reversed_edges` + `acyclic::undo`, back-edges
    // are keyed by their ORIGINAL (v, w, name) again, so the label is
    // still reachable via the hashmap — only the `edge_order` vector
    // that drives `g.edges()` is out of sync with insertion order.
    for (ei, src_obj, dst_obj, _, _, abs_id) in &edge_data {
        let ei = *ei;
        let src_dagre = mapper.to_dagre_id(*src_obj).to_owned();
        let dst_dagre = mapper.to_dagre_id(*dst_obj).to_owned();
        if let Some(edge_label) = dagre_g.edge(&src_dagre, &dst_dagre, Some(abs_id.as_str())) {
            let raw_points: Vec<Point> = edge_label
                .points
                .iter()
                .map(|p| Point::new(p.x, p.y))
                .collect();

            if raw_points.is_empty() {
                continue;
            }

            let edge = &g.edges[ei];

            // Reverse points for reverse arrows
            let mut points: Vec<Point> = if edge.src_arrow && !edge.dst_arrow {
                raw_points.into_iter().rev().collect()
            } else {
                raw_points
            };

            // Preliminary chop at src/dst boxes — mirrors Go's first pass
            // in `d2dagrelayout.Layout` (the block right after reading
            // `JSON.stringify(g.edge(...))`). The final TraceToShape runs
            // later, after spacing adjustments.
            if edge.src != edge.dst && points.len() >= 2 {
                let src_box = g.objects[edge.src].box_;
                let dst_box = g.objects[edge.dst].box_;
                let mut start_index = 0;
                let mut end_index = points.len() - 1;
                let mut start = points[0];
                let mut end = points[end_index];

                for i in 1..points.len() {
                    let seg = Segment::new(points[i - 1], points[i]);
                    let ints = src_box.intersections(&seg);
                    if !ints.is_empty() {
                        start = ints[0];
                        start_index = i - 1;
                    }
                    let ints = dst_box.intersections(&seg);
                    if !ints.is_empty() {
                        end = ints[0];
                        end_index = i;
                        break;
                    }
                }

                points = points[start_index..=end_index].to_vec();
                points[0] = start;
                let last = points.len() - 1;
                points[last] = end;
            }

            g.edges[ei].route = points;

            // Set edge label position
            if !g.edges[ei].label.value.is_empty() {
                g.edges[ei].label_position = Some("INSIDE_MIDDLE_CENTER".to_owned());
            }
        }
    }

    // Position default labels/icons (mirrors Go positionLabelsIcons).
    for i in 0..g.objects.len() {
        if i == g.root {
            continue;
        }
        position_labels_icons(&mut g.objects[i]);
    }

    // Post-dagre spacing passes (mirror Go d2dagrelayout.Layout). These
    // account for outside labels / icons and 3D/multiple shape visual
    // extensions, which dagre itself does not model. Without them, shapes
    // with `style.multiple` or `style.3d` end up 10px short on their right
    // side and outside labels overlap neighbouring shapes.
    //
    // Pass `excluded_objects` so grid descendants (which Go physically
    // extracts from the graph before running dagre) don't contribute margin
    // or padding adjustments that would grow their grid container — Go never
    // sees them during this phase because they've been pulled out into a
    // sub-graph.
    adjust_rank_spacing(g, ranksep as f64, is_horizontal, &excluded_objects);
    adjust_cross_rank_spacing(g, ranksep as f64, !is_horizontal, &excluded_objects);

    // Shrink containers around their children + padding.
    fit_container_padding(g, &excluded_objects);

    // Final edge post-processing (mirrors the second `for _, edge := range
    // g.Edges` block in Go `d2dagrelayout.Layout`). At this point node
    // positions and route control points have been fully shifted, so we
    // do short-segment fixes, a second TraceToShape pass, and curve
    // generation on the final points.
    for ei in 0..g.edges.len() {
        let edge_route = g.edges[ei].route.clone();
        if edge_route.is_empty() {
            continue;
        }
        let (src_id, dst_id) = (g.edges[ei].src, g.edges[ei].dst);
        // Skip edges whose endpoints live in objects excluded from the outer
        // dagre layout (e.g. grid-cell descendants). Those routes were laid
        // out and curve-generated by the cell's own nested sub-graph pass,
        // and re-running trace-to-shape / curve-gen here would reinterpret
        // the already-generated bezier control points as raw waypoints and
        // blow the route out with spurious interior curves.
        if excluded_objects.contains(&src_id) || excluded_objects.contains(&dst_id) {
            continue;
        }
        let src_box = g.objects[src_id].box_;
        let dst_box = g.objects[dst_id].box_;
        let mut points = edge_route;

        // Second-pass TraceToShape. Mirror Go `d2graph.Edge.TraceToShape`
        // closely enough that outside-label shapes (notably `shape: image`
        // with a label) push the edge endpoint past the label box instead
        // of snapping to the shape's own rectangle. Non-rectangular shape
        // border tracing is still TODO — for rectangles the box
        // intersection is what Go's `TraceToShapeBorder` returns.
        if points.len() >= 2 {
            // `shape.TraceToShapeBorder` in Go rounds the final point to
            // the nearest integer for any non-rectangular shape (after
            // tracing the true perimeter). We don't yet implement the
            // full perimeter trace, but rounding is enough to recover
            // the expected `(box_bottom, box_top) = (50, 171)` style
            // endpoints instead of the float box-intersect artefacts
            // (`49.5`, `171.5`).
            let src_is_rect = g.objects[src_id].is_rectangular_shape();
            let dst_is_rect = g.objects[dst_id].is_rectangular_shape();

            // Go `Edge.TraceToShape` operates on `(startIndex, endIndex)`
            // into `points` and may advance / retreat them when merging
            // short segments or walking past outside-label points. We
            // mirror that with plain indices and truncate the vec at the
            // end so downstream curve generation sees only the live
            // range.
            let mut start_idx: usize = 0;
            let mut end_idx: usize = points.len() - 1;

            // Mirror Go `d2dagrelayout.Layout` lines 314-331: when the edge's
            // source or destination carries a 3D/multiple visual offset, shift
            // the shape's effective top-left by `(dx, -dy)` BEFORE running
            // TraceToShape. This lifts the outside-label box (and the shape's
            // own rectangle on the non-rectangular perimeter trace) onto the
            // 3D-offset outline, so edges terminate at the visible border
            // rather than at the unoffset box. The shift is gated by the same
            // condition Go uses: the anchor point must sit inside the offset
            // zone on the upper-right of the shape.
            let start_point = points[start_idx];
            let end_point = points[end_idx];
            let (src_dx, src_dy) = g.objects[src_id].get_modifier_element_adjustments();
            let src_shift_box = if (src_dx != 0.0 || src_dy != 0.0)
                && start_point.x > g.objects[src_id].top_left.x + src_dx
                && start_point.y < g.objects[src_id].top_left.y + g.objects[src_id].height - src_dy
            {
                let mut b = src_box;
                b.top_left.x += src_dx;
                b.top_left.y -= src_dy;
                Some(b)
            } else {
                None
            };
            let (dst_dx, dst_dy) = g.objects[dst_id].get_modifier_element_adjustments();
            let dst_shift_box = if (dst_dx != 0.0 || dst_dy != 0.0)
                && end_point.x > g.objects[dst_id].top_left.x + dst_dx
                && end_point.y < g.objects[dst_id].top_left.y + g.objects[dst_id].height - dst_dy
            {
                let mut b = dst_box;
                b.top_left.x += dst_dx;
                b.top_left.y -= dst_dy;
                Some(b)
            } else {
                None
            };
            let src_label_shape_box = src_shift_box.unwrap_or(src_box);
            let dst_label_shape_box = dst_shift_box.unwrap_or(dst_box);

            // Source side.
            let src_label_box = outside_label_box_for(&g.objects[src_id], &src_label_shape_box);
            let starting_segment = Segment::new(points[start_idx + 1], points[start_idx]);
            let src_label_hit = src_label_box.as_ref().and_then(|(b, pos)| {
                let ints = b.intersections(&starting_segment);
                if ints.is_empty() {
                    None
                } else {
                    Some(find_outer_intersection(*pos, &ints))
                }
            });
            // Track whether a 3d/multiple modifier shift was applied so the
            // non-rectangular perimeter trace that follows sees the same
            // offset outline (mirror of Go's `edge.Src.TopLeft` mutation in
            // `d2dagrelayout.Layout`).
            let mut src_trace_box: Option<crate::geo::Box2D> = None;
            let mut overlaps_outside_src = false;
            if let Some(p) = src_label_hit {
                points[start_idx] = p;
                overlaps_outside_src = true;
                // Merge a too-short starting segment with the next one
                // (mirror Go lines 449-452): if the freshly clipped
                // segment is shorter than `MIN_SEGMENT_LEN`, collapse
                // `points[start_idx+1]` onto the endpoint and advance.
                if start_idx + 1 < end_idx {
                    let seg = Segment::new(points[start_idx + 1], points[start_idx]);
                    if seg.length() < crate::graph::MIN_SEGMENT_LEN {
                        points[start_idx + 1] = points[start_idx];
                        start_idx += 1;
                    }
                }
            } else {
                // Mirror Go `Edge.TraceToShape`'s `edge.Src.HasIcon()`
                // branch (layout.go lines 456-486). When the outside label
                // did not overlap but the shape has an outside icon,
                // clip the starting segment against the icon box. Uses
                // `MAX_ICON_SIZE` for Src / `GetIconSize` for Dst — Go
                // applies `MAX_ICON_SIZE` literal on the src side.
                let src_icon_box =
                    outside_icon_box_src_for(&g.objects[src_id], &src_label_shape_box);
                if let Some((icon_box, icon_pos)) = src_icon_box.as_ref() {
                    // Go layout.go lines 467-471 have `startIndex+1 > endIndex`
                    // as the walk-forward predicate, which is effectively
                    // never true (it would require startIndex >= endIndex).
                    // Preserve that dead-code behaviour so we stay byte-exact
                    // with Go's routing: no walk-forward is performed here.
                    let icon_seg = starting_segment;
                    let ints = icon_box.intersections(&icon_seg);
                    if !ints.is_empty() {
                        let p = find_outer_intersection(*icon_pos, &ints);
                        points[start_idx] = p;
                        overlaps_outside_src = true;
                        if start_idx + 1 < end_idx {
                            let seg = Segment::new(points[start_idx + 1], points[start_idx]);
                            if seg.length() < crate::graph::MIN_SEGMENT_LEN {
                                points[start_idx + 1] = points[start_idx];
                                start_idx += 1;
                            }
                        }
                    }
                }
            }
            if !overlaps_outside_src {
                // Mirror Go `Edge.TraceToShape`'s 3D/multiple handling:
                // when the segment start sits inside the visual offset
                // zone (upper-right for 3d/multiple), temporarily shift
                // the shape's top-left by (dx, -dy) so the intersection
                // lands on the offset shape border.
                let starting_segment2 = Segment::new(points[start_idx + 1], points[start_idx]);
                let (dx, dy) = g.objects[src_id].get_modifier_element_adjustments();
                let trace_box = if (dx != 0.0 || dy != 0.0)
                    && points[start_idx].x > src_box.top_left.x + dx
                    && points[start_idx].y < src_box.top_left.y + src_box.height - dy
                {
                    let mut b = src_box;
                    b.top_left.x += dx;
                    b.top_left.y -= dy;
                    src_trace_box = Some(b);
                    b
                } else {
                    src_box
                };
                let ints = trace_box.intersections(&starting_segment2);
                if let Some(p) = ints.first() {
                    points[start_idx] = *p;
                    if start_idx + 1 < end_idx {
                        let seg = Segment::new(points[start_idx + 1], points[start_idx]);
                        if seg.length() < crate::graph::MIN_SEGMENT_LEN {
                            points[start_idx + 1] = points[start_idx];
                            start_idx += 1;
                        }
                    }
                }
            }
            if !overlaps_outside_src && !src_is_rect {
                let bbox = src_trace_box.unwrap_or_else(|| {
                    crate::geo::Box2D::new(
                        g.objects[src_id].top_left,
                        g.objects[src_id].width,
                        g.objects[src_id].height,
                    )
                });
                let traced = trace_to_shape_border_with_box(
                    &g.objects[src_id],
                    bbox,
                    points[start_idx],
                    points[start_idx + 1],
                );
                points[start_idx] = traced;
            }

            // Destination side.
            let dst_label_box = outside_label_box_for(&g.objects[dst_id], &dst_label_shape_box);
            // Walk back `end_idx` while the segment START (the pre-endpoint
            // point on the incoming side) still sits inside the outside-
            // label box. This mirrors Go d2graph/layout.go lines 516-520:
            // when dagre emits an intermediate kink that actually lives
            // inside the label rectangle, we drop it so the final segment
            // starts from a point that is outside the label, which in turn
            // lets `labelBox.Intersections` find a proper clip point.
            if let Some((label_box, _)) = dst_label_box.as_ref() {
                while end_idx - 1 > start_idx && label_box.contains(&points[end_idx - 1]) {
                    end_idx -= 1;
                }
            }
            let ending_segment = Segment::new(points[end_idx - 1], points[end_idx]);
            let dst_label_hit = dst_label_box.as_ref().and_then(|(b, pos)| {
                let ints = b.intersections(&ending_segment);
                if ints.is_empty() {
                    None
                } else {
                    Some(find_outer_intersection(*pos, &ints))
                }
            });
            let mut overlaps_outside_dst = false;
            if let Some(p) = dst_label_hit {
                points[end_idx] = p;
                overlaps_outside_dst = true;
                // Merge a too-short ending segment with the previous one
                // (mirror Go lines 531-534): if the freshly clipped
                // segment is shorter than `MIN_SEGMENT_LEN`, collapse
                // `points[end_idx-1]` onto the endpoint and retreat.
                if end_idx - 1 > start_idx {
                    let seg = Segment::new(points[end_idx - 1], points[end_idx]);
                    if seg.length() < crate::graph::MIN_SEGMENT_LEN {
                        points[end_idx - 1] = points[end_idx];
                        end_idx -= 1;
                    }
                }
            } else {
                // Mirror Go `edge.Dst.HasIcon()` branch: when the label
                // didn't overlap but the shape has an outside icon, clip
                // the ending segment against the icon box instead of the
                // shape's own border. Skips when there is no outside icon.
                let dst_icon_box = outside_icon_box_for(&g.objects[dst_id], &dst_label_shape_box);
                if let Some((icon_box, icon_pos)) = dst_icon_box.as_ref() {
                    // Mirror Go layout.go lines 548-552: walk back the
                    // end_idx past any points the ending segment's start
                    // still inside the icon box.
                    let mut icon_seg = ending_segment;
                    while end_idx - 1 > start_idx && icon_box.contains(&icon_seg.start) {
                        end_idx -= 1;
                        icon_seg = Segment::new(points[end_idx - 1], points[end_idx]);
                    }
                    let ints = icon_box.intersections(&icon_seg);
                    if !ints.is_empty() {
                        let p = find_outer_intersection(*icon_pos, &ints);
                        points[end_idx] = p;
                        overlaps_outside_dst = true;
                        if end_idx - 1 > start_idx {
                            let seg = Segment::new(points[end_idx - 1], points[end_idx]);
                            if seg.length() < crate::graph::MIN_SEGMENT_LEN {
                                points[end_idx - 1] = points[end_idx];
                                end_idx -= 1;
                            }
                        }
                    }
                }
            }
            let mut dst_trace_box: Option<crate::geo::Box2D> = None;
            if !overlaps_outside_dst {
                let ending_segment2 = Segment::new(points[end_idx - 1], points[end_idx]);
                let (dx, dy) = g.objects[dst_id].get_modifier_element_adjustments();
                let trace_box = if (dx != 0.0 || dy != 0.0)
                    && points[end_idx].x > dst_box.top_left.x + dx
                    && points[end_idx].y < dst_box.top_left.y + dst_box.height - dy
                {
                    let mut b = dst_box;
                    b.top_left.x += dx;
                    b.top_left.y -= dy;
                    dst_trace_box = Some(b);
                    b
                } else {
                    dst_box
                };
                let ints = trace_box.intersections(&ending_segment2);
                if let Some(p) = ints.first() {
                    points[end_idx] = *p;
                    if end_idx - 1 > start_idx {
                        let seg = Segment::new(points[end_idx - 1], points[end_idx]);
                        if seg.length() < crate::graph::MIN_SEGMENT_LEN {
                            points[end_idx - 1] = points[end_idx];
                            end_idx -= 1;
                        }
                    }
                }
            }
            if !overlaps_outside_dst && !dst_is_rect {
                let bbox = dst_trace_box.unwrap_or_else(|| {
                    crate::geo::Box2D::new(
                        g.objects[dst_id].top_left,
                        g.objects[dst_id].width,
                        g.objects[dst_id].height,
                    )
                });
                let traced = trace_to_shape_border_with_box(
                    &g.objects[dst_id],
                    bbox,
                    points[end_idx],
                    points[end_idx - 1],
                );
                points[end_idx] = traced;
            }

            // Prune any points outside the live `[start_idx, end_idx]`
            // range so curve generation only sees the merged route.
            if end_idx + 1 < points.len() {
                points.truncate(end_idx + 1);
            }
            if start_idx > 0 {
                points.drain(0..start_idx);
            }
        }
        // Build curved path from route points. Mirror Go d2dagrelayout
        // pathData: the inner loop runs `for i := 1; i < len(vectors)-2;
        // i++`, so with len(vectors) == 4 it iterates only `i=1`.
        let mut path = Vec::new();
        if points.len() > 2 {
            let vectors: Vec<crate::geo::Vector> = (1..points.len())
                .map(|i| points[i - 1].vector_to(&points[i]))
                .collect();

            path.push(points[0]);
            if vectors.len() > 1 {
                path.push(points[0].add_vector(&vectors[0].multiply(0.8)));
                if vectors.len() >= 3 {
                    for i in 1..vectors.len() - 2 {
                        let p = points[i];
                        let v = &vectors[i];
                        path.push(p.add_vector(&v.multiply(0.2)));
                        path.push(p.add_vector(&v.multiply(0.5)));
                        path.push(p.add_vector(&v.multiply(0.8)));
                    }
                }
                let last_pt_idx = points.len() - 2;
                let last_vec_idx = vectors.len() - 1;
                path.push(points[last_pt_idx].add_vector(&vectors[last_vec_idx].multiply(0.2)));
                g.edges[ei].is_curve = true;
            }
            path.push(points[points.len() - 1]);
        } else {
            path = points;
        }

        g.edges[ei].route = path;
    }

    let deferred = apply_constant_near_subgraphs(g, &mut constant_near_subgraphs, opts)?;

    Ok(deferred)
}

/// Recursively shrink each top-level container to wrap its children with the
/// minimum padding (mirrors Go d2layouts/d2dagrelayout/layout.go
/// fitContainerPadding + fitPadding).
///
/// We only port the simple-rectangle path: containers with a non-rectangle
/// shape, an inside label, an inside icon, or any internal-edge label-mask
/// overlap detection are left alone for now. Those rely on `Object.Spacing`
/// returning per-direction margin/padding values that account for label and
/// icon positioning, which we haven't implemented yet.
fn fit_container_padding(g: &mut Graph, excluded_objects: &HashSet<ObjId>) {
    let root_children: Vec<ObjId> = g.objects[g.root].children_array.clone();
    for child_id in root_children {
        if is_constant_near_key(g.objects[child_id].near_key.as_deref()) {
            continue;
        }
        if excluded_objects.contains(&child_id) {
            continue;
        }
        fit_padding(g, child_id, excluded_objects);
    }
}

fn fit_padding(g: &mut Graph, obj_id: ObjId, excluded_objects: &HashSet<ObjId>) {
    let dsl_shape = g.objects[obj_id].shape.value.clone();
    let is_container = !g.objects[obj_id].children_array.is_empty();

    // Recurse depth-first regardless: nested containers must be sized first
    // so the parent's inner-box computation sees their final dimensions.
    let children: Vec<ObjId> = g.objects[obj_id]
        .children_array
        .iter()
        .copied()
        .filter(|c| !excluded_objects.contains(c))
        .collect();
    for child in &children {
        fit_padding(g, *child, excluded_objects);
    }

    // Match Go: only square-type containers (rectangle, sequence diagram,
    // hierarchy, default "") get their bounds shrunk. Others are left to
    // whatever dagre produced.
    let is_square = matches!(
        dsl_shape.as_str(),
        "" | "rectangle" | "sequence_diagram" | "hierarchy"
    );
    if !is_container || !is_square {
        return;
    }

    // Compute inner box from children's positions plus the parent's padding.
    // Use the container's own Spacing (which accounts for outside labels on
    // the container itself), then clamp to at least DEFAULT_PADDING.
    // Mirrors Go: `_, padding := obj.Spacing(); padding.Top = math.Max(...)`.
    let (_, own_pad) = g.objects[obj_id].spacing();
    let pad_top = own_pad.top.max(DEFAULT_PADDING);
    let pad_bottom = own_pad.bottom.max(DEFAULT_PADDING);
    let pad_left = own_pad.left.max(DEFAULT_PADDING);
    let pad_right = own_pad.right.max(DEFAULT_PADDING);

    let current_top = g.objects[obj_id].top_left.y;
    let current_bottom = g.objects[obj_id].top_left.y + g.objects[obj_id].height;
    let current_left = g.objects[obj_id].top_left.x;
    let current_right = g.objects[obj_id].top_left.x + g.objects[obj_id].width;

    let mut inner_top = f64::INFINITY;
    let mut inner_bottom = f64::NEG_INFINITY;
    let mut inner_left = f64::INFINITY;
    let mut inner_right = f64::NEG_INFINITY;

    // Mirror Go: collect inside-label / inside-icon boxes for this container so
    // we can prevent shrinkage that would collide with children's outside
    // label/icon boxes.
    let mut container_label_info: Option<(crate::geo::Box2D, crate::label::Position)> = None;
    let mut container_icon_info: Option<(crate::geo::Box2D, crate::label::Position)> = None;
    {
        let obj = &g.objects[obj_id];
        if obj.has_label()
            && let Some(lp_str) = obj.label_position.as_deref()
        {
            let lp = crate::label::Position::from_string(lp_str);
            use crate::label::Position::*;
            let is_inside = matches!(
                lp,
                InsideTopLeft
                    | InsideTopCenter
                    | InsideTopRight
                    | InsideBottomLeft
                    | InsideBottomCenter
                    | InsideBottomRight
                    | InsideMiddleLeft
                    | InsideMiddleRight
            );
            if is_inside && let Some(tl) = obj.get_label_top_left() {
                let lw = obj.label_dimensions.width as f64 + 2.0 * crate::label::PADDING;
                let lh = obj.label_dimensions.height as f64;
                container_label_info = Some((crate::geo::Box2D::new(tl, lw, lh), lp));
            }
        }
        if obj.has_icon()
            && let Some(ip_str) = obj.icon_position.as_deref()
        {
            let ip = crate::label::Position::from_string(ip_str);
            use crate::label::Position::*;
            let is_inside = matches!(
                ip,
                InsideTopLeft
                    | InsideTopCenter
                    | InsideTopRight
                    | InsideBottomLeft
                    | InsideBottomCenter
                    | InsideBottomRight
                    | InsideMiddleLeft
                    | InsideMiddleRight
            );
            if is_inside && let Some(tl) = obj.get_icon_top_left() {
                let sz = crate::target::MAX_ICON_SIZE as f64;
                container_icon_info = Some((crate::geo::Box2D::new(tl, sz, sz), ip));
            }
        }
    }
    let needs_inner_boxes = container_label_info.is_some() || container_icon_info.is_some();
    let mut inner_boxes: Vec<crate::geo::Box2D> = Vec::new();

    for &child in &children {
        // Use `Object::spacing()` so outside icons (MAX_ICON_SIZE) contribute
        // to the child's margin on the same axis as its outside label.
        // Mirrors Go `fitPadding` which calls `child.Spacing()`.
        let c = &g.objects[child];
        let (margin, _) = c.spacing();
        let (dx, dy) = c.get_modifier_element_adjustments();

        if needs_inner_boxes {
            if c.has_label()
                && let Some(lp_str) = c.label_position.as_deref()
            {
                let lp = crate::label::Position::from_string(lp_str);
                if lp.is_outside()
                    && let Some(tl) = c.get_label_top_left()
                {
                    inner_boxes.push(crate::geo::Box2D::new(
                        tl,
                        c.label_dimensions.width as f64,
                        c.label_dimensions.height as f64,
                    ));
                }
            }
            if c.has_icon()
                && let Some(ip_str) = c.icon_position.as_deref()
            {
                let ip = crate::label::Position::from_string(ip_str);
                if ip.is_outside()
                    && let Some(tl) = c.get_icon_top_left()
                {
                    let sz = crate::target::MAX_ICON_SIZE as f64;
                    inner_boxes.push(crate::geo::Box2D::new(tl, sz, sz));
                }
            }
        }

        inner_top = inner_top.min(c.top_left.y - dy - margin.top.max(pad_top));
        inner_bottom = inner_bottom.max(c.top_left.y + c.height + margin.bottom.max(pad_bottom));
        inner_left = inner_left.min(c.top_left.x - margin.left.max(pad_left));
        inner_right = inner_right.max(c.top_left.x + c.width + dx + margin.right.max(pad_right));
    }

    // Internal edges: walk all edges whose src AND dst are descendants of this
    // container and include their route points (and label boxes) in the inner
    // bounding box.  Mirrors Go `fitPadding` edge loop.
    for edge_idx in 0..g.edges.len() {
        let src_id = g.edges[edge_idx].src;
        let dst_id = g.edges[edge_idx].dst;
        let src_is_desc = g.objects[src_id].is_descendant_of(src_id, obj_id, g);
        let dst_is_desc = g.objects[dst_id].is_descendant_of(dst_id, obj_id, g);
        if !src_is_desc || !dst_is_desc {
            continue;
        }
        // Include edge label box
        if !g.edges[edge_idx].label.value.is_empty() {
            let label_width = g.edges[edge_idx].label_dimensions.width as f64;
            let label_height = g.edges[edge_idx].label_dimensions.height as f64;
            let route = crate::geo::Route(g.edges[edge_idx].route.clone());
            let lp_str = g.edges[edge_idx]
                .label_position
                .as_deref()
                .unwrap_or("InsideMiddleCenter");
            let lp = crate::label::Position::from_string(lp_str);
            if let Some((pt, _)) =
                lp.get_point_on_route(&route, 2.0, 0.0, label_width, label_height)
            {
                if needs_inner_boxes {
                    inner_boxes.push(crate::geo::Box2D::new(pt, label_width, label_height));
                }
                inner_top = inner_top.min(pt.y - pad_top);
                inner_bottom = inner_bottom.max(pt.y + label_height + pad_bottom);
                inner_left = inner_left.min(pt.x - pad_left);
                inner_right = inner_right.max(pt.x + label_width + pad_right);
            }
        }
        // Include route points
        for pt in &g.edges[edge_idx].route {
            inner_top = inner_top.min(pt.y - pad_top);
            inner_bottom = inner_bottom.max(pt.y + pad_bottom);
            inner_left = inner_left.min(pt.x - pad_left);
            inner_right = inner_right.max(pt.x + pad_right);
        }
    }

    let mut top_delta = inner_top - current_top;
    let mut bottom_delta = current_bottom - inner_bottom;
    let mut left_delta = inner_left - current_left;
    let mut right_delta = current_right - inner_right;

    // Mirror Go fitPadding: when a shrinking side would pull the container's
    // inside label/icon into one of the children's outside label/icon boxes,
    // reduce the shrink by the overlap depth plus MIN_SPACING so the container
    // stays large enough to avoid collisions.
    if (top_delta > 0.0 || bottom_delta > 0.0 || left_delta > 0.0 || right_delta > 0.0)
        && !inner_boxes.is_empty()
    {
        use crate::label::Position::*;
        let side_of = |pos: crate::label::Position| -> Option<u8> {
            match pos {
                InsideTopLeft | InsideTopCenter | InsideTopRight => Some(0),
                InsideBottomLeft | InsideBottomCenter | InsideBottomRight => Some(1),
                InsideMiddleLeft => Some(2),
                InsideMiddleRight => Some(3),
                _ => None,
            }
        };

        let mut top_overlap = 0.0f64;
        let mut bottom_overlap = 0.0f64;
        let mut left_overlap = 0.0f64;
        let mut right_overlap = 0.0f64;

        let process = |info: &Option<(crate::geo::Box2D, crate::label::Position)>,
                       top_overlap: &mut f64,
                       bottom_overlap: &mut f64,
                       left_overlap: &mut f64,
                       right_overlap: &mut f64| {
            let Some((b, pos)) = info else { return };
            let side = match side_of(*pos) {
                Some(s) => s,
                None => return,
            };
            // Move box by the projected shrink delta.
            let mut moved = *b;
            match side {
                0 if top_delta > 0.0 => {
                    moved.top_left.y += top_delta;
                }
                1 if bottom_delta > 0.0 => {
                    moved.top_left.y -= bottom_delta;
                }
                2 if left_delta > 0.0 => {
                    moved.top_left.x += left_delta;
                }
                3 if right_delta > 0.0 => {
                    moved.top_left.x -= right_delta;
                }
                _ => {}
            }
            for ib in &inner_boxes {
                if !moved.overlaps(ib) {
                    continue;
                }
                match side {
                    0 => {
                        let dy = moved.top_left.y + moved.height - ib.top_left.y;
                        if dy > 0.0 {
                            *top_overlap = top_overlap.max(dy);
                        }
                    }
                    1 => {
                        let dy = ib.top_left.y + ib.height - moved.top_left.y;
                        if dy > 0.0 {
                            *bottom_overlap = bottom_overlap.max(dy);
                        }
                    }
                    2 => {
                        let dx = moved.top_left.x + moved.width - ib.top_left.x;
                        if dx > 0.0 {
                            *left_overlap = left_overlap.max(dx);
                        }
                    }
                    3 => {
                        let dx = ib.top_left.x + ib.width - moved.top_left.x;
                        if dx > 0.0 {
                            *right_overlap = right_overlap.max(dx);
                        }
                    }
                    _ => {}
                }
            }
        };

        process(
            &container_label_info,
            &mut top_overlap,
            &mut bottom_overlap,
            &mut left_overlap,
            &mut right_overlap,
        );
        process(
            &container_icon_info,
            &mut top_overlap,
            &mut bottom_overlap,
            &mut left_overlap,
            &mut right_overlap,
        );

        if top_overlap > 0.0 {
            top_delta -= top_overlap + MIN_SPACING;
        }
        if bottom_overlap > 0.0 {
            bottom_delta -= bottom_overlap + MIN_SPACING;
        }
        if left_overlap > 0.0 {
            left_delta -= left_overlap + MIN_SPACING;
        }
        if right_overlap > 0.0 {
            right_delta -= right_overlap + MIN_SPACING;
        }
    }

    // Only shrink (positive delta = excess space we can trim). For each
    // side, first reduce `delta` by any edge that would otherwise get
    // squeezed past `DEFAULT_PADDING` from the collapsing edge, then move
    // any edge points that sit exactly on the old boundary so they stay
    // glued to the shrunk container. Mirrors Go `fitPadding` / `adjustEdges`
    // / `adjustDeltaForEdges`.
    if top_delta > 0.0 {
        let new_delta = adjust_delta_for_edges(g, obj_id, current_top, top_delta, false);
        if new_delta > 0.0 {
            adjust_edges(g, obj_id, current_top, new_delta, false);
            g.objects[obj_id].top_left.y += new_delta;
            g.objects[obj_id].height -= new_delta;
        }
    }
    if bottom_delta > 0.0 {
        let new_delta = adjust_delta_for_edges(g, obj_id, current_bottom, -bottom_delta, false);
        if new_delta > 0.0 {
            adjust_edges(g, obj_id, current_bottom, -new_delta, false);
            g.objects[obj_id].height -= new_delta;
        }
    }
    if left_delta > 0.0 {
        let new_delta = adjust_delta_for_edges(g, obj_id, current_left, left_delta, true);
        if new_delta > 0.0 {
            adjust_edges(g, obj_id, current_left, new_delta, true);
            g.objects[obj_id].top_left.x += new_delta;
            g.objects[obj_id].width -= new_delta;
        }
    }
    if right_delta > 0.0 {
        let new_delta = adjust_delta_for_edges(g, obj_id, current_right, -right_delta, true);
        if new_delta > 0.0 {
            adjust_edges(g, obj_id, current_right, -new_delta, true);
            g.objects[obj_id].width -= new_delta;
        }
    }
    g.objects[obj_id].update_box();
}

/// Match Go `adjustEdges`: move route endpoints that currently sit on the
/// collapsing side of `obj` (`obj_position` on the given axis) by `delta`.
/// Also moves points that lie strictly between `obj_position` and the new
/// edge if they happen to be on the perpendicular sides of the box.
fn adjust_edges(g: &mut Graph, obj_id: ObjId, obj_position: f64, delta: f64, is_horizontal: bool) {
    // Capture the object's rectangle before mutating so the side check
    // uses a consistent snapshot.
    let shape_box = crate::geo::Box2D::new(
        g.objects[obj_id].top_left,
        g.objects[obj_id].width,
        g.objects[obj_id].height,
    );
    for ei in 0..g.edges.len() {
        if g.edges[ei].src == obj_id
            && let Some(p) = g.edges[ei].route.first_mut()
        {
            adjust_one_point(p, obj_position, delta, is_horizontal, &shape_box);
        }
        if g.edges[ei].dst == obj_id {
            let last = g.edges[ei].route.len().saturating_sub(1);
            if !g.edges[ei].route.is_empty() {
                let pt = &mut g.edges[ei].route[last];
                adjust_one_point(pt, obj_position, delta, is_horizontal, &shape_box);
            }
        }
    }
}

fn adjust_one_point(
    p: &mut Point,
    obj_position: f64,
    delta: f64,
    is_horizontal: bool,
    shape_box: &crate::geo::Box2D,
) {
    let position = if is_horizontal { p.x } else { p.y };
    if precision_eq(position, obj_position) {
        if is_horizontal {
            p.x += delta;
        } else {
            p.y += delta;
        }
    } else {
        let tl_x = shape_box.top_left.x;
        let tl_y = shape_box.top_left.y;
        let is_on_side = if is_horizontal {
            precision_eq(p.y, tl_y) || precision_eq(p.y, tl_y + shape_box.height)
        } else {
            precision_eq(p.x, tl_x) || precision_eq(p.x, tl_x + shape_box.width)
        };
        if is_on_side {
            let in_range = if delta > 0.0 {
                obj_position < position && position < obj_position + delta
            } else {
                obj_position + delta < position && position < obj_position
            };
            if in_range {
                if is_horizontal {
                    p.x = obj_position + delta;
                } else {
                    p.y = obj_position + delta;
                }
            }
        }
    }
}

/// Match Go `adjustDeltaForEdges`: reduce `delta` so that shrinking `obj`
/// never collapses past an edge endpoint that is currently sitting within
/// `DEFAULT_PADDING` of the collapsing side.
fn adjust_delta_for_edges(
    g: &Graph,
    obj_id: ObjId,
    obj_position: f64,
    delta: f64,
    is_horizontal: bool,
) -> f64 {
    let tl_x = g.objects[obj_id].top_left.x;
    let tl_y = g.objects[obj_id].top_left.y;
    let w = g.objects[obj_id].width;
    let h = g.objects[obj_id].height;

    let is_on_collapsing_side = |p: &Point| -> bool {
        let position = if is_horizontal { p.x } else { p.y };
        if precision_eq(position, obj_position) {
            return false;
        }
        let is_on_side = if is_horizontal {
            precision_eq(p.y, tl_y) || precision_eq(p.y, tl_y + h)
        } else {
            precision_eq(p.x, tl_x) || precision_eq(p.x, tl_x + w)
        };
        if !is_on_side {
            return false;
        }
        let buffer = MIN_SPACING;
        if delta > 0.0 {
            obj_position <= position && position <= obj_position + delta + buffer
        } else {
            obj_position + delta - buffer <= position && position <= obj_position
        }
    };

    let mut has_edge_on_collapsing_side = false;
    let mut outermost = obj_position + delta;
    for edge in &g.edges {
        if edge.src == obj_id
            && let Some(p) = edge.route.first()
            && is_on_collapsing_side(p)
        {
            has_edge_on_collapsing_side = true;
            let position = if is_horizontal { p.x } else { p.y };
            if delta < 0.0 {
                outermost = outermost.max(position);
            } else {
                outermost = outermost.min(position);
            }
        }
        if edge.dst == obj_id
            && let Some(p) = edge.route.last()
            && is_on_collapsing_side(p)
        {
            has_edge_on_collapsing_side = true;
            let position = if is_horizontal { p.x } else { p.y };
            if delta < 0.0 {
                outermost = outermost.max(position);
            } else {
                outermost = outermost.min(position);
            }
        }
    }

    let mut new_magnitude = delta.abs();
    if has_edge_on_collapsing_side {
        new_magnitude = if delta < 0.0 {
            (obj_position - (outermost + DEFAULT_PADDING)).max(0.0)
        } else {
            ((outermost - DEFAULT_PADDING) - obj_position).max(0.0)
        };
    }
    new_magnitude
}

/// Match Go `geo.PrecisionCompare(a, b, 1) == 0` — i.e. two points are
/// considered on the same axis if they are within a pixel of each other.
fn precision_eq(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1.0
}

/// Thin wrapper around `crate::shape::trace_to_shape_border`. Constructs a
/// transient `crate::shape::Shape` from the object's current box and shape
/// type, then delegates the perimeter-intersection math there.
///
/// Important: `crate::shape::Shape::new` expects the *internal* shape type
/// (capitalized, e.g. `"Document"`), not the DSL name stored on
/// `Object.shape.value` (lowercase, e.g. `"document"`). Without the
/// translation `Shape::new` silently falls through to `Rectangle` and
/// `perimeter()` returns an empty `Vec`, so `trace_to_shape_border`
/// returns `rect_border_point` unchanged and every non-rectangular
/// shape endpoint looked like it was still at the bounding-box edge.
/// Route the name through `crate::target::dsl_shape_to_shape_type` the same
/// way `d2-svg-render` does when it builds shapes for rendering.
///
/// Cloud shapes additionally need their inner-box aspect ratio so the
/// dashed cloud outline matches Go; forward it if the object already
/// computed one via `Object::content_aspect_ratio`.
/// Like a thin wrapper around `crate::shape::trace_to_shape_border`, but lets
/// the caller override the shape's bounding box. Needed for the
/// 3d/multiple modifier zone: Go's `d2dagrelayout.Layout` temporarily
/// mutates `edge.Dst.TopLeft` before `Edge.TraceToShape`, so the
/// downstream `shape.TraceToShapeBorder` sees the offset outline. We
/// replicate that by passing the shifted box here instead of mutating
/// the graph object.
fn trace_to_shape_border_with_box(
    obj: &crate::graph::Object,
    bbox: crate::geo::Box2D,
    rect_border_point: Point,
    prev_point: Point,
) -> Point {
    let shape_type = crate::target::dsl_shape_to_shape_type(obj.shape.value.as_str());
    let mut shape = crate::shape::Shape::new(shape_type, bbox);
    if obj.shape.value == crate::target::SHAPE_CLOUD
        && let Some(ratio) = obj.content_aspect_ratio
    {
        shape.set_inner_box_aspect_ratio(ratio);
    }
    crate::shape::trace_to_shape_border(&shape, &rect_border_point, &prev_point)
}

/// Compute the outside-label `Box2D` and its position for an object — the
/// rectangle occupied by the shape's label when it sits outside the shape
/// border. Returns `None` when the object has no outside label (no label,
/// no position, or the position is not `OUTSIDE_*`). Mirrors the label box
/// construction at the top of Go `Edge.TraceToShape`.
fn outside_label_box_for(
    obj: &crate::graph::Object,
    shape_box: &crate::geo::Box2D,
) -> Option<(crate::geo::Box2D, crate::label::Position)> {
    if !obj.has_label() {
        return None;
    }
    let pos_str = obj.label_position.as_deref()?;
    let pos = crate::label::Position::from_string(pos_str);
    if !pos.is_outside() {
        return None;
    }
    let label_width = obj.label_dimensions.width as f64;
    let label_height = obj.label_dimensions.height as f64;
    let label_tl =
        pos.get_point_on_box(shape_box, crate::label::PADDING, label_width, label_height);
    // Go adds horizontal padding so the label box extends `PADDING` past
    // the label text on each side, preventing connections from clipping
    // the label's left/right edge.
    let mut box_ = crate::geo::Box2D::new(label_tl, label_width, label_height);
    box_.top_left.x -= crate::label::PADDING;
    box_.width += 2.0 * crate::label::PADDING;
    Some((box_, pos))
}

/// Compute the outside-icon `Box2D` for the DST side of an edge. Uses
/// `d2target.GetIconSize(Dst.Box, iconPosition)` — matches Go
/// `Edge.TraceToShape` lines 540-569.
fn outside_icon_box_for(
    obj: &crate::graph::Object,
    shape_box: &crate::geo::Box2D,
) -> Option<(crate::geo::Box2D, crate::label::Position)> {
    if !obj.has_icon() {
        return None;
    }
    let pos_str = obj.icon_position.as_deref()?;
    let pos = crate::label::Position::from_string(pos_str);
    if !pos.is_outside() {
        return None;
    }
    let icon_side = crate::target::get_icon_size(obj.width, obj.height, pos_str) as f64;
    if icon_side <= 0.0 {
        return None;
    }
    let icon_tl = pos.get_point_on_box(shape_box, crate::label::PADDING, icon_side, icon_side);
    let box_ = crate::geo::Box2D::new(icon_tl, icon_side, icon_side);
    Some((box_, pos))
}

/// Compute the outside-icon `Box2D` for the SRC side of an edge. Mirrors
/// Go `Edge.TraceToShape` lines 456-486 which hard-codes `MAX_ICON_SIZE`
/// for the icon side length on the source side (unlike the Dst side which
/// uses the position-aware `GetIconSize`).
fn outside_icon_box_src_for(
    obj: &crate::graph::Object,
    shape_box: &crate::geo::Box2D,
) -> Option<(crate::geo::Box2D, crate::label::Position)> {
    if !obj.has_icon() {
        return None;
    }
    let pos_str = obj.icon_position.as_deref()?;
    let pos = crate::label::Position::from_string(pos_str);
    if !pos.is_outside() {
        return None;
    }
    let icon_side = crate::target::MAX_ICON_SIZE as f64;
    let icon_tl = pos.get_point_on_box(shape_box, crate::label::PADDING, icon_side, icon_side);
    let box_ = crate::geo::Box2D::new(icon_tl, icon_side, icon_side);
    Some((box_, pos))
}

/// Mirror Go `findOuterIntersection`: from a set of label-box intersections,
/// pick the point that sits on the "outside" side of the labelled shape.
/// For an OUTSIDE_TOP_* label that means the smallest Y, OUTSIDE_BOTTOM_*
/// the largest Y, OUTSIDE_LEFT_* the smallest X, OUTSIDE_RIGHT_* the
/// largest X. Falls back to the first intersection for any other case.
fn find_outer_intersection(pos: crate::label::Position, intersections: &[Point]) -> Point {
    use crate::label::Position::*;
    if intersections.len() <= 1 {
        return intersections[0];
    }
    let mut sorted: Vec<Point> = intersections.to_vec();
    match pos {
        OutsideTopLeft | OutsideTopRight | OutsideTopCenter => {
            sorted.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
        }
        OutsideBottomLeft | OutsideBottomRight | OutsideBottomCenter => {
            sorted.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal));
        }
        OutsideLeftTop | OutsideLeftMiddle | OutsideLeftBottom => {
            sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        }
        OutsideRightTop | OutsideRightMiddle | OutsideRightBottom => {
            sorted.sort_by(|a, b| b.x.partial_cmp(&a.x).unwrap_or(std::cmp::Ordering::Equal));
        }
        _ => {}
    }
    sorted[0]
}

/// Outside-label margin for a child object — mirrors the margin half of
/// Go's `Object.Spacing()` but ignoring icons. Kept around for callers that
/// explicitly want label-only semantics; `fit_padding` now uses the full
/// `Object::spacing()` so outside icons contribute too.
#[allow(dead_code)]
fn child_outside_margin(obj: &crate::graph::Object) -> (crate::geo::Spacing, crate::geo::Spacing) {
    let zero = crate::geo::Spacing {
        top: 0.0,
        bottom: 0.0,
        left: 0.0,
        right: 0.0,
    };
    let mut margin = zero;
    if obj.has_label() && obj.label_position.is_some() {
        // Go uses 2 * label.PADDING (== 10) of slack around the label.
        const LABEL_PADDING_2X: f64 = 10.0;
        let lw = obj.label_dimensions.width as f64 + LABEL_PADDING_2X;
        let lh = obj.label_dimensions.height as f64 + LABEL_PADDING_2X;
        match obj.label_position.as_deref().unwrap_or("") {
            "OUTSIDE_TOP_LEFT" | "OUTSIDE_TOP_CENTER" | "OUTSIDE_TOP_RIGHT" => {
                margin.top = lh;
            }
            "OUTSIDE_BOTTOM_LEFT" | "OUTSIDE_BOTTOM_CENTER" | "OUTSIDE_BOTTOM_RIGHT" => {
                margin.bottom = lh;
            }
            "OUTSIDE_LEFT_TOP" | "OUTSIDE_LEFT_MIDDLE" | "OUTSIDE_LEFT_BOTTOM" => {
                margin.left = lw;
            }
            "OUTSIDE_RIGHT_TOP" | "OUTSIDE_RIGHT_MIDDLE" | "OUTSIDE_RIGHT_BOTTOM" => {
                margin.right = lw;
            }
            _ => {}
        }
    }
    (margin, zero)
}

// ---------------------------------------------------------------------------
// Post-layout rank spacing adjustments
//
// These mirror Go `d2layouts/d2dagrelayout/layout.go`:
//   - `getRanks`
//   - `shiftDown`
//   - `shiftUp`
//   - `shiftReachableDown`
//   - `adjustRankSpacing`
//   - `adjustCrossRankSpacing`
//   - `adjustDeltaForEdges` / `adjustEdges`
//
// They run after dagre finishes so that shapes with outside labels, icons,
// or 3D/multiple modifiers get enough surrounding space, and so that nested
// containers keep their padding clean.
// ---------------------------------------------------------------------------

use crate::geo::Spacing;

/// (ranks, object_rank, container_min_rank, container_max_rank).
type Ranks = (
    Vec<Vec<ObjId>>,
    HashMap<ObjId, usize>,
    HashMap<ObjId, usize>,
    HashMap<ObjId, usize>,
);

/// Build rank data for post-processing. Mirrors Go `getRanks` — groups
/// non-container objects by their (post-dagre) center position along the
/// cross-axis, then records, for every container, the min/max rank of its
/// non-container descendants. Non-containers land in `object_ranks`.
fn get_ranks(g: &Graph, is_horizontal: bool, excluded_objects: &HashSet<ObjId>) -> Ranks {
    // i64-keyed buckets so ordering is deterministic and stable across runs.
    let mut aligned: std::collections::BTreeMap<i64, Vec<ObjId>> = Default::default();
    for (i, obj) in g.objects.iter().enumerate() {
        if i == g.root {
            continue;
        }
        if obj.is_container() {
            continue;
        }
        // Skip objects that were physically extracted from the graph before
        // dagre ran (grid descendants, sequence diagram internals). Mirrors
        // Go's behaviour: those objects live in their own sub-graph while
        // dagre runs so `g.Objects` doesn't see them at all.
        if excluded_objects.contains(&i) {
            continue;
        }
        // Skip objects removed from layout (e.g. sequence diagram internals
        // marked with a sentinel shape).
        if obj.shape.value == "__d2_seq_nested_removed__"
            || obj.shape.value == "__d2_class_field_removed__"
        {
            continue;
        }
        let key = if is_horizontal {
            (obj.top_left.x + obj.width / 2.0).ceil()
        } else {
            (obj.top_left.y + obj.height / 2.0).ceil()
        };
        aligned.entry(key as i64).or_default().push(i);
    }

    let mut ranks: Vec<Vec<ObjId>> = Vec::with_capacity(aligned.len());
    let mut object_ranks: HashMap<ObjId, usize> = HashMap::new();
    for (rank_idx, (_, objs)) in aligned.into_iter().enumerate() {
        for &o in &objs {
            object_ranks.insert(o, rank_idx);
        }
        ranks.push(objs);
    }

    let mut starting_parent_ranks: HashMap<ObjId, usize> = HashMap::new();
    let mut ending_parent_ranks: HashMap<ObjId, usize> = HashMap::new();
    for (i, obj) in g.objects.iter().enumerate() {
        if i == g.root || obj.is_container() {
            continue;
        }
        if excluded_objects.contains(&i) {
            continue;
        }
        // Skip sentinel-shaped objects (same filter as rank assignment above).
        let r = match object_ranks.get(&i) {
            Some(&r) => r,
            None => continue,
        };
        let mut p = obj.parent;
        while let Some(pid) = p {
            if pid == g.root {
                break;
            }
            let e = starting_parent_ranks.entry(pid).or_insert(r);
            if r < *e {
                *e = r;
            }
            let e2 = ending_parent_ranks.entry(pid).or_insert(r);
            if r > *e2 {
                *e2 = r;
            }
            p = g.objects[pid].parent;
        }
    }

    (
        ranks,
        object_ranks,
        starting_parent_ranks,
        ending_parent_ranks,
    )
}

/// Shift everything at-or-below `start` down by `distance` (mirrors Go
/// `shiftDown`). Also shifts edge routes with guards to avoid collapsing
/// endpoints onto static neighbours.
fn shift_down(
    g: &mut Graph,
    start: f64,
    distance: f64,
    is_horizontal: bool,
    excluded_objects: &HashSet<ObjId>,
) {
    if is_horizontal {
        let edge_len = g.edges.len();
        for ei in 0..edge_len {
            let (src_id, dst_id) = (g.edges[ei].src, g.edges[ei].dst);
            if excluded_objects.contains(&src_id) || excluded_objects.contains(&dst_id) {
                continue;
            }
            let src_right = g.objects[src_id].top_left.x + g.objects[src_id].width;
            let src_left = g.objects[src_id].top_left.x;
            let dst_right = g.objects[dst_id].top_left.x + g.objects[dst_id].width;
            let dst_left = g.objects[dst_id].top_left.x;
            let route = &mut g.edges[ei].route;
            if route.is_empty() {
                continue;
            }
            let last = route.len() - 1;
            let first_x = route[0].x;
            let last_x = route[last].x;
            if start <= first_x {
                let on_static_src = first_x == src_right && src_left < start;
                if !on_static_src {
                    route[0].x += distance;
                }
            }
            if start <= last_x {
                let on_static_dst = last_x == dst_right && dst_left < start;
                if !on_static_dst {
                    route[last].x += distance;
                }
            }
            for pt in &mut route[1..last] {
                if pt.x >= start {
                    pt.x += distance;
                }
            }
        }
        for i in 0..g.objects.len() {
            if i == g.root {
                continue;
            }
            if excluded_objects.contains(&i) {
                continue;
            }
            if g.objects[i].top_left.x < start {
                continue;
            }
            g.objects[i].top_left.x += distance;
        }
    } else {
        let edge_len = g.edges.len();
        for ei in 0..edge_len {
            let (src_id, dst_id) = (g.edges[ei].src, g.edges[ei].dst);
            if excluded_objects.contains(&src_id) || excluded_objects.contains(&dst_id) {
                continue;
            }
            let src_bot = g.objects[src_id].top_left.y + g.objects[src_id].height;
            let src_top = g.objects[src_id].top_left.y;
            let dst_bot = g.objects[dst_id].top_left.y + g.objects[dst_id].height;
            let dst_top = g.objects[dst_id].top_left.y;
            let route = &mut g.edges[ei].route;
            if route.is_empty() {
                continue;
            }
            let last = route.len() - 1;
            let first_y = route[0].y;
            let last_y = route[last].y;
            if start <= first_y {
                let on_static_src = first_y == src_bot && src_top < start;
                if !on_static_src {
                    route[0].y += distance;
                }
            }
            if start <= last_y {
                let on_static_dst = last_y == dst_bot && dst_top < start;
                if !on_static_dst {
                    route[last].y += distance;
                }
            }
            for pt in &mut route[1..last] {
                if pt.y >= start {
                    pt.y += distance;
                }
            }
        }
        for i in 0..g.objects.len() {
            if i == g.root {
                continue;
            }
            if excluded_objects.contains(&i) {
                continue;
            }
            if g.objects[i].top_left.y < start {
                continue;
            }
            g.objects[i].top_left.y += distance;
        }
    }
}

/// Mirror of Go `shiftUp`: shift everything at-or-above `start` up by
/// `distance`, with the same edge-endpoint guards as `shift_down`.
fn shift_up(
    g: &mut Graph,
    start: f64,
    distance: f64,
    is_horizontal: bool,
    excluded_objects: &HashSet<ObjId>,
) {
    if is_horizontal {
        let edge_len = g.edges.len();
        for ei in 0..edge_len {
            let (src_id, dst_id) = (g.edges[ei].src, g.edges[ei].dst);
            if excluded_objects.contains(&src_id) || excluded_objects.contains(&dst_id) {
                continue;
            }
            let src_left = g.objects[src_id].top_left.x;
            let src_right = g.objects[src_id].top_left.x + g.objects[src_id].width;
            let dst_left = g.objects[dst_id].top_left.x;
            let dst_right = g.objects[dst_id].top_left.x + g.objects[dst_id].width;
            let route = &mut g.edges[ei].route;
            if route.is_empty() {
                continue;
            }
            let last = route.len() - 1;
            let first_x = route[0].x;
            let last_x = route[last].x;
            if first_x <= start {
                let on_static_src = first_x == src_left && start < src_right;
                if !on_static_src {
                    route[0].x -= distance;
                }
            }
            if last_x <= start {
                let on_static_dst = last_x == dst_left && start < dst_right;
                if !on_static_dst {
                    route[last].x -= distance;
                }
            }
            for pt in &mut route[1..last] {
                if start >= pt.x {
                    pt.x -= distance;
                }
            }
        }
        for i in 0..g.objects.len() {
            if i == g.root {
                continue;
            }
            if excluded_objects.contains(&i) {
                continue;
            }
            if start < g.objects[i].top_left.x {
                continue;
            }
            g.objects[i].top_left.x -= distance;
        }
    } else {
        let edge_len = g.edges.len();
        for ei in 0..edge_len {
            let (src_id, dst_id) = (g.edges[ei].src, g.edges[ei].dst);
            if excluded_objects.contains(&src_id) || excluded_objects.contains(&dst_id) {
                continue;
            }
            let src_top = g.objects[src_id].top_left.y;
            let src_bot = g.objects[src_id].top_left.y + g.objects[src_id].height;
            let dst_top = g.objects[dst_id].top_left.y;
            // Go `shiftUp` has a small typo where dst checks only
            // `top_left.y` for `onStaticDst`; keep that quirk for bytewise
            // parity.
            let route = &mut g.edges[ei].route;
            if route.is_empty() {
                continue;
            }
            let last = route.len() - 1;
            let first_y = route[0].y;
            let last_y = route[last].y;
            if first_y <= start {
                let on_static_src = first_y == src_top && start < src_bot;
                if !on_static_src {
                    route[0].y -= distance;
                }
            }
            if last_y <= start {
                let on_static_dst = last_y == dst_top && start < dst_top;
                if !on_static_dst {
                    route[last].y -= distance;
                }
            }
            for pt in &mut route[1..last] {
                if start >= pt.y {
                    pt.y -= distance;
                }
            }
        }
        for i in 0..g.objects.len() {
            if i == g.root {
                continue;
            }
            if excluded_objects.contains(&i) {
                continue;
            }
            if start < g.objects[i].top_left.y {
                continue;
            }
            g.objects[i].top_left.y -= distance;
        }
    }
}

/// BFS shift used by `adjust_cross_rank_spacing`. Shifts the chain of objects
/// reachable (via containment or edges) from `obj` down/right by `distance`,
/// growing containers that get stretched, and returning the set of shapes
/// whose "margin" has thereby expanded (so the caller can avoid re-adding the
/// same shift).
///
/// Mirrors Go `shiftReachableDown`. The `is_margin` flag distinguishes the
/// "move because of container padding" case (`false`) from the "move because
/// of this shape's own margin" case (`true`); the only difference is whether
/// objects exactly at `start` also move.
#[allow(clippy::too_many_arguments)]
fn shift_reachable_down(
    g: &mut Graph,
    obj: ObjId,
    start: f64,
    distance: f64,
    is_horizontal: bool,
    is_margin: bool,
    excluded_objects: &HashSet<ObjId>,
) -> HashSet<ObjId> {
    const THRESHOLD: f64 = 100.0;

    let mut q: Vec<ObjId> = vec![obj];
    let mut needs_move: HashSet<ObjId> = HashSet::new();
    let mut seen: HashSet<ObjId> = HashSet::new();
    let mut shifted: HashSet<ObjId> = HashSet::new();
    let mut shifted_edges: HashSet<usize> = HashSet::new();

    // Local helper: check whether any object `other` sits just below/right of
    // `curr` (within `threshold`) and should therefore also be shifted.
    let check_below = |g: &Graph,
                       q: &mut Vec<ObjId>,
                       seen: &HashSet<ObjId>,
                       shifted: &HashSet<ObjId>,
                       curr: ObjId| {
        let curr_obj = &g.objects[curr];
        let curr_bottom = curr_obj.top_left.y + curr_obj.height;
        let curr_right = curr_obj.top_left.x + curr_obj.width;
        if is_horizontal {
            let mut original_right = curr_right;
            if shifted.contains(&curr) {
                original_right -= distance;
            }
            for oi in 0..g.objects.len() {
                if oi == g.root || oi == curr {
                    continue;
                }
                if excluded_objects.contains(&oi) {
                    continue;
                }
                if g.objects[curr].is_descendant_of(curr, oi, g) {
                    continue;
                }
                let o = &g.objects[oi];
                if original_right < o.top_left.x
                    && o.top_left.x < original_right + distance + THRESHOLD
                    && curr_obj.top_left.y < o.top_left.y + o.height
                    && o.top_left.y < curr_bottom
                    && !seen.contains(&oi)
                {
                    q.push(oi);
                }
            }
        } else {
            let mut original_bottom = curr_bottom;
            if shifted.contains(&curr) {
                original_bottom -= distance;
            }
            for oi in 0..g.objects.len() {
                if oi == g.root || oi == curr {
                    continue;
                }
                if excluded_objects.contains(&oi) {
                    continue;
                }
                if g.objects[curr].is_descendant_of(curr, oi, g) {
                    continue;
                }
                let o = &g.objects[oi];
                if original_bottom < o.top_left.y
                    && o.top_left.y < original_bottom + distance + THRESHOLD
                    && curr_obj.top_left.x < o.top_left.x + o.width
                    && o.top_left.x < curr_right
                    && !seen.contains(&oi)
                {
                    q.push(oi);
                }
            }
        }
    };

    // Inner BFS loop — factored into a macro so it can also run inline from
    // the "grow containers" phase (Go calls `processQueue()` inline during
    // the parent walk, and we must do the same for nested containers' extra
    // width to propagate outward correctly).
    let mut grown: HashSet<ObjId> = HashSet::new();
    macro_rules! process_queue {
        () => {
            while let Some(curr) = (!q.is_empty()).then(|| q.remove(0)) {
                if seen.contains(&curr) {
                    continue;
                }
                if excluded_objects.contains(&curr) {
                    continue;
                }

                // Objects behind `start` don't move unless they were explicitly
                // queued via `needs_move` (e.g. their reverse-direction edge
                // anchor was pushed).
                if curr != obj && !needs_move.contains(&curr) {
                    let pos = if is_horizontal {
                        g.objects[curr].top_left.x
                    } else {
                        g.objects[curr].top_left.y
                    };
                    if pos < start {
                        continue;
                    }
                }

                // Decide whether to shift `curr`.
                let pos = if is_horizontal {
                    g.objects[curr].top_left.x
                } else {
                    g.objects[curr].top_left.y
                };
                let mut shift = needs_move.contains(&curr);
                if !shift {
                    shift = if is_margin { start <= pos } else { start < pos };
                }
                if shift {
                    if is_horizontal {
                        g.objects[curr].top_left.x += distance;
                    } else {
                        g.objects[curr].top_left.y += distance;
                    }
                    shifted.insert(curr);
                }
                seen.insert(curr);

                // Walk up the parent chain (unless curr is a descendant of the
                // original obj), to revisit ancestors that may need growing.
                if let Some(p) = g.objects[curr].parent {
                    if p != g.root
                        && !g.objects[curr].is_descendant_of(curr, obj, g)
                        && !seen.contains(&p)
                    {
                        q.push(p);
                    }
                }
                // Walk into children.
                let children: Vec<ObjId> = g.objects[curr].children_array.clone();
                for c in children {
                    if excluded_objects.contains(&c) {
                        continue;
                    }
                    if !seen.contains(&c) {
                        q.push(c);
                    }
                }

                // Walk edges incident on `curr`.
                for ei in 0..g.edges.len() {
                    if shifted_edges.contains(&ei) {
                        continue;
                    }
                    let (src, dst) = (g.edges[ei].src, g.edges[ei].dst);
                    if excluded_objects.contains(&src) || excluded_objects.contains(&dst) {
                        continue;
                    }
                    if src == curr && dst == curr {
                        // Self-edge: shift every route point.
                        let route = &mut g.edges[ei].route;
                        for p in route.iter_mut() {
                            if is_horizontal {
                                p.x += distance;
                            } else {
                                p.y += distance;
                            }
                        }
                        shifted_edges.insert(ei);
                        continue;
                    } else if src == curr {
                        let (route_len, first, last) = {
                            let r = &g.edges[ei].route;
                            if r.is_empty() {
                                (0, Point::new(0.0, 0.0), Point::new(0.0, 0.0))
                            } else {
                                (r.len(), r[0], r[r.len() - 1])
                            }
                        };
                        if route_len == 0 {
                            continue;
                        }
                        if is_horizontal {
                            if start <= last.x
                                && g.objects[dst].top_left.x + g.objects[dst].width
                                    < last.x + distance
                            {
                                needs_move.insert(dst);
                            }
                        } else if start <= last.y
                            && g.objects[dst].top_left.y + g.objects[dst].height < last.y + distance
                        {
                            needs_move.insert(dst);
                        }
                        if !seen.contains(&dst) {
                            q.push(dst);
                        }
                        let was_shifted = shifted.contains(&curr);
                        let curr_tl_x = g.objects[curr].top_left.x;
                        let curr_tl_y = g.objects[curr].top_left.y;
                        let route = &mut g.edges[ei].route;
                        let mut start_index = 0usize;
                        if is_horizontal {
                            if was_shifted && first.x < curr_tl_x && first.x < start {
                                route[0].x += distance;
                                start_index = 1;
                            }
                            for i in start_index..route.len() {
                                if start <= route[i].x {
                                    route[i].x += distance;
                                }
                            }
                        } else {
                            if was_shifted && first.y < curr_tl_y && first.y < start {
                                route[0].y += distance;
                                start_index = 1;
                            }
                            for i in start_index..route.len() {
                                if start <= route[i].y {
                                    route[i].y += distance;
                                }
                            }
                        }
                        shifted_edges.insert(ei);
                    } else if dst == curr {
                        let (route_len, first, last) = {
                            let r = &g.edges[ei].route;
                            if r.is_empty() {
                                (0, Point::new(0.0, 0.0), Point::new(0.0, 0.0))
                            } else {
                                (r.len(), r[0], r[r.len() - 1])
                            }
                        };
                        if route_len == 0 {
                            continue;
                        }
                        if is_horizontal {
                            if start <= first.x
                                && g.objects[src].top_left.x + g.objects[src].width
                                    < first.x + distance
                            {
                                needs_move.insert(src);
                            }
                        } else if start <= first.y
                            && g.objects[src].top_left.y + g.objects[src].height
                                < first.y + distance
                        {
                            needs_move.insert(src);
                        }
                        if !seen.contains(&src) {
                            q.push(src);
                        }
                        let was_shifted = shifted.contains(&curr);
                        let curr_tl_x = g.objects[curr].top_left.x;
                        let curr_tl_y = g.objects[curr].top_left.y;
                        let route = &mut g.edges[ei].route;
                        let mut end_index = route_len;
                        if is_horizontal {
                            if was_shifted && last.x < curr_tl_x && last.x < start {
                                route[route_len - 1].x += distance;
                                end_index = route_len - 1;
                            }
                            for i in 0..end_index {
                                if start <= route[i].x {
                                    route[i].x += distance;
                                }
                            }
                        } else {
                            if was_shifted && last.y < curr_tl_y && last.y < start {
                                route[route_len - 1].y += distance;
                                end_index = route_len - 1;
                            }
                            for i in 0..end_index {
                                if start <= route[i].y {
                                    route[i].y += distance;
                                }
                            }
                        }
                        shifted_edges.insert(ei);
                    }
                }

                check_below(g, &mut q, &seen, &shifted, curr);
            }
        };
    }

    process_queue!();

    // Grow ancestor containers that weren't themselves shifted but whose
    // descendants moved across `start`. Mirrors Go's post-BFS
    // container-grow walk exactly:
    //
    //   for o := range seen {
    //       for parent := o.Parent; parent != g.Root; parent = parent.Parent {
    //           if grown/shifted { break }
    //           if parent.TopLeft.X < start {
    //               parent.Width += distance
    //               grown[parent] = struct{}{}
    //               checkBelow(parent)
    //               processQueue()
    //           }
    //       }
    //   }
    //
    // The key subtlety: processQueue runs INLINE inside the parent loop, so
    // after growing `cell_tower` we drain the queue before walking up to
    // grow `network`. A naive "defer to outer loop" breaks multi-level
    // container chains because it restarts the parent walk from scratch
    // with `cell_tower` already in `grown`, so `network` is never visited
    // via that `o`.
    let seen_snapshot: Vec<ObjId> = seen.iter().copied().collect();
    for o in seen_snapshot {
        let mut p = g.objects[o].parent;
        while let Some(pid) = p {
            if pid == g.root {
                break;
            }
            if shifted.contains(&pid) || grown.contains(&pid) {
                break;
            }
            let (tl_x, tl_y) = (g.objects[pid].top_left.x, g.objects[pid].top_left.y);
            if is_horizontal {
                if tl_x < start {
                    g.objects[pid].width += distance;
                    grown.insert(pid);
                    check_below(g, &mut q, &seen, &shifted, pid);
                    process_queue!();
                }
            } else if tl_y < start {
                g.objects[pid].height += distance;
                grown.insert(pid);
                check_below(g, &mut q, &seen, &shifted, pid);
                process_queue!();
            }
            p = g.objects[pid].parent;
        }
    }
    {
        // After-grow bookkeeping (replaces the old `if q.is_empty()` branch —
        // inside the new single-pass structure, q is always drained once we
        // fall out of the grow loop, so we unconditionally compute increased
        // margins here).
        // Compute the set of "counted" margin increases, matching Go's
        // rule: a shifted/grown object's margin only counts if no other
        // shifted/grown object sits directly above (or beside) within
        // `threshold` of it, otherwise the other one already shifted
        // things and the margin was already absorbed.
        let mut moved: Vec<ObjId> = shifted.iter().copied().collect();
        moved.extend(grown.iter().copied());
        let mut increased: HashSet<ObjId> = HashSet::new();
        for &m in &moved {
            let mo = &g.objects[m];
            let (mx, my, mw, mh) = (mo.top_left.x, mo.top_left.y, mo.width, mo.height);
            let mut counts = true;
            for &other in &moved {
                if other == m {
                    continue;
                }
                let oo = &g.objects[other];
                if is_horizontal {
                    if oo.top_left.y + oo.height < my || my + mh < oo.top_left.y {
                        continue;
                    }
                    if oo.top_left.x < mx && mx < oo.top_left.x + oo.width + THRESHOLD {
                        counts = false;
                        break;
                    }
                } else {
                    if oo.top_left.x + oo.width < mx || mx + mw < oo.top_left.x {
                        continue;
                    }
                    if oo.top_left.y < my && my < oo.top_left.y + oo.height + THRESHOLD {
                        counts = false;
                        break;
                    }
                }
            }
            if counts {
                increased.insert(m);
            }
        }
        increased
    }
}

/// Port of Go `adjustRankSpacing`. Walks rank-by-rank from the deepest rank
/// up, pulls each container's starting / ending ancestor positions outward
/// based on its non-rank-based padding, and shifts everything behind those
/// positions to create room, growing the stretched containers in the
/// process. Preserves Go's two-pass (ending then starting) order so the
/// containers' boxes match bytewise.
fn adjust_rank_spacing(
    g: &mut Graph,
    rank_sep: f64,
    is_horizontal: bool,
    excluded_objects: &HashSet<ObjId>,
) {
    let (ranks, object_ranks, starting_parent_ranks, ending_parent_ranks) =
        get_ranks(g, is_horizontal, excluded_objects);

    for rank in (0..ranks.len()).rev() {
        let mut starting_parents: Vec<ObjId> = Vec::new();
        let mut ending_parents: Vec<ObjId> = Vec::new();
        for &obj_id in &ranks[rank] {
            let parent = match g.objects[obj_id].parent {
                Some(p) if p != g.root => p,
                _ => continue,
            };
            if ending_parent_ranks.get(&parent).copied() == Some(rank) {
                ending_parents.push(parent);
            }
            if starting_parent_ranks.get(&parent).copied() == Some(rank) {
                starting_parents.push(parent);
            }
        }

        // --- compute starting ancestor positions -------------------------------
        let mut starting_positions: HashMap<ObjId, f64> = HashMap::new();
        while !starting_parents.is_empty() {
            let mut ancestors: Vec<ObjId> = Vec::new();
            for &parent in &starting_parents {
                let (_, padding) = g.objects[parent].spacing();
                let entry = starting_positions.entry(parent).or_insert(f64::INFINITY);
                let start_position = if is_horizontal {
                    let padding_increase = (padding.left - rank_sep / 2.0).max(0.0);
                    g.objects[parent].top_left.x - padding_increase
                } else {
                    let padding_increase = (padding.top - rank_sep / 2.0).max(0.0);
                    g.objects[parent].top_left.y - padding_increase
                };
                if start_position < *entry {
                    *entry = start_position;
                }
                let children: Vec<ObjId> = g.objects[parent].children_array.clone();
                for child in children {
                    if excluded_objects.contains(&child) {
                        continue;
                    }
                    let in_rank = match object_ranks.get(&child) {
                        Some(&r) => r == rank,
                        None => starting_parent_ranks.get(&child).copied() == Some(rank),
                    };
                    if !in_rank {
                        continue;
                    }
                    let (margin, _) = g.objects[child].spacing();
                    let child_start = if is_horizontal {
                        g.objects[child].top_left.x - margin.left - padding.left
                    } else {
                        g.objects[child].top_left.y - margin.top - padding.top
                    };
                    let entry = starting_positions.entry(parent).or_insert(f64::INFINITY);
                    if child_start < *entry {
                        *entry = child_start;
                    }
                }
                if let Some(pp) = g.objects[parent].parent
                    && pp != g.root
                {
                    ancestors.push(pp);
                }
            }
            starting_parents = ancestors;
        }

        // --- compute ending ancestor positions ---------------------------------
        let mut ending_positions: HashMap<ObjId, f64> = HashMap::new();
        while !ending_parents.is_empty() {
            let mut ancestors: Vec<ObjId> = Vec::new();
            for &parent in &ending_parents {
                let (_, padding) = g.objects[parent].spacing();
                let entry = ending_positions.entry(parent).or_insert(f64::NEG_INFINITY);
                let end_position = if is_horizontal {
                    g.objects[parent].top_left.x + g.objects[parent].width + padding.right
                        - rank_sep / 2.0
                } else {
                    g.objects[parent].top_left.y + g.objects[parent].height + padding.bottom
                        - rank_sep / 2.0
                };
                if end_position > *entry {
                    *entry = end_position;
                }
                let children: Vec<ObjId> = g.objects[parent].children_array.clone();
                for child in children {
                    if excluded_objects.contains(&child) {
                        continue;
                    }
                    let in_rank = match object_ranks.get(&child) {
                        Some(&r) => r == rank,
                        None => ending_parent_ranks.get(&child).copied() == Some(rank),
                    };
                    if !in_rank {
                        continue;
                    }
                    let (margin, _) = g.objects[child].spacing();
                    let child_end = if is_horizontal {
                        g.objects[child].top_left.x
                            + g.objects[child].width
                            + margin.right
                            + padding.right
                    } else {
                        g.objects[child].top_left.y
                            + g.objects[child].height
                            + margin.bottom
                            + padding.bottom
                    };
                    let entry = ending_positions.entry(parent).or_insert(f64::NEG_INFINITY);
                    if child_end > *entry {
                        *entry = child_end;
                    }
                }
                if let Some(pp) = g.objects[parent].parent
                    && pp != g.root
                {
                    ancestors.push(pp);
                }
            }
            ending_parents = ancestors;
        }

        // Adjust ending ancestors bottom-up (largest end first).
        let mut ending_order: Vec<ObjId> = ending_positions.keys().copied().collect();
        ending_order.sort_by(|a, b| {
            ending_positions[b]
                .partial_cmp(&ending_positions[a])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for ancestor in &ending_order {
            let (pos, end_delta) = if is_horizontal {
                let p = g.objects[*ancestor].top_left.x + g.objects[*ancestor].width;
                (p, ending_positions[ancestor] - p)
            } else {
                let p = g.objects[*ancestor].top_left.y + g.objects[*ancestor].height;
                (p, ending_positions[ancestor] - p)
            };
            if end_delta > 0.0 {
                for oi in 0..g.objects.len() {
                    if oi == g.root {
                        continue;
                    }
                    if excluded_objects.contains(&oi) {
                        continue;
                    }
                    if !g.objects[oi].is_container() {
                        continue;
                    }
                    let start = starting_parent_ranks.get(&oi).copied();
                    let end = ending_parent_ranks.get(&oi).copied();
                    if let (Some(s), Some(e)) = (start, end)
                        && s <= rank
                        && rank <= e
                    {
                        if is_horizontal && pos <= g.objects[oi].top_left.x + g.objects[oi].width {
                            g.objects[oi].width += end_delta;
                        } else if !is_horizontal
                            && pos <= g.objects[oi].top_left.y + g.objects[oi].height
                        {
                            g.objects[oi].height += end_delta;
                        }
                    }
                }
                shift_down(g, pos, end_delta, is_horizontal, excluded_objects);
            }
        }

        // Adjust starting ancestors top-down (smallest start first).
        let mut starting_order: Vec<ObjId> = starting_positions.keys().copied().collect();
        starting_order.sort_by(|a, b| {
            starting_positions[a]
                .partial_cmp(&starting_positions[b])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for ancestor in &starting_order {
            let (pos, start_delta) = if is_horizontal {
                let p = g.objects[*ancestor].top_left.x;
                (p, p - starting_positions[ancestor])
            } else {
                let p = g.objects[*ancestor].top_left.y;
                (p, p - starting_positions[ancestor])
            };
            if start_delta > 0.0 {
                for oi in 0..g.objects.len() {
                    if oi == g.root {
                        continue;
                    }
                    if excluded_objects.contains(&oi) {
                        continue;
                    }
                    if !g.objects[oi].is_container() {
                        continue;
                    }
                    let start = starting_parent_ranks.get(&oi).copied();
                    let end = ending_parent_ranks.get(&oi).copied();
                    if let (Some(s), Some(e)) = (start, end)
                        && s <= rank
                        && rank <= e
                    {
                        if is_horizontal && g.objects[oi].top_left.x <= pos {
                            g.objects[oi].width += start_delta;
                        } else if !is_horizontal && g.objects[oi].top_left.y <= pos {
                            g.objects[oi].height += start_delta;
                        }
                    }
                }
                shift_up(g, pos, start_delta, is_horizontal, excluded_objects);
            }
        }
    }
}

/// Port of Go `adjustCrossRankSpacing`. For each non-grid object, uses
/// `Spacing()` to reserve margin on the side facing the cross-axis, shifting
/// reachable shapes (via `shift_reachable_down`) to open up room. The
/// `prev_margin` maps record how much of each direction's margin has already
/// been handed out to an object so nested margins don't double-count.
fn adjust_cross_rank_spacing(
    g: &mut Graph,
    _rank_sep: f64,
    is_horizontal: bool,
    excluded_objects: &HashSet<ObjId>,
) {
    let mut prev_top: HashMap<ObjId, f64> = HashMap::new();
    let mut prev_bottom: HashMap<ObjId, f64> = HashMap::new();
    let mut prev_left: HashMap<ObjId, f64> = HashMap::new();
    let mut prev_right: HashMap<ObjId, f64> = HashMap::new();

    let obj_ids: Vec<ObjId> = (0..g.objects.len())
        .filter(|&i| {
            i != g.root
                && !excluded_objects.contains(&i)
                && g.objects[i].shape.value != "__d2_seq_nested_removed__"
                && g.objects[i].shape.value != "__d2_class_field_removed__"
        })
        .collect();
    for obj in obj_ids {
        if g.objects[obj].is_grid_diagram() {
            continue;
        }
        let (mut margin, padding) = g.objects[obj].spacing();
        if !is_horizontal {
            if let Some(&pm) = prev_bottom.get(&obj) {
                margin.bottom -= pm;
            }
            if margin.bottom > 0.0 {
                let start = g.objects[obj].top_left.y + g.objects[obj].height;
                let increased = shift_reachable_down(
                    g,
                    obj,
                    start,
                    margin.bottom,
                    is_horizontal,
                    true,
                    excluded_objects,
                );
                for o in increased {
                    let e = prev_bottom.entry(o).or_insert(0.0);
                    if margin.bottom > *e {
                        *e = margin.bottom;
                    }
                }
            }
            if padding.bottom > 0.0 {
                let start = g.objects[obj].top_left.y + g.objects[obj].height;
                shift_reachable_down(
                    g,
                    obj,
                    start,
                    padding.bottom,
                    is_horizontal,
                    false,
                    excluded_objects,
                );
                g.objects[obj].height += padding.bottom;
            }
            if let Some(&pm) = prev_top.get(&obj) {
                margin.top -= pm;
            }
            if margin.top > 0.0 {
                let start = g.objects[obj].top_left.y;
                let increased = shift_reachable_down(
                    g,
                    obj,
                    start,
                    margin.top,
                    is_horizontal,
                    true,
                    excluded_objects,
                );
                for o in increased {
                    let e = prev_top.entry(o).or_insert(0.0);
                    if margin.top > *e {
                        *e = margin.top;
                    }
                }
            }
            if padding.top > 0.0 {
                let start = g.objects[obj].top_left.y;
                shift_reachable_down(
                    g,
                    obj,
                    start,
                    padding.top,
                    is_horizontal,
                    false,
                    excluded_objects,
                );
                g.objects[obj].height += padding.top;
            }
        } else {
            if let Some(&pm) = prev_right.get(&obj) {
                margin.right -= pm;
            }
            if margin.right > 0.0 {
                let start = g.objects[obj].top_left.x + g.objects[obj].width;
                let increased = shift_reachable_down(
                    g,
                    obj,
                    start,
                    margin.right,
                    is_horizontal,
                    true,
                    excluded_objects,
                );
                for o in increased {
                    let e = prev_right.entry(o).or_insert(0.0);
                    if margin.right > *e {
                        *e = margin.right;
                    }
                }
            }
            if padding.right > 0.0 {
                let start = g.objects[obj].top_left.x + g.objects[obj].width;
                shift_reachable_down(
                    g,
                    obj,
                    start,
                    padding.right,
                    is_horizontal,
                    false,
                    excluded_objects,
                );
                g.objects[obj].width += padding.right;
            }
            if let Some(&pm) = prev_left.get(&obj) {
                margin.left -= pm;
            }
            if margin.left > 0.0 {
                let start = g.objects[obj].top_left.x;
                let increased = shift_reachable_down(
                    g,
                    obj,
                    start,
                    margin.left,
                    is_horizontal,
                    true,
                    excluded_objects,
                );
                for o in increased {
                    let e = prev_left.entry(o).or_insert(0.0);
                    if margin.left > *e {
                        *e = margin.left;
                    }
                }
            }
            if padding.left > 0.0 {
                let start = g.objects[obj].top_left.x;
                shift_reachable_down(
                    g,
                    obj,
                    start,
                    padding.left,
                    is_horizontal,
                    false,
                    excluded_objects,
                );
                g.objects[obj].width += padding.left;
            }
        }
    }

    // Update box_ after all shifting.
    for i in 0..g.objects.len() {
        if i == g.root {
            continue;
        }
        if excluded_objects.contains(&i) {
            continue;
        }
        g.objects[i].update_box();
    }
    let _ = Spacing {
        top: 0.0,
        bottom: 0.0,
        left: 0.0,
        right: 0.0,
    }; // silence unused-import in minimal builds
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Object};

    /// Simple 2-node graph layout test.
    #[test]
    fn layout_two_nodes() {
        let mut g = Graph::new();
        let a = g.add_object(Object {
            id: "a".into(),
            abs_id: "a".into(),
            width: 100.0,
            height: 50.0,
            ..Default::default()
        });
        let b = g.add_object(Object {
            id: "b".into(),
            abs_id: "b".into(),
            width: 100.0,
            height: 50.0,
            ..Default::default()
        });
        g.add_edge(Edge {
            abs_id: "(a -> b)[0]".into(),
            src: a,
            dst: b,
            ..Default::default()
        });

        let result = layout(&mut g, None);
        assert!(result.is_ok(), "layout failed: {:?}", result.err());

        // After layout, objects should have positions
        let obj_a = &g.objects[a];
        let obj_b = &g.objects[b];

        // Both should have non-zero or at least assigned positions
        // In TB mode, b should be below a
        assert!(
            obj_b.top_left.y > obj_a.top_left.y,
            "b ({}) should be below a ({})",
            obj_b.top_left.y,
            obj_a.top_left.y
        );

        // Edge should have route points
        assert!(
            !g.edges[0].route.is_empty(),
            "edge should have route points"
        );
    }

    /// Test with configurable opts.
    #[test]
    fn layout_with_custom_opts() {
        let mut g = Graph::new();
        let a = g.add_object(Object {
            id: "a".into(),
            abs_id: "a".into(),
            width: 80.0,
            height: 40.0,
            ..Default::default()
        });
        let b = g.add_object(Object {
            id: "b".into(),
            abs_id: "b".into(),
            width: 80.0,
            height: 40.0,
            ..Default::default()
        });
        g.add_edge(Edge {
            abs_id: "(a -> b)[0]".into(),
            src: a,
            dst: b,
            ..Default::default()
        });

        let opts = ConfigurableOpts {
            node_sep: 100,
            edge_sep: 40,
        };
        let result = layout(&mut g, Some(&opts));
        assert!(result.is_ok());
    }

    /// Test with parent-child (container) relationships.
    #[test]
    fn layout_with_container() {
        let mut g = Graph::new();
        let parent = g.add_object(Object {
            id: "container".into(),
            abs_id: "container".into(),
            width: 200.0,
            height: 150.0,
            ..Default::default()
        });
        let child = g.add_object(Object {
            id: "child".into(),
            abs_id: "container.child".into(),
            parent: Some(parent),
            width: 80.0,
            height: 40.0,
            ..Default::default()
        });
        let other = g.add_object(Object {
            id: "other".into(),
            abs_id: "other".into(),
            width: 80.0,
            height: 40.0,
            ..Default::default()
        });
        g.add_edge(Edge {
            abs_id: "(child -> other)[0]".into(),
            src: child,
            dst: other,
            ..Default::default()
        });

        let result = layout(&mut g, None);
        assert!(result.is_ok(), "layout failed: {:?}", result.err());
    }

    /// Test horizontal layout direction.
    #[test]
    fn layout_horizontal() {
        let mut g = Graph::new();
        g.root_obj_mut().direction.value = "right".to_owned();
        let a = g.add_object(Object {
            id: "a".into(),
            abs_id: "a".into(),
            width: 100.0,
            height: 50.0,
            ..Default::default()
        });
        let b = g.add_object(Object {
            id: "b".into(),
            abs_id: "b".into(),
            width: 100.0,
            height: 50.0,
            ..Default::default()
        });
        g.add_edge(Edge {
            abs_id: "(a -> b)[0]".into(),
            src: a,
            dst: b,
            ..Default::default()
        });

        let result = layout(&mut g, None);
        assert!(result.is_ok());

        // In LR mode, b should be to the right of a
        let obj_a = &g.objects[a];
        let obj_b = &g.objects[b];
        assert!(
            obj_b.top_left.x > obj_a.top_left.x,
            "b ({}) should be right of a ({})",
            obj_b.top_left.x,
            obj_a.top_left.x
        );
    }

    /// Test layout with an edge that has labels.
    #[test]
    fn layout_edge_with_label() {
        let mut g = Graph::new();
        let a = g.add_object(Object {
            id: "a".into(),
            abs_id: "a".into(),
            width: 100.0,
            height: 50.0,
            ..Default::default()
        });
        let b = g.add_object(Object {
            id: "b".into(),
            abs_id: "b".into(),
            width: 100.0,
            height: 50.0,
            ..Default::default()
        });
        g.add_edge(Edge {
            abs_id: "(a -> b)[0]".into(),
            src: a,
            dst: b,
            label: crate::graph::Label {
                value: "connects".into(),
                map_key: None,
            },
            label_dimensions: crate::graph::Dimensions {
                width: 60,
                height: 16,
            },
            ..Default::default()
        });

        let result = layout(&mut g, None);
        assert!(result.is_ok());

        // Label position should be set
        assert_eq!(
            g.edges[0].label_position.as_deref(),
            Some("INSIDE_MIDDLE_CENTER")
        );
    }

    #[test]
    fn build_constant_near_subgraph_keeps_external_edges_out_of_main_layout() {
        let mut g = Graph::new();
        let a = g.add_object(Object {
            id: "a".into(),
            abs_id: "a".into(),
            width: 53.0,
            height: 66.0,
            ..Default::default()
        });
        let b = g.add_object(Object {
            id: "b".into(),
            abs_id: "b".into(),
            width: 53.0,
            height: 66.0,
            near_key: Some("center-left".into()),
            ..Default::default()
        });
        g.add_edge(Edge {
            abs_id: "(b -> a)[0]".into(),
            src: b,
            dst: a,
            ..Default::default()
        });

        let (subgraphs, excluded_objects, excluded_edges) = build_constant_near_subgraphs(&g);
        assert_eq!(subgraphs.len(), 1);
        assert!(excluded_objects.contains(&b));
        assert!(excluded_edges.contains(&0));
        assert_eq!(subgraphs[0].external_edge_indices, vec![0]);
    }

    #[test]
    fn layout_constant_near_with_external_edge_places_object_left_of_main_graph() {
        let mut g = Graph::new();
        let a = g.add_object(Object {
            id: "a".into(),
            abs_id: "a".into(),
            width: 53.0,
            height: 66.0,
            ..Default::default()
        });
        let b = g.add_object(Object {
            id: "b".into(),
            abs_id: "b".into(),
            width: 53.0,
            height: 66.0,
            near_key: Some("center-left".into()),
            ..Default::default()
        });
        g.add_edge(Edge {
            abs_id: "(b -> a)[0]".into(),
            src: b,
            dst: a,
            ..Default::default()
        });

        layout(&mut g, None).expect("layout");
        assert!(g.objects[b].top_left.x < g.objects[a].top_left.x);
    }
}
