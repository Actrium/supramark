//! Timeline parser — hand-rolled to match upstream
//! `diagrams/timeline/parser/timeline.jison` (87 LoC).
//!
//! The jison grammar is lenient: each body line is either a `title`, a
//! `section`, a `period` (anything not starting with `:` or `#`), or an
//! `event` (starting with `:` and a space). Period starts a new task;
//! event appends to the last task.
//!
//! We also honour a local `%%{init:...}%%` / `---\nconfig:\n---` scan
//! so the byte-exact test harness, which feeds raw `.mmd` bytes to
//! [`parse`], still sees `timeline.disableMulticolor`, theme picks and
//! `themeVariables.cScale*` overrides. When the outer preprocessor
//! already stripped those, this scan is a no-op.

use crate::error::{MermaidError, Result};
use crate::model::timeline::{TimelineDiagram, TimelineDirection, TimelineTask};

pub fn parse(source: &str) -> Result<TimelineDiagram> {
    let mut d = TimelineDiagram {
        // Upstream schema default (`schemas/config.schema.yaml`
        // → `TimelineDiagramConfig.leftMargin: 150`). The renderer's
        // nullish-coalesce fallback `?? 50` is only hit when the
        // config object omits the field entirely, which normal
        // flows never do because schema defaults populate it.
        left_margin: 150.0,
        ..TimelineDiagram::default()
    };

    // Extract YAML frontmatter (---\n...\n---) and %%{init:...}%% blocks.
    let after_fm = strip_frontmatter(source, &mut d);
    let cleaned = extract_init_directives(&after_fm, &mut d);

    // Line-by-line body parse.
    let lines: Vec<&str> = cleaned.lines().collect();
    let mut i = 0;

    // Skip blank / `%%`-comment lines.
    while i < lines.len() && is_skip_line(lines[i]) {
        i += 1;
    }
    if i >= lines.len() {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: "empty timeline source".into(),
        });
    }

    // Header: `timeline` [ `LR` | `TD` ].
    let header = lines[i].trim();
    let rest = strip_kw(header, "timeline").ok_or_else(|| MermaidError::Parse {
        line: i + 1,
        col: 1,
        message: format!("expected 'timeline' header, got {header:?}"),
    })?;
    let rest = rest.trim();
    if rest.eq_ignore_ascii_case("LR") {
        d.direction = TimelineDirection::LR;
    } else if rest.eq_ignore_ascii_case("TD") {
        d.direction = TimelineDirection::TD;
    }
    i += 1;

    while i < lines.len() {
        let raw = lines[i];
        i += 1;
        if is_skip_line(raw) {
            continue;
        }
        let line = raw.trim_start();

        if let Some(rest) = strip_kw(line, "title") {
            // Upstream substr(6) drops 'title ' without re-trimming.
            // But it trims the trailing newline implicitly via line split.
            let value = rest.trim_start();
            // mimic `$1.substr(6)` — note upstream adds exactly one space
            // between `title` and the rest in the regex `"title"\s`, so
            // `substr(6)` consumes that single space. If additional
            // leading whitespace follows it becomes part of the title.
            // Our `strip_kw` already consumed the single whitespace.
            let value = value.trim_end();
            if !value.is_empty() {
                d.meta.title = Some(value.to_string());
            }
            continue;
        }
        if let Some(rest) = strip_kw(line, "section") {
            let name = rest.trim_end().to_string();
            d.sections.push(name.clone());
            // currentSection tracked implicitly via sections vec.
            continue;
        }

        // Event vs period distinction: jison rule
        //   event  := ':' \s (?:[^:\n]|':'(?!\s))+
        //   period := [^#:\n]+
        // An event line starts with ':' followed by a space. Subsequent
        // `": "` segments on the same line each produce their own
        // `event` token, same as on a period line.
        if let Some(ev) = strip_event(line) {
            if let Some(task) = d.tasks.last_mut() {
                // Split additional `": "` segments — e.g. cypress/11
                // continuation lines like
                //   `: Research and Development : Purchasing Activities`
                // must yield TWO events, not one.
                let (first, extras) = split_event_line(ev);
                if !first.is_empty() {
                    task.events.push(first.to_string());
                }
                task.events.extend(extras);
            }
            continue;
        }

        // Period: may itself contain trailing `: event` segments that
        // jison tokenises into separate event tokens on the same line.
        // We replicate by splitting on `: ` (colon-space) boundaries.
        let current_section = d.sections.last().cloned().unwrap_or_default();
        let (period, extras) = split_period_and_events(line);
        if period.is_empty() {
            continue;
        }
        d.tasks.push(TimelineTask {
            section: current_section,
            task: period.to_string(),
            events: extras,
        });
    }

    Ok(d)
}

/// Hand-scan for `---\nconfig:\n---` frontmatter. We only pick three
/// knobs — `theme`, `themeVariables`, `config.timeline.disableMulticolor`
/// — because the rest of Config is already folded by the outer
/// [`crate::preprocess`] pipeline when wiring through `convert_with_id`.
fn strip_frontmatter<'a>(source: &'a str, d: &mut TimelineDiagram) -> String {
    // Match `^-{3}\s*\n(.*?)\n-{3}\s*\n+` in the same shape as
    // `crate::config::frontmatter`, but we only need the body lines so a
    // hand scan is enough.
    let bytes = source.as_bytes();
    if !(bytes.len() >= 4 && &bytes[..3] == b"---" && (bytes[3] == b'\n' || bytes[3] == b'\r')) {
        return source.to_string();
    }
    // Find the closing `---`.
    let rest = &source[4..];
    let Some(end_rel) = find_closing_fence(rest) else {
        return source.to_string();
    };
    let body = &rest[..end_rel];
    // Skip past the closing fence + its newline run.
    let after_close = &rest[end_rel..];
    let after = after_close
        .strip_prefix("---")
        .map(|s| s.trim_start_matches(|c: char| c == '\n' || c == '\r' || c == ' ' || c == '\t'))
        .unwrap_or(after_close);

    apply_yaml_body(body, d);
    after.to_string()
}

fn find_closing_fence(s: &str) -> Option<usize> {
    // look for `\n---` at start of a line.
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        // Newline or start
        let at_line_start = i == 0 || bytes[i - 1] == b'\n';
        if at_line_start
            && bytes.len() >= i + 3
            && &bytes[i..i + 3] == b"---"
            && (bytes.len() == i + 3
                || bytes[i + 3] == b'\n'
                || bytes[i + 3] == b'\r'
                || bytes[i + 3] == b' '
                || bytes[i + 3] == b'\t')
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Tiny YAML scanner — we only look for the scalars the timeline
/// fixtures actually use. Anything else (indented block sequences, flow
/// mappings, nested config) is ignored.
fn apply_yaml_body(body: &str, d: &mut TimelineDiagram) {
    // Track indentation stacks to recognise `config:` > `themeVariables:`.
    let mut in_config = false;
    let mut in_theme_vars = false;
    for raw in body.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let stripped = line.trim();

        if indent == 0 {
            in_config = false;
            in_theme_vars = false;
            if stripped == "config:" {
                in_config = true;
                continue;
            }
            // Other root keys (title, displayMode) handled by outer preprocess.
            continue;
        }

        if in_config && indent <= 4 && stripped == "themeVariables:" {
            in_theme_vars = true;
            continue;
        }

        let Some(colon) = stripped.find(':') else {
            continue;
        };
        let key = stripped[..colon].trim();
        let val = strip_inline_quote(stripped[colon + 1..].trim());

        if in_theme_vars && indent >= 8 {
            if let Some(idx) = c_scale_index(key) {
                if idx < d.theme_overrides.c_scale.len() {
                    d.theme_overrides.c_scale[idx] = Some(val.to_string());
                }
            } else {
                // Upstream accepts `fontFamily` / `fontSize` inside
                // `themeVariables` as well — e.g. cypress/timeline/12.
                match key {
                    "fontFamily" => d.font_family = Some(val.to_string()),
                    "fontSize" => d.font_size = Some(val.to_string()),
                    _ => {}
                }
            }
            continue;
        }

        if in_config && indent == 4 {
            match key {
                "theme" => d.theme_name = Some(val.to_string()),
                "fontFamily" => d.font_family = Some(val.to_string()),
                "fontSize" => d.font_size = Some(val.to_string()),
                _ => {}
            }
        }
    }
}

fn strip_inline_quote(s: &str) -> &str {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')) {
        return inner;
    }
    if let Some(inner) = s.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        return inner;
    }
    s
}

fn c_scale_index(key: &str) -> Option<usize> {
    let rest = key.strip_prefix("cScale")?;
    rest.parse::<usize>().ok()
}

/// Remove `%%{init:...}%%` blocks (possibly nested braces are unusual
/// but simple depth counter handles them) and capture the handful of
/// keys we care about. Matches the behaviour of
/// [`crate::preprocess::preprocess`] well enough for fixtures that
/// reach this function directly.
fn extract_init_directives(source: &str, d: &mut TimelineDiagram) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 3 <= bytes.len() && &bytes[i..i + 3] == b"%%{" {
            // Scan to matching "}%%" respecting nested `{`.
            let mut depth = 1i32;
            let mut j = i + 3;
            while j + 3 <= bytes.len() {
                if &bytes[j..j + 3] == b"}%%" && depth == 1 {
                    let body = &source[i + 3..j];
                    apply_directive_body(body, d);
                    // Advance past `}%%` and optionally one trailing \n.
                    let mut new_i = j + 3;
                    if new_i < bytes.len() && bytes[new_i] == b'\n' {
                        new_i += 1;
                    }
                    i = new_i;
                    break;
                }
                if bytes[j] == b'{' {
                    depth += 1;
                } else if bytes[j] == b'}' {
                    depth -= 1;
                }
                j += 1;
            }
            if j + 3 > bytes.len() {
                // Unterminated directive — preserve remaining source.
                out.push_str(&source[i..]);
                return out;
            }
            continue;
        }
        let ch = source[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn apply_directive_body(body: &str, d: &mut TimelineDiagram) {
    // "theme": "dark"
    if let Some(v) = scan_str_after(body, "\"theme\"") {
        d.theme_name = Some(v);
    }
    if let Some(v) = scan_str_after(body, "'theme'") {
        d.theme_name = Some(v);
    }
    // timeline.disableMulticolor
    if let Some(v) = scan_bool_after(body, "\"disableMulticolor\"") {
        d.disable_multicolor = v;
    }
    if let Some(v) = scan_bool_after(body, "'disableMulticolor'") {
        d.disable_multicolor = v;
    }
    // themeVariables → cScaleN
    for idx in 0..12 {
        let k1 = format!("\"cScale{idx}\"");
        let k2 = format!("'cScale{idx}'");
        if let Some(v) = scan_str_after(body, &k1).or_else(|| scan_str_after(body, &k2)) {
            d.theme_overrides.c_scale[idx] = Some(v);
        }
    }
    if let Some(v) =
        scan_str_after(body, "\"fontFamily\"").or_else(|| scan_str_after(body, "'fontFamily'"))
    {
        d.font_family = Some(v);
    }
    if let Some(v) =
        scan_str_after(body, "\"fontSize\"").or_else(|| scan_str_after(body, "'fontSize'"))
    {
        d.font_size = Some(v);
    }
}

fn scan_str_after(s: &str, key: &str) -> Option<String> {
    let idx = s.find(key)?;
    let rest = &s[idx + key.len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let (open, close) = if let Some(r) = rest.strip_prefix('"') {
        (r, '"')
    } else if let Some(r) = rest.strip_prefix('\'') {
        (r, '\'')
    } else {
        return None;
    };
    let end = open.find(close)?;
    Some(open[..end].to_string())
}

fn scan_bool_after(s: &str, key: &str) -> Option<bool> {
    let idx = s.find(key)?;
    let rest = &s[idx + key.len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

/// Check if a line should be ignored (blank, whole-line `%%` comment).
fn is_skip_line(raw: &str) -> bool {
    let t = raw.trim();
    t.is_empty() || (t.starts_with("%%") && !t.starts_with("%%{"))
}

/// Detect the `event` jison token: `: ` followed by content.
fn strip_event(line: &str) -> Option<&str> {
    let rest = line.strip_prefix(':')?;
    // Event token requires a following whitespace char.
    let after_space = rest.strip_prefix(' ').or_else(|| rest.strip_prefix('\t'))?;
    // The jison regex forbids unescaped `\n`; our input was line-split.
    // Keep the content trimmed of trailing whitespace.
    Some(after_space.trim_end())
}

/// Split an event-continuation line's body (everything after `: `) on
/// further `": "` boundaries. Reuses [`split_period_and_events`]'s
/// scanner so cypress/timeline/11's wrap-around events parse into the
/// same 9-event shape upstream produces.
fn split_event_line(body: &str) -> (&str, Vec<String>) {
    split_period_and_events(body)
}

/// Split a period line like `2004 : Facebook : Google` into the leading
/// period and trailing events. The jison tokeniser treats subsequent
/// `: X` segments on the same line as separate `event` tokens that call
/// `addEvent(…)` on the most recently created task.
fn split_period_and_events(line: &str) -> (&str, Vec<String>) {
    // Find the first `": "` sequence. Everything before it is the
    // period; subsequent `": X"` segments each become an event.
    let mut extras = Vec::new();
    let mut period_end = line.len();
    let mut cursor = 0usize;
    let bytes = line.as_bytes();
    while cursor + 1 < bytes.len() {
        if bytes[cursor] == b':' && (bytes[cursor + 1] == b' ' || bytes[cursor + 1] == b'\t') {
            if period_end == line.len() {
                period_end = cursor;
            }
            // Scan forward to next `: ` or EOL.
            let seg_start = cursor + 2;
            let mut k = seg_start;
            while k + 1 < bytes.len() {
                if bytes[k] == b':' && (bytes[k + 1] == b' ' || bytes[k + 1] == b'\t') {
                    break;
                }
                k += 1;
            }
            let end = if k + 1 >= bytes.len() { line.len() } else { k };
            extras.push(line[seg_start..end].trim_end().to_string());
            cursor = end;
            continue;
        }
        cursor += 1;
    }
    let period = line[..period_end].trim_end();
    (period, extras)
}

fn strip_kw<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let s = s.trim_start();
    let rest = s.strip_prefix(kw)?;
    match rest.chars().next() {
        None => Some(rest),
        Some(c) if c.is_whitespace() => Some(&rest[c.len_utf8()..]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_timeline() {
        let src = "timeline\n  title Hi\n  2002 : LinkedIn\n  2004 : Facebook : Google\n";
        let d = parse(src).unwrap();
        assert_eq!(d.meta.title.as_deref(), Some("Hi"));
        assert_eq!(d.tasks.len(), 2);
        assert_eq!(d.tasks[0].task, "2002");
        assert_eq!(d.tasks[0].events, vec!["LinkedIn"]);
        assert_eq!(d.tasks[1].task, "2004");
        assert_eq!(d.tasks[1].events, vec!["Facebook", "Google"]);
    }

    #[test]
    fn parses_continuation_events() {
        let src = "timeline\n  2300 BC : People arrive\n          : More stuff\n";
        let d = parse(src).unwrap();
        assert_eq!(d.tasks.len(), 1);
        assert_eq!(d.tasks[0].events, vec!["People arrive", "More stuff"]);
    }

    #[test]
    fn parses_sections_and_direction() {
        let src =
            "timeline TD\n  title A\n  section One\n    1 : foo\n  section Two\n    2 : bar\n";
        let d = parse(src).unwrap();
        assert_eq!(d.direction, TimelineDirection::TD);
        assert_eq!(d.sections, vec!["One", "Two"]);
        assert_eq!(d.tasks[0].section, "One");
        assert_eq!(d.tasks[1].section, "Two");
    }

    #[test]
    fn init_directive_knobs() {
        let src = "%%{init: { 'theme':'base', 'timeline': {'disableMulticolor': true}}}%%\ntimeline\n  2002 : X\n";
        let d = parse(src).unwrap();
        assert_eq!(d.theme_name.as_deref(), Some("base"));
        assert!(d.disable_multicolor);
    }
}
