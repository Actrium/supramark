//! ER diagram layout — builds a `LayoutData` from the parsed
//! `ErDiagram`, runs the shared dagre bridge, and returns a tidy
//! `ErLayout` struct holding the positioned geometry plus a few
//! pre-computed text widths the renderer needs.
//!
//! Upstream flow (`erRenderer-unified.ts` + `rendering-util/render.ts`):
//!
//!   1. `erDb.getData()` turns entities into nodes and relationships into
//!      edges, with `shape: 'erBox'`, `label: alias|name`, etc.
//!   2. `render(data4Layout, svg)` goes to `rendering-util/render.ts` which
//!      picks the `dagre` layout, populates flowchart defaults from
//!      `config.er` (nodeSpacing=140, rankSpacing=80), then lets dagre
//!      decide x/y centres + edge points.
//!   3. The shape code in `erBox.ts` sizes the entity box based on the
//!      measured label width/height (for the no-attribute branch) using
//!      PADDING=20 (diagramPadding) so `width=labelW+40`, `height=labelH+60`.
//!
//! The test harness's jsdom font shim (`tests/support/generate_ref.mjs`)
//! always measures text as sans-serif 14 px (DejaVu Sans) because no
//! element in the ER output ever sets an explicit `font-size` attribute
//! (the CSS `font-size:16px` rule in the `<style>` block is
//! not consulted by the shim's `resolveFont` walker). We mirror that
//! here so the widths come out byte-exact.

use crate::error::Result;
use crate::font_metrics::{line_height, text_width};
use crate::layout::unified::render as unified_render;
use crate::layout::unified::types::{Edge, LayoutData, LayoutResult, Node};
use crate::model::er::{ErDiagram, Relationship};
use crate::theme::ThemeVariables;

/// Trebuchet/etc. CSS default in the reference SVG — kept as a constant
/// so the renderer emits the exact same string.
pub const LABEL_FONT_FAMILY: &str = "sans-serif";
/// `<style>` default in the reference SVG.
pub const LABEL_FONT_SIZE: f64 = 14.0;
/// `config.er.diagramPadding`.
pub const PADDING: f64 = 20.0;
/// `config.er.minEntityWidth`.
pub const MIN_ENTITY_WIDTH: f64 = 100.0;
/// `config.er.minEntityHeight` (unused in the no-attribute branch — retained for completeness).
pub const MIN_ENTITY_HEIGHT: f64 = 75.0;
/// `config.er.nodeSpacing`.
pub const NODE_SPACING: f64 = 140.0;
/// `config.er.rankSpacing`.
pub const RANK_SPACING: f64 = 80.0;

/// One laid-out entity — renderer just copies `x/y/width/height` out.
#[derive(Debug, Clone)]
pub struct EntityLayout {
    pub id: String,
    pub label: String,
    pub label_width: f64,
    pub label_height: f64,
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    pub css_classes: String,
    /// Whether this entity has attributes → needs the richer erBox path.
    pub has_attrs: bool,
}

/// One laid-out relationship (edge + label geometry).
#[derive(Debug, Clone)]
pub struct EdgeLayout {
    pub id: String,
    pub src: String,
    pub dst: String,
    pub label: String,
    pub label_width: f64,
    pub label_height: f64,
    /// `pattern` — "solid" | "dashed".
    pub pattern: &'static str,
    /// Upper-case cardinality name, e.g. `ZERO_OR_MORE`.
    pub card_a: String,
    pub card_b: String,
    /// Spline waypoints post-dagre.
    pub points: Vec<(f64, f64)>,
    /// Label center.
    pub label_x: f64,
    pub label_y: f64,
}

/// Output of the ER layout pass.
#[derive(Debug, Clone, Default)]
pub struct ErLayout {
    pub entities: Vec<EntityLayout>,
    pub edges: Vec<EdgeLayout>,
    /// Overall post-dagre bounds — used by the renderer to build the viewBox.
    pub bounds: (f64, f64, f64, f64),
    /// Layout direction (TB/BT/LR/RL).
    pub direction: String,
    /// Title anchor x (centre of pre-title bbox). `None` when there is
    /// no title.
    pub title_anchor_x: Option<f64>,
}

/// Measure a single line at sans-serif 14 px.
fn measure_width(text: &str) -> f64 {
    if text.is_empty() {
        0.0
    } else {
        text_width(text, LABEL_FONT_FAMILY, LABEL_FONT_SIZE, false, false)
    }
}

fn measure_label_height() -> f64 {
    line_height(LABEL_FONT_FAMILY, LABEL_FONT_SIZE, false, false)
}

/// Compute the no-attribute entity box dimensions.
/// * `width  = max(MIN_ENTITY_WIDTH, label_w + PADDING*2)`
/// * `height = label_h + PADDING*1.5*2`
fn entity_box_size(label_w: f64, label_h: f64) -> (f64, f64) {
    let w_contrib = label_w + PADDING * 2.0;
    let width = if w_contrib < MIN_ENTITY_WIDTH {
        MIN_ENTITY_WIDTH
    } else {
        w_contrib
    };
    let height = label_h + PADDING * 1.5 * 2.0;
    (width, height)
}

pub fn layout(d: &ErDiagram, theme: &ThemeVariables) -> Result<ErLayout> {
    // ── 1. Build unified LayoutData ─────────────────────────────────
    let mut data = LayoutData::default();
    data.direction = Some(d.direction.clone());
    data.node_spacing = Some(NODE_SPACING);
    data.rank_spacing = Some(RANK_SPACING);
    data.diagram_type = Some("er".to_string());
    data.layout_algorithm = Some("dagre".to_string());

    let label_h = measure_label_height();

    // Nodes (entities).
    for name in &d.entity_keys {
        let entity = &d.entities[name];
        let rendered_label = if !entity.alias.is_empty() {
            entity.alias.clone()
        } else {
            entity.label.clone()
        };
        let label_w = measure_width(&rendered_label);
        // For the no-attribute case we can pre-size the box here so dagre
        // routes around the real geometry. Attribute-bearing entities
        // need a richer measurement — handled as a partial for now.
        let (w, h) = entity_box_size(label_w, label_h);
        let mut n = Node::default();
        n.id = entity.id.clone();
        n.label = Some(rendered_label);
        n.shape = Some("erBox".to_string());
        n.width = Some(w);
        n.height = Some(h);
        n.css_classes = Some(entity.css_classes.clone());
        n.look = Some("classic".to_string());
        n.label_type = Some("markdown".to_string());
        data.nodes.push(n);
    }

    // Edges (relationships). Dagre needs a label width/height so it can
    // pack an edge-label rank row between entities.
    for (i, rel) in d.relationships.iter().enumerate() {
        let label_w = measure_width(&rel.role_a);
        let mut e = Edge::default();
        e.id = edge_id(rel, i);
        e.source = Some(rel.entity_a.clone());
        e.target = Some(rel.entity_b.clone());
        e.start = Some(rel.entity_a.clone());
        e.end = Some(rel.entity_b.clone());
        e.label = Some(rel.role_a.clone());
        e.label_type = Some("markdown".to_string());
        e.arrow_type_end = Some(rel.card_a.as_lower());
        e.arrow_type_start = Some(rel.card_b.as_lower());
        e.pattern = Some(rel.rel_type.edge_pattern().to_string());
        e.curve = Some("basis".to_string());
        e.classes = Some("relationshipLine".to_string());
        e.thickness = Some("normal".to_string());
        e.labelpos = Some("c".to_string());
        e.look = Some("classic".to_string());
        // The dagre edge-label packing reads width/height from the edge
        // label meta; populating via the unified `extra` map keeps this
        // simple without mutating dagre_bridge.
        e.extra.insert("label_width".into(), label_w.to_string());
        e.extra.insert("label_height".into(), label_h.to_string());
        data.edges.push(e);
    }

    // ── 2. Dagre layout ──────────────────────────────────────────────
    let result: LayoutResult = unified_render::layout(&data, "dagre", theme)?;

    // ── 3. Pack ErLayout ─────────────────────────────────────────────
    let mut out = ErLayout::default();
    out.direction = d.direction.clone();

    for (idx, name) in d.entity_keys.iter().enumerate() {
        let entity = &d.entities[name];
        let n = result
            .nodes
            .get(idx)
            .cloned()
            .unwrap_or_else(|| Node::default());
        let w = n.width.unwrap_or(0.0);
        let h = n.height.unwrap_or(0.0);
        let x = n.x.unwrap_or(0.0);
        let y = n.y.unwrap_or(0.0);
        let rendered_label = if !entity.alias.is_empty() {
            entity.alias.clone()
        } else {
            entity.label.clone()
        };
        let label_w = measure_width(&rendered_label);
        out.entities.push(EntityLayout {
            id: entity.id.clone(),
            label: rendered_label,
            label_width: label_w,
            label_height: label_h,
            width: w,
            height: h,
            x,
            y,
            css_classes: entity.css_classes.clone(),
            has_attrs: !entity.attributes.is_empty(),
        });
    }

    for (i, rel) in d.relationships.iter().enumerate() {
        let laid = result.edges.get(i).cloned().unwrap_or_default();
        let label_w = measure_width(&rel.role_a);
        let points = laid
            .points
            .as_ref()
            .map(|pts| pts.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>())
            .unwrap_or_default();
        out.edges.push(EdgeLayout {
            id: edge_id(rel, i),
            src: rel.entity_a.clone(),
            dst: rel.entity_b.clone(),
            label: rel.role_a.clone(),
            label_width: label_w,
            label_height: label_h,
            pattern: rel.rel_type.edge_pattern(),
            card_a: rel.card_a.as_upper().to_string(),
            card_b: rel.card_b.as_upper().to_string(),
            points,
            label_x: laid.label_x.unwrap_or(0.0),
            label_y: laid.label_y.unwrap_or(0.0),
        });
    }

    // Compute SVG bounds. This mirrors jsdom's getBBox shim used by the
    // reference generator — it IGNORES `transform` attributes and instead
    // unions every element's local coords. Concretely we take:
    //
    //   * entity `<rect>`s at local (-w/2, -h/2, w, h)
    //   * entity foreignObject labels at (0, 0, label_w, label_h)
    //   * edge paths using absolute waypoint coords (paths have no transform)
    //   * edge-label foreignObjects at (0, 0, label_w, label_h)
    //
    // Without the foreignObject contributions the right/bottom edges
    // collapse to the rect/path extents, producing a narrower viewBox
    // than upstream.
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let acc = |min_x: &mut f64, min_y: &mut f64, max_x: &mut f64, max_y: &mut f64, x: f64, y: f64, w: f64, h: f64| {
        *min_x = min_x.min(x);
        *min_y = min_y.min(y);
        *max_x = max_x.max(x + w);
        *max_y = max_y.max(y + h);
    };
    for e in &out.entities {
        // rect at local (-w/2, -h/2, w, h)
        acc(&mut min_x, &mut min_y, &mut max_x, &mut max_y,
            -e.width / 2.0, -e.height / 2.0, e.width, e.height);
        // FO at (0, 0, label_w, label_h)
        acc(&mut min_x, &mut min_y, &mut max_x, &mut max_y,
            0.0, 0.0, e.label_width, e.label_height);
    }
    for e in &out.edges {
        // The reference `pathBBox` parses the emitted `d` attribute which
        // uses 3-decimal rounding (d3-path's `.appendRound(3)`). We mirror
        // that rounding here so bounds match.
        let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
        for (x, y) in &e.points {
            acc(&mut min_x, &mut min_y, &mut max_x, &mut max_y, r3(*x), r3(*y), 0.0, 0.0);
        }
        // Edge label FO at (0, 0, label_w, label_h)
        acc(&mut min_x, &mut min_y, &mut max_x, &mut max_y,
            0.0, 0.0, e.label_width, e.label_height);
    }
    // Snapshot the pre-title bounds — renderer needs `bounds.x + w/2`
    // for the title's `x` attribute.
    let pre_title_min_x = min_x;
    let pre_title_max_x = max_x;

    // Diagram title (frontmatter / `title` statement) renders as a
    // `<text class="erDiagramTitleText">` at the bottom of the SVG. The
    // reference-gen shim treats `<text>` bbox as `(0, 0, text_w, text_h)`
    // regardless of the `x/y` attrs — include that contribution.
    if let Some(title) = d.meta.title.as_deref() {
        if !title.trim().is_empty() {
            let tw = measure_width(title);
            acc(&mut min_x, &mut min_y, &mut max_x, &mut max_y,
                0.0, 0.0, tw, label_h);
        }
    }

    if !min_x.is_finite() {
        min_x = 0.0;
        min_y = 0.0;
        max_x = 0.0;
        max_y = 0.0;
    }
    out.bounds = (min_x, min_y, max_x - min_x, max_y - min_y);
    // Title x anchor for the renderer.
    if pre_title_min_x.is_finite() {
        out.title_anchor_x = Some(pre_title_min_x + (pre_title_max_x - pre_title_min_x) / 2.0);
    }
    Ok(out)
}

fn edge_id(rel: &Relationship, counter: usize) -> String {
    format!("id_{}_{}_{}", rel.entity_a, rel.entity_b, counter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::er as parser_er;
    use crate::theme::get_theme;

    #[test]
    fn customer_box_has_reference_dims() {
        let d = parser_er::parse("erDiagram\n    CUSTOMER ||--o{ ORDER : places\n").unwrap();
        let theme = get_theme("default");
        let l = layout(&d, &theme).unwrap();
        assert_eq!(l.entities.len(), 2);
        let cust = &l.entities[0];
        // Reference cypress/er/01 bbox for CUSTOMER: width 119.1328125 / height 76.296875.
        assert!((cust.width - 119.1328125).abs() < 1e-6, "CUSTOMER width {}", cust.width);
        assert!((cust.height - 76.296875).abs() < 1e-6, "CUSTOMER height {}", cust.height);
    }
}
