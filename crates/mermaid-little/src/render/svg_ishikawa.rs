//! Ishikawa (fishbone) SVG renderer.
//!
//! Produces byte-identical output to upstream mermaid@11.14.0 for every
//! fixture in `tests/ext_fixtures/{cypress,demos}/ishikawa`. The
//! `handDrawn` look (`%%{init: { 'look': 'handDrawn' } }%%`) routes
//! through the [`hand_drawn`] sub-module, which mirrors upstream's
//! rough.js call sequence so the LCG state advances bit-for-bit
//! identically.

use crate::error::Result;
use crate::layout::ishikawa::{Branch, IshikawaLayout, Pair, SubBranch};
use crate::model::ishikawa::IshikawaDiagram;
use crate::theme::ThemeVariables;

#[path = "svg_ishikawa_hand_drawn.rs"]
mod hand_drawn;

pub fn render(
    d: &IshikawaDiagram,
    l: &IshikawaLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    if matches!(d.look.as_deref(), Some("handDrawn")) {
        return hand_drawn::render(d, l, theme, id);
    }
    let _ = d; // the diagram model is fully digested by the layout.
    let mut out = String::with_capacity(8192);

    let (vx, vy, vw, vh) = l.viewbox;

    // ── Root <svg> tag ───────────────────────────────────────────────
    // Attribute order matches upstream: id, width, xmlns, style,
    // viewBox, role, aria-roledescription.
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: {mw}px;" viewBox="{vx} {vy} {vw} {vh}" role="graphics-document document" aria-roledescription="ishikawa">"#,
        id = id,
        mw = js_num(vw),
        vx = js_num(vx),
        vy = js_num(vy),
        vw = js_num(vw),
        vh = js_num(vh),
    ));

    // ── <style> block ────────────────────────────────────────────────
    out.push_str(&build_style_block(id, theme));

    // ── Seed empty group — upstream always emits one before main g. ──
    out.push_str("<g></g>");

    // ── Main ishikawa group ──────────────────────────────────────────
    out.push_str(r#"<g class="ishikawa">"#);

    if !l.has_root {
        out.push_str("</g></svg>");
        return Ok(out);
    }

    // Arrow marker defs.
    out.push_str(&format!(
        r##"<defs><marker id="ishikawa-arrow-{id}" viewBox="0 0 10 10" refX="0" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="M 10 0 L 0 5 L 10 10 Z" class="ishikawa-arrow"></path></marker></defs>"##,
        id = id,
    ));

    // Spine line — initial coordinates are `(0, spine_y) → (0, spine_y)`,
    // then `x1` is patched to `spine_x_left` (if any pair exists).
    out.push_str(&format!(
        r#"<line class="ishikawa-spine" x1="{x1}" y1="{y1}" x2="0" y2="{y1}"></line>"#,
        x1 = js_num(l.spine_x_left),
        y1 = js_num(l.spine_y),
    ));

    // Head group.
    render_head(&mut out, l);

    // Pairs.
    let marker_ref = format!("url(#ishikawa-arrow-{id})");
    for pair in &l.pairs {
        render_pair(&mut out, pair, &marker_ref);
    }

    out.push_str("</g></svg>");
    Ok(out)
}

fn render_head(out: &mut String, l: &IshikawaLayout) {
    out.push_str(&format!(
        r#"<g class="ishikawa-head-group" transform="translate(0,{y})">"#,
        y = js_num(l.spine_y),
    ));
    out.push_str(&format!(
        r#"<path class="ishikawa-head" d="M 0 {a} L 0 {b} Q {q} 0 0 {a} Z"></path>"#,
        a = js_num(-l.head_h / 2.0),
        b = js_num(l.head_h / 2.0),
        q = js_num(l.head_w * 2.4),
    ));
    // Head text.
    out.push_str(&format!(
        r#"<text class="ishikawa-head-label" text-anchor="start" x="0" y="{y}" transform="translate({tx},{ty})">"#,
        y = js_num(l.head_text_y),
        tx = js_num(l.head_text_x_shift),
        ty = js_num(l.head_text_y_shift),
    ));
    for (i, line) in l.head_text_lines.iter().enumerate() {
        let dy = if i == 0 { 0.0 } else { l.head_text_dy };
        out.push_str(&format!(
            r#"<tspan x="0" dy="{dy}">{t}</tspan>"#,
            dy = js_num(dy),
            t = html_escape(line),
        ));
    }
    out.push_str("</text></g>");
}

fn render_pair(out: &mut String, pair: &Pair, marker: &str) {
    out.push_str(r#"<g class="ishikawa-pair">"#);
    if let Some(b) = &pair.upper {
        render_branch(out, b, marker);
    }
    if let Some(b) = &pair.lower {
        render_branch(out, b, marker);
    }
    out.push_str("</g>");
}

fn render_branch(out: &mut String, b: &Branch, marker: &str) {
    // Cause branch line.
    let (x0, y0) = b.start;
    let (x1, y1) = b.end;
    out.push_str(&format!(
        r#"<line class="ishikawa-branch" x1="{x0}" y1="{y0}" x2="{x1}" y2="{y1}" marker-start="{m}"></line>"#,
        x0 = js_num(x0),
        y0 = js_num(y0),
        x1 = js_num(x1),
        y1 = js_num(y1),
        m = marker,
    ));

    // Cause label group — rect followed by text (the rect is `insert`'d
    // BEFORE the text, matching upstream `.insert('rect', ':first-child')`).
    out.push_str(r#"<g class="ishikawa-label-group">"#);
    let (rx, ry, rw, rh) = b.label_rect;
    out.push_str(&format!(
        r#"<rect class="ishikawa-label-box" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
        x = js_num(rx),
        y = js_num(ry),
        w = js_num(rw),
        h = js_num(rh),
    ));
    out.push_str(&format!(
        r#"<text class="ishikawa-label cause" text-anchor="middle" x="{x}" y="{y}">"#,
        x = js_num(b.label_text_x),
        y = js_num(b.label_text_y),
    ));
    for (i, line) in b.label_text.iter().enumerate() {
        let dy = if i == 0 { 0.0 } else { b.label_text_dy };
        out.push_str(&format!(
            r#"<tspan x="{x}" dy="{dy}">{t}</tspan>"#,
            x = js_num(b.label_text_x),
            dy = js_num(dy),
            t = html_escape(line),
        ));
    }
    out.push_str("</text></g>");

    // Sub-branches.
    for sb in &b.sub_branches {
        render_sub(out, sb, marker);
    }
}

fn render_sub(out: &mut String, sb: &SubBranch, marker: &str) {
    let (x1, y1, x2, y2) = sb.line;
    out.push_str(r#"<g class="ishikawa-sub-group">"#);
    out.push_str(&format!(
        r#"<line class="ishikawa-sub-branch" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" marker-start="{m}"></line>"#,
        x1 = js_num(x1),
        y1 = js_num(y1),
        x2 = js_num(x2),
        y2 = js_num(y2),
        m = marker,
    ));
    out.push_str(&format!(
        r#"<text class="{cls}" text-anchor="end" x="{x}" y="{y}">"#,
        cls = sb.text_class,
        x = js_num(sb.text_x),
        y = js_num(sb.text_y),
    ));
    for (i, line) in sb.text_lines.iter().enumerate() {
        let dy = if i == 0 { 0.0 } else { sb.text_dy };
        out.push_str(&format!(
            r#"<tspan x="{x}" dy="{dy}">{t}</tspan>"#,
            x = js_num(sb.text_x),
            dy = js_num(dy),
            t = html_escape(line),
        ));
    }
    out.push_str("</text></g>");
}

// ── Style block ────────────────────────────────────────────────────

pub(super) fn build_style_block(id: &str, theme: &ThemeVariables) -> String {
    // This is the boilerplate CSS mermaid injects before the per-diagram
    // rules from `ishikawaStyles.ts`. Output must be byte-identical to
    // the upstream pipeline — whitespace, attribute order, trailing
    // semicolons included.

    let font_family = theme
        .font_family
        .clone()
        .unwrap_or_else(|| r#""trebuchet ms", verdana, arial, sans-serif"#.to_string());
    let font_size = theme.font_size.clone().unwrap_or_else(|| "16px".into());
    // `fill` for root host rule: upstream reads textColor (default "#333",
    // forest "#000000").
    let text_color = theme.text_color.clone().unwrap_or_else(|| "#333".into());

    let line_color = theme.line_color.clone().unwrap_or_else(|| "#333333".into());
    let main_bkg = theme.main_bkg.clone().unwrap_or_else(|| "#ECECFF".into());
    let primary_color = theme
        .primary_color
        .clone()
        .unwrap_or_else(|| "#ECECFF".into());
    let _ = primary_color; // stylesheet uses mainBkg, not primaryColor.

    // Neo trailer — upstream hardcodes "#9370DB" for default / dark /
    // neutral themes and "#13540c" for forest. This is NOT driven by
    // any theme variable; getStyles.ts writes it literally. Select via
    // `useGradient` (set only on forest in our fixtures).
    let use_gradient = theme.use_gradient.unwrap_or(false);
    let neo_stroke: String = if use_gradient {
        "#13540c".into()
    } else {
        "#9370DB".into()
    };

    // Font-family minification: drop spaces after commas OUTSIDE
    // quotes. Mermaid's CSS serialiser does this when it emits the
    // <style> block.
    let font_family_compact = compact_font_family(&font_family);

    let mut css = String::with_capacity(6144);
    // Root host rule.
    css.push_str(&format!(
        "#{id}{{font-family:{ff};font-size:{fs};fill:{tc};}}",
        id = id,
        ff = font_family_compact,
        fs = font_size,
        tc = text_color,
    ));
    css.push_str("@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}");
    css.push_str("@keyframes dash{to{stroke-dashoffset:0;}}");
    for (sel, decl) in BOILERPLATE_RULES {
        // Inject the `line_color` into marker rules (forest's .marker is
        // `fill:#000000;stroke:#000000;` instead of #333333).
        let d = decl.replace("##LINE##", &line_color);
        css.push_str(&format!("#{id} {sel}{{{d}}}"));
    }
    css.push_str(&format!(
        "#{id} svg{{font-family:{ff};font-size:{fs};}}",
        id = id,
        ff = font_family_compact,
        fs = font_size,
    ));
    css.push_str(&format!("#{id} p{{margin:0;}}"));

    // Ishikawa-specific rules (from ishikawaStyles.ts).
    css.push_str(&format!(
        "#{id} .ishikawa .ishikawa-spine,#{id} .ishikawa .ishikawa-branch,#{id} .ishikawa .ishikawa-sub-branch{{stroke:{lc};stroke-width:2;fill:none;}}",
        id = id,
        lc = line_color,
    ));
    css.push_str(&format!(
        "#{id} .ishikawa .ishikawa-sub-branch{{stroke-width:1;}}",
        id = id,
    ));
    css.push_str(&format!(
        "#{id} .ishikawa .ishikawa-arrow{{fill:{lc};}}",
        id = id,
        lc = line_color,
    ));
    css.push_str(&format!(
        "#{id} .ishikawa .ishikawa-head{{fill:{mb};stroke:{lc};stroke-width:2;}}",
        id = id,
        mb = main_bkg,
        lc = line_color,
    ));
    css.push_str(&format!(
        "#{id} .ishikawa .ishikawa-label-box{{fill:{mb};stroke:{lc};stroke-width:2;}}",
        id = id,
        mb = main_bkg,
        lc = line_color,
    ));
    css.push_str(&format!(
        "#{id} .ishikawa text{{font-family:{ff};font-size:{fs};fill:{tc};}}",
        id = id,
        ff = font_family_compact,
        fs = font_size,
        tc = text_color,
    ));
    css.push_str(&format!(
        "#{id} .ishikawa .ishikawa-head-label{{font-weight:600;text-anchor:middle;dominant-baseline:middle;font-size:14px;}}",
        id = id,
    ));
    css.push_str(&format!(
        "#{id} .ishikawa .ishikawa-label{{text-anchor:end;}}",
        id = id,
    ));
    css.push_str(&format!(
        "#{id} .ishikawa .ishikawa-label.cause{{text-anchor:middle;dominant-baseline:middle;}}",
        id = id,
    ));
    css.push_str(&format!(
        "#{id} .ishikawa .ishikawa-label.align{{text-anchor:end;dominant-baseline:middle;}}",
        id = id,
    ));
    css.push_str(&format!(
        "#{id} .ishikawa .ishikawa-label.up{{dominant-baseline:baseline;}}",
        id = id,
    ));
    css.push_str(&format!(
        "#{id} .ishikawa .ishikawa-label.down{{dominant-baseline:hanging;}}",
        id = id,
    ));

    // Neo trailer (same across all diagram types). Forest variant uses
    // a gradient URL + a lighter drop-shadow; default uses the accent
    // color directly.
    let neo_gradient = if use_gradient {
        format!("url(#{id}-gradient)")
    } else {
        neo_stroke.clone()
    };
    let shadow = if use_gradient {
        "drop-shadow( 1px 2px 2px rgba(185,185,185,0.5))"
    } else {
        "drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))"
    };

    css.push_str(&format!("#{id} .node .neo-node{{stroke:{};}}", neo_stroke));
    css.push_str(&format!(
        "#{id} [data-look=\"neo\"].node rect,#{id} [data-look=\"neo\"].cluster rect,#{id} [data-look=\"neo\"].node polygon{{stroke:{ns};filter:{sh};}}",
        id = id,
        ns = neo_gradient,
        sh = shadow,
    ));
    css.push_str(&format!(
        "#{id} [data-look=\"neo\"].node path{{stroke:{ns};stroke-width:1px;}}",
        id = id,
        ns = neo_gradient,
    ));
    css.push_str(&format!(
        "#{id} [data-look=\"neo\"].node .outer-path{{filter:{sh};}}",
        id = id,
        sh = shadow,
    ));
    css.push_str(&format!(
        "#{id} [data-look=\"neo\"].node .neo-line path{{stroke:{ns};filter:none;}}",
        id = id,
        ns = neo_stroke,
    ));
    css.push_str(&format!(
        "#{id} [data-look=\"neo\"].node circle{{stroke:{ns};filter:{sh};}}",
        id = id,
        ns = neo_gradient,
        sh = shadow,
    ));
    css.push_str(&format!(
        "#{id} [data-look=\"neo\"].node circle .state-start{{fill:#000000;}}",
        id = id,
    ));
    css.push_str(&format!(
        "#{id} [data-look=\"neo\"].icon-shape .icon{{fill:{ns};filter:{sh};}}",
        id = id,
        ns = neo_gradient,
        sh = shadow,
    ));
    css.push_str(&format!(
        "#{id} [data-look=\"neo\"].icon-shape .icon-neo path{{stroke:{ns};filter:{sh};}}",
        id = id,
        ns = neo_gradient,
        sh = shadow,
    ));
    css.push_str(&format!(
        "#{id} :root{{--mermaid-font-family:{ff};}}",
        id = id,
        ff = font_family_compact,
    ));

    format!("<style>{css}</style>")
}

/// Mermaid's CSS boilerplate — every rule scoped by `#id`. The
/// `##LINE##` token is substituted with the theme `lineColor` so the
/// forest/dark themes (which use a non-default marker colour) produce
/// the right byte sequence.
const BOILERPLATE_RULES: &[(&str, &str)] = &[
    (
        ".edge-animation-slow",
        "stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;",
    ),
    (
        ".edge-animation-fast",
        "stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;",
    ),
    (".error-icon", "fill:#552222;"),
    (".error-text", "fill:#552222;stroke:#552222;"),
    (".edge-thickness-normal", "stroke-width:1px;"),
    (".edge-thickness-thick", "stroke-width:3.5px;"),
    (".edge-pattern-solid", "stroke-dasharray:0;"),
    (".edge-thickness-invisible", "stroke-width:0;fill:none;"),
    (".edge-pattern-dashed", "stroke-dasharray:3;"),
    (".edge-pattern-dotted", "stroke-dasharray:2;"),
    (".marker", "fill:##LINE##;stroke:##LINE##;"),
    (".marker.cross", "stroke:##LINE##;"),
];

/// Compact a CSS font-family list by dropping spaces after commas
/// outside quoted segments. Mermaid's stylesheet minifier produces
/// `"trebuchet ms",verdana,arial,sans-serif` from the spaced form.
fn compact_font_family(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_quote: Option<char> = None;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(q) = in_quote {
            out.push(c);
            if c == q {
                in_quote = None;
            }
        } else {
            match c {
                '"' | '\'' => {
                    in_quote = Some(c);
                    out.push(c);
                }
                ',' => {
                    out.push(',');
                    while let Some(&n) = chars.peek() {
                        if n == ' ' {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                _ => out.push(c),
            }
        }
    }
    out
}

// ── Formatters ─────────────────────────────────────────────────────

/// Format a floating-point value for an SVG attribute using JS's
/// `Number.prototype.toString()` rules. (Identical algorithm to the
/// radar renderer's `js_num`.)
pub(super) fn js_num(v: f64) -> String {
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

pub(super) fn html_escape(s: &str) -> String {
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

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::ishikawa as layout_mod;
    use crate::parser::ishikawa as parser_mod;
    use crate::preprocess::preprocess;
    use crate::theme::get_theme;

    fn render_fixture(source: &str, id: &str) -> String {
        let pre = preprocess(source).expect("preprocess");
        let theme_name = pre.config.theme.as_deref().unwrap_or("default");
        let theme = get_theme(theme_name);
        let mut diagram = parser_mod::parse(source).expect("parse");
        // Plumb look + handDrawnSeed (mirrors lib.rs::convert_with_id).
        diagram.look = pre.config.look.clone();
        diagram.hand_drawn_seed = pre
            .config
            .extras
            .get("handDrawnSeed")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .or(Some(0));
        let lay = layout_mod::layout(&diagram, &theme).expect("layout");
        render(&diagram, &lay, &theme, id).expect("render")
    }

    fn check_fixture(source_path: &str, reference_path: &str, id: &str) {
        let source = std::fs::read_to_string(source_path).expect("source");
        let reference = std::fs::read_to_string(reference_path).expect("reference");
        let got = render_fixture(&source, id);
        let want = reference.trim_end_matches('\n');
        if got != want {
            let got_len = got.len();
            let ref_len = want.len();
            let mut diff_at = 0;
            for (i, (a, b)) in got.bytes().zip(want.bytes()).enumerate() {
                if a != b {
                    diff_at = i;
                    break;
                }
            }
            let ctx = 160usize;
            let start = diff_at.saturating_sub(ctx);
            let end_got = (diff_at + ctx).min(got_len);
            let end_ref = (diff_at + ctx).min(ref_len);
            panic!(
                "byte mismatch for {source_path} at byte {diff_at} (got_len={got_len} want_len={ref_len})\n  got: ...{g}...\n  ref: ...{r}...",
                g = &got[start..end_got],
                r = &want[start..end_ref],
            );
        }
    }

    macro_rules! ishikawa_fixture_test {
        ($name:ident, $kind:literal, $num:literal) => {
            #[test]
            fn $name() {
                check_fixture(
                    concat!("tests/ext_fixtures/", $kind, "/ishikawa/", $num, ".mmd"),
                    concat!(
                        "tests/reference/ext_fixtures/",
                        $kind,
                        "/ishikawa/",
                        $num,
                        ".svg"
                    ),
                    concat!("ref-ext-fixtures-", $kind, "-ishikawa-", $num),
                );
            }
        };
    }

    ishikawa_fixture_test!(cypress_ishikawa_01, "cypress", "01");
    ishikawa_fixture_test!(cypress_ishikawa_02, "cypress", "02");
    ishikawa_fixture_test!(cypress_ishikawa_03, "cypress", "03");
    ishikawa_fixture_test!(cypress_ishikawa_04, "cypress", "04");
    ishikawa_fixture_test!(cypress_ishikawa_05, "cypress", "05");
    ishikawa_fixture_test!(cypress_ishikawa_06, "cypress", "06");
    ishikawa_fixture_test!(cypress_ishikawa_07, "cypress", "07");
    ishikawa_fixture_test!(cypress_ishikawa_08, "cypress", "08");
    ishikawa_fixture_test!(cypress_ishikawa_09, "cypress", "09");
    ishikawa_fixture_test!(cypress_ishikawa_10, "cypress", "10");
    ishikawa_fixture_test!(cypress_ishikawa_11, "cypress", "11");
    ishikawa_fixture_test!(cypress_ishikawa_12, "cypress", "12");
    ishikawa_fixture_test!(cypress_ishikawa_13, "cypress", "13");

    ishikawa_fixture_test!(demos_ishikawa_01, "demos", "01");
    ishikawa_fixture_test!(demos_ishikawa_02, "demos", "02");
    ishikawa_fixture_test!(demos_ishikawa_03, "demos", "03");
    ishikawa_fixture_test!(demos_ishikawa_04, "demos", "04");
    ishikawa_fixture_test!(demos_ishikawa_05, "demos", "05");
}
