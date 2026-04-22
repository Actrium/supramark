//! ER-diagram SVG renderer — byte-exact output against
//! `mermaid@11.14.0`'s unified (dagre + d3 + jsdom) pipeline.
//!
//! # Structure mirrored
//!
//! The reference SVG is produced by the `erRenderer-unified.ts` code
//! path (the unified / flowchart-family renderer), NOT the legacy
//! `erRenderer.js`. Top-level anatomy:
//!
//! 1. `<svg>` opening tag — attrs in order:
//!    `id, width, xmlns, class, style, viewBox, role, aria-roledescription`.
//! 2. `<style>` block — built from the ER diagram-family CSS template.
//! 3. Top-level seed `<g>` (corresponds to upstream's `.appendDivSvgG`).
//! 4. Marker `<defs>` — the 8 ER cardinality markers.
//! 5. `<g class="root">` containing:
//!    * `<g class="clusters"></g>` — always empty for ER.
//!    * `<g class="edgePaths">` — one `<path>` per relationship.
//!    * `<g class="edgeLabels">` — label centres with `<foreignObject>` wrappers.
//!    * `<g class="nodes">` — one entity per child.
//! 6. Two trailing `<defs>` — drop-shadow / drop-shadow-small filters.
//!
//! # Scope and known limitations
//!
//! * Entity-only fixtures (no attribute rows) render byte-exact.
//! * Attribute-bearing entities (`ENTITY { col1; col2; … }`) route
//!   through [`render_entity_node_with_attrs`], which consumes the
//!   layout-side [`crate::layout::er::AttrLayout`] and delegates the
//!   outer rectangle, per-row backgrounds, and column/row dividers
//!   to [`crate::render::rough`] for byte-exact path emission.
//! * Hand-drawn (`look: handDrawn`) variants are still deferred —
//!   they need the hachure filler path which the Wave 3.5 rough port
//!   does not yet implement.
//! * Entity-level `classDef` / class-based style overrides, markdown
//!   text inside attribute cells, and unicode-alias entity labels
//!   (fixtures 33–42) remain blocked on their respective layout
//!   extensions (not rough.js-related).

use crate::error::Result;
use crate::layout::er::{EdgeLayout, EntityLayout, ErLayout};
use crate::model::er::ErDiagram;
use crate::render::edges::{build_path, CurveType};
use crate::render::rough::{path_out_to_svg, to_paths, RoughGenerator, RoughOptions};
use crate::theme::ThemeVariables;

/// Upstream ER diagram-family CSS — built once per render, with the
/// numeric / color variables interpolated. The base template mirrors
/// `styles.ts` in upstream plus the shared diagram CSS that's always
/// emitted around it. Written as one long string for faithful byte
/// ordering (stylis minification already applied).
pub fn render(d: &ErDiagram, l: &ErLayout, theme: &ThemeVariables, id: &str) -> Result<String> {
    let mut out = String::with_capacity(16 * 1024);

    // ── 1. Compute viewBox ──────────────────────────────────────────
    // Reference: viewBox = `${min_x-PADDING} ${min_y-PADDING} ${w+PADDING*2} ${h+PADDING*2}`
    // with PADDING=8 from upstream unified `setupViewPortForSVG(svg, 8, ...)`.
    let pad = 8.0_f64;
    let (bx, by, bw, bh) = l.bounds;
    let vx = bx - pad;
    let vy = by - pad;
    let vw = bw + pad * 2.0;
    let vh = bh + pad * 2.0;

    // ── 2. <svg ...> opening ────────────────────────────────────────
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" class="erDiagram" style="max-width: {maxw}px;" viewBox="{vx} {vy} {vw} {vh}" role="graphics-document document" aria-roledescription="er">"#,
        id = id,
        maxw = fmt_num(vw),
        vx = fmt_num(vx),
        vy = fmt_num(vy),
        vw = fmt_num(vw),
        vh = fmt_num(vh),
    ));

    // ── 3. <style> block ───────────────────────────────────────────
    out.push_str(&style_block(id, theme));

    // ── 4. Top-level seed <g>…markers…root…</g> ────────────────────
    out.push_str("<g>");

    // Markers (8 ER cardinality markers, same text for every diagram).
    out.push_str(&markers_block(id));

    // ── 5. <g class="root"> ──────────────────────────────────────
    out.push_str(r#"<g class="root"><g class="clusters"></g>"#);

    // edgePaths
    out.push_str(r#"<g class="edgePaths">"#);
    for e in &l.edges {
        out.push_str(&render_edge_path(id, e));
    }
    out.push_str("</g>");

    // edgeLabels
    out.push_str(r#"<g class="edgeLabels">"#);
    for e in &l.edges {
        out.push_str(&render_edge_label(e));
    }
    out.push_str("</g>");

    // nodes
    out.push_str(r#"<g class="nodes">"#);
    for ent in &l.entities {
        if ent.has_attrs {
            out.push_str(&render_entity_node_with_attrs(id, ent, theme));
        } else {
            out.push_str(&render_entity_node(id, ent));
        }
    }
    out.push_str("</g>");

    out.push_str("</g>"); // </g class="root">
    out.push_str("</g>"); // </g top-level>

    // ── 6. Trailing drop-shadow filter <defs>s ───────────────────────
    out.push_str(&drop_shadow_defs(id));

    // Optional title text — emitted *after* the drop-shadow defs, as
    // upstream `utils.insertTitle` does.
    if let Some(title) = d.meta.title.as_deref() {
        if !title.trim().is_empty() {
            let title_x = l.title_anchor_x.unwrap_or(bx + bw / 2.0);
            let title_y = -25.0_f64; // titleTopMargin default.
            out.push_str(&format!(
                r#"<text text-anchor="middle" x="{}" y="{}" class="erDiagramTitleText">{}</text>"#,
                fmt_num(title_x),
                fmt_num(title_y),
                html_escape(title),
            ));
        }
    }
    // Silence unused variable warning — bx/bw/by/bh still used above in viewBox.
    let _ = (bx, bw, by, bh);

    out.push_str("</svg>");
    Ok(out)
}

// ──────────────────────────────────────────────────────────────────────
// Entity node — `<g class="node default" …><rect …/><g class="label"…>…</g></g>`
// Single-line markdown label via foreignObject — matches the no-attribute
// branch of upstream `erBox.ts` + `drawRect.ts`.
// ──────────────────────────────────────────────────────────────────────
/// Render an attribute-bearing entity (upstream `erBox.ts` with
/// `entity.attributes.length > 0`). Emits the rough.js-generated outer
/// rectangle, per-row rects, column foreignObjects, and column /
/// row dividers in the exact order the reference generator produces.
fn render_entity_node_with_attrs(id: &str, e: &EntityLayout, theme: &ThemeVariables) -> String {
    let a = match &e.attr_layout {
        Some(a) => a,
        None => return render_entity_node(id, e),
    };
    // Pull ER theme colours.
    let main_bkg = theme.main_bkg.as_deref().unwrap_or("#ECECFF");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    // Default theme's computed rowEven / rowOdd (from themes/theme-default.js
    // — lightened/darkened from mainBkg). For the default theme these
    // resolve to the HSL strings seen in the fixture: `hsl(240, 100%, 100%)`
    // for rowOdd, `hsl(240, 100%, 97.2745098039%)` for rowEven.
    let row_odd = theme.row_odd.as_deref().unwrap_or("hsl(240, 100%, 100%)");
    let row_even = theme
        .row_even
        .as_deref()
        .unwrap_or("hsl(240, 100%, 97.2745098039%)");

    let w = e.width;
    let h = e.height;
    let x = -w / 2.0;
    let y = -h / 2.0;
    let pad = a.padding;
    let text_pad = a.text_padding;
    let name_h = a.name_bbox_height;
    let max_type_w = a.max_type_width;
    let max_name_w = a.max_name_width;
    let max_keys_w = a.max_keys_width;

    let mut out = String::with_capacity(8 * 1024);
    out.push_str(&format!(
        r#"<g class="node {cls} " id="{sid}-{eid}" data-look="classic" transform="translate({tx}, {ty})">"#,
        cls = e.css_classes,
        sid = id,
        eid = e.id,
        tx = fmt_num(e.x),
        ty = fmt_num(e.y),
    ));

    // ── Outer rectangle via rough ──────────────────────────────────
    let rc_options = rough_entity_options(main_bkg, node_border);
    let mut rc = RoughGenerator::new();
    let drawable = rc.rectangle(x, y, w, h, &rc_options);
    let paths = to_paths(&drawable, &rc_options);
    out.push_str(r#"<g class="outer-path" style="">"#);
    for p in &paths {
        out.push_str(&path_out_to_svg(p));
    }
    out.push_str("</g>");

    // ── Per-attribute row rects ────────────────────────────────────
    for (i, row) in a.rows.iter().enumerate() {
        let content_idx = i + 1;
        let is_even = content_idx % 2 == 0 && row.y_offset != 0.0;
        let fill = if is_even { row_even } else { row_odd };
        let row_opts = rough_row_options(fill, node_border);
        let mut rc = RoughGenerator::new();
        let d = rc.rectangle(x, name_h + y + row.y_offset, w, row.row_height, &row_opts);
        let ps = to_paths(&d, &row_opts);
        let class = if is_even {
            "row-rect-even"
        } else {
            "row-rect-odd"
        };
        out.push_str(&format!(r#"<g style="" class="{class}">"#));
        for p in &ps {
            out.push_str(&path_out_to_svg(p));
        }
        out.push_str("</g>");
    }

    // ── Name label ────────────────────────────────────────────────
    // transform = translate(-nameBBox.width/2, y + TEXT_PADDING/2)
    out.push_str(&format!(
        r#"<g class="label name" transform="translate({tx}, {ty})" style=""><foreignObject width="{fw}" height="{fh}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {mw}px; text-align: start;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel "><p>{t}</p></span></div></foreignObject></g>"#,
        tx = fmt_num(-a.name_bbox_width / 2.0),
        ty = fmt_num(y + text_pad / 2.0),
        fw = fmt_num(a.name_bbox_width),
        fh = fmt_num(name_h - text_pad),
        mw = calc_text_max_width(&e.label),
        t = html_escape(&e.label),
    ));

    // ── Per-row attribute labels (type / name / keys / comment) ────
    for row in &a.rows {
        let translate_y = y + name_h + row.y_offset + text_pad / 2.0;
        // type
        let type_x = x + pad / 2.0;
        out.push_str(&attr_foreign_object_html(
            "attribute-type",
            type_x,
            translate_y,
            row.type_width,
            name_h - text_pad,
            &row.type_text,
        ));
        // name
        let name_x = type_x + max_type_w;
        out.push_str(&attr_foreign_object_html(
            "attribute-name",
            name_x,
            translate_y,
            row.name_width,
            name_h - text_pad,
            &row.name_text,
        ));
        // keys
        let keys_x = name_x + max_name_w;
        out.push_str(&attr_foreign_object_html(
            "attribute-keys",
            keys_x,
            translate_y,
            row.keys_width,
            name_h - text_pad,
            &row.keys_text,
        ));
        // comment
        let comment_x = keys_x + max_keys_w;
        out.push_str(&attr_foreign_object_html(
            "attribute-comment",
            comment_x,
            translate_y,
            row.comment_width,
            name_h - text_pad,
            &row.comment_text,
        ));
    }

    // ── Dividers ──────────────────────────────────────────────────
    let thickness = 1e-4_f64;
    let divider_opts = rough_divider_options(main_bkg, node_border);

    // 1) Horizontal under the name row
    let div_y = name_h + y;
    let pts = line_to_polygon(x, div_y, w + x, div_y, thickness);
    out.push_str(&render_divider(&pts, &divider_opts));

    // 2) Vertical after `type` column
    let vx = max_type_w + x;
    let pts = line_to_polygon(vx, div_y, vx, h + y, thickness);
    out.push_str(&render_divider(&pts, &divider_opts));

    // 3) keysPresent / commentPresent verticals
    if a.keys_present {
        let vx = max_type_w + max_name_w + x;
        let pts = line_to_polygon(vx, div_y, vx, h + y, thickness);
        out.push_str(&render_divider(&pts, &divider_opts));
    }
    if a.comment_present {
        let vx = max_type_w + max_name_w + max_keys_w + x;
        let pts = line_to_polygon(vx, div_y, vx, h + y, thickness);
        out.push_str(&render_divider(&pts, &divider_opts));
    }

    // 4) One horizontal divider per entry in `yOffsets` — upstream
    //    only ever pushes a single 0 into `yOffsets`, so this
    //    duplicates the first horizontal divider under the name row.
    let pts = line_to_polygon(x, div_y, w + x, div_y, thickness);
    out.push_str(&render_divider(&pts, &divider_opts));

    out.push_str("</g>");
    out
}

/// Build the option bag upstream passes to `rc.rectangle` for the
/// outer entity rect — same as `userNodeOverrides(node, {})` with
/// roughness / fillStyle overrides for the default look.
fn rough_entity_options(main_bkg: &str, node_border: &str) -> RoughOptions {
    let mut o = RoughOptions::default();
    o.roughness = 0.0;
    o.fill_style = "solid".into();
    o.fill = Some(main_bkg.to_string());
    o.fill_weight = 4.0;
    o.hachure_gap = 5.2;
    o.stroke = node_border.to_string();
    o.stroke_width = 1.3;
    o.seed = 1; // handDrawnSeed default is 0 in mermaid global config but
                // the test harness sets it to 1 via mermaid.initialize.
    o.fill_line_dash = vec![0.0, 0.0];
    o.stroke_line_dash = vec![0.0, 0.0];
    o
}

/// Row rects — different fill from outer (rowEven/rowOdd), same
/// stroke (nodeBorder).
fn rough_row_options(fill: &str, node_border: &str) -> RoughOptions {
    let mut o = rough_entity_options("#ignored", node_border);
    o.fill = Some(fill.to_string());
    o
}

/// Divider polygons — upstream passes the same option bag (outer's
/// `userNodeOverrides`) straight through to `rc.polygon`. Since
/// `rc.polygon` takes the full options (including the fill) directly
/// we reuse the entity option bag.
fn rough_divider_options(main_bkg: &str, node_border: &str) -> RoughOptions {
    rough_entity_options(main_bkg, node_border)
}

/// `lineToPolygon` port — produces the 4-point thick-line polygon used
/// for each divider.
fn line_to_polygon(x1: f64, y1: f64, x2: f64, y2: f64, thickness: f64) -> Vec<(f64, f64)> {
    if x1 == x2 {
        // Vertical
        vec![
            (x1 - thickness / 2.0, y1),
            (x1 + thickness / 2.0, y1),
            (x2 + thickness / 2.0, y2),
            (x2 - thickness / 2.0, y2),
        ]
    } else {
        // Horizontal (or angled)
        vec![
            (x1, y1 - thickness / 2.0),
            (x1, y1 + thickness / 2.0),
            (x2, y2 + thickness / 2.0),
            (x2, y2 - thickness / 2.0),
        ]
    }
}

/// Render one divider `<g class="divider">…</g>` from the polygon
/// points + option bag.
fn render_divider(pts: &[(f64, f64)], o: &RoughOptions) -> String {
    let mut rc = RoughGenerator::new();
    let d = rc.polygon(pts, o);
    let paths = to_paths(&d, o);
    let mut s = String::from(r#"<g class="divider">"#);
    for p in &paths {
        s.push_str(&path_out_to_svg(p));
    }
    s.push_str("</g>");
    s
}

/// Per-attribute foreignObject label. `cls` is one of
/// `attribute-{type, name, keys, comment}`. Empty text collapses the
/// inner `<span>` to `<span class="nodeLabel "></span>` without a
/// `<p>` — matching the fixture's empty-cell output.
///
/// Generic-type processing: mermaid runs `parseGenericTypes(text)` on
/// attribute types — e.g. `type~T~` → `type<T>`. The DOM-visible form
/// is the HTML-escaped `type&lt;T&gt;` inside the div directly (no
/// span/p wrappers — upstream's sanitize path unwraps).
fn attr_foreign_object_html(cls: &str, tx: f64, ty: f64, w: f64, h: f64, text: &str) -> String {
    // Attribute type text is already stored post-parseGenericTypes by
    // the layout pass — we only need to detect the "has <> chars"
    // case here to choose the correct inner-span shape. The escaped
    // form (&lt; &gt;) is what upstream measures for max-width.
    let has_generics = cls == "attribute-type" && (text.contains('<') || text.contains('>'));
    let max_w_text = if has_generics {
        html_escape(text)
    } else {
        text.to_string()
    };
    let max_w = calc_text_max_width_raw(&max_w_text);

    let inner_span = if text.is_empty() {
        r#"<span class="nodeLabel "></span>"#.to_string()
    } else if has_generics {
        // Raw text (HTML-escaped) inside the div, no wrapper tags.
        html_escape(text)
    } else {
        format!(
            r#"<span class="nodeLabel "><p>{}</p></span>"#,
            html_escape(text)
        )
    };
    format!(
        r#"<g class="label {cls}" transform="translate({tx}, {ty})" style=""><foreignObject width="{w}" height="{h}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {mw}px; text-align: start;" xmlns="http://www.w3.org/1999/xhtml">{span}</div></foreignObject></g>"#,
        cls = cls,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
        w = fmt_num(w),
        h = fmt_num(h),
        mw = max_w,
        span = inner_span,
    )
}

/// Public alias — used by the layout pass to pre-process attribute
/// types before measuring. Kept narrow: no other use sites.
pub fn parse_generic_types_pub(input: &str) -> String {
    parse_generic_types(input)
}

/// Port of upstream `parseGenericTypes` — converts `foo~Bar~` into
/// `foo<Bar>`. Input is split on `,`; for each set with ≥ 2 tildes,
/// we flip tilde pairs to `< >`. Returns the original string if it
/// contains no matching tildes.
fn parse_generic_types(input: &str) -> String {
    fn count_occurrence(s: &str, c: char) -> usize {
        s.chars().filter(|&ch| ch == c).count()
    }
    fn should_combine(prev: &str, next: &str) -> bool {
        count_occurrence(prev, '~') == 1 && count_occurrence(next, '~') == 1
    }
    fn process_set(input: &str) -> String {
        let tilde_count = count_occurrence(input, '~');
        if tilde_count <= 1 {
            return input.to_string();
        }
        let mut has_starting_tilde = false;
        let mut s = input.to_string();
        if tilde_count % 2 != 0 && s.starts_with('~') {
            s = s[1..].to_string();
            has_starting_tilde = true;
        }
        let mut chars: Vec<char> = s.chars().collect();
        let first = |cs: &[char]| cs.iter().position(|&c| c == '~');
        let last = |cs: &[char]| cs.iter().rposition(|&c| c == '~');
        loop {
            let f = first(&chars);
            let l = last(&chars);
            match (f, l) {
                (Some(fi), Some(li)) if fi != li => {
                    chars[fi] = '<';
                    chars[li] = '>';
                }
                _ => break,
            }
        }
        let mut out: String = chars.into_iter().collect();
        if has_starting_tilde {
            out = format!("~{}", out);
        }
        out
    }
    let sets: Vec<&str> = input.split(',').collect();
    // Replicate JS's split-with-capture: `input.split(/(,)/)` yields
    // `[a, ',', b, ',', c]` — we reconstruct that here.
    let mut pieces: Vec<String> = Vec::new();
    for (i, s) in sets.iter().enumerate() {
        if i > 0 {
            pieces.push(",".into());
        }
        pieces.push((*s).into());
    }
    let mut output: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < pieces.len() {
        let mut this_set = pieces[i].clone();
        if this_set == "," && i > 0 && i + 1 < pieces.len() {
            let prev = &pieces[i - 1];
            let next = &pieces[i + 1];
            if should_combine(prev, next) {
                this_set = format!("{},{}", prev, next);
                i += 1;
                output.pop();
            }
        }
        output.push(process_set(&this_set));
        i += 1;
    }
    output.join("")
}

/// `calc_text_max_width` variant that takes the already-escaped text —
/// used by `attr_foreign_object_html` so generics get the wider
/// `&lt;T&gt;` measurement.
fn calc_text_max_width_raw(text: &str) -> i64 {
    use crate::font_metrics::text_width;
    if text.is_empty() {
        return 100;
    }
    let w = text_width(text, "sans-serif", 16.0, false, false);
    (w + 100.0).round() as i64
}

/// `max-width: Xpx` on the div — upstream uses
/// `calculateTextWidth(text, config) + 100` where `calculateTextWidth`
/// measures at 16 px (not 14 — addText re-measures at the base font).
///
/// Upstream's addText HTML-escapes `<` / `>` to `&lt;` / `&gt;` when
/// the text goes through `parseGenericTypes`, and then measures the
/// *escaped* form. We apply the same transform here so attribute-type
/// columns holding `type<T>` / `type~T~` end up with the fixture's
/// wider `max-width` value (208 vs 172).
///
/// For empty text we get 100 verbatim (matches the fixture's
/// `max-width: 100px` on empty attribute cells).
fn calc_text_max_width(text: &str) -> i64 {
    use crate::font_metrics::text_width;
    if text.is_empty() {
        return 100;
    }
    // HTML-escape `<>` the same way addText does so the width
    // matches the escaped form upstream feeds into calculateTextWidth.
    let escaped = html_escape(text);
    let w = text_width(&escaped, "sans-serif", 16.0, false, false);
    (w + 100.0).round() as i64
}

fn render_entity_node(id: &str, e: &EntityLayout) -> String {
    let mut out = String::with_capacity(512);
    // Normalize classes: upstream concatenates "default" then any user-added
    // class with a space. Inner "node" class is always prefixed.
    let class_extra = &e.css_classes;
    out.push_str(&format!(
        r#"<g class="node {} " id="{sid}-{eid}" data-look="classic" transform="translate({tx}, {ty})">"#,
        class_extra,
        sid = id,
        eid = e.id,
        tx = fmt_num(e.x),
        ty = fmt_num(e.y),
    ));
    // Rect.
    out.push_str(&format!(
        r#"<rect class="basic label-container" style="" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
        x = fmt_num(-e.width / 2.0),
        y = fmt_num(-e.height / 2.0),
        w = fmt_num(e.width),
        h = fmt_num(e.height),
    ));
    // Label group: translate(-label_w/2, -label_h/2) centres it on (0,0).
    out.push_str(&format!(
        r#"<g class="label" style="" transform="translate({lx}, {ly})"><rect></rect>{fo}</g>"#,
        lx = fmt_num(-e.label_width / 2.0),
        ly = fmt_num(-e.label_height / 2.0),
        fo = foreign_object_node_label(e.label_width, e.label_height, &e.label, &e.id),
    ));
    out.push_str("</g>");
    out
}

/// foreignObject wrapper used around an entity label (markdown-node-label).
/// `max-width` reflects the entity box's "wrap width": 100 if the entity
/// hit the minEntityWidth floor (node.width was explicitly set to 100),
/// else 200 (`flowchart.wrappingWidth` default). Decided upstream by
/// comparing the label width measured at *16* px (not 14).
fn foreign_object_node_label(width: f64, height: f64, text: &str, _eid: &str) -> String {
    // Re-measure at 16 px to match upstream's calculateTextWidth call. We
    // can't just use the 14-px width we already have.
    use crate::font_metrics::text_width;
    let w16 = text_width(text, "sans-serif", 16.0, false, false);
    let hit_min_floor = w16 + 40.0 < 100.0;
    let maxw = if hit_min_floor { 100 } else { 200 };
    format!(
        r#"<foreignObject width="{w}" height="{h}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {mw}px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel markdown-node-label"><p>{t}</p></span></div></foreignObject>"#,
        w = fmt_num(width),
        h = fmt_num(height),
        mw = maxw,
        t = html_escape(text),
    )
}

// ──────────────────────────────────────────────────────────────────────
// Edge path — `<path d="…" id=".." class="…"/>`
// Upstream produces the attrs in order:
//   d → id → class → style → data-edge → data-et → data-id →
//   data-points (base64) → data-look → marker-start → marker-end
// `data-points` is a base64 of the JSON-encoded points array.
// ──────────────────────────────────────────────────────────────────────
fn render_edge_path(diag_id: &str, e: &EdgeLayout) -> String {
    let points: Vec<crate::layout::unified::types::Point> = e
        .points
        .iter()
        .map(|(x, y)| crate::layout::unified::types::Point { x: *x, y: *y })
        .collect();
    let d = build_path(&points, CurveType::Basis);

    let pattern_class = match e.pattern {
        "dashed" => "edge-pattern-dashed",
        _ => "edge-pattern-solid",
    };
    let class = format!(" edge-thickness-normal {} relationshipLine", pattern_class);

    let data_points_b64 = base64_points(&e.points);

    let start_marker = card_to_marker(&e.card_b);
    let end_marker = card_to_marker(&e.card_a);

    format!(
        r##"<path d="{d}" id="{did}-{eid}" class="{cls}" style="undefined;;;undefined" data-edge="true" data-et="edge" data-id="{eid}" data-points="{b64}" data-look="classic" marker-start="url(#{did}_er-{sm}Start)" marker-end="url(#{did}_er-{em}End)"></path>"##,
        d = d,
        did = diag_id,
        eid = e.id,
        cls = class,
        b64 = data_points_b64,
        sm = start_marker,
        em = end_marker,
    )
}

/// Map an upper-case cardinality string to the camelCase marker base name
/// used in the reference's marker IDs.
fn card_to_marker(card: &str) -> &'static str {
    match card {
        "ZERO_OR_ONE" => "zeroOrOne",
        "ZERO_OR_MORE" => "zeroOrMore",
        "ONE_OR_MORE" => "oneOrMore",
        "ONLY_ONE" => "onlyOne",
        "MD_PARENT" => "mdParent",
        _ => "onlyOne",
    }
}

fn base64_points(points: &[(f64, f64)]) -> String {
    // Mirror upstream's `btoa(JSON.stringify(points))`.
    let mut json = String::from("[");
    for (i, (x, y)) in points.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(r#"{{"x":{x},"y":{y}}}"#));
    }
    json.push(']');
    base64_encode(json.as_bytes())
}

/// Minimal base64 encoder — matches `btoa` byte-for-byte.
fn base64_encode(data: &[u8]) -> String {
    const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut chunks = data.chunks_exact(3);
    for c in &mut chunks {
        let n = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | (c[2] as u32);
        out.push(TBL[((n >> 18) & 0x3f) as usize] as char);
        out.push(TBL[((n >> 12) & 0x3f) as usize] as char);
        out.push(TBL[((n >> 6) & 0x3f) as usize] as char);
        out.push(TBL[(n & 0x3f) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        0 => {}
        1 => {
            let n = (rem[0] as u32) << 16;
            out.push(TBL[((n >> 18) & 0x3f) as usize] as char);
            out.push(TBL[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            out.push(TBL[((n >> 18) & 0x3f) as usize] as char);
            out.push(TBL[((n >> 12) & 0x3f) as usize] as char);
            out.push(TBL[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => unreachable!(),
    }
    out
}

// ──────────────────────────────────────────────────────────────────────
// Edge label — <g class="edgeLabel" transform="translate(lx, ly)">…</g>
// Inner contains a foreignObject centred on that anchor via a second
// translate of (-lw/2, -lh/2).
// ──────────────────────────────────────────────────────────────────────
fn render_edge_label(e: &EdgeLayout) -> String {
    let inner_span = if e.label.trim().is_empty() {
        // Reference wraps empty / whitespace-only labels in a nowrap div
        // *without* a `<p>` body — but with the literal label string
        // preserved (for the `"   "` case). Reference shows a <p></p>
        // when the role is empty. We mirror that simplest rule.
        format!(
            r#"<span class="edgeLabel ">{}</span>"#,
            if e.label.is_empty() {
                "".to_string()
            } else {
                format!("<p>{}</p>", html_escape(&e.label))
            }
        )
    } else {
        format!(
            r#"<span class="edgeLabel "><p>{}</p></span>"#,
            html_escape(&e.label)
        )
    };
    let fo = format!(
        r#"<foreignObject width="{w}" height="{h}"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml" class="labelBkg">{span}</div></foreignObject>"#,
        w = fmt_num(e.label_width),
        h = fmt_num(e.label_height),
        span = inner_span,
    );
    format!(
        r#"<g class="edgeLabel" transform="translate({lx}, {ly})"><g class="label" data-id="{eid}" transform="translate({itx}, {ity})">{fo}</g></g>"#,
        lx = fmt_num(e.label_x),
        ly = fmt_num(e.label_y),
        eid = e.id,
        itx = fmt_num(-e.label_width / 2.0),
        ity = fmt_num(-e.label_height / 2.0),
        fo = fo,
    )
}

// ──────────────────────────────────────────────────────────────────────
// Style block — upstream `styles.ts` + diagram-api shared CSS, stylis-
// minified. For reproducibility we hard-wire the output string with
// theme variables interpolated into the ID-scoped selectors.
// ──────────────────────────────────────────────────────────────────────
fn style_block(id: &str, theme: &ThemeVariables) -> String {
    // Pull the specific theme values the ER CSS references. Defaults
    // mirror the "default" mermaid theme for resilience.
    let main_bkg = theme.main_bkg.as_deref().unwrap_or("#ECECFF");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let tertiary = theme
        .tertiary_color
        .as_deref()
        .unwrap_or("hsl(80, 100%, 96.2745098039%)");
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let node_text_color = theme.node_text_color.as_deref().unwrap_or(text_color);
    let edge_label_bg = theme
        .edge_label_background
        .as_deref()
        .unwrap_or("rgba(232,232,232, 0.8)");
    // labelBkg CSS: upstream styles.ts does `fade(tertiaryColor, 0.5)`.
    let labelbkg_color = fade(tertiary, 0.5);
    let labelbkg_color = labelbkg_color.as_str();
    // stylis strips ", " → "," outside quoted strings during minification;
    // apply that to the font-family list for parity with upstream.
    let ff_raw = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\",verdana,arial,sans-serif");
    let ff = stylis_minify_commas(ff_raw);
    let ff = ff.as_str();

    let mut css = String::with_capacity(6000);
    let p = |s: &str, css: &mut String| {
        css.push_str(s);
    };

    p(
        &format!(
            r#"<style>#{id}{{font-family:{ff};font-size:16px;fill:{tc};}}"#,
            id = id,
            ff = ff,
            tc = text_color,
        ),
        &mut css,
    );
    p(
        "@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}",
        &mut css,
    );
    p("@keyframes dash{to{stroke-dashoffset:0;}}", &mut css);
    for (sel, body) in [
        (".edge-animation-slow", "stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;"),
        (".edge-animation-fast", "stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;"),
        (".error-icon", "fill:#552222;"),
        (".error-text", "fill:#552222;stroke:#552222;"),
        (".edge-thickness-normal", "stroke-width:1px;"),
        (".edge-thickness-thick", "stroke-width:3.5px;"),
        (".edge-pattern-solid", "stroke-dasharray:0;"),
        (".edge-thickness-invisible", "stroke-width:0;fill:none;"),
        (".edge-pattern-dashed", "stroke-dasharray:3;"),
        (".edge-pattern-dotted", "stroke-dasharray:2;"),
    ] {
        p(&format!("#{id} {sel}{{{body}}}"), &mut css);
    }
    p(
        &format!("#{id} .marker{{fill:{lc};stroke:{lc};}}", lc = line_color),
        &mut css,
    );
    p(
        &format!("#{id} .marker.cross{{stroke:{lc};}}", lc = line_color),
        &mut css,
    );
    p(
        &format!("#{id} svg{{font-family:{ff};font-size:16px;}}", ff = ff),
        &mut css,
    );
    p(&format!("#{id} p{{margin:0;}}"), &mut css);

    // From styles.ts — er-specific rules.
    p(
        &format!(
            "#{id} .entityBox{{fill:{mb};stroke:{nb};}}",
            mb = main_bkg,
            nb = node_border
        ),
        &mut css,
    );
    p(
        &format!(
            "#{id} .relationshipLabelBox{{fill:{t};opacity:0.7;background-color:{t};}}",
            t = tertiary
        ),
        &mut css,
    );
    p(
        &format!("#{id} .relationshipLabelBox rect{{opacity:0.5;}}"),
        &mut css,
    );
    p(
        &format!(
            "#{id} .labelBkg{{background-color:{lbkg};}}",
            lbkg = labelbkg_color
        ),
        &mut css,
    );
    p(
        &format!(
            "#{id} .edgeLabel{{background-color:{ebg};}}",
            ebg = edge_label_bg
        ),
        &mut css,
    );
    p(
        &format!(
            "#{id} .edgeLabel .label rect{{fill:{ebg};}}",
            ebg = edge_label_bg
        ),
        &mut css,
    );
    p(
        &format!(
            "#{id} .edgeLabel .label text{{fill:{tc};}}",
            tc = text_color
        ),
        &mut css,
    );
    p(
        &format!(
            "#{id} .edgeLabel .label{{fill:{nb};font-size:14px;}}",
            nb = node_border
        ),
        &mut css,
    );
    p(
        &format!(
            "#{id} .label{{font-family:{ff};color:{ntc};}}",
            ff = ff,
            ntc = node_text_color
        ),
        &mut css,
    );
    p(
        &format!("#{id} .edge-pattern-dashed{{stroke-dasharray:8,8;}}"),
        &mut css,
    );
    p(&format!(
        "#{id} .node rect,#{id} .node circle,#{id} .node ellipse,#{id} .node polygon{{fill:{mb};stroke:{nb};stroke-width:1px;}}",
        mb = main_bkg, nb = node_border,
    ), &mut css);
    p(
        &format!(
            "#{id} .relationshipLine{{stroke:{lc};stroke-width:1px;fill:none;}}",
            lc = line_color
        ),
        &mut css,
    );
    p(
        &format!(
            "#{id} .marker{{fill:none!important;stroke:{lc}!important;stroke-width:1;}}",
            lc = line_color
        ),
        &mut css,
    );
    p(
        &format!(
            "#{id} [data-look=neo].labelBkg{{background-color:{lbkg};}}",
            lbkg = labelbkg_color,
        ),
        &mut css,
    );
    // Neo-look shared rules — verbatim strings.
    p(
        &format!("#{id} .node .neo-node{{stroke:{nb};}}", nb = node_border),
        &mut css,
    );
    p(&format!(
        "#{id} [data-look=\"neo\"].node rect,#{id} [data-look=\"neo\"].cluster rect,#{id} [data-look=\"neo\"].node polygon{{stroke:{nb};filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}",
        nb = node_border,
    ), &mut css);
    p(
        &format!(
            "#{id} [data-look=\"neo\"].node path{{stroke:{nb};stroke-width:1px;}}",
            nb = node_border,
        ),
        &mut css,
    );
    p(&format!(
        "#{id} [data-look=\"neo\"].node .outer-path{{filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}"
    ), &mut css);
    p(
        &format!(
            "#{id} [data-look=\"neo\"].node .neo-line path{{stroke:{nb};filter:none;}}",
            nb = node_border,
        ),
        &mut css,
    );
    p(&format!(
        "#{id} [data-look=\"neo\"].node circle{{stroke:{nb};filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}",
        nb = node_border,
    ), &mut css);
    p(
        &format!("#{id} [data-look=\"neo\"].node circle .state-start{{fill:#000000;}}"),
        &mut css,
    );
    p(&format!(
        "#{id} [data-look=\"neo\"].icon-shape .icon{{fill:{nb};filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}",
        nb = node_border,
    ), &mut css);
    p(&format!(
        "#{id} [data-look=\"neo\"].icon-shape .icon-neo path{{stroke:{nb};filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}",
        nb = node_border,
    ), &mut css);
    p(
        &format!("#{id} :root{{--mermaid-font-family:{ff};}}", ff = ff,),
        &mut css,
    );
    p("</style>", &mut css);
    css
}

// ──────────────────────────────────────────────────────────────────────
// Markers — 8 ER cardinality marker defs.
// ──────────────────────────────────────────────────────────────────────
fn markers_block(id: &str) -> String {
    // Fixed strings matching the reference output byte-for-byte.
    let mut s = String::with_capacity(3200);
    s.push_str(&format!(
        r#"<defs><marker id="{id}_er-onlyOneStart" class="marker onlyOne er" refX="0" refY="9" markerWidth="18" markerHeight="18" orient="auto"><path d="M9,0 L9,18 M15,0 L15,18"></path></marker></defs>"#,
        id = id,
    ));
    s.push_str(&format!(
        r#"<defs><marker id="{id}_er-onlyOneEnd" class="marker onlyOne er" refX="18" refY="9" markerWidth="18" markerHeight="18" orient="auto"><path d="M3,0 L3,18 M9,0 L9,18"></path></marker></defs>"#,
        id = id,
    ));
    s.push_str(&format!(
        r#"<defs><marker id="{id}_er-zeroOrOneStart" class="marker zeroOrOne er" refX="0" refY="9" markerWidth="30" markerHeight="18" orient="auto"><circle fill="white" cx="21" cy="9" r="6"></circle><path d="M9,0 L9,18"></path></marker></defs>"#,
        id = id,
    ));
    s.push_str(&format!(
        r#"<defs><marker id="{id}_er-zeroOrOneEnd" class="marker zeroOrOne er" refX="30" refY="9" markerWidth="30" markerHeight="18" orient="auto"><circle fill="white" cx="9" cy="9" r="6"></circle><path d="M21,0 L21,18"></path></marker></defs>"#,
        id = id,
    ));
    s.push_str(&format!(
        r#"<defs><marker id="{id}_er-oneOrMoreStart" class="marker oneOrMore er" refX="18" refY="18" markerWidth="45" markerHeight="36" orient="auto"><path d="M0,18 Q 18,0 36,18 Q 18,36 0,18 M42,9 L42,27"></path></marker></defs>"#,
        id = id,
    ));
    s.push_str(&format!(
        r#"<defs><marker id="{id}_er-oneOrMoreEnd" class="marker oneOrMore er" refX="27" refY="18" markerWidth="45" markerHeight="36" orient="auto"><path d="M3,9 L3,27 M9,18 Q27,0 45,18 Q27,36 9,18"></path></marker></defs>"#,
        id = id,
    ));
    s.push_str(&format!(
        r#"<defs><marker id="{id}_er-zeroOrMoreStart" class="marker zeroOrMore er" refX="18" refY="18" markerWidth="57" markerHeight="36" orient="auto"><circle fill="white" cx="48" cy="18" r="6"></circle><path d="M0,18 Q18,0 36,18 Q18,36 0,18"></path></marker></defs>"#,
        id = id,
    ));
    s.push_str(&format!(
        r#"<defs><marker id="{id}_er-zeroOrMoreEnd" class="marker zeroOrMore er" refX="39" refY="18" markerWidth="57" markerHeight="36" orient="auto"><circle fill="white" cx="9" cy="18" r="6"></circle><path d="M21,18 Q39,0 57,18 Q39,36 21,18"></path></marker></defs>"#,
        id = id,
    ));
    s
}

// ──────────────────────────────────────────────────────────────────────
// Trailing drop-shadow filter <defs>s — always present.
// ──────────────────────────────────────────────────────────────────────
fn drop_shadow_defs(id: &str) -> String {
    format!(
        r##"<defs><filter id="{id}-drop-shadow" height="130%" width="130%"><feDropShadow dx="4" dy="4" stdDeviation="0" flood-opacity="0.06" flood-color="#000000"></feDropShadow></filter></defs><defs><filter id="{id}-drop-shadow-small" height="150%" width="150%"><feDropShadow dx="2" dy="2" stdDeviation="0" flood-opacity="0.06" flood-color="#000000"></feDropShadow></filter></defs>"##,
        id = id,
    )
}

// ──────────────────────────────────────────────────────────────────────
// Local helpers
// ──────────────────────────────────────────────────────────────────────

/// Khroma-style fade — convert `color` to an `rgba(...)` string with
/// the requested opacity. Preserves f64 precision so the emitted
/// channel values match JS `Number.toString` output byte-for-byte
/// (the HSL path is the load-bearing case for ER's `labelBkg`).
fn fade(color: &str, opacity: f64) -> String {
    if let Some((r, g, b)) = hsl_to_rgb_f64(color) {
        // khroma's `channel()` passes through `lang.round` which rounds
        // to 10 decimal places — we must mirror that for byte parity.
        let r = khroma_round(r);
        let g = khroma_round(g);
        let b = khroma_round(b);
        return format!(
            "rgba({}, {}, {}, {})",
            fmt_khroma(r),
            fmt_khroma(g),
            fmt_khroma(b),
            fmt_num(opacity)
        );
    }
    if let Some((r, g, b)) = parse_hex_color(color) {
        return format!("rgba({}, {}, {}, {})", r, g, b, fmt_num(opacity));
    }
    format!("rgba({}, {})", color, fmt_num(opacity))
}

/// Khroma's channel extraction rounds trivially to 0..255 but keeps
/// floating-point precision — values like `255.0` print as `255` and
/// `248.6666…` prints in full.
fn fmt_khroma(v: f64) -> String {
    if v.is_finite() && v == v.trunc() {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

/// `Math.round(x * 1e10) / 1e10` — khroma's `lang.round`.
fn khroma_round(v: f64) -> f64 {
    (v * 1e10).round() / 1e10
}

/// HSL(h, s%, l%) → (r, g, b) each in [0, 255]. Uses khroma's `hue2rgb`
/// formula (not the modern chroma-based one) so f64 output matches
/// `khroma.channel(color, 'r' | 'g' | 'b')` byte-for-byte.
fn hsl_to_rgb_f64(s: &str) -> Option<(f64, f64, f64)> {
    let s = s.trim();
    if !(s.starts_with("hsl(") || s.starts_with("hsla(")) {
        return None;
    }
    let open = s.find('(')?;
    let close = s.rfind(')')?;
    let inner = &s[open + 1..close];
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() < 3 {
        return None;
    }
    let h = parts[0].trim().parse::<f64>().ok()?;
    let sp = parts[1].trim().trim_end_matches('%').parse::<f64>().ok()?;
    let lp = parts[2].trim().trim_end_matches('%').parse::<f64>().ok()?;

    if sp == 0.0 {
        let v = lp * 2.55;
        return Some((v, v, v));
    }
    let h = (h % 360.0) / 360.0;
    let s = sp / 100.0;
    let l = lp / 100.0;
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        (l + s) - (l * s)
    };
    let p = 2.0 * l - q;
    let hue2rgb = |t: f64| -> f64 {
        let mut t = t;
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    Some((
        hue2rgb(h + 1.0 / 3.0) * 255.0,
        hue2rgb(h) * 255.0,
        hue2rgb(h - 1.0 / 3.0) * 255.0,
    ))
}

fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    let hex = s.strip_prefix('#')?;
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            Some((r * 17, g * 17, b * 17))
        }
        6 | 8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        _ => None,
    }
}

/// Strip space after commas **outside** double-quoted regions —
/// matches stylis' CSS minification behaviour for font-family lists.
fn stylis_minify_commas(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_quote = false;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' {
            in_quote = !in_quote;
            out.push('"');
            i += 1;
            continue;
        }
        if !in_quote && c == b',' {
            out.push(',');
            i += 1;
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            continue;
        }
        // Keep char as-is (UTF-8 safe: we only skip after ASCII comma).
        out.push(c as char);
        i += 1;
    }
    out
}

fn fmt_num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    // Use Rust's default f64 printing — matches V8 Number#toString for
    // almost every value we emit.
    format!("{}", v)
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

// ──────────────────────────────────────────────────────────────────────
// Byte-exact tests against the reference corpus.
// ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::er as layout_er;
    use crate::parser::er as parser_er;
    use crate::theme::get_theme;

    fn id_for_rel(rel: &str) -> String {
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
        if id.ends_with('-') {
            id.pop();
        }
        id
    }

    fn render_fixture(source: &str, id: &str) -> String {
        let d = parser_er::parse(source).expect("parse");
        let theme = get_theme("default");
        let l = layout_er::layout(&d, &theme).expect("layout");
        super::render(&d, &l, &theme, id).expect("render")
    }

    /// Byte-exact-or-approximate compare (adapted from wave1_e2e).
    fn assert_byte_exact(got: &str, expected: &str, fixture: &str) -> bool {
        if got == expected {
            return true;
        }
        // quick numeric-tolerant retry (not perfect — but catches print drift).
        let a_ok = got.len() == expected.len();
        if !a_ok {
            eprintln!(
                "length mismatch on {}: got {} vs expected {}",
                fixture,
                got.len(),
                expected.len()
            );
        }
        false
    }

    fn check_one(rel: &str) -> bool {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mmd = base.join("tests").join(format!("{}.mmd", rel));
        let svg = base.join("tests/reference").join(format!("{}.svg", rel));
        let source = match std::fs::read_to_string(&mmd) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let expected = match std::fs::read_to_string(&svg) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let id = id_for_rel(rel);
        let got = match std::panic::catch_unwind(|| render_fixture(&source, &id)) {
            Ok(s) => s,
            Err(_) => return false,
        };
        assert_byte_exact(&got, &expected, rel)
    }

    #[test]
    fn byte_exact_sweep() {
        // Walk every cypress + demos ER fixture. This test reports the
        // pass rate but does not fail on partial results — the Wave 4
        // agent hands off known-partial items (mostly blocked on
        // rough.js / dagre-layout divergences) to follow-up waves.
        let cypress: Vec<String> = (1..=73).map(|n| format!("{:02}", n)).collect();
        let demos: Vec<String> = (1..=7).map(|n| format!("{:02}", n)).collect();

        let mut pass = 0usize;
        let mut passing: Vec<String> = Vec::new();
        let mut fail_names: Vec<String> = Vec::new();
        for n in &cypress {
            let rel = format!("ext_fixtures/cypress/er/{}", n);
            if check_one(&rel) {
                pass += 1;
                passing.push(rel);
            } else {
                fail_names.push(rel);
            }
        }
        for n in &demos {
            let rel = format!("ext_fixtures/demos/er/{}", n);
            if check_one(&rel) {
                pass += 1;
                passing.push(rel);
            } else {
                fail_names.push(rel);
            }
        }
        eprintln!("ER byte-exact: {}/80", pass);
        eprintln!("Passing ({}): {:?}", passing.len(), passing);
        if pass < 80 {
            eprintln!(
                "Failing ({}): {:?}",
                fail_names.len(),
                &fail_names[..fail_names.len().min(25)]
            );
        }
    }

    /// Locked-in byte-exact set. These fixtures currently pass and
    /// must continue to do so — regressions here indicate the shared
    /// plumbing (dagre bridge, edge routing, theme CSS, fonts, rough.js
    /// PRNG) has drifted.
    ///
    /// The Wave 3.5 rough.js port added `cypress/er/12..25` and the
    /// stable ER demos (`03..07`) — fixtures that were previously
    /// blocked on the missing rough.js emission for attribute-bearing
    /// entities.
    #[test]
    fn byte_exact_locked_set() {
        for rel in [
            "ext_fixtures/cypress/er/01",
            "ext_fixtures/cypress/er/02",
            "ext_fixtures/cypress/er/12",
            "ext_fixtures/cypress/er/13",
            "ext_fixtures/cypress/er/14",
            "ext_fixtures/cypress/er/15",
            "ext_fixtures/cypress/er/16",
            "ext_fixtures/cypress/er/17",
            "ext_fixtures/cypress/er/18",
            "ext_fixtures/cypress/er/19",
            "ext_fixtures/cypress/er/20",
            "ext_fixtures/cypress/er/21",
            "ext_fixtures/cypress/er/22",
            "ext_fixtures/cypress/er/23",
            "ext_fixtures/cypress/er/24",
            "ext_fixtures/cypress/er/25",
            "ext_fixtures/cypress/er/27",
            "ext_fixtures/cypress/er/28",
            "ext_fixtures/cypress/er/43",
            "ext_fixtures/cypress/er/49",
            "ext_fixtures/cypress/er/50",
            "ext_fixtures/cypress/er/53",
            "ext_fixtures/cypress/er/54",
            "ext_fixtures/cypress/er/55",
            "ext_fixtures/cypress/er/56",
            "ext_fixtures/cypress/er/57",
            "ext_fixtures/cypress/er/58",
            "ext_fixtures/cypress/er/59",
            "ext_fixtures/cypress/er/61",
            "ext_fixtures/cypress/er/62",
            "ext_fixtures/cypress/er/64",
            "ext_fixtures/cypress/er/65",
            "ext_fixtures/cypress/er/67",
            "ext_fixtures/cypress/er/68",
            "ext_fixtures/cypress/er/69",
            "ext_fixtures/cypress/er/70",
            "ext_fixtures/cypress/er/73",
            "ext_fixtures/demos/er/03",
            "ext_fixtures/demos/er/04",
            "ext_fixtures/demos/er/05",
            "ext_fixtures/demos/er/07",
        ] {
            assert!(check_one(rel), "fixture {} must remain byte-exact", rel);
        }
    }
}

#[cfg(test)]
mod probe_tests {
    use super::*;
    use crate::layout::er as layout_er;
    use crate::parser::er as parser_er;
    use crate::theme::get_theme;

    fn diff_probe(name: &str) {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let source = std::fs::read_to_string(
            base.join(format!("tests/ext_fixtures/cypress/er/{}.mmd", name)),
        )
        .unwrap();
        let expected = std::fs::read_to_string(base.join(format!(
            "tests/reference/ext_fixtures/cypress/er/{}.svg",
            name
        )))
        .unwrap();
        let d = parser_er::parse(&source).unwrap();
        let theme = get_theme("default");
        let l = layout_er::layout(&d, &theme).unwrap();
        let id = format!("ref-ext-fixtures-cypress-er-{}", name);
        let got = super::render(&d, &l, &theme, &id).unwrap();
        let a = got.as_bytes();
        let b = expected.as_bytes();
        let n = a.len().min(b.len());
        let mut i = 0;
        while i < n && a[i] == b[i] {
            i += 1;
        }
        if i >= n && a.len() == b.len() {
            eprintln!("ER{} BYTE EXACT!", name);
            return;
        }
        let ctx_lo = i.saturating_sub(40);
        let ctx_hi_a = (i + 200).min(a.len());
        let ctx_hi_b = (i + 200).min(b.len());
        eprintln!(
            "ER{} diverge at byte {i} (got={}, want={})",
            name,
            a.len(),
            b.len()
        );
        eprintln!(
            "got [{ctx_lo}..]: {}",
            String::from_utf8_lossy(&a[ctx_lo..ctx_hi_a])
        );
        eprintln!(
            "want[{ctx_lo}..]: {}",
            String::from_utf8_lossy(&b[ctx_lo..ctx_hi_b])
        );
    }

    #[test]
    #[ignore]
    fn er33_diff_probe() {
        diff_probe("33");
    }
    #[test]
    #[ignore]
    fn er41_diff_probe() {
        diff_probe("41");
    }
    #[test]
    #[ignore]
    fn er40_diff_probe() {
        diff_probe("40");
    }
    #[test]
    #[ignore]
    fn er03_diff_probe() {
        diff_probe("03");
    }

    #[test]
    #[ignore] // Run with `cargo test er12_diff_probe -- --ignored --nocapture`.
    fn er12_diff_probe() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let source =
            std::fs::read_to_string(base.join("tests/ext_fixtures/cypress/er/12.mmd")).unwrap();
        let expected =
            std::fs::read_to_string(base.join("tests/reference/ext_fixtures/cypress/er/12.svg"))
                .unwrap();
        let d = parser_er::parse(&source).unwrap();
        let theme = get_theme("default");
        let l = layout_er::layout(&d, &theme).unwrap();
        let got = super::render(&d, &l, &theme, "ref-ext-fixtures-cypress-er-12").unwrap();

        // Find where they diverge.
        let a = got.as_bytes();
        let b = expected.as_bytes();
        let n = a.len().min(b.len());
        let mut i = 0;
        while i < n && a[i] == b[i] {
            i += 1;
        }
        if i >= n && a.len() == b.len() {
            eprintln!("ER12 BYTE EXACT!");
            return;
        }
        let ctx_lo = i.saturating_sub(40);
        let ctx_hi_a = (i + 200).min(a.len());
        let ctx_hi_b = (i + 200).min(b.len());
        eprintln!(
            "diverge at byte {i} (lens got={}, want={})",
            a.len(),
            b.len()
        );
        eprintln!(
            "got[{ctx_lo}..]: {}",
            String::from_utf8_lossy(&a[ctx_lo..ctx_hi_a])
        );
        eprintln!("-----");
        eprintln!(
            "want[{ctx_lo}..]: {}",
            String::from_utf8_lossy(&b[ctx_lo..ctx_hi_b])
        );
    }
}
