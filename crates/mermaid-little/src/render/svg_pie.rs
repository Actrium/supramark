//! Pie SVG renderer — emits the byte-exact output mermaid@11.14.0 produces.
//!
//! The output shape is driven by `pieRenderer.ts` (upstream) and the
//! jsdom-based reference generator at `tests/support/generate_ref.mjs`.
//!
//! This module deliberately builds the SVG as a single `String` via
//! `push_str`/`write!`, rather than any structured SVG library — every
//! attribute order, space and punctuation difference must line up with
//! the reference bytes.

use crate::error::Result;
use crate::layout::pie::PieLayout;
use crate::model::pie::PieDiagram;
use crate::theme::ThemeVariables;

pub fn render(d: &PieDiagram, l: &PieLayout, theme: &ThemeVariables, id: &str) -> Result<String> {
    let mut out = String::with_capacity(8192);

    // ── Opening <svg> ────────────────────────────────────────────────
    let vb = format!(
        "{} {} {} {}",
        fmt_num(l.viewbox_x),
        fmt_num(l.viewbox_y),
        fmt_num(l.viewbox_w),
        fmt_num(l.viewbox_h),
    );

    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" style="max-width: {w}px;" role="graphics-document document" aria-roledescription="pie""#,
        id = id,
        vb = vb,
        w = fmt_num(l.viewbox_w),
    ));

    // ── aria-* + <title>/<desc> ──────────────────────────────────────
    // Order in attributes: aria-describedby BEFORE aria-labelledby
    // (upstream accessibility.ts inserts desc first, then title).
    let a11y_title = d.meta.acc_title.as_deref().unwrap_or("");
    let a11y_descr = d.meta.acc_descr.as_deref().unwrap_or("");
    let has_title = !a11y_title.is_empty();
    let has_descr = !a11y_descr.is_empty();
    if has_descr {
        out.push_str(&format!(r#" aria-describedby="chart-desc-{id}""#));
    }
    if has_title {
        out.push_str(&format!(r#" aria-labelledby="chart-title-{id}""#));
    }
    out.push('>');

    // <title> inserted as first child, then <desc> as first child shoves
    // title to index 1 — net order in markup is <title> then <desc>.
    if has_title {
        out.push_str(&format!(
            r#"<title id="chart-title-{id}">{}</title>"#,
            escape_text(a11y_title)
        ));
    }
    if has_descr {
        out.push_str(&format!(
            r#"<desc id="chart-desc-{id}">{}</desc>"#,
            escape_text(a11y_descr)
        ));
    }

    // ── <style> block ────────────────────────────────────────────────
    out.push_str(&style_block(id, d, theme));

    // ── Content ──────────────────────────────────────────────────────
    out.push_str("<g></g>");
    out.push_str(&format!(
        r#"<g transform="translate({},{})">"#,
        (PIE_CENTER.0 as i64),
        (PIE_CENTER.1 as i64)
    ));

    // Outer circle.
    out.push_str(&format!(
        r#"<circle cx="0" cy="0" r="{}" class="pieOuterCircle"></circle>"#,
        fmt_num(l.outer_circle_r)
    ));

    // Slice paths.
    for s in &l.slices {
        if s.render_slice {
            out.push_str(&format!(
                r#"<path d="{}" fill="{}" class="pieCircle"></path>"#,
                s.path_d, s.fill
            ));
        }
    }

    // Slice labels.
    for s in &l.slices {
        if s.render_slice {
            out.push_str(&format!(
                r#"<text transform="translate({},{})" style="text-anchor: middle;" class="slice">{}</text>"#,
                s.centroid_x, s.centroid_y, s.percent_text
            ));
        }
    }

    // Title text — always emitted, even when empty (upstream calls
    // `append('text').text(db.getDiagramTitle())` unconditionally).
    out.push_str(&format!(
        r#"<text x="0" y="-200" class="pieTitleText">{}</text>"#,
        escape_text(&l.title)
    ));

    // Legend.
    for row in &l.legends {
        out.push_str(&format!(
            r#"<g class="legend" transform="translate({},{})"><rect width="18" height="18" style="fill: {fill}; stroke: {fill};"></rect><text x="22" y="14">{label}</text></g>"#,
            fmt_num(row.dx),
            fmt_num(row.dy),
            fill = row.fill,
            label = escape_text(&row.label_text),
        ));
    }

    out.push_str("</g>");
    out.push_str("</svg>");
    Ok(out)
}

const PIE_CENTER: (f64, f64) = (225.0, 225.0);

/// Emit the minified `<style>` block mermaid produces after stylis.
/// The template is static except for the scoping `#<id>` prefix and a
/// few theme-driven values (font-family, font-size, pie stroke/opacity,
/// pie text color/size, outer stroke width, etc.).
fn style_block(id: &str, d: &PieDiagram, theme: &ThemeVariables) -> String {
    // Theme defaults fallback — these match the upstream default theme.
    let font_family = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\", verdana, arial, sans-serif");
    // stylis minification removes spaces after commas but preserves
    // quoted segments verbatim.
    let font_family_min = minify_font_family(font_family);
    let font_size = theme.font_size.as_deref().unwrap_or("16px");
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let error_bkg = theme.error_bkg_color.as_deref().unwrap_or("#552222");
    let error_text = theme.error_text_color.as_deref().unwrap_or("#552222");
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");
    let stroke_width = theme.stroke_width.unwrap_or(1);
    let pie_stroke = theme.pie_stroke_color.as_deref().unwrap_or("black");
    let pie_stroke_w = theme.pie_stroke_width.as_deref().unwrap_or("2px");
    let pie_opacity = theme.pie_opacity.as_deref().unwrap_or("0.7");
    let pie_outer_stroke = theme.pie_outer_stroke_color.as_deref().unwrap_or("black");
    let pie_title_size = theme.pie_title_text_size.as_deref().unwrap_or("25px");
    let pie_title_color = theme.pie_title_text_color.as_deref().unwrap_or("black");
    let pie_section_color = theme.pie_section_text_color.as_deref().unwrap_or("#333");
    let pie_section_size = theme.pie_section_text_size.as_deref().unwrap_or("17px");
    let pie_legend_color = theme.pie_legend_text_color.as_deref().unwrap_or("black");
    let pie_legend_size = theme.pie_legend_text_size.as_deref().unwrap_or("17px");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let drop_shadow = theme
        .drop_shadow
        .as_deref()
        .unwrap_or("drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))");

    // Outer stroke width: from the diagram's init directive if present,
    // otherwise the theme default.
    let pie_outer_stroke_width = d.outer_stroke_width.as_str();

    // Build the minified CSS in one shot — verbatim to upstream's
    // stylis output. Every space inside this string is significant.
    let mut css = String::with_capacity(3072);
    css.push_str(&format!(
        "<style>#{id}{{font-family:{ff};font-size:{fs};fill:{tc};}}",
        ff = font_family_min,
        fs = font_size,
        tc = text_color,
    ));
    css.push_str("@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}");
    css.push_str("@keyframes dash{to{stroke-dashoffset:0;}}");
    css.push_str(&format!("#{id} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}"));
    css.push_str(&format!("#{id} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}"));
    css.push_str(&format!("#{id} .error-icon{{fill:{error_bkg};}}"));
    css.push_str(&format!(
        "#{id} .error-text{{fill:{error_text};stroke:{error_text};}}"
    ));
    css.push_str(&format!(
        "#{id} .edge-thickness-normal{{stroke-width:{stroke_width}px;}}"
    ));
    css.push_str(&format!(
        "#{id} .edge-thickness-thick{{stroke-width:3.5px;}}"
    ));
    css.push_str(&format!("#{id} .edge-pattern-solid{{stroke-dasharray:0;}}"));
    css.push_str(&format!(
        "#{id} .edge-thickness-invisible{{stroke-width:0;fill:none;}}"
    ));
    css.push_str(&format!(
        "#{id} .edge-pattern-dashed{{stroke-dasharray:3;}}"
    ));
    css.push_str(&format!(
        "#{id} .edge-pattern-dotted{{stroke-dasharray:2;}}"
    ));
    css.push_str(&format!(
        "#{id} .marker{{fill:{line_color};stroke:{line_color};}}"
    ));
    css.push_str(&format!("#{id} .marker.cross{{stroke:{line_color};}}"));
    css.push_str(&format!(
        "#{id} svg{{font-family:{ff};font-size:{fs};}}",
        ff = font_family_min,
        fs = font_size,
    ));
    css.push_str(&format!("#{id} p{{margin:0;}}"));
    css.push_str(&format!(
        "#{id} .pieCircle{{stroke:{pie_stroke};stroke-width:{pie_stroke_w};opacity:{pie_opacity};}}"
    ));
    css.push_str(&format!(
        "#{id} .pieOuterCircle{{stroke:{pie_outer_stroke};stroke-width:{pie_outer_stroke_width};fill:none;}}"
    ));
    css.push_str(&format!("#{id} .pieTitleText{{text-anchor:middle;font-size:{pie_title_size};fill:{pie_title_color};font-family:{ff};}}", ff = font_family_min));
    css.push_str(&format!(
        "#{id} .slice{{font-family:{ff};fill:{pie_section_color};font-size:{pie_section_size};}}",
        ff = font_family_min
    ));
    css.push_str(&format!(
        "#{id} .legend text{{fill:{pie_legend_color};font-family:{ff};font-size:{pie_legend_size};}}",
        ff = font_family_min
    ));
    css.push_str(&format!("#{id} .node .neo-node{{stroke:{node_border};}}"));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node rect,#{id} [data-look="neo"].cluster rect,#{id} [data-look="neo"].node polygon{{stroke:{node_border};filter:{drop_shadow};}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node path{{stroke:{node_border};stroke-width:{stroke_width}px;}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node .outer-path{{filter:{drop_shadow};}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node .neo-line path{{stroke:{node_border};filter:none;}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle{{stroke:{node_border};filter:{drop_shadow};}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle .state-start{{fill:#000000;}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon{{fill:{node_border};filter:{drop_shadow};}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon-neo path{{stroke:{node_border};filter:{drop_shadow};}}"#
    ));
    css.push_str(&format!(
        "#{id} :root{{--mermaid-font-family:{ff};}}",
        ff = font_family_min
    ));
    css.push_str("</style>");
    css
}

/// Strip whitespace after commas **outside** quoted segments, to match
/// what stylis emits for the `font-family` value. Spaces inside
/// double-quoted tokens (e.g. `"trebuchet ms"`) are preserved.
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
                // Skip the single space that stylis would minify.
                prev_comma = false;
                continue;
            }
        }
        out.push(c);
        prev_comma = false;
    }
    out
}

/// HTML/XML text-escape for user-supplied strings (title, labels, desc).
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

/// Format a number like mermaid's `toString()` on a JavaScript Number:
/// integers print without a decimal point, other finite numbers use the
/// shortest round-trip representation (matches Rust's default `{}` for
/// f64, which also emits shortest round-trip).
pub fn fmt_num(x: f64) -> String {
    if x.fract() == 0.0 && x.is_finite() {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

// ── byte-exact integration tests ────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::pie::parse;
    use crate::theme::get_theme;

    fn run(source: &str, id: &str) -> String {
        let d = parse(source).expect("parse");
        let theme = get_theme("default");
        let l = crate::layout::pie::layout(&d, &theme).expect("layout");
        render(&d, &l, &theme, id).expect("render")
    }

    fn first_diff(a: &str, b: &str) -> (usize, String, String) {
        let ab = a.as_bytes();
        let bb = b.as_bytes();
        let min = ab.len().min(bb.len());
        let mut idx = min;
        for i in 0..min {
            if ab[i] != bb[i] {
                idx = i;
                break;
            }
        }
        let window = |s: &str, at: usize| {
            let lo = at.saturating_sub(60);
            let hi = (at + 60).min(s.len());
            s[lo..hi].to_string()
        };
        (idx, window(a, idx), window(b, idx))
    }

    fn assert_match(source: &str, expected: &str, id: &str, label: &str) {
        let got = run(source, id);
        if got != expected {
            let (idx, gw, ew) = first_diff(&got, expected);
            panic!("{label}: mismatch at byte {idx}\n got: ...{gw}...\n exp: ...{ew}...\n",);
        }
    }

    macro_rules! fixture_test {
        ($name:ident, $mmd:literal, $svg:literal, $id:literal) => {
            #[test]
            fn $name() {
                let src = include_str!($mmd);
                let expected = include_str!($svg);
                assert_match(src, expected, $id, stringify!($name));
            }
        };
    }

    fixture_test!(
        fixture_01,
        "../../tests/fixtures/pie/01.mmd",
        "../../tests/reference/fixtures/pie/01.svg",
        "ref-fixtures-pie-01"
    );

    fixture_test!(
        demo_01,
        "../../tests/ext_fixtures/demos/pie/01.mmd",
        "../../tests/reference/ext_fixtures/demos/pie/01.svg",
        "ref-ext-fixtures-demos-pie-01"
    );
    fixture_test!(
        demo_02,
        "../../tests/ext_fixtures/demos/pie/02.mmd",
        "../../tests/reference/ext_fixtures/demos/pie/02.svg",
        "ref-ext-fixtures-demos-pie-02"
    );
    fixture_test!(
        demo_03,
        "../../tests/ext_fixtures/demos/pie/03.mmd",
        "../../tests/reference/ext_fixtures/demos/pie/03.svg",
        "ref-ext-fixtures-demos-pie-03"
    );

    fixture_test!(
        cypress_01,
        "../../tests/ext_fixtures/cypress/pie/01.mmd",
        "../../tests/reference/ext_fixtures/cypress/pie/01.svg",
        "ref-ext-fixtures-cypress-pie-01"
    );
    fixture_test!(
        cypress_02,
        "../../tests/ext_fixtures/cypress/pie/02.mmd",
        "../../tests/reference/ext_fixtures/cypress/pie/02.svg",
        "ref-ext-fixtures-cypress-pie-02"
    );
    fixture_test!(
        cypress_03,
        "../../tests/ext_fixtures/cypress/pie/03.mmd",
        "../../tests/reference/ext_fixtures/cypress/pie/03.svg",
        "ref-ext-fixtures-cypress-pie-03"
    );
    fixture_test!(
        cypress_04,
        "../../tests/ext_fixtures/cypress/pie/04.mmd",
        "../../tests/reference/ext_fixtures/cypress/pie/04.svg",
        "ref-ext-fixtures-cypress-pie-04"
    );
    fixture_test!(
        cypress_05,
        "../../tests/ext_fixtures/cypress/pie/05.mmd",
        "../../tests/reference/ext_fixtures/cypress/pie/05.svg",
        "ref-ext-fixtures-cypress-pie-05"
    );
    fixture_test!(
        cypress_06,
        "../../tests/ext_fixtures/cypress/pie/06.mmd",
        "../../tests/reference/ext_fixtures/cypress/pie/06.svg",
        "ref-ext-fixtures-cypress-pie-06"
    );
    fixture_test!(
        cypress_07,
        "../../tests/ext_fixtures/cypress/pie/07.mmd",
        "../../tests/reference/ext_fixtures/cypress/pie/07.svg",
        "ref-ext-fixtures-cypress-pie-07"
    );
    fixture_test!(
        cypress_08,
        "../../tests/ext_fixtures/cypress/pie/08.mmd",
        "../../tests/reference/ext_fixtures/cypress/pie/08.svg",
        "ref-ext-fixtures-cypress-pie-08"
    );
    fixture_test!(
        cypress_09,
        "../../tests/ext_fixtures/cypress/pie/09.mmd",
        "../../tests/reference/ext_fixtures/cypress/pie/09.svg",
        "ref-ext-fixtures-cypress-pie-09"
    );
    fixture_test!(
        cypress_10,
        "../../tests/ext_fixtures/cypress/pie/10.mmd",
        "../../tests/reference/ext_fixtures/cypress/pie/10.svg",
        "ref-ext-fixtures-cypress-pie-10"
    );
}
