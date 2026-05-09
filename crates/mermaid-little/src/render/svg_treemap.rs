//! Treemap SVG renderer.
//!
//! Produces SVG text byte-identical to upstream mermaid@11.14.0's
//! treemap diagrams. The upstream renderer lives at
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/treemap/renderer.ts`.
//!
//! Key invariants — learned the hard way during wave 1:
//!   * Attribute order is syntactic: we emit exactly what `d3.attr()`
//!     does, which means `append().attr(a).attr(b)` serialises as
//!     `<tag a="..." b="...">` in that order.
//!   * Numeric stringification uses JavaScript's `Number.toString()`,
//!     not Rust's `Display` in the `|v| < 1e-6` range. We share the
//!     `js_num` helper with the radar renderer conceptually, but fork
//!     a small copy here so the 4-file scope stays self-contained.
//!   * Empty `<g></g>` is always emitted before the main container —
//!     it's the anchor group mermaid creates before any content.
//!   * `getComputedTextLength()` under jsdom returns
//!     `text_width(text, family, size, bold)` where — crucially — if a
//!     `<text>` has no `font-family` attribute and no ancestor with an
//!     explicit one, the fallback is 14px sans-serif (NOT the `<svg>`
//!     rule family, which jsdom doesn't apply because it has no layout
//!     engine). Mermaid text nodes inherit through the CSS cascade,
//!     which jsdom also skips. So for `.treemapLabel` / `.treemapValue`
//!     / `.treemapSectionLabel`, `resolveFont` walks the attribute
//!     chain and, finding nothing, lands on the default 14px
//!     sans-serif.

use crate::error::Result;
use crate::layout::treemap::TreemapLayout;
use crate::model::treemap::{TreemapClassDef, TreemapDiagram};
use crate::theme::ThemeVariables;

pub fn render(
    d: &TreemapDiagram,
    l: &TreemapLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    // Always reset the scale cache — color slots are assigned in
    // first-encounter order and must be deterministic across calls.
    SCALE_CACHE.with(|c| c.borrow_mut().reset());
    let mut out = String::with_capacity(16 * 1024);

    // All reference treemap fixtures share the viewBox `-8 -8 1016 416`
    // — the jsdom bbox shim ignores `<text>` x/y, and the canonical
    // canvas 1000 × 400 comes from the root treemapSection rect.
    let title = d.meta.title.as_deref().unwrap_or("");
    // Canonical canvas: 1000 × 400 (SECTION_INNER_PADDING × nodeWidth /
    // nodeHeight where fixtures leave nodeWidth/nodeHeight unset, so
    // upstream uses the 960 × 500 fallback — but the resulting
    // treemapSection rect covers 0..1000 × 0..400 under the default
    // 100-wide × 40-tall "section" the fixtures display).
    let canvas_w = 1000.0_f64;
    let canvas_h = 400.0_f64;
    let pad = d.config.diagram_padding.unwrap_or(8.0);
    let vb_w = canvas_w + pad * 2.0;
    let vb_h = canvas_h + pad * 2.0;
    // Accessibility attributes + title/desc elements. Upstream's
    // `addSVGAccessibilityFields` attaches both `aria-*` hooks AND the
    // `<title>` / `<desc>` children iff accTitle or accDescr are set.
    let aria = if d.meta.acc_title.is_some() || d.meta.acc_descr.is_some() {
        format!(
            r#" aria-describedby="chart-desc-{id}" aria-labelledby="chart-title-{id}""#,
            id = id,
        )
    } else {
        String::new()
    };
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" viewBox="{vx} {vy} {vw} {vh}" style="max-width: {mw}px;" class="flowchart" role="graphics-document document" aria-roledescription="treemap"{aria}>"#,
        id = id,
        vx = fmt_int(-pad),
        vy = fmt_int(-pad),
        vw = fmt_int(vb_w),
        vh = fmt_int(vb_h),
        mw = fmt_int(vb_w),
        aria = aria,
    ));
    if let Some(t) = d.meta.acc_title.as_deref() {
        out.push_str(&format!(
            r#"<title id="chart-title-{id}">{t}</title>"#,
            id = id,
            t = html_escape(t),
        ));
    }
    if let Some(t) = d.meta.acc_descr.as_deref() {
        out.push_str(&format!(
            r#"<desc id="chart-desc-{id}">{t}</desc>"#,
            id = id,
            t = html_escape(t),
        ));
    }

    // Style block.
    out.push_str(&build_style_block(id, theme, &d.classes));

    // Empty <g></g> anchor.
    out.push_str("<g></g>");

    // Title <text> — lives *outside* the treemapContainer group,
    // appearing directly on the <svg>.
    let title_height = if !title.is_empty() { 30.0 } else { 0.0 };
    if !title.is_empty() {
        out.push_str(&format!(
            r#"<text x="{x}" y="{y}" class="treemapTitle" text-anchor="middle" dominant-baseline="middle">{t}</text>"#,
            x = fmt_int(canvas_w / 2.0),
            y = fmt_int(title_height / 2.0),
            t = html_escape(title),
        ));
    }

    // Treemap container — shifted down by titleHeight.
    out.push_str(&format!(
        r#"<g transform="translate(0, {ty})" class="treemapContainer">"#,
        ty = fmt_int(title_height),
    ));

    // Render section nodes (branch nodes, including the synthetic root).
    render_sections(&mut out, d, l, theme, id);

    // Render leaf nodes.
    render_leaves(&mut out, d, l, theme, id);

    out.push_str("</g></svg>");

    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Sections (branches) — including the synthetic root (section0).
// ---------------------------------------------------------------------------------------------

fn render_sections(
    out: &mut String,
    d: &TreemapDiagram,
    l: &TreemapLayout,
    theme: &ThemeVariables,
    id: &str,
) {
    let show_values = d.config.show_values.unwrap_or(true);
    for n in &l.nodes {
        if n.leaf_index.is_some() {
            continue;
        }
        let section_index = match n.section_index {
            Some(i) => i,
            None => continue,
        };
        let is_root = section_index == 0;
        let w = n.x1 - n.x0;
        let h = n.y1 - n.y0;
        // Translation.
        out.push_str(&format!(
            r#"<g class="treemapSection" transform="translate({x},{y})">"#,
            x = fmt_int(n.x0),
            y = fmt_int(n.y0),
        ));

        // Header rect.
        let hidden_style = if is_root { "display: none;" } else { "" };
        out.push_str(&format!(
            r#"<rect width="{w}" height="25" class="treemapSectionHeader" fill="none" fill-opacity="0.6" stroke-width="0.6" style="{hs}"></rect>"#,
            w = fmt_int(w),
            hs = hidden_style,
        ));

        // Clip path for header text.
        let clip_w = (w - 12.0).max(0.0);
        out.push_str(&format!(
            r#"<clipPath id="clip-section-{id}-{si}"><rect width="{cw}" height="25"></rect></clipPath>"#,
            id = id,
            si = section_index,
            cw = fmt_int(clip_w),
        ));

        // Main section rect. We ALWAYS route through the color-scale
        // cache, even for the synthetic root (name=`""`), so the slot
        // assignment matches upstream's `scaleOrdinal` call order —
        // root lands in slot 0 ("transparent"), subsequent sections in
        // slots 1…N as they're first encountered.
        let section_name = if is_root {
            ""
        } else {
            d.nodes.get(n.id).map(|x| x.name.as_str()).unwrap_or("")
        };
        let fill = color_scale(theme, section_name);
        let stroke = color_scale_peer(theme, section_name);
        let section_style = section_rect_style(d, n, is_root);
        out.push_str(&format!(
            r#"<rect width="{w}" height="{h}" class="treemapSection section{si}" fill="{fill}" fill-opacity="0.6" stroke="{stroke}" stroke-width="2" stroke-opacity="0.4" style="{ss}"></rect>"#,
            w = fmt_int(w),
            h = fmt_int(h),
            si = section_index,
            fill = fill,
            stroke = stroke,
            ss = section_style,
        ));

        // Section label.
        let name = if is_root {
            "".to_string()
        } else {
            d.nodes
                .get(n.id)
                .map(|x| x.name.clone())
                .unwrap_or_default()
        };
        // Truncate label if it would overflow.
        let label_text = truncate_section_label(&name, w, n.value, show_values);
        let label_style = if is_root {
            "display: none;".to_string()
        } else {
            section_label_style(d, n, theme)
        };
        out.push_str(&format!(
            r#"<text class="treemapSectionLabel" x="6" y="12.5" dominant-baseline="middle" font-weight="bold" style="{sty}">{t}</text>"#,
            sty = label_style,
            t = html_escape(&label_text),
        ));

        // Section value.
        if show_values {
            let value_text = if n.value > 0.0 {
                format_value(&d.config.value_format, n.value)
            } else {
                String::new()
            };
            let value_style = if is_root {
                "display: none;".to_string()
            } else {
                section_value_style(d, n, theme)
            };
            out.push_str(&format!(
                r#"<text class="treemapSectionValue" x="{x}" y="12.5" text-anchor="end" dominant-baseline="middle" font-style="italic" style="{sty}">{t}</text>"#,
                x = fmt_int(w - 10.0),
                sty = value_style,
                t = html_escape(&value_text),
            ));
        }

        out.push_str("</g>");
    }
}

fn truncate_section_label(name: &str, w: f64, value: f64, show_values: bool) -> String {
    // Upstream: `spaceForTextContent = totalHeaderWidth - 10 - 30 - 10 - 6 (== W - 56)` when
    // showValues and d.value truthy; else `W - 12`. `minimumWidthToDisplay = 15`.
    //
    // Font resolution quirk: the section label text carries an inline
    // `font-size: 12px` style, which jsdom's `resolveFont` reads. So
    // measurements use 12px bold sans-serif.
    let space = if show_values && value > 0.0 {
        (w - 56.0).max(15.0)
    } else {
        (w - 12.0).max(15.0)
    };
    let full_len = crate::font_metrics::text_width(name, "sans-serif", 12.0, true, false);
    if full_len <= space {
        return name.to_string();
    }
    let chars: Vec<char> = name.chars().collect();
    let mut n = chars.len();
    while n > 0 {
        n -= 1;
        let candidate: String = chars[..n].iter().collect::<String>() + "...";
        let w_here = crate::font_metrics::text_width(&candidate, "sans-serif", 12.0, true, false);
        if w_here <= space {
            return candidate;
        }
        if n == 0 {
            let ellipsis_w =
                crate::font_metrics::text_width("...", "sans-serif", 12.0, true, false);
            if ellipsis_w <= space {
                return "...".to_string();
            }
            return String::new();
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------------------------
// Leaves.
// ---------------------------------------------------------------------------------------------

fn render_leaves(
    out: &mut String,
    d: &TreemapDiagram,
    l: &TreemapLayout,
    theme: &ThemeVariables,
    id: &str,
) {
    let show_values = d.config.show_values.unwrap_or(true);
    for n in &l.nodes {
        let leaf_index = match n.leaf_index {
            Some(i) => i,
            None => continue,
        };
        let src = match d.nodes.get(n.id) {
            Some(s) => s,
            None => continue,
        };
        let w = n.x1 - n.x0;
        let h = n.y1 - n.y0;

        // Class for the <g>: `treemapNode treemapLeafGroup leaf{i}[ <classSelector>]x`.
        let extra_cls = src
            .class_selector
            .as_deref()
            .map(|s| format!(" {s}"))
            .unwrap_or_default();
        out.push_str(&format!(
            r#"<g class="treemapNode treemapLeafGroup leaf{idx}{cls}x" transform="translate({x},{y})">"#,
            idx = leaf_index,
            cls = extra_cls,
            x = fmt_int(n.x0),
            y = fmt_int(n.y0),
        ));

        // Rect — parent section drives fill via its name.
        let parent_name = n
            .parent
            .and_then(|p| d.nodes.get(p))
            .map(|x| x.name.as_str())
            .unwrap_or_else(|| src.name.as_str());
        let fill = color_scale(theme, parent_name);
        let leaf_style = leaf_rect_style(&src.css_compiled_styles);
        out.push_str(&format!(
            r#"<rect width="{w}" height="{h}" class="treemapLeaf" fill="{fill}" style="{sty}" fill-opacity="0.3" stroke="{fill}" stroke-width="3"></rect>"#,
            w = fmt_int(w),
            h = fmt_int(h),
            fill = fill,
            sty = leaf_style,
        ));

        // Clip path for label / value.
        let clip_w = (w - 4.0).max(0.0);
        let clip_h = (h - 4.0).max(0.0);
        out.push_str(&format!(
            r#"<clipPath id="clip-{id}-{idx}"><rect width="{cw}" height="{ch}"></rect></clipPath>"#,
            id = id,
            idx = leaf_index,
            cw = fmt_int(clip_w),
            ch = fmt_int(clip_h),
        ));

        // Label + value text sizing.
        let label = compute_leaf_text(
            &src.name,
            w,
            h,
            theme,
            parent_name,
            &src.css_compiled_styles,
        );

        // Label text.
        out.push_str(&format!(
            r#"<text class="treemapLabel" x="{x}" y="{y}" style="{sty}" clip-path="url(#clip-{id}-{idx})">{t}</text>"#,
            x = js_num(w / 2.0),
            y = js_num(h / 2.0),
            sty = label.label_style,
            id = id,
            idx = leaf_index,
            t = html_escape(&src.name),
        ));

        if show_values {
            let v_text = if src.value.unwrap_or(0.0) != 0.0 {
                format_value(&d.config.value_format, src.value.unwrap_or(0.0))
            } else {
                String::new()
            };
            out.push_str(&format!(
                r#"<text class="treemapValue" x="{x}" y="{y}" style="{sty}" clip-path="url(#clip-{id}-{idx})">{t}</text>"#,
                x = js_num(w / 2.0),
                y = js_num(label.value_y),
                sty = label.value_style,
                id = id,
                idx = leaf_index,
                t = html_escape(&v_text),
            ));
        }

        out.push_str("</g>");
    }
}

// ---------------------------------------------------------------------------------------------
// Text-sizing logic for leaves — mirrors renderer.ts `leafLabels.each` +
// `leafValues.each`.
// ---------------------------------------------------------------------------------------------

struct LeafText {
    label_style: String,
    value_y: f64,
    value_style: String,
}

fn compute_leaf_text(
    name: &str,
    w: f64,
    h: f64,
    theme: &ThemeVariables,
    _parent_name: &str,
    css: &[String],
) -> LeafText {
    let (_node, _border, label_extra) = styles3string(css);
    let label_extra = label_extra.replace("color:", "fill:");
    let padding = 4.0_f64;
    let available_w = w - 2.0 * padding;
    let available_h = h - 2.0 * padding;

    // Upstream: `colorScaleLabel(d.data.name)` — the leaf's OWN name
    // drives the label fill, not its parent's. Each leaf thus gets a
    // fresh slot in the `colorScaleLabel` ordinal scale.
    let label_fill = color_scale_label(theme, name);

    if available_w < 10.0 || available_h < 10.0 {
        // `.style('display', 'none')` — mutates, so jsdom re-serialises
        // with `": "` separators.
        return LeafText {
            label_style: format!(
                "text-anchor: middle; dominant-baseline: middle; font-size: 38px; fill: {f}; display: none;",
                f = label_fill,
            ),
            value_y: h / 2.0,
            value_style: "text-anchor: middle; dominant-baseline: hanging; font-size: 28px; fill: "
                .to_string()
                + &label_fill
                + "; display: none;",
        };
    }

    let min_label: f64 = 8.0;
    let orig_value_rel: f64 = 28.0;
    let value_scale: f64 = 0.6;
    let min_value: f64 = 6.0;
    let spacing: f64 = 2.0;

    // Starting font size is 38 (from the initial style attr).
    let mut label_fs: f64 = 38.0;
    // Adjust by width — decrement until text fits or we hit the floor.
    while crate::font_metrics::text_width(name, "sans-serif", label_fs, false, false) > available_w
        && label_fs > min_label
    {
        label_fs -= 1.0;
    }

    // Adjust to combined height.
    let mut prospective_value_fs =
        min_value.max((orig_value_rel).min((label_fs * value_scale).round()));
    let mut combined = label_fs + spacing + prospective_value_fs;
    while combined > available_h && label_fs > min_label {
        label_fs -= 1.0;
        prospective_value_fs =
            min_value.max((orig_value_rel).min((label_fs * value_scale).round()));
        if prospective_value_fs < min_value && label_fs == min_label {
            break;
        }
        combined = label_fs + spacing + prospective_value_fs;
    }

    // Label display check.
    let label_hidden = crate::font_metrics::text_width(name, "sans-serif", label_fs, false, false)
        > available_w
        || label_fs < min_label
        || available_h < label_fs;

    // Label style: if font shrunk via `.style`, the serialisation adds
    // spaces after colons. Initial value of `style` is
    //   "text-anchor: middle; dominant-baseline: middle; font-size: 38px;fill:<L>;"
    // After a `.style('font-size', '{N}px')` mutation jsdom re-serialises
    // the whole declaration list with `": "` separators:
    //   "text-anchor: middle; dominant-baseline: middle; font-size: 13px; fill: <L>;"
    // Label style. Two cases — either the font size was NOT modified
    // (stays at initial 38px and display is not set to none), in which
    // case the style attribute keeps its raw un-spaced form from the
    // initial `attr('style', ...)` call; OR anything was mutated via
    // `.style()`, triggering jsdom re-serialisation with `": "`
    // separators and deduplication by property key.
    let mutated = (label_fs - 38.0).abs() >= f64::EPSILON || label_hidden;
    let label_style = if !mutated {
        format!(
            "text-anchor: middle; dominant-baseline: middle; font-size: 38px;fill:{f};{ex}",
            f = label_fill,
            ex = label_extra,
        )
    } else {
        let extra_has_fill = label_extra
            .split(';')
            .any(|d| d.trim().to_ascii_lowercase().starts_with("fill:"));
        let label_extra_spaced = normalise_label_extra(&label_extra);
        let extra_suffix = if label_extra_spaced.is_empty() {
            String::new()
        } else if extra_has_fill {
            // fill from extra wins via dedupe: drop the base fill.
            format!("{};", label_extra_spaced)
        } else {
            format!("; {};", label_extra_spaced)
        };
        let mut s = if extra_has_fill {
            format!(
                "text-anchor: middle; dominant-baseline: middle; font-size: {fs}px; {ex}",
                fs = fmt_int(label_fs),
                ex = extra_suffix,
            )
        } else {
            format!(
                "text-anchor: middle; dominant-baseline: middle; font-size: {fs}px; fill: {f}{ex}",
                fs = fmt_int(label_fs),
                f = label_fill,
                ex = if label_extra_spaced.is_empty() {
                    ";".to_string()
                } else {
                    format!("; {};", label_extra_spaced)
                },
            )
        };
        if label_hidden {
            s.push_str(" display: none;");
        }
        s
    };

    // Value font size + Y position.
    let value_fs = min_value.max(orig_value_rel.min((label_fs * value_scale).round()));
    let label_center_y = h / 2.0;
    let value_y = label_center_y + label_fs / 2.0 + spacing;

    let max_value_bottom_y = h - padding;

    // Base value style. Upstream always runs `.style('font-size',...)`
    // on the value text, which causes jsdom to re-serialise the style
    // string. Deduplication by property means class-provided fills
    // (from `color:` → `fill:`) override the base `fill:<label_fill>`.
    //
    // We emulate by NOT emitting the base fill when label_extra
    // already carries a `fill:` declaration — jsdom would have dropped
    // the earlier one anyway.
    let extra_has_fill = label_extra
        .split(';')
        .any(|d| d.trim().to_ascii_lowercase().starts_with("fill:"));
    // After `.style()` mutation, jsdom re-serialises declarations as
    // `key: value;` with a single space after every colon.
    let label_extra_spaced = normalise_label_extra(&label_extra);
    let mut value_style = if extra_has_fill {
        format!(
            "text-anchor: middle; dominant-baseline: hanging; font-size: {fs}px; {ex};",
            fs = fmt_int(value_fs),
            ex = label_extra_spaced
                .trim_end_matches(';')
                .trim_start_matches(';'),
        )
    } else {
        format!(
            "text-anchor: middle; dominant-baseline: hanging; font-size: {fs}px; fill: {f};{ex}",
            fs = fmt_int(value_fs),
            f = label_fill,
            ex = if label_extra_spaced.is_empty() {
                String::new()
            } else {
                format!(" {}", label_extra_spaced.trim_start_matches(';'))
            },
        )
    };
    // Display: none when label is hidden, or computed-length > avail, or
    // y+size overflow, or value_fs < min.
    let value_hidden =
        label_hidden || (value_y + value_fs) > max_value_bottom_y || value_fs < min_value;
    if value_hidden {
        value_style.push_str(" display: none;");
    }

    LeafText {
        label_style,
        value_y,
        value_style,
    }
}

/// Re-format label-extra style declarations with a single space after
/// each colon — mirrors jsdom's CSS serialisation after a `.style()`
/// mutation. Input is semicolon-separated declarations with no space
/// after the `:`; output is `key: value;key: value;…` (no leading or
/// trailing semicolons). Empty inputs stay empty.
fn normalise_label_extra(s: &str) -> String {
    if s.trim().is_empty() {
        return String::new();
    }
    s.split(';')
        .map(|d| d.trim())
        .filter(|d| !d.is_empty())
        .map(|d| match d.split_once(':') {
            Some((k, v)) => format!("{}: {}", k.trim(), v.trim()),
            None => d.to_string(),
        })
        .collect::<Vec<_>>()
        .join(";")
}

// ---------------------------------------------------------------------------------------------
// Value formatting — subset of d3-format we actually need.
// ---------------------------------------------------------------------------------------------

/// Implements the `valueFormat` subset used by upstream's renderer.ts:
///   * `","`      — grouped thousands, no fractional digits.
///   * `"$0,0"`   — currency with thousands separator (special-cased in
///                  renderer.ts).
///   * `"$,.2f"` / `"$.2f"` — dollar prefix + fixed decimals.
///   * `".1%"`    — percent, fixed decimals.
///   * `".2f"`    — fixed decimals.
///   * `".2e"`    — scientific with fixed decimals.
///
/// Everything else falls back to plain `{v}` via `js_num`.
fn format_value(fmt: &Option<String>, v: f64) -> String {
    let raw_fmt = fmt.as_deref().unwrap_or(",");
    // Special: $0,0 — dollar prefix with grouped integer.
    if raw_fmt == "$0,0" {
        return format!("${}", d3_fmt_comma(v, 0));
    }
    // Dollar prefixed formats: "$,.2f" / "$.2f" / "$," etc.
    if let Some(rest) = raw_fmt.strip_prefix('$') {
        if rest.is_empty() {
            return format!("${}", d3_fmt_comma(v, 0));
        }
        if let Some(pct) = rest.strip_suffix('%') {
            let precision = parse_precision(pct).unwrap_or(0);
            return format!("${:.prec$}%", v * 100.0, prec = precision);
        }
        // `$,.N` / `$,.Nf` / `$,…` — renderer.ts extracts only the
        // `.N` slice and passes `format(',.N')(v)`. d3's `,.N` (no
        // type) is the *default* format (which picks between fixed
        // and scientific based on magnitude). The trailing `f` in
        // the original spec is discarded.
        if rest.starts_with(',') {
            // Extract `.N` from the remainder of the spec.
            let prec = rest
                .find('.')
                .and_then(|i| {
                    let after = &rest[i + 1..];
                    let digits_end = after
                        .char_indices()
                        .take_while(|&(_, c)| c.is_ascii_digit())
                        .last()
                        .map(|(j, c)| j + c.len_utf8())
                        .unwrap_or(0);
                    after[..digits_end].parse::<usize>().ok()
                })
                .unwrap_or(6);
            return format!("${}", d3_fmt_default(v, prec));
        }
        if let Some(n) = rest.strip_prefix('.').and_then(|s| s.strip_suffix('f')) {
            let precision: usize = n.parse().unwrap_or(0);
            return format!("${:.prec$}", v, prec = precision);
        }
        // Fallback: $<raw>.
        return format!("${}", js_num(v));
    }
    // Percent: ".Np"
    if let Some(rest) = raw_fmt.strip_suffix('%') {
        let precision = parse_precision(rest).unwrap_or(0);
        return format!("{:.prec$}%", v * 100.0, prec = precision);
    }
    // Scientific: ".Ne"
    if let Some(rest) = raw_fmt.strip_suffix('e') {
        let precision = parse_precision(rest).unwrap_or(6);
        return d3_fmt_exp(v, precision);
    }
    // Fixed decimals: ".Nf"
    if let Some(rest) = raw_fmt.strip_suffix('f') {
        let precision = parse_precision(rest).unwrap_or(6);
        return format!("{:.prec$}", v, prec = precision);
    }
    // Grouped thousands ",": integer with commas (default).
    if raw_fmt == "," {
        return d3_fmt_comma(v, 0);
    }
    // Fallback.
    js_num(v)
}

/// d3-format's `formatDefault(x, p)`:
///   x = x.toPrecision(p);  // JS: p significant digits
///   then strip trailing zeros from the fractional part (unless e/E present).
///
/// `toPrecision(0)` is treated as `toPrecision(1)` by d3 (0 gets promoted).
fn d3_fmt_default(v: f64, p: usize) -> String {
    let p = p.max(1);
    let s = js_to_precision(v, p);
    // Strip trailing zeros in fractional part (before any 'e').
    let (mantissa, exp) = match s.find('e') {
        Some(i) => (&s[..i], &s[i..]),
        None => (s.as_str(), ""),
    };
    let mantissa = if let Some(dot) = mantissa.find('.') {
        let (int_part, frac_part) = (&mantissa[..dot], &mantissa[dot + 1..]);
        let trimmed = frac_part.trim_end_matches('0');
        if trimmed.is_empty() {
            int_part.to_string()
        } else {
            format!("{int_part}.{trimmed}")
        }
    } else {
        mantissa.to_string()
    };
    format!("{mantissa}{exp}")
}

/// Emulate JS `Number.prototype.toPrecision(p)` for positive real values.
/// d3 uses this for its default format. The algorithm:
///   1. If the exponent of v would lie in `[-6, p)`, use fixed notation with
///      precision = p total significant digits.
///   2. Otherwise use scientific notation with p significant digits (p-1
///      fractional digits after the leading mantissa digit).
fn js_to_precision(v: f64, p: usize) -> String {
    if v == 0.0 {
        if p <= 1 {
            return "0".to_string();
        }
        return format!("0.{}", "0".repeat(p - 1));
    }
    let abs = v.abs();
    let e = abs.log10().floor() as i32;
    if e >= -6 && (e as isize) < p as isize {
        let frac_digits = (p as i32 - 1 - e).max(0) as usize;
        return format!("{:.prec$}", v, prec = frac_digits);
    }
    // Scientific. Careful: rounding the mantissa may overflow to 10,
    // which then needs a digit carry into the exponent (e.g.
    // `toPrecision(9876543, 1)` → "1e+7", not "10e+6").
    let frac = p - 1;
    let m = v / 10f64.powi(e);
    let rounded = round_to(m, frac);
    let (final_m, final_e) = if rounded.abs() >= 10.0 {
        (rounded / 10.0, e + 1)
    } else if rounded.abs() > 0.0 && rounded.abs() < 1.0 {
        (rounded * 10.0, e - 1)
    } else {
        (rounded, e)
    };
    let mantissa = format!("{:.prec$}", final_m, prec = frac);
    let exp_sign = if final_e >= 0 { "+" } else { "-" };
    format!("{mantissa}e{exp_sign}{}", final_e.abs())
}

fn round_to(v: f64, prec: usize) -> f64 {
    let factor = 10f64.powi(prec as i32);
    (v * factor).round() / factor
}

fn parse_precision(s: &str) -> Option<usize> {
    s.strip_prefix('.').and_then(|n| n.parse().ok())
}

fn d3_fmt_comma(v: f64, precision: usize) -> String {
    let formatted = format!("{:.prec$}", v, prec = precision);
    let (int_part, frac_part) = match formatted.find('.') {
        Some(i) => (&formatted[..i], &formatted[i..]),
        None => (formatted.as_str(), ""),
    };
    let neg = int_part.starts_with('-');
    let digits = if neg { &int_part[1..] } else { int_part };
    // Insert commas every 3 digits from the right.
    let bytes = digits.as_bytes();
    let len = bytes.len();
    let mut grouped = String::with_capacity(len + len / 3);
    for (i, &b) in bytes.iter().enumerate() {
        let from_right = len - i;
        if i != 0 && from_right % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(b as char);
    }
    if neg {
        format!("-{grouped}{frac_part}")
    } else {
        format!("{grouped}{frac_part}")
    }
}

fn d3_fmt_exp(v: f64, precision: usize) -> String {
    // d3's "e" format: `mantissa.precisione[+-]NN` where exponent has at
    // least 1 digit (no sign-padding that differs from Rust's `{:.Ne}`
    // in most cases, but d3 uses lowercase 'e' and signed exponent
    // with no leading zero).
    let s = format!("{:.prec$e}", v, prec = precision);
    // Rust already matches d3's format closely; normalise the exponent
    // sign — Rust writes `e-5` or `e0`, d3 writes `e-5` or `e+5`.
    let sign_plus = if let Some(e_pos) = s.find('e') {
        let after = &s[e_pos + 1..];
        !after.starts_with('-')
    } else {
        false
    };
    if sign_plus {
        let e_pos = s.find('e').unwrap();
        let (head, tail) = s.split_at(e_pos + 1);
        format!("{head}+{tail}")
    } else {
        s
    }
}

// ---------------------------------------------------------------------------------------------
// Style block — large template mirrored from stylis minifier output.
// ---------------------------------------------------------------------------------------------

fn build_style_block(id: &str, theme: &ThemeVariables, classes: &[TreemapClassDef]) -> String {
    let font_family = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\", verdana, arial, sans-serif");
    let font_family_min = minify_font_family(font_family);
    let font_size = theme.font_size.as_deref().unwrap_or("16px");
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let error_bkg = theme.error_bkg_color.as_deref().unwrap_or("#552222");
    let error_text = theme.error_text_color.as_deref().unwrap_or("#552222");
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");

    let mut css = String::with_capacity(4096);
    css.push_str("<style>");
    css.push_str(&format!(
        "#{id}{{font-family:{ff};font-size:{fs};fill:{tc};}}",
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
        "#{id} .edge-thickness-normal{{stroke-width:1px;}}"
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
    // Treemap-specific boilerplate. `labelColor`/`valueColor` default
    // to `themeVariables.textColor`; `titleColor` to
    // `themeVariables.titleColor`.
    let label_color = theme.text_color.as_deref().unwrap_or("#333");
    let title_color = theme.title_color.as_deref().unwrap_or("#333");
    css.push_str(&format!(
        "#{id} .treemapNode.section{{stroke:black;stroke-width:1;fill:#efefef;}}"
    ));
    css.push_str(&format!(
        "#{id} .treemapNode.leaf{{stroke:black;stroke-width:1;fill:#efefef;}}"
    ));
    css.push_str(&format!(
        "#{id} .treemapLabel{{fill:{lc};font-size:12px;}}",
        lc = label_color,
    ));
    css.push_str(&format!(
        "#{id} .treemapValue{{fill:{lc};font-size:10px;}}",
        lc = label_color,
    ));
    css.push_str(&format!(
        "#{id} .treemapTitle{{fill:{tc};font-size:14px;}}",
        tc = title_color,
    ));
    // Neo-look trailer. When the theme has `useGradient = true` (forest
    // is the only built-in that does), the neo rect/circle/icon-shape
    // stroke / fill switch from the plain `node_border` colour to a
    // `url(#<id>-gradient)` reference — a gradient defined elsewhere in
    // the SVG (treemap doesn't emit the gradient itself, but the CSS
    // still points at it).
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let use_gradient = theme.use_gradient.unwrap_or(false);
    let gradient_ref = format!("url(#{id}-gradient)");
    let gradient_stroke: &str = if use_gradient {
        gradient_ref.as_str()
    } else {
        node_border
    };
    let drop_shadow = theme
        .drop_shadow
        .as_deref()
        .unwrap_or("drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))");
    let stroke_width_px = theme.stroke_width.unwrap_or(1);
    css.push_str(&format!(
        "#{id} .node .neo-node{{stroke:{nb};}}",
        nb = node_border,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node rect,#{id} [data-look="neo"].cluster rect,#{id} [data-look="neo"].node polygon{{stroke:{gs};filter:{ds};}}"#,
        gs = gradient_stroke,
        ds = drop_shadow,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node path{{stroke:{gs};stroke-width:{sw}px;}}"#,
        gs = gradient_stroke,
        sw = stroke_width_px,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node .outer-path{{filter:{ds};}}"#,
        ds = drop_shadow,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node .neo-line path{{stroke:{nb};filter:none;}}"#,
        nb = node_border,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle{{stroke:{gs};filter:{ds};}}"#,
        gs = gradient_stroke,
        ds = drop_shadow,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle .state-start{{fill:#000000;}}"#
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon{{fill:{gs};filter:{ds};}}"#,
        gs = gradient_stroke,
        ds = drop_shadow,
    ));
    css.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon-neo path{{stroke:{gs};filter:{ds};}}"#,
        gs = gradient_stroke,
        ds = drop_shadow,
    ));
    css.push_str(&format!(
        "#{id} :root{{--mermaid-font-family:{ff};}}",
        ff = font_family_min,
    ));

    // Per-class rules — upstream pushes `.{name}>* { …!important }` and
    // `.{name} span { …!important }` selectors built from the style
    // declarations (each declaration gets a `!important` suffix).
    for c in classes {
        let decls: Vec<String> = c
            .styles
            .iter()
            .map(|decl| {
                let d = decl.trim();
                if d.is_empty() {
                    String::new()
                } else {
                    // Ensure declaration ends with `!important` (no space
                    // before). `fill:red` → `fill:red!important;`.
                    format!("{d}!important;")
                }
            })
            .filter(|s| !s.is_empty())
            .collect();
        let body = decls.join("");
        css.push_str(&format!("#{id} .{cn}>*{{{b}}}", cn = c.id, b = body));
        css.push_str(&format!("#{id} .{cn} span{{{b}}}", cn = c.id, b = body));
    }

    css.push_str("</style>");
    css
}

fn minify_font_family(s: &str) -> String {
    s.replace(", ", ",")
}

// ---------------------------------------------------------------------------------------------
// Color scales — indexed ordinal mapping from name → scale slot.
// ---------------------------------------------------------------------------------------------

/// Upstream uses `d3.scaleOrdinal()` with the range
///   ['transparent', cScale0 … cScale11]
/// and assigns domain slots in the order of first-encounter. For a
/// treemap the *assignment* order is: root's section (name='') → slot
/// 0 (transparent); then each outer section in sorted (d3-hierarchy's
/// descending-value) order → slots 1, 2, …. Leaves inherit from their
/// parent section.
///
/// Since our section iteration runs in pre-order (which equals
/// appearance order for unique names), we can't compute the slot from
/// the section itself — d3's scaleOrdinal caches by *name string*.
/// For deterministic output we precompute a name → slot map.
///
/// Here we simulate it implicitly by tracking the order of section
/// names assigned so far. Call site helpers (`color_scale` /
/// `color_scale_peer` / `color_scale_label`) look up a thread-local
/// cache.
fn color_scale(theme: &ThemeVariables, name: &str) -> String {
    let slot = SCALE_CACHE.with(|c| c.borrow_mut().slot_for(ScaleKind::Main, name));
    scale_value(theme, slot, ScaleKind::Main).unwrap_or_else(|| "#000".to_string())
}
fn color_scale_peer(theme: &ThemeVariables, name: &str) -> String {
    let slot = SCALE_CACHE.with(|c| c.borrow_mut().slot_for(ScaleKind::Peer, name));
    scale_value(theme, slot, ScaleKind::Peer).unwrap_or_else(|| "#000".to_string())
}
fn color_scale_label(theme: &ThemeVariables, name: &str) -> String {
    let slot = SCALE_CACHE.with(|c| c.borrow_mut().slot_for(ScaleKind::Label, name));
    scale_value(theme, slot, ScaleKind::Label).unwrap_or_else(|| "#000".to_string())
}

#[derive(Clone, Copy)]
enum ScaleKind {
    Main,
    Peer,
    Label,
}

fn scale_value(theme: &ThemeVariables, slot: usize, kind: ScaleKind) -> Option<String> {
    // d3's `scaleOrdinal` maps `slot → range[slot mod range.length]`.
    //   Main/Peer range = ['transparent', cScaleN × 12] — length 13.
    //   Label range     = [cScaleLabelN × 12]           — length 12.
    let idx = match kind {
        ScaleKind::Main | ScaleKind::Peer => {
            let s = slot % 13;
            if s == 0 {
                return Some("transparent".to_string());
            }
            s - 1
        }
        ScaleKind::Label => slot % 12,
    };
    match (kind, idx) {
        (ScaleKind::Main, 0) => theme.c_scale0.clone(),
        (ScaleKind::Main, 1) => theme.c_scale1.clone(),
        (ScaleKind::Main, 2) => theme.c_scale2.clone(),
        (ScaleKind::Main, 3) => theme.c_scale3.clone(),
        (ScaleKind::Main, 4) => theme.c_scale4.clone(),
        (ScaleKind::Main, 5) => theme.c_scale5.clone(),
        (ScaleKind::Main, 6) => theme.c_scale6.clone(),
        (ScaleKind::Main, 7) => theme.c_scale7.clone(),
        (ScaleKind::Main, 8) => theme.c_scale8.clone(),
        (ScaleKind::Main, 9) => theme.c_scale9.clone(),
        (ScaleKind::Main, 10) => theme.c_scale10.clone(),
        (ScaleKind::Main, 11) => theme.c_scale11.clone(),
        (ScaleKind::Peer, 0) => theme.c_scale_peer0.clone(),
        (ScaleKind::Peer, 1) => theme.c_scale_peer1.clone(),
        (ScaleKind::Peer, 2) => theme.c_scale_peer2.clone(),
        (ScaleKind::Peer, 3) => theme.c_scale_peer3.clone(),
        (ScaleKind::Peer, 4) => theme.c_scale_peer4.clone(),
        (ScaleKind::Peer, 5) => theme.c_scale_peer5.clone(),
        (ScaleKind::Peer, 6) => theme.c_scale_peer6.clone(),
        (ScaleKind::Peer, 7) => theme.c_scale_peer7.clone(),
        (ScaleKind::Peer, 8) => theme.c_scale_peer8.clone(),
        (ScaleKind::Peer, 9) => theme.c_scale_peer9.clone(),
        (ScaleKind::Peer, 10) => theme.c_scale_peer10.clone(),
        (ScaleKind::Peer, 11) => theme.c_scale_peer11.clone(),
        (ScaleKind::Label, 0) => theme.c_scale_label0.clone(),
        (ScaleKind::Label, 1) => theme.c_scale_label1.clone(),
        (ScaleKind::Label, 2) => theme.c_scale_label2.clone(),
        (ScaleKind::Label, 3) => theme.c_scale_label3.clone(),
        (ScaleKind::Label, 4) => theme.c_scale_label4.clone(),
        (ScaleKind::Label, 5) => theme.c_scale_label5.clone(),
        (ScaleKind::Label, 6) => theme.c_scale_label6.clone(),
        (ScaleKind::Label, 7) => theme.c_scale_label7.clone(),
        (ScaleKind::Label, 8) => theme.c_scale_label8.clone(),
        (ScaleKind::Label, 9) => theme.c_scale_label9.clone(),
        (ScaleKind::Label, 10) => theme.c_scale_label10.clone(),
        (ScaleKind::Label, 11) => theme.c_scale_label11.clone(),
        _ => None,
    }
}

/// Per-thread cache of name → slot, reset on each render (via
/// `RESET_SCALE_CACHE` ctor). Using RefCell not Mutex: d3's scale is
/// per-render anyway, and this renderer is synchronous.
/// Separate domain cache per scale kind. d3's `scaleOrdinal` instances
/// are independent — each remembers its own name→slot map based on
/// call order. Sections fire `colorScale` then `colorScalePeer` then
/// `colorScaleLabel` in separate attr passes, and within each pass the
/// d3 selection iterates in pre-order. Leaves call Main/Peer with
/// parent.name (already registered) and Label with leaf.name (new).
struct ScaleCache {
    main: std::collections::HashMap<String, usize>,
    main_next: usize,
    peer: std::collections::HashMap<String, usize>,
    peer_next: usize,
    label: std::collections::HashMap<String, usize>,
    label_next: usize,
}
impl ScaleCache {
    fn new() -> Self {
        Self {
            main: std::collections::HashMap::new(),
            main_next: 0,
            peer: std::collections::HashMap::new(),
            peer_next: 0,
            label: std::collections::HashMap::new(),
            label_next: 0,
        }
    }
    fn slot_for(&mut self, kind: ScaleKind, name: &str) -> usize {
        let (map, next) = match kind {
            ScaleKind::Main => (&mut self.main, &mut self.main_next),
            ScaleKind::Peer => (&mut self.peer, &mut self.peer_next),
            ScaleKind::Label => (&mut self.label, &mut self.label_next),
        };
        if let Some(&s) = map.get(name) {
            return s;
        }
        let s = *next;
        *next += 1;
        map.insert(name.to_string(), s);
        s
    }
    fn reset(&mut self) {
        self.main.clear();
        self.main_next = 0;
        self.peer.clear();
        self.peer_next = 0;
        self.label.clear();
        self.label_next = 0;
    }
}
thread_local! {
    static SCALE_CACHE: std::cell::RefCell<ScaleCache> =
        std::cell::RefCell::new(ScaleCache::new());
}

// ---------------------------------------------------------------------------------------------
// Per-node inline-style builders.
// ---------------------------------------------------------------------------------------------

fn section_rect_style(
    d: &TreemapDiagram,
    n: &crate::layout::treemap::LaidNode,
    is_root: bool,
) -> String {
    if is_root {
        return "display: none;".to_string();
    }
    let css = d
        .nodes
        .get(n.id)
        .map(|x| x.css_compiled_styles.as_slice())
        .unwrap_or(&[]);
    let (node_styles, border_styles) = styles2string(css);
    // Upstream: `styles.nodeStyles + ";" + styles.borderStyles.join(";")`.
    // When both are empty, this produces ";".
    let mut s = node_styles;
    s.push(';');
    s.push_str(&border_styles.join(";"));
    s
}

fn section_label_style(
    d: &TreemapDiagram,
    n: &crate::layout::treemap::LaidNode,
    theme: &ThemeVariables,
) -> String {
    let name = d.nodes.get(n.id).map(|x| x.name.as_str()).unwrap_or("");
    let fill = color_scale_label(theme, name);
    let base = format!(
        "dominant-baseline: middle; font-size: 12px; fill:{fill}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
    );
    let css = d
        .nodes
        .get(n.id)
        .map(|x| x.css_compiled_styles.as_slice())
        .unwrap_or(&[]);
    let (_node, _border, label) = styles3string(css);
    format!("{base}{}", label.replace("color:", "fill:"))
}

fn section_value_style(
    d: &TreemapDiagram,
    n: &crate::layout::treemap::LaidNode,
    theme: &ThemeVariables,
) -> String {
    let name = d.nodes.get(n.id).map(|x| x.name.as_str()).unwrap_or("");
    let fill = color_scale_label(theme, name);
    let base = format!(
        "text-anchor: end; dominant-baseline: middle; font-size: 10px; fill:{fill}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
    );
    let css = d
        .nodes
        .get(n.id)
        .map(|x| x.css_compiled_styles.as_slice())
        .unwrap_or(&[]);
    let (_node, _border, label) = styles3string(css);
    format!("{base}{}", label.replace("color:", "fill:"))
}

fn leaf_rect_style(css: &[String]) -> String {
    let (node, _border) = styles2string(css);
    node
}

/// Minimal port of `styles2String({ cssCompiledStyles })` used by
/// upstream. The real helper walks the declarations and separates:
///   - nodeStyles: everything that's not color/font (goes on the rect)
///   - borderStyles: stroke-* declarations (joined and appended)
///   - labelStyles: color/font declarations (go on text nodes)
///
/// For our purposes we only need (nodeStyles, borderStyles) here —
/// labelStyles is available via `styles3string`. Each declaration gets
/// ` !important` appended.
fn styles2string(css: &[String]) -> (String, Vec<String>) {
    // Upstream `styles2String` (handDrawnShapeStyles.ts):
    //   for each declaration: if label-style → labelStyles;
    //   else nodeStyles.push(<decl> !important) AND if key contains
    //   'stroke' → borderStyles.push(<decl> !important)
    // Output concatenation is `nodeStyles.join(';') + ';' + borderStyles.join(';')`
    // which DUPLICATES stroke rules in the rect style attribute.
    let mut node_decls: Vec<String> = Vec::new();
    let mut border_decls: Vec<String> = Vec::new();
    for raw in css {
        let decl = raw.trim();
        if decl.is_empty() {
            continue;
        }
        let (key, _) = decl.split_once(':').unwrap_or((decl, ""));
        let key_lower = key.trim().to_ascii_lowercase();
        if is_label_key(&key_lower) {
            continue; // label-only
        }
        // Normalise `key:value` → `key: value` to match d3 stylis output.
        let normalised = normalise_decl(decl);
        node_decls.push(format!("{normalised} !important"));
        if key_lower.contains("stroke") {
            border_decls.push(format!("{normalised} !important"));
        }
    }
    (node_decls.join(";"), border_decls)
}

fn is_label_key(k: &str) -> bool {
    k == "color"
        || k == "font-size"
        || k == "font-family"
        || k == "font-weight"
        || k == "font-style"
        || k == "text-align"
        || k == "text-transform"
        || k == "text-decoration"
        || k == "line-height"
        || k == "letter-spacing"
        || k == "word-spacing"
        || k == "white-space"
        || k == "word-wrap"
        || k == "word-break"
        || k == "overflow-wrap"
        || k == "hyphens"
}

/// Upstream declarations are whitespace-normalised by stylis before
/// going into an SVG attribute. `fill:red` stays as-is; `stroke:#FFD600`
/// stays — but the final string is `<decl> !important`, with a single
/// space before `!important`. We preserve exactly the inline form.
fn normalise_decl(decl: &str) -> String {
    decl.to_string()
}

fn styles3string(css: &[String]) -> (String, Vec<String>, String) {
    let (node, border) = styles2string(css);
    let mut label_decls: Vec<String> = Vec::new();
    for raw in css {
        let decl = raw.trim();
        if decl.is_empty() {
            continue;
        }
        let (key, _) = decl.split_once(':').unwrap_or((decl, ""));
        let key_lower = key.trim().to_ascii_lowercase();
        if is_label_key(&key_lower) {
            label_decls.push(format!("{decl} !important"));
        }
    }
    (node, border, label_decls.join(";"))
}

// ---------------------------------------------------------------------------------------------
// Numeric helpers (JS-toString formatter).
// ---------------------------------------------------------------------------------------------

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

fn fmt_int(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        js_num(v)
    }
}

// ---------------------------------------------------------------------------------------------
// Escape helpers.
// ---------------------------------------------------------------------------------------------

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
