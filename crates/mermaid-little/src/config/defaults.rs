//! Default config values — port of upstream `defaultConfig.ts` +
//! `schemas/config.schema.yaml` defaults, restricted to the keys we
//! actually consume in Wave 0/1/2.
//!
//! We deliberately do **not** translate all 2527 lines of the schema.
//! Unknown fields round-trip through [`super::Config::extras`], so the
//! surface here only grows when we promote a key to a typed field.

use super::{Config, FlowchartConfig};

/// Upstream default font family (see `schemas/config.schema.yaml`
/// l. 169-175). Mermaid ships this as the default `fontFamily` global
/// and the themes fall back to it.
pub const DEFAULT_FONT_FAMILY: &str = "\"trebuchet ms\", verdana, arial, sans-serif;";

/// The mermaid default theme identifier.
pub const DEFAULT_THEME: &str = "default";

/// The mermaid default security level — `strict` (schema l. 206-222).
pub const DEFAULT_SECURITY_LEVEL: &str = "strict";

/// The mermaid default look.
pub const DEFAULT_LOOK: &str = "classic";

/// The mermaid default layout algorithm.
pub const DEFAULT_LAYOUT: &str = "dagre";

/// Default `fontSize` — upstream hard-codes 16 in the CSS fall-through;
/// exposed here so parity work can reference a named constant.
pub const DEFAULT_FONT_SIZE: f64 = 16.0;

/// Defaults for the flowchart sub-config (schema l. 2095-2209).
pub fn flowchart_defaults() -> FlowchartConfig {
    FlowchartConfig {
        default_renderer: Some("dagre-wrapper".into()),
        html_labels: Some(true),
        curve: Some("basis".into()),
        node_spacing: Some(50),
        rank_spacing: Some(50),
        diagram_padding: Some(8),
        title_top_margin: Some(25),
        wrapping_width: Some(200),
        inherit_dir: None,
        extras: Default::default(),
    }
}

/// Produce the built-in default [`Config`]. This is the bottom of the
/// merge stack — every subsequent layer (site / frontmatter / init)
/// overlays on top of it.
pub fn defaults() -> Config {
    Config {
        theme: Some(DEFAULT_THEME.into()),
        font_family: Some(DEFAULT_FONT_FAMILY.into()),
        font_size: Some(DEFAULT_FONT_SIZE),
        security_level: Some(DEFAULT_SECURITY_LEVEL.into()),
        look: Some(DEFAULT_LOOK.into()),
        layout: Some(DEFAULT_LAYOUT.into()),
        start_on_load: Some(true),
        html_labels: Some(true),
        flowchart: Some(flowchart_defaults()),
        ..Config::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_include_theme_and_security_level() {
        let d = defaults();
        assert_eq!(d.theme.as_deref(), Some("default"));
        assert_eq!(d.security_level.as_deref(), Some("strict"));
        assert_eq!(d.look.as_deref(), Some("classic"));
    }

    #[test]
    fn defaults_include_flowchart_block() {
        let d = defaults();
        let fc = d.flowchart.as_ref().expect("flowchart defaults present");
        assert_eq!(fc.curve.as_deref(), Some("basis"));
        assert_eq!(fc.node_spacing, Some(50));
    }

    #[test]
    fn defaults_round_trip_via_serde() {
        let d = defaults();
        let encoded = serde_json::to_string(&d).unwrap();
        let back: Config = serde_json::from_str(&encoded).unwrap();
        assert_eq!(d, back);
    }
}
