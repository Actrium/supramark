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
    let mut style_by_key: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> = std::collections::BTreeMap::new();
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
    let dark_bg = false; // default theme background is light; dark-theme handling TODO if needed

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
                    if dark_bg {
                        adjust_l(&base_color, 30.0)
                    } else {
                        adjust_l(&base_color, -30.0)
                    }
                });

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
                fs = num(48.0 * scale), // upstream: `${48 * scale}px`
                label = escape_text(&area.render_label),
            ));
        } else {
            // venn-intersection
            let custom_fill = custom_style.and_then(|m| m.get("fill")).cloned();
            let fill_opacity = if custom_fill.is_some() { "1" } else { "0" };
            let fill_value = custom_fill.unwrap_or_else(|| "transparent".into());
            let text_color = custom_style
                .and_then(|m| m.get("color"))
                .cloned()
                .unwrap_or_else(|| l.set_text_color.clone());
            let label_text = area.label.clone().unwrap_or_default();

            out.push_str(&format!(
                r#"<g class="venn-area venn-intersection" data-venn-sets="{sets}"><path style="fill-opacity: {op}; fill: {fill};" d="{d}"></path><text class="label" text-anchor="middle" dy=".35em" x="{tx}" y="{ty}" style="fill: {tfill}; font-size: {fs}px;"><tspan x="{tx}" y="{ty}" dy="0.35em">{label}</tspan></text></g>"#,
                sets = area.sets.join("_"),
                op = fill_opacity,
                fill = fill_value,
                d = area.path,
                tx = area.text_x,
                ty = area.text_y,
                tfill = text_color,
                fs = num(48.0 * scale),
                label = escape_text(&label_text),
            ));
        }
    }

    out.push_str("</g></svg>");
    Ok(out)
}

/// CSS style block — fixed shape with theme-derived colors.
fn style_block(id: &str, theme: &ThemeVariables) -> String {
    let font_family_raw = theme
        .font_family
        .clone()
        .unwrap_or_else(|| "\"trebuchet ms\", verdana, arial, sans-serif".into());
    let font_family = minify_font_family(&font_family_raw);
    let font_size = theme.font_size.clone().unwrap_or_else(|| "16px".into());
    let text_color = theme
        .text_color
        .clone()
        .unwrap_or_else(|| "#333".into());
    let title_color = theme.title_color.clone().unwrap_or_else(|| "#333".into());
    let venn_title_color = theme
        .venn_title_text_color
        .clone()
        .unwrap_or_else(|| title_color.clone());
    let venn_set_text_color = theme
        .venn_set_text_color
        .clone()
        .unwrap_or_else(|| text_color.clone());

    format!(
        "<style>#{id}{{font-family:{ff};font-size:{fs};fill:{tc};}}\
@keyframes edge-animation-frame{{from{{stroke-dashoffset:0;}}}}\
@keyframes dash{{to{{stroke-dashoffset:0;}}}}\
#{id} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}\
#{id} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}\
#{id} .error-icon{{fill:#552222;}}\
#{id} .error-text{{fill:#552222;stroke:#552222;}}\
#{id} .edge-thickness-normal{{stroke-width:1px;}}\
#{id} .edge-thickness-thick{{stroke-width:3.5px;}}\
#{id} .edge-pattern-solid{{stroke-dasharray:0;}}\
#{id} .edge-thickness-invisible{{stroke-width:0;fill:none;}}\
#{id} .edge-pattern-dashed{{stroke-dasharray:3;}}\
#{id} .edge-pattern-dotted{{stroke-dasharray:2;}}\
#{id} .marker{{fill:#333333;stroke:#333333;}}\
#{id} .marker.cross{{stroke:#333333;}}\
#{id} svg{{font-family:{ff};font-size:{fs};}}\
#{id} p{{margin:0;}}\
#{id} .venn-title{{font-size:32px;fill:{vtc};font-family:{ff};}}\
#{id} .venn-circle text{{font-size:48px;font-family:{ff};}}\
#{id} .venn-intersection text{{font-size:48px;fill:{vstc};font-family:{ff};}}\
#{id} .venn-text-node{{font-family:{ff};color:{vstc};}}\
#{id} .node .neo-node{{stroke:#9370DB;}}\
#{id} [data-look=\"neo\"].node rect,#{id} [data-look=\"neo\"].cluster rect,#{id} [data-look=\"neo\"].node polygon{{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}\
#{id} [data-look=\"neo\"].node path{{stroke:#9370DB;stroke-width:1px;}}\
#{id} [data-look=\"neo\"].node .outer-path{{filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}\
#{id} [data-look=\"neo\"].node .neo-line path{{stroke:#9370DB;filter:none;}}\
#{id} [data-look=\"neo\"].node circle{{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}\
#{id} [data-look=\"neo\"].node circle .state-start{{fill:#000000;}}\
#{id} [data-look=\"neo\"].icon-shape .icon{{fill:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}\
#{id} [data-look=\"neo\"].icon-shape .icon-neo path{{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}\
#{id} :root{{--mermaid-font-family:{ff};}}</style>",
        id = id,
        ff = font_family,
        fs = font_size,
        tc = text_color,
        vtc = venn_title_color,
        vstc = venn_set_text_color,
    )
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

/// Adjust HSL lightness by `delta` (negative = darken). Mirrors khroma's
/// behaviour: clamp to [0, 100] then round to 10 decimal places.
fn adjust_l(color: &str, delta: f64) -> String {
    if let Some(stripped) = color.strip_prefix("hsl(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = stripped.split(',').map(|p| p.trim()).collect();
        if parts.len() == 3 {
            if let (Ok(h), Some(s), Some(ll)) = (
                parts[0].parse::<f64>(),
                parts[1].strip_suffix('%').and_then(|p| p.parse::<f64>().ok()),
                parts[2].strip_suffix('%').and_then(|p| p.parse::<f64>().ok()),
            ) {
                let new_l = (ll + delta).clamp(0.0, 100.0);
                // V8 Math.round semantics (floor(x+0.5)). Applied here for
                // parity with khroma's lightness rounding.
                let new_l = libm::floor(new_l * 1e10 + 0.5) / 1e10;
                return format!("hsl({h}, {s}%, {nl}%)", h = num(h), s = num(s), nl = num(new_l));
            }
        }
    }
    color.to_string()
}
