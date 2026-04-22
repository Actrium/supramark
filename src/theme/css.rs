//! Theme → CSS composition. Port of upstream
//! `packages/mermaid/src/styles.ts` — assembles the base preamble that
//! every mermaid diagram shares (animations, edge helpers, marker
//! defaults) with the per-diagram CSS, before stylis minification.
//!
//! The output string is *already minified* (no redundant whitespace or
//! comments), scoped under `#<id>`, so a byte-exact consumer can write
//! it straight into the `<style>` block without a second pass through
//! stylis. This works because mermaid's per-diagram CSS templates are
//! flat (no `& .foo` nesting beyond one level, no shorthand expansion),
//! so hand-emitting the minified form matches the stylis-minified form
//! byte-for-byte for the diagram kinds we cover.
//!
//! The trade-off is that each [`DiagramKind`] needs a matching CSS
//! template here. For the `Er`, `Block`, `Flowchart`, `State`,
//! `Requirement` diagrams in Wave 4, we reuse what the per-renderer
//! modules already produce; this module just exposes the base preamble
//! + a [`neo_look_block`] helper so renderers stop duplicating it.

use crate::render::stylis;
use crate::theme::ThemeVariables;

/// Narrow enum listing the diagram kinds that share the Wave-4 shell
/// + CSS machinery.
///
/// The upstream registry has ~30 entries; the rest (pie, packet,
/// radar, xy-chart, …) bring their own bespoke stylesheet and don't
/// call through this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramKind {
    Er,
    Block,
    Flowchart,
    State,
    Class,
    Requirement,
}

impl DiagramKind {
    /// The string used in `aria-roledescription` and upstream's
    /// registration key.
    #[must_use]
    pub fn aria(self) -> &'static str {
        match self {
            Self::Er => "er",
            Self::Block => "block",
            Self::Flowchart => "flowchart-v2",
            Self::State => "stateDiagram",
            Self::Class => "classDiagram",
            Self::Requirement => "requirement",
        }
    }

    /// The value of the SVG `class=` attribute on the outer element.
    #[must_use]
    pub fn svg_class(self) -> &'static str {
        match self {
            Self::Er => "erDiagram",
            Self::Block => "block",
            Self::Flowchart => "flowchart",
            Self::State => "statediagram",
            Self::Class => "classDiagram",
            Self::Requirement => "requirementDiagram",
        }
    }
}

/// Resolve the six theme values every preamble references, replacing
/// absent entries with upstream's "default" theme defaults.
struct BaseVars<'a> {
    font_family: String,
    font_size: &'a str,
    text_color: &'a str,
    error_bkg: &'a str,
    error_text: &'a str,
    line_color: &'a str,
    stroke_width: i64,
}

impl<'a> BaseVars<'a> {
    fn resolve(theme: &'a ThemeVariables) -> Self {
        let raw_ff = theme
            .font_family
            .as_deref()
            .unwrap_or("\"trebuchet ms\", verdana, arial, sans-serif");
        Self {
            font_family: stylis::strip_comma_spaces(raw_ff),
            font_size: theme.font_size.as_deref().unwrap_or("16px"),
            text_color: theme.text_color.as_deref().unwrap_or("#333"),
            error_bkg: theme.error_bkg_color.as_deref().unwrap_or("#552222"),
            error_text: theme.error_text_color.as_deref().unwrap_or("#552222"),
            line_color: theme.line_color.as_deref().unwrap_or("#333333"),
            stroke_width: theme.stroke_width.unwrap_or(1),
        }
    }
}

/// Emit the shared preamble that opens every diagram's `<style>` block.
///
/// The output covers (in order, matching upstream's `getStyles()`):
///
/// 1. The root `#<id>` rule: font-family, font-size, fill.
/// 2. Two `@keyframes` — `edge-animation-frame`, `dash`.
/// 3. Slow / fast edge animation helper classes.
/// 4. `.error-icon`, `.error-text`.
/// 5. Edge thickness + pattern helpers (5 rules).
/// 6. `.marker` + `.marker.cross`.
/// 7. `svg` + `p` defaults.
///
/// Output does **not** include an opening `<style>` tag — it's raw
/// CSS ready to be concatenated with per-diagram rules.
#[must_use]
pub fn base_preamble(id: &str, theme: &ThemeVariables) -> String {
    let v = BaseVars::resolve(theme);
    let mut s = String::with_capacity(1280);
    // (1) root rule.
    s.push_str(&format!(
        "#{id}{{font-family:{ff};font-size:{fs};fill:{tc};}}",
        id = id,
        ff = v.font_family,
        fs = v.font_size,
        tc = v.text_color,
    ));
    // (2) keyframes.
    s.push_str("@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}");
    s.push_str("@keyframes dash{to{stroke-dashoffset:0;}}");
    // (3) animation helpers.
    s.push_str(&format!(
        "#{id} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}",
    ));
    s.push_str(&format!(
        "#{id} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}",
    ));
    // (4) error helpers.
    s.push_str(&format!(
        "#{id} .error-icon{{fill:{eb};}}",
        id = id,
        eb = v.error_bkg,
    ));
    s.push_str(&format!(
        "#{id} .error-text{{fill:{et};stroke:{et};}}",
        id = id,
        et = v.error_text,
    ));
    // (5) edge thickness + pattern.
    s.push_str(&format!(
        "#{id} .edge-thickness-normal{{stroke-width:{sw}px;}}",
        id = id,
        sw = v.stroke_width,
    ));
    s.push_str(&format!(
        "#{id} .edge-thickness-thick{{stroke-width:3.5px;}}"
    ));
    s.push_str(&format!("#{id} .edge-pattern-solid{{stroke-dasharray:0;}}"));
    s.push_str(&format!(
        "#{id} .edge-thickness-invisible{{stroke-width:0;fill:none;}}",
    ));
    s.push_str(&format!(
        "#{id} .edge-pattern-dashed{{stroke-dasharray:3;}}"
    ));
    s.push_str(&format!(
        "#{id} .edge-pattern-dotted{{stroke-dasharray:2;}}"
    ));
    // (6) marker.
    s.push_str(&format!(
        "#{id} .marker{{fill:{lc};stroke:{lc};}}",
        id = id,
        lc = v.line_color,
    ));
    s.push_str(&format!(
        "#{id} .marker.cross{{stroke:{lc};}}",
        id = id,
        lc = v.line_color,
    ));
    // (7) svg + p.
    s.push_str(&format!(
        "#{id} svg{{font-family:{ff};font-size:{fs};}}",
        id = id,
        ff = v.font_family,
        fs = v.font_size,
    ));
    s.push_str(&format!("#{id} p{{margin:0;}}"));
    s
}

/// Emit the shared "neo look" tail that closes every diagram's style
/// block in upstream's `getStyles()`. This covers the 10 `[data-look]`
/// rules plus the final `:root` variable declaration.
///
/// Mermaid unconditionally emits these rules even when the diagram
/// does not use `look: neo`, so they're part of every reference SVG.
#[must_use]
pub fn neo_look_block(id: &str, theme: &ThemeVariables) -> String {
    let v = BaseVars::resolve(theme);
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    // Upstream's `dropShadow` theme variable defaults to
    // `drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))`. Not all themes
    // set it; when unset (common) mermaid substitutes the literal
    // string below.
    let drop_shadow = theme
        .drop_shadow
        .as_deref()
        .unwrap_or("drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))");
    let mut s = String::with_capacity(2048);
    s.push_str(&format!(
        "#{id} .node .neo-node{{stroke:{nb};}}",
        id = id,
        nb = node_border,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node rect,#{id} [data-look="neo"].cluster rect,#{id} [data-look="neo"].node polygon{{stroke:{nb};filter:{ds};}}"#,
        id = id,
        nb = node_border,
        ds = drop_shadow,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node path{{stroke:{nb};stroke-width:{sw}px;}}"#,
        id = id,
        nb = node_border,
        sw = v.stroke_width,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node .outer-path{{filter:{ds};}}"#,
        id = id,
        ds = drop_shadow,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node .neo-line path{{stroke:{nb};filter:none;}}"#,
        id = id,
        nb = node_border,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle{{stroke:{nb};filter:{ds};}}"#,
        id = id,
        nb = node_border,
        ds = drop_shadow,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].node circle .state-start{{fill:#000000;}}"#,
        id = id,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon{{fill:{nb};filter:{ds};}}"#,
        id = id,
        nb = node_border,
        ds = drop_shadow,
    ));
    s.push_str(&format!(
        r#"#{id} [data-look="neo"].icon-shape .icon-neo path{{stroke:{nb};filter:{ds};}}"#,
        id = id,
        nb = node_border,
        ds = drop_shadow,
    ));
    s.push_str(&format!(
        "#{id} :root{{--mermaid-font-family:{ff};}}",
        id = id,
        ff = v.font_family,
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::get_theme;

    #[test]
    fn preamble_starts_with_root_rule() {
        let th = get_theme("default");
        let css = base_preamble("foo", &th);
        assert!(css.starts_with("#foo{font-family:"));
        assert!(css.contains("@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}"));
        assert!(css.contains("#foo svg{font-family:"));
        assert!(css.contains("#foo p{margin:0;}"));
    }

    #[test]
    fn preamble_strips_comma_spaces_from_font_family() {
        let th = get_theme("default");
        let css = base_preamble("x", &th);
        // Font family list must be stylis-minified (no space after
        // commas outside the quoted "trebuchet ms").
        assert!(css.contains(r#"font-family:"trebuchet ms",verdana,arial,sans-serif"#));
    }

    #[test]
    fn neo_block_ends_with_root_mermaid_variable() {
        let th = get_theme("default");
        let css = neo_look_block("y", &th);
        assert!(css.ends_with(r#""trebuchet ms",verdana,arial,sans-serif;}"#));
        assert!(css.contains(r#"#y [data-look="neo"].node rect"#));
    }
}
