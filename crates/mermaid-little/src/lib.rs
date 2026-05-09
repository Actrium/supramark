//! mermaid-little — pure-Rust reimplementation of Mermaid, targeting
//! byte-exact SVG output parity with upstream `mermaid@11.14.0`.
//!
//! Current status: foundations + reference pipeline are landed.
//! `convert_with_id` dispatches the Wave 1/2 geometry families plus the
//! active Stratum 3 dagre family (`er` / `block` / `requirement` /
//! `class` / `state` / `flowchart`). `gantt` is parser/layout-only for
//! now; `sequence` / `gitGraph` / `mindmap` are still pending. `c4`
//! has parser scaffolding only — its layout/render port is open work
//! tracked in `tests/known_ignored.txt`.
//!
//! Licensing: core crate is MIT. Portions vendored from sister
//! projects (plantuml-little, dagre-rs, selkie, mmdr, mmdflux) are
//! marked with per-file attribution blocks.

pub mod config;
pub mod detect;
pub mod error;
pub mod font_data;
pub mod font_metrics;
#[cfg(feature = "katex")]
pub mod katex;
#[cfg(feature = "cose_bilkent")]
pub mod cose_bilkent_js;
pub mod layout;
pub mod math;
pub mod model;
pub mod parser;
pub mod preprocess;
pub mod render;
pub mod svg_match;
pub mod text;
pub mod theme;

pub use error::MermaidError;

/// Mirror `tests/support/generate_ref.mjs#renumberCounterIds` —
/// renumber matched `(prefix, counter)` pairs by first-appearance
/// order, splicing them back with `sep`. Used at SVG output time so
/// our render matches the normalised reference: upstream's mermaid
/// uses module-level counters that accumulate across batch renders;
/// `generate_ref` then renumbers by first-appearance per-SVG so the
/// output is stable. We must apply the same pass to our output —
/// otherwise our per-render sequential counters (e.g. classCounter
/// 0,1,2…) match the *literal* upstream values only when the upstream
/// batch happened to start at 0, which depends on fixture ordering.
fn renumber_counter_ids(svg: &str, re: &regex::Regex, sep: &str) -> String {
    use std::collections::HashMap;
    let mut map: HashMap<String, usize> = HashMap::new();
    let mut next = 0usize;
    re.replace_all(svg, |caps: &regex::Captures| {
        let id = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let counter = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let key = format!("{}:{}", id, counter);
        let v = *map.entry(key).or_insert_with(|| {
            let cur = next;
            next += 1;
            cur
        });
        format!("{}{}{}", id, sep, v)
    })
    .into_owned()
}

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
    let svg = convert_with_id_inner(source, id)?;
    // Apply the same per-SVG counter normalisation that
    // `generate_ref.mjs` runs over upstream's reference output. Only
    // `classId-\w+-N` is relevant today — class diagrams are the only
    // implemented family whose IDs leak a module-level counter.
    static CLASS_ID_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = CLASS_ID_RE.get_or_init(|| regex::Regex::new(r"(classId-\w+)-(\d+)").unwrap());
    Ok(renumber_counter_ids(&svg, re, "-"))
}

fn convert_with_id_inner(source: &str, id: &str) -> Result<String, MermaidError> {
    let pre = preprocess::preprocess(source)?;
    let theme_name = pre.config.theme.as_deref().unwrap_or("default");
    let mut theme = theme::get_theme(theme_name);
    // Apply `themeVariables` overlay (init/frontmatter). Currently
    // covers darkMode-derived text color plus a curated list of direct
    // string overrides — see `theme::apply_theme_variables` for the
    // whitelist. Values absent from the overlay leave the theme
    // intact, so default-theme fixtures are unaffected.
    if let Some(tv) = pre.config.theme_variables.as_ref() {
        theme::apply_theme_variables(&mut theme, tv);
    }
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
            let mut d = parser::ishikawa::parse(source)?;
            // Plumb look + handDrawnSeed from the resolved config stack
            // (default ← site ← frontmatter ← %%{init}%%) so the renderer
            // can dispatch the handDrawn variant deterministically.
            d.look = pre.config.look.clone();
            d.hand_drawn_seed = pre
                .config
                .extras
                .get("handDrawnSeed")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .or(Some(0));
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
            // Detect whether this theme recomputes cScaleInv from cScale
            // when the user overrides cScale. Upstream's default/forest/dark
            // themes call `updateColors()` in their constructor, baking the
            // derived `cScaleInv` into the prototype before any user override
            // arrives, so user-provided `cScale*` does NOT propagate to
            // `cScaleInv*`. Base/neutral DON'T call updateColors in the
            // constructor — their `cScaleInv` is only ever computed from the
            // current `cScale`, so a user override DOES propagate.
            let resolved_theme_name = d
                .theme_name
                .as_deref()
                .or(pre.config.theme.as_deref())
                .unwrap_or("default");
            let theme_recomputes_inv = matches!(resolved_theme_name, "base" | "neutral");
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
                    if theme_recomputes_inv {
                        let darkened = theme::color::darken(s, 25.0);
                        let inv = theme::color::invert(&darkened);
                        match i {
                            0 => effective_theme.c_scale_inv0 = Some(inv),
                            1 => effective_theme.c_scale_inv1 = Some(inv),
                            2 => effective_theme.c_scale_inv2 = Some(inv),
                            3 => effective_theme.c_scale_inv3 = Some(inv),
                            4 => effective_theme.c_scale_inv4 = Some(inv),
                            5 => effective_theme.c_scale_inv5 = Some(inv),
                            6 => effective_theme.c_scale_inv6 = Some(inv),
                            7 => effective_theme.c_scale_inv7 = Some(inv),
                            8 => effective_theme.c_scale_inv8 = Some(inv),
                            9 => effective_theme.c_scale_inv9 = Some(inv),
                            10 => effective_theme.c_scale_inv10 = Some(inv),
                            11 => effective_theme.c_scale_inv11 = Some(inv),
                            _ => {}
                        }
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
        detect::DiagramKind::Venn => {
            let d = parser::venn::parse(source)?;
            let effective_theme = if let Some(name) = d.theme_name.as_deref() {
                theme::get_theme(name)
            } else {
                theme.clone()
            };
            let l = layout::venn::layout(&d, &effective_theme)?;
            render::svg_venn::render(&d, &l, &effective_theme, id)
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
        detect::DiagramKind::C4 => {
            // Parser is in place; layout/render are placeholders. The
            // 11 c4 fixtures are listed in tests/known_ignored.txt
            // until the upstream `c4Renderer.js` + `svgDraw.js` port
            // lands. Returning the parser's outcome (then a stub
            // `Unsupported` from the renderer) lets the rest of the
            // dispatch arm be exhaustive without crashing on c4 input.
            let d = parser::c4::parse(source)?;
            render::svg_c4::render(&d, &theme, id)
        }
        detect::DiagramKind::Mindmap => {
            // Parser is wired (whitespace-indented tree), but layout
            // and render are stubs: upstream uses `cose-bilkent`
            // force-directed layout (cytoscape extension), which has
            // no Rust port yet. Every mindmap fixture sits in
            // `tests/known_ignored.txt` until that port lands.
            let d = parser::mindmap::parse(source)?;
            let effective_theme = if let Some(name) = d.theme_override.as_deref() {
                theme::get_theme(name)
            } else {
                theme.clone()
            };
            let l = layout::mindmap::layout(&d, &effective_theme)?;
            render::svg_mindmap::render(&d, &l, &effective_theme, id)
        }
        detect::DiagramKind::GitGraph => {
            let d = parser::gitgraph::parse(source)?;
            let effective_theme = if let Some(name) = d.theme_name.as_deref() {
                theme::get_theme(name)
            } else {
                theme.clone()
            };
            let l = layout::gitgraph::layout(&d, &effective_theme)?;
            render::svg_gitgraph::render(&d, &l, &effective_theme, id)
        }
        detect::DiagramKind::Sequence => {
            // Scaffold-only: parser is tolerant, layout/render emit a
            // minimum-viable SVG. All sequence fixtures sit in
            // `tests/known_ignored.txt` until the upstream
            // `sequenceRenderer.ts` + `svgDraw.js` ports land.
            let d = parser::sequence::parse(source)?;
            let l = layout::sequence::layout(&d, &theme)?;
            render::svg_sequence::render(&d, &l, &theme, id)
        }
        detect::DiagramKind::Info => render::svg_info::render(&theme, id),
        other => Err(MermaidError::Unsupported(format!(
            "diagram kind '{}' not yet implemented — Wave 7: sequence",
            other.id()
        ))),
    }
}

/// Convenience wrapper using a default id.
pub fn convert(source: &str) -> Result<String, MermaidError> {
    convert_with_id(source, "mermaid-1")
}
