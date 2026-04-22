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
pub mod math;
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
        detect::DiagramKind::Treemap => {
            let d = parser::treemap::parse(source)?;
            // Honour theme override from frontmatter if the parser lifted one.
            let effective_theme = if let Some(name) = d.theme_override.as_deref() {
                theme::get_theme(name)
            } else {
                theme.clone()
            };
            let l = layout::treemap::layout(&d, &effective_theme)?;
            render::svg_treemap::render(&d, &l, &effective_theme, id)
        }
        detect::DiagramKind::Ishikawa => {
            let d = parser::ishikawa::parse(source)?;
            let l = layout::ishikawa::layout(&d, &theme)?;
            render::svg_ishikawa::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Journey => {
            let d = parser::journey::parse(source)?;
            let l = layout::journey::layout(&d, &theme)?;
            render::svg_journey::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Timeline => {
            let d = parser::timeline::parse(source)?;
            let l = layout::timeline::layout(&d, &theme)?;
            render::svg_timeline::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Quadrant => {
            let d = parser::quadrant::parse(source)?;
            let l = layout::quadrant::layout(&d, &theme)?;
            render::svg_quadrant::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Xychart => {
            let d = parser::xychart::parse(source)?;
            let l = layout::xychart::layout(&d, &theme)?;
            render::svg_xychart::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Wardley => {
            let d = parser::wardley::parse(source)?;
            let l = layout::wardley::layout(&d, &theme)?;
            render::svg_wardley::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Sankey => {
            let d = parser::sankey::parse(source)?;
            let l = layout::sankey::layout(&d, &theme)?;
            render::svg_sankey::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Kanban => {
            let d = parser::kanban::parse(source)?;
            let l = layout::kanban::layout(&d, &theme)?;
            render::svg_kanban::render(&d, &l, &theme, id)
        }
        other => Err(MermaidError::Unsupported(format!(
            "diagram kind '{}' not yet implemented — Wave 4 scope: er/class/state/flowchart/block; Wave 7: sequence/c4/gitgraph; gantt/mindmap TBD",
            other.id()
        ))),
    }
}

/// Convenience wrapper using a default id.
pub fn convert(source: &str) -> Result<String, MermaidError> {
    convert_with_id(source, "mermaid-1")
}
