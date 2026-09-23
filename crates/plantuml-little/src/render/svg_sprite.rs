//! Convert SVG sprite content for inline rendering, matching Java's `SvgNanoParser`.
//!
//! Java's regex only matches `svg|path|g|circle|ellipse|text`.
//! - `<circle>` → native `<ellipse>` (via UEllipse)
//! - `<ellipse>` → `<path>` with 4 arcs (via UPath)
//! - `<path>` → translated `<path>`
//! - `<text>` → `<text>` with Font.PLAIN (no weight/style/decoration)
//! - `<rect>`, `<line>`, `<polyline>`, `<polygon>` → silently dropped
//! - Gradients: `url(#...)` fill references resolve to white (no gradient hoisting)

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt::Write;

use crate::klimt::svg::{fmt_coord, xml_escape};

thread_local! {
    static COLLECTED_GRADIENT_DEFS: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
    /// Map of id -> element content for `<use>` resolution within the current sprite.
    static SPRITE_DEFS_MAP: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    /// When true, all colors are converted to grayscale (skinparam monochrome true).
    static MONOCHROME_MODE: Cell<bool> = const { Cell::new(false) };
    /// Scale factor for coordinate conversion (1.0 = no scaling).
    /// Used by `convert_svg_elements_scaled` to pre-apply transforms.
    static SPRITE_SCALE: Cell<f64> = const { Cell::new(1.0) };
    /// Element-level scale override for arc radii.  When an element has a
    /// `scale(s)` transform, Java uses `s` (not the accumulated sprite scale)
    /// as the arc radius multiplier.  `None` means no override — use SPRITE_SCALE.
    static ELEMENT_ARC_SCALE: Cell<Option<f64>> = const { Cell::new(None) };
    /// Default stroke width inherited from the parent diagram context.
    /// Java UEllipse/UPath inherit the UGraphic's current stroke thickness.
    /// Sequence diagrams use 1.0 (UStroke.simple()), activity diagrams use 0.5.
    static DEFAULT_STROKE_WIDTH: Cell<f64> = const { Cell::new(1.0) };
    /// Optional color override from inline sprite parameters like `<$x{color=red}>`.
    static SPRITE_COLOR_OVERRIDE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Set monochrome mode for sprite rendering.
pub fn set_monochrome(enabled: bool) {
    MONOCHROME_MODE.with(|m| m.set(enabled));
}

/// Set the default stroke width for sprite elements that don't specify one.
/// Call this before converting sprite elements to match the parent diagram's context.
pub fn set_default_stroke_width(width: f64) {
    DEFAULT_STROKE_WIDTH.with(|w| w.set(width));
}

/// Get the default stroke width.
fn default_stroke_width() -> f64 {
    DEFAULT_STROKE_WIDTH.with(|w| w.get())
}

/// Get current sprite scale factor.
fn sprite_scale() -> f64 {
    SPRITE_SCALE.with(|s| s.get())
}

/// Apply sprite scale to a coordinate: `coord * scale`.
fn sc(v: f64) -> f64 {
    v * sprite_scale()
}

/// Scale a radius value for arc commands.
/// Uses the full accumulated sprite scale (including any element-level scale).
fn sc_arc(v: f64) -> f64 {
    sc(v)
}

pub fn clear_gradient_defs() {
    COLLECTED_GRADIENT_DEFS.with(|g| g.borrow_mut().clear());
}

pub fn take_gradient_defs() -> Vec<(String, String)> {
    COLLECTED_GRADIENT_DEFS.with(|g| std::mem::take(&mut *g.borrow_mut()))
}

/// Information about a sprite's viewBox dimensions.
#[derive(Debug, Clone)]
pub struct SpriteInfo {
    pub vb_width: f64,
    pub vb_height: f64,
}

/// Parse the viewBox from SVG content and return sprite dimensions.
pub fn sprite_info(svg: &str) -> SpriteInfo {
    let (w, h) = parse_viewbox(svg);
    SpriteInfo {
        vb_width: w,
        vb_height: h,
    }
}

/// Compute the gap between text and sprite: the space character width at the given font.
/// Java: the gap equals the space advance from getStringBounds.
pub fn sprite_text_gap(font_family: &str, font_size: f64, bold: bool, italic: bool) -> f64 {
    crate::font_metrics::char_width(' ', font_family, font_size, bold, italic)
}

/// Convert SVG sprite elements with scaling and absolute positioning.
///
/// All coordinates are multiplied by `scale`, then shifted by `(offset_x, offset_y)`.
/// Sizes (radius, width, height) are multiplied by `scale` but not shifted.
/// This matches Java's approach of pre-computing absolute coordinates instead of
/// using `<g transform>` wrappers.
pub fn convert_svg_elements_scaled(svg: &str, offset_x: f64, offset_y: f64, scale: f64) -> String {
    convert_svg_elements_scaled_with_options(svg, offset_x, offset_y, scale, None)
}

pub fn convert_svg_elements_scaled_with_options(
    svg: &str,
    offset_x: f64,
    offset_y: f64,
    scale: f64,
    color_override: Option<&str>,
) -> String {
    SPRITE_DEFS_MAP.with(|m| m.borrow_mut().clear());
    cache_defs_elements(svg);

    // Java's SvgNanoParser does NOT parse <defs> or gradient definitions.
    // Gradients are resolved to their first stop-color in resolve_gradient_url().
    // No gradient hoisting into parent <defs>.

    let mut buf = String::new();
    let inner = strip_svg_wrapper(svg);
    SPRITE_COLOR_OVERRIDE.with(|c| *c.borrow_mut() = color_override.map(|v| v.to_string()));
    SPRITE_SCALE.with(|s| s.set(scale));
    convert_elements(&mut buf, inner.trim(), offset_x, offset_y, None);
    SPRITE_SCALE.with(|s| s.set(1.0));
    SPRITE_COLOR_OVERRIDE.with(|c| *c.borrow_mut() = None);
    buf
}

/// Convert SVG sprite elements to path-based elements with absolute positioning.
///
/// `offset_x` and `offset_y` are added to all coordinates to position the sprite
/// content in the output SVG.  Returns a string containing `<path>`, `<text>`,
/// and other converted elements.
pub fn convert_svg_elements(svg: &str, offset_x: f64, offset_y: f64) -> String {
    // Cache <defs> elements for <use> resolution
    SPRITE_DEFS_MAP.with(|m| m.borrow_mut().clear());
    cache_defs_elements(svg);

    // Java's SvgNanoParser does NOT parse <defs> or gradient definitions.
    // Gradients are resolved to their first stop-color in resolve_gradient_url().
    // No gradient hoisting into parent <defs>.

    let mut buf = String::new();
    let inner = strip_svg_wrapper(svg);
    convert_elements(&mut buf, inner.trim(), offset_x, offset_y, None);
    buf
}

/// Extract and cache `<defs>` elements by id for `<use>` resolution.
fn cache_defs_elements(svg: &str) {
    let inner = strip_svg_wrapper(svg);
    let content = inner.trim();
    let mut pos = 0;
    while let Some(start) = content[pos..].find("<defs") {
        let abs_start = pos + start;
        if let Some(end) = content[abs_start..].find("</defs>") {
            let defs_block = &content[abs_start..abs_start + end + 7];
            // Find the opening <defs...> closing >
            if let Some(gt) = defs_block.find('>') {
                let defs_inner = &defs_block[gt + 1..defs_block.len() - 7]; // strip </defs>
                extract_defs_by_id(defs_inner.trim());
            }
            pos = abs_start + end + 7;
        } else {
            break;
        }
    }
}

/// Extract elements with `id` attributes from defs content into the thread-local map.
fn extract_defs_by_id(content: &str) {
    SPRITE_DEFS_MAP.with(|map| {
        let mut map = map.borrow_mut();
        let mut pos = 0;
        while pos < content.len() {
            // Skip comments
            if content[pos..].starts_with("<!--") {
                if let Some(end) = content[pos..].find("-->") {
                    pos += end + 3;
                    continue;
                }
            }
            if content.as_bytes().get(pos) != Some(&b'<') || content[pos..].starts_with("</") {
                // Step a whole character: the byte here is not `<`, so it may
                // be the lead of a multi-byte one, and the next iteration
                // slices `content` as a `str` at this offset. (The `pos += 1`
                // steps below are reached only when that byte is `<`.)
                pos += content[pos..].chars().next().map_or(1, char::len_utf8);
                continue;
            }
            // Parse element
            if let Some((element, consumed)) = parse_element(&content[pos..]) {
                if consumed == 0 {
                    pos += 1;
                    continue;
                }
                // Extract id attribute
                if let Some(id) = get_attr(&element, "id") {
                    map.insert(id.to_string(), element.clone());
                }
                pos += consumed;
            } else {
                pos += 1;
            }
        }
    });
}

/// Extract gradient `<defs>` from SVG content for inclusion in the parent SVG.
///
/// Returns a list of `(id, definition_xml)` pairs.  The caller must emit these
/// inside the root `<defs>` block and update fill references accordingly.
pub fn extract_gradient_defs(svg: &str) -> Vec<(String, String)> {
    let mut defs = Vec::new();
    let inner = strip_svg_wrapper(svg);
    collect_gradient_defs(inner.trim(), &mut defs);
    defs
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// Strip the outermost `<svg ...>...</svg>` wrapper, returning inner content.
fn strip_svg_wrapper(svg: &str) -> &str {
    let trimmed = svg.trim();
    // Find end of opening <svg ...> tag
    if let Some(gt) = trimmed.find('>') {
        let after_open = &trimmed[gt + 1..];
        // Remove closing </svg>
        if let Some(close) = after_open.rfind("</svg>") {
            return &after_open[..close];
        }
        return after_open;
    }
    trimmed
}

/// Parse viewBox attribute from SVG content.
fn parse_viewbox(svg: &str) -> (f64, f64) {
    if let Some(vb_start) = svg.find("viewBox=\"") {
        let rest = &svg[vb_start + 9..];
        if let Some(vb_end) = rest.find('"') {
            let vb_str = &rest[..vb_end];
            let parts: Vec<&str> = vb_str.split_whitespace().collect();
            if parts.len() == 4 {
                let w = parts[2].parse::<f64>().unwrap_or(100.0);
                let h = parts[3].parse::<f64>().unwrap_or(50.0);
                return (w, h);
            }
        }
    }
    // Fallback: try width/height attributes
    let w = parse_attr_f64(svg, "width").unwrap_or(100.0);
    let h = parse_attr_f64(svg, "height").unwrap_or(50.0);
    (w, h)
}

fn parse_attr_f64(s: &str, attr: &str) -> Option<f64> {
    let pattern = format!("{attr}=\"");
    if let Some(start) = s.find(&pattern) {
        let rest = &s[start + pattern.len()..];
        if let Some(end) = rest.find('"') {
            return rest[..end].trim_end_matches("px").parse::<f64>().ok();
        }
    }
    None
}

/// Parse a single XML attribute value from an element string.
fn get_attr<'a>(element: &'a str, attr: &str) -> Option<&'a str> {
    let pattern = format!("{attr}=\"");
    if let Some(start) = element.find(&pattern) {
        let rest = &element[start + pattern.len()..];
        if let Some(end) = rest.find('"') {
            return Some(&rest[..end]);
        }
    }
    None
}

/// Parse a style attribute and extract a specific property.
fn get_style_prop<'a>(style: &'a str, prop: &str) -> Option<&'a str> {
    let prefix = format!("{prop}:");
    for part in style.split(';') {
        let trimmed = part.trim();
        if let Some(rest) = trimmed.strip_prefix(prefix.as_str()) {
            return Some(rest.trim());
        }
    }
    None
}

/// Collect gradient definitions from the SVG content.
fn collect_gradient_defs(content: &str, defs: &mut Vec<(String, String)>) {
    // Find <defs>...</defs> blocks and extract gradients
    let mut pos = 0;
    while let Some(start) = content[pos..].find("<defs") {
        let abs_start = pos + start;
        if let Some(end) = content[abs_start..].find("</defs>") {
            let defs_content = &content[abs_start..abs_start + end + 7];
            // Extract individual gradient definitions
            extract_gradients_from_defs(defs_content, defs);
            pos = abs_start + end + 7;
        } else {
            break;
        }
    }
}

/// Extract gradient elements from a <defs> block.
fn extract_gradients_from_defs(defs_block: &str, out: &mut Vec<(String, String)>) {
    // Only extract linearGradient — radialGradient is resolved to first stop
    // color in get_fill_or(), matching Java's behavior.
    for tag in &["linearGradient"] {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        let mut pos = 0;
        while let Some(start) = defs_block[pos..].find(open.as_str()) {
            let abs_start = pos + start;
            if let Some(end) = defs_block[abs_start..].find(close.as_str()) {
                let grad = &defs_block[abs_start..abs_start + end + close.len()];
                if let Some(id) = get_attr(grad, "id") {
                    out.push((id.to_string(), normalize_gradient(grad, tag)));
                }
                pos = abs_start + end + close.len();
            } else {
                break;
            }
        }
    }
}

/// Normalize gradient XML to match Java's DOM serializer output:
/// - Attribute order: id, x1, x2, y1, y2 (for linear) or id, cx, cy, r, fx, fy (for radial)
/// - Child elements on same line, no extra whitespace
fn normalize_gradient(raw: &str, tag: &str) -> String {
    use std::fmt::Write;
    let mut result = String::new();

    // Build the opening tag with canonical attribute order
    let id = get_attr(raw, "id").unwrap_or("");
    write!(result, "<{tag} id=\"{id}\"").unwrap();
    // Java: spreadMethod (if not "pad") comes before coordinates
    if let Some(sm) = get_attr(raw, "spreadMethod") {
        if sm != "pad" {
            write!(result, " spreadMethod=\"{sm}\"").unwrap();
        }
    }
    if tag == "linearGradient" {
        for attr in &["x1", "x2", "y1", "y2", "gradientUnits", "gradientTransform"] {
            if let Some(v) = get_attr(raw, attr) {
                write!(result, " {attr}=\"{}\"", truncate_gradient_value(v)).unwrap();
            }
        }
    } else {
        for attr in &[
            "cx",
            "cy",
            "r",
            "fx",
            "fy",
            "gradientUnits",
            "gradientTransform",
        ] {
            if let Some(v) = get_attr(raw, attr) {
                write!(result, " {attr}=\"{}\"", truncate_gradient_value(v)).unwrap();
            }
        }
    }
    result.push('>');

    // Extract and append child <stop> elements with Java-canonical attribute order:
    // offset, stop-color, stop-opacity (matching Java SvgGraphics output)
    let close_tag = format!("</{tag}>");
    if let Some(inner_start) = raw.find('>') {
        let inner = &raw[inner_start + 1..raw.len() - close_tag.len()];
        for stop in inner.split("<stop") {
            let s = stop.trim();
            if s.is_empty() || !s.contains("offset") {
                continue;
            }
            let stop_raw = format!("<stop {s}");
            result.push_str("<stop");
            // Canonical order: offset, stop-color, stop-opacity
            if let Some(v) = get_attr(&stop_raw, "offset") {
                write!(result, " offset=\"{v}\"").unwrap();
            }
            if let Some(v) = get_attr(&stop_raw, "stop-color") {
                let color = if MONOCHROME_MODE.with(|m| m.get()) {
                    // normalize_hex_color already applies monochrome
                    normalize_hex_color(v)
                } else {
                    v.to_string()
                };
                write!(result, " stop-color=\"{color}\"").unwrap();
            }
            if let Some(v) = get_attr(&stop_raw, "stop-opacity") {
                // Java: integer values like "0" stay as-is, decimals get 4 places
                if let Ok(n) = v.parse::<f64>() {
                    if n == 0.0 {
                        result.push_str(" stop-opacity=\"0\"");
                    } else {
                        write!(result, " stop-opacity=\"{n:.4}\"").unwrap();
                    }
                } else {
                    write!(result, " stop-opacity=\"{v}\"").unwrap();
                }
            }
            result.push_str("/>");
        }
    }

    result.push_str(&close_tag);
    result
}

/// Truncate gradient coordinate values to 4 decimal places matching Java DecimalFormat("0.####").
/// Trailing zeros are stripped: "58.9717389%" → "58.9717%", "0.0000%" → "0%", "100.0000%" → "100%".
/// Non-numeric values (gradientUnits, gradientTransform) pass through unchanged.
fn truncate_gradient_value(v: &str) -> String {
    if let Some(num_str) = v.strip_suffix('%') {
        if let Ok(n) = num_str.parse::<f64>() {
            return format!("{}%", format_4dp(n));
        }
    }
    if let Ok(n) = v.parse::<f64>() {
        return format_4dp(n);
    }
    v.to_string()
}

/// Format a number with up to 4 decimal places, stripping trailing zeros.
/// Matches Java's DecimalFormat("0.####"): "58.9717389" → "58.9717", "0.0" → "0".
fn format_4dp(n: f64) -> String {
    let s = format!("{:.4}", n);
    if s.find('.').is_some() {
        let trimmed = s.trim_end_matches('0');
        if let Some(stripped) = trimmed.strip_suffix('.') {
            stripped.to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        s
    }
}

/// Recursively convert SVG elements to path-based output.
fn convert_elements(
    buf: &mut String,
    content: &str,
    ox: f64,
    oy: f64,
    parent_transform: Option<&str>,
) {
    let css_props = extract_css_text_props(content);
    convert_elements_inner(buf, content, ox, oy, ox, oy, parent_transform, &css_props);
}

/// Like convert_elements but with separate text offset.
/// Java: group transforms apply to shapes but NOT to text.
fn convert_elements_with_text_offset(
    buf: &mut String,
    content: &str,
    ox: f64,
    oy: f64,
    text_ox: f64,
    text_oy: f64,
    css_text_props: &[(String, String)],
) {
    convert_elements_inner(buf, content, ox, oy, text_ox, text_oy, None, css_text_props);
}

/// Extract CSS properties for the `text` tag from a `<style>` block.
/// Returns a simple list of (property, value) pairs.
fn extract_css_text_props(content: &str) -> Vec<(String, String)> {
    let mut props = Vec::new();
    // Find <style>...</style> block
    let start = match content.find("<style") {
        Some(s) => s,
        None => return props,
    };
    let close = match content[start..].find("</style>") {
        Some(c) => c,
        None => return props,
    };
    let style_block = &content[start..start + close];
    // Extract CSS body (skip CDATA wrapper if present)
    let css_body = if let Some(cdata) = style_block.find("<![CDATA[") {
        let body_start = cdata + 9;
        let body_end = style_block[body_start..]
            .find("]]>")
            .map_or(style_block.len(), |e| body_start + e);
        &style_block[body_start..body_end]
    } else if let Some(gt) = style_block.find('>') {
        &style_block[gt + 1..]
    } else {
        return props;
    };
    // Find `text {` selector (standalone word, not inside comments or other selectors)
    // Use regex-like matching: look for "text" preceded by whitespace/newline and followed by whitespace/{
    let lower = css_body.to_lowercase();
    let bytes = lower.as_bytes();
    for (i, _) in lower.match_indices("text") {
        // Must be at start or after whitespace/newline
        if i > 0 && !bytes[i - 1].is_ascii_whitespace() && bytes[i - 1] != b'\n' {
            continue;
        }
        // Must be followed by whitespace or {
        let after_pos = i + 4;
        if after_pos < bytes.len()
            && !bytes[after_pos].is_ascii_whitespace()
            && bytes[after_pos] != b'{'
        {
            continue;
        }
        // Found "text" selector — extract the rule body
        let after = &css_body[i..];
        if let Some(brace) = after.find('{') {
            if let Some(end_brace) = after[brace..].find('}') {
                let body = &after[brace + 1..brace + end_brace];
                for decl in body.split(';') {
                    let decl = decl.trim();
                    if let Some(colon) = decl.find(':') {
                        let prop = decl[..colon].trim().to_string();
                        let val = decl[colon + 1..].trim().trim_end_matches("px").to_string();
                        props.push((prop, val));
                    }
                }
                break;
            }
        }
    }
    props
}

fn convert_elements_inner(
    buf: &mut String,
    content: &str,
    ox: f64,
    oy: f64,
    text_ox: f64,
    text_oy: f64,
    parent_transform: Option<&str>,
    css_text_props: &[(String, String)],
) {
    let mut pos = 0;
    let mut iterations = 0;
    while pos < content.len() {
        iterations += 1;
        if iterations > 500 {
            log::warn!(
                "svg_sprite: exceeded 500 iterations at pos={}/{}, aborting",
                pos,
                content.len()
            );
            break;
        }
        // Skip whitespace, comments, and non-element text
        if content[pos..].starts_with("<!--") {
            if let Some(end) = content[pos..].find("-->") {
                pos += end + 3;
                continue;
            }
        }

        if content.as_bytes()[pos] != b'<' {
            // Step a whole character — this byte may lead a multi-byte one and
            // the checks below slice `content` as a `str` at `pos`.
            pos += content[pos..].chars().next().map_or(1, char::len_utf8);
            continue;
        }

        // Skip processing instructions, closing tags, and style blocks.
        // Java's SvgNanoParser regex sees through <defs> — it processes all
        // matching elements (path, g, circle, ellipse, text) regardless of
        // whether they're inside <defs>. So we skip only the <defs>/<symbol>
        // wrapper tags themselves, not their content.
        if content[pos..].starts_with("</")
            || content[pos..].starts_with("<?")
            || content[pos..].starts_with("<defs")
            || content[pos..].starts_with("<symbol")
            || content[pos..].starts_with("<style")
            || content[pos..].starts_with("<use")
        {
            // Skip to end of tag
            if let Some(end) = content[pos..].find('>') {
                let tag = &content[pos..pos + end + 1];
                // For <style>, skip the entire block including content
                if tag.starts_with("<style") && !tag.ends_with("/>") {
                    if let Some(close) = content[pos..].find("</style>") {
                        pos += close + 8;
                        continue;
                    }
                }
                // For <use>, skip the element (Java's regex doesn't match <use>)
                if tag.starts_with("<use") {
                    if tag.ends_with("/>") {
                        pos += end + 1;
                    } else if let Some(close) = content[pos..].find("</use>") {
                        pos += close + 6;
                    } else {
                        pos += end + 1;
                    }
                    continue;
                }
                // For <defs>, <symbol>, </defs>, </symbol>, </...>:
                // skip just the tag, process inner content normally
                pos += end + 1;
            } else {
                pos += 1;
            }
            continue;
        }

        // Try to parse an element
        if let Some((element, consumed)) = parse_element(&content[pos..]) {
            if consumed == 0 {
                // Safety: prevent infinite loop on zero-length parse
                pos += 1;
                continue;
            }
            // Inject CSS text{} font-size default into <text> elements that lack it.
            // Java sprite renderer applies CSS font-size but uses its own defaults
            // for fill (#000000) and font-family (sans-serif).
            let element = if element.starts_with("<text ") && !css_text_props.is_empty() {
                let mut e = element.clone();
                for (prop, val) in css_text_props {
                    if prop != "font-size" {
                        continue; // Only inject font-size from CSS
                    }
                    let attr = prop.as_str();
                    if get_attr(&e, attr).is_none()
                        && get_attr(&e, "style")
                            .and_then(|s| get_style_prop(s, attr))
                            .is_none()
                    {
                        e = format!("<text {}=\"{}\" {}", attr, val, &e[6..]);
                    }
                }
                e
            } else {
                element
            };
            convert_single_element_ext(
                buf,
                &element,
                ox,
                oy,
                text_ox,
                text_oy,
                parent_transform,
                css_text_props,
            );
            pos += consumed;
        } else {
            pos += 1;
        }
    }
}

/// Parse a single XML element (self-closing or with children).
/// Returns (element_text, bytes_consumed).
fn parse_element(s: &str) -> Option<(String, usize)> {
    if !s.starts_with('<') {
        return None;
    }

    // Get tag name
    let tag_name_end = s[1..]
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .map(|i| i + 1)?;
    let tag_name = &s[1..tag_name_end];

    // Self-closing tag: only check for /> before the first >
    let gt = s.find('>')?;
    // `gt - 1` is a byte index into arbitrary tag content, so compare bytes:
    // the character before `>` may be multi-byte.
    if gt >= 2 && s.as_bytes()[gt - 1] == b'/' {
        return Some((s[..gt + 1].to_string(), gt + 1));
    }

    // Find end of opening tag
    let gt = s.find('>')?;

    // Self-closing
    if s[..gt].ends_with('/') {
        return Some((s[..gt + 1].to_string(), gt + 1));
    }

    // Find matching closing tag
    let close_tag = format!("</{tag_name}>");
    let mut depth = 1;
    let mut search_pos = gt + 1;
    let mut guard = 0;
    while depth > 0 && search_pos < s.len() {
        guard += 1;
        if guard > 100 || depth > 5 {
            break;
        }
        let open_tag = format!("<{tag_name}");
        let next_open = s[search_pos..].find(open_tag.as_str());
        let next_close = s[search_pos..].find(close_tag.as_str());

        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                // Check if it's a real open tag (not just a substring match)
                let after_name = search_pos + o + open_tag.len();
                if after_name < s.len()
                    && (s.as_bytes()[after_name] == b' '
                        || s.as_bytes()[after_name] == b'>'
                        || s.as_bytes()[after_name] == b'/')
                {
                    depth += 1;
                }
                search_pos += o + 1;
            }
            (_, Some(c)) => {
                depth -= 1;
                if depth == 0 {
                    let end = search_pos + c + close_tag.len();
                    return Some((s[..end].to_string(), end));
                }
                search_pos += c + 1;
            }
            _ => break,
        }
    }

    // Fallback: treat as self-closing
    Some((s[..gt + 1].to_string(), gt + 1))
}

/// Convert a single SVG element to path-based output.
#[allow(dead_code)] // reserved for sprite path conversion
fn convert_single_element(
    buf: &mut String,
    element: &str,
    ox: f64,
    oy: f64,
    _parent_transform: Option<&str>,
) {
    convert_single_element_ext(buf, element, ox, oy, ox, oy, _parent_transform, &[]);
}

fn convert_single_element_ext(
    buf: &mut String,
    element: &str,
    ox: f64,
    oy: f64,
    text_ox: f64,
    text_oy: f64,
    _parent_transform: Option<&str>,
    css_text_props: &[(String, String)],
) {
    let tag = element_tag_name(element);

    // Handle element-level transform attribute by pre-applying it to coordinates.
    // Java applies transforms to coordinates before rendering, so the output
    // uses absolute coordinates instead of SVG transform attributes.
    let elem_transform = if tag != "g" && tag != "use" {
        get_attr(element, "transform")
    } else {
        None
    };

    // Check for affine transforms (rotate, matrix) that need full matrix application
    let affine_matrix = elem_transform.and_then(parse_affine_transform);

    if let Some(ref matrix) = affine_matrix {
        // Full affine transform: convert the element by transforming each point
        convert_element_with_affine(buf, element, tag, ox, oy, matrix);
    } else {
        // Simple translate/scale or no transform
        let (eff_ox, eff_oy, saved_scale) = if let Some(transform) = elem_transform {
            apply_element_transform(transform, ox, oy)
        } else {
            (ox, oy, None)
        };

        match tag {
            // Java SvgNanoParser regex only matches: svg|path|g|circle|ellipse|text
            // rect, line, polyline, polygon, image are silently dropped.
            "circle" => convert_circle(buf, element, eff_ox, eff_oy),
            "ellipse" => convert_ellipse(buf, element, eff_ox, eff_oy),
            "path" => convert_path(buf, element, eff_ox, eff_oy),
            "text" => convert_text(buf, element, text_ox, text_oy),
            "g" => convert_group(buf, element, ox, oy, text_ox, text_oy, css_text_props),
            "use" => convert_use(buf, element, ox, oy, text_ox, text_oy, css_text_props),
            _ => {}
        }

        if let Some(prev) = saved_scale {
            SPRITE_SCALE.with(|s| s.set(prev));
            ELEMENT_ARC_SCALE.with(|s| s.set(None));
        }
    }
}

/// Apply an element-level SVG transform to the current offset and scale.
/// Returns (new_ox, new_oy, saved_scale) where saved_scale is Some if the
/// SPRITE_SCALE was modified (needs restoration after rendering).
fn apply_element_transform(transform: &str, ox: f64, oy: f64) -> (f64, f64, Option<f64>) {
    let (tx, ty) = parse_translate(transform);
    let elem_scale = parse_scale(transform);

    if let Some(es) = elem_scale {
        // scale(s) transform: multiply current scale and adjust offset
        let prev_scale = sprite_scale();
        let new_scale = prev_scale * es;
        SPRITE_SCALE.with(|s| s.set(new_scale));
        // Java's SAX parser uses the element scale (not accumulated) for arc radii
        ELEMENT_ARC_SCALE.with(|s| s.set(Some(es)));
        (ox + sc(tx), oy + sc(ty), Some(prev_scale))
    } else if tx.abs() > 0.001 || ty.abs() > 0.001 {
        (ox + sc(tx), oy + sc(ty), None)
    } else {
        (ox, oy, None)
    }
}

/// Parse a full affine transform matrix from an SVG transform attribute.
/// Supports: translate(tx[,ty]), scale(s[,sy]), rotate(a[,cx,cy]), matrix(a,b,c,d,e,f)
/// Returns the combined [a, b, c, d, e, f] affine matrix.
fn parse_affine_transform(transform: &str) -> Option<[f64; 6]> {
    let t = transform.trim();
    if let Some(args) = t.strip_prefix("matrix(").and_then(|s| s.strip_suffix(')')) {
        let vals: Vec<f64> = args
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if vals.len() == 6 {
            return Some([vals[0], vals[1], vals[2], vals[3], vals[4], vals[5]]);
        }
    }
    if let Some(args) = t.strip_prefix("rotate(").and_then(|s| s.strip_suffix(')')) {
        let vals: Vec<f64> = args
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !vals.is_empty() {
            let angle = vals[0] * std::f64::consts::PI / 180.0;
            let (sin_a, cos_a) = angle.sin_cos();
            let (cx, cy) = if vals.len() >= 3 {
                (vals[1], vals[2])
            } else {
                (0.0, 0.0)
            };
            // rotate(a, cx, cy) = translate(cx,cy) * rotate(a) * translate(-cx,-cy)
            let e = cx - cx * cos_a + cy * sin_a;
            let f = cy - cx * sin_a - cy * cos_a;
            return Some([cos_a, sin_a, -sin_a, cos_a, e, f]);
        }
    }
    None
}

/// Apply an affine transform matrix [a,b,c,d,e,f] to a point (x,y).
fn affine_transform_point(m: &[f64; 6], x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// Convert an element by applying a full affine transform to its coordinates.
/// Used for rotate() and matrix() transforms where simple offset+scale isn't enough.
fn convert_element_with_affine(
    buf: &mut String,
    element: &str,
    tag: &str,
    ox: f64,
    oy: f64,
    matrix: &[f64; 6],
) {
    let s = sprite_scale();
    // Transform a point: apply element affine, then sprite scale + offset
    let tp = |x: f64, y: f64| -> (f64, f64) {
        let (tx, ty) = affine_transform_point(matrix, x, y);
        (tx * s + ox, ty * s + oy)
    };

    // Java SvgNanoParser regex only matches: svg|path|g|circle|ellipse|text
    // rect, line, polyline, polygon are silently dropped.
    match tag {
        "circle" => {
            let cx = get_attr(element, "cx")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
            let cy = get_attr(element, "cy")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
            let r = get_attr(element, "r")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
            // Under affine transform, circle becomes ellipse with transformed center
            let (acx, acy) = tp(cx, cy);
            let det = (matrix[0] * matrix[3] - matrix[1] * matrix[2]).abs();
            let avg_scale = det.sqrt();
            let ar = r * avg_scale * s;
            let fill = get_fill(element);
            let style = get_stroke_style(element);
            write!(
                buf,
                r#"<ellipse cx="{}" cy="{}" fill="{fill}" rx="{}" ry="{}""#,
                fmt_coord(acx),
                fmt_coord(acy),
                fmt_coord_raw(ar),
                fmt_coord_raw(ar),
            )
            .unwrap();
            if !style.is_empty() {
                write!(buf, r#" style="{style}""#).unwrap();
            }
            buf.push_str("/>");
        }
        _ => {
            // Unsupported element under affine transform — skip
        }
    }
}

fn element_tag_name(element: &str) -> &str {
    let s = element.strip_prefix('<').unwrap_or(element);
    let end = s
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(s.len());
    &s[..end]
}

// ── Element converters ──────────────────────────────────────────────────────

#[allow(dead_code)] // used by convert_single_element
fn convert_rect(buf: &mut String, element: &str, ox: f64, oy: f64) {
    let x = get_attr(element, "x")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let y = get_attr(element, "y")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let w = get_attr(element, "width")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let h = get_attr(element, "height")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);

    let ax = sc(x) + ox;
    let ay = sc(y) + oy;
    let ax2 = ax + sc(w);
    let ay2 = ay + sc(h);

    // Build path: M x,y L x+w,y L x+w,y+h L x,y+h L x,y
    let d = format!(
        "M{},{} L{},{} L{},{} L{},{} L{},{}",
        fmt_coord(ax),
        fmt_coord(ay),
        fmt_coord(ax2),
        fmt_coord(ay),
        fmt_coord(ax2),
        fmt_coord(ay2),
        fmt_coord(ax),
        fmt_coord(ay2),
        fmt_coord(ax),
        fmt_coord(ay),
    );

    let fill = get_fill(element);
    let style = get_stroke_style(element);
    // Java: shapes with gradient fill and no explicit stroke get a default
    // stroke matching the fill gradient (stroke-width:1)
    let style = if style.is_empty() && fill.starts_with("url(") {
        format!("stroke:{fill};stroke-width:1;")
    } else {
        style
    };

    write!(buf, r#"<path d="{d}" fill="{fill}""#).unwrap();
    if !style.is_empty() {
        write!(buf, r#" style="{style}""#).unwrap();
    }
    buf.push_str("/>");
}

fn convert_circle(buf: &mut String, element: &str, ox: f64, oy: f64) {
    let cx = get_attr(element, "cx")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let cy = get_attr(element, "cy")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let r_raw = get_attr(element, "r")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    // Arc radii use element scale only (matching Java SAX parser behavior)
    let r_arc = sc_arc(r_raw);

    let acx = sc(cx) + ox;
    let acy = sc(cy) + oy;

    // Java: circle → UEllipse → native <ellipse> element
    // Attribute order: cx, cy, fill, rx, ry, style (matching Java SvgGraphics)
    let fill = get_fill(element);
    let style = get_stroke_style(element);
    // Java UEllipse always emits stroke. When no explicit stroke is set,
    // it inherits the current UGraphic's stroke thickness.
    let style = if style.is_empty() {
        let sw = default_stroke_width();
        if (sw - sw.round()).abs() < f64::EPSILON {
            format!("stroke:{fill};stroke-width:{};", sw as i32)
        } else {
            format!("stroke:{fill};stroke-width:{sw};")
        }
    } else {
        style
    };

    write!(
        buf,
        r#"<ellipse cx="{}" cy="{}" fill="{fill}" rx="{}" ry="{}""#,
        fmt_coord(acx),
        fmt_coord(acy),
        fmt_coord_raw(r_arc),
        fmt_coord_raw(r_arc),
    )
    .unwrap();
    if !style.is_empty() {
        write!(buf, r#" style="{style}""#).unwrap();
    }
    buf.push_str("/>");
}

fn convert_ellipse(buf: &mut String, element: &str, ox: f64, oy: f64) {
    let cx = get_attr(element, "cx")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let cy = get_attr(element, "cy")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let rx_raw = get_attr(element, "rx")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let ry_raw = get_attr(element, "ry")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    // Point positions use the full accumulated sprite scale
    let rx_pos = sc(rx_raw);
    let ry_pos = sc(ry_raw);
    // Arc radii use element scale only (matching Java SAX parser behavior)
    let rx_arc = sc_arc(rx_raw);
    let ry_arc = sc_arc(ry_raw);

    let acx = sc(cx) + ox;
    let acy = sc(cy) + oy;

    // Ellipse as 4 arcs: M(left) → top → right → bottom → left (no trailing L close)
    // Java: UPath with moveTo + 4x arcTo + closePath — closePath produces no
    // extra coordinate in SVG path data since start and end coincide.
    let d = format!(
        "M{},{} A{rx},{ry} 0 0 1 {},{} A{rx},{ry} 0 0 1 {},{} A{rx},{ry} 0 0 1 {},{} A{rx},{ry} 0 0 1 {},{}",
        fmt_coord(acx - rx_pos), fmt_coord(acy),
        fmt_coord(acx), fmt_coord(acy - ry_pos),
        fmt_coord(acx + rx_pos), fmt_coord(acy),
        fmt_coord(acx), fmt_coord(acy + ry_pos),
        fmt_coord(acx - rx_pos), fmt_coord(acy),
        rx = fmt_coord_raw(rx_arc),
        ry = fmt_coord_raw(ry_arc),
    );

    let fill = get_fill(element);
    let style = get_stroke_style(element);

    write!(buf, r#"<path d="{d}" fill="{fill}""#).unwrap();
    if !style.is_empty() {
        write!(buf, r#" style="{style}""#).unwrap();
    }
    buf.push_str("/>");
}

#[allow(dead_code)] // used by convert_single_element
fn convert_line(buf: &mut String, element: &str, ox: f64, oy: f64) {
    let x1 = get_attr(element, "x1")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let y1 = get_attr(element, "y1")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let x2 = get_attr(element, "x2")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let y2 = get_attr(element, "y2")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);

    let d = format!(
        "M{},{} L{},{}",
        fmt_coord(sc(x1) + ox),
        fmt_coord(sc(y1) + oy),
        fmt_coord(sc(x2) + ox),
        fmt_coord(sc(y2) + oy),
    );

    let fill = get_fill_or(element, "#000000");
    let style = get_stroke_style(element);

    write!(buf, r#"<path d="{d}" fill="{fill}""#).unwrap();
    if !style.is_empty() {
        write!(buf, r#" style="{style}""#).unwrap();
    }
    buf.push_str("/>");
}

// NOTE: convert_polyline, convert_polygon removed — Java SvgNanoParser silently drops these.

fn convert_path(buf: &mut String, element: &str, ox: f64, oy: f64) {
    let d = get_attr(element, "d").unwrap_or("");
    let mut translated = translate_path_data(d, ox, oy);
    // Java's sprite path processing strips trailing Z from closed paths
    if translated.ends_with(" Z") {
        translated.truncate(translated.len() - 2);
    }

    let fill = get_fill(element);
    let style = get_stroke_style(element);

    write!(buf, r#"<path d="{translated}" fill="{fill}""#).unwrap();
    if !style.is_empty() {
        write!(buf, r#" style="{style}""#).unwrap();
    }
    buf.push_str("/>");
}

/// Get a text attribute from either a direct attribute or the style property.
fn get_text_attr_or<'a>(
    element: &'a str,
    attr: &str,
    style_prop: &str,
    default: &'a str,
) -> &'a str {
    get_attr(element, attr)
        .or_else(|| get_attr(element, "style").and_then(|s| get_style_prop(s, style_prop)))
        .unwrap_or(default)
}

fn convert_text(buf: &mut String, element: &str, ox: f64, oy: f64) {
    // Extract text content
    let inner = extract_element_content(element, "text");

    // Get attributes (check both attribute and style property)
    let mut x = sc(get_attr(element, "x")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0));
    let y = sc(get_attr(element, "y")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0));
    let fill = normalize_hex_color(get_text_attr_or(element, "fill", "fill", "#000000"));
    let font_family_raw = get_text_attr_or(element, "font-family", "font-family", "sans-serif");
    // Strip "px" suffix from font-size (CSS style may use "16px")
    let font_size_raw = get_text_attr_or(element, "font-size", "font-size", "14");
    let font_size = font_size_raw.trim_end_matches("px");
    let font_family = font_family_raw;
    // Java: sprite text always uses Font.PLAIN — no font-weight, font-style,
    // or text-decoration attributes are emitted.

    let size = font_size.parse::<f64>().unwrap_or(14.0);
    let bold = false;
    let italic = false;

    // Java joins multiline sprite SVG without newlines, so text content
    // between <text> and </text> may have leading spaces from source
    // indentation.  Java's DriverTextSvg.draw() advances x for each
    // leading space using the font's space advance, then trims.
    // Simulate this: strip newlines, count leading spaces, advance x.
    let joined = inner.replace(['\n', '\r'], "");
    let leading_spaces = joined.len() - joined.trim_start_matches(' ').len();
    if leading_spaces > 0 {
        let space_w = crate::font_metrics::char_width(' ', font_family, size, bold, italic);
        x += space_w * leading_spaces as f64;
    }

    let text_content = joined.trim();
    if text_content.is_empty() {
        return;
    }
    let text_length =
        crate::font_metrics::text_width(text_content, font_family, size, bold, italic);

    // Java's SvgNanoParser ignores the text-anchor attribute entirely.
    // It uses x directly without any anchor-based adjustment.

    // Java: "monospaced" → "monospace"
    let font_family = if font_family.eq_ignore_ascii_case("monospaced") {
        "monospace"
    } else {
        font_family
    };
    // Java: replace spaces with non-breaking space (&#160;) for monospace/courier fonts
    let text_output: std::borrow::Cow<str> = if font_family.eq_ignore_ascii_case("monospace")
        || font_family.eq_ignore_ascii_case("courier")
    {
        std::borrow::Cow::Owned(text_content.replace(' ', "\u{00A0}"))
    } else {
        std::borrow::Cow::Borrowed(text_content)
    };

    // Java: sprite text always uses Font.PLAIN — no font-style, font-weight,
    // or text-decoration attributes are emitted.
    write!(
        buf,
        r#"<text fill="{fill}" font-family="{font_family}" font-size="{font_size}""#,
    )
    .unwrap();
    write!(buf, r#" lengthAdjust="spacing""#).unwrap();
    write!(
        buf,
        r#" textLength="{}" x="{}" y="{}">{}</text>"#,
        fmt_coord(text_length),
        fmt_coord(x + ox),
        fmt_coord(y + oy),
        xml_escape(&text_output),
    )
    .unwrap();
}

// NOTE: <image> elements are silently dropped by Java's SvgNanoParser
// (its regex only matches svg|path|g|circle|ellipse|text).

fn convert_group(
    buf: &mut String,
    element: &str,
    ox: f64,
    oy: f64,
    text_ox: f64,
    text_oy: f64,
    css_text_props: &[(String, String)],
) {
    let inner = extract_element_content(element, "g");
    // Apply transform="translate(x,y)" to both shape and text coordinates.
    // Java's SvgNanoParser accumulates group transforms in the affine matrix,
    // which affects all child elements including text (via deltax/deltay).
    let (tx, ty) = if let Some(transform) = get_attr(element, "transform") {
        parse_translate(transform)
    } else {
        (0.0, 0.0)
    };
    // Scale the group's translation offset and apply to both shape and text
    convert_elements_with_text_offset(
        buf,
        inner.trim(),
        ox + sc(tx),
        oy + sc(ty),
        text_ox + sc(tx),
        text_oy + sc(ty),
        css_text_props,
    );
}

/// Handle `<use>` elements by resolving `xlink:href` or `href` to a `<defs>` element.
fn convert_use(
    buf: &mut String,
    element: &str,
    ox: f64,
    oy: f64,
    text_ox: f64,
    text_oy: f64,
    css_text_props: &[(String, String)],
) {
    // Get href (strips the leading #)
    let href = get_attr(element, "xlink:href")
        .or_else(|| get_attr(element, "href"))
        .unwrap_or("");
    let ref_id = href.strip_prefix('#').unwrap_or(href);
    if ref_id.is_empty() {
        return;
    }

    // Get position (apply sprite scale)
    let use_x = sc(get_attr(element, "x")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0));
    let use_y = sc(get_attr(element, "y")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0));

    // Get optional transform (e.g. scale)
    let mut scale_factor = 1.0_f64;
    let (tx, ty) = if let Some(transform) = get_attr(element, "transform") {
        // Handle scale transform by adjusting positions
        if let Some(scale) = parse_scale(transform) {
            // For scale, we adjust the position and scale the content
            // Java processes <use> by inlining the referenced content at the
            // scaled position
            scale_factor = scale;
            (use_x * scale, use_y * scale)
        } else {
            let (ptx, pty) = parse_translate(transform);
            (use_x + ptx, use_y + pty)
        }
    } else {
        (use_x, use_y)
    };

    // Look up the referenced element
    let ref_content = SPRITE_DEFS_MAP.with(|map| map.borrow().get(ref_id).cloned());

    if let Some(ref_element) = ref_content {
        // If there's a scale transform, pre-scale the referenced content
        let ref_element = if (scale_factor - 1.0).abs() > 0.001 {
            scale_svg_content(&ref_element, scale_factor)
        } else {
            ref_element
        };
        let tag_name = element_tag_name(&ref_element);
        match tag_name {
            "g" => {
                // Inline the group content at the use position
                let inner = extract_element_content(&ref_element, "g");
                convert_elements_with_text_offset(
                    buf,
                    inner.trim(),
                    ox + tx,
                    oy + ty,
                    text_ox + tx,
                    text_oy + ty,
                    css_text_props,
                );
            }
            "symbol" => {
                // Symbol: similar to g, but may have its own viewBox
                let inner = extract_element_content(&ref_element, "symbol");
                convert_elements_with_text_offset(
                    buf,
                    inner.trim(),
                    ox + tx,
                    oy + ty,
                    text_ox + tx,
                    text_oy + ty,
                    css_text_props,
                );
            }
            _ => {
                // Single element — render it at the offset position
                convert_single_element_ext(
                    buf,
                    &ref_element,
                    ox + tx,
                    oy + ty,
                    text_ox + tx,
                    text_oy + ty,
                    None,
                    css_text_props,
                );
            }
        }
    }
}

/// Parse scale(factor) from a transform string
fn parse_scale(transform: &str) -> Option<f64> {
    if let Some(start) = transform.find("scale(") {
        let rest = &transform[start + 6..];
        if let Some(end) = rest.find(')') {
            let val = &rest[..end];
            // Handle "scale(x,y)" or "scale(x)"
            let first = val.split(',').next().unwrap_or(val).trim();
            return first.parse::<f64>().ok();
        }
    }
    None
}

fn parse_translate(transform: &str) -> (f64, f64) {
    if let Some(start) = transform.find("translate(") {
        let rest = &transform[start + 10..];
        if let Some(end) = rest.find(')') {
            let coords = &rest[..end];
            let parts: Vec<&str> = coords.split(',').collect();
            if parts.len() == 2 {
                let x = parts[0].trim().parse::<f64>().unwrap_or(0.0);
                let y = parts[1].trim().parse::<f64>().unwrap_or(0.0);
                return (x, y);
            }
            // Try space separator
            let parts: Vec<&str> = coords.split_whitespace().collect();
            if parts.len() == 2 {
                let x = parts[0].parse::<f64>().unwrap_or(0.0);
                let y = parts[1].parse::<f64>().unwrap_or(0.0);
                return (x, y);
            }
        }
    }
    (0.0, 0.0)
}

/// Scale coordinate/size attributes in SVG content by a factor.
/// Uses regex-based approach to reliably scale attributes within SVG elements.
fn scale_svg_content(content: &str, scale: f64) -> String {
    let mut result = String::with_capacity(content.len() * 2);
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'<' {
            // We're at a tag start. Find the end of this tag.
            let tag_end = content[i..]
                .find('>')
                .map(|e| i + e + 1)
                .unwrap_or(content.len());
            let tag = &content[i..tag_end];

            // Scale numeric attributes: cx, cy, r, rx, ry, x, y, width, height, x1, y1, x2, y2
            let scaled_tag = scale_tag_attributes(tag, scale);
            result.push_str(&scaled_tag);
            i = tag_end;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

fn scale_tag_attributes(tag: &str, scale: f64) -> String {
    let coord_attrs = [
        "cx",
        "cy",
        "r",
        "rx",
        "ry",
        "x",
        "y",
        "x1",
        "y1",
        "x2",
        "y2",
        "width",
        "height",
        "stroke-width",
    ];
    let mut result = tag.to_string();

    // Scale coordinate attributes
    for attr_name in &coord_attrs {
        let pattern = format!("{}=\"", attr_name);
        if let Some(pos) = result.find(&pattern) {
            // Ensure it's a full attribute match (preceded by space or tag start)
            let before = if pos > 0 {
                result.as_bytes()[pos - 1]
            } else {
                b' '
            };
            if before != b' ' && before != b'\t' && before != b'\n' {
                continue;
            }
            let val_start = pos + pattern.len();
            if let Some(val_end) = result[val_start..].find('"') {
                let val_end = val_start + val_end;
                if let Ok(val) = result[val_start..val_end].parse::<f64>() {
                    let scaled = val * scale;
                    let new_val = crate::klimt::svg::fmt_coord(scaled);
                    let old = result[pos..val_end + 1].to_string();
                    let new = format!("{}=\"{}\"", attr_name, new_val);
                    result = result.replacen(&old, &new, 1);
                }
            }
        }
    }

    // Scale path d= data
    let d_pattern = "d=\"";
    if let Some(d_pos) = result.find(d_pattern) {
        let d_val_start = d_pos + d_pattern.len();
        if let Some(d_val_end_rel) = result[d_val_start..].find('"') {
            let d_val_end = d_val_start + d_val_end_rel;
            let d_str = result[d_val_start..d_val_end].to_string();
            let scaled_d = scale_path_data(&d_str, scale);
            result = format!(
                "{}d=\"{}\"{}",
                &result[..d_pos],
                scaled_d,
                &result[d_val_end + 1..]
            );
        }
    }

    // Scale stroke-width in style attribute
    if let Some(sw_pos) = result.find("stroke-width:") {
        let val_start = sw_pos + 13;
        // Find end of value (semicolon or quote)
        let mut val_end = val_start;
        while val_end < result.len() {
            let c = result.as_bytes()[val_end];
            if c == b';' || c == b'"' || c == b'\'' {
                break;
            }
            val_end += 1;
        }
        if let Ok(val) = result[val_start..val_end].parse::<f64>() {
            let scaled = val * scale;
            let old_frag = result[sw_pos..val_end].to_string();
            let new_frag = format!("stroke-width:{}", crate::klimt::svg::fmt_coord(scaled));
            result = result.replacen(&old_frag, &new_frag, 1);
        }
    }

    result
}

fn scale_path_data(d: &str, scale: f64) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(d.len() * 2);
    let mut chars = d.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit()
            || (ch == '-' && {
                // Check if this is a negative number sign (not part of a range)
                let prev = result.chars().last().unwrap_or(' ');
                prev == ' ' || prev == ',' || prev.is_ascii_alphabetic()
            })
            || (ch == '.'
                && result
                    .chars()
                    .last()
                    .map(|c| c == ' ' || c == ',')
                    .unwrap_or(true))
        {
            let mut num_str = String::new();
            if ch == '-' {
                num_str.push(ch);
                chars.next();
            }
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() || c == '.' {
                    num_str.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            if let Ok(val) = num_str.parse::<f64>() {
                write!(result, "{}", crate::klimt::svg::fmt_coord(val * scale)).unwrap();
            } else {
                result.push_str(&num_str);
            }
        } else {
            result.push(ch);
            chars.next();
        }
    }
    result
}

// ── Attribute helpers ───────────────────────────────────────────────────────

/// Normalize hex color to uppercase.  Java DOM serializes all hex colors in
/// uppercase (#RRGGBB). Expands 3-digit hex to 6-digit. Pass-through non-hex
/// values like "none" or "url(#id)".
fn normalize_hex_color(s: &str) -> String {
    // Convert CSS named colors to hex (Java always uses hex notation)
    let s = named_color_to_hex(s).unwrap_or_else(|| s.to_string());
    let normalized = if let Some(hex) = s.strip_prefix('#') {
        if hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let upper = hex.to_ascii_uppercase();
            if upper.len() == 3 {
                // Expand #RGB → #RRGGBB
                let mut expanded = String::with_capacity(7);
                expanded.push('#');
                for c in upper.chars() {
                    expanded.push(c);
                    expanded.push(c);
                }
                expanded
            } else {
                format!("#{}", upper)
            }
        } else {
            s.to_string()
        }
    } else {
        s.to_string()
    };

    // Apply monochrome conversion if enabled
    if MONOCHROME_MODE.with(|m| m.get()) {
        if let Some(mono) = to_monochrome_color(&normalized) {
            return mono;
        }
    }
    normalized
}

/// Convert a CSS named color to uppercase hex (#RRGGBB).
/// Returns None if the name is not recognized.
fn named_color_to_hex(name: &str) -> Option<String> {
    let hex = match name.to_lowercase().as_str() {
        "black" => "#000000",
        "white" => "#FFFFFF",
        "red" => "#FF0000",
        "green" => "#008000",
        "blue" => "#0000FF",
        "yellow" => "#FFFF00",
        "cyan" | "aqua" => "#00FFFF",
        "magenta" | "fuchsia" => "#FF00FF",
        "orange" => "#FFA500",
        "purple" => "#800080",
        "lime" => "#00FF00",
        "maroon" => "#800000",
        "navy" => "#000080",
        "olive" => "#808000",
        "teal" => "#008080",
        "silver" => "#C0C0C0",
        "gray" | "grey" => "#808080",
        "darkgray" | "darkgrey" => "#A9A9A9",
        "lightgray" | "lightgrey" => "#D3D3D3",
        "brown" => "#A52A2A",
        "pink" => "#FFC0CB",
        "coral" => "#FF7F50",
        "crimson" => "#DC143C",
        "gold" => "#FFD700",
        "indigo" => "#4B0082",
        "violet" => "#EE82EE",
        "tan" => "#D2B48C",
        "none" | "transparent" => return Some(name.to_string()),
        _ => return None,
    };
    Some(hex.to_string())
}

/// Convert a hex color to monochrome (grayscale).
/// Uses Java's integer formula: (r*299 + g*587 + b*114) / 1000
fn to_monochrome_color(color: &str) -> Option<String> {
    if !color.starts_with('#') || color.len() != 7 {
        return None;
    }
    let r = u8::from_str_radix(&color[1..3], 16).ok()?;
    let g = u8::from_str_radix(&color[3..5], 16).ok()?;
    let b = u8::from_str_radix(&color[5..7], 16).ok()?;
    let gray = ((r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000) as u8;
    Some(format!("#{:02X}{:02X}{:02X}", gray, gray, gray))
}

fn get_fill(element: &str) -> String {
    get_fill_or(element, "#000000")
}

fn get_fill_or(element: &str, default: &str) -> String {
    // Check fill attribute
    if let Some(fill) = get_attr(element, "fill") {
        let resolved = resolve_gradient_url(fill);
        return normalize_hex_color(&resolved);
    }
    // Check style attribute for fill
    if let Some(style) = get_attr(element, "style") {
        if let Some(fill) = get_style_prop(style, "fill") {
            let resolved = resolve_gradient_url(fill);
            return normalize_hex_color(&resolved);
        }
    }
    if let Some(color) = SPRITE_COLOR_OVERRIDE.with(|c| c.borrow().clone()) {
        return color;
    }
    default.to_string()
}

/// Java's `SvgNanoParser` does NOT parse `<defs>` or gradient definitions.
/// Its regex only matches `svg|path|g|circle|ellipse|text`, so gradient
/// elements are never stored.  Any `url(#...)` fill reference is unresolvable
/// and falls back to white (#FFFFFF).
fn resolve_gradient_url(fill: &str) -> String {
    if fill.starts_with("url(#") {
        return "#FFFFFF".to_string();
    }
    fill.to_string()
}

fn get_stroke_style(element: &str) -> String {
    let mut parts = Vec::new();

    // Collect stroke properties from attributes
    let stroke = get_attr(element, "stroke")
        .or_else(|| get_attr(element, "style").and_then(|s| get_style_prop(s, "stroke")));
    let stroke_width = get_attr(element, "stroke-width")
        .or_else(|| get_attr(element, "style").and_then(|s| get_style_prop(s, "stroke-width")));
    let stroke_dasharray = get_attr(element, "stroke-dasharray")
        .or_else(|| get_attr(element, "style").and_then(|s| get_style_prop(s, "stroke-dasharray")));

    if let Some(s) = stroke {
        parts.push(format!("stroke:{};", normalize_hex_color(s)));
    }
    if let Some(sw) = stroke_width {
        parts.push(format!("stroke-width:{sw};"));
    }
    if let Some(sd) = stroke_dasharray {
        parts.push(format!("stroke-dasharray:{sd};"));
    }

    parts.join("")
}

// NOTE: parse_points removed — only used by convert_polyline/convert_polygon which Java drops.

/// Translate path data by converting all commands to absolute, applying
/// sprite scale + offset.  Matches Java's `SvgPath` which converts every
/// command to absolute uppercase before rendering via `UPath`.
fn translate_path_data(d: &str, ox: f64, oy: f64) -> String {
    let mut result = String::new();
    let mut chars = d.chars().peekable();
    let mut current_cmd = ' ';
    // Current point (absolute, in sprite coordinate space before scale/offset).
    let mut cur_x = 0.0_f64;
    let mut cur_y = 0.0_f64;
    // Start point of current sub-path (for Z).
    let mut start_x = 0.0_f64;
    let mut start_y = 0.0_f64;

    while chars.peek().is_some() {
        // Skip whitespace
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }

        if chars.peek().is_none() {
            break;
        }

        let c = *chars.peek().unwrap();
        if c.is_alphabetic() {
            current_cmd = c;
            chars.next();
        }

        // Determine if command is relative (lowercase).
        let is_rel = current_cmd.is_ascii_lowercase();
        let upper = current_cmd.to_ascii_uppercase();

        // Emit absolute command letter.
        match upper {
            'Z' => {
                if !result.is_empty() {
                    result.push(' ');
                }
                result.push('Z');
                cur_x = start_x;
                cur_y = start_y;
            }
            'M' | 'L' | 'T' => {
                if let Some((rx, ry)) = parse_coord_pair(&mut chars) {
                    let (ax, ay) = if is_rel {
                        (cur_x + rx, cur_y + ry)
                    } else {
                        (rx, ry)
                    };
                    if !result.is_empty() {
                        result.push(' ');
                    }
                    result.push(upper);
                    write!(
                        result,
                        "{},{}",
                        fmt_coord(sc(ax) + ox),
                        fmt_coord(sc(ay) + oy)
                    )
                    .unwrap();
                    cur_x = ax;
                    cur_y = ay;
                    if upper == 'M' {
                        start_x = ax;
                        start_y = ay;
                    }
                }
            }
            'H' => {
                if let Some(v) = parse_number(&mut chars) {
                    let ax = if is_rel { cur_x + v } else { v };
                    // Emit as L ax,cur_y
                    if !result.is_empty() {
                        result.push(' ');
                    }
                    result.push('L');
                    write!(
                        result,
                        "{},{}",
                        fmt_coord(sc(ax) + ox),
                        fmt_coord(sc(cur_y) + oy)
                    )
                    .unwrap();
                    cur_x = ax;
                }
            }
            'V' => {
                if let Some(v) = parse_number(&mut chars) {
                    let ay = if is_rel { cur_y + v } else { v };
                    // Emit as L cur_x,ay
                    if !result.is_empty() {
                        result.push(' ');
                    }
                    result.push('L');
                    write!(
                        result,
                        "{},{}",
                        fmt_coord(sc(cur_x) + ox),
                        fmt_coord(sc(ay) + oy)
                    )
                    .unwrap();
                    cur_y = ay;
                }
            }
            'A' => {
                // Arc: rx,ry x-rotation large-arc sweep x,y
                if let Some(rrx) = parse_number(&mut chars) {
                    skip_comma(&mut chars);
                    if let Some(rry) = parse_number(&mut chars) {
                        skip_whitespace_comma(&mut chars);
                        if let Some(rot) = parse_number(&mut chars) {
                            skip_whitespace_comma(&mut chars);
                            if let Some(large) = parse_number(&mut chars) {
                                skip_whitespace_comma(&mut chars);
                                if let Some(sweep) = parse_number(&mut chars) {
                                    skip_whitespace_comma(&mut chars);
                                    if let Some((ex, ey)) = parse_coord_pair(&mut chars) {
                                        let (ax, ay) = if is_rel {
                                            (cur_x + ex, cur_y + ey)
                                        } else {
                                            (ex, ey)
                                        };
                                        if !result.is_empty() {
                                            result.push(' ');
                                        }
                                        result.push('A');
                                        write!(
                                            result,
                                            "{},{} {} {} {} {},{}",
                                            fmt_coord_raw(sc(rrx)),
                                            fmt_coord_raw(sc(rry)),
                                            rot as i32,
                                            large as i32,
                                            sweep as i32,
                                            fmt_coord(sc(ax) + ox),
                                            fmt_coord(sc(ay) + oy),
                                        )
                                        .unwrap();
                                        cur_x = ax;
                                        cur_y = ay;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            'C' => {
                // Cubic bezier: x1,y1 x2,y2 x,y
                let mut pts = [(0.0, 0.0); 3];
                let mut ok = true;
                for p in pts.iter_mut() {
                    if let Some((rx, ry)) = parse_coord_pair(&mut chars) {
                        *p = if is_rel {
                            (cur_x + rx, cur_y + ry)
                        } else {
                            (rx, ry)
                        };
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    if !result.is_empty() {
                        result.push(' ');
                    }
                    result.push('C');
                    for (i, (ax, ay)) in pts.iter().enumerate() {
                        let sep = if i == 0 { "" } else { " " };
                        write!(
                            result,
                            "{sep}{},{}",
                            fmt_coord(sc(*ax) + ox),
                            fmt_coord(sc(*ay) + oy)
                        )
                        .unwrap();
                    }
                    cur_x = pts[2].0;
                    cur_y = pts[2].1;
                }
            }
            'S' => {
                // Smooth cubic: x2,y2 x,y
                let mut pts = [(0.0, 0.0); 2];
                let mut ok = true;
                for p in pts.iter_mut() {
                    if let Some((rx, ry)) = parse_coord_pair(&mut chars) {
                        *p = if is_rel {
                            (cur_x + rx, cur_y + ry)
                        } else {
                            (rx, ry)
                        };
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    if !result.is_empty() {
                        result.push(' ');
                    }
                    result.push('S');
                    for (i, (ax, ay)) in pts.iter().enumerate() {
                        let sep = if i == 0 { "" } else { " " };
                        write!(
                            result,
                            "{sep}{},{}",
                            fmt_coord(sc(*ax) + ox),
                            fmt_coord(sc(*ay) + oy)
                        )
                        .unwrap();
                    }
                    cur_x = pts[1].0;
                    cur_y = pts[1].1;
                }
            }
            'Q' => {
                // Quadratic bezier: x1,y1 x,y
                let mut pts = [(0.0, 0.0); 2];
                let mut ok = true;
                for p in pts.iter_mut() {
                    if let Some((rx, ry)) = parse_coord_pair(&mut chars) {
                        *p = if is_rel {
                            (cur_x + rx, cur_y + ry)
                        } else {
                            (rx, ry)
                        };
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    if !result.is_empty() {
                        result.push(' ');
                    }
                    result.push('Q');
                    for (i, (ax, ay)) in pts.iter().enumerate() {
                        let sep = if i == 0 { "" } else { " " };
                        write!(
                            result,
                            "{sep}{},{}",
                            fmt_coord(sc(*ax) + ox),
                            fmt_coord(sc(*ay) + oy)
                        )
                        .unwrap();
                    }
                    cur_x = pts[1].0;
                    cur_y = pts[1].1;
                }
            }
            _ => {
                // Unknown command, try to skip one number
                if parse_number(&mut chars).is_some() {
                    // consumed
                }
            }
        }
    }

    result
}

fn parse_coord_pair(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<(f64, f64)> {
    skip_whitespace_comma(chars);
    let x = parse_number(chars)?;
    skip_comma(chars);
    let y = parse_number(chars)?;
    Some((x, y))
}

fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<f64> {
    skip_whitespace_comma(chars);
    let mut s = String::new();
    // Optional sign
    if chars.peek().is_some_and(|&c| c == '-' || c == '+') {
        s.push(chars.next().unwrap());
    }
    // Digits and decimal point
    while chars
        .peek()
        .is_some_and(|&c| c.is_ascii_digit() || c == '.')
    {
        s.push(chars.next().unwrap());
    }
    if s.is_empty() || s == "-" || s == "+" {
        return None;
    }
    s.parse::<f64>().ok()
}

fn skip_comma(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while chars.peek().is_some_and(|&c| c == ',' || c.is_whitespace()) {
        chars.next();
    }
}

fn skip_whitespace_comma(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while chars.peek().is_some_and(|&c| c.is_whitespace() || c == ',') {
        chars.next();
    }
}

fn extract_element_content<'a>(element: &'a str, tag: &str) -> &'a str {
    let close = format!("</{tag}>");
    if let Some(gt) = element.find('>') {
        let after = &element[gt + 1..];
        if let Some(close_pos) = after.rfind(close.as_str()) {
            return &after[..close_pos];
        }
        return after;
    }
    ""
}

/// Format a raw f64 without trailing zeros (for use in arc radii etc.)
fn fmt_coord_raw(v: f64) -> String {
    if v == v.floor() {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_to_path() {
        let mut buf = String::new();
        let elem = "<rect x=\"10\" y=\"10\" width=\"80\" height=\"30\" fill=\"#FF0000\" stroke=\"#000000\" stroke-width=\"2\"/>";
        convert_rect(&mut buf, elem, 71.3804, 50.2969);
        assert!(buf.contains("<path"));
        assert!(buf.contains("fill=\"#FF0000\""));
        assert!(buf.contains("stroke:#000000"));
    }

    #[test]
    fn test_circle_to_ellipse() {
        let mut buf = String::new();
        let elem = "<circle cx=\"18\" cy=\"18\" r=\"10\" fill=\"#FF0000\"/>";
        convert_circle(&mut buf, elem, 71.3804, 50.2969);
        assert!(buf.contains("<ellipse"), "expected <ellipse>, got: {buf}");
        assert!(buf.contains("rx=\"10\""), "expected rx=10, got: {buf}");
        assert!(buf.contains("ry=\"10\""), "expected ry=10, got: {buf}");
        assert!(buf.contains("fill=\"#FF0000\""));
    }

    #[test]
    fn test_line_to_path() {
        let mut buf = String::new();
        let elem =
            "<line x1=\"0\" y1=\"2\" x2=\"100\" y2=\"2\" stroke=\"#FF0000\" stroke-width=\"4\"/>";
        convert_line(&mut buf, elem, 71.3804, 61.4297);
        assert!(buf.contains("<path"));
        assert!(buf.contains("stroke:#FF0000"));
    }

    #[test]
    fn test_strip_svg_wrapper() {
        let svg = r#"<svg viewBox="0 0 100 50" xmlns="http://www.w3.org/2000/svg"><rect/></svg>"#;
        assert_eq!(strip_svg_wrapper(svg), "<rect/>");
    }

    #[test]
    fn test_viewbox_parse() {
        let info = sprite_info(
            r#"<svg viewBox="0 0 100 50" xmlns="http://www.w3.org/2000/svg"><rect/></svg>"#,
        );
        assert_eq!(info.vb_width, 100.0);
        assert_eq!(info.vb_height, 50.0);
    }

    #[test]
    fn test_path_relative_commands() {
        // Relative arc + H/V/h/v commands (bootstrap icon path)
        let d = "M16 8A8 8 0 1 1 0 8a8 8 0 0 1 16 0M9.283 4.002H7.971L6.072 5.385v1.271l1.834-1.318h.065V12h1.312z";
        let result = translate_path_data(d, 10.0, 20.0);
        // All commands should be absolute; M translated, arcs have absolute endpoints
        assert!(
            result.starts_with("M26,28"),
            "expected M26,28, got: {result}"
        );
        // 'a' (relative arc) should become absolute 'A'
        assert!(
            result.contains(" A8,8 0 0 1 26,28"),
            "expected absolute arc, got: {result}"
        );
        // 'H' should become 'L' with translated x
        assert!(result.contains(" L17.971,24.002"), "H -> L, got: {result}");
        // 'v' (relative vertical) should become 'L' with translated absolute y
        assert!(result.contains("L16.072,26.656"), "v -> L, got: {result}");
    }

    #[test]
    fn test_path_absolute_commands_unchanged() {
        // Simple absolute M L Z: offsets applied correctly
        let d = "M0,0 L10,0 L10,10 L0,10 Z";
        let result = translate_path_data(d, 5.0, 3.0);
        assert_eq!(result, "M5,3 L15,3 L15,13 L5,13 Z");
    }
}
