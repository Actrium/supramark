//! Shared types + helpers for per-shape drawers.
//!
//! Upstream reference: `packages/mermaid/src/rendering-util/rendering-elements/shapes/util.ts`
//! plus the `Node` / `RectOptions` types from `rendering-util/types.ts`.
//!
//! This module is deliberately small. It centralises:
//!
//! 1. Numeric → string formatting that matches upstream's JS → SVG
//!    serialisation (integers render without `.0`; fractions render
//!    without trailing zeros). Used for `d=` path attributes and for
//!    `x`/`y`/`width`/`height` values.
//! 2. Common helpers that appear in `util.ts` —
//!    [`create_path_from_points`], [`get_node_classes`] — ported
//!    verbatim to Rust so every per-shape drawer reads the same as
//!    the upstream TypeScript.
//! 3. A [`Point`] alias re-exported from
//!    [`crate::layout::unified::types::Point`] for the geometry
//!    upstream expresses as `{ x, y }` literals.
//!
//! Byte-exactness note: upstream ultimately generates SVG attribute
//! values via d3's `.attr()` which stringifies `number`s with JS's
//! default `Number.prototype.toString`. We mirror that here in
//! [`fmt_num`] — integers print without a dot, non-integers print the
//! shortest decimal that round-trips (Rust's `{}` for `f64` already
//! uses Grisu3 / Ryū which is spec-compatible with JS `toString`).

use crate::layout::unified::types::Point;

/// Format a floating-point number the way JS stringifies it by
/// default — integer values lose their `.0`, fractional values use the
/// shortest round-trippable decimal.
///
/// Use this at every `format!()` site that inlines an `f64` into the
/// emitted SVG, so the output matches upstream byte-for-byte.
pub fn fmt_num(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_owned();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-Infinity".into() } else { "Infinity".into() };
    }
    // Integer values — no trailing `.0`.
    if v.fract() == 0.0 && v.abs() < 1e16 {
        return format!("{}", v as i64);
    }
    // JS uses ~15-17 significant digits by default; Rust's default
    // `{}` formatter uses the shortest round-trippable repr which is
    // what we want.
    format!("{}", v)
}

/// Build an SVG `d=` string from a point list via move-to + line-to +
/// close-path — direct port of upstream `createPathFromPoints`.
///
/// Output shape: `M{p0.x},{p0.y} L{p1.x},{p1.y} L{p2.x},{p2.y} Z`.
pub fn create_path_from_points(points: &[Point]) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(points.len() + 1);
    for (i, p) in points.iter().enumerate() {
        let cmd = if i == 0 { 'M' } else { 'L' };
        parts.push(format!("{}{},{}", cmd, fmt_num(p.x), fmt_num(p.y)));
    }
    parts.push("Z".into());
    parts.join(" ")
}

/// Port of upstream `roundedRectPath.createRoundedRectPathD`.
///
/// Emits a rounded-rectangle path with `radius` corners, anchored at
/// `(x, y)` with dimensions `w × h`.
pub fn create_rounded_rect_path_d(x: f64, y: f64, w: f64, h: f64, radius: f64) -> String {
    let r = radius;
    let parts = [
        format!("M {} {}", fmt_num(x + r), fmt_num(y)),
        format!("H {}", fmt_num(x + w - r)),
        format!("A {} {} 0 0 1 {} {}", fmt_num(r), fmt_num(r), fmt_num(x + w), fmt_num(y + r)),
        format!("V {}", fmt_num(y + h - r)),
        format!("A {} {} 0 0 1 {} {}", fmt_num(r), fmt_num(r), fmt_num(x + w - r), fmt_num(y + h)),
        format!("H {}", fmt_num(x + r)),
        format!("A {} {} 0 0 1 {} {}", fmt_num(r), fmt_num(r), fmt_num(x), fmt_num(y + h - r)),
        format!("V {}", fmt_num(y + r)),
        format!("A {} {} 0 0 1 {} {}", fmt_num(r), fmt_num(r), fmt_num(x + r), fmt_num(y)),
        "Z".into(),
    ];
    parts.join(" ")
}

/// Port of upstream `cylinder.createCylinderPathD`.
pub fn create_cylinder_path_d(x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) -> String {
    let parts = [
        format!("M{},{}", fmt_num(x), fmt_num(y + ry)),
        format!("a{},{} 0,0,0 {},0", fmt_num(rx), fmt_num(ry), fmt_num(w)),
        format!("a{},{} 0,0,0 {},0", fmt_num(rx), fmt_num(ry), fmt_num(-w)),
        format!("l0,{}", fmt_num(h)),
        format!("a{},{} 0,0,0 {},0", fmt_num(rx), fmt_num(ry), fmt_num(w)),
        format!("l0,{}", fmt_num(-h)),
    ];
    parts.join(" ")
}

/// Port of upstream `stadium.createStadiumPathD`. Stadium (a.k.a.
/// pill) — rounded rect whose corner radius equals half its height.
pub fn create_stadium_path_d(x: f64, y: f64, w: f64, h: f64) -> String {
    let r = h / 2.0;
    let parts = [
        format!("M {} {}", fmt_num(x + r), fmt_num(y)),
        format!("H {}", fmt_num(x + w - r)),
        format!("A {} {} 0 0 1 {} {}", fmt_num(r), fmt_num(r), fmt_num(x + w), fmt_num(y + r)),
        format!("H {}", fmt_num(x)),
        format!("A {} {} 0 0 1 {} {}", fmt_num(r), fmt_num(r), fmt_num(x + w - r), fmt_num(y + h)),
        format!("H {}", fmt_num(x + r)),
        format!("A {} {} 0 0 1 {} {}", fmt_num(r), fmt_num(r), fmt_num(x), fmt_num(y + r)),
        "Z".into(),
    ];
    parts.join(" ")
}

/// Port of upstream `hexagon.createHexagonPathD`.
pub fn create_hexagon_path_d(x: f64, y: f64, w: f64, h: f64, m: f64) -> String {
    let parts = [
        format!("M{},{}", fmt_num(x + m), fmt_num(y)),
        format!("L{},{}", fmt_num(x + w - m), fmt_num(y)),
        format!("L{},{}", fmt_num(x + w), fmt_num(y - h / 2.0)),
        format!("L{},{}", fmt_num(x + w - m), fmt_num(y - h)),
        format!("L{},{}", fmt_num(x + m), fmt_num(y - h)),
        format!("L{},{}", fmt_num(x), fmt_num(y - h / 2.0)),
        "Z".into(),
    ];
    parts.join(" ")
}

/// Port of upstream `subroutine.createSubroutinePathD`.
pub fn create_subroutine_path_d(x: f64, y: f64, w: f64, h: f64) -> String {
    let offset = 8.0;
    let parts = [
        format!("M{},{}", fmt_num(x - offset), fmt_num(y)),
        format!("H{}", fmt_num(x + w + offset)),
        format!("V{}", fmt_num(y + h)),
        format!("H{}", fmt_num(x - offset)),
        format!("V{}", fmt_num(y)),
        "M".into(),
        fmt_num(x),
        fmt_num(y),
        "H".into(),
        fmt_num(x + w),
        "V".into(),
        fmt_num(y + h),
        "H".into(),
        fmt_num(x),
        "Z".into(),
    ];
    parts.join(" ")
}

/// Port of upstream `question.createDecisionBoxPathD`.
pub fn create_decision_box_path_d(x: f64, y: f64, size: f64) -> String {
    let parts = [
        format!("M{},{}", fmt_num(x + size / 2.0), fmt_num(y)),
        format!("L{},{}", fmt_num(x + size), fmt_num(y - size / 2.0)),
        format!("L{},{}", fmt_num(x + size / 2.0), fmt_num(y - size)),
        format!("L{},{}", fmt_num(x), fmt_num(y - size / 2.0)),
        "Z".into(),
    ];
    parts.join(" ")
}

/// Render the upstream `getNodeClasses(node, extra?)` helper.
///
/// Upstream produces `"${look === 'handDrawn' ? 'rough-node' : 'node'} ${cssClasses} ${extra ?? ''}"`
/// — note the trailing space when `extra` is empty, which we preserve
/// for byte exactness.
pub fn get_node_classes(look: Option<&str>, css_classes: Option<&str>, extra: Option<&str>) -> String {
    let base = if matches!(look, Some("handDrawn")) {
        "rough-node"
    } else {
        "node"
    };
    // Upstream: `${base} ${cssClasses} ${extra ?? ''}` — `cssClasses`
    // is `undefined` → literal "undefined" string in JS.
    let css = css_classes.unwrap_or("undefined");
    let ex = extra.unwrap_or("");
    format!("{} {} {}", base, css, ex)
}

/// Minimal label-sizing helper used by shapes that need a label bbox
/// but are running outside of a DOM. Mirrors `labelHelper` at a
/// much-reduced level of fidelity — just width×height of the
/// rendered text using [`crate::font_metrics::text_width`] + a fixed
/// line-height heuristic. Used only for shape size maths, not for
/// the emitted `<text>` element (which is composed by each shape).
///
/// Line height: upstream mermaid uses 1.1em for flowchart labels; we
/// hard-code `font_size * 1.1` unless a label is empty.
pub fn measure_label(label: &str, font_family: &str, font_size: f64, bold: bool) -> (f64, f64) {
    if label.is_empty() {
        return (0.0, 0.0);
    }
    // Handle multi-line labels (<br/> separator is upstream's spec).
    let lines: Vec<&str> = label.split("<br/>").collect();
    let mut max_w = 0.0f64;
    for line in &lines {
        let w = crate::font_metrics::text_width(line, font_family, font_size, bold, false);
        if w > max_w {
            max_w = w;
        }
    }
    let h = font_size * 1.1 * lines.len() as f64;
    (max_w, h)
}

/// Emit a standard polygon-shaped node block. Shared by every shape
/// whose geometry is a closed polygon (hexagon, lean_left/right,
/// trapezoid variants, diamond, choice, …) so that attribute order
/// and whitespace are identical across them.
///
/// `pts` are already in centred-around-origin coordinates — no
/// further translation is applied inside the inner `<polygon>`.
pub fn emit_polygon_node(
    node: &crate::layout::unified::types::Node,
    pts: &[(f64, f64)],
) -> String {
    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let label = node.label.clone().unwrap_or_default();
    let pts_attr: Vec<String> = pts
        .iter()
        .map(|(x, y)| format!("{},{}", fmt_num(*x), fmt_num(*y)))
        .collect();

    let mut out = String::new();
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}" transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = xml_escape(&id),
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    out.push_str(&format!(
        r#"<polygon class="basic label-container" style="" points="{p}"/>"#,
        p = pts_attr.join(" "),
    ));
    if !label.is_empty() {
        out.push_str(&crate::render::foreign_object::shape_label_block(
            &xml_escape(&label),
            &crate::render::foreign_object::HtmlLabelFont::default(),
        ));
    }
    out.push_str("</g>");
    out
}

/// Escape a text fragment for safe insertion into an SVG attribute or
/// between tags. This is the escape set upstream mermaid uses in
/// `sanitizeText`/`decodeEntities` → `&amp; &lt; &gt; &quot;`.
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_num_strips_trailing_zero() {
        assert_eq!(fmt_num(5.0), "5");
        assert_eq!(fmt_num(-3.0), "-3");
        assert_eq!(fmt_num(2.5), "2.5");
        assert_eq!(fmt_num(0.0), "0");
    }

    #[test]
    fn path_from_points_port_matches_upstream() {
        let pts = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 5.0 },
        ];
        assert_eq!(create_path_from_points(&pts), "M0,0 L10,0 L10,5 Z");
    }

    #[test]
    fn rounded_rect_path_is_byte_identical_to_upstream() {
        // Upstream sample: x=-50, y=-25, w=100, h=50, r=5 → deterministic.
        let d = create_rounded_rect_path_d(-50.0, -25.0, 100.0, 50.0, 5.0);
        assert_eq!(
            d,
            "M -45 -25 H 45 A 5 5 0 0 1 50 -20 V 20 A 5 5 0 0 1 45 25 H -45 A 5 5 0 0 1 -50 20 V -20 A 5 5 0 0 1 -45 -25 Z"
        );
    }

    #[test]
    fn hexagon_path_byte_exact() {
        let d = create_hexagon_path_d(0.0, 0.0, 100.0, 50.0, 12.0);
        assert_eq!(
            d,
            "M12,0 L88,0 L100,-25 L88,-50 L12,-50 L0,-25 Z"
        );
    }

    #[test]
    fn stadium_path_byte_exact() {
        let d = create_stadium_path_d(-40.0, -10.0, 80.0, 20.0);
        assert_eq!(
            d,
            "M -30 -10 H 30 A 10 10 0 0 1 40 0 H -40 A 10 10 0 0 1 30 10 H -30 A 10 10 0 0 1 -40 0 Z"
        );
    }

    #[test]
    fn decision_box_path_byte_exact() {
        let d = create_decision_box_path_d(0.0, 0.0, 60.0);
        assert_eq!(d, "M30,0 L60,-30 L30,-60 L0,-30 Z");
    }

    #[test]
    fn xml_escape_basic() {
        assert_eq!(xml_escape("a & b < c > d \"e\""), "a &amp; b &lt; c &gt; d &quot;e&quot;");
    }

    #[test]
    fn get_node_classes_default_look() {
        assert_eq!(get_node_classes(None, Some("flowchart"), None), "node flowchart ");
        assert_eq!(get_node_classes(Some("handDrawn"), Some("flowchart"), None), "rough-node flowchart ");
        assert_eq!(get_node_classes(None, None, Some("extra")), "node undefined extra");
    }
}
