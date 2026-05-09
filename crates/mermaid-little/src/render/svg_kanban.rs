//! Kanban SVG renderer — byte-exact parity against upstream
//! /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/kanban/kanbanRenderer.ts
//!
//! The layout stage already computed every geometry number, so this
//! module is a straight string-builder: attribute order, whitespace, and
//! number formatting all match upstream exactly.

use crate::error::Result;
use crate::layout::kanban::KanbanLayout;
use crate::model::kanban::{KanbanDiagram, KanbanItem};
use crate::theme::ThemeVariables;

/// Render a kanban diagram into an SVG string.
pub fn render(
    d: &KanbanDiagram,
    l: &KanbanLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(16 * 1024);

    // --- <svg ...> opener --------------------------------------------------------------------
    let (vb_x, vb_y, vb_w, vb_h) = l.view_box;
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: {mw}px;" viewBox="{vx} {vy} {vw} {vh}" role="graphics-document document" aria-roledescription="kanban">"#,
        id = id,
        mw = js_num(vb_w),
        vx = js_num(vb_x),
        vy = js_num(vb_y),
        vw = js_num(vb_w),
        vh = js_num(vb_h),
    ));

    // --- <style> block -----------------------------------------------------------------------
    out.push_str(&build_style_block(id, theme));

    // --- The empty anchor group -- upstream emits one even when nothing --
    out.push_str("<g></g>");

    // --- <g class="sections"> --------------------------------------------------------------
    out.push_str(r#"<g class="sections">"#);
    for (i, sec) in d.sections.iter().enumerate() {
        let sl = &l.sections[i];
        let section_class = sl.index as i64 + 1;
        out.push_str(&format!(
            r#"<g class="cluster undefined section-{sc}" id="{id}-{sid}" data-look="classic">"#,
            sc = section_class,
            id = id,
            sid = sec.id,
        ));
        out.push_str(&format!(
            r#"<rect style="" rx="5" ry="5" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
            x = js_num(sl.x),
            y = js_num(sl.y),
            w = js_num(sl.width),
            h = js_num(sl.height),
        ));
        out.push_str(&format!(
            r#"<g class="cluster-label " transform="translate({tx}, {ty})">"#,
            tx = js_num(sl.label_tx),
            ty = js_num(sl.label_ty),
        ));
        out.push_str(&foreign_object_section(sl.label_width, &sec.label));
        out.push_str("</g></g>");
    }
    out.push_str("</g>");

    // --- <g class="items"> ------------------------------------------------------------------
    out.push_str(r#"<g class="items">"#);
    let mut cursor = 0usize;
    for sec in &d.sections {
        for item in &sec.items {
            let il = &l.items[cursor];
            cursor += 1;
            out.push_str(&format!(
                r#"<g class="node undefined " id="{id}-{iid}" transform="translate({cx}, {cy})">"#,
                id = id,
                iid = item.id,
                cx = js_num(il.cx),
                cy = js_num(il.cy),
            ));
            // rect
            out.push_str(&format!(
                r#"<rect class="basic label-container __APA__" style="" rx="5" ry="5" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
                x = js_num(-il.width / 2.0),
                y = js_num(-il.height / 2.0),
                w = js_num(il.width),
                h = js_num(il.height),
            ));
            // When `config.kanban.ticketBaseUrl` is set AND the item has a
            // ticket, upstream's `kanbanItem` shape inserts an `<a>`
            // **before** the title label (via `shapeSvg.insert('a',
            // ':first-child')`). The ticket label lives inside that
            // `<a>`. When the URL is absent the ticket falls back to a
            // plain `<g class="label">` rendered *after* the title.
            let has_link = link_present(d, item);
            if has_link {
                render_ticket_label(&mut out, d, item, il);
            }
            // title label
            out.push_str(&format!(
                r#"<g class="label" style="text-align:left !important" transform="translate({tx}, {ty})"><rect></rect>"#,
                tx = js_num(il.title_tx),
                ty = js_num(il.title_ty),
            ));
            out.push_str(&foreign_object_item_title(il.title_width, &item.label));
            out.push_str("</g>");
            if !has_link {
                render_ticket_label(&mut out, d, item, il);
            }
            // assigned label
            out.push_str(&format!(
                r#"<g class="label" style="text-align:left !important" transform="translate({tx}, 0)"><rect></rect>"#,
                tx = js_num(il.assigned_tx),
            ));
            out.push_str(&foreign_object_item_side(
                il.assigned_width,
                item.assigned.as_deref().unwrap_or(""),
            ));
            out.push_str("</g>");

            // priority stripe
            if let Some((priority, y1, y2)) = il.priority {
                let lx = -il.width / 2.0 + 2.0;
                if let Some(stroke) = priority.stroke() {
                    out.push_str(&format!(
                        r#"<line x1="{x}" y1="{y1}" x2="{x}" y2="{y2}" stroke-width="4" stroke="{stroke}"></line>"#,
                        x = js_num(lx),
                        y1 = js_num(y1),
                        y2 = js_num(y2),
                        stroke = stroke,
                    ));
                }
            }
            out.push_str("</g>");
        }
    }
    out.push_str("</g>");
    out.push_str("</svg>");
    Ok(out)
}

// -------------------------------------------------------------------------------------------------
// Ticket label — gets wrapped in an `<a>` when `config.kanban.ticketBaseUrl` is set.
// -------------------------------------------------------------------------------------------------

fn render_ticket_label(
    out: &mut String,
    d: &KanbanDiagram,
    item: &KanbanItem,
    il: &crate::layout::kanban::ItemLayout,
) {
    let link = match (&d.ticket_base_url, &item.ticket) {
        (Some(url), Some(ticket)) if !url.is_empty() => Some(url.replace("#TICKET#", ticket)),
        _ => None,
    };

    if let Some(href) = link {
        out.push_str(&format!(
            r#"<a class="kanban-ticket-link" href="{href}" target="_blank">"#,
            href = html_escape(&href),
        ));
    }

    out.push_str(&format!(
        r#"<g class="label" style="text-align:left !important" transform="translate({tx}, 0)"><rect></rect>"#,
        tx = js_num(il.ticket_tx),
    ));
    out.push_str(&foreign_object_item_side(
        il.ticket_width,
        item.ticket.as_deref().unwrap_or(""),
    ));
    out.push_str("</g>");

    if link_present(d, item) {
        out.push_str("</a>");
    }
}

fn link_present(d: &KanbanDiagram, item: &KanbanItem) -> bool {
    matches!(
        (&d.ticket_base_url, &item.ticket),
        (Some(url), Some(_)) if !url.is_empty()
    )
}

// -------------------------------------------------------------------------------------------------
// Foreign-object builders — the three flavours encode the style attr order that the upstream
// d3/jsdom combo produces. See `createText.ts` + `util.ts` in the upstream tree.
// -------------------------------------------------------------------------------------------------

/// Section cluster label: `display` ordered first because there's no
/// pre-existing `style` attribute on the div.
fn foreign_object_section(width: f64, text: &str) -> String {
    let inner = if text.is_empty() {
        String::new()
    } else {
        format!("<p>{}</p>", html_escape(text))
    };
    format!(
        r#"<foreignObject width="{w}" height="16.296875"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel ">{inner}</span></div></foreignObject>"#,
        w = js_num(width),
        inner = inner,
    )
}

/// Item title (markdown-node-label): upstream sets the div style attr up
/// front to `text-align:left !important`, then later `.style(...)` calls
/// push new props AFTER that key. Overriding `text-align` via `.style()`
/// clears the `!important` flag while keeping the property first in the
/// serialization order.
fn foreign_object_item_title(width: f64, text: &str) -> String {
    let inner = if text.is_empty() {
        String::new()
    } else {
        format!("<p>{}</p>", html_escape(text))
    };
    format!(
        r#"<foreignObject width="{w}" height="16.296875"><div style="text-align: center; display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 175px;" xmlns="http://www.w3.org/1999/xhtml"><span style="text-align:left !important" class="nodeLabel markdown-node-label">{inner}</span></div></foreignObject>"#,
        w = js_num(width),
        inner = inner,
    )
}

/// Item ticket/assigned label — same div/span shape as the title but
/// without the `markdown-node-label` class (upstream's `insertLabel`
/// doesn't forward a class name).
fn foreign_object_item_side(width: f64, text: &str) -> String {
    let inner = if text.is_empty() {
        String::new()
    } else {
        format!("<p>{}</p>", html_escape(text))
    };
    format!(
        r#"<foreignObject width="{w}" height="16.296875"><div style="text-align: center; display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 175px;" xmlns="http://www.w3.org/1999/xhtml"><span style="text-align:left !important" class="nodeLabel ">{inner}</span></div></foreignObject>"#,
        w = js_num(width),
        inner = inner,
    )
}

// -------------------------------------------------------------------------------------------------
// Style block — 12 per-section blocks + root + icon helpers.
// Matches upstream `diagrams/kanban/styles.ts` minified through `cssMin`.
// -------------------------------------------------------------------------------------------------

fn build_style_block(id: &str, theme: &ThemeVariables) -> String {
    let mut s = String::with_capacity(20 * 1024);
    s.push_str("<style>");
    // Common prefix. CSS minification in upstream collapses
    // whitespace after commas in font-family lists.
    let ff_raw = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\", verdana, arial, sans-serif");
    let ff_minified = ff_raw.replace(", ", ",");
    let font_family = ff_minified.as_str();
    let font_size = theme.font_size.as_deref().unwrap_or("16px");
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let error_bkg = theme.error_bkg_color.as_deref().unwrap_or("#552222");
    let error_txt = theme.error_text_color.as_deref().unwrap_or("#552222");
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let background = theme.background.as_deref().unwrap_or("white");

    s.push_str(&format!(
        "#{id}{{font-family:{ff};font-size:{fs};fill:{tc};}}",
        id = id,
        ff = font_family,
        fs = font_size,
        tc = text_color,
    ));
    s.push_str("@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}");
    s.push_str("@keyframes dash{to{stroke-dashoffset:0;}}");
    s.push_str(&format!(
        "#{id} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}",
        id = id,
    ));
    s.push_str(&format!(
        "#{id} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}",
        id = id,
    ));
    s.push_str(&format!(
        "#{id} .error-icon{{fill:{c};}}",
        id = id,
        c = error_bkg
    ));
    s.push_str(&format!(
        "#{id} .error-text{{fill:{c};stroke:{c};}}",
        id = id,
        c = error_txt,
    ));
    s.push_str(&format!(
        "#{id} .edge-thickness-normal{{stroke-width:1px;}}",
        id = id,
    ));
    s.push_str(&format!(
        "#{id} .edge-thickness-thick{{stroke-width:3.5px;}}",
        id = id,
    ));
    s.push_str(&format!(
        "#{id} .edge-pattern-solid{{stroke-dasharray:0;}}",
        id = id,
    ));
    s.push_str(&format!(
        "#{id} .edge-thickness-invisible{{stroke-width:0;fill:none;}}",
        id = id,
    ));
    s.push_str(&format!(
        "#{id} .edge-pattern-dashed{{stroke-dasharray:3;}}",
        id = id,
    ));
    s.push_str(&format!(
        "#{id} .edge-pattern-dotted{{stroke-dasharray:2;}}",
        id = id,
    ));
    s.push_str(&format!(
        "#{id} .marker{{fill:{c};stroke:{c};}}",
        id = id,
        c = line_color,
    ));
    s.push_str(&format!(
        "#{id} .marker.cross{{stroke:{c};}}",
        id = id,
        c = line_color,
    ));
    s.push_str(&format!(
        "#{id} svg{{font-family:{ff};font-size:{fs};}}",
        id = id,
        ff = font_family,
        fs = font_size,
    ));
    s.push_str(&format!("#{id} p{{margin:0;}}", id = id));

    // `.edge { stroke-width: 3 }` — first genSections prelude line.
    s.push_str(&format!("#{id} .edge{{stroke-width:3;}}", id = id));

    // Twelve iterations (i = 0 .. 11) of genSections — producing
    // .section-{i-1} blocks with the unusual -1..10 numeric range.
    for i in 0..12i32 {
        s.push_str(&section_block(id, theme, i));
    }

    // --- Root section + shared trailing styles -----------------------------------------------
    let git0 = theme.git0.as_deref().unwrap_or("");
    let git_lbl0 = theme.git_branch_label0.as_deref().unwrap_or("");
    s.push_str(&format!(
        "#{id} .section-root rect,#{id} .section-root path,#{id} .section-root circle,#{id} .section-root polygon{{fill:{c};}}",
        id = id,
        c = git0,
    ));
    s.push_str(&format!(
        "#{id} .section-root text{{fill:{c};}}",
        id = id,
        c = git_lbl0,
    ));
    s.push_str(&format!(
        "#{id} .icon-container{{height:100%;display:flex;justify-content:center;align-items:center;}}",
        id = id,
    ));
    s.push_str(&format!("#{id} .edge{{fill:none;}}", id = id));
    s.push_str(&format!(
        "#{id} .cluster-label,#{id} .label{{color:{c};fill:{c};}}",
        id = id,
        c = text_color,
    ));
    s.push_str(&format!(
        "#{id} .kanban-label{{dy:1em;alignment-baseline:middle;text-anchor:middle;dominant-baseline:middle;text-align:center;}}",
        id = id,
    ));
    // getIconStyles
    s.push_str(&format!(
        "#{id} .label-icon{{display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;}}",
        id = id,
    ));
    s.push_str(&format!(
        "#{id} .node .label-icon path{{fill:currentColor;stroke:revert;stroke-width:revert;}}",
        id = id,
    ));
    // Neo data-look suffix from the shared `styles.ts`.
    s.push_str(&format!(
        "#{id} .node .neo-node{{stroke:{nb};}}",
        id = id,
        nb = node_border,
    ));
    let drop_shadow = theme
        .drop_shadow
        .as_deref()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "none".to_string());
    let filter_expr = if drop_shadow == "none" {
        "none".to_string()
    } else {
        format!("drop-shadow({})", drop_shadow_args(&drop_shadow))
    };
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node rect,#{id} [data-look="neo"].cluster rect,#{id} [data-look="neo"].node polygon{{stroke:{nb};filter:{f};}}"#,
        id = id,
        nb = node_border,
        f = filter_expr,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node path{{stroke:{nb};stroke-width:1px;}}"#,
        id = id,
        nb = node_border,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node .outer-path{{filter:{f};}}"#,
        id = id,
        f = filter_expr,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node .neo-line path{{stroke:{nb};filter:none;}}"#,
        id = id,
        nb = node_border,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle{{stroke:{nb};filter:{f};}}"#,
        id = id,
        nb = node_border,
        f = filter_expr,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle .state-start{{fill:#000000;}}"#,
        id = id,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon{{fill:{nb};filter:{f};}}"#,
        id = id,
        nb = node_border,
        f = filter_expr,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon-neo path{{stroke:{nb};filter:{f};}}"#,
        id = id,
        nb = node_border,
        f = filter_expr,
    ));
    // `:root { --mermaid-font-family: ... }` — upstream's final tail.
    s.push_str(&format!(
        "#{id} :root{{--mermaid-font-family:{ff};}}",
        id = id,
        ff = font_family,
    ));

    s.push_str("</style>");
    let _ = background; // used only inside `section_block`
    s
}

/// The upstream theme stores `drop_shadow` already as a `drop-shadow(...)`
/// function — but after jsdom CSS minification the wrapper string is
/// produced by `drop-shadow(${inner})`. We keep whatever is inside the
/// theme value untouched to match byte-for-byte.
fn drop_shadow_args(raw: &str) -> &str {
    if let Some(rest) = raw.strip_prefix("drop-shadow(") {
        if let Some(stripped) = rest.strip_suffix(')') {
            return stripped;
        }
    }
    raw
}

fn section_block(id: &str, theme: &ThemeVariables, i: i32) -> String {
    // Upstream styles.ts `genSections` iterates i=0..11 and emits a block
    // keyed by `.section-${i-1}` — that's where the unusual -1..10 range
    // comes from. `sw` is `17 - 3*i`.
    let scale = c_scale(theme, i as usize).unwrap_or("");
    let scale_inv = c_scale_inv(theme, i as usize).unwrap_or("");
    let scale_label = c_scale_label(theme, i as usize).unwrap_or("");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let background = theme.background.as_deref().unwrap_or("white");
    let sw = 17 - 3 * i;
    let n = i - 1;
    let fill = adjuster_light(scale, 10.0);

    let mut out = String::with_capacity(1024);
    out.push_str(&format!(
        "#{id} .section-{n} rect,#{id} .section-{n} path,#{id} .section-{n} circle,#{id} .section-{n} polygon,#{id} .section-{n} path{{fill:{f};stroke:{f};}}",
        id = id,
        n = n,
        f = fill,
    ));
    out.push_str(&format!(
        "#{id} .section-{n} text{{fill:{sl};}}",
        id = id,
        n = n,
        sl = scale_label,
    ));
    out.push_str(&format!(
        "#{id} .node-icon-{n}{{font-size:40px;color:{sl};}}",
        id = id,
        n = n,
        sl = scale_label,
    ));
    // section-edge-{n} stroke = cScale[i] raw (not the lighten/darken helper).
    out.push_str(&format!(
        "#{id} .section-edge-{n}{{stroke:{s};}}",
        id = id,
        n = n,
        s = scale,
    ));
    out.push_str(&format!(
        "#{id} .edge-depth-{n}{{stroke-width:{sw};}}",
        id = id,
        n = n,
        sw = sw,
    ));
    out.push_str(&format!(
        "#{id} .section-{n} line{{stroke:{s};stroke-width:3;}}",
        id = id,
        n = n,
        s = scale_inv,
    ));
    out.push_str(&format!(
        "#{id} .disabled,#{id} .disabled circle,#{id} .disabled text{{fill:lightgray;}}",
        id = id,
    ));
    out.push_str(&format!("#{id} .disabled text{{fill:#efefef;}}", id = id,));
    out.push_str(&format!(
        "#{id} .node rect,#{id} .node circle,#{id} .node ellipse,#{id} .node polygon,#{id} .node path{{fill:{bg};stroke:{nb};stroke-width:1px;}}",
        id = id,
        bg = background,
        nb = node_border,
    ));
    out.push_str(&format!(
        "#{id} .kanban-ticket-link{{fill:{bg};stroke:{nb};text-decoration:underline;}}",
        id = id,
        bg = background,
        nb = node_border,
    ));
    out
}

/// Apply `khroma.lighten(hslColor, 10)` on an HSL string. The stored
/// theme values are already in `hsl(H, 100%, L%)` form, so we just bump
/// `L` by 10 percentage points (capped at 100) and reserialize.
fn adjuster_light(hsl: &str, delta: f64) -> String {
    if let Some((h, s, l)) = parse_hsl(hsl) {
        let l2 = (l + delta).clamp(0.0, 100.0);
        return format_hsl(h, s, l2);
    }
    hsl.to_string()
}

fn parse_hsl(s: &str) -> Option<(f64, f64, f64)> {
    let rest = s.strip_prefix("hsl(")?.strip_suffix(')')?;
    let parts: Vec<&str> = rest.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: f64 = parts[0].trim().parse().ok()?;
    let s_pct: f64 = parts[1].trim().trim_end_matches('%').parse().ok()?;
    let l_pct: f64 = parts[2].trim().trim_end_matches('%').parse().ok()?;
    Some((h, s_pct, l_pct))
}

/// Serialize back into the upstream spelling: `hsl(H, S%, L%)` where the
/// lightness is printed with whatever precision the khroma routine
/// would have produced. The fixtures round to 10 decimal places —
/// matching js's `Number.prototype.toString()` for `86.2745098039`.
fn format_hsl(h: f64, s: f64, l: f64) -> String {
    format!(
        "hsl({h}, {s}%, {l}%)",
        h = js_num(h),
        s = js_num(s),
        l = js_num(l),
    )
}

// -------------------------------------------------------------------------------------------------
// Theme palette accessors — kanban uses cScale / cScaleInv / cScaleLabel in the 12-iteration loop.
// The theme struct keeps each slot optional; every default fixture expects slots 0..10 populated.
// -------------------------------------------------------------------------------------------------

fn c_scale(theme: &ThemeVariables, i: usize) -> Option<&str> {
    match i {
        0 => theme.c_scale0.as_deref(),
        1 => theme.c_scale1.as_deref(),
        2 => theme.c_scale2.as_deref(),
        3 => theme.c_scale3.as_deref(),
        4 => theme.c_scale4.as_deref(),
        5 => theme.c_scale5.as_deref(),
        6 => theme.c_scale6.as_deref(),
        7 => theme.c_scale7.as_deref(),
        8 => theme.c_scale8.as_deref(),
        9 => theme.c_scale9.as_deref(),
        10 => theme.c_scale10.as_deref(),
        11 => theme.c_scale11.as_deref(),
        _ => None,
    }
}

fn c_scale_inv(theme: &ThemeVariables, i: usize) -> Option<&str> {
    match i {
        0 => theme.c_scale_inv0.as_deref(),
        1 => theme.c_scale_inv1.as_deref(),
        2 => theme.c_scale_inv2.as_deref(),
        3 => theme.c_scale_inv3.as_deref(),
        4 => theme.c_scale_inv4.as_deref(),
        5 => theme.c_scale_inv5.as_deref(),
        6 => theme.c_scale_inv6.as_deref(),
        7 => theme.c_scale_inv7.as_deref(),
        8 => theme.c_scale_inv8.as_deref(),
        9 => theme.c_scale_inv9.as_deref(),
        10 => theme.c_scale_inv10.as_deref(),
        11 => theme.c_scale_inv11.as_deref(),
        _ => None,
    }
}

fn c_scale_label(theme: &ThemeVariables, i: usize) -> Option<&str> {
    match i {
        0 => theme.c_scale_label0.as_deref(),
        1 => theme.c_scale_label1.as_deref(),
        2 => theme.c_scale_label2.as_deref(),
        3 => theme.c_scale_label3.as_deref(),
        4 => theme.c_scale_label4.as_deref(),
        5 => theme.c_scale_label5.as_deref(),
        6 => theme.c_scale_label6.as_deref(),
        7 => theme.c_scale_label7.as_deref(),
        8 => theme.c_scale_label8.as_deref(),
        9 => theme.c_scale_label9.as_deref(),
        10 => theme.c_scale_label10.as_deref(),
        11 => theme.c_scale_label11.as_deref(),
        _ => None,
    }
}

// -------------------------------------------------------------------------------------------------
// Number / string formatting helpers.
// -------------------------------------------------------------------------------------------------

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
    use crate::layout::kanban as layout_mod;
    use crate::parser::kanban as parser_mod;
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
        let expected = reference.trim_end_matches('\n');
        if got != expected {
            let got_len = got.len();
            let ref_len = expected.len();
            let mut diff_at = 0;
            for (i, (a, b)) in got.bytes().zip(expected.bytes()).enumerate() {
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
                r = &expected[start..end_ref],
            );
        }
    }

    macro_rules! fixture_test {
        ($name:ident, $num:expr) => {
            #[test]
            fn $name() {
                check_fixture(
                    concat!("tests/ext_fixtures/cypress/kanban/", $num, ".mmd"),
                    concat!("tests/reference/ext_fixtures/cypress/kanban/", $num, ".svg"),
                    concat!("ref-ext-fixtures-cypress-kanban-", $num),
                );
            }
        };
    }

    fixture_test!(cypress_kanban_01, "01");
    fixture_test!(cypress_kanban_02, "02");
    fixture_test!(cypress_kanban_03, "03");
    fixture_test!(cypress_kanban_04, "04");
    fixture_test!(cypress_kanban_05, "05");
    fixture_test!(cypress_kanban_06, "06");
    fixture_test!(cypress_kanban_07, "07");
    fixture_test!(cypress_kanban_08, "08");
    fixture_test!(cypress_kanban_09, "09");
    fixture_test!(cypress_kanban_10, "10");
    fixture_test!(cypress_kanban_11, "11");
}
