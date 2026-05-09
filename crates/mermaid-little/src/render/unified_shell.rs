//! Unified SVG shell + `<g>` hierarchy helpers for Stratum 3 diagrams.
//!
//! Upstream mermaid wraps every non-bespoke diagram (er / block / class
//! / state / flowchart / requirement) in a common shell produced by
//! `mermaidAPI.appendDivSvgG` + `setupViewPortForSVG`. The outer frame
//! looks like:
//!
//! ```svg
//! <svg id viewBox style class role aria-roledescription>
//!   <style>…</style>
//!   <g></g>                <!-- seed group appendDivSvgG emits -->
//!   <defs>…markers…</defs>
//!   <g class="root">
//!     <g class="clusters">…</g>
//!     <g class="edgePaths">…</g>
//!     <g class="edgeLabels">…</g>
//!     <g class="nodes">…</g>
//!   </g>
//!   <defs><filter id="*-drop-shadow">…</filter></defs>
//!   <defs><filter id="*-drop-shadow-small">…</filter></defs>
//! </svg>
//! ```
//!
//! Up to Wave 4 each renderer hand-assembled this shell. This module
//! exposes helpers the renderers can call to share:
//!
//! * the attribute-order on the outer `<svg>`;
//! * the `<g>` hierarchy (seed, root, clusters, edgePaths, …);
//! * the drop-shadow filter defs;
//! * the base64 `data-points` serializer.

use crate::layout::unified::types::Point;
use crate::theme::ThemeVariables;

/// Canonical SVG attribute order emitted by mermaid's `appendDivSvgG`
/// + `setupViewPortForSVG`: `id → width → xmlns → class → style →
/// viewBox → role → aria-roledescription`.
///
/// `class` may be empty for a handful of diagram kinds where upstream
/// doesn't set it (e.g. the `block` family). Pass `None` to omit the
/// attribute entirely.
#[must_use]
pub fn open_unified_svg(
    id: &str,
    max_width: f64,
    view_box: (f64, f64, f64, f64),
    class_attr: Option<&str>,
    aria: &str,
) -> String {
    open_unified_svg_with_a11y(id, max_width, view_box, class_attr, aria, false, false)
}

/// Like [`open_unified_svg`] but with optional accessibility attributes
/// (`aria-describedby` / `aria-labelledby`) appended after
/// `aria-roledescription` when the diagram declares `accDescr` / `accTitle`.
///
/// Upstream emits these only when the diagram source contains the
/// corresponding directives, with element ids `chart-desc-{id}` /
/// `chart-title-{id}`.
#[must_use]
pub fn open_unified_svg_with_a11y(
    id: &str,
    max_width: f64,
    view_box: (f64, f64, f64, f64),
    class_attr: Option<&str>,
    aria: &str,
    has_acc_descr: bool,
    has_acc_title: bool,
) -> String {
    let (vx, vy, vw, vh) = view_box;
    let class_frag = match class_attr {
        Some(c) => format!(r#" class="{c}""#),
        None => String::new(),
    };
    let mut a11y = String::new();
    if has_acc_descr {
        a11y.push_str(&format!(r#" aria-describedby="chart-desc-{id}""#));
    }
    if has_acc_title {
        a11y.push_str(&format!(r#" aria-labelledby="chart-title-{id}""#));
    }
    format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg"{cls} style="max-width: {mw}px;" viewBox="{vx} {vy} {vw} {vh}" role="graphics-document document" aria-roledescription="{aria}"{a11y}>"#,
        id = id,
        cls = class_frag,
        mw = fmt_num(max_width),
        vx = fmt_num(vx),
        vy = fmt_num(vy),
        vw = fmt_num(vw),
        vh = fmt_num(vh),
        aria = aria,
        a11y = a11y,
    )
}

/// Emit `<title>` and/or `<desc>` accessibility elements immediately after
/// opening the `<svg>` tag. Call this right after `open_unified_svg_with_a11y`
/// when `acc_title` or `acc_descr` are present.
#[must_use]
pub fn emit_a11y_elements(id: &str, acc_title: Option<&str>, acc_descr: Option<&str>) -> String {
    let mut out = String::new();
    if let Some(t) = acc_title {
        out.push_str(&format!(
            r#"<title id="chart-title-{id}">{t}</title>"#,
            t = xml_escape_text(t)
        ));
    }
    if let Some(d) = acc_descr {
        out.push_str(&format!(
            r#"<desc id="chart-desc-{id}">{d}</desc>"#,
            d = xml_escape_text(d)
        ));
    }
    out
}

fn xml_escape_text(s: &str) -> std::borrow::Cow<'_, str> {
    if s.chars().any(|c| matches!(c, '<' | '>' | '&' | '"' | '\'')) {
        let mut result = String::with_capacity(s.len() + 8);
        for c in s.chars() {
            match c {
                '<' => result.push_str("&lt;"),
                '>' => result.push_str("&gt;"),
                '&' => result.push_str("&amp;"),
                '"' => result.push_str("&quot;"),
                '\'' => result.push_str("&#39;"),
                _ => result.push(c),
            }
        }
        std::borrow::Cow::Owned(result)
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

/// Closing tag of the `<svg>` shell.
#[must_use]
pub const fn close_unified_svg() -> &'static str {
    "</svg>"
}

/// Emit the empty `<g></g>` that upstream's `appendDivSvgG` leaves as
/// the seed child of the `<svg>`. Use this form only for renderers
/// that emit **no** child content inside the seed group (rare — the
/// Stratum 3 diagrams all use [`open_seed_group`] / [`close_seed_group`]
/// instead since they pack markers + root hierarchy inside).
#[must_use]
pub const fn seed_group() -> &'static str {
    "<g></g>"
}

/// Open the seed `<g>` tag. Pair with [`close_seed_group`]. Every
/// Stratum-3 renderer uses this form because upstream mermaid's
/// dagre-unified pipeline appends its markers, root, and node /
/// edge groups directly into the seed group produced by
/// `appendDivSvgG`, rather than emitting a separate wrapper.
#[must_use]
pub const fn open_seed_group() -> &'static str {
    "<g>"
}

/// Close tag for [`open_seed_group`].
#[must_use]
pub const fn close_seed_group() -> &'static str {
    "</g>"
}

/// Open the root `<g class="root">` container.
#[must_use]
pub const fn open_root_group() -> &'static str {
    r#"<g class="root">"#
}

/// Close the root group.
#[must_use]
pub const fn close_root_group() -> &'static str {
    "</g>"
}

/// Open a sub-layer of the root group — one of `clusters`, `edgePaths`,
/// `edgeLabels`, `nodes`. Upstream's dagre → SVG pipeline emits all
/// four groups unconditionally, even when empty.
#[must_use]
pub fn open_layer(name: &str) -> String {
    format!(r#"<g class="{name}">"#)
}

/// Close the currently-open sub-layer.
#[must_use]
pub const fn close_layer() -> &'static str {
    "</g>"
}

/// Emit the optional `<linearGradient>` definition that upstream's
/// `render6` pipeline appends to the SVG when
/// [`ThemeVariables::use_gradient`] is `Some(true)`. Returns an empty
/// string when the theme does not request a gradient.
///
/// Layout matches upstream byte-for-byte:
///
/// ```svg
/// <linearGradient id="{id}-gradient" gradientUnits="objectBoundingBox"
///   x1="0%" y1="0%" x2="100%" y2="0%">
///   <stop offset="0%"   stop-color="{gradientStart}" stop-opacity="1"></stop>
///   <stop offset="100%" stop-color="{gradientStop}"  stop-opacity="1"></stop>
/// </linearGradient>
/// ```
///
/// Upstream reference (`mermaid@11.14.0` minified):
/// `src/rendering-util/render.ts → render6` — appends the gradient
/// directly under the root `<svg>` after the two drop-shadow `<defs>`.
#[must_use]
pub fn emit_gradient_defs(id: &str, theme: &ThemeVariables) -> String {
    if theme.use_gradient != Some(true) {
        return String::new();
    }
    // When `useGradient` is true, upstream always supplies both endpoints
    // (theme-base/dark/forest/neutral all populate `gradientStart` /
    // `gradientStop`). Empty fall-backs are defensive: even an unset
    // start/stop produces a syntactically valid `<stop>` (mermaid's JS
    // would emit `stop-color="undefined"`).
    let start = theme.gradient_start.as_deref().unwrap_or("");
    let stop = theme.gradient_stop.as_deref().unwrap_or("");
    format!(
        concat!(
            r#"<linearGradient id="{id}-gradient" gradientUnits="objectBoundingBox" x1="0%" y1="0%" x2="100%" y2="0%">"#,
            r#"<stop offset="0%" stop-color="{start}" stop-opacity="1"></stop>"#,
            r#"<stop offset="100%" stop-color="{stop}" stop-opacity="1"></stop>"#,
            r#"</linearGradient>"#
        ),
        id = id,
        start = start,
        stop = stop,
    )
}

/// Emit the trailing drop-shadow filter `<defs>` pair that upstream
/// unconditionally appends after the root group.
///
/// * `include_regular` — emit the `<id>-drop-shadow` filter (4px
///   dx/dy, 130% dims). Every diagram in Wave 4 passes `true`.
/// * `include_small` — emit the `<id>-drop-shadow-small` filter (2px
///   dx/dy, 150% dims). Every diagram in Wave 4 passes `true`.
///
/// The two flags exist so callers can gate emission on
/// `themeVariables.useGradient` / `look=neo`, though current upstream
/// always emits both.
#[must_use]
pub fn emit_defs_shell(id: &str, include_regular: bool, include_small: bool) -> String {
    let mut s = String::with_capacity(256);
    if include_regular {
        s.push_str(&format!(
            r##"<defs><filter id="{id}-drop-shadow" height="130%" width="130%"><feDropShadow dx="4" dy="4" stdDeviation="0" flood-opacity="0.06" flood-color="#000000"></feDropShadow></filter></defs>"##,
            id = id,
        ));
    }
    if include_small {
        s.push_str(&format!(
            r##"<defs><filter id="{id}-drop-shadow-small" height="150%" width="150%"><feDropShadow dx="2" dy="2" stdDeviation="0" flood-opacity="0.06" flood-color="#000000"></feDropShadow></filter></defs>"##,
            id = id,
        ));
    }
    s
}

/// Serialize a polyline into the `data-edge` + `data-points` attribute
/// pair that upstream's unified renderer stamps on every `<path>`.
///
/// Upstream emits this as `data-edge="true" data-points="<base64>"`,
/// where the base64 encodes `JSON.stringify(points)` with each point
/// rendered as `{"x":…,"y":…}` (numeric fields use JS `Number#toString`
/// semantics — integers print without `.0`).
#[must_use]
pub fn data_edge_attrs(points: &[Point]) -> String {
    let mut json = String::from("[");
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            r#"{{"x":{x},"y":{y}}}"#,
            x = fmt_num(p.x),
            y = fmt_num(p.y),
        ));
    }
    json.push(']');
    let b64 = base64_encode(json.as_bytes());
    format!(r#"data-edge="true" data-points="{b64}""#)
}

/// JS `btoa()`-compatible base64 — plain alphabet, no line wrapping,
/// `=` padding.
///
/// Exposed for the tiny handful of call-sites (ER edge renderer)
/// that want just the base64 without the surrounding `data-*` attrs.
#[must_use]
pub fn base64_encode(data: &[u8]) -> String {
    const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut chunks = data.chunks_exact(3);
    for c in &mut chunks {
        let n = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | (c[2] as u32);
        out.push(TBL[((n >> 18) & 0x3f) as usize] as char);
        out.push(TBL[((n >> 12) & 0x3f) as usize] as char);
        out.push(TBL[((n >> 6) & 0x3f) as usize] as char);
        out.push(TBL[(n & 0x3f) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        0 => {}
        1 => {
            let n = (rem[0] as u32) << 16;
            out.push(TBL[((n >> 18) & 0x3f) as usize] as char);
            out.push(TBL[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            out.push(TBL[((n >> 18) & 0x3f) as usize] as char);
            out.push(TBL[((n >> 12) & 0x3f) as usize] as char);
            out.push(TBL[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => unreachable!(),
    }
    out
}

/// Sanitize a stroke-color string into the DOM id suffix upstream uses
/// for per-color marker variants.
///
/// Upstream `edgeMarker.ts` (`addEdgeMarker`) builds the colored marker
/// id as:
///
/// ```js
/// const colorId = strokeColor.replace(/[^\dA-Za-z]/g, '_');
/// const coloredMarkerId = `${originalMarkerId}_${colorId}`;
/// ```
///
/// So `#f66` becomes `_f66` and ` orange` becomes `_orange`. Combined
/// with the leading separator underscore, the final id reads like
/// `pointEnd__f66` / `pointEnd__orange` (two underscores).
#[must_use]
pub fn marker_color_id(stroke_color: &str) -> String {
    let mut out = String::with_capacity(stroke_color.len());
    for c in stroke_color.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

/// Emit a colored variant of one of the flowchart point/circle/cross
/// markers. `family` is the marker family name as known by
/// [`crate::render::markers`] (e.g. `"point"`, `"circle"`, `"cross"`).
/// `position` is `"End"` or `"Start"`. `id_prefix` / `kind` mirror the
/// arguments passed to [`crate::render::markers::defs`].
///
/// `stroke_color` is the raw style value found after `stroke:` in the
/// edge `pathStyle`. Returns the empty string for unsupported families
/// — which today are exactly the three flowchart markers (point /
/// circle / cross). Class / state / er markers do not get colored
/// variants.
#[must_use]
pub fn colored_marker(
    family: &str,
    position: &str,
    kind: &str,
    id_prefix: &str,
    stroke_color: &str,
) -> String {
    let color_id = marker_color_id(stroke_color);
    // Upstream: `${originalMarkerId}_${colorId}` with a single
    // separator underscore. The visual "double underscore" comes from
    // colorId itself starting with `_` whenever the color value begins
    // with a non-alphanumeric character (e.g. `#`, ` `).
    let raw_id = format!("{id_prefix}_{kind}-{family}{position}_{color_id}");
    // `stroke_color` is splatted unmodified into `stroke=` / `fill=`
    // attributes — preserving leading whitespace / the `#` prefix etc.,
    // exactly as upstream's `setAttribute('stroke', strokeColor)` does.
    match family {
        "point" => {
            // Clone of `markers::point` for End/Start (no `-margin`
            // suffix — upstream only colors the un-suffixed primary
            // marker), plus `stroke=` + `fill=` on the `<path>`.
            let (ref_x, d) = match position {
                "End" => ("5", "M 0 0 L 10 5 L 0 10 z"),
                "Start" => ("4.5", "M 0 5 L 10 10 L 10 0 z"),
                _ => return String::new(),
            };
            format!(
                "<marker id=\"{raw_id}\" class=\"marker {kind}\" viewBox=\"0 0 10 10\" refX=\"{ref_x}\" refY=\"5\" markerUnits=\"userSpaceOnUse\" markerWidth=\"8\" markerHeight=\"8\" orient=\"auto\"><path d=\"{d}\" class=\"arrowMarkerPath\" style=\"stroke-width: 1; stroke-dasharray: 1,0;\" stroke=\"{stroke_color}\" fill=\"{stroke_color}\"></path></marker>"
            )
        }
        "circle" => {
            let ref_x = match position {
                "End" => "11",
                "Start" => "-1",
                _ => return String::new(),
            };
            // Circle markers are NOT filled (`arrow_circle.fill = false`
            // in upstream `arrowTypesMap`), so only `stroke=` is set.
            format!(
                "<marker id=\"{raw_id}\" class=\"marker {kind}\" viewBox=\"0 0 10 10\" refX=\"{ref_x}\" refY=\"5\" markerUnits=\"userSpaceOnUse\" markerWidth=\"11\" markerHeight=\"11\" orient=\"auto\"><circle cx=\"5\" cy=\"5\" r=\"5\" class=\"arrowMarkerPath\" style=\"stroke-width: 1; stroke-dasharray: 1,0;\" stroke=\"{stroke_color}\"></circle></marker>"
            )
        }
        "cross" => {
            let ref_x = match position {
                "End" => "12",
                "Start" => "-1",
                _ => return String::new(),
            };
            // Cross markers are NOT filled (`arrow_cross.fill = false`).
            format!(
                "<marker id=\"{raw_id}\" class=\"marker cross {kind}\" viewBox=\"0 0 11 11\" refX=\"{ref_x}\" refY=\"5.2\" markerUnits=\"userSpaceOnUse\" markerWidth=\"11\" markerHeight=\"11\" orient=\"auto\"><path d=\"M 1,1 l 9,9 M 10,1 l -9,9\" class=\"arrowMarkerPath\" style=\"stroke-width: 2; stroke-dasharray: 1,0;\" stroke=\"{stroke_color}\"></path></marker>"
            )
        }
        _ => String::new(),
    }
}

/// Extract the stroke color from a flowchart-style `pathStyle` string,
/// matching upstream's regex `/stroke:([^;]+)/`. Returns the captured
/// substring **including** leading whitespace (because the regex does
/// not trim) — that whitespace is part of the byte-exact id mapping
/// upstream uses for ` orange` vs `orange`.
///
/// Returns `None` when the style does not contain a `stroke:` segment.
#[must_use]
pub fn extract_stroke_color(path_style: &str) -> Option<String> {
    // Find the first `stroke:` not preceded by an alphanumeric or dash
    // (so `stroke-width:` does NOT match). Upstream uses a regex that
    // anchors via the literal `stroke:` token — `stroke-width:` does
    // contain `stroke:` as a substring? No: `stroke-width:` contains
    // the byte sequence `stroke-` then `width:`. Substring `stroke:`
    // does not appear inside `stroke-width:`, so a plain substring
    // search suffices for upstream parity.
    let mut start = 0usize;
    while let Some(pos) = path_style[start..].find("stroke:") {
        let abs = start + pos;
        // Boundary check: `stroke:` must not be preceded by `-` (would
        // be `stroke-:` which never occurs upstream) or alpha/digit
        // (would be e.g. `mystroke:` — vanishingly rare but cheap to
        // guard).
        let prev = abs
            .checked_sub(1)
            .and_then(|i| path_style.as_bytes().get(i));
        if matches!(prev, Some(b) if b.is_ascii_alphanumeric() || *b == b'-') {
            start = abs + "stroke:".len();
            continue;
        }
        let after = abs + "stroke:".len();
        let end = path_style[after..]
            .find(';')
            .map(|i| after + i)
            .unwrap_or(path_style.len());
        return Some(path_style[after..end].to_string());
    }
    None
}

/// Emit the stylis-scoped `<style>` block: opening tag, CSS body, and
/// closing tag. Caller supplies a pre-composed / pre-minified CSS
/// string (typically [`crate::theme::css::base_preamble`] plus per-diagram
/// rules plus [`crate::theme::css::neo_look_block`]).
#[must_use]
pub fn emit_style_block(css: &str) -> String {
    let mut s = String::with_capacity(css.len() + 16);
    s.push_str("<style>");
    s.push_str(css);
    s.push_str("</style>");
    s
}

/// JS-Number-compatible printer — integers without `.0`, fractions
/// shortest round-trip form. Used for every numeric attribute emitted
/// by the shell helpers.
fn fmt_num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if v.fract() == 0.0 && v.is_finite() && v.abs() < 1e16 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_svg_matches_upstream_attribute_order() {
        let got = open_unified_svg(
            "my-id",
            154.7,
            (-67.6, -46.1, 154.7, 407.3),
            Some("erDiagram"),
            "er",
        );
        // Spot check every attribute token in order.
        let head = r#"<svg id="my-id" width="100%" xmlns="http://www.w3.org/2000/svg" class="erDiagram" style="max-width: 154.7px;" viewBox="-67.6 -46.1 154.7 407.3" role="graphics-document document" aria-roledescription="er">"#;
        assert_eq!(got, head);
    }

    #[test]
    fn open_svg_without_class_attribute() {
        let got = open_unified_svg("x", 100.0, (0.0, 0.0, 100.0, 50.0), None, "block");
        assert!(!got.contains(" class="));
        assert!(got.contains(r#"aria-roledescription="block""#));
    }

    #[test]
    fn drop_shadow_defs_contain_both_filters() {
        let s = emit_defs_shell("svg1", true, true);
        assert!(s.contains(r#"id="svg1-drop-shadow""#));
        assert!(s.contains(r#"id="svg1-drop-shadow-small""#));
        assert!(s.contains(r#"dx="4""#));
        assert!(s.contains(r#"dx="2""#));
    }

    #[test]
    fn marker_color_id_strips_non_alphanum() {
        assert_eq!(marker_color_id("#f66"), "_f66");
        assert_eq!(marker_color_id(" orange"), "_orange");
        assert_eq!(marker_color_id("orange"), "orange");
        assert_eq!(marker_color_id("#D50000"), "_D50000");
        assert_eq!(marker_color_id("greenyellow"), "greenyellow");
    }

    #[test]
    fn colored_marker_point_end_byte_exact() {
        // Matches the byte-exact reference snippet from cypress/197.
        let got = colored_marker(
            "point",
            "End",
            "flowchart-v2",
            "ref-ext-fixtures-cypress-flowchart-197",
            "#f66",
        );
        let expected = r##"<marker id="ref-ext-fixtures-cypress-flowchart-197_flowchart-v2-pointEnd__f66" class="marker flowchart-v2" viewBox="0 0 10 10" refX="5" refY="5" markerUnits="userSpaceOnUse" markerWidth="8" markerHeight="8" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1,0;" stroke="#f66" fill="#f66"></path></marker>"##;
        assert_eq!(got, expected);
    }

    #[test]
    fn colored_marker_with_leading_space_orange() {
        // cypress/143 stores the color as ` orange` (space-prefixed),
        // and upstream emits it back as ` orange` in the stroke/fill
        // attrs while the id strips the space.
        let got = colored_marker(
            "point",
            "End",
            "flowchart-v2",
            "ref-ext-fixtures-cypress-flowchart-143",
            " orange",
        );
        let expected = r##"<marker id="ref-ext-fixtures-cypress-flowchart-143_flowchart-v2-pointEnd__orange" class="marker flowchart-v2" viewBox="0 0 10 10" refX="5" refY="5" markerUnits="userSpaceOnUse" markerWidth="8" markerHeight="8" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1,0;" stroke=" orange" fill=" orange"></path></marker>"##;
        assert_eq!(got, expected);
    }

    #[test]
    fn extract_stroke_color_finds_first_match() {
        assert_eq!(
            extract_stroke_color("fill:#bbf;stroke:#f66;stroke-width:2px;color:white;"),
            Some("#f66".to_string())
        );
        // ` orange` keeps leading whitespace.
        assert_eq!(
            extract_stroke_color("color:orange, stroke: orange;"),
            Some(" orange".to_string())
        );
        // `stroke-width:` without `stroke:` returns None.
        assert_eq!(extract_stroke_color("fill:none;stroke-width:1px;"), None);
        // No stroke at all.
        assert_eq!(extract_stroke_color(";;"), None);
    }

    #[test]
    fn data_edge_attrs_round_trip() {
        let pts = vec![Point { x: 1.0, y: 2.5 }, Point { x: 3.0, y: 4.0 }];
        let got = data_edge_attrs(&pts);
        // Verify the base64 decodes back to the expected JSON.
        let attr_prefix = r#"data-edge="true" data-points=""#;
        assert!(got.starts_with(attr_prefix));
        let b64 = &got[attr_prefix.len()..got.len() - 1];
        let bytes = decode_b64(b64);
        assert_eq!(bytes, r#"[{"x":1,"y":2.5},{"x":3,"y":4}]"#);
    }

    fn decode_b64(s: &str) -> String {
        let tbl: [i32; 128] = {
            let mut t = [-1i32; 128];
            let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut i = 0;
            while i < alphabet.len() {
                t[alphabet[i] as usize] = i as i32;
                i += 1;
            }
            t
        };
        let mut out = Vec::new();
        let mut buf: u32 = 0;
        let mut bits: u32 = 0;
        for &b in s.as_bytes() {
            if b == b'=' {
                break;
            }
            let v = tbl[b as usize];
            if v < 0 {
                continue;
            }
            buf = (buf << 6) | (v as u32);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
                buf &= (1 << bits) - 1;
            }
        }
        String::from_utf8(out).unwrap()
    }
}
