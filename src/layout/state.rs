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
use crate::layout::unified::types::{Edge as LEdge, LayoutData, LayoutResult, Node as LNode};
use crate::layout::unified::render as unified_render;
use crate::model::state::{StateDiagram, StateKind};
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

const DEFAULT_NODE_SPACING: f64 = 50.0;
const DEFAULT_RANK_SPACING: f64 = 50.0;
const DEFAULT_LABEL_PAD_X: f64 = 8.0;
const DEFAULT_LABEL_PAD_Y: f64 = 8.0;
const DEFAULT_FONT_SIZE: f64 = 16.0;
const DEFAULT_LINE_HEIGHT: f64 = 1.5;

/// Public entry.
pub fn layout(d: &StateDiagram, theme: &ThemeVariables) -> Result<StateLayout> {
    let direction = d.direction.clone().unwrap_or_else(|| "TB".into());

    let mut data = LayoutData::default();
    data.diagram_type = Some(if d.is_v2 { "stateDiagram".into() } else { "stateDiagram".into() });
    data.direction = Some(direction.clone());
    data.node_spacing = Some(DEFAULT_NODE_SPACING);
    data.rank_spacing = Some(DEFAULT_RANK_SPACING);
    data.layout_algorithm = Some("dagre".into());
    data.markers.push("barbEnd".into());

    // Emit nodes.
    for state in &d.states {
        let mut n = LNode::default();
        n.id = state.id.clone();
        n.parent_id = state.parent.clone();
        n.dom_id = Some(state.id.clone());
        n.label = state.label.clone().or_else(|| Some(state.id.clone()));
        n.description = state.description.clone();
        match state.kind {
            StateKind::StartEnd => {
                // Shape picked by the renderer based on edge role; default
                // to start marker here (renderer may swap to state_end
                // when the node is only ever a transition target).
                n.shape = Some("stateStart".into());
                n.width = Some(14.0);
                n.height = Some(14.0);
                n.label = None;
            }
            StateKind::Fork | StateKind::Join => {
                n.shape = Some("forkJoin".into());
                // Bar is horizontal for TB/BT, vertical for LR/RL.
                let horizontal = matches!(direction.as_str(), "TB" | "BT");
                if horizontal {
                    n.width = Some(70.0);
                    n.height = Some(8.0);
                } else {
                    n.width = Some(8.0);
                    n.height = Some(70.0);
                }
                n.label = None;
            }
            StateKind::Choice => {
                n.shape = Some("choice".into());
                n.width = Some(30.0);
                n.height = Some(30.0);
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
                n.css_classes = Some("statediagram-cluster".into());
                n.padding = Some(8.0);
            }
            StateKind::Note => {
                n.shape = Some("note".into());
                let (w, h) = measure_label_box(
                    state.label.as_deref().unwrap_or(""),
                    DEFAULT_FONT_SIZE,
                );
                n.width = Some(w);
                n.height = Some(h);
            }
            StateKind::Divider => {
                n.shape = Some("basic".into());
                n.width = Some(0.0);
                n.height = Some(1.0);
                n.label = None;
                n.implicit_skip_render(true);
            }
            StateKind::Simple => {
                n.shape = Some("state".into());
                let label = state.label.as_deref().unwrap_or(&state.id);
                let mut lines: Vec<&str> = vec![label];
                if let Some(desc) = state.description.as_ref() {
                    for l in desc {
                        lines.push(l.as_str());
                    }
                }
                let (w, h) = measure_lines_box(&lines, DEFAULT_FONT_SIZE);
                n.width = Some(w);
                n.height = Some(h);
                n.label_padding_x = Some(DEFAULT_LABEL_PAD_X);
                n.label_padding_y = Some(DEFAULT_LABEL_PAD_Y);
                n.rx = Some(5.0);
                n.ry = Some(5.0);
            }
        }
        if n.css_classes.is_none() {
            n.css_classes = Some("statediagram-state".into());
        }
        data.nodes.push(n);
    }

    // Emit edges (transitions).
    for (i, t) in d.transitions.iter().enumerate() {
        let mut e = LEdge::default();
        e.id = format!("edge{}", i);
        e.start = Some(t.source.clone());
        e.end = Some(t.target.clone());
        e.arrowhead = Some("barbEnd".into());
        e.arrow_type_end = Some("barbEnd".into());
        e.classes = Some("transition".into());
        e.curve = Some("basis".into());
        e.thickness = Some("normal".into());
        e.pattern = Some("solid".into());
        if let Some(lines) = &t.label {
            e.label = Some(lines.join("\n"));
        }
        data.edges.push(e);
    }

    // Notes: emit as extra nodes on the same composite level as target
    // + a dotted edge connecting them. Layout-wise they share geometry
    // machinery with regular nodes.
    for (ni, note) in d.notes.iter().enumerate() {
        let nid = format!("note{}", ni);
        let mut n = LNode::default();
        n.id = nid.clone();
        n.shape = Some("note".into());
        n.css_classes = Some("statediagram-note".into());
        n.label = Some(note.text.clone());
        let (w, h) = measure_label_box(&note.text, DEFAULT_FONT_SIZE);
        n.width = Some(w);
        n.height = Some(h);
        // Parent it next to the target so dagre keeps them close.
        if let Some(target) = d.states.iter().find(|s| s.id == note.target) {
            n.parent_id = target.parent.clone();
        }
        data.nodes.push(n);

        let mut e = LEdge::default();
        e.id = format!("note-edge{}", ni);
        e.start = Some(note.target.clone());
        e.end = Some(nid);
        e.classes = Some("note-edge".into());
        e.pattern = Some("dashed".into());
        data.edges.push(e);
    }

    // Dagre-rs currently panics on some compound (nested composite
    // state) graphs — see `tests/ext_fixtures/cypress/state/{20,21,..}`.
    // Wrap the call in `catch_unwind` so the caller sees a clean
    // `MermaidError::Render` rather than an abort. Once dagre-rs is
    // patched this guard can go.
    let data_boxed = &data;
    let theme_boxed = theme;
    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        unified_render::layout(data_boxed, "dagre", theme_boxed)
    })) {
        Ok(r) => r?,
        Err(_) => {
            return Err(crate::error::MermaidError::Render(
                "dagre panic during state layout (compound graph edge case)".into(),
            ));
        }
    };

    Ok(StateLayout {
        result,
        direction,
        is_v2: d.is_v2,
    })
}

/// Crude label-box measurement: width = longest line × 8px + padding;
/// height = lines × lineheight × fontsize + padding. A full-fidelity
/// measurer would use `crate::font_metrics::DejaVu` per glyph; this
/// approximation is enough for dagre to rank the graph without
/// overlap. Renderer computes its own precise width for SVG emission.
fn measure_label_box(text: &str, font_size: f64) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    measure_lines_box(&lines.iter().copied().collect::<Vec<_>>(), font_size)
}

fn measure_lines_box(lines: &[&str], font_size: f64) -> (f64, f64) {
    let max_chars = lines.iter().map(|l| visual_width(l)).max().unwrap_or(0);
    let w = (max_chars as f64) * (font_size * 0.5) + 2.0 * DEFAULT_LABEL_PAD_X;
    let lines_n = lines.len().max(1) as f64;
    let h = lines_n * font_size * DEFAULT_LINE_HEIGHT + 2.0 * DEFAULT_LABEL_PAD_Y;
    (w.max(40.0), h.max(20.0))
}

fn visual_width(s: &str) -> usize {
    s.chars().count()
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
        assert!(l.result.nodes.iter().any(|n| n.id == "Parent" && n.is_group));
    }
}
