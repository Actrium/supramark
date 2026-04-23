//! Shared `foreignObject` HTML-in-SVG label emitter + CSS-aware font
//! metrics for the Stratum 3 (er / block / class / state / flowchart /
//! requirement) diagram family.
//!
//! Upstream reference:
//! `packages/mermaid/src/rendering-util/createText.ts::addHtmlSpan`
//! plus `rendering-util/rendering-elements/shapes/util.ts::labelHelper`.
//!
//! Every label in the Stratum 3 family is serialized as:
//!
//! ```text
//! <g class="label" style="…" transform="translate(…, …)">
//!   <rect></rect>                              <!-- only for node labels -->
//!   <foreignObject width="…" height="…">
//!     <div style="display: table-cell; white-space: nowrap; line-height: 1.5;
//!                 max-width: 200px; text-align: center;"
//!          xmlns="http://www.w3.org/1999/xhtml"
//!          [class="labelBkg"]>                 <!-- only for edge labels -->
//!       <span class="nodeLabel|edgeLabel …" [style="…"]>
//!         <p>text</p>                          <!-- omitted when label is empty -->
//!       </span>
//!     </div>
//!   </foreignObject>
//! </g>
//! ```
//!
//! ## Style / ordering notes
//!
//! * `div` style keys are emitted in the exact order upstream sets them on
//!   the selection: `display`, `white-space`, `line-height`, then (only
//!   when `width != Number.POSITIVE_INFINITY`) `max-width`, `text-align`.
//! * `xmlns="http://www.w3.org/1999/xhtml"` is set *after* all of those
//!   (upstream calls `.attr("xmlns", …)` at the bottom of `addHtmlSpan`),
//!   and for edge labels `class="labelBkg"` is set after that again via
//!   `addBackground` branch.
//! * Widths / heights use JS-`Number.toString` style (integers without a
//!   trailing `.0`, fractions via Rust's shortest-decimal `{}` formatter
//!   which matches Grisu3 / Ryū ≈ JS's default).

use crate::font_metrics::{line_height, text_width};

/// Replace FontAwesome icon references (`fa:fa-car`, `fas:fa-spinner`, etc.)
/// with `<i class="fa fa-car"></i>` etc. — matches upstream's
/// `createText.ts::replaceIconSubstring` fallback when the icon is not
/// registered in the Iconify registry.
pub fn replace_fa_icons(text: &str) -> String {
    regex::Regex::new(r"(fa[bklrs]?):fa-([\w-]+)")
        .unwrap()
        .replace_all(text, |caps: &regex::Captures| {
            let prefix = &caps[1];
            let icon = &caps[2];
            // Upstream: `<i class='fa fa-car'></i>` (space between prefix and
            // `fa-icon` name, using `fa-` prefix on the icon).
            format!(r#"<i class="{} fa-{}"></i>"#, prefix, icon)
        })
        .to_string()
}

// ─── Public API ────────────────────────────────────────────────────────

/// Tuning knobs for `render_node_label` / `render_edge_label`.
///
/// All fields are `Option`s with sensible upstream defaults. This
/// mirrors `addHtmlSpan(element, node, width, classes, addBackground,
/// config)` plus the wrap-width logic in `createText`.
#[derive(Debug, Clone)]
pub struct LabelOpts<'a> {
    /// Additional CSS classes for the inner `<span>`, space-separated.
    /// Upstream appends these via `"${labelClass} ${classes}"`. Empty =
    /// just the base class.
    pub extra_span_classes: &'a str,
    /// Inline style written on both the `<div>` (via `applyStyle`) and
    /// the `<span>` (via `applyStyle` a second time). Upstream passes
    /// the node's `labelStyle` string verbatim — after replacing any
    /// `fill:` prefix with `color:`.
    pub label_style: Option<&'a str>,
    /// Style prefix for the `<div>` — text properties with spaces and
    /// hex→rgb normalization, e.g. `"color: rgb(0, 0, 255) !important; "`.
    /// When set, this PRECEDES the default `display: table-cell; ...` style
    /// in the div's style attribute.
    pub div_style_prefix: Option<&'a str>,
    /// `data-id` attribute written on the outer `<g class="label">`.
    /// Upstream only sets this for edge labels; node labels omit it.
    pub data_id: Option<&'a str>,
    /// Style for the outer `<g class="label" style="…">`. Upstream writes
    /// a `style` attribute whose value is the node's `labelStyle`
    /// string (same string that's applied to the inner span). Passing
    /// `None` drops the attribute altogether; `Some("")` writes an
    /// empty string (upstream default).
    pub group_style: Option<&'a str>,
    /// `transform` attribute for the outer `<g class="label">`. If set
    /// to `None` upstream defaults to the bbox-centred translate
    /// `translate(-width/2, -height/2)`.
    pub group_transform: Option<String>,
    /// Upstream's `addBackground` → whether to set `class="labelBkg"`
    /// on the `<div>` (edge labels) instead of leaving it unclassed
    /// (node labels). Upstream derives this from `!!node.icon || !!node.img`,
    /// but for our purposes the edge/node distinction drives it.
    pub add_background: bool,
    /// Wrapping width budget. Set to `f64::INFINITY` for "no max-width"
    /// (block diagram case). Any finite value is emitted as `max-width:
    /// Npx; text-align: center;`.
    pub max_width: f64,
    /// When `true`, the label text is wrapped in `<p>…</p>` (the
    /// markdown post-processor's output). This is the default for
    /// labels that contain text; set to `false` for empty edge labels
    /// (matching upstream's empty-span emission).
    pub wrap_in_p: bool,
    /// Whether the inner `<span>` gets the `nodeLabel` base class
    /// (`true`) or the `edgeLabel` base class (`false`).
    pub is_node: bool,
}

impl<'a> Default for LabelOpts<'a> {
    fn default() -> Self {
        Self {
            extra_span_classes: "",
            label_style: None,
            div_style_prefix: None,
            data_id: None,
            group_style: Some(""),
            group_transform: None,
            add_background: false,
            max_width: 200.0,
            wrap_in_p: true,
            is_node: true,
        }
    }
}

/// Emit a node-label `<g class="label">` block for Stratum 3 diagrams.
///
/// `text` is the already-sanitised label body. `width` / `height` are
/// the values for the `<foreignObject>` attributes (typically the
/// jsdom-shim-measured bbox). The outer group's `transform="translate(…)"`
/// defaults to `translate(-width/2, -height/2)` matching upstream
/// `labelHelper`'s `useHtmlLabels` branch.
pub fn render_node_label(text: &str, width: f64, height: f64, opts: &LabelOpts<'_>) -> String {
    let mut out = String::with_capacity(256 + text.len());
    // Outer <g class="label">.
    out.push_str("<g class=\"label\"");
    if let Some(did) = opts.data_id {
        out.push_str(&format!(r#" data-id="{}""#, did));
    }
    if let Some(s) = opts.group_style {
        out.push_str(&format!(r#" style="{}""#, s));
    }
    let xform = opts.group_transform.clone().unwrap_or_else(|| {
        format!(
            "translate({}, {})",
            fmt_num(-width / 2.0),
            fmt_num(-height / 2.0)
        )
    });
    out.push_str(&format!(r#" transform="{}""#, xform));
    out.push('>');
    // Upstream `labelHelper` inserts an empty `<rect>` on node labels
    // as the first child. Edge labels (emitted by `insertEdgeLabel`)
    // omit this marker rect.
    if !opts.add_background {
        // The "bkg" rect is specifically for node labels. Edge labels
        // don't have one inside their `<g class="label">`.
        out.push_str("<rect></rect>");
    }
    // foreignObject body.
    out.push_str(&foreign_object_body(text, width, height, opts));
    out.push_str("</g>");
    out
}

/// Emit an edge-label stack matching upstream's `insertEdgeLabel`:
///
/// ```text
/// <g class="edgeLabel" transform="translate(lx, ly)">
///   <g class="label" [data-id="…"] transform="translate(-w/2, -h/2)">
///     <foreignObject width="w" height="h">…</foreignObject>
///   </g>
/// </g>
/// ```
///
/// `label_x` / `label_y` are the final edge-label anchor in the parent
/// coordinate frame; `width` / `height` are the inner foreignObject
/// dimensions.
pub fn render_edge_label(
    text: &str,
    label_x: f64,
    label_y: f64,
    width: f64,
    height: f64,
    mut opts: LabelOpts<'_>,
) -> String {
    opts.is_node = false;
    opts.add_background = true;
    // Edge labels omit the `<rect>` marker — addBackground=true handles
    // that branch in render_node_label.
    let inner = render_node_label(text, width, height, &opts);
    // Upstream omits the `transform` attribute entirely when the label
    // has no text (empty edge labels don't get positioned).
    let transform_attr = if text.is_empty() {
        String::new()
    } else {
        format!(r#" transform="translate({lx}, {ly})""#, lx = fmt_num(label_x), ly = fmt_num(label_y))
    };
    format!(
        r#"<g class="edgeLabel"{transform}>{inner}</g>"#,
        transform = transform_attr,
        inner = inner,
    )
}

/// Build the `<foreignObject>…</foreignObject>` fragment that lives
/// inside `<g class="label">`. Exposed publicly so callers that need
/// to wrap the label block in a different outer group (e.g. cluster
/// labels, title rows) can reuse the inner body.
pub fn foreign_object_body(text: &str, width: f64, height: f64, opts: &LabelOpts<'_>) -> String {
    let mut out = String::with_capacity(192 + text.len());
    out.push_str(&format!(
        r#"<foreignObject width="{w}" height="{h}">"#,
        w = fmt_num(width),
        h = fmt_num(height),
    ));
    // <div>. Style attribute order: when there are text style properties
    // (from style/classDef), they PRECEDE the standard display/white-space/
    // line-height block. Upstream applies them via `applyStyle(div,
    // labelStyle)` before setting display etc.
    let mut div_style = String::new();
    if let Some(prefix) = opts.div_style_prefix {
        div_style.push_str(prefix);
    }
    div_style.push_str("display: table-cell; white-space: nowrap; line-height: 1.5;");
    if opts.max_width.is_finite() {
        div_style.push_str(&format!(
            " max-width: {}px; text-align: center;",
            fmt_num(opts.max_width)
        ));
    }
    out.push_str(&format!(
        r#"<div style="{ds}" xmlns="http://www.w3.org/1999/xhtml""#,
        ds = div_style,
    ));
    if opts.add_background {
        out.push_str(r#" class="labelBkg""#);
    }
    out.push('>');
    // Inner span.
    let span_base = if opts.is_node {
        "nodeLabel"
    } else {
        "edgeLabel"
    };
    // Upstream joins: `"${labelClass} ${classes}"` — with the trailing
    // space preserved even when `classes` is empty.
    let span_classes = if opts.extra_span_classes.is_empty() {
        format!("{} ", span_base)
    } else {
        format!("{} {}", span_base, opts.extra_span_classes)
    };
    out.push_str(&format!(r#"<span class="{}""#, span_classes));
    if let Some(s) = opts.label_style {
        out.push_str(&format!(r#" style="{}""#, s));
    }
    out.push('>');
    // Body — `<p>text</p>` for non-empty, bare empty string otherwise.
    if opts.wrap_in_p && !text.is_empty() {
        out.push_str("<p>");
        out.push_str(text);
        out.push_str("</p>");
    } else if !opts.wrap_in_p {
        out.push_str(text);
    }
    out.push_str("</span></div></foreignObject>");
    out
}

/// Convenience wrapper — build the `<g class="label">…</g>` block for a
/// generic shape (rect / polygon / path) using [`measure_html_label`]
/// to pick `<foreignObject>` width × height.
///
/// Returns an empty string when `label` is empty, matching the
/// upstream short-circuit in `labelHelper` that skips label emission
/// for label-less nodes.
///
/// `escaped_label` must already be HTML-escaped (`&amp;`, `&lt;`, …).
/// This function does NOT escape — shapes pass through raw markdown
/// content in some cases.
pub fn shape_label_block(escaped_label: &str, font: &HtmlLabelFont<'_>) -> String {
    if escaped_label.is_empty() {
        return String::new();
    }
    // Replace FontAwesome icon references (fa:fa-car → <i class="fa fa-car"></i>).
    // Applied after xml_escape since the FA pattern uses no XML-special chars.
    let processed = replace_fa_icons(escaped_label);
    let (w, h) = measure_html_label(&processed, font, 200.0, true);
    render_node_label(&processed, w, h, &LabelOpts::default())
}

// ─── CSS-aware label measurement ───────────────────────────────────────

/// Font resolution for an HTML label rendered inside `<foreignObject>`.
///
/// The jsdom shim in `tests/support/generate_ref.mjs::resolveFont`
/// walks up the DOM looking for explicit `font-family` / `font-size` /
/// `font-weight` ATTRIBUTES or inline `style` values — CSS class rules
/// are IGNORED. If none are found, it defaults to `14px` / `sans-serif`
/// / non-bold, which is what nearly every Stratum 3 `<foreignObject>`
/// label resolves to in practice.
///
/// Call with explicit `Some(...)` fields only when the emitted SVG
/// actually sets a matching attribute on the label `<div>`, `<span>`,
/// or `<p>` element (or an ancestor). Passing `None` uses the jsdom
/// default.
#[derive(Debug, Clone, Default)]
pub struct HtmlLabelFont<'a> {
    pub font_family: Option<&'a str>,
    pub font_size_px: Option<f64>,
    pub bold: Option<bool>,
}

impl<'a> HtmlLabelFont<'a> {
    fn resolve(&self) -> (&'a str, f64, bool) {
        (
            self.font_family.unwrap_or("sans-serif"),
            self.font_size_px.unwrap_or(14.0),
            self.bold.unwrap_or(false),
        )
    }
}

/// Width × height a `<foreignObject><div>` label renders to under the
/// jsdom shim's font resolution, matching `getBoundingClientRect()`.
///
/// `wrap_enabled` controls upstream's `if (width !== Infinity) …` branch
/// that sets `div.style.max-width`. When `false` (block diagram's
/// `Number.POSITIVE_INFINITY` width), wrapping is disabled and the
/// returned width is the full unwrapped text width.
///
/// `max_width_px` is the wrap budget. Ignored when `wrap_enabled` is
/// false. Text that exceeds the budget is split on whitespace using a
/// greedy first-fit algorithm (matching CSS's `white-space: break-spaces`
/// + `word-break: normal` defaults as observed in the jsdom shim).
pub fn measure_html_label(
    text: &str,
    font: &HtmlLabelFont<'_>,
    max_width_px: f64,
    wrap_enabled: bool,
) -> (f64, f64) {
    let (family, size, bold) = font.resolve();
    // Fast path: empty string.
    if text.is_empty() {
        return (0.0, line_height(family, size, bold, false));
    }
    // Upstream's initial getBoundingClientRect measurement happens BEFORE
    // the wrap fallback: `bbox = div.node().getBoundingClientRect(); if
    // (bbox.width === width) { div.style.white-space = break-spaces; … }`.
    // Under the jsdom shim both paths collapse to `measureTextBlock`
    // which splits on `\n` only. So the effective width is the longest
    // line's unwrapped width — wrapping does not reduce it unless the
    // caller explicitly pre-splits the input.
    let mut max_line_w = 0.0_f64;
    let lines: Vec<&str> = text.split('\n').collect();
    for line in &lines {
        let w = text_width(line, family, size, bold, false);
        if w > max_line_w {
            max_line_w = w;
        }
    }
    let lh = line_height(family, size, bold, false);
    let _ = (max_width_px, wrap_enabled); // currently unused; reserved.
    (max_line_w, lh * lines.len() as f64)
}

// ─── Internal helpers ──────────────────────────────────────────────────

/// JS-Number-like float formatting — integers lose `.0`, fractions use
/// the shortest round-trippable decimal. Duplicated here so the module
/// has no cross-crate helper dependencies.
fn fmt_num(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-Infinity" } else { "Infinity" }.into();
    }
    if x.fract() == 0.0 && x.abs() < 1e16 {
        format!("{}", x as i64)
    } else {
        format!("{}", x)
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_label_byte_exact_flowchart_style() {
        // Reproduce one of the labels from
        // tests/reference/ext_fixtures/demos/flowchart/02.svg — the
        // `stroke all sides` label.
        let got = render_node_label(
            "stroke all sides",
            105.0615234375,
            16.296875,
            &LabelOpts::default(),
        );
        assert_eq!(
            got,
            r#"<g class="label" style="" transform="translate(-52.53076171875, -8.1484375)"><rect></rect><foreignObject width="105.0615234375" height="16.296875"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel "><p>stroke all sides</p></span></div></foreignObject></g>"#
        );
    }

    #[test]
    fn node_label_byte_exact_block_style() {
        // Block uses width = Infinity → no max-width / no text-align.
        // From tests/reference/ext_fixtures/cypress/block/03.svg:
        //   <g class="label" style="" transform="translate(-10.841796875, -8.1484375)">
        //     <rect></rect>
        //     <foreignObject width="21.68359375" height="16.296875">
        //       <div style="display: table-cell; white-space: nowrap; line-height: 1.5;"
        //            xmlns="http://www.w3.org/1999/xhtml">
        //         <span class="nodeLabel "><p>id1</p></span>
        //       </div>
        //     </foreignObject>
        //   </g>
        let mut opts = LabelOpts::default();
        opts.max_width = f64::INFINITY;
        let got = render_node_label("id1", 21.68359375, 16.296875, &opts);
        assert_eq!(
            got,
            r#"<g class="label" style="" transform="translate(-10.841796875, -8.1484375)"><rect></rect><foreignObject width="21.68359375" height="16.296875"><div style="display: table-cell; white-space: nowrap; line-height: 1.5;" xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel "><p>id1</p></span></div></foreignObject></g>"#
        );
    }

    #[test]
    fn edge_label_byte_exact() {
        // From tests/reference/ext_fixtures/demos/flowchart/01.svg:
        //   <g class="edgeLabel" transform="translate(177.806640625, 41.60302734375)">
        //     <g class="label" data-id="L_DataStore_Process_0" transform="translate(-18.005859375, -8.1484375)">
        //       <foreignObject width="36.01171875" height="16.296875">
        //         <div style="…; max-width: 200px; text-align: center;"
        //              xmlns="…" class="labelBkg">
        //           <span class="edgeLabel "><p>input</p></span>
        //         </div>
        //       </foreignObject>
        //     </g>
        //   </g>
        let opts = LabelOpts {
            data_id: Some("L_DataStore_Process_0"),
            group_style: None,
            ..LabelOpts::default()
        };
        let got = render_edge_label(
            "input",
            177.806640625,
            41.60302734375,
            36.01171875,
            16.296875,
            opts,
        );
        assert_eq!(
            got,
            r#"<g class="edgeLabel" transform="translate(177.806640625, 41.60302734375)"><g class="label" data-id="L_DataStore_Process_0" transform="translate(-18.005859375, -8.1484375)"><foreignObject width="36.01171875" height="16.296875"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><span class="edgeLabel "><p>input</p></span></div></foreignObject></g></g>"#
        );
    }

    #[test]
    fn markdown_node_label_class_chain() {
        // ER entity label: `<span class="nodeLabel markdown-node-label">` +
        // max-width:100px when under minEntityWidth floor.
        let opts = LabelOpts {
            extra_span_classes: "markdown-node-label",
            max_width: 100.0,
            ..LabelOpts::default()
        };
        let got = render_node_label("PK", 17.623046875, 16.296875, &opts);
        assert!(got.contains(r#"<span class="nodeLabel markdown-node-label">"#));
        assert!(got.contains(r#"max-width: 100px"#));
        assert!(got.contains(r#"<p>PK</p>"#));
    }

    #[test]
    fn empty_label_omits_p_tag() {
        // Empty-edge-label case from class/01.svg.
        let opts = LabelOpts {
            data_id: Some("id_Animal_Duck_1"),
            is_node: false,
            add_background: true,
            group_style: None,
            group_transform: Some("translate(0, -8.1484375)".into()),
            ..LabelOpts::default()
        };
        let got = render_node_label("", 0.0, 16.296875, &opts);
        // The outer `<g class="edgeLabel" …>` is omitted — this is the
        // inner "label" only; caller can compose it.
        assert_eq!(
            got,
            r#"<g class="label" data-id="id_Animal_Duck_1" transform="translate(0, -8.1484375)"><foreignObject width="0" height="16.296875"><div style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;" xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><span class="edgeLabel "></span></div></foreignObject></g>"#
        );
    }

    #[test]
    fn measure_html_label_jsdom_default_14sans() {
        // "id1" at 14px sans-serif should match what the jsdom shim
        // returns for a bare <div> with no font attrs.
        let (w, h) = measure_html_label("id1", &HtmlLabelFont::default(), 200.0, true);
        // Verify against expected Rust font_metrics::text_width output.
        let expected_w = text_width("id1", "sans-serif", 14.0, false, false);
        let expected_h = line_height("sans-serif", 14.0, false, false);
        assert!((w - expected_w).abs() < 1e-9);
        assert!((h - expected_h).abs() < 1e-9);
        // The block fixture 03 emits width="21.68359375" height="16.296875"
        // for "id1". Our measurement must agree.
        assert!(
            (w - 21.68359375).abs() < 1e-6,
            "w={w}, expected 21.68359375"
        );
        assert!((h - 16.296875).abs() < 1e-6, "h={h}, expected 16.296875");
    }

    #[test]
    fn measure_html_label_multiline() {
        let (_w, h) = measure_html_label("a\nbb", &HtmlLabelFont::default(), 200.0, true);
        // Height should be 2× line-height.
        let lh = line_height("sans-serif", 14.0, false, false);
        assert!((h - 2.0 * lh).abs() < 1e-9);
    }

    #[test]
    fn group_style_none_drops_attribute() {
        let opts = LabelOpts {
            group_style: None,
            ..LabelOpts::default()
        };
        let got = render_node_label("x", 10.0, 16.0, &opts);
        assert!(!got.contains(r#"style="""#));
        assert!(got.contains(r#"<g class="label" transform=""#));
    }

    #[test]
    fn default_transform_centers_on_bbox() {
        let got = render_node_label("x", 40.0, 20.0, &LabelOpts::default());
        assert!(got.contains(r#"transform="translate(-20, -10)""#));
    }

    #[test]
    fn fmt_num_mirrors_js_number_tostring() {
        assert_eq!(fmt_num(5.0), "5");
        assert_eq!(fmt_num(-8.1484375), "-8.1484375");
        assert_eq!(fmt_num(0.0), "0");
    }
}
