//! xychart / xychart-beta parser.
//!
//! Line-oriented port of upstream's jison grammar
//! (`diagrams/xychart/parser/xychart.jison`, 172 LoC) + the populate
//! step in `xychartDb.ts`. The grammar is simple enough that a
//! hand-rolled lexer-less parser suffices — each non-empty body line
//! starts with a keyword (`title`, `x-axis`, `y-axis`, `bar`, `line`,
//! `accTitle:`, `accDescr:`) and the remainder is tokenised locally.
//!
//! Extra work done here (beyond the jison):
//!   - Strip + parse an optional YAML frontmatter block (`--- … ---`)
//!     so frontmatter `config.xyChart` / `config.themeVariables.xyChart`
//!     overrides reach the model without going through the global
//!     `preprocess::Config`.
//!   - Strip + parse any number of `%%{init: { … }}%%` inline
//!     directives (upstream merge-order: default ← frontmatter ← init).
//!   - Normalise `\r\n` / `\r` to `\n`.
//!
//! All colour / numeric theme fields are picked up from the merged
//! `themeVariables.xyChart` block; axis / chart config from
//! `config.xyChart`.

use crate::error::{MermaidError, Result};
use crate::model::xychart::{
    AxisSpec, ChartOrientation, PlotSpec, XyAxisConfig, XychartConfig, XychartData, XychartDiagram,
    XychartThemeOverride,
};
use crate::model::DiagramMeta;

/// Parse an xychart source string into [`XychartDiagram`].
pub fn parse(source: &str) -> Result<XychartDiagram> {
    let normalized = normalize_newlines(source);

    // 1. Frontmatter.
    let (fm_title, fm_value, after_fm) = strip_frontmatter(&normalized);

    // 2. Collect any `%%{init: {...}}%%` directive blocks and strip them.
    let (init_values, body) = extract_init_directives(&after_fm);

    // 3. Merge config layers: frontmatter first, then init.
    let mut cfg = XychartConfig::default();
    let mut theme = XychartThemeOverride::default();
    let mut theme_name: Option<String> = None;
    if let Some(v) = fm_value.as_ref() {
        apply_top_level(v, &mut cfg, &mut theme, &mut theme_name);
    }
    for v in &init_values {
        apply_top_level(v, &mut cfg, &mut theme, &mut theme_name);
    }

    // Palette is resolved per-plot at layout time — the parser only
    // records the directive order.

    // 4. Parse the body.
    let mut diagram = XychartDiagram {
        meta: DiagramMeta::default(),
        config: cfg,
        theme_override: theme,
        data: XychartData::default(),
        theme_name,
    };
    if let Some(t) = fm_title {
        diagram.meta.title = Some(t);
    }

    let mut saw_header = false;
    let mut has_set_x = false;
    let mut has_set_y = false;
    let mut plot_index: usize = 0;

    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("%%") {
            continue;
        }
        if !saw_header {
            // Accept `xychart` / `xychart-beta`, optionally followed
            // by `vertical` / `horizontal` orientation hint.
            let rest = match line.strip_prefix("xychart-beta") {
                Some(r) => r,
                None => match line.strip_prefix("xychart") {
                    Some(r) => r,
                    None => {
                        return Err(MermaidError::Parse {
                            line: 1,
                            col: 1,
                            message: format!("expected 'xychart' header, got {line:?}"),
                        });
                    }
                },
            };
            let rest = rest.trim();
            if rest == "horizontal" {
                diagram.config.chart_orientation = ChartOrientation::Horizontal;
            } else if rest == "vertical" {
                diagram.config.chart_orientation = ChartOrientation::Vertical;
            }
            saw_header = true;
            continue;
        }

        // Recognise keyword lines.
        if let Some(rest) = strip_colon_keyword(line, "accTitle") {
            diagram.meta.acc_title = Some(rest.to_string());
            continue;
        }
        if let Some(rest) = strip_colon_keyword(line, "accDescr") {
            diagram.meta.acc_descr = Some(rest.to_string());
            continue;
        }
        if let Some(rest) = strip_keyword(line, "title") {
            let text = parse_text(rest);
            diagram.data.title = text.clone();
            diagram.meta.title = Some(text);
            continue;
        }
        if let Some(rest) = strip_keyword(line, "x-axis") {
            parse_x_axis(rest, &mut diagram.data)?;
            has_set_x = true;
            continue;
        }
        if let Some(rest) = strip_keyword(line, "y-axis") {
            parse_y_axis(rest, &mut diagram.data)?;
            has_set_y = true;
            continue;
        }
        if let Some(rest) = strip_keyword(line, "bar") {
            parse_plot_line(
                rest,
                /*is_bar=*/ true,
                &mut diagram.data,
                &mut plot_index,
                &mut has_set_x,
                &mut has_set_y,
            )?;
            continue;
        }
        if let Some(rest) = strip_keyword(line, "line") {
            parse_plot_line(
                rest,
                /*is_bar=*/ false,
                &mut diagram.data,
                &mut plot_index,
                &mut has_set_x,
                &mut has_set_y,
            )?;
            continue;
        }

        // Unknown — upstream's jison would error; be lenient here and
        // just skip (matches the other diagram parsers' behaviour).
    }

    if !saw_header {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: "empty xychart source (no header)".into(),
        });
    }

    Ok(diagram)
}

// ── Line utilities ───────────────────────────────────────────────────

fn normalize_newlines(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

/// Strip `keyword` followed by whitespace. Returns the trimmed remainder.
fn strip_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?;
    let first = rest.chars().next()?;
    if !first.is_whitespace() {
        return None;
    }
    Some(rest.trim_start())
}

/// Strip `keyword:` (with optional whitespace around the colon).
fn strip_colon_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    Some(rest.trim_start())
}

// ── Frontmatter ──────────────────────────────────────────────────────

fn strip_frontmatter(src: &str) -> (Option<String>, Option<serde_yml::Value>, String) {
    if !src.starts_with("---") {
        return (None, None, src.to_string());
    }
    let after_open = match src.find('\n') {
        Some(p) => &src[p + 1..],
        None => return (None, None, src.to_string()),
    };
    let mut body_end: Option<usize> = None;
    let mut rest_start: Option<usize> = None;
    let mut offset = 0usize;
    for seg in after_open.split_inclusive('\n') {
        let trimmed = seg.trim_end_matches('\n').trim_end();
        if trimmed == "---" {
            body_end = Some(offset);
            rest_start = Some(offset + seg.len());
            break;
        }
        offset += seg.len();
    }
    let Some(body_end) = body_end else {
        return (None, None, src.to_string());
    };
    let body = &after_open[..body_end];
    let rest = rest_start
        .map(|s| after_open[s..].to_string())
        .unwrap_or_default();

    let parsed: Option<serde_yml::Value> = serde_yml::from_str(body).ok();
    let mut title: Option<String> = None;
    if let Some(serde_yml::Value::Mapping(m)) = parsed.as_ref() {
        if let Some(t) = m.get(serde_yml::Value::String("title".into())) {
            title = yaml_to_string(t);
        }
    }
    (title, parsed, rest)
}

// ── %%{init:…}%% directive ───────────────────────────────────────────

fn extract_init_directives(src: &str) -> (Vec<serde_yml::Value>, String) {
    let mut values = Vec::new();
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Look for `%%{` on any position — preserving surrounding text.
        if i + 3 <= bytes.len() && &bytes[i..i + 3] == b"%%{" {
            // Find the matching `}%%` sequence. Track brace nesting.
            let mut depth = 0isize;
            let mut j = i + 2; // position of `{`
            let mut end: Option<usize> = None;
            while j < bytes.len() {
                match bytes[j] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            // Expect `%%` right after.
                            if j + 3 <= bytes.len() && &bytes[j + 1..j + 3] == b"%%" {
                                end = Some(j + 3);
                            }
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if let Some(end_pos) = end {
                let directive_body = &src[i + 3..end_pos - 3]; // between `%%{` and `}%%`
                                                               // Try parsing as JSON5-ish via serde_yml (which handles
                                                               // the `{"xyChart":{...}}` shape upstream uses). Prefix
                                                               // with `{` since directive body starts with e.g.
                                                               // `init: {...}` or `init:{...}`.
                let wrapped = format!("{{{directive_body}}}");
                if let Ok(val) = serde_yml::from_str::<serde_yml::Value>(&wrapped) {
                    if let serde_yml::Value::Mapping(m) = &val {
                        if let Some(init) = m.get(serde_yml::Value::String("init".into())) {
                            values.push(init.clone());
                        }
                    }
                }
                i = end_pos;
                // Eat a single trailing newline after the directive so
                // the body doesn't gain a blank line where the
                // directive used to be.
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    (values, out)
}

// ── Config/theme merge from YAML ────────────────────────────────────

fn apply_top_level(
    v: &serde_yml::Value,
    cfg: &mut XychartConfig,
    theme: &mut XychartThemeOverride,
    theme_name: &mut Option<String>,
) {
    let serde_yml::Value::Mapping(m) = v else {
        return;
    };
    let get = |key: &str| m.get(serde_yml::Value::String(key.into()));
    if let Some(serde_yml::Value::Mapping(inner)) = get("config") {
        apply_top_level(
            &serde_yml::Value::Mapping(inner.clone()),
            cfg,
            theme,
            theme_name,
        );
    }
    if let Some(xy) = get("xyChart") {
        apply_xychart_config(xy, cfg);
    }
    if let Some(tv) = get("themeVariables") {
        apply_theme_variables(tv, theme);
    }
    if let Some(name) = get("theme").and_then(yaml_to_string) {
        *theme_name = Some(name);
    }
}

fn apply_xychart_config(v: &serde_yml::Value, cfg: &mut XychartConfig) {
    let serde_yml::Value::Mapping(m) = v else {
        return;
    };
    let get = |key: &str| m.get(serde_yml::Value::String(key.into()));
    if let Some(x) = get("width").and_then(yaml_to_f64) {
        cfg.width = x;
    }
    if let Some(x) = get("height").and_then(yaml_to_f64) {
        cfg.height = x;
    }
    if let Some(x) = get("titleFontSize").and_then(yaml_to_f64) {
        cfg.title_font_size = x;
    }
    if let Some(x) = get("titlePadding").and_then(yaml_to_f64) {
        cfg.title_padding = x;
    }
    if let Some(x) = get("showTitle").and_then(yaml_to_bool) {
        cfg.show_title = x;
    }
    if let Some(x) = get("showDataLabel").and_then(yaml_to_bool) {
        cfg.show_data_label = x;
    }
    if let Some(x) = get("showDataLabelOutsideBar").and_then(yaml_to_bool) {
        cfg.show_data_label_outside_bar = x;
    }
    if let Some(x) = get("plotReservedSpacePercent").and_then(yaml_to_f64) {
        cfg.plot_reserved_space_percent = x;
    }
    if let Some(x) = get("chartOrientation").and_then(yaml_to_string) {
        cfg.chart_orientation = if x == "horizontal" {
            ChartOrientation::Horizontal
        } else {
            ChartOrientation::Vertical
        };
    }
    if let Some(ax) = get("xAxis") {
        apply_axis_config(ax, &mut cfg.x_axis);
    }
    if let Some(ax) = get("yAxis") {
        apply_axis_config(ax, &mut cfg.y_axis);
    }
}

fn apply_axis_config(v: &serde_yml::Value, ac: &mut XyAxisConfig) {
    let serde_yml::Value::Mapping(m) = v else {
        return;
    };
    let get = |key: &str| m.get(serde_yml::Value::String(key.into()));
    if let Some(x) = get("showLabel").and_then(yaml_to_bool) {
        ac.show_label = x;
    }
    if let Some(x) = get("labelFontSize").and_then(yaml_to_f64) {
        ac.label_font_size = x;
    }
    if let Some(x) = get("labelPadding").and_then(yaml_to_f64) {
        ac.label_padding = x;
    }
    if let Some(x) = get("showTitle").and_then(yaml_to_bool) {
        ac.show_title = x;
    }
    if let Some(x) = get("titleFontSize").and_then(yaml_to_f64) {
        ac.title_font_size = x;
    }
    if let Some(x) = get("titlePadding").and_then(yaml_to_f64) {
        ac.title_padding = x;
    }
    if let Some(x) = get("showTick").and_then(yaml_to_bool) {
        ac.show_tick = x;
    }
    if let Some(x) = get("tickLength").and_then(yaml_to_f64) {
        ac.tick_length = x;
    }
    if let Some(x) = get("tickWidth").and_then(yaml_to_f64) {
        ac.tick_width = x;
    }
    if let Some(x) = get("showAxisLine").and_then(yaml_to_bool) {
        ac.show_axis_line = x;
    }
    if let Some(x) = get("axisLineWidth").and_then(yaml_to_f64) {
        ac.axis_line_width = x;
    }
}

fn apply_theme_variables(v: &serde_yml::Value, theme: &mut XychartThemeOverride) {
    let serde_yml::Value::Mapping(m) = v else {
        return;
    };
    if let Some(xy) = m.get(serde_yml::Value::String("xyChart".into())) {
        apply_xychart_theme(xy, theme);
    }
}

fn apply_xychart_theme(v: &serde_yml::Value, theme: &mut XychartThemeOverride) {
    let serde_yml::Value::Mapping(m) = v else {
        return;
    };
    let get_str = |key: &str| {
        m.get(serde_yml::Value::String(key.into()))
            .and_then(yaml_to_string)
    };
    if let Some(s) = get_str("backgroundColor") {
        theme.background_color = Some(s);
    }
    if let Some(s) = get_str("titleColor") {
        theme.title_color = Some(s);
    }
    if let Some(s) = get_str("dataLabelColor") {
        theme.data_label_color = Some(s);
    }
    if let Some(s) = get_str("xAxisLabelColor") {
        theme.x_axis_label_color = Some(s);
    }
    if let Some(s) = get_str("xAxisLineColor") {
        theme.x_axis_line_color = Some(s);
    }
    if let Some(s) = get_str("xAxisTickColor") {
        theme.x_axis_tick_color = Some(s);
    }
    if let Some(s) = get_str("xAxisTitleColor") {
        theme.x_axis_title_color = Some(s);
    }
    if let Some(s) = get_str("yAxisLabelColor") {
        theme.y_axis_label_color = Some(s);
    }
    if let Some(s) = get_str("yAxisLineColor") {
        theme.y_axis_line_color = Some(s);
    }
    if let Some(s) = get_str("yAxisTickColor") {
        theme.y_axis_tick_color = Some(s);
    }
    if let Some(s) = get_str("yAxisTitleColor") {
        theme.y_axis_title_color = Some(s);
    }
    if let Some(s) = get_str("plotColorPalette") {
        theme.plot_color_palette = Some(s);
    }
}

fn yaml_to_string(v: &serde_yml::Value) -> Option<String> {
    match v {
        serde_yml::Value::String(s) => Some(s.clone()),
        serde_yml::Value::Number(n) => Some(n.to_string()),
        serde_yml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn yaml_to_f64(v: &serde_yml::Value) -> Option<f64> {
    match v {
        serde_yml::Value::Number(n) => n.as_f64(),
        serde_yml::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn yaml_to_bool(v: &serde_yml::Value) -> Option<bool> {
    match v {
        serde_yml::Value::Bool(b) => Some(*b),
        serde_yml::Value::String(s) => {
            let s = s.trim().to_ascii_lowercase();
            if s == "true" {
                Some(true)
            } else if s == "false" {
                Some(false)
            } else {
                None
            }
        }
        _ => None,
    }
}

// ── Body line parsers ────────────────────────────────────────────────

/// Parse a quoted `"..."` or bare alphanumeric run into a plain string.
/// Mirrors the jison `text` rule which accepts either form.
fn parse_text(s: &str) -> String {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('"') {
        if let Some(end) = inner.rfind('"') {
            return inner[..end].to_string();
        }
    }
    s.to_string()
}

/// Parse an x-axis directive body: optional title + optional data
/// (categories list or `N --> M` range).
fn parse_x_axis(rest: &str, data: &mut XychartData) -> Result<()> {
    let s = rest.trim();
    // Split into (title, data_tail). Data is whichever comes first
    // of `[` (band) or a signed number followed by `-->` (linear).
    let (title, tail) = split_axis_head(s);
    if !title.is_empty() {
        data.x_axis.set_title(title);
    }
    if tail.is_empty() {
        return Ok(());
    }
    if tail.starts_with('[') {
        let cats = parse_band_data(&tail)?;
        let title = data.x_axis.title().to_string();
        data.x_axis = AxisSpec::Band {
            title,
            categories: cats,
        };
    } else if let Some((min, max)) = parse_linear_range(&tail) {
        let title = data.x_axis.title().to_string();
        data.x_axis = AxisSpec::Linear { title, min, max };
    }
    Ok(())
}

fn parse_y_axis(rest: &str, data: &mut XychartData) -> Result<()> {
    let s = rest.trim();
    let (title, tail) = split_axis_head(s);
    if !title.is_empty() {
        data.y_axis.set_title(title);
    }
    if tail.is_empty() {
        return Ok(());
    }
    // y-axis only supports linear.
    if let Some((min, max)) = parse_linear_range(&tail) {
        let title = data.y_axis.title().to_string();
        data.y_axis = AxisSpec::Linear { title, min, max };
    }
    Ok(())
}

/// Split an axis body into (title_part, data_part). The title is
/// everything before the first `[`, `-->`, or signed-number token that
/// matches the linear-range form.
fn split_axis_head(s: &str) -> (String, String) {
    // First, find the earliest `[`.
    let bracket_pos = s.find('[');
    // And the earliest occurrence of a number-then-`-->` that marks a
    // linear range; the title would be everything before that number.
    // Walk the string token-by-token to find it.
    let lin_pos = find_linear_range_start(s);

    let split_at = match (bracket_pos, lin_pos) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    match split_at {
        Some(pos) => {
            let title = parse_text(s[..pos].trim());
            let tail = s[pos..].to_string();
            (title, tail)
        }
        None => (parse_text(s), String::new()),
    }
}

/// Scan for the first index where a signed-or-unsigned number is followed
/// (after whitespace) by `-->`, which marks the start of a linear range.
fn find_linear_range_start(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip to a possible number start.
        while i < bytes.len() && !is_number_start(bytes[i]) {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        // Consume the number.
        let start = i;
        if bytes[i] == b'+' || bytes[i] == b'-' {
            i += 1;
        }
        let mut saw_digit = false;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            if bytes[i].is_ascii_digit() {
                saw_digit = true;
            }
            i += 1;
        }
        if !saw_digit {
            continue;
        }
        // Skip whitespace, then check for `-->`.
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j + 3 <= bytes.len() && &bytes[j..j + 3] == b"-->" {
            return Some(start);
        }
    }
    None
}

fn is_number_start(b: u8) -> bool {
    b.is_ascii_digit() || b == b'+' || b == b'-' || b == b'.'
}

/// Parse `[a, b, c]` into a Vec<String>. Each element may be quoted.
fn parse_band_data(s: &str) -> Result<Vec<String>> {
    let s = s.trim();
    let inner = s
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| MermaidError::Parse {
            line: 0,
            col: 0,
            message: format!("expected `[...]` got {s:?}"),
        })?;
    let mut out = Vec::new();
    let mut depth = 0;
    let mut in_quote = false;
    let mut start = 0usize;
    let bytes = inner.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if in_quote {
            if b == b'"' {
                in_quote = false;
            }
            continue;
        }
        match b {
            b'"' => in_quote = true,
            b'[' => depth += 1,
            b']' => depth -= 1,
            b',' if depth == 0 => {
                out.push(parse_text(inner[start..i].trim()));
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        out.push(parse_text(last));
    }
    Ok(out)
}

/// Parse `MIN --> MAX` into `(min, max)`.
fn parse_linear_range(s: &str) -> Option<(f64, f64)> {
    let s = s.trim();
    let pos = s.find("-->")?;
    let left = s[..pos].trim();
    let right = s[pos + 3..].trim();
    let min: f64 = left.parse().ok()?;
    let max: f64 = right.parse().ok()?;
    Some((min, max))
}

/// Parse `[v1, v2, ...]` into a Vec<f64>.
fn parse_number_list(s: &str) -> Result<Vec<f64>> {
    let s = s.trim();
    let inner = s
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| MermaidError::Parse {
            line: 0,
            col: 0,
            message: format!("expected `[...]` got {s:?}"),
        })?;
    let mut out = Vec::new();
    for piece in inner.split(',') {
        let p = piece.trim();
        if p.is_empty() {
            continue;
        }
        let n: f64 = p.parse().map_err(|_| MermaidError::Parse {
            line: 0,
            col: 0,
            message: format!("invalid number in plot data: {p:?}"),
        })?;
        out.push(n);
    }
    Ok(out)
}

/// Parse a `bar`/`line` directive body (optional title + data list) and
/// append a [`PlotSpec`] to `data.plots`, transforming values to
/// `(category, value)` pairs exactly the way upstream's
/// `transformDataWithoutCategory` does.
fn parse_plot_line(
    rest: &str,
    is_bar: bool,
    data: &mut XychartData,
    plot_index: &mut usize,
    has_set_x: &mut bool,
    has_set_y: &mut bool,
) -> Result<()> {
    let s = rest.trim();
    // Optional title (quoted or bare alphanumeric) precedes the `[` list.
    let bracket_pos = s.find('[').ok_or_else(|| MermaidError::Parse {
        line: 0,
        col: 0,
        message: format!("expected `[...]` in plot line: {s:?}"),
    })?;
    let _title = parse_text(s[..bracket_pos].trim());
    let list = &s[bracket_pos..];
    let values = parse_number_list(list)?;
    if values.is_empty() {
        return Ok(());
    }

    // Auto-set x-axis range if none given: upstream uses `[1, n]`.
    if !*has_set_x {
        match &mut data.x_axis {
            AxisSpec::Linear { min, max, .. } => {
                let new_min = min.min(1.0);
                let new_max = max.max(values.len() as f64);
                *min = if new_min.is_finite() { new_min } else { 1.0 };
                *max = if new_max.is_finite() {
                    new_max
                } else {
                    values.len() as f64
                };
            }
            _ => {
                let title = data.x_axis.title().to_string();
                data.x_axis = AxisSpec::Linear {
                    title,
                    min: 1.0,
                    max: values.len() as f64,
                };
            }
        }
        *has_set_x = true;
    }

    // Auto-set y-axis from plot data (upstream accumulates across plots).
    if !*has_set_y {
        let (lo, hi) = min_max(&values);
        match &mut data.y_axis {
            AxisSpec::Linear { min, max, .. } => {
                *min = min.min(lo);
                *max = max.max(hi);
            }
            _ => {
                let title = data.y_axis.title().to_string();
                data.y_axis = AxisSpec::Linear {
                    title,
                    min: lo,
                    max: hi,
                };
            }
        }
    }

    // Produce `(category, value)` pairs.
    let pairs: Vec<(String, f64)> = match &data.x_axis {
        AxisSpec::Band { categories, .. } => categories
            .iter()
            .enumerate()
            .filter_map(|(i, c)| values.get(i).map(|&v| (c.clone(), v)))
            .collect(),
        AxisSpec::Linear { min, max, .. } => {
            let n = values.len();
            let mut cats: Vec<String> = Vec::with_capacity(n);
            if n > 1 {
                let step = (max - min) / (n as f64 - 1.0);
                let mut x = *min;
                for _ in 0..n {
                    cats.push(js_number_to_string(x));
                    x += step;
                }
            } else {
                cats.push(js_number_to_string(*min));
            }
            cats.into_iter().zip(values.iter().copied()).collect()
        }
    };

    let plot = if is_bar {
        PlotSpec::Bar {
            plot_index: *plot_index,
            data: pairs,
        }
    } else {
        PlotSpec::Line {
            plot_index: *plot_index,
            stroke_width: 2.0,
            data: pairs,
        }
    };
    data.plots.push(plot);
    *plot_index += 1;
    Ok(())
}

fn min_max(values: &[f64]) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &v in values {
        if v < lo {
            lo = v;
        }
        if v > hi {
            hi = v;
        }
    }
    (lo, hi)
}

/// Stringify a float using JavaScript's `Number.prototype.toString()`
/// rules — used for category keys generated from a linear x-axis.
fn js_number_to_string(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    if v.fract() == 0.0 && v.is_finite() && v.abs() < 1e21 {
        return format!("{}", v as i64);
    }
    format!("{}", v)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_line() {
        let d = parse("xychart-beta\n  line [10, 30, 20]\n").unwrap();
        assert_eq!(d.data.plots.len(), 1);
        match &d.data.plots[0] {
            PlotSpec::Line { data, .. } => {
                assert_eq!(data.len(), 3);
            }
            _ => panic!("expected line plot"),
        }
    }

    #[test]
    fn parses_title_and_band_axis() {
        let src = "xychart\n  title \"Sales Revenue\"\n  x-axis Months [jan, feb, mar]\n  y-axis \"Revenue (in $)\" 4000 --> 11000\n  bar [5000, 6000, 7500]\n";
        let d = parse(src).unwrap();
        assert_eq!(d.data.title, "Sales Revenue");
        match &d.data.x_axis {
            AxisSpec::Band { title, categories } => {
                assert_eq!(title, "Months");
                assert_eq!(categories, &vec!["jan", "feb", "mar"]);
            }
            _ => panic!("expected band axis"),
        }
        match &d.data.y_axis {
            AxisSpec::Linear { title, min, max } => {
                assert_eq!(title, "Revenue (in $)");
                assert_eq!(*min, 4000.0);
                assert_eq!(*max, 11000.0);
            }
            _ => panic!("expected linear axis"),
        }
    }

    #[test]
    fn parses_frontmatter_config() {
        let src = "---\nconfig:\n  xyChart:\n    width: 1000\n    chartOrientation: horizontal\n---\nxychart\n  bar [1,2,3]\n";
        let d = parse(src).unwrap();
        assert_eq!(d.config.width, 1000.0);
        assert_eq!(d.config.chart_orientation, ChartOrientation::Horizontal);
    }

    #[test]
    fn parses_init_directive() {
        let src = r#"%%{init: {"xyChart": {"width": 1000}}}%%
xychart
  bar [1,2,3]
"#;
        let d = parse(src).unwrap();
        assert_eq!(d.config.width, 1000.0);
    }
}
