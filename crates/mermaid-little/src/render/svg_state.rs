//! State-diagram SVG renderer.
//!
//! Upstream reference:
//! * `stateRenderer-v3-unified.ts` (v2 path) — 370 LoC.
//! * `stateRenderer.js` (v1 path) — emits the classic look.
//!
//! # Byte-exactness caveat (wave 4, first pass)
//!
//! Full byte-exact parity requires three pieces that are **not yet**
//! ported and are all in the "hundreds of LoC each" bucket:
//!
//! 1. The stylis CSS minifier applied to the `<style>` block
//!    (`packages/mermaid/src/styles.ts` + the per-diagram CSS at
//!    `state/styles.js`).
//! 2. d3-shape's arc / circle emitter, which upstream uses for
//!    `state-start` markers — output is a 36-vertex cubic-bezier
//!    polyline, not a single `<circle r="7">`.
//! 3. The dagre → cluster-aware SVG pipeline's exact iteration order
//!    for `edgePaths`, `edgeLabels`, `nodes` groups, plus the
//!    `data-points` base64 blob each edge carries.
//!
//! This renderer intentionally produces **structurally plausible** SVG
//! that doesn't pass byte-exact comparison yet but does:
//!   * open `<svg>` with the canonical attribute order;
//!   * emit the standard `<g><defs><marker .../></defs><g class="root">…`
//!     skeleton;
//!   * draw states using `shapes::draw`;
//!   * route edges via `render::edges` with `basis` interpolation;
//!   * apply the `statediagram` class + placeholder `<style>` tag.
//!
//! The `tests` section below compares byte-counts, not byte-equality,
//! and reports the gap against reference output.

use crate::error::Result;
use crate::layout::routing;
use crate::layout::state::StateLayout;
use crate::layout::unified::types::{Bounds, Edge, Node, Point};
use crate::model::state::StateDiagram;
use crate::render::edges::{self, CurveType};
use crate::render::shapes::{self, types::fmt_num};
use crate::render::unified_shell;
use crate::theme::css as theme_css;
use crate::theme::ThemeVariables;
use regex::Regex;
use std::cell::Cell;
use std::sync::OnceLock;

thread_local! {
    /// Effective `look` value for the current `render(..)` call. Set on entry
    /// from `StateDiagram::look_override` (frontmatter `config.look`); falls
    /// back to upstream default `"classic"`. Cluster / node / edge emitters
    /// read this to populate their `data-look="..."` attribute.
    static CURRENT_LOOK: Cell<&'static str> = const { Cell::new("classic") };
}

/// Set `CURRENT_LOOK` for the duration of `f`. We intern a small set of
/// known look identifiers to `&'static str`; unknown values fall back to
/// upstream `"classic"` (silently, mirroring upstream's no-op behaviour).
fn with_look<R>(look: Option<&str>, f: impl FnOnce() -> R) -> R {
    let resolved: &'static str = match look {
        Some("default") => "default",
        Some("classic") => "classic",
        Some("neo") => "neo",
        Some("handDrawn") => "handDrawn",
        _ => "classic",
    };
    let prev = CURRENT_LOOK.with(|c| c.replace(resolved));
    let r = f();
    CURRENT_LOOK.with(|c| c.set(prev));
    r
}

#[inline]
fn current_look() -> &'static str {
    CURRENT_LOOK.with(|c| c.get())
}

pub fn render(
    d: &StateDiagram,
    l: &StateLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    with_look(d.look_override.as_deref(), || render_inner(d, l, theme, id))
}

fn render_inner(
    d: &StateDiagram,
    l: &StateLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(16 * 1024);

    // Compute viewBox from the bounding box of all rendered elements,
    // matching upstream's `svg.node().getBBox()` + `setupViewPortForSVG`
    // (padding = 8 on each side).  Upstream renders first, then calls
    // getBBox; since we can't do that, we compute the union of all node
    // and edge bounding boxes from the layout data.
    let pad = 8.0_f64;
    let title = d.meta.title.as_deref();
    let (vx, vy, vw, vh, title_center_x) = compute_viewbox(l, pad, title);

    // ── Opening <svg> — canonical attribute order -----------------
    let has_acc_descr = d.meta.acc_descr.is_some();
    let has_acc_title = d.meta.acc_title.is_some();
    out.push_str(&unified_shell::open_unified_svg_with_a11y(
        id,
        vw,
        (vx, vy, vw, vh),
        Some("statediagram"),
        "stateDiagram",
        has_acc_descr,
        has_acc_title,
    ));
    // Accessibility elements — must appear immediately after <svg> opens,
    // before the <style> block. Upstream emits <title> before <desc>.
    out.push_str(&unified_shell::emit_a11y_elements(
        id,
        d.meta.acc_title.as_deref(),
        d.meta.acc_descr.as_deref(),
    ));

    // ── <style> block — base preamble + state-specific rules + tail.
    out.push_str(&style_block(id, theme, d));

    // ── Seed <g> wrapping markers + root --------------------------
    out.push_str(unified_shell::open_seed_group());

    // ── Markers -------------------------------------------------
    out.push_str(&format!(
        concat!(
            r#"<defs>"#,
            r#"<marker id="{id}_stateDiagram-barbEnd" refX="19" refY="7""#,
            r#" markerWidth="20" markerHeight="14" markerUnits="userSpaceOnUse" orient="auto">"#,
            r#"<path d="M 19,7 L9,13 L14,7 L9,1 Z"></path>"#,
            r#"</marker>"#,
            r#"</defs>"#,
        ),
        id = id
    ));

    // ── Root <g> with clusters, edges, labels, nodes ------------
    out.push_str(unified_shell::open_root_group());

    // Isolated clusters (no cross-boundary edges) are rendered as inner
    // <g class="root" transform="translate(tx, ty)"> wrappers inside the
    // outer <g class="nodes"> section, mirroring upstream mermaid's
    // recursiveRender path. They are skipped in the outer cluster/edge/node
    // sections; their contents go in the inner root.
    let iso_ids = &l.result.isolated_cluster_ids;

    // Clusters (composite states + note groups) ----------------
    // Skip isolated clusters and clusters that descend from any isolated cluster.
    out.push_str(r#"<g class="clusters">"#);
    for n in l.result.nodes.iter().filter(|n| n.is_group) {
        if iso_ids.contains(&n.id) {
            continue;
        }
        if has_isolated_ancestor(&n.id, &l.result.nodes, iso_ids) {
            continue;
        }
        out.push_str(&emit_cluster(id, n));
    }
    out.push_str("</g>");

    // Edge paths ------------------------------------------------
    // Skip edges whose endpoints are inside an isolated cluster (they
    // are emitted in the inner root's <g class="edgePaths"> instead).
    out.push_str(r#"<g class="edgePaths">"#);
    for e in &l.result.edges {
        if edge_endpoint_is_isolated(e, &l.result.nodes, iso_ids) {
            continue;
        }
        // Skip the original self-loop edge — replaced by 3 cyclic-special
        // segments in `dagre_bridge::expand_self_edge`.
        if is_replaced_self_loop(e) {
            continue;
        }
        out.push_str(&emit_edge_path(id, e, &l.result.nodes));
    }
    out.push_str("</g>");

    // Edge labels ----------------------------------------------
    out.push_str(r#"<g class="edgeLabels">"#);
    for e in &l.result.edges {
        if edge_endpoint_is_isolated(e, &l.result.nodes, iso_ids) {
            continue;
        }
        if is_replaced_self_loop(e) {
            continue;
        }
        out.push_str(&emit_edge_label(e, &l.result.nodes));
    }
    out.push_str("</g>");

    // Nodes -----------------------------------------------------
    out.push_str(r#"<g class="nodes">"#);

    // Walk `l.result.nodes` in insertion (source) order, emitting:
    //   - top-level isolated clusters as inner `<g class="root">` wrappers,
    //   - regular leaf nodes that live outside any isolated cluster.
    //
    // Iterating once preserves the parser's source-declaration order — the
    // upstream renderer's `recursiveRender` walks the doc in declaration
    // order, so an outer `[*]` declared before `state TV { … }` must be
    // emitted before the TV cluster's inner root group.
    for node in l.result.nodes.iter() {
        if node.is_group {
            if !iso_ids.contains(&node.id) {
                continue;
            }
            let parent_also_isolated = node
                .parent_id
                .as_deref()
                .map(|p| iso_ids.contains(p))
                .unwrap_or(false);
            if parent_also_isolated {
                continue;
            }
            out.push_str(&render_isolated_cluster_inner_root(
                node, &l.result, theme, id,
            ));
        } else {
            if node.extra.get("__skip_render").is_some() {
                continue;
            }
            if has_isolated_ancestor(&node.id, &l.result.nodes, iso_ids) {
                continue;
            }
            if let Some(svg) = emit_node(id, node, theme) {
                out.push_str(&svg);
            }
        }
    }
    out.push_str("</g>");

    out.push_str(unified_shell::close_root_group());
    out.push_str(unified_shell::close_seed_group());

    // Drop-shadow filter defs (match upstream tail).
    out.push_str(&unified_shell::emit_defs_shell(id, true, true));
    // Optional theme gradient defs — see `emit_gradient_defs`.
    out.push_str(&unified_shell::emit_gradient_defs(id, theme));

    // Diagram title — upstream's `insertTitle` appends a <text> element
    // AFTER the main content and drop-shadow defs but BEFORE </svg>.
    // The title x is the center of the content bounding box (before the
    // title itself widens it). y is fixed at -25.
    if let Some(t) = title {
        if !t.is_empty() {
            out.push_str(&format!(
                r#"<text text-anchor="middle" x="{x}" y="-25" class="statediagramTitleText">{text}</text>"#,
                x = fmt_num(title_center_x),
                text = xml_escape(t),
            ));
        }
    }

    out.push_str(unified_shell::close_unified_svg());
    Ok(out)
}

/// Compute the viewBox by unioning the bounding boxes of all nodes
/// and edge paths/labels, then adding `pad` on each side. This mirrors
/// upstream's `svg.node().getBBox()` → `setupViewPortForSVG` flow.
///
/// Upstream's getBBox() returns the bounding box of the rendered SVG
/// content. For state diagrams, the key observation is that the
/// viewBox left/top edges align with `-(max_half_width + pad)` and
/// `-(max_half_height + pad)`, where max_half_width/height come from
/// the regular state nodes (not start/end circles). The right/bottom
/// edges are derived from the actual content extent.
/// Round to 3 decimal places — matches d3's `appendRound(3)` / `fmt_coord`
/// rounding used for SVG path coordinates. The upstream getBBox() is called
/// on the rendered SVG, so viewBox extents must use rounded path coords.
#[inline]
fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Compute the bounding box (x_min, x_max, y_min, y_max) of a d3 curveBasis
/// path rendered from `points`. Mirrors the upstream flow where getBBox() is
/// called on the actual SVG <path> element. Chrome/WebKit's getBBox for cubic
/// Bezier paths uses the convex hull (control point bbox), not the geometric
/// tight bbox. So we scan ALL path coordinate values (endpoints + control
/// points) after 3-decimal rounding, matching the browser's approach.
fn basis_spline_tight_bbox(
    points: &[crate::layout::unified::types::Point],
) -> (f64, f64, f64, f64) {
    if points.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let mut x_min = f64::INFINITY;
    let mut x_max = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;

    // Expand bbox with one coordinate pair (rounded).
    macro_rules! add_coord {
        ($px:expr, $py:expr) => {{
            let rx = round3($px);
            let ry = round3($py);
            if rx < x_min {
                x_min = rx;
            }
            if rx > x_max {
                x_max = rx;
            }
            if ry < y_min {
                y_min = ry;
            }
            if ry > y_max {
                y_max = ry;
            }
        }};
    }

    // Scan all path coordinates including control points of each cubic.
    // Mirrors the Chrome getBBox control-point scanning behaviour.
    macro_rules! scan_cubic {
        ($c1x:expr, $c1y:expr, $c2x:expr, $c2y:expr, $ex:expr, $ey:expr) => {{
            add_coord!($c1x, $c1y);
            add_coord!($c2x, $c2y);
            add_coord!($ex, $ey);
        }};
    }

    // Re-implement the path_basis coordinate walk.
    let mut x0 = f64::NAN;
    let mut x1 = f64::NAN;
    let mut y0 = f64::NAN;
    let mut y1 = f64::NAN;
    let mut state: u8 = 0;

    for p in points {
        let (x, y) = (p.x, p.y);
        match state {
            0 => {
                // M command — just the start point.
                add_coord!(x, y);
                state = 1;
            }
            1 => {
                state = 2;
            }
            2 => {
                // L anchor + first cubic.
                let lpx = (5.0 * x0 + x1) / 6.0;
                let lpy = (5.0 * y0 + y1) / 6.0;
                add_coord!(lpx, lpy);
                scan_cubic!(
                    (2.0 * x0 + x1) / 3.0,
                    (2.0 * y0 + y1) / 3.0,
                    (x0 + 2.0 * x1) / 3.0,
                    (y0 + 2.0 * y1) / 3.0,
                    (x0 + 4.0 * x1 + x) / 6.0,
                    (y0 + 4.0 * y1 + y) / 6.0
                );
                state = 3;
            }
            _ => {
                scan_cubic!(
                    (2.0 * x0 + x1) / 3.0,
                    (2.0 * y0 + y1) / 3.0,
                    (x0 + 2.0 * x1) / 3.0,
                    (y0 + 2.0 * y1) / 3.0,
                    (x0 + 4.0 * x1 + x) / 6.0,
                    (y0 + 4.0 * y1 + y) / 6.0
                );
            }
        }
        x0 = x1;
        x1 = x;
        y0 = y1;
        y1 = y;
    }

    // lineEnd
    match state {
        3 => {
            // Tail cubic (emit_basis_cubic with last point repeated).
            scan_cubic!(
                (2.0 * x0 + x1) / 3.0,
                (2.0 * y0 + y1) / 3.0,
                (x0 + 2.0 * x1) / 3.0,
                (y0 + 2.0 * y1) / 3.0,
                (x0 + 4.0 * x1 + x1) / 6.0,
                (y0 + 4.0 * y1 + y1) / 6.0
            );
            // Final L to last point.
            add_coord!(x1, y1);
        }
        2 => {
            add_coord!(x1, y1);
        }
        _ => {}
    }
    (x_min, x_max, y_min, y_max)
}

/// Returns `(vx, vy, vw, vh, title_center_x)`.
///
/// `title_center_x` is the center of the content-only bounding box (before
/// the title text widens it), matching upstream's `insertTitle` which records
/// the bounds center before adding the `<text>` element.
fn compute_viewbox(l: &StateLayout, pad: f64, title: Option<&str>) -> (f64, f64, f64, f64, f64) {
    // Simulate upstream's `svg.node().getBBox()` with the jsdom shim used
    // to generate reference SVGs.  That shim collects intrinsic bounding
    // boxes of all SVG primitives (rect, circle, path, foreignObject …)
    // while **ignoring all transform attributes**.  Since every node group
    // carries a `transform="translate(cx,cy)"` that the shim skips, each
    // node contributes its LOCAL-coordinate bbox to the union:
    //
    //   regular state node  →  rect: {x:-w/2, y:-h/2, w, h}
    //                          foreignObject: {x:0, y:0, w:lw, h:lh}
    //                          union: {x:-w/2, x_max:max(w/2,lw), y:-h/2, y_max:max(h/2,lh)}
    //
    //   start/end circle    →  circle at cx=0,cy=0,r=7: {x:-7, y:-7, w:14, h:14}
    //
    // Edge paths live in <g class="edgePaths"> with no transform, so their
    // `d`-attribute coordinates are already in the same space as node locals
    // (i.e. the layout's absolute space — both merge correctly).

    let mut g_x_min: f64 = f64::INFINITY;
    let mut g_x_max: f64 = f64::NEG_INFINITY;
    let mut g_y_min: f64 = f64::INFINITY;
    let mut g_y_max: f64 = f64::NEG_INFINITY;

    // NoteGroup clusters: rendered as <g class="note-cluster"> with absolute
    // coordinate rect (no transform). Include their absolute bounds in the
    // viewbox computation.
    for n in l
        .result
        .nodes
        .iter()
        .filter(|n| n.is_group && n.shape.as_deref() == Some("noteGroup"))
    {
        if let (Some(cx), Some(cy), Some(w), Some(h)) = (n.x, n.y, n.width, n.height) {
            let x_left = cx - w / 2.0;
            let x_right = cx + w / 2.0;
            let y_top = cy - h / 2.0;
            let y_bottom = cy + h / 2.0;
            g_x_min = g_x_min.min(x_left);
            g_x_max = g_x_max.max(x_right);
            g_y_min = g_y_min.min(y_top);
            g_y_max = g_y_max.max(y_bottom);
        }
    }

    // Composite-state cluster outer/inner rects. These contribute their
    // absolute coordinates to the global bbox union — jsdom getBBox ignores
    // ALL transforms, so the rect contributes its cnode-local coordinates
    // (which equal absolute when the rect sits in an outer `<g class="root">`
    // or `<g class="clusters">` group with no transform).
    //
    // Both isolated clusters (rendered inside an inner-root group) and
    // non-isolated clusters (rendered directly under the outer cluster group)
    // emit `<rect class="outer" x="cx - w/2" y="cy - h/2" w h>` in the SVG,
    // so both must be unioned into the viewbox bbox.
    let iso_ids = &l.result.isolated_cluster_ids;
    for n in l.result.nodes.iter().filter(|n| n.is_group) {
        // Skip noteGroup wrappers (already handled above) and divider clusters
        // (their rect is included via the `divider` shape branch below).
        let shape = n.shape.as_deref().unwrap_or("");
        if shape == "noteGroup" {
            continue;
        }
        if let (Some(cx), Some(cy), Some(w), Some(h)) = (n.x, n.y, n.width, n.height) {
            let x_left = cx - w / 2.0;
            let x_right = cx + w / 2.0;
            let y_top = cy - h / 2.0;
            let y_bottom = cy + h / 2.0;
            g_x_min = g_x_min.min(x_left);
            g_x_max = g_x_max.max(x_right);
            g_y_min = g_y_min.min(y_top);
            g_y_max = g_y_max.max(y_bottom);

            // Cluster-title foreignObject. The label sits in a `<g
            // class="cluster-label" transform="translate(lx, ly)"><foreignObject
            // width="lw" height="lh">…</foreignObject></g>` chain. jsdom ignores
            // every transform, so the foreignObject contributes its LOCAL
            // bbox `[0, lw] × [0, lh]` to the global getBBox union.
            //
            // Only `rect` (composite state) and `roundedWithTitle` clusters
            // emit a label foreignObject. Apply the contribution only to
            // isolated clusters for now — non-isolated cluster labels already
            // fit within the outer rect's right edge in known fixtures, and
            // adding them unconditionally caused width regressions on cy/30.
            if iso_ids.contains(&n.id) && shape == "rect" {
                if let Some(label) = n.label.as_deref() {
                    if !label.is_empty() {
                        use crate::render::foreign_object::{
                            measure_html_markup_label, HtmlLabelFont,
                        };
                        let escaped = xml_escape(label);
                        let font = HtmlLabelFont::default();
                        let (lw, lh) = measure_html_markup_label(&escaped, &font, 200.0, true);
                        g_x_min = g_x_min.min(0.0);
                        g_x_max = g_x_max.max(lw);
                        g_y_min = g_y_min.min(0.0);
                        g_y_max = g_y_max.max(lh);
                    }
                }
            }
        }
    }

    for n in &l.result.nodes {
        if n.is_group || n.extra.get("__skip_render").is_some() {
            continue;
        }
        let shape = n.shape.as_deref().unwrap_or("state");
        let (nx_min, nx_max, ny_min, ny_max) = if matches!(
            shape,
            "stateStart"
                | "state_start"
                | "start"
                | "stateEnd"
                | "state_end"
                | "end"
                | "forkJoin"
                | "fork_join"
                | "fork"
                | "join"
        ) {
            // Circle or rectangle at local origin.
            // Use n.width/2 as the half-extent so that stateEnd (width =
            // 14.0177…) contributes the correct rough-path bounding box.
            let hw = n.width.unwrap_or(14.0) / 2.0;
            let hh = n.height.unwrap_or(14.0) / 2.0;
            (-hw, hw, -hh, hh)
        } else if shape == "rectWithTitle" {
            // rectWithTitle (state with description): outer rect + two
            // foreignObjects. jsdom ignores ALL transforms, so both FOs
            // contribute local (0, text_w)×(0, lh) bboxes.
            //
            // Node layout: width W = max_text_w + padding, height H = lh + padding.
            // Local bbox union:
            //   rect:  (-W/2, W/2)×(-H/2, H/2)
            //   FOs:   (0, W-padding)×(0, lh)
            // Union:  (-W/2, W-padding)×(-H/2, lh)
            let w = n.width.unwrap_or(0.0);
            let h = n.height.unwrap_or(0.0);
            let padding = n.label_padding_x.unwrap_or(8.0); // node.padding
            let lh_fo = h - padding; // = line_height (stored in height = lh + padding)
            let text_max_w = w - padding; // = max(title_w, desc_w)
            let hw = w / 2.0;
            let hh = h / 2.0;
            (-hw, text_max_w.max(hw), -hh, lh_fo.max(hh))
        } else if shape == "note" {
            // Note shape: rough-path rect (-hw..hw, -hh..hh) plus foreignObject
            // (0..fw, 0..fh) where fw = w - 2*15 (note padding = 15).
            let w = n.width.unwrap_or(0.0);
            let h = n.height.unwrap_or(0.0);
            const NOTE_PAD: f64 = 15.0;
            let fw = (w - 2.0 * NOTE_PAD).max(0.0);
            let fh = (h - 2.0 * NOTE_PAD).max(0.0);
            let hw = w / 2.0;
            let hh = h / 2.0;
            // Local bbox = union of rough-path rect (-hw..hw)×(-hh..hh)
            // and foreignObject [0..fw]×[0..fh] (at local origin).
            (-hw, fw.max(hw), -hh, fh.max(hh))
        } else {
            let w = n.width.unwrap_or(0.0);
            let h = n.height.unwrap_or(0.0);
            let padx = n.label_padding_x.unwrap_or(8.0);
            let pady = n.label_padding_y.unwrap_or(8.0);
            // foreignObject dimensions (label content area).
            let lw = (w - 2.0 * padx).max(0.0);
            let lh = (h - 2.0 * pady).max(0.0);
            let hw = w / 2.0;
            let hh = h / 2.0;
            // Local bbox = union of rect and foreignObject.
            (-hw, lw.max(hw), -hh, lh.max(hh))
        };
        g_x_min = g_x_min.min(nx_min);
        g_x_max = g_x_max.max(nx_max);
        g_y_min = g_y_min.min(ny_min);
        g_y_max = g_y_max.max(ny_max);
    }

    // Edge paths — compute tight bounding box of the rendered basis spline.
    // The upstream getBBox() is called on the *rendered* SVG path element
    // (whose coordinates are rounded to 3 decimal places by fmt_coord).
    // For a basis spline, the actual path extent can be smaller than the
    // convex hull of the control points, so we compute the tight bbox.
    //
    // Also account for edge labels: the label foreignObject is at
    // (label_x - lw/2, label_y - lh/2) with width=lw, height=lh.
    // The upstream jsdom shim includes foreignObject bboxes in getBBox()
    // without applying transforms (since the edgeLabel <g> has a transform,
    // but the foreignObject's bbox is in the LOCAL coordinate of the
    // transformed group, so we use label_x ± lw/2 directly).
    for e in &l.result.edges {
        if let Some(pts) = e.points.as_deref() {
            let (bx_min, bx_max, by_min, by_max) = basis_spline_tight_bbox(pts);
            g_x_min = g_x_min.min(bx_min);
            g_x_max = g_x_max.max(bx_max);
            g_y_min = g_y_min.min(by_min);
            g_y_max = g_y_max.max(by_max);
        }
        // Edge label bbox.
        // The upstream jsdom shim used to generate reference SVGs calls
        // getBBox() without applying CSS/SVG transforms. So a foreignObject
        // with width=lw, height=lh inside nested <g transform=...> groups
        // contributes its UNTRANSFORMED local bbox [0,lw]×[0,lh] to the
        // global getBBox union. We replicate this behavior here.
        if e.label_x.is_some() {
            // Get label text and measure width.
            let raw_label = e.label.as_deref().unwrap_or("");
            if !raw_label.trim().is_empty() {
                // Decode mermaid entities for accurate width measurement.
                let decoded = decode_mermaid_entities(raw_label);
                if !decoded.trim().is_empty() {
                    use crate::font_metrics::text_width as ftw;
                    let lw = ftw(decoded.trim(), "sans-serif", 14.0, false, false);
                    let lh = 16.296875_f64; // one line height
                                            // foreignObject LOCAL bbox (ignoring parent transforms): [0,lw]×[0,lh].
                    g_x_min = g_x_min.min(0.0);
                    g_x_max = g_x_max.max(lw);
                    g_y_min = g_y_min.min(0.0);
                    g_y_max = g_y_max.max(lh);
                }
            }
        }
    }

    // Fall back when the layout has no renderable content at all.
    if !g_x_min.is_finite() {
        return (
            -(7.0 + pad),
            -(7.0 + pad),
            14.0 + 2.0 * pad,
            14.0 + 2.0 * pad,
            0.0,
        );
    }

    // Record center of the content-only bounding box — this is the x
    // coordinate used for the diagram title <text> element, matching
    // upstream's `insertTitle` which saves `bounds.x + bounds.width/2`
    // (i.e. g_x_min + (g_x_max-g_x_min)/2) BEFORE adding the text.
    let content_center_x = g_x_min + (g_x_max - g_x_min) / 2.0;

    // Expand bounds to include the diagram title text if present.
    // The title <text> element has no inline font attributes, so jsdom's
    // resolveFont falls back to default: sans-serif 14px.
    // Its intrinsicBox is {x:0, y:0, width:text_width, height:lh} (x,y
    // are always 0 regardless of the `x` attribute).
    if let Some(t) = title {
        if !t.is_empty() {
            use crate::font_metrics::text_width as ftw;
            let tw = ftw(t, "sans-serif", 14.0, false, false);
            let lh = 16.296875_f64;
            g_x_min = g_x_min.min(0.0);
            g_x_max = g_x_max.max(tw);
            g_y_min = g_y_min.min(0.0);
            g_y_max = g_y_max.max(lh);
        }
    }

    let vx = g_x_min - pad;
    let vy = g_y_min - pad;
    let vw = (g_x_max - g_x_min) + 2.0 * pad;
    let vh = (g_y_max - g_y_min) + 2.0 * pad;

    (vx, vy, vw.max(1.0), vh.max(1.0), content_center_x)
}

fn viewbox(b: &Bounds, pad: f64) -> (f64, f64, f64, f64) {
    let w = (b.width + 2.0 * pad).max(1.0);
    let h = (b.height + 2.0 * pad).max(1.0);
    let x = b.x - pad;
    let y = b.y - pad;
    (x, y, w, h)
}

fn emit_cluster(svg_id: &str, n: &Node) -> String {
    let w = n.width.unwrap_or(0.0);
    let h = n.height.unwrap_or(0.0);
    let dom_id = n.dom_id.as_deref().unwrap_or(&n.id);
    // Note groups: render as <g class="note-cluster"> with an absolute-coordinate rect.
    if n.shape.as_deref() == Some("noteGroup") {
        let cx = n.x.unwrap_or(0.0);
        let cy = n.y.unwrap_or(0.0);
        return format!(
            r#"<g class="note-cluster" id="{svg_id}-{dom_id}"><rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none"></rect></g>"#,
            svg_id = svg_id,
            dom_id = xml_escape(dom_id),
            x = fmt_num(cx - w / 2.0),
            y = fmt_num(cy - h / 2.0),
            w = fmt_num(w),
            h = fmt_num(h),
        );
    }
    // Regular composite state cluster — share the rich-shape emitter with
    // the isolated-cluster path so that data-id, class, label translate, and
    // inner-rect attributes match upstream's `roundedWithTitle` shape.
    emit_isolated_cluster_shape(n, svg_id)
}

/// Walk up the parent chain from `node_id`. Return true if any ancestor is in
/// `iso_ids`. Used to decide whether a node/cluster should be skipped at the
/// outer render level (because it lives inside an isolated cluster's inner
/// root group).
fn has_isolated_ancestor(
    node_id: &str,
    nodes: &[Node],
    iso_ids: &std::collections::HashSet<String>,
) -> bool {
    let mut current = node_id;
    loop {
        match nodes
            .iter()
            .find(|n| n.id == current)
            .and_then(|n| n.parent_id.as_deref())
        {
            Some(p) => {
                if iso_ids.contains(p) {
                    return true;
                }
                current = p;
            }
            None => return false,
        }
    }
}

/// True for the user-level edge that has been replaced by `expand_self_edge`
/// in `dagre_bridge`. Its `start` and `end` are equal — the dagre graph
/// dropped the direct `setEdge(v, v)` call and inserted three cyclic-special
/// segments instead. The original user-level Edge is still in
/// `LayoutData.edges` (we keep it for label/styling templates), but the
/// renderer must not emit a path or label for it; that role is taken over by
/// the synthetic `cyclic-special-mid` segment.
///
/// Synthetic cyclic-special segments themselves carry `synthetic="cyclic_segment"`
/// — they are NOT replaced and must still be rendered.
fn is_replaced_self_loop(e: &Edge) -> bool {
    if e.extra.get("synthetic").map(|s| s.as_str()) == Some("cyclic_segment") {
        return false;
    }
    let os = e
        .extra
        .get("orig_start")
        .map(|s| s.as_str())
        .or(e.start.as_deref())
        .unwrap_or("");
    let od = e
        .extra
        .get("orig_end")
        .map(|s| s.as_str())
        .or(e.end.as_deref())
        .unwrap_or("");
    !os.is_empty() && os == od
}

/// Return true if either of the edge's endpoints (or any ancestor of either)
/// is an isolated cluster's interior — meaning the edge belongs to that
/// cluster's inner-root and must be skipped at the top-level edge pass.
///
/// Cluster-level boundary edges (where an endpoint IS an isolated cluster id
/// itself, rather than a descendant of one) are NOT considered "inside" — they
/// connect a cluster super-node to its sibling and belong in the outer
/// `<g class="edgePaths">`. This includes:
///   - cluster-to-cluster edges (both endpoints are cluster ids)
///   - cluster-to-leaf or leaf-to-cluster edges where the cluster is isolated
///     (e.g. `[*] --> TV` in cy/22 where TV is an isolated composite state).
fn edge_endpoint_is_isolated(
    e: &Edge,
    nodes: &[Node],
    iso_ids: &std::collections::HashSet<String>,
) -> bool {
    // An endpoint is "inside" an isolated cluster only if it is a strict
    // descendant of the cluster — being the cluster itself does not count.
    // This matches upstream's recursiveRender flow: outer dagre handles edges
    // whose endpoints are top-level (including isolated cluster super-nodes);
    // inner dagre handles edges fully contained within a cluster's interior.
    let in_iso_interior = |id: &str| has_isolated_ancestor(id, nodes, iso_ids);
    if let Some(s) = e.start.as_deref() {
        if in_iso_interior(s) {
            return true;
        }
    }
    if let Some(d) = e.end.as_deref() {
        if in_iso_interior(d) {
            return true;
        }
    }
    false
}

/// True if `node_id` is a transitive descendant of `cluster_id`.
fn is_descendant_of(node_id: &str, cluster_id: &str, nodes: &[Node]) -> bool {
    let mut current = node_id;
    loop {
        match nodes
            .iter()
            .find(|n| n.id == current)
            .and_then(|n| n.parent_id.as_deref())
        {
            Some(p) => {
                if p == cluster_id {
                    return true;
                }
                current = p;
            }
            None => return false,
        }
    }
}

/// Render an isolated cluster as an inner `<g class="root" transform="...">`
/// group (mirrors upstream mermaid's `recursiveRender` for clusters with no
/// cross-boundary edges).
///
/// Layout pre-computed `outer_tx`/`outer_ty` in `cnode.extra` from
/// `dagre_bridge::layout`'s positionNode formula:
///   tx = outer_x - padding - bbox_w/2
///   ty = outer_y - bbox_h/2 - padding
///
/// The cluster's own `<rect class="outer">`, label, and `<rect class="inner">`
/// sit inside the inner root group at absolute coordinates derived from the
/// inner dagre's `cnode.x/y/width/height` and the label bbox.
///
/// Recursive: nested isolated clusters get their own inner roots.
fn render_isolated_cluster_inner_root(
    cnode: &Node,
    l: &crate::layout::unified::types::LayoutResult,
    theme: &ThemeVariables,
    svg_id: &str,
) -> String {
    let iso_ids = &l.isolated_cluster_ids;
    let cluster_padding = 8.0_f64;

    // Outer translate — pre-computed by the layout engine.
    let tx = cnode
        .extra
        .get("outer_tx")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or_else(|| {
            let cx = cnode.x.unwrap_or(0.0);
            let w = cnode.width.unwrap_or(0.0);
            cx - cluster_padding - w / 2.0
        });
    let ty = cnode
        .extra
        .get("outer_ty")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or_else(|| {
            let cy = cnode.y.unwrap_or(0.0);
            let h = cnode.height.unwrap_or(0.0);
            cy - h / 2.0 - cluster_padding
        });

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="root" transform="translate({tx}, {ty})">"#,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));

    // Inner <g class="clusters"> ─────────────────────────────────────
    // This cluster's own rect/label/inner-rect, plus any non-isolated
    // sub-cluster rects (direct children only). Nested isolated sub-clusters
    // get their own inner root groups inside <g class="nodes">.
    out.push_str(r#"<g class="clusters">"#);
    out.push_str(&emit_isolated_cluster_shape(cnode, svg_id));
    for n in l.nodes.iter() {
        if !n.is_group {
            continue;
        }
        if n.parent_id.as_deref() != Some(cnode.id.as_str()) {
            continue;
        }
        if iso_ids.contains(&n.id) {
            continue;
        }
        // Non-isolated sub-cluster: emit using the legacy emit_cluster path.
        out.push_str(&emit_cluster(svg_id, n));
    }
    out.push_str("</g>");

    // Inner <g class="edgePaths"> — edges between descendants of this cluster
    // that are not inside any isolated sub-cluster.
    out.push_str(r#"<g class="edgePaths">"#);
    for e in l.edges.iter() {
        let src = match e.start.as_deref() {
            Some(s) => s,
            None => continue,
        };
        let dst = match e.end.as_deref() {
            Some(s) => s,
            None => continue,
        };
        if !is_descendant_of(src, &cnode.id, &l.nodes)
            || !is_descendant_of(dst, &cnode.id, &l.nodes)
        {
            continue;
        }
        // Skip if either endpoint is in a sub-isolated cluster.
        if endpoint_in_sub_isolated(src, cnode.id.as_str(), &l.nodes, iso_ids)
            || endpoint_in_sub_isolated(dst, cnode.id.as_str(), &l.nodes, iso_ids)
        {
            continue;
        }
        out.push_str(&emit_edge_path(svg_id, e, &l.nodes));
    }
    out.push_str("</g>");

    // Inner <g class="edgeLabels">.
    out.push_str(r#"<g class="edgeLabels">"#);
    for e in l.edges.iter() {
        let src = match e.start.as_deref() {
            Some(s) => s,
            None => continue,
        };
        let dst = match e.end.as_deref() {
            Some(s) => s,
            None => continue,
        };
        if !is_descendant_of(src, &cnode.id, &l.nodes)
            || !is_descendant_of(dst, &cnode.id, &l.nodes)
        {
            continue;
        }
        if endpoint_in_sub_isolated(src, cnode.id.as_str(), &l.nodes, iso_ids)
            || endpoint_in_sub_isolated(dst, cnode.id.as_str(), &l.nodes, iso_ids)
        {
            continue;
        }
        out.push_str(&emit_edge_label(e, &l.nodes));
    }
    out.push_str("</g>");

    // Inner <g class="nodes"> — sub-isolated cluster wrappers + leaf nodes.
    //
    // Emit in source-declaration order (the layout `nodes` vec is insertion-
    // ordered by the parser): for each candidate that lives under `cnode`,
    // dispatch to either the nested-isolated-cluster recursion or the leaf
    // `emit_node` path. Mixing both kinds in a single insertion-order pass
    // matches upstream mermaid's `recursiveRender`, which iterates the parent
    // cluster's children once and renders each in declaration order regardless
    // of whether it is a leaf or a sub-cluster.
    out.push_str(r#"<g class="nodes">"#);

    for n in l.nodes.iter() {
        if n.is_group {
            // Direct sub-isolated cluster: recurse into nested inner-root.
            if n.parent_id.as_deref() != Some(cnode.id.as_str()) {
                continue;
            }
            if !iso_ids.contains(&n.id) {
                continue;
            }
            out.push_str(&render_isolated_cluster_inner_root(n, l, theme, svg_id));
        } else {
            // Leaf node: any descendant of `cnode` that is not inside a
            // sub-isolated cluster (those are emitted by the recursive call
            // above, inside their own inner root).
            if n.extra.get("__skip_render").is_some() {
                continue;
            }
            if !is_descendant_of(&n.id, &cnode.id, &l.nodes) {
                continue;
            }
            if endpoint_in_sub_isolated(&n.id, cnode.id.as_str(), &l.nodes, iso_ids) {
                continue;
            }
            if let Some(svg) = emit_node(svg_id, n, theme) {
                out.push_str(&svg);
            }
        }
    }
    out.push_str("</g>");

    let _ = theme; // theme reserved for future per-theme cluster styling.

    out.push_str("</g>"); // close inner root
    out
}

/// True when `node_id` belongs to an isolated sub-cluster of `parent_cluster_id`
/// (i.e. one of its ancestors is an isolated cluster other than `parent_cluster_id`).
fn endpoint_in_sub_isolated(
    node_id: &str,
    parent_cluster_id: &str,
    nodes: &[Node],
    iso_ids: &std::collections::HashSet<String>,
) -> bool {
    let mut current = node_id;
    loop {
        match nodes
            .iter()
            .find(|n| n.id == current)
            .and_then(|n| n.parent_id.as_deref())
        {
            Some(p) => {
                if p == parent_cluster_id {
                    return false;
                }
                if iso_ids.contains(p) {
                    return true;
                }
                current = p;
            }
            None => return false,
        }
    }
}

/// Emit the cluster's own `<g>` containing the outer rect, label, and inner rect.
/// Mirrors upstream mermaid's `roundedWithTitle` cluster shape:
///   outer rect at (cx - w/2, cy - h/2, w, h)
///   inner rect at (cx - w/2, cy - h/2 + bbox.h + 2, w, h - bbox.h - 6)
///   label translate (cx - bbox.w/2, cy - h/2 + 1)
fn emit_isolated_cluster_shape(n: &Node, svg_id: &str) -> String {
    use crate::render::foreign_object::{measure_html_markup_label, HtmlLabelFont};

    let w = n.width.unwrap_or(0.0);
    let h = n.height.unwrap_or(0.0);
    let cx = n.x.unwrap_or(0.0);
    let cy = n.y.unwrap_or(0.0);
    let dom_id = n.dom_id.as_deref().unwrap_or(&n.id);
    let raw_id = n.id.as_str();
    let label = n.label.as_deref().unwrap_or("");

    // Divider-shape cluster (a `--` partition wrapper): no label, no inner
    // rect, just a single `<rect class="divider">` wrapped in `<g>`. Mirrors
    // upstream's `divider` cluster shape (chunk-C7LX3TON.mjs):
    //   shapeSvg.insert("g").attr("class", cssClasses).attr("id", domId)
    //     .insert("g", ":first-child")
    //     .insert("rect", ":first-child")
    //     .attr("class", "divider").attr(x,y,w,h, data-look)
    // No `data-id` is emitted (upstream's divider shape skips the id setter).
    if n.shape.as_deref() == Some("divider") {
        let css = n.css_classes.as_deref().unwrap_or("statediagram-cluster");
        let trimmed = css.trim();
        let css_attr =
            if trimmed.contains("statediagram-state") && trimmed.contains("statediagram-cluster") {
                // Keep the full class string verbatim, prefixed with a leading
                // space so the resulting attribute reads ` statediagram-state ...`
                // exactly like upstream's dataFetcher template.
                let last_is_cluster = trimmed
                    .split_ascii_whitespace()
                    .next_back()
                    .map(|t| t == "statediagram-cluster")
                    .unwrap_or(false);
                if last_is_cluster {
                    format!(" {} ", trimmed)
                } else {
                    format!(" {}", trimmed)
                }
            } else {
                // Synthesize a default for safety; layout normally provides the
                // full string.
                " statediagram-state statediagram-cluster ".to_string()
            };

        let rx = cx - w / 2.0;
        let ry = cy - h / 2.0;
        return format!(
            r#"<g class="{css}" id="{svg_id}-{dom_id}" data-look="{look}"><g><rect class="divider" x="{x}" y="{y}" width="{w}" height="{h}" data-look="{look}"></rect></g></g>"#,
            css = css_attr,
            svg_id = svg_id,
            dom_id = xml_escape(dom_id),
            look = current_look(),
            x = fmt_num(rx),
            y = fmt_num(ry),
            w = fmt_num(w),
            h = fmt_num(h),
        );
    }

    // Measure label dimensions (HTML markup path — upstream's createLabel +
    // getBoundingClientRect on the foreignObject's div).
    let escaped = xml_escape(label);
    let font = HtmlLabelFont::default();
    let (lw, lh) = if escaped.is_empty() {
        (0.0, 0.0)
    } else {
        measure_html_markup_label(&escaped, &font, 200.0, true)
    };

    // Cluster rect coordinates (absolute within the inner root group).
    // Upstream `roundedWithTitle`: padding=0, x = node.x - width/2, y = node.y - height/2.
    let rx = cx - w / 2.0;
    let ry = cy - h / 2.0;
    // Inner rect: y = node.y - node.height/2 + bbox.height + 2,
    //             height = node.height - bbox.height - 6.
    let inner_y = cy - h / 2.0 + lh + 2.0;
    let inner_h = h - lh - 6.0;
    // Label translate: (node.x - bbox.width/2, y + 1).
    let label_tx = cx - lw / 2.0;
    let label_ty = ry + 1.0;

    // CSS class string. Upstream `dataFetcher` builds the cluster's class
    // list as
    //   `${cssClasses} ${CSS_DIAGRAM_CLUSTER} ${altFlag ? CSS_DIAGRAM_CLUSTER_ALT : ""}`
    // The final interpolation is empty when the cluster sits at an odd
    // depth, leaving a TRAILING SPACE; at even depth the suffix is
    // `statediagram-cluster-alt` and the string has NO trailing space.
    // Either form gets emitted verbatim as `class=" ${cssClasses}"`.
    //
    // Our state layout already mirrors that template into `n.css_classes`
    // (without the leading space). Prefer it verbatim; fall back to the
    // canonical default only when missing entirely.
    let css = n.css_classes.as_deref().unwrap_or("statediagram-cluster");
    let trimmed = css.trim();
    let css_attr = if trimmed.is_empty() {
        " statediagram-state statediagram-cluster ".to_string()
    } else if trimmed == "statediagram-cluster" {
        // Bare layout default — wrap with the diagram-state prefix.
        " statediagram-state statediagram-cluster ".to_string()
    } else if trimmed.contains("statediagram-state") && trimmed.contains("statediagram-cluster") {
        // Already a fully-qualified class list. Replicate upstream's exact
        // trailing-space behaviour: keep the trailing space iff the
        // statediagram-cluster token is the last token (i.e. no -alt
        // suffix).
        let last_is_cluster = trimmed
            .split_ascii_whitespace()
            .next_back()
            .map(|t| t == "statediagram-cluster")
            .unwrap_or(false);
        if last_is_cluster {
            format!(" {} ", trimmed)
        } else {
            format!(" {}", trimmed)
        }
    } else if trimmed.contains("statediagram-cluster") {
        // Cluster class present but missing the state prefix — synthesize.
        let last_is_cluster = trimmed
            .split_ascii_whitespace()
            .next_back()
            .map(|t| t == "statediagram-cluster")
            .unwrap_or(false);
        if last_is_cluster {
            format!(" statediagram-state {} ", trimmed)
        } else {
            format!(" statediagram-state {}", trimmed)
        }
    } else {
        " statediagram-state statediagram-cluster ".to_string()
    };

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{css}" id="{svg_id}-{dom_id}" data-id="{data_id}" data-look="{look}">"#,
        css = css_attr,
        svg_id = svg_id,
        dom_id = xml_escape(dom_id),
        data_id = xml_escape(raw_id),
        look = current_look(),
    ));
    out.push_str(&format!(
        r#"<g><rect class="outer" x="{rx}" y="{ry}" width="{w}" height="{h}" data-look="{look}"></rect></g>"#,
        rx = fmt_num(rx),
        ry = fmt_num(ry),
        w = fmt_num(w),
        h = fmt_num(h),
        look = current_look(),
    ));
    if !escaped.is_empty() {
        out.push_str(&format!(
            r#"<g class="cluster-label" transform="translate({lx}, {ly})"><foreignObject width="{lw}" height="{lh}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel "><p>{lbl}</p></span></div></foreignObject></g>"#,
            lx = fmt_num(label_tx),
            ly = fmt_num(label_ty),
            lw = fmt_num(lw),
            lh = fmt_num(lh),
            lbl = escaped,
        ));
    } else {
        out.push_str(r#"<g class="cluster-label"><foreignObject width="0" height="0"><div xmlns="http://www.w3.org/1999/xhtml"></div></foreignObject></g>"#);
    }
    out.push_str(&format!(
        r#"<rect class="inner" x="{rx}" y="{iy}" width="{w}" height="{ih}"></rect>"#,
        rx = fmt_num(rx),
        iy = fmt_num(inner_y),
        w = fmt_num(w),
        ih = fmt_num(inner_h),
    ));
    out.push_str("</g>");
    out
}

fn emit_edge_path(id: &str, e: &Edge, nodes: &[Node]) -> String {
    fn point_inside_cluster(bounds: &Bounds, p: Point) -> bool {
        let cx = bounds.x + bounds.width / 2.0;
        let cy = bounds.y + bounds.height / 2.0;
        let dx = (p.x - cx).abs();
        let dy = (p.y - cy).abs();
        dx < bounds.width / 2.0 && dy < bounds.height / 2.0
    }

    fn should_clip_to_cluster(points: &[Point], bounds: &Bounds) -> bool {
        points
            .last()
            .copied()
            .is_some_and(|p| point_inside_cluster(bounds, p))
            && points[..points.len().saturating_sub(1)]
                .iter()
                .copied()
                .any(|p| !point_inside_cluster(bounds, p))
    }

    fn should_clip_from_cluster(points: &[Point], bounds: &Bounds) -> bool {
        points
            .first()
            .copied()
            .is_some_and(|p| point_inside_cluster(bounds, p))
            && points
                .iter()
                .copied()
                .skip(1)
                .any(|p| !point_inside_cluster(bounds, p))
    }

    let Some(points) = &e.points else {
        return String::new();
    };
    if points.len() < 2 {
        return String::new();
    }
    let pts: Vec<Point> = points.iter().map(|p| Point { x: p.x, y: p.y }).collect();
    let mut render_pts = pts.clone();
    if let Some(cluster_id) = e.extra.get("to_cluster") {
        if let Some(cluster) = nodes.iter().find(|n| n.id == *cluster_id) {
            if let (Some(cx), Some(cy), Some(w), Some(h)) =
                (cluster.x, cluster.y, cluster.width, cluster.height)
            {
                let bounds = Bounds {
                    x: cx - w / 2.0,
                    y: cy - h / 2.0,
                    width: w,
                    height: h,
                };
                if should_clip_to_cluster(&render_pts, &bounds) {
                    render_pts = routing::clip_to_cluster_border(&render_pts, &bounds);
                }
            }
        }
    }
    if let Some(cluster_id) = e.extra.get("from_cluster") {
        if let Some(cluster) = nodes.iter().find(|n| n.id == *cluster_id) {
            if let (Some(cx), Some(cy), Some(w), Some(h)) =
                (cluster.x, cluster.y, cluster.width, cluster.height)
            {
                let bounds = Bounds {
                    x: cx - w / 2.0,
                    y: cy - h / 2.0,
                    width: w,
                    height: h,
                };
                if should_clip_from_cluster(&render_pts, &bounds) {
                    render_pts.reverse();
                    render_pts = routing::clip_to_cluster_border(&render_pts, &bounds);
                    render_pts.reverse();
                }
            }
        }
    }
    let d = edges::build_path(&render_pts, CurveType::Basis);
    let class = format!(
        " edge-thickness-{} edge-pattern-{} {}",
        e.thickness.as_deref().unwrap_or("normal"),
        e.pattern.as_deref().unwrap_or("solid"),
        e.classes.as_deref().unwrap_or("transition"),
    );
    // Base64-encoded JSON points array, matching upstream's
    // `btoa(JSON.stringify(points))`.
    // Attribute order: data-edge → data-et → data-id → data-points → data-look
    let data_points_b64 = {
        let mut json = String::from("[");
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                r#"{{"x":{x},"y":{y}}}"#,
                x = shapes::types::fmt_num(p.x),
                y = shapes::types::fmt_num(p.y),
            ));
        }
        json.push(']');
        unified_shell::base64_encode(json.as_bytes())
    };
    // Note edges have no arrowhead (arrowhead=None, class contains "note-edge").
    let is_note_edge = e
        .classes
        .as_deref()
        .map_or(false, |c| c.contains("note-edge"));
    let suppress_marker_end = matches!(e.arrow_type_end.as_deref(), Some("none"));
    if is_note_edge || suppress_marker_end {
        format!(
            r##"<path d="{d}" id="{id}-{eid}" class="{cls}" style="fill:none;;;fill:none" data-edge="true" data-et="edge" data-id="{eid}" data-points="{b64}" data-look="{look}"></path>"##,
            d = d,
            id = id,
            eid = e.id,
            cls = class,
            b64 = data_points_b64,
            look = current_look(),
        )
    } else {
        format!(
            r##"<path d="{d}" id="{id}-{eid}" class="{cls}" style="fill:none;;;fill:none" data-edge="true" data-et="edge" data-id="{eid}" data-points="{b64}" data-look="{look}" marker-end="url(#{id}_stateDiagram-barbEnd)"></path>"##,
            d = d,
            id = id,
            eid = e.id,
            cls = class,
            b64 = data_points_b64,
            look = current_look(),
        )
    }
}

fn round_to_5(v: f64) -> f64 {
    (v * 100_000.0).round() / 100_000.0
}

/// Upstream `utils.isLabelCoordinateInPath` is intentionally crude:
/// round the candidate point and every decimal in the `d=` string to integers,
/// then test whether either rounded coordinate appears anywhere in the path.
fn label_coordinate_in_path(point: Point, d_attr: &str) -> bool {
    static DECIMAL_RE: OnceLock<Regex> = OnceLock::new();
    let re = DECIMAL_RE.get_or_init(|| Regex::new(r"(\d+\.\d+)").expect("valid decimal regex"));
    let sanitized = re.replace_all(d_attr, |caps: &regex::Captures<'_>| {
        caps[1]
            .parse::<f64>()
            .map(|v| v.round().to_string())
            .unwrap_or_else(|_| caps[1].to_string())
    });
    let rounded_x = point.x.round().to_string();
    let rounded_y = point.y.round().to_string();
    sanitized.contains(&rounded_x) || sanitized.contains(&rounded_y)
}

/// Mirror upstream `positionEdgeLabel` trigger for state edges.
///
/// Mermaid keeps dagre's `edge.x / edge.y` unless the midpoint control point
/// is not actually visited by the rendered basis path. In that case it falls
/// back to `calcLabelPosition` on the polyline and rounds the result to 5
/// decimals.
fn recompute_edge_label_position(e: &Edge, _nodes: &[Node]) -> Option<(f64, f64)> {
    let pts_ref = e.points.as_ref()?;
    if pts_ref.len() < 2 {
        return None;
    }
    let d = edges::build_path(pts_ref, CurveType::Basis);
    let mid = pts_ref[pts_ref.len() / 2];
    if label_coordinate_in_path(mid, &d) {
        return None;
    }
    let mid = routing::place_label_midpoint(pts_ref)?;
    Some((round_to_5(mid.x), round_to_5(mid.y)))
}

fn emit_edge_label(e: &Edge, nodes: &[Node]) -> String {
    use crate::font_metrics::text_width;
    use crate::render::foreign_object::{self, LabelOpts};

    let raw = e.label.as_deref().unwrap_or("");
    // Decode mermaid #name; entities before rendering (upstream decodeEntities).
    let decoded = decode_mermaid_entities(raw);
    // Build the HTML body for the <p> content.
    // The raw label uses actual \n for <br/> splits (from the parser); literal
    // backslash+n text is preserved as-is.  We reconstruct the HTML by:
    //   1. Splitting on actual \n → these come from <br/> parsing → emit <br/>
    //   2. XML-escaping each segment (leaves literal \n chars as their chars)
    //   3. Joining with <br/> tag
    let (body, wrap_in_p) = if decoded.trim().is_empty() {
        if decoded.is_empty() {
            (String::new(), false)
        } else {
            // Whitespace-only: preserve literal whitespace in <p>.
            (format!("<p>{}</p>", xml_escape(&decoded)), false)
        }
    } else {
        let parts: Vec<String> = decoded.split('\n').map(|seg| xml_escape(seg)).collect();
        (parts.join("<br/>"), true)
    };

    // Measure label text for foreignObject dimensions.
    // Upstream measures via getBoundingClientRect → textContent of the HTML
    // element. textContent concatenates text nodes without the <br/> tag content,
    // yielding the same result as text_width(joined_label_without_newlines).
    // Since \n has zero advance in our font metrics, text_width(decoded) gives
    // the correct sum: each line's chars plus zero for each \n separator.
    let (lw, lh) = if decoded.is_empty() {
        (0.0, 16.296875) // default line-height at 14px sans-serif
    } else {
        let tw = text_width(decoded.trim(), "sans-serif", 14.0, false, false);
        (tw, 16.296875)
    };

    let (x, y) = recompute_edge_label_position(e, nodes)
        .unwrap_or((e.label_x.unwrap_or(0.0), e.label_y.unwrap_or(0.0)));
    let eid = &e.id;

    // Empty labels: outer <g class="edgeLabel"> with NO transform;
    // inner <g class="label" data-id="…" transform="translate(0, -lh/2)">.
    // Non-empty labels: outer <g class="edgeLabel" transform="translate(x,y)">;
    // inner <g class="label" data-id="…" transform="translate(-lw/2, -lh/2)">.
    let (outer_transform, inner_translate) = if body.is_empty() {
        (
            String::new(),
            format!("translate(0, {})", fmt_num(-lh / 2.0)),
        )
    } else {
        (
            format!(r#" transform="translate({}, {})""#, fmt_num(x), fmt_num(y)),
            format!("translate({}, {})", fmt_num(-lw / 2.0), fmt_num(-lh / 2.0)),
        )
    };

    let opts = LabelOpts {
        data_id: Some(eid),
        group_style: None,
        group_transform: Some(inner_translate),
        add_background: true,
        is_node: false,
        wrap_in_p,
        ..LabelOpts::default()
    };

    let inner = foreign_object::render_node_label(&body, lw, lh, &opts);
    format!(
        r#"<g class="edgeLabel"{outer_transform}>{inner}</g>"#,
        outer_transform = outer_transform,
        inner = inner,
    )
}

fn emit_node(id: &str, n: &Node, theme: &ThemeVariables) -> Option<String> {
    let shape = n.shape.as_deref().unwrap_or("state");
    match shape {
        "stateStart" | "state_start" | "start" => emit_state_start(id, n, theme),
        "stateEnd" | "state_end" | "end" => emit_state_end(id, n, theme),
        "forkJoin" | "fork_join" | "fork" | "join" => emit_fork_join(id, n, theme),
        "state" => emit_state_node(id, n, theme),
        "rectWithTitle" => emit_rect_with_title(id, n, theme),
        "note" => emit_note_node(id, n, theme),
        // Fallback: delegate to the generic shape drawer, then patch the
        // emitted `id="{dom_id}"` so it includes the svg_id prefix —
        // matching the convention used by every dedicated state-node emitter
        // above. Without this, shapes like `choice` come out with bare
        // `id="state-MyChoice-4"` instead of `id="{svg_id}-state-MyChoice-4"`.
        //
        // Exception: synthetic self-loop helper nodes (`labelRect` with
        // `extra["synthetic"] == "cyclic_helper"`) keep their bare
        // `{owner}---{owner}---{1|2}` id — upstream emits these helpers
        // without the diagram-id prefix in the rendered SVG.
        _ => {
            let raw = shapes::draw(shape, n, theme).ok()?;
            let is_cyclic_helper = n
                .extra
                .get("synthetic")
                .map(|s| s.as_str() == "cyclic_helper")
                .unwrap_or(false);
            if is_cyclic_helper {
                Some(raw)
            } else {
                let dom_id = xml_escape(n.dom_id.as_deref().unwrap_or(&n.id));
                let old_id = format!(r#"id="{}""#, dom_id);
                let new_id = format!(r#"id="{}-{}""#, id, dom_id);
                Some(raw.replacen(&old_id, &new_id, 1))
            }
        }
    }
}

/// Render a note node, applying the `{svg_id}-{dom_id}` id convention
/// used by the state renderer (same as other state node types).
fn emit_note_node(svg_id: &str, n: &Node, theme: &ThemeVariables) -> Option<String> {
    // Delegate to the note shape drawer, then patch the id attribute to
    // include the svg_id prefix (matching upstream's note shape id format).
    let raw = shapes::draw("note", n, theme).ok()?;
    // The note shape emits `id="{dom_id}"`. Replace with `id="{svg_id}-{dom_id}"`.
    let dom_id = xml_escape(n.dom_id.as_deref().unwrap_or(&n.id));
    let old_id = format!(r#"id="{}""#, dom_id);
    let new_id = format!(r#"id="{}-{}""#, svg_id, dom_id);
    Some(raw.replacen(&old_id, &new_id, 1))
}

fn emit_state_start(id: &str, n: &Node, _theme: &ThemeVariables) -> Option<String> {
    let w = n.width.unwrap_or(14.0).max(14.0);
    let r = w / 2.0;
    let nid = n.dom_id.clone().unwrap_or_else(|| n.id.clone());
    let tx = n.x.unwrap_or(0.0);
    let ty = n.y.unwrap_or(0.0);
    Some(format!(
        r#"<g class="node default" id="{id}-{nid}" data-look="{look}" transform="translate({tx}, {ty})"><circle class="state-start" r="{r}" width="{w}" height="{w}"></circle></g>"#,
        id = id,
        nid = xml_escape(&nid),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
        look = current_look(),
        r = fmt_num(r),
        w = fmt_num(w),
    ))
}

/// Rough.js-generated cubic-bezier circle path for outer ring (r=7).
/// Deterministic for the default rough.js seed on a 14×14 state-end marker.
const STATE_END_OUTER_PATH: &str = "M7 0 C7 0.40517908122283747, 6.964012880168563 0.816513743121899, 6.893654271085456 1.2155372436685123 C6.823295662002349 1.6145607442151257, 6.716427752933756 2.013397210557766, 6.5778483455013586 2.394141003279681 C6.439268938068961 2.7748847960015954, 6.26476736710249 3.149104622578984, 6.062177826491071 3.4999999999999996 C5.859588285879653 3.8508953774210153, 5.622755194947063 4.189128084166967, 5.362311101832846 4.499513267805774 C5.10186700871863 4.809898451444582, 4.809898451444583 5.10186700871863, 4.499513267805775 5.362311101832846 C4.189128084166968 5.622755194947063, 3.8508953774210166 5.859588285879652, 3.500000000000001 6.06217782649107 C3.149104622578985 6.264767367102489, 2.7748847960015963 6.439268938068961, 2.3941410032796817 6.5778483455013586 C2.013397210557767 6.716427752933756, 1.6145607442151264 6.823295662002349, 1.2155372436685128 6.893654271085456 C0.8165137431218992 6.964012880168563, 0.4051790812228379 7, 4.286263797015736e-16 7 C-0.405179081222837 7, -0.8165137431218985 6.964012880168563, -1.2155372436685121 6.893654271085456 C-1.6145607442151257 6.823295662002349, -2.0133972105577667 6.716427752933756, -2.394141003279681 6.5778483455013586 C-2.774884796001595 6.439268938068961, -3.149104622578983 6.26476736710249, -3.4999999999999982 6.062177826491071 C-3.8508953774210135 5.859588285879653, -4.189128084166966 5.6227551949470636, -4.499513267805773 5.362311101832848 C-4.809898451444581 5.101867008718632, -5.101867008718627 4.809898451444586, -5.362311101832843 4.499513267805779 C-5.622755194947058 4.189128084166971, -5.859588285879649 3.8508953774210206, -6.062177826491068 3.5000000000000053 C-6.264767367102486 3.14910462257899, -6.439268938068958 2.774884796001602, -6.577848345501356 2.394141003279688 C-6.716427752933754 2.0133972105577738, -6.823295662002347 1.614560744215134, -6.893654271085454 1.215537243668521 C-6.9640128801685615 0.816513743121908, -6.999999999999999 0.4051790812228472, -7 1.0183126166254463e-14 C-7.000000000000001 -0.40517908122282686, -6.964012880168565 -0.8165137431218878, -6.893654271085459 -1.215537243668501 C-6.823295662002352 -1.6145607442151142, -6.716427752933759 -2.0133972105577542, -6.577848345501363 -2.394141003279669 C-6.439268938068967 -2.7748847960015834, -6.264767367102496 -3.149104622578972, -6.062177826491078 -3.4999999999999876 C-5.859588285879661 -3.8508953774210033, -5.6227551949470715 -4.1891280841669545, -5.362311101832856 -4.499513267805763 C-5.10186700871864 -4.809898451444571, -4.809898451444594 -5.101867008718621, -4.499513267805787 -5.362311101832837 C-4.189128084166979 -5.622755194947054, -3.850895377421028 -5.859588285879643, -3.5000000000000133 -6.062177826491062 C-3.1491046225789985 -6.264767367102482, -2.774884796001611 -6.439268938068954, -2.3941410032796973 -6.577848345501353 C-2.0133972105577835 -6.716427752933752, -1.6145607442151435 -6.823295662002345, -1.2155372436685306 -6.893654271085453 C-0.8165137431219176 -6.9640128801685615, -0.40517908122285695 -6.999999999999999, -1.9937625952807352e-14 -7 C0.4051790812228171 -7.000000000000001, 0.8165137431218781 -6.964012880168565, 1.2155372436684913 -6.89365427108546 C1.6145607442151044 -6.823295662002354, 2.013397210557745 -6.716427752933763, 2.3941410032796595 -6.5778483455013665 C2.774884796001574 -6.43926893806897, 3.149104622578963 -6.2647673671025, 3.499999999999979 -6.062177826491083 C3.8508953774209953 -5.859588285879665, 4.189128084166947 -5.622755194947077, 4.499513267805756 -5.362311101832862 C4.809898451444564 -5.1018670087186475, 5.101867008718613 -4.809898451444602, 5.362311101832829 -4.499513267805796 C5.622755194947046 -4.189128084166989, 5.859588285879637 -3.8508953774210393, 6.062177826491056 -3.500000000000025 C6.2647673671024755 -3.1491046225790105, 6.439268938068949 -2.774884796001623, 6.577848345501348 -2.3941410032797092 C6.716427752933747 -2.0133972105577955, 6.823295662002342 -1.6145607442151562, 6.893654271085451 -1.2155372436685434 C6.96401288016856 -0.8165137431219307, 6.982275711847575 -0.2025895406114567, 7 -3.2800750208310675e-14 C7.017724288152425 0.2025895406113911, 7.017724288152424 -0.2025895406114242, 7 0";

/// Rough.js-generated cubic-bezier circle path for inner dot (r=2.5).
const STATE_END_INNER_PATH: &str = "M2.5 0 C2.5 0.14470681472244193, 2.487147457203058 0.29161205111496386, 2.46201938253052 0.4341204441673258 C2.436891307857982 0.5766288372196877, 2.3987241974763416 0.7190704323420595, 2.3492315519647713 0.8550503583141718 C2.299738906453201 0.991030284286284, 2.2374169168223177 1.124680222349637, 2.165063509461097 1.2499999999999998 C2.092710102099876 1.3753197776503625, 2.0081268553382365 1.496117172916774, 1.915111107797445 1.6069690242163481 C1.8220953602566536 1.7178208755159223, 1.7178208755159226 1.8220953602566536, 1.6069690242163484 1.915111107797445 C1.4961171729167742 2.0081268553382365, 1.375319777650363 2.0927101020998755, 1.2500000000000002 2.1650635094610964 C1.1246802223496375 2.2374169168223172, 0.9910302842862845 2.2997389064532, 0.8550503583141721 2.349231551964771 C0.7190704323420597 2.3987241974763416, 0.576628837219688 2.436891307857982, 0.43412044416732604 2.46201938253052 C0.291612051114964 2.487147457203058, 0.14470681472244212 2.5, 1.5308084989341916e-16 2.5 C-0.1447068147224418 2.5, -0.2916120511149638 2.487147457203058, -0.43412044416732576 2.46201938253052 C-0.5766288372196877 2.436891307857982, -0.7190704323420595 2.3987241974763416, -0.8550503583141718 2.3492315519647713 C-0.991030284286284 2.299738906453201, -1.124680222349637 2.2374169168223177, -1.2499999999999996 2.165063509461097 C-1.375319777650362 2.092710102099876, -1.4961171729167735 2.008126855338237, -1.6069690242163475 1.9151111077974459 C-1.7178208755159214 1.8220953602566548, -1.8220953602566525 1.7178208755159234, -1.9151111077974439 1.6069690242163495 C-2.008126855338235 1.4961171729167755, -2.0927101020998746 1.3753197776503645, -2.1650635094610955 1.250000000000002 C-2.2374169168223164 1.1246802223496395, -2.2997389064531992 0.9910302842862865, -2.34923155196477 0.8550503583141743 C-2.3987241974763407 0.7190704323420621, -2.436891307857981 0.5766288372196907, -2.4620193825305194 0.434120444167329 C-2.487147457203058 0.29161205111496724, -2.5 0.14470681472244545, -2.5 3.636830773662308e-15 C-2.5 -0.14470681472243818, -2.4871474572030587 -0.2916120511149599, -2.4620193825305208 -0.4341204441673218 C-2.436891307857983 -0.5766288372196837, -2.398724197476343 -0.7190704323420553, -2.3492315519647726 -0.8550503583141675 C-2.2997389064532023 -0.9910302842862798, -2.23741691682232 -1.1246802223496328, -2.165063509461099 -1.2499999999999956 C-2.092710102099878 -1.3753197776503583, -2.00812685533824 -1.4961171729167695, -1.9151111077974488 -1.606969024216344 C-1.8220953602566576 -1.7178208755159183, -1.7178208755159263 -1.8220953602566505, -1.6069690242163523 -1.915111107797442 C-1.4961171729167784 -2.008126855338234, -1.3753197776503672 -2.092710102099873, -1.2500000000000047 -2.1650635094610937 C-1.1246802223496422 -2.2374169168223146, -0.9910302842862897 -2.299738906453198, -0.8550503583141776 -2.3492315519647686 C-0.7190704323420656 -2.3987241974763394, -0.5766288372196942 -2.4368913078579806, -0.43412044416733236 -2.462019382530519 C-0.29161205111497057 -2.4871474572030574, -0.1447068147224489 -2.4999999999999996, -7.120580697431198e-15 -2.5 C0.14470681472243463 -2.5000000000000004, 0.29161205111495647 -2.487147457203059, 0.4341204441673183 -2.4620193825305217 C0.5766288372196802 -2.436891307857984, 0.7190704323420518 -2.3987241974763442, 0.8550503583141642 -2.349231551964774 C0.9910302842862766 -2.2997389064532037, 1.1246802223496295 -2.2374169168223212, 1.2499999999999925 -2.165063509461101 C1.3753197776503554 -2.0927101020998804, 1.4961171729167668 -2.008126855338242, 1.6069690242163412 -1.915111107797451 C1.7178208755159157 -1.82209536025666, 1.8220953602566472 -1.7178208755159294, 1.915111107797439 -1.6069690242163557 C2.0081268553382308 -1.496117172916782, 2.09271010209987 -1.3753197776503712, 2.1650635094610915 -1.2500000000000089 C2.237416916822313 -1.1246802223496466, 2.299738906453196 -0.9910302842862939, 2.3492315519647673 -0.855050358314182 C2.3987241974763385 -0.71907043234207, 2.4368913078579792 -0.5766288372196986, 2.462019382530518 -0.4341204441673369 C2.487147457203057 -0.29161205111497523, 2.4936698970884197 -0.07235340736123454, 2.5 -1.1714553645825241e-14 C2.5063301029115803 0.07235340736121111, 2.50633010291158 -0.07235340736122292, 2.5 0";

fn emit_state_end(id: &str, n: &Node, theme: &ThemeVariables) -> Option<String> {
    let nid = n.dom_id.clone().unwrap_or_else(|| n.id.clone());
    let tx = n.x.unwrap_or(0.0);
    let ty = n.y.unwrap_or(0.0);
    // Rough.js-generated circle paths for the default 14×14 state-end
    // marker (outer r=7, inner r=2.5). These are deterministic for the
    // same rough.js seed and match upstream exactly.
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");
    let main_bkg = theme.main_bkg.as_deref().unwrap_or("#ECECFF");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    Some(format!(
        concat!(
            r#"<g class="node default" id="{id}-{nid}" data-look="{look}" transform="translate({tx}, {ty})">"#,
            r#"<g class="outer-path">"#,
            r#"<path d="{outer_fill}" stroke="none" stroke-width="0" fill="{mb}" style=""></path>"#,
            r#"<path d="{outer_stroke}" stroke="{lc}" stroke-width="2" fill="none" stroke-dasharray="0 0" style=""></path>"#,
            r#"<g>"#,
            r#"<path d="{inner_fill}" stroke="none" stroke-width="0" fill="{nb}" style=""></path>"#,
            r#"<path d="{inner_stroke}" stroke="{nb}" stroke-width="2" fill="none" stroke-dasharray="0 0" style=""></path>"#,
            r#"</g>"#,
            r#"</g>"#,
            r#"</g>"#,
        ),
        id = id,
        nid = xml_escape(&nid),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
        look = current_look(),
        outer_fill = STATE_END_OUTER_PATH,
        outer_stroke = STATE_END_OUTER_PATH,
        inner_fill = STATE_END_INNER_PATH,
        inner_stroke = STATE_END_INNER_PATH,
        lc = line_color,
        mb = main_bkg,
        nb = node_border,
    ))
}

/// Rough.js-generated outline path for a 70x10 horizontal fork/join bar.
/// Path coordinates are local (centred at 0,0). The seed is the upstream
/// default; the bar's two-edge sketchy stroke is identical across all
/// fork/join nodes that share dimensions.
const FORK_JOIN_HORIZONTAL_FILL: &str = "M-35 -5 L35 -5 L35 5 L-35 5";
const FORK_JOIN_HORIZONTAL_STROKE: &str = "M-35 -5 C-20.999685308896005 -5, -6.99937061779201 -5, 35 -5 M-35 -5 C-19.5053675705567 -5, -4.010735141113397 -5, 35 -5 M35 -5 C35 -2.8249769369140267, 35 -0.6499538738280535, 35 5 M35 -5 C35 -2.316042452119291, 35 0.36791509576141834, 35 5 M35 5 C19.902926645241678 5, 4.8058532904833555 5, -35 5 M35 5 C8.686119192279875 5, -17.62776161544025 5, -35 5 M-35 5 C-35 2.1319068232551213, -35 -0.7361863534897575, -35 -5 M-35 5 C-35 1.362337320111692, -35 -2.275325359776616, -35 -5";

fn emit_fork_join(id: &str, n: &Node, theme: &ThemeVariables) -> Option<String> {
    let dir = n.dir.as_deref();
    let horizontal = !matches!(dir, Some("LR") | Some("RL"));
    let classes =
        shapes::types::get_node_classes(n.look.as_deref(), n.css_classes.as_deref(), None);
    let nid = n.dom_id.clone().unwrap_or_else(|| n.id.clone());
    let tx = n.x.unwrap_or(0.0);
    let ty = n.y.unwrap_or(0.0);
    let line = theme.line_color.as_deref().unwrap_or("black");
    if horizontal {
        // Horizontal 70x10 bar — emit pre-baked rough.js paths so we match
        // upstream's <g><path/><path/></g> shape instead of a flat <rect>.
        Some(format!(
            concat!(
                r#"<g class="{classes}" id="{id}-{nid}" data-look="{look}" transform="translate({tx}, {ty})">"#,
                r#"<g>"#,
                r#"<path d="{fill}" stroke="none" stroke-width="0" fill="{line}" style=""></path>"#,
                r#"<path d="{stroke_d}" stroke="{line}" stroke-width="1.3" fill="none" stroke-dasharray="0 0" style=""></path>"#,
                r#"</g>"#,
                r#"</g>"#,
            ),
            classes = classes,
            id = id,
            nid = xml_escape(&nid),
            tx = fmt_num(tx),
            ty = fmt_num(ty),
            fill = FORK_JOIN_HORIZONTAL_FILL,
            stroke_d = FORK_JOIN_HORIZONTAL_STROKE,
            line = line,
            look = current_look(),
        ))
    } else {
        // Vertical (LR/RL) bar — fall back to the legacy flat-rect emission
        // until a pre-baked LR rough.js path is captured. Current state
        // fixtures don't exercise this branch, so the placeholder keeps
        // existing behaviour stable.
        let (w, h) = (10.0, 70.0);
        let x = -w / 2.0;
        let y = -h / 2.0;
        Some(format!(
            r#"<g class="{classes}" id="{id}-{nid}" data-look="{look}" transform="translate({tx}, {ty})"><rect class="fork-join" x="{x}" y="{y}" width="{w}" height="{h}" style="fill:{line};stroke:{line}"></rect></g>"#,
            classes = classes,
            id = id,
            nid = xml_escape(&nid),
            tx = fmt_num(tx),
            ty = fmt_num(ty),
            x = fmt_num(x),
            y = fmt_num(y),
            w = fmt_num(w),
            h = fmt_num(h),
            line = line,
            look = current_look(),
        ))
    }
}

/// Emit a normal state node (rounded rect + label).
/// Matches upstream's `drawRect` with `rx=5, ry=5` + `labelHelper`.
fn emit_state_node(id: &str, n: &Node, _theme: &ThemeVariables) -> Option<String> {
    use crate::render::foreign_object::{measure_html_label, LabelOpts};

    let w = n.width.unwrap_or(0.0);
    let h = n.height.unwrap_or(0.0);
    let r = n.rx.unwrap_or(5.0);
    let classes =
        shapes::types::get_node_classes(n.look.as_deref(), n.css_classes.as_deref(), None);
    let nid = n.dom_id.clone().unwrap_or_else(|| n.id.clone());
    let tx = n.x.unwrap_or(0.0);
    let ty = n.y.unwrap_or(0.0);
    let label = n.label.clone().unwrap_or_default();
    let is_markdown = n.label_type.as_deref() == Some("markdown");

    // Merge cssCompiledStyles (from classDef) and cssStyles/labelStyle
    // (from inline `style X ...` directive). Upstream uses a Map so that
    // cssStyles overrides cssCompiledStyles for the same key.
    // We build a merged comma-separated string: compiled first, inline last
    // (later entries override earlier ones in styles2_string).
    let merged_style =
        merge_node_styles(n.css_compiled_styles.as_deref(), n.label_style.as_deref());
    let label_style = merged_style.as_str();

    // Split merged style into node/label parts with !important.
    let (node_styles, label_styles_str, div_prefix) = if label_style.is_empty() {
        (String::new(), String::new(), String::new())
    } else {
        styles2_string(label_style)
    };

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}-{nid}" data-look="{look}" transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = id,
        nid = xml_escape(&nid),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
        look = current_look(),
    ));
    out.push_str(&format!(
        r#"<rect class="basic label-container" style="{node_styles}" rx="{r}" ry="{r}" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
        node_styles = node_styles,
        r = fmt_num(r),
        x = fmt_num(-w / 2.0),
        y = fmt_num(-h / 2.0),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    if !label.is_empty() {
        let escaped = xml_escape(&label);
        // Detect bold from merged style (classDef or inline style).
        let label_is_bold = merged_style
            .split(',')
            .any(|p| p.trim().trim_end_matches(';').replace(' ', "") == "font-weight:bold");
        let (lw, lh) = measure_html_label(
            &escaped,
            &crate::render::foreign_object::HtmlLabelFont {
                bold: if label_is_bold { Some(true) } else { None },
                ..Default::default()
            },
            200.0,
            true,
        );
        let label_styles_ref: Option<&str> = if label_styles_str.is_empty() {
            None
        } else {
            Some(&label_styles_str)
        };
        let div_prefix_ref: Option<&str> = if div_prefix.is_empty() {
            None
        } else {
            Some(&div_prefix)
        };
        let group_style_val: &str = if label_styles_str.is_empty() {
            ""
        } else {
            &label_styles_str
        };
        let opts = LabelOpts {
            extra_span_classes: if is_markdown {
                "markdown-node-label"
            } else {
                ""
            },
            group_style: Some(group_style_val),
            label_style: label_styles_ref,
            div_style_prefix: div_prefix_ref,
            ..LabelOpts::default()
        };
        out.push_str(&crate::render::foreign_object::render_node_label(
            &escaped, lw, lh, &opts,
        ));
    }
    out.push_str("</g>");
    Some(out)
}

/// Render a state node that has description lines (upstream shape: rectWithTitle).
///
/// Structure mirrors the upstream `rectWithTitle` function in dagre-wrapper:
/// - An outer rect ("title-state") sized to the label group bbox + padding.
/// - A divider line at the bottom of the title row.
/// - A label group containing two foreignObjects: title and description.
fn emit_rect_with_title(id: &str, n: &Node, _theme: &ThemeVariables) -> Option<String> {
    use crate::font_metrics::text_width as ft_w;
    const FONT_FAMILY: &str = "sans-serif";
    const FONT_SIZE: f64 = 14.0;
    const HALF_PAD: f64 = 4.0; // halfPadding = node.padding / 2 = 8/2 = 4

    let _w = n.width.unwrap_or(0.0);
    let h = n.height.unwrap_or(0.0);
    let padding = n.label_padding_x.unwrap_or(8.0);
    let lh = h - padding; // = line_height (=16.296875)
                          // Upstream rectWithTitle uses `"node " + node2.classes` (no trailing space),
                          // not the `get_node_classes` helper (which appends a trailing space).
    let css_cls = n.css_classes.as_deref().unwrap_or("undefined");
    let classes = format!("node {}", css_cls);
    let nid = n.dom_id.clone().unwrap_or_else(|| n.id.clone());
    let tx = n.x.unwrap_or(0.0);
    let ty = n.y.unwrap_or(0.0);

    let merged_style =
        merge_node_styles(n.css_compiled_styles.as_deref(), n.label_style.as_deref());
    let label_style = merged_style.as_str();
    let (node_styles, label_styles_str, _div_prefix) = if label_style.is_empty() {
        (String::new(), String::new(), String::new())
    } else {
        styles2_string(label_style)
    };
    let label_is_bold = merged_style
        .split(',')
        .any(|p| p.trim().trim_end_matches(';').replace(' ', "") == "font-weight:bold");

    // Title = first label line. Description = n.description.
    let title = n.label.as_deref().unwrap_or("");
    let desc_lines: Vec<&str> = n
        .description
        .as_deref()
        .map(|d| d.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();

    // Measure text widths.
    let title_w = ft_w(title, FONT_FAMILY, FONT_SIZE, label_is_bold, false);
    let mut desc_max_w = 0.0f64;
    for dl in &desc_lines {
        let dw = ft_w(dl, FONT_FAMILY, FONT_SIZE, label_is_bold, false);
        if dw > desc_max_w {
            desc_max_w = dw;
        }
    }
    // Label group bbox.width = max(title_w, desc_max_w) (jsdom ignores transforms).
    let bbox_w = title_w.max(desc_max_w);

    // Outer rect geometry.
    // rect x/y = -bbox_w/2 - halfPad, -lh/2 - halfPad
    let rect_x = -bbox_w / 2.0 - HALF_PAD;
    let rect_y = -lh / 2.0 - HALF_PAD;
    let rect_w = bbox_w + padding;
    let rect_h = lh + padding;
    // Divider y = -lh/2 - halfPad + lh + halfPad = lh/2
    let div_y = lh / 2.0;
    // Label group translate: (-bbox_w/2, -lh/2 - halfPad + 3)
    let label_tx = -bbox_w / 2.0;
    let label_ty = -lh / 2.0 - HALF_PAD + 3.0;

    // Title foreignObject: offset depends on whether title is wider than desc.
    let title_fo_tx = if title_w >= desc_max_w {
        0.0_f64
    } else {
        -(title_w - desc_max_w) / 2.0
    };
    let title_fo_width = title_w;

    // Description foreignObject(s): one per description line.
    // The upstream joins all desc lines with <br/> and measures as one block.
    // For single-line descriptions the width is that line's text width.
    // Position: translate( desc_x_off, titleBox.height + halfPad + 5 )
    let desc_y = lh + HALF_PAD + 5.0; // titleBox.height + halfPadding + 5

    // Upstream always emits style="..." even when empty for rectWithTitle.
    let label_style_attr = format!(r#" style="{}""#, label_styles_str);
    let node_style_attr = format!(r#" style="{}""#, node_styles);

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}-{nid}" data-look="{look}" transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = id,
        nid = xml_escape(&nid),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
        look = current_look(),
    ));
    // Outer group with rect and divider.
    out.push_str("<g>");
    out.push_str(&format!(
        r#"<rect class="outer title-state"{node_style_attr} x="{rx}" y="{ry}" width="{rw}" height="{rh}"></rect>"#,
        node_style_attr = node_style_attr,
        rx = fmt_num(rect_x),
        ry = fmt_num(rect_y),
        rw = fmt_num(rect_w),
        rh = fmt_num(rect_h),
    ));
    out.push_str(&format!(
        r#"<line class="divider" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}"></line>"#,
        x1 = fmt_num(rect_x),
        x2 = fmt_num(-rect_x),
        y1 = fmt_num(div_y),
        y2 = fmt_num(div_y),
    ));
    out.push_str("</g>");
    // Label group.
    out.push_str(&format!(
        r#"<g class="label"{label_style_attr} transform="translate({ltx}, {lty})">"#,
        label_style_attr = label_style_attr,
        ltx = fmt_num(label_tx),
        lty = fmt_num(label_ty),
    ));
    // Title foreignObject.
    // Upstream rectWithTitle (dagre-wrapper) uses `createLabel` which emits
    // `<span class="nodeLabel ">` (trailing space, no markdown class).
    out.push_str(&format!(
        r#"<foreignObject width="{tw}" height="{lh}" transform="translate({fo_tx}, 0)"><div style="display: table-cell; white-space: nowrap; line-height: 1.5;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel "><p>{title}</p></span></div></foreignObject>"#,
        tw = fmt_num(title_fo_width),
        lh = fmt_num(lh),
        fo_tx = format!(" {}", fmt_num(title_fo_tx)),
        title = xml_escape(title),
    ));
    // Description foreignObject(s).
    // Upstream joins all desc lines with <br/> in one foreignObject for a
    // multi-line description, but we emit individual foreignObjects to match
    // the reference when there is exactly one description line. The upstream
    // `createLabel` for multiple lines produces one block; for the reference
    // SVGs we only see single-description cases (one extra line). If the
    // description has exactly one line we emit it directly.
    let desc_text = if desc_lines.len() == 1 {
        desc_lines[0].to_string()
    } else {
        desc_lines.join("<br/>")
    };
    let actual_desc_w = desc_max_w;
    // desc x offset: if desc is narrower than title, center it.
    let desc_fo_tx = if actual_desc_w <= title_w {
        (title_w - actual_desc_w) / 2.0
    } else {
        0.0
    };
    out.push_str(&format!(
        r#"<foreignObject width="{dw}" height="{lh}" transform="translate({fo_tx}, {fo_ty})"><div style="display: table-cell; white-space: nowrap; line-height: 1.5;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel "><p>{desc}</p></span></div></foreignObject>"#,
        dw = fmt_num(actual_desc_w),
        lh = fmt_num(lh),
        fo_tx = format!(" {}", fmt_num(desc_fo_tx)),
        fo_ty = fmt_num(desc_y),
        desc = xml_escape(&desc_text),
    ));
    out.push_str("</g>");
    out.push_str("</g>");
    Some(out)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Decode mermaid `#name;` entities to their HTML/text equivalents.
///
/// Upstream mermaid uses `encodeEntities` (parser phase) to convert `#colon;`
/// → `ﬂ°colon¶ß`, then `decodeEntities` (render phase) converts back to
/// `&colon;`. We skip the intermediate encoding and go directly from the
/// `#name;` form (as stored in our parsed model) to the character/HTML entity.
///
/// Numeric entities like `#123;` → `&#123;` (HTML numeric ref).
/// Named entities like `#colon;` → `:`, `#amp;` → `&`, etc.
fn decode_mermaid_entities(s: &str) -> String {
    // Fast path: if there's no '#' followed by a letter/digit and ';', return as-is.
    if !s.contains('#') {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    while i < n {
        if bytes[i] == b'#' {
            // Find the closing ';'
            let start = i + 1;
            let mut end = start;
            while end < n && bytes[end] != b';' {
                end += 1;
            }
            if end < n {
                let name = &s[start..end];
                // Check if numeric
                if name.chars().all(|c| c.is_ascii_digit()) {
                    // Numeric entity: #123; → &#123;  (decoded as HTML)
                    if let Ok(cp) = name.parse::<u32>() {
                        if let Some(ch) = char::from_u32(cp) {
                            result.push(ch);
                        } else {
                            result.push_str(&format!("&#{};", name));
                        }
                    } else {
                        result.push('#');
                        result.push_str(name);
                        result.push(';');
                    }
                } else {
                    // Named entity: map common ones used in mermaid labels.
                    let decoded = match name {
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
                        "grave" => "`",
                        "lowbar" | "underbar" => "_",
                        "space" => " ",
                        "nbsp" => "\u{00A0}",
                        "ndash" => "\u{2013}",
                        "mdash" => "\u{2014}",
                        "laquo" => "\u{00AB}",
                        "raquo" => "\u{00BB}",
                        _ => {
                            // Unknown: keep as-is
                            result.push('#');
                            result.push_str(name);
                            result.push(';');
                            i = end + 1;
                            continue;
                        }
                    };
                    result.push_str(decoded);
                }
                i = end + 1;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

/// Merge CSS styles from `cssCompiledStyles` (classDef) and inline `style`
/// directive into a single comma-separated string. Later entries override
/// earlier ones for the same property key (matching upstream `styles2Map`).
///
/// `compiled` is a slice of individual CSS property strings like
/// `["fill:#fff", "color: blue"]`. `inline` is a comma-separated string
/// like `"fill:#636,color:white"`. Returns a comma-joined merged string.
fn merge_node_styles(compiled: Option<&[String]>, inline: Option<&str>) -> String {
    // Use a Vec to preserve insertion order; later entries override earlier
    // ones for the same key (matching upstream styles2Map behaviour).
    let mut entries: Vec<(String, String)> = Vec::new();

    if let Some(props) = compiled {
        for prop in props {
            let p = prop.trim();
            if let Some(c) = p.find(':') {
                let key = p[..c].trim().to_string();
                let val = p[c + 1..].trim().to_string();
                entries.push((key, val));
            }
        }
    }
    if let Some(raw) = inline {
        for prop in raw.split(',') {
            let p = prop.trim();
            if let Some(c) = p.find(':') {
                let key = p[..c].trim().to_string();
                let val = p[c + 1..].trim().to_string();
                entries.push((key, val));
            }
        }
    }

    if entries.is_empty() {
        return String::new();
    }

    // Dedup: last entry with same key wins (like a Map built with forEach).
    // Build unique keys in order of first appearance, then overwrite value.
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut ordered_keys: Vec<String> = Vec::new();
    let mut final_vals: Vec<String> = vec![String::new(); entries.len()];

    for (i, (k, v)) in entries.iter().enumerate() {
        if let Some(&prev_idx) = seen.get(k) {
            // Override: update the value at the prev slot.
            final_vals[prev_idx] = v.clone();
        } else {
            seen.insert(k.clone(), i);
            ordered_keys.push(k.clone());
            final_vals[i] = v.clone();
        }
    }

    // Build in key order.
    let mut result = Vec::new();
    for key in &ordered_keys {
        if let Some(&idx) = seen.get(key) {
            result.push(format!("{}:{}", key, final_vals[idx]));
        }
    }
    result.join(",")
}

/// Mirrors upstream `styles2String` + `isLabelStyle` in
/// `handDrawnShapeStyles.ts`.
///
/// Splits a comma-separated CSS string (e.g. `"fill:#636,border:1px solid
/// red,color:white"`) into three derived strings:
/// * `node_styles`  — semicolon-joined non-label properties with `!important`,
///   e.g. `"fill:#636 !important;border:1px solid red !important"`.
/// * `label_styles` — semicolon-joined label/text properties with `!important`,
///   e.g. `"color:white !important"`.
/// * `div_prefix`   — label properties formatted for D3-normalised div style
///   prefix, e.g. `"color: white !important; "`.
fn styles2_string(raw_css: &str) -> (String, String, String) {
    let mut node_parts: Vec<String> = Vec::new();
    let mut label_parts: Vec<String> = Vec::new();
    let mut div_parts: Vec<String> = Vec::new();

    for prop in raw_css.split(',') {
        let prop = prop.trim();
        if prop.is_empty() {
            continue;
        }
        // Split only on the first `:` — values like `1px solid red` contain no colon.
        if let Some(colon_pos) = prop.find(':') {
            let key = prop[..colon_pos].trim();
            let value = prop[colon_pos + 1..].trim();
            let with_important = format!("{}:{} !important", key, value);
            if is_label_style_key(key) {
                label_parts.push(with_important);
                // D3's `.style()` normalises to "key: value !important; ".
                div_parts.push(format!("{}: {} !important; ", key, value));
            } else {
                node_parts.push(with_important);
            }
        }
    }

    (
        node_parts.join(";"),
        label_parts.join(";"),
        div_parts.join(""),
    )
}

/// Mirrors upstream `isLabelStyle` in `handDrawnShapeStyles.ts`.
fn is_label_style_key(key: &str) -> bool {
    matches!(
        key,
        "color"
            | "font-size"
            | "font-family"
            | "font-weight"
            | "font-style"
            | "text-decoration"
            | "text-align"
            | "text-transform"
            | "line-height"
            | "letter-spacing"
            | "word-spacing"
            | "text-shadow"
            | "text-overflow"
            | "white-space"
            | "word-wrap"
            | "word-break"
            | "overflow-wrap"
            | "hyphens"
    )
}

/// `<style>` block — built from the shared base preamble + the full
/// state-specific CSS (ported from upstream `state/styles.js`) + the
/// shared neo-look tail + any `classDef` rules from the diagram.
fn style_block(id: &str, theme: &ThemeVariables, d: &StateDiagram) -> String {
    let mut s = String::with_capacity(4096);
    s.push_str("<style>");
    s.push_str(&theme_css::base_preamble(id, theme));
    s.push_str(&state_specific_css(id, theme));
    s.push_str(&theme_css::neo_look_block(id, theme));
    // classDef CSS injected after :root rule, before </style>.
    // Mirrors upstream `createCssStyles` + Stylis minification.
    if !d.class_defs.is_empty() {
        s.push_str(&class_def_css(id, d));
    }
    s.push_str("</style>");
    s
}

/// Generate Stylis-minified CSS for `classDef` directives.
///
/// Mirrors upstream `createCssStyles` in `mermaidAPI.ts`:
/// * For each classDef, emit rules for `> *` and `span` (htmlLabels path)
///   with all styles as `!important`.
/// * Emit a `tspan` rule using only text/label-style properties
///   (with `color` → `fill` substitution).
fn class_def_css(id: &str, d: &StateDiagram) -> String {
    let mut out = String::new();
    for def in &d.class_defs {
        // Split comma-separated styles from classDef value.
        // e.g. "fill:#fff,color: blue" → ["fill:#fff", "color: blue"]
        let raw = def.styles.trim().trim_end_matches(';');
        let props: Vec<&str> = raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        let mut styles: Vec<String> = Vec::new(); // all properties
        let mut text_styles: Vec<String> = Vec::new(); // label/color properties only

        for prop in &props {
            // Strip trailing `;`
            let fixed = prop.trim_end_matches(';').trim();
            if let Some(colon_pos) = fixed.find(':') {
                let key = fixed[..colon_pos].trim();
                let val = fixed[colon_pos + 1..].trim();
                // Upstream: if COLOR_KEYWORD matches the attrib:
                //   newStyle1 = fixedAttrib.replace('fill', 'bgFill')
                //   newStyle2 = newStyle1.replace('color', 'fill')
                //   textStyles.push(newStyle2)
                if key.contains("color") {
                    // Replace 'fill' → 'bgFill' in key (noop for 'color'),
                    // then 'color' → 'fill'. Rebuild without extra spaces.
                    let new_key = key.replace("fill", "bgFill").replace("color", "fill");
                    // Value is preserved as-is.
                    text_styles.push(format!("{}:{}", new_key, val));
                }
                styles.push(format!("{}:{}", key, val));
            }
        }

        if !styles.is_empty() {
            // ">*" rule has no space; "span" rule has a space.
            // Stylis minifies "> *" → ">*" but "span" stays as " span".
            for element in &[">*", " span"] {
                out.push_str(&format!(
                    "#{id} .{cls}{el}{{",
                    id = id,
                    cls = def.name,
                    el = element
                ));
                for s in &styles {
                    out.push_str(s);
                    out.push_str("!important;");
                }
                out.push('}');
            }
        }
        if !text_styles.is_empty() {
            // tspan rule — text/color properties with !important.
            // Space before "tspan" in selector.
            out.push_str(&format!("#{id} .{cls} tspan{{", id = id, cls = def.name,));
            for s in &text_styles {
                out.push_str(s);
                out.push_str("!important;");
            }
            out.push('}');
        }
    }
    out
}

/// Full port of upstream `packages/mermaid/src/diagrams/state/styles.js`.
/// All ~50 CSS rules, with theme variable interpolation matching the
/// default theme's computed values. Stylis-minified (no whitespace,
/// no comments).
fn state_specific_css(id: &str, theme: &ThemeVariables) -> String {
    let transition_color = theme.transition_color.as_deref().unwrap_or("#333333");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let main_bkg = theme.main_bkg.as_deref().unwrap_or("#ECECFF");
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");
    let stroke_width = theme.stroke_width.unwrap_or(1);
    let state_label_color = theme.state_label_color.as_deref().unwrap_or("#131300");
    let special_state_color = theme.special_state_color.as_deref().unwrap_or("#333333");
    let inner_end_bg = theme.inner_end_background.as_deref().unwrap_or("#333333");
    let background = theme.background.as_deref().unwrap_or("white");
    let composite_bg = theme
        .composite_background
        .as_deref()
        .or(theme.background.as_deref())
        .unwrap_or("white");
    let composite_title_bg = theme
        .composite_title_background
        .as_deref()
        .unwrap_or("#ECECFF");
    let state_bkg = theme
        .state_bkg
        .as_deref()
        .or(theme.main_bkg.as_deref())
        .unwrap_or("#ECECFF");
    let state_border = theme
        .state_border
        .as_deref()
        .or(theme.node_border.as_deref())
        .unwrap_or("#9370DB");
    let alt_bg = theme.alt_background.as_deref().unwrap_or("#efefef");
    let note_bkg = theme.note_bkg_color.as_deref().unwrap_or("#fff5ad");
    let note_border = theme.note_border_color.as_deref().unwrap_or("#aaaa33");
    let note_text = theme.note_text_color.as_deref().unwrap_or("#333");
    let label_bg = theme.label_background_color.as_deref().unwrap_or("#ECECFF");
    let edge_label_bg = theme
        .edge_label_background
        .as_deref()
        .unwrap_or("rgba(232,232,232, 0.8)");
    let transition_label_color = theme
        .transition_label_color
        .as_deref()
        .or(theme.tertiary_text_color.as_deref())
        .unwrap_or("#333");
    let radius = theme.radius.unwrap_or(5);
    // drop-shadow for neo look
    let drop_shadow = theme
        .drop_shadow
        .as_deref()
        .unwrap_or("drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))");
    let neo_ds = drop_shadow.replace("url(#drop-shadow)", &format!("url({}-drop-shadow)", id));

    let mut s = String::with_capacity(3000);

    // defs [id$="-barbEnd"]
    s.push_str(&format!(
        "#{id} defs [id$=\"-barbEnd\"]{{fill:{tc};stroke:{tc};}}",
        tc = transition_color,
    ));
    // g.stateGroup text (first occurrence)
    s.push_str(&format!(
        "#{id} g.stateGroup text{{fill:{nb};stroke:none;font-size:10px;}}",
        nb = node_border,
    ));
    // g.stateGroup text (second occurrence — upstream emits it twice)
    s.push_str(&format!(
        "#{id} g.stateGroup text{{fill:{tc2};stroke:none;font-size:10px;}}",
        tc2 = text_color,
    ));
    // g.stateGroup .state-title
    s.push_str(&format!(
        "#{id} g.stateGroup .state-title{{font-weight:bolder;fill:{slc};}}",
        slc = state_label_color,
    ));
    // g.stateGroup rect
    s.push_str(&format!(
        "#{id} g.stateGroup rect{{fill:{mb};stroke:{nb};}}",
        mb = main_bkg,
        nb = node_border,
    ));
    // g.stateGroup line
    s.push_str(&format!(
        "#{id} g.stateGroup line{{stroke:{lc};stroke-width:{sw};}}",
        lc = line_color,
        sw = stroke_width,
    ));
    // .transition
    s.push_str(&format!(
        "#{id} .transition{{stroke:{tc};stroke-width:{sw};fill:none;}}",
        tc = transition_color,
        sw = stroke_width,
    ));
    // .stateGroup .composit (upstream typo preserved)
    s.push_str(&format!(
        "#{id} .stateGroup .composit{{fill:{bg};border-bottom:1px;}}",
        bg = background,
    ));
    // .stateGroup .alt-composit
    s.push_str(&format!(
        "#{id} .stateGroup .alt-composit{{fill:#e0e0e0;border-bottom:1px;}}",
    ));
    // .state-note
    s.push_str(&format!(
        "#{id} .state-note{{stroke:{nbc};fill:{nbg};}}",
        nbc = note_border,
        nbg = note_bkg,
    ));
    // .state-note text
    s.push_str(&format!(
        "#{id} .state-note text{{fill:{ntc};stroke:none;font-size:10px;}}",
        ntc = note_text,
    ));
    // .stateLabel .box
    s.push_str(&format!(
        "#{id} .stateLabel .box{{stroke:none;stroke-width:0;fill:{mb};opacity:0.5;}}",
        mb = main_bkg,
    ));
    // .edgeLabel .label rect
    s.push_str(&format!(
        "#{id} .edgeLabel .label rect{{fill:{lbg};opacity:0.5;}}",
        lbg = label_bg,
    ));
    // .edgeLabel — upstream merges background-color and text-align into one rule
    // (stylis merges duplicate selectors).
    s.push_str(&format!(
        "#{id} .edgeLabel{{background-color:{elbg};text-align:center;}}",
        elbg = edge_label_bg,
    ));
    // .edgeLabel p
    s.push_str(&format!(
        "#{id} .edgeLabel p{{background-color:{elbg};}}",
        elbg = edge_label_bg,
    ));
    // .edgeLabel rect
    s.push_str(&format!(
        "#{id} .edgeLabel rect{{opacity:0.5;background-color:{elbg};fill:{elbg};}}",
        elbg = edge_label_bg,
    ));
    // .edgeLabel .label text
    s.push_str(&format!(
        "#{id} .edgeLabel .label text{{fill:{tlc};}}",
        tlc = transition_label_color,
    ));
    // .label div .edgeLabel
    s.push_str(&format!(
        "#{id} .label div .edgeLabel{{color:{tlc};}}",
        tlc = transition_label_color,
    ));
    // .stateLabel text
    s.push_str(&format!(
        "#{id} .stateLabel text{{fill:{slc};font-size:10px;font-weight:bold;}}",
        slc = state_label_color,
    ));
    // .node circle.state-start
    s.push_str(&format!(
        "#{id} .node circle.state-start{{fill:{ssc};stroke:{ssc};}}",
        ssc = special_state_color,
    ));
    // .node .fork-join
    s.push_str(&format!(
        "#{id} .node .fork-join{{fill:{ssc};stroke:{ssc};}}",
        ssc = special_state_color,
    ));
    // .node circle.state-end
    s.push_str(&format!(
        "#{id} .node circle.state-end{{fill:{ieb};stroke:{bg};stroke-width:1.5;}}",
        ieb = inner_end_bg,
        bg = background,
    ));
    // .end-state-inner
    s.push_str(&format!(
        "#{id} .end-state-inner{{fill:{cbg};stroke-width:1.5;}}",
        cbg = composite_bg,
    ));
    // .node rect
    s.push_str(&format!(
        "#{id} .node rect{{fill:{sb};stroke:{sbr};stroke-width:{sw}px;}}",
        sb = state_bkg,
        sbr = state_border,
        sw = stroke_width,
    ));
    // .node polygon
    s.push_str(&format!(
        "#{id} .node polygon{{fill:{mb};stroke:{sbr};stroke-width:{sw}px;}}",
        mb = main_bkg,
        sbr = state_border,
        sw = stroke_width,
    ));
    // [id$="-barbEnd"]
    s.push_str(&format!(
        "#{id} [id$=\"-barbEnd\"]{{fill:{lc};}}",
        lc = line_color,
    ));
    // .statediagram-cluster rect
    s.push_str(&format!(
        "#{id} .statediagram-cluster rect{{fill:{ctbg};stroke:{sbr};stroke-width:{sw}px;}}",
        ctbg = composite_title_bg,
        sbr = state_border,
        sw = stroke_width,
    ));
    // .cluster-label, .nodeLabel
    s.push_str(&format!(
        "#{id} .cluster-label,#{id} .nodeLabel{{color:{slc};}}",
        slc = state_label_color,
    ));
    // .statediagram-cluster rect.outer
    s.push_str(&format!(
        "#{id} .statediagram-cluster rect.outer{{rx:5px;ry:5px;}}",
    ));
    // .statediagram-state .divider
    s.push_str(&format!(
        "#{id} .statediagram-state .divider{{stroke:{sbr};}}",
        sbr = state_border,
    ));
    // .statediagram-state .title-state
    s.push_str(&format!(
        "#{id} .statediagram-state .title-state{{rx:5px;ry:5px;}}",
    ));
    // .statediagram-cluster.statediagram-cluster .inner
    s.push_str(&format!(
        "#{id} .statediagram-cluster.statediagram-cluster .inner{{fill:{cbg};}}",
        cbg = composite_bg,
    ));
    // .statediagram-cluster.statediagram-cluster-alt .inner
    s.push_str(&format!(
        "#{id} .statediagram-cluster.statediagram-cluster-alt .inner{{fill:{abg};}}",
        abg = alt_bg,
    ));
    // .statediagram-cluster .inner
    s.push_str(&format!("#{id} .statediagram-cluster .inner{{rx:0;ry:0;}}",));
    // .statediagram-state rect.basic
    s.push_str(&format!(
        "#{id} .statediagram-state rect.basic{{rx:5px;ry:5px;}}",
    ));
    // .statediagram-state rect.divider
    s.push_str(&format!(
        "#{id} .statediagram-state rect.divider{{stroke-dasharray:10,10;fill:{abg};}}",
        abg = alt_bg,
    ));
    // .note-edge
    s.push_str(&format!("#{id} .note-edge{{stroke-dasharray:5;}}",));
    // .statediagram-note rect (twice — upstream emits it twice)
    s.push_str(&format!(
        "#{id} .statediagram-note rect{{fill:{nbg};stroke:{nbc};stroke-width:1px;rx:0;ry:0;}}",
        nbg = note_bkg,
        nbc = note_border,
    ));
    s.push_str(&format!(
        "#{id} .statediagram-note rect{{fill:{nbg};stroke:{nbc};stroke-width:1px;rx:0;ry:0;}}",
        nbg = note_bkg,
        nbc = note_border,
    ));
    // .statediagram-note text
    s.push_str(&format!(
        "#{id} .statediagram-note text{{fill:{ntc};}}",
        ntc = note_text,
    ));
    // .statediagram-note .nodeLabel
    s.push_str(&format!(
        "#{id} .statediagram-note .nodeLabel{{color:{ntc};}}",
        ntc = note_text,
    ));
    // .statediagram .edgeLabel (upstream has `color: red; // ${options.noteTextColor};`)
    s.push_str(&format!("#{id} .statediagram .edgeLabel{{color:red;}}",));
    // [id$="-dependencyStart"], [id$="-dependencyEnd"]
    s.push_str(&format!(
        "#{id} [id$=\"-dependencyStart\"],#{id} [id$=\"-dependencyEnd\"]{{fill:{lc};stroke:{lc};stroke-width:{sw};}}",
        lc = line_color, sw = stroke_width,
    ));
    // .statediagramTitleText
    s.push_str(&format!(
        "#{id} .statediagramTitleText{{text-anchor:middle;font-size:18px;fill:{tc2};}}",
        tc2 = text_color,
    ));
    // [data-look="neo"].statediagram-cluster rect
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].statediagram-cluster rect{{fill:{mb};stroke:{sbr};stroke-width:{sw};}}"#,
        mb = main_bkg, sbr = state_border, sw = stroke_width,
    ));
    // [data-look="neo"].statediagram-cluster rect.outer
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].statediagram-cluster rect.outer{{rx:{r}px;ry:{r}px;filter:{ds};}}"#,
        r = radius, ds = neo_ds,
    ));

    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::state::parse;
    use crate::theme::get_theme;
    use std::fs;
    use std::path::PathBuf;

    /// Diagnostic probe: sweeps all cypress/state fixtures, ranks by
    /// common-prefix ratio, dumps top mismatches to /tmp.
    /// Never asserts; use with `-- --nocapture`.
    #[test]
    fn dump_state_multi_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let dir = base.join("tests/ext_fixtures/cypress/state");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut mmds: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("mmd") {
                    let stem = p.file_stem()?.to_str()?.to_string();
                    Some(format!("ext_fixtures/cypress/state/{}", stem))
                } else {
                    None
                }
            })
            .collect();
        mmds.sort();
        let mut results: Vec<(String, usize, usize, usize)> = vec![];
        for rel in &mmds {
            let Ok(mmd) = std::fs::read_to_string(base.join(format!("tests/{}.mmd", rel))) else {
                continue;
            };
            let Ok(exp) =
                std::fs::read_to_string(base.join(format!("tests/reference/{}.svg", rel)))
            else {
                continue;
            };
            let stem = rel.replace('/', "-").replace("ext-fixtures-", "");
            let id = format!("ref-{}", rel.replace('/', "-").replace("_", "-"));
            let Ok(d) = parse(&mmd) else { continue };
            let theme = get_theme("default");
            let Ok(l) = crate::layout::state::layout(&d, &theme) else {
                continue;
            };
            let Ok(got) = render(&d, &l, &theme, &id) else {
                continue;
            };
            let prefix = got
                .bytes()
                .zip(exp.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            results.push((rel.clone(), got.len(), exp.len(), prefix));
            if got == exp {
                let _ = std::fs::write(format!("/tmp/rust_state_{}.svg", stem), &got);
            }
        }
        // Sort by descending prefix ratio (prefix / min(got,exp)).
        results.sort_by(|a, b| {
            let ra = a.3 as f64 / a.1.min(a.2) as f64;
            let rb = b.3 as f64 / b.1.min(b.2) as f64;
            rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
        });
        let exact: Vec<_> = results
            .iter()
            .filter(|r| r.3 == r.1 && r.1 == r.2)
            .collect();
        eprintln!(
            "=== dump_state_multi_diff: {} fixtures, {} exact ===",
            results.len(),
            exact.len()
        );
        for r in exact {
            eprintln!("  EXACT: {}", r.0);
        }
        eprintln!("=== Top 10 by prefix ratio ===");
        for r in results.iter().take(10) {
            let p = r.3;
            let stem = r.0.replace("ext_fixtures/cypress/state/", "");
            eprintln!("  [{}] got={} exp={} prefix={}", stem, r.1, r.2, p);
            if p < r.1.min(r.2) {
                // Write top mismatches for examination.
                let _ = std::fs::write(format!("/tmp/rust_state_{}.svg", stem), {
                    let mmd_path = base.join(format!("tests/{}.mmd", r.0));
                    let Ok(mmd) = std::fs::read_to_string(&mmd_path) else {
                        continue;
                    };
                    let Ok(d) = parse(&mmd) else { continue };
                    let theme = get_theme("default");
                    let Ok(l) = crate::layout::state::layout(&d, &theme) else {
                        continue;
                    };
                    let Ok(got) = render(
                        &d,
                        &l,
                        &theme,
                        &format!("ref-{}", r.0.replace(['/', '_'], "-")),
                    ) else {
                        continue;
                    };
                    got
                });
                let exp_path = base.join(format!("tests/reference/{}.svg", r.0));
                let Ok(exp) = std::fs::read_to_string(&exp_path) else {
                    continue;
                };
                let Ok(got_bytes) = std::fs::read(format!("/tmp/rust_state_{}.svg", stem)) else {
                    continue;
                };
                let got = String::from_utf8_lossy(&got_bytes);
                eprintln!(
                    "  got[{}..{}] = {:?}",
                    p.saturating_sub(20),
                    (p + 80).min(got.len()),
                    &got[p.saturating_sub(20)..(p + 80).min(got.len())]
                );
                eprintln!(
                    "  exp[{}..{}] = {:?}",
                    p.saturating_sub(20),
                    (p + 80).min(exp.len()),
                    &exp[p.saturating_sub(20)..(p + 80).min(exp.len())]
                );
            }
        }
    }

    /// Diagnostic probe that reports alignment of the renderer's
    /// `<svg>`-shell + `<style>` block against the reference. The
    /// Wave 3.5 unified-shell work aims to minimise the post-viewBox
    /// drift — with byte-exact layout we'd hit `prefix == exp.len()`.
    /// Never asserts; use with `-- --nocapture` for a one-fixture diff.
    #[test]
    fn dump_state_01_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let rel = "ext_fixtures/cypress/state/01";
        let Ok(mmd) = std::fs::read_to_string(base.join(format!("tests/{}.mmd", rel))) else {
            return;
        };
        let Ok(exp) = std::fs::read_to_string(base.join(format!("tests/reference/{}.svg", rel)))
        else {
            return;
        };
        let id = "ref-ext-fixtures-cypress-state-01";
        let Ok(d) = parse(&mmd) else { return };
        let theme = get_theme("default");
        let Ok(l) = crate::layout::state::layout(&d, &theme) else {
            return;
        };
        let Ok(got) = render(&d, &l, &theme, id) else {
            return;
        };
        let _ = std::fs::write("/tmp/rust_state01.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "[state-diag] got={} exp={} prefix={}",
            got.len(),
            exp.len(),
            prefix
        );
        // Substitute the reference viewBox back into `got` so we can
        // measure *shell+style* alignment independently of the layout
        // divergence.
        if let (Some(got_vbox_end), Some(exp_vbox_end)) =
            (got.find("\" role="), exp.find("\" role="))
        {
            let got_vbox_start = got.rfind("style=\"max-width").unwrap_or(0);
            let exp_vbox_start = exp.rfind("style=\"max-width").unwrap_or(0);
            let (gpre, gpost) = got.split_at(got_vbox_start);
            let (_epre, epost) = exp.split_at(exp_vbox_start);
            let got_tail = &gpost[gpost.find("\" role=").unwrap_or(0) + 2..];
            let exp_tail = &epost[epost.find("\" role=").unwrap_or(0) + 2..];
            // tail starts at `role="graphics-document…`
            let tail_prefix = got_tail
                .bytes()
                .zip(exp_tail.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            eprintln!(
                "[state-diag] post-viewBox shell+style prefix={} (got_tail_len={}, exp_tail_len={})",
                tail_prefix,
                got_tail.len(),
                exp_tail.len()
            );
            let _ = (got_vbox_end, exp_vbox_end, gpre);
        }
    }

    #[test]
    fn sweep_all_fixtures_detail() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut all: Vec<(String, usize, usize, usize)> = vec![];
        for sub in ["cypress", "demos"] {
            let dir = base.join(format!("tests/ext_fixtures/{}/state", sub));
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut files: Vec<_> = entries.flatten().collect();
            files.sort_by_key(|e| e.file_name());
            for entry in files {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) != Some("mmd") {
                    continue;
                }
                let stem = p.file_stem().unwrap().to_str().unwrap().to_string();
                let rel = format!("ext_fixtures/{}/state/{}", sub, stem);
                let Ok(mmd) = fs::read_to_string(&p) else {
                    continue;
                };
                let ref_path = base.join(format!("tests/reference/{}.svg", rel));
                let Ok(exp) = fs::read_to_string(&ref_path) else {
                    continue;
                };
                let id = fixture_id(&rel);
                let Ok(d) = parse(&mmd) else { continue };
                let theme = get_theme("default");
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::layout::state::layout(&d, &theme)
                        .and_then(|l| render(&d, &l, &theme, &id))
                }));
                let got = match result {
                    Ok(Ok(s)) => s,
                    _ => continue,
                };
                let prefix = got
                    .bytes()
                    .zip(exp.bytes())
                    .take_while(|(a, b)| a == b)
                    .count();
                all.push((rel, got.len(), exp.len(), prefix));
            }
        }
        all.sort_by(|a, b| {
            let ra = a.3 as f64 / a.1.min(a.2) as f64;
            let rb = b.3 as f64 / b.1.min(b.2) as f64;
            rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
        });
        eprintln!("=== sweep_all_fixtures_detail: {} rendered ===", all.len());
        let exact: Vec<_> = all.iter().filter(|r| r.3 == r.1 && r.1 == r.2).collect();
        eprintln!(
            "EXACT ({}): {:?}",
            exact.len(),
            exact.iter().map(|r| &r.0).collect::<Vec<_>>()
        );
        eprintln!("Top 20 non-exact:");
        for r in all.iter().filter(|r| r.3 != r.1 || r.1 != r.2).take(20) {
            let stem = r.0.split('/').last().unwrap_or("");
            eprintln!("  [{}] got={} ref={} prefix={}", stem, r.1, r.2, r.3);
        }
    }

    #[test]
    fn gen_fixture_32_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/32.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/32.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-32";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        for n in &l.result.nodes {
            eprintln!(
                "node id={:?} w={:?} h={:?} x={:?} y={:?} label={:?} shape={:?} css={:?}",
                n.id, n.width, n.height, n.x, n.y, n.label, n.shape, n.css_classes
            );
        }
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_32.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "32: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 60;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &got[p.saturating_sub(ctx)..(p + 300).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &exp[p.saturating_sub(ctx)..(p + 300).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_29_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/29.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/29.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-29";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        for n in &l.result.nodes {
            eprintln!(
                "node id={:?} w={:?} h={:?} x={:?} y={:?} label={:?} shape={:?}",
                n.id, n.width, n.height, n.x, n.y, n.label, n.shape
            );
        }
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_29.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "29: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            eprintln!(
                "got[p-10:p+150]: {:?}",
                &got[prefix.saturating_sub(10)..(prefix + 150).min(got.len())]
            );
            eprintln!(
                "ref[p-10:p+150]: {:?}",
                &exp[prefix.saturating_sub(10)..(prefix + 150).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_04_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/04.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/04.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-04";
        let Ok(d) = parse(&mmd) else { return };
        let theme = get_theme("default");
        let Ok(l) = crate::layout::state::layout(&d, &theme) else {
            return;
        };
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_04.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "04: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            eprintln!(
                "got[p-10:p+150]: {:?}",
                &got[p.saturating_sub(10)..(p + 150).min(got.len())]
            );
            eprintln!(
                "ref[p-10:p+150]: {:?}",
                &exp[p.saturating_sub(10)..(p + 150).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_09_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/09.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/09.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-09";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_09.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "09: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            eprintln!(
                "got[p-20:p+120]: {:?}",
                &got[prefix.saturating_sub(20)..(prefix + 120).min(got.len())]
            );
            eprintln!(
                "ref[p-20:p+120]: {:?}",
                &exp[prefix.saturating_sub(20)..(prefix + 120).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_41_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/41.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/41.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-41";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_41.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "41: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 60;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &got[p.saturating_sub(ctx)..(p + 300).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &exp[p.saturating_sub(ctx)..(p + 300).min(exp.len())]
            );
        }
    }

    #[test]
    fn renders_minimal_diagram_without_panicking() {
        let src = "stateDiagram-v2\n[*] --> S1\nS1 --> [*]\n";
        let d = parse(src).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let svg = render(&d, &l, &theme, "t1").unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains(r#"class="statediagram""#));
        assert!(svg.contains(r#"aria-roledescription="stateDiagram""#));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn gen_fixture_06_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/06.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/06.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-06";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        // Show edge data
        for e in &l.result.edges {
            eprintln!("edge id={:?} points={:?}", e.id, e.points);
        }
        for n in &l.result.nodes {
            eprintln!(
                "node id={:?} x={:?} y={:?} w={:?} h={:?} shape={:?}",
                n.id, n.x, n.y, n.width, n.height, n.shape
            );
        }
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_06.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "06: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 30;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &got[p.saturating_sub(ctx)..(p + 300).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &exp[p.saturating_sub(ctx)..(p + 300).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_03_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/03.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/03.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-03";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        for n in &l.result.nodes {
            eprintln!("node id={:?} w={:?} h={:?} x={:?} y={:?} label={:?} shape={:?} css={:?} parent={:?} is_group={:?}",
                      n.id, n.width, n.height, n.x, n.y, n.label, n.shape, n.css_classes, n.parent_id, n.is_group);
        }
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_03.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "03: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 30;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &got[p.saturating_sub(ctx)..(p + 300).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &exp[p.saturating_sub(ctx)..(p + 300).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_30_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/30.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/30.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-30";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        for n in &l.result.nodes {
            eprintln!(
                "node id={:?} w={:?} h={:?} x={:?} y={:?} label={:?} shape={:?} css_classes={:?}",
                n.id, n.width, n.height, n.x, n.y, n.label, n.shape, n.css_classes
            );
        }
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_30.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "30: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 30;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &got[p.saturating_sub(ctx)..(p + 300).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &exp[p.saturating_sub(ctx)..(p + 300).min(exp.len())]
            );
        }
    }

    fn fixture_id(rel: &str) -> String {
        let mut id = String::from("ref-");
        let mut last_sep = false;
        for c in rel.chars() {
            if c.is_ascii_alphanumeric() {
                id.push(c);
                last_sep = false;
            } else if !last_sep {
                id.push('-');
                last_sep = true;
            }
        }
        while id.ends_with('-') {
            id.pop();
        }
        id
    }

    /// Smoke test across all fixtures. Reports byte-exact match count,
    /// never panics on mismatch (this renderer isn't byte-exact yet).
    #[test]
    fn reports_byte_exact_pass_count() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut groups = vec![];
        for sub in ["cypress", "demos"] {
            let dir = base.join(format!("tests/ext_fixtures/{}/state", sub));
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            let mut files: Vec<_> = entries.flatten().collect();
            files.sort_by_key(|e| e.file_name());
            for entry in files {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) != Some("mmd") {
                    continue;
                }
                let stem = p
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let rel = format!("ext_fixtures/{}/state/{}", sub, stem);
                let mmd = match fs::read_to_string(&p) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let ref_svg = base.join(format!("tests/reference/{}.svg", rel));
                let expected = match fs::read_to_string(&ref_svg) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let id = fixture_id(&rel);
                let mmd_c = mmd.clone();
                let id_c = id.clone();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::convert_with_id(&mmd_c, &id_c)
                }));
                let got = match result {
                    Ok(Ok(s)) => s,
                    _ => {
                        groups.push((rel, false, false, 0usize));
                        continue;
                    }
                };
                let exact = got == expected;
                // Common-prefix length: load-bearing for tracking how
                // much of the `<svg><style><g>…` shell aligns with the
                // reference. The remainder is the diagram body diff
                // (node/edge geometry, label markup).
                let prefix = got
                    .bytes()
                    .zip(expected.bytes())
                    .take_while(|(a, b)| a == b)
                    .count();
                groups.push((rel, true, exact, prefix));
            }
        }
        let total = groups.len();
        let rendered = groups.iter().filter(|(_, r, _, _)| *r).count();
        let exact = groups.iter().filter(|(_, _, e, _)| *e).count();
        let avg_prefix: usize = if rendered > 0 {
            groups.iter().map(|(_, _, _, p)| *p).sum::<usize>() / rendered
        } else {
            0
        };
        eprintln!(
            "[state] fixtures={} rendered={} byte-exact={} avg-common-prefix={}",
            total, rendered, exact, avg_prefix
        );
        let failed: Vec<&String> = groups
            .iter()
            .filter(|(_, r, _, _)| !*r)
            .map(|(rel, _, _, _)| rel)
            .collect();
        if !failed.is_empty() {
            eprintln!("[state] render-failures ({}):", failed.len());
            for f in failed {
                eprintln!("  - {}", f);
            }
        }
        // Print all non-exact fixtures with prefix info
        for (rel, rendered, exact, prefix) in &groups {
            if !*exact {
                eprintln!(
                    "[state] FAIL prefix={} rendered={} : {}",
                    prefix, rendered, rel
                );
            }
        }
    }

    #[test]
    fn gen_fixture_70_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/70.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/70.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-70";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_70.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "70: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 60;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &got[p.saturating_sub(ctx)..(p + 400).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &exp[p.saturating_sub(ctx)..(p + 400).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_39_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/39.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/39.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-39";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_39.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "39: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 60;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &got[p.saturating_sub(ctx)..(p + 300).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &exp[p.saturating_sub(ctx)..(p + 300).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_38_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/38.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/38.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-38";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_38.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "38: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 120;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                200,
                &got[p.saturating_sub(ctx)..(p + 200).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                200,
                &exp[p.saturating_sub(ctx)..(p + 200).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_06b_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/06.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/06.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-06";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_06b.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "06: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 120;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &got[p.saturating_sub(ctx)..(p + 400).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &exp[p.saturating_sub(ctx)..(p + 400).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_37_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/37.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/37.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-37";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_37.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "37: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 120;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                600,
                &got[p.saturating_sub(ctx)..(p + 600).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                600,
                &exp[p.saturating_sub(ctx)..(p + 600).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_04c_diff() {
        // fixture 04 in cypress directory
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/04.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/04.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-04";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_04c.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "04c: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 60;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &got[p.saturating_sub(ctx)..(p + 400).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &exp[p.saturating_sub(ctx)..(p + 400).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_16_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/16.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/16.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-16";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_16.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "16: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 120;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &got[p.saturating_sub(ctx)..(p + 400).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &exp[p.saturating_sub(ctx)..(p + 400).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_demos06_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/demos/state/06.mmd")).unwrap();
        let exp = fs::read_to_string(base.join("tests/reference/ext_fixtures/demos/state/06.svg"))
            .unwrap();
        let id = "ref-ext-fixtures-demos-state-06";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_demos06.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "demos06: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 120;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &got[p.saturating_sub(ctx)..(p + 400).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &exp[p.saturating_sub(ctx)..(p + 400).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_17_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/17.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/17.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-17";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_17.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "17: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 120;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &got[p.saturating_sub(ctx)..(p + 400).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &exp[p.saturating_sub(ctx)..(p + 400).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_demos03_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/demos/state/03.mmd")).unwrap();
        let exp = fs::read_to_string(base.join("tests/reference/ext_fixtures/demos/state/03.svg"))
            .unwrap();
        let id = "ref-ext-fixtures-demos-state-03";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        for n in &l.result.nodes {
            eprintln!(
                "node id={:?} x={:?} y={:?} w={:?} h={:?} shape={:?}",
                n.id, n.x, n.y, n.width, n.height, n.shape
            );
        }
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_demos03.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "demos03: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 40;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &got[p.saturating_sub(ctx)..(p + 300).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &exp[p.saturating_sub(ctx)..(p + 300).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_demos04_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/demos/state/04.mmd")).unwrap();
        let exp = fs::read_to_string(base.join("tests/reference/ext_fixtures/demos/state/04.svg"))
            .unwrap();
        let id = "ref-ext-fixtures-demos-state-04";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_demos04.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "demos04: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 40;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &got[p.saturating_sub(ctx)..(p + 300).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &exp[p.saturating_sub(ctx)..(p + 300).min(exp.len())]
            );
        }
    }

    #[test]
    fn check_diagram_title_width() {
        use crate::font_metrics::text_width;
        let title_text = "Very simple diagram";
        // resolveFont finds default: sans-serif 14px (no font-size on SVG inline style)
        let w14 = text_width(title_text, "sans-serif", 14.0, false, false);
        let w16 = text_width(title_text, "sans-serif", 16.0, false, false);
        eprintln!(
            "'Very simple diagram' sans 14px: {} (target: 145.5400390625)",
            w14
        );
        eprintln!("'Very simple diagram' sans 16px: {}", w16);
    }

    #[test]
    fn gen_fixture_demos05_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/demos/state/05.mmd")).unwrap();
        let exp = fs::read_to_string(base.join("tests/reference/ext_fixtures/demos/state/05.svg"))
            .unwrap();
        let id = "ref-ext-fixtures-demos-state-05";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        for n in &l.result.nodes {
            eprintln!(
                "node id={:?} x={:?} y={:?} w={:?} h={:?} shape={:?}",
                n.id, n.x, n.y, n.width, n.height, n.shape
            );
        }
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_demos05.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "demos05: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 60;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &got[p.saturating_sub(ctx)..(p + 400).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &exp[p.saturating_sub(ctx)..(p + 400).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_demos01_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/demos/state/01.mmd")).unwrap();
        let exp = fs::read_to_string(base.join("tests/reference/ext_fixtures/demos/state/01.svg"))
            .unwrap();
        let id = "ref-ext-fixtures-demos-state-01";
        let d = parse(&mmd).unwrap();
        eprintln!(
            "acc_title={:?} acc_descr={:?}",
            d.meta.acc_title, d.meta.acc_descr
        );
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        for n in &l.result.nodes {
            eprintln!(
                "node id={:?} x={:?} y={:?} w={:?} h={:?} shape={:?}",
                n.id, n.x, n.y, n.width, n.height, n.shape
            );
        }
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_demos01.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "demos01: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 40;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &got[p.saturating_sub(ctx)..(p + 400).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &exp[p.saturating_sub(ctx)..(p + 400).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_demos02_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/demos/state/02.mmd")).unwrap();
        let exp = fs::read_to_string(base.join("tests/reference/ext_fixtures/demos/state/02.svg"))
            .unwrap();
        let id = "ref-ext-fixtures-demos-state-02";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_demos02.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "demos02: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 40;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &got[p.saturating_sub(ctx)..(p + 400).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &exp[p.saturating_sub(ctx)..(p + 400).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_cypress08_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/08.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/08.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-08";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        for n in &l.result.nodes {
            eprintln!(
                "node id={:?} x={:?} y={:?} w={:?} h={:?} shape={:?}",
                n.id, n.x, n.y, n.width, n.height, n.shape
            );
        }
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_cy08.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "cy08: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 40;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &got[p.saturating_sub(ctx)..(p + 400).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &exp[p.saturating_sub(ctx)..(p + 400).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_cypress10_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/10.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/10.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-10";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        for e in &l.result.edges {
            eprintln!(
                "edge id={:?} label={:?} label_x={:?} label_y={:?}",
                e.id, e.label, e.label_x, e.label_y
            );
        }
        for n in &l.result.nodes {
            eprintln!(
                "node id={:?} x={:?} y={:?} w={:?} h={:?} shape={:?}",
                n.id, n.x, n.y, n.width, n.height, n.shape
            );
        }
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_cy10.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "cy10: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 40;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &got[p.saturating_sub(ctx)..(p + 400).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &exp[p.saturating_sub(ctx)..(p + 400).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_cypress43_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/43.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/43.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-43";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        for n in &l.result.nodes {
            eprintln!(
                "node id={:?} x={:?} y={:?} w={:?} h={:?} shape={:?}",
                n.id, n.x, n.y, n.width, n.height, n.shape
            );
        }
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_cy43.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "cy43: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 40;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &got[p.saturating_sub(ctx)..(p + 400).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                400,
                &exp[p.saturating_sub(ctx)..(p + 400).min(exp.len())]
            );
        }
    }

    #[test]
    fn gen_fixture_cypress03_diff() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = fs::read_to_string(base.join("tests/ext_fixtures/cypress/state/03.mmd")).unwrap();
        let exp =
            fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/state/03.svg"))
                .unwrap();
        let id = "ref-ext-fixtures-cypress-state-03";
        let d = parse(&mmd).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        for n in &l.result.nodes {
            eprintln!(
                "node id={:?} x={:?} y={:?} w={:?} h={:?} shape={:?}",
                n.id, n.x, n.y, n.width, n.height, n.shape
            );
        }
        let got = render(&d, &l, &theme, id).unwrap();
        let _ = fs::write("/tmp/fresh_state_cy03.svg", &got);
        let prefix = got
            .bytes()
            .zip(exp.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        eprintln!(
            "cy03: got={} exp={} prefix={} exact={}",
            got.len(),
            exp.len(),
            prefix,
            got == exp
        );
        if got != exp {
            let p = prefix;
            let ctx = 40;
            eprintln!(
                "got[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &got[p.saturating_sub(ctx)..(p + 300).min(got.len())]
            );
            eprintln!(
                "ref[p-{}:p+{}]: {:?}",
                ctx,
                300,
                &exp[p.saturating_sub(ctx)..(p + 300).min(exp.len())]
            );
        }
    }

    /// Verbose per-fixture diff analysis for near-miss fixtures.
    #[test]
    fn report_per_fixture_prefix() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut groups: Vec<(String, bool, bool, usize, usize, usize)> = vec![];
        for sub in ["cypress", "demos"] {
            let dir = base.join(format!("tests/ext_fixtures/{}/state", sub));
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            let mut files: Vec<_> = entries.flatten().collect();
            files.sort_by_key(|e| e.file_name());
            for entry in files {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) != Some("mmd") {
                    continue;
                }
                let stem = p
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let rel = format!("ext_fixtures/{}/state/{}", sub, stem);
                let mmd = match fs::read_to_string(&p) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let ref_svg = base.join(format!("tests/reference/{}.svg", rel));
                let expected = match fs::read_to_string(&ref_svg) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let id = fixture_id(&rel);
                let mmd_c = mmd.clone();
                let id_c = id.clone();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::convert_with_id(&mmd_c, &id_c)
                }));
                let got = match result {
                    Ok(Ok(s)) => s,
                    _ => {
                        groups.push((rel, false, false, 0, 0, 0));
                        continue;
                    }
                };
                let exact = got == expected;
                let prefix = got
                    .bytes()
                    .zip(expected.bytes())
                    .take_while(|(a, b)| a == b)
                    .count();
                groups.push((rel, true, exact, prefix, got.len(), expected.len()));
            }
        }
        // Sort by prefix descending (near-misses first)
        let mut non_exact: Vec<_> = groups.iter().filter(|(_, _, e, _, _, _)| !*e).collect();
        non_exact.sort_by(|a, b| b.3.cmp(&a.3));
        eprintln!("=== Non-exact fixtures sorted by prefix (highest first) ===");
        for (rel, rendered, _, prefix, got_len, exp_len) in &non_exact {
            if !rendered {
                eprintln!("  PANIC  {}", rel);
                continue;
            }
            let stem: &str = rel.split('/').last().unwrap_or("");
            eprintln!(
                "  prefix={:6}  got={:6}  exp={:6}  {}",
                prefix, got_len, exp_len, stem
            );
        }
        let exact_count = groups.iter().filter(|(_, _, e, _, _, _)| *e).count();
        eprintln!("=== exact={}/{} ===", exact_count, groups.len());

        // Detailed diff for each failing fixture — only prints the divergence context
        // Sort by prefix descending so highest near-miss prints last.
        let base2 = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for (rel, rendered, exact, prefix, _, _) in &groups {
            if *exact || !rendered {
                continue;
            }
            let mmd = match fs::read_to_string(base2.join(format!("tests/{}.mmd", rel))) {
                Ok(s) => s,
                _ => continue,
            };
            let expected =
                match fs::read_to_string(base2.join(format!("tests/reference/{}.svg", rel))) {
                    Ok(s) => s,
                    _ => continue,
                };
            let id = fixture_id(rel);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::convert_with_id(&mmd, &id)
            }));
            let got = match result {
                Ok(Ok(s)) => s,
                _ => continue,
            };
            let p = *prefix;
            let ctx = 40;
            let g_ctx = &got[p.saturating_sub(ctx)..(p + 120).min(got.len())];
            let e_ctx = &expected[p.saturating_sub(ctx)..(p + 120).min(expected.len())];
            eprintln!(
                "DIFF[{}] got: {:?}",
                rel.split('/').last().unwrap_or(rel),
                g_ctx
            );
            eprintln!(
                "DIFF[{}] exp: {:?}",
                rel.split('/').last().unwrap_or(rel),
                e_ctx
            );
        }
    }
}
