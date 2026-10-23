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

use crate::font_metrics::text_width;

/// Browser-resolved default for labels rendered inside Mermaid's HTML
/// `<foreignObject>` labels. The generated SVG root declares
/// `font-size:16px`, and the label `<div>` declares `line-height: 1.5`.
pub const HTML_LABEL_FONT_SIZE: f64 = 16.0;

/// Upstream `flowchart.wrappingWidth` default: the `max-width` (and, once
/// clamped, the `width`) of every flowchart label `<div>`, and the budget the
/// text wraps inside. See [`clamp_html_label`].
pub const HTML_LABEL_WRAPPING_WIDTH: f64 = 200.0;

/// Horizontal padding mermaid-little adds to flowchart edge-label backgrounds.
/// A deliberate readability deviation from upstream (see
/// `tests/known_alignment_warnings.txt`); it sits outside `max-width`, so
/// layout and render both have to account for it.
pub const HTML_LABEL_EDGE_PADDING_X: f64 = 4.0;

pub const HTML_LABEL_LINE_HEIGHT_RATIO: f64 = 1.5;

pub fn html_label_line_height(font_size_px: f64) -> f64 {
    font_size_px * HTML_LABEL_LINE_HEIGHT_RATIO
}

/// Detect a KaTeX block — at least one paired `$$..$$` on a single line.
/// Mirrors mermaid's `katexRegex.test(text)` (`/\$\$(.*)\$\$/g`).
pub fn contains_katex_marker(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut found_open = None;
    while i + 1 < bytes.len() {
        if bytes[i] == b'$' && bytes[i + 1] == b'$' {
            if let Some(open) = found_open {
                if i > open + 2 {
                    return true;
                }
            } else {
                found_open = Some(i);
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    false
}

/// Try to render a label that may contain `$$..$$` math through KaTeX. On
/// success returns `(html, width, height)` ready to feed into
/// `render_node_label` (the caller passes `wrap_in_p: false`); on failure
/// or when the `katex` feature is off the function returns `None` and the
/// caller falls back to its normal text path.
///
/// The label is the raw .mmd label (XML-escaped — `&` → `&amp;` etc.).
/// `font` controls bbox measurement.
#[cfg(feature = "katex")]
pub fn try_render_katex_label(label: &str, font: &HtmlLabelFont<'_>) -> Option<(String, f64, f64)> {
    if !contains_katex_marker(label) {
        return None;
    }
    match crate::katex::render_label(label) {
        Ok(html) => {
            let (w, h) = measure_html_markup_label(&html, font, 200.0, true);
            Some((html, w, h))
        }
        Err(_) => None,
    }
}

/// Stub used when the `katex` feature is off — always falls back to the
/// upstream jsdom-default placeholder behavior (text rendered verbatim).
#[cfg(not(feature = "katex"))]
pub fn try_render_katex_label(
    _label: &str,
    _font: &HtmlLabelFont<'_>,
) -> Option<(String, f64, f64)> {
    None
}

/// Normalise every `<br>` / `<br />` / `<br\t/>` (and other whitespace
/// variants) to upstream's canonical `<br/>` form. Other tags pass through
/// unchanged, including their original casing.
///
/// Used for label inputs that may already contain literal HTML (edge labels,
/// shape-side labels) where we cannot run them through `string_label_to_html`
/// because that would also escape `<`/`>` text bodies. Mermaid upstream's
/// `markdownToHTML` re-serialises every `<br>` form to `<br/>` before
/// emission, so matching the cypress fixtures requires the same canonical
/// form regardless of how the source was authored.
pub fn normalize_br_tags(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len() + 4);
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Only treat as a tag when `<` is followed by an ASCII letter
            // (open tag) or `/letter` (close tag) — matches the recovery
            // rule in real HTML parsers and avoids rewriting text like
            // `< br>` that contains a stray `<`.
            let next = bytes.get(i + 1).copied();
            let is_tag_start = match next {
                Some(c) if c.is_ascii_alphabetic() => true,
                Some(b'/') => bytes
                    .get(i + 2)
                    .map(|c| c.is_ascii_alphabetic())
                    .unwrap_or(false),
                _ => false,
            };
            if is_tag_start {
                if let Some(rel_end) = src[i..].find('>') {
                    let tag_full = &src[i..i + rel_end + 1];
                    let inner = &tag_full[1..tag_full.len() - 1];
                    // Strip self-closing `/` (either side) plus surrounding
                    // whitespace and compare case-insensitively to catch
                    // every `<br>` / `<br/>` / `<br />` / `<BR>` variant.
                    let core = inner
                        .trim_end_matches('/')
                        .trim()
                        .trim_start_matches('/')
                        .trim();
                    if core.eq_ignore_ascii_case("br") {
                        out.push_str("<br/>");
                    } else {
                        out.push_str(tag_full);
                    }
                    i += rel_end + 1;
                    continue;
                }
            }
            out.push('<');
            i += 1;
        } else {
            // UTF-8-safe: copy a whole char (1..4 bytes) without truncating
            // multibyte sequences. Casting `bytes[i] as char` would split
            // CJK / emoji / accented bytes into Latin-1 supplements and
            // emit mojibake into the SVG.
            let ch_len = utf8_char_len(bytes[i]);
            out.push_str(&src[i..i + ch_len]);
            i += ch_len;
        }
    }
    out
}

/// Length in bytes of the UTF-8 character starting at the given lead byte.
/// Returns 1 for any invalid lead so callers always advance.
#[inline]
fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xC0 {
        // Continuation byte hit on its own — treat as 1 to avoid stalling.
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

/// One FontAwesome reference — `fa:fa-car`, `fas:fa-spinner`, … — inside a
/// label, carrying the exact span upstream's regex matches.
pub(crate) struct FaIconRef<'a> {
    /// Byte offset of the token start (the `f` of the prefix).
    pub start: usize,
    /// Byte offset just past the last name character.
    pub end: usize,
    /// `fa` / `fab` / `fak` / `fal` / `far` / `fas`.
    pub prefix: &'a str,
    /// The icon name without its `fa-` leader.
    pub name: &'a str,
}

/// The first FontAwesome reference at or after `from`.
///
/// This is the single definition of what an FA token is — the renderer
/// ([`replace_fa_icons`]) and the layout measurement
/// (`layout::flowchart::strip_fa_icons`, via [`strip_fa_icons`]) both consume
/// these spans, so the two can no longer disagree about where a token ends.
///
/// Mirrors upstream `createText.ts::replaceIconSubstring`'s
/// `/(fa[bklrs]?):fa-([\w-]+)/g`, including the parts that surprise:
///
/// - JavaScript's `\w` is ASCII (`[A-Za-z0-9_]`), so `_` *is* a name
///   character (`fa:fa-car_x` is one icon named `car_x`, not `car` followed
///   by text), while a non-ASCII letter *ends* the name and stays text —
///   Rust's Unicode `\w` used to swallow it into the icon name (the lexicon
///   tests below pin the literal).
/// - The name needs at least one character, so a name-less `fa:fa-` stays
///   literal text on both sides.
pub(crate) fn find_fa_icon(text: &str, from: usize) -> Option<FaIconRef<'_>> {
    let mut search = from;
    while let Some(rest) = text.get(search..) {
        let start = match rest.find("fa") {
            Some(rel) => search + rel,
            None => return None,
        };
        if let Some(icon) = fa_icon_at(text, start) {
            return Some(icon);
        }
        // Not a token here — resume past the `fa` we just rejected.
        search = start + 2;
    }
    None
}

/// Read one FA token starting exactly at `start`, which must point at the `f`
/// of the prefix. `None` when this position is not a token.
fn fa_icon_at(text: &str, start: usize) -> Option<FaIconRef<'_>> {
    let rest = text.get(start..)?;
    // Upstream's `fa[bklrs]?`: the optional style letter belongs to the
    // prefix, and a rejected longer prefix does not fall back to the short
    // one (`fabx:fa-car` is not a token, exactly as the regex backtracks).
    let prefix_len = match rest.as_bytes().get(2) {
        Some(b'b' | b'k' | b'l' | b'r' | b's') => 3,
        _ => 2,
    };
    let prefix = rest.get(..prefix_len)?;
    if !matches!(prefix, "fa" | "fab" | "fak" | "fal" | "far" | "fas") {
        return None;
    }
    let name_start = prefix_len + 1 + 3; // past `:` and `fa-`
    let after_colon = rest.get(prefix_len..)?.strip_prefix(':')?;
    let after_leader = after_colon.strip_prefix("fa-")?;
    let name_len = after_leader
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(after_leader.len());
    if name_len == 0 {
        return None;
    }
    Some(FaIconRef {
        start,
        end: start + name_start + name_len,
        prefix,
        name: &after_leader[..name_len],
    })
}

/// Rewrite every FA reference in `text` through `replacement`, copying the
/// text between references verbatim. Both sides of the icon pipeline go
/// through here, which is what keeps them on the same token lexicon.
fn rewrite_fa_icons(text: &str, mut replacement: impl FnMut(&FaIconRef<'_>) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(icon) = find_fa_icon(text, cursor) {
        out.push_str(&text[cursor..icon.start]);
        out.push_str(&replacement(&icon));
        cursor = icon.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// Replace FontAwesome icon references (`fa:fa-car`, `fas:fa-spinner`, etc.)
/// with `<i class="fa fa-car"></i>` etc. — matches upstream's
/// `createText.ts::replaceIconSubstring` fallback when the icon is not
/// registered in the Iconify registry.
pub fn replace_fa_icons(text: &str) -> String {
    rewrite_fa_icons(text, |icon| {
        // Upstream: `<i class='fa fa-car'></i>` (space between prefix and
        // `fa-` icon name); the reference SVGs carry the double-quoted form
        // a DOM serialiser writes.
        format!(r#"<i class="{} fa-{}"></i>"#, icon.prefix, icon.name)
    })
}

/// Remove every FA reference — the measuring side of [`replace_fa_icons`],
/// consuming exactly the same tokens. The `<i>` element contributes
/// negligible width under the jsdom shim, so layout measures the label
/// without them.
pub(crate) fn strip_fa_icons(text: &str) -> String {
    rewrite_fa_icons(text, |_| String::new())
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
    /// Extra horizontal CSS padding on the HTML label body. Upstream
    /// Mermaid leaves flowchart edge-label backgrounds flush with text;
    /// callers may opt into a small non-upstream visual readability
    /// enhancement while keeping the default byte-exact path unchanged.
    pub horizontal_padding: f64,
    /// Whether the browser clamped this label to [`Self::max_width`], and the
    /// emitter should therefore use upstream's wrapping style
    /// (`display: table; white-space: break-spaces; width: Npx;`) instead of
    /// `display: table-cell; white-space: nowrap;`.
    ///
    /// Set by the caller that measured the label through
    /// [`measure_html_markup_label_clamped`] — never inferred from the width,
    /// because a label whose shim width happens to *equal* the budget was
    /// never clamped (see issue #254's review).
    pub clamped: bool,
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
            horizontal_padding: 0.0,
            clamped: false,
        }
    }
}

fn padded_width(text: &str, width: f64, opts: &LabelOpts<'_>) -> f64 {
    if text.is_empty() || opts.horizontal_padding <= 0.0 {
        width
    } else {
        width + opts.horizontal_padding * 2.0
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
    let width = padded_width(text, width, opts);
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
        format!(
            r#" transform="translate({lx}, {ly})""#,
            lx = fmt_num(label_x),
            ly = fmt_num(label_y)
        )
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
    // A label the browser clamped to its wrap budget was measured through
    // `measure_html_markup_label_clamped`, and upstream `addHtmlSpan`
    // re-styles its div to match (`bbox.width === width` →
    // `display: table; white-space: break-spaces; width: Npx;`). The div then
    // wraps its text inside N instead of being squeezed to N while `nowrap`
    // paints the overflow outside the box — issue #254.
    //
    // The flag is threaded in by that measurement rather than inferred from
    // `width == max_width`: a label whose shim width happens to land exactly
    // on the budget (reachable — the metrics are 1/128 px) is *not* clamped,
    // and upstream emits `nowrap` for it.
    let clamped = opts.clamped;
    // <div>. Style attribute order: when there are text style properties
    // (from style/classDef), they PRECEDE the standard display/white-space/
    // line-height block. Upstream applies them via `applyStyle(div,
    // labelStyle)` before setting display etc.
    let mut div_style = String::new();
    if let Some(prefix) = opts.div_style_prefix {
        div_style.push_str(prefix);
    }
    if clamped {
        div_style.push_str("display: table; white-space: break-spaces; line-height: 1.5;");
    } else {
        div_style.push_str("display: table-cell; white-space: nowrap; line-height: 1.5;");
    }
    if !text.is_empty() && opts.horizontal_padding > 0.0 {
        div_style.push_str(&format!(
            " padding: 0 {}px;",
            fmt_num(opts.horizontal_padding)
        ));
    }
    if opts.max_width.is_finite() {
        div_style.push_str(&format!(
            " max-width: {}px; text-align: center;",
            fmt_num(opts.max_width)
        ));
    }
    if clamped {
        // Upstream sets `width` last, after max-width (d3 mutates the style
        // object in insertion order).
        div_style.push_str(&format!(" width: {}px;", fmt_num(opts.max_width)));
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
    // Upstream labelHelper emits style before class when a label style is present.
    if let Some(s) = opts.label_style {
        out.push_str(&format!(r#"<span style="{}" class="{}""#, s, span_classes));
    } else {
        out.push_str(&format!(r#"<span class="{}""#, span_classes));
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
///
/// Markdown shortcut: when `escaped_label` carries paired inline markdown
/// markers (`**bold**`, `*italic*`, `` `code` ``), upstream funnels the
/// label through `markdownToHTML` before emission and tags the
/// surrounding `<span>` with the `markdown-node-label` class. Detect that
/// here and emit the same HTML so shape paths that don't go through
/// `svg_flowchart`'s markdown branch (e.g. the diamond polygon) still
/// produce byte-exact output.
pub fn shape_label_block(escaped_label: &str, font: &HtmlLabelFont<'_>, clamp: bool) -> String {
    shape_label_block_inner(escaped_label, font, 0.0, 0.0, &[], clamp)
}

/// Variant of [`shape_label_block`] that threads the node's inline
/// `style …` css declarations through to the emitted `<g class="label"
/// style="…">`, the inner `<div style="…">` prefix (with hex→rgb
/// conversion), and the `<span style="…">`. Mirrors the
/// `applyStyle(div, labelStyle)` chain that rect/round shapes already
/// implement directly. See cypress fixture 105 — without threading the
/// styles through, every non-rect/round node renders an empty `style=""`.
pub fn shape_label_block_with_styles(
    escaped_label: &str,
    font: &HtmlLabelFont<'_>,
    css_styles: &[String],
    clamp: bool,
) -> String {
    shape_label_block_inner(escaped_label, font, 0.0, 0.0, css_styles, clamp)
}

/// Variant of [`shape_label_block`] that adds a vertical offset to the outer
/// `<g class="label">` transform. Used by shapes whose upstream rendering
/// translates the label by `node.padding / k` along the Y axis (cylinder.ts
/// uses `node.padding / 1.5`, ~10 for the default 15-unit padding).
pub fn shape_label_block_with_y_offset(
    escaped_label: &str,
    font: &HtmlLabelFont<'_>,
    y_offset: f64,
    clamp: bool,
) -> String {
    shape_label_block_inner(escaped_label, font, 0.0, y_offset, &[], clamp)
}

/// Combined `with_y_offset` + `with_styles` variant.
pub fn shape_label_block_with_y_offset_and_styles(
    escaped_label: &str,
    font: &HtmlLabelFont<'_>,
    y_offset: f64,
    css_styles: &[String],
    clamp: bool,
) -> String {
    shape_label_block_inner(escaped_label, font, 0.0, y_offset, css_styles, clamp)
}

/// Variant of [`shape_label_block`] that adds an arbitrary x/y offset
/// to the outer `<g class="label">` transform. Used by shapes whose
/// upstream rendering shifts the label horizontally too — e.g.
/// `rectLeftInvArrow` where the wrapping group is translated by
/// `-notch/2` and the label by the same amount on top of `-bbox.width/2`.
pub fn shape_label_block_with_xy_offset_and_styles(
    escaped_label: &str,
    font: &HtmlLabelFont<'_>,
    x_offset: f64,
    y_offset: f64,
    css_styles: &[String],
    clamp: bool,
) -> String {
    shape_label_block_inner(escaped_label, font, x_offset, y_offset, css_styles, clamp)
}

fn shape_label_block_inner(
    escaped_label: &str,
    font: &HtmlLabelFont<'_>,
    x_offset: f64,
    y_offset: f64,
    css_styles: &[String],
    clamp: bool,
) -> String {
    if escaped_label.is_empty() {
        return String::new();
    }
    // Label box under the diagram's policy: a clamped label reports the box a
    // browser paints after wrapping it inside the wrapping width, and the
    // flag travels into `LabelOpts` so the `<div>` gets the matching style.
    // Decide the clamp in the font the label is finally painted in (its inline
    // `font-size` / `font-weight`) — the same resolution
    // `shapes::types::measure_node_label` and the layout use. A shape whose
    // label only overflows at that size must still switch the `<div>` to the
    // wrapping style, or a CSS-aware host paints the larger text out of a box
    // sized for the smaller one. (The un-clamped fallbacks below keep the
    // caller's font, the domain their measurements have always used.)
    let clamp_font = || crate::layout::flowchart::resolved_label_font_from_styles(css_styles);
    let measure = |html: &str| -> (f64, f64, bool) {
        if clamp {
            measure_html_markup_label_clamped(html, &clamp_font(), HTML_LABEL_WRAPPING_WIDTH)
        } else {
            let (w, h) = measure_html_markup_label(html, font, HTML_LABEL_WRAPPING_WIDTH, true);
            (w, h, false)
        }
    };
    // Replace FontAwesome icon references (fa:fa-car → <i class="fa fa-car"></i>).
    // Applied after xml_escape since the FA pattern uses no XML-special chars.
    let processed = replace_fa_icons(escaped_label);
    // Compute the style strings once. The helper below threads them into
    // every `LabelOpts` it builds so the emitted `<g>`/`<div>`/`<span>`
    // carry the upstream `applyStyle(div, labelStyle)` chain.
    //
    // `font-size` is intentionally excluded here: the surrounding flowchart
    // pipeline runs a separate `font_size_postprocess_node_svg` pass that
    // re-measures the label at the requested font and rewrites the
    // foreignObject width/height + the `<g class="label" style="">`
    // transform together with the `font-size` style. Threading `font-size`
    // here would emit `style="font-size:30px"` on the `<g>` while leaving
    // the bbox measured at the default 14 px font, producing a layout that
    // mismatches both upstream and the postprocess output.
    let css_styles_no_fs: Vec<String> = css_styles
        .iter()
        .filter(|s| {
            let trimmed = s.trim().trim_end_matches(';');
            let key = trimmed
                .find(':')
                .map(|i| trimmed[..i].trim())
                .unwrap_or(trimmed);
            key != "font-size"
        })
        .cloned()
        .collect();
    let lbl_style: String = crate::render::shapes::types::build_label_style(&css_styles_no_fs);
    let div_prefix: String =
        crate::render::shapes::types::build_div_style_prefix(&css_styles_no_fs);
    fn apply_style_overrides<'a>(
        mut base: LabelOpts<'a>,
        w: f64,
        h: f64,
        x_offset: f64,
        y_offset: f64,
        lbl_style: &'a str,
        div_prefix: &'a str,
    ) -> LabelOpts<'a> {
        if !lbl_style.is_empty() {
            base.group_style = Some(lbl_style);
            base.label_style = Some(lbl_style);
        }
        if !div_prefix.is_empty() {
            base.div_style_prefix = Some(div_prefix);
        }
        if x_offset != 0.0 || y_offset != 0.0 {
            base.group_transform = Some(format!(
                "translate({}, {})",
                fmt_num(-w / 2.0 + x_offset),
                fmt_num(-h / 2.0 + y_offset)
            ));
        }
        base
    }
    if has_paired_markdown_markers(&processed) {
        // Build the HTML the same way `markdownToHTML` would, then measure
        // the marker-free `textContent` width via `measure_html_markup_label`.
        // The input may already contain XML entities (`&amp;`/`&lt;`/…) from
        // the caller's `xml_escape`; `markdown_label_to_html` re-escapes
        // unescaped specials, but it does not double-escape entities that
        // already begin with `&` because `xml_escape_label` only rewrites
        // bare `<>"'&` and the entity prefix `&` has nothing left to
        // escape inside the named-entity body. So the round-trip is safe
        // for the markdown patterns we handle here.
        let html = markdown_label_to_html(&processed);
        let (w, h, clamped) = measure(&html);
        let base = LabelOpts {
            extra_span_classes: "markdown-node-label",
            ..LabelOpts::default()
        };
        let mut opts =
            apply_style_overrides(base, w, h, x_offset, y_offset, &lbl_style, &div_prefix);
        opts.clamped = clamped;
        return render_node_label(&html, w, h, &opts);
    }
    // Plain string labels with embedded `<br>` family tags must be re-emitted
    // as real `<br/>` tags inside the foreignObject — upstream's
    // `markdownToHTML` step preserves `<br>` while collapsing every variant
    // (`<br>` / `<br/>` / `<br />`) to the canonical `<br/>` form. Without
    // this, callers that pre-`xml_escape` their label (every shape under
    // `src/render/shapes/*`) would emit a literal `&lt;br/&gt;` and measure
    // `<br/>` as 6 chars of text — see e.g. cypress fixtures 67 / 200 and
    // demos 06 / 07 where a diamond label spans three `<br/>`-separated
    // segments and the `<foreignObject>` width must equal the concatenated
    // segment widths (because `display: table-cell; white-space: nowrap`
    // disables soft-wrap on `<br/>`).
    if has_escaped_br(&processed) {
        let html = restore_escaped_br(&processed);
        let (w, h, clamped) = measure(&html);
        let mut opts = apply_style_overrides(
            LabelOpts::default(),
            w,
            h,
            x_offset,
            y_offset,
            &lbl_style,
            &div_prefix,
        );
        opts.clamped = clamped;
        return render_node_label(&html, w, h, &opts);
    }
    // Plain string labels with a literal `\n` need the same `<br/>` rewrite
    // as `<br>`-bearing labels — upstream's `parseGenericTypes` /
    // `parseEdge` converts every `\n` to `<br/>` before the label reaches
    // foreignObject. Without this, the rendered `<p>` body would contain a
    // raw `\n` (mismatching upstream's `<br/>` token). See cypress
    // fixture 251 — the diamond label `What is\nyourmermaid version?`
    // must render as `<p>What is<br/>yourmermaid version?</p>`, measured
    // two lines tall as the browser paints it.
    if processed.contains('\n') {
        let html: String = processed.split('\n').collect::<Vec<&str>>().join("<br/>");
        let (w, h, clamped) = measure(&html);
        let mut opts = apply_style_overrides(
            LabelOpts::default(),
            w,
            h,
            x_offset,
            y_offset,
            &lbl_style,
            &div_prefix,
        );
        opts.clamped = clamped;
        return render_node_label(&html, w, h, &opts);
    }
    // After `replace_fa_icons`, FA references like `fa:fa-code` become
    // `<i class="fa fa-code"></i>` markup. `measure_html_label` does
    // not strip HTML tags — it would measure the raw `<i …>` chars
    // as text. Detect any leftover `<i` (the only tag injection done
    // here) and route to `measure_html_markup_label` which strips
    // tags via the textContent rule.
    if processed.contains("<i ") {
        let (w, h, clamped) = measure(&processed);
        let mut opts = apply_style_overrides(
            LabelOpts::default(),
            w,
            h,
            x_offset,
            y_offset,
            &lbl_style,
            &div_prefix,
        );
        opts.clamped = clamped;
        return render_node_label(&processed, w, h, &opts);
    }
    let (w, h, clamped) = if clamp {
        measure_html_label_clamped(&processed, &clamp_font(), HTML_LABEL_WRAPPING_WIDTH)
    } else {
        let (w, h) = measure_html_label(&processed, font, HTML_LABEL_WRAPPING_WIDTH, true);
        (w, h, false)
    };
    let mut opts = apply_style_overrides(
        LabelOpts::default(),
        w,
        h,
        x_offset,
        y_offset,
        &lbl_style,
        &div_prefix,
    );
    opts.clamped = clamped;
    render_node_label(&processed, w, h, &opts)
}

/// Cheap pre-check: is there at least one `&lt;…&gt;` fragment whose body is
/// some `<br>` variant? Avoids the regex / case-fold cost on the common path.
fn has_escaped_br(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if &bytes[i..i + 4] == b"&lt;" {
            // Find the matching `&gt;` on the same line.
            let after = i + 4;
            let line_end = s[after..].find('\n').map(|n| after + n).unwrap_or(s.len());
            if let Some(rel) = s[after..line_end].find("&gt;") {
                let inner = s[after..after + rel]
                    .trim_end_matches('/')
                    .trim()
                    .trim_start_matches('/')
                    .trim();
                if inner.eq_ignore_ascii_case("br") {
                    return true;
                }
                i = after + rel + 4;
                continue;
            }
        }
        i += 1;
    }
    false
}

/// Replace every `&lt;br&gt;` / `&lt;br/&gt;` / `&lt;br /&gt;` (and their
/// case variants) with the canonical `<br/>` tag, leaving every other
/// `&lt;…&gt;` fragment untouched. Callers should run this only after
/// [`has_escaped_br`] returns true.
fn restore_escaped_br(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 4 <= bytes.len() && &bytes[i..i + 4] == b"&lt;" {
            let after = i + 4;
            let line_end = s[after..].find('\n').map(|n| after + n).unwrap_or(s.len());
            if let Some(rel) = s[after..line_end].find("&gt;") {
                let inner = s[after..after + rel]
                    .trim_end_matches('/')
                    .trim()
                    .trim_start_matches('/')
                    .trim();
                if inner.eq_ignore_ascii_case("br") {
                    out.push_str("<br/>");
                    i = after + rel + 4;
                    continue;
                }
            }
        }
        let ch_len = utf8_char_len(bytes[i]);
        out.push_str(&s[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// True iff the input contains at least one paired markdown inline
/// emphasis marker (`**bold**`, `*italic*`, or `` `code` ``).
fn has_paired_markdown_markers(s: &str) -> bool {
    // `**…**` — two stars then closing two stars on the same line.
    if let Some(open) = s.find("**") {
        if let Some(rel) = s[open + 2..].find("**") {
            // Close marker on same line and non-empty body.
            let body = &s[open + 2..open + 2 + rel];
            if !body.is_empty() && !body.contains('\n') {
                return true;
            }
        }
    }
    // `` `…` `` — backtick then closing backtick on the same line.
    if let Some(open) = s.find('`') {
        if let Some(rel) = s[open + 1..].find('`') {
            let body = &s[open + 1..open + 1 + rel];
            if !body.is_empty() && !body.contains('\n') {
                return true;
            }
        }
    }
    // Single-star italic — must avoid matching `**bold**` (already covered)
    // and the lone-star case. Walk left-to-right looking for a `*` whose
    // immediate predecessor is not also `*` and that has a non-empty
    // matching close.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes.get(i + 1) != Some(&b'*') && (i == 0 || bytes[i - 1] != b'*') {
            if let Some(rel) = s[i + 1..].find('*') {
                let body = &s[i + 1..i + 1 + rel];
                if !body.is_empty() && !body.contains('\n') {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

// ─── CSS-aware label measurement ───────────────────────────────────────

/// Font resolution for an HTML label rendered inside `<foreignObject>`.
///
/// The jsdom shim in `tests/support/generate_ref.mjs::resolveFont`
/// walks up the DOM looking for explicit `font-family` / `font-size` /
/// `font-weight` ATTRIBUTES or inline `style` values — CSS class rules
/// are IGNORED. In real browser consumers the generated SVG root declares
/// `font-size:16px` and the HTML label div declares `line-height: 1.5`,
/// so the fallback must match that rendered size instead of the older
/// jsdom-reference-only 14px assumption.
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
            self.font_family
                .unwrap_or("trebuchet ms,verdana,arial,sans-serif"),
            self.font_size_px.unwrap_or(HTML_LABEL_FONT_SIZE),
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
///
/// Markdown convenience: paired `**bold**`, `*italic*`, and `` `code` ``
/// inline markers are stripped before measurement. Upstream funnels
/// markdown-source labels through `markdownToHTML` before they reach the
/// foreignObject `<div>`, so what `textContent` returns is the marker-free
/// text. Some renderer paths (e.g. the diamond polygon shape) re-measure
/// node labels at draw time without going through that pipeline; matching
/// upstream's geometry requires us to strip the markers here too.
/// Unpaired markers are left in place — labels that *intend* to display
/// a literal `*` continue to measure correctly.
pub fn measure_html_label(
    text: &str,
    font: &HtmlLabelFont<'_>,
    max_width_px: f64,
    wrap_enabled: bool,
) -> (f64, f64) {
    let (family, size, bold) = font.resolve();
    // Fast path: empty string.
    if text.is_empty() {
        return (0.0, html_label_line_height(size));
    }
    // Upstream's parseEdge / parseGenericTypes converts every literal `\n`
    // in a label string to `<br/>` BEFORE the label reaches the
    // foreignObject — see
    // `vendor/mermaid/packages/mermaid/src/diagrams/flowchart/flowDb.ts`
    // and `parseEdge` in `utils.ts`. Our shape callers (diamond / circle /
    // cylinder / etc.) hand `xml_escape(label)` to this function with the
    // literal `\n` still present, so each `\n` is a forced line break here,
    // exactly as `<br/>` is in `measure_html_markup_label`.
    //
    // A browser breaks the line at every `<br/>` even under
    // `white-space: nowrap`, so the block is `lines × line-height` tall. (The
    // jsdom reference shim measured `textContent` as ONE line, which left
    // the second and later lines painting below the node box — markon #97.)
    // Width stays the concatenated-text width of the reference geometry.
    let stripped = strip_paired_markdown_markers(text);
    let lines = crate::layout::label_metrics::split_label_lines(&stripped).len();
    let concatenated: String = stripped.split('\n').collect();
    let concat_w = text_width(&concatenated, family, size, bold, false);
    let lh = html_label_line_height(size);
    let _ = (max_width_px, wrap_enabled); // wrapping is modelled by the *_clamped variants
    (concat_w, lh * lines as f64)
}

/// The `max-width` clamp a browser applies to a label `<div>`, and the wrap
/// upstream mermaid performs once it sees the clamp — issue #254.
///
/// Upstream `addHtmlSpan` measures the label div in a real DOM, so a div
/// carrying `max-width: Npx` (the flowchart `wrappingWidth`, 200 by default)
/// shrinks to `min(content width, N)`. When the measured bbox width equals `N`
/// it declares the label clamped and re-styles the div
/// (`display: table; white-space: break-spaces; width: Npx;`), which makes the
/// browser wrap the text inside `N` and grows the label to one line box per
/// painted line. Node boxes and label positions are then computed from that
/// wrapped box.
///
/// Headless measurement has no layout engine, so it must model both steps
/// itself: the shrink-to-fit width is the widest *painted line* (not the
/// concatenated text), and any line wider than `N` is re-wrapped to `N`.
///
/// The returned box is what the browser paints, which is not always `N` wide:
/// a `display: table` box never shrinks below its min-content width, so a word
/// wider than the budget (no break opportunity inside it — `overflow-wrap:
/// normal`) pushes the whole label — and the foreignObject the upstream
/// measurement feeds — wider than `N`. Upstream re-measures after the style
/// switch, so it reports that wider box too.
///
/// Only diagrams whose *layout* also reserves this box may measure through
/// here — see [`measure_html_markup_label_clamped`]. The shim under-measures
/// CJK, so the width estimate floors each glyph at the browser's rendered
/// width ([`crate::layout::label_metrics::cjk_aware_label_width`]): without it
/// a CJK label a browser clamps would be measured as fitting and stay
/// mis-aligned, which is the very symptom this fixes.
///
/// Returns `Some((width, height of the wrapped block))` when the browser would
/// clamp, `None` when the label fits and nothing changes.
pub fn clamp_html_label(
    text: &str,
    font: &HtmlLabelFont<'_>,
    max_width_px: f64,
    wrap_enabled: bool,
) -> Option<(f64, f64)> {
    if !wrap_enabled || !max_width_px.is_finite() || max_width_px <= 0.0 {
        return None;
    }
    let (family, size, bold) = font.resolve();
    // Glyph floors, so the estimate tracks the real painted width rather than
    // the shim's under-measurement. Gated on the label actually containing a
    // wide glyph, exactly like the other `cjk_aware_label_width` consumers, so
    // pure-Latin labels keep the shim's byte-exact geometry.
    let width_of = |s: &str| {
        use crate::layout::label_metrics::{is_wide_glyph, painted_line_width};
        let shim = text_width(s, family, size, bold, false);
        if s.chars().any(is_wide_glyph) {
            painted_line_width(s, shim, size, &|ch| {
                text_width(&ch.to_string(), family, size, bold, false)
            })
        } else {
            shim
        }
    };
    let painted: Vec<String> = crate::layout::label_metrics::split_label_lines(text)
        .iter()
        .map(|line| html_text_content(line))
        .collect();
    // Upstream compares the measured bbox against the wrap budget and takes
    // the clamped branch on equality; the browser's shrink-to-fit width is the
    // widest line, so that is what the comparison has to use.
    if !painted.iter().any(|line| width_of(line) >= max_width_px) {
        return None;
    }
    let mut wrapped = 0usize;
    // Min-content width of the whole label: the widest run the browser cannot
    // break, which is what `width: Npx` clamps against.
    let mut widest_segment = 0.0f64;
    for line in &painted {
        let segments =
            crate::layout::label_metrics::wrap_line_to_width(line, max_width_px, &width_of);
        wrapped += segments.len();
        for segment in segments {
            // A line holding one unbreakable run is that run's min-content
            // width; shorter lines were broken at a space and cannot drive the
            // table's min-content width up on their own.
            let trimmed = crate::layout::label_metrics::trim_trailing_breakable_spaces(segment);
            widest_segment = widest_segment.max(width_of(trimmed));
        }
    }
    Some((
        max_width_px.max(widest_segment),
        html_label_line_height(size) * wrapped as f64,
    ))
}

/// Measure an HTML label the way a diagram whose *layout* clamps does: the
/// shim box, replaced by the browser's clamped box when the label overflows the
/// wrapping width. Returns `(width, height, clamped)`; `clamped` belongs on
/// [`LabelOpts::clamped`] so the emitted `<div>` gets upstream's wrapping
/// style.
///
/// Only call this where the box was reserved the same way — flowchart node and
/// edge labels today (`layout::flowchart`). Diagrams whose layout still sizes
/// from the un-clamped text must keep [`measure_html_markup_label`], or their
/// labels wrap into boxes that were never grown for them.
pub fn measure_html_markup_label_clamped(
    text: &str,
    font: &HtmlLabelFont<'_>,
    max_width_px: f64,
) -> (f64, f64, bool) {
    match clamp_html_label(text, font, max_width_px, true) {
        Some((w, h)) => (w, h, true),
        None => {
            let (w, h) = measure_html_markup_label(text, font, max_width_px, true);
            (w, h, false)
        }
    }
}

/// Measure a node's HTML label under a diagram's clamp policy: `true` selects
/// the box the browser paints after clamping the label to the wrapping width,
/// `false` the plain shim measurement.
///
/// Every caller that has to agree with the emitted label — the viewBox passes
/// and the `font-size` / `bold` post-process passes above all — must reach this
/// decision the same way. Getting it wrong is silent: a clamped measurement on
/// a label that was *not* clamped makes the post-process needles miss, so the
/// geometry patch is dropped while the `<div>` keeps its `font-size: 30px
/// !important` and the text paints at a size nothing reserved room for.
pub fn measure_node_label_box(
    text: &str,
    font: &HtmlLabelFont<'_>,
    max_width_px: f64,
    clamp: bool,
) -> (f64, f64, bool) {
    if clamp {
        measure_html_markup_label_clamped(text, font, max_width_px)
    } else {
        let (w, h) = measure_html_markup_label(text, font, max_width_px, true);
        (w, h, false)
    }
}

/// [`measure_html_markup_label_clamped`] for `xml_escape`d shape labels — see
/// [`measure_html_label`] for the differences.
pub fn measure_html_label_clamped(
    text: &str,
    font: &HtmlLabelFont<'_>,
    max_width_px: f64,
) -> (f64, f64, bool) {
    match clamp_html_label(text, font, max_width_px, true) {
        Some((w, h)) => (w, h, true),
        None => {
            let (w, h) = measure_html_label(text, font, max_width_px, true);
            (w, h, false)
        }
    }
}

/// Strip paired markdown inline emphasis markers (`**bold**`, `*italic*`,
/// `` `code` ``) from a string while leaving unpaired markers intact.
///
/// Used by [`measure_html_label`] so that labels written as markdown source
/// (e.g. the flowchart diamond's `` `The **cat** in the hat` `` body) are
/// measured at the width that `textContent` would return after upstream's
/// `markdownToHTML` step. A label containing a literal `*` such as
/// `"a*b"` has only one `*` and no closing pair, so it is returned
/// unchanged.
fn strip_paired_markdown_markers(text: &str) -> String {
    // Inline-code spans `` `…` `` are stripped first because they are
    // delimited by a single character on each side and never nest.
    let after_code = strip_paired_with(text, "`", "`");
    // `**…**` must be checked before `*…*` so the longer marker wins.
    let after_strong = strip_paired_with(&after_code, "**", "**");
    strip_paired_with(&after_strong, "*", "*")
}

/// Replace each `<open>X<close>` pair in `s` with `X`, leaving any
/// unmatched `<open>` token literal. Operates left-to-right and never
/// crosses newlines so an unterminated marker on one line cannot consume
/// the next.
fn strip_paired_with(s: &str, open: &str, close: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if s[i..].starts_with(open) {
            // Look for the closing marker on the same line.
            let after = i + open.len();
            // For the `*` / `**` cases, do not match an empty body —
            // otherwise `**` (with no inner text) would fold into `""`.
            let nl = s[after..].find('\n').unwrap_or(s.len() - after);
            if let Some(rel) = s[after..after + nl].find(close) {
                if rel > 0 {
                    out.push_str(&s[after..after + rel]);
                    i = after + rel + close.len();
                    continue;
                }
            }
        }
        // Literal — copy a whole UTF-8 char (1..4 bytes).
        let ch_len = utf8_char_len(bytes[i]);
        out.push_str(&s[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// Width × height of a label where the input is already HTML markup
/// (tags like `<strong>`, `<br/>`, `<em>`, plus HTML entities).
///
/// This is the "post-`markdownToHTML`" measurement used by flowchart and
/// any other caller that hands `measure_html_*` a string it has already
/// converted to HTML. jsdom's `getBoundingClientRect` on a `<div>` built
/// from this markup measures the rendered `textContent` — tags do not
/// contribute width, `<br/>` collapses to a zero-width break, and HTML
/// entities are decoded back to their represented character.
///
/// Callers that pass **plain text** (even text that happens to contain a
/// literal `<` such as `<<requirement>>`) must use `measure_html_label`
/// instead — this function would otherwise strip the `<…>` fragment as a
/// (non-existent) tag.
pub fn measure_html_markup_label(
    text: &str,
    font: &HtmlLabelFont<'_>,
    max_width_px: f64,
    wrap_enabled: bool,
) -> (f64, f64) {
    let (family, size, base_bold) = font.resolve();
    if text.is_empty() {
        return (0.0, html_label_line_height(size));
    }
    let _ = (max_width_px, wrap_enabled);
    let lines = crate::layout::label_metrics::split_label_lines(text).len();
    let lh = html_label_line_height(size);
    // Height: one line-height per painted line. Width: the concatenated
    // text, as the reference geometry measures it.
    let total_w = text_width(&html_text_content(text), family, size, base_bold, false);
    (total_w, lh * lines as f64)
}

/// jsdom-`textContent`-style plain text of label markup: every tag
/// (including `<br>`) stripped, entities decoded, measured as one segment.
fn html_text_content(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // A `<` only starts an HTML tag when the next character is an
            // ASCII letter (open tag like `<br>`, `<strong>`) or a `/`
            // followed by a letter (close tag like `</strong>`). Anything
            // else (`<<`, `< `, `<1`, `<!`, `<?` without the full form, …)
            // is treated as literal text — matching how a real HTML parser
            // recovers from invalid tag starts and how jsdom's `textContent`
            // surfaces the offending `<` as a normal character.
            if let Some(len) = crate::layout::label_metrics::tag_len(html, i) {
                i += len;
                continue;
            }
            text.push('<');
            i += 1;
        } else if bytes[i] == b'&' {
            // HTML entity — decode to plain text.
            if let Some(semi_rel) = html[i..].find(';') {
                let entity = &html[i + 1..i + semi_rel];
                let ch = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some('\u{00A0}'),
                    _ => None,
                };
                if let Some(c) = ch {
                    text.push(c);
                    i += semi_rel + 1;
                    continue;
                }
            }
            text.push('&');
            i += 1;
        } else {
            // UTF-8-safe copy of the whole char (1..4 bytes).
            let ch_len = utf8_char_len(bytes[i]);
            text.push_str(&html[i..i + ch_len]);
            i += ch_len;
        }
    }
    text
}

/// Convert a markdown-syntax label string to rendered HTML for embedding
/// in a `<foreignObject>` label.
///
/// Rules (subset of mermaid's `markdownToLines`):
/// - `**text**` → `<strong>text</strong>`
/// - `*text*` → `<em>text</em>`
/// - `` `code` `` → `<code>code</code>`
/// - `<br>` / `<br/>` embedded HTML tags → passed through as `<br/>`
/// - plain text characters are XML-escaped (`>` → `&gt;`, etc.)
/// - `\n` is treated the same as `<br/>` → `<br/>`
///
/// This matches what upstream's `markdownToLines` + `dedupPostProcessor`
/// produce for the inline markdown labels used in flowchart nodes.
pub fn markdown_label_to_html(src: &str) -> String {
    // Mirror marked.lexer's paragraph splitting: blank-line-separated runs
    // become separate paragraph tokens. Each paragraph drops its trailing
    // whitespace during tokenisation. Within a paragraph, single newlines
    // remain (and become `<br/>` here, matching GFM `breaks: true`).
    //
    // The caller (foreign_object_body with wrap_in_p=true) wraps the result
    // in a single `<p>…</p>`, so we emit `</p><p>` between paragraphs to
    // get the upstream `<p>p1</p><p>p2</p>` shape.
    let paragraphs = split_markdown_paragraphs(src);
    if paragraphs.len() <= 1 {
        return markdown_paragraph_to_html(paragraphs.first().map(|s| s.as_str()).unwrap_or(src));
    }
    let mut out = String::with_capacity(src.len() * 2);
    for (i, para) in paragraphs.iter().enumerate() {
        if i > 0 {
            out.push_str("</p><p>");
        }
        out.push_str(&markdown_paragraph_to_html(para));
    }
    out
}

/// Split a markdown source into paragraph chunks at runs of blank lines
/// (lines containing only whitespace), and trim trailing whitespace
/// from each chunk. Mirrors marked.lexer's paragraph tokenisation.
///
/// Returns a single-element vec containing the whole input (with trailing
/// whitespace trimmed) when there are no blank-line separators.
fn split_markdown_paragraphs(src: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut blank_run = 0usize;
    for line in src.split_inclusive('\n') {
        // Strip the trailing '\n' (if any) to test for blank-ness.
        let body = line.strip_suffix('\n').unwrap_or(line);
        let is_blank = body.chars().all(|c| c.is_whitespace());
        if is_blank {
            if !current.is_empty() {
                // End the current paragraph; trailing whitespace dropped.
                out.push(current.trim_end().to_string());
                current = String::new();
            }
            blank_run += 1;
        } else {
            blank_run = 0;
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        out.push(current.trim_end().to_string());
    }
    if out.is_empty() {
        // Source was empty or all-whitespace — preserve original behaviour
        // (the caller's wrap_in_p will emit no `<p>` for an empty body).
        out.push(String::new());
    }
    let _ = blank_run;
    out
}

/// Convert a single markdown paragraph (no blank-line splits) to HTML.
fn markdown_paragraph_to_html(src: &str) -> String {
    let mut out = String::with_capacity(src.len() * 2);
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // Check for **bold** or *italic*
        if b == b'*' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                // **bold**
                if let Some(end) = src[i + 2..].find("**") {
                    let inner = &src[i + 2..i + 2 + end];
                    out.push_str("<strong>");
                    out.push_str(&xml_escape_label(inner));
                    out.push_str("</strong>");
                    i += 2 + end + 2;
                    continue;
                }
            }
            // *italic*
            if let Some(end) = src[i + 1..].find('*') {
                if end > 0 {
                    let inner = &src[i + 1..i + 1 + end];
                    out.push_str("<em>");
                    out.push_str(&xml_escape_label(inner));
                    out.push_str("</em>");
                    i += 1 + end + 1;
                    continue;
                }
            }
            // bare *, treat as literal
            out.push('*');
            i += 1;
        } else if b == b'`' {
            // inline code: `code`
            if let Some(end) = src[i + 1..].find('`') {
                let inner = &src[i + 1..i + 1 + end];
                out.push_str("<code>");
                out.push_str(&xml_escape_label(inner));
                out.push_str("</code>");
                i += 1 + end + 1;
                continue;
            }
            out.push('`');
            i += 1;
        } else if b == b'<' {
            // Embedded HTML tag — pass through (with normalisation of <br> → <br/>)
            if let Some(rel_end) = src[i..].find('>') {
                let tag = &src[i..i + rel_end + 1];
                let inner = tag[1..tag.len() - 1].trim();
                let tag_lc = inner.trim_end_matches('/').trim().to_ascii_lowercase();
                if tag_lc == "br" {
                    out.push_str("<br/>");
                } else {
                    out.push_str(tag); // pass through other tags verbatim
                }
                i += rel_end + 1;
            } else {
                out.push_str("&lt;");
                i += 1;
            }
        } else if b == b'\n' {
            out.push_str("<br/>");
            i += 1;
        } else {
            // Plain text — XML-escape ASCII metacharacters; pass any other
            // bytes through as part of their parent UTF-8 char so that CJK /
            // emoji / accented text survives intact.
            match b {
                b'&' => {
                    out.push_str("&amp;");
                    i += 1;
                }
                b'>' => {
                    out.push_str("&gt;");
                    i += 1;
                }
                b'"' => {
                    out.push_str("&quot;");
                    i += 1;
                }
                _ => {
                    let ch_len = utf8_char_len(b);
                    out.push_str(&src[i..i + ch_len]);
                    i += ch_len;
                }
            }
        }
    }
    out
}

/// XML-escape a plain text segment (for use within HTML element content).
fn xml_escape_label(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'&' => {
                out.push_str("&amp;");
                i += 1;
            }
            b'<' => {
                out.push_str("&lt;");
                i += 1;
            }
            b'>' => {
                out.push_str("&gt;");
                i += 1;
            }
            b'"' => {
                out.push_str("&quot;");
                i += 1;
            }
            b => {
                // UTF-8-safe: copy the entire char (1..4 bytes).
                let ch_len = utf8_char_len(b);
                out.push_str(&s[i..i + ch_len]);
                i += ch_len;
            }
        }
    }
    out
}

/// Convert a "string"-type label (double-quoted string) to HTML for
/// embedding in a `<foreignObject>`.
///
/// In upstream mermaid, double-quoted labels may contain embedded HTML tags
/// (e.g. `<strong>text</strong>`) which are rendered as HTML. Text content
/// outside of tags has `>` escaped to `&gt;` and `&` to `&amp;`. This
/// matches the browser's serialization behavior (innerHTML round-trip).
///
/// Rules:
/// - `\n` → `<br/>` (converted to `<br/>` in rendering, stripped by textContent)
/// - `<letter` / `</letter` / `<!` — HTML tag start, pass through INCLUDING its closing `>`
/// - `<br>` / `<br />` / `<br/>` — normalised to upstream's canonical
///   `<br/>` form regardless of whitespace / case in the source. Upstream
///   `markdownToHTML` always re-serialises `<br>` variants as `<br/>` so
///   matching the cypress fixtures requires the same canonicalisation here.
/// - `<` NOT followed by tag-start char (e.g. `< 4`) → `&lt;` (text content)
/// - `>` in text content → `&gt;`
/// - `&` in text content → `&amp;`
pub fn string_label_to_html(src: &str) -> String {
    let mut out = String::with_capacity(src.len() * 2);
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\n' {
            out.push_str("<br/>");
            i += 1;
        } else if b == b'<' {
            // Check if this starts an HTML tag (letter, '/', '!' follows).
            let next = bytes.get(i + 1).copied().unwrap_or(0);
            if next.is_ascii_alphabetic() || next == b'/' || next == b'!' {
                // HTML tag — pass through the entire tag (including its closing `>`).
                // Capture the entire tag span to detect the `<br>` family and
                // normalise to upstream's canonical `<br/>` form.
                let tag_start = i;
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b'>' {
                    j += 1;
                }
                let tag_end = if j < bytes.len() { j + 1 } else { j };
                let inner = &src[tag_start + 1..tag_end.saturating_sub(1).max(tag_start + 1)];
                let inner_trim = inner
                    .trim_end_matches('/')
                    .trim()
                    .trim_start_matches('/')
                    .trim();
                if inner_trim.eq_ignore_ascii_case("br") {
                    out.push_str("<br/>");
                } else {
                    // Pass through verbatim.
                    out.push_str(&src[tag_start..tag_end]);
                }
                i = tag_end;
            } else {
                // Not a valid HTML tag start — treat as literal `<` in text content.
                out.push_str("&lt;");
                i += 1;
            }
        } else if b == b'>' {
            // `>` in text content — escape it.
            out.push_str("&gt;");
            i += 1;
        } else if b == b'&' {
            // `&` in text content — escape it.
            out.push_str("&amp;");
            i += 1;
        } else {
            // UTF-8-safe copy of the entire char (1..4 bytes) — naked
            // `bytes[i] as char` would shred multibyte sequences (CJK,
            // emoji, accented Latin) into Latin-1 supplements and emit
            // mojibake into the SVG.
            let ch_len = utf8_char_len(b);
            out.push_str(&src[i..i + ch_len]);
            i += ch_len;
        }
    }
    out
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
    fn clamp_fires_on_the_widest_painted_line_not_the_concatenation() {
        let font = HtmlLabelFont::default();
        let max = HTML_LABEL_WRAPPING_WIDTH;
        // Two lines that fit side by side in no browser: concatenated they
        // measure past the budget (~260 px), each line is well under it
        // (~130 px). A concatenation test would clamp here; a browser would
        // not.
        let markup = "aaaaaaaaaaaaaaa<br/>bbbbbbbbbbbbbbb";
        assert!(clamp_html_label(markup, &font, max, true).is_none());
        // A single line past the budget clamps, and the block grows to the
        // wrapped line count (24 px per line at the label font size).
        let long = format!("<p>{}<br/>x</p>", "word ".repeat(60));
        let (w, h) = clamp_html_label(&long, &font, max, true).expect("clamped");
        assert_eq!(w, max);
        assert!(h > 2.0 * html_label_line_height(HTML_LABEL_FONT_SIZE));
        // `wrap_enabled = false` is upstream's `width === Infinity` case.
        assert!(clamp_html_label(&long, &font, max, false).is_none());
    }

    #[test]
    fn narrow_glyphs_do_not_make_a_label_look_over_budget() {
        let font = HtmlLabelFont::default();
        // 22 `i`s (~4.4 px each in the shim) plus one ideograph: a browser
        // paints ~116 px and leaves it alone. Billing every glyph a flat
        // 0.56em would report 213 px and clamp it — the box would then grow
        // for text that never wraps.
        let markup = format!("<p>{}\u{4E2D}</p>", "i".repeat(22));
        assert!(
            clamp_html_label(&markup, &font, HTML_LABEL_WRAPPING_WIDTH, true).is_none(),
            "narrow-dense label must not be clamped"
        );
        // The same label with enough ideographs to genuinely overflow does
        // clamp.
        let wide = format!("<p>{}</p>", "\u{4E2D}".repeat(15));
        assert!(clamp_html_label(&wide, &font, HTML_LABEL_WRAPPING_WIDTH, true).is_some());
    }

    #[test]
    fn clamped_width_never_goes_below_the_widest_unbreakable_run() {
        let font = HtmlLabelFont::default();
        let max = HTML_LABEL_WRAPPING_WIDTH;
        // A word with no break opportunity: `display: table` cannot shrink
        // below its min-content width, so the browser paints (and upstream
        // reports) the word's own width — a box fixed at the budget would clip
        // the text instead.
        let word = "W".repeat(40);
        let (w, h) =
            clamp_html_label(&format!("<p>{word}</p>"), &font, max, true).expect("clamped");
        assert!(w > max, "width {w} must follow the unbreakable run");
        assert_eq!(h, html_label_line_height(HTML_LABEL_FONT_SIZE));
        // Widest *run*, not widest line: a line of several words stays at the
        // budget even though it is wider than any single word.
        let (w, _) = clamp_html_label(&format!("<p>{}</p>", "word ".repeat(60)), &font, max, true)
            .expect("clamped");
        assert_eq!(w, max);
    }

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
        let opts = LabelOpts {
            max_width: f64::INFINITY,
            ..LabelOpts::default()
        };
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
    fn edge_label_padding_is_opt_in() {
        let opts = LabelOpts {
            data_id: Some("L_DataStore_Process_0"),
            group_style: None,
            horizontal_padding: 4.0,
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

        assert!(got.contains(r#"width="44.01171875""#));
        assert!(got.contains(r#"transform="translate(-22.005859375, -8.1484375)""#));
        assert!(got.contains("padding: 0 4px;"));
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
            group_transform: Some("translate(0, 0)".into()),
            ..LabelOpts::default()
        };
        let got = render_node_label("", 0.0, 0.0, &opts);
        // The outer `<g class="edgeLabel" …>` is omitted — this is the
        // inner "label" only; caller can compose it.
        assert!(got.contains(r#"height="0""#));
        assert!(got.contains(r#"<span class="edgeLabel "></span>"#));
        assert!(!got.contains("<p>"));
    }

    #[test]
    fn measure_html_label_uses_browser_default_line_height() {
        // Browser-rendered Mermaid uses 16px HTML labels with `line-height: 1.5`.
        let (w, h) = measure_html_label("id1", &HtmlLabelFont::default(), 200.0, true);
        let (family, size, bold) = HtmlLabelFont::default().resolve();
        let expected_w = text_width("id1", family, size, bold, false);
        let expected_h = html_label_line_height(size);
        assert!((w - expected_w).abs() < 1e-9);
        assert!((h - expected_h).abs() < 1e-9);
        assert_eq!(h, 24.0);
    }

    #[test]
    fn measure_html_label_treats_newline_as_br() {
        // Upstream `parseGenericTypes` / `parseEdge` converts `\n` to `<br/>`
        // before the label reaches foreignObject, and a browser breaks the
        // line there: the block is two lines tall (markon #97). Width stays
        // the concatenated text.
        let (w, h) = measure_html_label("a\nbb", &HtmlLabelFont::default(), 200.0, true);
        let (family, size, bold) = HtmlLabelFont::default().resolve();
        let lh = html_label_line_height(size);
        assert!(
            (h - 2.0 * lh).abs() < 1e-9,
            "h={h} expected two lines {}",
            2.0 * lh
        );
        let expected_w = text_width("abb", family, size, bold, false);
        assert!(
            (w - expected_w).abs() < 1e-9,
            "w={w} expected concat width {expected_w}"
        );
    }

    #[test]
    fn measure_html_markup_label_breaks_lines_at_every_br_form() {
        let font = HtmlLabelFont::default();
        let (family, size, bold) = font.resolve();
        let lh = html_label_line_height(size);
        for markup in [
            "line1<br/>line22",
            "line1<br>line22",
            "line1<br />line22",
            "line1<BR>line22",
        ] {
            let (w, h) = measure_html_markup_label(markup, &font, 200.0, true);
            assert_eq!(h, 2.0 * lh, "{markup:?} should measure two lines");
            // Width keeps the concatenated-text reference geometry.
            assert_eq!(
                w,
                text_width("line1line22", family, size, bold, false),
                "{markup:?}"
            );
        }
        // Other tags are stripped without breaking the line.
        let (w, h) = measure_html_markup_label("<b>ab</b>c", &font, 200.0, true);
        assert_eq!(h, lh);
        assert_eq!(w, text_width("abc", family, size, bold, false));
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

    #[test]
    fn normalize_br_tag_variants() {
        // Every `<br>` variant should canonicalise to `<br/>`.
        assert_eq!(normalize_br_tags("a<br>b"), "a<br/>b");
        assert_eq!(normalize_br_tags("a<br/>b"), "a<br/>b");
        assert_eq!(normalize_br_tags("a<br />b"), "a<br/>b");
        assert_eq!(normalize_br_tags("a<BR>b"), "a<br/>b");
        assert_eq!(normalize_br_tags("a<br\t/>b"), "a<br/>b");
        // `< br>` is not a valid tag start, so pass it through verbatim.
        assert_eq!(normalize_br_tags("a< br>b"), "a< br>b");
        // Other tags pass through verbatim.
        assert_eq!(
            normalize_br_tags("<strong>hi</strong>"),
            "<strong>hi</strong>"
        );
        // Unterminated `<` stays literal.
        assert_eq!(normalize_br_tags("a<b"), "a<b");
    }

    #[test]
    fn shape_label_block_restores_xml_escaped_br() {
        // Diamond / circle / cylinder / etc. shapes pre-escape the label via
        // `xml_escape` before calling `shape_label_block`, which turns
        // literal `<br/>` source into `&lt;br/&gt;`. The block must restore
        // the canonical `<br/>` tag so the rendered foreignObject contains
        // a real line break and the width sums all segments as a single
        // text-content line (cypress/67, cypress/200, demos/06, demos/07).
        let got = shape_label_block("Multi&lt;br/&gt;Line", &HtmlLabelFont::default(), false);
        // Final `<p>` body must carry the live `<br/>` tag, not the entity.
        assert!(
            got.contains("<p>Multi<br/>Line</p>"),
            "expected `<p>Multi<br/>Line</p>` body, got {got}"
        );
        // `<br>` and `<br />` variants normalise the same way.
        let got2 = shape_label_block(
            "A&lt;br&gt;B&lt;br /&gt;C",
            &HtmlLabelFont::default(),
            false,
        );
        assert!(
            got2.contains("<p>A<br/>B<br/>C</p>"),
            "expected canonical <br/> form, got {got2}"
        );
        // Non-br escaped tags must NOT be restored — only `<br>` family.
        let got3 = shape_label_block(
            "a&lt;span&gt;b&lt;/span&gt;c",
            &HtmlLabelFont::default(),
            false,
        );
        assert!(
            got3.contains("&lt;span&gt;"),
            "non-br escaped tags must remain escaped, got {got3}"
        );
    }

    #[test]
    fn string_label_to_html_normalises_br_variants() {
        // The flowchart fixtures cypress/81/89/90/91/214 author labels
        // with literal `<br>` and `<br />` — upstream's `markdownToHTML`
        // re-serialises them all to `<br/>` before emission, which the
        // jsdom shim then renders inside `<p>…</p>`. Match that exactly.
        assert_eq!(string_label_to_html("Multi<br>Line"), "Multi<br/>Line");
        assert_eq!(string_label_to_html("Multi<br />Line"), "Multi<br/>Line");
        assert_eq!(string_label_to_html("Multi<br/>Line"), "Multi<br/>Line");
        assert_eq!(string_label_to_html("Multi<BR>Line"), "Multi<br/>Line");
        // Other tags still pass through unchanged (verbatim).
        assert_eq!(
            string_label_to_html("a<strong>b</strong>c"),
            "a<strong>b</strong>c"
        );
    }

    // ─── FA token lexicon (issue #242) ─────────────────────────────────
    //
    // Upstream `createText.ts::replaceIconSubstring` matches
    // `/(fa[bklrs]?):fa-([\w-]+)/g` against JavaScript's ASCII `\w`. The
    // measuring side consumes exactly the spans this side renders, so these
    // cases pin the lexicon and the two sides' agreement.

    #[test]
    fn fa_icon_lexicon_matches_upstream_ascii_word() {
        // `_` is a name character: `car_x` is one icon name, not `car` plus
        // a stray `_x` measured as text.
        assert_eq!(
            replace_fa_icons("fa:fa-car_x"),
            r#"<i class="fa fa-car_x"></i>"#
        );
        // cjk-allow: the CJK glyph is the token boundary under test.
        let glyph = "中";
        assert_eq!(
            replace_fa_icons(&format!("fa:fa-car{glyph}")),
            format!(r#"<i class="fa fa-car"></i>{glyph}"#)
        );
        // A name-less `fa:fa-` is literal text on both sides.
        assert_eq!(replace_fa_icons("fa:fa-"), "fa:fa-");
        assert_eq!(strip_fa_icons("fa:fa-"), "fa:fa-");
        // Every style prefix upstream's `fa[bklrs]?` accepts.
        for prefix in ["fa", "fab", "fak", "fal", "far", "fas"] {
            let label = format!("{prefix}:fa-car");
            assert_eq!(
                replace_fa_icons(&label),
                format!(r#"<i class="{prefix} fa-car"></i>"#),
                "prefix {prefix}"
            );
        }
        // Not tokens: an unknown prefix letter, an uppercase spelling
        // (upstream's regex is case-sensitive), a word merely containing
        // `fa`, and a rejected longer prefix that must not fall back to
        // `fa:` in front of it.
        for label in ["faq:fa-car", "FA:FA-car", "sofa", "fa", "fabx:fa-car"] {
            assert_eq!(replace_fa_icons(label), label, "label {label}");
        }
        // Consecutive tokens both convert, and the gap is copied verbatim.
        assert_eq!(
            replace_fa_icons("fa:fa-a and fas:fa-b"),
            r#"<i class="fa fa-a"></i> and <i class="fas fa-b"></i>"#
        );
    }

    #[test]
    fn measuring_and_rendering_consume_the_same_fa_tokens() {
        // The regression this guards (issue #242): the two sides used to scan
        // with different alphabets, so a token's tail was measured as text
        // while being painted as part of the icon (or the reverse). Removing
        // the `<i>` markup this side emits must reproduce the measuring side
        // byte for byte.
        let labels = [
            "fa:fa-car Car",
            "fa:fa-car_x",
            "fa:fa-car中", // cjk-allow: CJK glyphs are the token-boundary cases under test.
            "fa:fa-car的图标",
            "fa:fa-",
            "fa:fa-<br/>Longer text here",
            "fa:fa-car<br/>Longer text",
            "sofa fa:fa-a",
            "before fab:fa-github after",
            "fa:fa-a fa:fa-b",
            "fak:fa-x|fal:fa-y",
        ];
        for label in labels {
            let replaced = replace_fa_icons(label);
            let mut stripped = String::new();
            let mut rest = replaced.as_str();
            while let Some(start) = rest.find("<i class=\"") {
                stripped.push_str(&rest[..start]);
                let past_close = rest[start..].find("></i>").expect("icon close tag") + start + 5;
                rest = &rest[past_close..];
            }
            stripped.push_str(rest);
            assert_eq!(stripped, strip_fa_icons(label), "label: {label}");
        }
    }
}
