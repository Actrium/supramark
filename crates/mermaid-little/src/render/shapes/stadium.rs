//! Stadium / pill shape — upstream `stadium.ts`.
//!
//! A rounded rectangle whose corner radius equals half its height;
//! degenerates to a circle when `w == h`. Upstream generates it as a
//! 102-point polygon (line + 50-point arc + line + 50-point arc) and
//! draws via `rough.path` — even when `look !== 'handDrawn'` (the
//! non-handDrawn branch sets `roughness: 0` + `fillStyle: 'solid'` but
//! still uses `rc.path`, so the emitted SVG carries two `<path>`
//! elements per node, not one analytic path).
//!
//! Theme colour wiring matches `userNodeOverrides`:
//!   `fill   = stylesMap.get('fill')   || themeVariables.mainBkg`
//!   `stroke = stylesMap.get('stroke') || themeVariables.nodeBorder`
//!
//! Output structure:
//! ```text
//! <g class="node default <css> " id=… data-look="classic" transform=…>
//!   <g class="basic label-container outer-path">
//!     <path d=… stroke="none"  stroke-width="0" fill=…   style="…"></path>
//!     <path d=… stroke=…       stroke-width=…   fill="none" stroke-dasharray=… style="…"></path>
//!   </g>
//!   <g class="label" …>…</g>
//! </g>
//! ```

use super::types::{fmt_num, get_node_classes, xml_escape, xml_escape_label};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::math::v8_trig;
use crate::render::rough::{path_out_to_svg, to_paths, RoughGenerator, RoughOptions};
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, theme: &ThemeVariables) -> Result<String> {
    // `node.width` is the polygon-bbox-corrected dagre width.  Recover the
    // analytical (`upstream stadium.ts` formula) width from which the
    // 102-point polygon was sampled — both the path geometry and the
    // intersection logic (see `flowchart::fix_polygon_edge_endpoints`,
    // `edges::intersect_node_boundary`) operate on this analytical width.
    let h = node.height.unwrap_or(0.0);
    let radius = h / 2.0;
    let n_arc_points = 50_usize;
    let half_step = std::f64::consts::PI / (2.0 * (n_arc_points as f64 - 1.0));
    let polygon_correction = 2.0 * radius * (1.0 - half_step.cos());
    let w = node.width.unwrap_or(0.0) + polygon_correction;

    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let label = node.label.clone().unwrap_or_default();

    let is_hand_drawn = matches!(node.look.as_deref(), Some("handDrawn"));
    let hand_drawn_seed: i32 = 1; // matches generate_ref.mjs handDrawnSeed: 1

    // Theme colour resolution (mirrors userNodeOverrides).
    let main_bkg = theme.main_bkg.clone().unwrap_or_else(|| "#ECECFF".into());
    let node_border = theme
        .node_border
        .clone()
        .unwrap_or_else(|| "#9370DB".into());

    // Compile node css styles → key/value map. Mirrors compileStyles() in
    // upstream handDrawnShapeStyles.ts: cssCompiledStyles + cssStyles +
    // labelStyle, dedup on key (last wins).
    let css_styles = node.css_styles.as_deref().unwrap_or(&[]);
    let mut styles_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for s in css_styles {
        if let Some((k, v)) = s.split_once(':') {
            styles_map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    let fill = styles_map
        .get("fill")
        .cloned()
        .unwrap_or(main_bkg);
    let stroke = styles_map
        .get("stroke")
        .cloned()
        .unwrap_or(node_border);
    let stroke_width: f64 = styles_map
        .get("stroke-width")
        .map(|s| {
            s.trim_end_matches("px")
                .trim()
                .parse::<f64>()
                .unwrap_or(1.3)
        })
        .unwrap_or(1.3);

    // Build the 102-point polygon.
    let path_d = build_stadium_path_d(w, h, radius);

    // Run rough.path to produce two SVG <path> elements.
    let mut o = RoughOptions::default();
    o.seed = hand_drawn_seed;
    if is_hand_drawn {
        o.roughness = 0.7;
        o.fill_style = "hachure".into();
        o.fill_weight = 4.0;
        o.hachure_gap = 5.2;
    } else {
        o.roughness = 0.0;
        o.fill_style = "solid".into();
    }
    o.fill = Some(fill.clone());
    o.stroke = stroke.clone();
    o.stroke_width = stroke_width;
    o.fill_line_dash = vec![0.0, 0.0];
    o.stroke_line_dash = vec![0.0, 0.0];

    let mut rc = RoughGenerator::new();
    let drawable = rc.path(&path_d, &o);
    let paths = to_paths(&drawable, &o);

    // Compose SVG. The non-handDrawn branch wraps with style="" (empty
    // when the node has no inline styles, matching d3's
    // `attr('style', '')` behaviour).
    //
    // `nodeStyles` from upstream `styles2String`: every non-label-style key
    // emitted as `key:value !important`, joined with `;`.
    let label_style_keys: &[&str] = &[
        "color",
        "font-size",
        "font-family",
        "font-weight",
        "font-style",
        "text-decoration",
        "text-align",
        "text-transform",
        "line-height",
        "letter-spacing",
        "word-spacing",
        "text-shadow",
        "text-overflow",
        "white-space",
        "word-wrap",
        "word-break",
        "overflow-wrap",
        "hyphens",
    ];
    let mut node_style_parts: Vec<String> = Vec::new();
    // Iterate over the original css_styles vec to preserve declaration order.
    for s in css_styles {
        if let Some((k, v)) = s.split_once(':') {
            let k = k.trim();
            let v = v.trim();
            if !label_style_keys.contains(&k) {
                node_style_parts.push(format!("{}:{} !important", k, v));
            }
        }
    }
    let path_style = node_style_parts.join(";");
    let path_style = path_style.as_str();

    let mut paths_svg = String::new();
    for p in &paths {
        let raw = path_out_to_svg(p);
        // Inject `style="…"` before `></path>` to match upstream d3
        // `selectChildren('path').attr('style', cssStyles)` ordering.
        let injected = if let Some(idx) = raw.rfind("></path>") {
            let mut s = raw[..idx].to_string();
            s.push_str(&format!(r#" style="{}""#, path_style));
            s.push_str(&raw[idx..]);
            s
        } else {
            raw
        };
        paths_svg.push_str(&injected);
    }

    let mut out = String::new();
    let data_look_attr = if is_hand_drawn {
        ""
    } else {
        " data-look=\"classic\""
    };
    out.push_str(&format!(
        r#"<g class="{classes}" id="{id}"{dla} transform="translate({tx}, {ty})">"#,
        classes = classes,
        id = xml_escape(&id),
        dla = data_look_attr,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
    ));
    out.push_str(r#"<g class="basic label-container outer-path">"#);
    out.push_str(&paths_svg);
    out.push_str("</g>");
    if !label.is_empty() {
        let css_styles = node.css_styles.as_deref().unwrap_or(&[]);
        out.push_str(
            &crate::render::foreign_object::shape_label_block_with_styles(
                &xml_escape_label(&label),
                &crate::render::foreign_object::HtmlLabelFont::default(),
                css_styles,
            ),
        );
    }
    out.push_str("</g>");
    Ok(out)
}

/// Build the upstream stadium polygon path. Reproduces:
///
/// ```js
/// const points = [
///   { x: -w/2 + radius, y: -h/2 },
///   { x:  w/2 - radius, y: -h/2 },
///   ...generateCirclePoints(-w/2 + radius, 0, radius, 50,  90, 270),
///   { x:  w/2 - radius, y:  h/2 },
///   ...generateCirclePoints( w/2 - radius, 0, radius, 50, 270, 450),
/// ];
/// const pathData = createPathFromPoints(points);
/// ```
fn build_stadium_path_d(w: f64, h: f64, radius: f64) -> String {
    let mut pts: Vec<(f64, f64)> = Vec::with_capacity(102);
    pts.push((-w / 2.0 + radius, -h / 2.0));
    pts.push((w / 2.0 - radius, -h / 2.0));
    let n = 50_usize;
    let arc1_cx = -w / 2.0 + radius;
    let start1 = std::f64::consts::PI / 2.0;
    let end1 = std::f64::consts::PI * 3.0 / 2.0;
    let step1 = (end1 - start1) / (n as f64 - 1.0);
    for i in 0..n {
        let angle = start1 + i as f64 * step1;
        let xr = arc1_cx + radius * v8_trig::cos(angle);
        let yr = radius * v8_trig::sin(angle);
        pts.push((-xr, -yr));
    }
    pts.push((w / 2.0 - radius, h / 2.0));
    let arc2_cx = w / 2.0 - radius;
    let start2 = std::f64::consts::PI * 3.0 / 2.0;
    let end2 = std::f64::consts::PI * 5.0 / 2.0;
    let step2 = (end2 - start2) / (n as f64 - 1.0);
    for i in 0..n {
        let angle = start2 + i as f64 * step2;
        let xr = arc2_cx + radius * v8_trig::cos(angle);
        let yr = radius * v8_trig::sin(angle);
        pts.push((-xr, -yr));
    }
    let mut parts: Vec<String> = Vec::with_capacity(pts.len() + 1);
    for (i, p) in pts.iter().enumerate() {
        let cmd = if i == 0 { 'M' } else { 'L' };
        parts.push(format!("{}{},{}", cmd, fmt_num(p.0), fmt_num(p.1)));
    }
    parts.push("Z".into());
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stadium_emits_two_paths() {
        let mut n = Node::default();
        n.id = "s".into();
        n.width = Some(80.0);
        n.height = Some(20.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        // Two <path> elements (fill + stroke).
        let path_count = got.matches("<path ").count();
        assert_eq!(path_count, 2, "expected fill + stroke paths, got {}", got);
    }
}
