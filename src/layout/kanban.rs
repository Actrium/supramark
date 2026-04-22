//! Kanban layout — ports upstream
//! /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/kanban/kanbanRenderer.ts
//! and the geometry block of
//! /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/rendering-util/rendering-elements/shapes/kanbanItem.ts
//!
//! The crucial jsdom quirk: `element.getBBox()` ignores `transform=""` and
//! returns only the raw `x/y/width/height` attributes. `foreignObject`
//! likewise returns `width = measured_text_width`, `height = 16.296875`
//! (14 px sans-serif, DejaVu metrics). The upstream renderer computes
//! the final SVG viewBox from that untransformed bbox — we emulate the
//! same "flat" bbox here.

use crate::error::Result;
use crate::font_metrics::text_width;
use crate::model::kanban::{KanbanDiagram, Priority};
use crate::theme::ThemeVariables;

/// Upstream default for `conf.kanban.sectionWidth`.
pub const SECTION_WIDTH: f64 = 200.0;
/// Upstream padding passed into `kanbanRenderer`.
pub const RENDER_PADDING: f64 = 10.0;
/// Every `<foreignObject>` jsdom measures in the default-theme kanban
/// output returns this height (DejaVu sans 14 px: (1901+483)/2048 * 14).
pub const LABEL_HEIGHT: f64 = 16.296875;
/// `maxLabelHeight` clamp floor in `kanbanRenderer.ts:45`.
pub const LABEL_MIN_HEIGHT: f64 = 25.0;
/// Per-item vertical padding used inside the shape (`kanbanItem.ts:83`).
pub const LABEL_PADDING_Y: f64 = 10.0;
/// Per-item horizontal padding — same file, line 32.
pub const LABEL_PADDING_X: f64 = 10.0;
/// `kanbanNode.padding || 10` default in the shape.
pub const ITEM_PADDING: f64 = 10.0;
/// `kanbanNode.rx ?? 5` and matching `ry`.
pub const CORNER_RADIUS: f64 = 5.0;

/// Geometry for one section column.
#[derive(Debug, Clone)]
pub struct SectionLayout {
    pub index: usize,
    /// Rect left edge.
    pub x: f64,
    /// Rect top edge (= -SECTION_WIDTH*3/2 for the default theme).
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub label_width: f64,
    pub label_tx: f64,
    pub label_ty: f64,
}

/// Geometry for one item card.
#[derive(Debug, Clone)]
pub struct ItemLayout {
    pub section: usize,
    pub cx: f64,
    pub cy: f64,
    pub width: f64,
    pub height: f64,
    pub title_width: f64,
    pub ticket_width: f64,
    pub assigned_width: f64,
    pub title_tx: f64,
    pub title_ty: f64,
    pub ticket_tx: f64,
    pub assigned_tx: f64,
    /// Priority stripe `(priority, y1, y2)` — absent when `Medium`.
    pub priority: Option<(Priority, f64, f64)>,
}

/// Full diagram geometry handed to the renderer.
#[derive(Debug, Clone, Default)]
pub struct KanbanLayout {
    pub sections: Vec<SectionLayout>,
    pub items: Vec<ItemLayout>,
    /// `(x, y, width, height)` for the SVG `viewBox` attr.
    pub view_box: (f64, f64, f64, f64),
}

/// Measure text with the jsdom default font (sans-serif, 14 px, regular).
fn measure(text: &str) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    text_width(text, "sans-serif", 14.0, false, false)
}

pub fn layout(d: &KanbanDiagram, _theme: &ThemeVariables) -> Result<KanbanLayout> {
    // --- Sections: compute label width & column positions ----------------------------------
    let mut max_label_h = LABEL_MIN_HEIGHT;
    let mut sections = Vec::with_capacity(d.sections.len());
    for (i, sec) in d.sections.iter().enumerate() {
        let label_w = measure(&sec.label);
        if !sec.label.is_empty() {
            max_label_h = max_label_h.max(LABEL_HEIGHT);
        }
        let cnt = (i + 1) as f64;
        // Upstream: `section.x = WIDTH*cnt + (cnt-1)*padding/2` is the
        // *centre*, then the rect is drawn at `x - width/2`.
        let centre_x = SECTION_WIDTH * cnt + (cnt - 1.0) * RENDER_PADDING / 2.0;
        let x = centre_x - SECTION_WIDTH / 2.0;
        sections.push(SectionLayout {
            index: i,
            x,
            y: -SECTION_WIDTH * 3.0 / 2.0,
            width: SECTION_WIDTH,
            height: 0.0,
            label_width: label_w,
            label_tx: centre_x - label_w / 2.0,
            label_ty: -SECTION_WIDTH * 3.0 / 2.0,
        });
    }

    // --- Items: vertical stack inside each section ----------------------------------------
    let mut items = Vec::new();
    let top = -SECTION_WIDTH * 3.0 / 2.0 + max_label_h;
    for (i, sec) in d.sections.iter().enumerate() {
        let centre_x = sections[i].x + SECTION_WIDTH / 2.0;
        let total_w = SECTION_WIDTH - 1.5 * RENDER_PADDING; // 185
        let mut y = top;

        for item in &sec.items {
            let title_w = measure(&item.label);
            let ticket_w = measure(item.ticket.as_deref().unwrap_or(""));
            let assigned_w = measure(item.assigned.as_deref().unwrap_or(""));

            // jsdom always hands back LABEL_HEIGHT for the
            // ticket/assigned foreignObject even when empty, so
            // `height_adj` is a fixed constant.
            let height_adj = LABEL_HEIGHT / 2.0;
            let total_h = (LABEL_HEIGHT + LABEL_PADDING_Y * 2.0).max(0.0) + height_adj;

            let cy = y + total_h / 2.0;
            y = cy + total_h / 2.0 + RENDER_PADDING / 2.0;

            let title_tx = ITEM_PADDING - total_w / 2.0;
            let title_ty = -height_adj - LABEL_HEIGHT / 2.0;
            let ticket_tx = ITEM_PADDING - total_w / 2.0;
            let assigned_tx = ITEM_PADDING + total_w / 2.0 - assigned_w - 2.0 * LABEL_PADDING_X;

            let priority = item.priority.and_then(|p| {
                p.stroke()?; // `Medium` returns None → no stripe
                let half_corner = (CORNER_RADIUS / 2.0).floor();
                let y1 = -total_h / 2.0 + half_corner;
                let y2 = total_h / 2.0 - half_corner;
                Some((p, y1, y2))
            });

            items.push(ItemLayout {
                section: i,
                cx: centre_x,
                cy,
                width: total_w,
                height: total_h,
                title_width: title_w,
                ticket_width: ticket_w,
                assigned_width: assigned_w,
                title_tx,
                title_ty,
                ticket_tx,
                assigned_tx,
                priority,
            });
        }

        // `kanbanRenderer.ts:87` — final section height, clamped to 50.
        let height = (y - top + 3.0 * RENDER_PADDING).max(50.0) + (max_label_h - 25.0);
        sections[i].height = height;
    }

    // --- ViewBox: unioned raw bbox (no transforms) + RENDER_PADDING -----------------------
    // jsdom `svg.getBBox()` walks every graphical descendant and merges
    // their raw `x/y/width/height`, ignoring any `<g transform>` on the
    // way down. Concretely we must include:
    //   * section rect — `(s.x, s.y, s.width, s.height)`
    //   * section cluster-label `<foreignObject>` — raw `(0, 0,
    //     label_w, LABEL_HEIGHT)` (no x/y attrs, transform ignored)
    //   * item rect — `(-w/2, -h/2, w, h)`
    //   * item `<foreignObject>`s — raw `(0, 0, text_w, LABEL_HEIGHT)`,
    //     so the widest-text contribution can extend the bbox well past
    //     the item card itself
    //   * priority `<line>` — `(x, y1, 0, y2-y1)` where `x = -w/2 + 2`
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut add_box = |x0: f64, y0: f64, w: f64, h: f64| {
        min_x = min_x.min(x0);
        max_x = max_x.max(x0 + w);
        min_y = min_y.min(y0);
        max_y = max_y.max(y0 + h);
    };
    for s in &sections {
        add_box(s.x, s.y, s.width, s.height);
        // Cluster-label foreignObject — raw (0, 0, label_w, LABEL_HEIGHT).
        if !d.sections[s.index].label.is_empty() {
            add_box(0.0, 0.0, s.label_width, LABEL_HEIGHT);
        }
    }
    for it in &items {
        add_box(-it.width / 2.0, -it.height / 2.0, it.width, it.height);
        // Three labels per item: title + ticket + assigned.
        if it.title_width > 0.0 {
            add_box(0.0, 0.0, it.title_width, LABEL_HEIGHT);
        }
        if it.ticket_width > 0.0 {
            add_box(0.0, 0.0, it.ticket_width, LABEL_HEIGHT);
        }
        if it.assigned_width > 0.0 {
            add_box(0.0, 0.0, it.assigned_width, LABEL_HEIGHT);
        }
        // Empty FOs are also drawn (width=0, height=LABEL_HEIGHT) —
        // contribute (0,0,0,16.296875) which never grows the bbox past
        // what we already collected, but be explicit so empty-text fixtures
        // still produce a non-empty viewBox if they come up.
        if let Some((_, y1, y2)) = it.priority {
            add_box(-it.width / 2.0 + 2.0, y1, 0.0, y2 - y1);
        }
    }
    if !min_x.is_finite() {
        min_x = 0.0;
        min_y = 0.0;
        max_x = 0.0;
        max_y = 0.0;
    }
    let pad = RENDER_PADDING;
    let view_box = (
        min_x - pad,
        min_y - pad,
        (max_x - min_x) + 2.0 * pad,
        (max_y - min_y) + 2.0 * pad,
    );

    Ok(KanbanLayout {
        sections,
        items,
        view_box,
    })
}
