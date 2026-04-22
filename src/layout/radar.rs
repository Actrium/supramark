//! Radar layout.
//!
//! Upstream reference: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/radar/renderer.ts
//!
//! Layout computes all of the numeric coordinates required by the SVG
//! renderer but emits nothing itself. The actual textual serialisation
//! (which must mirror JavaScript's `Number.prototype.toString()` rules)
//! lives entirely in `render/svg_radar.rs`.
//!
//! The calculations here intentionally mirror upstream's code path exactly
//! — including the intermediate multiplications that cause the
//! characteristic `1.8369701987210297e-14` style outputs you see in the
//! reference SVGs. Any refactor towards "cleaner" math (e.g. collapsing
//! `radius * cos(angle)` into `sin(angle) * radius`) would break byte
//! parity and must be avoided.

use crate::error::Result;
use crate::model::radar::{Graticule, RadarDiagram};
use crate::theme::ThemeVariables;

/// Hard-coded configuration constants from upstream
/// `config.schema.yaml :: RadarDiagramConfig`. These defaults have never
/// been overridden by any fixture so we bake them in directly instead of
/// threading a full config through the pipeline.
pub const RADAR_WIDTH: f64 = 600.0;
pub const RADAR_HEIGHT: f64 = 600.0;
pub const RADAR_MARGIN_TOP: f64 = 50.0;
pub const RADAR_MARGIN_RIGHT: f64 = 50.0;
pub const RADAR_MARGIN_BOTTOM: f64 = 50.0;
pub const RADAR_MARGIN_LEFT: f64 = 50.0;
pub const RADAR_AXIS_SCALE_FACTOR: f64 = 1.0;
pub const RADAR_AXIS_LABEL_FACTOR: f64 = 1.05;
pub const RADAR_CURVE_TENSION: f64 = 0.17;

/// Final positions for every drawable element. All coordinates are
/// relative to the centred `<g transform="translate(cx, cy)">` group,
/// matching upstream's coordinate system.
#[derive(Debug, Clone, Default)]
pub struct RadarLayout {
    /// Full SVG viewport width.
    pub width: f64,
    /// Full SVG viewport height.
    pub height: f64,
    /// Centre of the radar chart, in SVG coordinates.
    pub cx: f64,
    pub cy: f64,
    /// Unit radius of the outermost graticule.
    pub radius: f64,

    /// Graticule rings. For `Circle` graticule each entry is a radius;
    /// for `Polygon` each entry still holds the radius so the renderer
    /// can derive vertex points from `axes_angles`.
    pub graticule_radii: Vec<f64>,

    /// Axis angles, measured in radians with zero pointing "up" (y = -1).
    pub axes_angles: Vec<f64>,

    /// Pre-computed endpoint of each axis line `(x2, y2)`.
    pub axes_endpoints: Vec<(f64, f64)>,
    /// Pre-computed position of each axis label `(x, y)`.
    pub axes_label_positions: Vec<(f64, f64)>,

    /// For each curve that is drawable (matches axes length), the
    /// (x, y) points at each axis index.
    pub curves: Vec<CurveLayout>,

    /// Legend anchor `(x, y)` — position of the first legend entry's
    /// group transform; subsequent entries are 20px below.
    pub legend_origin: (f64, f64),

    /// Title anchor y-coordinate (x is always 0 in the centred group).
    pub title_y: f64,
}

#[derive(Debug, Clone, Default)]
pub struct CurveLayout {
    /// Original index in the source `curves` vec (needed for class suffix).
    pub source_index: usize,
    /// Vertex positions at each axis.
    pub points: Vec<(f64, f64)>,
}

/// Compute the layout for a radar diagram.
///
/// The theme is accepted for API symmetry with other diagrams but the
/// current layout is theme-independent — all positions are driven by
/// the baked config constants.
pub fn layout(d: &RadarDiagram, _theme: &ThemeVariables) -> Result<RadarLayout> {
    let total_width = RADAR_WIDTH + RADAR_MARGIN_LEFT + RADAR_MARGIN_RIGHT;
    let total_height = RADAR_HEIGHT + RADAR_MARGIN_TOP + RADAR_MARGIN_BOTTOM;
    let cx = RADAR_MARGIN_LEFT + RADAR_WIDTH / 2.0;
    let cy = RADAR_MARGIN_TOP + RADAR_HEIGHT / 2.0;
    let radius = f64::min(RADAR_WIDTH, RADAR_HEIGHT) / 2.0;

    let num_axes = d.axes.len();

    // Graticule radii — `(radius * (i + 1)) / ticks`, exactly as upstream.
    let ticks = d.options.ticks.max(1);
    let mut graticule_radii = Vec::with_capacity(ticks as usize);
    for i in 0..ticks {
        graticule_radii.push(radius * (i as f64 + 1.0) / ticks as f64);
    }

    // Angles: `2 * i * PI / N - PI/2`. Careful to match upstream's
    // multiplication order so that floating rounding matches.
    let mut axes_angles = Vec::with_capacity(num_axes);
    for i in 0..num_axes {
        let angle = 2.0 * i as f64 * std::f64::consts::PI / num_axes as f64
            - std::f64::consts::PI / 2.0;
        axes_angles.push(angle);
    }

    // Axis line endpoints: `radius * scale * cos/sin(angle)`.
    let axes_endpoints: Vec<(f64, f64)> = axes_angles
        .iter()
        .map(|a| {
            let x = radius * RADAR_AXIS_SCALE_FACTOR * a.cos();
            let y = radius * RADAR_AXIS_SCALE_FACTOR * a.sin();
            (x, y)
        })
        .collect();

    // Axis label positions: `radius * labelFactor * cos/sin(angle)`.
    let axes_label_positions: Vec<(f64, f64)> = axes_angles
        .iter()
        .map(|a| {
            let x = radius * RADAR_AXIS_LABEL_FACTOR * a.cos();
            let y = radius * RADAR_AXIS_LABEL_FACTOR * a.sin();
            (x, y)
        })
        .collect();

    // Curve value range: max = options.max ?? max-of-values.
    let max_value: f64 = d.options.max.unwrap_or_else(|| {
        d.curves
            .iter()
            .flat_map(|c| c.values.iter().copied())
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let min_value: f64 = d.options.min;

    // Curves: only those whose value-count matches axis-count are drawn.
    let mut curves = Vec::new();
    for (idx, curve) in d.curves.iter().enumerate() {
        if curve.values.len() != num_axes {
            continue;
        }
        let mut points = Vec::with_capacity(num_axes);
        for (i, value) in curve.values.iter().enumerate() {
            let angle = 2.0 * std::f64::consts::PI * i as f64 / num_axes as f64
                - std::f64::consts::PI / 2.0;
            let r = relative_radius(*value, min_value, max_value, radius);
            let x = r * angle.cos();
            let y = r * angle.sin();
            points.push((x, y));
        }
        curves.push(CurveLayout {
            source_index: idx,
            points,
        });
    }

    // Legend origin (upstream: `(width/2 + marginRight) * 3 / 4`,
    // `-(height/2 + marginTop) * 3 / 4`).
    let legend_x = (RADAR_WIDTH / 2.0 + RADAR_MARGIN_RIGHT) * 3.0 / 4.0;
    let legend_y = -(RADAR_HEIGHT / 2.0 + RADAR_MARGIN_TOP) * 3.0 / 4.0;

    // Title y coordinate.
    let title_y = -RADAR_HEIGHT / 2.0 - RADAR_MARGIN_TOP;

    Ok(RadarLayout {
        width: total_width,
        height: total_height,
        cx,
        cy,
        radius,
        graticule_radii,
        axes_angles,
        axes_endpoints,
        axes_label_positions,
        curves,
        legend_origin: (legend_x, legend_y),
        title_y,
    })
}

/// Clip `value` to `[min, max]` then scale to `[0, radius]`. Mirrors
/// upstream's `relativeRadius` exactly.
pub fn relative_radius(value: f64, min_value: f64, max_value: f64, radius: f64) -> f64 {
    let clipped = value.max(min_value).min(max_value);
    radius * (clipped - min_value) / (max_value - min_value)
}

/// Convenience check: does this layout use polygon graticule?
/// Used by the renderer to decide between `<circle>`/`<polygon>` and
/// `<polygon>`/`<path>`.
pub fn uses_polygon(d: &RadarDiagram) -> bool {
    matches!(d.options.graticule, Graticule::Polygon)
}
