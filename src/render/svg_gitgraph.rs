//! gitGraph SVG renderer — minimal port of upstream `gitGraphRenderer.ts`.
//!
//! Targets byte-exact parity with mermaid@11.14.0 for the linear /
//! single-branch / no-tag / no-merge / non-rotated subset of fixtures.
//! Anything more complex is rejected at the layout stage and falls
//! through to the global Unsupported handler (which the byte-exact
//! sweep treats via `tests/known_ignored.txt`).

use crate::error::Result;
use crate::layout::gitgraph::GitGraphLayout;
use crate::model::gitgraph::GitGraphDiagram;
use crate::theme::ThemeVariables;

pub fn render(
    d: &GitGraphDiagram,
    l: &GitGraphLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
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
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: {w}px;" viewBox="{vb}" role="graphics-document document" aria-roledescription="gitGraph">"#,
        id = id,
        vb = vb,
        w = fmt_num(l.viewbox_w),
    ));

    // ── <style> ──────────────────────────────────────────────────────
    out.push_str(&style_block(id, theme));

    // ── First-pass placeholder groups (jsdom artefact) ───────────────
    // Upstream does a pre-pass `drawCommits(modifyGraph=false)` to
    // populate commitPos for the arrow-router; in the DOM that leaves
    // empty <g class="commit-bullets"></g> + <g class="commit-labels">
    // groups behind. Reference SVGs have these empty placeholders, plus
    // a leading empty <g></g> from the per-branch label measurement.
    out.push_str("<g></g>");
    out.push_str(r#"<g class="commit-bullets"></g>"#);
    out.push_str(r#"<g class="commit-labels"></g>"#);

    // ── Branch group (line + label background + label) ───────────────
    // Layout currently only supports the single-branch case, so this is
    // straightforward.
    let b = &l.branches[0];
    let bbox_w = b.label_width;
    let bbox_h = b.label_height;
    let rotate_pad = if d.config.rotate_commit_label { 30.0 } else { 0.0 };
    let bkg_x = -bbox_w - 4.0 - rotate_pad;
    let bkg_y = -bbox_h / 2.0 + 10.0;
    let bkg_w = bbox_w + 18.0;
    let bkg_h = bbox_h + 4.0;
    let spine_y = -2.0_f64;
    let label_translate_x = -bbox_w - 14.0 - rotate_pad;
    let label_translate_y = spine_y - bbox_h / 2.0 - 2.0;

    out.push_str(&format!(
        r#"<g><line x1="0" y1="{sy}" x2="{maxp}" y2="{sy}" class="branch branch{idx}"></line>"#,
        sy = fmt_num(spine_y),
        maxp = fmt_num(l.max_pos),
        idx = b.index,
    ));
    out.push_str(&format!(
        r#"<rect class="branchLabelBkg label{idx}" style="" rx="4" ry="4" x="{x}" y="{y}" width="{w}" height="{h}" transform="translate(-19, -14)"></rect>"#,
        idx = b.index,
        x = fmt_num(bkg_x),
        y = fmt_num(bkg_y),
        w = fmt_num(bkg_w),
        h = fmt_num(bkg_h),
    ));
    out.push_str(&format!(
        r#"<g class="branchLabel"><g class="label branch-label{idx}" transform="translate({tx}, {ty})"><text><tspan xml:space="preserve" dy="1em" x="0" class="row">{name}</tspan></text></g></g></g>"#,
        idx = b.index,
        tx = fmt_num(label_translate_x),
        ty = fmt_num(label_translate_y),
        name = escape_text(&b.name),
    ));

    // ── Arrows (between consecutive commits on same branch) ──────────
    out.push_str(r#"<g class="commit-arrows">"#);
    for win in l.commits.windows(2) {
        let a = &win[0];
        let b = &win[1];
        out.push_str(&format!(
            r#"<path d="M {ax} {ay} L {bx} {by}" class="arrow arrow0"></path>"#,
            ax = fmt_num(a.cx),
            ay = fmt_num(a.cy),
            bx = fmt_num(b.cx),
            by = fmt_num(b.cy),
        ));
    }
    out.push_str("</g>");

    // ── Commit bullets ───────────────────────────────────────────────
    out.push_str(r#"<g class="commit-bullets">"#);
    for (i, c) in l.commits.iter().enumerate() {
        let commit = &d.commits[i];
        out.push_str(&format!(
            r#"<circle cx="{cx}" cy="{cy}" r="10" class="commit {id} commit0"></circle>"#,
            cx = fmt_num(c.cx),
            cy = fmt_num(c.cy),
            id = escape_text(&commit.id),
        ));
    }
    out.push_str("</g>");

    // ── Commit labels (rotated -45°) ─────────────────────────────────
    out.push_str(r#"<g class="commit-labels">"#);
    for (i, c) in l.commits.iter().enumerate() {
        let commit = &d.commits[i];
        let lw = l.commit_label_widths[i];
        let lh = l.commit_label_text_height;
        let py = 2.0_f64;
        let r_x = -7.5 - ((lw + 10.0) / 25.0) * 9.5;
        let r_y = 10.0 + (lw / 25.0) * 8.5;
        let rect_x = c.pos_with_offset - lw / 2.0 - py;
        let rect_y = c.cy + 13.5;
        let rect_w = lw + 2.0 * py;
        let rect_h = lh + 2.0 * py;
        let text_x = c.pos_with_offset - lw / 2.0;
        let text_y = c.cy + 25.0;
        out.push_str(&format!(
            r#"<g transform="translate({rx}, {ry}) rotate(-45, {pos}, {cy})"><rect class="commit-label-bkg" x="{x}" y="{y}" width="{w}" height="{h}"></rect><text x="{tx}" y="{ty}" class="commit-label">{label}</text></g>"#,
            rx = fmt_num(r_x),
            ry = fmt_num(r_y),
            pos = fmt_num(c.pos),
            cy = fmt_num(c.cy),
            x = fmt_num(rect_x),
            y = fmt_num(rect_y),
            w = fmt_num(rect_w),
            h = fmt_num(rect_h),
            tx = fmt_num(text_x),
            ty = fmt_num(text_y),
            label = escape_text(&commit.id),
        ));
    }
    out.push_str("</g>");

    out.push_str("</svg>");
    Ok(out)
}

/// Format a number the way d3/jsdom does in mermaid output:
///   - integral values render without a decimal point ("0", "150").
///   - fractional values keep their full precision (no trimming) so
///     the bytes match upstream.
fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        // Match JS Number.toString — `format!("{}", v)` for f64 in Rust
        // yields the shortest round-trip representation, which aligns
        // with V8 / Node for the values we produce here.
        format!("{}", v)
    }
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Strip whitespace after commas outside quoted segments, mirroring
/// stylis's CSS minification for the `font-family` value.
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

/// Compose the gitGraph CSS block — port of upstream
/// `diagrams/git/styles.js` with the default theme branch only
/// (the only one we need for the byte-exact subset we currently support).
fn style_block(id: &str, theme: &ThemeVariables) -> String {
    let ff_raw = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\", verdana, arial, sans-serif");
    let ff = minify_font_family(ff_raw);
    let ff = ff.as_str();
    let fs = theme
        .font_size
        .as_deref()
        .unwrap_or("16px");
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let error_bkg = theme.error_bkg_color.as_deref().unwrap_or("#552222");
    let error_text = theme.error_text_color.as_deref().unwrap_or("#552222");
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let main_bkg = theme.main_bkg.as_deref().unwrap_or("#ECECFF");
    let primary_color = theme.primary_color.as_deref().unwrap_or("#ECECFF");
    let stroke_width = theme.stroke_width.unwrap_or(1);
    let commit_label_bg = theme
        .commit_label_background
        .as_deref()
        .unwrap_or("#ffffde");
    let commit_label_color = theme.commit_label_color.as_deref().unwrap_or("#000021");
    let commit_label_size = theme
        .commit_label_font_size
        .as_deref()
        .unwrap_or("10px");
    let tag_label_bg = theme.tag_label_background.as_deref().unwrap_or("#ECECFF");
    let tag_label_border = theme
        .tag_label_border
        .as_deref()
        .unwrap_or("hsl(240, 60%, 86.2745098039%)");
    let tag_label_color = theme.tag_label_color.as_deref().unwrap_or("#131300");
    let tag_label_size = theme
        .tag_label_font_size
        .as_deref()
        .unwrap_or("10px");

    // Default-theme git color palette (12 entries, cycled mod 8). These come
    // from theme/default.rs; no need to look them up unless explicitly set
    // — the defaults match upstream exactly.
    const GIT0: [&str; 8] = [
        "hsl(240, 100%, 46.2745098039%)",
        "hsl(60, 100%, 43.5294117647%)",
        "hsl(80, 100%, 46.2745098039%)",
        "hsl(210, 100%, 46.2745098039%)",
        "hsl(180, 100%, 46.2745098039%)",
        "hsl(150, 100%, 46.2745098039%)",
        "hsl(300, 100%, 46.2745098039%)",
        "hsl(0, 100%, 46.2745098039%)",
    ];
    const GIT_INV: [&str; 8] = [
        "hsl(60, 100%, 3.7254901961%)",
        "rgb(0, 0, 160.5)",
        "rgb(48.8333333334, 0, 146.5000000001)",
        "rgb(146.5000000001, 73.2500000001, 0)",
        "rgb(146.5000000001, 0, 0)",
        "rgb(146.5000000001, 0, 73.2500000001)",
        "rgb(0, 146.5000000001, 0)",
        "rgb(0, 146.5000000001, 146.5000000001)",
    ];
    const GIT_BRANCH_LABEL: [&str; 8] = [
        "#ffffff", "black", "black", "#ffffff", "black", "black", "black", "black",
    ];

    let mut css = String::with_capacity(8192);
    css.push_str(&format!(
        "<style>#{id}{{font-family:{ff};font-size:{fs};fill:{text_color};}}@keyframes edge-animation-frame{{from{{stroke-dashoffset:0;}}}}@keyframes dash{{to{{stroke-dashoffset:0;}}}}#{id} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}#{id} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}#{id} .error-icon{{fill:{eb};}}#{id} .error-text{{fill:{et};stroke:{et};}}#{id} .edge-thickness-normal{{stroke-width:1px;}}#{id} .edge-thickness-thick{{stroke-width:3.5px;}}#{id} .edge-pattern-solid{{stroke-dasharray:0;}}#{id} .edge-thickness-invisible{{stroke-width:0;fill:none;}}#{id} .edge-pattern-dashed{{stroke-dasharray:3;}}#{id} .edge-pattern-dotted{{stroke-dasharray:2;}}#{id} .marker{{fill:{lc};stroke:{lc};}}#{id} .marker.cross{{stroke:{lc};}}#{id} svg{{font-family:{ff};font-size:{fs};}}#{id} p{{margin:0;}}#{id} .commit-id,#{id} .commit-msg,#{id} .branch-label{{fill:lightgrey;color:lightgrey;font-family:'trebuchet ms',verdana,arial,sans-serif;font-family:var(--mermaid-font-family);}}",
        eb = error_bkg, et = error_text, lc = line_color,
    ));

    // 12 sets of branch-label / commit / commit-highlight / label / arrow.
    for i in 0..12 {
        let ci = i % 8;
        css.push_str(&format!(
            "#{id} .branch-label{i}{{fill:{c};}}#{id} .commit{i}{{stroke:{g};fill:{g};}}#{id} .commit-highlight{i}{{stroke:{inv};fill:{inv};}}#{id} .label{i}{{fill:{g};}}#{id} .arrow{i}{{stroke:{g};}}",
            c = GIT_BRANCH_LABEL[ci],
            g = GIT0[ci],
            inv = GIT_INV[ci],
        ));
    }

    css.push_str(&format!(
        "#{id} .branch{{stroke-width:{sw};stroke:{lc};stroke-dasharray:2;}}#{id} .commit-label{{font-size:{cls};fill:{clc};}}#{id} .commit-label-bkg{{font-size:{cls};fill:{clb};opacity:0.5;}}#{id} .tag-label{{font-size:{tls};fill:{tlc};}}#{id} .tag-label-bkg{{fill:{tlb};stroke:{tlbo};}}#{id} .tag-hole{{fill:{tc};}}#{id} .commit-merge{{stroke:{pc};fill:{pc};}}#{id} .commit-reverse{{stroke:{pc};fill:{pc};stroke-width:3;}}#{id} .commit-highlight-inner{{stroke:{pc};fill:{pc};}}#{id} .arrow{{stroke-width:8;stroke-linecap:round;fill:none;}}#{id} .gitTitleText{{text-anchor:middle;font-size:18px;fill:{tc};}}",
        sw = stroke_width, lc = line_color,
        cls = commit_label_size, clc = commit_label_color, clb = commit_label_bg,
        tls = tag_label_size, tlc = tag_label_color, tlb = tag_label_bg, tlbo = tag_label_border,
        tc = text_color, pc = primary_color,
    ));

    // Neo-look fragment (always emitted, even on default theme — upstream
    // does this unconditionally inside the styles function).
    css.push_str(&format!(
        "#{id} .node .neo-node{{stroke:{nb};}}#{id} [data-look=\"neo\"].node rect,#{id} [data-look=\"neo\"].cluster rect,#{id} [data-look=\"neo\"].node polygon{{stroke:{nb};filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}#{id} [data-look=\"neo\"].node path{{stroke:{nb};stroke-width:1px;}}#{id} [data-look=\"neo\"].node .outer-path{{filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}#{id} [data-look=\"neo\"].node .neo-line path{{stroke:{nb};filter:none;}}#{id} [data-look=\"neo\"].node circle{{stroke:{nb};filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}#{id} [data-look=\"neo\"].node circle .state-start{{fill:{mb};}}#{id} [data-look=\"neo\"].icon-shape .icon{{fill:{nb};filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}#{id} [data-look=\"neo\"].icon-shape .icon-neo path{{stroke:{nb};filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}#{id} :root{{--mermaid-font-family:{ff};}}",
        nb = node_border, mb = "#000000", ff = ff,
    ));
    css.push_str("</style>");
    css
}
