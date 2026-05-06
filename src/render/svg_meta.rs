use std::fmt::Write;
use std::io::Write as IoWrite;

use flate2::write::DeflateEncoder;
use flate2::Compression;

use crate::font_metrics;
use crate::klimt::svg::fmt_coord;
use crate::model::DiagramMeta;
use crate::skin::rose::{DIVIDER_COLOR, LEGEND_BG, LEGEND_BORDER, TEXT_COLOR};
use crate::style::SkinParams;
use crate::Result;

use super::svg::{
    ensure_visible_int, write_bg_rect, write_svg_root_bg, write_svg_title, DOC_MARGIN_BOTTOM,
    DOC_MARGIN_RIGHT, PLANTUML_VERSION,
};
use super::svg_richtext::{creole_table_width, render_creole_display_lines, render_creole_text};

// ── Meta rendering constants ────────────────────────────────────────

const META_TITLE_FONT_SIZE: f64 = 14.0;
const META_HF_FONT_SIZE: f64 = 10.0;
const META_CAPTION_FONT_SIZE: f64 = 14.0;
const META_LEGEND_FONT_SIZE: f64 = 14.0;
/// Java TextBlockBordered.calculateDimension() returns (width+1, height+1).
/// See TextBlockBordered.java:98.
const BORDERED_EXTRA: f64 = 1.0;
const TITLE_PADDING: f64 = 5.0;
const TITLE_MARGIN: f64 = 5.0;
const CAPTION_PADDING: f64 = 0.0;
const CAPTION_MARGIN: f64 = 1.0;
const LEGEND_PADDING: f64 = 5.0;
const LEGEND_MARGIN: f64 = 12.0;
const LEGEND_ROUND_CORNER: f64 = 15.0;

pub(super) fn creole_text_w(text: &str, font_size: f64, bold: bool) -> f64 {
    crate::render::svg_richtext::creole_max_line_width(text, "SansSerif", font_size, bold, false)
}
pub(super) fn text_block_h(font_size: f64, bold: bool) -> f64 {
    font_metrics::ascent("SansSerif", font_size, bold, false)
        + font_metrics::descent("SansSerif", font_size, bold, false)
}
pub(super) fn bordered_dim(text_w: f64, text_h: f64, padding: f64) -> (f64, f64) {
    (
        text_w + 2.0 * padding + BORDERED_EXTRA,
        text_h + 2.0 * padding + BORDERED_EXTRA,
    )
}
pub(super) fn block_dim(text_w: f64, text_h: f64, padding: f64, margin: f64) -> (f64, f64) {
    let (bw, bh) = bordered_dim(text_w, text_h, padding);
    (bw + 2.0 * margin, bh + 2.0 * margin)
}
pub(super) fn merge_tb(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0.max(b.0), a.1 + b.1)
}

pub(super) fn extract_dimensions(svg: &str) -> (f64, f64) {
    if let Some(vb_start) = svg.find("viewBox=\"") {
        let after = &svg[vb_start + 9..];
        if let Some(vb_end) = after.find('"') {
            let parts: Vec<&str> = after[..vb_end].split_whitespace().collect();
            if parts.len() == 4 {
                let w = parts[2].parse::<f64>().unwrap_or(400.0);
                let h = parts[3].parse::<f64>().unwrap_or(300.0);
                return (w, h);
            }
        }
    }
    let w = extract_attr(svg, "width").unwrap_or(400.0);
    let h = extract_attr(svg, "height").unwrap_or(300.0);
    (w, h)
}

pub(super) fn extract_attr(svg: &str, attr: &str) -> Option<f64> {
    let needle = format!("{attr}=\"");
    if let Some(pos) = svg.find(&needle) {
        let after = &svg[pos + needle.len()..];
        if let Some(end) = after.find('"') {
            return after[..end].parse::<f64>().ok();
        }
    }
    None
}

/// Compute the body (dx, dy) offset for meta wrapping.
///
/// This is the offset from SVG origin to the body content start, accounting
/// for header and title dimensions. Used to pre-apply the offset in the body
/// renderer, avoiding lossy string-level coordinate shifting.
pub(super) fn compute_meta_body_offset(meta: &DiagramMeta, skin: &SkinParams) -> (f64, f64) {
    let title_font_size = skin
        .get("document.title.fontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(META_TITLE_FONT_SIZE);
    let title_bold = title_font_size == META_TITLE_FONT_SIZE;

    let hdr_font_size = skin
        .get("document.header.fontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(META_HF_FONT_SIZE);
    let hdr_text_h = if meta.header.is_some() {
        text_block_h(hdr_font_size, false)
    } else {
        0.0
    };
    let hdr_text_w = meta
        .header
        .as_ref()
        .map(|t| creole_text_w(t, hdr_font_size, false))
        .unwrap_or(0.0);
    let hdr_dim = if meta.header.is_some() {
        block_dim(hdr_text_w, hdr_text_h, 0.0, 0.0)
    } else {
        (0.0, 0.0)
    };

    let title_text_h = if let Some(ref t) = meta.title {
        let lh = font_metrics::line_height("SansSerif", title_font_size, title_bold, false);
        let n_lines = t
            .split(crate::NEWLINE_CHAR)
            .flat_map(|s| s.lines())
            .count()
            .max(1);
        let mut h = n_lines as f64 * lh;
        // Java: tables inside title use AtomWithMargin(table, 2, 2) adding 4px.
        let has_table = t
            .split(crate::NEWLINE_CHAR)
            .flat_map(|s| s.lines())
            .any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with('|') || (trimmed.starts_with('<') && trimmed.contains(">|"))
            });
        if has_table {
            h += 4.0;
        }
        h
    } else {
        0.0
    };
    let title_text_w = meta
        .title
        .as_ref()
        .map(|t| {
            creole_table_width(t, title_font_size, title_bold)
                .unwrap_or_else(|| creole_text_w(t, title_font_size, title_bold))
        })
        .unwrap_or(0.0);
    let title_dim = if meta.title.is_some() {
        block_dim(title_text_w, title_text_h, TITLE_PADDING, TITLE_MARGIN)
    } else {
        (0.0, 0.0)
    };

    // body_abs_y = hdr_dim.1 + title_dim.1
    // body_abs_x = centering terms (typically 0 when body is wider than meta)
    (0.0, hdr_dim.1 + title_dim.1)
}

pub(super) fn extract_svg_content(svg: &str) -> String {
    let mut body = svg;
    if body.starts_with("<?plantuml ") {
        if let Some(end) = body.find("?>") {
            body = &body[end + 2..];
        }
    }
    if let Some(tag_end) = body.find('>') {
        let after_open = &body[tag_end + 1..];
        if let Some(close_pos) = after_open.rfind("</svg>") {
            return after_open[..close_pos].to_string();
        }
        return after_open.to_string();
    }
    body.to_string()
}

// ── SVG interactive CSS/JS resources ─────────────────────────────────

/// CSS for sequence diagrams when `!pragma svginteractive true`
const SEQUENCE_INTERACTIVE_CSS: &str = include_str!("interactive/sequencediagram.css");
/// JS for sequence diagrams when `!pragma svginteractive true`
const SEQUENCE_INTERACTIVE_JS: &str = include_str!("interactive/sequencediagram.js");
/// CSS for non-sequence diagrams when `!pragma svginteractive true`
const DEFAULT_INTERACTIVE_CSS: &str = include_str!("interactive/default.css");
/// JS for non-sequence diagrams when `!pragma svginteractive true`
const DEFAULT_INTERACTIVE_JS: &str = include_str!("interactive/default.js");

/// Ensure text ends with a newline (matches Java FileUtils.readText behavior).
fn ensure_trailing_newline(s: &str) -> String {
    if s.ends_with('\n') {
        s.to_string()
    } else {
        format!("{}\n", s)
    }
}

/// XML-escape text for embedding in SVG `<script>` elements.
fn xml_escape_js(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 10);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Inject Google Fonts @import directives into `<defs>` for non-builtin fonts.
///
/// Java PlantUML detects when text elements reference fonts that should be
/// loaded from Google Fonts (currently Roboto, with all weights/styles) and
/// emits a `<style type="text/css">@import url(...);</style>` block into the
/// SVG `<defs>`. This ensures the SVG renders correctly when opened in a browser.
pub(super) fn inject_google_fonts(svg: String) -> String {
    // Detect Roboto usage. Java emits the @import once per SVG when any text
    // element uses font-family="Roboto".
    let needs_roboto = svg.contains(r#"font-family="Roboto""#);
    if !needs_roboto {
        return svg;
    }

    let import_block = r#"<style type="text/css">@import url('https://fonts.googleapis.com/css?family=Roboto:400,100,100italic,300,300italic,400italic,500,500italic,700,700italic,900,900italic');</style>"#;

    // Replace empty <defs/> with populated <defs>
    if let Some(pos) = svg.find("<defs/>") {
        let mut result = String::with_capacity(svg.len() + import_block.len());
        result.push_str(&svg[..pos]);
        result.push_str("<defs>");
        result.push_str(import_block);
        result.push_str("</defs>");
        result.push_str(&svg[pos + 7..]);
        result
    } else if let Some(pos) = svg.find("<defs>") {
        // Already has <defs>...</defs> — inject at start of defs content
        let insert_pos = pos + 6;
        let mut result = String::with_capacity(svg.len() + import_block.len());
        result.push_str(&svg[..insert_pos]);
        result.push_str(import_block);
        result.push_str(&svg[insert_pos..]);
        result
    } else {
        svg
    }
}

/// Inject interactive CSS and JS into the SVG `<defs>` section.
pub(super) fn inject_svginteractive(svg: String, diagram_type: &str) -> String {
    let (css, js) = if diagram_type == "SEQUENCE" {
        (SEQUENCE_INTERACTIVE_CSS, SEQUENCE_INTERACTIVE_JS)
    } else {
        (DEFAULT_INTERACTIVE_CSS, DEFAULT_INTERACTIVE_JS)
    };

    // Java readText() reads line-by-line and appends \n after each line,
    // effectively ensuring trailing newline. Replicate that behavior.
    let css_text = ensure_trailing_newline(css);
    let js_text = ensure_trailing_newline(js);

    // Java wraps CSS content inside CDATA block.
    let defs_content = format!(
        "<style type=\"text/css\"><![CDATA[{}]]></style><script>{}</script>",
        css_text,
        xml_escape_js(&js_text)
    );

    // Replace empty <defs/> with populated <defs>
    if let Some(pos) = svg.find("<defs/>") {
        let mut result = String::with_capacity(svg.len() + defs_content.len());
        result.push_str(&svg[..pos]);
        result.push_str("<defs>");
        result.push_str(&defs_content);
        result.push_str("</defs>");
        result.push_str(&svg[pos + 7..]);
        result
    } else if let Some(pos) = svg.find("<defs>") {
        // Already has <defs>...</defs> — inject at start of defs content
        let insert_pos = pos + 6;
        let mut result = String::with_capacity(svg.len() + defs_content.len());
        result.push_str(&svg[..insert_pos]);
        result.push_str(&defs_content);
        result.push_str(&svg[insert_pos..]);
        result
    } else {
        // No defs section found — insert before <g>
        if let Some(pos) = svg.find("<g>") {
            let mut result = String::with_capacity(svg.len() + defs_content.len() + 14);
            result.push_str(&svg[..pos]);
            result.push_str("<defs>");
            result.push_str(&defs_content);
            result.push_str("</defs>");
            result.push_str(&svg[pos..]);
            result
        } else {
            svg
        }
    }
}

pub(crate) fn inject_plantuml_source(mut svg: String, source: &str) -> Result<String> {
    let encoded = encode_plantuml_source(source)?;
    let pi = format!("<?plantuml-src {encoded}?>");
    if let Some(pos) = svg.rfind("</g></svg>") {
        svg.insert_str(pos, &pi);
        return Ok(svg);
    }
    if let Some(pos) = svg.rfind("</svg>") {
        svg.insert_str(pos, &pi);
        return Ok(svg);
    }
    Err(crate::Error::Render(
        "rendered SVG missing closing tag for plantuml-src injection".into(),
    ))
}

pub(super) fn encode_plantuml_source(source: &str) -> Result<String> {
    let compressed_source = compress_plantuml_source_for_pi(source);
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
    encoder
        .write_all(compressed_source.as_bytes())
        .map_err(|e| crate::Error::Render(format!("failed to deflate PlantUML source: {e}")))?;
    let compressed = encoder
        .finish()
        .map_err(|e| crate::Error::Render(format!("failed to finish PlantUML deflate: {e}")))?;
    Ok(encode_plantuml_ascii(&compressed))
}

fn compress_plantuml_source_for_pi(source: &str) -> String {
    // Java emits the plantuml-src processing instruction with two distinct
    // normalisations, depending on the diagram family:
    //
    // * `@startuml`/`@enduml` — the surrounding markers are stripped and only
    //   the body is deflated; no version footer.
    // * Every other `@startXxx`/`@endXxx` (yaml, json, files, regex, ebnf,
    //   salt, …) — the full source including the delimiters is deflated, and
    //   Java appends an empty line followed by the PlantUML version literal
    //   (`1.2026.2`). We replicate that trailer here to stay byte-identical.
    let trimmed = source.trim_matches(|c| matches!(c, ' ' | '\t' | '\r' | '\n' | '\0'));

    // Detect which family we're in by scanning for the first `@start...` marker.
    let (start_line, _end_line) = trimmed
        .lines()
        .find(|l| l.starts_with('@'))
        .map(|l| {
            let name = l
                .trim_start_matches('@')
                .split_whitespace()
                .next()
                .unwrap_or("");
            (name.to_string(), name.replace("start", "end"))
        })
        .unwrap_or_default();

    if start_line == "startuml" {
        // Strip the markers and return the body only. Matches pre-existing behaviour.
        let mut body = Vec::new();
        let mut in_diagram = false;
        for line in source.lines() {
            if !in_diagram {
                if line.starts_with("@startuml") {
                    in_diagram = true;
                }
                continue;
            }
            if line.starts_with("@enduml") {
                break;
            }
            body.push(line);
        }
        return if in_diagram {
            trim_plantuml_source(&body.join("\n"))
        } else {
            trim_plantuml_source(source)
        };
    }

    // Non-uml family: keep the delimiters intact and append the Java version footer.
    // We still trim stray whitespace around the outside to avoid format drift.
    let mut out = String::with_capacity(trimmed.len() + 16);
    out.push_str(trimmed);
    out.push_str("\n\n");
    out.push_str(PLANTUML_VERSION);
    out
}

fn trim_plantuml_source(source: &str) -> String {
    source
        .trim_matches(|c| matches!(c, ' ' | '\t' | '\r' | '\n' | '\0'))
        .to_string()
}

fn encode_plantuml_ascii(data: &[u8]) -> String {
    let mut result = String::with_capacity((data.len() * 4).div_ceil(3));
    for chunk in data.chunks(3) {
        let b1 = chunk[0];
        let b2 = *chunk.get(1).unwrap_or(&0);
        let b3 = *chunk.get(2).unwrap_or(&0);
        append_plantuml_3bytes(&mut result, b1, b2, b3);
    }
    result
}

fn append_plantuml_3bytes(buf: &mut String, b1: u8, b2: u8, b3: u8) {
    let c1 = b1 >> 2;
    let c2 = ((b1 & 0x03) << 4) | (b2 >> 4);
    let c3 = ((b2 & 0x0F) << 2) | (b3 >> 6);
    let c4 = b3 & 0x3F;
    buf.push(encode6bit(c1 & 0x3F));
    buf.push(encode6bit(c2 & 0x3F));
    buf.push(encode6bit(c3 & 0x3F));
    buf.push(encode6bit(c4 & 0x3F));
}

fn encode6bit(b: u8) -> char {
    match b {
        0..=9 => (b'0' + b) as char,
        10..=35 => (b'A' + (b - 10)) as char,
        36..=61 => (b'a' + (b - 36)) as char,
        62 => '-',
        63 => '_',
        _ => '?',
    }
}

pub(super) fn wrap_with_meta(
    body_svg: &str,
    meta: &DiagramMeta,
    diagram_type: &str,
    bg: &str,
    raw_body_dim: Option<(f64, f64)>,
    body_pre_offset: bool,
    body_degenerated: bool,
    skin: &crate::style::SkinParams,
) -> Result<String> {
    // SEQUENCE diagrams have a distinct chrome layout (SequenceDiagramArea) that
    // differs from the TextBlockExporter path used by other diagram types.
    // Java disables the annotation/chrome wrapping in SequenceDiagram.createImageBuilder
    // (annotations(false)) and instead lets SequenceDiagramFileMakerPuma2.createUDrawable
    // compose the chrome directly around the body.  Route to a dedicated function.
    if diagram_type == "SEQUENCE" {
        let has_meta = meta.title.is_some()
            || meta.header.is_some()
            || meta.footer.is_some()
            || meta.caption.is_some()
            || meta.legend.is_some();
        if has_meta {
            return wrap_with_meta_sequence(body_svg, meta, bg, raw_body_dim, skin);
        }
    }

    let (svg_w, svg_h) = extract_dimensions(body_svg);
    let body_content = extract_svg_content(body_svg);

    // Document-level margin: Java TextBlockExporter12026 applies diagram.getDefaultMargins().
    // Sequence diagrams (Puma2 classic): margin(top=5, right=5, bottom=5, left=0)
    // Sequence diagrams (Teoz):          margin(top=5, right=5, bottom=5, left=5)
    // CucaDiagram (class/component/etc): margin(top=0, right=5, bottom=5, left=0)
    // The body SVG viewport already bakes in these margins via the layout engine,
    // so we recover the raw textBlock dimensions by subtracting them.
    // Java ImageBuilder default margins per diagram type.
    // Activity (FTile): body viewport already includes internal padding from
    // compute_bounds (TOP_MARGIN + BOTTOM_MARGIN + 3) which absorbs the external
    // margin budget.  title_margin_top=10 shifts meta elements down.
    // Sequence (Puma2/Teoz): margin_top=5, body includes right margin.
    // CucaDiagram (class/component/etc): margin_top=0.
    // Java ImageBuilder default margins per diagram type.
    let doc_margin_top = match diagram_type {
        "SEQUENCE" => 5.0,
        "ACTIVITY" => 10.0,
        // Java TitledDiagram.getDefaultMargins() = 10 all sides for mindmap.
        "MINDMAP" => 10.0,
        _ => 0.0,
    };
    // Activity body viewport already includes right padding from compute_bounds,
    // so DOC_MARGIN_RIGHT must not be added again for the canvas width.
    let doc_margin_right = match diagram_type {
        "ACTIVITY" => 0.0,
        // Mindmap: Java uses 10+10=20 (margin_left + margin_right).
        "MINDMAP" => 20.0,
        // Java NwDiagram.getDefaultMargins() = none → 0 all sides.
        "NWDIAG" => 0.0,
        _ => DOC_MARGIN_RIGHT,
    };
    let doc_margin_bottom = match diagram_type {
        // Mindmap: Java uses 10+10=20 total vertical margins.
        "MINDMAP" => 10.0,
        // Java NwDiagram.getDefaultMargins() = none → 0 all sides.
        "NWDIAG" => 0.0,
        _ => DOC_MARGIN_BOTTOM,
    };

    // Use raw body dimensions if available (avoids integer truncation loss).
    // Otherwise fall back to extracting from SVG header (lossy).
    let body_is_empty = body_content.trim().is_empty()
        || body_content.trim() == "<defs/><g></g>"
        || body_content.trim() == "<defs/><g/>";
    let (body_w, body_h) = if let Some((rw, rh)) = raw_body_dim {
        // Java SvekResult.calculateDimension() returns getDimension().delta(0, 12)
        // where getDimension() = (maxX - minX, maxY - minY) is the LimitFinder span.
        // raw_body_dim is the absolute max_point; the moveDelta(6,6) ensures minX=minY=6,
        // so span = (rw - 6, rh - 6) and calculateDimension = (rw - 6, rh - 6 + 12).
        // Apply span conversion only for CLASS (svek-based) diagrams with meta elements
        // that need centering; other diagram types use raw dimensions directly.
        let has_meta = meta.title.is_some()
            || meta.header.is_some()
            || meta.footer.is_some()
            || meta.caption.is_some()
            || meta.legend.is_some();
        if diagram_type == "CLASS"
            && meta.legend.is_some()
            && has_meta
            && body_is_empty
            && rw == 0.0
            && rh == 0.0
        {
            // Empty svek graphs still carry a 10x10 SVG canvas in Java's meta
            // composition path; collapsing that to zero lifts a legend-only block
            // by 10px and shrinks the final viewport.
            (svg_w, svg_h)
        } else {
            let (svek_delta_w, svek_delta_h) = if diagram_type == "CLASS" && has_meta && rh > 0.0 {
                // Degenerated svek path (single entity, no edges/notes) already
                // returns (node.width + 14, node.height + 14) which is the
                // pre-margin textBlock dimension that wrap_with_meta expects.
                // The normal -6 / +6 correction only applies to the moveDelta-shifted
                // LimitFinder span used by the multi-node code path.
                if body_degenerated {
                    (0.0, 0.0)
                } else {
                    (-6.0, 6.0) // span: subtract minX=6; height: span - 6 + 12 = +6
                }
            } else if diagram_type == "SEQUENCE" && has_meta && rh > 0.0 {
                // Java's LimitFinder tracks the participant tail bottom + extra border.
                // The layout formula undershoots by ~2.5px vs the actual drawn bounds.
                (0.0, 2.5)
            } else {
                (0.0, 0.0)
            };
            (rw + svek_delta_w, rh + svek_delta_h)
        }
    } else {
        // Body SVG includes DOC_MARGIN + 1: recover raw textBlock dimensions.
        (
            svg_w - doc_margin_right - 1.0,
            svg_h - doc_margin_top - doc_margin_bottom - 1.0,
        )
    };
    log::trace!("wrap_with_meta: svg_w={svg_w} svg_h={svg_h} body_w={body_w} body_h={body_h} doc_margin_top={doc_margin_top}");

    // ── Resolve document section styles ──────────────────────────────
    let hdr_font_size = skin
        .get("document.header.fontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(META_HF_FONT_SIZE);
    let hdr_font_color = skin.get("document.header.fontcolor").map(|s| s.to_string());
    let hdr_bg_color = skin
        .get("document.header.backgroundcolor")
        .map(|s| s.to_string());

    let ftr_font_size = skin
        .get("document.footer.fontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(META_HF_FONT_SIZE);
    let ftr_font_color = skin.get("document.footer.fontcolor").map(|s| s.to_string());
    let ftr_bg_color = skin
        .get("document.footer.backgroundcolor")
        .map(|s| s.to_string());

    let title_font_size = skin
        .get("document.title.fontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(META_TITLE_FONT_SIZE);
    let title_font_color = skin.get("document.title.fontcolor").map(|s| s.to_string());
    let title_bg_color = skin
        .get("document.title.backgroundcolor")
        .map(|s| s.to_string());

    let leg_font_size = skin
        .get("document.legend.fontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(META_LEGEND_FONT_SIZE);
    let leg_font_color = skin.get("document.legend.fontcolor").map(|s| s.to_string());
    let leg_bg_color = skin
        .get("document.legend.backgroundcolor")
        .map(|s| s.to_string());

    let cap_font_size = skin
        .get("document.caption.fontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(META_CAPTION_FONT_SIZE);
    let cap_font_color = skin
        .get("document.caption.fontcolor")
        .map(|s| s.to_string());
    let cap_bg_color = skin
        .get("document.caption.backgroundcolor")
        .map(|s| s.to_string());

    let title_bold = title_font_size == META_TITLE_FONT_SIZE; // default title is bold

    // ── 1. Compute block dimensions for each meta element ───────────
    let hdr_text_w = meta
        .header
        .as_ref()
        .map(|t| creole_text_w(t, hdr_font_size, false))
        .unwrap_or(0.0);
    let hdr_text_h = if meta.header.is_some() {
        text_block_h(hdr_font_size, false)
    } else {
        0.0
    };
    let hdr_dim = if meta.header.is_some() {
        block_dim(hdr_text_w, hdr_text_h, 0.0, 0.0)
    } else {
        (0.0, 0.0)
    };

    let ftr_text_w = meta
        .footer
        .as_ref()
        .map(|t| creole_text_w(t, ftr_font_size, false))
        .unwrap_or(0.0);
    let ftr_text_h = if meta.footer.is_some() {
        text_block_h(ftr_font_size, false)
    } else {
        0.0
    };
    let ftr_dim = if meta.footer.is_some() {
        block_dim(ftr_text_w, ftr_text_h, 0.0, 0.0)
    } else {
        (0.0, 0.0)
    };

    let title_text_w = meta
        .title
        .as_ref()
        .map(|t| {
            // For tables, compute width from table layout (column-based) instead of raw text
            creole_table_width(t, title_font_size, title_bold)
                .unwrap_or_else(|| creole_text_w(t, title_font_size, title_bold))
        })
        .unwrap_or(0.0);
    let title_text_h = if let Some(ref t) = meta.title {
        let lh = font_metrics::line_height("SansSerif", title_font_size, title_bold, false);
        let n_lines = t
            .split(crate::NEWLINE_CHAR)
            .flat_map(|s| s.lines())
            .count()
            .max(1);
        let mut h = n_lines as f64 * lh;
        // Java: tables inside title use AtomWithMargin(table, 2, 2) which adds 4px.
        // Detect table content: any line starts with '|' or color prefix '<#...>|'.
        let has_table = t
            .split(crate::NEWLINE_CHAR)
            .flat_map(|s| s.lines())
            .any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with('|') || (trimmed.starts_with('<') && trimmed.contains(">|"))
            });
        if has_table {
            h += 4.0; // TABLE_MARGIN_Y * 2 (Java AtomWithMargin top+bottom)
        }
        h
    } else {
        0.0
    };
    let title_dim = if meta.title.is_some() {
        block_dim(title_text_w, title_text_h, TITLE_PADDING, TITLE_MARGIN)
    } else {
        (0.0, 0.0)
    };
    log::trace!("wrap_with_meta: title text_w={title_text_w:.10} text_h={title_text_h:.10} title_dim={title_dim:?}");

    let cap_text_w = meta
        .caption
        .as_ref()
        .map(|t| creole_text_w(t, cap_font_size, false))
        .unwrap_or(0.0);
    let cap_text_h = if meta.caption.is_some() {
        text_block_h(cap_font_size, false)
    } else {
        0.0
    };
    let cap_dim = if meta.caption.is_some() {
        block_dim(cap_text_w, cap_text_h, CAPTION_PADDING, CAPTION_MARGIN)
    } else {
        (0.0, 0.0)
    };

    let leg_text_w = meta
        .legend
        .as_ref()
        .map(|t| creole_text_w(t, leg_font_size, false))
        .unwrap_or(0.0);
    let leg_text_h = if let Some(ref leg) = meta.legend {
        crate::render::svg_richtext::compute_creole_note_text_height(leg, leg_font_size)
    } else {
        0.0
    };
    let leg_dim = if meta.legend.is_some() {
        block_dim(leg_text_w, leg_text_h, LEGEND_PADDING, LEGEND_MARGIN)
    } else {
        (0.0, 0.0)
    };

    // ── 2. Compute total dimensions (inside-out stacking) ──────────
    let body_dim = (body_w, body_h);
    let after_legend = merge_tb(body_dim, leg_dim);
    let after_title = merge_tb(title_dim, after_legend);
    let after_caption = merge_tb(after_title, cap_dim);
    let hf_dim = merge_tb(hdr_dim, ftr_dim);
    let total_dim = merge_tb(after_caption, hf_dim);
    // textBlock dimensions for positioning
    let tb_w = total_dim.0;
    let tb_h = total_dim.1;
    // Java viewport = SvgGraphics.ensureVisible(getFinalDimension) where:
    //   getFinalDimension = lf_maxX + 1 + margins
    //   ensureVisible(x) = (int)(x + 1)
    // ensure_visible_int applies the ensureVisible +1.
    // CucaDiagram (class/object/component) and MINDMAP both expose raw
    // textBlock dimensions that mirror Java's MindMap.calculateDimension /
    // svek limitFinder span; the extra +1 from Java's getFinalDimension must
    // be added here so the canvas size grows by 1px while keeping caption
    // centering aligned to the unpadded body width.
    // Other diagram types (sequence, activity) bake the +1 into their layout
    // arithmetic already.
    let get_final_dim_extra = match diagram_type {
        "CLASS" | "MINDMAP" => 1.0,
        // Java NwDiagram LimitFinder span exceeds the raw layout maxX by ~1px
        // (text/box rendering tracks slightly wider in AWT than in our
        // DejaVu metrics). +1 keeps the canvas size matching Java while
        // letting the title block centre on the un-padded layout width.
        "NWDIAG" => 1.0,
        _ => 0.0,
    };
    let canvas_w = ensure_visible_int(tb_w + get_final_dim_extra + doc_margin_right) as f64;
    let canvas_h =
        ensure_visible_int(tb_h + get_final_dim_extra + doc_margin_top + doc_margin_bottom) as f64;
    log::trace!(
        "wrap_with_meta: tb_w={tb_w:.6} tb_h={tb_h:.6} canvas_w={canvas_w} canvas_h={canvas_h}"
    );
    log::trace!("wrap_with_meta: body_dim=({body_w},{body_h}) after_legend={after_legend:?} after_title={after_title:?} after_caption={after_caption:?}");

    // ── 3. Compute absolute drawing positions ──────────────────────
    let outer_inner_x = ((tb_w - after_caption.0) / 2.0).max(0.0);
    let cap_inner_x = ((after_caption.0 - after_title.0) / 2.0).max(0.0);
    let title_inner_x = ((after_title.0 - after_legend.0) / 2.0).max(0.0);
    let leg_inner_x = ((after_legend.0 - body_w) / 2.0).max(0.0);

    let body_abs_x = outer_inner_x + cap_inner_x + title_inner_x + leg_inner_x;
    let body_abs_y = hdr_dim.1 + title_dim.1;
    // Java TextBlockExporter12026 applies UTranslate(margin_left, margin_top) to the
    // whole textBlock. For sequence diagrams margin_top=5 shifts all meta elements down.
    // The body content already has this margin baked into its internal coordinates
    // (layout MARGIN=5), so only meta elements need the shift.
    let meta_dy = doc_margin_top;
    // Horizontal margin for meta elements. For mindmap/WBS, Java ImageBuilder
    // shifts everything by margin_left=10, including meta elements.
    let meta_dx = match diagram_type {
        "MINDMAP" => 10.0,
        _ => 0.0,
    };
    log::trace!(
        "body_pos: body_abs_x={body_abs_x:.6} body_abs_y={body_abs_y:.6} meta_dy={meta_dy}"
    );

    // ── 4. Render SVG ──────────────────────────────────────────────
    let mut buf = String::with_capacity(body_svg.len() + 2048);
    write_svg_root_bg(&mut buf, canvas_w, canvas_h, diagram_type, bg);
    // SVG document title (metadata, not the visible title block)
    // Java joins multi-line title Display with "\n" and XML-escapes it for the <title> element.
    // The raw text is used, preserving link/table markup — creole is NOT stripped.
    if let Some(ref t) = meta.title {
        if !t.is_empty() {
            write_svg_title(&mut buf, t);
        }
    }
    buf.push_str("<defs/><g>");
    write_bg_rect(&mut buf, canvas_w, canvas_h, bg);

    // Header (RIGHT-aligned)
    if let Some(ref hdr) = meta.header {
        let hdr_x = tb_w - hdr_dim.0;
        let text_y = meta_dy + font_metrics::ascent("SansSerif", hdr_font_size, false, false);
        let text_color = hdr_font_color.as_deref().unwrap_or(DIVIDER_COLOR);
        write!(buf, r#"<g class="header""#).unwrap();
        if let Some(sl) = meta.header_line {
            write!(buf, r#" data-source-line="{sl}""#).unwrap();
        }
        buf.push('>');
        if let Some(ref bg) = hdr_bg_color {
            write!(buf,
                r#"<rect fill="{}" height="{}" style="stroke:none;stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
                bg, fmt_coord(hdr_text_h), fmt_coord(hdr_text_w), fmt_coord(hdr_x), fmt_coord(meta_dy)
            ).unwrap();
        }
        render_creole_text(
            &mut buf,
            hdr,
            hdr_x,
            text_y,
            text_block_h(hdr_font_size, false),
            text_color,
            None,
            &format!(r#"font-size="{}""#, hdr_font_size as i32),
        );
        if buf.ends_with('\n') {
            buf.pop();
        }
        buf.push_str("</g>");
    }

    // Title (CENTER-aligned)
    if let Some(ref title) = meta.title {
        // Java centres the title using `dimOriginal.width` (the activity tile
        // span max-min, no margins).  Our `body_w` for activity equals
        // `layout.width = dimOriginal + TOP_MARGIN(11) + right_pad(10) =
        // dimOriginal + 21`, while Java's `dimTotal = dimOriginal + 2*margin.left
        // = dimOriginal + 20`.  The +1 asymmetry from TOP_MARGIN vs right_pad
        // shifts our centring 0.5px right.  Compensate by trimming 1 from the
        // centring denominator for ACTIVITY only — body_w remains untouched so
        // the canvas size keeps using the unadjusted layout.width.
        let activity_centering_adjust = if diagram_type == "ACTIVITY" { 1.0 } else { 0.0 };
        let title_block_x = outer_inner_x
            + cap_inner_x
            + ((after_title.0 - activity_centering_adjust - title_dim.0) / 2.0).max(0.0);
        let text_x = title_block_x + TITLE_MARGIN + TITLE_PADDING;
        let text_y = meta_dy
            + hdr_dim.1
            + TITLE_MARGIN
            + TITLE_PADDING
            + font_metrics::ascent("SansSerif", title_font_size, title_bold, false);
        let text_color = title_font_color.as_deref().unwrap_or(TEXT_COLOR);
        write!(buf, r#"<g class="title""#).unwrap();
        if let Some(sl) = meta.title_line {
            write!(buf, r#" data-source-line="{sl}""#).unwrap();
        }
        buf.push('>');
        if let Some(ref bg) = title_bg_color {
            let rect_x = title_block_x + TITLE_MARGIN;
            let rect_y = meta_dy + hdr_dim.1 + TITLE_MARGIN;
            let rect_w = title_text_w + 2.0 * TITLE_PADDING;
            let rect_h = title_text_h + 2.0 * TITLE_PADDING;
            write!(buf,
                r#"<rect fill="{}" height="{}" style="stroke:none;stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
                bg, fmt_coord(rect_h), fmt_coord(rect_w), fmt_coord(rect_x), fmt_coord(rect_y)
            ).unwrap();
        }
        let weight_str = if title_bold {
            r#" font-weight="bold""#
        } else {
            ""
        };
        let outer_attrs = format!(r#"font-size="{}"{}"#, title_font_size as i32, weight_str);
        // Detect table content: use block-level rendering which handles tables
        let title_lines: Vec<String> = title
            .split(crate::NEWLINE_CHAR)
            .flat_map(|s| s.lines())
            .map(|s| s.to_string())
            .collect();
        let has_table = creole_table_width(title, title_font_size, title_bold).is_some();
        if has_table {
            render_creole_display_lines(
                &mut buf,
                &title_lines,
                text_x,
                meta_dy + hdr_dim.1 + TITLE_MARGIN + TITLE_PADDING,
                text_color,
                &outer_attrs,
                false,
            );
        } else {
            render_creole_text(
                &mut buf,
                title,
                text_x,
                text_y,
                text_block_h(title_font_size, title_bold),
                text_color,
                None,
                &outer_attrs,
            );
        }
        if buf.ends_with('\n') {
            buf.pop();
        }
        buf.push_str("</g>");
    }

    // Body — Java renders body at absolute coordinates (no <g transform>).
    // Strip the <defs/><g>...</g> wrapper from body_content (already have top-level <defs/>)
    // and shift coordinates by (body_abs_x, body_abs_y).
    let body_inner = body_content
        .strip_prefix("<defs/><g>")
        .unwrap_or(&body_content);
    let body_inner = body_inner.strip_suffix("</g>").unwrap_or(body_inner);
    // Strip body-level background rect if present (wrap_with_meta provides its own).
    // Pattern: <rect fill="..." height="N" style="stroke:none;stroke-width:1;" width="N" x="0" y="0"/>
    let body_inner = if body_inner.starts_with("<rect fill=\"") {
        if let Some(end) = body_inner.find("/>") {
            let rect_tag = &body_inner[..end + 2];
            if rect_tag.contains("stroke:none")
                && rect_tag.contains("x=\"0\"")
                && rect_tag.contains("y=\"0\"")
            {
                &body_inner[end + 2..]
            } else {
                body_inner
            }
        } else {
            body_inner
        }
    } else {
        body_inner
    };
    if !body_inner.trim().is_empty() {
        if body_pre_offset || (body_abs_x.abs() < 0.001 && body_abs_y.abs() < 0.001) {
            // Body already has absolute coordinates (pre-offset applied by renderer).
            buf.push_str(body_inner);
        } else {
            let shifted = offset_svg_coords(body_inner, body_abs_x, body_abs_y);
            buf.push_str(&shifted);
        }
    }

    // Legend (CENTER-aligned)
    if let Some(ref leg) = meta.legend {
        let leg_wrapper_x = outer_inner_x + cap_inner_x + title_inner_x;
        let leg_wrapper_y = meta_dy + hdr_dim.1 + title_dim.1 + body_h;
        let leg_block_x = ((after_legend.0 - leg_dim.0) / 2.0).max(0.0);
        let rect_x = leg_wrapper_x + leg_block_x + LEGEND_MARGIN;
        let rect_y = leg_wrapper_y + LEGEND_MARGIN;
        let draw_w = leg_text_w + 2.0 * LEGEND_PADDING;
        let draw_h = leg_text_h + 2.0 * LEGEND_PADDING;
        let half_rc = LEGEND_ROUND_CORNER / 2.0;

        let legend_fill = leg_bg_color.as_deref().unwrap_or(LEGEND_BG);
        let text_color = leg_font_color.as_deref().unwrap_or(TEXT_COLOR);

        write!(buf, r#"<g class="legend""#).unwrap();
        // Java: legend includes data-source-line only when no document <style> block is used
        let has_style = leg_bg_color.is_some()
            || title_bg_color.is_some()
            || hdr_bg_color.is_some()
            || ftr_bg_color.is_some()
            || cap_bg_color.is_some();
        if !has_style {
            if let Some(sl) = meta.legend_line {
                write!(buf, r#" data-source-line="{sl}""#).unwrap();
            }
        }
        buf.push('>');
        write!(buf,
            r#"<rect fill="{}" height="{}" rx="{}" ry="{}" style="stroke:{LEGEND_BORDER};stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
            legend_fill, fmt_coord(draw_h), fmt_coord(half_rc), fmt_coord(half_rc),
            fmt_coord(draw_w), fmt_coord(rect_x), fmt_coord(rect_y),
        ).unwrap();
        let text_x = rect_x + LEGEND_PADDING;
        let text_y = rect_y
            + LEGEND_PADDING
            + font_metrics::ascent("SansSerif", leg_font_size, false, false);
        render_creole_text(
            &mut buf,
            leg,
            text_x,
            text_y,
            text_block_h(leg_font_size, false),
            text_color,
            None,
            &format!(r#"font-size="{}""#, leg_font_size as i32),
        );
        if buf.ends_with('\n') {
            buf.pop();
        }
        buf.push_str("</g>");
    }

    // Caption (CENTER-aligned)
    if let Some(ref cap) = meta.caption {
        let cap_y_start = meta_dy + hdr_dim.1 + after_title.1;
        let cap_block_x = meta_dx + outer_inner_x + ((after_caption.0 - cap_dim.0) / 2.0).max(0.0);
        let text_x = cap_block_x + CAPTION_MARGIN + CAPTION_PADDING;
        let text_y = cap_y_start
            + CAPTION_MARGIN
            + CAPTION_PADDING
            + font_metrics::ascent("SansSerif", cap_font_size, false, false);
        let text_color = cap_font_color.as_deref().unwrap_or(TEXT_COLOR);
        write!(buf, r#"<g class="caption""#).unwrap();
        if let Some(sl) = meta.caption_line {
            write!(buf, r#" data-source-line="{sl}""#).unwrap();
        }
        buf.push('>');
        if let Some(ref bg) = cap_bg_color {
            let rect_x = cap_block_x + CAPTION_MARGIN;
            let rect_y = cap_y_start + CAPTION_MARGIN;
            let rect_w = cap_text_w + 2.0 * CAPTION_PADDING;
            let rect_h = cap_text_h + 2.0 * CAPTION_PADDING;
            write!(buf,
                r#"<rect fill="{}" height="{}" style="stroke:none;stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
                bg, fmt_coord(rect_h), fmt_coord(rect_w), fmt_coord(rect_x), fmt_coord(rect_y)
            ).unwrap();
        }
        render_creole_text(
            &mut buf,
            cap,
            text_x,
            text_y,
            text_block_h(cap_font_size, false),
            text_color,
            None,
            &format!(r#"font-size="{}""#, cap_font_size as i32),
        );
        if buf.ends_with('\n') {
            buf.pop();
        }
        buf.push_str("</g>");
    }

    // Footer (CENTER-aligned)
    if let Some(ref ftr) = meta.footer {
        let ftr_y_start = meta_dy + hdr_dim.1 + after_caption.1;
        let ftr_x = ((tb_w - ftr_dim.0) / 2.0).max(0.0);
        let text_y = ftr_y_start + font_metrics::ascent("SansSerif", ftr_font_size, false, false);
        let text_color = ftr_font_color.as_deref().unwrap_or(DIVIDER_COLOR);
        write!(buf, r#"<g class="footer""#).unwrap();
        if let Some(sl) = meta.footer_line {
            write!(buf, r#" data-source-line="{sl}""#).unwrap();
        }
        buf.push('>');
        if let Some(ref bg) = ftr_bg_color {
            write!(buf,
                r#"<rect fill="{}" height="{}" style="stroke:none;stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
                bg, fmt_coord(ftr_text_h), fmt_coord(ftr_text_w), fmt_coord(ftr_x), fmt_coord(ftr_y_start)
            ).unwrap();
        }
        render_creole_text(
            &mut buf,
            ftr,
            ftr_x,
            text_y,
            text_block_h(ftr_font_size, false),
            text_color,
            None,
            &format!(r#"font-size="{}""#, ftr_font_size as i32),
        );
        if buf.ends_with('\n') {
            buf.pop();
        }
        buf.push_str("</g>");
    }

    buf.push_str("</g></svg>");
    Ok(buf)
}

/// Sequence diagram chrome wrapping — mirrors Java's `SequenceDiagramFileMakerPuma2`
/// + `SequenceDiagramArea` composition.  Key differences from the generic
///   `wrap_with_meta` code path:
///
/// 1. Drawing order (top-to-bottom in SVG DOM): title → caption → body → header
///    → footer → legend.  Java draws them in exactly this order (see
///    `SequenceDiagramFileMakerPuma2.createUDrawable`, lines 214-233).
/// 2. Chrome elements (title/caption/header/footer/legend) are rendered as bare
///    `<rect>` + `<text>` without a surrounding `<g class="...">` wrapper.  Java
///    does NOT wrap annotation chrome for sequence because
///    `SequenceDiagram.createImageBuilder` disables `AnnotatedWorker` via
///    `annotations(false)` (SequenceDiagram.java:308-311).
/// 3. Canvas width model: `sequenceWidth = drawableSet.getDimension().width` which,
///    for Puma2, equals `lastParticipant.startX + headWidth + 2*outMargin`.
///    Rust's `svg.rs::BodyResult` builds `raw_body_dim = (sl.total_width - 5,
///    sl.total_height - 10)` from the layout (subtracting doc margins), and
///    `sl.total_width` already equals Java's `freeX`.  So `sequenceWidth =
///    raw_body_dim.0 + 5`.  Empirically `sequenceHeight = raw_body_dim.1 + 2`
///    (the 2px accounts for a layout-vs-render accounting difference).
/// 4. `area.getWidth() = max(sequenceWidth, headerWidth, titleWidth, footerWidth,
///    captionWidth)` — this is what Java's LimitFinder reads as `maxX` because any
///    `TextBlockMarged` inside chrome draws a `UEmpty` spanning its full (margined)
///    dimension.  Right-aligned header in particular ensures `lf.maxX >= area.getWidth()`.
/// 5. Final canvas width = `(int)(area.getWidth() + 1 + margin.left + margin.right + 1)`
///    = `(int)(area.getWidth() + 7)` for Puma2 (margin left=0, right=5) because
///    `getFinalDimension` adds +1 and `SvgGraphics.ensureVisible` adds another +1
///    via `(int)(x + 1)`.
///
/// Y positioning mirrors `SequenceDiagramArea` getters (lines 133-178):
/// ```text
/// headerY   = 0
/// titleY    = headerHeight + headerMargin
/// seqAreaY  = titleY + titleHeight           (+legendHeight if legend-top)
/// legendY   = sequenceHeight + headerHeight + titleHeight
/// captionY  = legendY + legendHeight
/// footerY   = captionY + captionHeight
/// ```
/// The ImageBuilder applies `UTranslate(margin.left, margin.top)` = `(0, 5)` to the
/// whole drawable, so every drawn element gets +5 on Y.  Each chrome element's own
/// margin is added on top (e.g. title margin 5, legend margin 12, caption margin 1).
#[allow(clippy::too_many_arguments)]
pub(super) fn wrap_with_meta_sequence(
    body_svg: &str,
    meta: &DiagramMeta,
    bg: &str,
    raw_body_dim: Option<(f64, f64)>,
    skin: &crate::style::SkinParams,
) -> Result<String> {
    let body_content = extract_svg_content(body_svg);

    // ── Document margins (SequenceDiagram.getDefaultMargins for Puma2 mode) ──
    // Puma2: ClockwiseTopRightBottomLeft(top=5, right=5, bottom=5, left=0).
    let doc_margin_top = 5.0;
    let doc_margin_left = 0.0;
    let doc_margin_right = 5.0;
    let doc_margin_bottom = 5.0;

    // ── Resolve document section styles ─────────────────────────────
    let hdr_font_size = skin
        .get("document.header.fontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(META_HF_FONT_SIZE);
    let hdr_font_color = skin.get("document.header.fontcolor").map(|s| s.to_string());
    let hdr_bg_color = skin
        .get("document.header.backgroundcolor")
        .map(|s| s.to_string());

    let ftr_font_size = skin
        .get("document.footer.fontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(META_HF_FONT_SIZE);
    let ftr_font_color = skin.get("document.footer.fontcolor").map(|s| s.to_string());
    let ftr_bg_color = skin
        .get("document.footer.backgroundcolor")
        .map(|s| s.to_string());

    let title_font_size = skin
        .get("document.title.fontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(META_TITLE_FONT_SIZE);
    let title_font_color = skin.get("document.title.fontcolor").map(|s| s.to_string());
    let title_bg_color = skin
        .get("document.title.backgroundcolor")
        .map(|s| s.to_string());

    let leg_font_size = skin
        .get("document.legend.fontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(META_LEGEND_FONT_SIZE);
    let leg_font_color = skin.get("document.legend.fontcolor").map(|s| s.to_string());
    let leg_bg_color = skin
        .get("document.legend.backgroundcolor")
        .map(|s| s.to_string());

    let cap_font_size = skin
        .get("document.caption.fontsize")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(META_CAPTION_FONT_SIZE);
    let cap_font_color = skin
        .get("document.caption.fontcolor")
        .map(|s| s.to_string());
    let cap_bg_color = skin
        .get("document.caption.backgroundcolor")
        .map(|s| s.to_string());

    let title_bold = title_font_size == META_TITLE_FONT_SIZE;

    // ── Chrome block dimensions (matches Java TextBlockBordered+Marged) ──
    // For each block:
    //   bordered_dim = text + 2*padding + 1  (TextBlockBordered +1)
    //   full_dim     = bordered_dim + 2*margin (TextBlockMarged)
    let hdr_text_w = meta
        .header
        .as_ref()
        .map(|t| creole_text_w(t, hdr_font_size, false))
        .unwrap_or(0.0);
    let hdr_text_h = if meta.header.is_some() {
        text_block_h(hdr_font_size, false)
    } else {
        0.0
    };
    let hdr_dim = if meta.header.is_some() {
        block_dim(hdr_text_w, hdr_text_h, 0.0, 0.0)
    } else {
        (0.0, 0.0)
    };

    let ftr_text_w = meta
        .footer
        .as_ref()
        .map(|t| creole_text_w(t, ftr_font_size, false))
        .unwrap_or(0.0);
    let ftr_text_h = if meta.footer.is_some() {
        text_block_h(ftr_font_size, false)
    } else {
        0.0
    };
    let ftr_dim = if meta.footer.is_some() {
        block_dim(ftr_text_w, ftr_text_h, 0.0, 0.0)
    } else {
        (0.0, 0.0)
    };

    let title_text_w = meta
        .title
        .as_ref()
        .map(|t| {
            creole_table_width(t, title_font_size, title_bold)
                .unwrap_or_else(|| creole_text_w(t, title_font_size, title_bold))
        })
        .unwrap_or(0.0);
    let title_text_h = if let Some(ref t) = meta.title {
        let lh = font_metrics::line_height("SansSerif", title_font_size, title_bold, false);
        let n_lines = t
            .split(crate::NEWLINE_CHAR)
            .flat_map(|s| s.lines())
            .count()
            .max(1);
        let mut h = n_lines as f64 * lh;
        let has_table = t
            .split(crate::NEWLINE_CHAR)
            .flat_map(|s| s.lines())
            .any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with('|') || (trimmed.starts_with('<') && trimmed.contains(">|"))
            });
        if has_table {
            h += 4.0;
        }
        h
    } else {
        0.0
    };
    let title_dim = if meta.title.is_some() {
        block_dim(title_text_w, title_text_h, TITLE_PADDING, TITLE_MARGIN)
    } else {
        (0.0, 0.0)
    };

    let cap_text_w = meta
        .caption
        .as_ref()
        .map(|t| creole_text_w(t, cap_font_size, false))
        .unwrap_or(0.0);
    let cap_text_h = if meta.caption.is_some() {
        text_block_h(cap_font_size, false)
    } else {
        0.0
    };
    let cap_dim = if meta.caption.is_some() {
        block_dim(cap_text_w, cap_text_h, CAPTION_PADDING, CAPTION_MARGIN)
    } else {
        (0.0, 0.0)
    };

    let leg_text_w = meta
        .legend
        .as_ref()
        .map(|t| creole_text_w(t, leg_font_size, false))
        .unwrap_or(0.0);
    let leg_text_h = if let Some(ref leg) = meta.legend {
        crate::render::svg_richtext::compute_creole_note_text_height(leg, leg_font_size)
    } else {
        0.0
    };
    let leg_dim = if meta.legend.is_some() {
        block_dim(leg_text_w, leg_text_h, LEGEND_PADDING, LEGEND_MARGIN)
    } else {
        (0.0, 0.0)
    };

    // ── Body dimensions ──────────────────────────────────────────────
    // `raw_body_dim` for SEQUENCE is populated by `render::svg::body_result`
    // from `SeqLayout.total_{width,height}` minus doc margins:
    //   raw_w = sl.total_width  - DOC_MARGIN_RIGHT   (= total_width - 5)
    //   raw_h = sl.total_height - DOC_MARGIN_TOP - DOC_MARGIN_BOTTOM (= total_height - 10)
    // Empirically (verified by instrumenting Java's `SequenceDiagramFileMakerPuma2`
    // on `sequence/a0006.puml`), `sl.total_width == drawableSet.getDimension().width`
    // == Java's `freeX` (for Puma2 mode).  So:
    //   Java sequenceWidth  = raw_w + 5
    //   Java sequenceHeight = raw_h + 2   (2px layout-vs-render accounting delta)
    let (raw_w, raw_h) = raw_body_dim.unwrap_or((0.0, 0.0));
    let sequence_width = raw_w + 5.0;
    let sequence_height_java = raw_h + 2.0;

    // ── area.getWidth() = max(sequenceWidth, chrome widths) ─────────
    // Each chrome's dim.0 here is the post-margin width (what TextBlockMarged
    // reports via calculateDimension), matching Java's area width inputs.
    let area_width = sequence_width
        .max(hdr_dim.0)
        .max(title_dim.0)
        .max(ftr_dim.0)
        .max(cap_dim.0);

    // ── Y positions in SequenceDiagramArea coordinates (pre margin shift) ──
    // See SequenceDiagramArea.java:133-178.  Legend is assumed non-top
    // (isLegendTop == false) which matches default behaviour when legend has
    // no explicit vertical alignment.  TODO: handle top-aligned legend.
    let is_legend_top = false;
    let header_height = hdr_dim.1;
    let header_margin_internal = 0.0; // initHeader sets headerMargin=0
    let title_height = title_dim.1;
    let legend_height = leg_dim.1;
    let caption_height = cap_dim.1;
    let footer_height = ftr_dim.1;
    let footer_margin_internal = 0.0; // initFooter sets footerMargin=0
    let sequence_height = sequence_height_java;

    let title_y_area = header_height + header_margin_internal;
    let sequence_area_y = if is_legend_top {
        title_y_area + title_height + legend_height
    } else {
        title_y_area + title_height
    };
    let legend_y_area = if is_legend_top {
        title_height + header_height + header_margin_internal
    } else {
        sequence_height + header_height + header_margin_internal + title_height
    };
    let caption_y_area =
        sequence_height + header_height + header_margin_internal + title_height + legend_height;
    let footer_y_area = sequence_height
        + header_height
        + header_margin_internal
        + title_height
        + footer_margin_internal
        + caption_height
        + legend_height;

    // ── Canvas dimensions ────────────────────────────────────────────
    // Java: getFinalDimension = lf.maxX + 1 + margin.left + margin.right.
    // SvgGraphics.ensureVisible(dim) sets maxX = (int)(dim + 1).
    // lf.maxX = area.getWidth() when a header is present (it draws a UEmpty
    // spanning full area width); for other cases the max drawn element sets it.
    // We conservatively use area_width, which is >= any drawn extent.
    let body_end_y = sequence_height
        + header_height
        + header_margin_internal
        + title_height
        + legend_height
        + caption_height
        + footer_height
        + footer_margin_internal;
    let final_dim_w = area_width + 1.0 + doc_margin_left + doc_margin_right;
    let final_dim_h = body_end_y + 1.0 + doc_margin_top + doc_margin_bottom;
    let canvas_w = ensure_visible_int(final_dim_w) as f64;
    let canvas_h = ensure_visible_int(final_dim_h) as f64;

    log::trace!(
        "wrap_with_meta_sequence: sequence_width={sequence_width:.4} area_width={area_width:.4} \
        canvas_w={canvas_w} canvas_h={canvas_h} sequence_height={sequence_height}"
    );

    // ── Render SVG ───────────────────────────────────────────────────
    let mut buf = String::with_capacity(body_svg.len() + 2048);
    write_svg_root_bg(&mut buf, canvas_w, canvas_h, "SEQUENCE", bg);
    if let Some(ref t) = meta.title {
        if !t.is_empty() {
            write_svg_title(&mut buf, t);
        }
    }
    buf.push_str("<defs/><g>");
    write_bg_rect(&mut buf, canvas_w, canvas_h, bg);

    // ── Draw order (matches Java SequenceDiagramFileMakerPuma2.createUDrawable):
    //   1. title, 2. caption, 3. body, 4. header, 5. footer, 6. legend.
    // No <g class="..."> wrappers — Java emits raw rect+text for chrome when
    // annotations(false) is set in the ImageBuilder.

    // 1. Title (CENTER-aligned, drawn at area coords + (0, img_margin_top))
    if let Some(ref title) = meta.title {
        // area.getTitleX() = (getWidth() - titleWidth) / 2; then + title margin (5).
        let title_x_area = ((area_width - title_dim.0) / 2.0).max(0.0);
        let rect_x = doc_margin_left + title_x_area + TITLE_MARGIN;
        let rect_y = doc_margin_top + title_y_area + TITLE_MARGIN;
        let rect_w = title_text_w + 2.0 * TITLE_PADDING;
        let rect_h = title_text_h + 2.0 * TITLE_PADDING;
        let title_fill = title_bg_color.as_deref();
        if let Some(fill) = title_fill {
            write!(
                buf,
                r#"<rect fill="{}" height="{}" style="stroke:none;stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
                fill,
                fmt_coord(rect_h),
                fmt_coord(rect_w),
                fmt_coord(rect_x),
                fmt_coord(rect_y),
            )
            .unwrap();
        }
        let text_x = rect_x + TITLE_PADDING;
        let text_y = rect_y
            + TITLE_PADDING
            + font_metrics::ascent("SansSerif", title_font_size, title_bold, false);
        let text_color = title_font_color.as_deref().unwrap_or(TEXT_COLOR);
        let weight_str = if title_bold {
            r#" font-weight="bold""#
        } else {
            ""
        };
        let outer_attrs = format!(r#"font-size="{}"{}"#, title_font_size as i32, weight_str);
        let title_lines: Vec<String> = title
            .split(crate::NEWLINE_CHAR)
            .flat_map(|s| s.lines())
            .map(|s| s.to_string())
            .collect();
        let has_table = creole_table_width(title, title_font_size, title_bold).is_some();
        if has_table {
            render_creole_display_lines(
                &mut buf,
                &title_lines,
                text_x,
                rect_y + TITLE_PADDING,
                text_color,
                &outer_attrs,
                false,
            );
        } else {
            render_creole_text(
                &mut buf,
                title,
                text_x,
                text_y,
                text_block_h(title_font_size, title_bold),
                text_color,
                None,
                &outer_attrs,
            );
        }
        if buf.ends_with('\n') {
            buf.pop();
        }
    }

    // 2. Caption (CENTER-aligned)
    if let Some(ref cap) = meta.caption {
        let cap_x_area = ((area_width - cap_dim.0) / 2.0).max(0.0);
        let rect_x = doc_margin_left + cap_x_area + CAPTION_MARGIN;
        let rect_y = doc_margin_top + caption_y_area + CAPTION_MARGIN;
        let rect_w = cap_text_w + 2.0 * CAPTION_PADDING;
        let rect_h = cap_text_h + 2.0 * CAPTION_PADDING;
        if let Some(ref fill) = cap_bg_color {
            write!(
                buf,
                r#"<rect fill="{}" height="{}" style="stroke:none;stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
                fill,
                fmt_coord(rect_h),
                fmt_coord(rect_w),
                fmt_coord(rect_x),
                fmt_coord(rect_y),
            )
            .unwrap();
        }
        let text_x = rect_x + CAPTION_PADDING;
        let text_y = rect_y
            + CAPTION_PADDING
            + font_metrics::ascent("SansSerif", cap_font_size, false, false);
        let text_color = cap_font_color.as_deref().unwrap_or(TEXT_COLOR);
        render_creole_text(
            &mut buf,
            cap,
            text_x,
            text_y,
            text_block_h(cap_font_size, false),
            text_color,
            None,
            &format!(r#"font-size="{}""#, cap_font_size as i32),
        );
        if buf.ends_with('\n') {
            buf.pop();
        }
    }

    // 3. Body (rendered by svg_sequence::render_sequence).  Java draws at
    //    (sequenceAreaX + delta1/2, sequenceAreaY).  For Puma2 with no wide
    //    legend, delta1=0 and sequenceAreaX = (area_width - sequenceWidth)/2.
    let body_inner = body_content
        .strip_prefix("<defs/><g>")
        .unwrap_or(&body_content);
    let body_inner = body_inner.strip_suffix("</g>").unwrap_or(body_inner);
    let body_inner = if body_inner.starts_with("<rect fill=\"") {
        if let Some(end) = body_inner.find("/>") {
            let rect_tag = &body_inner[..end + 2];
            if rect_tag.contains("stroke:none")
                && rect_tag.contains("x=\"0\"")
                && rect_tag.contains("y=\"0\"")
            {
                &body_inner[end + 2..]
            } else {
                body_inner
            }
        } else {
            body_inner
        }
    } else {
        body_inner
    };
    let delta1 = (leg_dim.0 - area_width).max(0.0);
    let sequence_area_x = ((area_width - sequence_width) / 2.0).max(0.0);
    // Rust's svg_sequence already bakes in a 5px top/left margin into the body
    // internal coordinates (layout MARGIN=5).  This coincidentally equals Java's
    // ImageBuilder top margin (=5) and left margin (=0).  So we DO NOT add
    // doc_margin_top here — the internal +5 already provides the image margin
    // shift.  For X, doc_margin_left=0 so it doesn't matter.
    let body_abs_x = doc_margin_left + sequence_area_x + delta1 / 2.0;
    let body_abs_y = sequence_area_y;
    if !body_inner.trim().is_empty() {
        if body_abs_x.abs() < 0.001 && body_abs_y.abs() < 0.001 {
            buf.push_str(body_inner);
        } else {
            let shifted = offset_svg_coords(body_inner, body_abs_x, body_abs_y);
            buf.push_str(&shifted);
        }
    }

    // 4. Header (RIGHT-aligned)
    if let Some(ref hdr) = meta.header {
        // area.getHeaderX(RIGHT) = getWidth() - headerWidth.  Header has no margin/pad.
        let hdr_x_area = area_width - hdr_dim.0;
        let rect_x = doc_margin_left + hdr_x_area;
        let rect_y = doc_margin_top + 0.0; // headerY = 0
        if let Some(ref fill) = hdr_bg_color {
            write!(
                buf,
                r#"<rect fill="{}" height="{}" style="stroke:none;stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
                fill,
                fmt_coord(hdr_text_h),
                fmt_coord(hdr_text_w),
                fmt_coord(rect_x),
                fmt_coord(rect_y),
            )
            .unwrap();
        }
        let text_y = rect_y + font_metrics::ascent("SansSerif", hdr_font_size, false, false);
        let text_color = hdr_font_color.as_deref().unwrap_or(DIVIDER_COLOR);
        render_creole_text(
            &mut buf,
            hdr,
            rect_x,
            text_y,
            text_block_h(hdr_font_size, false),
            text_color,
            None,
            &format!(r#"font-size="{}""#, hdr_font_size as i32),
        );
        if buf.ends_with('\n') {
            buf.pop();
        }
    }

    // 5. Footer (CENTER-aligned by default)
    if let Some(ref ftr) = meta.footer {
        let ftr_x_area = ((area_width - ftr_dim.0) / 2.0).max(0.0);
        let rect_x = doc_margin_left + ftr_x_area;
        let rect_y = doc_margin_top + footer_y_area;
        if let Some(ref fill) = ftr_bg_color {
            write!(
                buf,
                r#"<rect fill="{}" height="{}" style="stroke:none;stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
                fill,
                fmt_coord(ftr_text_h),
                fmt_coord(ftr_text_w),
                fmt_coord(rect_x),
                fmt_coord(rect_y),
            )
            .unwrap();
        }
        let text_y = rect_y + font_metrics::ascent("SansSerif", ftr_font_size, false, false);
        let text_color = ftr_font_color.as_deref().unwrap_or(DIVIDER_COLOR);
        render_creole_text(
            &mut buf,
            ftr,
            rect_x,
            text_y,
            text_block_h(ftr_font_size, false),
            text_color,
            None,
            &format!(r#"font-size="{}""#, ftr_font_size as i32),
        );
        if buf.ends_with('\n') {
            buf.pop();
        }
    }

    // 6. Legend (CENTER-aligned by default; rounded rect + border)
    if let Some(ref leg) = meta.legend {
        let leg_x_area = ((area_width - leg_dim.0) / 2.0).max(0.0);
        let rect_x = doc_margin_left + leg_x_area + LEGEND_MARGIN;
        let rect_y = doc_margin_top + legend_y_area + LEGEND_MARGIN;
        let draw_w = leg_text_w + 2.0 * LEGEND_PADDING;
        let draw_h = leg_text_h + 2.0 * LEGEND_PADDING;
        let half_rc = LEGEND_ROUND_CORNER / 2.0;
        let legend_fill = leg_bg_color.as_deref().unwrap_or(LEGEND_BG);
        let text_color = leg_font_color.as_deref().unwrap_or(TEXT_COLOR);
        write!(
            buf,
            r#"<rect fill="{}" height="{}" rx="{}" ry="{}" style="stroke:{LEGEND_BORDER};stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
            legend_fill,
            fmt_coord(draw_h),
            fmt_coord(half_rc),
            fmt_coord(half_rc),
            fmt_coord(draw_w),
            fmt_coord(rect_x),
            fmt_coord(rect_y),
        )
        .unwrap();
        let text_x = rect_x + LEGEND_PADDING;
        let text_y = rect_y
            + LEGEND_PADDING
            + font_metrics::ascent("SansSerif", leg_font_size, false, false);
        render_creole_text(
            &mut buf,
            leg,
            text_x,
            text_y,
            text_block_h(leg_font_size, false),
            text_color,
            None,
            &format!(r#"font-size="{}""#, leg_font_size as i32),
        );
        if buf.ends_with('\n') {
            buf.pop();
        }
    }

    buf.push_str("</g></svg>");
    Ok(buf)
}

/// Shift all coordinate attributes in SVG content by (dx, dy).
/// Java renders body at absolute coordinates; this replaces <g transform="translate">.
pub(super) fn offset_svg_coords(svg: &str, dx: f64, dy: f64) -> String {
    use regex::Regex;
    use std::sync::OnceLock;

    // Match position attributes: x="N", y="N", cx="N", cy="N", x1="N", y1="N", x2="N", y2="N"
    static RE_X: OnceLock<Regex> = OnceLock::new();
    static RE_Y: OnceLock<Regex> = OnceLock::new();
    static RE_POINTS: OnceLock<Regex> = OnceLock::new();
    static RE_PATH_D: OnceLock<Regex> = OnceLock::new();

    let re_x = RE_X.get_or_init(|| {
        Regex::new(r#"(?P<attr>(?:^| )(?:x|cx|x1|x2))="(?P<val>-?[\d.]+)""#).unwrap()
    });
    let re_y = RE_Y
        .get_or_init(|| Regex::new(r#"(?P<attr> (?:y|cy|y1|y2))="(?P<val>-?[\d.]+)""#).unwrap());
    let re_points = RE_POINTS.get_or_init(|| Regex::new(r#"points="([^"]*)""#).unwrap());
    let re_path_d = RE_PATH_D.get_or_init(|| Regex::new(r#" d="([^"]*)""#).unwrap());

    let mut result = svg.to_string();

    // Shift x-coordinate attributes
    result = re_x
        .replace_all(&result, |caps: &regex::Captures| {
            let attr = &caps["attr"];
            let val: f64 = caps["val"].parse().unwrap_or(0.0);
            format!("{}=\"{}\"", attr, fmt_coord(val + dx))
        })
        .to_string();

    // Shift y-coordinate attributes
    result = re_y
        .replace_all(&result, |caps: &regex::Captures| {
            let attr = &caps["attr"];
            let val: f64 = caps["val"].parse().unwrap_or(0.0);
            format!("{}=\"{}\"", attr, fmt_coord(val + dy))
        })
        .to_string();

    // Shift polygon points="x,y x,y ..."
    result = re_points
        .replace_all(&result, |caps: &regex::Captures| {
            let points = &caps[1];
            let shifted: Vec<String> = points
                .split(',')
                .collect::<Vec<_>>()
                .chunks(2)
                .filter_map(|pair| {
                    if pair.len() == 2 {
                        let x: f64 = pair[0].trim().parse().unwrap_or(0.0);
                        let y: f64 = pair[1].trim().parse().unwrap_or(0.0);
                        Some(format!("{},{}", fmt_coord(x + dx), fmt_coord(y + dy)))
                    } else {
                        None
                    }
                })
                .collect();
            format!("points=\"{}\"", shifted.join(","))
        })
        .to_string();

    // Shift path d="M x,y L x,y C x,y x,y x,y ..."
    result = re_path_d
        .replace_all(&result, |caps: &regex::Captures| {
            let d = &caps[1];
            let shifted = offset_path_data(d, dx, dy);
            format!(" d=\"{}\"", shifted)
        })
        .to_string();

    result
}

/// Offset all coordinates in an SVG path data string by (dx, dy).
///
/// SVG-path-command-aware: correctly handles arc commands (A/a) where
/// rx, ry, x-rotation, and flags must NOT be offset.
fn offset_path_data(d: &str, dx: f64, dy: f64) -> String {
    let mut result = String::with_capacity(d.len());
    let mut chars = d.chars().peekable();
    let mut cmd = ' ';

    while chars.peek().is_some() {
        // Skip whitespace
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            result.push(chars.next().unwrap());
        }
        if chars.peek().is_none() {
            break;
        }

        let c = *chars.peek().unwrap();
        if c.is_alphabetic() {
            cmd = chars.next().unwrap();
            result.push(cmd);
            continue;
        }

        match cmd.to_ascii_uppercase() {
            'Z' => {
                // No parameters
                if let Some(ch) = chars.next() {
                    result.push(ch);
                }
            }
            'H' => {
                // Horizontal line: 1 x-value
                if let Some(x) = parse_path_number(&mut chars) {
                    result.push_str(&fmt_coord(x + dx));
                }
            }
            'V' => {
                // Vertical line: 1 y-value
                if let Some(y) = parse_path_number(&mut chars) {
                    result.push_str(&fmt_coord(y + dy));
                }
            }
            'A' => {
                // Arc: rx,ry x-rotation large-arc-flag sweep-flag x,y
                // rx,ry and rotation/flags are NOT offset
                if let Some(rx) = parse_path_number(&mut chars) {
                    result.push_str(&fmt_coord(rx)); // rx (no offset)
                    skip_path_sep(&mut chars, &mut result);
                    if let Some(ry) = parse_path_number(&mut chars) {
                        result.push_str(&fmt_coord(ry)); // ry (no offset)
                        skip_path_sep(&mut chars, &mut result);
                        if let Some(rot) = parse_path_number(&mut chars) {
                            result.push_str(&fmt_coord(rot)); // x-rotation (no offset)
                            skip_path_sep(&mut chars, &mut result);
                            if let Some(la) = parse_path_number(&mut chars) {
                                result.push_str(&fmt_coord(la)); // large-arc-flag (no offset)
                                skip_path_sep(&mut chars, &mut result);
                                if let Some(sw) = parse_path_number(&mut chars) {
                                    result.push_str(&fmt_coord(sw)); // sweep-flag (no offset)
                                    skip_path_sep(&mut chars, &mut result);
                                    if let Some(x) = parse_path_number(&mut chars) {
                                        result.push_str(&fmt_coord(x + dx)); // endpoint x
                                        skip_path_sep(&mut chars, &mut result);
                                        if let Some(y) = parse_path_number(&mut chars) {
                                            result.push_str(&fmt_coord(y + dy));
                                            // endpoint y
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            'C' => {
                // Cubic bezier: x1,y1 x2,y2 x,y (3 pairs)
                for _ in 0..3 {
                    if let Some(x) = parse_path_number(&mut chars) {
                        result.push_str(&fmt_coord(x + dx));
                        skip_path_sep(&mut chars, &mut result);
                        if let Some(y) = parse_path_number(&mut chars) {
                            result.push_str(&fmt_coord(y + dy));
                            skip_path_sep(&mut chars, &mut result);
                        }
                    }
                }
            }
            'S' | 'Q' => {
                // Smooth cubic / quadratic: 2 pairs
                for _ in 0..2 {
                    if let Some(x) = parse_path_number(&mut chars) {
                        result.push_str(&fmt_coord(x + dx));
                        skip_path_sep(&mut chars, &mut result);
                        if let Some(y) = parse_path_number(&mut chars) {
                            result.push_str(&fmt_coord(y + dy));
                            skip_path_sep(&mut chars, &mut result);
                        }
                    }
                }
            }
            _ => {
                // M, L, T and others: 1 coordinate pair
                if let Some(x) = parse_path_number(&mut chars) {
                    result.push_str(&fmt_coord(x + dx));
                    skip_path_sep(&mut chars, &mut result);
                    if let Some(y) = parse_path_number(&mut chars) {
                        result.push_str(&fmt_coord(y + dy));
                    }
                } else if let Some(ch) = chars.next() {
                    result.push(ch);
                }
            }
        }
    }
    result
}

fn parse_path_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<f64> {
    let mut s = String::new();
    if chars.peek() == Some(&'-') {
        s.push(chars.next().unwrap());
    }
    while chars
        .peek()
        .is_some_and(|c| c.is_ascii_digit() || *c == '.')
    {
        s.push(chars.next().unwrap());
    }
    if s.is_empty() || s == "-" {
        None
    } else {
        s.parse().ok()
    }
}

fn skip_path_sep(chars: &mut std::iter::Peekable<std::str::Chars>, result: &mut String) {
    while chars.peek().is_some_and(|c| *c == ',' || c.is_whitespace()) {
        result.push(chars.next().unwrap());
    }
}
