//! d2-sequence: sequence diagram layout engine.
//!
//! Ported from Go `d2layouts/d2sequence/`.

use std::collections::{HashMap, HashSet};

use crate::geo::Point;
use crate::graph::{Edge, Graph, ObjId, Object, ScalarValue, Style};

// ---------------------------------------------------------------------------
// Constants (from Go d2sequence/constants.go)
// ---------------------------------------------------------------------------

const HORIZONTAL_PAD: f64 = 40.0;
const LABEL_HORIZONTAL_PAD: f64 = 60.0;
const VERTICAL_PAD: f64 = 40.0;
const MIN_ACTOR_DISTANCE: f64 = 150.0;
const MIN_ACTOR_WIDTH: f64 = 100.0;
const SELF_MESSAGE_HORIZONTAL_TRAVEL: f64 = 80.0;
const GROUP_CONTAINER_PADDING: f64 = 12.0;
#[allow(dead_code)]
const EDGE_GROUP_LABEL_PADDING: f64 = 20.0;
const MIN_MESSAGE_DISTANCE: f64 = 30.0;
#[allow(dead_code)]
const SPAN_BASE_WIDTH: f64 = 12.0;
#[allow(dead_code)]
const SPAN_DEPTH_GROWTH_FACTOR: f64 = 8.0;
#[allow(dead_code)]
const MIN_SPAN_HEIGHT: f64 = 30.0;
#[allow(dead_code)]
const SPAN_MESSAGE_PAD: f64 = 10.0;
const LIFELINE_STROKE_WIDTH: i32 = 2;
const LIFELINE_STROKE_DASH: i32 = 6;
const LIFELINE_LABEL_PAD: f64 = 5.0;

const LIFELINE_Z_INDEX: i32 = 1;
#[allow(dead_code)]
const SPAN_Z_INDEX: i32 = 2;
#[allow(dead_code)]
const GROUP_Z_INDEX: i32 = 3;
const MESSAGE_Z_INDEX: i32 = 4;
#[allow(dead_code)]
const NOTE_Z_INDEX: i32 = 5;

// ---------------------------------------------------------------------------
// SequenceDiagram
// ---------------------------------------------------------------------------

struct SequenceDiagram {
    root: ObjId,
    messages: Vec<usize>, // edge indices
    lifelines: Vec<Edge>, // synthesized lifeline edges
    actors: Vec<ObjId>,
    #[allow(dead_code)]
    groups: Vec<ObjId>,
    spans: Vec<ObjId>,
    notes: Vec<ObjId>,

    /// rank: left-to-right position of actors/spans
    object_rank: HashMap<ObjId, usize>,

    /// first and last message of each actor/span
    first_message: HashMap<ObjId, usize>,
    last_message: HashMap<ObjId, usize>,

    /// distance from actor[i] center to actor[i+1] center
    actor_x_step: Vec<f64>,

    y_step: f64,
    max_actor_height: f64,

    /// vertical ordering by source line number
    vertical_indices: HashMap<String, usize>,

    /// Objects that have been positioned (have valid top_left).
    placed: HashSet<ObjId>,
}

// ---------------------------------------------------------------------------
// Helpers for source ordering
// ---------------------------------------------------------------------------

/// Get earliest source line number from object references.
fn get_obj_earliest_line_num(obj: &Object) -> usize {
    let mut min = usize::MAX;
    for r in &obj.references {
        min = min.min(r.key.range.start.line);
    }
    min
}

/// Get earliest source line number from edge first_ast_range.
fn get_edge_earliest_line_num(edge: &Edge) -> usize {
    edge.first_ast_range
        .as_ref()
        .map(|r| r.start.line)
        .unwrap_or(usize::MAX)
}

// ---------------------------------------------------------------------------
// Constructor
// ---------------------------------------------------------------------------

fn new_sequence_diagram(
    g: &mut Graph,
    root: ObjId,
    objects: &[ObjId],
    message_indices: &[usize],
) -> Result<SequenceDiagram, String> {
    // Sort objects by earliest line number
    let mut sorted_objects: Vec<ObjId> = objects.to_vec();
    sorted_objects.sort_by_key(|&id| get_obj_earliest_line_num(&g.objects[id]));

    // Sort messages by earliest line number
    let mut sorted_messages: Vec<usize> = message_indices.to_vec();
    sorted_messages.sort_by_key(|&idx| get_edge_earliest_line_num(&g.edges[idx]));

    // Separate actors from groups. Mirrors Go newSequenceDiagram:
    // groups are objects not connected by any edge, whose children all
    // contain edges (are not notes).
    let mut actors: Vec<ObjId> = Vec::new();
    let mut groups: Vec<ObjId> = Vec::new();

    for &obj_id in &sorted_objects {
        if is_sequence_diagram_group(g, obj_id, &sorted_messages) {
            // Groups may contain nested groups — flatten them all
            let mut queue = vec![obj_id];
            while let Some(curr) = queue.first().copied() {
                queue.remove(0);
                g.objects[curr].label_position =
                    Some(crate::label::Position::InsideTopLeft.to_string());
                g.objects[curr].is_sequence_diagram_group = true;
                groups.push(curr);
                let children = g.objects[curr].children_array.clone();
                queue.extend(children);
            }
        } else {
            actors.push(obj_id);
        }
    }

    if actors.is_empty() {
        return Err("no actors declared in sequence diagram".to_string());
    }

    let mut sd = SequenceDiagram {
        root,
        messages: sorted_messages.clone(),
        lifelines: Vec::new(),
        actors: actors.clone(),
        groups,
        spans: Vec::new(),
        notes: Vec::new(),
        object_rank: HashMap::new(),
        first_message: HashMap::new(),
        last_message: HashMap::new(),
        actor_x_step: vec![0.0; actors.len().saturating_sub(1)],
        y_step: MIN_MESSAGE_DISTANCE,
        max_actor_height: 0.0,
        vertical_indices: HashMap::new(),
        placed: HashSet::new(),
    };

    for (rank, &actor_id) in actors.iter().enumerate() {
        sd.object_rank.insert(actor_id, rank);

        // Enforce minimum actor width (Go: MIN_ACTOR_WIDTH = 100)
        {
            let actor = &mut g.objects[actor_id];
            if actor.width < MIN_ACTOR_WIDTH {
                let dsl_shape = actor.shape.value.to_lowercase();
                match dsl_shape.as_str() {
                    crate::target::SHAPE_PERSON | "oval" | "square" | "circle" => {
                        // Scale shape up to min width uniformly
                        actor.height *= MIN_ACTOR_WIDTH / actor.width;
                    }
                    _ => {}
                }
                actor.width = MIN_ACTOR_WIDTH;
            }
        }

        {
            let actor = &g.objects[actor_id];
            sd.max_actor_height = sd.max_actor_height.max(actor.height);
        }

        // Process children: find notes and spans
        let mut queue: Vec<ObjId> = g.objects[actor_id].children_array.clone();
        let mut max_note_width: f64 = 0.0;

        while let Some(child_id) = queue.first().copied() {
            queue.remove(0);

            // Gather data from immutable borrow before mutating
            let has_edge_ref = child_has_edge_ref(g, child_id);
            let has_children = !g.objects[child_id].children_array.is_empty();
            let child_children = g.objects[child_id].children_array.clone();
            let child_width = g.objects[child_id].width;

            if !has_edge_ref && !has_children {
                // It's a note — matches Go behavior:
                //   child.Shape = PAGE_TYPE
                //   child.LabelPosition = INSIDE_MIDDLE_CENTER
                let note = &mut g.objects[child_id];
                note.shape = ScalarValue {
                    value: crate::target::SHAPE_PAGE.to_owned(),
                };
                note.label_position = Some("INSIDE_MIDDLE_CENTER".to_string());
                sd.vertical_indices
                    .insert(note.abs_id.clone(), get_obj_earliest_line_num(note));
                sd.notes.push(child_id);
                sd.object_rank.insert(child_id, rank);
                max_note_width = max_note_width.max(child_width);
            } else {
                // It's a span — matches Go behavior:
                //   child.Label = ""
                //   child.Shape = SQUARE_TYPE
                let span = &mut g.objects[child_id];
                span.label = crate::graph::Label {
                    value: String::new(),
                    map_key: span.label.map_key.take(),
                };
                span.shape = ScalarValue {
                    value: "square".to_owned(),
                };
                sd.spans.push(child_id);
                sd.object_rank.insert(child_id, rank);
            }

            queue.extend_from_slice(&child_children);
        }

        if rank < actors.len() - 1 {
            let actor_hw = g.objects[actor_id].width / 2.0;
            let next_actor = &g.objects[actors[rank + 1]];
            let next_actor_hw = next_actor.width / 2.0;
            sd.actor_x_step[rank] =
                (actor_hw + next_actor_hw + HORIZONTAL_PAD).max(MIN_ACTOR_DISTANCE);
            sd.actor_x_step[rank] =
                (max_note_width / 2.0 + HORIZONTAL_PAD).max(sd.actor_x_step[rank]);
            if rank > 0 {
                sd.actor_x_step[rank - 1] =
                    (max_note_width / 2.0 + HORIZONTAL_PAD).max(sd.actor_x_step[rank - 1]);
            }
        }
    }

    // Process messages for spacing
    for &msg_idx in &sd.messages {
        let message = &g.edges[msg_idx];
        sd.vertical_indices
            .insert(message.abs_id.clone(), get_edge_earliest_line_num(message));

        let src_rank = sd.object_rank.get(&message.src).copied().unwrap_or(0);
        let dst_rank = sd.object_rank.get(&message.dst).copied().unwrap_or(0);
        let rank_diff = (src_rank as f64 - dst_rank as f64).abs();

        if rank_diff != 0.0 {
            let distributed_label_width = message.label_dimensions.width as f64 / rank_diff;
            let min_rank = src_rank.min(dst_rank);
            let max_rank = src_rank.max(dst_rank);
            for rank in min_rank..max_rank {
                if rank < sd.actor_x_step.len() {
                    sd.actor_x_step[rank] =
                        sd.actor_x_step[rank].max(distributed_label_width + LABEL_HORIZONTAL_PAD);
                }
            }
        } else {
            // Self-message
            let next_rank = src_rank;
            if next_rank < sd.actor_x_step.len() {
                let label_adjust =
                    message.label_dimensions.width as f64 + crate::label::PADDING * 4.0;
                sd.actor_x_step[next_rank] = sd.actor_x_step[next_rank].max(label_adjust);
            }
        }

        sd.last_message.insert(message.src, msg_idx);
        sd.first_message.entry(message.src).or_insert(msg_idx);
        sd.last_message.insert(message.dst, msg_idx);
        sd.first_message.entry(message.dst).or_insert(msg_idx);
    }

    sd.y_step += VERTICAL_PAD;
    sd.max_actor_height += VERTICAL_PAD;
    let root_obj = &g.objects[root];
    if root_obj.has_label() {
        sd.max_actor_height += root_obj.label_dimensions.height as f64;
    }

    Ok(sd)
}

/// Check if child has any edge referencing it (src or dst).
fn child_has_edge_ref(g: &Graph, child_id: ObjId) -> bool {
    for edge in &g.edges {
        if edge.src == child_id || edge.dst == child_id {
            return true;
        }
    }
    false
}

/// Check if an object is a sequence diagram group.
/// Groups are objects inside a sequence diagram that:
/// 1. Have no edges directly connecting to them (not an actor endpoint)
/// 2. All children either contain edges or are sub-groups (no note-like children)
/// 3. Contain objects or edges within their scope
///
/// Mirrors Go `Object.IsSequenceDiagramGroup()`.
fn is_sequence_diagram_group(g: &Graph, obj_id: ObjId, message_indices: &[usize]) -> bool {
    // Match Go for direct endpoints, plus treat direct-child endpoints as
    // actors so cases like `b.1 -> b.1` still classify `b` as an actor.
    for &idx in message_indices {
        let edge = &g.edges[idx];
        if edge.src == obj_id
            || edge.dst == obj_id
            || g.objects[edge.src].parent == Some(obj_id)
            || g.objects[edge.dst].parent == Some(obj_id)
        {
            return false;
        }
    }
    // All children must contain edges (not be note-like)
    let obj = &g.objects[obj_id];
    for &child_id in &obj.children_array {
        if !obj_contains_any_edge(g, child_id, message_indices) {
            return false;
        }
    }
    // Must contain some objects or edges
    obj_contains_any_object(g, obj_id) || obj_contains_any_edge(g, obj_id, message_indices)
}

/// Check if obj contains any other object (has descendant objects within its scope).
/// Mirrors Go `Object.ContainsAnyObject` which uses `Object.ContainedBy` →
/// walks Reference.ScopeObj up the parent chain.
fn obj_contains_any_object(g: &Graph, obj_id: ObjId) -> bool {
    for (i, o) in g.objects.iter().enumerate() {
        if i == obj_id {
            continue;
        }
        // Check references: if any reference's scope_obj walks up to obj_id
        for r in &o.references {
            if let Some(scope) = r.scope_obj {
                let mut cur = scope;
                loop {
                    if cur == obj_id {
                        return true;
                    }
                    match g.objects[cur].parent {
                        Some(p) => cur = p,
                        None => break,
                    }
                }
            }
        }
    }
    false
}

/// Check if obj contains any edge (has edges scoped within it).
/// Uses edge.scope_obj to match Go's Reference-based containment.
fn obj_contains_any_edge(g: &Graph, obj_id: ObjId, message_indices: &[usize]) -> bool {
    for &idx in message_indices {
        if edge_contained_by(g, idx, obj_id) {
            return true;
        }
    }
    false
}

/// Check if an edge is contained by (scoped within) an object.
/// Mirrors Go `Edge.ContainedBy(obj)` which walks Reference.ScopeObj up
/// the parent chain. We use edge.scope_obj for the same purpose.
fn edge_contained_by(g: &Graph, edge_idx: usize, obj_id: ObjId) -> bool {
    let edge = &g.edges[edge_idx];
    if let Some(scope) = edge.scope_obj {
        let mut cur = scope;
        loop {
            if cur == obj_id {
                return true;
            }
            match g.objects[cur].parent {
                Some(p) => cur = p,
                None => return false,
            }
        }
    }
    false
}

/// Check if an object is "contained by" another via Reference.scope_obj.
/// Mirrors Go `Object.ContainedBy(obj)` which walks ScopeObj up the parent chain.
fn obj_contained_by_scope(g: &Graph, obj_id: ObjId, container_id: ObjId) -> bool {
    for r in &g.objects[obj_id].references {
        if let Some(scope) = r.scope_obj {
            let mut cur = scope;
            loop {
                if cur == container_id {
                    return true;
                }
                match g.objects[cur].parent {
                    Some(p) => cur = p,
                    None => break,
                }
            }
        }
    }
    false
}

/// Check if child_id is a descendant of parent_id.
#[allow(dead_code)]
fn obj_is_descendant_of(g: &Graph, child_id: ObjId, parent_id: ObjId) -> bool {
    let mut cur = child_id;
    while let Some(p) = g.objects[cur].parent {
        if p == parent_id {
            return true;
        }
        cur = p;
    }
    false
}

// ---------------------------------------------------------------------------
// Layout methods
// ---------------------------------------------------------------------------

impl SequenceDiagram {
    fn layout(&mut self, g: &mut Graph) -> Result<(), String> {
        self.place_actors(g);
        self.place_notes(g);
        self.route_messages(g)?;
        self.place_spans(g);
        self.adjust_route_endpoints(g);
        self.place_groups(g);
        self.add_lifeline_edges(g);
        Ok(())
    }

    /// Place actors bottom-aligned, side by side with centers spaced by actor_x_step.
    fn place_actors(&mut self, g: &mut Graph) {
        let mut center_x = g.objects[self.actors[0]].width / 2.0;

        for (rank, &actor_id) in self.actors.iter().enumerate() {
            let actor = &g.objects[actor_id];
            let has_outside_bottom = actor.has_outside_bottom_label();
            let has_label = actor.has_label();
            let actor_height = actor.height;
            let actor_width = actor.width;
            let label_height = actor.label_dimensions.height as f64;
            let has_icon = actor.icon.is_some();
            let shape_val = actor.shape.value.clone();
            let icon_position = actor.icon_position.clone();

            let y_offset;
            if has_outside_bottom {
                // Set label position
                let actor = &mut g.objects[actor_id];
                if icon_position.is_none() {
                    actor.label_position = Some("OUTSIDE_BOTTOM_CENTER".to_string());
                }
                y_offset = self.max_actor_height
                    - actor_height
                    - if has_label { label_height } else { 0.0 };
            } else {
                let actor = &mut g.objects[actor_id];
                if has_icon && shape_val != crate::target::SHAPE_IMAGE {
                    if actor.label_position.is_none() {
                        actor.label_position = Some("OUTSIDE_TOP_CENTER".to_string());
                    }
                    if actor.icon_position.is_none() {
                        actor.icon_position = Some("INSIDE_MIDDLE_CENTER".to_string());
                    }
                } else if actor.icon_position.is_none() {
                    actor.label_position = Some("INSIDE_MIDDLE_CENTER".to_string());
                }
                y_offset = self.max_actor_height - actor_height;
            }

            let half_width = actor_width / 2.0;
            let actor = &mut g.objects[actor_id];
            actor.top_left = Point::new((center_x - half_width).round(), y_offset);
            actor.box_ = crate::geo::Box2D::new(actor.top_left, actor.width, actor.height);
            self.placed.insert(actor_id);

            if rank < self.actors.len() - 1 {
                center_x += self.actor_x_step[rank];
            }
        }
    }

    /// Place notes vertically based on their position relative to messages.
    fn place_notes(&self, g: &mut Graph) {
        let mut rank_to_x: HashMap<usize, f64> = HashMap::new();
        for &actor_id in &self.actors {
            let actor = &g.objects[actor_id];
            let center_x = actor.top_left.x + actor.width / 2.0;
            rank_to_x.insert(self.object_rank[&actor_id], center_x);
        }

        for &note_id in &self.notes {
            let note = &g.objects[note_id];
            let vertical_index = self.vertical_indices[&note.abs_id];
            let mut y = self.max_actor_height + self.y_step;

            for &msg_idx in &self.messages {
                let msg = &g.edges[msg_idx];
                if self.vertical_indices[&msg.abs_id] < vertical_index {
                    if msg.src == msg.dst {
                        y += self.y_step
                            + (msg.label_dimensions.height as f64).max(MIN_MESSAGE_DISTANCE) * 1.5;
                    } else {
                        y += self.y_step + msg.label_dimensions.height as f64;
                    }
                }
            }
            for &other_note_id in &self.notes {
                let other_note = &g.objects[other_note_id];
                if self
                    .vertical_indices
                    .get(&other_note.abs_id)
                    .copied()
                    .unwrap_or(0)
                    < vertical_index
                {
                    y += other_note.height + self.y_step;
                }
            }

            let rank = self.object_rank[&note_id];
            let x = rank_to_x[&rank] - (note.width / 2.0);

            let note = &mut g.objects[note_id];
            note.top_left = Point::new(x, y);
            note.box_ = crate::geo::Box2D::new(note.top_left, note.width, note.height);
            note.z_index = NOTE_Z_INDEX;
            note.shape = ScalarValue {
                value: crate::target::SHAPE_PAGE.to_string(),
            };
            note.label_position = Some("INSIDE_MIDDLE_CENTER".to_string());
            note.is_sequence_diagram_note = true;
        }
    }

    /// Route messages as horizontal edges from src to dst lifeline.
    fn route_messages(&mut self, g: &mut Graph) -> Result<(), String> {
        let mut message_offset = self.max_actor_height + self.y_step;

        for &msg_idx in &self.messages.clone() {
            g.edges[msg_idx].z_index = MESSAGE_Z_INDEX;

            let msg_abs_id = g.edges[msg_idx].abs_id.clone();
            let msg_vi = self.vertical_indices[&msg_abs_id];

            // Calculate note offset
            let mut note_offset: f64 = 0.0;
            for &note_id in &self.notes {
                let note = &g.objects[note_id];
                if self
                    .vertical_indices
                    .get(&note.abs_id)
                    .copied()
                    .unwrap_or(0)
                    < msg_vi
                {
                    note_offset += note.height + self.y_step;
                }
            }

            let src_id = g.edges[msg_idx].src;
            let dst_id = g.edges[msg_idx].dst;

            let start_x = match get_center_x_with_placed(g, src_id, &self.placed) {
                Some(x) => x,
                None => {
                    return Err(format!(
                        "could not find center of {}. Is it declared as an actor?",
                        g.objects[src_id].id
                    ));
                }
            };
            let end_x = match get_center_x_with_placed(g, dst_id, &self.placed) {
                Some(x) => x,
                None => {
                    return Err(format!(
                        "could not find center of {}. Is it declared as an actor?",
                        g.objects[dst_id].id
                    ));
                }
            };

            let is_self_message = src_id == dst_id;
            let is_to_descendant = g.objects[dst_id]
                .abs_id
                .starts_with(&format!("{}.", g.objects[src_id].abs_id));
            let is_from_descendant = g.objects[src_id]
                .abs_id
                .starts_with(&format!("{}.", g.objects[dst_id].abs_id));

            // Check if src and dst share the same top-level actor
            let is_to_sibling = {
                let mut curr_src = src_id;
                while g.objects[curr_src].parent != Some(self.root)
                    && g.objects[curr_src].parent.is_some()
                {
                    curr_src = g.objects[curr_src].parent.unwrap();
                }
                let mut curr_dst = dst_id;
                while g.objects[curr_dst].parent != Some(self.root)
                    && g.objects[curr_dst].parent.is_some()
                {
                    curr_dst = g.objects[curr_dst].parent.unwrap();
                }
                curr_src == curr_dst
            };

            let label_w = g.edges[msg_idx].label_dimensions.width as f64;
            let label_h = g.edges[msg_idx].label_dimensions.height as f64;
            // Go does integer division: `message.LabelDimensions.Height/2`
            // then converts to float64. We must match to avoid 0.5px drift.
            let label_h_half = (g.edges[msg_idx].label_dimensions.height / 2) as f64;

            if is_self_message || is_to_descendant || is_from_descendant || is_to_sibling {
                let mid_x = start_x
                    + SELF_MESSAGE_HORIZONTAL_TRAVEL
                        .max(label_w / 2.0 + crate::label::PADDING * 2.0);
                let start_y = message_offset + note_offset;
                let end_y = start_y + label_h.max(MIN_MESSAGE_DISTANCE) * 1.5;
                g.edges[msg_idx].route = vec![
                    Point::new(start_x, start_y),
                    Point::new(mid_x, start_y),
                    Point::new(mid_x, end_y),
                    Point::new(end_x, end_y),
                ];
                message_offset = end_y + self.y_step - note_offset;
            } else {
                let start_y = message_offset + note_offset + label_h_half;
                g.edges[msg_idx].route =
                    vec![Point::new(start_x, start_y), Point::new(end_x, start_y)];
                message_offset = start_y + label_h_half + self.y_step - note_offset;
            }

            if !g.edges[msg_idx].label.value.is_empty() {
                g.edges[msg_idx].label_position = Some("INSIDE_MIDDLE_CENTER".to_string());
            }
        }
        Ok(())
    }

    /// Place spans over the object lifeline.
    fn place_spans(&self, g: &mut Graph) {
        let mut rank_to_x: HashMap<usize, f64> = HashMap::new();
        for &actor_id in &self.actors {
            let actor = &g.objects[actor_id];
            let center_x = actor.top_left.x + actor.width / 2.0;
            rank_to_x.insert(self.object_rank[&actor_id], center_x);
        }

        // Sort spans from most to least nested (deepest first)
        let mut spans_sorted: Vec<ObjId> = self.spans.clone();
        spans_sorted.sort_by(|&a, &b| {
            let level_a = obj_level(g, a);
            let level_b = obj_level(g, b);
            level_b.cmp(&level_a)
        });

        for &span_id in &spans_sorted {
            // Find position based on children
            let mut min_child_y = f64::INFINITY;
            let mut max_child_y = f64::NEG_INFINITY;
            for &child_id in &g.objects[span_id].children_array.clone() {
                let child = &g.objects[child_id];
                min_child_y = min_child_y.min(child.top_left.y);
                max_child_y = max_child_y.max(child.top_left.y + child.height);
            }

            // Find position from messages
            let mut min_message_y = f64::INFINITY;
            if let Some(&first_msg) = self.first_message.get(&span_id) {
                let msg = &g.edges[first_msg];
                if msg.src == msg.dst || span_id == msg.src {
                    if let Some(p) = msg.route.first() {
                        min_message_y = p.y;
                    }
                } else if let Some(p) = msg.route.last() {
                    min_message_y = p.y;
                }
            }
            let mut max_message_y = f64::NEG_INFINITY;
            if let Some(&last_msg) = self.last_message.get(&span_id) {
                let msg = &g.edges[last_msg];
                if msg.src == msg.dst || span_id == msg.dst {
                    if let Some(p) = msg.route.last() {
                        max_message_y = p.y;
                    }
                } else if let Some(p) = msg.route.first() {
                    max_message_y = p.y;
                }
            }

            let mut min_y = min_message_y.min(min_child_y);
            if min_y == min_child_y || min_y == min_message_y {
                min_y -= SPAN_MESSAGE_PAD;
            }
            let mut max_y = max_message_y.max(max_child_y);
            if max_y == max_child_y || max_y == max_message_y {
                max_y += SPAN_MESSAGE_PAD;
            }

            let height = (max_y - min_y).max(MIN_SPAN_HEIGHT);
            let root_level = obj_level(g, self.root);
            let span_level = obj_level(g, span_id);
            // -1 because the actors count as 1 level
            let width = SPAN_BASE_WIDTH
                + ((span_level as f64 - root_level as f64 - 2.0) * SPAN_DEPTH_GROWTH_FACTOR);
            let rank = self.object_rank[&span_id];
            let x = rank_to_x[&rank] - (width / 2.0);

            let span = &mut g.objects[span_id];
            span.top_left = Point::new(x, min_y);
            span.width = width;
            span.height = height;
            span.box_ = crate::geo::Box2D::new(span.top_left, width, height);
            span.z_index = SPAN_Z_INDEX;
            span.label = crate::graph::Label {
                value: String::new(),
                map_key: None,
            };
            span.shape = ScalarValue {
                value: "rectangle".to_string(),
            };
        }
    }

    /// Adjust route endpoints for span widths.
    fn adjust_route_endpoints(&self, g: &mut Graph) {
        for &msg_idx in &self.messages {
            let src_id = g.edges[msg_idx].src;
            let dst_id = g.edges[msg_idx].dst;

            let src_is_actor = g.objects[src_id].parent == Some(self.root);
            let dst_is_actor = g.objects[dst_id].parent == Some(self.root);

            let src_rank = self.object_rank.get(&src_id).copied().unwrap_or(0);
            let dst_rank = self.object_rank.get(&dst_id).copied().unwrap_or(0);
            let src_width = g.objects[src_id].width;
            let dst_width = g.objects[dst_id].width;

            let route_len = g.edges[msg_idx].route.len();
            if route_len == 0 {
                continue;
            }

            if !src_is_actor {
                if src_rank <= dst_rank {
                    g.edges[msg_idx].route[0].x += src_width / 2.0;
                } else {
                    g.edges[msg_idx].route[0].x -= src_width / 2.0;
                }
            }
            if !dst_is_actor {
                if src_rank < dst_rank {
                    g.edges[msg_idx].route[route_len - 1].x -= dst_width / 2.0;
                } else {
                    g.edges[msg_idx].route[route_len - 1].x += dst_width / 2.0;
                }
            }
        }
    }

    /// Place groups as bounding boxes around messages they contain.
    fn place_groups(&self, g: &mut Graph) {
        // Sort groups from most to least nested (deepest first)
        let mut sorted_groups = self.groups.clone();
        sorted_groups.sort_by(|&a, &b| {
            let la = obj_level(g, a);
            let lb = obj_level(g, b);
            lb.cmp(&la) // deepest first
        });

        for &group_id in &sorted_groups {
            g.objects[group_id].z_index = GROUP_Z_INDEX;
            self.place_group(g, group_id);
        }

        // Adjust group labels
        for &group_id in &sorted_groups {
            self.adjust_group_label(g, group_id);
        }
    }

    fn place_group(&self, g: &mut Graph, group_id: ObjId) {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        // Check messages contained by this group
        for &msg_idx in &self.messages {
            if edge_contained_by(g, msg_idx, group_id) {
                for p in &g.edges[msg_idx].route {
                    let label_height = g.edges[msg_idx].label_dimensions.height as f64 / 2.0;
                    let edge_pad =
                        (label_height + GROUP_CONTAINER_PADDING).max(MIN_MESSAGE_DISTANCE / 2.0);
                    min_x = min_x.min(p.x - HORIZONTAL_PAD);
                    min_y = min_y.min(p.y - edge_pad);
                    max_x = max_x.max(p.x + HORIZONTAL_PAD);
                    max_y = max_y.max(p.y + edge_pad);
                }
            }
        }

        // Groups should encompass notes of actors within the group.
        // In Go, this checks n.References[].ScopeObj walks up to group.
        for &note_id in &self.notes {
            if obj_contained_by_scope(g, note_id, group_id) {
                let note = &g.objects[note_id];
                min_x = min_x.min(note.top_left.x - HORIZONTAL_PAD);
                min_y = min_y.min(note.top_left.y - MIN_MESSAGE_DISTANCE / 2.0);
                max_x = max_x.max(note.top_left.x + note.width + HORIZONTAL_PAD);
                max_y = max_y.max(note.top_left.y + note.height + MIN_MESSAGE_DISTANCE / 2.0);
            }
        }

        // Encompass child groups
        let children = g.objects[group_id].children_array.clone();
        for &child_id in &children {
            if self.groups.contains(&child_id) {
                let ch = &g.objects[child_id];
                min_x = min_x.min(ch.top_left.x - GROUP_CONTAINER_PADDING);
                min_y = min_y.min(ch.top_left.y - GROUP_CONTAINER_PADDING);
                max_x = max_x.max(ch.top_left.x + ch.width + GROUP_CONTAINER_PADDING);
                max_y = max_y.max(ch.top_left.y + ch.height + GROUP_CONTAINER_PADDING);
            }
        }

        if min_x.is_finite() && max_x.is_finite() {
            let group = &mut g.objects[group_id];
            group.top_left = Point::new(min_x, min_y);
            group.width = max_x - min_x;
            group.height = max_y - min_y;
            group.box_ = crate::geo::Box2D::new(group.top_left, group.width, group.height);
        }
    }

    fn adjust_group_label(&self, g: &mut Graph, group_id: ObjId) {
        if !g.objects[group_id].has_label() {
            return;
        }

        let height_add =
            g.objects[group_id].label_dimensions.height as f64 + EDGE_GROUP_LABEL_PADDING / 2.0;
        if height_add < GROUP_CONTAINER_PADDING {
            return;
        }

        let group_top_y = g.objects[group_id].top_left.y;
        g.objects[group_id].height += height_add;

        // Extend stuff within this group
        for &gid in &self.groups {
            let g_obj = &g.objects[gid];
            if g_obj.top_left.y < group_top_y && g_obj.top_left.y + g_obj.height > group_top_y {
                g.objects[gid].height += height_add;
            }
        }
        for &sid in &self.spans {
            let s_obj = &g.objects[sid];
            if s_obj.top_left.y < group_top_y && s_obj.top_left.y + s_obj.height > group_top_y {
                g.objects[sid].height += height_add;
            }
        }

        // Move stuff down that's below this group
        for &msg_idx in &self.messages {
            let route_y = g.edges[msg_idx]
                .route
                .first()
                .map(|p| p.y)
                .unwrap_or(0.0)
                .min(g.edges[msg_idx].route.last().map(|p| p.y).unwrap_or(0.0));
            if route_y > group_top_y {
                for p in &mut g.edges[msg_idx].route {
                    p.y += height_add;
                }
            }
        }
        for &sid in &self.spans {
            if g.objects[sid].top_left.y > group_top_y {
                g.objects[sid].top_left.y += height_add;
            }
        }
        for &gid in &self.groups {
            if g.objects[gid].top_left.y > group_top_y {
                g.objects[gid].top_left.y += height_add;
            }
        }
        for &nid in &self.notes {
            if g.objects[nid].top_left.y > group_top_y {
                g.objects[nid].top_left.y += height_add;
            }
        }
    }

    /// Add lifeline edges for each actor.
    fn add_lifeline_edges(&mut self, g: &mut Graph) {
        let mut end_y: f64 = 0.0;

        // Find the bottom of all messages
        for &msg_idx in &self.messages {
            for p in &g.edges[msg_idx].route {
                end_y = end_y.max(p.y);
            }
        }
        // Consider notes
        for &note_id in &self.notes {
            let note = &g.objects[note_id];
            end_y = end_y.max(note.top_left.y + note.height);
        }
        // Consider actors
        for &actor_id in &self.actors {
            let actor = &g.objects[actor_id];
            end_y = end_y.max(actor.top_left.y + actor.height);
        }
        end_y += self.y_step;

        for &actor_id in &self.actors {
            let actor = &g.objects[actor_id];
            let center_x = actor.top_left.x + actor.width / 2.0;
            let mut actor_bottom_y = actor.top_left.y + actor.height;

            if actor.label_position.as_deref() == Some("OUTSIDE_BOTTOM_CENTER") && actor.has_label()
            {
                actor_bottom_y += actor.label_dimensions.height as f64 + LIFELINE_LABEL_PAD;
            }

            let mut stroke_dash = ScalarValue {
                value: format!("{}", LIFELINE_STROKE_DASH),
            };
            let stroke_width = ScalarValue {
                value: format!("{}", LIFELINE_STROKE_WIDTH),
            };
            let mut stroke: Option<ScalarValue> = None;

            if let Some(ref sd) = actor.style.stroke_dash {
                stroke_dash = sd.clone();
            }
            if let Some(ref s) = actor.style.stroke {
                stroke = Some(s.clone());
            }

            // Synthetic lifeline-end ID matching Go format:
            // actor.ID + "-lifeline-end-" + StringToIntHash(actor.ID + "-lifeline-end")
            let lifeline_end_id = format!(
                "{}-lifeline-end-{}",
                actor.id,
                string_to_int_hash(&format!("{}-lifeline-end", actor.id))
            );

            self.lifelines.push(Edge {
                abs_id: format!("({} -- )[0]", actor.abs_id),
                src: actor_id,
                dst: 0, // placeholder - lifeline end is synthetic
                dst_id_override: Some(lifeline_end_id),
                src_arrow: false,
                dst_arrow: false,
                route: vec![
                    Point::new(center_x, actor_bottom_y),
                    Point::new(center_x, end_y),
                ],
                style: Style {
                    stroke_dash: Some(stroke_dash),
                    stroke_width: Some(stroke_width),
                    stroke,
                    ..Default::default()
                },
                z_index: LIFELINE_Z_INDEX,
                label: crate::graph::Label {
                    value: String::new(),
                    map_key: None,
                },
                ..Default::default()
            });
        }
    }

    fn get_width(&self, g: &Graph) -> f64 {
        let last_actor_id = *self.actors.last().unwrap();
        let last_actor = &g.objects[last_actor_id];
        let mut rightmost = last_actor.top_left.x + last_actor.width;

        for &msg_idx in &self.messages {
            for p in &g.edges[msg_idx].route {
                rightmost = rightmost.max(p.x);
            }
            // Self-referential messages may have labels that extend further
            let msg = &g.edges[msg_idx];
            if msg.src == msg.dst && msg.route.len() > 1 {
                rightmost = rightmost.max(msg.route[1].x + msg.label_dimensions.width as f64 / 2.0);
            }
        }

        rightmost
    }

    fn get_height(&self) -> f64 {
        if self.lifelines.is_empty() {
            0.0
        } else {
            self.lifelines[0].route[1].y
        }
    }

    fn shift(&mut self, g: &mut Graph, dx: f64, dy: f64) {
        // Shift actors
        for &actor_id in &self.actors {
            let obj = &mut g.objects[actor_id];
            obj.top_left.x += dx;
            obj.top_left.y += dy;
            obj.box_ = crate::geo::Box2D::new(obj.top_left, obj.width, obj.height);
        }
        // Shift spans
        for &span_id in &self.spans {
            let obj = &mut g.objects[span_id];
            obj.top_left.x += dx;
            obj.top_left.y += dy;
            obj.box_ = crate::geo::Box2D::new(obj.top_left, obj.width, obj.height);
        }
        // Shift groups
        for &group_id in &self.groups {
            let obj = &mut g.objects[group_id];
            obj.top_left.x += dx;
            obj.top_left.y += dy;
            obj.box_ = crate::geo::Box2D::new(obj.top_left, obj.width, obj.height);
        }
        // Shift notes
        for &note_id in &self.notes {
            let obj = &mut g.objects[note_id];
            obj.top_left.x += dx;
            obj.top_left.y += dy;
            obj.box_ = crate::geo::Box2D::new(obj.top_left, obj.width, obj.height);
        }
        // Shift messages
        for &msg_idx in &self.messages {
            for p in &mut g.edges[msg_idx].route {
                p.x += dx;
                p.y += dy;
            }
        }
        // Shift lifelines
        for lifeline in &mut self.lifelines {
            for p in &mut lifeline.route {
                p.x += dx;
                p.y += dy;
            }
        }
    }
}

/// Compute object nesting level (root = 0, children of root = 1, etc.)
fn obj_level(g: &Graph, obj_id: ObjId) -> usize {
    let mut level = 0;
    let mut cur = obj_id;
    while let Some(parent) = g.objects[cur].parent {
        level += 1;
        cur = parent;
    }
    level
}

/// Get center X of an object, walking up to parent if not yet placed.
/// In Go, objects have Box.TopLeft = nil until placed. In Rust, we check
/// if the object has been explicitly placed by the sequence layout.
/// Placed objects: actors (which have been positioned by place_actors).
/// Unplaced: spans and notes (before place_spans/place_notes).
fn get_center_x_with_placed(
    g: &Graph,
    obj_id: ObjId,
    placed: &std::collections::HashSet<ObjId>,
) -> Option<f64> {
    if placed.contains(&obj_id) {
        let obj = &g.objects[obj_id];
        Some(obj.top_left.x + obj.width / 2.0)
    } else if let Some(parent) = g.objects[obj_id].parent {
        get_center_x_with_placed(g, parent, placed)
    } else {
        None
    }
}

/// FNV-1a 32-bit hash matching Go's `go2.StringToIntHash` (which uses
/// `hash/fnv.New32a`).
fn string_to_int_hash(s: &str) -> u32 {
    const OFFSET_BASIS: u32 = 2166136261;
    const PRIME: u32 = 16777619;
    let mut h: u32 = OFFSET_BASIS;
    for &b in s.as_bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(PRIME);
    }
    h
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Layout a sequence diagram. Called when the root object has shape=sequence_diagram.
///
/// Mirrors Go `d2sequence.Layout`.
pub fn layout(g: &mut Graph) -> Result<(), String> {
    let root = g.root;

    // Collect top-level children (the actors)
    let children: Vec<ObjId> = g.objects[root].children_array.clone();

    // Collect edges inside the sequence diagram
    let root_abs = g.objects[root].abs_id.clone();
    let message_indices: Vec<usize> = (0..g.edges.len())
        .filter(|&i| {
            let edge = &g.edges[i];
            if root == g.root {
                true // Root-level sequence diagram: all edges
            } else {
                let src_abs = &g.objects[edge.src].abs_id;
                let dst_abs = &g.objects[edge.dst].abs_id;
                src_abs.starts_with(&format!("{}.", root_abs))
                    && dst_abs.starts_with(&format!("{}.", root_abs))
            }
        })
        .collect();

    let mut sd = new_sequence_diagram(g, root, &children, &message_indices)?;
    sd.layout(g)?;

    let width = sd.get_width(g) + GROUP_CONTAINER_PADDING * 2.0;
    let height = sd.get_height() + GROUP_CONTAINER_PADDING * 2.0;

    // Set root box
    g.objects[root].top_left = Point::new(0.0, 0.0);
    g.objects[root].width = width;
    g.objects[root].height = height;
    g.objects[root].box_ = crate::geo::Box2D::new(Point::new(0.0, 0.0), width, height);
    g.objects[root].label_position = Some("INSIDE_TOP_CENTER".to_string());

    // Shift everything by GROUP_CONTAINER_PADDING
    sd.shift(g, GROUP_CONTAINER_PADDING, GROUP_CONTAINER_PADDING);

    // Rebuild root's children to be just actors (+ groups at root level)
    // This matches Go's behavior of resetting obj.Children and obj.ChildrenArray
    let mut new_children: Vec<ObjId> = Vec::new();
    for &actor_id in &sd.actors {
        new_children.push(actor_id);
    }
    g.objects[root].children_array = new_children;

    // Add lifeline edges to the graph
    for lifeline in sd.lifelines {
        g.edges.push(lifeline);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(input: &str) -> crate::graph::Graph {
        crate::compiler::compile("test.d2", input).expect("compile")
    }

    #[test]
    fn root_sequence_span_cover_has_actors() {
        let mut g = compile("shape: sequence_diagram\nb.1 -> b.1\nb.1 -> b.1\n");
        let root = g.root;
        let children = g.objects[root].children_array.clone();
        let message_indices: Vec<usize> = (0..g.edges.len()).collect();

        assert_eq!(children.len(), 1);
        let b = children[0];
        assert_eq!(g.objects[b].abs_id, "b");
        assert!(!is_sequence_diagram_group(&g, b, &message_indices));
        assert!(new_sequence_diagram(&mut g, root, &children, &message_indices).is_ok());
    }

    #[test]
    fn root_sequence_nested_group_without_actors_errors() {
        let mut g = compile("shape: sequence_diagram\ngroup.nested: {\n  a -> b\n}\n");
        let err = layout(&mut g).expect_err("expected missing-actor error");
        assert_eq!(err, "no actors declared in sequence diagram");
    }

    #[test]
    fn root_sequence_missing_actor_center_errors() {
        let mut g = compile(
            "\nshape: sequence_diagram\n\na\n\ngroup: {\n  inner_group: {\n    a -> b\n  }\n}\n",
        );
        let err = layout(&mut g).expect_err("expected missing center error");
        assert_eq!(
            err,
            "could not find center of b. Is it declared as an actor?"
        );
    }
}
