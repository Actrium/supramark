//! XY chart SVG renderer — byte-exact port of upstream's
//! `xychartRenderer.ts` (262 LoC) on top of [`crate::layout::xychart`].
//!
//! Responsibilities:
//! 1. Emit the `<svg>` root with upstream attribute ordering (id,
//!    width, xmlns, style, viewBox, role, aria-roledescription).
//! 2. Inline the mermaid boilerplate CSS block, scoped by `#id`.
//! 3. Emit the two leading empty-anchor `<g>`s (root anchor + "main").
//! 4. Walk the pre-laid-out [`DrawableElem`]s and serialise each group
//!    (chart-title, plot, axes) inside its nested `<g>` wrapper.
//! 5. Trailing `<g class="mermaid-tmp-group"></g>`.

use crate::error::Result;
use crate::layout::xychart::{
    DrawableElem, DrawablePath, DrawableRect, DrawableText, TextHorizontalPos, TextVerticalPos,
    XychartLayout,
};
use crate::model::xychart::{ChartOrientation, XychartDiagram};
use crate::theme::ThemeVariables;

/// Public render entry point.
pub fn render(
    d: &XychartDiagram,
    l: &XychartLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(16 * 1024);

    // 1. Root <svg>.
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: {w}px;" viewBox="0 0 {w} {h}" role="graphics-document document" aria-roledescription="xychart">"#,
        id = id,
        w = fmt_int(l.width),
        h = fmt_int(l.height),
    ));

    // 2. <style> block.
    out.push_str(&build_style_block(id, theme));

    // 3. Anchor <g></g>.
    out.push_str("<g></g>");

    // 4. <g class="main">
    out.push_str(r#"<g class="main">"#);

    // 4a. Background rect.
    out.push_str(&format!(
        r#"<rect width="{w}" height="{h}" class="background" fill="{bg}"></rect>"#,
        w = fmt_int(l.width),
        h = fmt_int(l.height),
        bg = &l.background_color,
    ));

    // 4b. Drawable elements — upstream uses getGroup() which re-uses
    // a nested `<g class="X">` tree by prefix. We simulate that by
    // tracking the currently-open path and only closing/opening the
    // tail that changes between two consecutive elements.
    let mut current_path: Vec<&str> = Vec::new();
    for elem in &l.elements {
        let target: &[&str] = elem_groups(elem);
        // Compute the common prefix length.
        let mut common = 0;
        while common < current_path.len()
            && common < target.len()
            && current_path[common] == target[common]
        {
            common += 1;
        }
        // Close everything below the common prefix.
        for _ in common..current_path.len() {
            out.push_str("</g>");
        }
        current_path.truncate(common);
        // Open the remaining segments.
        for g in &target[common..] {
            out.push_str(&format!(r#"<g class="{g}">"#));
            current_path.push(g);
        }
        emit_element_inner(&mut out, elem, d);
    }
    // Close any still-open group stack before `</main>`.
    for _ in 0..current_path.len() {
        out.push_str("</g>");
    }

    out.push_str("</g>"); // close `main`.

    // 5. Trailing mermaid-tmp-group anchor.
    out.push_str(r#"<g class="mermaid-tmp-group"></g></svg>"#);

    Ok(out)
}

// ── Element emission ─────────────────────────────────────────────────

fn elem_groups(elem: &DrawableElem) -> &[&'static str] {
    match elem {
        DrawableElem::Rect { group_texts, .. }
        | DrawableElem::RectWithLabels { group_texts, .. }
        | DrawableElem::Text { group_texts, .. }
        | DrawableElem::Path { group_texts, .. } => group_texts.as_slice(),
    }
}

fn emit_element_inner(out: &mut String, elem: &DrawableElem, d: &XychartDiagram) {
    match elem {
        DrawableElem::Rect { data, .. } => {
            for r in data {
                emit_rect(out, r);
            }
        }
        DrawableElem::RectWithLabels { data, labels, .. } => {
            for r in data {
                emit_rect(out, r);
            }
            if d.config.show_data_label {
                emit_data_labels(out, data, labels, d);
            }
        }
        DrawableElem::Text { data, .. } => {
            for t in data {
                emit_text(out, t);
            }
        }
        DrawableElem::Path { data, .. } => {
            for p in data {
                emit_path(out, p);
            }
        }
    }
}

fn emit_rect(out: &mut String, r: &DrawableRect) {
    out.push_str(&format!(
        r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"></rect>"#,
        x = fmt_num(r.x),
        y = fmt_num(r.y),
        w = fmt_num(r.width),
        h = fmt_num(r.height),
        fill = escape_attr(&r.fill),
        stroke = escape_attr(&r.stroke_fill),
        sw = fmt_num(r.stroke_width),
    ));
}

fn emit_path(out: &mut String, p: &DrawablePath) {
    let fill = p.fill.clone().unwrap_or_else(|| "none".to_string());
    out.push_str(&format!(
        r#"<path d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"></path>"#,
        d = p.path,
        fill = escape_attr(&fill),
        stroke = escape_attr(&p.stroke_fill),
        sw = fmt_num(p.stroke_width),
    ));
}

fn emit_text(out: &mut String, t: &DrawableText) {
    let db = match t.vertical_pos {
        TextVerticalPos::Top => "text-before-edge",
        TextVerticalPos::Middle => "middle",
    };
    let ta = match t.horizontal_pos {
        TextHorizontalPos::Left => "start",
        TextHorizontalPos::Right => "end",
        TextHorizontalPos::Center => "middle",
    };
    out.push_str(&format!(
        r#"<text x="0" y="0" fill="{fill}" font-size="{fs}" dominant-baseline="{db}" text-anchor="{ta}" transform="translate({x}, {y}) rotate({r})">{text}</text>"#,
        fill = escape_attr(&t.fill),
        fs = fmt_int(t.font_size),
        db = db,
        ta = ta,
        x = fmt_num(t.x),
        y = fmt_num(t.y),
        r = fmt_num(t.rotation),
        text = escape_text(&t.text),
    ));
}

// ── Data-label emission (showDataLabel) ──────────────────────────────

fn emit_data_labels(
    out: &mut String,
    rects: &[DrawableRect],
    labels: &[String],
    d: &XychartDiagram,
) {
    use crate::theme;
    let theme_name = d.theme_name.as_deref().unwrap_or("default");
    let theme_xy = theme::get_theme(theme_name)
        .xy_chart
        .clone()
        .unwrap_or_default();
    let color = d
        .theme_override
        .data_label_color
        .clone()
        .or(theme_xy.data_label_color.clone())
        .unwrap_or_else(|| "#000".to_string());
    let _ = color.clone();
    // Filter valid items.
    let valid: Vec<(&DrawableRect, &String)> = rects
        .iter()
        .zip(labels.iter())
        .filter(|(r, _)| r.width > 0.0 && r.height > 0.0)
        .collect();
    if valid.is_empty() {
        return;
    }

    match d.config.chart_orientation {
        ChartOrientation::Horizontal => {
            // charWidthFactor=0.7; rightMargin=10.
            let right_margin = 10.0;
            let char_factor = 0.7;
            let mut candidate_sizes: Vec<f64> = Vec::with_capacity(valid.len());
            for (r, label) in &valid {
                let mut fs = r.height * 0.7;
                while fs > 0.0 {
                    let tw = fs * label.chars().count() as f64 * char_factor;
                    if tw <= r.width - right_margin {
                        break;
                    }
                    fs -= 1.0;
                }
                candidate_sizes.push(fs);
            }
            let uniform = candidate_sizes
                .iter()
                .cloned()
                .fold(f64::INFINITY, f64::min)
                .floor();
            let outside = d.config.show_data_label_outside_bar;
            for (r, label) in &valid {
                let x = if outside {
                    r.x + r.width + right_margin
                } else {
                    r.x + r.width - right_margin
                };
                let y = r.y + r.height / 2.0;
                let anchor = if outside { "start" } else { "end" };
                out.push_str(&format!(
                    r#"<text x="{x}" y="{y}" text-anchor="{anchor}" dominant-baseline="middle" fill="{fill}" font-size="{fs}px">{text}</text>"#,
                    x = fmt_num(x),
                    y = fmt_num(y),
                    anchor = anchor,
                    fill = escape_attr(&color),
                    fs = fmt_int(uniform),
                    text = escape_text(label),
                ));
            }
        }
        ChartOrientation::Vertical => {
            let y_offset = 10.0;
            let char_factor = 0.7;
            let mut candidate_sizes: Vec<f64> = Vec::with_capacity(valid.len());
            for (r, label) in &valid {
                let mut fs = r.width / ((label.chars().count() as f64) * char_factor);
                while fs > 0.0 {
                    let tw = fs * label.chars().count() as f64 * char_factor;
                    let center_x = r.x + r.width / 2.0;
                    let left = center_x - tw / 2.0;
                    let right = center_x + tw / 2.0;
                    let h_fits = left >= r.x && right <= r.x + r.width;
                    let v_fits = r.y + y_offset + fs <= r.y + r.height;
                    if h_fits && v_fits {
                        break;
                    }
                    fs -= 1.0;
                }
                candidate_sizes.push(fs);
            }
            let uniform = candidate_sizes
                .iter()
                .cloned()
                .fold(f64::INFINITY, f64::min)
                .floor();
            let outside = d.config.show_data_label_outside_bar;
            for (r, label) in &valid {
                let x = r.x + r.width / 2.0;
                let y = if outside {
                    r.y - y_offset
                } else {
                    r.y + y_offset
                };
                let baseline = if outside { "auto" } else { "hanging" };
                out.push_str(&format!(
                    r#"<text x="{x}" y="{y}" text-anchor="middle" dominant-baseline="{baseline}" fill="{fill}" font-size="{fs}px">{text}</text>"#,
                    x = fmt_num(x),
                    y = fmt_num(y),
                    baseline = baseline,
                    fill = escape_attr(&color),
                    fs = fmt_int(uniform),
                    text = escape_text(label),
                ));
            }
        }
    }
}

// ── CSS block ────────────────────────────────────────────────────────

fn build_style_block(id: &str, theme: &ThemeVariables) -> String {
    let font_family = theme
        .font_family
        .clone()
        .unwrap_or_else(|| "\"trebuchet ms\", verdana, arial, sans-serif".to_string());
    let font_size = theme
        .font_size
        .clone()
        .unwrap_or_else(|| "16px".to_string());
    // Upstream `styles.ts::getStyles` uses `textColor` for the top
    // `#id { fill: ... }` rule — `titleColor` is only used for
    // diagram-title text. `textColor` is theme-specific: #333 for
    // default, #ccc for dark, etc.
    let primary_text_color = theme
        .text_color
        .clone()
        .unwrap_or_else(|| "#333".to_string());
    // Minify font-family spacing the way upstream's CSS does.
    let font_family_compact = font_family.replace(", ", ",");

    // Error / marker colours come from the theme so dark/forest/etc.
    // pick up the right tones.
    let error_bkg = theme
        .error_bkg_color
        .clone()
        .unwrap_or_else(|| "#552222".to_string());
    let error_text = theme
        .error_text_color
        .clone()
        .unwrap_or_else(|| "#552222".to_string());
    let line_color = theme
        .line_color
        .clone()
        .unwrap_or_else(|| "#333333".to_string());

    let mut css = String::with_capacity(4096);
    css.push_str(&format!(
        "#{id}{{font-family:{ff};font-size:{fs};fill:{tc};}}",
        ff = font_family_compact,
        fs = font_size,
        tc = primary_text_color,
    ));
    css.push_str(concat!(
        "@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}",
        "@keyframes dash{to{stroke-dashoffset:0;}}",
    ));
    let boilerplate: &[(&str, String)] = &[
        (
            ".edge-animation-slow",
            "stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;".to_string(),
        ),
        (
            ".edge-animation-fast",
            "stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;".to_string(),
        ),
        (".error-icon", format!("fill:{error_bkg};")),
        (
            ".error-text",
            format!("fill:{error_text};stroke:{error_text};"),
        ),
        (".edge-thickness-normal", "stroke-width:1px;".to_string()),
        (".edge-thickness-thick", "stroke-width:3.5px;".to_string()),
        (".edge-pattern-solid", "stroke-dasharray:0;".to_string()),
        (
            ".edge-thickness-invisible",
            "stroke-width:0;fill:none;".to_string(),
        ),
        (".edge-pattern-dashed", "stroke-dasharray:3;".to_string()),
        (".edge-pattern-dotted", "stroke-dasharray:2;".to_string()),
        (
            ".marker",
            format!("fill:{line_color};stroke:{line_color};"),
        ),
        (".marker.cross", format!("stroke:{line_color};")),
    ];
    for (sel, decl) in boilerplate {
        css.push_str(&format!("#{id} {sel}{{{decl}}}"));
    }
    css.push_str(&format!(
        "#{id} svg{{font-family:{ff};font-size:{fs};}}",
        ff = font_family_compact,
        fs = font_size,
    ));
    css.push_str(&format!("#{id} p{{margin:0;}}"));
    // Neo-look trailer rules. `node_border` is the ink colour for the
    // neo-look node strokes; `use_gradient` flips the stroke to a
    // per-diagram `<linearGradient>` URL (the gradient element itself
    // is only emitted when this flag is true in upstream — for xychart
    // the gradient is never rendered but the CSS rule still references it).
    let node_border = theme
        .node_border
        .as_deref()
        .unwrap_or("#9370DB")
        .to_string();
    let use_gradient = theme.use_gradient.unwrap_or(false);
    let drop_shadow = theme
        .drop_shadow
        .clone()
        .unwrap_or_else(|| "drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))".to_string());
    let neo_stroke = if use_gradient {
        format!("url(#{id}-gradient)")
    } else {
        node_border.clone()
    };
    let neo_filter = if drop_shadow.is_empty() {
        "none".to_string()
    } else {
        drop_shadow.replace("url(#drop-shadow)", &format!("url({id}-drop-shadow)"))
    };
    css.push_str(&format!(
        "#{id} .node .neo-node{{stroke:{nb};}}",
        nb = node_border,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node rect,#{id} [data-look="neo"].cluster rect,#{id} [data-look="neo"].node polygon{{stroke:{s};filter:{f};}}"#,
        s = neo_stroke,
        f = neo_filter,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node path{{stroke:{s};stroke-width:1px;}}"#,
        s = neo_stroke,
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
    css.push_str(&format!(
        "#{id} :root{{--mermaid-font-family:{ff};}}",
        ff = font_family_compact,
    ));

    format!("<style>{css}</style>")
}

// (Boilerplate rules are built inline inside `build_style_block` so
// they can substitute theme-dependent colour slots.)

// ── Helpers ─────────────────────────────────────────────────────────

fn fmt_num(v: f64) -> String {
    crate::layout::xychart::fmt_num(v)
}

fn fmt_int(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        fmt_num(v)
    }
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::xychart as layout_mod;
    use crate::parser::xychart as parser_mod;
    use crate::theme::get_theme;

    fn render_fixture(source: &str, id: &str) -> String {
        let diagram = parser_mod::parse(source).expect("parse");
        let name = diagram.theme_name.as_deref().unwrap_or("default");
        let theme = get_theme(name);
        let lay = layout_mod::layout(&diagram, &theme).expect("layout");
        super::render(&diagram, &lay, &theme, id).expect("render")
    }

    fn check_fixture(source_path: &str, reference_path: &str, id: &str) {
        let source = std::fs::read_to_string(source_path).expect("source");
        let reference = std::fs::read_to_string(reference_path).expect("reference");
        let got = render_fixture(&source, id);
        let ref_trim = reference.trim_end_matches('\n');
        if got != ref_trim {
            let mut diff_at = 0usize;
            for (i, (a, b)) in got.bytes().zip(ref_trim.bytes()).enumerate() {
                if a != b {
                    diff_at = i;
                    break;
                }
            }
            let ctx = 160usize;
            let start = diff_at.saturating_sub(ctx);
            let end_got = (diff_at + ctx).min(got.len());
            let end_ref = (diff_at + ctx).min(ref_trim.len());
            panic!(
                "byte mismatch for {source_path} at byte {diff_at}\n  got: ...{g}...\n  ref: ...{r}...",
                g = &got[start..end_got],
                r = &ref_trim[start..end_ref],
            );
        }
    }

    #[test]
    fn cypress_01() {
        check_fixture(
            "tests/ext_fixtures/cypress/xychart/01.mmd",
            "tests/reference/ext_fixtures/cypress/xychart/01.svg",
            "ref-ext-fixtures-cypress-xychart-01",
        );
    }

    #[test]
    fn cypress_02() {
        check_fixture(
            "tests/ext_fixtures/cypress/xychart/02.mmd",
            "tests/reference/ext_fixtures/cypress/xychart/02.svg",
            "ref-ext-fixtures-cypress-xychart-02",
        );
    }

    #[test]
    fn cypress_03() {
        check_fixture(
            "tests/ext_fixtures/cypress/xychart/03.mmd",
            "tests/reference/ext_fixtures/cypress/xychart/03.svg",
            "ref-ext-fixtures-cypress-xychart-03",
        );
    }

    #[test]
    fn cypress_04() {
        check_fixture(
            "tests/ext_fixtures/cypress/xychart/04.mmd",
            "tests/reference/ext_fixtures/cypress/xychart/04.svg",
            "ref-ext-fixtures-cypress-xychart-04",
        );
    }

    #[test]
    fn cypress_09() {
        check_fixture(
            "tests/ext_fixtures/cypress/xychart/09.mmd",
            "tests/reference/ext_fixtures/cypress/xychart/09.svg",
            "ref-ext-fixtures-cypress-xychart-09",
        );
    }

    #[test]
    fn cypress_19() {
        check_fixture(
            "tests/ext_fixtures/cypress/xychart/19.mmd",
            "tests/reference/ext_fixtures/cypress/xychart/19.svg",
            "ref-ext-fixtures-cypress-xychart-19",
        );
    }
}
