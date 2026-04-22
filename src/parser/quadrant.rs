//! Hand-rolled parser for the `quadrantChart` diagram.
//!
//! Upstream jison grammar: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/quadrant-chart/parser/quadrant.jison
//! Upstream DB: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/quadrant-chart/quadrantDb.ts
//!
//! Grammar supported (covers every fixture in tests/ext_fixtures/{cypress,demos}/quadrant):
//!
//! ```text
//! quadrantChart
//!     title <free text>
//!     x-axis <text> [--> <text>]
//!     y-axis <text> [--> <text>]
//!     quadrant-1 <text>
//!     quadrant-2 <text>
//!     quadrant-3 <text>
//!     quadrant-4 <text>
//!     accTitle: <text>
//!     accDescr: <text>
//!     <label>[:::className]: [x, y] [styles]
//!     classDef <name> <styles>
//! ```
//!
//! We also hoover up `%%{init: {...}}%%` blocks to capture
//! `quadrantChart.*`, `themeVariables.*`, and `theme` overrides — the
//! standard preprocess pipeline strips these before our parser runs in
//! production, but the byte-exact test harness feeds raw `.mmd` bytes
//! directly so we re-detect them here.

use crate::error::{MermaidError, Result};
use crate::model::quadrant::{
    QuadrantClassDef, QuadrantConfigOverride, QuadrantDiagram, QuadrantPoint, QuadrantStyles,
};
use serde_json::Value;

pub fn parse(source: &str) -> Result<QuadrantDiagram> {
    let mut d = QuadrantDiagram::default();

    // 1. Extract any `%%{init: ...}%%` directives before the body scan.
    let body = extract_init_directives(source, &mut d);

    // 2. Strip `%%` whole-line comments (but we already consumed the
    //    `%%{...}%%` directives).
    let body = strip_line_comments(&body);

    // 3. Line-oriented scan. Upstream's jison allows blank lines and
    //    leading whitespace before every statement.
    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0;

    // Skip leading blank lines.
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }

    // Header: `quadrantChart`.
    if i >= lines.len() {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: "empty quadrantChart source".into(),
        });
    }
    let header = lines[i].trim();
    if !header.eq_ignore_ascii_case("quadrantChart") && !header.starts_with("quadrantChart") {
        return Err(MermaidError::Parse {
            line: i + 1,
            col: 1,
            message: format!("expected 'quadrantChart' header, got {header:?}"),
        });
    }
    i += 1;

    while i < lines.len() {
        let raw = lines[i];
        let line = raw.trim();
        i += 1;
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = strip_kw(line, "title") {
            d.meta.title = Some(sanitize_text(rest.trim()));
        } else if let Some(rest) = strip_kw_colon(line, "accTitle") {
            d.meta.acc_title = Some(rest.trim().to_string());
        } else if let Some(rest) = strip_kw_colon(line, "accDescr") {
            d.meta.acc_descr = Some(rest.trim().to_string());
        } else if let Some(rest) = strip_kw(line, "x-axis") {
            let (left, right) = split_axis_text(rest.trim());
            d.x_axis_left_text = sanitize_text(&left);
            d.x_axis_right_text = right.map(|s| sanitize_text(&s)).unwrap_or_default();
        } else if let Some(rest) = strip_kw(line, "y-axis") {
            let (bottom, top) = split_axis_text(rest.trim());
            d.y_axis_bottom_text = sanitize_text(&bottom);
            d.y_axis_top_text = top.map(|s| sanitize_text(&s)).unwrap_or_default();
        } else if let Some(rest) = strip_kw(line, "quadrant-1") {
            d.quadrant1_text = sanitize_text(rest.trim());
        } else if let Some(rest) = strip_kw(line, "quadrant-2") {
            d.quadrant2_text = sanitize_text(rest.trim());
        } else if let Some(rest) = strip_kw(line, "quadrant-3") {
            d.quadrant3_text = sanitize_text(rest.trim());
        } else if let Some(rest) = strip_kw(line, "quadrant-4") {
            d.quadrant4_text = sanitize_text(rest.trim());
        } else if let Some(rest) = strip_kw(line, "classDef") {
            let (name, styles) = parse_class_def(rest.trim(), i)?;
            d.classes.push(QuadrantClassDef { name, styles });
        } else {
            // Point line: `<label>[:::class]: [x, y] [styles]`.
            let point = parse_point_line(line, i)?;
            // Upstream `addPoints([point], ...)` prepends — new points
            // go to the FRONT of `data.points`. See quadrantBuilder.ts:
            //   this.data.points = [...points, ...this.data.points];
            d.points.insert(0, point);
        }
    }

    Ok(d)
}

// -------------------------------------------------------------------------------------------------
// Directive extraction.
// -------------------------------------------------------------------------------------------------

fn extract_init_directives(source: &str, d: &mut QuadrantDiagram) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 3 <= bytes.len() && &bytes[i..i + 3] == b"%%{" {
            // Brace / quote-aware scan for matching `}%%`.
            let mut j = i + 3;
            let mut depth: i32 = 1;
            let mut quote: Option<u8> = None;
            let mut escaped = false;
            let mut found_end: Option<usize> = None;
            while j < bytes.len() {
                let c = bytes[j];
                if let Some(q) = quote {
                    if escaped {
                        escaped = false;
                    } else if c == b'\\' {
                        escaped = true;
                    } else if c == q {
                        quote = None;
                    }
                    j += 1;
                    continue;
                }
                match c {
                    b'"' | b'\'' => quote = Some(c),
                    b'{' | b'[' => depth += 1,
                    b']' => depth -= 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 && j + 3 <= bytes.len() && &bytes[j + 1..j + 3] == b"%%" {
                            found_end = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if let Some(close) = found_end {
                let body = &source[i + 3..close];
                apply_directive_body(body, d);
                i = close + 3;
                // Drop an immediately-following newline to avoid a
                // blank residual line.
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
                continue;
            }
            // Unterminated — bail out of directive handling.
        }
        // Copy one UTF-8 char.
        let ch_len = source[i..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(1);
        out.push_str(&source[i..i + ch_len]);
        i += ch_len;
    }
    out
}

fn apply_directive_body(body: &str, d: &mut QuadrantDiagram) {
    // The body has the shape `init: { ... json5 ... }` (or
    // `initialize: { ... }`). Split on the first `:`.
    let trimmed = body.trim();
    let Some(pos) = trimmed.find(':') else {
        return;
    };
    let key = trimmed[..pos].trim().to_ascii_lowercase();
    if key != "init" && key != "initialize" {
        return;
    }
    let inner = trimmed[pos + 1..].trim();
    let Ok(value): std::result::Result<Value, _> = json5::from_str(inner) else {
        return;
    };
    let Some(obj) = value.as_object() else {
        return;
    };

    if let Some(Value::String(s)) = obj.get("theme") {
        d.theme_name = Some(s.clone());
    }
    if let Some(qc) = obj.get("quadrantChart").and_then(|v| v.as_object()) {
        apply_quadrant_chart_config(qc, &mut d.config);
    }
    if let Some(tv) = obj.get("themeVariables") {
        d.theme_overrides_json = Some(tv.clone());
    }
}

fn apply_quadrant_chart_config(
    obj: &serde_json::Map<String, Value>,
    c: &mut QuadrantConfigOverride,
) {
    macro_rules! set_num {
        ($key:literal, $field:ident) => {
            if let Some(v) = obj.get($key).and_then(|v| v.as_f64()) {
                c.$field = Some(v);
            }
        };
    }
    macro_rules! set_str {
        ($key:literal, $field:ident) => {
            if let Some(Value::String(s)) = obj.get($key) {
                c.$field = Some(s.clone());
            }
        };
    }
    set_num!("chartWidth", chart_width);
    set_num!("chartHeight", chart_height);
    set_num!("titlePadding", title_padding);
    set_num!("titleFontSize", title_font_size);
    set_num!("quadrantPadding", quadrant_padding);
    set_num!("quadrantTextTopPadding", quadrant_text_top_padding);
    set_num!("quadrantLabelFontSize", quadrant_label_font_size);
    set_num!(
        "quadrantInternalBorderStrokeWidth",
        quadrant_internal_border_stroke_width
    );
    set_num!(
        "quadrantExternalBorderStrokeWidth",
        quadrant_external_border_stroke_width
    );
    set_num!("xAxisLabelPadding", x_axis_label_padding);
    set_num!("xAxisLabelFontSize", x_axis_label_font_size);
    set_num!("yAxisLabelPadding", y_axis_label_padding);
    set_num!("yAxisLabelFontSize", y_axis_label_font_size);
    set_num!("pointTextPadding", point_text_padding);
    set_num!("pointLabelFontSize", point_label_font_size);
    set_num!("pointRadius", point_radius);
    set_str!("xAxisPosition", x_axis_position);
    set_str!("yAxisPosition", y_axis_position);
}

// -------------------------------------------------------------------------------------------------
// Body scanning helpers.
// -------------------------------------------------------------------------------------------------

fn strip_line_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("%%") && !trimmed.starts_with("%%{") {
            // Skip whole-line comment.
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// If `s` starts with `kw` followed by whitespace / EOL, return the
/// remainder after the keyword (leading whitespace preserved).
fn strip_kw<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    match rest.chars().next() {
        None => Some(rest),
        Some(c) if c.is_whitespace() => Some(rest),
        _ => None,
    }
}

/// `accTitle: ...` — keyword followed by optional whitespace then ':'.
fn strip_kw_colon<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    let rest = rest.trim_start();
    rest.strip_prefix(':')
}

/// Split an axis body on `-->` (actually `\-\-+\>` in jison — two or more
/// dashes then `>`), returning (left, Some(right)) or (whole, None).
/// Upstream quirk: if the source reads `x-axis Reach -->`, the jison
/// treats that as only the left side *with the arrow appended* — we
/// replicate `$2.text += " ⟶ "` here.
fn split_axis_text(raw: &str) -> (String, Option<String>) {
    // Find `->` with at least two leading dashes. The jison pattern
    // allows any surrounding whitespace to be consumed by the token.
    if let Some(idx) = find_arrow(raw) {
        let left = strip_quotes(raw[..idx.0].trim_end());
        let right_raw = raw[idx.1..].trim();
        if right_raw.is_empty() {
            // Lone arrow — append " ⟶ " to the left, no right.
            (format!("{} ⟶ ", left.trim_end()), None)
        } else {
            (left, Some(strip_quotes(right_raw)))
        }
    } else {
        (strip_quotes(raw.trim()), None)
    }
}

/// Jison's `["]`-lexer for STR strips the surrounding double-quotes.
/// Applied here to any axis / quadrant / title text that might arrive
/// quoted.
fn strip_quotes(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Locate an arrow `--+>`, returning `(start, end)` byte offsets or
/// `None`. End is exclusive of the `>`.
fn find_arrow(s: &str) -> Option<(usize, usize)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'-' && bytes[i + 1] == b'-' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j] == b'-' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'>' {
                return Some((i, j + 1));
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

/// Text sanitisation under `securityLevel = "strict"` (default).
/// Upstream `sanitizeText` wraps the string in DOMPurify then replaces
/// `<`, `>`, `=` with HTML entities. Because our output layer is SVG
/// text (not HTML) we only need the entity replacement — DOMPurify's
/// HTML tag stripping is a no-op on the plain-text fixtures we cover.
fn sanitize_text(s: &str) -> String {
    // In practice none of our quadrant fixtures contain `<`, `>` or
    // `=`, but we mirror the upstream escape to stay byte-exact if they
    // ever do. The trim mirrors upstream `text.trim()` in
    // `textSanitizer`.
    let trimmed = s.trim();
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '=' => out.push_str("&equals;"),
            _ => out.push(ch),
        }
    }
    out
}

// -------------------------------------------------------------------------------------------------
// Point / classDef parsing.
// -------------------------------------------------------------------------------------------------

/// Parse a line like `Campaign A:::class1: [0.3, 0.6] radius: 20`.
/// The jison grammar allows double-quoted strings and markdown strings
/// for the label; none of our fixtures use either so we accept any text
/// up to `:::` (class marker) or `:` (point-start).
fn parse_point_line(line: &str, lineno: usize) -> Result<QuadrantPoint> {
    // Find the start of `[` for the coordinates.
    let open = line.find('[').ok_or_else(|| MermaidError::Parse {
        line: lineno,
        col: 1,
        message: format!("point line missing '[': {line:?}"),
    })?;
    let close = line[open..].find(']').ok_or_else(|| MermaidError::Parse {
        line: lineno,
        col: 1,
        message: format!("point line missing ']': {line:?}"),
    })?;
    let close_abs = open + close;

    // Everything before `[` (minus the trailing `:` and optional
    // whitespace) is the label + optional class name.
    let head = &line[..open];
    // Strip the trailing `: `.
    let head_trim = head.trim_end();
    let head_trim = head_trim.strip_suffix(':').unwrap_or(head_trim);
    let head_trim = head_trim.trim_end();

    let (label, class_name) = if let Some(pos) = head_trim.find(":::") {
        let lbl = head_trim[..pos].trim().to_string();
        let class_start = pos + 3;
        let class = head_trim[class_start..].trim().to_string();
        (lbl, Some(class))
    } else {
        (head_trim.trim().to_string(), None)
    };

    // Coordinates.
    let coords_body = &line[open + 1..close_abs];
    let (x, y) = parse_coords(coords_body, lineno)?;

    // Trailing styles, if any.
    let rest = line[close_abs + 1..].trim();
    let styles = if rest.is_empty() {
        QuadrantStyles::default()
    } else {
        parse_styles_list(rest, lineno)?
    };

    Ok(QuadrantPoint {
        text: sanitize_text(&label),
        x,
        y,
        class_name,
        styles,
    })
}

fn parse_coords(body: &str, lineno: usize) -> Result<(f64, f64)> {
    let body = body.trim();
    let (xs, ys) = body.split_once(',').ok_or_else(|| MermaidError::Parse {
        line: lineno,
        col: 1,
        message: format!("expected 'x, y' in {body:?}"),
    })?;
    let x: f64 = xs.trim().parse().map_err(|e| MermaidError::Parse {
        line: lineno,
        col: 1,
        message: format!("bad x {xs:?}: {e}"),
    })?;
    let y: f64 = ys.trim().parse().map_err(|e| MermaidError::Parse {
        line: lineno,
        col: 1,
        message: format!("bad y {ys:?}: {e}"),
    })?;
    Ok((x, y))
}

fn parse_class_def(rest: &str, lineno: usize) -> Result<(String, QuadrantStyles)> {
    // `<name> <styles>` — whitespace-separated.
    let (name, rest) = rest
        .split_once(char::is_whitespace)
        .ok_or_else(|| MermaidError::Parse {
            line: lineno,
            col: 1,
            message: format!("classDef needs 'name styles', got {rest:?}"),
        })?;
    let styles = parse_styles_list(rest.trim(), lineno)?;
    Ok((name.trim().to_string(), styles))
}

/// Parse a comma-separated list of `key: value` pairs into the four
/// known style fields. Unknown keys are ignored (upstream would raise,
/// but we only consume well-formed fixtures).
fn parse_styles_list(body: &str, _lineno: usize) -> Result<QuadrantStyles> {
    let mut styles = QuadrantStyles::default();
    if body.is_empty() {
        return Ok(styles);
    }
    for pair in body.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let Some((key, value)) = pair.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "radius" => {
                if let Ok(n) = value.parse::<f64>() {
                    styles.radius = Some(n);
                }
            }
            "color" => styles.color = Some(value.to_string()),
            "stroke-color" => styles.stroke_color = Some(value.to_string()),
            "stroke-width" => styles.stroke_width = Some(value.to_string()),
            _ => {}
        }
    }
    Ok(styles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_chart() {
        let d = parse("quadrantChart\n").expect("parse");
        assert_eq!(d.quadrant1_text, "");
        assert!(d.points.is_empty());
    }

    #[test]
    fn parses_headered_chart() {
        let src = "quadrantChart\n  title T\n  x-axis A --> B\n  quadrant-1 One\n";
        let d = parse(src).expect("parse");
        assert_eq!(d.meta.title.as_deref(), Some("T"));
        assert_eq!(d.x_axis_left_text, "A");
        assert_eq!(d.x_axis_right_text, "B");
        assert_eq!(d.quadrant1_text, "One");
    }

    #[test]
    fn lone_arrow_x_axis() {
        let d = parse("quadrantChart\n  x-axis Reach -->\n").expect("parse");
        // `sanitize_text` trims the trailing space upstream appends
        // together with the arrow; final text is "Reach ⟶".
        assert_eq!(d.x_axis_left_text, "Reach ⟶");
        assert_eq!(d.x_axis_right_text, "");
    }

    #[test]
    fn parses_points_with_classes_and_styles() {
        let src = "quadrantChart\n  A:::cls: [0.1, 0.2] radius: 20\n  B: [0.5, 0.5]\n  classDef cls radius: 10, color: #abc\n";
        let d = parse(src).expect("parse");
        assert_eq!(d.points.len(), 2);
        // Prepend order — last-parsed comes first.
        assert_eq!(d.points[0].text, "B");
        assert_eq!(d.points[1].text, "A");
        assert_eq!(d.points[1].class_name.as_deref(), Some("cls"));
        assert_eq!(d.points[1].styles.radius, Some(20.0));
        assert_eq!(d.classes.len(), 1);
        assert_eq!(d.classes[0].name, "cls");
        assert_eq!(d.classes[0].styles.radius, Some(10.0));
    }

    #[test]
    fn captures_directive_config() {
        let src = "%%{init: {\"quadrantChart\": {\"chartWidth\": 600, \"chartHeight\": 600}}}%%\nquadrantChart\n";
        let d = parse(src).expect("parse");
        assert_eq!(d.config.chart_width, Some(600.0));
        assert_eq!(d.config.chart_height, Some(600.0));
    }

    #[test]
    fn captures_directive_theme() {
        let src = "%%{init: {\"theme\": \"forest\"}}%%\nquadrantChart\n";
        let d = parse(src).expect("parse");
        assert_eq!(d.theme_name.as_deref(), Some("forest"));
    }
}
