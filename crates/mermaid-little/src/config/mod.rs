//! Mermaid config — directives, frontmatter, defaults, schema.
//!
//! Merge order (port of upstream mermaid `config.ts`):
//!   default  ←  site  ←  frontmatter  ←  `%%{init: ...}%%`
//! Later wins. Byte-exact parity with upstream depends on this exact order.
//!
//! Wave 0 exposes only the fields we know we'll consume in the near term
//! (pie / packet / radar don't need much). Additional keys round-trip
//! through [`Config::extras`] as raw JSON, so we stay forward-compatible
//! without pre-defining every corner of the 2527-line JSON schema.

pub mod defaults;
pub mod directive;
pub mod frontmatter;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Flowchart-specific sub-config. Fields cover the keys we peek at during
/// detection (`defaultRenderer`) plus layout-sensitive dimensions we know
/// Wave 1 flowchart work will consume.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct FlowchartConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_renderer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_labels: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curve: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_spacing: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_spacing: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram_padding: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_top_margin: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapping_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherit_dir: Option<bool>,
    /// Unknown flowchart-scoped keys — kept for round-trip & forward-compat.
    #[serde(flatten)]
    pub extras: BTreeMap<String, Value>,
}

/// Gantt-specific sub-config. Only carries `displayMode` today, because
/// that's the single legacy field [`crate::preprocess`] has to hoist out
/// of frontmatter.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GanttConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_mode: Option<String>,
    #[serde(flatten)]
    pub extras: BTreeMap<String, Value>,
}

/// Top-level mermaid config — mirrors upstream `MermaidConfig` but with
/// only the globals we actually read in Wave 0/1/2.
///
/// All declared fields are `Option<T>`, which lets us distinguish
/// "unset" from "explicitly default". [`Config::merge`] uses this to
/// implement the later-wins overlay semantics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_on_load: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_labels: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flowchart: Option<FlowchartConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gantt: Option<GanttConfig>,
    /// `themeVariables` is a free-form map (hundreds of CSS-ish keys);
    /// we store it as raw JSON and touch it only on demand.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_variables: Option<Value>,
    /// `wrap` (set by the `%%{wrap}%%` directive shorthand).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    /// Catch-all for keys we haven't promoted to typed fields yet.
    /// This is what keeps future-compat cheap — a new mermaid version
    /// that adds a top-level key still round-trips through `extras`.
    #[serde(flatten)]
    pub extras: BTreeMap<String, Value>,
}

impl Config {
    /// Produce the starting-point default config (port of upstream
    /// `defaultConfig.ts`).
    pub fn builtin_defaults() -> Self {
        defaults::defaults()
    }

    /// Deep-merge `overlay` on top of `self`. Overlay fields that are
    /// `Some(_)` replace `self`'s values; `None` leaves `self` alone.
    ///
    /// For nested structs (`flowchart`, `gantt`) we recurse so a partial
    /// overlay only overwrites the fields it actually mentions — that
    /// mirrors upstream's `assignWithDepth` semantics.
    pub fn merge(mut self, overlay: Self) -> Self {
        macro_rules! take {
            ($field:ident) => {
                if overlay.$field.is_some() {
                    self.$field = overlay.$field;
                }
            };
        }
        take!(theme);
        take!(font_family);
        take!(font_size);
        take!(security_level);
        take!(title);
        take!(look);
        take!(layout);
        take!(start_on_load);
        take!(html_labels);
        take!(theme_variables);
        take!(wrap);

        self.flowchart = merge_flowchart(self.flowchart, overlay.flowchart);
        self.gantt = merge_gantt(self.gantt, overlay.gantt);

        for (k, v) in overlay.extras {
            self.extras.insert(k, v);
        }
        self
    }

    /// Fold a chain of overlays in merge-order (`default ← site ← fm ← init`).
    ///
    /// Convenience wrapper around [`Config::merge`] that makes the
    /// directive-stack semantics explicit at the call site in
    /// [`crate::preprocess::preprocess`].
    pub fn fold(base: Self, layers: impl IntoIterator<Item = Self>) -> Self {
        layers.into_iter().fold(base, Config::merge)
    }
}

fn merge_flowchart(
    base: Option<FlowchartConfig>,
    overlay: Option<FlowchartConfig>,
) -> Option<FlowchartConfig> {
    match (base, overlay) {
        (None, None) => None,
        (Some(b), None) => Some(b),
        (None, Some(o)) => Some(o),
        (Some(mut b), Some(o)) => {
            macro_rules! take {
                ($field:ident) => {
                    if o.$field.is_some() {
                        b.$field = o.$field;
                    }
                };
            }
            take!(default_renderer);
            take!(html_labels);
            take!(curve);
            take!(node_spacing);
            take!(rank_spacing);
            take!(diagram_padding);
            take!(title_top_margin);
            take!(wrapping_width);
            take!(inherit_dir);
            for (k, v) in o.extras {
                b.extras.insert(k, v);
            }
            Some(b)
        }
    }
}

fn merge_gantt(base: Option<GanttConfig>, overlay: Option<GanttConfig>) -> Option<GanttConfig> {
    match (base, overlay) {
        (None, None) => None,
        (Some(b), None) => Some(b),
        (None, Some(o)) => Some(o),
        (Some(mut b), Some(o)) => {
            if o.display_mode.is_some() {
                b.display_mode = o.display_mode;
            }
            for (k, v) in o.extras {
                b.extras.insert(k, v);
            }
            Some(b)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_overlay_replaces_scalar_fields() {
        let base = Config {
            theme: Some("default".into()),
            font_family: Some("sans".into()),
            ..Config::default()
        };
        let overlay = Config {
            theme: Some("dark".into()),
            ..Config::default()
        };
        let merged = base.merge(overlay);
        assert_eq!(merged.theme.as_deref(), Some("dark"));
        // unset overlay field must leave base intact
        assert_eq!(merged.font_family.as_deref(), Some("sans"));
    }

    #[test]
    fn merge_is_associative_for_directive_stacks() {
        // default ← frontmatter ← init — confirms fold order matches spec.
        let default = Config {
            theme: Some("default".into()),
            security_level: Some("strict".into()),
            ..Config::default()
        };
        let frontmatter = Config {
            theme: Some("forest".into()),
            ..Config::default()
        };
        let init = Config {
            theme: Some("dark".into()),
            ..Config::default()
        };
        let merged = Config::fold(default, [frontmatter, init]);
        assert_eq!(merged.theme.as_deref(), Some("dark")); // init wins
        assert_eq!(merged.security_level.as_deref(), Some("strict"));
    }

    #[test]
    fn merge_recurses_into_flowchart() {
        let base = Config {
            flowchart: Some(FlowchartConfig {
                default_renderer: Some("dagre-wrapper".into()),
                curve: Some("basis".into()),
                ..FlowchartConfig::default()
            }),
            ..Config::default()
        };
        let overlay = Config {
            flowchart: Some(FlowchartConfig {
                curve: Some("linear".into()),
                ..FlowchartConfig::default()
            }),
            ..Config::default()
        };
        let merged = base.merge(overlay);
        let fc = merged.flowchart.unwrap();
        assert_eq!(fc.default_renderer.as_deref(), Some("dagre-wrapper"));
        assert_eq!(fc.curve.as_deref(), Some("linear"));
    }

    #[test]
    fn extras_round_trip_through_json() {
        let json = r#"{"theme":"dark","myCustom":{"foo":1}}"#;
        let c: Config = serde_json::from_str(json).unwrap();
        assert_eq!(c.theme.as_deref(), Some("dark"));
        assert!(c.extras.contains_key("myCustom"));
        let back = serde_json::to_string(&c).unwrap();
        // `extras` round-trips losslessly
        assert!(back.contains("\"myCustom\""));
    }
}
