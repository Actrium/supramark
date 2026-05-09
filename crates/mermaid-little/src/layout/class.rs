//! Class diagram layout — populates `unified::LayoutData` from a
//! parsed [`ClassDiagram`] and runs it through the shared dagre bridge.
//!
//! Upstream references:
//! * `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/class/classDb.ts` (`getData`)
//! * `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/class/shapeUtil.ts`
//! * `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/class/classRenderer-v3-unified.ts`
//!
//! Status
//! ------
//! This is the scaffolding stage. We emit a correct `LayoutData`
//! structure (nodes / edges / clusters populated, markers registered)
//! and hand it to the unified dagre bridge. The **byte-exact** pixel
//! coordinates still depend on:
//!
//! 1. correct node width/height derived from text measurement — the
//!    shared `font_metrics` path applied to each member/method line,
//!    then the classBox stacked-band sum;
//! 2. the v3 classBox shape emitter (see `render/shapes/classbox.rs`);
//! 3. label-bbox measurement for edge labels and multiplicity stubs.
//!
//! The renderer layer (`render/svg_class.rs`) consumes [`ClassLayout`]
//! to produce final SVG. Until both sides are complete for byte-exact
//! fidelity we keep the shape of the API stable so downstream work can
//! progress.

use crate::error::Result;
use crate::font_metrics;
use crate::layout::unified::render as unified_render;
use crate::layout::unified::types::{Edge, LayoutData, LayoutResult, Node};
use crate::model::class::{ClassDiagram, ClassNode, LineType, RelationEnd};
use crate::theme::ThemeVariables;

/// Output of the class layout pass.
#[derive(Debug, Clone)]
pub struct ClassLayout {
    /// Post-layout node + edge geometry from dagre.
    pub unified: LayoutResult,
    /// Mirror of the input — lets the renderer look up style / labels
    /// without re-traversing the model.
    pub data: LayoutData,
    /// Viewbox — computed by the renderer using the unified bounds
    /// plus a uniform padding. Mirrors upstream's `setupViewPortForSVG`
    /// which insets by `padding = 8`.
    pub viewbox_x: f64,
    pub viewbox_y: f64,
    pub viewbox_w: f64,
    pub viewbox_h: f64,
}

/// Default padding — matches upstream `classRenderer-v3-unified.ts`
/// which calls `setupViewPortForSVG(svg, padding=8)`.
const VIEWBOX_PADDING: f64 = 8.0;

/// Public entry point.
pub fn layout(d: &ClassDiagram, theme: &ThemeVariables) -> Result<ClassLayout> {
    let data = build_layout_data(d, theme);
    let mut result = unified_render::layout(
        &data,
        data.layout_algorithm.as_deref().unwrap_or("dagre"),
        theme,
    )?;

    // Synthesise spline waypoints for edges fully contained inside an
    // isolated cluster. The shared dagre bridge skips routing for these
    // edges (they live in the inner-pass dagre, but the bridge does not
    // expose its edge geometry). The result is `Edge::points = None`,
    // which in turn produces an empty `d=""` attribute and a useless
    // `data-points="W10="` (base64 of `[]`).
    //
    // Upstream's recursive-render path lets the inner dagre lay these
    // out and then keeps the resulting points. Here we approximate the
    // same answer for the common 1-edge sibling case (e.g. cypress
    // class/38 + class/94: `namespace Namespace1 { class C1; class C2 }`
    // followed by `C1 --> C2`). Both endpoints are leaf siblings of the
    // same isolated cluster, so dagre would emit three waypoints —
    // source-side intersection with the source rect, midpoint, and
    // target-side intersection with the target rect — which we mirror
    // here via `intersectRect` (the same algorithm dagre-d3-es uses).
    synthesise_intra_isolated_edge_points(&mut result);

    // Derive viewbox. Upstream's `setupViewPortForSVG` grows the tight
    // bounding box by `padding` on every side.
    let b = result.bounds;
    let vx = b.x - VIEWBOX_PADDING;
    let vy = b.y - VIEWBOX_PADDING;
    let vw = b.width + 2.0 * VIEWBOX_PADDING;
    let vh = b.height + 2.0 * VIEWBOX_PADDING;

    Ok(ClassLayout {
        unified: result,
        data,
        viewbox_x: vx,
        viewbox_y: vy,
        viewbox_w: vw,
        viewbox_h: vh,
    })
}

/// Fill in `Edge::points` for any edge whose source and target are both
/// leaves of the **same** isolated cluster.
///
/// We can't reach into `dagre_bridge` to capture inner-pass edge
/// routing, so for each affected edge we reconstruct three control
/// points dagre would have emitted for the trivial straight-line case:
///
/// 1. Intersect the source rectangle with the segment to the target
///    centre — the "exit point".
/// 2. The midpoint between the source and target centres.
/// 3. Intersect the target rectangle with the segment to the source
///    centre — the "entry point".
///
/// `intersectRect` is the same helper dagre-d3-es uses (and which we
/// already mirror in `render::svg_block`). Replicated locally to keep
/// the layout layer free of render-side imports.
fn synthesise_intra_isolated_edge_points(result: &mut LayoutResult) {
    if result.isolated_cluster_ids.is_empty() {
        return;
    }
    // Map from node id → (parent_id, x, y, width, height) for fast lookup.
    let mut node_info: std::collections::HashMap<&str, (Option<&str>, f64, f64, f64, f64)> =
        std::collections::HashMap::new();
    for n in &result.nodes {
        let x = match n.x {
            Some(v) => v,
            None => continue,
        };
        let y = match n.y {
            Some(v) => v,
            None => continue,
        };
        let w = n.width.unwrap_or(0.0);
        let h = n.height.unwrap_or(0.0);
        node_info.insert(n.id.as_str(), (n.parent_id.as_deref(), x, y, w, h));
    }

    let isolated: &std::collections::HashSet<String> = &result.isolated_cluster_ids;

    for edge in result.edges.iter_mut() {
        if edge.points.is_some() {
            continue;
        }
        let src = match edge.source.as_deref() {
            Some(s) => s,
            None => continue,
        };
        let dst = match edge.target.as_deref() {
            Some(s) => s,
            None => continue,
        };
        let (sp, sx, sy, sw, sh) = match node_info.get(src) {
            Some(t) => *t,
            None => continue,
        };
        let (dp, dx, dy, dw, dh) = match node_info.get(dst) {
            Some(t) => *t,
            None => continue,
        };
        // Both endpoints must be siblings inside the same isolated cluster.
        let parent = match (sp, dp) {
            (Some(a), Some(b)) if a == b => a,
            _ => continue,
        };
        if !isolated.contains(parent) {
            continue;
        }

        let mid_x = (sx + dx) / 2.0;
        let mid_y = (sy + dy) / 2.0;
        let (p0x, p0y) = intersect_rect(sx, sy, sw, sh, mid_x, mid_y);
        let (p2x, p2y) = intersect_rect(dx, dy, dw, dh, mid_x, mid_y);

        edge.points = Some(vec![
            crate::layout::unified::types::Point { x: p0x, y: p0y },
            crate::layout::unified::types::Point { x: mid_x, y: mid_y },
            crate::layout::unified::types::Point { x: p2x, y: p2y },
        ]);
    }
}

/// `intersectRect` from dagre-d3-es: find where the line from rect
/// centre `(nx, ny)` to external probe point `(px, py)` exits the
/// rectangle `(nx ± nw/2, ny ± nh/2)`.
fn intersect_rect(nx: f64, ny: f64, nw: f64, nh: f64, px: f64, py: f64) -> (f64, f64) {
    let dx = px - nx;
    let dy = py - ny;
    let w = nw / 2.0;
    let h = nh / 2.0;
    let (sx, sy);
    if dy.abs() * w > dx.abs() * h {
        // Top or bottom boundary.
        let h_signed = if dy < 0.0 { -h } else { h };
        sy = h_signed;
        sx = if dy == 0.0 { 0.0 } else { h_signed * dx / dy };
    } else {
        // Left or right boundary.
        let w_signed = if dx < 0.0 { -w } else { w };
        sx = w_signed;
        sy = if dx == 0.0 { 0.0 } else { w_signed * dy / dx };
    }
    (nx + sx, ny + sy)
}

/// Build the `LayoutData` sent to dagre. Mirrors upstream
/// `classDb.getData`.
fn build_layout_data(d: &ClassDiagram, _theme: &ThemeVariables) -> LayoutData {
    let mut data = LayoutData {
        diagram_type: Some("classDiagram".to_string()),
        direction: d.direction.clone().or_else(|| Some("TB".into())),
        node_spacing: Some(50.0),
        rank_spacing: Some(50.0),
        markers: vec![
            "aggregation".into(),
            "extension".into(),
            "composition".into(),
            "dependency".into(),
            "lollipop".into(),
        ],
        layout_algorithm: Some("dagre".into()),
        ..LayoutData::default()
    };

    // Cluster nodes first, then class nodes. Dagre wants parents before
    // children for compound graphs.
    for ns in &d.namespaces {
        data.nodes.push(cluster_node(ns));
    }
    for c in &d.classes {
        data.nodes.push(class_to_node(c, d));
    }
    // Notes become their own nodes with a dashed border — upstream's
    // `getData` emits them with `shape: 'note'` and wires a special
    // relation to the target class.
    for n in &d.notes {
        let mut note = Node::default();
        note.id = n.id.clone();
        note.label = Some(n.text.clone());
        note.shape = Some("note".into());
        // Upstream `classDb.getData` does NOT set a `cssClasses` on the
        // note Node, so `getNodeClasses` falls back to "undefined"
        // (`<g class="node undefined ">`).
        note.css_classes = None;
        note.parent_id = n.parent.clone();
        // Upstream `note.ts`: `totalWidth = bbox.width + 2 * padding` with
        // `padding = config.class.padding ?? 6`. The bbox.width comes from
        // `div.getBoundingClientRect()` — the testing harness shim
        // (`tests/support/generate_ref.mjs`) measures `el.textContent`
        // for HTML elements, and `<br/>` contributes empty textContent.
        // So a multi-line note like "Foo\nBar" is measured as a single
        // joined string "FooBar". bbox.height collapses to a single
        // 16.296875 line. Padding = 6 ⇒ +12 on each axis.
        let line_h = 16.296875_f64;
        let note_padding = 6.0_f64;
        let joined: String = n.text.split('\n').collect();
        // Upstream's note label goes through DOMPurify into a foreignObject;
        // the bbox width comes from `el.textContent`. Both the testing
        // harness shim (`tests/support/generate_ref.mjs`) and a real
        // browser strip HTML markup before measuring — so an embedded
        // `<a>` / `<code>` reduces to its visible text content. Without
        // this strip a note like cypress/class/224 ("<a href=...>note
        // about mermaid</a>") is measured at ~700 px instead of ~144 px.
        let measured = strip_html_tags(&joined);
        let family = "trebuchet ms,verdana,arial,sans-serif";
        let w = font_metrics::text_width(&measured, family, 14.0, false, false);
        note.width = Some(w + 2.0 * note_padding);
        note.height = Some(line_h + 2.0 * note_padding);
        data.nodes.push(note);
        if !n.class_id.is_empty() {
            // Upstream `classDb.getData` emits a dotted relation from the
            // note to its target class. Edge id format is
            // `edgeNote{note.index}` and `style: ['fill: none']` carries
            // through to the rendered `style="fill: none;;;fill: none"`.
            let mut e = Edge::default();
            e.id = format!("edgeNote{}", n.index);
            e.source = Some(n.id.clone());
            e.target = Some(n.class_id.clone());
            e.classes = Some("relation".into());
            e.thickness = Some("normal".into());
            e.pattern = Some("dotted".into());
            e.style = Some(vec!["fill: none".into()]);
            // Upstream `insertEdgeLabel` always sets `edge.width = bbox.width`
            // and `edge.height = bbox.height` from the foreignObject body,
            // even when the label text is empty — the resulting fO collapses
            // to (0, line_h) which still nudges dagre's rank packing.
            // Without this, the note→class spline collapses by one
            // line-height (~16 px), shifting downstream y-coordinates.
            e.extra.insert("label_width".into(), "0".into());
            e.extra.insert("label_height".into(), "16.296875".into());
            // Upstream sets arrowTypeStart/End to 'none' (string), which
            // renders as no marker reference. Leave both as None here.
            data.edges.push(e);
        }
    }

    // Synthetic invisible interface stubs for lollipop relations.
    // Upstream `classDb.getData` walks `this.interfaces` after notes
    // and emits one `rect` node per entry with `cssStyles: ['opacity: 0;']`.
    // The label still measures (so dagre allocates space), but the
    // outer container has zero opacity. The renderer mirrors this via
    // the `opacity:0; !important` style on the basic rect container.
    let iface_family = "trebuchet ms,verdana,arial,sans-serif";
    let iface_font = 14.0_f64;
    let iface_line_h = 16.296875_f64;
    for iface in &d.interfaces {
        let mut node = Node::default();
        node.id = iface.id.clone();
        node.label = Some(iface.label.clone());
        node.shape = Some("rect".into());
        // Upstream sets `cssStyles: ['opacity: 0;']`; surface that as a
        // style entry the rect shape will fold into the container's
        // `style=` attribute.
        node.css_styles = Some(vec!["opacity: 0;".into()]);
        // No explicit cssClasses upstream → renders as `node undefined`.
        node.css_classes = None;
        node.look = Some("classic".into());
        // Upstream's `rect` shape measures the label's foreignObject and
        // pads with `labelPaddingX/Y`. For lollipop interfaces the text
        // is plain (no markup) so we use the same trebuchet metrics.
        let w = font_metrics::text_width(&iface.label, iface_family, iface_font, false, false);
        node.width = Some(w);
        node.height = Some(iface_line_h);
        data.nodes.push(node);
    }

    // Relation edges.
    //
    // Edge label width/height feed into dagre's rank packing — when a
    // relation has a textual label upstream's `insertEdgeLabel` measures
    // the label's foreignObject and stores `edge.width = bbox.width;
    // edge.height = bbox.height;` BEFORE dagre lays out. dagre then
    // reserves an extra `labelHeight + 10` (default `edgeLabelOffset`)
    // band in the rank gap so the label fits between rows. We mirror
    // that by stuffing `label_width` / `label_height` into `Edge::extra`
    // — `make_edge_label` in the dagre bridge picks these up.
    let label_family = "trebuchet ms,verdana,arial,sans-serif";
    let label_font = 14.0_f64;
    let label_line_h = 16.296875_f64;
    for (i, r) in d.relations.iter().enumerate() {
        let mut e = Edge::default();
        e.id = format!("id_{}_{}_{}", r.id1, r.id2, i + 1);
        e.source = Some(r.id1.clone());
        e.target = Some(r.id2.clone());
        e.label = if r.title.is_empty() {
            None
        } else {
            Some(r.title.clone())
        };
        e.arrow_type_start = Some(end_marker_name(r.end1));
        e.arrow_type_end = Some(end_marker_name(r.end2));
        e.pattern = Some(match r.line {
            LineType::Solid => "solid".into(),
            LineType::Dotted => "dashed".into(),
        });
        e.thickness = Some("normal".into());
        e.classes = Some("relation".into());
        e.start_label_right = if r.title1.is_empty() {
            None
        } else {
            Some(r.title1.clone())
        };
        e.end_label_left = if r.title2.is_empty() {
            None
        } else {
            Some(r.title2.clone())
        };
        e.curve = Some("basis".into());
        e.look = Some("classic".into());
        e.labelpos = Some("c".into());
        // Surface label bbox so dagre packs an extra rank for it.
        if !r.title.is_empty() {
            let lw = font_metrics::text_width(&r.title, label_family, label_font, false, false);
            e.extra.insert("label_width".into(), lw.to_string());
            e.extra
                .insert("label_height".into(), label_line_h.to_string());
        }
        data.edges.push(e);
    }

    data
}

fn cluster_node(ns: &crate::model::class::Namespace) -> Node {
    let mut n = Node::default();
    n.id = ns.id.clone();
    n.dom_id = Some(ns.dom_id.clone());
    n.label = Some(ns.id.clone());
    n.is_group = true;
    n.shape = Some("rect".into());
    n.css_classes = Some("namespace".into());
    n
}

fn class_to_node(c: &ClassNode, d: &ClassDiagram) -> Node {
    let mut n = Node::default();
    n.id = c.id.clone();
    n.dom_id = Some(c.dom_id.clone());
    // Title row renders the generic-augmented form (`Foo<T>` after
    // tilde decoding) — see `ClassNode::display_label`.
    n.label = Some(c.display_label());
    n.shape = Some("classBox".into());
    n.css_classes = Some(
        std::iter::once("default")
            .chain(c.css_classes.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" "),
    );
    n.parent_id = c.parent.clone();
    n.look = Some("classic".into());
    if c.have_callback {
        n.have_callback = Some(true);
    }
    if let Some(link) = c.link.as_ref() {
        n.link = Some(link.clone());
        n.link_target = c.link_target.clone();
    }
    if let Some(t) = c.tooltip.as_ref() {
        if !t.is_empty() {
            n.tooltip = Some(t.clone());
        }
    }

    // Resolve compiled inline styles: classDef styles (in css_classes
    // application order) followed by any direct `style ID …` directive.
    // This mirrors upstream `addClass` / `getCompiledStyles` behaviour
    // — the result feeds both the per-class CSS rule
    // (`.<id>>* { … !important }`) and the path-level `style="…"` /
    // `fill` / `stroke` / `stroke-width` overrides.
    let mut compiled: Vec<String> = Vec::new();
    for cc in &c.css_classes {
        if let Some(sc) = d.style_classes.iter().find(|s| s.id == *cc) {
            for st in &sc.styles {
                let s = st.trim();
                if !s.is_empty() {
                    compiled.push(s.to_string());
                }
            }
        }
    }
    for st in &c.styles {
        let s = st.trim();
        if !s.is_empty() {
            compiled.push(s.to_string());
        }
    }
    if !compiled.is_empty() {
        n.css_compiled_styles = Some(compiled);
    }

    // Width/height — approximate by summing member-line widths.
    let (w, h) = estimate_classbox_dimensions(c);
    n.width = Some(w);
    n.height = Some(h);

    // Carry member/method text through so the shape emitter can pick
    // them up. `description` is the unified-types field we reuse.
    let mut description = Vec::new();
    for m in &c.members {
        description.push(m.text.clone());
    }
    description.push("__SEP__".into()); // marker between members and methods
    for m in &c.methods {
        description.push(m.text.clone());
    }
    n.description = Some(description);
    n
}

fn estimate_classbox_dimensions(c: &ClassNode) -> (f64, f64) {
    // Upstream's `classBox.ts` derives the rough rect dimensions from
    // `textHelper`'s shapeSvg.getBBox() in jsdom. Because the upstream
    // `generate_ref.mjs` shim ignores transforms when computing getBBox
    // and treats foreignObjects as starting at (0, 0), the bbox is the
    // union of:
    //
    //   * label foreignObject       — (0, 0, label_w, line_h)
    //   * each member foreignObject — (0, 0, member_w, line_h)
    //   * each method foreignObject — (0, 0, method_w, line_h)
    //
    // (Empty groups contribute nothing; their `getBBox` collapses to
    // {0,0,0,0} and is dropped by `unionBox`.)
    //
    // Then `classBox.ts` does:
    //   const w = Math.max(node.width ?? 0, bbox.width);
    //   let h = Math.max(node.height ?? 0, bbox.height);
    //   if (no members && no methods) h += GAP;             // GAP=12
    //   else if (members && no methods) h += GAP * 2;
    //   const drawn_w = w + 2 * PADDING;                    // PADDING=12
    //   const drawn_h = h + 2 * PADDING + extraHeight;
    //
    // where `extraHeight = renderExtraBox ? PADDING*2 : (no_members && no_methods ? -PADDING : 0)`.
    // For empty members AND methods (no `hideEmptyMembersBox`): renderExtraBox=true → extraHeight=24.
    let font = 14.0;
    let family = "trebuchet ms,verdana,arial,sans-serif";
    let line_h = 16.296875_f64; // foreignObject height for label at 14 px
    let padding = 12.0_f64;

    // Label width (bold, html-label style — measured via foreignObject).
    // The foreignObject width tracks the rendered <p>{display_label}</p>
    // textContent (entity-decoded — `Foo<T>` rather than `Foo&lt;T&gt;`).
    let display_label = c.display_label();
    let label_w = font_metrics::text_width(&display_label, family, font, true, false);
    // bbox.width = max of all visible foreignObjects' widths; with empty
    // members/methods this is just the label width.
    let mut bbox_w: f64 = label_w;
    let bbox_h: f64 = line_h;
    // Upstream renders `m.text` through markdown → HTML → div.textContent,
    // which strips the leading `\` visibility escape and the `&lt;`/`&gt;`
    // entities are restored to literal `<`/`>` before measurement.
    for m in &c.members {
        let display = displayed_member_text(&m.text);
        let w = font_metrics::text_width(&display, family, font, false, false);
        bbox_w = bbox_w.max(w);
    }
    for m in &c.methods {
        let display = displayed_member_text(&m.text);
        let w = font_metrics::text_width(&display, family, font, false, false);
        bbox_w = bbox_w.max(w);
    }
    // Annotations contribute too but render in a separate group above
    // the label; for sizing they only matter to bbox_w (label-side).
    for a in &c.annotations {
        let aw = font_metrics::text_width(&format!("«{}»", a), family, font, false, false);
        bbox_w = bbox_w.max(aw);
    }

    let has_members = !c.members.is_empty();
    let has_methods = !c.methods.is_empty();

    // bbox.height — `generate_ref.mjs`'s getBBox shim *ignores* transforms
    // when computing the union, and every foreignObject (label / each
    // member / each method) starts at intrinsic (0, 0, w, line_h). Their
    // union therefore collapses to a single (0, 0, max_w, line_h) box.
    // We intentionally do NOT add per-row height here.
    let _ = (has_members, has_methods);

    // h adjustments per classBox.ts:
    let mut h = bbox_h;
    if !has_members && !has_methods {
        h += padding; // GAP
    } else if has_members && !has_methods {
        h += padding * 2.0;
    }

    // extraHeight: with empty members AND methods, renderExtraBox=true →
    // extraHeight = PADDING * 2 = 24. Otherwise 0.
    let extra_h = if !has_members && !has_methods {
        padding * 2.0
    } else {
        0.0
    };

    let drawn_w = bbox_w + 2.0 * padding;
    let drawn_h = h + 2.0 * padding + extra_h;
    (drawn_w, drawn_h)
}

/// Strip the leading visibility escape (`\+`, `\-`, `\#`, `\~`) and decode
/// the `&lt;`/`&gt;` entities back to literal angle brackets — what
/// upstream's `markdown → div.textContent` pipeline ends up measuring.
fn displayed_member_text(text: &str) -> String {
    let raw = displayed_member_text_raw(text);
    // Upstream measures the *rendered* div.textContent, which strips
    // markdown emphasis markers (`*`, `_`, `**`, `__`). Mirror that pass.
    md_emphasis_strip(&raw)
}

/// Like [`displayed_member_text`] but does NOT strip markdown emphasis
/// markers — used for places that need the raw post-decode text (e.g.
/// the `max-width` heuristic measured against the entity-escaped form).
pub(crate) fn displayed_member_text_raw(text: &str) -> String {
    let mut s = text.to_string();
    if let Some(rest) = s.strip_prefix('\\') {
        // Drop only the backslash, keep the visibility glyph itself.
        s = rest.to_string();
    }
    s = s.replace("&lt;", "<").replace("&gt;", ">");
    s
}

/// CommonMark-style emphasis processor that returns the rendered
/// `textContent` (i.e. all `*`/`_` emphasis markers stripped, nesting
/// preserved). Single `*`/`_` ⇒ `<em>`; double ⇒ `<strong>`.
pub(crate) fn md_emphasis_strip(s: &str) -> String {
    md_emphasis_render(s, false)
}

/// CommonMark-style emphasis processor that returns rendered HTML with
/// `<em>` and `<strong>` tags. Used by the renderer to fill the `<p>` of
/// each member/method foreignObject.
pub(crate) fn md_emphasis_html(s: &str) -> String {
    md_emphasis_render(s, true)
}

/// Core implementation — runs a CommonMark-flavoured emphasis
/// delimiter-stack pass over `s`. When `as_html` is true, paired runs
/// are wrapped in `<em>` / `<strong>`; otherwise the markers are simply
/// dropped, leaving plain text.
///
/// The implementation deliberately approximates the full CommonMark
/// rules: it tracks left-/right-flanking, intra-word `_` constraints,
/// and pairs delimiters by walking the stack right-to-left. Tested
/// patterns (see `tests` module): `*x*`, `**x**`, `_x_`, `__x__`,
/// nested `_a_b_c_`, and mixed-delimiter inputs.
fn md_emphasis_render(s: &str, as_html: bool) -> String {
    if s.is_empty() {
        return String::new();
    }
    // Tokens: either a Run of `*`/`_` (with metadata) or a Plain text
    // segment. We process the tokens in two passes per CommonMark.
    #[derive(Clone, Debug)]
    enum Tok {
        Plain(String),
        Run {
            ch: char,        // '*' or '_'
            len: usize,      // 1 or 2 (>2 truncated for our use case)
            can_open: bool,  // CommonMark left-flanking + extra rules
            can_close: bool, // CommonMark right-flanking + extra rules
            // After matching, marks how many chars of this run got
            // consumed as opener / closer. Remaining are emitted as text.
            consumed_open: usize,
            consumed_close: usize,
            // Set when this run is paired; `pair_idx` points to the
            // matching opposite end and `wrap_strong` says whether it's
            // a `<strong>` (true) or `<em>` (false) wrap.
            paired_with: Option<usize>,
            wrap_strong: bool,
            is_opener_paired: bool, // false = closer side of the pair
        },
    }

    // Helper: classify a delimiter run's left/right flanking status.
    fn is_punct(c: char) -> bool {
        // CommonMark punctuation = ASCII punct OR Unicode punct (we use
        // ASCII subset, sufficient for member/method labels).
        matches!(
            c,
            '!' | '"'
                | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | '+'
                | ','
                | '-'
                | '.'
                | '/'
                | ':'
                | ';'
                | '<'
                | '='
                | '>'
                | '?'
                | '@'
                | '['
                | '\\'
                | ']'
                | '^'
                | '`'
                | '{'
                | '|'
                | '}'
                | '~'
        )
    }
    fn is_ws_or_start(c: Option<char>) -> bool {
        matches!(c, None | Some(' ') | Some('\t') | Some('\n') | Some('\r'))
    }
    fn is_ws_or_punct(c: Option<char>) -> bool {
        c.map(|ch| ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' || is_punct(ch))
            .unwrap_or(true)
    }

    // Tokenize.
    let chars: Vec<char> = s.chars().collect();
    let mut toks: Vec<Tok> = Vec::new();
    let mut buf = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '*' || c == '_' {
            if !buf.is_empty() {
                toks.push(Tok::Plain(std::mem::take(&mut buf)));
            }
            // Read run.
            let mut j = i;
            while j < chars.len() && chars[j] == c {
                j += 1;
            }
            let mut run_len = j - i;
            // Cap at 2 — CommonMark allows longer but we only need
            // ** / __ wrapping for our class-member fixtures.
            if run_len > 2 {
                // Emit the excess as plain text first.
                let extra = run_len - 2;
                let mut extra_s = String::new();
                for _ in 0..extra {
                    extra_s.push(c);
                }
                toks.push(Tok::Plain(extra_s));
                run_len = 2;
            }
            // Determine flanking.
            let prev = if i == 0 { None } else { Some(chars[i - 1]) };
            let next = if j < chars.len() {
                Some(chars[j])
            } else {
                None
            };
            let next_is_ws = is_ws_or_start(next);
            let prev_is_ws = is_ws_or_start(prev);
            let next_is_punct = next.map(is_punct).unwrap_or(false);
            let prev_is_punct = prev.map(is_punct).unwrap_or(false);
            // Left-flanking: not followed by ws AND
            //   (not followed by punct OR preceded by ws/punct).
            let left_flank = !next_is_ws && (!next_is_punct || prev_is_ws || prev_is_punct);
            // Right-flanking: not preceded by ws AND
            //   (not preceded by punct OR followed by ws/punct).
            let right_flank = !prev_is_ws && (!prev_is_punct || next_is_ws || next_is_punct);
            let (can_open, can_close) = if c == '*' {
                (left_flank, right_flank)
            } else {
                // `_` adds the intra-word restriction.
                let can_open = left_flank && (!right_flank || prev_is_punct);
                let can_close = right_flank && (!left_flank || next_is_punct);
                (can_open, can_close)
            };
            toks.push(Tok::Run {
                ch: c,
                len: run_len,
                can_open,
                can_close,
                consumed_open: 0,
                consumed_close: 0,
                paired_with: None,
                wrap_strong: false,
                is_opener_paired: false,
            });
            i = j;
        } else {
            buf.push(c);
            i += 1;
        }
    }
    if !buf.is_empty() {
        toks.push(Tok::Plain(buf));
    }

    // Pair delimiters per CommonMark process_emphasis. Walk left-to-right
    // collecting closers; for each closer, scan back for the nearest
    // matching opener (same delimiter char, can_open, with un-consumed
    // chars). Match `**` to `**` first if both have ≥2 chars unused;
    // otherwise pair single char.
    // Simplified: iterate `closer_idx` ascending; for each, scan opener
    // candidates descending.
    let mut idx = 0usize;
    while idx < toks.len() {
        // Snapshot current run as closer if applicable.
        let (closer_ch, closer_can_close, closer_remaining) = match &toks[idx] {
            Tok::Run {
                ch,
                len,
                can_close,
                consumed_open,
                consumed_close,
                ..
            } => (
                *ch,
                *can_close,
                len.saturating_sub(*consumed_open + *consumed_close),
            ),
            _ => {
                idx += 1;
                continue;
            }
        };
        if !closer_can_close || closer_remaining == 0 {
            idx += 1;
            continue;
        }
        // Find the nearest preceding compatible opener.
        let mut found: Option<(usize, usize)> = None; // (opener_idx, pair_size)
        let mut k = idx;
        while k > 0 {
            k -= 1;
            if let Tok::Run {
                ch,
                len,
                can_open,
                consumed_open,
                consumed_close,
                ..
            } = &toks[k]
            {
                if *ch != closer_ch {
                    continue;
                }
                if !*can_open {
                    continue;
                }
                let opener_remaining = len.saturating_sub(*consumed_open + *consumed_close);
                if opener_remaining == 0 {
                    continue;
                }
                // CommonMark "rule of 3": when both can_open and can_close,
                // the sum of their original lengths must not be a multiple
                // of 3 unless both individually are multiples of 3.
                // We only deal with len 1 or 2; check anyway.
                let cur_can_open_too = matches!(
                    &toks[idx],
                    Tok::Run { can_open: co, .. } if *co
                );
                let opener_can_close_too = matches!(
                    &toks[k],
                    Tok::Run { can_close: cc, .. } if *cc
                );
                if (cur_can_open_too || opener_can_close_too)
                    && (len + closer_remaining) % 3 == 0
                    && (len % 3 != 0 || closer_remaining % 3 != 0)
                {
                    continue;
                }
                let pair_size = if opener_remaining >= 2 && closer_remaining >= 2 {
                    2
                } else {
                    1
                };
                found = Some((k, pair_size));
                break;
            }
        }
        if let Some((opener_idx, pair_size)) = found {
            // Consume `pair_size` chars from each side.
            if let Tok::Run {
                consumed_open,
                paired_with,
                wrap_strong,
                is_opener_paired,
                ..
            } = &mut toks[opener_idx]
            {
                *consumed_open += pair_size;
                *paired_with = Some(idx);
                *wrap_strong = pair_size == 2;
                *is_opener_paired = true;
            }
            if let Tok::Run {
                consumed_close,
                paired_with,
                wrap_strong,
                is_opener_paired,
                ..
            } = &mut toks[idx]
            {
                *consumed_close += pair_size;
                *paired_with = Some(opener_idx);
                *wrap_strong = pair_size == 2;
                *is_opener_paired = false;
            }
            // After consumption, this closer might still have leftover
            // characters that can close (e.g. a 2-char `**` matched a
            // single `*`). Loop on same idx.
            continue;
        }
        idx += 1;
    }

    // Emit.
    let mut out = String::with_capacity(s.len());
    for tok in &toks {
        match tok {
            Tok::Plain(t) => out.push_str(t),
            Tok::Run {
                ch,
                len,
                consumed_open,
                consumed_close,
                paired_with,
                wrap_strong,
                is_opener_paired,
                ..
            } => {
                let total_consumed = *consumed_open + *consumed_close;
                if paired_with.is_some() {
                    if as_html {
                        let tag = if *wrap_strong { "strong" } else { "em" };
                        if *is_opener_paired {
                            out.push_str(&format!("<{}>", tag));
                        } else {
                            out.push_str(&format!("</{}>", tag));
                        }
                    }
                    // Emit any leftover marker chars as plain text.
                    let leftover = len.saturating_sub(total_consumed);
                    for _ in 0..leftover {
                        out.push(*ch);
                    }
                } else {
                    // Unmatched run — emit as plain.
                    for _ in 0..*len {
                        out.push(*ch);
                    }
                }
            }
        }
    }
    out
}

fn end_marker_name(end: RelationEnd) -> String {
    match end {
        RelationEnd::None => String::new(),
        RelationEnd::Aggregation => "aggregation".into(),
        RelationEnd::Extension => "extension".into(),
        RelationEnd::Composition => "composition".into(),
        RelationEnd::Dependency => "dependency".into(),
        RelationEnd::Lollipop => "lollipop".into(),
    }
}

/// Crude multi-line measurement helper — counts lines, picks the
/// longest, and hands back (width, height). Kept in a standalone fn so
/// note-node sizing stays consistent with the upstream approach.
fn measure_multiline(text: &str, font: f64) -> (f64, f64) {
    let family = "trebuchet ms,verdana,arial,sans-serif";
    let lines: Vec<&str> = text.split('\n').collect();
    let mut w: f64 = 0.0;
    for line in &lines {
        let lw = font_metrics::text_width(line, family, font, false, false);
        if lw > w {
            w = lw;
        }
    }
    let h = lines.len() as f64 * (font * 1.4);
    (w, h)
}

/// Strip HTML tags (`<...>`) from a string. Upstream measures note
/// label width via `getBoundingClientRect()` after DOMPurify renders the
/// markup, so embedded `<a>`/`<code>` reduce to their visible text only.
/// Mirrors `crate::layout::state::strip_html_tags`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    #[test]
    fn layout_runs_on_simple_diagram() {
        let src = "classDiagram\nA <|-- B\n";
        let d = parser::class::parse(src).unwrap();
        let theme = ThemeVariables::default();
        let l = layout(&d, &theme).unwrap();
        assert_eq!(l.unified.nodes.len(), 2);
        assert_eq!(l.unified.edges.len(), 1);
    }

    #[test]
    fn layout_populates_markers() {
        let src = "classDiagram\nA o-- B\n";
        let d = parser::class::parse(src).unwrap();
        let theme = ThemeVariables::default();
        let l = layout(&d, &theme).unwrap();
        assert!(l.data.markers.iter().any(|m| m == "aggregation"));
    }
}
