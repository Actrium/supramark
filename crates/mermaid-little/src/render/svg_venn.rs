//! Venn SVG renderer — emits the byte-exact output mermaid@11.14.0 produces.
//!
//! Mirrors `vennRenderer.ts` (upstream) plus the d3-driven path emission
//! inside `@upsetjs/venn.js`. Produces:
//!
//!  1. Outer `<svg>` with the standard mermaid wrapping (`<g></g>`,
//!     style block, viewBox).
//!  2. Optional `<text class="venn-title">` when the diagram has a title.
//!  3. `<g transform="translate(0, titleHeight)">` containing one
//!     `<g class="venn-area venn-circle venn-set-N" data-venn-sets="X">`
//!     per single-set subset, then one
//!     `<g class="venn-area venn-intersection" data-venn-sets="X_Y">`
//!     per multi-set subset, in the order they appear in the source.
//!
//! The path bytes mirror upstream's `circlePath` / `arcsToPath`
//! verbatim (including the embedded newlines in the `d=` attribute).

use crate::error::Result;
use crate::layout::venn::VennLayout;
use crate::model::venn::VennDiagram;
use crate::render::rough::{RoughGenerator, RoughOptions, to_paths};
use crate::theme::ThemeVariables;

pub fn render(d: &VennDiagram, l: &VennLayout, theme: &ThemeVariables, id: &str) -> Result<String> {
    let mut out = String::with_capacity(8192);

    // ── Opening <svg> ────────────────────────────────────────────────
    let viewbox_w = l.viewbox_w;
    let viewbox_h = l.viewbox_h;
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" style="max-width: {w}px;" role="graphics-document document" aria-roledescription="venn">"#,
        id = id,
        w = num_int(viewbox_w),
        h = num_int(viewbox_h),
    ));

    // ── <style> block ────────────────────────────────────────────────
    out.push_str(&style_block(id, theme));

    // ── empty <g></g> separator (mermaid emits this as the first sibling) ──
    out.push_str("<g></g>");

    // ── Optional title text ──────────────────────────────────────────
    let scale = l.scale;
    let title_h = l.title_height;
    if let Some(title) = d.meta.title.as_deref() {
        let title_font_size = 32.0 * scale;
        let title_y = 32.0 * scale; // upstream: `'y', 32 * scale` then later transform; the SVG has y=16 here
                                    // Upstream sets y=`32 * scale` but SVG shows y=16 for scale=0.5. 32*0.5=16. ✓
        out.push_str(&format!(
            r#"<text class="venn-title" font-size="{fs}px" text-anchor="middle" dominant-baseline="middle" x="50%" y="{y}" style="fill: {fill};">{text}</text>"#,
            fs = num(title_font_size),
            y = num(title_y),
            fill = l.title_text_color,
            text = escape_text(title),
        ));
    }

    // ── Container <g transform="translate(0, titleH)"> ───────────────
    out.push_str(&format!(
        r#"<g transform="translate(0, {th})">"#,
        th = num_int(title_h),
    ));

    // Helper: build a styles_by_key (sets joined with `|`) from the diagram.
    let mut style_by_key: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, String>,
    > = std::collections::BTreeMap::new();
    for s in &d.styles {
        let key = s.targets.join("|");
        let entry = style_by_key.entry(key).or_default();
        for (k, v) in &s.styles {
            entry.insert(k.clone(), v.clone());
        }
    }

    // Walk circle areas first (single-set), then intersection areas (multi-set),
    // each in source order, assigning venn-set-N indices to circles.
    let mut single_index: usize = 0;
    // Mirror upstream `vennRenderer.ts` ln 121:
    //   `const themeDark = isDark(themeVariables.background || "#f4f4f4");`
    let dark_bg = crate::theme::color::is_dark(theme.background.as_deref().unwrap_or("#f4f4f4"));

    for area in &l.areas {
        let key_pipe = area.sets.join("|");
        let custom_style = style_by_key.get(&key_pipe);
        if area.sets.len() == 1 {
            // venn-circle
            let i = single_index;
            single_index += 1;
            let base_color = custom_style
                .and_then(|m| m.get("fill"))
                .cloned()
                .or_else(|| l.theme_colors.get(i % l.theme_colors.len().max(1)).cloned())
                .unwrap_or_else(|| theme.primary_color.clone().unwrap_or("#fff".into()));
            let fill_opacity = custom_style
                .and_then(|m| m.get("fill-opacity"))
                .cloned()
                .unwrap_or_else(|| "0.1".into());
            let stroke_color = custom_style
                .and_then(|m| m.get("stroke"))
                .cloned()
                .unwrap_or_else(|| base_color.clone());
            let stroke_width = custom_style
                .and_then(|m| m.get("stroke-width"))
                .cloned()
                .unwrap_or_else(|| num(5.0 * scale));

            let text_color = custom_style
                .and_then(|m| m.get("color"))
                .cloned()
                .unwrap_or_else(|| {
                    // Mirror upstream `vennRenderer.ts` line 154:
                    // `themeDark ? lighten(baseColor, 30) : darken(baseColor, 30)`.
                    // Khroma's lighten/darken also handles hex / rgb inputs by
                    // converting to HSL first; the legacy `adjust_l` here only
                    // matched HSL strings, missing the hex-input branch used
                    // by `style A fill:#ff6b6b` etc.
                    if dark_bg {
                        crate::theme::color::lighten(&base_color, 30.0)
                    } else {
                        crate::theme::color::darken(&base_color, 30.0)
                    }
                });

            if d.hand_drawn {
                let fill_color_trans = crate::theme::color::transparentize(&base_color, 0.7);
                let sw_val: f64 = stroke_width.parse().unwrap_or(5.0 * scale);
                let mut rc = RoughGenerator::new();
                let mut o = RoughOptions::default();
                o.seed = d.hand_drawn_seed.unwrap_or(0) as i32;
                o.roughness = 0.7;
                o.fill = Some(fill_color_trans);
                o.fill_style = "hachure".into();
                o.fill_weight = 2.0;
                o.hachure_gap = 8.0;
                o.hachure_angle = -41.0 + (i as f64) * 60.0;
                o.stroke = stroke_color.clone();
                o.stroke_width = sw_val;
                // Mermaid uses the padding=8 layout (`layoutByKey`) for the
                // handDrawn rough.js circle — distinct from the padding=15
                // circles that drive the non-handDrawn `<path>` outline.
                let c = area
                    .text_node_circles
                    .first()
                    .copied()
                    .unwrap_or(area.circles[0]);
                let drawable = rc.circle(c.x, c.y, c.radius * 2.0, &o);
                let paths = to_paths(&drawable, &o);
                let mut paths_html = String::new();
                for p in &paths {
                    paths_html.push_str(&format!(
                        r#"<path d="{d}" stroke="{s}" stroke-width="{sw}" fill="{f}"></path>"#,
                        d = p.d,
                        s = p.stroke,
                        sw = crate::render::rough::fmt_num(p.stroke_width),
                        f = p.fill,
                    ));
                }
                out.push_str(&format!(
                    r#"<g class="venn-area venn-circle venn-set-{i}" data-venn-sets="{sets}"><g>{paths_html}</g><text class="label" text-anchor="middle" dy=".35em" x="{tx}" y="{ty}" style="fill: {tfill}; font-size: {fs}px;"><tspan x="{tx}" y="{ty}" dy="0.35em">{label}</tspan></text></g>"#,
                    i = i % 8,
                    sets = area.sets.join("_"),
                    paths_html = paths_html,
                    tx = area.text_x,
                    ty = area.text_y,
                    tfill = text_color,
                    fs = num(48.0 * scale),
                    label = escape_text(&area.render_label),
                ));
            } else {
            out.push_str(&format!(
                r#"<g class="venn-area venn-circle venn-set-{i}" data-venn-sets="{sets}"><path style="fill-opacity: {op}; fill: {fill}; stroke: {stroke}; stroke-width: {sw}; stroke-opacity: 0.95;" d="{d}"></path><text class="label" text-anchor="middle" dy=".35em" x="{tx}" y="{ty}" style="fill: {tfill}; font-size: {fs}px;"><tspan x="{tx}" y="{ty}" dy="0.35em">{label}</tspan></text></g>"#,
                i = i % 8,
                sets = area.sets.join("_"),
                op = fill_opacity,
                fill = base_color,
                stroke = stroke_color,
                sw = stroke_width,
                d = area.path,
                tx = area.text_x,
                ty = area.text_y,
                tfill = text_color,
                fs = num(48.0 * scale),
                label = escape_text(&area.render_label),
            ));
            }
        } else {
            // venn-intersection
            let custom_fill = custom_style.and_then(|m| m.get("fill")).cloned();
            let fill_opacity = if custom_fill.is_some() { "1" } else { "0" };
            let fill_value = custom_fill.clone().unwrap_or_else(|| "transparent".into());
            let text_color = custom_style
                .and_then(|m| m.get("color"))
                .cloned()
                .unwrap_or_else(|| l.set_text_color.clone());
            let label_text = area.label.clone().unwrap_or_default();

            if d.hand_drawn && custom_fill.is_some() {
                // Mermaid's handDrawn intersection-with-customFill path:
                // upstream replaces the original `<path>` with a
                // rough.js cross-hatch fill path entirely (no stroke).
                // See vennRenderer.ts: `roughSvg.path(pathD, {…})`,
                // `existingPath.parentNode.insertBefore(roughNode, …)`,
                // `pathEl.remove()`.
                let cf = custom_fill.clone().unwrap();
                let fill_trans = crate::theme::color::transparentize(&cf, 0.3);
                let mut rc = RoughGenerator::new();
                let mut o = RoughOptions::default();
                o.seed = d.hand_drawn_seed.unwrap_or(0) as i32;
                o.roughness = 0.7;
                o.fill = Some(fill_trans);
                o.fill_style = "cross-hatch".into();
                o.fill_weight = 2.0;
                o.hachure_gap = 6.0;
                o.hachure_angle = 60.0;
                o.stroke = "none".into();
                let drawable = rc.path(&area.path, &o);
                let paths = to_paths(&drawable, &o);
                let mut paths_html = String::new();
                for p in &paths {
                    paths_html.push_str(&format!(
                        r#"<path d="{d}" stroke="{s}" stroke-width="{sw}" fill="{f}"></path>"#,
                        d = p.d,
                        s = p.stroke,
                        sw = crate::render::rough::fmt_num(p.stroke_width),
                        f = p.fill,
                    ));
                }
                out.push_str(&format!(
                    r#"<g class="venn-area venn-intersection" data-venn-sets="{sets}"><g>{paths_html}</g><text class="label" text-anchor="middle" dy=".35em" x="{tx}" y="{ty}" style="fill: {tfill}; font-size: {fs}px;"><tspan x="{tx}" y="{ty}" dy="0.35em">{label}</tspan></text></g>"#,
                    sets = area.sets.join("_"),
                    paths_html = paths_html,
                    tx = area.text_x,
                    ty = area.text_y,
                    tfill = text_color,
                    fs = num(48.0 * scale),
                    label = escape_text(&label_text),
                ));
            } else {
                // Mermaid's handDrawn path: when no customFill is
                // supplied, upstream only mutates `fill-opacity` on
                // the existing VennDiagram path (no `fill` style).
                let path_style = if d.hand_drawn && custom_fill.is_none() {
                    format!(r#"fill-opacity: {op};"#, op = fill_opacity)
                } else {
                    format!(
                        r#"fill-opacity: {op}; fill: {fill};"#,
                        op = fill_opacity,
                        fill = fill_value
                    )
                };

                out.push_str(&format!(
                    r#"<g class="venn-area venn-intersection" data-venn-sets="{sets}"><path style="{ps}" d="{d}"></path><text class="label" text-anchor="middle" dy=".35em" x="{tx}" y="{ty}" style="fill: {tfill}; font-size: {fs}px;"><tspan x="{tx}" y="{ty}" dy="0.35em">{label}</tspan></text></g>"#,
                    sets = area.sets.join("_"),
                    ps = path_style,
                    d = area.path,
                    tx = area.text_x,
                    ty = area.text_y,
                    tfill = text_color,
                    fs = num(48.0 * scale),
                    label = escape_text(&label_text),
                ));
            }
        }
    }

    // ── Text nodes (foreignObject) ──────────────────────────────────
    // Mirrors `renderTextNodes` in upstream `vennRenderer.ts`.
    if !d.text_nodes.is_empty() {
        render_text_nodes(&mut out, d, l, &style_by_key, scale);
    }

    out.push_str("</g></svg>");
    Ok(out)
}

/// Render `<g class="venn-text-nodes">…</g>` block from `d.text_nodes`,
/// mirroring `renderTextNodes` in `vennRenderer.ts`.
fn render_text_nodes(
    out: &mut String,
    d: &crate::model::venn::VennDiagram,
    l: &crate::layout::venn::VennLayout,
    style_by_key: &std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    scale: f64,
) {
    use crate::layout::venn::AreaLayout;

    // Group text nodes by their (sorted) sets-key. Preserve the insertion
    // order of the FIRST node in each group so the output matches upstream
    // (which uses `Map`'s iteration order).
    let mut order: Vec<String> = Vec::new();
    let mut nodes_by_area: std::collections::BTreeMap<
        String,
        Vec<&crate::model::venn::VennTextNode>,
    > = std::collections::BTreeMap::new();
    for node in &d.text_nodes {
        let key = node.sets.join("|");
        if !nodes_by_area.contains_key(&key) {
            order.push(key.clone());
        }
        nodes_by_area.entry(key).or_default().push(node);
    }

    // Look up areas by their (sorted-sets) join("|") key.
    let mut areas_by_key: std::collections::BTreeMap<String, &AreaLayout> =
        std::collections::BTreeMap::new();
    for a in &l.areas {
        let mut sorted = a.sets.clone();
        sorted.sort();
        areas_by_key.insert(sorted.join("|"), a);
    }

    out.push_str(r#"<g class="venn-text-nodes">"#);

    for key in &order {
        let nodes = match nodes_by_area.get(key) {
            Some(v) => v,
            None => continue,
        };
        let area = match areas_by_key.get(key) {
            Some(a) => *a,
            None => continue, // upstream's `if (!area?.text)` guard
        };

        // Use the padding=8 circles + their text centres for text-node
        // positioning. Upstream's `renderTextNodes` reads `area.text.x/y` and
        // `area.circles` from `layoutByKey`, which is built from the second
        // `venn.layout(...)` call (padding sourced from `config.venn.padding`,
        // default 8) — distinct from the visible `VennDiagram` circles
        // (hardcoded padding 15).
        let center_x = area.text_node_centre_x;
        let center_y = area.text_node_centre_y;
        let circles_for_radii = &area.text_node_circles;
        // Math.min over circle radii (NaN if empty — no text would render then).
        let min_radius = circles_for_radii
            .iter()
            .map(|c| c.radius)
            .fold(f64::INFINITY, f64::min);
        // ECMAScript Math.hypot(a, b) — V8 follows the spec's iterative
        // Kahan-summed algorithm for >2 args but for the 2-arg case it
        // directly evaluates `max * sqrt(1 + (min/max)²)`. This differs
        // from libm's `hypot` (fdlibm hi/lo splitting) by up to 1 ULP on
        // pathological inputs (e.g. one arg ≪ the other). Use the V8
        // formulation here so triple-intersection text-node sizes match
        // byte-for-byte.
        let v8_hypot = |a: f64, b: f64| -> f64 {
            let aa = a.abs();
            let bb = b.abs();
            if aa.is_infinite() || bb.is_infinite() {
                return f64::INFINITY;
            }
            if aa.is_nan() || bb.is_nan() {
                return f64::NAN;
            }
            let (hi, lo) = if aa > bb { (aa, bb) } else { (bb, aa) };
            if hi == 0.0 {
                return 0.0;
            }
            let r = lo / hi;
            hi * libm::sqrt(1.0 + r * r)
        };
        let inner_radius_raw = circles_for_radii
            .iter()
            .map(|c| c.radius - v8_hypot(center_x - c.x, center_y - c.y))
            .fold(f64::INFINITY, f64::min);
        let mut inner_radius = if inner_radius_raw.is_finite() {
            inner_radius_raw.max(0.0)
        } else {
            0.0
        };
        if inner_radius == 0.0 && min_radius.is_finite() {
            inner_radius = min_radius * 0.6;
        }

        let font_size = 40.0 * scale;
        out.push_str(&format!(
            r#"<g class="venn-text-area" font-size="{fs}px">"#,
            fs = num(font_size),
        ));

        if d.use_debug_layout {
            out.push_str(&format!(
                r#"<circle class="venn-text-debug-circle" cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="purple" stroke-width="{sw}" stroke-dasharray="{da1} {da2}"></circle>"#,
                cx = num(center_x),
                cy = num(center_y),
                r = num(inner_radius),
                sw = num(1.5 * scale),
                da1 = num(6.0 * scale),
                da2 = num(4.0 * scale),
            ));
        }

        let inner_width = (80.0 * scale).max(inner_radius * 2.0 * 0.95);
        let inner_height = (60.0 * scale).max(inner_radius * 2.0 * 0.95);
        let has_label = area.label.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
        let label_offset_base = if has_label {
            (32.0 * scale).min(inner_radius * 0.25)
        } else {
            0.0
        };
        let label_offset = label_offset_base + if nodes.len() <= 2 { 30.0 * scale } else { 0.0 };
        let start_x = center_x - inner_width / 2.0;
        let start_y = center_y - inner_height / 2.0 + label_offset;
        let cols = ((nodes.len() as f64).sqrt().ceil() as usize).max(1);
        let rows = ((nodes.len() as f64) / (cols as f64)).ceil() as usize;
        let rows = rows.max(1);
        let cell_width = inner_width / (cols as f64);
        let cell_height = inner_height / (rows as f64);

        for (i, node) in nodes.iter().enumerate() {
            let col = (i % cols) as f64;
            let row = (i / cols) as f64;
            let x = start_x + cell_width * (col + 0.5);
            let y = start_y + cell_height * (row + 0.5);

            if d.use_debug_layout {
                out.push_str(&format!(
                    r#"<rect class="venn-text-debug-cell" x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="teal" stroke-width="{sw}" stroke-dasharray="{da1} {da2}"></rect>"#,
                    x = num(start_x + cell_width * col),
                    y = num(start_y + cell_height * row),
                    w = num(cell_width),
                    h = num(cell_height),
                    sw = num(1.0 * scale),
                    da1 = num(4.0 * scale),
                    da2 = num(3.0 * scale),
                ));
            }

            let box_width = cell_width * 0.9;
            let box_height = cell_height * 0.9;
            let fo_x = x - box_width / 2.0;
            let fo_y = y - box_height / 2.0;

            // Text colour comes from `style <id> color:...` — keyed on the
            // node's own id (single-element targets list).
            let text_color = style_by_key
                .get(&node.id)
                .and_then(|m| m.get("color"))
                .cloned();

            // Body of the <span style="…">: trailing `color: <c>;` only when
            // the node has a custom colour. Mirrors d3 `.style('color', …)`
            // which appends a single declaration without leading whitespace.
            let mut span_style = String::from(
                "display: flex; width: 100%; height: 100%; white-space: normal; align-items: center; justify-content: center; text-align: center; overflow-wrap: normal; word-break: normal;",
            );
            if let Some(c) = &text_color {
                // Mirror jsdom/CSSOM serialisation: a hex literal passed to
                // `el.style.color = '#ff0000'` round-trips back as
                // `rgb(255, 0, 0)`. Named colours and rgb()/rgba()/hsl()
                // already pass through unchanged.
                let normalized = normalize_color_for_span(c);
                span_style.push_str(&format!(" color: {normalized};"));
            }

            let label_text = node.label.clone().unwrap_or_else(|| node.id.clone());

            out.push_str(&format!(
                r#"<foreignObject class="venn-text-node-fo" width="{w}" height="{h}" x="{x}" y="{y}" overflow="visible"><span class="venn-text-node" style="{style}">{label}</span></foreignObject>"#,
                w = num(box_width),
                h = num(box_height),
                x = num(fo_x),
                y = num(fo_y),
                style = span_style,
                label = escape_text(&label_text),
            ));
        }

        out.push_str("</g>");
    }

    out.push_str("</g>");
}

/// Build the venn `<style>` block, mirroring upstream's
/// `getStyles(type, userStyles, options, svgId)` (`src/styles.ts`)
/// concatenated with `diagrams/venn/styles.ts`. Theme-aware so that
/// `theme: dark/forest/neutral` and `themeVariables` overrides flow into
/// every CSS rule.
fn style_block(id: &str, theme: &ThemeVariables) -> String {
    let ff_raw = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\", verdana, arial, sans-serif");
    let font_family = minify_font_family(ff_raw);
    let font_size = theme.font_size.as_deref().unwrap_or("16px");
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let title_color = theme.title_color.as_deref().unwrap_or("#333");
    let venn_title_color = theme
        .venn_title_text_color
        .as_deref()
        .unwrap_or(title_color);
    let venn_set_text_color = theme.venn_set_text_color.as_deref().unwrap_or(text_color);
    let error_bkg = theme.error_bkg_color.as_deref().unwrap_or("#552222");
    let error_text = theme.error_text_color.as_deref().unwrap_or("#552222");
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let drop_shadow = theme
        .drop_shadow
        .as_deref()
        .unwrap_or("drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))");
    // Upstream `getStyles()` substitutes `url(${svgId}-gradient)` for the
    // `[data-look="neo"]` strokes when `theme.useGradient` is truthy
    // (theme: base / dark / forest / neutral). Otherwise the raw
    // nodeBorder hex flows through.
    let use_gradient = theme.use_gradient.unwrap_or(false);
    let neo_stroke: String = if use_gradient {
        format!("url(#{id}-gradient)")
    } else {
        node_border.to_string()
    };

    let mut s = String::with_capacity(4 * 1024);
    s.push_str("<style>");

    s.push_str(&format!(
        "#{id}{{font-family:{ff};font-size:{fs};fill:{tc};}}",
        ff = font_family,
        fs = font_size,
        tc = text_color,
    ));
    s.push_str("@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}");
    s.push_str("@keyframes dash{to{stroke-dashoffset:0;}}");
    s.push_str(&format!(
        "#{id} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}",
    ));
    s.push_str(&format!(
        "#{id} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}",
    ));
    s.push_str(&format!("#{id} .error-icon{{fill:{c};}}", c = error_bkg,));
    s.push_str(&format!(
        "#{id} .error-text{{fill:{c};stroke:{c};}}",
        c = error_text,
    ));
    s.push_str(&format!(
        "#{id} .edge-thickness-normal{{stroke-width:1px;}}"
    ));
    s.push_str(&format!(
        "#{id} .edge-thickness-thick{{stroke-width:3.5px;}}"
    ));
    s.push_str(&format!("#{id} .edge-pattern-solid{{stroke-dasharray:0;}}"));
    s.push_str(&format!(
        "#{id} .edge-thickness-invisible{{stroke-width:0;fill:none;}}",
    ));
    s.push_str(&format!(
        "#{id} .edge-pattern-dashed{{stroke-dasharray:3;}}"
    ));
    s.push_str(&format!(
        "#{id} .edge-pattern-dotted{{stroke-dasharray:2;}}"
    ));
    s.push_str(&format!(
        "#{id} .marker{{fill:{c};stroke:{c};}}",
        c = line_color,
    ));
    s.push_str(&format!(
        "#{id} .marker.cross{{stroke:{c};}}",
        c = line_color,
    ));
    s.push_str(&format!(
        "#{id} svg{{font-family:{ff};font-size:{fs};}}",
        ff = font_family,
        fs = font_size,
    ));
    s.push_str(&format!("#{id} p{{margin:0;}}"));

    // ── Venn-specific styles (`diagrams/venn/styles.ts`).
    s.push_str(&format!(
        "#{id} .venn-title{{font-size:32px;fill:{vtc};font-family:{ff};}}",
        vtc = venn_title_color,
        ff = font_family,
    ));
    s.push_str(&format!(
        "#{id} .venn-circle text{{font-size:48px;font-family:{ff};}}",
        ff = font_family,
    ));
    s.push_str(&format!(
        "#{id} .venn-intersection text{{font-size:48px;fill:{vstc};font-family:{ff};}}",
        vstc = venn_set_text_color,
        ff = font_family,
    ));
    s.push_str(&format!(
        "#{id} .venn-text-node{{font-family:{ff};color:{vstc};}}",
        ff = font_family,
        vstc = venn_set_text_color,
    ));

    // ── Common postamble (shared `data-look="neo"` rules).
    s.push_str(&format!(
        "#{id} .node .neo-node{{stroke:{nb};}}",
        nb = node_border,
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].node rect,#{id} [data-look=\"neo\"].cluster rect,#{id} [data-look=\"neo\"].node polygon{{stroke:{ns};filter:{ds};}}",
        ns = neo_stroke,
        ds = drop_shadow,
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].node path{{stroke:{ns};stroke-width:1px;}}",
        ns = neo_stroke,
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].node .outer-path{{filter:{ds};}}",
        ds = drop_shadow,
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].node .neo-line path{{stroke:{nb};filter:none;}}",
        nb = node_border,
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].node circle{{stroke:{ns};filter:{ds};}}",
        ns = neo_stroke,
        ds = drop_shadow,
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].node circle .state-start{{fill:#000000;}}",
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].icon-shape .icon{{fill:{ns};filter:{ds};}}",
        ns = neo_stroke,
        ds = drop_shadow,
    ));
    s.push_str(&format!(
        "#{id} [data-look=\"neo\"].icon-shape .icon-neo path{{stroke:{ns};filter:{ds};}}",
        ns = neo_stroke,
        ds = drop_shadow,
    ));
    s.push_str(&format!(
        "#{id} :root{{--mermaid-font-family:{ff};}}",
        ff = font_family,
    ));

    s.push_str("</style>");
    s
}

fn num(v: f64) -> String {
    crate::layout::venn::fmt_num(v)
}

fn num_int(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Convert hex colour literals (`#rgb`, `#rrggbb`, `#rrggbbaa`) into the
/// `rgb(r, g, b)` / `rgba(r, g, b, a)` form that jsdom (and any
/// CSS-Object-Model conformant browser) returns when reading back a
/// `style.color = '#…'` assignment.
///
/// Non-hex inputs (named colours, existing `rgb()`/`rgba()`/`hsl()`
/// declarations) are returned untouched.
fn normalize_color_for_span(c: &str) -> String {
    let s = c.trim();
    if !s.starts_with('#') {
        return c.to_string();
    }
    let body = &s[1..];
    let parse_hex = |h: &str| -> Option<u8> { u8::from_str_radix(h, 16).ok() };
    match body.len() {
        3 => {
            let r = parse_hex(&body[0..1]);
            let g = parse_hex(&body[1..2]);
            let b = parse_hex(&body[2..3]);
            if let (Some(r), Some(g), Some(b)) = (r, g, b) {
                let r = r * 17;
                let g = g * 17;
                let b = b * 17;
                return format!("rgb({r}, {g}, {b})");
            }
            c.to_string()
        }
        6 => {
            let r = parse_hex(&body[0..2]);
            let g = parse_hex(&body[2..4]);
            let b = parse_hex(&body[4..6]);
            if let (Some(r), Some(g), Some(b)) = (r, g, b) {
                return format!("rgb({r}, {g}, {b})");
            }
            c.to_string()
        }
        8 => {
            let r = parse_hex(&body[0..2]);
            let g = parse_hex(&body[2..4]);
            let b = parse_hex(&body[4..6]);
            let a = parse_hex(&body[6..8]);
            if let (Some(r), Some(g), Some(b), Some(a)) = (r, g, b, a) {
                let alpha = (a as f64) / 255.0;
                // CSS serialises alpha to canonical rounded form. Match the
                // shape jsdom emits: 3 fractional digits trimmed.
                let alpha_str = format_alpha(alpha);
                return format!("rgba({r}, {g}, {b}, {alpha_str})");
            }
            c.to_string()
        }
        _ => c.to_string(),
    }
}

fn format_alpha(a: f64) -> String {
    if a == 1.0 {
        "1".to_string()
    } else if a == 0.0 {
        "0".to_string()
    } else {
        // jsdom preserves a sane number of digits; round to 3 then strip.
        let s = format!("{:.3}", a);
        let trimmed = s.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    }
}

fn escape_text(s: &str) -> String {
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

/// stylis minification: drop the single ASCII space immediately after
/// each unquoted comma. Mirrors `svg_pie::minify_font_family`.
fn minify_font_family(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_quote = false;
    let mut prev_comma = false;
    for c in s.chars() {
        if c == '"' {
            in_quote = !in_quote;
            out.push(c);
            prev_comma = false;
            continue;
        }
        if !in_quote {
            if c == ',' {
                out.push(c);
                prev_comma = true;
                continue;
            }
            if prev_comma && c == ' ' {
                prev_comma = false;
                continue;
            }
        }
        out.push(c);
        prev_comma = false;
    }
    out
}
