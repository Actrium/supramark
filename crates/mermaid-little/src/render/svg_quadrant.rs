//! Quadrant-chart SVG renderer.
//!
//! Produces SVG text byte-identical to upstream mermaid@11.14.0's output
//! for every fixture in tests/ext_fixtures/{cypress,demos}/quadrant.
//!
//! Upstream pipeline:
//!   - `quadrantRenderer.ts` writes the `<g>` skeleton + element tree.
//!   - `styles.ts` + per-theme generators produce the scoped CSS block.
//!
//! The number formatter mirrors JavaScript's `Number.prototype.toString()`
//! (see the `js_num` function). Float stringification is load-bearing
//! because upstream's `d3.attr()` invokes that exact algorithm on every
//! numeric attribute — the `295.47999999999996` cases in the reference
//! SVGs would turn into `295.48` if we used plain `%g` or naive rounding.

use crate::error::Result;
use crate::layout::quadrant::{QuadrantLayout, QuadrantPointOut, QuadrantText};
use crate::model::quadrant::QuadrantDiagram;
use crate::theme::ThemeVariables;

pub fn render(
    _d: &QuadrantDiagram,
    l: &QuadrantLayout,
    _theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    // The layout carries the already-merged effective theme (baseline +
    // themeVariables overrides). Use that for the CSS block; the raw
    // `_theme` argument is kept in the signature for symmetry with sibling
    // diagrams but is deliberately unused.
    let theme = &l.effective_theme;

    let mut out = String::with_capacity(16 * 1024);

    // --- Root <svg> tag ---------------------------------------------------------------------
    // Attribute order observed in every reference SVG:
    //   id, width, xmlns, style, viewBox, role, aria-roledescription.
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: {w}px;" viewBox="0 0 {w} {h}" role="graphics-document document" aria-roledescription="quadrantChart">"#,
        id = id,
        w = fmt_int(l.chart_width),
        h = fmt_int(l.chart_height),
    ));

    // --- <style> block ----------------------------------------------------------------------
    out.push_str(&build_style_block(id, theme));

    // --- The empty anchor <g></g>. ---
    out.push_str("<g></g>");

    // --- Main group ---
    out.push_str(r#"<g class="main">"#);

    // Sub-groups — emission order matches upstream renderer exactly:
    //   quadrants, border, data-points, labels, title.
    render_quadrants(&mut out, l);
    render_borders(&mut out, l);
    render_data_points(&mut out, l);
    render_labels(&mut out, l);
    render_title(&mut out, l);

    out.push_str("</g></svg>");
    Ok(out)
}

// -------------------------------------------------------------------------------------------------
// Section renderers.
// -------------------------------------------------------------------------------------------------

fn render_quadrants(out: &mut String, l: &QuadrantLayout) {
    out.push_str(r#"<g class="quadrants">"#);
    for q in &l.quadrants {
        out.push_str(r#"<g class="quadrant"><rect "#);
        out.push_str(&format!(
            r#"x="{x}" y="{y}" width="{w}" height="{h}" fill="{f}""#,
            x = fmt_int(q.x),
            y = fmt_int(q.y),
            w = fmt_int(q.width),
            h = fmt_int(q.height),
            f = attr_escape(&q.fill),
        ));
        out.push_str("></rect>");
        write_text(out, &q.text);
        out.push_str("</g>");
    }
    out.push_str("</g>");
}

fn render_borders(out: &mut String, l: &QuadrantLayout) {
    out.push_str(r#"<g class="border">"#);
    for line in &l.border_lines {
        out.push_str(&format!(
            r#"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" style="stroke: {stroke}; stroke-width: {sw};"></line>"#,
            x1 = fmt_int(line.x1),
            y1 = fmt_int(line.y1),
            x2 = fmt_int(line.x2),
            y2 = fmt_int(line.y2),
            stroke = line.stroke,
            sw = fmt_int(line.stroke_width),
        ));
    }
    out.push_str("</g>");
}

fn render_data_points(out: &mut String, l: &QuadrantLayout) {
    out.push_str(r#"<g class="data-points">"#);
    for p in &l.points {
        out.push_str(r#"<g class="data-point">"#);
        render_point_circle(out, p);
        write_text(out, &p.text);
        out.push_str("</g>");
    }
    out.push_str("</g>");
}

fn render_point_circle(out: &mut String, p: &QuadrantPointOut) {
    out.push_str(&format!(
        r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"></circle>"#,
        cx = js_num(p.cx),
        cy = js_num(p.cy),
        r = fmt_int(p.radius),
        fill = attr_escape(&p.fill),
        stroke = attr_escape(&p.stroke),
        sw = attr_escape(&p.stroke_width),
    ));
}

fn render_labels(out: &mut String, l: &QuadrantLayout) {
    out.push_str(r#"<g class="labels">"#);
    for label in &l.axis_labels {
        out.push_str(r#"<g class="label">"#);
        write_text(out, label);
        out.push_str("</g>");
    }
    out.push_str("</g>");
}

fn render_title(out: &mut String, l: &QuadrantLayout) {
    out.push_str(r#"<g class="title">"#);
    if let Some(t) = &l.title {
        write_text(out, t);
    }
    out.push_str("</g>");
}

/// Emit a `<text>` element with upstream's attribute order:
/// `x, y, fill, font-size, dominant-baseline, text-anchor, transform`.
fn write_text(out: &mut String, t: &QuadrantText) {
    out.push_str(&format!(
        r#"<text x="0" y="0" fill="{fill}" font-size="{fs}" dominant-baseline="{db}" text-anchor="{ta}" transform="translate({tx}, {ty}) rotate({rot})">{body}</text>"#,
        fill = attr_escape(&t.fill),
        fs = fmt_int(t.font_size),
        db = t.horizontal_pos.dominant_baseline(),
        ta = t.vertical_pos.text_anchor(),
        tx = js_num(t.x),
        ty = js_num(t.y),
        rot = fmt_int(t.rotation),
        body = html_escape(&t.text),
    ));
}

// -------------------------------------------------------------------------------------------------
// Style block.
// -------------------------------------------------------------------------------------------------

fn build_style_block(id: &str, theme: &ThemeVariables) -> String {
    // Mirror upstream `styles.ts::getStyles`: the fixed boilerplate
    // scoped by `#id`, with the few theme-variable slots the template
    // substitutes in (font-family, font-size, textColor, lineColor,
    // errorBkgColor, errorTextColor, nodeBorder, useGradient,
    // dropShadow). The quadrant diagram itself ships an empty `styles`
    // provider (quadrantDiagram.ts: `styles: () => ''`), so there's no
    // diagram-specific fragment to splice in.

    let font_family = theme
        .font_family
        .clone()
        .unwrap_or_else(|| "\"trebuchet ms\", verdana, arial, sans-serif".to_string());
    // The CSS block strips ` ` after `,` in the font list — mermaid's
    // stylis serializer minifies whitespace there.
    let font_family_compact = minify_commas(&font_family);
    let font_size = theme
        .font_size
        .clone()
        .unwrap_or_else(|| "16px".to_string());
    // Upstream: `options.textColor = themeVariables.textColor`.
    let text_color = theme.text_color.clone().unwrap_or_else(|| "#333".into());
    let line_color = theme.line_color.clone().unwrap_or_else(|| "#333333".into());
    let error_bkg = theme
        .error_bkg_color
        .clone()
        .unwrap_or_else(|| "#552222".into());
    let error_txt = theme
        .error_text_color
        .clone()
        .unwrap_or_else(|| "#552222".into());
    let node_border = theme
        .node_border
        .clone()
        .unwrap_or_else(|| "#9370DB".into());
    let use_gradient = theme.use_gradient.unwrap_or(false);
    let drop_shadow = theme.drop_shadow.clone().unwrap_or_default();
    let stroke_width = theme.stroke_width.unwrap_or(1);

    let mut css = String::with_capacity(4096);

    // 1. Root host rules.
    css.push_str(&format!(
        "#{id}{{font-family:{ff};font-size:{fs};fill:{tc};}}",
        id = id,
        ff = font_family_compact,
        fs = font_size,
        tc = text_color,
    ));
    // 2. Global keyframes.
    css.push_str(
        "@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}\
@keyframes dash{to{stroke-dashoffset:0;}}",
    );
    // 3. Shared boilerplate rules.
    let boilerplate: &[(&str, String)] = &[
        (
            ".edge-animation-slow",
            "stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;"
                .to_string(),
        ),
        (
            ".edge-animation-fast",
            "stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;"
                .to_string(),
        ),
        (".error-icon", format!("fill:{};", error_bkg)),
        (
            ".error-text",
            format!("fill:{};stroke:{};", error_txt, error_txt),
        ),
        (
            ".edge-thickness-normal",
            format!("stroke-width:{}px;", stroke_width),
        ),
        (".edge-thickness-thick", "stroke-width:3.5px;".into()),
        (".edge-pattern-solid", "stroke-dasharray:0;".into()),
        (
            ".edge-thickness-invisible",
            "stroke-width:0;fill:none;".into(),
        ),
        (".edge-pattern-dashed", "stroke-dasharray:3;".into()),
        (".edge-pattern-dotted", "stroke-dasharray:2;".into()),
        (
            ".marker",
            format!("fill:{};stroke:{};", line_color, line_color),
        ),
        (".marker.cross", format!("stroke:{};", line_color)),
    ];
    for (sel, decl) in boilerplate {
        css.push_str(&format!("#{id} {sel}{{{decl}}}"));
    }
    // 4. svg{...} and p{...}.
    css.push_str(&format!(
        "#{id} svg{{font-family:{ff};font-size:{fs};}}",
        id = id,
        ff = font_family_compact,
        fs = font_size,
    ));
    css.push_str(&format!("#{id} p{{margin:0;}}"));

    // 5. Neo-look trailer. Formatting matches upstream stylis output
    //    byte-for-byte, including the drop-shadow whitespace quirks of
    //    the forest theme (`drop-shadow( 1px 2px 2px rgba(185,185,185,0.5))`
    //    vs default's `drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))`).
    let neo_stroke = if use_gradient {
        format!("url(#{id}-gradient)")
    } else {
        node_border.clone()
    };
    let neo_filter = if drop_shadow.is_empty() {
        "none".to_string()
    } else {
        drop_shadow.replace("url(#drop-shadow)", &format!("url({}-drop-shadow)", id))
    };

    css.push_str(&format!(
        "#{id} .node .neo-node{{stroke:{nb};}}",
        nb = node_border
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node rect,#{id} [data-look="neo"].cluster rect,#{id} [data-look="neo"].node polygon{{stroke:{s};filter:{f};}}"#,
        s = neo_stroke,
        f = neo_filter,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node path{{stroke:{s};stroke-width:{sw}px;}}"#,
        s = neo_stroke,
        sw = stroke_width,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node .outer-path{{filter:{f};}}"#,
        f = neo_filter,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node .neo-line path{{stroke:{nb};filter:none;}}"#,
        nb = node_border,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle{{stroke:{s};filter:{f};}}"#,
        s = neo_stroke,
        f = neo_filter,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle .state-start{{fill:#000000;}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon{{fill:{s};filter:{f};}}"#,
        s = neo_stroke,
        f = neo_filter,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon-neo path{{stroke:{s};filter:{f};}}"#,
        s = neo_stroke,
        f = neo_filter,
    ));

    // 6. `:root{--mermaid-font-family:...}`.
    css.push_str(&format!(
        "#{id} :root{{--mermaid-font-family:{ff};}}",
        ff = font_family_compact,
    ));

    format!("<style>{css}</style>")
}

/// Collapse `, ` into `,` inside a CSS value — but leave quoted string
/// literals alone (the `"trebuchet ms"` family name contains a space
/// that must survive).
fn minify_commas(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_quote = false;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' || c == b'\'' {
            in_quote = !in_quote;
            out.push(c as char);
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
        let ch_len = s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        out.push_str(&s[i..i + ch_len]);
        i += ch_len;
    }
    out
}

// -------------------------------------------------------------------------------------------------
// Number / text formatting helpers.
// -------------------------------------------------------------------------------------------------

/// Format a float the way JavaScript's `Number.prototype.toString()`
/// would.
fn js_num(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let abs = v.abs();
    if !(1e-6..1e21).contains(&abs) {
        let s = format!("{:e}", v);
        if let Some(e_pos) = s.find('e') {
            let exp = &s[e_pos + 1..];
            if !exp.starts_with('-') {
                let mut fixed = String::with_capacity(s.len() + 1);
                fixed.push_str(&s[..=e_pos]);
                fixed.push('+');
                fixed.push_str(exp);
                return fixed;
            }
        }
        s
    } else {
        format!("{}", v)
    }
}

/// Render `v` as an integer if it has no fractional part.
fn fmt_int(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        js_num(v)
    }
}

/// Escape reserved SVG attribute chars.
fn attr_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape reserved chars for text nodes (`<text>…</text>`).
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
