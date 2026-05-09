//! Radar diagram parsed model.
//!
//! Upstream reference: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/radar/
//! Grammar (langium): /ext/mermaid-official-stable-v11.14.0/packages/parser/src/language/radar/radar.langium
//!
//! The parser reduces a `radar-beta` source document to these plain data
//! structs, mirroring upstream's `RadarData`: axes, curves, options in
//! one-to-one correspondence. When an `axis` / `curve` has no explicit
//! `["..."]` label we copy `name` into `label`, matching upstream's
//! `axis.label ?? axis.name` fallback.

use crate::model::DiagramMeta;

/// Chart options; mirrors upstream `RadarOptions` field for field.
///
/// `max = None` means "not set"; the renderer falls back to the maximum
/// value across all curves, like upstream does.
#[derive(Debug, Clone, PartialEq)]
pub struct RadarOptions {
    pub show_legend: bool,
    pub ticks: u32,
    pub max: Option<f64>,
    pub min: f64,
    pub graticule: Graticule,
}

impl Default for RadarOptions {
    fn default() -> Self {
        // Matches upstream `db.ts::defaultOptions`.
        Self {
            show_legend: true,
            ticks: 5,
            max: None,
            min: 0.0,
            graticule: Graticule::Circle,
        }
    }
}

/// Graticule shape. The upstream terminal only accepts `circle | polygon`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Graticule {
    Circle,
    Polygon,
}

/// A single axis. `name` is the identifier; `label` may be an alias
/// supplied via the `["..."]` syntax.
#[derive(Debug, Clone, PartialEq)]
pub struct RadarAxis {
    pub name: String,
    pub label: String,
}

/// A single curve. When entries are written in axis-reference form the
/// parser already reorders them to match `axes`, so this model stores
/// only the final numeric sequence (length == axes.len()).
#[derive(Debug, Clone, PartialEq)]
pub struct RadarCurve {
    pub label: String,
    pub values: Vec<f64>,
}

/// Top-level radar model. Fields mirror upstream `RadarData`
/// (`axes / curves / options`) plus the shared `meta` (title etc.).
#[derive(Debug, Clone, Default)]
pub struct RadarDiagram {
    pub meta: DiagramMeta,
    pub axes: Vec<RadarAxis>,
    pub curves: Vec<RadarCurve>,
    pub options: RadarOptions,
}
