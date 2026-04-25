//! Theme-variable re-derivation for user `themeVariables` overlays.
//!
//! When a fixture supplies `themeVariables: { primaryColor: ..., darkMode: true, ... }`,
//! upstream's `theme-base.js#updateColors()` runs after the user keys
//! are merged in, recomputing every dependent field that the user
//! didn't explicitly override (`field = field || derived`).
//!
//! Our pre-resolved [`super::base::variables`] is a snapshot of that
//! pipeline for the *default* seed (`primaryColor=#fff4dd`). When the
//! user changes the seed we need to re-run a focused subset of
//! `updateColors` against the new seed and replace the derived fields
//! that aren't explicitly user-supplied.
//!
//! Scope: this initial pass covers the rules that participate in the
//! flowchart base preamble (`.error-icon`, `.cluster rect`, etc.) plus
//! the hooks `unified_shell`/`css.rs` read. Future agents can extend
//! the table without touching the call sites.

use super::color;
use super::ThemeVariables;
use serde_json::Value;

/// True if the user explicitly supplied `key` in their overlay JSON.
fn user_has(map: &serde_json::Map<String, Value>, key: &str) -> bool {
    map.contains_key(key)
}

/// Equivalent of `theme-base.js#updateColors()` — re-runs the subset of
/// derivations that touch fields visible in our current renderer
/// output. Only writes a slot when the user didn't supply that key.
pub fn re_derive_base(
    theme: &mut ThemeVariables,
    overrides: &serde_json::Map<String, Value>,
    dark_mode: bool,
) {
    // Helper closure: only set if user hasn't supplied this key.
    macro_rules! derive {
        ($key:literal, $slot:expr, $value:expr) => {
            if !user_has(overrides, $key) {
                $slot = Some($value);
            }
        };
    }

    let primary = theme
        .primary_color
        .clone()
        .unwrap_or_else(|| "#fff4dd".to_string());

    // primaryTextColor = primaryTextColor || (darkMode ? '#eee' : '#333')
    if !user_has(overrides, "primaryTextColor")
        && dark_mode
        && theme.primary_text_color.as_deref() != Some("#eee")
    {
        // Only overwrite when darkMode is on AND the slot still holds
        // the light-mode default — avoids clobbering already-correct
        // dark-theme renderings.
        theme.primary_text_color = Some("#eee".into());
    }

    // secondaryColor = secondaryColor || adjust(primaryColor, {h: -120})
    derive!(
        "secondaryColor",
        theme.secondary_color,
        color::adjust(&primary, &[('h', -120.0)])
    );
    let secondary = theme
        .secondary_color
        .clone()
        .unwrap_or_else(|| primary.clone());

    // tertiaryColor = tertiaryColor || adjust(primaryColor, {h: 180, l: 5})
    derive!(
        "tertiaryColor",
        theme.tertiary_color,
        color::adjust(&primary, &[('h', 180.0), ('l', 5.0)])
    );
    let tertiary = theme
        .tertiary_color
        .clone()
        .unwrap_or_else(|| primary.clone());

    // primaryBorderColor = primaryBorderColor || mkBorder(primaryColor, darkMode)
    derive!(
        "primaryBorderColor",
        theme.primary_border_color,
        color::mk_border(&primary, dark_mode)
    );
    derive!(
        "secondaryBorderColor",
        theme.secondary_border_color,
        color::mk_border(&secondary, dark_mode)
    );
    derive!(
        "tertiaryBorderColor",
        theme.tertiary_border_color,
        color::mk_border(&tertiary, dark_mode)
    );
    let tertiary_border = theme.tertiary_border_color.clone().unwrap_or_default();

    // noteBorderColor = noteBorderColor || mkBorder(noteBkgColor, darkMode)
    if let Some(nb) = theme.note_bkg_color.clone() {
        derive!(
            "noteBorderColor",
            theme.note_border_color,
            color::mk_border(&nb, dark_mode)
        );
    }

    // secondaryTextColor / tertiaryTextColor / lineColor / arrowheadColor
    // — produced by `invert(...)`. We do invert too (tested up to
    // simple cases like `#411d4e -> rgb(190, 226, 177)`).
    derive!(
        "secondaryTextColor",
        theme.secondary_text_color,
        color::invert(&secondary)
    );
    derive!(
        "tertiaryTextColor",
        theme.tertiary_text_color,
        color::invert(&tertiary)
    );
    if let Some(bg) = theme.background.clone() {
        derive!("lineColor", theme.line_color, color::invert(&bg));
        derive!("arrowheadColor", theme.arrowhead_color, color::invert(&bg));
    }

    // textColor = textColor || primaryTextColor
    if !user_has(overrides, "textColor") {
        if let Some(ptc) = theme.primary_text_color.clone() {
            theme.text_color = Some(ptc);
        }
    }

    // border2 = border2 || tertiaryBorderColor
    if !user_has(overrides, "border2") && !tertiary_border.is_empty() {
        theme.border2 = Some(tertiary_border.clone());
    }

    // nodeBkg = nodeBkg || primaryColor
    derive!("nodeBkg", theme.node_bkg, primary.clone());
    // mainBkg = mainBkg || primaryColor
    derive!("mainBkg", theme.main_bkg, primary.clone());
    // nodeBorder = nodeBorder || primaryBorderColor
    if !user_has(overrides, "nodeBorder") {
        if let Some(pbc) = theme.primary_border_color.clone() {
            theme.node_border = Some(pbc);
        }
    }
    // clusterBkg = clusterBkg || tertiaryColor
    derive!("clusterBkg", theme.cluster_bkg, tertiary.clone());
    // clusterBorder = clusterBorder || tertiaryBorderColor
    if !user_has(overrides, "clusterBorder") && !tertiary_border.is_empty() {
        theme.cluster_border = Some(tertiary_border.clone());
    }
    // titleColor = titleColor || tertiaryTextColor
    if !user_has(overrides, "titleColor") {
        if let Some(ttc) = theme.tertiary_text_color.clone() {
            theme.title_color = Some(ttc);
        }
    }
    // edgeLabelBackground = edgeLabelBackground || (darkMode ? darken(secondaryColor, 30) : secondaryColor)
    if !user_has(overrides, "edgeLabelBackground") {
        let derived = if dark_mode {
            color::darken(&secondary, 30.0)
        } else {
            secondary.clone()
        };
        theme.edge_label_background = Some(derived);
    }
    // nodeTextColor = nodeTextColor || primaryTextColor
    if !user_has(overrides, "nodeTextColor") {
        if let Some(ptc) = theme.primary_text_color.clone() {
            theme.node_text_color = Some(ptc);
        }
    }
    // errorBkgColor = errorBkgColor || tertiaryColor
    derive!("errorBkgColor", theme.error_bkg_color, tertiary.clone());
    // errorTextColor = errorTextColor || tertiaryTextColor
    if !user_has(overrides, "errorTextColor") {
        if let Some(ttc) = theme.tertiary_text_color.clone() {
            theme.error_text_color = Some(ttc);
        }
    }
    // labelBackgroundColor = labelBackgroundColor || stateBkg (= mainBkg)
    if !user_has(overrides, "labelBackgroundColor") {
        if let Some(mb) = theme.main_bkg.clone() {
            theme.label_background_color = Some(mb);
        }
    }
    // sectionBkgColor = sectionBkgColor || tertiaryColor
    derive!("sectionBkgColor", theme.section_bkg_color, tertiary.clone());
    // sectionBkgColor2 = sectionBkgColor2 || primaryColor
    derive!(
        "sectionBkgColor2",
        theme.section_bkg_color2,
        primary.clone()
    );
    // altBackground = altBackground || tertiaryColor
    derive!("altBackground", theme.alt_background, tertiary.clone());
    // compositeBackground = compositeBackground || background || tertiaryColor
    if !user_has(overrides, "compositeBackground") {
        let cb = theme.background.clone().unwrap_or_else(|| tertiary.clone());
        theme.composite_background = Some(cb);
    }
    // compositeTitleBackground = compositeTitleBackground || mainBkg
    if !user_has(overrides, "compositeTitleBackground") {
        if let Some(mb) = theme.main_bkg.clone() {
            theme.composite_title_background = Some(mb);
        }
    }
    // compositeBorder = compositeBorder || nodeBorder
    if !user_has(overrides, "compositeBorder") {
        if let Some(nb) = theme.node_border.clone() {
            theme.composite_border = Some(nb);
        }
    }
    // innerEndBackground = nodeBorder
    if !user_has(overrides, "innerEndBackground") {
        if let Some(nb) = theme.node_border.clone() {
            theme.inner_end_background = Some(nb);
        }
    }

    // gradient_start / gradient_stop = primaryBorderColor / secondaryBorderColor
    if let Some(pbc) = theme.primary_border_color.clone() {
        theme.gradient_start = Some(pbc);
    }
    if let Some(sbc) = theme.secondary_border_color.clone() {
        theme.gradient_stop = Some(sbc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{base, color::adjust};

    #[test]
    fn rederive_after_primary_override() {
        let mut th = base::variables();
        th.primary_color = Some("#411d4e".into());
        let mut overrides = serde_json::Map::new();
        overrides.insert("primaryColor".into(), Value::String("#411d4e".into()));
        overrides.insert("darkMode".into(), Value::Bool(true));
        re_derive_base(&mut th, &overrides, true);
        assert_eq!(
            th.tertiary_color.as_deref(),
            Some("hsl(104.0816326531, 45.7943925234%, 25.9803921569%)")
        );
        assert_eq!(
            th.error_bkg_color.as_deref(),
            Some("hsl(104.0816326531, 45.7943925234%, 25.9803921569%)")
        );
        // adjust round-trip: tertiary should match adjust("#411d4e", h=180,l=5).
        let expect = adjust("#411d4e", &[('h', 180.0), ('l', 5.0)]);
        assert_eq!(th.tertiary_color.as_deref(), Some(expect.as_str()));
    }
}
