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
use crate::text::markdown_text_content;
use crate::theme::ThemeVariables;

/// Trebuchet/etc. CSS default in the reference SVG — kept as a constant
/// so the renderer emits the exact same string.
pub const LABEL_FONT_FAMILY: &str = "sans-serif";
/// `<style>` default in the reference SVG.
pub const LABEL_FONT_SIZE: f64 = 14.0;
/// Theme default `fontSize` — used by `calculateTextWidth` in upstream's
/// erBox.ts minEntityWidth check. The check uses the *config* fontSize
/// (16 px from the default theme), not the label's rendered 14 px.
/// Using the wrong font size here causes INVOICE (short entity name)
/// to be erroneously clamped to MIN_ENTITY_WIDTH, widening the graph.
pub const THEME_FONT_SIZE: f64 = 16.0;
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

/// Per-attribute row — column widths + y placement, as computed by
/// [`compute_attr_layout`]. Consumed by the rough-based entity
/// renderer in `svg_er.rs` to emit column labels and row rects.
#[derive(Debug, Clone, Default)]
pub struct AttrRow {
    pub type_text: String,
    pub name_text: String,
    pub keys_text: String,
    pub comment_text: String,
    pub type_width: f64,
    pub name_width: f64,
    pub keys_width: f64,
    pub comment_width: f64,
    pub row_height: f64,
    /// y-offset from the top of the attribute area (not the entity origin).
    pub y_offset: f64,
}

/// Aggregate per-entity attribute layout state — everything the
/// renderer needs to reproduce upstream's `erBox.ts` attribute pass.
#[derive(Debug, Clone, Default)]
pub struct AttrLayout {
    pub rows: Vec<AttrRow>,
    pub name_bbox_width: f64,
    /// `nameBBox.height + TEXT_PADDING` — matches upstream variable.
    pub name_bbox_height: f64,
    pub max_type_width: f64,
    pub max_name_width: f64,
    pub max_keys_width: f64,
    pub max_comment_width: f64,
    pub keys_present: bool,
    pub comment_present: bool,
    /// `PADDING * 1.25` when htmlLabels is falsy (ER default).
    pub padding: f64,
    /// `TEXT_PADDING * 1.25` when htmlLabels is falsy (ER default).
    pub text_padding: f64,
}

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
    /// Inline style strings applied via `style` statements (e.g. `fill:#f9f`).
    pub css_styles: Vec<String>,
    /// Whether this entity has attributes → needs the richer erBox path.
    pub has_attrs: bool,
    /// Populated iff `has_attrs`. Holds per-row + per-column geometry.
    pub attr_layout: Option<AttrLayout>,
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
    /// Set to `"NONE"` for synthetic self-loop segments that suppress
    /// their end-side marker (mid + start segments).
    pub card_a: String,
    /// As `card_a`, for the start-side marker. `"NONE"` suppresses the
    /// `marker-start` attribute entirely.
    pub card_b: String,
    /// Spline waypoints post-dagre.
    pub points: Vec<(f64, f64)>,
    /// Label centre. `None` when dagre did not assign one — happens for
    /// zero-width labels and reverse edges in cyclic ranks. Mermaid then
    /// writes `translate(undefined, NaN)` verbatim, which we mirror.
    pub label_x: Option<f64>,
    pub label_y: Option<f64>,
}

/// One self-loop helper node — a 10×10 invisible rectangle inserted
/// between the two cyclic-special path halves. Upstream renders these
/// inside the `<g class="nodes">` block as `<g class="label edgeLabel">`
/// with id `{owner}---{owner}---{1|2}`. Coordinates come straight from
/// dagre via `dagre_bridge::collect_self_loop_helpers`.
#[derive(Debug, Clone, Default)]
pub struct SelfLoopHelper {
    pub id: String,
    pub x: f64,
    pub y: f64,
    /// Helper rect width (always 10 in upstream's `expand_self_edge`).
    pub width: f64,
    /// Helper rect height (always 10 in upstream's `expand_self_edge`).
    pub height: f64,
    /// Per-segment label height — pulled from the original edge's label
    /// height so the inner foreignObject matches the role-label font.
    pub label_height: f64,
}

/// Output of the ER layout pass.
#[derive(Debug, Clone, Default)]
pub struct ErLayout {
    pub entities: Vec<EntityLayout>,
    /// All edges to render — regular relationships **plus** the
    /// synthetic cyclic-special-{1,mid,2} sub-edges that replace each
    /// user self-edge (a self-edge appears as 3 entries in this list,
    /// in order, at the position of the original relationship).
    pub edges: Vec<EdgeLayout>,
    /// Synthetic self-loop helper nodes — rendered inside the nodes
    /// block as `<g class="label edgeLabel">` placeholders. Empty when
    /// no self-loops exist.
    pub self_loop_helpers: Vec<SelfLoopHelper>,
    /// Overall post-dagre bounds — used by the renderer to build the viewBox.
    pub bounds: (f64, f64, f64, f64),
    /// Layout direction (TB/BT/LR/RL).
    pub direction: String,
    /// Title anchor x (centre of pre-title bbox). `None` when there is
    /// no title.
    pub title_anchor_x: Option<f64>,
    /// classDef definitions — keyed by class id.
    pub classes: std::collections::BTreeMap<String, crate::model::er::EntityClass>,
}

/// Measure a label at sans-serif 14 px.
/// When the text contains `<br />` or `<br/>` HTML line breaks (as used in
/// relationship role labels), split on them and sum each line's width —
/// matching upstream `calculateTextWidth` which accumulates per-line widths
/// across the jsdom foreignObject bbox measurement.
fn measure_width(text: &str) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    // Whitespace-only labels: mermaid's `markdownToHTML` lexes them as a
    // single `space` token and `output()` returns the empty string for
    // it. The resulting `<span>` is empty, the foreignObject's
    // `getBoundingClientRect()` reports width 0, and dagre then never
    // assigns a label centre — so we mirror that here.
    if !text.contains("<br") && text.chars().all(char::is_whitespace) {
        return 0.0;
    }
    // Split on HTML break tags (<br/>, <br />, <br/> variants), then strip
    // markdown decorators from each segment before measuring.  Upstream's
    // calculateTextWidth feeds the plain-text form (after marked renders
    // **bold** / __bold__ / *italic* etc.) to the canvas measurer, so we
    // must match that by applying markdown_text_content per segment.
    let raw_lines = split_br(text);
    raw_lines
        .iter()
        .map(|line| {
            if line.is_empty() {
                0.0f64
            } else {
                let plain = markdown_text_content(line);
                text_width(&plain, LABEL_FONT_FAMILY, LABEL_FONT_SIZE, false, false)
            }
        })
        .sum()
}

/// Split a string on `<br...>` HTML break tags, returning the segments.
pub fn split_br(text: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = Vec::new();
    let mut rest = text;
    loop {
        if let Some(pos) = rest.find("<br") {
            let after = &rest[pos + 3..];
            if let Some(end_off) = after.find('>') {
                parts.push(&rest[..pos]);
                rest = &after[end_off + 1..];
            } else {
                break;
            }
        } else {
            break;
        }
    }
    parts.push(rest);
    parts
}

fn measure_label_height() -> f64 {
    line_height(LABEL_FONT_FAMILY, LABEL_FONT_SIZE, false, false)
}

/// Compute the no-attribute entity box dimensions.
///
/// Mirrors upstream `erBox.ts`'s no-attribute branch:
/// 1. Check `calculateTextWidth(label, config) + paddingX*2 < minEntityWidth`
///    using the *theme* font size (16 px) — matching how `calculateTextWidth`
///    reads `config.fontSize`. If true, `node.width = minEntityWidth`.
/// 2. `drawRect` computes `max(bbox.width + paddingX*2, node.width || 0)`.
///    The `bbox` comes from the label measured at 14 px.
///
/// The label_w parameter is measured at 14 px (the actual label size).
/// For the minEntityWidth check we must use the theme fontSize (16 px)
/// because that's what `calculateTextWidth` uses upstream.
fn entity_box_size(label_w: f64, label_h: f64, rendered_label: &str) -> (f64, f64) {
    // Upstream's minEntityWidth check uses calculateTextWidth which
    // reads config.fontSize (16 px by default), NOT the label's 14 px.
    let check_w = text_width(
        rendered_label,
        LABEL_FONT_FAMILY,
        THEME_FONT_SIZE,
        false,
        false,
    );
    let width = if check_w + PADDING * 2.0 < MIN_ENTITY_WIDTH {
        MIN_ENTITY_WIDTH
    } else {
        label_w + PADDING * 2.0
    };
    let height = label_h + PADDING * 1.5 * 2.0;
    (width, height)
}

/// Port of upstream `erBox.ts`'s attribute-bearing layout pass.
///
/// Upstream flow (abbreviated):
/// * `PADDING = config.er.diagramPadding` (20); `TEXT_PADDING = entityPadding` (15).
/// * When `!config.htmlLabels`, both `PADDING *= 1.25` / `TEXT_PADDING *= 1.25`.
///   Mermaid's default for the ER sub-config leaves `htmlLabels` unset (null-ish),
///   so the 1.25 multiplier is applied — matches the reference output byte-for-byte.
/// * For each attribute, measure `{type, name, keys, comment}` at the same 14 px
///   sans-serif font as the rest of the ER renderer.
/// * `max<col>Width = max(col_text_w + PADDING)` across rows.
/// * `keysPresent` / `commentPresent` are **false** when the column's total max
///   is `≤ PADDING` (i.e. every row's text was empty).
/// * `rowHeight = max_column_text_height + TEXT_PADDING`.
/// * `nameBBox.height = label_height + TEXT_PADDING` (upstream mutates in place).
/// * `totalWidthSections = 4 - (keysPresent?0:1) - (commentPresent?0:1)`.
///   If `nameBBox.width + PADDING*2 > sum(max*Width)`, distribute the diff.
/// * `maxWidth = sum(maxTypeW + maxNameW + maxKeysW + maxCommentW)` (post-adjust).
/// * `w = max(shapeBBox.w + PADDING*2, node.width || 0, maxWidth)`.
/// * `h = max(totalShapeBBoxHeight + nameBBox.height, node.height || 0)`.
///
/// The returned layout is consumed by the renderer **and** contributes the
/// entity's post-measure `(width, height)` used by dagre.
pub(crate) fn compute_attr_layout(
    name_label: &str,
    attributes: &[crate::model::er::Attribute],
) -> AttrLayout {
    // ── 1. Base padding (htmlLabels-disabled branch) ────────────────
    let padding = PADDING * 1.25; // = 25
    let text_padding = 15.0_f64 * 1.25; // = 18.75 (entityPadding default)

    let label_h = measure_label_height();
    let name_w = measure_width(name_label);
    // Upstream: nameBBox is the `<g class="name">`'s box → label-width/label-height.
    // Then the code mutates `nameBBox.height += TEXT_PADDING;` — we apply that here.
    let name_bbox_width = name_w;
    let name_bbox_height = label_h + text_padding;

    // ── 2. Per-row widths / heights ──────────────────────────────────
    let mut rows: Vec<AttrRow> = Vec::with_capacity(attributes.len());
    let mut max_type_w = 0.0_f64;
    let mut max_name_w = 0.0_f64;
    let mut max_keys_w = 0.0_f64;
    let mut max_comment_w = 0.0_f64;
    let mut y_offset = 0.0_f64;
    for attr in attributes {
        let keys_joined = attr.keys.join(",");
        // Attribute type text goes through upstream's parseGenericTypes,
        // turning `foo~Bar~` into `foo<Bar>` — the FO width used by the
        // renderer is measured on the PROCESSED text (so it matches the
        // reference SVG's `foreignObject width="…"` exactly).
        let processed_type = crate::render::svg_er::parse_generic_types_pub(&attr.attr_type);
        let type_w = measure_width(&processed_type);
        let name_w = measure_width(&attr.name);
        let keys_w = measure_width(&keys_joined);
        let comment_w = measure_width(&attr.comment);
        // `max<Col>Width = Math.max(max<Col>Width, box.width + PADDING);`
        max_type_w = max_type_w.max(type_w + padding);
        max_name_w = max_name_w.max(name_w + padding);
        max_keys_w = max_keys_w.max(keys_w + padding);
        max_comment_w = max_comment_w.max(comment_w + padding);
        let row_h = label_h.max(label_h).max(label_h).max(label_h) + text_padding;
        rows.push(AttrRow {
            // Store the processed (generic-unwrapped) form — the
            // renderer HTML-escapes this at emission time.
            type_text: processed_type,
            name_text: attr.name.clone(),
            keys_text: keys_joined,
            comment_text: attr.comment.clone(),
            type_width: type_w,
            name_width: name_w,
            keys_width: keys_w,
            comment_width: comment_w,
            row_height: row_h,
            y_offset,
        });
        y_offset += row_h;
    }

    // ── 3. keysPresent / commentPresent guards ───────────────────────
    let mut total_sections = 4;
    let mut keys_present = true;
    let mut comment_present = true;
    if max_keys_w <= padding {
        keys_present = false;
        max_keys_w = 0.0;
        total_sections -= 1;
    }
    if max_comment_w <= padding {
        comment_present = false;
        max_comment_w = 0.0;
        total_sections -= 1;
    }

    // ── 4. nameBBox.width +  2*PADDING vs sum adjustment ────────────
    let sum_cols = max_type_w + max_name_w + max_keys_w + max_comment_w;
    if name_bbox_width + padding * 2.0 - sum_cols > 0.0 {
        let diff = name_bbox_width + padding * 2.0 - sum_cols;
        max_type_w += diff / total_sections as f64;
        max_name_w += diff / total_sections as f64;
        if max_keys_w > 0.0 {
            max_keys_w += diff / total_sections as f64;
        }
        if max_comment_w > 0.0 {
            max_comment_w += diff / total_sections as f64;
        }
    }

    AttrLayout {
        rows,
        name_bbox_width,
        name_bbox_height,
        max_type_width: max_type_w,
        max_name_width: max_name_w,
        max_keys_width: max_keys_w,
        max_comment_width: max_comment_w,
        keys_present,
        comment_present,
        padding,
        text_padding,
    }
}

/// Total post-layout (w, h) for an attribute-bearing entity.
/// Mirrors `erBox.ts`'s:
///   `w = max(shapeBBox.w + PADDING*2, node.width||0, maxWidth)`
///   `h = max(totalShapeBBoxHeight + nameBBox.height, node.height||0)`
pub(crate) fn attr_entity_bbox(a: &AttrLayout) -> (f64, f64) {
    // totalShapeBBoxHeight = sum(row.rowHeight)
    let total_rows_h: f64 = a.rows.iter().map(|r| r.row_height).sum();
    // shapeBBox.width + PADDING*2 — union of text foreignObject widths at x=0.
    // For our purposes upstream's `shapeBBox` never exceeds `maxWidth` in the
    // attribute-bearing case (per-row widths are already accounted for in
    // the column widths), so compute maxWidth and take the larger.
    let max_w = a.max_type_width + a.max_name_width + a.max_keys_width + a.max_comment_width;
    // Upstream evaluates `shapeBBox.width + PADDING*2` against the pre-existing
    // `<g class="name">` + attribute labels. The conservative clamp below
    // matches all fixtures we've probed: max(maxWidth, shapeBBoxWidth+padding*2)
    // where shapeBBoxWidth is the wider of (name_bbox_width, per-col max text_w).
    let mut shape_bbox_w = a.name_bbox_width;
    for r in &a.rows {
        shape_bbox_w = shape_bbox_w
            .max(r.type_width)
            .max(r.name_width)
            .max(r.keys_width)
            .max(r.comment_width);
    }
    let w = max_w.max(shape_bbox_w + a.padding * 2.0);
    let h = total_rows_h + a.name_bbox_height;
    (w, h)
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

        // Collect styled font properties from classDefs + style commands.
        let (styled_size, styled_bold) = resolve_styled_font(entity, &d.classes);
        let label_w = if styled_size != LABEL_FONT_SIZE || styled_bold {
            text_width(
                &rendered_label,
                LABEL_FONT_FAMILY,
                styled_size,
                styled_bold,
                false,
            )
        } else {
            measure_width(&rendered_label)
        };
        let label_h_s = if styled_size != LABEL_FONT_SIZE || styled_bold {
            line_height(LABEL_FONT_FAMILY, styled_size, styled_bold, false)
        } else {
            label_h
        };

        let (w, h) = if entity.attributes.is_empty() {
            entity_box_size(label_w, label_h_s, &rendered_label)
        } else {
            let a = compute_attr_layout(&rendered_label, &entity.attributes);
            attr_entity_bbox(&a)
        };
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
    out.classes = d.classes.clone();

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
        let (styled_size, styled_bold) = resolve_styled_font(entity, &d.classes);
        let label_w = if styled_size != LABEL_FONT_SIZE || styled_bold {
            text_width(
                &rendered_label,
                LABEL_FONT_FAMILY,
                styled_size,
                styled_bold,
                false,
            )
        } else {
            measure_width(&rendered_label)
        };
        let label_h_s = if styled_size != LABEL_FONT_SIZE || styled_bold {
            line_height(LABEL_FONT_FAMILY, styled_size, styled_bold, false)
        } else {
            label_h
        };
        let attr_layout = if entity.attributes.is_empty() {
            None
        } else {
            Some(compute_attr_layout(&rendered_label, &entity.attributes))
        };
        out.entities.push(EntityLayout {
            id: entity.id.clone(),
            label: rendered_label,
            label_width: label_w,
            label_height: label_h_s,
            width: w,
            height: h,
            x,
            y,
            css_classes: entity.css_classes.clone(),
            css_styles: entity.css_styles.clone(),
            has_attrs: !entity.attributes.is_empty(),
            attr_layout,
        });
    }

    // Pre-build a per-owner index of synthetic cyclic-special sub-edges so
    // we can splice them in at the position of each user self-edge while
    // keeping every other edge at its original index. dagre_bridge already
    // emits them in segment order (0 → 1 → 2).
    let synthetic_segments_by_owner: std::collections::HashMap<&str, Vec<&Edge>> = {
        let mut m: std::collections::HashMap<&str, Vec<&Edge>> = std::collections::HashMap::new();
        for e in &result.edges {
            if e.extra.get("synthetic").map(|s| s.as_str()) != Some("cyclic_segment") {
                continue;
            }
            if let Some(owner) = e.extra.get("cyclic_owner") {
                m.entry(owner.as_str()).or_default().push(e);
            }
        }
        // Sort each owner's segments by cyclic_index ascending.
        for v in m.values_mut() {
            v.sort_by_key(|e| {
                e.extra
                    .get("cyclic_index")
                    .and_then(|s| s.parse::<u8>().ok())
                    .unwrap_or(0)
            });
        }
        m
    };

    for (i, rel) in d.relationships.iter().enumerate() {
        // Self-edge → splice in the synthetic cyclic-special-{1,mid,2}
        // sub-edges generated by upstream `expand_self_edge`. We don't
        // emit a path for the user's edge itself (dagre never assigns
        // points/label to it once it's been expanded). See
        // `dagre_bridge::collect_self_loop_segments` + upstream
        // `rendering-util/.../dagre/index.js:308-364`.
        if rel.entity_a == rel.entity_b {
            if let Some(segs) = synthetic_segments_by_owner.get(rel.entity_a.as_str()) {
                for seg in segs {
                    let seg_idx = seg
                        .extra
                        .get("cyclic_index")
                        .and_then(|s| s.parse::<u8>().ok())
                        .unwrap_or(0);
                    // Cardinality mapping (mirrors `index.js:343-361`):
                    //   * seg 0 (cyclic-special-1)  : marker-start = original card_b, marker-end = NONE
                    //   * seg 1 (cyclic-special-mid): both NONE
                    //   * seg 2 (cyclic-special-2)  : marker-start = NONE, marker-end = original card_a
                    // The sentinel `"NONE"` tells the renderer to skip that marker attribute.
                    let (card_a_str, card_b_str) = match seg_idx {
                        0 => ("NONE".to_string(), rel.card_b.as_upper().to_string()),
                        2 => (rel.card_a.as_upper().to_string(), "NONE".to_string()),
                        _ => ("NONE".to_string(), "NONE".to_string()),
                    };
                    let (label_str, label_w) = if seg_idx == 1 {
                        (rel.role_a.clone(), measure_width(&rel.role_a))
                    } else {
                        (String::new(), 0.0)
                    };
                    let points: Vec<(f64, f64)> = seg
                        .points
                        .as_ref()
                        .map(|pts| pts.iter().map(|p| (p.x, p.y)).collect())
                        .unwrap_or_default();
                    // Empty/whitespace-only label → suppress the label
                    // transform (mirror non-self-edge handling above).
                    let label_x = if label_str.trim().is_empty() && !label_str.is_empty() {
                        None
                    } else if label_str.is_empty() {
                        None
                    } else {
                        seg.label_x
                    };
                    let label_y = if label_str.trim().is_empty() && !label_str.is_empty() {
                        None
                    } else if label_str.is_empty() {
                        None
                    } else {
                        seg.label_y
                    };
                    out.edges.push(EdgeLayout {
                        id: seg.id.clone(),
                        src: seg.source.clone().unwrap_or_default(),
                        dst: seg.target.clone().unwrap_or_default(),
                        label: label_str,
                        label_width: label_w,
                        label_height: label_h,
                        pattern: rel.rel_type.edge_pattern(),
                        card_a: card_a_str,
                        card_b: card_b_str,
                        points,
                        label_x,
                        label_y,
                    });
                }
            }
            continue;
        }
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
            // Mermaid only emits a label transform when `if (edge.label)`
            // is truthy, and the laid coords come from dagre's edge-label
            // dummy. When the role label is whitespace-only the upstream
            // markdown lexer drops it (label width = 0), dagre never
            // creates an edge-label rank, and the routing fallback's mid-
            // point isn't reflected in the reference. Force `None` so
            // the renderer prints `translate(undefined, NaN)` to match.
            label_x: if rel.role_a.trim().is_empty() && !rel.role_a.is_empty() {
                None
            } else {
                laid.label_x
            },
            label_y: if rel.role_a.trim().is_empty() && !rel.role_a.is_empty() {
                None
            } else {
                laid.label_y
            },
        });
    }

    // ── 3.5. Extract synthetic self-loop helper rectangles ──
    // `dagre_bridge` appends two helper labelRect nodes per user self-edge
    // (tagged `extra["synthetic"] = "cyclic_helper"`) — these get rendered
    // inside the nodes block as `<g class="label edgeLabel">` placeholders.
    for n in &result.nodes {
        if n.extra.get("synthetic").map(|s| s.as_str()) != Some("cyclic_helper") {
            continue;
        }
        out.self_loop_helpers.push(SelfLoopHelper {
            id: n.id.clone(),
            x: n.x.unwrap_or(0.0),
            y: n.y.unwrap_or(0.0),
            width: n.width.unwrap_or(10.0),
            height: n.height.unwrap_or(10.0),
            label_height: label_h,
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
    let acc = |min_x: &mut f64,
               min_y: &mut f64,
               max_x: &mut f64,
               max_y: &mut f64,
               x: f64,
               y: f64,
               w: f64,
               h: f64| {
        *min_x = min_x.min(x);
        *min_y = min_y.min(y);
        *max_x = max_x.max(x + w);
        *max_y = max_y.max(y + h);
    };
    for e in &out.entities {
        // rect at local (-w/2, -h/2, w, h)
        acc(
            &mut min_x,
            &mut min_y,
            &mut max_x,
            &mut max_y,
            -e.width / 2.0,
            -e.height / 2.0,
            e.width,
            e.height,
        );
        // FO at (0, 0, label_w, label_h)
        acc(
            &mut min_x,
            &mut min_y,
            &mut max_x,
            &mut max_y,
            0.0,
            0.0,
            e.label_width,
            e.label_height,
        );
    }
    // Walk every edge (regular + synthetic cyclic-special) and union their
    // basis-spline path bbox + label foreignObject contribution.
    //
    // The reference `pathBBox` parses the emitted `d` attribute which uses
    // 3-decimal rounding (d3-path's `.appendRound(3)`). pathBBox unions
    // every M/L/C anchor and control point. The raw dagre waypoints are
    // NOT directly visible in the path (basis only interpolates the
    // endpoints), so unioning raw points overshoots / undershoots the true
    // rendered envelope.  Mirror `render::edges::path_basis` to collect the
    // exact set of (x,y) tokens the renderer will emit.
    //
    // IMPORTANT: each edge path forms a **per-element bbox** that the
    // reference's `unionBox` merges using `maxY = max(maxY, box.y +
    // box.height)`.  When a path spans a large Y range (e.g. minY≈84,
    // maxY≈391), the intermediate `height = maxY - minY` loses 1 ULP
    // relative to the exact span, and `minY + height` can round 1 ULP
    // above the original `maxY`.  We must replicate this per-box round-
    // trip so our global bounds match the reference byte-for-byte.
    let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
    let mut path_bboxes: Vec<(f64, f64, f64, f64)> = Vec::new();
    for edge in &out.edges {
        let mut e_min_x = f64::INFINITY;
        let mut e_min_y = f64::INFINITY;
        let mut e_max_x = f64::NEG_INFINITY;
        let mut e_max_y = f64::NEG_INFINITY;
        let mut acc_pt = |x: f64, y: f64| {
            let rx = r3(x);
            let ry = r3(y);
            e_min_x = e_min_x.min(rx);
            e_min_y = e_min_y.min(ry);
            e_max_x = e_max_x.max(rx);
            e_max_y = e_max_y.max(ry);
        };
        let n = edge.points.len();
        if n == 1 {
            acc_pt(edge.points[0].0, edge.points[0].1);
        } else if n >= 2 {
            let (mut x0, mut y0) = (f64::NAN, f64::NAN);
            let (mut x1, mut y1) = (f64::NAN, f64::NAN);
            let mut state = 0u8;
            for &(x, y) in &edge.points {
                match state {
                    0 => {
                        acc_pt(x, y);
                        state = 1;
                    }
                    1 => {
                        state = 2;
                    }
                    2 => {
                        acc_pt((5.0 * x0 + x1) / 6.0, (5.0 * y0 + y1) / 6.0);
                        acc_pt((2.0 * x0 + x1) / 3.0, (2.0 * y0 + y1) / 3.0);
                        acc_pt((x0 + 2.0 * x1) / 3.0, (y0 + 2.0 * y1) / 3.0);
                        acc_pt((x0 + 4.0 * x1 + x) / 6.0, (y0 + 4.0 * y1 + y) / 6.0);
                        state = 3;
                    }
                    _ => {
                        acc_pt((2.0 * x0 + x1) / 3.0, (2.0 * y0 + y1) / 3.0);
                        acc_pt((x0 + 2.0 * x1) / 3.0, (y0 + 2.0 * y1) / 3.0);
                        acc_pt((x0 + 4.0 * x1 + x) / 6.0, (y0 + 4.0 * y1 + y) / 6.0);
                    }
                }
                x0 = x1;
                x1 = x;
                y0 = y1;
                y1 = y;
            }
            match state {
                3 => {
                    acc_pt((2.0 * x0 + x1) / 3.0, (2.0 * y0 + y1) / 3.0);
                    acc_pt((x0 + 2.0 * x1) / 3.0, (y0 + 2.0 * y1) / 3.0);
                    acc_pt((x0 + 5.0 * x1) / 6.0, (y0 + 5.0 * y1) / 6.0);
                    acc_pt(x1, y1);
                }
                2 => {
                    acc_pt(x1, y1);
                }
                _ => {}
            }
        }
        if e_min_x.is_finite() {
            path_bboxes.push((e_min_x, e_min_y, e_max_x - e_min_x, e_max_y - e_min_y));
        }
        // Edge label FO at (0, 0, label_w, label_h) — unioned as a
        // separate per-element box to match the reference's unionBox flow.
        if edge.label_width > 0.0 || edge.label_height > 0.0 {
            path_bboxes.push((0.0, 0.0, edge.label_width, edge.label_height));
        }
    }
    // Union all per-element bboxes using the reference's unionBox formula:
    //   minY_global = min(b.y)
    //   maxY_global = max(b.y + b.height)
    // This introduces the 1-ULP rounding from (minY + (maxY - minY))
    // that the reference's unionBox produces for wide-range boxes.
    for (bx, by, bw, bh) in &path_bboxes {
        min_x = min_x.min(*bx);
        min_y = min_y.min(*by);
        max_x = max_x.max(bx + bw);
        max_y = max_y.max(by + bh);
    }
    // Self-loop helper rect placeholders (10×10 invisibles) — upstream's
    // `getBBox` shim treats their `<rect>` at local (0, 0, 0.1, 0.1) and
    // their inner foreignObject at (0, 0, 0, label_h). The rect is too
    // small to move bounds; the inner FO contributes a `label_h` row.
    for h in &out.self_loop_helpers {
        acc(
            &mut min_x,
            &mut min_y,
            &mut max_x,
            &mut max_y,
            0.0,
            0.0,
            0.0,
            h.label_height,
        );
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
            acc(
                &mut min_x, &mut min_y, &mut max_x, &mut max_y, 0.0, 0.0, tw, label_h,
            );
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

/// Resolve the effective font size and bold state for an entity by
/// examining its `css_styles` (style command) and any classDef classes.
/// Returns `(font_size, bold)`.
fn resolve_styled_font(
    entity: &crate::model::er::Entity,
    classes: &std::collections::BTreeMap<String, crate::model::er::EntityClass>,
) -> (f64, bool) {
    let mut font_size = LABEL_FONT_SIZE;
    let mut bold = false;

    let mut all_styles: Vec<String> = Vec::new();
    for cls_name in entity.css_classes.split_whitespace() {
        if let Some(class_def) = classes.get(cls_name) {
            for s in &class_def.styles {
                all_styles.push(s.clone());
            }
        }
    }
    for s in &entity.css_styles {
        all_styles.push(s.clone());
    }

    for style in &all_styles {
        let style = style.trim();
        if style.is_empty() {
            continue;
        }
        if let Some((prop, val)) = style.split_once(':') {
            let prop = prop.trim();
            let val = val.trim();
            match prop {
                "font-size" => {
                    if let Some(num) = val.trim_end_matches("px").parse::<f64>().ok() {
                        font_size = num;
                    }
                }
                "font-weight" => {
                    if val == "bold"
                        || val == "bolder"
                        || val.starts_with('7')
                        || val.starts_with('8')
                        || val.starts_with('9')
                    {
                        bold = true;
                    }
                }
                _ => {}
            }
        }
    }

    (font_size, bold)
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
        assert!(
            (cust.width - 119.1328125).abs() < 1e-6,
            "CUSTOMER width {}",
            cust.width
        );
        assert!(
            (cust.height - 76.296875).abs() < 1e-6,
            "CUSTOMER height {}",
            cust.height
        );
    }

    #[test]
    fn er03_node_positions_match_reference() {
        let d = parser_er::parse(
            "erDiagram\n\
             CUSTOMER ||--o{ ORDER : places\n\
             ORDER ||--|{ LINE-ITEM : contains\n\
             CUSTOMER ||--|{ ADDRESS : \"invoiced at\"\n\
             CUSTOMER ||--|{ ADDRESS : \"receives goods at\"\n\
             ORDER ||--o{ INVOICE : \"liable for\"\n",
        )
        .unwrap();
        let theme = get_theme("default");
        let l = layout(&d, &theme).unwrap();

        // Reference positions from upstream mermaid@11.14.0
        // (generated by dagre-d3-es v7.0.14):
        let ref_positions: &[(&str, f64, f64, f64)] = &[
            ("entity-CUSTOMER-0", 347.3310546875, 46.1484375, 119.1328125),
            ("entity-ORDER-1", 184.915283203125, 218.7421875, 100.0),
            (
                "entity-LINE-ITEM-2",
                62.9521484375,
                391.3359375,
                109.904296875,
            ),
            (
                "entity-ADDRESS-3",
                428.657470703125,
                218.7421875,
                107.484375,
            ),
            (
                "entity-INVOICE-4",
                306.87841796875,
                391.3359375,
                97.9482421875,
            ),
        ];

        for (id, ref_x, ref_y, ref_w) in ref_positions {
            let entity = l
                .entities
                .iter()
                .find(|e| e.id == *id)
                .unwrap_or_else(|| panic!("entity {} not found", id));
            assert!(
                (entity.x - ref_x).abs() < 1e-3,
                "{} x: got {}, expected {}",
                id,
                entity.x,
                ref_x
            );
            assert!(
                (entity.y - ref_y).abs() < 1e-3,
                "{} y: got {}, expected {}",
                id,
                entity.y,
                ref_y
            );
            assert!(
                (entity.width - ref_w).abs() < 1e-3,
                "{} width: got {}, expected {}",
                id,
                entity.width,
                ref_w
            );
        }
    }
}
