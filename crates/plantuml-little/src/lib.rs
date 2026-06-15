#![allow(clippy::too_many_arguments)]
// Lints added by rustc/clippy stable 1.95. These fire on idiomatic
// pattern-in-match and `.into()` usage that the codebase relies on
// heavily; rewriting 200+ call sites is out of scope for the
// graphviz-anywhere swap.
#![allow(clippy::collapsible_match)]
#![allow(clippy::useless_conversion)]

use std::path::Path;

pub mod abel;
pub mod decoration;
pub mod dot;
pub mod error;
// Historical note: the legacy `pub use font_metrics_core::static_dejavu::
// font_data;` re-export under `crate::font_data` was dropped on
// 2026-05-10 along with the offline-baked range tables themselves.
// The one direct caller (`layout::flow`'s serif helpers) was already
// routing through the `Metrics` trait via `crate::font_metrics`; that
// path remains the only supported way to reach raw face data.
pub mod font_metrics;
pub mod klimt;
pub mod layout;
pub mod model;
pub mod openiconic;
pub mod parser;
pub mod preproc;
pub mod render;
pub mod semantic;
pub mod skin;
pub mod style;
pub mod svek;
pub mod tim;

pub use error::{Error, Result};
pub use semantic::PlantumlEngine;

/// Private-use Unicode character for `%newline()` / `%n()` expansion.
///
/// Java PlantUML uses U+E100 to keep the semantic newline from `%newline()`
/// distinct from the literal `\n` two-char escape in source text.  Downstream
/// renderers and layout code recognise this character as a line break.
pub const NEWLINE_CHAR: char = '\u{E100}';

/// Convert PlantUML text to an SVG string
pub fn convert(puml_source: &str) -> Result<String> {
    let cwd = std::env::current_dir().ok();
    let expanded = if let Some(base_dir) = cwd.as_deref() {
        preproc::preprocess_with_base_dir(puml_source, base_dir)?
    } else {
        preproc::preprocess(puml_source)?
    };
    render_expanded(puml_source, &expanded, None)
}

/// Convert PlantUML text to SVG using an explicit base directory for relative
/// preprocessor includes.
pub fn convert_with_base_dir(puml_source: &str, base_dir: &Path) -> Result<String> {
    let expanded = preproc::preprocess_with_base_dir(puml_source, base_dir)?;
    render_expanded(puml_source, &expanded, None)
}

/// Convert PlantUML text to SVG using the original input file path.
/// This preserves filename/dirpath preprocessor context.
pub fn convert_with_input_path(puml_source: &str, input_path: &Path) -> Result<String> {
    let expanded = preproc::preprocess_with_source_path(puml_source, input_path)?;
    render_expanded(puml_source, &expanded, Some(input_path))
}

fn render_expanded(
    original_source: &str,
    expanded: &str,
    input_path: Option<&Path>,
) -> Result<String> {
    // Java emits one SVG per @startuml block. When the source has multiple
    // standard @startuml/@enduml blocks (no @startwbs/@startsalt mixed in),
    // detect them and render each separately, concatenating the SVGs.
    let blocks = split_uml_only_blocks(original_source);
    let expanded_blocks = split_uml_only_blocks(expanded);
    if blocks.len() > 1 && blocks.len() == expanded_blocks.len() {
        let mut combined = String::new();
        for (orig_blk, exp_blk) in blocks.iter().zip(expanded_blocks.iter()) {
            let svg = render_one_block(orig_blk, exp_blk, input_path)?;
            combined.push_str(&svg);
        }
        return Ok(combined);
    }
    render_one_block(original_source, expanded, input_path)
}

/// Split source into @startuml/@enduml blocks. Returns single-element vector
/// if any non-uml block (@startwbs etc) is present, since those have
/// different reference handling.
fn split_uml_only_blocks(source: &str) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in source.lines() {
        let trimmed = line.trim();
        // Bail out: any non-@startuml block disqualifies multi-block mode.
        if trimmed.starts_with("@start") && !trimmed.starts_with("@startuml") {
            return vec![source.to_string()];
        }
        if trimmed.starts_with("@startuml") {
            current = Some(String::new());
        }
        if let Some(buf) = current.as_mut() {
            buf.push_str(line);
            buf.push('\n');
        }
        if trimmed.starts_with("@enduml") {
            if let Some(buf) = current.take() {
                blocks.push(buf);
            }
        }
    }
    if blocks.len() <= 1 {
        vec![source.to_string()]
    } else {
        blocks
    }
}

/// Convert PlantUML text to SVG WITHOUT running the preprocessor.
///
/// Java's embedded subdiagram renderer (`{{ }}` blocks) does NOT invoke the
/// preprocessor on the inner content.  Directives like `!theme` are therefore
/// unrecognised syntax and produce the "Welcome to PlantUML" error page.
/// This function replicates that behaviour.
pub(crate) fn convert_no_preproc(puml_source: &str) -> Result<String> {
    render_expanded(puml_source, puml_source, None)
}

fn render_one_block(
    original_source: &str,
    expanded: &str,
    input_path: Option<&Path>,
) -> Result<String> {
    // Extract SVG sprite definitions before parsing (sprite lines would confuse parsers)
    let (cleaned, sprites, gray_data) = parser::common::extract_sprites(expanded);
    render::svg_richtext::set_sprites(sprites);
    render::svg_richtext::set_sprite_gray_data(gray_data);
    // Use a guard to ensure sprites are cleared even if rendering panics
    struct SpriteGuard;
    impl Drop for SpriteGuard {
        fn drop(&mut self) {
            crate::render::svg_richtext::clear_sprites();
        }
    }
    let _guard = SpriteGuard;
    render_cleaned(original_source, &cleaned, expanded, input_path)
}

fn render_cleaned(
    original_source: &str,
    source: &str,
    meta_source: &str,
    input_path: Option<&Path>,
) -> Result<String> {
    // Set the source-seeded SVG id early, before layout, because layout may
    // trigger richtext rendering that registers back-highlight filter ids.
    klimt::svg::set_svg_id_seed_override(Some(klimt::svg::java_source_seed(original_source)));
    struct EarlySeedGuard;
    impl Drop for EarlySeedGuard {
        fn drop(&mut self) {
            crate::klimt::svg::set_svg_id_seed_override(None);
        }
    }
    let _early_seed = EarlySeedGuard;

    let diagram = match parser::parse_with_original(source, Some(original_source)) {
        Ok(diagram) => diagram,
        Err(crate::Error::JavaErrorPage { line, message }) => {
            return crate::render::error_page::render_compact_error_svg(
                meta_source,
                input_path,
                line,
                &message,
            )
        }
        Err(crate::Error::UnsupportedReleasePage) => {
            return crate::render::error_page::render_unsupported_release_svg(meta_source)
        }
        Err(err) => return Err(err),
    };
    let skin = style::parse_skinparams(source);
    let diagram_layout = layout::layout(&diagram, &skin)?;
    let mut meta = parser::common::parse_meta_with_original(meta_source, Some(original_source));
    enrich_meta_source_lines(&mut meta, meta_source);
    let svg = render::svg::render_with_source(
        &diagram,
        &diagram_layout,
        &skin,
        &meta,
        Some(original_source),
    )?;
    Ok(svg)
}
fn enrich_meta_source_lines(meta: &mut model::DiagramMeta, source: &str) {
    for (i, line) in source.lines().enumerate() {
        let t = line.trim();
        if meta.header.is_some()
            && meta.header_line.is_none()
            && (t.starts_with("header ") || t == "header")
        {
            meta.header_line = Some(i);
        }
        if meta.title.is_some()
            && meta.title_line.is_none()
            && (t.starts_with("title ") || t == "title")
        {
            meta.title_line = Some(i);
        }
        if meta.footer.is_some()
            && meta.footer_line.is_none()
            && (t.starts_with("footer ") || t == "footer")
        {
            meta.footer_line = Some(i);
        }
        if meta.caption.is_some() && meta.caption_line.is_none() && t.starts_with("caption ") {
            meta.caption_line = Some(i);
        }
        if meta.legend.is_some()
            && meta.legend_line.is_none()
            && t.starts_with("legend")
            && (t.len() == 6 || t.as_bytes().get(6) == Some(&b' '))
        {
            meta.legend_line = Some(i);
        }
    }
}
