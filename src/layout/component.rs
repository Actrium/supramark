//! Component diagram layout engine.
//!
//! Converts a `ComponentDiagram` into a fully positioned `ComponentLayout`
//! ready for SVG rendering. Uses Graphviz/Smetana for node positioning.

use std::collections::HashMap;

use crate::font_metrics;
use crate::layout::graphviz::{
    self, LayoutClusterSpec, LayoutEdge, LayoutGraph, LayoutNode, RankDir,
};
use crate::model::component::{ComponentDiagram, ComponentEntity, ComponentKind, ComponentLink};
use crate::model::Direction;
use crate::render::svg::{compute_viewport, ViewportConfig};
use crate::svek::node::EntityPosition;
use crate::svek::shape_type::ShapeType;
use crate::Result;

// ---------------------------------------------------------------------------
// Layout output types
// ---------------------------------------------------------------------------

/// Fully positioned component diagram ready for rendering.
#[derive(Debug)]
pub struct ComponentLayout {
    pub nodes: Vec<ComponentNodeLayout>,
    pub edges: Vec<ComponentEdgeLayout>,
    pub notes: Vec<ComponentNoteLayout>,
    pub groups: Vec<ComponentGroupLayout>,
    pub width: f64,
    pub height: f64,
}

/// A single positioned component/rectangle/node/etc.
#[derive(Debug, Clone)]
pub struct ComponentNodeLayout {
    pub id: String,
    pub name: String,
    /// Java "code": alias if given, else display name.
    pub code: String,
    pub kind: ComponentKind,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub description: Vec<String>,
    pub source_line: Option<usize>,
    pub stereotype: Option<String>,
    /// Full stereotype list (C4 stdlib: `<<container>>`, `<<system>>`, etc.).
    /// The first element equals `stereotype` for backwards compatibility.
    pub stereotypes: Vec<String>,
    pub color: Option<String>,
}

/// An edge between two components.
#[derive(Debug, Clone)]
pub struct ComponentEdgeLayout {
    pub from: String,
    pub to: String,
    pub points: Vec<(f64, f64)>,
    pub raw_path_d: Option<String>,
    pub label: String,
    pub dashed: bool,
    /// True when the DOT edge direction was inverted from the original link direction.
    /// Java: LinkType.looksLikeRevertedForSvg() — controls "reverse link" SVG comment.
    pub reversed_for_svg: bool,
    /// Label center position from graphviz/svek solve (x, y).
    pub label_xy: Option<(f64, f64)>,
    /// Head-side arrow decoration (Java `LinkDecor`).  Currently only
    /// `Arrow` (the default rhombus) and `ArrowTriangle` (`>>`,
    /// hollow 4-point triangle from C4 stdlib `Rel(...)`) are
    /// distinguished here — other decorations still go through the
    /// legacy hard-coded path.
    pub head_decoration: crate::svek::edge::LinkDecoration,
}

/// A positioned note.
#[derive(Debug, Clone)]
pub struct ComponentNoteLayout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub text: String,
    pub position: String,
    pub target: Option<String>,
    /// Y coordinate of the connector ear tip (points towards the target entity).
    /// For "top" notes, this is below the note body; for "bottom" notes, above.
    pub ear_tip_y: Option<f64>,
    /// X coordinate of the connector ear tip center.
    pub ear_tip_x: Option<f64>,
    /// Qualified name for the entity group wrapper (e.g. "GMN3").
    pub qualified_name: String,
    /// Source line of the note command in the PlantUML source.
    pub source_line: Option<usize>,
    /// Pre-rendered embedded diagram (`{{ }}` block) as base64 data URI with dimensions.
    /// `(data_uri, width, height, text_before, text_after)`
    pub embedded: Option<EmbeddedDiagramData>,
}

/// Data for a rendered embedded diagram inside a note.
#[derive(Debug, Clone)]
pub struct EmbeddedDiagramData {
    /// Base64 data URI (`data:image/svg+xml;base64,...`).
    pub data_uri: String,
    /// Width of the embedded SVG image.
    pub width: f64,
    /// Height of the embedded SVG image.
    pub height: f64,
    /// Text lines before the `{{ }}` block.
    pub text_before: String,
    /// Text lines after the `}}` block.
    pub text_after: String,
}

/// A positioned group (rectangle container).
#[derive(Debug, Clone)]
pub struct ComponentGroupLayout {
    pub id: String,
    pub name: String,
    /// Java "code": alias if given, else display name.
    pub code: String,
    pub kind: ComponentKind,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub source_line: Option<usize>,
    pub stereotype: Option<String>,
    /// Full stereotype list (C4 stdlib produces chained stereotypes
    /// like `<<system_boundary>><<boundary>>`). The first element equals
    /// `stereotype` for backwards compatibility.
    pub stereotypes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const FONT_SIZE: f64 = 14.0;
// Java: line_height = (ascent + descent) from AWT FontMetrics for SansSerif 14pt
const LINE_HEIGHT: f64 = 16.296875; // (1901 + 483) / 2048 * 14 — exact
                                    // Java: component node padding = 15px top + 15px bottom
const PADDING: f64 = 15.0;
// Java: no explicit minimum width for components; the name + icon determines width
const NODE_MIN_WIDTH: f64 = 0.0;
// Java has no global minimum entity height — each type's size is purely
// determined by its margins + text content (from USymbol.Margin.addDimension).
// Java Smetana: nodesep ≈ 35px (0.486111 inches * 72)
#[allow(dead_code)] // Java-ported layout constant
const NODE_SPACING_X: f64 = 35.0;
#[allow(dead_code)] // Java-ported layout constant
const NODE_SPACING_Y: f64 = 50.0;
#[allow(dead_code)] // Java-ported layout constant
const GROUP_PADDING: f64 = 20.0;
#[allow(dead_code)] // Java-ported layout constant
const GROUP_HEADER: f64 = 30.0;
const NOTE_OFFSET: f64 = 20.0;
#[allow(dead_code)] // Java-ported layout constant
const NOTE_MAX_WIDTH: f64 = 200.0;
const MARGIN: f64 = 7.0;
#[allow(dead_code)] // Java-ported layout constant
const GRID_COLS: usize = 3;

// ---------------------------------------------------------------------------
// Text measurement helpers
// ---------------------------------------------------------------------------

fn text_width(text: &str) -> f64 {
    font_metrics::text_width(text, "SansSerif", FONT_SIZE, false, false)
}

/// Get sprite dimensions if the stereotype references a registered sprite.
/// Returns (width, height) of the sprite, or None if not a sprite.
fn sprite_stereo_dimensions(stereotype: &str) -> Option<(f64, f64)> {
    if !stereotype.starts_with('$') {
        return None;
    }
    let sprite_name = &stereotype[1..];
    let svg = crate::render::svg_richtext::get_sprite_svg(sprite_name)?;
    let info = crate::render::svg_sprite::sprite_info(&svg);
    Some((info.vb_width, info.vb_height))
}

/// Returns (supp_height, supp_width) for cluster labels of the given entity kind.
/// Java: USymbol.suppHeightBecauseOfShape() / suppWidthBecauseOfShape()
/// These values are added to the cluster label dimension in ClusterHeader.
fn cluster_supp_for_shape(kind: &ComponentKind) -> (f64, f64) {
    match kind {
        ComponentKind::Node => (5.0, 60.0),
        ComponentKind::Database => (15.0, 0.0),
        _ => (0.0, 0.0),
    }
}

/// Returns (margin_left, margin_right, margin_top, margin_bottom) for each entity kind.
/// Values from Java's USymbol subclasses `getMargin()` methods.
pub fn entity_margins(kind: &ComponentKind) -> (f64, f64, f64, f64) {
    match kind {
        ComponentKind::Card => (10.0, 10.0, 3.0, 3.0),
        // USymbolComponent2: Margin(10+5, 20+5, 15+5, 5+5)
        ComponentKind::Component => (15.0, 25.0, 20.0, 10.0),
        ComponentKind::Rectangle => (10.0, 10.0, 10.0, 10.0),
        ComponentKind::Interface => (10.0, 10.0, 10.0, 10.0),
        ComponentKind::Cloud => (15.0, 15.0, 15.0, 15.0),
        ComponentKind::Database => (10.0, 10.0, 24.0, 5.0),
        ComponentKind::Node => (15.0, 25.0, 20.0, 10.0),
        ComponentKind::Package => (10.0, 10.0, 10.0, 10.0),
        ComponentKind::Artifact => (10.0, 20.0, 13.0, 10.0),
        ComponentKind::Storage => (10.0, 10.0, 10.0, 10.0),
        ComponentKind::Folder => (10.0, 20.0, 13.0, 10.0),
        ComponentKind::Frame => (15.0, 25.0, 20.0, 10.0),
        ComponentKind::Agent => (10.0, 10.0, 10.0, 10.0),
        ComponentKind::Archimate => (10.0, 10.0, 10.0, 10.0),
        ComponentKind::Stack => (25.0, 25.0, 10.0, 10.0),
        ComponentKind::Queue => (5.0, 15.0, 5.0, 5.0),
        ComponentKind::PortIn | ComponentKind::PortOut => (0.0, 0.0, 0.0, 0.0),
        // Actor and UseCase are handled via special sizing in estimate_entity_size
        ComponentKind::Actor => (0.0, 0.0, 0.0, 0.0),
        ComponentKind::UseCase => (0.0, 0.0, 0.0, 0.0),
    }
}

/// Parse a C4 entity name line into its effective font properties.
/// Returns (display_text, font_size, bold, italic).
pub fn parse_c4_line_props(line: &str) -> (&str, f64, bool, bool) {
    // Check for `== Heading` (creole heading order 1 → FONT_SIZE+2, bold)
    if let Some(rest) = line.strip_prefix("== ").or_else(|| line.strip_prefix("==")) {
        return (rest.trim(), FONT_SIZE + 2.0, true, false);
    }
    // Check for `//<size:N>text</size>//` (italic + explicit size)
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix("//")
        .and_then(|s| s.strip_suffix("//"))
        .unwrap_or(trimmed);
    if let Some(after_size) = inner.strip_prefix("<size:") {
        if let Some(end) = after_size.find('>') {
            let size_str = &after_size[..end];
            if let Ok(sz) = size_str.parse::<f64>() {
                let rest = &after_size[end + 1..];
                let text = rest.strip_suffix("</size>").unwrap_or(rest);
                let is_italic = trimmed.starts_with("//") && trimmed.ends_with("//");
                return (text, sz, false, is_italic);
            }
        }
    }
    // Check for italic wrapper `//text//`
    if let Some(inner) = trimmed
        .strip_prefix("//")
        .and_then(|s| s.strip_suffix("//"))
    {
        return (inner, FONT_SIZE, false, true);
    }
    (trimmed, FONT_SIZE, false, false)
}

/// Count how many wrapped lines a text segment produces at the given font,
/// and the maximum line width. Returns (num_lines, max_line_width).
fn wrapped_line_metrics(
    text: &str,
    font_size: f64,
    bold: bool,
    italic: bool,
    max_w: f64,
) -> (usize, f64) {
    let words: Vec<&str> = text.split(' ').collect();
    if words.is_empty() {
        return (1, 0.0);
    }
    let space_w = font_metrics::char_width(' ', "SansSerif", font_size, bold, italic);
    let mut lines = 1usize;
    let mut cur_w = 0.0_f64;
    let mut max_line_w = 0.0_f64;

    for (i, word) in words.iter().enumerate() {
        let ww = font_metrics::text_width(word, "SansSerif", font_size, bold, italic);
        let needed = if i == 0 || cur_w == 0.0 {
            ww
        } else {
            space_w + ww
        };
        if cur_w > 0.0 && cur_w + needed > max_w {
            max_line_w = max_line_w.max(cur_w);
            lines += 1;
            cur_w = ww;
        } else {
            cur_w += needed;
        }
    }
    max_line_w = max_line_w.max(cur_w);
    (lines, max_line_w)
}

/// Estimate the size of a component entity.
fn estimate_entity_size(entity: &ComponentEntity, wrap_width: Option<f64>) -> (f64, f64) {
    // Ports are small: 12x12 square (Java EntityPosition.RADIUS * 2)
    // The text label is rendered outside the graphviz node, so the DOT node is just the port square.
    if matches!(entity.kind, ComponentKind::PortIn | ComponentKind::PortOut) {
        let port_size: f64 = 12.0;
        return (port_size, port_size);
    }

    // Actor: Java ActorStickMan dimensions + label below.
    // Java USymbolSimpleAbstract.calculateDimension = mergeLayoutT12B3(stereo, actor, label)
    // ActorStickMan: width = max(armsLength,legsX)*2+2*thickness = 26+1 = 27
    //               height = headDiam+bodyLength+legsY+2*thickness+shadow+1 = 16+27+15+1+0+1 = 60
    if entity.kind == ComponentKind::Actor {
        const ACTOR_FIG_W: f64 = 27.0;
        const ACTOR_FIG_H: f64 = 60.0;
        let label_w = text_width(&entity.name);
        let label_h = LINE_HEIGHT;
        let w = ACTOR_FIG_W.max(label_w);
        let h = ACTOR_FIG_H + label_h;
        log::debug!(
            "estimate_entity_size: ACTOR name={:?} label_w={:.4} w={:.4} h={:.4}",
            entity.name,
            label_w,
            w,
            h
        );
        return (w, h);
    }

    // UseCase: Java TextBlockInEllipse computes a smallest-enclosing-ellipse
    // around the text bounding rectangle. The algorithm:
    //   1) alpha = clamp(text_h / text_w, 0.2, 0.8)
    //   2) Points of text rect are y-scaled by 1/alpha (making it ~square)
    //   3) Smallest enclosing circle of the scaled rectangle (= circumscribed circle)
    //   4) Ellipse: width = 2*radius, height = 2*radius*alpha
    //   5) bigger(6): width += 6, height += 6
    if entity.kind == ComponentKind::UseCase {
        let tw = text_width(&entity.name);
        let th = LINE_HEIGHT;
        let alpha = (th / tw).clamp(0.2, 0.8);
        // Java Footprint.drawText: y -= dim.getHeight() - 1.5;
        // Rect in y-scaled space: width = tw, height = th / alpha
        let scaled_h = th / alpha;
        let diag = (tw * tw + scaled_h * scaled_h).sqrt();
        let radius = diag / 2.0;
        let ellipse_w = 2.0 * radius + 6.0;
        let ellipse_h = 2.0 * radius * alpha + 6.0;
        log::debug!(
            "estimate_entity_size: USECASE name={:?} tw={:.4} th={:.4} alpha={:.4} radius={:.4} w={:.4} h={:.4}",
            entity.name, tw, th, alpha, radius, ellipse_w, ellipse_h
        );
        return (ellipse_w, ellipse_h);
    }

    // Check if stereotype references a sprite
    let sprite_dims = entity
        .stereotype
        .as_ref()
        .and_then(|s| sprite_stereo_dimensions(s));

    if let Some((sprite_w, sprite_h)) = sprite_dims {
        // Java: USymbolRectangle.asSmall uses Margin(10,10,10,10).
        // Dimension = margin.addDimension(stereo.mergeTB(label))
        //   stereo = (sprite_w, sprite_h)
        //   label  = (name_text_width, name_line_height)
        //   mergeTB: width = max(sprite_w, label_w), height = sprite_h + label_h
        //   addDimension: width += 20, height += 20
        let name_lines: Vec<&str> = entity.name.lines().collect();
        let name_line_count = name_lines.len().max(1);
        let label_w = name_lines
            .iter()
            .map(|line| text_width(line))
            .fold(0.0_f64, f64::max);
        let label_h = name_line_count as f64 * LINE_HEIGHT;
        let content_w = label_w.max(sprite_w);
        let content_h = sprite_h + label_h;
        let margin = 10.0; // Java Margin(10,10,10,10)
        let width = content_w + 2.0 * margin;
        let height = content_h + 2.0 * margin;
        return (width, height);
    }

    // When wrapWidth is active (e.g. C4 `skinparam wrapWidth 200`), the entity
    // body text wraps at that pixel width. Java: SheetBlock1 uses MaximumWidth
    // to constrain each stripe. This produces narrower, taller entities.
    if let Some(max_w) = wrap_width {
        let (ml, mr, mt, mb) = entity_margins(&entity.kind);
        let content_max = max_w; // Java wraps text at wrapWidth, margins are outside

        let name_lines: Vec<&str> = entity.name.split("\\n").flat_map(|s| s.lines()).collect();
        let base_lh = font_metrics::line_height("SansSerif", FONT_SIZE, false, false);

        let mut total_text_h = 0.0_f64; // cumulative text height (per-line)
        let mut max_content_w = 0.0_f64;
        let mut sprite_height = 0.0_f64;

        for raw_line in &name_lines {
            if raw_line.trim().is_empty() {
                total_text_h += base_lh;
                continue;
            }
            // Sprite reference: `<$name>` — uses the sprite's pixel
            // dimensions instead of text metrics.
            // Java: CommandCreoleSprite sets scale = fc.getSize2D() / 13.0
            // where fc is the enclosing text block's base font (FONT_SIZE=14pt).
            let trimmed_l = raw_line.trim();
            if trimmed_l.starts_with("<$") && trimmed_l.ends_with('>') {
                let sprite_name = &trimmed_l[2..trimmed_l.len() - 1];
                if let Some((sw, sh)) = sprite_stereo_dimensions(&format!("${sprite_name}")) {
                    let sprite_scale = FONT_SIZE / 13.0;
                    let rendered_w = sw * sprite_scale;
                    let rendered_h = sh * sprite_scale;
                    max_content_w = max_content_w.max(rendered_w);
                    sprite_height += rendered_h;
                    continue;
                }
            }
            let (text, font_size, bold, italic) = parse_c4_line_props(raw_line);
            if text.is_empty() {
                total_text_h += base_lh;
                continue;
            }
            // Use per-line line_height matching Java's per-stripe dimension.
            let line_lh = font_metrics::line_height("SansSerif", font_size, bold, italic);
            let full_w = font_metrics::text_width(text, "SansSerif", font_size, bold, italic);
            if full_w > content_max {
                let (wrapped_count, wrapped_max) =
                    wrapped_line_metrics(text, font_size, bold, italic, content_max);
                total_text_h += wrapped_count as f64 * line_lh;
                max_content_w = max_content_w.max(wrapped_max);
            } else {
                total_text_h += line_lh;
                max_content_w = max_content_w.max(full_w);
            }
        }

        // Stereotype line (e.g. «container»).
        let stereo_font_size = FONT_SIZE - 2.0;
        let stereo_w = entity.stereotype.as_ref().map_or(0.0, |s| {
            let gt = format!("\u{00AB}{s}\u{00BB}");
            font_metrics::text_width(&gt, "SansSerif", stereo_font_size, false, true)
        });
        max_content_w = max_content_w.max(stereo_w);
        // The stereo line is rendered above the body but counted separately.
        let stereo_line_h = if entity.stereotype.is_some() {
            font_metrics::line_height("SansSerif", stereo_font_size, false, true)
        } else {
            0.0
        };

        let width = max_content_w + ml + mr;
        let height = total_text_h + stereo_line_h + sprite_height + mt + mb;

        log::debug!(
            "estimate_entity_size(wrapped): name={:?} wrapWidth={} content_w={:.1} text_h={:.1} w={:.1} h={:.1}",
            entity.name, max_w, max_content_w, total_text_h, width, height
        );
        return (width, height);
    }

    // Non-sprite path: use type-specific margins from Java USymbol classes.
    // Card has tight margins (10,10,3,3); most others use PADDING=15 as fallback.
    let (ml, mr, mt, mb) = entity_margins(&entity.kind);
    // Split on creole line breaks (\n as literal backslash-n text) AND actual newlines.
    // Java's creole parser treats `\n` as a line separator for both sizing and rendering.
    let name_lines: Vec<&str> = entity.name.split("\\n").flat_map(|s| s.lines()).collect();
    let name_line_count = name_lines.len().max(1);
    // Component icon space is already included in the right margin (25px).
    // For lines with creole markup, strip markup before measuring (matches Java).
    // For plain lines, use raw text_width to preserve existing behavior.
    let name_w = name_lines
        .iter()
        .map(|line| {
            if line.contains('<') {
                crate::render::svg_richtext::creole_text_width_preserve_newline(
                    line,
                    "SansSerif",
                    FONT_SIZE,
                    false,
                    false,
                )
            } else {
                text_width(line)
            }
        })
        .fold(0.0_f64, f64::max)
        + ml
        + mr;

    // Description width: use creole_text_width for styled text measurement.
    // Inside <code> blocks, text is literal monospace (no creole stripping).
    let desc_w = {
        let mut max_w = 0.0_f64;
        let mut in_code = false;
        for line in &entity.description {
            let t = line.trim();
            if t.eq_ignore_ascii_case("<code>") {
                in_code = true;
                continue;
            }
            if t.eq_ignore_ascii_case("</code>") {
                in_code = false;
                continue;
            }
            let w = if in_code {
                let code_pad = font_metrics::char_width(' ', "Monospaced", FONT_SIZE, false, false);
                font_metrics::text_width(line, "Monospaced", FONT_SIZE, false, false)
                    + ml
                    + code_pad
                    + mr
            } else {
                crate::render::svg_richtext::creole_text_width_preserve_newline(
                    line,
                    "SansSerif",
                    FONT_SIZE,
                    false,
                    false,
                ) + ml
                    + mr
            };
            max_w = max_w.max(w);
        }
        max_w
    };

    // Java EntityImageDescription measures the stereotype with guillemets at italic 14pt.
    // The entity dimension = TextBlockVertical2(stereo, name).addDimension(margin).
    // The +2 accounts for the 1px inner draw offset on each side in Java rendering.
    let stereo_w = entity.stereotype.as_ref().map_or(0.0, |s| {
        let guillemet_text = format!("\u{00AB}{s}\u{00BB}");
        font_metrics::text_width(&guillemet_text, "SansSerif", FONT_SIZE, false, true)
            + ml
            + mr
            + 2.0
    });

    // Java USymbolFolder.getDimTitle returns min width=40 for the folder tab.
    let folder_min_w = if matches!(entity.kind, ComponentKind::Folder) {
        40.0 + ml + mr
    } else {
        0.0
    };
    let width = name_w
        .max(desc_w)
        .max(stereo_w)
        .max(folder_min_w)
        .max(NODE_MIN_WIDTH);

    let stereo_lines = if entity.stereotype.is_some() {
        1.0
    } else {
        0.0
    };
    // When entity has a body description `[...]`, the body replaces the name display.
    // Java: EntityImageDescription uses the body text block only (name is just an alias).
    // Also filter structural tags like `<code>` / `</code>` which are not visual lines.
    let total_lines = if entity.description.is_empty() {
        name_line_count as f64 + stereo_lines
    } else {
        let effective_desc_lines = entity
            .description
            .iter()
            .filter(|line| {
                let t = line.trim();
                !t.eq_ignore_ascii_case("<code>") && !t.eq_ignore_ascii_case("</code>")
            })
            .count() as f64;
        effective_desc_lines + stereo_lines
    };
    // Java USymbolFolder.calculateDimension adds getDimTitle() height to the
    // merged dimension. For showTitle=false (the default): getDimTitle = (40, 15).
    // This accounts for the folder tab visual space.
    let folder_tab_h = if matches!(entity.kind, ComponentKind::Folder) {
        15.0
    } else {
        0.0
    };
    // Java EntityImageDescription uses font.line_height (ascent+descent) = 16.2969
    // for each text line. However, the existing C4 and component tests were calibrated
    // against LINE_HEIGHT = 16.0. Use the exact font line height only for archimate
    // entities where the difference matters for the viewport.
    let text_line_h = if matches!(entity.kind, ComponentKind::Archimate) {
        font_metrics::line_height("SansSerif", FONT_SIZE, false, false)
    } else {
        LINE_HEIGHT
    };
    let height = total_lines * text_line_h + mt + mb + folder_tab_h;

    log::debug!(
        "estimate_entity_size: name={:?} kind={:?} margins=({},{},{},{}) lines={} w={:.1} h={:.1}",
        entity.name,
        entity.kind,
        ml,
        mr,
        mt,
        mb,
        total_lines,
        width,
        height
    );

    (width, height)
}

/// Estimate note size matching Java Opale note dimensions.
/// Java EntityImageNote uses: marginX1=6, marginX2=15, marginY=5.
/// The DOT node dimension = calculateDimension = (textWidth + 21, textHeight + 10).
fn estimate_note_size(text: &str) -> (f64, f64) {
    estimate_note_size_with_embedded(text, None)
}

/// Estimate note size, accounting for an embedded diagram if present.
fn estimate_note_size_with_embedded(
    text: &str,
    embedded: Option<&EmbeddedDiagramData>,
) -> (f64, f64) {
    const NOTE_FONT_SIZE: f64 = 13.0;
    const NOTE_LINE_HEIGHT: f64 = 15.1328; // SansSerif 13pt: ascent+descent
    const MARGIN_X1: f64 = 6.0;
    const MARGIN_X2: f64 = 15.0;
    const MARGIN_Y: f64 = 5.0;

    if let Some(emb) = embedded {
        // Note contains an embedded diagram.
        // Java: before_text + image + after_text stacked vertically.
        let before_lines: Vec<&str> = if emb.text_before.is_empty() {
            vec![]
        } else {
            emb.text_before.lines().collect()
        };
        let after_lines: Vec<&str> = if emb.text_after.is_empty() {
            // Check if the original note text has a trailing `\n` after `}}`.
            // Java counts this as one blank line for note height calculation.
            if text.trim_end().ends_with("}}") && text.ends_with('\n') {
                vec![""]
            } else {
                vec![]
            }
        } else {
            emb.text_after.lines().collect()
        };

        let before_text_width = before_lines
            .iter()
            .map(|l| font_metrics::text_width(l, "SansSerif", NOTE_FONT_SIZE, false, false))
            .fold(0.0_f64, f64::max);
        let after_text_width = after_lines
            .iter()
            .map(|l| font_metrics::text_width(l, "SansSerif", NOTE_FONT_SIZE, false, false))
            .fold(0.0_f64, f64::max);

        let content_width = before_text_width.max(emb.width).max(after_text_width);
        let before_height = before_lines.len() as f64 * NOTE_LINE_HEIGHT;
        let after_height = after_lines.len() as f64 * NOTE_LINE_HEIGHT;
        let content_height = before_height + emb.height + after_height;

        let width = content_width + MARGIN_X1 + MARGIN_X2;
        let height = content_height + 2.0 * MARGIN_Y;
        (width, height)
    } else {
        let lines: Vec<&str> = text.lines().collect();
        let max_line_width = lines
            .iter()
            .map(|l| font_metrics::text_width(l, "SansSerif", NOTE_FONT_SIZE, false, false))
            .fold(0.0_f64, f64::max);
        let text_height = lines.len().max(1) as f64 * NOTE_LINE_HEIGHT;
        let width = max_line_width + MARGIN_X1 + MARGIN_X2;
        let height = text_height + 2.0 * MARGIN_Y;
        (width, height)
    }
}

fn parse_path_start(d: &str) -> Option<(f64, f64)> {
    let d = d.trim_start();
    let d = d.strip_prefix('M').or_else(|| d.strip_prefix('m'))?;
    let d = d.trim_start();
    let comma = d.find(',')?;
    let x: f64 = d[..comma].trim().parse().ok()?;
    let rest = &d[comma + 1..];
    let y_end = rest
        .find(|c: char| c.is_whitespace() || c.is_ascii_alphabetic())
        .unwrap_or(rest.len());
    let y: f64 = rest[..y_end].trim().parse().ok()?;
    Some((x, y))
}

fn align_raw_path_d(raw_d: &str, points: &[(f64, f64)], dx: f64, dy: f64) -> String {
    let Some(&(px, py)) = points.first() else {
        return graphviz::transform_path_d(raw_d, dx, dy);
    };
    let Some((rx, ry)) = parse_path_start(raw_d) else {
        return graphviz::transform_path_d(raw_d, dx, dy);
    };

    graphviz::transform_path_d(raw_d, dx + (px - rx), dy + (py - ry))
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn layout_component(
    cd: &ComponentDiagram,
    skin: &crate::style::SkinParams,
) -> Result<ComponentLayout> {
    log::debug!(
        "layout_component: {} entities, {} links, {} groups, {} notes",
        cd.entities.len(),
        cd.links.len(),
        cd.groups.len(),
        cd.notes.len()
    );

    let entity_map: HashMap<String, &ComponentEntity> =
        cd.entities.iter().map(|e| (e.id.clone(), e)).collect();

    // Simulate Java's cpt1 counter to compute GMN qualified names.
    // In Java, entities and notes are processed in source-line order.
    // Each entity consumes 1 counter value (for uid).
    // Each note consumes 3 counter values: GMN name, entity uid, link uid.
    // The counter starts at 1, first addAndGet(1) returns 2.
    let note_gmn_names: Vec<String> = {
        enum Item {
            Entity(usize),      // source_line
            Note(usize, usize), // (note_index, source_line)
        }
        let mut items: Vec<Item> = Vec::new();
        for e in &cd.entities {
            items.push(Item::Entity(e.source_line.unwrap_or(usize::MAX)));
        }
        for (i, n) in cd.notes.iter().enumerate() {
            items.push(Item::Note(i, n.source_line.unwrap_or(usize::MAX)));
        }
        items.sort_by_key(|item| match item {
            Item::Entity(sl) => *sl,
            Item::Note(_, sl) => *sl,
        });

        let mut cpt1: u32 = 1;
        let mut names = vec![String::new(); cd.notes.len()];
        for item in &items {
            match item {
                Item::Entity(_) => {
                    cpt1 += 1; // entity uid
                }
                Item::Note(idx, _) => {
                    cpt1 += 1; // GMN name
                    names[*idx] = format!("GMN{}", cpt1);
                    cpt1 += 1; // note entity uid
                    cpt1 += 1; // note link uid
                }
            }
        }
        names
    };

    let group_ids: std::collections::HashSet<String> =
        cd.groups.iter().map(|g| g.id.clone()).collect();

    fn sanitize_id(name: &str) -> String {
        name.replace('<', "_LT_")
            .replace('>', "_GT_")
            .replace(',', "_COMMA_")
            .replace(' ', "_")
            .replace('"', "_Q_")
    }

    let id_to_dot: HashMap<String, String> = cd
        .entities
        .iter()
        .map(|e| (e.id.clone(), sanitize_id(&e.id)))
        .collect();

    // Collect IDs of groups that are referenced by edges (as link endpoints).
    // These need proxy nodes inside their clusters so edges can connect.
    let group_ids_in_edges: std::collections::HashSet<String> = cd
        .links
        .iter()
        .flat_map(|link| [link.from.clone(), link.to.clone()])
        .filter(|id| group_ids.contains(id))
        .collect();

    let wrap_width = skin.wrap_width();

    let mut layout_nodes: Vec<LayoutNode> = cd
        .entities
        .iter()
        .filter(|e| !group_ids.contains(&e.id))
        .map(|e| {
            let (w, h) = estimate_entity_size(e, wrap_width);
            let entity_position = match e.kind {
                ComponentKind::PortIn => Some(EntityPosition::PortIn),
                ComponentKind::PortOut => Some(EntityPosition::PortOut),
                _ => None,
            };
            let shape = match e.kind {
                ComponentKind::PortIn | ComponentKind::PortOut => Some(ShapeType::RectanglePort),
                // Java EntityImageDescription: usecase → ShapeType.OVAL
                ComponentKind::UseCase => Some(ShapeType::Oval),
                // Actor: ShapeType.RECTANGLE (default)
                _ => None,
            };
            let max_label_width = match e.kind {
                ComponentKind::PortIn | ComponentKind::PortOut => Some(text_width(&e.name)),
                _ => None,
            };
            LayoutNode {
                id: id_to_dot
                    .get(&e.id)
                    .cloned()
                    .unwrap_or_else(|| sanitize_id(&e.id)),
                label: e.name.clone(),
                width_pt: w,
                height_pt: h,
                shape,
                shield: None,
                entity_position,
                max_label_width,
                port_label_width: None,
                order: e.source_line,
                image_width_pt: None,
                image_height_pt: None,
                lf_extra_left: 0.0,
                // Java LimitFinder shape correction by entity draw type:
                // - URectangle entities: drawRectangle adds (-1,-1) and (-1,-1)
                // - UEllipse entities (UseCase): drawEllipse adds (0,0) and (-1,-1)
                // - UPath entities (Database, Queue): drawUPath adds (0,0) and (0,0)
                // - UPolygon entities (Node, Folder): drawUPolygon adds HACK_X_FOR_POLYGON
                lf_rect_correction: !matches!(
                    e.kind,
                    ComponentKind::UseCase
                        | ComponentKind::Database
                        | ComponentKind::Queue
                        | ComponentKind::Node
                        | ComponentKind::Folder
                        | ComponentKind::Actor
                ),
                lf_has_body_separator: false,
                // Entities that draw UEmpty(10,10) extending their bounding box:
                // - Database: UEmpty at (width, height) → extends +10 on both X and Y
                // - Node: UEmpty at (0, height) → extends +10 on Y only
                lf_node_polygon: matches!(e.kind, ComponentKind::Node | ComponentKind::Database),
                lf_polygon_hack: matches!(e.kind, ComponentKind::Node | ComponentKind::Folder),
                lf_actor_stickman: e.kind == ComponentKind::Actor,
                hidden: false,
            }
        })
        .collect();

    // Proxy nodes for group edge endpoints are emitted directly in the cluster
    // DOT (via special_point_id on LayoutClusterSpec). They must also exist as
    // regular layout nodes so edges can reference them, but they are marked
    // hidden so they don't participate in LimitFinder calculations.
    // This matches Java where zaent special points are DOT artifacts.
    for gid in &group_ids_in_edges {
        let proxy_id = format!("{}_proxy", sanitize_id(gid));
        layout_nodes.push(LayoutNode {
            id: proxy_id,
            label: String::new(),
            width_pt: 0.01,
            height_pt: 0.01,
            shape: None,
            shield: None,
            entity_position: None,
            max_label_width: None,
            port_label_width: None,
            order: None,
            image_width_pt: None,
            image_height_pt: None,
            lf_extra_left: 0.0,
            lf_rect_correction: true,
            lf_has_body_separator: false,
            lf_node_polygon: false,
            lf_polygon_hack: false,
            lf_actor_stickman: false,
            hidden: true, // excluded from LimitFinder span
        });
    }

    let layout_edges: Vec<LayoutEdge> = cd
        .links
        .iter()
        .map(|link| {
            // Map group entity IDs to their proxy node IDs
            let from_dot = if group_ids_in_edges.contains(&link.from) {
                format!("{}_proxy", sanitize_id(&link.from))
            } else {
                id_to_dot
                    .get(&link.from)
                    .cloned()
                    .unwrap_or_else(|| sanitize_id(&link.from))
            };
            let to_dot = if group_ids_in_edges.contains(&link.to) {
                format!("{}_proxy", sanitize_id(&link.to))
            } else {
                id_to_dot
                    .get(&link.to)
                    .cloned()
                    .unwrap_or_else(|| sanitize_id(&link.to))
            };
            // Java: CommandLinkElement.executeArg() handles direction hints:
            //   LEFT/RIGHT cross-axis → queue="-" (length=1, minlen=0), no constraint=false
            //   LEFT/UP → link.getInv() (invert edge direction)
            // This matches Java's behavior for description/component diagrams.
            let is_vertical = matches!(
                cd.direction,
                Direction::TopToBottom | Direction::BottomToTop
            );
            let hint = link.direction_hint.as_deref();
            let is_cross_axis = hint.is_some_and(|h| {
                if is_vertical {
                    h == "left" || h == "right"
                } else {
                    h == "up" || h == "down"
                }
            });
            // Java inverts for LEFT and UP directions (regardless of main/cross axis)
            let invert = hint.is_some_and(|h| h == "up" || h == "left");
            let (edge_from, edge_to) = if invert {
                (to_dot, from_dot)
            } else {
                (from_dot, to_dot)
            };
            // Java: cross-axis links force queue="-" (length=1) → minlen=0
            let minlen = if is_cross_axis {
                0
            } else {
                link.arrow_len.saturating_sub(1) as u32
            };
            // The legacy component pipeline left `head_decoration=None`
            // for every edge and let the renderer hard-code an
            // `ExtremityArrow` 5-point rhombus.  We deliberately keep
            // `head_decoration=None` here at the *graphviz* layer so
            // DOT routing (and therefore the spline endpoints) remain
            // identical to the previous baseline regardless of `>>`
            // vs `>`.  The arrow shape (rhombus vs triangle) is
            // selected at SVG render time from the per-edge
            // `ComponentEdgeLayout.head_decoration` field that we
            // populate downstream from `link.head_arrow_triangle`.
            let head_decoration = crate::svek::edge::LinkDecoration::None;
            let tail_decoration = crate::svek::edge::LinkDecoration::None;
            LayoutEdge {
                from: edge_from,
                to: edge_to,
                label: if link.label.is_empty() {
                    None
                } else {
                    Some(link.label.clone())
                },
                label_dimension: None,
                tail_label: None,
                tail_label_dimension: None,
                tail_label_boxed: false,
                head_label: None,
                head_label_dimension: None,
                head_label_boxed: false,
                tail_decoration,
                head_decoration,
                line_style: crate::svek::edge::LinkStyle::Normal,
                minlen,
                invisible: false,
                is_opale: false,
                no_constraint: false, // Java doesn't use constraint=false for component links
            }
        })
        .collect();

    let rankdir = match cd.direction {
        Direction::TopToBottom => RankDir::TopToBottom,
        Direction::LeftToRight => RankDir::LeftToRight,
        Direction::BottomToTop => RankDir::BottomToTop,
        Direction::RightToLeft => RankDir::RightToLeft,
    };

    // Build cluster specs from parsed groups
    let clusters: Vec<LayoutClusterSpec> = cd
        .groups
        .iter()
        .map(|g| {
            let node_ids: Vec<String> = g
                .children
                .iter()
                .filter_map(|child_id| id_to_dot.get(child_id).cloned())
                .collect();
            // Proxy nodes for edge endpoints are emitted via special_point_id
            // in write_cluster, not as regular cluster members.
            // Compute cluster label dimensions from group name.
            // Java ClusterHeader: stereoAndTitle = mergeTB(stereo, title)
            // Uses creole-aware height computation for names containing
            // block separators (----) and bullet items (* text).
            let name_lines: Vec<&str> = g.name.lines().collect();
            let label_w = name_lines
                .iter()
                .map(|line| {
                    let trimmed = line.trim();
                    if let Some(bullet_text) = trimmed.strip_prefix("* ") {
                        // Bullet line: indent(12) + text width
                        12.0 + font_metrics::text_width(
                            bullet_text,
                            "SansSerif",
                            FONT_SIZE,
                            true,
                            false,
                        )
                    } else {
                        font_metrics::text_width(line, "SansSerif", FONT_SIZE, true, false)
                    }
                })
                .fold(0.0_f64, f64::max);
            let is_boundary = g.stereotypes.iter().any(|s| s.ends_with("_boundary"));
            // Compute title height.
            // C4 boundaries use heading-aware per-line heights (== prefix bumps font).
            // Other groups use the general creole height computation which handles
            // separators (----) and bullet items (* text) correctly.
            let title_h = if is_boundary {
                g.name
                    .lines()
                    .map(|line| {
                        let fs = if let Some((_, order)) =
                            crate::parser::creole::strip_heading_prefix_ordered(line)
                        {
                            match order {
                                0 => FONT_SIZE + 4.0,
                                1 => FONT_SIZE + 2.0,
                                2 => FONT_SIZE + 1.0,
                                _ => FONT_SIZE,
                            }
                        } else {
                            FONT_SIZE
                        };
                        font_metrics::line_height("SansSerif", fs, true, false)
                    })
                    .sum()
            } else {
                crate::render::svg_richtext::compute_creole_entity_name_height(&g.name, FONT_SIZE)
            };
            // Add sprite height if stereotype references a sprite
            let sprite_h = g
                .stereotype
                .as_ref()
                .and_then(|s| sprite_stereo_dimensions(s))
                .map_or(0.0, |(_, h)| h);
            // C4 boundary subtitle: extra line for "[system]" etc.
            // Java renders this at FONT_SIZE - 2 (12pt for base 14).
            let boundary_subtitle_h = if !g.name.contains("<size:") && is_boundary {
                font_metrics::line_height("SansSerif", FONT_SIZE - 2.0, true, false)
            } else {
                0.0
            };
            // Java ClusterHeader: dimLabel = mergeTB(stereo, title).calculateDimension()
            // The stereo block is either:
            //   1. A sprite image (when stereotype references a sprite) — handled by sprite_h
            //   2. Text labels for each visible stereotype — computed here as stereo_h
            // When stereotype is a sprite, getSprite() returns the image and the
            // text label path is skipped. Only add text-label stereo height when
            // there is NO sprite.
            //
            // For C4 boundaries `StereotypeFontSize` is typically 6 (transparent),
            // and Java AtomText enforces a minimum height of 10px per line, making
            // each stereo line exactly 10px.
            let has_sprite_stereo = sprite_h > 0.0;
            let stereo_h = if has_sprite_stereo || g.stereotypes.is_empty() {
                0.0 // sprite already counted in sprite_h, or no stereotypes
            } else {
                let stereo_refs: Vec<&str> = g.stereotypes.iter().map(|s| s.as_str()).collect();
                let stereo_fs = skin
                    .stereotype_font_size_for("rectangle", &stereo_refs)
                    .unwrap_or(FONT_SIZE);
                let line_h =
                    font_metrics::line_height("SansSerif", stereo_fs, false, true).max(10.0); // Java AtomText: if (h < 10) h = 10
                g.stereotypes.len() as f64 * line_h
            };
            // Java ClusterHeader: titleAndAttributeHeight =
            //   (int)(dimLabel.getHeight() + attributeHeight + marginForFields
            //         + suppHeightBecauseOfShape)
            // suppHeightBecauseOfShape: Node=5, Database=15, others=0
            // suppWidthBecauseOfShape:  Node=60, others=0
            // The -5 adjustment is applied in cluster_dot_label (svek/mod.rs:888).
            let (supp_h, supp_w) = cluster_supp_for_shape(&g.kind);
            let raw_h = stereo_h + sprite_h + title_h + boundary_subtitle_h + supp_h;
            let label_h = if sprite_h > 0.0 { raw_h.floor() } else { raw_h };
            let final_label_w = label_w.floor().max(0.0) + supp_w;
            // Java: thereALinkFromOrToGroup generates extra _a/_i wrappers
            // and a special point node inside the cluster.
            let is_edge_endpoint = group_ids_in_edges.contains(&g.id);
            let special_point_id = if is_edge_endpoint {
                Some(format!("{}_proxy", sanitize_id(&g.id)))
            } else {
                None
            };
            // Java draws node / artifact / cloud clusters as UPolygon; these
            // feed LimitFinder.drawUPolygon → `HACK_X_FOR_POLYGON = 10`.
            // Other symbols (component / frame / rectangle / storage / …)
            // use URectangle. Propagate the shape so svek applies the
            // matching LF extension during `moveDelta` computation.
            let style = match g.kind {
                crate::model::ComponentKind::Node
                | crate::model::ComponentKind::Cloud
                | crate::model::ComponentKind::Artifact => crate::svek::cluster::ClusterStyle::Node,
                _ => crate::svek::cluster::ClusterStyle::Rectangle,
            };
            LayoutClusterSpec {
                id: sanitize_id(&g.id),
                qualified_name: g.code.clone(),
                title: Some(g.name.clone()),
                style,
                label_width: Some(final_label_w),
                label_height: Some(label_h.floor().max(0.0)),
                node_ids,
                sub_clusters: vec![],
                order: g.source_line,
                has_link_from_or_to_group: is_edge_endpoint,
                special_point_id,
            }
        })
        .collect();

    // Add note entities as graphviz nodes with invisible edges to targets.
    // Java: notes are real entities (LeafType.NOTE) with GMN* IDs, connected
    // to their target via invisible dashed links.  Graphviz determines their
    // positions, then the Opale renderer draws the ear connector.
    let mut note_dot_ids: Vec<String> = Vec::new();
    let mut note_edges: Vec<LayoutEdge> = Vec::new();
    for (i, note) in cd.notes.iter().enumerate() {
        let note_id = format!("GMN{}", i);
        let (nw, nh) = estimate_note_size(&note.text);
        // Notes use UPath (not URectangle), so lf_rect_correction = false.
        layout_nodes.push(LayoutNode {
            id: note_id.clone(),
            label: String::new(),
            width_pt: nw,
            height_pt: nh,
            shape: None,
            shield: None,
            entity_position: None,
            max_label_width: None,
            port_label_width: None,
            order: None,
            image_width_pt: None,
            image_height_pt: None,
            lf_extra_left: 0.0,
            lf_rect_correction: false,
            lf_has_body_separator: false,
            lf_node_polygon: false,
            lf_polygon_hack: false,
            lf_actor_stickman: false,
            hidden: false,
        });

        // Create invisible edge between note and target entity (Java link pattern).
        if let Some(ref target) = note.target {
            let target_dot = if group_ids_in_edges.contains(target) {
                format!("{}_proxy", sanitize_id(target))
            } else {
                id_to_dot
                    .get(target)
                    .cloned()
                    .unwrap_or_else(|| sanitize_id(target))
            };
            // Java: TOP → note→target (length=2), BOTTOM → target→note (length=2),
            //        LEFT → note→target (length=1), RIGHT → target→note (length=1).
            let (from, to, minlen) = match note.position.as_str() {
                "top" => (note_id.clone(), target_dot, 1),
                "bottom" => (target_dot, note_id.clone(), 1),
                "left" => (note_id.clone(), target_dot.clone(), 0),
                "right" => (target_dot.clone(), note_id.clone(), 0),
                _ => (note_id.clone(), target_dot, 1),
            };
            let no_constraint = matches!(note.position.as_str(), "left" | "right");
            note_edges.push(LayoutEdge {
                from,
                to,
                label: None,
                label_dimension: None,
                tail_label: None,
                tail_label_dimension: None,
                tail_label_boxed: false,
                head_label: None,
                head_label_dimension: None,
                head_label_boxed: false,
                tail_decoration: crate::svek::edge::LinkDecoration::None,
                head_decoration: crate::svek::edge::LinkDecoration::None,
                line_style: crate::svek::edge::LinkStyle::Dashed,
                minlen,
                // Java: setOpale(true) — Graphviz lays out the spline so we
                // can read its endpoints, but the edge itself is rendered as
                // an Opale connector ear by `render_note`, not as a regular
                // line. Keeping the link non-invisible matches Java exactly:
                // wasm Graphviz emits a real Bezier path whose endpoints
                // give us pp1 (note border) and pp2 (entity border) at
                // sub-pixel precision.
                invisible: false,
                is_opale: true,
                no_constraint,
            });
        }
        note_dot_ids.push(note_id);
    }
    let mut all_edges = layout_edges;
    all_edges.extend(note_edges);

    // Java applySingleStrategy: arrange standalone (unlinked) entities in a
    // square grid using invisible edges.  SquareMaker.putInSquare() computes
    // branch = ceil(sqrt(n)) and emits leftRight (minlen=0, same rank) and
    // topDown (minlen=1, next rank) invisible links.
    // Only applies to entities in the same group; here we handle root-level
    // standalones (not inside any cluster).
    {
        let linked_ids: std::collections::HashSet<&str> = all_edges
            .iter()
            .flat_map(|e| [e.from.as_str(), e.to.as_str()])
            .collect();
        // Collect IDs of entities that are inside clusters
        let clustered_ids: std::collections::HashSet<&str> = clusters
            .iter()
            .flat_map(|c| c.node_ids.iter().map(|s| s.as_str()))
            .collect();
        let mut standalone_ids: Vec<String> = Vec::new();
        let mut seen_standalone = std::collections::HashSet::new();
        for n in &layout_nodes {
            if !n.hidden
                && !linked_ids.contains(n.id.as_str())
                && !clustered_ids.contains(n.id.as_str())
                && seen_standalone.insert(n.id.clone())
            {
                standalone_ids.push(n.id.clone());
            }
        }
        if standalone_ids.len() >= 3 {
            let branch = {
                let sqrt = (standalone_ids.len() as f64).sqrt();
                let r = sqrt as usize;
                if r * r == standalone_ids.len() {
                    r
                } else {
                    r + 1
                }
            };
            let mut head_branch = 0usize;
            for i in 1..standalone_ids.len() {
                let dist = i - head_branch;
                if dist == branch {
                    // topDown: first-of-row → first-of-next-row (minlen=1)
                    all_edges.push(LayoutEdge {
                        from: standalone_ids[head_branch].clone(),
                        to: standalone_ids[i].clone(),
                        label: None,
                        label_dimension: None,
                        tail_label: None,
                        tail_label_dimension: None,
                        tail_label_boxed: false,
                        head_label: None,
                        head_label_dimension: None,
                        head_label_boxed: false,
                        tail_decoration: crate::svek::edge::LinkDecoration::None,
                        head_decoration: crate::svek::edge::LinkDecoration::None,
                        line_style: crate::svek::edge::LinkStyle::Normal,
                        minlen: 1,
                        invisible: true,
                        is_opale: false,
                        no_constraint: false,
                    });
                    head_branch = i;
                } else {
                    // leftRight: same row, adjacent (minlen=0, same rank)
                    all_edges.push(LayoutEdge {
                        from: standalone_ids[i - 1].clone(),
                        to: standalone_ids[i].clone(),
                        label: None,
                        label_dimension: None,
                        tail_label: None,
                        tail_label_dimension: None,
                        tail_label_boxed: false,
                        head_label: None,
                        head_label_dimension: None,
                        head_label_boxed: false,
                        tail_decoration: crate::svek::edge::LinkDecoration::None,
                        head_decoration: crate::svek::edge::LinkDecoration::None,
                        line_style: crate::svek::edge::LinkStyle::Normal,
                        minlen: 0,
                        invisible: true,
                        is_opale: false,
                        no_constraint: false,
                    });
                }
            }
        }
    }

    let graph = LayoutGraph {
        nodes: layout_nodes,
        edges: all_edges,
        clusters,
        rankdir,
        is_activity: false,
        ranksep_override: None,
        nodesep_override: None,
        use_simplier_dot_link_strategy: false,
        arrow_font_size: skin.font_size_opt("arrow"),
    };
    let gl = graphviz::layout_with_svek(&graph)?;

    let dot_to_id: HashMap<String, String> = id_to_dot
        .iter()
        .map(|(k, v)| (v.clone(), k.clone()))
        .collect();
    // Java: SvekResult applies moveDelta(6 - LF_minX, 6 - LF_minY), then nodes
    // are drawn at (node.minX, node.minY) in the moveDelta-shifted coordinate system.
    // Our svek solve uses moveDelta(6 - polygon_minX, 6 - polygon_minY) instead.
    // render_offset compensates: it is (6 + polygon_min - lf_min) per axis.
    // Using render_offset as the edge offset gives the same final positions as Java.
    let edge_offset_x = gl.render_offset.0;
    // When links target groups directly (Java: thereALinkFromOrToGroup), the
    // special-point wrapper cluster leaves the raw Y render offset 1px too low
    // for geometry in the final SVG. Keep labels on the original offset below,
    // but pull entity/cluster/edge bodies back by 1px for those cases only.
    let edge_offset_y = if group_ids_in_edges.is_empty() {
        gl.render_offset.1
    } else {
        gl.render_offset.1 - 1.0
    };

    let mut nodes: Vec<ComponentNodeLayout> = Vec::new();
    let mut node_positions: HashMap<String, (f64, f64, f64, f64)> = HashMap::new();

    for nl in &gl.nodes {
        let entity_id = dot_to_id.get(&nl.id).cloned().unwrap_or(nl.id.clone());
        let entity = match entity_map.get(&entity_id) {
            Some(e) => *e,
            None => continue,
        };
        let x = nl.cx - nl.width / 2.0 + edge_offset_x;
        let y = nl.cy - nl.height / 2.0 + edge_offset_y;
        node_positions.insert(entity_id.clone(), (x, y, nl.width, nl.height));
        nodes.push(ComponentNodeLayout {
            id: entity_id,
            name: entity.name.clone(),
            code: entity.code.clone(),
            kind: entity.kind.clone(),
            x,
            y,
            width: nl.width,
            height: nl.height,
            description: entity.description.clone(),
            stereotype: entity.stereotype.clone(),
            stereotypes: entity.stereotypes.clone(),
            color: entity.color.clone(),
            source_line: entity.source_line,
        });
    }

    if !group_ids_in_edges.is_empty() {
        let group_order_by_id: HashMap<&str, usize> = cd
            .groups
            .iter()
            .enumerate()
            .map(|(idx, group)| (group.id.as_str(), idx))
            .collect();
        nodes.sort_by_key(|node| {
            let entity = entity_map.get(&node.id).copied();
            let parent = entity.and_then(|entity| entity.parent.as_deref());
            let grouped = parent.is_some();
            let group_order = parent
                .and_then(|parent| group_order_by_id.get(parent).copied())
                .unwrap_or(usize::MAX);
            let source_order = entity
                .and_then(|entity| entity.source_line)
                .unwrap_or(usize::MAX);
            (!grouped, group_order, source_order, node.id.clone())
        });
    }

    // Pre-compute per-link inversion flags for SVG comment generation.
    // Java: inverted links get "reverse link" comment (LinkType.looksLikeRevertedForSvg).
    let link_inversions: Vec<bool> = cd
        .links
        .iter()
        .map(|link| {
            let hint = link.direction_hint.as_deref();
            hint.is_some_and(|h| h == "up" || h == "left")
        })
        .collect();

    let mut edges: Vec<ComponentEdgeLayout> =
        gl.edges
            .iter()
            .zip(cd.links.iter())
            .enumerate()
            .map(|(i, (el, link))| {
                let mut points = el.points.clone();
                for pt in &mut points {
                    pt.0 += edge_offset_x;
                    pt.1 += edge_offset_y;
                }
                let inverted = link_inversions.get(i).copied().unwrap_or(false);
                let (from, to) = if inverted {
                    // When inverted, the DOT direction is (to→from), so the SVG
                    // should show "reverse link TO to FROM" matching Java.
                    (link.to.clone(), link.from.clone())
                } else {
                    (link.from.clone(), link.to.clone())
                };
                // label_xy from svek is in YDelta-transformed space (pre-moveDelta,
                // pre-normalization). Apply the same transform as path points to put
                // it in the final SVG coordinate space:
                //   final = raw + moveDelta + render_offset - normalize_offset
                let label_xy = el.label_xy.map(|(lx, ly)| {
                    (
                        lx + gl.move_delta.0 + gl.render_offset.0 - gl.normalize_offset.0,
                        ly + gl.move_delta.1 + gl.render_offset.1 - gl.normalize_offset.1,
                    )
                });
                // Only mark `ArrowTriangle` here so the legacy `None`-default
                // path (used by `Arrow`) is preserved for diagrams that don't
                // use C4 `Rel(...)`.  When `head_decoration` is `None`, the
                // renderer falls through to the original 5-point rhombus.
                let head_decoration = if link.head_arrow_triangle {
                    crate::svek::edge::LinkDecoration::ArrowTriangle
                } else {
                    crate::svek::edge::LinkDecoration::None
                };
                ComponentEdgeLayout {
                    from,
                    to,
                    points,
                    raw_path_d: el.raw_path_d.as_ref().map(|raw_d| {
                        align_raw_path_d(raw_d, &el.points, edge_offset_x, edge_offset_y)
                    }),
                    label: link.label.clone(),
                    dashed: link.dashed,
                    reversed_for_svg: inverted,
                    label_xy,
                    head_decoration,
                }
            })
            .collect();

    // Build group layouts from graphviz cluster output
    let group_map: HashMap<String, &crate::model::component::ComponentGroup> =
        cd.groups.iter().map(|g| (sanitize_id(&g.id), g)).collect();
    let group_layouts: Vec<ComponentGroupLayout> = gl
        .clusters
        .iter()
        .filter_map(|cl| {
            let dot_id = sanitize_id(&cl.qualified_name);
            let group = group_map.get(&dot_id).or_else(|| group_map.get(&cl.id))?;
            Some(ComponentGroupLayout {
                id: group.id.clone(),
                name: group.name.clone(),
                code: group.code.clone(),
                kind: group.kind.clone(),
                x: cl.x + edge_offset_x,
                y: cl.y + edge_offset_y,
                width: cl.width,
                height: cl.height,
                source_line: group.source_line,
                stereotype: group.stereotype.clone(),
                stereotypes: group.stereotypes.clone(),
            })
        })
        .collect();

    {
        let group_rects: HashMap<&str, crate::klimt::geom::RectangleArea> = group_layouts
            .iter()
            .map(|group| {
                (
                    group.id.as_str(),
                    crate::klimt::geom::RectangleArea::new(
                        group.x,
                        group.y,
                        group.x + group.width,
                        group.y + group.height,
                    ),
                )
            })
            .collect();
        for edge in &mut edges {
            let tail_rect = group_rects.get(edge.from.as_str());
            let head_rect = group_rects.get(edge.to.as_str());
            if tail_rect.is_none() && head_rect.is_none() {
                continue;
            }
            let Some(raw_d) = edge.raw_path_d.as_ref() else {
                continue;
            };
            let Some(dot_path) = crate::svek::svg_result::parse_svg_path_d_to_dotpath(raw_d) else {
                continue;
            };
            let clipped = dot_path.simulate_compound(head_rect, tail_rect);
            if clipped.beziers.is_empty() {
                continue;
            }
            edge.raw_path_d = Some(clipped.to_svg_d());
            let mut new_points = Vec::new();
            for (idx, bez) in clipped.beziers.iter().enumerate() {
                if idx == 0 {
                    new_points.push((bez.x1, bez.y1));
                }
                new_points.push((bez.ctrlx1, bez.ctrly1));
                new_points.push((bez.ctrlx2, bez.ctrly2));
                new_points.push((bez.x2, bez.y2));
            }
            edge.points = new_points;
        }
    }

    // Extract note positions from graphviz results.
    // Notes are now real graphviz nodes (GMN0, GMN1, ...) with positions
    // determined by the graph layout, matching Java's approach.
    let note_node_positions: HashMap<String, (f64, f64, f64, f64)> = gl
        .nodes
        .iter()
        .filter(|nl| nl.id.starts_with("GMN"))
        .map(|nl| {
            let x = nl.cx - nl.width / 2.0 + edge_offset_x;
            let y = nl.cy - nl.height / 2.0 + edge_offset_y;
            (nl.id.clone(), (x, y, nl.width, nl.height))
        })
        .collect();

    let mut note_layouts = Vec::new();
    for (i, note) in cd.notes.iter().enumerate() {
        let note_id = format!("GMN{}", i);

        // Detect and render embedded diagrams (`{{ }}` blocks) in note text
        let embedded = crate::render::embedded::extract_embedded(&note.text).and_then(|block| {
            crate::render::embedded::render_embedded(&block.inner_source, &block.diagram_type).map(
                |(inner_svg, w, h)| EmbeddedDiagramData {
                    data_uri: crate::render::embedded::svg_to_data_uri(&inner_svg),
                    width: w,
                    height: h,
                    text_before: block.before,
                    text_after: block.after,
                },
            )
        });

        let (nw, nh) = estimate_note_size_with_embedded(&note.text, embedded.as_ref());

        // Use graphviz position if available, else fallback
        let (nx, ny) = if let Some(&(gx, gy, _, _)) = note_node_positions.get(&note_id) {
            (gx, gy)
        } else {
            // Fallback for notes without graphviz position
            let all_right = nodes.iter().map(|n| n.x + n.width).fold(0.0_f64, f64::max);
            (
                all_right + NOTE_OFFSET + MARGIN,
                MARGIN + i as f64 * (nh + PADDING),
            )
        };

        // Compute ear tip from the Graphviz-laid-out spline that connects
        // the note to its target entity (Java Opale: pp1/pp2 derive from
        // the SmetanaEdge endpoints — the boundary contact points on each
        // side of the spline).  The note->entity link is marked `is_opale`
        // so Graphviz emits a real Bezier path; we read its endpoint nearest
        // the target entity to obtain the ear-tip coordinates.
        let (ear_tip_y, ear_tip_x) = if let Some(ref target) = note.target {
            if let Some(&(_tx, _ty, _tw, _th)) = node_positions.get(target) {
                let target_dot = if group_ids_in_edges.contains(target) {
                    format!("{}_proxy", sanitize_id(target))
                } else {
                    id_to_dot
                        .get(target)
                        .cloned()
                        .unwrap_or_else(|| sanitize_id(target))
                };
                // Locate the Graphviz spline for note->entity (or entity->note).
                // The endpoint touching the entity is the ear-tip; the offset
                // direction (top/bottom note) decides which endpoint to pick.
                let spline_tip = gl.edges.iter().find_map(|e| {
                    let (note_side_endpoint, entity_side_endpoint) =
                        if e.from == note_id && e.to == target_dot {
                            (e.spline_start, e.spline_end)
                        } else if e.from == target_dot && e.to == note_id {
                            (e.spline_end, e.spline_start)
                        } else {
                            return None;
                        };
                    let _ = note_side_endpoint; // pp1 reserved for future use
                    entity_side_endpoint
                });
                if let Some((sx, sy)) = spline_tip {
                    let ear_x = sx + edge_offset_x;
                    let ear_y = sy + edge_offset_y;
                    match note.position.as_str() {
                        "top" | "bottom" => (Some(ear_y), Some(ear_x)),
                        _ => (None, None),
                    }
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        let qname = note_gmn_names
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("GMN{}", i));

        note_layouts.push(ComponentNoteLayout {
            x: nx,
            y: ny,
            width: nw,
            height: nh,
            text: note.text.clone(),
            position: note.position.clone(),
            target: note.target.clone(),
            ear_tip_y,
            ear_tip_x,
            qualified_name: qname,
            source_line: note.source_line,
            embedded,
        });
    }

    // Viewport calculation: match Java's degenerated vs normal path.
    // A diagram with clusters (groups) is not degenerated even if it has ≤1 node.
    // Notes are now part of the graphviz LF span, so no separate extension needed.
    let real_entity_count = nodes.len();
    let is_degenerated = real_entity_count <= 1
        && edges.is_empty()
        && group_layouts.is_empty()
        && cd.notes.is_empty();
    let (raw_body_w, raw_body_h) = if is_degenerated && !nodes.is_empty() {
        const DEGENERATED_DELTA: f64 = 7.0;
        let entity_w = nodes[0].width;
        let entity_h = nodes[0].height;
        (
            entity_w + DEGENERATED_DELTA * 2.0 + 1.0,
            entity_h + DEGENERATED_DELTA * 2.0 + 1.0,
        )
    } else {
        // Java ImageBuilder.getFinalDimension(): shifted LF max + 1.
        // Java moveDelta = (6 - lf_min), so shifted_max = lf_span + 6.
        const SVEK_MOVE_DELTA: f64 = 6.0;
        let mut shifted_max_x = gl.lf_span.0 + SVEK_MOVE_DELTA;
        let shifted_max_y = gl.lf_span.1 + SVEK_MOVE_DELTA;
        // Card groups draw a full-width separator line whose LF bound extends
        // 1px beyond the cluster rectangle.
        let has_card_group = cd.groups.iter().any(|g| g.kind == ComponentKind::Card);
        if has_card_group {
            shifted_max_x += 1.0;
        }
        (shifted_max_x + 1.0, shifted_max_y + 1.0)
    };

    // Extend viewport for group layouts that may extend beyond the LF span.
    let mut max_right = raw_body_w;
    let mut max_bottom = raw_body_h;
    for group in &group_layouts {
        let gr = group.x + group.width - MARGIN + ViewportConfig::COMPONENT.margin_right;
        let gb = group.y + group.height - MARGIN + ViewportConfig::COMPONENT.margin_bottom;
        if gr > max_right {
            max_right = gr;
        }
        if gb > max_bottom {
            max_bottom = gb;
        }
    }

    let (total_width, total_height) =
        compute_viewport(max_right, max_bottom, &ViewportConfig::COMPONENT);

    log::debug!(
        "layout_component done: {:.0}x{:.0} (span={:.1}x{:.1})",
        total_width,
        total_height,
        gl.lf_span.0,
        gl.lf_span.1
    );

    Ok(ComponentLayout {
        nodes,
        edges,
        notes: note_layouts,
        groups: group_layouts,
        width: total_width,
        height: total_height,
    })
}

// ---------------------------------------------------------------------------
// Direction transform
// ---------------------------------------------------------------------------

/// Apply a coordinate transform based on the diagram direction.
/// The layout algorithm always computes in top-to-bottom orientation;
/// for other directions we transform after the fact.
#[allow(dead_code)] // reserved for multi-direction layout
fn apply_direction_transform(
    layout: &mut ComponentLayout,
    direction: &crate::model::diagram::Direction,
) {
    use crate::model::diagram::Direction;
    match direction {
        Direction::TopToBottom => {}
        Direction::LeftToRight => {
            for node in &mut layout.nodes {
                std::mem::swap(&mut node.x, &mut node.y);
                std::mem::swap(&mut node.width, &mut node.height);
            }
            for edge in &mut layout.edges {
                for pt in &mut edge.points {
                    std::mem::swap(&mut pt.0, &mut pt.1);
                }
            }
            for note in &mut layout.notes {
                std::mem::swap(&mut note.x, &mut note.y);
                std::mem::swap(&mut note.width, &mut note.height);
            }
            for group in &mut layout.groups {
                std::mem::swap(&mut group.x, &mut group.y);
                std::mem::swap(&mut group.width, &mut group.height);
            }
            std::mem::swap(&mut layout.width, &mut layout.height);
        }
        Direction::RightToLeft => {
            for node in &mut layout.nodes {
                std::mem::swap(&mut node.x, &mut node.y);
                std::mem::swap(&mut node.width, &mut node.height);
            }
            for edge in &mut layout.edges {
                for pt in &mut edge.points {
                    std::mem::swap(&mut pt.0, &mut pt.1);
                }
            }
            for note in &mut layout.notes {
                std::mem::swap(&mut note.x, &mut note.y);
                std::mem::swap(&mut note.width, &mut note.height);
            }
            for group in &mut layout.groups {
                std::mem::swap(&mut group.x, &mut group.y);
                std::mem::swap(&mut group.width, &mut group.height);
            }
            std::mem::swap(&mut layout.width, &mut layout.height);
            let w = layout.width;
            for node in &mut layout.nodes {
                node.x = w - node.x - node.width;
            }
            for edge in &mut layout.edges {
                for pt in &mut edge.points {
                    pt.0 = w - pt.0;
                }
            }
            for note in &mut layout.notes {
                note.x = w - note.x - note.width;
            }
            for group in &mut layout.groups {
                group.x = w - group.x - group.width;
            }
        }
        Direction::BottomToTop => {
            let h = layout.height;
            for node in &mut layout.nodes {
                node.y = h - node.y - node.height;
            }
            for edge in &mut layout.edges {
                for pt in &mut edge.points {
                    pt.1 = h - pt.1;
                }
            }
            for note in &mut layout.notes {
                note.y = h - note.y - note.height;
            }
            for group in &mut layout.groups {
                group.y = h - group.y - group.height;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Edge routing
// ---------------------------------------------------------------------------

#[allow(dead_code)] // reserved for non-svek edge routing
fn layout_edges(
    links: &[ComponentLink],
    pos_map: &HashMap<String, (f64, f64, f64, f64)>,
) -> Vec<ComponentEdgeLayout> {
    let mut result = Vec::new();

    for link in links {
        let from_pos = pos_map.get(&link.from);
        let to_pos = pos_map.get(&link.to);

        let (fx, fy, fw, fh) = if let Some(p) = from_pos {
            *p
        } else {
            log::warn!("edge source '{}' not found in layout", link.from);
            continue;
        };

        let (tx, ty, tw, th) = if let Some(p) = to_pos {
            *p
        } else {
            log::warn!("edge target '{}' not found in layout", link.to);
            continue;
        };

        let from_cx = fx + fw / 2.0;
        let from_cy = fy + fh / 2.0;
        let to_cx = tx + tw / 2.0;
        let to_cy = ty + th / 2.0;

        // Determine connection points based on direction hint or relative position
        let points = if let Some(ref hint) = link.direction_hint {
            route_with_hint(fx, fy, fw, fh, tx, ty, tw, th, hint)
        } else {
            route_auto(
                from_cx, from_cy, fx, fy, fw, fh, to_cx, to_cy, tx, ty, tw, th,
            )
        };

        log::debug!(
            "  edge '{}' -> '{}' [{}]: {:?}",
            link.from,
            link.to,
            link.label,
            points
        );

        let head_decoration = if link.head_arrow_triangle {
            crate::svek::edge::LinkDecoration::ArrowTriangle
        } else {
            crate::svek::edge::LinkDecoration::Arrow
        };
        result.push(ComponentEdgeLayout {
            from: link.from.clone(),
            to: link.to.clone(),
            points,
            raw_path_d: None,
            label: link.label.clone(),
            dashed: link.dashed,
            reversed_for_svg: false,
            label_xy: None,
            head_decoration,
        });
    }

    result
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)] // reserved for non-svek edge routing
fn route_with_hint(
    fx: f64,
    fy: f64,
    fw: f64,
    fh: f64,
    tx: f64,
    ty: f64,
    tw: f64,
    th: f64,
    hint: &str,
) -> Vec<(f64, f64)> {
    let from_cx = fx + fw / 2.0;
    let from_cy = fy + fh / 2.0;
    let to_cx = tx + tw / 2.0;
    let to_cy = ty + th / 2.0;

    match hint {
        "up" => vec![(from_cx, fy), (to_cx, ty + th)],
        "down" => vec![(from_cx, fy + fh), (to_cx, ty)],
        "left" => vec![(fx, from_cy), (tx + tw, to_cy)],
        "right" => vec![(fx + fw, from_cy), (tx, to_cy)],
        _ => route_auto(
            from_cx, from_cy, fx, fy, fw, fh, to_cx, to_cy, tx, ty, tw, th,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)] // reserved for non-svek edge routing
fn route_auto(
    from_cx: f64,
    from_cy: f64,
    fx: f64,
    fy: f64,
    fw: f64,
    fh: f64,
    to_cx: f64,
    to_cy: f64,
    tx: f64,
    ty: f64,
    tw: f64,
    th: f64,
) -> Vec<(f64, f64)> {
    let dx = (to_cx - from_cx).abs();
    let dy = (to_cy - from_cy).abs();

    if dy > dx {
        // Vertical connection
        if to_cy > from_cy {
            vec![(from_cx, fy + fh), (to_cx, ty)]
        } else {
            vec![(from_cx, fy), (to_cx, ty + th)]
        }
    } else {
        // Horizontal connection
        if to_cx > from_cx {
            vec![(fx + fw, from_cy), (tx, to_cy)]
        } else {
            vec![(fx, from_cy), (tx + tw, to_cy)]
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::component::{
        ComponentDiagram, ComponentEntity, ComponentGroup, ComponentKind, ComponentLink,
        ComponentNote,
    };

    fn empty_diagram() -> ComponentDiagram {
        ComponentDiagram {
            entities: vec![],
            links: vec![],
            groups: vec![],
            notes: vec![],
            direction: Default::default(),
        }
    }

    fn simple_entity(name: &str) -> ComponentEntity {
        ComponentEntity {
            name: name.to_string(),
            id: name.to_string(),
            code: name.to_string(),
            kind: ComponentKind::Component,
            stereotype: None,
            stereotypes: Vec::new(),
            description: vec![],
            parent: None,
            color: None,
            source_line: None,
        }
    }

    fn simple_link(from: &str, to: &str, label: &str) -> ComponentLink {
        ComponentLink {
            from: from.to_string(),
            to: to.to_string(),
            label: label.to_string(),
            dashed: false,
            direction_hint: None,
            arrow_len: 2,
            source_line: None,
            direction_inverted: false,
            head_arrow_triangle: false,
            tail_arrow_triangle: false,
        }
    }

    // 1. Empty diagram
    #[test]
    fn test_empty_diagram() {
        let d = empty_diagram();
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        assert!(layout.nodes.is_empty());
        assert!(layout.edges.is_empty());
        assert!(layout.notes.is_empty());
        assert!(layout.width > 0.0);
        assert!(layout.height > 0.0);
    }

    // 2. Single component
    #[test]
    fn test_single_component() {
        let d = ComponentDiagram {
            entities: vec![simple_entity("comp1")],
            links: vec![],
            groups: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        assert_eq!(layout.nodes.len(), 1);
        let n = &layout.nodes[0];
        assert_eq!(n.id, "comp1");
        assert!(n.width > 0.0);
        // Component kind: margin_top(20) + LINE_HEIGHT(16.2969) + margin_bottom(10) = 46.2969
        assert!(
            n.height > 40.0,
            "Component entity should be >40px tall: {}",
            n.height
        );
        assert!(n.x >= MARGIN);
        assert!(n.y >= MARGIN);
    }

    // 3. Two components with arrow
    #[test]
    fn test_two_components_with_arrow() {
        let d = ComponentDiagram {
            entities: vec![simple_entity("A"), simple_entity("B")],
            links: vec![simple_link("A", "B", "uses")],
            groups: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        assert_eq!(layout.nodes.len(), 2);
        assert_eq!(layout.edges.len(), 1);
        assert_eq!(layout.edges[0].label, "uses");
        assert!(!layout.edges[0].points.is_empty());
    }

    #[test]
    fn test_align_raw_path_d_matches_points_start() {
        let raw_d = "M39,113.03 C39,125.82 39,153.48 39,166.63";
        let points = vec![
            (33.0, 107.03),
            (33.0, 119.82),
            (33.0, 147.48),
            (33.0, 160.63),
        ];

        let aligned = align_raw_path_d(raw_d, &points, 7.0, 7.0);

        assert!(aligned.starts_with("M40,114.03"), "got: {aligned}");
        assert!(
            aligned.contains("C40,126.82 40,154.48 40,167.63"),
            "got: {aligned}"
        );
    }

    // 4. Grid layout (more than GRID_COLS entities)
    #[test]
    fn test_grid_layout() {
        let d = ComponentDiagram {
            entities: vec![
                simple_entity("A"),
                simple_entity("B"),
                simple_entity("C"),
                simple_entity("D"),
                simple_entity("E"),
            ],
            links: vec![],
            groups: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        assert_eq!(layout.nodes.len(), 5);

        // All nodes should have valid positions
        for n in &layout.nodes {
            assert!(n.x >= 0.0, "node {} x should be >= 0", n.id);
            assert!(n.y >= 0.0, "node {} y should be >= 0", n.id);
        }
    }

    // 5. Entity sizing
    #[test]
    fn test_entity_sizing() {
        let e = ComponentEntity {
            name: "A very long component name".to_string(),
            id: "long".to_string(),
            code: "long".to_string(),
            kind: ComponentKind::Component,
            stereotype: None,
            stereotypes: Vec::new(),
            description: vec![],
            parent: None,
            color: None,
            source_line: None,
        };
        let (w, _) = estimate_entity_size(&e, None);
        assert!(w > NODE_MIN_WIDTH, "long name should produce wider node");
    }

    // 6. Entity with description
    #[test]
    fn test_entity_with_description() {
        let e = ComponentEntity {
            name: "A".to_string(),
            id: "A".to_string(),
            code: "A".to_string(),
            kind: ComponentKind::Rectangle,
            stereotype: None,
            stereotypes: Vec::new(),
            description: vec![
                "line1".to_string(),
                "line2".to_string(),
                "line3".to_string(),
            ],
            parent: None,
            color: None,
            source_line: None,
        };
        let (_, h) = estimate_entity_size(&e, None);
        // When description is present, it replaces the name display.
        // So total lines = desc lines (3), not name + desc (4).
        let (_, _, mt, mb) = entity_margins(&ComponentKind::Rectangle);
        let expected = 3.0 * LINE_HEIGHT + mt + mb;
        assert!(
            h >= expected,
            "description should increase height: h={h} expected={expected}"
        );
    }

    // 7. Note layout
    #[test]
    fn test_note_layout() {
        let d = ComponentDiagram {
            entities: vec![simple_entity("A")],
            links: vec![],
            groups: vec![],
            notes: vec![ComponentNote {
                text: "important note".to_string(),
                position: "right".to_string(),
                target: Some("A".to_string()),
                source_line: None,
                is_block: false,
            }],
            direction: Default::default(),
        };
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        assert_eq!(layout.notes.len(), 1);
        let note = &layout.notes[0];
        assert!(note.width > 0.0);
        assert!(note.height > 0.0);
    }

    // 8. Dashed edge
    #[test]
    fn test_dashed_edge() {
        let d = ComponentDiagram {
            entities: vec![simple_entity("A"), simple_entity("B")],
            links: vec![ComponentLink {
                from: "A".to_string(),
                to: "B".to_string(),
                label: String::new(),
                dashed: true,
                direction_hint: None,
                arrow_len: 2,
                source_line: None,
                direction_inverted: false,
                head_arrow_triangle: false,
                tail_arrow_triangle: false,
            }],
            groups: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        assert!(layout.edges[0].dashed);
    }

    // 9. Direction hint routing
    #[test]
    fn test_direction_hint_routing() {
        let d = ComponentDiagram {
            entities: vec![simple_entity("A"), simple_entity("B")],
            links: vec![ComponentLink {
                from: "A".to_string(),
                to: "B".to_string(),
                label: String::new(),
                dashed: false,
                direction_hint: Some("right".to_string()),
                arrow_len: 2,
                source_line: None,
                direction_inverted: false,
                head_arrow_triangle: false,
                tail_arrow_triangle: false,
            }],
            groups: vec![],
            direction: Default::default(),
            notes: vec![],
        };
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        assert!(!layout.edges[0].points.is_empty());
    }

    // 10. Group layout
    #[test]
    fn test_group_layout() {
        let d = ComponentDiagram {
            entities: vec![
                ComponentEntity {
                    name: "Outer".to_string(),
                    id: "Outer".to_string(),
                    code: "Outer".to_string(),
                    kind: ComponentKind::Rectangle,
                    stereotype: None,
                    stereotypes: Vec::new(),
                    description: vec![],
                    parent: None,
                    color: None,
                    source_line: None,
                },
                ComponentEntity {
                    name: "Inner".to_string(),
                    id: "Inner".to_string(),
                    code: "Inner".to_string(),
                    kind: ComponentKind::Component,
                    stereotype: None,
                    stereotypes: Vec::new(),
                    description: vec![],
                    parent: Some("Outer".to_string()),
                    color: None,
                    source_line: None,
                },
            ],
            links: vec![],
            groups: vec![ComponentGroup {
                name: "Outer".to_string(),
                id: "Outer".to_string(),
                code: "Outer".to_string(),
                kind: ComponentKind::Rectangle,
                stereotype: None,
                stereotypes: Vec::new(),
                children: vec!["Inner".to_string()],
                source_line: None,
            }],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        let inner = layout.nodes.iter().find(|n| n.id == "Inner").unwrap();
        assert!(inner.width > 0.0);
        assert!(inner.height > 0.0);
    }

    // 11. Bounding box includes all elements
    #[test]
    fn test_bounding_box() {
        let d = ComponentDiagram {
            entities: vec![simple_entity("A"), simple_entity("B")],
            links: vec![simple_link("A", "B", "")],
            groups: vec![],
            notes: vec![ComponentNote {
                text: "note".to_string(),
                position: "right".to_string(),
                target: Some("A".to_string()),
                source_line: None,
                is_block: false,
            }],
            direction: Default::default(),
        };
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        for node in &layout.nodes {
            assert!(
                node.x + node.width <= layout.width,
                "node right {} exceeds width {}",
                node.x + node.width,
                layout.width
            );
        }
    }

    // 12. Note size estimation
    #[test]
    fn test_note_size_estimation() {
        let (w, h) = estimate_note_size("hello");
        // Java: width = textWidth + marginX1(6) + marginX2(15), no minimum
        assert!(w > 0.0, "note width must be positive, got {w}");
        assert!(h > 0.0, "note height must be positive, got {h}");

        let (_w2, h2) = estimate_note_size("line1\nline2\nline3");
        assert!(h2 > h, "multiline note should be taller");
    }

    // 13. Text width estimation
    #[test]
    fn test_text_width() {
        assert_eq!(text_width(""), 0.0);
        let expected_a = crate::font_metrics::text_width("a", "SansSerif", FONT_SIZE, false, false);
        assert!((text_width("a") - expected_a).abs() < 0.001);
        let expected_abc =
            crate::font_metrics::text_width("abc", "SansSerif", FONT_SIZE, false, false);
        assert!((text_width("abc") - expected_abc).abs() < 0.001);
    }

    // 14. Missing edge target
    #[test]
    fn test_missing_edge_target() {
        let d = ComponentDiagram {
            entities: vec![simple_entity("A")],
            links: vec![simple_link("A", "nonexistent", "")],
            groups: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        // Edge should be skipped for missing target
        assert_eq!(layout.edges.len(), 0);
    }

    // 15. Entity with stereotype sizing
    #[test]
    fn test_stereotype_sizing() {
        let e = ComponentEntity {
            name: "A".to_string(),
            id: "A".to_string(),
            code: "A".to_string(),
            kind: ComponentKind::Component,
            stereotype: Some("MyStereotype".to_string()),
            stereotypes: vec!["MyStereotype".to_string()],
            description: vec![],
            parent: None,
            color: None,
            source_line: None,
        };
        let (_, h) = estimate_entity_size(&e, None);
        let plain_e = simple_entity("A");
        let (_, h_plain) = estimate_entity_size(&plain_e, None);
        assert!(h > h_plain, "stereotype should increase height");
    }

    // 16. Multiple notes
    #[test]
    fn test_multiple_notes() {
        let d = ComponentDiagram {
            entities: vec![simple_entity("A")],
            links: vec![],
            groups: vec![],
            notes: vec![
                ComponentNote {
                    text: "note 1".to_string(),
                    position: "top".to_string(),
                    target: Some("A".to_string()),
                    source_line: None,
                    is_block: false,
                },
                ComponentNote {
                    text: "note 2".to_string(),
                    position: "bottom".to_string(),
                    target: Some("A".to_string()),
                    source_line: None,
                    is_block: false,
                },
            ],
            direction: Default::default(),
        };
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        assert_eq!(layout.notes.len(), 2);
    }

    // 17. LeftToRight direction: wider than tall
    #[test]
    fn test_left_to_right_direction() {
        use crate::model::diagram::Direction;
        let d = ComponentDiagram {
            entities: vec![simple_entity("A"), simple_entity("B")],
            links: vec![simple_link("A", "B", "")],
            groups: vec![],
            notes: vec![],
            direction: Direction::LeftToRight,
        };
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        assert_eq!(layout.nodes.len(), 2);
        assert!(layout.width > 0.0);
        assert!(layout.height > 0.0);
    }

    // 18. TopToBottom is the default
    #[test]
    fn test_top_to_bottom_is_default() {
        use crate::model::diagram::Direction;
        let d1 = ComponentDiagram {
            entities: vec![simple_entity("A"), simple_entity("B")],
            links: vec![],
            groups: vec![],
            notes: vec![],
            direction: Direction::TopToBottom,
        };
        let d2 = ComponentDiagram {
            entities: vec![simple_entity("A"), simple_entity("B")],
            links: vec![],
            groups: vec![],
            notes: vec![],
            direction: Default::default(),
        };
        let l1 = layout_component(&d1, &crate::style::SkinParams::default()).unwrap();
        let l2 = layout_component(&d2, &crate::style::SkinParams::default()).unwrap();

        // Default should match TopToBottom
        assert!((l1.width - l2.width).abs() < 0.01);
        assert!((l1.height - l2.height).abs() < 0.01);
    }

    // 19. BottomToTop direction: first node at bottom
    #[test]
    fn test_bottom_to_top_direction() {
        use crate::model::diagram::Direction;
        let d = ComponentDiagram {
            entities: vec![simple_entity("A"), simple_entity("B")],
            links: vec![simple_link("A", "B", "")],
            groups: vec![],
            notes: vec![],
            direction: Direction::BottomToTop,
        };
        let layout = layout_component(&d, &crate::style::SkinParams::default()).unwrap();
        assert_eq!(layout.nodes.len(), 2);
        assert!(layout.width > 0.0);
        assert!(layout.height > 0.0);
    }

    #[test]
    fn test_multiline_name_sizing() {
        let single = simple_entity("Web");
        let (_, h_single) = estimate_entity_size(&single, None);

        let multi = ComponentEntity {
            name: "Line1\nLine2\nLine3".to_string(),
            id: "multi".to_string(),
            code: "multi".to_string(),
            kind: ComponentKind::Component,
            stereotype: None,
            stereotypes: Vec::new(),
            description: vec![],
            parent: None,
            color: None,
            source_line: None,
        };
        let (_, h_multi) = estimate_entity_size(&multi, None);
        // 3 name lines should be taller than 1 name line
        assert!(
            h_multi > h_single,
            "multi-line name height {h_multi} should exceed single-line {h_single}"
        );
        // Height difference should be 2 * LINE_HEIGHT (2 extra lines)
        let diff = h_multi - h_single;
        assert!(
            (diff - 2.0 * LINE_HEIGHT).abs() < 0.01,
            "height diff {diff} should be ~{:.4}",
            2.0 * LINE_HEIGHT
        );
    }
}
