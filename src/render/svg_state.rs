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
use crate::layout::state::StateLayout;
use crate::layout::unified::types::{Bounds, Edge, Node, Point};
use crate::model::state::StateDiagram;
use crate::render::edges::{self, CurveType};
use crate::render::shapes::{self, types::fmt_num};
use crate::render::unified_shell;
use crate::theme::css as theme_css;
use crate::theme::ThemeVariables;

pub fn render(
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
    let (vx, vy, vw, vh) = compute_viewbox(l, pad);

    // ── Opening <svg> — canonical attribute order -----------------
    out.push_str(&unified_shell::open_unified_svg(
        id,
        vw,
        (vx, vy, vw, vh),
        Some("statediagram"),
        "stateDiagram",
    ));

    // ── <style> block — base preamble + state-specific rules + tail.
    out.push_str(&style_block(id, theme));

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

    // Clusters (composite states) -------------------------------
    out.push_str(r#"<g class="clusters">"#);
    for n in l.result.nodes.iter().filter(|n| n.is_group) {
        out.push_str(&emit_cluster(n));
    }
    out.push_str("</g>");

    // Edge paths ------------------------------------------------
    out.push_str(r#"<g class="edgePaths">"#);
    for e in &l.result.edges {
        out.push_str(&emit_edge_path(id, e));
    }
    out.push_str("</g>");

    // Edge labels ----------------------------------------------
    out.push_str(r#"<g class="edgeLabels">"#);
    for e in &l.result.edges {
        out.push_str(&emit_edge_label(e));
    }
    out.push_str("</g>");

    // Nodes -----------------------------------------------------
    out.push_str(r#"<g class="nodes">"#);
    for n in l.result.nodes.iter().filter(|n| !n.is_group) {
        if n.extra.get("__skip_render").is_some() {
            continue;
        }
        if let Some(svg) = emit_node(id, n, theme) {
            out.push_str(&svg);
        }
    }
    out.push_str("</g>");

    out.push_str(unified_shell::close_root_group());
    out.push_str(unified_shell::close_seed_group());

    // Drop-shadow filter defs (match upstream tail).
    out.push_str(&unified_shell::emit_defs_shell(id, true, true));

    out.push_str(unified_shell::close_unified_svg());
    let _ = d; // reserved for v1/v2-specific tweaks once wired.
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
fn compute_viewbox(l: &StateLayout, pad: f64) -> (f64, f64, f64, f64) {
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

    for n in &l.result.nodes {
        if n.is_group || n.extra.get("__skip_render").is_some() {
            continue;
        }
        let shape = n.shape.as_deref().unwrap_or("state");
        let (nx_min, nx_max, ny_min, ny_max) = if matches!(
            shape,
            "stateStart" | "state_start" | "start"
            | "stateEnd" | "state_end" | "end"
            | "forkJoin" | "fork_join" | "fork" | "join"
        ) {
            // Circle or rectangle at local origin, r=7.
            (-7.0_f64, 7.0_f64, -7.0_f64, 7.0_f64)
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

    // Edge points — in absolute layout space (no transform on edge groups).
    for e in &l.result.edges {
        if let Some(pts) = &e.points {
            for p in pts {
                g_x_min = g_x_min.min(p.x);
                g_x_max = g_x_max.max(p.x);
                g_y_min = g_y_min.min(p.y);
                g_y_max = g_y_max.max(p.y);
            }
        }
    }

    // Fall back when the layout has no renderable content at all.
    if !g_x_min.is_finite() {
        return (-(7.0 + pad), -(7.0 + pad), 14.0 + 2.0 * pad, 14.0 + 2.0 * pad);
    }

    let vx = g_x_min - pad;
    let vy = g_y_min - pad;
    let vw = (g_x_max - g_x_min) + 2.0 * pad;
    let vh = (g_y_max - g_y_min) + 2.0 * pad;

    (vx, vy, vw.max(1.0), vh.max(1.0))
}

fn viewbox(b: &Bounds, pad: f64) -> (f64, f64, f64, f64) {
    let w = (b.width + 2.0 * pad).max(1.0);
    let h = (b.height + 2.0 * pad).max(1.0);
    let x = b.x - pad;
    let y = b.y - pad;
    (x, y, w, h)
}

fn emit_cluster(n: &Node) -> String {
    let w = n.width.unwrap_or(0.0);
    let h = n.height.unwrap_or(0.0);
    let label = n.label.as_deref().unwrap_or("");
    let css = n.css_classes.as_deref().unwrap_or("statediagram-cluster");
    format!(
        concat!(
            r#"<g class=" statediagram-state {css}" id="{id}" data-id="{nid}" data-look="classic">"#,
            r#"<g><rect class="outer" x="{rx}" y="{ry}" width="{w}" height="{h}" data-look="classic"></rect></g>"#,
            r#"<g class="cluster-label"><foreignObject width="0" height="0"><div xmlns="http://www.w3.org/1999/xhtml">{lbl}</div></foreignObject></g>"#,
            r#"</g>"#,
        ),
        css = css,
        id = xml_escape(&n.id),
        nid = xml_escape(&n.id),
        rx = fmt_num(-w / 2.0),
        ry = fmt_num(-h / 2.0),
        w = fmt_num(w),
        h = fmt_num(h),
        lbl = xml_escape(label),
    )
}

fn emit_edge_path(id: &str, e: &Edge) -> String {
    let Some(points) = &e.points else {
        return String::new();
    };
    if points.len() < 2 {
        return String::new();
    }
    let pts: Vec<Point> = points.iter().map(|p| Point { x: p.x, y: p.y }).collect();
    let d = edges::build_path(&pts, CurveType::Basis);
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
    format!(
        r##"<path d="{d}" id="{id}-{eid}" class="{cls}" style="fill:none;;;fill:none" data-edge="true" data-et="edge" data-id="{eid}" data-points="{b64}" data-look="classic" marker-end="url(#{id}_stateDiagram-barbEnd)"></path>"##,
        d = d,
        id = id,
        eid = e.id,
        cls = class,
        b64 = data_points_b64,
    )
}

fn emit_edge_label(e: &Edge) -> String {
    use crate::render::foreign_object::{self, LabelOpts};
    use crate::font_metrics::text_width;

    let raw = e.label.as_deref().unwrap_or("");
    let (body, wrap_in_p) = if raw.trim().is_empty() {
        if raw.is_empty() {
            (String::new(), false)
        } else {
            // Whitespace-only: preserve literal whitespace in <p>.
            (format!("<p>{}</p>", xml_escape(raw)), false)
        }
    } else {
        (xml_escape(raw), true)
    };

    // Measure label text for foreignObject dimensions.
    let (lw, lh) = if raw.is_empty() {
        (0.0, 16.296875) // default line-height at 14px sans-serif
    } else {
        let tw = text_width(raw.trim(), "sans-serif", 14.0, false, false);
        (tw, 16.296875)
    };

    let x = e.label_x.unwrap_or(0.0);
    let y = e.label_y.unwrap_or(0.0);
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
        _ => shapes::draw(shape, n, theme).ok(),
    }
}

fn emit_state_start(id: &str, n: &Node, _theme: &ThemeVariables) -> Option<String> {
    let w = n.width.unwrap_or(14.0).max(14.0);
    let r = w / 2.0;
    let nid = n.dom_id.clone().unwrap_or_else(|| n.id.clone());
    let tx = n.x.unwrap_or(0.0);
    let ty = n.y.unwrap_or(0.0);
    Some(format!(
        r#"<g class="node default" id="{id}-{nid}" data-look="classic" transform="translate({tx}, {ty})"><circle class="state-start" r="{r}" width="{w}" height="{w}"></circle></g>"#,
        id = id,
        nid = xml_escape(&nid),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
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
            r#"<g class="node default" id="{id}-{nid}" data-look="classic" transform="translate({tx}, {ty})">"#,
            r#"<g class="outer-path">"#,
            r#"<path d="{outer_fill}" stroke="none" stroke-width="0" fill="{mb}" style=""></path>"#,
            r#"<path d="{outer_stroke}" stroke="{lc}" stroke-width="2" fill="none" stroke-dasharray="0 0" style=""></path>"#,
            r#"</g>"#,
            r#"<g>"#,
            r#"<path d="{inner_fill}" stroke="none" stroke-width="0" fill="{nb}" style=""></path>"#,
            r#"<path d="{inner_stroke}" stroke="{nb}" stroke-width="2" fill="none" stroke-dasharray="0 0" style=""></path>"#,
            r#"</g>"#,
            r#"</g>"#,
        ),
        id = id,
        nid = xml_escape(&nid),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
        outer_fill = STATE_END_OUTER_PATH,
        outer_stroke = STATE_END_OUTER_PATH,
        inner_fill = STATE_END_INNER_PATH,
        inner_stroke = STATE_END_INNER_PATH,
        lc = line_color,
        mb = main_bkg,
        nb = node_border,
    ))
}

fn emit_fork_join(id: &str, n: &Node, theme: &ThemeVariables) -> Option<String> {
    let dir = n.dir.as_deref();
    let (w, h) = if matches!(dir, Some("LR")) {
        (n.width.unwrap_or(10.0).max(10.0), n.height.unwrap_or(70.0).max(70.0))
    } else {
        (n.width.unwrap_or(70.0).max(70.0), n.height.unwrap_or(10.0).max(10.0))
    };
    let x = -w / 2.0;
    let y = -h / 2.0;
    let classes = shapes::types::get_node_classes(n.look.as_deref(), n.css_classes.as_deref(), None);
    let nid = n.dom_id.clone().unwrap_or_else(|| n.id.clone());
    let tx = n.x.unwrap_or(0.0);
    let ty = n.y.unwrap_or(0.0);
    let line = theme.line_color.as_deref().unwrap_or("black");
    Some(format!(
        r#"<g class="{classes}" id="{id}-{nid}" data-look="classic" transform="translate({tx}, {ty})"><rect class="fork-join" x="{x}" y="{y}" width="{w}" height="{h}" style="fill:{line};stroke:{line}"></rect></g>"#,
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
    ))
}

/// Emit a normal state node (rounded rect + label).
/// Matches upstream's `drawRect` with `rx=5, ry=5` + `labelHelper`.
fn emit_state_node(id: &str, n: &Node, _theme: &ThemeVariables) -> Option<String> {
    use crate::render::foreign_object::{measure_html_label, LabelOpts};

    let w = n.width.unwrap_or(0.0);
    let h = n.height.unwrap_or(0.0);
    let r = n.rx.unwrap_or(5.0);
    let classes = shapes::types::get_node_classes(n.look.as_deref(), n.css_classes.as_deref(), None);
    let nid = n.dom_id.clone().unwrap_or_else(|| n.id.clone());
    let tx = n.x.unwrap_or(0.0);
    let ty = n.y.unwrap_or(0.0);
    let label = n.label.clone().unwrap_or_default();
    let is_markdown = n.label_type.as_deref() == Some("markdown");
    let label_style = n.label_style.as_deref().unwrap_or("");

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}-{nid}" data-look="classic" transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = id,
        nid = xml_escape(&nid),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    out.push_str(&format!(
        r#"<rect class="basic label-container" style="" rx="{r}" ry="{r}" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
        r = fmt_num(r),
        x = fmt_num(-w / 2.0),
        y = fmt_num(-h / 2.0),
        w = fmt_num(w),
        h = fmt_num(h),
    ));
    if !label.is_empty() {
        let escaped = xml_escape(&label);
        let (lw, lh) = measure_html_label(
            &escaped,
            &crate::render::foreign_object::HtmlLabelFont::default(),
            200.0,
            true,
        );
        let opts = LabelOpts {
            extra_span_classes: if is_markdown { "markdown-node-label" } else { "" },
            group_style: if label_style.is_empty() { Some("") } else { Some(label_style) },
            ..LabelOpts::default()
        };
        out.push_str(&crate::render::foreign_object::render_node_label(
            &escaped, lw, lh, &opts,
        ));
    }
    out.push_str("</g>");
    Some(out)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `<style>` block — built from the shared base preamble + the full
/// state-specific CSS (ported from upstream `state/styles.js`) + the
/// shared neo-look tail.
fn style_block(id: &str, theme: &ThemeVariables) -> String {
    let mut s = String::with_capacity(4096);
    s.push_str("<style>");
    s.push_str(&theme_css::base_preamble(id, theme));
    s.push_str(&state_specific_css(id, theme));
    s.push_str(&theme_css::neo_look_block(id, theme));
    s.push_str("</style>");
    s
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
    let composite_bg = theme.composite_background.as_deref().or(theme.background.as_deref()).unwrap_or("white");
    let composite_title_bg = theme.composite_title_background.as_deref().unwrap_or("#ECECFF");
    let state_bkg = theme.state_bkg.as_deref().or(theme.main_bkg.as_deref()).unwrap_or("#ECECFF");
    let state_border = theme.state_border.as_deref().or(theme.node_border.as_deref()).unwrap_or("#9370DB");
    let alt_bg = theme.alt_background.as_deref().unwrap_or("#efefef");
    let note_bkg = theme.note_bkg_color.as_deref().unwrap_or("#fff5ad");
    let note_border = theme.note_border_color.as_deref().unwrap_or("#aaaa33");
    let note_text = theme.note_text_color.as_deref().unwrap_or("#333");
    let label_bg = theme.label_background_color.as_deref().unwrap_or("#ECECFF");
    let edge_label_bg = theme.edge_label_background.as_deref().unwrap_or("rgba(232,232,232, 0.8)");
    let transition_label_color = theme.transition_label_color.as_deref().or(theme.tertiary_text_color.as_deref()).unwrap_or("#333");
    let radius = theme.radius.unwrap_or(5);
    // drop-shadow for neo look
    let drop_shadow = theme.drop_shadow.as_deref().unwrap_or("drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))");
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
        mb = main_bkg, nb = node_border,
    ));
    // g.stateGroup line
    s.push_str(&format!(
        "#{id} g.stateGroup line{{stroke:{lc};stroke-width:{sw};}}",
        lc = line_color, sw = stroke_width,
    ));
    // .transition
    s.push_str(&format!(
        "#{id} .transition{{stroke:{tc};stroke-width:{sw};fill:none;}}",
        tc = transition_color, sw = stroke_width,
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
        nbc = note_border, nbg = note_bkg,
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
        ieb = inner_end_bg, bg = background,
    ));
    // .end-state-inner
    s.push_str(&format!(
        "#{id} .end-state-inner{{fill:{cbg};stroke-width:1.5;}}",
        cbg = composite_bg,
    ));
    // .node rect
    s.push_str(&format!(
        "#{id} .node rect{{fill:{sb};stroke:{sbr};stroke-width:{sw}px;}}",
        sb = state_bkg, sbr = state_border, sw = stroke_width,
    ));
    // .node polygon
    s.push_str(&format!(
        "#{id} .node polygon{{fill:{mb};stroke:{sbr};stroke-width:{sw}px;}}",
        mb = main_bkg, sbr = state_border, sw = stroke_width,
    ));
    // [id$="-barbEnd"]
    s.push_str(&format!(
        "#{id} [id$=\"-barbEnd\"]{{fill:{lc};}}",
        lc = line_color,
    ));
    // .statediagram-cluster rect
    s.push_str(&format!(
        "#{id} .statediagram-cluster rect{{fill:{ctbg};stroke:{sbr};stroke-width:{sw}px;}}",
        ctbg = composite_title_bg, sbr = state_border, sw = stroke_width,
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
    s.push_str(&format!(
        "#{id} .statediagram-cluster .inner{{rx:0;ry:0;}}",
    ));
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
    s.push_str(&format!(
        "#{id} .note-edge{{stroke-dasharray:5;}}",
    ));
    // .statediagram-note rect (twice — upstream emits it twice)
    s.push_str(&format!(
        "#{id} .statediagram-note rect{{fill:{nbg};stroke:{nbc};stroke-width:1px;rx:0;ry:0;}}",
        nbg = note_bkg, nbc = note_border,
    ));
    s.push_str(&format!(
        "#{id} .statediagram-note rect{{fill:{nbg};stroke:{nbc};stroke-width:1px;rx:0;ry:0;}}",
        nbg = note_bkg, nbc = note_border,
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
    s.push_str(&format!(
        "#{id} .statediagram .edgeLabel{{color:red;}}",
    ));
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
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
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
        let exact: Vec<_> = results.iter().filter(|r| r.3 == r.1 && r.1 == r.2).collect();
        eprintln!("=== dump_state_multi_diff: {} fixtures, {} exact ===", results.len(), exact.len());
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
                    let Ok(mmd) = std::fs::read_to_string(&mmd_path) else { continue };
                    let Ok(d) = parse(&mmd) else { continue };
                    let theme = get_theme("default");
                    let Ok(l) = crate::layout::state::layout(&d, &theme) else { continue };
                    let Ok(got) = render(&d, &l, &theme, &format!("ref-{}", r.0.replace(['/', '_'], "-"))) else { continue };
                    got
                });
                let exp_path = base.join(format!("tests/reference/{}.svg", r.0));
                let Ok(exp) = std::fs::read_to_string(&exp_path) else { continue };
                let Ok(got_bytes) = std::fs::read(format!("/tmp/rust_state_{}.svg", stem)) else { continue };
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
                let theme = get_theme("default");
                let mmd_c = mmd.clone();
                let id_c = id.clone();
                let theme_c = theme.clone();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    parse(&mmd_c).and_then(|d| {
                        let eff = d
                            .theme_override
                            .as_deref()
                            .map(get_theme)
                            .unwrap_or_else(|| theme_c.clone());
                        let l = crate::layout::state::layout(&d, &eff)?;
                        render(&d, &l, &eff, &id_c)
                    })
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
    }
}
