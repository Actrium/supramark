use crate::font_metrics;
use crate::model::creole_diagram::{CreoleDiagram, CreoleElement};
use crate::Result;

/// Heading level 1 font size (Java Serif 18pt bold).
const HEADING1_FONT_SIZE: f64 = 18.0;
/// Normal text font size (Java Serif 14pt).
const TEXT_FONT_SIZE: f64 = 14.0;
/// Bullet circle radius (Java `Bullet.drawU` draws a 5x5 ellipse).
const BULLET_RADIUS: f64 = 2.5;
/// Java `Bullet.calculateDimensionSlow` returns `(12, 5)` for a circle bullet
/// (order=0). We share the constants below so layout math stays explicit.
const BULLET_DIM_W: f64 = 12.0;
const BULLET_DIM_H: f64 = 5.0;
/// Java `Bullet.getStartingAltitude` returns -5 for a circle bullet.
const BULLET_STARTING_ALTITUDE: f64 = -5.0;
/// Java `Bullet.drawU` translates the ellipse by `dx=3` inside the 12-wide box.
const BULLET_DX: f64 = 3.0;
/// Java `LimitFinder.drawText` shifts the y reported for a `UText`:
/// `y -= dim.height - 1.5; addPoint(x, y + dim.height)` ⇒ contribution =
/// `baseline + 1.5`. This constant captures that 1.5pt below the baseline.
const TEXT_LIMIT_FOOTPRINT: f64 = 1.5;

/// A positioned element in the creole layout.
#[derive(Debug, Clone)]
pub enum CreoleLayoutElement {
    Heading {
        text: String,
        x: f64,
        y: f64,
        text_width: f64,
        font_size: f64,
    },
    Bullet {
        cx: f64,
        cy: f64,
        text: String,
        text_x: f64,
        text_y: f64,
        text_width: f64,
    },
    Text {
        text: String,
        x: f64,
        y: f64,
        text_width: f64,
    },
}

/// Full creole layout.
#[derive(Debug)]
pub struct CreoleLayout {
    pub width: f64,
    pub height: f64,
    pub elements: Vec<CreoleLayoutElement>,
}

pub fn layout_creole(d: &CreoleDiagram) -> Result<CreoleLayout> {
    let mut elements = Vec::new();
    // `y_top` tracks the top of the current line in Java `SheetBlock1.initMap`
    // accumulator semantics (`y += sea.getHeight()` per stripe).
    let mut y_top: f64 = 0.0;
    // `max_x` and `max_y` track the LimitFinder bounding box (matches Java
    // `LimitFinder` semantics: text contributes `baseline + 1.5` below, ellipse
    // contributes `top + dim.h - 1`, and final dim is `maxY + 1`).
    let mut max_x: f64 = 0.0;
    let mut max_y: f64 = 0.0;

    for elem in &d.elements {
        match elem {
            CreoleElement::Heading { text, level } => {
                let fs = match level {
                    1 => HEADING1_FONT_SIZE,
                    2 => 16.0,
                    3 => 14.0,
                    _ => 14.0,
                };
                let ascent = font_metrics::ascent("Serif", fs, true, false);
                let line_h = font_metrics::line_height("Serif", fs, true, false);
                let tw = font_metrics::text_width(text, "Serif", fs, true, false);

                let baseline = y_top + ascent;
                elements.push(CreoleLayoutElement::Heading {
                    text: text.clone(),
                    x: 0.0,
                    y: baseline,
                    text_width: tw,
                    font_size: fs,
                });
                max_x = max_x.max(tw);
                max_y = max_y.max(baseline + TEXT_LIMIT_FOOTPRINT);
                y_top += line_h;
            }
            CreoleElement::Bullet { text, level: _ } => {
                let ascent = font_metrics::ascent("Serif", TEXT_FONT_SIZE, false, false);
                let line_h = font_metrics::line_height("Serif", TEXT_FONT_SIZE, false, false);
                let tw = font_metrics::text_width(text, "Serif", TEXT_FONT_SIZE, false, false);

                // Java `Sea.doAlign + translateMinYto(y_top)` math for a
                // (Bullet, AtomText) stripe. Both atoms share the same x=0
                // origin first, then the bullet's `dx=3` translate and
                // `getStartingAltitude=-5` shift the ellipse box vertically:
                //   bullet_atom_y = y_top + line_h - (BULLET_DIM_H - BULLET_STARTING_ALTITUDE)
                let bullet_atom_y = y_top + line_h - (BULLET_DIM_H - BULLET_STARTING_ALTITUDE);
                let cx = BULLET_DX + BULLET_RADIUS;
                let cy = bullet_atom_y + BULLET_RADIUS;
                let text_x = BULLET_DIM_W;
                let baseline = y_top + ascent;
                elements.push(CreoleLayoutElement::Bullet {
                    cx,
                    cy,
                    text: text.clone(),
                    text_x,
                    text_y: baseline,
                    text_width: tw,
                });
                max_x = max_x.max(text_x + tw);
                // Ellipse contribution: `addPoint(x + dim.w - 1, y + dim.h - 1)`.
                let ellipse_max_y = bullet_atom_y + BULLET_DIM_H - 1.0;
                max_y = max_y
                    .max(baseline + TEXT_LIMIT_FOOTPRINT)
                    .max(ellipse_max_y);
                y_top += line_h;
            }
            CreoleElement::Text(text) => {
                let ascent = font_metrics::ascent("Serif", TEXT_FONT_SIZE, false, false);
                let line_h = font_metrics::line_height("Serif", TEXT_FONT_SIZE, false, false);
                let tw = font_metrics::text_width(text, "Serif", TEXT_FONT_SIZE, false, false);

                let baseline = y_top + ascent;
                elements.push(CreoleLayoutElement::Text {
                    text: text.clone(),
                    x: 0.0,
                    y: baseline,
                    text_width: tw,
                });
                max_x = max_x.max(tw);
                max_y = max_y.max(baseline + TEXT_LIMIT_FOOTPRINT);
                y_top += line_h;
            }
        }
    }

    // Java `ImageBuilder.getFinalDimension` adds +1 to LimitFinder's max:
    //   dim = (maxX + 1, maxY + 1).
    // Caller then runs ensure_visible_int which adds another +1 to match the
    // `(int)(x + 1)` rounding `SvgGraphics.ensureVisible` performs.
    let width = max_x + 1.0;
    let height = max_y + 1.0;

    Ok(CreoleLayout {
        width,
        height,
        elements,
    })
}
