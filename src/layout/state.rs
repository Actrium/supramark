//! State diagram layout engine.
//!
//! Converts a `StateDiagram` into a fully positioned `StateLayout` ready for
//! SVG rendering.  Uses Graphviz (dot) for layout via the svek pipeline,
//! matching Java PlantUML behaviour.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::layout::graphviz::{
    self, LayoutClusterSpec, LayoutEdge, LayoutGraph, LayoutNode, RankDir,
};
use crate::model::state::{State, StateDiagram, StateKind, Transition};
use crate::Result;

// ---------------------------------------------------------------------------
// Layout output types
// ---------------------------------------------------------------------------

/// Fully positioned state diagram ready for rendering.
#[derive(Debug)]
pub struct StateLayout {
    pub width: f64,
    pub height: f64,
    pub state_layouts: Vec<StateNodeLayout>,
    pub transition_layouts: Vec<TransitionLayout>,
    pub note_layouts: Vec<StateNoteLayout>,
    /// Svek moveDelta (dx, dy) for viewport calculation.
    pub move_delta: (f64, f64),
    /// LimitFinder span (w, h) for viewport calculation.
    pub lf_span: (f64, f64),
}

/// A single positioned state node.
#[derive(Debug, Clone)]
pub struct StateNodeLayout {
    pub id: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub description: Vec<String>,
    pub stereotype: Option<String>,
    pub is_initial: bool,
    pub is_final: bool,
    pub is_composite: bool,
    pub children: Vec<StateNodeLayout>,
    /// Pseudo-state kind (fork, join, choice, history, etc.)
    pub kind: StateKind,
    /// Internal transitions resolved by the inner graphviz solve.
    /// These are stored relative to the inner origin (0,0) and offset
    /// to absolute coordinates when the composite is positioned.
    pub internal_transitions: Vec<TransitionLayout>,
    /// Y positions of concurrent region separators (dashed lines)
    pub region_separators: Vec<f64>,
    /// Indices into `children` where each concurrent region (after the first)
    /// starts. Empty for non-concurrent composites. For a composite with two
    /// regions [r0, r1], `children` is r0 followed by r1 and this vector
    /// holds [len(r0)] so `children[..region_child_starts[0]]` is region 0.
    pub region_child_starts: Vec<usize>,
    /// Inner transitions split per concurrent region. Empty when not
    /// concurrent. For two regions, holds [region0_transitions, region1_transitions].
    pub region_inner_transitions: Vec<Vec<TransitionLayout>>,
    /// Source line (0-based) for data-source-line attribute.
    pub source_line: Option<usize>,
    /// True when this composite came from a Graphviz cluster solve rather than
    /// from the legacy two-level standalone composite layout.
    pub render_as_cluster: bool,
}

/// A transition edge between two states.
#[derive(Debug, Clone)]
pub struct TransitionLayout {
    pub from_id: String,
    pub to_id: String,
    pub label: String,
    pub points: Vec<(f64, f64)>,
    /// Raw SVG path d-string from Graphviz (Bezier curves). When set, the
    /// renderer should use this instead of building M/L segments from `points`.
    pub raw_path_d: Option<String>,
    /// Arrowhead polygon points from Graphviz SVG.
    pub arrow_polygon: Option<Vec<(f64, f64)>>,
    /// Label position (x, y) from Graphviz edge label placement.
    pub label_xy: Option<(f64, f64)>,
    /// Label block dimension (width, height) for LimitFinder-style empty tracking.
    pub label_wh: Option<(f64, f64)>,
    /// Source line (0-based) for data-source-line attribute.
    pub source_line: Option<usize>,
    /// True for transitions from inner graphviz solve (non-cluster composite).
    /// Rendered inline after their parent composite, matching Java order.
    pub is_inner: bool,
}

/// A positioned note.
#[derive(Debug, Clone)]
pub struct StateNoteLayout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub text: String,
    pub position: String,
    pub target: Option<String>,
    pub entity_id: Option<String>,
    pub source_line: Option<usize>,
    pub anchor: Option<(f64, f64)>,
    /// Original helper-edge endpoints used by Java's Opale linked-note shape.
    pub opale_points: Option<((f64, f64), (f64, f64))>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

#[allow(dead_code)] // Java-ported layout constant
const CHAR_WIDTH: f64 = 7.2;
#[allow(dead_code)] // Java-ported layout constant
const LINE_HEIGHT: f64 = 16.0;
const PADDING: f64 = 10.0;
/// Java: state name uses FontParam.STATE = SansSerif 14pt.
const STATE_NAME_FONT_SIZE: f64 = 14.0;
/// Java: state body/description uses FontParam.STATE_ATTRIBUTE = SansSerif 12pt.
const STATE_DESC_FONT_SIZE: f64 = 12.0;
/// Minimum state dimensions matching Java PlantUML defaults.
const STATE_MIN_WIDTH: f64 = 50.0;
const STATE_MIN_HEIGHT: f64 = 50.0;
/// Graphviz default ranksep (0.5 inches = 36pt) — used for inner composite layouts
/// where Java's inner graphviz runs with default parameters (no ranksep= directive).
const INNER_RANKSEP: f64 = 36.0;
/// Graphviz default nodesep (0.25 inches = 18pt) — used for inner composite layouts.
const INNER_NODESEP: f64 = 18.0;
const SPECIAL_STATE_RADIUS: f64 = 10.0;
const FINAL_STATE_DIAMETER: f64 = 22.0;
/// Java: IEntityImage.MARGIN (padding around inner content).
const IE_MARGIN: f64 = 5.0;
/// Java: IEntityImage.MARGIN_LINE (line/section separator padding).
const IE_MARGIN_LINE: f64 = 5.0;
/// Padding inside composite states around children (horizontal).
const COMPOSITE_PADDING: f64 = 12.0;
const NOTE_OFFSET: f64 = 30.0;
const NOTE_FONT_SIZE: f64 = 13.0;
const FORK_BAR_WIDTH: f64 = 80.0;
const FORK_BAR_HEIGHT: f64 = 8.0;
/// Choice diamond side length.
const CHOICE_SIZE: f64 = 24.0;
const HISTORY_DIAMETER: f64 = 22.0;
/// Java: <<inputPin>>/<<outputPin>> entity images are 12x12 rectangles.
const PIN_SIZE: f64 = 12.0;
/// Height bonus per pin type (inputPin/outputPin) for composite states.
/// Java uses {rank=source}/{rank=sink} cluster constraints which add
/// approximately ranksep/2 + cluster_margin + pin_size of vertical space
/// per pin type. Empirically ~40px matches Java's cluster layout.
const PIN_RANK_HEIGHT_BONUS: f64 = 45.0;
const MARGIN: f64 = 7.0;

// ---------------------------------------------------------------------------
// Composite state dimension helpers (Java: InnerStateAutonom)
// ---------------------------------------------------------------------------

/// Title text height for a composite state name.
/// Java: title.calculateDimension(sb).getHeight() — the line height of the
/// state name font (SansSerif 14pt, ascent+descent).
fn composite_title_height() -> f64 {
    crate::font_metrics::line_height("SansSerif", STATE_NAME_FONT_SIZE, false, false)
}

/// Header height for a composite state: the y-offset of the separator line.
/// Java: `titreHeight = IEntityImage.MARGIN + text.getHeight() + IEntityImage.MARGIN_LINE`
fn composite_header_height() -> f64 {
    IE_MARGIN + composite_title_height() + IE_MARGIN_LINE
}

/// Y-offset where inner children start within a composite state.
/// Java: `getSpaceYforURL() = titreHeight + marginForFields + attr.height + MARGIN_LINE`
/// (simplified for no attributes: `titreHeight + MARGIN_LINE`)
fn composite_inner_y_offset() -> f64 {
    composite_header_height() + IE_MARGIN_LINE
}

/// Total height overhead for a composite state (total - inner_children_height).
/// Java: `calculateDimensionSlow().height - inner.height = title_h + 2*MARGIN + 2*MARGIN_LINE`
fn composite_height_overhead() -> f64 {
    composite_title_height() + 2.0 * IE_MARGIN + 2.0 * IE_MARGIN_LINE
}

/// Compute the inner moveDelta margin for a composite state.
///
/// Java: `moveDelta.y = 6 - LF_min_y`.  The LimitFinder applies a -1
/// correction for rectangles (`drawRectangle`) but not for circles
/// (`drawEllipse`).  If the topmost child (minimum y after graphviz
/// positioning) is a rect, LF_min_y = -1 → margin = 7.  If the topmost
/// child is a circle (initial/final/choice), LF_min_y = 0 → margin = 6.
fn composite_inner_margin(children: &[StateNodeLayout]) -> f64 {
    fn is_circle(c: &StateNodeLayout) -> bool {
        c.is_initial
            || c.is_final
            || matches!(
                c.kind,
                StateKind::Choice
                    | StateKind::History
                    | StateKind::DeepHistory
                    | StateKind::EntryPoint
                    | StateKind::ExitPoint
                    | StateKind::End
            )
    }
    let topmost = children
        .iter()
        .min_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
    match topmost {
        Some(c) if is_circle(c) => 6.0,
        _ => MARGIN, // 7.0
    }
}

fn collect_state_descendant_ids(state: &State, out: &mut HashSet<String>) {
    for child in &state.children {
        out.insert(child.id.clone());
        collect_state_descendant_ids(child, out);
    }
    for region in &state.regions {
        for child in region {
            out.insert(child.id.clone());
            collect_state_descendant_ids(child, out);
        }
    }
}

fn state_has_direct_pin_child(state: &State) -> bool {
    state.children.iter().any(|child| {
        matches!(
            child.stereotype.as_deref(),
            Some("inputPin") | Some("outputPin")
        )
    })
}

fn qualifies_for_inner_pin_cluster(state: &State, transitions: &[Transition]) -> bool {
    let is_composite = !state.children.is_empty() || !state.regions.is_empty();
    if !is_composite || !state.regions.is_empty() || !state_has_direct_pin_child(state) {
        return false;
    }

    // Java inner state solves still export child groups with port nodes as real
    // DOT clusters even when links cross the child-group boundary through those
    // descendants. The only case we still cannot model in the inner solve is a
    // transition whose endpoint is the composite group ID itself, because that
    // needs the cluster special-point routing used by the outer solve.
    !transitions
        .iter()
        .any(|tr| tr.from == state.id || tr.to == state.id)
}

// ---------------------------------------------------------------------------
// Text measurement helpers
// ---------------------------------------------------------------------------

/// Compute the pixel width of a single line of text using font metrics.
/// Handles `\t` (literal backslash-t from PlantUML preprocessing) with
/// Java-compatible tab-stop expansion (default tabSize=8 spaces).
/// See AtomText.java getWidth() and tabString().
fn text_width(text: &str, font_size: f64) -> f64 {
    // Java's default tabSize=8, tabString() returns 8 spaces
    let tab_pixel_size =
        crate::font_metrics::text_width("        ", "SansSerif", font_size, false, false);
    let mut x = 0.0;
    // Split on literal "\t" sequences (PlantUML preprocessor output)
    let mut rest = text;
    while let Some(pos) = rest.find("\\t") {
        if pos > 0 {
            x +=
                crate::font_metrics::text_width(&rest[..pos], "SansSerif", font_size, false, false);
        }
        // Tab-stop snap: advance to next multiple of tab_pixel_size
        let remainder = x % tab_pixel_size;
        x += tab_pixel_size - remainder;
        rest = &rest[pos + 2..];
    }
    if !rest.is_empty() {
        x += crate::font_metrics::text_width(rest, "SansSerif", font_size, false, false);
    }
    x
}

/// Compute the visual width of a description line, accounting for `**bold**` markup.
/// This matches Java which measures text width after creole parsing.
fn desc_line_visual_width(line: &str, font_size: f64) -> f64 {
    if !line.contains("**") {
        return text_width(line, font_size);
    }
    let mut cx = 0.0;
    let mut is_bold = false;
    for part in line.split("**") {
        if part.is_empty() {
            is_bold = !is_bold;
            continue;
        }
        cx += crate::font_metrics::text_width(part, "SansSerif", font_size, is_bold, false);
        is_bold = !is_bold;
    }
    cx
}

/// Estimate the size of a simple (non-composite, non-special) state.
/// Returns `(width, height)`.
///
/// Matches Java PlantUML sizing: simple state is 50x50 minimum,
/// header area is ~26px, description lines add ~14px each.
fn estimate_state_size(state: &State) -> (f64, f64) {
    let name_w = text_width(&state.name, STATE_NAME_FONT_SIZE) + 2.0 * PADDING;

    // Expand \n within descriptions to visual lines (matching render)
    let visual_lines = expand_visual_lines(&state.description);

    let desc_w = visual_lines
        .iter()
        .map(|line| desc_line_visual_width(line, STATE_DESC_FONT_SIZE) + 2.0 * PADDING)
        .fold(0.0_f64, f64::max);

    let stereo_w = state
        .stereotype
        .as_ref()
        .map_or(0.0, |s| text_width(s, STATE_NAME_FONT_SIZE) + 2.0 * PADDING);

    let width = name_w.max(desc_w).max(stereo_w).max(STATE_MIN_WIDTH);

    // Header (name at 14pt) + optional stereotype + description (at 12pt).
    // Java: EntityImageState layout uses different fonts for name vs body.
    let name_h = crate::font_metrics::line_height("SansSerif", STATE_NAME_FONT_SIZE, false, false);
    let desc_h = crate::font_metrics::line_height("SansSerif", STATE_DESC_FONT_SIZE, false, false);
    let stereo_h = if state.stereotype.is_some() {
        desc_h
    } else {
        0.0
    };
    let desc_total = visual_lines.len() as f64 * desc_h;
    let height = (name_h + stereo_h + desc_total + 2.0 * PADDING).max(STATE_MIN_HEIGHT);

    (width, height)
}

/// Expand description lines by splitting on literal `\n` sequences.
/// Each `\n` produces an additional visual line (empty string for spacing).
fn expand_visual_lines(descriptions: &[String]) -> Vec<String> {
    let mut lines = Vec::new();
    for desc in descriptions {
        let mut start = 0;
        let b = desc.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'\\' && i + 1 < b.len() && b[i + 1] == b'n' {
                lines.push(desc[start..i].to_string());
                start = i + 2;
                i += 2;
            } else {
                i += 1;
            }
        }
        lines.push(desc[start..].to_string());
    }
    lines
}

/// Estimate the size of a note from its text content.
fn estimate_note_size(text: &str) -> (f64, f64) {
    let lines: Vec<&str> = text.lines().collect();
    let note_line_height =
        crate::font_metrics::line_height("SansSerif", NOTE_FONT_SIZE, false, false);
    let max_line_width = lines
        .iter()
        .map(|line| {
            crate::font_metrics::text_width(line, "SansSerif", NOTE_FONT_SIZE, false, false)
        })
        .fold(0.0_f64, f64::max);
    let width = (max_line_width + 21.0).max(60.0);
    let height = (lines.len().max(1) as f64 * note_line_height + 10.0).max(25.1328);
    (width, height)
}

// ---------------------------------------------------------------------------
// Determine initial / final status
// ---------------------------------------------------------------------------

/// Determine which `[*]` state IDs serve as initial and which serve as final.
///
/// A `[*]` state is **initial** if it appears as the `from` of a transition.
/// A `[*]` state is **final** if it appears as the `to` of a transition.
fn classify_special_states(
    states: &[State],
    transitions: &[Transition],
) -> (HashSet<String>, HashSet<String>) {
    fn collect_special_ids(states: &[State], out: &mut HashSet<String>) {
        for state in states {
            if state.is_special {
                out.insert(state.id.clone());
            }
            collect_special_ids(&state.children, out);
            for region in &state.regions {
                collect_special_ids(region, out);
            }
        }
    }

    let mut special_ids = HashSet::new();
    collect_special_ids(states, &mut special_ids);

    let mut initial_ids = HashSet::new();
    let mut final_ids = HashSet::new();

    for tr in transitions {
        if special_ids.contains(&tr.from) {
            initial_ids.insert(tr.from.clone());
        }
        if special_ids.contains(&tr.to) {
            final_ids.insert(tr.to.clone());
        }
    }

    // If a special state has no transitions at all, default to initial
    for id in &special_ids {
        if !initial_ids.contains(id) && !final_ids.contains(id) {
            initial_ids.insert(id.clone());
        }
    }

    (initial_ids, final_ids)
}

// ---------------------------------------------------------------------------
// Collect implicit states
// ---------------------------------------------------------------------------

/// Collect state IDs that are referenced in transitions but not declared in the
/// state list.  These need synthesized layout entries.
fn collect_implicit_states(states: &[State], transitions: &[Transition]) -> Vec<State> {
    let mut declared: HashSet<String> = HashSet::new();
    collect_declared_ids(states, &mut declared);

    let mut implicit = Vec::new();
    let mut seen = HashSet::new();

    for tr in transitions {
        for id in [&tr.from, &tr.to] {
            if !declared.contains(id.as_str()) && seen.insert(id.clone()) {
                let is_special = id.starts_with("[*]");
                let kind = if id.ends_with("[H*]") {
                    StateKind::DeepHistory
                } else if id.ends_with("[H]") {
                    StateKind::History
                } else {
                    StateKind::default()
                };
                implicit.push(State {
                    name: id.clone(),
                    id: id.clone(),
                    description: Vec::new(),
                    stereotype: None,
                    children: Vec::new(),
                    is_special,
                    kind,
                    regions: Vec::new(),
                    source_line: None,
                    explicit_source_line: None,
                });
            }
        }
    }

    implicit
}

/// Deduplicate states by ID, preferring composite states over simple ones.
/// When two states have the same ID, the one with children (composite) wins.
/// Descriptions and stereotypes are merged.
fn dedup_states(states: &mut Vec<State>) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut to_remove: Vec<usize> = Vec::new();

    for i in 0..states.len() {
        if let Some(&prev_idx) = seen.get(&states[i].id) {
            let prev_is_composite =
                !states[prev_idx].children.is_empty() || !states[prev_idx].regions.is_empty();
            let curr_is_composite = !states[i].children.is_empty() || !states[i].regions.is_empty();

            if curr_is_composite && !prev_is_composite {
                // Current is composite, previous is simple -> remove previous
                to_remove.push(prev_idx);
                seen.insert(states[i].id.clone(), i);
            } else {
                // Previous is composite or both are simple -> remove current
                to_remove.push(i);
            }
        } else {
            seen.insert(states[i].id.clone(), i);
        }
    }

    // Remove duplicates in reverse order to preserve indices
    to_remove.sort_unstable();
    to_remove.dedup();
    for &idx in to_remove.iter().rev() {
        states.remove(idx);
    }
}

/// Recursively collect all declared state IDs (including regions).
fn collect_declared_ids(states: &[State], ids: &mut HashSet<String>) {
    for s in states {
        ids.insert(s.id.clone());
        collect_declared_ids(&s.children, ids);
        for region in &s.regions {
            collect_declared_ids(region, ids);
        }
    }
}

// ---------------------------------------------------------------------------
// Core layout logic
// ---------------------------------------------------------------------------

/// Compute the layout node for a single state (sizing, children layout, etc.)
/// without assigning position. Returns (node, width, height).
fn compute_state_node(
    state: &State,
    transitions: &[Transition],
    initial_ids: &HashSet<String>,
    final_ids: &HashSet<String>,
) -> (StateNodeLayout, f64, f64) {
    let is_initial = initial_ids.contains(&state.id);
    let is_final = final_ids.contains(&state.id);
    let is_composite = !state.children.is_empty() || !state.regions.is_empty();

    if state.is_special {
        let diameter = if is_final {
            FINAL_STATE_DIAMETER
        } else {
            2.0 * SPECIAL_STATE_RADIUS
        };
        return (
            StateNodeLayout {
                id: state.id.clone(),
                name: state.name.clone(),
                x: 0.0,
                y: 0.0,
                width: diameter,
                height: diameter,
                description: Vec::new(),
                stereotype: None,
                is_initial,
                is_final,
                is_composite: false,
                children: Vec::new(),
                kind: state.kind.clone(),
                internal_transitions: Vec::new(),
                region_separators: Vec::new(),
                region_child_starts: Vec::new(),
                region_inner_transitions: Vec::new(),
                source_line: state.source_line,
                render_as_cluster: false,
            },
            diameter,
            diameter,
        );
    }

    if matches!(state.kind, StateKind::Fork | StateKind::Join) {
        let w = FORK_BAR_WIDTH;
        let h = FORK_BAR_HEIGHT;
        return (
            StateNodeLayout {
                id: state.id.clone(),
                name: state.name.clone(),
                x: 0.0,
                y: 0.0,
                width: w,
                height: h,
                description: Vec::new(),
                stereotype: state.stereotype.clone(),
                is_initial: false,
                is_final: false,
                is_composite: false,
                children: Vec::new(),
                kind: state.kind.clone(),
                internal_transitions: Vec::new(),
                region_separators: Vec::new(),
                region_child_starts: Vec::new(),
                region_inner_transitions: Vec::new(),
                source_line: state.source_line,
                render_as_cluster: false,
            },
            w,
            h,
        );
    }

    if state.kind == StateKind::Choice {
        let s = CHOICE_SIZE;
        return (
            StateNodeLayout {
                id: state.id.clone(),
                name: state.name.clone(),
                x: 0.0,
                y: 0.0,
                width: s,
                height: s,
                description: Vec::new(),
                stereotype: state.stereotype.clone(),
                is_initial: false,
                is_final: false,
                is_composite: false,
                children: Vec::new(),
                kind: state.kind.clone(),
                internal_transitions: Vec::new(),
                region_separators: Vec::new(),
                region_child_starts: Vec::new(),
                region_inner_transitions: Vec::new(),
                source_line: state.source_line,
                render_as_cluster: false,
            },
            s,
            s,
        );
    }

    if matches!(state.kind, StateKind::History | StateKind::DeepHistory) {
        let d = HISTORY_DIAMETER;
        return (
            StateNodeLayout {
                id: state.id.clone(),
                name: state.name.clone(),
                x: 0.0,
                y: 0.0,
                width: d,
                height: d,
                description: Vec::new(),
                stereotype: state.stereotype.clone(),
                is_initial: false,
                is_final: false,
                is_composite: false,
                children: Vec::new(),
                kind: state.kind.clone(),
                internal_transitions: Vec::new(),
                region_separators: Vec::new(),
                region_child_starts: Vec::new(),
                region_inner_transitions: Vec::new(),
                source_line: state.source_line,
                render_as_cluster: false,
            },
            d,
            d,
        );
    }

    if state.kind == StateKind::End {
        let diameter = FINAL_STATE_DIAMETER;
        return (
            StateNodeLayout {
                id: state.id.clone(),
                name: state.name.clone(),
                x: 0.0,
                y: 0.0,
                width: diameter,
                height: diameter,
                description: Vec::new(),
                stereotype: state.stereotype.clone(),
                is_initial: false,
                is_final: true,
                is_composite: false,
                children: Vec::new(),
                kind: state.kind.clone(),
                internal_transitions: Vec::new(),
                region_separators: Vec::new(),
                region_child_starts: Vec::new(),
                region_inner_transitions: Vec::new(),
                source_line: state.source_line,
                render_as_cluster: false,
            },
            diameter,
            diameter,
        );
    }

    if is_composite {
        // Composite state: recursively layout children
        let mut all_child_layouts = Vec::new();
        let mut region_separators = Vec::new();
        let mut total_child_w = 0.0_f64;
        let total_child_h: f64;

        // Collect all regions: regions[] + children (last region)
        let mut all_regions: Vec<&[State]> = Vec::new();
        for region in &state.regions {
            all_regions.push(region);
        }
        if !state.children.is_empty() {
            all_regions.push(&state.children);
        }

        let mut all_inner_transitions: Vec<TransitionLayout> = Vec::new();
        let mut region_inner_transitions_per_region: Vec<Vec<TransitionLayout>> = Vec::new();
        let mut region_child_starts: Vec<usize> = Vec::new();
        let mut is_concurrent = false;
        if all_regions.len() > 1 {
            is_concurrent = true;
            // Multiple concurrent regions.
            // Java: each region produces its own SvekResult with
            //   calculateDimension() = lf_span + delta(15, 15).
            // ConcurrentStates.calculateDimensionSlow() sums the heights
            // with NO extra spacing between regions.
            // The separator line is drawn at the boundary between regions.
            let mut region_y = 0.0;
            for (i, region) in all_regions.iter().enumerate() {
                let (mut child_layouts, mut inner_tr, child_w, child_h) =
                    layout_children_with_graphviz(
                        region,
                        transitions,
                        initial_ids,
                        final_ids,
                        false,
                    );
                offset_children(&mut child_layouts, 0.0, region_y);
                for tr in &mut inner_tr {
                    offset_transition(tr, 0.0, region_y);
                }
                total_child_w = total_child_w.max(child_w);
                // Each region's SvekResult height = lf_span_h + 12.0
                // Java: SvekResult.calculateDimension() = lf_span.delta(0, 12)
                let region_h = child_h + 12.0;
                region_y += region_h;
                if i > 0 {
                    region_child_starts.push(all_child_layouts.len());
                }
                all_child_layouts.extend(child_layouts);
                region_inner_transitions_per_region.push(inner_tr.clone());
                all_inner_transitions.extend(inner_tr);

                if i < all_regions.len() - 1 {
                    region_separators.push(region_y);
                }
            }
            total_child_h = region_y;
        } else {
            let parent_has_pins = state_has_direct_pin_child(state);
            let (child_layouts, inner_tr, child_w, child_h) = layout_children_with_graphviz(
                &state.children,
                transitions,
                initial_ids,
                final_ids,
                parent_has_pins,
            );
            total_child_w = child_w;
            total_child_h = child_h;
            all_child_layouts = child_layouts;
            all_inner_transitions = inner_tr;
        }

        // Java: InnerStateAutonom.calculateDimensionSlow()
        //   inner_img = SvekResult.calculateDimension() = lf_span + delta(0, 12)
        //   dim = title.mergeTB(attr, inner_img)  →  (max(title_w, inner_w), title_h + inner_h)
        //   result = dim.delta(2*MARGIN + 2*MARGIN_LINE)  →  (dim_w + 20, dim_h + 20)
        //
        // Java SvekResult.calculateDimension() returns
        //   minMax.getDimension().delta(0, 12)
        // i.e. width = lf_span_w (no extra), height = lf_span_h + 12.
        // For concurrent regions, each region individually gets delta(0, 12).
        let inner_img_w = total_child_w;
        let inner_img_h = if is_concurrent {
            total_child_h
        } else {
            total_child_h + 12.0
        };

        // Java uses {rank=source}/{rank=sink} cluster constraints for
        // inputPin/outputPin children. Each pin type at a cluster boundary
        // adds vertical space via the cluster's rank separation (ranksep=60).
        // Our inner graphviz solve runs without a cluster wrapper, so the rank
        // constraints don't take effect. We add a height bonus to approximate
        // the extra vertical space Java produces from the rank separation.
        let pin_bonus = {
            let children = &state.children;
            let has_regular = children.iter().any(|c| {
                c.stereotype.as_deref() != Some("inputPin")
                    && c.stereotype.as_deref() != Some("outputPin")
                    && !c.is_special
            });
            if has_regular {
                let has_input = children
                    .iter()
                    .any(|c| c.stereotype.as_deref() == Some("inputPin"));
                let has_output = children
                    .iter()
                    .any(|c| c.stereotype.as_deref() == Some("outputPin"));
                let pin_types = (has_input as u32) + (has_output as u32);
                pin_types as f64 * PIN_RANK_HEIGHT_BONUS
            } else {
                0.0
            }
        };
        // When a composite has no direct pin children but contains child
        // composites that themselves have pins, Java's cluster-based rank
        // constraints propagate extra vertical space to the parent level.
        // Approximate this with a per-child-pin-type bonus (only when the
        // parent doesn't already have its own direct pin_bonus).
        let nested_pin_bonus = if pin_bonus == 0.0 {
            let mut bonus = 0.0;
            for child in &state.children {
                if qualifies_for_inner_pin_cluster(child, transitions) {
                    continue;
                }
                if !child.children.is_empty() || !child.regions.is_empty() {
                    let child_has_input = child
                        .children
                        .iter()
                        .any(|c| c.stereotype.as_deref() == Some("inputPin"));
                    let child_has_output = child
                        .children
                        .iter()
                        .any(|c| c.stereotype.as_deref() == Some("outputPin"));
                    let child_pin_types = (child_has_input as u32) + (child_has_output as u32);
                    bonus += child_pin_types as f64 * 16.0;
                }
            }
            bonus
        } else {
            0.0
        };
        let inner_height = inner_img_h + composite_height_overhead() + pin_bonus + nested_pin_bonus;

        let name_w = text_width(&state.name, STATE_NAME_FONT_SIZE);
        let merged_w = inner_img_w.max(name_w);
        let width = (merged_w + 2.0 * IE_MARGIN + 2.0 * IE_MARGIN_LINE).max(STATE_MIN_WIDTH);
        let height = inner_height.max(STATE_MIN_HEIGHT);

        return (
            StateNodeLayout {
                id: state.id.clone(),
                name: state.name.clone(),
                x: 0.0,
                y: 0.0,
                width,
                height,
                description: state.description.clone(),
                stereotype: state.stereotype.clone(),
                is_initial,
                is_final,
                is_composite: true,
                children: all_child_layouts,
                kind: state.kind.clone(),
                internal_transitions: all_inner_transitions,
                region_separators,
                region_child_starts,
                region_inner_transitions: region_inner_transitions_per_region,
                source_line: state.explicit_source_line.or(state.source_line),
                render_as_cluster: state_has_direct_pin_child(state),
            },
            width,
            height,
        );
    }

    // Simple state
    let (w, h) = estimate_state_size(state);
    (
        StateNodeLayout {
            id: state.id.clone(),
            name: state.name.clone(),
            x: 0.0,
            y: 0.0,
            width: w,
            height: h,
            description: state.description.clone(),
            stereotype: state.stereotype.clone(),
            is_initial,
            is_final,
            is_composite: false,
            children: Vec::new(),
            kind: state.kind.clone(),
            internal_transitions: Vec::new(),
            region_separators: Vec::new(),
            region_child_starts: Vec::new(),
            region_inner_transitions: Vec::new(),
            source_line: state.source_line,
            render_as_cluster: false,
        },
        w,
        h,
    )
}

/// Assign ranks to states based on transition graph connectivity.
///
/// States are grouped into rows (ranks): source states get rank 0,
/// their targets rank 1, etc.  States not participating in any transitions
/// within this scope are placed on the same rank as their declaration
/// order peers.
fn assign_ranks(
    state_ids: &[String],
    transitions: &[Transition],
    _initial_ids: &HashSet<String>,
    _final_ids: &HashSet<String>,
) -> Vec<Vec<usize>> {
    let n = state_ids.len();
    if n == 0 {
        return Vec::new();
    }

    let id_to_idx: HashMap<&str, usize> = state_ids
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();

    // Identify special [*] states that act as both initial and final.
    // Edges TO these states should not create back-edges for SCC/ranking,
    // since [*] logically represents two separate nodes (start dot + end dot).
    let special_set: HashSet<usize> = (0..n)
        .filter(|&i| state_ids[i] == "[*]" || state_ids[i].starts_with("[*]"))
        .collect();

    // Build adjacency from transitions scoped to this level.
    // Edges to special [*] states are excluded from the rank graph
    // to avoid artificial cycles (start and end are logically separate).
    let mut out_edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_degree: Vec<usize> = vec![0; n];
    let mut has_edge: Vec<bool> = vec![false; n];

    for tr in transitions {
        if let (Some(&fi), Some(&ti)) = (
            id_to_idx.get(tr.from.as_str()),
            id_to_idx.get(tr.to.as_str()),
        ) {
            // Skip self-loops for ranking
            if fi == ti {
                has_edge[fi] = true;
                continue;
            }

            // Skip edges TO special [*] states for ranking purposes.
            // These represent "go to final state" and shouldn't create cycles.
            if special_set.contains(&ti) {
                has_edge[fi] = true;
                has_edge[ti] = true;
                continue;
            }

            out_edges[fi].push(ti);
            in_degree[ti] += 1;
            has_edge[fi] = true;
            has_edge[ti] = true;
        }
    }

    // Topological rank assignment with cycle breaking.
    //
    // 1. Find strongly connected components (SCCs) and collapse them.
    // 2. Rank the DAG of SCCs using longest-path from sources.
    // 3. States within the same SCC get the same rank.

    // DFS-based Tarjan's SCC algorithm
    let mut scc_id: Vec<i32> = vec![-1; n];
    let mut scc_stack: Vec<usize> = Vec::new();
    let mut on_stack: Vec<bool> = vec![false; n];
    let mut dfs_num: Vec<i32> = vec![-1; n];
    let mut dfs_low: Vec<i32> = vec![0; n];
    let mut dfs_counter: i32 = 0;
    let mut num_sccs: usize = 0;

    // Iterative Tarjan
    {
        // Use a work stack to avoid recursion
        enum Action {
            Visit(usize),
            Finish(usize),
        }
        let mut work: Vec<Action> = Vec::new();

        for start in 0..n {
            if dfs_num[start] >= 0 {
                continue;
            }
            work.push(Action::Visit(start));

            while let Some(action) = work.pop() {
                match action {
                    Action::Visit(u) => {
                        if dfs_num[u] >= 0 {
                            continue;
                        }
                        dfs_num[u] = dfs_counter;
                        dfs_low[u] = dfs_counter;
                        dfs_counter += 1;
                        scc_stack.push(u);
                        on_stack[u] = true;

                        // Push finish action first (will be processed after children)
                        work.push(Action::Finish(u));

                        // Push children in reverse order for correct DFS ordering
                        for &v in out_edges[u].iter().rev() {
                            if dfs_num[v] < 0 {
                                work.push(Action::Visit(v));
                            }
                        }
                    }
                    Action::Finish(u) => {
                        // Update low-link from children
                        for &v in &out_edges[u] {
                            if scc_id[v] < 0 {
                                // v is still on stack or not yet visited
                                if on_stack[v] {
                                    dfs_low[u] = dfs_low[u].min(dfs_low[v]);
                                }
                            }
                        }

                        if dfs_low[u] == dfs_num[u] {
                            // Root of an SCC
                            let scc = num_sccs;
                            num_sccs += 1;
                            while let Some(w) = scc_stack.pop() {
                                on_stack[w] = false;
                                scc_id[w] = scc as i32;
                                if w == u {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Assign any unvisited nodes to their own SCC
    for item in scc_id.iter_mut().take(n) {
        if *item < 0 {
            *item = num_sccs as i32;
            num_sccs += 1;
        }
    }

    // Build DAG of SCCs
    let mut scc_out: Vec<HashSet<usize>> = vec![HashSet::new(); num_sccs];
    let mut scc_in_degree: Vec<usize> = vec![0; num_sccs];
    let mut scc_has_edge: Vec<bool> = vec![false; num_sccs];

    for u in 0..n {
        let su = scc_id[u] as usize;
        for &v in &out_edges[u] {
            let sv = scc_id[v] as usize;
            if su != sv && scc_out[su].insert(sv) {
                scc_in_degree[sv] += 1;
                scc_has_edge[su] = true;
                scc_has_edge[sv] = true;
            }
        }
        if has_edge[u] {
            scc_has_edge[su] = true;
        }
    }

    // Topological sort + longest path for SCC DAG
    let mut scc_rank: Vec<i32> = vec![-1; num_sccs];
    let mut queue = VecDeque::new();

    for s in 0..num_sccs {
        if scc_has_edge[s] && scc_in_degree[s] == 0 {
            scc_rank[s] = 0;
            queue.push_back(s);
        }
    }

    while let Some(su) = queue.pop_front() {
        for &sv in &scc_out[su] {
            let new_rank = scc_rank[su] + 1;
            if new_rank > scc_rank[sv] {
                scc_rank[sv] = new_rank;
            }
            scc_in_degree[sv] -= 1;
            if scc_in_degree[sv] == 0 {
                queue.push_back(sv);
            }
        }
    }

    // Map SCC ranks back to state ranks
    let mut rank: Vec<i32> = vec![-1; n];
    for i in 0..n {
        let si = scc_id[i] as usize;
        rank[i] = scc_rank[si];
    }

    // Check if ANY states have edges in this scope
    let any_edges = has_edge.iter().any(|&e| e);

    if !any_edges {
        // No edges at all: fall back to vertical stacking (one state per rank)
        for (i, r) in rank.iter_mut().enumerate().take(n) {
            *r = i as i32;
        }
    } else {
        // States without edges: place at the same rank as nearest connected state
        // in declaration order, or rank 0 if none.
        let mut last_connected_rank = 0;
        for i in 0..n {
            if !has_edge[i] {
                rank[i] = last_connected_rank;
            } else if rank[i] >= 0 {
                last_connected_rank = rank[i];
            }
        }

        // Ensure all unranked nodes are at rank 0
        for r in &mut rank {
            if *r < 0 {
                *r = 0;
            }
        }
    }

    // Build rank -> [state_indices]
    let max_rank = rank.iter().copied().max().unwrap_or(0);
    let mut ranks: Vec<Vec<usize>> = vec![Vec::new(); (max_rank + 1) as usize];
    for i in 0..n {
        ranks[rank[i] as usize].push(i);
    }

    // Remove empty ranks
    ranks.retain(|r| !r.is_empty());

    ranks
}

/// Layout a list of states using rank-based placement.
///
/// States connected by transitions are placed on successive rows.
/// States on the same rank are placed side-by-side horizontally.
///
/// Returns `(laid_out_nodes, content_width, content_height)`.
fn layout_states_ranked(
    states: &[State],
    transitions: &[Transition],
    initial_ids: &HashSet<String>,
    final_ids: &HashSet<String>,
    start_x: f64,
    start_y: f64,
) -> (Vec<StateNodeLayout>, f64, f64) {
    layout_states_ranked_with_spacing(
        states,
        transitions,
        initial_ids,
        final_ids,
        start_x,
        start_y,
        INNER_RANKSEP,
        INNER_NODESEP,
    )
}

fn layout_states_ranked_with_spacing(
    states: &[State],
    transitions: &[Transition],
    initial_ids: &HashSet<String>,
    final_ids: &HashSet<String>,
    start_x: f64,
    start_y: f64,
    ranksep: f64,
    nodesep: f64,
) -> (Vec<StateNodeLayout>, f64, f64) {
    if states.is_empty() {
        return (Vec::new(), 0.0, 0.0);
    }

    // First pass: compute sizes for all states
    let mut sized_entries: Vec<(StateNodeLayout, f64, f64)> = Vec::new();
    for state in states {
        sized_entries.push(compute_state_node(
            state,
            transitions,
            initial_ids,
            final_ids,
        ));
    }

    let state_ids: Vec<String> = states.iter().map(|s| s.id.clone()).collect();

    // Assign ranks based on transition connectivity
    let ranks = assign_ranks(&state_ids, transitions, initial_ids, final_ids);

    // Place states row by row
    let mut y_cursor = start_y;
    let mut total_width = 0.0_f64;
    let mut positioned: Vec<Option<(f64, f64)>> = vec![None; states.len()];

    for rank_indices in &ranks {
        // Get the entries in this rank
        let row_entries: Vec<usize> = rank_indices.to_vec();

        if row_entries.is_empty() {
            continue;
        }

        // Compute row dimensions
        let row_height = row_entries
            .iter()
            .map(|&i| sized_entries[i].2)
            .fold(0.0_f64, f64::max);
        let row_width: f64 = row_entries.iter().map(|&i| sized_entries[i].1).sum::<f64>()
            + nodesep * (row_entries.len() as f64 - 1.0).max(0.0);

        total_width = total_width.max(row_width);

        // Place each state in the row
        let mut x_cursor = start_x;
        for &idx in &row_entries {
            let (_, w, h) = &sized_entries[idx];
            // Vertically center within the row
            let y_offset = (row_height - h) / 2.0;
            positioned[idx] = Some((x_cursor, y_cursor + y_offset));
            x_cursor += w + nodesep;
        }

        y_cursor += row_height + ranksep;
    }

    // Remove trailing spacing
    let total_height = if ranks.is_empty() {
        0.0
    } else {
        y_cursor - start_y - ranksep
    };

    // Center each row within the total width
    for rank_indices in &ranks {
        let row_width: f64 = rank_indices
            .iter()
            .map(|&i| sized_entries[i].1)
            .sum::<f64>()
            + nodesep * (rank_indices.len() as f64 - 1.0).max(0.0);
        let offset = (total_width - row_width) / 2.0;
        if offset > 0.5 {
            for &idx in rank_indices {
                if let Some((ref mut x, _)) = positioned[idx] {
                    *x += offset;
                }
            }
        }
    }

    // Build final node list
    let mut nodes = Vec::new();
    for (idx, (mut node, _w, _h)) in sized_entries.into_iter().enumerate() {
        if let Some((x, y)) = positioned[idx] {
            node.x = x;
            node.y = y;

            // Offset children to absolute positions within the composite.
            // Java inner SvekResult adds moveDelta (inner margin) on top of
            // the composite header offset.
            if node.is_composite {
                let inner_margin = composite_inner_margin(&node.children);
                let child_offset_x = x + COMPOSITE_PADDING;
                let child_offset_y = y + composite_inner_y_offset() + inner_margin;
                offset_children(&mut node.children, child_offset_x, child_offset_y);
                // Java ConcurrentStates separators sit at the SvekResult origin
                // (composite_inner_y_offset, no inner_margin) plus each
                // region's dimension. Children, however, were offset by the
                // moveDelta (= inner_margin), so they appear below the
                // separator origin even though the separator y is computed
                // without the inner_margin.
                let sep_offset_y = y + composite_inner_y_offset();
                for sep_y in &mut node.region_separators {
                    *sep_y += sep_offset_y;
                }
                for region_trs in &mut node.region_inner_transitions {
                    for tr in region_trs {
                        offset_transition(tr, child_offset_x, child_offset_y);
                    }
                }
            }

            log::debug!(
                "  state '{}' @ ({:.1}, {:.1}) {}x{} composite={} initial={} final={}",
                node.id,
                node.x,
                node.y,
                node.width,
                node.height,
                node.is_composite,
                node.is_initial,
                node.is_final
            );

            nodes.push(node);
        }
    }

    (nodes, total_width, total_height)
}

/// Layout composite children using Graphviz, matching Java's approach of running
/// a separate graphviz pass for each composite state's inner content.
///
/// Returns `(laid_out_nodes, inner_transitions, content_width, content_height)` where
/// positions are relative to (0, 0) in the composite's inner space.
fn layout_children_with_graphviz(
    states: &[State],
    transitions: &[Transition],
    initial_ids: &HashSet<String>,
    final_ids: &HashSet<String>,
    parent_has_direct_pins: bool,
) -> (Vec<StateNodeLayout>, Vec<TransitionLayout>, f64, f64) {
    if states.is_empty() {
        return (Vec::new(), Vec::new(), 0.0, 0.0);
    }

    let state_order: HashMap<&str, usize> = states
        .iter()
        .enumerate()
        .map(|(idx, state)| (state.id.as_str(), idx))
        .collect();

    fn push_state_gv_node(
        state: &State,
        w: f64,
        h: f64,
        gv_nodes: &mut Vec<LayoutNode>,
        node_id_order: &mut Vec<String>,
        use_port_labels: bool,
    ) {
        let is_input_pin = state.stereotype.as_deref() == Some("inputPin");
        let is_output_pin = state.stereotype.as_deref() == Some("outputPin");
        let is_pin = is_input_pin || is_output_pin;
        let is_circle = state.is_special
            || matches!(
                state.kind,
                StateKind::History
                    | StateKind::DeepHistory
                    | StateKind::EntryPoint
                    | StateKind::ExitPoint
                    | StateKind::End
            );
        let is_diamond = matches!(state.kind, StateKind::Choice);
        let (node_w, node_h) = if is_pin { (PIN_SIZE, PIN_SIZE) } else { (w, h) };
        let shape = if is_circle {
            Some(crate::svek::shape_type::ShapeType::Circle)
        } else if is_pin {
            Some(crate::svek::shape_type::ShapeType::RectanglePort)
        } else if is_diamond {
            Some(crate::svek::shape_type::ShapeType::Diamond)
        } else {
            None
        };
        let entity_pos = if is_input_pin {
            Some(crate::svek::node::EntityPosition::InputPin)
        } else if is_output_pin {
            Some(crate::svek::node::EntityPosition::OutputPin)
        } else {
            None
        };
        // Port label width is used by the LimitFinder to track pin name text
        // extent. When the parent has direct pin children and inner clusters are
        // disabled, compute_state_node's pin_bonus already accounts for rank
        // spacing; tracking port labels would over-extend the bounding box.
        let plw = if is_pin && use_port_labels {
            Some(text_width(&state.name, STATE_NAME_FONT_SIZE))
        } else {
            None
        };
        gv_nodes.push(LayoutNode {
            id: state.id.clone(),
            label: state.name.clone(),
            width_pt: node_w,
            height_pt: node_h,
            shape,
            shield: None,
            entity_position: entity_pos,
            max_label_width: None,
            port_label_width: plw,
            order: Some(node_id_order.len()),
            image_width_pt: None,
            image_height_pt: None,
            lf_extra_left: 0.0,
            lf_rect_correction: !is_circle,
            lf_has_body_separator: !is_circle && !is_diamond && !is_pin,
            lf_node_polygon: false,
            lf_polygon_hack: false,
            lf_actor_stickman: false,
            hidden: false,
        });
        node_id_order.push(state.id.clone());
    }

    // Compute sizes for all child states
    let mut sized_map: HashMap<String, (StateNodeLayout, f64, f64)> = HashMap::new();
    for state in states {
        let (node, w, h) = compute_state_node(state, transitions, initial_ids, final_ids);
        sized_map.insert(state.id.clone(), (node, w, h));
    }

    // Java's inner state solve exports child groups into the same DOT graph.
    // For composites without direct pin children, child composites with pins
    // are rendered as real DOT clusters in the parent's inner solve. When the
    // parent itself has direct pin children, the pre-sized standalone approach
    // is used instead, because compute_state_node's pin_bonus already accounts
    // for the rank separation and inner clusters would produce different spacing.
    let mut cluster_child_ids: HashSet<String> = HashSet::new();
    let mut cluster_descendant_ids: HashMap<String, HashSet<String>> = HashMap::new();
    if !parent_has_direct_pins {
        for state in states {
            if !qualifies_for_inner_pin_cluster(state, transitions) {
                continue;
            }
            let mut descendant_ids = HashSet::new();
            collect_state_descendant_ids(state, &mut descendant_ids);
            cluster_child_ids.insert(state.id.clone());
            cluster_descendant_ids.insert(state.id.clone(), descendant_ids);
        }
    }

    // Build graphviz nodes and inner cluster specs.
    let mut gv_nodes: Vec<LayoutNode> = Vec::new();
    let mut node_id_order: Vec<String> = Vec::new();
    let mut has_pins = false;
    let mut cluster_specs: Vec<LayoutClusterSpec> = Vec::new();
    let mut graph_node_ids: HashSet<String> = HashSet::new();
    for state in states {
        if cluster_child_ids.contains(&state.id) {
            let mut node_ids = Vec::new();
            for child in &state.children {
                let (_, w, h) = sized_map.get(&child.id).cloned().unwrap_or_else(|| {
                    let (node, w, h) =
                        compute_state_node(child, transitions, initial_ids, final_ids);
                    sized_map.insert(child.id.clone(), (node.clone(), w, h));
                    (node, w, h)
                });
                if matches!(
                    child.stereotype.as_deref(),
                    Some("inputPin") | Some("outputPin")
                ) {
                    has_pins = true;
                }
                push_state_gv_node(child, w, h, &mut gv_nodes, &mut node_id_order, true);
                node_ids.push(child.id.clone());
                graph_node_ids.insert(child.id.clone());
            }
            let title_w = text_width(&state.name, STATE_NAME_FONT_SIZE);
            let title_h =
                crate::font_metrics::typo_ascent("SansSerif", STATE_NAME_FONT_SIZE, false, false)
                    .round()
                    + 5.0;
            cluster_specs.push(LayoutClusterSpec {
                id: state.id.replace(|c: char| !c.is_ascii_alphanumeric(), "_"),
                qualified_name: state.id.clone(),
                title: Some(state.name.clone()),
                style: crate::svek::cluster::ClusterStyle::Rectangle,
                label_width: Some(title_w),
                label_height: Some(title_h),
                node_ids,
                sub_clusters: Vec::new(),
                order: state.source_line,
                has_link_from_or_to_group: false,
                special_point_id: None,
            });
            continue;
        }

        let (_, w, h) = sized_map.get(&state.id).unwrap();
        if matches!(
            state.stereotype.as_deref(),
            Some("inputPin") | Some("outputPin")
        ) {
            has_pins = true;
        }
        push_state_gv_node(
            state,
            *w,
            *h,
            &mut gv_nodes,
            &mut node_id_order,
            !parent_has_direct_pins,
        );
        graph_node_ids.insert(state.id.clone());
    }

    // Build graphviz edges for transitions involving the flattened inner graph.
    let mut gv_edges: Vec<LayoutEdge> = Vec::new();
    let mut active_transitions: Vec<&Transition> = Vec::new();
    for tr in transitions {
        if !graph_node_ids.contains(&tr.from) || !graph_node_ids.contains(&tr.to) {
            continue;
        }
        gv_edges.push(LayoutEdge {
            from: tr.from.clone(),
            to: tr.to.clone(),
            label: if tr.label.is_empty() {
                None
            } else {
                Some(tr.label.clone())
            },
            label_dimension: None,
            tail_label: None,
            tail_label_dimension: None,
            tail_label_boxed: false,
            head_label: None,
            head_label_dimension: None,
            head_label_boxed: false,
            tail_decoration: crate::svek::edge::LinkDecoration::None,
            head_decoration: crate::svek::edge::LinkDecoration::None,
            line_style: crate::svek::edge::LinkStyle::Normal,
            minlen: if tr.length > 0 {
                (tr.length - 1) as u32
            } else {
                0
            },
            invisible: false,
            is_opale: false,
            no_constraint: false,
        });
        active_transitions.push(tr);
    }

    let use_legacy_pin_ranksep = has_pins && cluster_specs.is_empty();
    let graph = LayoutGraph {
        nodes: gv_nodes,
        edges: gv_edges,
        clusters: cluster_specs,
        rankdir: RankDir::TopToBottom,
        is_activity: false,
        // Java uses a single flat DOT graph with clusters where all nodes share
        // the same ranksep (60pt) at the OUTER state solve. For this inner solve,
        // once pin-backed child composites are represented as real clusters, Java
        // falls back to the default Graphviz 0.5in gap. Only the legacy no-cluster
        // path still needs the larger ranksep compensation.
        ranksep_override: Some(if use_legacy_pin_ranksep {
            graphviz::MIN_RANK_SEP_PX
        } else {
            INNER_RANKSEP
        }),
        // Java inner composite graphs omit nodesep, using Graphviz's default 0.25in (18pt).
        nodesep_override: Some(INNER_NODESEP),
        use_simplier_dot_link_strategy: false,
        arrow_font_size: None,
    };

    // Run graphviz for inner composite content
    let gv_layout = match graphviz::layout_with_svek(&graph) {
        Ok(layout) => {
            log::debug!(
                "inner graphviz: {}x{}, lf_span=({:.1},{:.1}), move_delta=({:.1},{:.1})",
                layout.total_width,
                layout.total_height,
                layout.lf_span.0,
                layout.lf_span.1,
                layout.move_delta.0,
                layout.move_delta.1,
            );
            layout
        }
        Err(e) => {
            log::warn!("graphviz inner layout failed: {e}, falling back to ranked layout");
            let (nodes, w, h) =
                layout_states_ranked(states, transitions, initial_ids, final_ids, 0.0, 0.0);
            return (nodes, Vec::new(), w, h);
        }
    };

    // Inner composite children are positioned relative to (0,0).
    // The outer layout adds composite header + padding offsets, so we must NOT
    // apply the render_offset here — that would double-count the moveDelta margin.

    let all_cluster_descendant_ids: HashSet<String> = cluster_descendant_ids
        .values()
        .flat_map(|ids| ids.iter().cloned())
        .collect();

    // Convert graphviz results to child node positions (relative to origin)
    let mut nodes = Vec::new();
    for gv_node in &gv_layout.nodes {
        if all_cluster_descendant_ids.contains(&gv_node.id) {
            continue;
        }
        if let Some((template, _w, _h)) = sized_map.remove(&gv_node.id) {
            let x = gv_node.cx - gv_node.width / 2.0;
            let y = gv_node.cy - gv_node.height / 2.0;
            let w = gv_node.width;
            let h = gv_node.height;

            let mut node = template;
            node.x = x;
            node.y = y;
            node.width = w;
            node.height = h;

            // For nested composites, offset children
            if node.is_composite {
                let child_offset_x = x + COMPOSITE_PADDING;
                let child_offset_y = y + composite_inner_y_offset();
                offset_children(&mut node.children, child_offset_x, child_offset_y);
                for sep_y in &mut node.region_separators {
                    *sep_y += child_offset_y;
                }
            }

            nodes.push(node);
        }
    }

    // Reconstruct flattened child composites from Graphviz cluster bounds.
    for state in states {
        if !cluster_child_ids.contains(&state.id) {
            continue;
        }
        let cluster_id = state.id.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
        let Some(cl) = gv_layout.clusters.iter().find(|c| c.id == cluster_id) else {
            continue;
        };
        let direct_child_ids: HashSet<&str> = state
            .children
            .iter()
            .map(|child| child.id.as_str())
            .collect();
        let child_order: HashMap<&str, usize> = state
            .children
            .iter()
            .enumerate()
            .map(|(idx, child)| (child.id.as_str(), idx))
            .collect();
        let mut children = Vec::new();
        for gv_node in &gv_layout.nodes {
            if !direct_child_ids.contains(gv_node.id.as_str()) {
                continue;
            }
            if let Some((template, _, _)) = sized_map.remove(&gv_node.id) {
                let x = gv_node.cx - gv_node.width / 2.0;
                let y = gv_node.cy - gv_node.height / 2.0;
                let w = gv_node.width;
                let h = gv_node.height;

                let mut child_node = template;
                child_node.x = x;
                child_node.y = y;
                child_node.width = w;
                child_node.height = h;

                if child_node.is_composite {
                    let child_offset_x = x + COMPOSITE_PADDING;
                    let child_offset_y = y + composite_inner_y_offset();
                    offset_children(&mut child_node.children, child_offset_x, child_offset_y);
                    for sep_y in &mut child_node.region_separators {
                        *sep_y += child_offset_y;
                    }
                }

                children.push(child_node);
            }
        }
        children.sort_by_key(|child| {
            child_order
                .get(child.id.as_str())
                .copied()
                .unwrap_or(usize::MAX)
        });

        nodes.push(StateNodeLayout {
            id: state.id.clone(),
            name: state.name.clone(),
            x: cl.x,
            y: cl.y,
            width: cl.width,
            height: cl.height,
            description: state.description.clone(),
            stereotype: state.stereotype.clone(),
            is_initial: initial_ids.contains(&state.id),
            is_final: final_ids.contains(&state.id),
            is_composite: true,
            children,
            kind: state.kind.clone(),
            internal_transitions: Vec::new(),
            region_separators: Vec::new(),
            region_child_starts: Vec::new(),
            region_inner_transitions: Vec::new(),
            source_line: state.source_line,
            render_as_cluster: true,
        });
    }
    nodes.sort_by_key(|node| {
        state_order
            .get(node.id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });

    // Java's inner SvekResult keeps children below any extra LimitFinder-only
    // content (for example pin labels above the cluster). Our outer composite
    // margin already contributes the baseline 6/7px moveDelta, so only apply
    // the excess render_offset beyond that baseline here.
    let baseline_inner_margin = composite_inner_margin(&nodes);
    let extra_inner_y = (gv_layout.render_offset.1 - baseline_inner_margin).max(0.0);
    if extra_inner_y > 0.001 {
        offset_children(&mut nodes, 0.0, extra_inner_y);
    }

    // Build inner transitions from the graphviz edges.
    // These are positioned relative to (0,0) in the inner space, matching the
    // child node positions. The outer layout will offset them later.
    let mut inner_transitions = Vec::new();
    for (i, gv_edge) in gv_layout.edges.iter().enumerate() {
        let (from_id, to_id) = if i < active_transitions.len() {
            (
                active_transitions[i].from.clone(),
                active_transitions[i].to.clone(),
            )
        } else {
            (gv_edge.from.clone(), gv_edge.to.clone())
        };
        let label = if i < active_transitions.len() {
            active_transitions[i].label.clone()
        } else {
            gv_edge.label.clone().unwrap_or_default()
        };
        let source_line = if i < active_transitions.len() {
            active_transitions[i].source_line
        } else {
            None
        };

        let points: Vec<(f64, f64)> = gv_edge
            .points
            .iter()
            .map(|&(x, y)| (x, y + extra_inner_y))
            .collect();
        let raw_path_d = gv_edge
            .raw_path_d
            .as_ref()
            .map(|d| graphviz::transform_path_d(d, 0.0, extra_inner_y));
        let arrow_polygon = gv_edge
            .arrow_polygon_points
            .as_ref()
            .map(|pts| pts.iter().map(|&(x, y)| (x, y + extra_inner_y)).collect());

        let label_xy = gv_edge.label_xy.map(|(x, y)| {
            let nx = x + gv_layout.move_delta.0 - gv_layout.normalize_offset.0;
            let ny = y + gv_layout.move_delta.1 - gv_layout.normalize_offset.1 + extra_inner_y;
            (nx, ny)
        });

        inner_transitions.push(TransitionLayout {
            from_id,
            to_id,
            label,
            points,
            raw_path_d,
            arrow_polygon,
            label_xy,
            label_wh: gv_edge.label_wh,
            source_line,
            is_inner: true,
        });
    }

    // Use LimitFinder span for the inner image dimensions, matching
    // Java's SvekResult.calculateDimension() = minMax.getDimension().
    let inner_w = gv_layout.lf_span.0;
    let inner_h = gv_layout.lf_span.1;

    (nodes, inner_transitions, inner_w, inner_h)
}

/// Recursively offset children's positions from relative (0,0) to absolute.
fn offset_children(children: &mut [StateNodeLayout], offset_x: f64, offset_y: f64) {
    for child in children.iter_mut() {
        child.x += offset_x;
        child.y += offset_y;
        // Also offset any internal transitions stored in this child
        for tr in &mut child.internal_transitions {
            offset_transition(tr, offset_x, offset_y);
        }
        if child.is_composite {
            // Children of children are already relative to the child; the
            // recursive layout already set them.  But since we just moved the
            // parent, the children's absolute coords from the recursive call
            // were relative to (0,0), so we need to offset them too.
            offset_children(&mut child.children, offset_x, offset_y);
        }
    }
}

/// Offset a single transition layout by (dx, dy).
fn offset_transition(tr: &mut TransitionLayout, dx: f64, dy: f64) {
    for p in &mut tr.points {
        p.0 += dx;
        p.1 += dy;
    }
    if let Some(ref mut d) = tr.raw_path_d {
        *d = crate::layout::graphviz::transform_path_d(d, dx, dy);
    }
    if let Some(ref mut pts) = tr.arrow_polygon {
        for p in pts.iter_mut() {
            p.0 += dx;
            p.1 += dy;
        }
    }
    if let Some(ref mut xy) = tr.label_xy {
        xy.0 += dx;
        xy.1 += dy;
    }
}

// ---------------------------------------------------------------------------
// Transition routing
// ---------------------------------------------------------------------------

/// Build a lookup from state ID to its center position.
fn build_position_map(nodes: &[StateNodeLayout]) -> HashMap<String, (f64, f64, f64, f64)> {
    let mut map = HashMap::new();
    collect_positions(nodes, &mut map);
    map
}

/// Recursively collect (x, y, w, h) for every state node.
fn collect_positions(nodes: &[StateNodeLayout], map: &mut HashMap<String, (f64, f64, f64, f64)>) {
    for node in nodes {
        map.insert(node.id.clone(), (node.x, node.y, node.width, node.height));
        collect_positions(&node.children, map);
    }
}

/// Create transition layouts by connecting state positions.
fn layout_transitions(
    transitions: &[Transition],
    pos_map: &HashMap<String, (f64, f64, f64, f64)>,
) -> Vec<TransitionLayout> {
    let mut result = Vec::new();

    for tr in transitions {
        let from_pos = pos_map.get(&tr.from);
        let to_pos = pos_map.get(&tr.to);

        let (from_x, from_y, from_w, from_h) = if let Some(p) = from_pos {
            *p
        } else {
            log::warn!("transition source '{}' not found in layout", tr.from);
            continue;
        };

        let (to_x, to_y, to_w, to_h) = if let Some(p) = to_pos {
            *p
        } else {
            log::warn!("transition target '{}' not found in layout", tr.to);
            continue;
        };

        // Determine connection direction based on relative positions
        let from_cx = from_x + from_w / 2.0;
        let from_cy = from_y + from_h / 2.0;
        let to_cx = to_x + to_w / 2.0;
        let to_cy = to_y + to_h / 2.0;

        // Build connection points and Bezier path matching Java's graphviz output.
        // Java uses raw graphviz Bezier curves; we generate synthetic cubic Beziers
        // that approximate the same shape for vertical/horizontal connections.
        let (points, raw_path_d) = if (from_cy - to_cy).abs() < 1.0 {
            // Horizontal: connect right-center to left-center
            let (start, end) = if from_cx < to_cx {
                ((from_x + from_w, from_cy), (to_x, to_cy))
            } else {
                ((from_x, from_cy), (to_x + to_w, to_cy))
            };
            (vec![start, end], None)
        } else if to_cy > from_cy {
            // Target is below: bottom-center to top-center
            let sx = from_cx;
            let sy = from_y + from_h;
            let ex = to_cx;
            let ey = to_y;
            let dy = ey - sy;
            // Generate Bezier control points that approximate graphviz curve shape
            let cy1 = sy + dy * 0.25;
            let cy2 = sy + dy * 0.50;
            let raw = format!(
                "M{},{} C{},{} {},{} {},{}",
                crate::klimt::svg::fmt_coord(sx),
                crate::klimt::svg::fmt_coord(sy),
                crate::klimt::svg::fmt_coord(sx),
                crate::klimt::svg::fmt_coord(cy1),
                crate::klimt::svg::fmt_coord(ex),
                crate::klimt::svg::fmt_coord(cy2),
                crate::klimt::svg::fmt_coord(ex),
                crate::klimt::svg::fmt_coord(ey),
            );
            (vec![(sx, sy), (ex, ey)], Some(raw))
        } else {
            // Target is above: top-center to bottom-center
            let sx = from_cx;
            let sy = from_y;
            let ex = to_cx;
            let ey = to_y + to_h;
            (vec![(sx, sy), (ex, ey)], None)
        };

        log::debug!(
            "  transition '{}' -> '{}' [{}]: {:?}",
            tr.from,
            tr.to,
            tr.label,
            points
        );

        result.push(TransitionLayout {
            from_id: tr.from.clone(),
            to_id: tr.to.clone(),
            label: tr.label.clone(),
            points,
            raw_path_d,
            arrow_polygon: None,
            label_xy: None,
            label_wh: None,
            source_line: tr.source_line,
            is_inner: false,
        });
    }

    result
}

fn transition_key(tr: &Transition) -> (String, String, Option<usize>) {
    (tr.from.clone(), tr.to.clone(), tr.source_line)
}

fn transition_layout_key(tr: &TransitionLayout) -> (String, String, Option<usize>) {
    (tr.from_id.clone(), tr.to_id.clone(), tr.source_line)
}

/// Search for a nested state by ID within a list of top-level states.
fn find_nested_state_in_list<'a>(states: &'a [State], target_id: &str) -> Option<&'a State> {
    fn search<'a>(state: &'a State, target: &str) -> Option<&'a State> {
        if state.id == target {
            return Some(state);
        }
        for child in &state.children {
            if let Some(found) = search(child, target) {
                return Some(found);
            }
        }
        for region in &state.regions {
            for child in region {
                if let Some(found) = search(child, target) {
                    return Some(found);
                }
            }
        }
        None
    }
    for state in states {
        if let Some(found) = search(state, target_id) {
            return Some(found);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Perform the complete layout of a state diagram using Graphviz.
///
/// The result contains absolute positions for every state node, transition edge,
/// and note so that a renderer can draw them without further computation.
pub fn layout_state(diagram: &StateDiagram) -> Result<StateLayout> {
    log::debug!(
        "layout_state: {} states, {} transitions, {} notes",
        diagram.states.len(),
        diagram.transitions.len(),
        diagram.notes.len()
    );

    // Classify [*] states as initial or final
    let (initial_ids, final_ids) = classify_special_states(&diagram.states, &diagram.transitions);

    log::debug!("  initial_ids: {initial_ids:?}, final_ids: {final_ids:?}");

    // Collect implicit states (referenced in transitions but not declared)
    let implicit_states = collect_implicit_states(&diagram.states, &diagram.transitions);
    log::debug!("  implicit states: {}", implicit_states.len());

    // Merge declared + implicit states, deduplicating by ID.
    let mut all_states: Vec<State> = diagram.states.clone();
    all_states.extend(implicit_states);
    dedup_states(&mut all_states);

    // Re-classify after adding implicit states
    let (initial_ids, final_ids) = classify_special_states(&all_states, &diagram.transitions);

    // ---------------------------------------------------------------
    // Detect composites that have cross-boundary edges (Java:
    // thereALinkFromOrToGroup). These use a single-graph cluster model
    // where children are real nodes in the outer DOT inside a
    // `subgraph cluster_X`, with a special point (zaent) for edge
    // routing. Composites WITHOUT cross-boundary edges keep the
    // existing two-level solve.
    // ---------------------------------------------------------------
    // Collect all child IDs (recursively) for each top-level composite.
    fn collect_all_child_ids(state: &State, out: &mut HashSet<String>) {
        for child in &state.children {
            out.insert(child.id.clone());
            collect_all_child_ids(child, out);
        }
        for region in &state.regions {
            for child in region {
                out.insert(child.id.clone());
                collect_all_child_ids(child, out);
            }
        }
    }

    fn collect_recursive_states<'a>(state: &'a State, out: &mut Vec<&'a State>) {
        out.push(state);
        for child in &state.children {
            collect_recursive_states(child, out);
        }
        for region in &state.regions {
            for child in region {
                collect_recursive_states(child, out);
            }
        }
    }

    let mut all_states_recursive: Vec<&State> = Vec::new();
    for state in &all_states {
        collect_recursive_states(state, &mut all_states_recursive);
    }

    let mut composite_child_ids: HashMap<String, HashSet<String>> = HashMap::new();
    for state in &all_states_recursive {
        let state = *state;
        let is_composite = !state.children.is_empty() || !state.regions.is_empty();
        if is_composite {
            let mut child_ids = HashSet::new();
            collect_all_child_ids(state, &mut child_ids);
            composite_child_ids.insert(state.id.clone(), child_ids);
        }
    }

    // Determine which composites need the cluster model. Java's outer state
    // graph exports two important classes of groups as real DOT clusters:
    //   1) groups with direct port children, so input/output pins sit outside
    //      the cluster border on source/sink ranks;
    //   2) groups whose children participate in cross-boundary links.
    // The latter also need a cluster special point for links targeting the
    // group entity itself.
    let mut cluster_composite_ids: HashSet<String> = HashSet::new();
    for state in &all_states_recursive {
        let state = *state;
        let is_composite = !state.children.is_empty() || !state.regions.is_empty();
        if !is_composite || !state.regions.is_empty() {
            // Skip concurrent regions — they can't be represented as simple clusters
            continue;
        }
        let has_direct_pin_child = state_has_direct_pin_child(state);
        let child_ids = composite_child_ids.get(&state.id).unwrap();
        let has_cross_child_link = diagram.transitions.iter().any(|tr| {
            // External node → child inside composite (not the composite itself)
            let ext_to_child =
                !child_ids.contains(&tr.from) && tr.from != state.id && child_ids.contains(&tr.to);
            let ext_from_child =
                !child_ids.contains(&tr.to) && tr.to != state.id && child_ids.contains(&tr.from);
            ext_to_child || ext_from_child
        });
        if has_direct_pin_child || has_cross_child_link {
            let reason = if has_direct_pin_child && has_cross_child_link {
                "direct pins + cross-child-links"
            } else if has_direct_pin_child {
                "direct pins"
            } else {
                "cross-child-links"
            };
            log::debug!("  composite '{}' has {} → cluster model", state.id, reason);
            cluster_composite_ids.insert(state.id.clone());
        }
    }

    // Compute sizes for all states. For cluster-composites, we only
    // need leaf-level sizing (children are real nodes); for non-cluster
    // composites, compute_state_node does the inner graphviz solve.
    let mut sized_map: HashMap<String, (StateNodeLayout, f64, f64)> = HashMap::new();
    for state in &all_states {
        let (node, w, h) =
            compute_state_node(state, &diagram.transitions, &initial_ids, &final_ids);
        sized_map.insert(state.id.clone(), (node, w, h));
    }

    // Build graphviz nodes and clusters.
    let mut gv_nodes: Vec<LayoutNode> = Vec::new();
    let mut node_id_order: Vec<String> = Vec::new();

    // Helper: add a state as a graphviz node
    fn push_state_gv_node(
        state: &State,
        w: f64,
        h: f64,
        gv_nodes: &mut Vec<LayoutNode>,
        node_id_order: &mut Vec<String>,
    ) {
        let is_circle_kind = state.is_special
            || matches!(
                state.kind,
                StateKind::History
                    | StateKind::DeepHistory
                    | StateKind::EntryPoint
                    | StateKind::ExitPoint
                    | StateKind::End
            );
        let is_diamond = matches!(state.kind, StateKind::Choice);
        let is_pin = state.stereotype.as_deref() == Some("inputPin")
            || state.stereotype.as_deref() == Some("outputPin");
        let (node_w, node_h) = if is_pin { (PIN_SIZE, PIN_SIZE) } else { (w, h) };
        let shape = if is_circle_kind {
            Some(crate::svek::shape_type::ShapeType::Circle)
        } else if is_pin {
            Some(crate::svek::shape_type::ShapeType::RectanglePort)
        } else if is_diamond {
            Some(crate::svek::shape_type::ShapeType::Diamond)
        } else {
            None
        };
        let is_circle = is_circle_kind;
        let entity_pos = if state.stereotype.as_deref() == Some("inputPin") {
            Some(crate::svek::node::EntityPosition::InputPin)
        } else if state.stereotype.as_deref() == Some("outputPin") {
            Some(crate::svek::node::EntityPosition::OutputPin)
        } else {
            None
        };
        gv_nodes.push(LayoutNode {
            id: state.id.clone(),
            label: state.name.clone(),
            width_pt: node_w,
            height_pt: node_h,
            shape,
            shield: None,
            entity_position: entity_pos,
            max_label_width: None,
            port_label_width: if is_pin {
                Some(text_width(&state.name, STATE_NAME_FONT_SIZE))
            } else {
                None
            },
            order: Some(node_id_order.len()),
            image_width_pt: None,
            image_height_pt: None,
            lf_extra_left: 0.0,
            lf_rect_correction: !is_circle,
            lf_has_body_separator: !is_circle && !is_diamond && !is_pin,
            lf_node_polygon: false,
            lf_polygon_hack: false,
            lf_actor_stickman: false,
            hidden: false,
        });
        node_id_order.push(state.id.clone());
    }

    // Recursively flatten children of a cluster-composite into gv_nodes.
    // Returns (cluster_spec, list of child IDs added).
    fn flatten_composite_children(
        state: &State,
        transitions: &[Transition],
        initial_ids: &HashSet<String>,
        final_ids: &HashSet<String>,
        sized_map: &mut HashMap<String, (StateNodeLayout, f64, f64)>,
        gv_nodes: &mut Vec<LayoutNode>,
        node_id_order: &mut Vec<String>,
        cluster_composite_ids: &HashSet<String>,
    ) -> LayoutClusterSpec {
        let mut node_ids: Vec<String> = Vec::new();
        let mut sub_clusters: Vec<LayoutClusterSpec> = Vec::new();

        for child in &state.children {
            let is_child_composite = !child.children.is_empty() || !child.regions.is_empty();

            if is_child_composite && cluster_composite_ids.contains(&child.id) {
                // Nested cluster-composite: recurse
                let sub_spec = flatten_composite_children(
                    child,
                    transitions,
                    initial_ids,
                    final_ids,
                    sized_map,
                    gv_nodes,
                    node_id_order,
                    cluster_composite_ids,
                );
                sub_clusters.push(sub_spec);
            } else {
                // Leaf node or non-cluster composite (pre-sized as single rect)
                let (_, w, h) = sized_map.get(&child.id).cloned().unwrap_or_else(|| {
                    let (node, w, h) =
                        compute_state_node(child, transitions, initial_ids, final_ids);
                    sized_map.insert(child.id.clone(), (node.clone(), w, h));
                    (node, w, h)
                });
                push_state_gv_node(child, w, h, gv_nodes, node_id_order);
                node_ids.push(child.id.clone());
            }
        }

        // Compute label dimensions for the cluster (composite title).
        // Java: <TABLE BGCOLOR="..." WIDTH="tw" HEIGHT="th">
        let title_w = text_width(&state.name, STATE_NAME_FONT_SIZE);
        // Java: dim.getHeight() for the title text block. Java uses OS/2
        // sTypoAscender for the text dimension height in DOT labels.
        // For DejaVu Sans 14pt: round(1556/2048 * 14) = 11.
        // cluster_dot_label() subtracts 5 from the height, so we add 5 here
        // to pass through the correct DOT TABLE HEIGHT value.
        let title_h =
            crate::font_metrics::typo_ascent("SansSerif", STATE_NAME_FONT_SIZE, false, false)
                .round()
                + 5.0;
        // Java divides these by 72 to pass as inches in DOT, we pass pixels
        // and the builder converts. The label_width/height are passed through
        // to the <TABLE WIDTH="..." HEIGHT="..."> in the DOT cluster label.

        // Generate a special point ID for edge routing through this cluster.
        let sp_id = format!(
            "zaent_{}",
            state.id.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        );

        LayoutClusterSpec {
            id: state
                .id
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
                .to_string(),
            qualified_name: state.id.clone(),
            title: Some(state.name.clone()),
            style: crate::svek::cluster::ClusterStyle::Rectangle,
            label_width: Some(title_w),
            label_height: Some(title_h),
            node_ids,
            sub_clusters,
            order: state.source_line,
            has_link_from_or_to_group: true,
            special_point_id: Some(sp_id),
        }
    }

    // Map from composite state ID → special point ID for edge rewriting.
    let mut composite_special_points: HashMap<String, String> = HashMap::new();

    // Build cluster specs and nodes.
    let mut cluster_specs: Vec<LayoutClusterSpec> = Vec::new();
    for state in &all_states {
        let is_composite = !state.children.is_empty() || !state.regions.is_empty();

        if is_composite && cluster_composite_ids.contains(&state.id) {
            // Cluster-composite: flatten children into the outer DOT.
            let spec = flatten_composite_children(
                state,
                &diagram.transitions,
                &initial_ids,
                &final_ids,
                &mut sized_map,
                &mut gv_nodes,
                &mut node_id_order,
                &cluster_composite_ids,
            );
            if let Some(ref sp) = spec.special_point_id {
                composite_special_points.insert(state.id.clone(), sp.clone());
            }
            cluster_specs.push(spec);
        } else {
            // Non-cluster state: add as a regular node with pre-computed size.
            let (_, w, h) = sized_map.get(&state.id).unwrap();
            push_state_gv_node(state, *w, *h, &mut gv_nodes, &mut node_id_order);
        }
    }

    let mut attached_notes: Vec<(&crate::model::state::StateNote, f64, f64)> = Vec::new();
    let mut standalone_note_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for note in &diagram.notes {
        if let Some(entity_id) = note.entity_id.as_ref() {
            let (w, h) = estimate_note_size(&note.text);
            gv_nodes.push(LayoutNode {
                id: entity_id.clone(),
                label: entity_id.clone(),
                width_pt: w,
                height_pt: h,
                shape: None,
                shield: None,
                entity_position: None,
                max_label_width: None,
                port_label_width: None,
                order: Some(node_id_order.len()),
                image_width_pt: None,
                image_height_pt: None,
                lf_extra_left: 0.0,
                lf_rect_correction: false,
                lf_has_body_separator: false,
                lf_node_polygon: false,
                lf_polygon_hack: false,
                lf_actor_stickman: false,
                hidden: false,
            });
            node_id_order.push(entity_id.clone());
            if note.target.is_some() {
                attached_notes.push((note, w, h));
            } else {
                standalone_note_ids.insert(entity_id.clone());
            }
        }
    }

    // Build graphviz edges. For cluster-composites, edges to/from
    // the composite ID are rewritten to use the special point (zaent).
    // Inner transitions of cluster-composites are also included since
    // their children are now real nodes in the outer DOT.
    let outer_node_ids: HashSet<String> = gv_nodes.iter().map(|n| n.id.clone()).collect();
    let mut gv_edges: Vec<LayoutEdge> = Vec::new();

    // Collect all child IDs across all cluster-composites for edge routing.
    let mut all_cluster_child_ids: HashSet<String> = HashSet::new();
    for cid in &cluster_composite_ids {
        if let Some(child_ids) = composite_child_ids.get(cid) {
            all_cluster_child_ids.extend(child_ids.iter().cloned());
        }
    }

    for tr in &diagram.transitions {
        // Rewrite from/to: if an endpoint is a cluster-composite ID,
        // route through its special point (zaent).
        let from = if let Some(sp) = composite_special_points.get(&tr.from) {
            sp.clone()
        } else {
            tr.from.clone()
        };
        let to = if let Some(sp) = composite_special_points.get(&tr.to) {
            sp.clone()
        } else {
            tr.to.clone()
        };

        // Include edge if both endpoints are in the outer DOT (real nodes
        // or special points). Special points are added by the cluster builder.
        let from_known = outer_node_ids.contains(&from)
            || composite_special_points.values().any(|sp| sp == &from);
        let to_known =
            outer_node_ids.contains(&to) || composite_special_points.values().any(|sp| sp == &to);
        if !from_known || !to_known {
            continue;
        }

        gv_edges.push(LayoutEdge {
            from,
            to,
            label: if tr.label.is_empty() {
                None
            } else {
                Some(tr.label.clone())
            },
            label_dimension: None,
            tail_label: None,
            tail_label_dimension: None,
            tail_label_boxed: false,
            head_label: None,
            head_label_dimension: None,
            head_label_boxed: false,
            tail_decoration: crate::svek::edge::LinkDecoration::None,
            head_decoration: crate::svek::edge::LinkDecoration::None,
            line_style: crate::svek::edge::LinkStyle::Normal,
            minlen: if tr.length > 0 {
                (tr.length - 1) as u32
            } else {
                0
            },
            invisible: false,
            is_opale: false,
            no_constraint: false,
        });
    }

    // Track the starting index of note edges in gv_edges so we can identify
    // them after graphviz solve. Note edges are kept visible (not invisible)
    // so that graphviz routes splines for them — the edge endpoints provide
    // precise anchor coordinates for the note arrow, matching Java Smetana.
    let note_edge_start_idx = gv_edges.len();
    for (note, _w, _h) in &attached_notes {
        let (from, to, minlen) = match note.position.as_str() {
            "left" => (
                note.entity_id.clone().unwrap(),
                note.target.clone().unwrap(),
                0,
            ),
            "top" => (
                note.entity_id.clone().unwrap(),
                note.target.clone().unwrap(),
                2,
            ),
            "bottom" => (
                note.target.clone().unwrap(),
                note.entity_id.clone().unwrap(),
                2,
            ),
            _ => (
                note.target.clone().unwrap(),
                note.entity_id.clone().unwrap(),
                0,
            ),
        };
        gv_edges.push(LayoutEdge {
            from,
            to,
            label: None,
            label_dimension: None,
            tail_label: None,
            tail_label_dimension: None,
            tail_label_boxed: false,
            head_label: None,
            head_label_dimension: None,
            head_label_boxed: false,
            tail_decoration: crate::svek::edge::LinkDecoration::None,
            head_decoration: crate::svek::edge::LinkDecoration::None,
            line_style: crate::svek::edge::LinkStyle::Dashed,
            minlen,
            invisible: false,
            is_opale: false,
            no_constraint: false,
        });
    }

    let rankdir = match diagram.direction {
        crate::model::diagram::Direction::TopToBottom => RankDir::TopToBottom,
        crate::model::diagram::Direction::LeftToRight => RankDir::LeftToRight,
        crate::model::diagram::Direction::BottomToTop => RankDir::BottomToTop,
        crate::model::diagram::Direction::RightToLeft => RankDir::RightToLeft,
    };

    let graph = LayoutGraph {
        nodes: gv_nodes,
        edges: gv_edges,
        clusters: cluster_specs,
        rankdir,
        is_activity: false,
        ranksep_override: Some(graphviz::MIN_RANK_SEP_PX),
        nodesep_override: None,
        use_simplier_dot_link_strategy: false,
        arrow_font_size: None,
    };

    // Run graphviz via svek pipeline
    let gv_layout = graphviz::layout_with_svek(&graph)
        .map_err(|e| crate::error::Error::Layout(format!("state graphviz layout: {e}")))?;

    log::debug!(
        "graphviz layout: {:.0}x{:.0}, {} nodes, {} edges, move_delta=({:.1},{:.1}), lf_span=({:.1},{:.1})",
        gv_layout.total_width, gv_layout.total_height,
        gv_layout.nodes.len(), gv_layout.edges.len(),
        gv_layout.move_delta.0, gv_layout.move_delta.1,
        gv_layout.lf_span.0, gv_layout.lf_span.1,
    );
    // Convert graphviz NodeLayout (center coords) to StateNodeLayout (top-left coords).
    // Graphviz results are already normalized to origin (0,0) by layout_with_svek.
    //
    // Compute effective y-margin: Java's moveDelta.y depends on what element is at
    // the top of the diagram. Rects in LimitFinder draw at (y-1) → margin_y=7.
    // Circles don't get the -1 offset → margin_y=6. We detect which case applies
    // by checking if the topmost node is a special (circle) state.
    let margin_y = {
        let topmost = gv_layout
            .nodes
            .iter()
            .filter_map(|node| {
                find_nested_state_in_list(&all_states, &node.id).map(|state| (node, state))
            })
            .min_by(|(a, _), (b, _)| {
                let a_top = a.cy - a.height / 2.0;
                let b_top = b.cy - b.height / 2.0;
                a_top.partial_cmp(&b_top).unwrap()
            });
        if let Some((top_node, top_state)) = topmost {
            let is_pin = matches!(
                top_state.stereotype.as_deref(),
                Some("inputPin") | Some("outputPin")
            );
            let is_circle = top_state.is_special
                || matches!(
                    top_state.kind,
                    StateKind::History
                        | StateKind::DeepHistory
                        | StateKind::EntryPoint
                        | StateKind::ExitPoint
                        | StateKind::End
                );
            log::debug!(
                "  topmost node='{}' pin={} circle={} render_offset_y={:.1}",
                top_node.id,
                is_pin,
                is_circle,
                gv_layout.render_offset.1,
            );
            if is_pin {
                // Java's effective outer Y margin for pin-led state graphs lands on an
                // integer boundary; carrying the fractional render_offset here shifts the
                // entire non-pin subtree and all routed edges by the same residue.
                gv_layout.render_offset.1.floor() + PIN_RANK_HEIGHT_BONUS + MARGIN
            } else if is_circle {
                6.0
            } else {
                7.0
            }
        } else {
            let topmost_raw = gv_layout.nodes.iter().min_by(|a, b| {
                let a_top = a.cy - a.height / 2.0;
                let b_top = b.cy - b.height / 2.0;
                a_top.partial_cmp(&b_top).unwrap()
            });
            if let Some(top_node) = topmost_raw {
                let is_pin = false;
                let is_circle = false;
                log::debug!(
                    "  topmost node='{}' pin={} circle={} render_offset_y={:.1}",
                    top_node.id,
                    is_pin,
                    is_circle,
                    gv_layout.render_offset.1,
                );
            }
            7.0
        }
    };
    log::debug!("  margin_y={:.0}", margin_y);

    // Use svek render_offset for x margin (varies with note presence).
    // Java: moveDelta(6 - LF_minX, ...) where LF_minX depends on node types.
    // For diagrams with only rect-drawn entities, render_offset.0 = 7 (= MARGIN).
    // When note entities (UPath-drawn) determine the min, render_offset.0 = 6.
    let margin_x = gv_layout.render_offset.0;

    fn position_state_node_template(
        mut template: StateNodeLayout,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> StateNodeLayout {
        template.x = x;
        template.y = y;
        template.width = w;
        template.height = h;

        if template.is_composite {
            let inner_margin = composite_inner_margin(&template.children);
            let child_offset_x = x + COMPOSITE_PADDING;
            let child_offset_y = y + composite_inner_y_offset() + inner_margin;
            offset_children(&mut template.children, child_offset_x, child_offset_y);
            for tr in &mut template.internal_transitions {
                offset_transition(tr, child_offset_x, child_offset_y);
            }
            for region_trs in &mut template.region_inner_transitions {
                for tr in region_trs {
                    offset_transition(tr, child_offset_x, child_offset_y);
                }
            }
            // Java: ConcurrentStates draws separators at the SvekResult origin
            // (composite_inner_y_offset, no extra inner_margin) + each inner's
            // dimension. The children themselves were offset by the moveDelta
            // (= inner_margin) so they're below the separator origin. Subtract
            // the inner_margin here so the separator y matches Java.
            let sep_offset_y = y + composite_inner_y_offset();
            for sep_y in &mut template.region_separators {
                *sep_y += sep_offset_y;
            }
        }

        template
    }

    let gv_nodes_by_id: HashMap<&str, &crate::layout::graphviz::NodeLayout> = gv_layout
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let gv_clusters_by_qname: HashMap<&str, &crate::layout::graphviz::ClusterLayout> = gv_layout
        .clusters
        .iter()
        .map(|cluster| (cluster.qualified_name.as_str(), cluster))
        .collect();

    let mut state_layouts: Vec<StateNodeLayout> = Vec::new();
    let mut node_position_map: HashMap<String, (f64, f64, f64, f64)> = HashMap::new();
    let attached_note_ids: HashSet<&str> = attached_notes
        .iter()
        .filter_map(|(note, _, _)| note.entity_id.as_deref())
        .collect();
    let mut attached_note_positions: HashMap<String, (f64, f64, f64, f64)> = HashMap::new();
    let mut standalone_note_positions: HashMap<String, (f64, f64, f64, f64)> = HashMap::new();

    for gv_node in &gv_layout.nodes {
        if attached_note_ids.contains(gv_node.id.as_str()) {
            let x = gv_node.cx - gv_node.width / 2.0 + margin_x;
            let y = gv_node.cy - gv_node.height / 2.0 + margin_y;
            attached_note_positions
                .insert(gv_node.id.clone(), (x, y, gv_node.width, gv_node.height));
            continue;
        }
        if standalone_note_ids.contains(gv_node.id.as_str()) {
            let x = gv_node.cx - gv_node.width / 2.0 + margin_x;
            let y = gv_node.cy - gv_node.height / 2.0 + margin_y;
            standalone_note_positions
                .insert(gv_node.id.clone(), (x, y, gv_node.width, gv_node.height));
            continue;
        }
        // Skip cluster-composite children: processed during cluster reconstruction
        if all_cluster_child_ids.contains(&gv_node.id) {
            continue;
        }
        if let Some((template, _w, _h)) = sized_map.remove(&gv_node.id) {
            let x = gv_node.cx - gv_node.width / 2.0 + margin_x;
            let y = gv_node.cy - gv_node.height / 2.0 + margin_y;
            let w = gv_node.width;
            let h = gv_node.height;

            node_position_map.insert(gv_node.id.clone(), (x, y, w, h));
            log::debug!(
                "  state '{}': gv_cx={:.1} gv_cy={:.1} → x={:.1} y={:.1} w={:.0} h={:.0} initial={} final={}",
                gv_node.id, gv_node.cx, gv_node.cy, x, y, w, h,
                template.is_initial, template.is_final,
            );

            state_layouts.push(position_state_node_template(template, x, y, w, h));
        }
    }

    // Reconstruct cluster-composites from graphviz cluster bounds.
    // For each cluster-composite, find the cluster layout and children
    // node positions, then build a StateNodeLayout with absolute child coords.
    fn rebuild_cluster_composite(
        state: &State,
        cluster_composite_ids: &HashSet<String>,
        sized_map: &mut HashMap<String, (StateNodeLayout, f64, f64)>,
        gv_nodes_by_id: &HashMap<&str, &crate::layout::graphviz::NodeLayout>,
        gv_clusters_by_qname: &HashMap<&str, &crate::layout::graphviz::ClusterLayout>,
        margin_x: f64,
        margin_y: f64,
        initial_ids: &HashSet<String>,
        final_ids: &HashSet<String>,
        node_position_map: &mut HashMap<String, (f64, f64, f64, f64)>,
    ) -> Option<StateNodeLayout> {
        let cluster_layout = gv_clusters_by_qname.get(state.id.as_str())?;
        let x = cluster_layout.x + margin_x;
        let y = cluster_layout.y + margin_y;
        let w = cluster_layout.width;
        let h = cluster_layout.height;

        let mut children: Vec<StateNodeLayout> = Vec::new();
        let mut collect_child = |child: &State, children: &mut Vec<StateNodeLayout>| {
            if cluster_composite_ids.contains(&child.id) {
                if let Some(cluster_child) = rebuild_cluster_composite(
                    child,
                    cluster_composite_ids,
                    sized_map,
                    gv_nodes_by_id,
                    gv_clusters_by_qname,
                    margin_x,
                    margin_y,
                    initial_ids,
                    final_ids,
                    node_position_map,
                ) {
                    children.push(cluster_child);
                }
                return;
            }

            let Some(gv_node) = gv_nodes_by_id.get(child.id.as_str()) else {
                return;
            };
            let Some((template, _, _)) = sized_map.remove(&child.id) else {
                return;
            };
            let child_x = gv_node.cx - gv_node.width / 2.0 + margin_x;
            let child_y = gv_node.cy - gv_node.height / 2.0 + margin_y;
            let child_w = gv_node.width;
            let child_h = gv_node.height;
            node_position_map.insert(child.id.clone(), (child_x, child_y, child_w, child_h));
            children.push(position_state_node_template(
                template, child_x, child_y, child_w, child_h,
            ));
        };

        for child in &state.children {
            collect_child(child, &mut children);
        }
        for region in &state.regions {
            for child in region {
                collect_child(child, &mut children);
            }
        }

        let composite_node = StateNodeLayout {
            id: state.id.clone(),
            name: state.name.clone(),
            x,
            y,
            width: w,
            height: h,
            description: state.description.clone(),
            stereotype: state.stereotype.clone(),
            is_initial: initial_ids.contains(&state.id),
            is_final: final_ids.contains(&state.id),
            is_composite: true,
            children,
            kind: state.kind.clone(),
            internal_transitions: Vec::new(),
            region_separators: Vec::new(),
            region_child_starts: Vec::new(),
            region_inner_transitions: Vec::new(),
            source_line: state.explicit_source_line.or(state.source_line),
            render_as_cluster: true,
        };
        node_position_map.insert(state.id.clone(), (x, y, w, h));
        Some(composite_node)
    }

    for state in &all_states {
        if !cluster_composite_ids.contains(&state.id) {
            continue;
        }
        if let Some(composite_node) = rebuild_cluster_composite(
            state,
            &cluster_composite_ids,
            &mut sized_map,
            &gv_nodes_by_id,
            &gv_clusters_by_qname,
            margin_x,
            margin_y,
            &initial_ids,
            &final_ids,
            &mut node_position_map,
        ) {
            log::debug!(
                "  cluster-composite '{}': cluster=({:.1},{:.1}) {:.1}x{:.0}",
                state.id,
                composite_node.x,
                composite_node.y,
                composite_node.width,
                composite_node.height,
            );
            state_layouts.push(composite_node);
        }
    }

    // Convert graphviz EdgeLayout to TransitionLayout.
    // The svek pipeline returns edges with raw SVG path data and arrow polygons.
    // Build a mapping from special point IDs back to composite state IDs.
    let _sp_to_composite: HashMap<String, String> = composite_special_points
        .iter()
        .map(|(composite_id, sp_id)| (sp_id.clone(), composite_id.clone()))
        .collect();
    // Build a parallel list of original transitions corresponding to each
    // gv_edge (excluding note edges). Each gv_edge was built from a diagram
    // transition, possibly with from/to rewritten to a special point.
    let active_transitions: Vec<&Transition> = {
        let mut result = Vec::new();
        for tr in &diagram.transitions {
            let from = composite_special_points.get(&tr.from).unwrap_or(&tr.from);
            let to = composite_special_points.get(&tr.to).unwrap_or(&tr.to);
            let from_known = outer_node_ids.contains(from)
                || composite_special_points.values().any(|sp| sp == from);
            let to_known =
                outer_node_ids.contains(to) || composite_special_points.values().any(|sp| sp == to);
            if from_known && to_known {
                result.push(tr);
            }
        }
        result
    };
    let mut transition_layouts: Vec<TransitionLayout> = Vec::new();
    let mut visible_transition_keys: HashSet<(String, String, Option<usize>)> = HashSet::new();

    // Extract note edge endpoints for precise anchor coordinates.
    // Note edges start at index `note_edge_start_idx` in gv_layout.edges.
    // For each note edge, extract the endpoint on the target node boundary.
    let mut note_edge_anchors: HashMap<String, (f64, f64)> = HashMap::new();
    for (note_i, (note, _w, _h)) in attached_notes.iter().enumerate() {
        let edge_idx = note_edge_start_idx + note_i;
        if edge_idx < gv_layout.edges.len() {
            let gv_edge = &gv_layout.edges[edge_idx];
            if !gv_edge.points.is_empty() {
                // The edge direction depends on note position:
                //   left/top:    from=note, to=target → endpoint (last point) is on target
                //   right/bottom: from=target, to=note → startpoint (first point) is on target
                let (ax, ay) = match note.position.as_str() {
                    "left" | "top" => {
                        let last = gv_edge.points[gv_edge.points.len() - 1];
                        (last.0 + margin_x, last.1 + margin_y)
                    }
                    _ => {
                        let first = gv_edge.points[0];
                        (first.0 + margin_x, first.1 + margin_y)
                    }
                };
                if let Some(ref eid) = note.entity_id {
                    log::debug!(
                        "  note edge anchor: note={} pos={} ax={:.4} ay={:.4}",
                        eid,
                        note.position,
                        ax,
                        ay,
                    );
                    note_edge_anchors.insert(eid.clone(), (ax, ay));
                }
            }
        }
    }

    for (i, gv_edge) in gv_layout.edges.iter().enumerate() {
        // Skip note edges — they are not rendered as transitions.
        if i >= note_edge_start_idx {
            continue;
        }

        let (from_id, to_id) = if i < active_transitions.len() {
            (
                active_transitions[i].from.clone(),
                active_transitions[i].to.clone(),
            )
        } else {
            (gv_edge.from.clone(), gv_edge.to.clone())
        };
        let label = if i < active_transitions.len() {
            active_transitions[i].label.clone()
        } else {
            gv_edge.label.clone().unwrap_or_default()
        };

        // Shift points by MARGIN (x) and margin_y (y) to match state positions
        let mut points: Vec<(f64, f64)> = gv_edge
            .points
            .iter()
            .map(|&(x, y)| (x + margin_x, y + margin_y))
            .collect();

        let mut raw_path_d = gv_edge
            .raw_path_d
            .as_ref()
            .map(|d| graphviz::transform_path_d(d, margin_x, margin_y));

        let arrow_polygon = gv_edge.arrow_polygon_points.as_ref().map(|pts| {
            pts.iter()
                .map(|&(x, y)| (x + margin_x, y + margin_y))
                .collect()
        });

        // simulateCompound: clip edge paths at cluster borders (Java DotPath.simulateCompound).
        // Edges routed through zaent special points start/end inside the cluster.
        // Java clips them to the cluster border for visual correctness.
        {
            let tail_rect = if cluster_composite_ids.contains(&from_id) {
                gv_layout
                    .clusters
                    .iter()
                    .find(|c| c.qualified_name == from_id)
                    .map(|cl| {
                        crate::klimt::geom::RectangleArea::new(
                            cl.x + margin_x,
                            cl.y + margin_y,
                            cl.x + margin_x + cl.width,
                            cl.y + margin_y + cl.height,
                        )
                    })
            } else {
                None
            };
            let head_rect = if cluster_composite_ids.contains(&to_id) {
                gv_layout
                    .clusters
                    .iter()
                    .find(|c| c.qualified_name == to_id)
                    .map(|cl| {
                        crate::klimt::geom::RectangleArea::new(
                            cl.x + margin_x,
                            cl.y + margin_y,
                            cl.x + margin_x + cl.width,
                            cl.y + margin_y + cl.height,
                        )
                    })
            } else {
                None
            };
            if tail_rect.is_some() || head_rect.is_some() {
                if let Some(ref d) = raw_path_d {
                    if let Some(dot_path) = crate::svek::svg_result::parse_svg_path_d_to_dotpath(d)
                    {
                        let clipped =
                            dot_path.simulate_compound(head_rect.as_ref(), tail_rect.as_ref());
                        if !clipped.beziers.is_empty() {
                            raw_path_d = Some(clipped.to_svg_d());
                            let mut new_pts = Vec::new();
                            for (bi, bez) in clipped.beziers.iter().enumerate() {
                                if bi == 0 {
                                    new_pts.push((bez.x1, bez.y1));
                                }
                                new_pts.push((bez.ctrlx1, bez.ctrly1));
                                new_pts.push((bez.ctrlx2, bez.ctrly2));
                                new_pts.push((bez.x2, bez.y2));
                            }
                            points = new_pts;
                        }
                    }
                }
            }
        }

        // label_xy from GraphLayout is pre-moveDelta, pre-normalization.
        // Apply moveDelta + normalization + MARGIN to match path/node coords.
        let label_xy = gv_edge.label_xy.map(|(x, y)| {
            let nx = x + gv_layout.move_delta.0 - gv_layout.normalize_offset.0 + margin_x;
            let ny = y + gv_layout.move_delta.1 - gv_layout.normalize_offset.1 + margin_y;
            (nx, ny)
        });

        let label_wh = gv_edge.label_wh;

        let source_line = if i < active_transitions.len() {
            active_transitions[i].source_line
        } else {
            None
        };

        let layout = TransitionLayout {
            from_id,
            to_id,
            label,
            points,
            raw_path_d,
            arrow_polygon,
            label_xy,
            label_wh,
            source_line,
            is_inner: false,
        };
        if !layout.points.is_empty() || layout.raw_path_d.is_some() {
            visible_transition_keys.insert(transition_layout_key(&layout));
        }
        transition_layouts.push(layout);
    }

    // Inject internal transitions from composite states (resolved by inner graphviz).
    // These have proper Bezier paths from the inner solve, unlike synthesized transitions.
    fn collect_internal_transitions(
        node: &mut StateNodeLayout,
        out: &mut Vec<TransitionLayout>,
        keys: &mut HashSet<(String, String, Option<usize>)>,
    ) {
        let drained: Vec<TransitionLayout> = node.internal_transitions.drain(..).collect();
        for tr in drained {
            let key = transition_layout_key(&tr);
            log::debug!("[inject] internal transition: {} -> {} path_d={:?} points={} key={:?} already_present={}", tr.from_id, tr.to_id, tr.raw_path_d.as_ref().map(|d| &d[..d.len().min(60)]), tr.points.len(), key, keys.contains(&key));
            if !keys.contains(&key) {
                keys.insert(key);
                out.push(tr);
            }
        }
        for child in &mut node.children {
            collect_internal_transitions(child, out, keys);
        }
    }
    for node in &mut state_layouts {
        collect_internal_transitions(node, &mut transition_layouts, &mut visible_transition_keys);
    }

    // Synthesize any remaining missing transitions from child positions
    // (fallback for transitions not resolved by inner graphviz).
    let full_pos_map = build_position_map(&state_layouts);
    let missing_transitions: Vec<Transition> = diagram
        .transitions
        .iter()
        .filter(|tr| !visible_transition_keys.contains(&transition_key(tr)))
        .cloned()
        .collect();
    if !missing_transitions.is_empty() {
        transition_layouts.extend(layout_transitions(&missing_transitions, &full_pos_map));
    }
    // Expand content width to include edge label extents (Java LimitFinder
    // tracks text elements which can extend beyond node boundaries).
    let mut content_width = gv_layout.total_width;
    for edge in &gv_layout.edges {
        if let Some(ref label) = edge.label {
            if let Some((lx, _ly)) = edge.label_xy {
                // lx is pre-moveDelta, pre-normalization. Transform to post-normalization space.
                let lx_norm = lx + gv_layout.move_delta.0 - gv_layout.normalize_offset.0;
                let tl = crate::font_metrics::text_width(label, "SansSerif", 13.0, false, false);
                let label_right = lx_norm + tl;
                log::debug!(
                    "  edge label '{}': lx={:.1} tl={:.2} right={:.2}, content_width={:.1}",
                    label,
                    lx_norm,
                    tl,
                    label_right,
                    content_width
                );
                if label_right > content_width {
                    content_width = label_right;
                }
            }
        }
    }

    // Layout notes. Attached notes are anchored to their target state box and
    // centered on the target axis; detached notes fall back to the legacy
    // stacked-right placement.
    let content_height = gv_layout.total_height;
    let detached_note_x = margin_x + content_width + NOTE_OFFSET;
    let mut detached_note_y = margin_y;
    let mut note_layouts = Vec::new();

    for note in &diagram.notes {
        let (nw, nh) = estimate_note_size(&note.text);
        let (note_x, note_y, anchor) = if let Some(entity_id) = note.entity_id.as_ref() {
            if let Some(&(nx, ny, _nw_gv, _nh_gv)) = attached_note_positions.get(entity_id) {
                // Use the precise anchor from the graphviz edge endpoint if available.
                // This matches Java Smetana's edge routing, which clips the note-to-target
                // edge spline to the target node's polygon boundary.
                let (ax, ay, note_x, note_y) =
                    if let Some(&(ea_x, ea_y)) = note_edge_anchors.get(entity_id) {
                        (ea_x, ea_y, nx, ny)
                    } else {
                        note.target
                            .as_ref()
                            .and_then(|target_id| {
                                node_position_map.get(target_id).map(|&(tx, ty, tw, th)| {
                                    // Fallback: approximate anchor from node geometry.
                                    let polygon_center_y = ty + th / 2.0 - (th - th.round()) / 2.0;
                                    let polygon_right = tx + tw.round();
                                    let polygon_center_x = tx + tw / 2.0 - (tw - tw.round()) / 2.0;
                                    match note.position.as_str() {
                                        "left" => (tx, polygon_center_y, nx, ny),
                                        "top" => (polygon_center_x, ty, nx, ny),
                                        "bottom" => (polygon_center_x, ty + th, nx, ny),
                                        _ => (polygon_right, polygon_center_y, nx, ny),
                                    }
                                })
                            })
                            .unwrap_or((nx, ny, nx, ny))
                    };
                (note_x, note_y, Some((ax, ay)))
            } else if let Some(target_id) = note.target.as_ref() {
                if let Some(&(tx, ty, tw, th)) = node_position_map.get(target_id) {
                    let center_x = tx + tw / 2.0;
                    let center_y = ty + th / 2.0;
                    match note.position.as_str() {
                        "left" => (tx - 35.0 - nw, center_y - nh / 2.0, Some((tx, center_y))),
                        "top" => (center_x - nw / 2.0, ty - 35.0 - nh, Some((center_x, ty))),
                        "bottom" => (
                            center_x - nw / 2.0,
                            ty + th + 35.0,
                            Some((center_x, ty + th)),
                        ),
                        _ => (
                            tx + tw + 35.0,
                            center_y - nh / 2.0,
                            Some((tx + tw, center_y)),
                        ),
                    }
                } else {
                    (detached_note_x, detached_note_y, None)
                }
            } else if let Some(&(sx, sy, _sw, _sh)) = standalone_note_positions.get(entity_id) {
                // Standalone `note as ALIAS` — use graphviz-determined position
                (sx, sy, None)
            } else {
                (detached_note_x, detached_note_y, None)
            }
        } else if let Some(target_id) = note.target.as_ref() {
            if let Some(&(tx, ty, tw, th)) = node_position_map.get(target_id) {
                let center_x = tx + tw / 2.0;
                let center_y = ty + th / 2.0;
                match note.position.as_str() {
                    "left" => (tx - 35.0 - nw, center_y - nh / 2.0, Some((tx, center_y))),
                    "top" => (center_x - nw / 2.0, ty - 35.0 - nh, Some((center_x, ty))),
                    "bottom" => (
                        center_x - nw / 2.0,
                        ty + th + 35.0,
                        Some((center_x, ty + th)),
                    ),
                    _ => (
                        tx + tw + 35.0,
                        center_y - nh / 2.0,
                        Some((tx + tw, center_y)),
                    ),
                }
            } else {
                (detached_note_x, detached_note_y, None)
            }
        } else {
            (detached_note_x, detached_note_y, None)
        };
        log::debug!(
            "  note @ ({:.1}, {:.1}) {}x{} pos={} target={:?}: '{}'",
            note_x,
            note_y,
            nw,
            nh,
            note.position,
            note.target,
            note.text
        );
        note_layouts.push(StateNoteLayout {
            x: note_x,
            y: note_y,
            width: nw,
            height: nh,
            text: note.text.clone(),
            position: note.position.clone(),
            target: note.target.clone(),
            entity_id: note.entity_id.clone(),
            source_line: note.source_line,
            anchor,
            opale_points: None,
        });
        if note.target.is_none() {
            detached_note_y += nh + PADDING;
        }
    }

    // Compute total bounding box.
    // Notes positioned by graphviz (attached or standalone) are already included
    // in content_width/content_height. Only truly detached notes (not in graphviz)
    // need separate bounding-box tracking.
    let non_gv_notes: Vec<&StateNoteLayout> = note_layouts
        .iter()
        .filter(|n| {
            n.entity_id.as_ref().is_none_or(|id| {
                !attached_note_positions.contains_key(id)
                    && !standalone_note_positions.contains_key(id)
            })
        })
        .collect();
    let notes_right = non_gv_notes
        .iter()
        .map(|n| n.x + n.width)
        .fold(0.0_f64, f64::max);
    let states_right = margin_x + content_width;
    let total_width = states_right.max(notes_right) + margin_x;
    let total_width = total_width.max(2.0 * margin_x);

    let notes_bottom = non_gv_notes
        .iter()
        .map(|n| n.y + n.height)
        .fold(0.0_f64, f64::max);
    let states_bottom = margin_y + content_height;
    let total_height = states_bottom.max(notes_bottom) + margin_y;
    let total_height = total_height.max(2.0 * margin_y);

    log::debug!(
        "layout_state done: {:.0}x{:.0}, {} states, {} transitions, {} notes",
        total_width,
        total_height,
        state_layouts.len(),
        transition_layouts.len(),
        note_layouts.len()
    );

    Ok(StateLayout {
        width: total_width,
        height: total_height,
        state_layouts,
        transition_layouts,
        note_layouts,
        move_delta: gv_layout.move_delta,
        lf_span: gv_layout.lf_span,
    })
}

// ---------------------------------------------------------------------------
// Direction transform
// ---------------------------------------------------------------------------

/// Apply a coordinate transform based on the diagram direction.
/// The layout algorithm always computes in top-to-bottom orientation;
/// for other directions we transform after the fact.
#[allow(dead_code)] // reserved for multi-direction state layout
fn apply_direction_transform(
    layout: &mut StateLayout,
    direction: &crate::model::diagram::Direction,
) {
    use crate::model::diagram::Direction;
    match direction {
        Direction::TopToBottom => {}
        Direction::LeftToRight => {
            transform_state_nodes_swap_xy(&mut layout.state_layouts);
            for tr in &mut layout.transition_layouts {
                for pt in &mut tr.points {
                    std::mem::swap(&mut pt.0, &mut pt.1);
                }
            }
            for note in &mut layout.note_layouts {
                std::mem::swap(&mut note.x, &mut note.y);
                std::mem::swap(&mut note.width, &mut note.height);
            }
            std::mem::swap(&mut layout.width, &mut layout.height);
        }
        Direction::RightToLeft => {
            transform_state_nodes_swap_xy(&mut layout.state_layouts);
            for tr in &mut layout.transition_layouts {
                for pt in &mut tr.points {
                    std::mem::swap(&mut pt.0, &mut pt.1);
                }
            }
            for note in &mut layout.note_layouts {
                std::mem::swap(&mut note.x, &mut note.y);
                std::mem::swap(&mut note.width, &mut note.height);
            }
            std::mem::swap(&mut layout.width, &mut layout.height);
            let w = layout.width;
            transform_state_nodes_mirror_x(&mut layout.state_layouts, w);
            for tr in &mut layout.transition_layouts {
                for pt in &mut tr.points {
                    pt.0 = w - pt.0;
                }
            }
            for note in &mut layout.note_layouts {
                note.x = w - note.x - note.width;
            }
        }
        Direction::BottomToTop => {
            let h = layout.height;
            transform_state_nodes_mirror_y(&mut layout.state_layouts, h);
            for tr in &mut layout.transition_layouts {
                for pt in &mut tr.points {
                    pt.1 = h - pt.1;
                }
            }
            for note in &mut layout.note_layouts {
                note.y = h - note.y - note.height;
            }
        }
    }
}

/// Recursively swap x <-> y for state nodes and their children.
#[allow(dead_code)] // used by apply_direction_transform
fn transform_state_nodes_swap_xy(nodes: &mut [StateNodeLayout]) {
    for node in nodes.iter_mut() {
        std::mem::swap(&mut node.x, &mut node.y);
        std::mem::swap(&mut node.width, &mut node.height);
        transform_state_nodes_swap_xy(&mut node.children);
    }
}

/// Recursively mirror state nodes horizontally.
#[allow(dead_code)] // used by apply_direction_transform
fn transform_state_nodes_mirror_x(nodes: &mut [StateNodeLayout], total_width: f64) {
    for node in nodes.iter_mut() {
        node.x = total_width - node.x - node.width;
        transform_state_nodes_mirror_x(&mut node.children, total_width);
    }
}

/// Recursively mirror state nodes vertically.
#[allow(dead_code)] // used by apply_direction_transform
fn transform_state_nodes_mirror_y(nodes: &mut [StateNodeLayout], total_height: f64) {
    for node in nodes.iter_mut() {
        node.y = total_height - node.y - node.height;
        transform_state_nodes_mirror_y(&mut node.children, total_height);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::state::{State, StateDiagram, StateNote, Transition};

    fn empty_diagram() -> StateDiagram {
        StateDiagram {
            states: vec![],
            transitions: vec![],
            notes: vec![],
            direction: Default::default(),
        }
    }

    fn simple_state(name: &str) -> State {
        State {
            name: name.to_string(),
            id: name.to_string(),
            description: vec![],
            stereotype: None,
            children: vec![],
            is_special: false,
            kind: crate::model::state::StateKind::default(),
            regions: vec![],
            source_line: None,
            explicit_source_line: None,
        }
    }

    fn special_state(id: &str) -> State {
        State {
            name: "[*]".to_string(),
            id: id.to_string(),
            description: vec![],
            stereotype: None,
            children: vec![],
            is_special: true,
            kind: crate::model::state::StateKind::default(),
            regions: vec![],
            source_line: None,
            explicit_source_line: None,
        }
    }

    fn transition(from: &str, to: &str, label: &str) -> Transition {
        Transition {
            from: from.to_string(),
            to: to.to_string(),
            label: label.to_string(),
            dashed: false,
            length: 2,
            source_line: None,
        }
    }

    // 1. Empty diagram
    #[test]
    fn test_empty_diagram() {
        let d = empty_diagram();
        let layout = layout_state(&d).unwrap();
        assert!(layout.state_layouts.is_empty());
        assert!(layout.transition_layouts.is_empty());
        assert!(layout.note_layouts.is_empty());
        assert!(layout.width > 0.0);
        assert!(layout.height > 0.0);
    }

    // 2. Single state
    #[test]
    fn test_single_state() {
        let d = StateDiagram {
            states: vec![simple_state("Active")],
            transitions: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();
        assert_eq!(layout.state_layouts.len(), 1);
        let node = &layout.state_layouts[0];
        assert_eq!(node.id, "Active");
        assert_eq!(node.name, "Active");
        assert!(node.width >= STATE_MIN_WIDTH);
        assert!(node.height >= STATE_MIN_HEIGHT);
        assert!(!node.is_initial);
        assert!(!node.is_final);
        assert!(!node.is_composite);
    }

    // 3. Initial [*] state
    #[test]
    fn test_initial_state() {
        let d = StateDiagram {
            states: vec![special_state("[*]"), simple_state("Active")],
            transitions: vec![transition("[*]", "Active", "")],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();
        assert_eq!(layout.state_layouts.len(), 2);

        let initial = &layout.state_layouts[0];
        assert!(initial.is_initial);
        assert!(!initial.is_final);
        assert_eq!(initial.width, 2.0 * SPECIAL_STATE_RADIUS);
        assert_eq!(initial.height, 2.0 * SPECIAL_STATE_RADIUS);
    }

    // 4. Final [*] state
    #[test]
    fn test_final_state() {
        let d = StateDiagram {
            states: vec![simple_state("Active"), special_state("[*]_final")],
            transitions: vec![transition("Active", "[*]_final", "")],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();

        let final_node = layout
            .state_layouts
            .iter()
            .find(|n| n.id == "[*]_final")
            .unwrap();
        assert!(final_node.is_final);
        assert!(!final_node.is_initial);
    }

    // 5. Start and stop states (scxml0001 style)
    #[test]
    fn test_start_stop_with_transitions() {
        let d = StateDiagram {
            states: vec![
                special_state("[*]_start"),
                simple_state("Active"),
                simple_state("Inactive"),
                special_state("[*]_end"),
            ],
            transitions: vec![
                transition("[*]_start", "Active", ""),
                transition("Active", "Inactive", "deactivate"),
                transition("Inactive", "[*]_end", ""),
            ],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();

        assert_eq!(layout.state_layouts.len(), 4);
        assert_eq!(layout.transition_layouts.len(), 3);

        // Start should be above Active
        let start = layout
            .state_layouts
            .iter()
            .find(|n| n.id == "[*]_start")
            .unwrap();
        let active = layout
            .state_layouts
            .iter()
            .find(|n| n.id == "Active")
            .unwrap();
        assert!(
            start.y < active.y,
            "start.y={} should be < active.y={}",
            start.y,
            active.y
        );

        // Transitions should have points
        for tl in &layout.transition_layouts {
            assert!(!tl.points.is_empty());
        }
    }

    // 6. Composite state
    #[test]
    fn test_composite_state() {
        let d = StateDiagram {
            states: vec![State {
                name: "Container".to_string(),
                id: "Container".to_string(),
                description: vec![],
                stereotype: None,
                children: vec![simple_state("Inner1"), simple_state("Inner2")],
                is_special: false,
                kind: crate::model::state::StateKind::default(),
                regions: vec![],
                source_line: None,
                explicit_source_line: None,
            }],
            transitions: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();
        assert_eq!(layout.state_layouts.len(), 1);

        let container = &layout.state_layouts[0];
        assert!(container.is_composite);
        assert_eq!(container.children.len(), 2);

        // Children should be inside the container
        for child in &container.children {
            assert!(child.x >= container.x);
            assert!(child.y >= container.y + composite_inner_y_offset());
            assert!(child.x + child.width <= container.x + container.width + 1.0);
        }
    }

    // 7. Nested composite (deeply nested)
    #[test]
    fn test_nested_composite() {
        let inner_composite = State {
            name: "Middle".to_string(),
            id: "Middle".to_string(),
            description: vec![],
            stereotype: None,
            children: vec![simple_state("Deep1"), simple_state("Deep2")],
            is_special: false,
            kind: crate::model::state::StateKind::default(),
            source_line: None,
            regions: vec![],
            explicit_source_line: None,
        };

        let d = StateDiagram {
            states: vec![State {
                name: "Outer".to_string(),
                id: "Outer".to_string(),
                description: vec![],
                stereotype: None,
                children: vec![inner_composite, simple_state("Sibling")],
                is_special: false,
                source_line: None,
                kind: crate::model::state::StateKind::default(),
                regions: vec![],
                explicit_source_line: None,
            }],
            transitions: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();

        let outer = &layout.state_layouts[0];
        assert!(outer.is_composite);
        assert_eq!(outer.children.len(), 2);

        let middle = &outer.children[0];
        assert!(middle.is_composite);
        assert_eq!(middle.children.len(), 2);

        // Deep children should have absolute positions inside outer
        for deep in &middle.children {
            assert!(
                deep.x >= outer.x,
                "deep child x={} should be >= outer x={}",
                deep.x,
                outer.x
            );
            assert!(
                deep.y >= outer.y,
                "deep child y={} should be >= outer y={}",
                deep.y,
                outer.y
            );
        }
    }

    // 8. Transitions connect correct positions
    #[test]
    fn test_transition_points() {
        let d = StateDiagram {
            states: vec![simple_state("A"), simple_state("B")],
            transitions: vec![transition("A", "B", "go")],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();
        assert_eq!(layout.transition_layouts.len(), 1);

        let tl = &layout.transition_layouts[0];
        assert_eq!(tl.from_id, "A");
        assert_eq!(tl.to_id, "B");
        assert_eq!(tl.label, "go");
        // Graphviz returns Bezier control points (typically 4+ points for a cubic)
        assert!(!tl.points.is_empty(), "should have at least some points");

        // With graphviz, the first point should be above the last (vertical layout)
        let (_, from_y) = tl.points[0];
        let (_, to_y) = *tl.points.last().unwrap();
        assert!(from_y < to_y, "from_y={} should be < to_y={}", from_y, to_y);
    }

    #[test]
    fn test_composite_internal_transition_is_synthesized() {
        let d = StateDiagram {
            states: vec![State {
                name: "Active".to_string(),
                id: "Active".to_string(),
                description: vec![],
                stereotype: None,
                children: vec![
                    State {
                        name: "[*]".to_string(),
                        id: "[*]Active".to_string(),
                        description: vec![],
                        stereotype: None,
                        children: vec![],
                        is_special: true,
                        kind: StateKind::Normal,
                        regions: vec![],
                        source_line: Some(4),
                        explicit_source_line: None,
                    },
                    simple_state("Running"),
                ],
                is_special: false,
                kind: StateKind::Normal,
                regions: vec![],
                source_line: Some(3),
                explicit_source_line: None,
            }],
            transitions: vec![transition("[*]Active", "Running", "")],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();
        let tr = layout
            .transition_layouts
            .iter()
            .find(|tr| tr.from_id == "[*]Active" && tr.to_id == "Running")
            .expect("missing composite-internal transition");
        assert!(
            !tr.points.is_empty() || tr.raw_path_d.is_some(),
            "composite-internal transition should have visible geometry"
        );
    }

    // 9. Notes layout
    #[test]
    fn test_notes() {
        let d = StateDiagram {
            states: vec![simple_state("A")],
            transitions: vec![],
            notes: vec![
                StateNote {
                    alias: None,
                    entity_id: None,
                    text: "first note".to_string(),
                    position: "right".to_string(),
                    target: None,
                    source_line: None,
                },
                StateNote {
                    alias: Some("n1".to_string()),
                    entity_id: Some("n1".to_string()),
                    text: "second note\nwith two lines".to_string(),
                    position: "right".to_string(),
                    target: None,
                    source_line: None,
                },
            ],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();
        assert_eq!(layout.note_layouts.len(), 2);

        let n0 = &layout.note_layouts[0];
        let n1 = &layout.note_layouts[1];
        assert_eq!(n0.text, "first note");
        assert_eq!(n1.text, "second note\nwith two lines");

        // Notes should be to the right of the state
        let state_right = layout.state_layouts[0].x + layout.state_layouts[0].width;
        assert!(
            n0.x > state_right,
            "note x={} should be > state right={}",
            n0.x,
            state_right
        );

        // Second note should be below the first
        assert!(n1.y > n0.y);
    }

    #[test]
    fn test_attached_side_notes_anchor_to_target_state() {
        let d = StateDiagram {
            states: vec![simple_state("Active"), simple_state("Inactive")],
            transitions: vec![],
            notes: vec![
                StateNote {
                    alias: None,
                    entity_id: Some("GMN2".to_string()),
                    text: "This is active".to_string(),
                    position: "right".to_string(),
                    target: Some("Active".to_string()),
                    source_line: Some(3),
                },
                StateNote {
                    alias: None,
                    entity_id: Some("GMN3".to_string()),
                    text: "Multi line\nnote text".to_string(),
                    position: "left".to_string(),
                    target: Some("Inactive".to_string()),
                    source_line: Some(4),
                },
            ],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();

        let active = layout
            .state_layouts
            .iter()
            .find(|node| node.id == "Active")
            .unwrap();
        let inactive = layout
            .state_layouts
            .iter()
            .find(|node| node.id == "Inactive")
            .unwrap();
        let right_note = layout
            .note_layouts
            .iter()
            .find(|note| note.position == "right")
            .unwrap();
        let left_note = layout
            .note_layouts
            .iter()
            .find(|note| note.position == "left")
            .unwrap();

        assert!(right_note.x > active.x + active.width);
        assert!(left_note.x + left_note.width < inactive.x);
        // Note positions are determined by graphviz layout; allow small tolerance.
        assert!(
            (right_note.y + right_note.height / 2.0 - (active.y + active.height / 2.0)).abs() < 1.0
        );
        assert!(
            (left_note.y + left_note.height / 2.0 - (inactive.y + inactive.height / 2.0)).abs()
                < 1.0
        );
        // Anchor coordinates come from graphviz edge endpoints which clip to the
        // target polygon boundary. Verify they are close to the expected boundary,
        // allowing small tolerance from graphviz polygon clipping.
        let (ra_x, ra_y) = right_note.anchor.unwrap();
        assert!(
            (ra_x - (active.x + active.width)).abs() < 1.0,
            "right anchor x={ra_x} expected near {}",
            active.x + active.width
        );
        assert!(
            (ra_y - (active.y + active.height / 2.0)).abs() < 1.0,
            "right anchor y={ra_y} expected near {}",
            active.y + active.height / 2.0
        );
        let (la_x, la_y) = left_note.anchor.unwrap();
        assert!(
            (la_x - inactive.x).abs() < 1.0,
            "left anchor x={la_x} expected near {}",
            inactive.x
        );
        assert!(
            (la_y - (inactive.y + inactive.height / 2.0)).abs() < 1.0,
            "left anchor y={la_y} expected near {}",
            inactive.y + inactive.height / 2.0
        );
    }

    // 10. Text sizing for states with descriptions
    #[test]
    fn test_description_state_sizing() {
        let d = StateDiagram {
            states: vec![State {
                name: "Described".to_string(),
                id: "Described".to_string(),
                description: vec![
                    "line one".to_string(),
                    "a much longer description line".to_string(),
                    "line three".to_string(),
                ],
                stereotype: None,
                children: vec![],
                source_line: None,
                is_special: false,
                kind: crate::model::state::StateKind::default(),
                regions: vec![],
                explicit_source_line: None,
            }],
            transitions: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();
        let node = &layout.state_layouts[0];

        // Width should accommodate the longest description line
        let expected_min_w = crate::font_metrics::text_width(
            "a much longer description line",
            "SansSerif",
            STATE_DESC_FONT_SIZE,
            false,
            false,
        ) + 2.0 * PADDING;
        assert!(
            node.width >= expected_min_w,
            "width {} should be >= {}",
            node.width,
            expected_min_w
        );

        // Height should accommodate name (14pt) + 3 description lines (12pt)
        let name_h =
            crate::font_metrics::line_height("SansSerif", STATE_NAME_FONT_SIZE, false, false);
        let desc_h =
            crate::font_metrics::line_height("SansSerif", STATE_DESC_FONT_SIZE, false, false);
        let expected_min_h = name_h + 3.0 * desc_h + 2.0 * PADDING;
        assert!(
            node.height >= expected_min_h,
            "height {} should be >= {}",
            node.height,
            expected_min_h
        );

        // Descriptions should be preserved
        assert_eq!(node.description.len(), 3);
    }

    // 11. Implicit states (referenced but not declared)
    #[test]
    fn test_implicit_states() {
        let d = StateDiagram {
            states: vec![simple_state("A")],
            transitions: vec![transition("A", "B", "go")],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();

        // "B" is implicit — it should still appear in layouts
        assert_eq!(layout.state_layouts.len(), 2);
        let b = layout.state_layouts.iter().find(|n| n.id == "B");
        assert!(b.is_some(), "implicit state B should be in layout");
    }

    // 12. State with stereotype
    #[test]
    fn test_state_with_stereotype() {
        let d = StateDiagram {
            states: vec![State {
                name: "MyState".to_string(),
                id: "MyState".to_string(),
                description: vec![],
                stereotype: Some("<<inputPin>>".to_string()),
                source_line: None,
                children: vec![],
                is_special: false,
                kind: crate::model::state::StateKind::default(),
                regions: vec![],
                explicit_source_line: None,
            }],
            transitions: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();
        let node = &layout.state_layouts[0];
        assert_eq!(node.stereotype.as_deref(), Some("<<inputPin>>"));

        // Height should be taller than a state without stereotype
        let plain = StateDiagram {
            states: vec![simple_state("MyState")],
            transitions: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let plain_layout = layout_state(&plain).unwrap();
        assert!(
            node.height > plain_layout.state_layouts[0].height,
            "stereotype state should be taller"
        );
    }

    // 13. Multiple states ordered (graphviz places unconnected states on same rank)
    #[test]
    fn test_vertical_ordering() {
        // With transitions, graphviz places connected states on successive ranks
        let d = StateDiagram {
            states: vec![
                simple_state("First"),
                simple_state("Second"),
                simple_state("Third"),
            ],
            transitions: vec![
                transition("First", "Second", ""),
                transition("Second", "Third", ""),
            ],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();
        assert_eq!(layout.state_layouts.len(), 3);

        let y0 = layout.state_layouts[0].y;
        let y1 = layout.state_layouts[1].y;
        let y2 = layout.state_layouts[2].y;

        assert!(y0 < y1, "First ({}) should be above Second ({})", y0, y1);
        assert!(y1 < y2, "Second ({}) should be above Third ({})", y1, y2);
    }

    // 14. Empty composite state
    #[test]
    fn test_empty_composite() {
        let d = StateDiagram {
            states: vec![State {
                name: "Empty".to_string(),
                id: "Empty".to_string(),
                description: vec![],
                source_line: None,
                stereotype: None,
                children: vec![], // technically not composite since children is empty
                is_special: false,
                kind: crate::model::state::StateKind::default(),
                regions: vec![],
                explicit_source_line: None,
            }],
            transitions: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();
        assert_eq!(layout.state_layouts.len(), 1);
        assert!(!layout.state_layouts[0].is_composite);
    }

    // 15. Bounding box includes all elements
    #[test]
    fn test_bounding_box() {
        let d = StateDiagram {
            states: vec![simple_state("A"), simple_state("B")],
            transitions: vec![transition("A", "B", "")],
            notes: vec![StateNote {
                alias: None,
                entity_id: None,
                text: "a note".to_string(),
                position: "right".to_string(),
                target: None,
                source_line: None,
            }],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();

        // All state nodes should be within bounds
        for node in &layout.state_layouts {
            assert!(
                node.x + node.width <= layout.width,
                "state right edge {} exceeds width {}",
                node.x + node.width,
                layout.width
            );
            assert!(
                node.y + node.height <= layout.height,
                "state bottom edge {} exceeds height {}",
                node.y + node.height,
                layout.height
            );
        }

        // Notes should be within bounds
        for note in &layout.note_layouts {
            assert!(
                note.x + note.width <= layout.width,
                "note right edge {} exceeds width {}",
                note.x + note.width,
                layout.width
            );
        }
    }

    // 16. Special state defaults to initial when no transitions
    #[test]
    fn test_special_state_default_initial() {
        let d = StateDiagram {
            states: vec![special_state("[*]")],
            transitions: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_state(&d).unwrap();
        let node = &layout.state_layouts[0];
        assert!(node.is_initial, "unconnected [*] should default to initial");
    }

    // 17. LeftToRight direction
    #[test]
    fn test_left_to_right_direction() {
        use crate::model::diagram::Direction;
        let d = StateDiagram {
            states: vec![
                simple_state("First"),
                simple_state("Second"),
                simple_state("Third"),
            ],
            transitions: vec![
                transition("First", "Second", ""),
                transition("Second", "Third", ""),
            ],
            notes: vec![],
            direction: Direction::LeftToRight,
        };
        let layout = layout_state(&d).unwrap();

        // With LR direction, width should be > height
        assert!(
            layout.width > layout.height,
            "LR: width ({:.1}) should be > height ({:.1})",
            layout.width,
            layout.height
        );

        // Nodes should flow left-to-right: x positions should increase
        let x0 = layout.state_layouts[0].x;
        let x1 = layout.state_layouts[1].x;
        let x2 = layout.state_layouts[2].x;
        assert!(x0 < x1, "LR: First x ({:.1}) < Second x ({:.1})", x0, x1);
        assert!(x1 < x2, "LR: Second x ({:.1}) < Third x ({:.1})", x1, x2);
    }

    // 18. TB direction: height > width (requires transitions for vertical ordering)
    #[test]
    fn test_top_to_bottom_direction() {
        use crate::model::diagram::Direction;
        let d = StateDiagram {
            states: vec![
                simple_state("First"),
                simple_state("Second"),
                simple_state("Third"),
            ],
            transitions: vec![
                transition("First", "Second", ""),
                transition("Second", "Third", ""),
            ],
            notes: vec![],
            direction: Direction::TopToBottom,
        };
        let layout = layout_state(&d).unwrap();

        // With TB direction and connected states, height should be > width
        assert!(
            layout.height > layout.width,
            "TB: height ({:.1}) should be > width ({:.1})",
            layout.height,
            layout.width
        );
    }

    // 19. BottomToTop direction: first state at bottom
    #[test]
    fn test_bottom_to_top_direction() {
        use crate::model::diagram::Direction;
        let d = StateDiagram {
            states: vec![simple_state("First"), simple_state("Second")],
            transitions: vec![transition("First", "Second", "")],
            notes: vec![],
            direction: Direction::BottomToTop,
        };
        let layout = layout_state(&d).unwrap();

        // In BT direction, graphviz places First at bottom rank, Second at top
        let y0 = layout.state_layouts[0].y;
        let y1 = layout.state_layouts[1].y;
        assert!(
            y0 > y1,
            "BT: First y ({:.1}) should be > Second y ({:.1})",
            y0,
            y1
        );
    }
}
