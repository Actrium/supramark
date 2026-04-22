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
    let (vx, vy, vw, vh) = view_box;
    let class_frag = match class_attr {
        Some(c) => format!(r#" class="{c}""#),
        None => String::new(),
    };
    format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg"{cls} style="max-width: {mw}px;" viewBox="{vx} {vy} {vw} {vh}" role="graphics-document document" aria-roledescription="{aria}">"#,
        id = id,
        cls = class_frag,
        mw = fmt_num(max_width),
        vx = fmt_num(vx),
        vy = fmt_num(vy),
        vw = fmt_num(vw),
        vh = fmt_num(vh),
        aria = aria,
    )
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
