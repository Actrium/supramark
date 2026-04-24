//! mermaid-little — pure-Rust reimplementation of Mermaid, targeting
//! byte-exact SVG output parity with upstream `mermaid@11.14.0`.
//!
//! Current status: foundations + reference pipeline are landed.
//! `convert_with_id` dispatches the Wave 1/2 geometry families plus the
//! active Stratum 3 dagre family (`er` / `block` / `requirement` /
//! `class` / `state` / `flowchart`). `gantt` is parser/layout-only for
//! now; `sequence` / `c4` / `gitGraph` / `mindmap` are still pending.
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
            // Honour theme overrides captured by the timeline parser:
            // — `theme_name` (from frontmatter `config.theme` or a
            //   `%%{init: { theme: ... }}%%` directive),
            // — `theme_overrides.c_scale*` (from `themeVariables`).
            let mut effective_theme = if let Some(name) = d.theme_name.as_deref() {
                theme::get_theme(name)
            } else {
                theme.clone()
            };
            for (i, v) in d.theme_overrides.c_scale.iter().enumerate() {
                if let Some(s) = v {
                    match i {
                        0 => effective_theme.c_scale0 = Some(s.clone()),
                        1 => effective_theme.c_scale1 = Some(s.clone()),
                        2 => effective_theme.c_scale2 = Some(s.clone()),
                        3 => effective_theme.c_scale3 = Some(s.clone()),
                        4 => effective_theme.c_scale4 = Some(s.clone()),
                        5 => effective_theme.c_scale5 = Some(s.clone()),
                        6 => effective_theme.c_scale6 = Some(s.clone()),
                        7 => effective_theme.c_scale7 = Some(s.clone()),
                        8 => effective_theme.c_scale8 = Some(s.clone()),
                        9 => effective_theme.c_scale9 = Some(s.clone()),
                        10 => effective_theme.c_scale10 = Some(s.clone()),
                        11 => effective_theme.c_scale11 = Some(s.clone()),
                        _ => {}
                    }
                }
            }
            let l = layout::timeline::layout(&d, &effective_theme)?;
            render::svg_timeline::render(&d, &l, &effective_theme, id)
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
        detect::DiagramKind::Er => {
            let d = parser::er::parse(source)?;
            let l = layout::er::layout(&d, &theme)?;
            render::svg_er::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Block => {
            let d = parser::block::parse(source)?;
            let l = layout::block::layout(&d, &theme)?;
            render::svg_block::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Requirement => {
            let d = parser::requirement::parse(source)?;
            let l = layout::requirement::layout(&d, &theme)?;
            render::svg_requirement::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Class => {
            let d = parser::class::parse(source)?;
            let l = layout::class::layout(&d, &theme)?;
            render::svg_class::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::State => {
            let d = parser::state::parse(source)?;
            let l = layout::state::layout(&d, &theme)?;
            render::svg_state::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Flowchart => {
            let d = parser::flowchart::parse(source)?;
            let l = layout::flowchart::layout(&d, &theme)?;
            render::svg_flowchart::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Gantt => {
            let d = parser::gantt::parse(source)?;
            let effective_theme = if let Some(name) = d.theme_name.as_deref() {
                theme::get_theme(name)
            } else {
                theme.clone()
            };
            let l = layout::gantt::layout(&d, &effective_theme)?;
            render::svg_gantt::render(&d, &l, &effective_theme, id)
        }
        other => Err(MermaidError::Unsupported(format!(
            "diagram kind '{}' not yet implemented — Wave 7: sequence/c4/gitgraph; mindmap TBD",
            other.id()
        ))),
    }
}

/// Convenience wrapper using a default id.
pub fn convert(source: &str) -> Result<String, MermaidError> {
    convert_with_id(source, "mermaid-1")
}
