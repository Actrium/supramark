use std::fmt::Write;

use crate::layout::DiagramLayout;
use crate::model::{Diagram, DiagramMeta};
use crate::style::SkinParams;
use crate::Result;

use super::svg_richtext::set_default_font_family;
use super::svg_sequence;

// Re-export from svg_meta for backward compatibility and internal use
pub(crate) use super::svg_meta::inject_plantuml_source;
use super::svg_meta::{
    compute_meta_body_offset, extract_dimensions, inject_google_fonts, inject_svginteractive,
    wrap_with_meta,
};
#[cfg(test)]
use super::svg_meta::{encode_plantuml_source, extract_svg_content};

// Re-export from svg_class for tests
use super::svg_class::render_class;
#[cfg(test)]
use super::svg_class::{
    emit_arrowhead, emit_plus_head, format_member, move_edge_end_point, move_edge_start_point,
    qualifier_edge_translation, sanitize_id, KalPlacement, QualifierEndpoint,
};

// Test-only imports for items that moved to sub-modules
#[cfg(test)]
use crate::klimt::svg::SvgGraphic;
#[cfg(test)]
use crate::skin::rose::{BORDER_COLOR, ENTITY_BG, LEGEND_BG, LEGEND_BORDER, NOTE_BG};

// ── Style constants ──────────────────────────────────────────────────

/// SvekResult.java:135 -- minMax.getDimension().delta(15, 15).
#[allow(dead_code)] // Java-ported rendering constant
pub(crate) const CANVAS_DELTA: f64 = 15.0;
/// TextBlockExporter12026.java:196 -- margin from plantuml.skin root.document style: right=5.
pub(crate) const DOC_MARGIN_RIGHT: f64 = 5.0;
/// TextBlockExporter12026.java:197 -- margin from plantuml.skin root.document style: bottom=5.
pub(crate) const DOC_MARGIN_BOTTOM: f64 = 5.0;
pub(crate) const PLANTUML_VERSION: &str = "1.2026.2";

// ── Viewport computation ────────────────────────────────────────────

/// Per-diagram-type viewport margin configuration.
///
/// Java computes the final SVG viewport as:
///   `(int)(max_point + extra + margin_right + 1)`           (ensureVisible)
/// where `extra` is 1.0 for svek diagrams (getFinalDimension) and 0.0 otherwise.
///
/// This struct captures those margin parameters so every renderer uses the
/// same `compute_viewport()` helper instead of inline arithmetic.
pub(crate) struct ViewportConfig {
    /// Document right margin (Java root.document style margin-right).
    pub margin_right: f64,
    /// Document bottom margin (Java root.document style margin-bottom).
    pub margin_bottom: f64,
    /// Extra +1 from Java's `getFinalDimension = lf_maxX + 1 + margins`.
    /// Svek-backed diagrams (class, state) set this to 1.0; others use 0.0.
    pub extra: f64,
}

impl ViewportConfig {
    /// Svek-backed cuca diagrams (class, state): max_point + 1 + margin.
    pub const SVEK: Self = Self {
        margin_right: DOC_MARGIN_RIGHT,
        margin_bottom: DOC_MARGIN_BOTTOM,
        extra: 1.0,
    };

    /// Component diagrams: max_point + margin (no extra +1).
    pub const COMPONENT: Self = Self {
        margin_right: DOC_MARGIN_RIGHT,
        margin_bottom: DOC_MARGIN_BOTTOM,
        extra: 0.0,
    };

    /// Old-style activity diagrams: max + 5 (hardcoded margin, no extra).
    pub const ACTIVITY_OLD: Self = Self {
        margin_right: 5.0,
        margin_bottom: 5.0,
        extra: 0.0,
    };

    /// Sequence LimitFinder path: raw_w already includes the +1 from layout.
    pub const SEQUENCE_LF: Self = Self {
        margin_right: DOC_MARGIN_RIGHT,
        margin_bottom: DOC_MARGIN_BOTTOM,
        extra: 0.0,
    };
}

/// Compute viewport (w, h) from max-point and config.
///
/// Applies the Java `ensureVisible` formula:
///   `(int)(max_point + extra + margin + 1)`
/// and returns integer-valued f64 suitable for SVG width/height attributes.
pub(crate) fn compute_viewport(max_x: f64, max_y: f64, config: &ViewportConfig) -> (f64, f64) {
    let w = ensure_visible_int(max_x + config.extra + config.margin_right) as f64;
    let h = ensure_visible_int(max_y + config.extra + config.margin_bottom) as f64;
    (w, h)
}

pub(crate) use crate::klimt::svg::fmt_coord;

/// Write a Java PlantUML-compatible SVG root element and open a `<g>` wrapper.
#[allow(dead_code)] // convenience wrapper for write_svg_root_bg
pub(crate) fn write_svg_root(buf: &mut String, w: f64, h: f64, diagram_type: &str) {
    write_svg_root_bg(buf, w, h, diagram_type, "#FFFFFF");
}

pub(crate) fn write_svg_root_bg(buf: &mut String, w: f64, h: f64, diagram_type: &str, bg: &str) {
    write_svg_root_bg_opt(buf, w, h, Some(diagram_type), bg);
}

/// Write an SVG `<title>` element (the document-level title, not a visible element).
/// Java emits this via `SvgGraphics` when the diagram has a `title` directive.
/// Must be called right after `write_svg_root_bg*`, before `<defs/>`.
pub(crate) fn write_svg_title(buf: &mut String, title: &str) {
    use crate::klimt::svg::xml_escape;
    write!(buf, "<title>{}</title>", xml_escape(title)).unwrap();
}

/// Java `SvgGraphics.ensureVisible` truncation: `maxX = (int)(x + 1)`.
/// Converts a floating-point dimension to the integer viewport value used by
/// Java PlantUML.  Callers must pass the RAW dimension BEFORE the +1 truncation.
/// The minimum is 10, matching Java's `SvgGraphics.maxX/maxY` initial value.
pub(crate) fn ensure_visible_int(x: f64) -> i32 {
    if x.is_finite() && x > 0.0 {
        ((x + 1.0) as i32).max(10)
    } else {
        10 // Java default
    }
}

/// Write SVG root element. `diagram_type` is optional — Java's PSystemSalt and
/// PSystemDot don't go through TitledDiagram, so they omit `data-diagram-type`.
///
/// `w` and `h` should already be integer-valued viewport dimensions (having
/// gone through `ensure_visible_int` or equivalent truncation).
/// The function rounds via `as i32` for safety but does NOT add +1.
pub(crate) fn write_svg_root_bg_opt(
    buf: &mut String,
    w: f64,
    h: f64,
    diagram_type: Option<&str>,
    bg: &str,
) {
    let wi = if w.is_finite() && w > 0.0 {
        w as i32
    } else {
        100
    };
    let hi = if h.is_finite() && h > 0.0 {
        h as i32
    } else {
        100
    };
    write!(buf, "<?plantuml {PLANTUML_VERSION}?>").unwrap();
    buf.push_str(r#"<svg xmlns="http://www.w3.org/2000/svg""#);
    buf.push_str(r#" xmlns:xlink="http://www.w3.org/1999/xlink""#);
    buf.push_str(r#" contentStyleType="text/css""#);
    if let Some(dtype) = diagram_type {
        write!(buf, r#" data-diagram-type="{dtype}""#).unwrap();
    }
    write!(
        buf,
        concat!(
            r#" height="{hi}px""#,
            r#" preserveAspectRatio="none""#,
            r#" style="width:{wi}px;height:{hi}px;background:{bg};""#,
            r#" version="1.1""#,
            r#" viewBox="0 0 {wi} {hi}""#,
            r#" width="{wi}px""#,
            r#" zoomAndPan="magnify">"#,
        ),
        hi = hi,
        wi = wi,
        bg = bg,
    )
    .unwrap();
}

#[cfg(test)]
use crate::klimt::svg::xml_escape;

/// Write a background `<rect>` covering the entire canvas when the background
/// color differs from the default #FFFFFF. Java PlantUML emits this rect as the
/// first child of `<g>` when `skinparam backgroundColor` is set.
pub(crate) fn write_bg_rect(buf: &mut String, w: f64, h: f64, bg: &str) {
    if !bg.eq_ignore_ascii_case("#FFFFFF") {
        let wi = if w.is_finite() && w > 0.0 {
            w as i32
        } else {
            100
        };
        let hi = if h.is_finite() && h > 0.0 {
            h as i32
        } else {
            100
        };
        write!(
            buf,
            r#"<rect fill="{bg}" height="{hi}" style="stroke:none;stroke-width:1;" width="{wi}" x="0" y="0"/>"#,
        )
        .unwrap();
    }
}

// ── Public entry point ───────────────────────────────────────────────

/// Render a Diagram + DiagramLayout into an SVG string.
pub fn render(
    diagram: &Diagram,
    layout: &DiagramLayout,
    skin: &SkinParams,
    meta: &DiagramMeta,
) -> Result<String> {
    render_with_source(diagram, layout, skin, meta, None)
}

pub fn render_with_source(
    diagram: &Diagram,
    layout: &DiagramLayout,
    skin: &SkinParams,
    meta: &DiagramMeta,
    source: Option<&str>,
) -> Result<String> {
    struct SvgSeedGuard;
    impl Drop for SvgSeedGuard {
        fn drop(&mut self) {
            crate::klimt::svg::set_svg_id_seed_override(None);
        }
    }
    crate::klimt::svg::set_svg_id_seed_override(source.map(crate::klimt::svg::java_source_seed));
    let _svg_seed_guard = SvgSeedGuard;

    // For activity diagrams with meta elements (title/header/footer),
    // pre-compute the body offset so the body renderer can emit absolute
    // coordinates directly, avoiding lossy string-level coordinate shifting.
    let activity_body_offset = if matches!(diagram, Diagram::Activity(_)) && !meta.is_empty() {
        Some(compute_meta_body_offset(meta, skin))
    } else {
        None
    };

    // Nwdiag bakes its body coordinates into absolute positions ahead of
    // wrap_with_meta so the body offset can stay in f64 (avoiding the
    // format→parse→add→format double-round that drifts ±0.0001 from Java
    // when shifting y values through the SVG text).
    let nwdiag_body_offset = if matches!(diagram, Diagram::Nwdiag(_)) && !meta.is_empty() {
        Some(compute_meta_body_offset(meta, skin))
    } else {
        None
    };

    // Class diagrams use the same offset_svg_coords post-shift path; for
    // a degenerated svek result (single entity, no edges/notes) we know the
    // offset upfront and can pre-shift the body, avoiding the
    // format→parse→add→format double-round that otherwise drifts y values
    // by ±0.0001 from Java.
    let class_body_offset = if matches!(diagram, Diagram::Class(_)) && !meta.is_empty() {
        let is_degen = matches!(layout, DiagramLayout::Class(gl)
            if gl.nodes.len() <= 1 && gl.edges.is_empty() && gl.notes.is_empty());
        if is_degen {
            Some(compute_meta_body_offset(meta, skin))
        } else {
            None
        }
    } else {
        None
    };

    // Note: handwritten mode does NOT change fonts, only jiggling shapes.
    set_default_font_family(None);
    let body_result = render_body(
        diagram,
        layout,
        skin,
        activity_body_offset,
        class_body_offset,
        nwdiag_body_offset,
    )?;
    set_default_font_family(None);

    // Extract diagram type from body SVG
    let dtype = body_result
        .svg
        .find("data-diagram-type=\"")
        .and_then(|pos| {
            let start = pos + 19;
            body_result.svg[start..]
                .find('"')
                .map(|end| body_result.svg[start..start + end].to_string())
        })
        .unwrap_or_else(|| "CLASS".to_string());

    // EBNF and Regex diagrams handle their own title rendering in the body.
    // Clear meta.title so wrap_with_meta doesn't add a duplicate visible title.
    let meta_for_wrap;
    let effective_meta = if matches!(dtype.as_str(), "EBNF" | "REGEX") && meta.title.is_some() {
        meta_for_wrap = DiagramMeta {
            title: None,
            title_line: None,
            ..meta.clone()
        };
        &meta_for_wrap
    } else {
        meta
    };
    // Java applies the document-level chrome (header/footer/title/caption/legend)
    // via TextBlockExporter, which only fires when there's actual meta content to
    // render.  The `svginteractive` pragma is unrelated chrome — Java handles it
    // as an `SvgOption` flag that injects CSS/JS into `<defs>` on the existing
    // SvgGraphics root.  Wrapping the body just to inject CSS/JS would force a
    // re-derivation of the viewport from `raw_body_dim`, which truncates the
    // fractional half-pixel that the body's `ensureVisible` had already rounded
    // up — losing 1px on width and height.  Mirror Java by only wrapping when
    // the meta is non-empty; the standalone `inject_svginteractive` step below
    // takes care of the pragma without touching dimensions.
    let mut svg = if effective_meta.is_empty() {
        body_result.svg
    } else {
        // Document-level BackGroundColor from <style> is stored as "document.backgroundcolor";
        // skinparam BackGroundColor is stored as "backgroundcolor". Try both.
        let bg = skin
            .get("document.backgroundcolor")
            .or_else(|| skin.get("backgroundcolor"))
            .unwrap_or("#FFFFFF");
        wrap_with_meta(
            &body_result.svg,
            effective_meta,
            &dtype,
            bg,
            body_result.raw_body_dim,
            body_result.body_pre_offset,
            body_result.body_degenerated,
            skin,
        )?
    };

    // Inject svginteractive CSS/JS if pragma is set
    if meta
        .pragmas
        .get("svginteractive")
        .is_some_and(|v| v == "true")
    {
        svg = inject_svginteractive(svg, &dtype);
    }

    // Inject Google Fonts @import for non-built-in font families used in the SVG.
    // Java PlantUML emits an @import URL into <defs> when text elements reference
    // fonts that need to be loaded from Google Fonts (e.g. Roboto).
    svg = inject_google_fonts(svg);

    // Java PlantUML suppresses DOT rendering with a simple notice SVG
    // that does not include the plantuml-src processing instruction.
    let is_dot = matches!(diagram, Diagram::Dot(_));
    if !is_dot {
        if let Some(source) = source {
            svg = inject_plantuml_source(svg, source)?;
        }
    }

    Ok(svg)
}

/// Body rendering result: (svg_string, raw_body_content_dimensions).
/// The raw dimensions are the precise body content size (Java SvekResult.calculateDimension)
/// before DOC_MARGIN and ensureVisible integer truncation. When present, wrap_with_meta
/// uses these instead of extracting lossy integer dimensions from the SVG header.
pub(super) struct BodyResult {
    pub(super) svg: String,
    pub(super) raw_body_dim: Option<(f64, f64)>,
    /// When true, body coordinates already include the meta offset (body_abs_x/y).
    /// wrap_with_meta should NOT apply offset_svg_coords.
    pub(super) body_pre_offset: bool,
    /// When true, the body has at most one entity and no edges/notes.  In that
    /// "degenerated" svek case, raw_body_dim is computed from `node.width + 14`
    /// (DEGENERATED_DELTA*2) directly rather than from a moveDelta-shifted
    /// LimitFinder span, so wrap_with_meta must not subtract the +6 minX
    /// offset that the normal svek code path needs.
    pub(super) body_degenerated: bool,
}

fn render_body(
    diagram: &Diagram,
    layout: &DiagramLayout,
    skin: &SkinParams,
    activity_body_offset: Option<(f64, f64)>,
    class_body_offset: Option<(f64, f64)>,
    nwdiag_body_offset: Option<(f64, f64)>,
) -> Result<BodyResult> {
    match (diagram, layout) {
        (Diagram::Bpm(bd), DiagramLayout::Bpm(bl)) => {
            super::svg_bpm::render_bpm(bd, bl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Class(cd), DiagramLayout::Class(gl)) => {
            render_class(cd, gl, skin, class_body_offset)
        }
        (Diagram::Sequence(sd), DiagramLayout::Sequence(sl)) => {
            // Sequence layout total_width/total_height include document margins
            // (top=5, right=5, bottom=5 for Puma2). Recover raw textBlock dimensions.
            let margin_top = 5.0;
            let margin_right = DOC_MARGIN_RIGHT;
            let margin_bottom = DOC_MARGIN_BOTTOM;
            // For diagrams with right-boundary arrows (`A ->]`), the layout's
            // `total_width` does not include the trailing arrow-head polygon
            // (Java's LimitFinder HACK_X_FOR_POLYGON = 10). `render_sequence`
            // runs a LimitFinder-style bounds pass and bakes the correct width
            // into the inner SVG header — we pick that up here. Guard on the
            // presence of `]` (right-border) or `[` (left-border) endpoints so
            // normal diagrams keep using `sl.total_width` and avoid the
            // off-by-one ceiling slop that measure_sequence_body_dim can
            // introduce for tightly packed layouts.
            let has_boundary_arrow = sd.events.iter().any(|e| {
                matches!(e,
                    crate::model::sequence::SeqEvent::Message(m)
                        if m.to == "]" || m.from == "[")
            });
            svg_sequence::render_sequence(sd, sl, skin).map(|svg| {
                let (body_w, body_h) = if has_boundary_arrow {
                    let (rendered_w, rendered_h) = extract_dimensions(&svg);
                    (
                        (rendered_w - margin_right).max(sl.total_width - margin_right),
                        (rendered_h - margin_top - margin_bottom)
                            .max(sl.total_height - margin_top - margin_bottom),
                    )
                } else {
                    (
                        sl.total_width - margin_right,
                        sl.total_height - margin_top - margin_bottom,
                    )
                };
                BodyResult {
                    svg,
                    raw_body_dim: Some((body_w, body_h)),
                    body_pre_offset: false,
                    body_degenerated: false,
                }
            })
        }
        (Diagram::Activity(ad), DiagramLayout::Activity(al)) => {
            super::svg_activity::render_activity(ad, al, skin, activity_body_offset).map(
                |(svg, raw_body_dim)| BodyResult {
                    svg,
                    raw_body_dim,
                    body_pre_offset: activity_body_offset.is_some(),
                    body_degenerated: false,
                },
            )
        }
        (Diagram::State(sd), DiagramLayout::State(sl)) => {
            super::svg_state::render_state(sd, sl, skin).map(|(svg, raw_body_dim)| BodyResult {
                svg,
                raw_body_dim,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Component(cd), DiagramLayout::Component(cl)) => {
            super::svg_component::render_component(cd, cl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Chart(cd), DiagramLayout::Chart(cl)) => {
            super::svg_chart::render_chart(cd, cl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Files(fd), DiagramLayout::Files(fl)) => {
            super::svg_files::render_files(fd, fl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Flow(fd), DiagramLayout::Flow(fl)) => super::svg_flow::render_flow(fd, fl, skin)
            .map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            }),
        (Diagram::Ditaa(dd), DiagramLayout::Ditaa(dl)) => {
            super::svg_ditaa::render_ditaa(dd, dl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Erd(ed), DiagramLayout::Erd(el)) => {
            super::svg_erd::render_erd(ed, el, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Gantt(gd), DiagramLayout::Gantt(gl)) => {
            super::svg_gantt::render_gantt(gd, gl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Json(jd), DiagramLayout::Json(jl)) => super::svg_json::render_json(jd, jl, skin)
            .map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            }),
        (Diagram::Mindmap(md), DiagramLayout::Mindmap(ml)) => {
            let raw_body_dim = ml.raw_body_dim;
            super::svg_mindmap::render_mindmap(md, ml, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Nwdiag(nd), DiagramLayout::Nwdiag(nl)) => {
            // Surface the un-padded body extent so wrap_with_meta can centre
            // the title against Java's LimitFinder span (no integer rounding
            // through the SVG header). When meta is present the body is
            // pre-shifted in f64 to avoid format/parse round-off noise.
            let raw_dim = Some((nl.width, nl.height));
            super::svg_nwdiag::render_nwdiag(nd, nl, skin, nwdiag_body_offset).map(|svg| {
                BodyResult {
                    svg,
                    raw_body_dim: raw_dim,
                    body_pre_offset: nwdiag_body_offset.is_some(),
                    body_degenerated: false,
                }
            })
        }
        (Diagram::Salt(sd), DiagramLayout::Salt(sl)) => super::svg_salt::render_salt(sd, sl, skin)
            .map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            }),
        (Diagram::Timing(td), DiagramLayout::Timing(tl)) => {
            super::svg_timing::render_timing(td, tl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Wbs(wd), DiagramLayout::Wbs(wl)) => {
            super::svg_wbs::render_wbs(wd, wl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Yaml(yd), DiagramLayout::Yaml(yl)) => super::svg_json::render_yaml(yd, yl, skin)
            .map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            }),
        (Diagram::UseCase(ud), DiagramLayout::Component(cl)) => {
            // Use case diagrams are routed through the component rendering pipeline.
            let cd = crate::model::component::ComponentDiagram::from(ud);
            super::svg_component::render_component(&cd, cl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Dot(_dd), DiagramLayout::Dot(_gl)) => {
            // Java PlantUML suppresses DOT rendering
            Ok(BodyResult {
                svg: render_dot_suppressed(),
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Packet(pd), DiagramLayout::Packet(pl)) => {
            super::svg_packet::render_packet(pd, pl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Git(gd), DiagramLayout::Git(gl)) => {
            super::svg_git::render_git(gd, gl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Regex(_rd), DiagramLayout::Regex(rl)) => {
            super::svg_regex::render_regex_ebnf(rl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Ebnf(ed), DiagramLayout::Ebnf(el)) => super::svg_ebnf::render_ebnf(ed, el, skin)
            .map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            }),
        (Diagram::Pie(pd), DiagramLayout::Pie(pl)) => {
            super::svg_pie::render_pie(pd, pl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Board(bd), DiagramLayout::Board(bl)) => {
            super::svg_board::render_board(bd, bl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Chronology(cd), DiagramLayout::Chronology(cl)) => {
            super::svg_chronology::render_chronology(cd, cl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Hcl(hd), DiagramLayout::Hcl(hl)) => {
            super::svg_hcl::render_hcl(hd, hl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Wire(wd), DiagramLayout::Wire(wl)) => super::svg_wire::render_wire(wd, wl, skin)
            .map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            }),
        (Diagram::Math(md), DiagramLayout::Math(ml)) => super::svg_math::render_math(md, ml, skin)
            .map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            }),
        (Diagram::Latex(ld), DiagramLayout::Latex(ll)) => {
            super::svg_math::render_math(ld, ll, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Creole(cd), DiagramLayout::Creole(cl)) => {
            super::svg_creole::render_creole(cd, cl, skin).map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            })
        }
        (Diagram::Def(dd), DiagramLayout::Def(dl)) => super::svg_math::render_def(dd, dl, skin)
            .map(|svg| BodyResult {
                svg,
                raw_body_dim: None,
                body_pre_offset: false,
                body_degenerated: false,
            }),
        _ => Err(crate::Error::Render("diagram/layout type mismatch".into())),
    }
}

/// Render a suppressed-feature notice for DOT diagrams, matching Java PlantUML.
fn render_dot_suppressed() -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">\n",
    );
    s.push_str("<a xlink:href=\"https://github.com/plantuml/plantuml/issues/2495\">\n");
    s.push_str("<text x=\"10\" y=\"30\" font-family=\"sans-serif\" font-size=\"14\" fill=\"blue\" text-decoration=\"underline\">This feature has been suppressed</text>\n");
    s.push_str("</a>\n");
    s.push_str("</svg>");
    s
}

/// Inline bounding-box tracker mirroring Java's LimitFinder.
/// Intercepts every draw call during rendering to compute the exact canvas size.
/// Tracks drawing bounds, mirroring Java `LimitFinder`.
///
/// Java uses a two-pass model:
///   Pass 1: LimitFinder tracks min/max of all draw operations
///   Pass 2: SvgGraphics ensureVisible uses (int)(x+1)
///
/// We use LimitFinder semantics (min/max tracking). The final SVG dimensions
/// are computed as: `(int)(span + CANVAS_DELTA + DOC_MARGIN + 1)` which is
/// equivalent to Java's `ensureVisible(span + delta + margin)`.
pub(crate) struct BoundsTracker {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl BoundsTracker {
    pub fn new() -> Self {
        Self {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }

    fn add_point(&mut self, x: f64, y: f64) {
        log::trace!("BoundsTracker.addPoint({:.4}, {:.4})", x, y);
        if x < self.min_x {
            self.min_x = x;
        }
        if y < self.min_y {
            self.min_y = y;
        }
        if x > self.max_x {
            self.max_x = x;
        }
        if y > self.max_y {
            self.max_y = y;
        }
    }

    /// Java LimitFinder.drawRectangle: (x-1, y-1) to (x+w-1+shadow*2, y+h-1+shadow*2)
    pub fn track_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.track_rect_shadow(x, y, w, h, 0.0);
    }

    /// Java LimitFinder.drawRectangle with delta shadow
    pub fn track_rect_shadow(&mut self, x: f64, y: f64, w: f64, h: f64, shadow: f64) {
        log::trace!(
            "BoundsTracker.drawRect x={:.2} y={:.2} w={:.2} h={:.2} shadow={:.2}",
            x,
            y,
            w,
            h,
            shadow
        );
        self.add_point(x - 1.0, y - 1.0);
        self.add_point(x + w - 1.0 + shadow * 2.0, y + h - 1.0 + shadow * 2.0);
    }

    /// Java LimitFinder.drawEmpty: (x, y) to (x+w, y+h) — NO -1 adjustment
    pub fn track_empty(&mut self, x: f64, y: f64, w: f64, h: f64) {
        log::trace!(
            "BoundsTracker.drawEmpty x={:.2} y={:.2} w={:.2} h={:.2}",
            x,
            y,
            w,
            h
        );
        self.add_point(x, y);
        self.add_point(x + w, y + h);
    }

    /// Java LimitFinder.drawEllipse: (x, y) to (x+w-1+shadow*2, y+h-1+shadow*2)
    /// where x,y is top-left of bounding box
    pub fn track_ellipse(&mut self, cx: f64, cy: f64, rx: f64, ry: f64) {
        self.track_ellipse_shadow(cx, cy, rx, ry, 0.0);
    }

    /// Java LimitFinder.drawEllipse with delta shadow
    pub fn track_ellipse_shadow(&mut self, cx: f64, cy: f64, rx: f64, ry: f64, shadow: f64) {
        // Java draws UEllipse at translate position (x, y) with width=2*rx, height=2*ry
        // LimitFinder.drawEllipse(x, y, ellipse): addPoint(x, y), addPoint(x+w-1+s*2, y+h-1+s*2)
        let x = cx - rx;
        let y = cy - ry;
        let w = 2.0 * rx;
        let h = 2.0 * ry;
        log::trace!(
            "BoundsTracker.drawEllipse x={:.2} y={:.2} w={:.2} h={:.2} shadow={:.2}",
            x,
            y,
            w,
            h,
            shadow
        );
        self.add_point(x, y);
        self.add_point(x + w - 1.0 + shadow * 2.0, y + h - 1.0 + shadow * 2.0);
    }

    /// Java LimitFinder.drawUPolygon: HACK_X_FOR_POLYGON = 10
    pub fn track_polygon(&mut self, points: &[(f64, f64)]) {
        if points.is_empty() {
            return;
        }
        let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        log::trace!(
            "BoundsTracker.drawPolygon minX={:.2} maxX={:.2} minY={:.2} maxY={:.2}",
            min_x,
            max_x,
            min_y,
            max_y
        );
        self.add_point(min_x - 10.0, min_y);
        self.add_point(max_x + 10.0, max_y);
    }

    /// Java LimitFinder.drawULine
    pub fn track_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        log::trace!(
            "BoundsTracker.drawLine ({:.2},{:.2})-({:.2},{:.2})",
            x1,
            y1,
            x2,
            y2
        );
        self.add_point(x1, y1);
        self.add_point(x2, y2);
    }

    /// Java LimitFinder.drawDotPath — path bounding box
    pub fn track_path_bounds(&mut self, min_x: f64, min_y: f64, max_x: f64, max_y: f64) {
        log::trace!(
            "BoundsTracker.drawDotPath min=({:.2},{:.2}) max=({:.2},{:.2})",
            min_x,
            min_y,
            max_x,
            max_y
        );
        self.add_point(min_x, min_y);
        self.add_point(max_x, max_y);
    }

    /// Java LimitFinder.drawText:
    ///   y_adj = y - h + 1.5
    ///   addPoint(x, y_adj), addPoint(x, y_adj+h), addPoint(x+w, y_adj), addPoint(x+w, y_adj+h)
    ///   i.e. (x, y-h+1.5) to (x+w, y+1.5)
    pub fn track_text(&mut self, x: f64, y: f64, text_width: f64, text_height: f64) {
        let y_adj = y - text_height + 1.5;
        log::trace!(
            "BoundsTracker.drawText x={:.4} y={:.4} w={:.4} h={:.4} y_adj={:.4}",
            x,
            y,
            text_width,
            text_height,
            y_adj
        );
        self.add_point(x, y_adj);
        self.add_point(x, y_adj + text_height);
        self.add_point(x + text_width, y_adj);
        self.add_point(x + text_width, y_adj + text_height);
    }

    /// Span: max - min in each dimension. Used with CANVAS_DELTA + DOC_MARGIN
    /// to compute final SVG dimensions matching Java's ensureVisible.
    #[allow(dead_code)] // Java-ported BoundsTracker method
    pub fn span(&self) -> (f64, f64) {
        if self.max_x.is_finite() && self.min_x.is_finite() {
            (self.max_x - self.min_x, self.max_y - self.min_y)
        } else {
            (0.0, 0.0)
        }
    }

    #[allow(dead_code)] // Java-ported BoundsTracker method
    pub fn min_point(&self) -> (f64, f64) {
        if self.max_x.is_finite() && self.min_x.is_finite() {
            (self.min_x, self.min_y)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn max_point(&self) -> (f64, f64) {
        if self.max_x.is_finite() && self.min_x.is_finite() {
            (self.max_x, self.max_y)
        } else {
            (0.0, 0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::layout::graphviz::{EdgeLayout, GraphLayout, NodeLayout};
    use crate::layout::DiagramLayout;
    use crate::model::{
        ArrowHead, ClassDiagram, Diagram, Direction, Entity, EntityKind, LineStyle, Link, Member,
        MemberModifiers, Visibility,
    };

    fn empty_class_diagram() -> ClassDiagram {
        ClassDiagram {
            entities: vec![],
            links: vec![],
            groups: vec![],
            direction: Direction::TopToBottom,
            direction_explicit: false,
            notes: vec![],
            hide_show_rules: vec![],
            stereotype_backgrounds: HashMap::new(),
        }
    }

    #[test]
    fn test_fmt_coord_matches_java() {
        // Matches Java SvgGraphics.format() behavior exactly
        assert_eq!(fmt_coord(0.0), "0");
        assert_eq!(fmt_coord(1.0), "1");
        assert_eq!(fmt_coord(42.0), "42");
        assert_eq!(fmt_coord(3.5), "3.5");
        assert_eq!(fmt_coord(3.50), "3.5");
        assert_eq!(fmt_coord(3.1234), "3.1234");
        assert_eq!(fmt_coord(3.12340), "3.1234");
        assert_eq!(fmt_coord(3.1200), "3.12");
        assert_eq!(fmt_coord(3.1000), "3.1");
        assert_eq!(fmt_coord(100.0), "100");
        assert_eq!(fmt_coord(-5.25), "-5.25");
        assert_eq!(fmt_coord(0.0001), "0.0001");
        assert_eq!(fmt_coord(0.00001), "0"); // rounds to 0.0000
    }

    fn assert_point_eq(actual: (f64, f64), expected: (f64, f64)) {
        assert!(
            (actual.0 - expected.0).abs() < 0.0001,
            "x mismatch: actual={} expected={}",
            actual.0,
            expected.0
        );
        assert!(
            (actual.1 - expected.1).abs() < 0.0001,
            "y mismatch: actual={} expected={}",
            actual.1,
            expected.1
        );
    }

    #[test]
    fn move_edge_start_point_moves_first_control_point_like_java_dotpath() {
        let mut points = vec![
            (201.0, 61.11),
            (201.0, 89.21),
            (201.0, 112.39),
            (201.0, 140.62),
        ];
        move_edge_start_point(&mut points, 0.0, 18.2969);

        assert_point_eq(points[0], (201.0, 79.4069));
        assert_point_eq(points[1], (201.0, 107.5069));
        assert_point_eq(points[2], (201.0, 112.39));
        assert_point_eq(points[3], (201.0, 140.62));
    }

    #[test]
    fn move_edge_end_point_moves_last_control_point_like_java_dotpath() {
        let mut points = vec![
            (201.0, 79.4069),
            (201.0, 107.5069),
            (201.0, 112.39),
            (201.0, 140.62),
        ];
        move_edge_end_point(&mut points, 0.0, -18.2969);

        assert_point_eq(points[0], (201.0, 79.4069));
        assert_point_eq(points[1], (201.0, 107.5069));
        assert_point_eq(points[2], (201.0, 94.0931));
        assert_point_eq(points[3], (201.0, 122.3231));
    }

    #[test]
    fn emit_diamond_hollow_arrowhead_uses_none_fill_like_java() {
        let mut sg = SvgGraphic::new(0, 1.0);
        let mut tracker = BoundsTracker::new();
        emit_arrowhead(
            &mut sg,
            &mut tracker,
            &ArrowHead::DiamondHollow,
            &[
                (201.0, 237.4069),
                (201.0, 265.5069),
                (201.0, 258.0931),
                (201.0, 286.3231),
            ],
            true,
            "#181818",
            0.0,
            0.0,
        );
        assert!(
            sg.body().contains(r#"<polygon fill="none""#),
            "expected hollow diamond fill to match Java aggregation output"
        );
    }

    #[test]
    fn emit_plus_head_horizontal_matches_java_geometry() {
        let mut sg = SvgGraphic::new(0, 1.0);
        let mut tracker = BoundsTracker::new();
        emit_plus_head(&mut sg, &mut tracker, 118.9061, 183.0, 0.0, "#181818");
        let body = sg.body();
        assert!(body.contains(r##"<ellipse cx="110.9061" cy="183" fill="#FFFFFF" rx="8" ry="8""##));
        assert!(body.contains(r#"x1="102.9061" x2="118.9061" y1="183" y2="183""#));
        assert!(body.contains(r#"x1="110.9061" x2="110.9061" y1="175" y2="191""#));
    }

    fn make_link() -> Link {
        Link {
            uid: None,
            from: "Foo".into(),
            to: "Bar".into(),
            left_head: ArrowHead::None,
            right_head: ArrowHead::None,
            line_style: LineStyle::Solid,
            label: None,
            from_label: None,
            to_label: None,
            from_qualifier: None,
            to_qualifier: None,
            source_line: None,
            arrow_len: 2,
        }
    }

    #[test]
    fn qualifier_without_start_decoration_does_not_push_start_point_down() {
        let mut link = make_link();
        link.from_qualifier = Some("x: String".into());
        let placement = KalPlacement {
            x: 436.7054,
            y: 55.11,
            width: 63.2334,
            height: 18.2969,
            shift_x: 0.0,
        };

        assert_eq!(
            qualifier_edge_translation(&link, QualifierEndpoint::Tail, &placement),
            None
        );
    }

    #[test]
    fn qualifier_with_start_decoration_matches_java_downward_translation() {
        let mut link = make_link();
        link.from_qualifier = Some("c3".into());
        link.left_head = ArrowHead::DiamondHollow;
        let placement = KalPlacement {
            x: 175.0,
            y: 55.11,
            width: 21.7939,
            height: 18.2969,
            shift_x: 0.0,
        };

        assert_eq!(
            qualifier_edge_translation(&link, QualifierEndpoint::Tail, &placement),
            Some((0.0, 18.2969))
        );
    }

    #[test]
    fn downward_qualifier_overlap_only_moves_start_point_horizontally_without_decoration() {
        let mut link = make_link();
        link.from_qualifier = Some("x".into());
        let placement = KalPlacement {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 18.2969,
            shift_x: 7.5,
        };

        assert_eq!(
            qualifier_edge_translation(&link, QualifierEndpoint::Tail, &placement),
            Some((7.5, 0.0))
        );
    }

    fn simple_diagram() -> (Diagram, DiagramLayout) {
        let entity = Entity {
            name: "Foo".into(),
            members: vec![
                Member {
                    visibility: Some(Visibility::Public),
                    name: "bar".into(),
                    return_type: Some("String".into()),
                    is_method: false,
                    modifiers: MemberModifiers::default(),
                    display: None,
                },
                Member {
                    visibility: Some(Visibility::Private),
                    name: "baz".into(),
                    return_type: None,
                    is_method: true,
                    modifiers: MemberModifiers {
                        is_static: true,
                        is_abstract: false,
                    },
                    display: None,
                },
            ],
            ..Entity::default()
        };
        let entity2 = Entity {
            name: "Bar".into(),
            kind: EntityKind::Interface,
            ..Entity::default()
        };
        let link = Link {
            uid: None,
            from: "Foo".into(),
            to: "Bar".into(),
            left_head: ArrowHead::None,
            right_head: ArrowHead::Triangle,
            line_style: LineStyle::Dashed,
            label: Some("implements".into()),
            from_label: None,
            to_label: None,
            from_qualifier: None,
            to_qualifier: None,
            source_line: None,
            arrow_len: 2,
        };
        let mut cd = empty_class_diagram();
        cd.entities = vec![entity, entity2];
        cd.links = vec![link];
        let gl = GraphLayout {
            nodes: vec![
                NodeLayout {
                    id: "Foo".into(),
                    cx: 100.0,
                    cy: 50.0,
                    width: 120.0,
                    height: 80.0,
                    image_width: 120.0,
                    min_x: 40.0,
                    min_y: 10.0,
                },
                NodeLayout {
                    id: "Bar".into(),
                    cx: 100.0,
                    cy: 180.0,
                    width: 120.0,
                    height: 40.0,
                    image_width: 120.0,
                    min_x: 40.0,
                    min_y: 160.0,
                },
            ],
            edges: vec![EdgeLayout {
                from: "Foo".into(),
                to: "Bar".into(),
                points: vec![(100.0, 90.0), (100.0, 160.0)],
                arrow_tip: None,
                spline_start: None,
                spline_end: None,
                raw_path_d: None,
                arrow_polygon_points: None,
                label: None,
                tail_label: None,
                tail_label_xy: None,
                tail_label_wh: None,
                tail_label_boxed: false,
                head_label: None,
                head_label_xy: None,
                head_label_wh: None,
                head_label_boxed: false,
                label_xy: None,
                label_wh: None,
            }],
            clusters: vec![],
            notes: vec![],
            total_width: 240.0,
            total_height: 220.0,
            move_delta: (7.0, 7.0),
            normalize_offset: (0.0, 0.0),
            lf_span: (240.0, 220.0),
            lf_max: (240.0, 220.0),
            render_offset: (7.0, 7.0),
        };
        (Diagram::Class(cd), DiagramLayout::Class(gl))
    }

    fn default_skin() -> SkinParams {
        SkinParams::default()
    }
    fn default_meta() -> DiagramMeta {
        DiagramMeta::default()
    }

    #[test]
    fn test_basic_render_produces_valid_svg() {
        let (d, l) = simple_diagram();
        let svg = render(&d, &l, &default_skin(), &default_meta()).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
    }

    #[test]
    fn test_entity_box_contains_name() {
        let (d, l) = simple_diagram();
        let svg = render(&d, &l, &default_skin(), &default_meta()).unwrap();
        assert!(svg.contains("Foo"));
        assert!(svg.contains("Bar"));
        assert!(svg.contains("font-style=\"italic\""));
    }

    #[test]
    fn test_edge_rendering_produces_path() {
        let (d, l) = simple_diagram();
        let svg = render(&d, &l, &default_skin(), &default_meta()).unwrap();
        assert!(svg.contains("<path"));
        assert!(svg.contains("stroke-dasharray"));
        assert!(
            svg.contains("<polygon"),
            "arrow should render as inline polygon"
        );
    }

    #[test]
    fn test_xml_escaping() {
        assert_eq!(xml_escape("A & B"), "A &amp; B");
        assert_eq!(xml_escape("<T>"), "&lt;T&gt;");
        assert_eq!(xml_escape(r#"a"b"#), r#"a"b"#);
        assert_eq!(xml_escape("plain"), "plain");
    }

    #[test]
    fn test_member_formatting() {
        let m = Member {
            visibility: Some(Visibility::Protected),
            name: "calc()".into(),
            return_type: Some("int".into()),
            is_method: true,
            modifiers: MemberModifiers::default(),
            display: None,
        };
        assert_eq!(format_member(&m), "# calc(): int");
    }

    #[test]
    fn test_entity_with_special_chars() {
        let entity = Entity {
            name: "Map<K, V>".into(),
            ..Entity::default()
        };
        let mut cd = empty_class_diagram();
        cd.entities = vec![entity];
        let gl = GraphLayout {
            nodes: vec![NodeLayout {
                id: sanitize_id("Map<K, V>"),
                cx: 80.0,
                cy: 40.0,
                width: 100.0,
                height: 40.0,
                image_width: 100.0,
                min_x: 30.0,
                min_y: 20.0,
            }],
            edges: vec![],
            clusters: vec![],
            notes: vec![],
            total_width: 200.0,
            total_height: 100.0,
            move_delta: (7.0, 7.0),
            normalize_offset: (0.0, 0.0),
            lf_span: (200.0, 100.0),
            lf_max: (200.0, 100.0),
            render_offset: (7.0, 7.0),
        };
        let svg = render(
            &Diagram::Class(cd),
            &DiagramLayout::Class(gl),
            &default_skin(),
            &default_meta(),
        )
        .unwrap();
        assert!(svg.contains("Map&lt;K, V&gt;"));
    }

    #[test]
    fn test_object_entity_renders_without_circle_icon() {
        let entity = Entity {
            name: "myObj".into(),
            kind: EntityKind::Object,
            ..Entity::default()
        };
        let mut cd = empty_class_diagram();
        cd.entities = vec![entity];
        let gl = GraphLayout {
            nodes: vec![NodeLayout {
                id: "myObj".into(),
                cx: 80.0,
                cy: 40.0,
                width: 100.0,
                height: 40.0,
                image_width: 100.0,
                min_x: 30.0,
                min_y: 20.0,
            }],
            edges: vec![],
            clusters: vec![],
            notes: vec![],
            total_width: 200.0,
            total_height: 100.0,
            move_delta: (7.0, 7.0),
            normalize_offset: (0.0, 0.0),
            lf_span: (200.0, 100.0),
            lf_max: (200.0, 100.0),
            render_offset: (7.0, 7.0),
        };
        let svg = render(
            &Diagram::Class(cd),
            &DiagramLayout::Class(gl),
            &default_skin(),
            &default_meta(),
        )
        .expect("render failed");
        assert!(svg.contains("myObj"), "SVG must contain object name");
        // EntityImageObject: no underline by default (only in strict UML mode)
        assert!(
            !svg.contains(r#"text-decoration="underline""#),
            "object name must NOT have underline text-decoration by default"
        );
        // EntityImageObject: no stereotype circle icon
        assert!(
            !svg.contains("ellipse"),
            "object entity must NOT have stereotype circle"
        );
        // Must have exactly one separator line
        assert!(
            svg.contains("<line"),
            "object entity must have a separator line"
        );
    }

    // ── SkinParams tests ────────────────────────────────────────────

    #[test]
    fn test_skinparam_class_bg() {
        let (d, l) = simple_diagram();
        let mut skin = SkinParams::default();
        skin.set("ClassBackgroundColor", "#AABBCC");
        let svg = render(&d, &l, &skin, &default_meta()).unwrap();
        assert!(svg.contains(r##"fill="#AABBCC""##));
    }

    #[test]
    fn test_skinparam_class_border() {
        let (d, l) = simple_diagram();
        let mut skin = SkinParams::default();
        skin.set("ClassBorderColor", "#112233");
        let svg = render(&d, &l, &skin, &default_meta()).unwrap();
        assert!(svg.contains(r##"stroke:#112233"##));
    }

    #[test]
    fn test_skinparam_arrow_color() {
        let (d, l) = simple_diagram();
        let mut skin = SkinParams::default();
        skin.set("ArrowColor", "#00FF00");
        let svg = render(&d, &l, &skin, &default_meta()).unwrap();
        assert!(svg.contains(r##"stroke:#00FF00"##));
    }

    #[test]
    fn test_skinparam_font_color() {
        let (d, l) = simple_diagram();
        let mut skin = SkinParams::default();
        skin.set("ClassFontColor", "#FF0000");
        let svg = render(&d, &l, &skin, &default_meta()).unwrap();
        assert!(svg.contains(r##"fill="#FF0000""##));
    }

    #[test]
    fn test_default_colors() {
        let (d, l) = simple_diagram();
        let svg = render(&d, &l, &default_skin(), &default_meta()).unwrap();
        assert!(svg.contains(&format!(r#"fill="{ENTITY_BG}""#)));
        assert!(svg.contains(&format!(r#"stroke:{BORDER_COLOR}"#)));
    }

    // ── Meta rendering tests ────────────────────────────────────────

    #[test]
    fn test_meta_empty_passthrough() {
        let (d, l) = simple_diagram();
        let svg = render(&d, &l, &default_skin(), &default_meta()).unwrap();
        assert!(!svg.contains("translate(0,"));
    }

    #[test]
    fn test_meta_title() {
        let (d, l) = simple_diagram();
        let meta = DiagramMeta {
            title: Some("My Title".into()),
            ..Default::default()
        };
        let svg = render(&d, &l, &default_skin(), &meta).unwrap();
        assert!(svg.contains("My Title"));
        assert!(svg.contains("font-weight"));
        assert!(svg.contains("font-size=\"14\""));
        // Body coordinates are now shifted inline (no <g transform>)
        assert!(
            !svg.contains("translate("),
            "body should use inline coordinate offset, not <g transform>"
        );
    }

    #[test]
    fn test_meta_title_can_expand_canvas_width() {
        let (d, l) = simple_diagram();
        let body_result = render_body(&d, &l, &default_skin(), None, None, None).unwrap();
        let (body_w, _) = extract_dimensions(&body_result.svg);
        let meta = DiagramMeta {
            title: Some(
                "This is a deliberately very long title with [[https://example.com Link]]".into(),
            ),
            ..Default::default()
        };
        let svg = render(&d, &l, &default_skin(), &meta).unwrap();
        let (svg_w, _) = extract_dimensions(&svg);
        assert!(svg_w > body_w);
        // Body coordinates are shifted inline, not via <g transform>
    }

    #[test]
    fn test_meta_title_renders_creole_and_link() {
        let (d, l) = simple_diagram();
        let meta = DiagramMeta {
            title: Some("**Bold** [[https://example.com{hover} Link]]".into()),
            ..Default::default()
        };
        let svg = render(&d, &l, &default_skin(), &meta).unwrap();
        assert!(svg.contains(r#"font-weight="bold""#));
        assert!(svg.contains(r#"href="https://example.com""#));
        assert!(svg.contains(r#"title="hover""#));
        assert!(svg.contains("Link"));
    }

    #[test]
    fn test_meta_header() {
        let (d, l) = simple_diagram();
        let meta = DiagramMeta {
            header: Some("Page Header".into()),
            ..Default::default()
        };
        let svg = render(&d, &l, &default_skin(), &meta).unwrap();
        assert!(svg.contains("Page Header"));
    }

    #[test]
    fn test_meta_footer() {
        let (d, l) = simple_diagram();
        let meta = DiagramMeta {
            footer: Some("Page Footer".into()),
            ..Default::default()
        };
        let svg = render(&d, &l, &default_skin(), &meta).unwrap();
        assert!(svg.contains("Page Footer"));
    }

    #[test]
    fn test_meta_caption() {
        let (d, l) = simple_diagram();
        let meta = DiagramMeta {
            caption: Some("Figure 1".into()),
            ..Default::default()
        };
        let svg = render(&d, &l, &default_skin(), &meta).unwrap();
        assert!(svg.contains("Figure 1"));
        assert!(svg.contains("font-style=\"italic\""));
    }

    #[test]
    fn test_meta_legend() {
        let (d, l) = simple_diagram();
        let meta = DiagramMeta {
            legend: Some("Legend text".into()),
            ..Default::default()
        };
        let svg = render(&d, &l, &default_skin(), &meta).unwrap();
        assert!(svg.contains("Legend text"));
        assert!(svg.contains(LEGEND_BG));
        assert!(svg.contains(LEGEND_BORDER));
    }

    #[test]
    fn test_meta_all() {
        let (d, l) = simple_diagram();
        let meta = DiagramMeta {
            title: Some("T".into()),
            header: Some("H".into()),
            footer: Some("F".into()),
            caption: Some("C".into()),
            legend: Some("L".into()),
            ..Default::default()
        };
        let svg = render(&d, &l, &default_skin(), &meta).unwrap();
        for s in &["T", "H", "F", "C", "L"] {
            assert!(svg.contains(s));
        }
    }

    #[test]
    fn test_extract_dimensions() {
        let svg = r#"<svg viewBox="0 0 200.5 300.0" width="200.5" height="300.0">x</svg>"#;
        let (w, h) = extract_dimensions(svg);
        assert!((w - 200.5).abs() < 0.1);
        assert!((h - 300.0).abs() < 0.1);
    }

    #[test]
    fn test_extract_svg_content() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>"#;
        assert_eq!(extract_svg_content(svg), "<rect/>");
    }

    #[test]
    fn test_extract_svg_content_strips_plantuml_pi() {
        let svg =
            r#"<?plantuml 1.2026.2?><svg xmlns="http://www.w3.org/2000/svg"><defs/><g/></svg>"#;
        assert_eq!(extract_svg_content(svg), "<defs/><g/>");
    }

    #[test]
    fn test_encode_plantuml_source_matches_java() {
        let source = "@startuml\nclass A {\n}\n\nclass B{\n}\n\nA -->B\n@enduml\n";
        assert_eq!(
            encode_plantuml_source(source).unwrap(),
            "Iyv9B2vMS5Ievghbuae6Svp0R4S5NLqx9m00"
        );
    }

    #[test]
    fn test_dot_suppressed_produces_valid_svg() {
        let svg = render_dot_suppressed();
        assert!(svg.contains("<svg"), "must contain <svg tag");
        assert!(svg.contains("</svg>"), "must contain </svg> tag");
        assert!(
            svg.contains("suppressed"),
            "must contain suppressed message"
        );
        assert!(svg.contains("2495"), "must reference issue 2495");
    }

    // ── Note rendering tests ────────────────────────────────────────

    #[test]
    fn test_note_renders_polygon_and_text() {
        use crate::layout::graphviz::ClassNoteLayout;

        let entity = Entity {
            name: "Foo".into(),
            ..Entity::default()
        };
        let mut cd = empty_class_diagram();
        cd.entities = vec![entity];
        cd.notes = vec![crate::model::ClassNote {
            text: "test note".into(),
            position: "right".into(),
            target: Some("Foo".into()),
        }];
        let gl = GraphLayout {
            nodes: vec![NodeLayout {
                id: "Foo".into(),
                cx: 100.0,
                cy: 50.0,
                width: 120.0,
                height: 80.0,
                image_width: 120.0,
                min_x: 40.0,
                min_y: 10.0,
            }],
            edges: vec![],
            notes: vec![ClassNoteLayout {
                text: "test note".into(),
                x: 180.0,
                y: 30.0,
                width: 90.0,
                height: 36.0,
                lines: vec!["test note".into()],
                connector: Some((180.0, 50.0, 160.0, 50.0)),
                embedded: None,
                position: "left".into(),
            }],
            clusters: vec![],
            total_width: 300.0,
            total_height: 120.0,
            move_delta: (7.0, 7.0),
            normalize_offset: (0.0, 0.0),
            lf_span: (200.0, 100.0),
            lf_max: (200.0, 100.0),
            render_offset: (7.0, 7.0),
        };
        let svg = render(
            &Diagram::Class(cd),
            &DiagramLayout::Class(gl),
            &default_skin(),
            &default_meta(),
        )
        .unwrap();

        assert!(svg.contains(NOTE_BG), "note should use yellow background");
        assert!(svg.contains("test note"), "note text must appear in SVG");
        // Opale note with connector renders body as <path> with embedded connector arrow
        assert!(
            svg.contains("<path d=\"M"),
            "opale note should render as <path> with connector arrow"
        );
        // No separate dashed connector line for opale notes
        assert_eq!(
            svg.matches("stroke-dasharray").count(),
            0,
            "opale connector is embedded in path, not a dashed line"
        );
    }

    #[test]
    fn test_note_without_connector() {
        use crate::layout::graphviz::ClassNoteLayout;

        let mut cd = empty_class_diagram();
        cd.notes = vec![crate::model::ClassNote {
            text: "floating".into(),
            position: "right".into(),
            target: None,
        }];
        let gl = GraphLayout {
            nodes: vec![],
            edges: vec![],
            notes: vec![ClassNoteLayout {
                text: "floating".into(),
                x: 10.0,
                y: 10.0,
                width: 80.0,
                height: 36.0,
                lines: vec!["floating".into()],
                connector: None,
                embedded: None,
                position: "left".into(),
            }],
            clusters: vec![],
            total_width: 100.0,
            total_height: 60.0,
            move_delta: (7.0, 7.0),
            normalize_offset: (0.0, 0.0),
            lf_span: (200.0, 100.0),
            lf_max: (200.0, 100.0),
            render_offset: (7.0, 7.0),
        };
        let svg = render(
            &Diagram::Class(cd),
            &DiagramLayout::Class(gl),
            &default_skin(),
            &default_meta(),
        )
        .unwrap();

        assert!(svg.contains("floating"), "note text must appear");
        assert!(svg.contains(NOTE_BG), "note background must appear");
        // No connector line - count dashed lines (only note polygon, no connector dash)
        let dash_count = svg.matches("stroke-dasharray=\"5,3\"").count();
        assert_eq!(dash_count, 0, "floating note should have no connector line");
    }
}
