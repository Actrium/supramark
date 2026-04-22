//! Radar SVG renderer.
//!
//! Produces SVG text that is byte-identical to upstream
//! mermaid@11.14.0's output for every fixture in
//! tests/ext_fixtures/{cypress,demos}/radar.
//!
//! The upstream renderer lives at
//! /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/radar/renderer.ts
//! and writes coordinates through `d3.attr()`, which stringifies
//! numbers with JavaScript's `Number.prototype.toString()`. Rust's
//! default `f64::Display` agrees with that for all values except those
//! with `|v| < 1e-6`, which JS prints in scientific notation while Rust
//! prints as fixed-point decimals. We therefore use a bespoke
//! [`js_num`] formatter on every numeric attribute.

use crate::error::Result;
use crate::layout::radar::{
    RadarLayout, RADAR_CURVE_TENSION, RADAR_HEIGHT, RADAR_MARGIN_BOTTOM, RADAR_MARGIN_LEFT,
    RADAR_MARGIN_RIGHT, RADAR_MARGIN_TOP, RADAR_WIDTH,
};
use crate::model::radar::{Graticule, RadarDiagram};
use crate::theme::{RadarVars, ThemeVariables};

/// Render a radar diagram to an SVG string.
pub fn render(
    d: &RadarDiagram,
    l: &RadarLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(8192);

    // --- Root <svg> tag ----------------------------------------------------------------------
    // Attribute order matches upstream: id, width, xmlns, style, viewBox, role, aria-roledescription.
    let total_width = RADAR_WIDTH + RADAR_MARGIN_LEFT + RADAR_MARGIN_RIGHT;
    let total_height = RADAR_HEIGHT + RADAR_MARGIN_TOP + RADAR_MARGIN_BOTTOM;
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: {w}px;" viewBox="0 0 {w} {h}" role="graphics-document document" aria-roledescription="radar">"#,
        id = id,
        w = fmt_int(total_width),
        h = fmt_int(total_height),
    ));

    // --- <style> block -----------------------------------------------------------------------
    out.push_str(&build_style_block(id, theme));

    // --- Empty <g></g>: upstream always emits one anchor <g> before the main group. ----------
    out.push_str("<g></g>");

    // --- Main centred group ------------------------------------------------------------------
    out.push_str(&format!(
        r#"<g transform="translate({cx}, {cy})">"#,
        cx = fmt_int(l.cx),
        cy = fmt_int(l.cy),
    ));

    // --- Graticule ---------------------------------------------------------------------------
    render_graticule(&mut out, d, l);

    // --- Axes --------------------------------------------------------------------------------
    render_axes(&mut out, d, l);

    // --- Curves ------------------------------------------------------------------------------
    render_curves(&mut out, d, l);

    // --- Legend ------------------------------------------------------------------------------
    if d.options.show_legend {
        render_legend(&mut out, d, l);
    }

    // --- Title -------------------------------------------------------------------------------
    let title = d.meta.title.as_deref().unwrap_or("");
    out.push_str(&format!(
        r#"<text class="radarTitle" x="0" y="{y}">{t}</text>"#,
        y = js_num(l.title_y),
        t = html_escape(title),
    ));

    out.push_str("</g></svg>");
    Ok(out)
}

// -------------------------------------------------------------------------------------------------
// Section renderers.
// -------------------------------------------------------------------------------------------------

fn render_graticule(out: &mut String, d: &RadarDiagram, l: &RadarLayout) {
    match d.options.graticule {
        Graticule::Circle => {
            for r in &l.graticule_radii {
                out.push_str(&format!(
                    r#"<circle r="{r}" class="radarGraticule"></circle>"#,
                    r = js_num(*r),
                ));
            }
        }
        Graticule::Polygon => {
            let num_axes = d.axes.len();
            for r in &l.graticule_radii {
                let points: Vec<String> = (0..num_axes)
                    .map(|j| {
                        let angle = 2.0 * j as f64 * std::f64::consts::PI / num_axes as f64
                            - std::f64::consts::PI / 2.0;
                        let x = r * angle.cos();
                        let y = r * angle.sin();
                        format!("{},{}", js_num(x), js_num(y))
                    })
                    .collect();
                out.push_str(&format!(
                    r#"<polygon points="{p}" class="radarGraticule"></polygon>"#,
                    p = points.join(" "),
                ));
            }
        }
    }
}

fn render_axes(out: &mut String, d: &RadarDiagram, l: &RadarLayout) {
    for (i, axis) in d.axes.iter().enumerate() {
        let (x2, y2) = l.axes_endpoints[i];
        let (lx, ly) = l.axes_label_positions[i];
        out.push_str(&format!(
            r#"<line x1="0" y1="0" x2="{x2}" y2="{y2}" class="radarAxisLine"></line>"#,
            x2 = js_num(x2),
            y2 = js_num(y2),
        ));
        out.push_str(&format!(
            r#"<text x="{lx}" y="{ly}" class="radarAxisLabel">{label}</text>"#,
            lx = js_num(lx),
            ly = js_num(ly),
            label = html_escape(&axis.label),
        ));
    }
}

fn render_curves(out: &mut String, d: &RadarDiagram, l: &RadarLayout) {
    match d.options.graticule {
        Graticule::Polygon => {
            for curve in &l.curves {
                let points: Vec<String> = curve
                    .points
                    .iter()
                    .map(|(x, y)| format!("{},{}", js_num(*x), js_num(*y)))
                    .collect();
                out.push_str(&format!(
                    r#"<polygon points="{p}" class="radarCurve-{idx}"></polygon>"#,
                    p = points.join(" "),
                    idx = curve.source_index,
                ));
            }
        }
        Graticule::Circle => {
            for curve in &l.curves {
                let d_attr = closed_round_curve(&curve.points, RADAR_CURVE_TENSION);
                out.push_str(&format!(
                    r#"<path d="{d}" class="radarCurve-{idx}"></path>"#,
                    d = d_attr,
                    idx = curve.source_index,
                ));
            }
        }
    }
}

fn render_legend(out: &mut String, d: &RadarDiagram, l: &RadarLayout) {
    let (lx, ly) = l.legend_origin;
    const LINE_HEIGHT: f64 = 20.0;
    for (ord, curve) in l.curves.iter().enumerate() {
        let y = ly + (ord as f64) * LINE_HEIGHT;
        let label = d
            .curves
            .get(curve.source_index)
            .map(|c| c.label.as_str())
            .unwrap_or("");
        out.push_str(&format!(
            r#"<g transform="translate({x}, {y})"><rect width="12" height="12" class="radarLegendBox-{idx}"></rect><text x="16" y="0" class="radarLegendText">{label}</text></g>"#,
            x = js_num(lx),
            y = js_num(y),
            idx = curve.source_index,
            label = html_escape(label),
        ));
    }
}

// -------------------------------------------------------------------------------------------------
// Bezier curve for circle-graticule mode.
// -------------------------------------------------------------------------------------------------

/// Produce the `d` attribute for a closed Catmull-Rom spline through
/// `points`, using the upstream `closedRoundCurve` formula exactly.
fn closed_round_curve(points: &[(f64, f64)], tension: f64) -> String {
    let n = points.len();
    let mut d = String::new();
    d.push_str(&format!("M{},{}", js_num(points[0].0), js_num(points[0].1)));
    for i in 0..n {
        let p0 = points[(i + n - 1) % n];
        let p1 = points[i];
        let p2 = points[(i + 1) % n];
        let p3 = points[(i + 2) % n];
        let cp1x = p1.0 + (p2.0 - p0.0) * tension;
        let cp1y = p1.1 + (p2.1 - p0.1) * tension;
        let cp2x = p2.0 - (p3.0 - p1.0) * tension;
        let cp2y = p2.1 - (p3.1 - p1.1) * tension;
        d.push_str(&format!(
            " C{},{} {},{} {},{}",
            js_num(cp1x),
            js_num(cp1y),
            js_num(cp2x),
            js_num(cp2y),
            js_num(p2.0),
            js_num(p2.1),
        ));
    }
    d.push_str(" Z");
    d
}

// -------------------------------------------------------------------------------------------------
// CSS <style> block.
// -------------------------------------------------------------------------------------------------

fn build_style_block(id: &str, theme: &ThemeVariables) -> String {
    // This block is a byte-exact reproduction of the text upstream's
    // style pipeline produces for a default-theme radar diagram. The
    // style template combines:
    //  - mermaid's fixed boilerplate from `getStyles.ts`
    //  - the radar-specific fragment in `radar/styles.ts`
    //  - generated per-index `.radarCurve-N` / `.radarLegendBox-N`
    //    selectors driven by `THEME_COLOR_LIMIT` (12 for default).
    //
    // Reproducing this verbatim (rather than synthesising from theme
    // variables) is the only practical way to match upstream's
    // whitespace / attribute ordering / trailing semicolon habits;
    // upstream itself hard-codes the template string.

    let font_family = theme_str(
        &theme.font_family,
        "\"trebuchet ms\", verdana, arial, sans-serif",
    );
    let font_size = theme_str(&theme.font_size, "16px");
    let primary_text_color = theme_str(&theme.title_color, "#333");

    let radar = theme.radar.clone().unwrap_or_default();
    let axis_color = radar_str(&radar, "axis_color", "#333333");
    let axis_stroke_width = radar_int(&radar.axis_stroke_width, 2);
    let axis_label_font_size = radar_int(&radar.axis_label_font_size, 12);
    let graticule_color = radar_str(&radar, "graticule_color", "#DEDEDE");
    let graticule_opacity = radar_float(&radar.graticule_opacity, 0.3);
    let graticule_stroke_width = radar_int(&radar.graticule_stroke_width, 1);
    let legend_font_size = radar_int(&radar.legend_font_size, 12);
    let curve_opacity = radar_float(&radar.curve_opacity, 0.5);
    let curve_stroke_width = radar_int(&radar.curve_stroke_width, 2);

    // Compress font_family — upstream minifies `"trebuchet ms", verdana, arial, sans-serif`
    // to `"trebuchet ms",verdana,arial,sans-serif` inside the style block.
    let font_family_compact = font_family.replace(", ", ",");

    let theme_color_limit = theme.theme_color_limit.unwrap_or(12) as usize;

    let mut css = String::with_capacity(6144);
    // Root host rules (font + colour defaults).
    css.push_str(&format!(
        "#{id}{{font-family:{ff};font-size:{fs};fill:{tc};}}",
        id = id,
        ff = font_family_compact,
        fs = font_size,
        tc = primary_text_color,
    ));
    // Global keyframes + shared mermaid boilerplate.
    css.push_str(concat!(
        "@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}",
        "@keyframes dash{to{stroke-dashoffset:0;}}",
    ));
    // Edge / error / thickness / marker rules — upstream's boilerplate
    // from `getStyles.ts`, all scoped by #id.
    for (sel, decl) in BOILERPLATE_RULES {
        css.push_str(&format!("#{id} {sel}{{{decl}}}"));
    }
    css.push_str(&format!(
        "#{id} svg{{font-family:{ff};font-size:{fs};}}",
        id = id,
        ff = font_family_compact,
        fs = font_size,
    ));
    css.push_str(&format!("#{id} p{{margin:0;}}"));

    // Radar-specific rules.
    css.push_str(&format!(
        "#{id} .radarTitle{{font-size:{fs};color:{tc};dominant-baseline:hanging;text-anchor:middle;}}",
        fs = font_size,
        tc = primary_text_color,
    ));
    css.push_str(&format!(
        "#{id} .radarAxisLine{{stroke:{axis};stroke-width:{sw};}}",
        axis = axis_color,
        sw = axis_stroke_width,
    ));
    css.push_str(&format!(
        "#{id} .radarAxisLabel{{dominant-baseline:middle;text-anchor:middle;font-size:{alfs}px;color:{axis};}}",
        alfs = axis_label_font_size,
        axis = axis_color,
    ));
    css.push_str(&format!(
        "#{id} .radarGraticule{{fill:{gc};fill-opacity:{go};stroke:{gc};stroke-width:{gsw};}}",
        gc = graticule_color,
        go = fmt_css_float(graticule_opacity),
        gsw = graticule_stroke_width,
    ));
    css.push_str(&format!(
        "#{id} .radarLegendText{{text-anchor:start;font-size:{lfs}px;dominant-baseline:hanging;}}",
        lfs = legend_font_size,
    ));
    // Generated per-index color rules.
    for i in 0..theme_color_limit {
        let color = c_scale(theme, i).unwrap_or_else(|| "#000".to_string());
        css.push_str(&format!(
            "#{id} .radarCurve-{i}{{color:{c};fill:{c};fill-opacity:{co};stroke:{c};stroke-width:{csw};}}",
            i = i,
            c = color,
            co = fmt_css_float(curve_opacity),
            csw = curve_stroke_width,
        ));
        css.push_str(&format!(
            "#{id} .radarLegendBox-{i}{{fill:{c};fill-opacity:{co};stroke:{c};}}",
            i = i,
            c = color,
            co = fmt_css_float(curve_opacity),
        ));
    }

    // Neo-look trailer rules — upstream's fixed block that ships with every
    // diagram regardless of look.
    css.push_str(&format!(
        "#{id} .node .neo-node{{stroke:#9370DB;}}\
#{id} [data-look=\"neo\"].node rect,\
#{id} [data-look=\"neo\"].cluster rect,\
#{id} [data-look=\"neo\"].node polygon{{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}\
#{id} [data-look=\"neo\"].node path{{stroke:#9370DB;stroke-width:1px;}}\
#{id} [data-look=\"neo\"].node .outer-path{{filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}\
#{id} [data-look=\"neo\"].node .neo-line path{{stroke:#9370DB;filter:none;}}\
#{id} [data-look=\"neo\"].node circle{{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}\
#{id} [data-look=\"neo\"].node circle .state-start{{fill:#000000;}}\
#{id} [data-look=\"neo\"].icon-shape .icon{{fill:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}\
#{id} [data-look=\"neo\"].icon-shape .icon-neo path{{stroke:#9370DB;filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}"
    ));
    // Finally the `:root` font variable.
    css.push_str(&format!(
        "#{id} :root{{--mermaid-font-family:{ff};}}",
        id = id,
        ff = font_family_compact,
    ));

    format!("<style>{css}</style>")
}

/// Fixed boilerplate rules that upstream `getStyles.ts` always prepends.
/// `(selector, declaration-body)` tuples, each scoped by `#id` at render
/// time to match mermaid's per-diagram CSS sandbox.
const BOILERPLATE_RULES: &[(&str, &str)] = &[
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
    (".marker", "fill:#333333;stroke:#333333;"),
    (".marker.cross", "stroke:#333333;"),
];

// -------------------------------------------------------------------------------------------------
// Theme helpers.
// -------------------------------------------------------------------------------------------------

fn theme_str(slot: &Option<String>, fallback: &str) -> String {
    slot.clone().unwrap_or_else(|| fallback.to_string())
}

fn radar_str(r: &RadarVars, field: &str, fallback: &str) -> String {
    let v = match field {
        "axis_color" => r.axis_color.clone(),
        "graticule_color" => r.graticule_color.clone(),
        _ => None,
    };
    v.unwrap_or_else(|| fallback.to_string())
}

fn radar_int(slot: &Option<i64>, fallback: i64) -> i64 {
    slot.unwrap_or(fallback)
}

fn radar_float(slot: &Option<f64>, fallback: f64) -> f64 {
    slot.unwrap_or(fallback)
}

fn c_scale(theme: &ThemeVariables, i: usize) -> Option<String> {
    match i {
        0 => theme.c_scale0.clone(),
        1 => theme.c_scale1.clone(),
        2 => theme.c_scale2.clone(),
        3 => theme.c_scale3.clone(),
        4 => theme.c_scale4.clone(),
        5 => theme.c_scale5.clone(),
        6 => theme.c_scale6.clone(),
        7 => theme.c_scale7.clone(),
        8 => theme.c_scale8.clone(),
        9 => theme.c_scale9.clone(),
        10 => theme.c_scale10.clone(),
        11 => theme.c_scale11.clone(),
        12 => theme.c_scale12.clone(),
        _ => None,
    }
}

// -------------------------------------------------------------------------------------------------
// Number formatters.
// -------------------------------------------------------------------------------------------------

/// Format a floating-point value for an SVG numeric attribute using the
/// same algorithm as JavaScript's `Number.prototype.toString()`.
///
/// Rules:
/// * `+0.0` / `-0.0` → `"0"`.
/// * `|v| >= 1e21` → scientific (`e+NN`).
/// * `0 < |v| < 1e-6` → scientific (`e-NN`).
/// * otherwise → Rust's default `Display` (which matches JS in this range).
///
/// The scientific form itself is produced via Rust's `{:e}`, which
/// already matches JS's `1e-7` / `6.123233995736766e-15` formatting (no
/// `1.0e-7` variant) for the values we care about.
fn js_num(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let abs = v.abs();
    if !(1e-6..1e21).contains(&abs) {
        // Use scientific notation with `e+NN` for positive exponents,
        // `e-NN` (already default) for negative ones.
        let s = format!("{:e}", v);
        // Rust's `{:e}` writes the exponent with a leading `-` or plain
        // digits (no `+`). JS uses `e+N` for positive exponents.
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
        // `{}` for f64 in modern Rust matches JS for non-scientific ranges.
        format!("{}", v)
    }
}

/// Format a value that is guaranteed to be a non-negative integer in
/// disguise (width / height coming from our config). Render as an
/// integer if the fractional part is zero, otherwise fall back to
/// [`js_num`].
fn fmt_int(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        js_num(v)
    }
}

/// Numeric formatter for a CSS declaration value (i.e., `fill-opacity`).
/// These values are always finite floats in `[0, 1]`; we want `"0.5"`
/// rather than `".5"` and drop a trailing `.0` like JS would.
fn fmt_css_float(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

// -------------------------------------------------------------------------------------------------
// HTML/XML escaping.
// -------------------------------------------------------------------------------------------------

/// Minimal escape for text content embedded in SVG `<text>` nodes.
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

// -------------------------------------------------------------------------------------------------
// Tests — byte-exact parity against every reference fixture.
// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::radar as layout_mod;
    use crate::parser::radar as parser_mod;
    use crate::theme::get_theme;

    fn render_fixture(source: &str, id: &str) -> String {
        let diagram = parser_mod::parse(source).expect("parse");
        let theme = get_theme("default");
        let lay = layout_mod::layout(&diagram, &theme).expect("layout");
        super::render(&diagram, &lay, &theme, id).expect("render")
    }

    fn check_fixture(source_path: &str, reference_path: &str, id: &str) {
        let source = std::fs::read_to_string(source_path).expect("source");
        let reference = std::fs::read_to_string(reference_path).expect("reference");
        let got = render_fixture(&source, id);
        if got != reference.trim_end_matches('\n') {
            // Show a helpful diff pointer on failure.
            let got_len = got.len();
            let ref_len = reference.len();
            let mut diff_at = 0;
            for (i, (a, b)) in got.bytes().zip(reference.bytes()).enumerate() {
                if a != b {
                    diff_at = i;
                    break;
                }
            }
            let ctx = 120usize;
            let start = diff_at.saturating_sub(ctx);
            let end_got = (diff_at + ctx).min(got_len);
            let end_ref = (diff_at + ctx).min(ref_len);
            panic!(
                "byte mismatch for {source_path} at byte {diff_at}\n  got: ...{g}...\n  ref: ...{r}...",
                g = &got[start..end_got],
                r = &reference[start..end_ref],
            );
        }
    }

    #[test]
    fn js_num_scientific_small() {
        assert_eq!(js_num(6.123233995736766e-15), "6.123233995736766e-15");
        assert_eq!(js_num(1.8369701987210297e-14), "1.8369701987210297e-14");
        assert_eq!(js_num(0.0), "0");
        assert_eq!(js_num(-0.0), "0");
    }

    #[test]
    fn js_num_decimal_range() {
        assert_eq!(js_num(259.8076211353316), "259.8076211353316");
        assert_eq!(js_num(-315.0), "-315");
        assert_eq!(js_num(262.5), "262.5");
    }

    #[test]
    fn cypress_radar_01() {
        check_fixture(
            "tests/ext_fixtures/cypress/radar/01.mmd",
            "tests/reference/ext_fixtures/cypress/radar/01.svg",
            "ref-ext-fixtures-cypress-radar-01",
        );
    }

    #[test]
    fn cypress_radar_02() {
        check_fixture(
            "tests/ext_fixtures/cypress/radar/02.mmd",
            "tests/reference/ext_fixtures/cypress/radar/02.svg",
            "ref-ext-fixtures-cypress-radar-02",
        );
    }

    #[test]
    fn cypress_radar_03() {
        check_fixture(
            "tests/ext_fixtures/cypress/radar/03.mmd",
            "tests/reference/ext_fixtures/cypress/radar/03.svg",
            "ref-ext-fixtures-cypress-radar-03",
        );
    }

    #[test]
    fn cypress_radar_04() {
        check_fixture(
            "tests/ext_fixtures/cypress/radar/04.mmd",
            "tests/reference/ext_fixtures/cypress/radar/04.svg",
            "ref-ext-fixtures-cypress-radar-04",
        );
    }

    #[test]
    fn cypress_radar_05() {
        check_fixture(
            "tests/ext_fixtures/cypress/radar/05.mmd",
            "tests/reference/ext_fixtures/cypress/radar/05.svg",
            "ref-ext-fixtures-cypress-radar-05",
        );
    }

    #[test]
    fn cypress_radar_06() {
        check_fixture(
            "tests/ext_fixtures/cypress/radar/06.mmd",
            "tests/reference/ext_fixtures/cypress/radar/06.svg",
            "ref-ext-fixtures-cypress-radar-06",
        );
    }

    #[test]
    fn demos_radar_01() {
        check_fixture(
            "tests/ext_fixtures/demos/radar/01.mmd",
            "tests/reference/ext_fixtures/demos/radar/01.svg",
            "ref-ext-fixtures-demos-radar-01",
        );
    }
}
