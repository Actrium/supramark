//! Cluster / subgraph box emission — ports upstream's
//! `rendering-util/rendering-elements/clusters.js` (526 LoC).
//!
//! A cluster is a visible rectangle drawn around the children of a
//! subgraph node, with an optional title placed above the top edge.
//! Upstream supports six variants:
//!
//! * `rect` / `squareRect`    — flowchart subgraph border,
//! * `roundedWithTitle`       — state-diagram composite-state border,
//! * `noteGroup`              — invisible note wrapper,
//! * `divider`                — state-diagram divider (concurrent
//!                              region separator, drawn as a dashed
//!                              rectangle),
//! * `kanbanSection`          — Kanban column body.
//!
//! This module emits each variant as an `<rect>` (or `<path>` for
//! handDrawn, though handDrawn mode is a caller concern) plus a
//! `<text>`-shaped title block. Title label text is emitted through
//! `crate::render::svg_richtext` — this module just produces the
//! geometric envelope.

use std::fmt::Write;

use crate::layout::unified::types::{Bounds, Node};
use crate::theme::ThemeVariables;

/// Cluster style variant selector. Mirrors the upstream `shapes` map
/// keys in `clusters.js`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterShape {
    /// `rect` / `squareRect` — plain rounded-rectangle border with the
    /// label centred above the top edge.
    Rect,
    /// `roundedWithTitle` — outer rectangle + inner "body" rectangle
    /// below the title bar. Used by state-diagram composite states.
    RoundedWithTitle,
    /// `noteGroup` — invisible rect (fill="none"). Carries the
    /// geometry so layout is preserved but paints nothing.
    NoteGroup,
    /// `divider` — dashed rectangle separating concurrent regions.
    Divider,
    /// `kanbanSection` — Kanban-column body; shape-wise identical to
    /// `Rect` but the caller uses a column-specific palette.
    KanbanSection,
}

impl ClusterShape {
    pub fn parse(name: &str) -> Self {
        match name {
            "roundedWithTitle" => Self::RoundedWithTitle,
            "noteGroup" => Self::NoteGroup,
            "divider" => Self::Divider,
            "kanbanSection" => Self::KanbanSection,
            _ => Self::Rect,
        }
    }
}

/// Output of a cluster emission: the SVG chunk plus updated geometry
/// (width / height may have grown to accommodate a too-wide title).
#[derive(Debug, Clone)]
pub struct ClusterSvg {
    /// The SVG snippet — a single `<g class="cluster">…</g>` element.
    pub svg: String,
    /// The rect's outer bounding box after any title-driven growth.
    pub bounds: Bounds,
    /// Upstream's `node.labelBBox` — the title text's rendered size.
    /// Caller uses this to offset child nodes downward.
    pub label_bbox: Bounds,
}

/// Emission parameters. Keeps the public signature short and makes
/// future extensions (icon, asset) additive.
#[derive(Debug, Clone)]
pub struct ClusterEmit<'a> {
    pub node: &'a Node,
    pub theme: &'a ThemeVariables,
    pub shape: ClusterShape,
    /// Rendered-label bounding box (from `createText` in upstream).
    /// Callers pass `(0, 0)` when there is no title.
    pub label_bbox: Bounds,
    /// The label's pre-rendered `<text>` / `<foreignObject>` snippet,
    /// ready to be dropped inside the cluster `<g>`. Empty string for
    /// untitled variants.
    pub label_svg: &'a str,
    /// Top-of-title y-offset — mirrors upstream's
    /// `subGraphTitleTopMargin`. Flowchart uses 0; state diagram uses
    /// a non-zero value.
    pub title_top_margin: f64,
}

/// Emit the cluster. Mirrors upstream's `shapes[shape](parent, node)`
/// dispatch. Handles the five variants directly; the caller routes
/// `handDrawn` nodes to the rough-js path emitter (not yet ported).
pub fn emit(opts: &ClusterEmit<'_>) -> ClusterSvg {
    match opts.shape {
        ClusterShape::Rect | ClusterShape::KanbanSection => emit_rect(opts),
        ClusterShape::RoundedWithTitle => emit_rounded_with_title(opts),
        ClusterShape::NoteGroup => emit_note_group(opts),
        ClusterShape::Divider => emit_divider(opts),
    }
}

fn emit_rect(opts: &ClusterEmit<'_>) -> ClusterSvg {
    // Upstream lines 56-61:
    //   width = node.width <= bbox.width + node.padding
    //           ? bbox.width + node.padding
    //           : node.width;
    let node = opts.node;
    let padding = node.padding.unwrap_or(0.0);
    let node_w = node.width.unwrap_or(0.0);
    let node_h = node.height.unwrap_or(0.0);
    let cx = node.x.unwrap_or(0.0);
    let cy = node.y.unwrap_or(0.0);

    let width = if node_w <= opts.label_bbox.width + padding {
        opts.label_bbox.width + padding
    } else {
        node_w
    };
    let height = node_h;
    let x = cx - width / 2.0;
    let y = cy - height / 2.0;
    let rx = node.rx.unwrap_or(0.0);
    let ry = node.ry.unwrap_or(0.0);

    let fill = opts.theme.cluster_bkg.as_deref().unwrap_or("#ffffff");
    let stroke = opts.theme.cluster_border.as_deref().unwrap_or("#aaaaaa");

    let dom_id = node.dom_id.as_deref().unwrap_or(&node.id);
    let css_classes = node.css_classes.as_deref().unwrap_or("");

    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<g class="cluster {css}" id="{id}" data-look="{look}">"#,
        css = escape_attr(css_classes),
        id = escape_attr(dom_id),
        look = node.look.as_deref().unwrap_or("classic"),
    );
    let _ = write!(
        svg,
        r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" ry="{ry}" \
style="fill:{fill};stroke:{stroke}"/>"#,
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(width),
        h = fmt_num(height),
        rx = fmt_num(rx),
        ry = fmt_num(ry),
        fill = fill,
        stroke = stroke,
    );
    // Title sits above the top edge, centred on the node's x.
    let label_tx = cx - opts.label_bbox.width / 2.0;
    let label_ty = cy - height / 2.0 + opts.title_top_margin;
    if !opts.label_svg.is_empty() {
        let _ = write!(
            svg,
            r#"<g class="cluster-label" transform="translate({tx},{ty})">{inner}</g>"#,
            tx = fmt_num(label_tx),
            ty = fmt_num(label_ty),
            inner = opts.label_svg,
        );
    }
    svg.push_str("</g>");

    ClusterSvg {
        svg,
        bounds: Bounds {
            x,
            y,
            width,
            height,
        },
        label_bbox: opts.label_bbox,
    }
}

fn emit_rounded_with_title(opts: &ClusterEmit<'_>) -> ClusterSvg {
    // Upstream lines 207-272: outer rect + inner rect, the title
    // stretches along the top band (`innerHeight = h - bbox.height - 6`).
    let node = opts.node;
    let padding = node.padding.unwrap_or(0.0);
    let node_w = node.width.unwrap_or(0.0);
    let node_h = node.height.unwrap_or(0.0);
    let cx = node.x.unwrap_or(0.0);
    let cy = node.y.unwrap_or(0.0);

    let width = if node_w <= opts.label_bbox.width + padding {
        opts.label_bbox.width + padding
    } else {
        node_w
    };
    let height = node_h;
    let inner_h = (node_h - opts.label_bbox.height - 6.0).max(0.0);
    let x = cx - width / 2.0;
    let y = cy - height / 2.0;
    let inner_y = cy - node_h / 2.0 + opts.label_bbox.height + 2.0;

    let dom_id = node.dom_id.as_deref().unwrap_or(&node.id);
    let css_classes = node.css_classes.as_deref().unwrap_or("");

    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<g class="{css}" id="{id}" data-id="{did}" data-look="{look}">"#,
        css = escape_attr(css_classes),
        id = escape_attr(dom_id),
        did = escape_attr(&node.id),
        look = node.look.as_deref().unwrap_or("classic"),
    );
    // Outer rect (title band + body background).
    let _ = write!(
        svg,
        r#"<rect class="outer" x="{x}" y="{y}" width="{w}" height="{h}" data-look="{look}"/>"#,
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(width),
        h = fmt_num(height),
        look = node.look.as_deref().unwrap_or("classic"),
    );
    // Inner rect (body) — sits below the title band.
    let _ = write!(
        svg,
        r#"<rect class="inner" x="{x}" y="{iy}" width="{w}" height="{ih}"/>"#,
        x = fmt_num(x),
        iy = fmt_num(inner_y),
        w = fmt_num(width),
        ih = fmt_num(inner_h),
    );
    // Title.
    if !opts.label_svg.is_empty() {
        let label_tx = cx - opts.label_bbox.width / 2.0;
        // Upstream uses `y + 1 - 3` for SVG labels (html labels use 0).
        let label_ty = y + 1.0 - 3.0;
        let _ = write!(
            svg,
            r#"<g class="cluster-label" transform="translate({tx},{ty})">{inner}</g>"#,
            tx = fmt_num(label_tx),
            ty = fmt_num(label_ty),
            inner = opts.label_svg,
        );
    }
    svg.push_str("</g>");

    ClusterSvg {
        svg,
        bounds: Bounds {
            x,
            y,
            width,
            height,
        },
        label_bbox: opts.label_bbox,
    }
}

fn emit_note_group(opts: &ClusterEmit<'_>) -> ClusterSvg {
    let node = opts.node;
    let node_w = node.width.unwrap_or(0.0);
    let node_h = node.height.unwrap_or(0.0);
    let cx = node.x.unwrap_or(0.0);
    let cy = node.y.unwrap_or(0.0);
    let x = cx - node_w / 2.0;
    let y = cy - node_h / 2.0;
    let rx = node.rx.unwrap_or(0.0);
    let ry = node.ry.unwrap_or(0.0);
    let dom_id = node.dom_id.as_deref().unwrap_or(&node.id);

    let svg = format!(
        r#"<g class="note-cluster" id="{id}"><rect x="{x}" y="{y}" width="{w}" \
height="{h}" rx="{rx}" ry="{ry}" fill="none"/></g>"#,
        id = escape_attr(dom_id),
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(node_w),
        h = fmt_num(node_h),
        rx = fmt_num(rx),
        ry = fmt_num(ry),
    );
    ClusterSvg {
        svg,
        bounds: Bounds {
            x,
            y,
            width: node_w,
            height: node_h,
        },
        label_bbox: Bounds::default(),
    }
}

fn emit_divider(opts: &ClusterEmit<'_>) -> ClusterSvg {
    let node = opts.node;
    let node_w = node.width.unwrap_or(0.0);
    let node_h = node.height.unwrap_or(0.0);
    let cx = node.x.unwrap_or(0.0);
    let cy = node.y.unwrap_or(0.0);
    let x = cx - node_w / 2.0;
    let y = cy - node_h / 2.0;
    let dom_id = node.dom_id.as_deref().unwrap_or(&node.id);
    let css_classes = node.css_classes.as_deref().unwrap_or("");

    let svg = format!(
        r#"<g class="{css}" id="{id}" data-look="{look}">\
<rect class="divider" x="{x}" y="{y}" width="{w}" height="{h}" data-look="{look}"/></g>"#,
        css = escape_attr(css_classes),
        id = escape_attr(dom_id),
        look = node.look.as_deref().unwrap_or("classic"),
        x = fmt_num(x),
        y = fmt_num(y),
        w = fmt_num(node_w),
        h = fmt_num(node_h),
    );
    ClusterSvg {
        svg,
        bounds: Bounds {
            x,
            y,
            width: node_w,
            height: node_h,
        },
        label_bbox: Bounds::default(),
    }
}

// ── formatting helpers ──────────────────────────────────────────────

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn escape_attr(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("&quot;"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

// ── tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_node() -> Node {
        let mut n = Node::default();
        n.id = "subgraph-a".into();
        n.dom_id = Some("sg-a".into());
        n.x = Some(100.0);
        n.y = Some(50.0);
        n.width = Some(120.0);
        n.height = Some(80.0);
        n.rx = Some(4.0);
        n.ry = Some(4.0);
        n.padding = Some(8.0);
        n.css_classes = Some("cluster".into());
        n.look = Some("classic".into());
        n.is_group = true;
        n
    }

    fn demo_theme() -> ThemeVariables {
        let mut t = ThemeVariables::default();
        t.cluster_bkg = Some("#ECECFF".into());
        t.cluster_border = Some("#9370DB".into());
        t
    }

    #[test]
    fn cluster_shape_parse_known_names() {
        assert_eq!(ClusterShape::parse("rect"), ClusterShape::Rect);
        assert_eq!(
            ClusterShape::parse("roundedWithTitle"),
            ClusterShape::RoundedWithTitle
        );
        assert_eq!(ClusterShape::parse("divider"), ClusterShape::Divider);
        assert_eq!(ClusterShape::parse("unknown"), ClusterShape::Rect);
    }

    #[test]
    fn rect_cluster_emits_g_rect_and_label_in_order() {
        let node = demo_node();
        let theme = demo_theme();
        let out = emit(&ClusterEmit {
            node: &node,
            theme: &theme,
            shape: ClusterShape::Rect,
            label_bbox: Bounds {
                x: 0.0,
                y: 0.0,
                width: 60.0,
                height: 14.0,
            },
            label_svg: "<text>A</text>",
            title_top_margin: 4.0,
        });
        assert!(out.svg.starts_with(r#"<g class="cluster cluster""#));
        assert!(out
            .svg
            .contains(r#"<rect x="40" y="10" width="120" height="80""#));
        assert!(out.svg.contains("translate(70,14)"));
        assert!(out.svg.contains("<text>A</text>"));
        assert!(out.svg.ends_with("</g>"));
    }

    #[test]
    fn rect_cluster_grows_to_fit_oversize_label() {
        let node = demo_node();
        let theme = demo_theme();
        let out = emit(&ClusterEmit {
            node: &node,
            theme: &theme,
            shape: ClusterShape::Rect,
            label_bbox: Bounds {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 14.0,
            },
            label_svg: "<text>A</text>",
            title_top_margin: 0.0,
        });
        // Label-width (200) + padding (8) = 208, exceeds node.width (120).
        assert!(out.bounds.width > 200.0);
        assert!(out.svg.contains(r#"width="208""#));
    }

    #[test]
    fn rounded_with_title_emits_two_rects() {
        let node = demo_node();
        let theme = demo_theme();
        let out = emit(&ClusterEmit {
            node: &node,
            theme: &theme,
            shape: ClusterShape::RoundedWithTitle,
            label_bbox: Bounds {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 14.0,
            },
            label_svg: "<text>State</text>",
            title_top_margin: 0.0,
        });
        let outer = out.svg.matches(r#"class="outer""#).count();
        let inner = out.svg.matches(r#"class="inner""#).count();
        assert_eq!(outer, 1);
        assert_eq!(inner, 1);
        assert!(out.svg.contains("<text>State</text>"));
    }

    #[test]
    fn note_group_is_invisible() {
        let node = demo_node();
        let theme = demo_theme();
        let out = emit(&ClusterEmit {
            node: &node,
            theme: &theme,
            shape: ClusterShape::NoteGroup,
            label_bbox: Bounds::default(),
            label_svg: "",
            title_top_margin: 0.0,
        });
        assert!(out.svg.contains(r#"fill="none""#));
        assert!(out.svg.contains(r#"class="note-cluster""#));
    }

    #[test]
    fn divider_has_divider_class() {
        let node = demo_node();
        let theme = demo_theme();
        let out = emit(&ClusterEmit {
            node: &node,
            theme: &theme,
            shape: ClusterShape::Divider,
            label_bbox: Bounds::default(),
            label_svg: "",
            title_top_margin: 0.0,
        });
        assert!(out.svg.contains(r#"class="divider""#));
    }

    #[test]
    fn escape_attr_handles_quotes_and_ampersands() {
        assert_eq!(escape_attr(r#"a"b&c"#), "a&quot;b&amp;c");
    }
}
