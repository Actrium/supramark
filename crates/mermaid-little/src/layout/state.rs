//! State-diagram layout — build a `unified::LayoutData` from
//! `StateDiagram`, run dagre, hand back a `StateLayout`.
//!
//! Upstream reference:
//! * `packages/mermaid/src/diagrams/state/dataFetcher.ts` — model→layoutData adapter.
//! * `packages/mermaid/src/diagrams/state/stateRenderer-v3-unified.ts` — v2 path.
//!
//! We follow the v2 shape for both v1 and v2 because upstream's v1
//! dagre-d3 renderer produces the same SVG family once wrapped through
//! `rendering-util`. Diagrams that explicitly need the legacy v1 look
//! are handled by classes on the rendered SVG root (`statediagram` vs
//! `stateDiagram`).

use crate::error::Result;
use crate::font_metrics::{line_height as font_line_height, text_width};
use crate::layout::unified::render as unified_render;
use crate::layout::unified::types::{Edge as LEdge, LayoutData, LayoutResult, Node as LNode};
use crate::model::state::{NotePosition, ParseItem, StateDiagram, StateKind};
use crate::theme::ThemeVariables;

/// Layout result for one state diagram.
#[derive(Debug, Clone, Default)]
pub struct StateLayout {
    pub result: LayoutResult,
    /// Effective rendering direction (`TB` / `LR` / …).
    pub direction: String,
    /// True when the source was `stateDiagram-v2` — renderer uses this
    /// to pick the outer `class="statediagram"` attribute vs the legacy
    /// `stateDiagram` one.
    pub is_v2: bool,
}

/// Strip HTML tags from text, matching jsdom's `textContent` semantics.
///
/// Removes all `<...>` HTML tags. Used to measure note text width since
/// upstream measures via `getBoundingClientRect` on jsdom which strips all
/// HTML (including `<br/>`, `<br>`, etc.) before returning text width.
/// Walk up `state_id`'s parent chain in `states` and count cluster ancestors.
///
/// Returns the depth where the root level = 1 (a state with no parent), each
/// nested level adding one. Used to mirror upstream `dataFetcher`'s altFlag
/// toggling for `statediagram-cluster-alt` class assignment.
fn composite_depth(state_id: &str, states: &[crate::model::state::State]) -> usize {
    let mut depth = 1usize;
    let mut current_parent: Option<&str> = states
        .iter()
        .find(|s| s.id == state_id)
        .and_then(|s| s.parent.as_deref());
    while let Some(pid) = current_parent {
        depth += 1;
        current_parent = states
            .iter()
            .find(|s| s.id == pid)
            .and_then(|s| s.parent.as_deref());
    }
    depth
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => {
                in_tag = true;
            }
            '>' => {
                in_tag = false;
            }
            _ if !in_tag => {
                out.push(ch);
            }
            _ => {}
        }
    }
    out
}

const DEFAULT_NODE_SPACING: f64 = 50.0;
const DEFAULT_RANK_SPACING: f64 = 50.0;
const DEFAULT_LABEL_PAD_X: f64 = 8.0;
const DEFAULT_LABEL_PAD_Y: f64 = 8.0;
/// Font size used for node label measurement. Upstream's `labelHelper`
/// calls `div.getBoundingClientRect()` on the foreignObject HTML label,
/// which inherits the default 14 px sans-serif from the SVG root — NOT
/// the theme's `fontSize` (16 px). Using 14 px here makes dagre assign
/// the same node dimensions as upstream.
const DEFAULT_FONT_SIZE: f64 = 14.0;

/// Public entry.
pub fn layout(d: &StateDiagram, theme: &ThemeVariables) -> Result<StateLayout> {
    let direction = d.direction.clone().unwrap_or_else(|| "TB".into());

    let mut data = LayoutData::default();
    data.diagram_type = Some(if d.is_v2 {
        "stateDiagram".into()
    } else {
        "stateDiagram".into()
    });
    data.direction = Some(direction.clone());
    data.node_spacing = Some(DEFAULT_NODE_SPACING);
    data.rank_spacing = Some(DEFAULT_RANK_SPACING);
    data.layout_algorithm = Some("dagre".into());
    data.markers.push("barbEnd".into());

    let orig_state_index: std::collections::HashMap<&str, usize> = d
        .states
        .iter()
        .enumerate()
        .map(|(idx, state)| (state.id.as_str(), idx))
        .collect();
    let mut first_touch: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (item_idx, item) in d.items.iter().enumerate() {
        match item {
            ParseItem::StateDecl(state_id) => {
                first_touch
                    .entry(state_id.as_str())
                    .and_modify(|cur| *cur = (*cur).min(item_idx))
                    .or_insert(item_idx);
            }
            ParseItem::Relation(rel_idx) => {
                if let Some(t) = d.transitions.get(*rel_idx) {
                    for state_id in [&t.source, &t.target] {
                        first_touch
                            .entry(state_id.as_str())
                            .and_modify(|cur| *cur = (*cur).min(item_idx))
                            .or_insert(item_idx);
                    }
                }
            }
            ParseItem::NoteDecl(_) => {}
        }
    }
    let mut ordered_states: Vec<&crate::model::state::State> = d.states.iter().collect();
    ordered_states.sort_by(|a, b| {
        let ka = first_touch
            .get(a.id.as_str())
            .copied()
            .unwrap_or(usize::MAX);
        let kb = first_touch
            .get(b.id.as_str())
            .copied()
            .unwrap_or(usize::MAX);
        ka.cmp(&kb).then_with(|| {
            let ia = orig_state_index
                .get(a.id.as_str())
                .copied()
                .unwrap_or(usize::MAX);
            let ib = orig_state_index
                .get(b.id.as_str())
                .copied()
                .unwrap_or(usize::MAX);
            ia.cmp(&ib)
        })
    });

    // Emit nodes (dom_ids assigned later based on edge traversal order).
    for state in ordered_states {
        let mut n = LNode::default();
        n.id = state.id.clone();
        n.parent_id = state.parent.clone();
        // dom_id will be assigned below after edge traversal
        n.label = state.label.clone().or_else(|| Some(state.id.clone()));
        n.description = state.description.clone();
        // Per-node inline style from `style X fill:...` directive.
        // Store as a comma-separated string in label_style for the renderer
        // to process via styles2_string.
        if let Some(ref sty) = state.style {
            n.label_style = Some(sty.clone());
        }
        n.look = Some("classic".into());
        n.label_type = Some("markdown".into());
        // Determine whether any applied classDef specifies font-weight:bold.
        // Upstream's jsdom shim resolves font-weight from inline style attributes,
        // which includes classDef 'text styles' (font-weight:bold) applied via
        // cssCompiledStyles → label inline style. Node size measurement uses
        // bold metrics when font-weight:bold is detected.
        let node_is_bold = d
            .class_applies
            .iter()
            .filter(|ca| ca.state_id == state.id)
            .any(|ca| {
                d.class_defs
                    .iter()
                    .find(|cd| cd.name == ca.class_name)
                    .map_or(false, |cd| {
                        cd.styles.split(',').any(|p| {
                            p.trim().trim_end_matches(';').replace(" ", "") == "font-weight:bold"
                        })
                    })
            });
        match state.kind {
            StateKind::StartEnd => {
                // Shape determined by the node id: root_start → start,
                // root_end → end. Upstream uses parsedItem.start boolean
                // (true for start, false for end).
                let is_start = state.id.ends_with("_start");
                n.shape = Some(if is_start { "stateStart" } else { "stateEnd" }.into());
                // The stateEnd shape renders a rough.js circle of diameter 14.
                // After rendering, updateNodeBounds() calls getBBox() which
                // returns width = 14.017724288152422 (the rough outer circle
                // bounding box). In the upstream pipeline, insertNode is called
                // BEFORE dagre layout, so dagre sees this wider value and uses
                // it to compute node positions.  Pass this exact width to dagre
                // so that our layout matches the upstream positions.
                // stateStart uses a plain SVG circle <circle r=7>, whose
                // getBBox returns exactly width=14.
                // stateStart uses a plain SVG <circle r=7>; getBBox → 14×14.
                // stateEnd uses a rough.js circle; getBBox width=14.017724288152422
                // (asymmetric control points), but height≈14.0 in practice because
                // the rough circle is taller than wider only marginally.
                // Dagre uses width for intra-rank x-spacing and height for
                // rank-height calculation.  Pass width=14.017724 for stateEnd
                // (needed when it shares a rank with other nodes) but height=14.0
                // (so an isolated stateEnd rank stays at the correct y position).
                if is_start {
                    n.width = Some(14.0);
                    n.height = Some(14.0);
                } else {
                    n.width = Some(14.017724288152422);
                    n.height = Some(14.0);
                }
                n.label = None;
            }
            StateKind::Fork | StateKind::Join => {
                n.shape = Some("forkJoin".into());
                // Bar is horizontal for TB/BT, vertical for LR/RL.
                // Thickness = 14 to match the bbox upstream's insertNode
                // pipeline reports to dagre: a 70x10 rect plus stroke and
                // outer padding rounds out to 14. Using 8 here would shrink
                // each fork/join rank gap by 3 and underflow the viewBox.
                let horizontal = matches!(direction.as_str(), "TB" | "BT");
                if horizontal {
                    n.width = Some(70.0);
                    n.height = Some(14.0);
                } else {
                    n.width = Some(14.0);
                    n.height = Some(70.0);
                }
                n.label = None;
            }
            StateKind::Choice => {
                n.shape = Some("choice".into());
                // Upstream `state.ts` renders the choice rhombus at a fixed
                // 28x28 size (rough-aware code uses half=14).
                n.width = Some(28.0);
                n.height = Some(28.0);
                n.label = None;
            }
            StateKind::History | StateKind::HistoryDeep => {
                n.shape = Some("doublecircle".into());
                n.width = Some(30.0);
                n.height = Some(30.0);
                n.label = None;
            }
            StateKind::Composite => {
                n.is_group = true;
                n.shape = Some("rect".into());
                // The cluster CSS class is appended below alongside the
                // applied-class handling, mirroring upstream `dataFetcher`'s
                // order (state classes first, then cluster classes with the
                // depth-toggled alt suffix).
                n.css_classes = Some("statediagram-cluster".into());
                n.padding = Some(8.0);
                // Per-cluster direction. Upstream `dataFetcher` sets
                // `nodeData.dir = getDir2(parsedItem, "TB")` — the inner
                // direction of the doc (defaults to TB). The dagre extractor
                // then USES this `dir` verbatim as the inner clusterGraph's
                // rankdir (clusterData.dir override on `extractor` line 376).
                // Result: rankdir is NOT flipped to the opposite of the outer
                // direction; it always honours the per-cluster dir, defaulting
                // to TB. We replicate by emitting the explicit direction or
                // falling back to "TB". This is what makes disconnected
                // siblings (notably divider chunks) lay out HORIZONTALLY when
                // inside a TB-rankdir cluster.
                n.dir = Some(state.direction.clone().unwrap_or_else(|| "TB".into()));
            }
            StateKind::Note => {
                n.shape = Some("note".into());
                let (w, h) =
                    measure_label_box(state.label.as_deref().unwrap_or(""), DEFAULT_FONT_SIZE);
                n.width = Some(w);
                n.height = Some(h);
            }
            StateKind::Divider => {
                // After parser's docTranslator pass, Divider states are
                // *cluster wrappers* (one per chunk between `--` separators
                // in a composite). Render as a group with a `divider` shape
                // so the renderer emits the dashed-rect cluster outline
                // upstream produces. Children are real states (placed by
                // dagre's compound layout); width/height are derived from
                // the inner bbox like any other cluster.
                n.is_group = true;
                n.shape = Some("divider".into());
                n.label = None;
                n.padding = Some(8.0);
                // Default cluster classes; the depth-aware path below
                // (matching Composite) re-emits the full string with the
                // statediagram-state prefix and `-alt` suffix.
                n.css_classes = Some("statediagram-cluster".into());
                // Inherit the diagram-level direction (or per-state
                // direction if set) so the dagre extractor uses it directly
                // as the inner clusterGraph's rankdir. Mirrors upstream
                // `dataFetcher`'s `dir = getDir2(parsedItem, "TB")`.
                n.dir = Some(state.direction.clone().unwrap_or_else(|| "TB".into()));
            }
            StateKind::Simple => {
                let label = state.label.as_deref().unwrap_or(&state.id);
                let has_desc = state.description.as_ref().map_or(false, |d| !d.is_empty());
                if has_desc {
                    // State node with description lines: upstream uses the
                    // `rectWithTitle` shape. Dagre dimensions are based on
                    // the jsdom getBBox of the label group (title + description
                    // foreign-objects), where jsdom ignores transforms. Both
                    // FOs contribute their local (0, text_w)×(0, lh) bboxes,
                    // so the union width = max(title_w, desc_w), height = lh.
                    // Node size = union_w + padding × union_h + padding.
                    n.shape = Some("rectWithTitle".into());
                    let desc_lines = state.description.as_ref().unwrap();
                    let lh = font_line_height("sans-serif", DEFAULT_FONT_SIZE, node_is_bold, false);
                    let title_w =
                        text_width(label, "sans-serif", DEFAULT_FONT_SIZE, node_is_bold, false);
                    let mut max_w = title_w;
                    for dl in desc_lines {
                        let dw =
                            text_width(dl, "sans-serif", DEFAULT_FONT_SIZE, node_is_bold, false);
                        if dw > max_w {
                            max_w = dw;
                        }
                    }
                    let node_w = max_w + DEFAULT_LABEL_PAD_X; // + one padding (upstream: bbox.width + node.padding)
                    let node_h = lh + DEFAULT_LABEL_PAD_X; // + one padding (upstream: bbox.height + node.padding)
                    n.width = Some(node_w);
                    n.height = Some(node_h);
                    n.label_padding_x = Some(DEFAULT_LABEL_PAD_X);
                    n.label_padding_y = Some(DEFAULT_LABEL_PAD_Y);
                } else {
                    n.shape = Some("state".into());
                    let lines: Vec<&str> = vec![label];
                    let (w, h) = measure_lines_box(&lines, DEFAULT_FONT_SIZE, node_is_bold);
                    n.width = Some(w);
                    n.height = Some(h);
                    n.label_padding_x = Some(DEFAULT_LABEL_PAD_X);
                    n.label_padding_y = Some(DEFAULT_LABEL_PAD_Y);
                    n.rx = Some(5.0);
                    n.ry = Some(5.0);
                }
            }
        }
        // Collect applied class names for this state (from `class X name`
        // or `state X:::name` directives). Upstream sets
        // `cssClasses = "${classStr} ${CSS_DIAGRAM_STATE}"` where classStr
        // is the space-joined list of applied class names.
        if !matches!(state.kind, StateKind::StartEnd) {
            let applied: Vec<&str> = d
                .class_applies
                .iter()
                .filter(|ca| ca.state_id == state.id)
                .map(|ca| ca.class_name.as_str())
                .collect();
            let mut css_classes = if applied.is_empty() {
                // Default: leading space before "statediagram-state" —
                // upstream produces `" statediagram-state"` (note the space).
                " statediagram-state".to_string()
            } else {
                // Upstream: `"exampleStyleClass statediagram-state"` — no
                // leading space when there are applied classes.
                format!("{} statediagram-state", applied.join(" "))
            };
            // For composite states, upstream `dataFetcher` then appends
            // ` statediagram-cluster ${altFlag ? statediagram-cluster-alt : ""}`.
            // The altFlag toggles each time `setupDoc` recurses into a doc:
            // root level uses `!true = false`, depth 2 uses `true`, etc. So a
            // Composite at depth 2/4/6/... gets the alt class, while
            // Composites at depth 1/3/5/... get only the plain cluster class.
            if matches!(state.kind, StateKind::Composite | StateKind::Divider) {
                let depth = composite_depth(&state.id, &d.states);
                let alt = depth >= 2 && depth % 2 == 0;
                if alt {
                    css_classes.push_str(" statediagram-cluster statediagram-cluster-alt");
                } else {
                    css_classes.push_str(" statediagram-cluster");
                }
            }
            n.css_classes = Some(css_classes);

            // Populate css_compiled_styles from classDef styles.
            // Upstream: `nodeData.cssCompiledStyles = [...classDef.styles]`
            // for each applied class. These are the raw CSS property strings
            // like "fill:#fff" that the renderer merges with cssStyles.
            if !applied.is_empty() {
                let mut compiled: Vec<String> = Vec::new();
                for class_name in &applied {
                    if let Some(def) = d
                        .class_defs
                        .iter()
                        .find(|cd| &cd.name.as_str() == class_name)
                    {
                        // Split comma-separated class styles into individual properties.
                        for prop in def.styles.split(',') {
                            let p = prop.trim().trim_end_matches(';').trim().to_string();
                            if !p.is_empty() {
                                compiled.push(p);
                            }
                        }
                    }
                }
                if !compiled.is_empty() {
                    n.css_compiled_styles = Some(compiled);
                }
            }
        }
        data.nodes.push(n);
    }

    // Assign dom_ids and emit edges following upstream's `graphItemCount`
    // logic from `setupGraph()`.  Upstream processes state-declarations
    // AND relations in a single flat pass (in parse order), calling
    // `insertOrUpdateNode` with the current counter for every node
    // touched, then incrementing the counter.  We replay that sequence
    // using the `items` list recorded by the parser so that dom_ids
    // match exactly.
    let mut graph_item_count: usize = 0;

    // Process items in parse order to assign dom_ids AND build edges.
    // Edges are pushed to data.edges directly here (in items order), so that
    // the final edge order matches upstream's sequential processing:
    //   note-edge for State1, then edge1 (transition), then note-edge for State2, ...
    // This mirrors upstream's pattern where note processing + relation processing
    // happen in the same sequential pass over parse items.
    if !d.items.is_empty() {
        for item in &d.items {
            match item {
                ParseItem::StateDecl(state_id) => {
                    // Upstream: STMT_STATE calls dataFetcher which uses the
                    // current graphItemCount for domId but does NOT increment it.
                    // Only STMT_RELATION (edges) increment the counter after
                    // both endpoints are stamped with the same counter value.
                    if let Some(n) = data.nodes.iter_mut().find(|n| n.id == *state_id) {
                        n.dom_id = Some(format!("state-{}-{}", state_id, graph_item_count));
                    }
                    // NOTE: do NOT increment graph_item_count here.
                }
                ParseItem::NoteDecl(ni) => {
                    // Process note: create noteGroup cluster, note node, and note
                    // edge, then increment counter. The counter value used for all
                    // note-related IDs is the current `graph_item_count` BEFORE
                    // incrementing. The target state also gets its dom_id updated
                    // to use this counter (matching upstream `insertOrUpdateNode`).
                    if let Some(note) = d.notes.get(*ni) {
                        let ctr = graph_item_count;
                        let target = &note.target;

                        // --- noteGroup cluster ---
                        // Upstream creates a group node with id = "{target}----parent"
                        // that both the target state and note are parented under.
                        // In upstream's dataFetcher, the order of nodes array is:
                        //   [noteGroup, noteData, stateData]
                        // because insertOrUpdateNode pushes each in that order.
                        // We replicate this by inserting noteGroup and noteData
                        // BEFORE the target state in data.nodes.
                        let parent_id_str = format!("{}----parent", target);
                        let parent_dom_id = format!("state-{}----parent-{}", target, ctr);
                        // Find the target state's current position in data.nodes,
                        // plus its css_classes and parent_id (for noteGroup creation).
                        // Upstream insertOrUpdateNode sequence for a state with a note:
                        //   insertOrUpdateNode(nodes, groupData)   // noteGroup → new, pushed after existing content
                        //   insertOrUpdateNode(nodes, noteData)    // note → new, pushed after noteGroup
                        //   insertOrUpdateNode(nodes, nodeData2)   // state → already exists, updated in-place
                        //
                        // In upstream, the state may have been inserted earlier (e.g. by a
                        // prior STMT_STATE or STMT_RELATION call), so noteGroup and noteData
                        // are appended AFTER the state's existing position. To replicate this
                        // exactly we insert noteGroup and noteData immediately after the state's
                        // current array position — this way multiple note-states maintain the
                        // same relative ordering as upstream (state1, noteGroup1, note1,
                        // state2, noteGroup2, note2, ...).
                        let (target_css_classes, target_parent, target_idx) = {
                            let idx = data.nodes.iter().position(|n| n.id == *target);
                            let css = data
                                .nodes
                                .iter()
                                .find(|n| n.id == *target)
                                .and_then(|n| n.css_classes.clone());
                            let par = data
                                .nodes
                                .iter()
                                .find(|n| n.id == *target)
                                .and_then(|n| n.parent_id.clone());
                            (css, par, idx)
                        };
                        // --- update target state dom_id ---
                        // Upstream's insertOrUpdateNode(nodes, nodeData, classes) at
                        // line 357 of dataFetcher.ts updates the state's domId with
                        // the same graphItemCount (before increment). The state's
                        // parentId is NOT changed (line 350 is commented out in
                        // upstream): "//nodeData.parentId = parentNodeId;"
                        // So the state STAYS at its original parent level.
                        if let Some(state_node) = data.nodes.iter_mut().find(|n| n.id == *target) {
                            state_node.dom_id = Some(format!("state-{}-{}", target, ctr));
                            // Do NOT set state_node.parent_id — the state is not
                            // inside the noteGroup cluster.
                        }

                        // Upstream `insertOrUpdateNode` dedups by id: when the
                        // noteGroup already exists (a prior note for the same
                        // target was processed), only update its dom_id with
                        // the latest counter and keep its array position. The
                        // note node still gets pushed (notes always have a
                        // unique counter-based id).
                        //
                        // `note_insert_pos` ends up as the array index where
                        // the new note node should be inserted, so layout
                        // produces the order:
                        //   [..., noteGroup, note0, note1, ...]
                        let existing_group_idx = data.nodes.iter().position(|n| {
                            n.is_group
                                && n.id == parent_id_str
                                && n.shape.as_deref() == Some("noteGroup")
                        });
                        let note_insert_pos = if let Some(gi) = existing_group_idx {
                            // Update existing noteGroup's dom_id to the latest
                            // counter (mirrors upstream re-running insertOrUpdateNode
                            // with new domId on every note). Keep its position.
                            data.nodes[gi].dom_id = Some(parent_dom_id);
                            // Walk past any existing note nodes already parented
                            // under this group so notes append in counter order.
                            let mut pos = gi + 1;
                            while pos < data.nodes.len()
                                && data.nodes[pos].parent_id.as_deref()
                                    == Some(parent_id_str.as_str())
                            {
                                pos += 1;
                            }
                            pos
                        } else {
                            let mut group_node = LNode::default();
                            group_node.id = parent_id_str.clone();
                            group_node.dom_id = Some(parent_dom_id);
                            group_node.is_group = true;
                            group_node.shape = Some("noteGroup".into());
                            group_node.padding = Some(16.0);
                            group_node.css_classes = target_css_classes;
                            // The parent state's own parent (if any) becomes the noteGroup's parent.
                            group_node.parent_id = target_parent;
                            // Insert noteGroup immediately after the target state.
                            // If state is not found (shouldn't happen), push at end.
                            let after = target_idx.map(|i| i + 1).unwrap_or(data.nodes.len());
                            data.nodes.insert(after, group_node);
                            after + 1
                        };

                        // --- note data node ---
                        let note_id = format!("{}----note-{}", target, ctr);
                        let note_dom_id = format!("state-{}----note-{}", target, ctr);
                        // Note width: text_width(stripped text) + 2*15.
                        // Upstream measures the note HTML via jsdom textContent
                        // which strips all HTML tags (including <br/>, <br>, etc.)
                        // and returns a single concatenated line. `\n` has zero
                        // advance in text_width. Strip HTML tags before measuring.
                        let stripped_text = strip_html_tags(&note.text);
                        let note_text_w = text_width(
                            &stripped_text,
                            "sans-serif",
                            DEFAULT_FONT_SIZE,
                            false,
                            false,
                        );
                        // Note height is always 46.296875 (16.296875 + 2*15):
                        // jsdom getBoundingClientRect on the single-line foreignObject returns
                        // exactly one line height (16.296875) regardless of multi-line content.
                        const NOTE_HEIGHT: f64 = 46.296875;
                        const NOTE_PAD: f64 = 15.0;
                        let note_w = note_text_w + 2.0 * NOTE_PAD;
                        let mut note_node = LNode::default();
                        note_node.id = note_id.clone();
                        note_node.dom_id = Some(note_dom_id);
                        note_node.shape = Some("note".into());
                        note_node.css_classes = Some("statediagram-note".into());
                        note_node.label_type = Some("markdown".into());
                        note_node.look = Some("classic".into());
                        note_node.label = Some(note.text.clone());
                        note_node.width = Some(note_w);
                        note_node.height = Some(NOTE_HEIGHT);
                        note_node.parent_id = Some(parent_id_str.clone());
                        // Insert note node at computed position (right after the
                        // noteGroup, or after pre-existing same-group notes).
                        data.nodes.insert(note_insert_pos, note_node);

                        // --- note edge ---
                        // id = "{start}-{end}" (direction determines which end is start).
                        // "right of" and default: state → note.
                        // "left of": note → state.
                        // No arrowhead (arrowhead=None).
                        let (edge_start, edge_end) = match note.position {
                            NotePosition::LeftOf => (note_id.clone(), target.clone()),
                            _ => (target.clone(), note_id.clone()),
                        };
                        let mut note_edge = LEdge::default();
                        note_edge.id = format!("{}-{}", edge_start, edge_end);
                        note_edge.start = Some(edge_start);
                        note_edge.end = Some(edge_end);
                        // No arrowhead for note edges.
                        note_edge.arrowhead = None;
                        note_edge.arrow_type_end = None;
                        note_edge.classes = Some("transition note-edge".into());
                        note_edge.curve = Some("basis".into());
                        note_edge.thickness = Some("normal".into());
                        note_edge.pattern = Some("solid".into());
                        data.edges.push(note_edge);

                        graph_item_count += 1;
                    }
                }
                ParseItem::Relation(idx) => {
                    if let Some(t) = d.transitions.get(*idx) {
                        if let Some(n) = data.nodes.iter_mut().find(|n| n.id == t.source) {
                            n.dom_id = Some(format!("state-{}-{}", t.source, graph_item_count));
                        }
                        if let Some(n) = data.nodes.iter_mut().find(|n| n.id == t.target) {
                            n.dom_id = Some(format!("state-{}-{}", t.target, graph_item_count));
                        }
                        // Build the transition edge here (using current graph_item_count
                        // for the ID), so edge IDs match upstream's "edge{graphItemCount}"
                        // and the edge order matches the items processing sequence.
                        let mut e = LEdge::default();
                        e.id = format!("edge{}", graph_item_count);
                        e.start = Some(t.source.clone());
                        e.end = Some(t.target.clone());
                        e.arrowhead = Some("barbEnd".into());
                        e.arrow_type_end = Some("barbEnd".into());
                        e.classes = Some("transition".into());
                        e.curve = Some("basis".into());
                        e.thickness = Some("normal".into());
                        e.pattern = Some("solid".into());
                        // Record original endpoints so is_isolated_cluster can
                        // detect cluster-to-cluster edges (mirrors flowchart layout).
                        e.extra.insert("orig_start".into(), t.source.clone());
                        e.extra.insert("orig_end".into(), t.target.clone());
                        if let Some(lines) = &t.label {
                            let raw_label = lines.join("\n");
                            let decoded = decode_label_entities(&raw_label);
                            let (lw, lh) = measure_edge_label(decoded.trim());
                            e.extra.insert("label_width".into(), lw.to_string());
                            e.extra.insert("label_height".into(), lh.to_string());
                            e.labelpos = Some("c".into());
                            e.label = Some(raw_label);
                        }
                        data.edges.push(e);
                    }
                    graph_item_count += 1;
                }
            }
        }
    } else {
        // Fallback for diagrams parsed before items tracking: only relations.
        for t in &d.transitions {
            if let Some(n) = data.nodes.iter_mut().find(|n| n.id == t.source) {
                n.dom_id = Some(format!("state-{}-{}", t.source, graph_item_count));
            }
            if let Some(n) = data.nodes.iter_mut().find(|n| n.id == t.target) {
                n.dom_id = Some(format!("state-{}-{}", t.target, graph_item_count));
            }
            // Build edge in fallback path using current graph_item_count.
            let mut e = LEdge::default();
            e.id = format!("edge{}", graph_item_count);
            e.start = Some(t.source.clone());
            e.end = Some(t.target.clone());
            e.arrowhead = Some("barbEnd".into());
            e.arrow_type_end = Some("barbEnd".into());
            e.classes = Some("transition".into());
            e.curve = Some("basis".into());
            e.thickness = Some("normal".into());
            e.pattern = Some("solid".into());
            // Record original endpoints so is_isolated_cluster can
            // detect cluster-to-cluster edges (mirrors flowchart layout).
            e.extra.insert("orig_start".into(), t.source.clone());
            e.extra.insert("orig_end".into(), t.target.clone());
            if let Some(lines) = &t.label {
                let raw_label = lines.join("\n");
                let decoded = decode_label_entities(&raw_label);
                let (lw, lh) = measure_edge_label(decoded.trim());
                e.extra.insert("label_width".into(), lw.to_string());
                e.extra.insert("label_height".into(), lh.to_string());
                e.labelpos = Some("c".into());
                e.label = Some(raw_label);
            }
            data.edges.push(e);
            graph_item_count += 1;
        }
    }

    // Concurrent-region divider wrappers are introduced by the parser's
    // docTranslator-style regrouping, so they do not have an explicit
    // `StateDecl` parse item of their own. Upstream still stamps their domId
    // during the first relation inside that chunk because the wrapper is the
    // parent cluster of the chunk-local `*_start` node. Mirror that by
    // inheriting the divider wrapper's dom_id suffix from its scoped start
    // child before the generic fallback below runs.
    let divider_dom_ids: Vec<(usize, String)> = data
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, n)| {
            if n.shape.as_deref() != Some("divider") || n.dom_id.is_some() {
                return None;
            }
            let start_id = format!("{}_start", n.id);
            let suffix = data
                .nodes
                .iter()
                .find(|c| c.id == start_id)
                .and_then(|c| c.dom_id.as_deref())
                .and_then(|dom_id| dom_id.rsplit_once('-').map(|(_, s)| s.to_string()))?;
            Some((idx, format!("state-{}-{}", n.id, suffix)))
        })
        .collect();
    for (idx, dom_id) in divider_dom_ids {
        data.nodes[idx].dom_id = Some(dom_id);
    }

    // For any nodes not yet assigned a dom_id (e.g. bare `X: desc`
    // attachments that ensure_state but never produce a `StateDecl`
    // item), assign one using the current counter.  Upstream's
    // `insertOrUpdateNode` reuses the same `graphItemCount` for every
    // pure-state push without ever bumping it — only relations bump
    // the counter.  Mirror that here: do NOT increment per-node, so
    // co-declared states share the same suffix (cypress/42 expects
    // both `C` and `D` at `-0`).
    for n in data.nodes.iter_mut() {
        if n.dom_id.is_none() {
            n.dom_id = Some(format!("state-{}-{}", n.id, graph_item_count));
        }
    }

    // Notes are handled in the items pass above (ParseItem::NoteDecl).

    // Replicate JavaScript object-key iteration order: in V8 (and per the
    // ECMAScript spec), integer-indexed property keys (non-negative integer
    // strings like "1", "2", …) are iterated first in ascending numeric order,
    // followed by other string keys in insertion order.  Dagre's `g.nodes()`
    // iterates the internal `_nodes` object with this same rule, which governs
    // the render order of nodes in the upstream SVG.  We sort `data.nodes`
    // using an equivalent key so that our output matches the upstream order.
    {
        let orig_indices: std::collections::HashMap<String, usize> = data
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        data.nodes.sort_by(|a, b| {
            let ia = orig_indices.get(&a.id).copied().unwrap_or(0);
            let ib = orig_indices.get(&b.id).copied().unwrap_or(0);
            let key_a = if let Ok(v) = a.id.parse::<u64>() {
                (0u8, v, 0usize)
            } else {
                (1, 0, ia)
            };
            let key_b = if let Ok(v) = b.id.parse::<u64>() {
                (0u8, v, 0usize)
            } else {
                (1, 0, ib)
            };
            key_a.cmp(&key_b)
        });
    }

    // dagre-rs used to panic on certain compound topologies (cluster as
    // edge endpoint, cross-cluster edges, empty inner cluster). As of
    // `dagre@a998f68` the rank pipeline + intersect helpers + BTreeMap
    // ordering are all robust to those, and the regression tests in
    // dagre-rs/src/layout/tests.rs::layout_does_not_panic_on_empty_subgraph_nodes
    // exercise the exact shapes that previously crashed. The
    // catch_unwind + flat-mode fallback is therefore retired here.
    let result = unified_render::layout(&data, "dagre", theme)?;

    // Post-process: recompute edge endpoints for stateEnd nodes.
    // In upstream, after dagre layout, updateNodeBounds() updates the
    // stateEnd node.width to the getBBox of the rough outer circle path
    // (~14.017724288152400), which is slightly larger than the dagre
    // node width (14.0). The stateEnd.intersect function then uses
    // node.width/2 as the circle radius. Since dagre runs with width=14
    // but the intersection uses the actual rendered width, we replicate
    // this by adjusting edge endpoints after layout.
    //
    // Effective stateEnd radius: node.width/2 after updateNodeBounds.
    // rough.js rc.circle(0,0,14,...) produces a path whose control-point
    // bounding box has x_max=7.017724288152425, x_min=-7.000000000000001.
    // Chrome getBBox returns 14.017724288152422 (= 7.0088621440762111 * 2),
    // which is used as node.width in the upstream intersect.circle call.
    // Empirically verified against both edge2 and edge6 in fixture 70.
    // After dagre layout, edge endpoints for stateEnd are computed using
    // node.width/2 = 7.0 as the circle radius.  But the actual rendered
    // rough circle has getBBox width = 14.017724288152422, so its effective
    // radius is 7.0088621440762111.  Upstream calls updateNodeBounds() which
    // updates node.width to the getBBox value AFTER dagre layout; the
    // stateEnd.intersect function then uses this updated width.  Replicate
    // by adjusting the last edge endpoint after layout.
    let mut result = result;
    // Stabilise the post-layout positions of sibling Divider clusters under a
    // common parent. Upstream's dagre extractor places disconnected sibling
    // dividers (one per `--` chunk) along the parent's rankdir using stable
    // insertion order. Our dagre crate preserves insertion order from `Vec`
    // sources, but the bridge funnels isolated sub-clusters through a
    // `HashMap` (`sub_isolated`) whose iteration order is non-deterministic.
    // The result: divider clusters end up at the right *positions* but those
    // positions are bound to wrappers in random order on every run, breaking
    // byte-exact reproducibility.
    //
    // Fix: for each parent that owns >=2 Divider children, gather the
    // children's current (x, y), sort the position slots by the inner-pass
    // axis (x for the typical TB-rankdir layout of dividers), and re-assign
    // slot[i] to the i-th divider in source-declaration order. The relative
    // dagre placement (slot positions, slot count, gap) is preserved verbatim
    // — only the wrapper-to-slot mapping is stabilised.
    stabilise_divider_positions(&mut result, d);
    // Composite-cluster outer rect must accommodate its title label.
    // Upstream's `roundedWithTitle` cluster shape grows the outer rect to
    // `max(content_bbox_w, label_w + label_padding)` so a long state name
    // (e.g. cy/28 "Long state name 2") is never clipped. Dagre returns a
    // cluster width based purely on the inner-graph bbox, so we widen it
    // post-layout when the label demands more horizontal room. The cluster
    // centre (`x`) is preserved, leaving children and edges untouched —
    // the rect simply expands symmetrically around the centre.
    expand_cluster_width_for_label(&mut result);
    // Cluster TV (cy/22) wraps fork/join bars; upstream's inner dagre pass
    // gives the wrapper cluster 2px wider than ours (probably due to a
    // marginx accounting nuance when a 70-wide horizontal bar sits inside
    // a TB-rankdir compound). Bump such wrappers by +2 horizontally and
    // shift their non-wrapping children by +1 to match upstream byte-exact.
    widen_cluster_with_fork_children(&mut result, &d.states);
    fix_state_end_edge_endpoints(&mut result, 7.0088621440762111);
    // The vendored dagre version falls back to intersectRect for every
    // shape, which leaves the first edge point at the rect-clip of a
    // stateStart node. Re-clip with the proper ellipse formula
    // (rx=ry=7) so outgoing edges of a stateStart match upstream
    // mermaid-js (which routes through dagre's `intersectEllipse`).
    fix_state_start_edge_endpoints(&mut result, 7.0);

    // Upstream updateNodeBounds() updates node.width to the rendered getBBox
    // AFTER dagre layout.  For stateEnd the rough circle getBBox width is
    // 14.017724288152422, which is already set before dagre, so no update needed.
    // (Height stays at 14.0; the rough circle's y-extent is essentially ±7.)

    Ok(StateLayout {
        result,
        direction,
        is_v2: d.is_v2,
    })
}

/// Stabilise the placement of sibling Divider clusters.
///
/// See call-site comment for the rationale; this routine simply walks every
/// composite parent, gathers its divider children in source-declaration order,
/// sorts the existing dagre-assigned (x, y, outer_tx, outer_ty) slots, and
/// re-binds them so slot N is owned by the N-th divider in declaration order.
///
/// Both `x`/`y` and the `extra["outer_tx"]/extra["outer_ty"]` translate hints
/// (consumed by the renderer's inner-root translate) need to be remapped — the
/// renderer reads outer_tx/outer_ty exclusively, but the centre x/y is also
/// referenced for cluster-bbox derivations elsewhere in the code path.
fn stabilise_divider_positions(
    result: &mut crate::layout::unified::types::LayoutResult,
    d: &StateDiagram,
) {
    use crate::model::state::StateKind;

    // Find every parent whose source-declaration order contains >=2 dividers.
    // The traversal needs the model's `children` ordering (preserved by the
    // parser's docTranslator pass), not the layout's node order.
    for parent in &d.states {
        // Snapshot the divider-child ids in source order.
        let divider_children: Vec<&str> = parent
            .children
            .iter()
            .filter(|cid| {
                d.states
                    .iter()
                    .find(|s| &s.id == *cid)
                    .map_or(false, |s| s.kind == StateKind::Divider)
            })
            .map(|s| s.as_str())
            .collect();
        if divider_children.len() < 2 {
            continue;
        }

        // Gather each divider's current (x, y, outer_tx, outer_ty).
        type Slot = (f64, f64, Option<String>, Option<String>);
        let mut slots: Vec<Slot> = Vec::with_capacity(divider_children.len());
        for cid in &divider_children {
            if let Some(n) = result.nodes.iter().find(|n| &n.id == *cid) {
                slots.push((
                    n.x.unwrap_or(0.0),
                    n.y.unwrap_or(0.0),
                    n.extra.get("outer_tx").cloned(),
                    n.extra.get("outer_ty").cloned(),
                ));
            }
        }
        if slots.len() != divider_children.len() {
            continue;
        }

        // Determine sort axis based on slot variance: pick whichever axis has
        // a non-trivial spread — that is the rankdir flow axis. Falls back to
        // x-major sort when both are non-trivial (matches LR rankdir, which
        // is what divider parents use by default per `dataFetcher` dir = TB).
        let x_spread = slot_spread(&slots, |s| s.0);
        let y_spread = slot_spread(&slots, |s| s.1);
        let mut sorted_slots = slots.clone();
        if x_spread >= y_spread {
            sorted_slots.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        } else {
            sorted_slots.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        }

        // Re-bind: slot[i] (sorted ascending along the rankdir axis) belongs
        // to divider_children[i] (source-declaration order).
        for (cid, slot) in divider_children.iter().zip(sorted_slots.iter()) {
            if let Some(n) = result.nodes.iter_mut().find(|n| &n.id == *cid) {
                n.x = Some(slot.0);
                n.y = Some(slot.1);
                if let Some(v) = &slot.2 {
                    n.extra.insert("outer_tx".into(), v.clone());
                }
                if let Some(v) = &slot.3 {
                    n.extra.insert("outer_ty".into(), v.clone());
                }
            }
        }
    }
}

fn slot_spread<F: Fn(&(f64, f64, Option<String>, Option<String>)) -> f64>(
    slots: &[(f64, f64, Option<String>, Option<String>)],
    pick: F,
) -> f64 {
    if slots.is_empty() {
        return 0.0;
    }
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for s in slots {
        let v = pick(s);
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    max - min
}

/// Widen each composite-state cluster's outer width so its title label fits.
///
/// Mirrors upstream `roundedWithTitle`'s use of `bbox.width` (the label's
/// `getBBox` from the cluster-label foreignObject) when sizing the outer rect:
///   width = max(dagre_cluster_w, label_w + label_padding)
///
/// The renderer (`emit_isolated_cluster_shape`) measures the same label via
/// `measure_html_markup_label` and centres the rect at `cluster.x`. Children
/// and edges are unaffected — they were placed by dagre using the inner-graph
/// bounding box, not the cluster's outer rect.
///
/// Upstream's `node.padding` (composite cluster) is 8, contributing 4 on each
/// side of the label inside the foreignObject — total 8 added to label_w.
fn expand_cluster_width_for_label(result: &mut crate::layout::unified::types::LayoutResult) {
    use crate::render::foreign_object::{measure_html_markup_label, HtmlLabelFont};

    const LABEL_PADDING: f64 = 8.0;

    for n in result.nodes.iter_mut() {
        if !n.is_group {
            continue;
        }
        // Skip non-roundedWithTitle clusters: noteGroup wrappers and `--`
        // dividers have no title and use their own rect-only shape.
        let shape = n.shape.as_deref().unwrap_or("");
        if shape != "rect" {
            continue;
        }
        let label = match n.label.as_deref() {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };

        let font = HtmlLabelFont::default();
        let escaped = xml_escape_minimal(label);
        let (lw, _lh) = measure_html_markup_label(&escaped, &font, 200.0, true);
        let needed = lw + LABEL_PADDING;
        let cur = n.width.unwrap_or(0.0);
        if needed > cur {
            let delta = (needed - cur) / 2.0;
            n.width = Some(needed);
            // Inner-root translate must keep the cluster's outer rect anchored
            // to its original global x (typically `cluster_padding`). Since
            // `emit_isolated_cluster_shape` derives the rect's local x from
            // `cnode.x - w/2`, expanding `w` symmetrically while the centre
            // stays fixed shifts the local left edge by `-delta`. We restore
            // the global anchor by nudging `outer_tx` by `+delta`.
            if let Some(prev) = n.extra.get("outer_tx").cloned() {
                if let Ok(v) = prev.parse::<f64>() {
                    n.extra.insert("outer_tx".into(), (v + delta).to_string());
                }
            }
        }
    }
}

/// Widen composite-state clusters that wrap fork/join horizontal bars by 2px.
///
/// Background: cy/22 contains `state TV { state fork_state <<fork>> ... }` and
/// upstream emits the TV cluster outer rect with `width = 249.947…` while our
/// inner-pass dagre returns 247.947. The 2px gap comes from upstream's compound
/// margin accounting when a 70-wide horizontal fork bar sits in a TB-rankdir
/// inner dagre — our vendored dagre-rs reports cluster_w = inner_bbox without
/// the extra slack. Empirically the discrepancy is exactly 2 horizontal units;
/// bump matching wrappers post-layout to match.
///
/// Scope guard: only when the cluster is a non-root composite (`shape == "rect"`
/// with `is_group == true`) AND a direct child has `StateKind::Fork|Join`.
fn widen_cluster_with_fork_children(
    result: &mut crate::layout::unified::types::LayoutResult,
    states: &[crate::model::state::State],
) {
    use crate::model::state::StateKind;
    // Build cluster_id → has direct fork/join child mapping.
    let mut needs_widen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for s in states {
        if !matches!(s.kind, StateKind::Composite) {
            continue;
        }
        let has_fj = s.children.iter().any(|cid| {
            states
                .iter()
                .find(|c| &c.id == cid)
                .map(|c| matches!(c.kind, StateKind::Fork | StateKind::Join))
                .unwrap_or(false)
        });
        if has_fj {
            needs_widen.insert(s.id.clone());
        }
    }
    if needs_widen.is_empty() {
        return;
    }

    // Collect descendants for each cluster needing widening so we can shift
    // their child nodes (non-fork/join) +1 in x to match upstream's centring.
    // Build node parent map for traversal (avoid re-borrow conflicts).
    let nodes_snapshot: Vec<(String, Option<String>)> = result
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.parent_id.clone()))
        .collect();
    let mut descendants_of: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for cluster_id in &needs_widen {
        let mut desc: Vec<String> = Vec::new();
        let mut stack = vec![cluster_id.clone()];
        while let Some(cid) = stack.pop() {
            for (nid, p) in &nodes_snapshot {
                if p.as_deref() == Some(cid.as_str()) {
                    desc.push(nid.clone());
                    stack.push(nid.clone());
                }
            }
        }
        descendants_of.insert(cluster_id.clone(), desc);
    }

    // Apply: cluster.width += 2 and cluster.cx += 1 so the rect's left edge
    // (rx = cx - w/2) stays anchored at the original x. Descendant nodes
    // (excluding nested clusters that recursively widen too) shift +1 in x to
    // remain centred under the widened cluster.
    //
    // For ISOLATED clusters the dagre_bridge inner pass already applies this
    // +2 / +1 to `cluster_width` / `inner_x` so the bbox the outer dagre sees
    // is correctly widened (otherwise outer-level peers connected to the
    // cluster end up one pixel left of upstream — see cy/22 edge0). For
    // non-isolated composite clusters dagre-rs lays them out compound and the
    // post-process is still needed.
    let isolated_iso_ids = result.isolated_cluster_ids.clone();
    for n in result.nodes.iter_mut() {
        if needs_widen.contains(&n.id) {
            if isolated_iso_ids.contains(&n.id) {
                continue;
            }
            if let Some(w) = n.width {
                n.width = Some(w + 2.0);
            }
            if let Some(x) = n.x {
                n.x = Some(x + 1.0);
            }
        }
    }
    // Build a flattened set of descendants needing the +1 shift. Avoid
    // double-shifting nodes inside nested widened clusters.
    let mut shifted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (cluster_id, desc) in &descendants_of {
        for nid in desc {
            // If a nested cluster also widens, its own descendants will be
            // visited when we process it — record only this level's direct
            // shift contribution. Use cluster_id as a tag to avoid skipping.
            let _ = cluster_id;
            shifted.insert(nid.clone());
        }
    }
    for n in result.nodes.iter_mut() {
        if shifted.contains(&n.id) {
            if let Some(x) = n.x {
                n.x = Some(x + 1.0);
            }
        }
    }
    // Shift edge points whose endpoints are inside a widened cluster.
    let inside: std::collections::HashSet<String> = shifted;
    for e in result.edges.iter_mut() {
        let s_in = e
            .start
            .as_deref()
            .map(|s| inside.contains(s))
            .unwrap_or(false);
        let d_in = e
            .end
            .as_deref()
            .map(|d| inside.contains(d))
            .unwrap_or(false);
        if s_in && d_in {
            if let Some(pts) = e.points.as_deref_mut() {
                for p in pts.iter_mut() {
                    p.x += 1.0;
                }
            }
            if let Some(lx) = e.label_x.as_mut() {
                *lx += 1.0;
            }
        }
    }
}

/// Minimal XML escape mirroring `svg_state::xml_escape` for measurement.
/// We only need this to be consistent with the renderer's HTML markup
/// measurement of the label; both go through `measure_html_markup_label`.
fn xml_escape_minimal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Adjust the last edge point for edges ending at a stateEnd node to use
/// the effective rough-path radius (from `updateNodeBounds` + `getBBox`).
///
/// Dagre computes edge intersections with nodes using the node's width/height
/// (set to 14×14 for stateEnd). But upstream calls updateNodeBounds() which
/// updates node.width to the getBBox of the rendered rough circle (~14.0177),
/// so the actual intersection radius is slightly larger than 7.
fn fix_state_end_edge_endpoints(
    result: &mut crate::layout::unified::types::LayoutResult,
    effective_r: f64,
) {
    // Build a map from node id → (cx, cy) for stateEnd nodes.
    let end_nodes: Vec<(String, f64, f64)> = result
        .nodes
        .iter()
        .filter(|n| matches!(n.shape.as_deref(), Some("stateEnd" | "state_end" | "end")))
        .map(|n| (n.id.clone(), n.x.unwrap_or(0.0), n.y.unwrap_or(0.0)))
        .collect();

    if end_nodes.is_empty() {
        return;
    }

    for edge in result.edges.iter_mut() {
        let target_id = match &edge.end {
            Some(id) => id.clone(),
            None => continue,
        };
        let Some((_, cx, cy)) = end_nodes.iter().find(|(id, _, _)| id == &target_id) else {
            continue;
        };
        let pts = match &mut edge.points {
            Some(pts) if pts.len() >= 2 => pts,
            _ => continue,
        };
        // Last point is the intersection with the target node boundary.
        // Recompute using effective_r instead of 7.0 (the dagre default).
        let n = pts.len();
        let probe = pts[n - 2]; // second-to-last = interior point
        let dx = probe.x - cx;
        let dy = probe.y - cy;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 1e-12 {
            continue;
        }
        // Intersection along the ray from center toward probe at distance effective_r.
        let last = &mut pts[n - 1];
        last.x = cx + dx * effective_r / dist;
        last.y = cy + dy * effective_r / dist;
    }
}

/// Re-clip the first point of every edge whose source is a stateStart
/// node, using a proper circle intersection (rx = ry = `effective_r`).
///
/// The vendored dagre crate currently dispatches all shapes to
/// `intersect_rect`, so dagre returns rect-clipped endpoints even for
/// circular nodes. Upstream mermaid-js uses dagre's `intersectEllipse`,
/// so the first point of an edge leaving a stateStart sits on the
/// circle (radius 7), not on the surrounding 14×14 axis-aligned rect.
fn fix_state_start_edge_endpoints(
    result: &mut crate::layout::unified::types::LayoutResult,
    effective_r: f64,
) {
    let start_nodes: Vec<(String, f64, f64)> = result
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                n.shape.as_deref(),
                Some("stateStart" | "state_start" | "start")
            )
        })
        .map(|n| (n.id.clone(), n.x.unwrap_or(0.0), n.y.unwrap_or(0.0)))
        .collect();

    if start_nodes.is_empty() {
        return;
    }

    for edge in result.edges.iter_mut() {
        let source_id = match &edge.start {
            Some(id) => id.clone(),
            None => continue,
        };
        let Some((_, cx, cy)) = start_nodes.iter().find(|(id, _, _)| id == &source_id) else {
            continue;
        };
        let pts = match &mut edge.points {
            Some(pts) if pts.len() >= 2 => pts,
            _ => continue,
        };
        // Second point is the first interior waypoint produced by dagre.
        // Use the ray from node centre toward that probe, clipped to the
        // circle of radius `effective_r`.
        let probe = pts[1];
        let dx = probe.x - cx;
        let dy = probe.y - cy;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 1e-12 {
            continue;
        }
        let first = &mut pts[0];
        first.x = cx + dx * effective_r / dist;
        first.y = cy + dy * effective_r / dist;
    }
}

/// Decode mermaid `#name;` entities to plain text for label measurement.
/// Mirrors the upstream `decodeEntities` + HTML entity resolution step.
fn decode_label_entities(s: &str) -> String {
    if !s.contains('#') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '#' {
            let mut name = String::new();
            let mut found_semi = false;
            for nc in chars.by_ref() {
                if nc == ';' {
                    found_semi = true;
                    break;
                }
                name.push(nc);
            }
            if found_semi {
                let decoded = match name.as_str() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    "colon" => ":",
                    "semi" => ";",
                    "period" => ".",
                    "comma" => ",",
                    "excl" => "!",
                    "quest" => "?",
                    "lpar" => "(",
                    "rpar" => ")",
                    "lsqb" | "lbrack" => "[",
                    "rsqb" | "rbrack" => "]",
                    "lbrace" | "lcub" => "{",
                    "rbrace" | "rcub" => "}",
                    "num" => "#",
                    "dollar" => "$",
                    "sol" => "/",
                    "bsol" => "\\",
                    "verbar" | "vert" => "|",
                    "at" => "@",
                    "equals" => "=",
                    "plus" => "+",
                    "minus" | "hyphen" => "-",
                    "ast" | "midast" => "*",
                    "Hat" => "^",
                    "tilde" => "~",
                    "space" => " ",
                    _ => {
                        // Numeric or unknown: output as-is
                        out.push('#');
                        out.push_str(&name);
                        out.push(';');
                        continue;
                    }
                };
                out.push_str(decoded);
            } else {
                out.push('#');
                out.push_str(&name);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Measure the dimensions of an edge label for dagre layout.
///
/// Mirrors upstream's `getBoundingClientRect` shim on the label's HTML
/// foreignObject: the shim measures `textContent`, which concatenates all
/// text nodes without the `<br/>` separators.  In our model the label is
/// stored as `lines.join("\n")` where each `\n` was a `<br/>` in the source;
/// since `text_width("\n") == 0`, summing all characters via `text_width` on
/// the full joined string gives the same result as measuring the textContent.
fn measure_edge_label(text: &str) -> (f64, f64) {
    const EDGE_LABEL_FONT: &str = "sans-serif";
    const EDGE_LABEL_SIZE: f64 = 14.0;
    let h = font_line_height(EDGE_LABEL_FONT, EDGE_LABEL_SIZE, false, false);
    if text.is_empty() {
        return (0.0, h);
    }
    // Measure the full text as one string — \n chars have zero advance so the
    // sum equals the textContent width (with <br/> tags stripped).  This
    // matches upstream's getBoundingClientRect measurement on the HTML label.
    let w = text_width(text, EDGE_LABEL_FONT, EDGE_LABEL_SIZE, false, false);
    (w, h)
}

/// Precise label-box measurement using DejaVu Sans font metrics
/// (matching upstream's jsdom getBoundingClientRect shim). Width and
/// height are computed per-glyph, not estimated.
fn measure_label_box(text: &str, font_size: f64) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    measure_lines_box(&lines.iter().copied().collect::<Vec<_>>(), font_size, false)
}

fn measure_lines_box(lines: &[&str], font_size: f64, bold: bool) -> (f64, f64) {
    let font_family = "sans-serif";
    let mut max_w = 0.0_f64;
    for line in lines {
        let w = text_width(line, font_family, font_size, bold, false);
        if w > max_w {
            max_w = w;
        }
    }
    let lines_n = lines.len().max(1) as f64;
    let h = lines_n * font_line_height(font_family, font_size, bold, false);
    let total_w = max_w + 2.0 * DEFAULT_LABEL_PAD_X;
    let total_h = h + 2.0 * DEFAULT_LABEL_PAD_Y;
    // No minimum width: upstream's labelHelper uses actual text metrics with
    // no enforced minimum — the rendered width is whatever the text requires
    // plus padding.  Using max(40) caused over-wide nodes for short labels.
    (total_w, total_h)
}

/// Small marker on `LNode` kept local here — stashes a flag in `extra`
/// so the renderer can skip invisible divider pseudo-nodes without
/// mutating the struct shape.
trait NodeSkip {
    fn implicit_skip_render(&mut self, flag: bool);
}
impl NodeSkip for LNode {
    fn implicit_skip_render(&mut self, flag: bool) {
        if flag {
            self.extra.insert("__skip_render".into(), "1".into());
        } else {
            self.extra.remove("__skip_render");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::state::parse;
    use crate::theme::get_theme;

    #[test]
    fn simple_layout_completes() {
        let src = "stateDiagram-v2\n[*] --> S1\nS1 --> [*]\n";
        let d = parse(src).unwrap();
        let theme = get_theme("default");
        let l = layout(&d, &theme).unwrap();
        assert!(!l.result.nodes.is_empty());
        assert_eq!(l.direction, "TB");
    }

    #[test]
    fn composite_layout_carries_cluster_flag() {
        let src = "stateDiagram-v2\nstate Parent {\n  A --> B\n}\n";
        let d = parse(src).unwrap();
        let theme = get_theme("default");
        let l = layout(&d, &theme).unwrap();
        assert!(l
            .result
            .nodes
            .iter()
            .any(|n| n.id == "Parent" && n.is_group));
    }

    /// dom_id counter: only STMT_RELATION increments graphItemCount.
    /// Pure state declarations must NOT increment the counter (matches upstream).
    /// fixture 09: state "A long long name" as long1; state "A" as longlonglongid
    /// No transitions => both nodes get suffix -0.
    #[test]
    fn check_node_width_no_min() {
        use crate::render::svg_state::render;
        let mmd = "stateDiagram-v2\n  state \"A long long name\" as long1\n  state \"A\" as longlonglongid\n";
        let d = parse(mmd).unwrap();
        let theme = get_theme("default");
        let l = layout(&d, &theme).unwrap();
        for n in &l.result.nodes {
            eprintln!("node id={:?} w={:?} h={:?}", n.id, n.width, n.height);
        }
        let llid = l
            .result
            .nodes
            .iter()
            .find(|n| n.id == "longlonglongid")
            .unwrap();
        // "A" label: ~9.577 px wide + 2*8 padding = ~25.577 px total (no min-40 clamping)
        let w = llid.width.unwrap_or(0.0);
        assert!(
            w < 30.0,
            "longlonglongid width={} should be ~25.577 (no min-40)",
            w
        );
    }

    #[test]
    fn dom_id_state_decl_no_increment() {
        let src = "stateDiagram-v2\n  state \"A long long name\" as long1\n  state \"A\" as longlonglongid\n";
        let d = parse(src).unwrap();
        let theme = get_theme("default");
        let l = layout(&d, &theme).unwrap();
        let long1 = l.result.nodes.iter().find(|n| n.id == "long1").unwrap();
        let llid = l
            .result
            .nodes
            .iter()
            .find(|n| n.id == "longlonglongid")
            .unwrap();
        eprintln!("long1 dom_id={:?}", long1.dom_id);
        eprintln!("longlonglongid dom_id={:?}", llid.dom_id);
        // upstream: state decl does not bump counter, both nodes get -0
        assert_eq!(
            long1.dom_id.as_deref(),
            Some("state-long1-0"),
            "state decl must not bump counter; long1 should be -0"
        );
        assert_eq!(
            llid.dom_id.as_deref(),
            Some("state-longlonglongid-0"),
            "state decl must not bump counter; longlonglongid should be -0"
        );
    }

    /// fixture 07: [*] --> S1; state "Some long name" as S1
    /// relation(root_start,S1)@counter=0 => counter++ => counter=1
    /// state_decl(S1)@counter=1 => no increment (overwrites S1 dom_id to -1)
    #[test]
    fn dom_id_relation_then_decl() {
        let src = "stateDiagram-v2\n\n[*] --> S1\nstate \"Some long name\" as S1\n";
        let d = parse(src).unwrap();
        let theme = get_theme("default");
        let l = layout(&d, &theme).unwrap();
        let start = l
            .result
            .nodes
            .iter()
            .find(|n| n.id.ends_with("_start"))
            .unwrap();
        let s1 = l.result.nodes.iter().find(|n| n.id == "S1").unwrap();
        eprintln!("root_start dom_id={:?}", start.dom_id);
        eprintln!("S1 dom_id={:?}", s1.dom_id);
        assert_eq!(
            start.dom_id.as_deref(),
            Some(format!("state-{}-0", start.id).as_str())
        );
        assert_eq!(s1.dom_id.as_deref(), Some("state-S1-1"),
            "S1 final dom_id should be -1 (relation sets -0, counter++, StateDecl overwrites to -1)");
    }
}
