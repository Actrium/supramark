//! `rect_left_inv_arrow` — rectangle with a left-pointing notch
//! ("asymmetric" / `>text]` syntax). Upstream: `rectLeftInvArrow.ts`.
//!
//! Five-vertex polygon (raw, before the shape's own translate):
//!   (x + notch, y), (x, 0), (x + notch, -y), (-x, -y), (-x, y)
//! where `x = -w/2`, `y = -h/2`, `notch = y/2`. Upstream applies a
//! final `translate(-notch/2, 0)` to the outer group.
//!
//! Upstream emits the geometry through `rc.path` — even when
//! `look !== 'handDrawn'` the non-handDrawn branch sets
//! `roughness: 0` + `fillStyle: 'solid'` but still uses `rc.path`,
//! so the SVG carries two `<path>` elements per node, plus a
//! `<g class="basic label-container outer-path" transform="translate(...)">`
//! wrapper that absorbs the `dx = -notch/2` offset.
//!
//! `updateNodeBounds(node, polygon)` reads the **visual** polygon
//! bbox (after the translate), so dagre receives
//!   node.width = w + h/4  (visual)
//!   node.height = h
//! For the visual polygon to come out at upstream coordinates, we
//! must recover the original `w` first by subtracting the `h/4`
//! width inflation.
//!
//! Output structure (mirrors upstream):
//! ```text
//! <g class="node default <css> " id=… data-look="classic" transform=…>
//!   <g class="basic label-container outer-path" transform="translate(-notch/2,0)">
//!     <path d=… stroke="none"  stroke-width="0" fill=…   style="…"></path>
//!     <path d=… stroke=…       stroke-width=…   fill="none" stroke-dasharray=… style="…"></path>
//!   </g>
//!   <g class="label" …>…</g>
//! </g>
//! ```

use super::types::{
    create_path_from_points, fmt_num, get_node_classes, xml_escape, xml_escape_label,
};
use crate::error::Result;
use crate::layout::unified::types::{Node, Point};
use crate::render::rough::{path_out_to_svg, to_paths, RoughGenerator, RoughOptions};
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, theme: &ThemeVariables) -> Result<String> {
    // Recover upstream's pre-translate `w` from the dagre-stored
    // visual width: node.width = w + h/4 (set by updateNodeBounds
    // reading the post-translate polygon bbox).
    let visual_w = node.width.unwrap_or(0.0);
    let h = node.height.unwrap_or(0.0);
    let w = visual_w - h / 4.0;
    let x = -w / 2.0;
    let y = -h / 2.0;
    let notch = y / 2.0; // upstream keeps sign; y is negative
    let dx = -notch / 2.0; // wrapper-g translate offset

    // Upstream pts (pre-translate). Note: the wrapper-g carries `dx`,
    // so the path-d itself uses the un-translated coordinates.
    let pts: [Point; 5] = [
        Point { x: x + notch, y },
        Point { x, y: 0.0 },
        Point {
            x: x + notch,
            y: -y,
        },
        Point { x: -x, y: -y },
        Point { x: -x, y },
    ];
    let path_d = create_path_from_points(&pts);

    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let label = node.label.clone().unwrap_or_default();

    let is_hand_drawn = matches!(node.look.as_deref(), Some("handDrawn"));
    let hand_drawn_seed: i32 = 1; // matches generate_ref.mjs handDrawnSeed: 1

    // Theme colour resolution (mirrors `userNodeOverrides`).
    let main_bkg = theme.main_bkg.clone().unwrap_or_else(|| "#ECECFF".into());
    let node_border = theme
        .node_border
        .clone()
        .unwrap_or_else(|| "#9370DB".into());

    // Compile node css styles → key/value map. Mirrors `compileStyles()`
    // in upstream `handDrawnShapeStyles.ts`.
    let css_styles = node.css_styles.as_deref().unwrap_or(&[]);
    let mut styles_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for s in css_styles {
        if let Some((k, v)) = s.split_once(':') {
            styles_map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    let fill = styles_map.get("fill").cloned().unwrap_or(main_bkg);
    let stroke = styles_map.get("stroke").cloned().unwrap_or(node_border);
    let stroke_width: f64 = styles_map
        .get("stroke-width")
        .map(|s| {
            s.trim_end_matches("px")
                .trim()
                .parse::<f64>()
                .unwrap_or(1.3)
        })
        .unwrap_or(1.3);

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

    // Build the per-path inline `style="…"` from non-label css_styles
    // (mirrors stadium / upstream `selectChildren('path').attr('style', cssStyles)`).
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
    // Inner wrapper-g carries the `dx = -notch/2` translate so the
    // path coordinates stay un-translated (upstream parity).
    out.push_str(&format!(
        r#"<g class="basic label-container outer-path" transform="translate({},0)">"#,
        fmt_num(dx),
    ));
    out.push_str(&paths_svg);
    out.push_str("</g>");
    if !label.is_empty() {
        let css_styles = node.css_styles.as_deref().unwrap_or(&[]);
        // Upstream label transform: translate(-notch/2 - bbox.width/2, -bbox.height/2).
        // shape_label_block_with_xy_offset_and_styles centers at -w/2,-h/2 and adds dx.
        out.push_str(
            &crate::render::foreign_object::shape_label_block_with_xy_offset_and_styles(
                &xml_escape_label(&label),
                &crate::render::foreign_object::HtmlLabelFont::default(),
                dx,
                0.0,
                css_styles,
            ),
        );
    }
    out.push_str("</g>");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_left_inv_arrow_emits_two_paths() {
        let mut n = Node::default();
        n.id = "arr".into();
        n.width = Some(80.0);
        n.height = Some(40.0);
        let theme = ThemeVariables::default();
        let got = draw(&n, &theme).unwrap();
        // Two <path> elements (fill + stroke) from rough.path.
        let path_count = got.matches("<path ").count();
        assert_eq!(path_count, 2, "expected fill + stroke paths, got {}", got);
        assert!(got.contains("basic label-container outer-path"));
    }
}
