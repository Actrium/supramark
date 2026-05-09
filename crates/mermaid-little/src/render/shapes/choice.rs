//! Choice diamond — upstream `choice.ts`.
//!
//! Mermaid does not emit a flat `<path d="M0,14 L14,0 ...">` here.
//! Upstream delegates to `rough.svg(...).path(choicePath, options)`, and
//! even in classic look (`roughness = 0`, `fillStyle = "solid"`) the
//! serialized SVG is a `<g><path fill/><path stroke/></g>` pair:
//! - fill = one cubic segment per edge
//! - stroke = two rough-style cubic strokes per edge
//!
//! The state layout fixes choice nodes to 28×28, but the geometry scales
//! linearly with `s`, so we keep it parameterized.

use super::types::{fmt_num, get_node_classes, xml_escape};
use crate::error::Result;
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

pub fn draw(node: &Node, _theme: &ThemeVariables) -> Result<String> {
    let s = node.width.unwrap_or(28.0).max(28.0);
    let half = s / 2.0;
    let fill = fill_path(half);
    let stroke = stroke_path(half);

    let classes = get_node_classes(node.look.as_deref(), node.css_classes.as_deref(), None);
    let id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
    let tx = node.x.unwrap_or(0.0);
    let ty = node.y.unwrap_or(0.0);
    let fill_color = node.background_color.as_deref().unwrap_or("#ECECFF");
    let stroke_color = node.border_color.as_deref().unwrap_or("#9370DB");
    let data_look = match node.look.as_deref() {
        Some(look) if !look.is_empty() => format!(r#" data-look="{}""#, look),
        _ => String::new(),
    };

    Ok(format!(
        concat!(
            r#"<g class="{classes}" id="{id}"{data_look} transform="translate({tx}, {ty})">"#,
            r#"<g>"#,
            r#"<path d="{fill}" stroke="none" stroke-width="0" fill="{fill_color}" style=""></path>"#,
            r#"<path d="{stroke}" stroke="{stroke_color}" stroke-width="1.3" fill="none" stroke-dasharray="0 0" style=""></path>"#,
            r#"</g>"#,
            r#"</g>"#
        ),
        classes = classes,
        id = xml_escape(&id),
        data_look = data_look,
        tx = fmt_num(tx),
        ty = fmt_num(ty),
        fill = fill,
        fill_color = fill_color,
        stroke = stroke,
        stroke_color = stroke_color,
    ))
}

// Upstream rough.js fill path on the four diamond edges, normalized to
// `half = 14`. These coefficients come from the authoritative Mermaid
// reference SVG and scale linearly with the fixed-size choice node.
const FILL_C_TOP: f64 = 0.21998448511585595;
const FILL_C_RIGHT: f64 = 0.3253558577038348;
const FILL_C_BOTTOM: f64 = 0.3354219778440893;
const FILL_C_LEFT: f64 = 0.24324055621400476;

// Rough.js stroke fractions for `seed: 1`, `roughness: 0`,
// `fillStyle: "solid"`. These match the existing rectangle/line rough
// helpers used elsewhere in the renderer.
const STROKE_C_TOP1: f64 = 0.20000449558719993;
const STROKE_C_TOP2: f64 = 0.22135189184919002;
const STROKE_C_RIGHT1: f64 = 0.21750230630859735;
const STROKE_C_RIGHT2: f64 = 0.26839575478807093;
const STROKE_C_BOTTOM1: f64 = 0.21567247649654747;
const STROKE_C_BOTTOM2: f64 = 0.37591258296743035;
const STROKE_C_LEFT1: f64 = 0.28680931767448786;
const STROKE_C_LEFT2: f64 = 0.3637662679888308;

fn fill_path(half: f64) -> String {
    let mut s = String::with_capacity(420);
    cubic_edge(&mut s, 0.0, half, half, 0.0, FILL_C_TOP, true);
    cubic_edge(&mut s, half, 0.0, 0.0, -half, FILL_C_RIGHT, false);
    cubic_edge(&mut s, 0.0, -half, -half, 0.0, FILL_C_BOTTOM, false);
    cubic_edge(&mut s, -half, 0.0, 0.0, half, FILL_C_LEFT, false);
    s
}

fn stroke_path(half: f64) -> String {
    let mut s = String::with_capacity(880);
    rough_edge(&mut s, 0.0, half, half, 0.0, STROKE_C_TOP1, true);
    rough_edge(&mut s, 0.0, half, half, 0.0, STROKE_C_TOP2, false);
    rough_edge(&mut s, half, 0.0, 0.0, -half, STROKE_C_RIGHT1, false);
    rough_edge(&mut s, half, 0.0, 0.0, -half, STROKE_C_RIGHT2, false);
    rough_edge(&mut s, 0.0, -half, -half, 0.0, STROKE_C_BOTTOM1, false);
    rough_edge(&mut s, 0.0, -half, -half, 0.0, STROKE_C_BOTTOM2, false);
    rough_edge(&mut s, -half, 0.0, 0.0, half, STROKE_C_LEFT1, false);
    rough_edge(&mut s, -half, 0.0, 0.0, half, STROKE_C_LEFT2, false);
    s
}

fn cubic_edge(out: &mut String, x1: f64, y1: f64, x2: f64, y2: f64, c: f64, first: bool) {
    if first {
        out.push('M');
        out.push_str(&fmt_num(x1));
        out.push(' ');
        out.push_str(&fmt_num(y1));
        out.push(' ');
    } else {
        out.push(' ');
    }
    let cp1x = (x2 - x1) * c + x1;
    let cp1y = (y2 - y1) * c + y1;
    let cp2x = (x2 - x1) * 2.0 * c + x1;
    let cp2y = (y2 - y1) * 2.0 * c + y1;
    out.push_str(&format!(
        "C{} {}, {} {}, {} {}",
        fmt_num(cp1x),
        fmt_num(cp1y),
        fmt_num(cp2x),
        fmt_num(cp2y),
        fmt_num(x2),
        fmt_num(y2),
    ));
}

fn rough_edge(out: &mut String, x1: f64, y1: f64, x2: f64, y2: f64, c: f64, first: bool) {
    if !first {
        out.push(' ');
    }
    let cp1x = (x2 - x1) * c + x1;
    let cp1y = (y2 - y1) * c + y1;
    let cp2x = (x2 - x1) * 2.0 * c + x1;
    let cp2y = (y2 - y1) * 2.0 * c + y1;
    out.push_str(&format!(
        "M{} {} C{} {}, {} {}, {} {}",
        fmt_num(x1),
        fmt_num(y1),
        fmt_num(cp1x),
        fmt_num(cp1y),
        fmt_num(cp2x),
        fmt_num(cp2y),
        fmt_num(x2),
        fmt_num(y2),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_byte_exact_default() {
        let mut n = Node::default();
        n.id = "c".into();
        n.width = Some(28.0);
        n.height = Some(28.0);
        let got = draw(&n, &ThemeVariables::default()).unwrap();
        assert!(got.contains(r##"<path d="M0 14 C3.0797827916219833 10.920217208378016, 6.159565583243967 7.840434416756033, 14 0 C9.445017992146312 -4.554982007853687, 4.890035984292625 -9.109964015707375, 0 -14 C-4.69590768981725 -9.30409231018275, -9.3918153796345 -4.608184620365501, -14 0 C-10.594632213003933 3.4053677869960666, -7.189264426007867 6.810735573992133, 0 14" stroke="none" stroke-width="0" fill="#ECECFF" style=""></path>"##));
        assert!(got.contains(r##"<path d="M0 14 C2.800062938220799 11.1999370617792, 5.600125876441598 8.399874123558401, 14 0 M0 14 C3.0989264858886605 10.901073514111339, 6.197852971777321 7.802147028222679, 14 0 M14 0 C10.954967711679636 -3.045032288320363, 7.909935423359274 -6.090064576640726, 0 -14 M14 0 C10.242459432967006 -3.757540567032993, 6.484918865934014 -7.515081134065986, 0 -14 M0 -14 C-3.0194146709516647 -10.980585329048335, -6.038829341903329 -7.961170658096671, -14 0 M0 -14 C-5.262776161544025 -8.737223838455975, -10.52555232308805 -3.47444767691195, -14 0 M-14 0 C-9.98466955255717 4.01533044744283, -5.96933910511434 8.03066089488566, 0 14 M-14 0 C-8.907272248156367 5.092727751843632, -3.8145444963127364 10.185455503687264, 0 14" stroke="#9370DB" stroke-width="1.3" fill="none" stroke-dasharray="0 0" style=""></path>"##));
    }
}
