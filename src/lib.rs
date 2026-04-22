//! mermaid-little — pure-Rust reimplementation of Mermaid, targeting
//! byte-exact SVG output parity with upstream `mermaid@11.14.0`.
//!
//! Wave 0 lands foundations: module tree, Diagram/DiagramLayout enums
//! with placeholders for all 25 diagram types, config + preprocess +
//! detect pipeline, theme variants, eval harness. No diagram types
//! render yet — each Wave 1+ milestone fills in one or more variants.
//!
//! Licensing: core crate is MIT. Portions vendored from sister
//! projects (plantuml-little, dagre-rs, selkie, mmdr, mmdflux) are
//! marked with per-file attribution blocks.

pub mod config;
pub mod detect;
pub mod error;
pub mod font_data;
pub mod font_metrics;
pub mod layout;
pub mod model;
pub mod parser;
pub mod preprocess;
pub mod render;
pub mod text;
pub mod theme;

pub use error::MermaidError;

/// Convert mermaid source text (`.mmd`) into SVG.
///
/// The `id` argument becomes the root `<svg id="..">` attribute and is
/// scoped through CSS selectors. Use a stable value — e.g. the
/// fixture path — for byte-exact reproducibility.
pub fn convert_with_id(source: &str, id: &str) -> Result<String, MermaidError> {
    // Preprocess only to (a) pick the right diagram type — detection
    // runs on the frontmatter/directive-stripped head — and (b) read
    // the global `theme` name. Per-diagram parsers receive the RAW
    // source because each one self-extracts its own frontmatter and
    // `%%{init:...}%%` directive (e.g. `pie.textPosition`,
    // `packet.showBits`, `themeVariables.pieOuterStrokeWidth`). Doing
    // it this way lets Wave 1 agents keep one API boundary —
    // `parse(&str)` — without a Config parameter.
    let pre = preprocess::preprocess(source)?;
    let theme_name = pre.config.theme.as_deref().unwrap_or("default");
    let theme = theme::get_theme(theme_name);
    let kind = detect::detect(&pre.cleaned_source);

    match kind {
        detect::DiagramKind::Pie => {
            let d = parser::pie::parse(source)?;
            let l = layout::pie::layout(&d, &theme)?;
            render::svg_pie::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Packet => {
            let d = parser::packet::parse(source)?;
            let l = layout::packet::layout(&d, &theme)?;
            render::svg_packet::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Radar => {
            let d = parser::radar::parse(source)?;
            let l = layout::radar::layout(&d, &theme)?;
            render::svg_radar::render(&d, &l, &theme, id)
        }
        other => Err(MermaidError::Unsupported(format!(
            "diagram kind '{}' not yet implemented — Wave 1 covers pie, packet, radar",
            other.id()
        ))),
    }
}

/// Convenience wrapper using a default id.
pub fn convert(source: &str) -> Result<String, MermaidError> {
    convert_with_id(source, "mermaid-1")
}
