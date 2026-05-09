//! Gantt diagram parser — hand-rolled to match upstream
//! `diagrams/gantt/parser/gantt.jison` (188 LoC).
//!
//! The jison grammar is line-oriented. Each body line is one of:
//! - Directive: `dateFormat`, `axisFormat`, `tickInterval`, `includes`,
//!   `excludes`, `todayMarker`, `inclusiveEndDates`, `topAxis`,
//!   `weekday`, `weekend`, `title`, `accTitle`, `accDescr`
//! - Section: `section <name>`
//! - Task: `<taskTxt> : <taskData>`
//! - Click: `click <id> ...` (href / callback)
//!
//! We also honour a local `%%{init:...}%%` scan so the byte-exact test
//! harness, which feeds raw `.mmd` bytes to [`parse`], still sees
//! `gantt.*` config overrides. When the outer preprocessor already
//! stripped those, this scan is a no-op.

use crate::error::{MermaidError, Result};
use crate::model::gantt::{GanttDiagram, Section, Task};

/// Tags that can appear in task data (matched case-insensitively,
/// as a whole token in the comma-separated data).
const TASK_TAGS: &[&str] = &["active", "done", "crit", "milestone", "vert"];

pub fn parse(source: &str) -> Result<GanttDiagram> {
    let mut d = GanttDiagram {
        weekday: "sunday".to_string(),
        weekend: "saturday".to_string(),
        ..GanttDiagram::default()
    };

    // First strip an optional leading YAML frontmatter (`---\n...\n---`).
    let source = strip_frontmatter(source, &mut d);

    // Second pass: hoover up any remaining `%%{init:...}%%` directives.
    let source_after_directives = extract_init_directives(&source, &mut d);

    // Second pass: line-oriented parse of the body.
    let lines: Vec<&str> = source_after_directives.lines().collect();
    let mut i = 0;

    // Skip leading blank lines.
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }

    // Header: `gantt`
    if i >= lines.len() {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: "empty gantt source".into(),
        });
    }
    let header = lines[i].trim();
    if !header.eq_ignore_ascii_case("gantt") {
        return Err(MermaidError::Parse {
            line: i + 1,
            col: 1,
            message: format!("expected 'gantt' header, got {header:?}"),
        });
    }
    i += 1;

    // Current section index (0 means "no section" — tasks before any
    // section declaration go into an implicit empty section).
    let mut current_section_idx: usize = 0;
    // Whether the implicit empty section has been seeded.
    let mut has_implicit_section = false;

    // Auto-incrementing task ID counter (mirrors upstream `taskCnt`).
    let mut auto_id_counter: usize = 0;

    while i < lines.len() {
        let raw = lines[i];
        i += 1;
        if is_skip_line(raw) {
            continue;
        }
        let line = raw.trim();

        // --- Directives (case-insensitive matching) ---
        if let Some(rest) = strip_kw_ci(line, "dateFormat") {
            d.date_format = rest.trim().to_string();
            continue;
        }
        if let Some(rest) = strip_kw_ci(line, "axisFormat") {
            // Upstream jison does `$1.substr(11)`, preserving any
            // leading whitespace between "axisFormat" and the format
            // string. d3 timeFormat passes that through verbatim, so
            // ` %d/%m` and `%d/%m` produce different tick labels.
            d.axis_format = Some(rest.trim_end().to_string());
            continue;
        }
        if let Some(rest) = strip_kw_ci(line, "tickInterval") {
            d.tick_interval = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = strip_kw_ci(line, "excludes") {
            d.excludes = rest
                .trim()
                .to_lowercase()
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            continue;
        }
        if let Some(rest) = strip_kw_ci(line, "includes") {
            d.includes = rest
                .trim()
                .to_lowercase()
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            continue;
        }
        if let Some(rest) = strip_kw_ci(line, "todayMarker") {
            d.today_marker = Some(rest.trim().to_string());
            continue;
        }
        if line.eq_ignore_ascii_case("inclusiveEndDates") {
            d.inclusive_end_dates = true;
            continue;
        }
        if line.eq_ignore_ascii_case("topAxis") {
            d.top_axis = true;
            continue;
        }
        if let Some(rest) = strip_kw_ci(line, "weekday") {
            // e.g. `weekday monday`
            let day = rest.trim().to_lowercase();
            if !day.is_empty() {
                d.weekday = day;
            }
            continue;
        }
        if let Some(rest) = strip_kw_ci(line, "weekend") {
            let day = rest.trim().to_lowercase();
            if !day.is_empty() {
                d.weekend = day;
            }
            continue;
        }
        if let Some(rest) = strip_kw_ci(line, "title") {
            let value = rest.trim_end().to_string();
            if d.meta.title.is_none() && !value.is_empty() {
                d.meta.title = Some(value);
            }
            continue;
        }
        // accTitle / accDescr — match `accTitle:` or `accTitle :` (with
        // or without a space before the colon). Upstream's grammar
        // (`title.spec.ts`) treats the `:` as a token-separator.
        if let Some(rest) = strip_kw_punct(line, "accTitle") {
            d.meta.acc_title = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = strip_kw_punct(line, "accDescr") {
            d.meta.acc_descr = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = strip_kw_ci(line, "displayMode") {
            d.display_mode = Some(rest.trim().to_string());
            continue;
        }

        // --- Section ---
        if let Some(rest) = strip_kw_ci(line, "section") {
            let name = rest.trim_end().to_string();
            d.sections.push(Section {
                name,
                task_indices: Vec::new(),
            });
            current_section_idx = d.sections.len(); // 1-based; 0 = implicit
            continue;
        }

        // --- Click (interactivity) ---
        // We parse it enough to not break, but don't store interactive
        // callbacks (Rust side doesn't run JS). We only extract `click
        // <id> call <fn>` and `click <id> href <url>` for CSS class
        // purposes.
        if let Some(rest) = strip_kw_ci(line, "click") {
            parse_click(rest, &mut d);
            continue;
        }

        // --- Task ---
        // A task line contains a colon that separates the task text
        // from the task data: `Task name : a1, 2014-01-01, 30d`
        // The jison grammar matches `[^:\n]+` for taskTxt and
        // `:[^#\n;]+` for taskData.
        if let Some(colon_pos) = line.find(':') {
            // Upstream jison's `[^:\n]+` matches the raw task text
            // including trailing whitespace; the renderer outputs that
            // verbatim (and uses its bbox width). Trimming here would
            // shrink the task label and skew text-width-based branches.
            let task_txt = line[..colon_pos].trim_start();
            let task_data = line[colon_pos + 1..].trim();
            if !task_txt.trim().is_empty() && !task_data.is_empty() {
                // Ensure an implicit section exists if no section declared yet.
                if current_section_idx == 0 && !has_implicit_section {
                    has_implicit_section = true;
                }
                let section_idx = if current_section_idx == 0 {
                    0
                } else {
                    current_section_idx - 1
                };

                let task = parse_task(task_txt, task_data, section_idx, &mut auto_id_counter);

                // Record task index in the section.
                let task_idx = d.tasks.len();
                if current_section_idx > 0 {
                    // current_section_idx is 1-based
                    if let Some(sec) = d.sections.get_mut(current_section_idx - 1) {
                        sec.task_indices.push(task_idx);
                    }
                }

                d.tasks.push(task);
                continue;
            }
        }

        // Lines we don't recognise are silently ignored (matches upstream
        // jison behaviour where unmatched tokens just don't reduce).
    }

    Ok(d)
}

/// Parse a single task line's data portion.
///
/// Upstream `parseData` splits the data on commas, extracts tags
/// (`active`, `done`, `crit`, `milestone`), then interprets the
/// remaining 1-3 fields as:
///
/// - 1 field:  `[duration_or_end]` — start = prev task end
/// - 2 fields: `[start, duration_or_end]` — auto ID
/// - 3 fields: `[id, start, duration_or_end]`
fn parse_task(name: &str, data: &str, section_idx: usize, auto_id_counter: &mut usize) -> Task {
    // Split on commas, trim whitespace.
    let mut parts: Vec<&str> = data.split(',').map(|s| s.trim()).collect();

    // Extract tags — upstream `getTaskTags` iterates and removes leading
    // tags that match exactly (case-insensitive).
    let mut done = false;
    let mut active = false;
    let mut critical = false;
    let mut milestone = false;
    let mut vert = false;

    let mut found_tag = true;
    while found_tag {
        found_tag = false;
        if parts.is_empty() {
            break;
        }
        let first = parts[0].to_lowercase();
        for &tag in TASK_TAGS {
            if first == tag {
                match tag {
                    "active" => active = true,
                    "done" => done = true,
                    "crit" => critical = true,
                    "milestone" => milestone = true,
                    "vert" => vert = true,
                    _ => {}
                }
                parts.remove(0);
                found_tag = true;
                break;
            }
        }
    }

    // Interpret remaining fields.
    let (id, start, end) = match parts.len() {
        0 => {
            // No data at all — just a task name with empty data.
            *auto_id_counter += 1;
            (Some(format!("task{}", auto_id_counter)), None, None)
        }
        1 => {
            // [duration_or_end]
            *auto_id_counter += 1;
            (
                Some(format!("task{}", auto_id_counter)),
                None, // start = previous task's end (resolved later)
                Some(parts[0].to_string()),
            )
        }
        2 => {
            // [start, duration_or_end]
            *auto_id_counter += 1;
            (
                Some(format!("task{}", auto_id_counter)),
                Some(parts[0].to_string()),
                Some(parts[1].to_string()),
            )
        }
        3.. => {
            // [id, start, duration_or_end]
            let id_val = parts[0].to_string();
            // Treat empty id as auto-generated.
            let id = if id_val.is_empty() {
                *auto_id_counter += 1;
                Some(format!("task{}", auto_id_counter))
            } else {
                Some(id_val)
            };
            (id, Some(parts[1].to_string()), Some(parts[2].to_string()))
        }
    };

    Task {
        id,
        name: name.to_string(),
        start,
        end,
        done,
        active,
        critical,
        milestone,
        vert,
        section: section_idx,
        classes: Vec::new(),
    }
}

/// Parse `click` interactivity lines, mapping `clickable` CSS classes
/// onto the targeted task(s). We don't store the JS callback / URL
/// itself since the Rust side never executes JavaScript.
fn parse_click(rest: &str, d: &mut GanttDiagram) {
    // Format: `click <ids> call <fn>(<args>)` or `click <ids> href "<url>"`.
    // We just need to know which ids are clickable.
    let rest = rest.trim();
    if rest.is_empty() {
        return;
    }
    // First whitespace-separated token after `click` is the id list.
    let mut iter = rest.splitn(2, char::is_whitespace);
    let id_blob = iter.next().unwrap_or("");
    let _verb_and_rest = iter.next().unwrap_or("");
    for id in id_blob.split(',') {
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        for t in d.tasks.iter_mut() {
            if t.id.as_deref() == Some(id) && !t.classes.iter().any(|c| c == "clickable") {
                t.classes.push("clickable".to_string());
            }
        }
    }
}

/// Strip leading YAML frontmatter `---\n...\n---\n`, lifting the
/// `displayMode` / `title` keys we care about. This is a coarse
/// best-effort scan, tolerant of trailing CR/LF.
fn strip_frontmatter(source: &str, d: &mut GanttDiagram) -> String {
    let trimmed = source.trim_start_matches('\u{FEFF}');
    if !trimmed.starts_with("---") {
        return source.to_string();
    }
    let after_open = match trimmed.find('\n') {
        Some(idx) if &trimmed[..idx].trim() == &"---" => &trimmed[idx + 1..],
        _ => return source.to_string(),
    };
    // Find closing `---` line.
    let mut end_idx = None;
    let mut pos = 0usize;
    for line in after_open.split('\n') {
        if line.trim() == "---" {
            end_idx = Some(pos + line.len());
            break;
        }
        pos += line.len() + 1;
    }
    let close_pos = match end_idx {
        Some(p) => p,
        None => return source.to_string(),
    };
    let body = &after_open[..close_pos.saturating_sub(3).min(after_open.len())];
    // Trivial key:value scan for displayMode, title.
    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("displayMode:") {
            let v = rest.trim().trim_matches(|c: char| c == '"' || c == '\'').to_string();
            if !v.is_empty() {
                d.display_mode = Some(v);
            }
        } else if let Some(rest) = line.strip_prefix("title:") {
            let v = rest.trim().trim_matches(|c: char| c == '"' || c == '\'').to_string();
            if !v.is_empty() && d.meta.title.is_none() {
                d.meta.title = Some(v);
            }
        }
    }
    let after_close = &after_open[close_pos..];
    // Skip trailing newline after the closing fence.
    let after_close = after_close.strip_prefix('\n').unwrap_or(after_close);
    after_close.to_string()
}

/// Remove `%%{init:...}%%` blocks and capture the handful of gantt
/// config keys we care about. Matches the behaviour of
/// [`crate::preprocess::preprocess`] well enough for fixtures that
/// reach this function directly.
fn extract_init_directives(source: &str, d: &mut GanttDiagram) -> String {
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

fn apply_directive_body(body: &str, d: &mut GanttDiagram) {
    // "theme": "dark"
    if let Some(v) = scan_str_after(body, "\"theme\"") {
        d.theme_name = Some(v);
    }
    if let Some(v) = scan_str_after(body, "'theme'") {
        d.theme_name = Some(v);
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

/// Check if a line should be ignored (blank, whole-line `%%` comment).
fn is_skip_line(raw: &str) -> bool {
    let t = raw.trim();
    t.is_empty() || (t.starts_with("%%") && !t.starts_with("%%{"))
}

/// Variant of [`strip_kw_ci`] that accepts a literal `:` (with optional
/// surrounding whitespace) immediately after the keyword.
fn strip_kw_punct<'a>(line: &'a str, kw: &str) -> Option<&'a str> {
    let trimmed = line.trim_start();
    if !trimmed.is_char_boundary(kw.len()) {
        return None;
    }
    if trimmed.len() < kw.len() {
        return None;
    }
    if !trimmed[..kw.len()].eq_ignore_ascii_case(kw) {
        return None;
    }
    let after = &trimmed[kw.len()..];
    let after = after.trim_start();
    after.strip_prefix(':').map(|r| r.trim_start())
}

/// Case-insensitive keyword stripping. Returns `Some(rest)` if `line`
/// starts with `kw` followed by whitespace or end-of-string.
fn strip_kw_ci<'a>(line: &'a str, kw: &str) -> Option<&'a str> {
    let line = line.trim_start();
    if line.len() < kw.len() {
        return None;
    }
    // Guard against multi-byte boundaries: line[..kw.len()] panics if
    // the kw.len()-th byte is mid-codepoint (e.g. CJK input).
    if !line.is_char_boundary(kw.len()) {
        return None;
    }
    if line[..kw.len()].eq_ignore_ascii_case(kw) {
        let rest = &line[kw.len()..];
        match rest.chars().next() {
            None => Some(rest),
            Some(c) if c.is_whitespace() => Some(&rest[c.len_utf8()..]),
            _ => None,
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_gantt() {
        let src = "\
gantt
    title A Gantt Chart
    dateFormat YYYY-MM-DD
    section Section
    A task          :a1, 2014-01-01, 30d
    Another task    :after a1, 20d
";
        let d = parse(src).unwrap();
        assert_eq!(d.meta.title.as_deref(), Some("A Gantt Chart"));
        assert_eq!(d.date_format, "YYYY-MM-DD");
        assert_eq!(d.sections.len(), 1);
        assert_eq!(d.sections[0].name, "Section");
        assert_eq!(d.tasks.len(), 2);
        assert_eq!(d.tasks[0].id.as_deref(), Some("a1"));
        // Trailing whitespace is preserved verbatim — upstream's
        // `[^:\n]+` jison rule keeps it, and the renderer relies on
        // the bbox of the literal text for its outside-label branch.
        assert_eq!(d.tasks[0].name.trim_end(), "A task");
        assert_eq!(d.tasks[0].start.as_deref(), Some("2014-01-01"));
        assert_eq!(d.tasks[0].end.as_deref(), Some("30d"));
        assert_eq!(d.tasks[1].start.as_deref(), Some("after a1"));
        assert_eq!(d.tasks[1].end.as_deref(), Some("20d"));
    }

    #[test]
    fn parses_task_tags() {
        let src = "\
gantt
    dateFormat YYYY-MM-DD
    Done task    :done, a1, 2014-01-01, 30d
    Active task  :active, a2, 2014-01-01, 30d
    Critical     :crit, a3, 2014-01-01, 30d
    Milestone    :milestone, a4, 2014-01-01, 0d
";
        let d = parse(src).unwrap();
        assert!(d.tasks[0].done);
        assert!(!d.tasks[0].active);
        assert!(d.tasks[1].active);
        assert!(d.tasks[2].critical);
        assert!(d.tasks[3].milestone);
    }

    #[test]
    fn parses_directives() {
        let src = "\
gantt
    dateFormat YYYY-MM-DD
    axisFormat %m/%d
    excludes weekends
    includes 2024-12-25
    todayMarker off
    inclusiveEndDates
    topAxis
    weekday monday
    weekend friday
";
        let d = parse(src).unwrap();
        assert_eq!(d.axis_format.as_deref(), Some("%m/%d"));
        assert_eq!(d.excludes, vec!["weekends"]);
        assert_eq!(d.includes, vec!["2024-12-25"]);
        assert_eq!(d.today_marker.as_deref(), Some("off"));
        assert!(d.inclusive_end_dates);
        assert!(d.top_axis);
        assert_eq!(d.weekday, "monday");
        assert_eq!(d.weekend, "friday");
    }

    #[test]
    fn parses_multiple_sections() {
        let src = "\
gantt
    dateFormat YYYY-MM-DD
    section First
    Task A : a1, 2014-01-01, 10d
    section Second
    Task B : a2, 2014-01-15, 10d
";
        let d = parse(src).unwrap();
        assert_eq!(d.sections.len(), 2);
        assert_eq!(d.sections[0].name, "First");
        assert_eq!(d.sections[1].name, "Second");
        assert_eq!(d.sections[0].task_indices, vec![0]);
        assert_eq!(d.sections[1].task_indices, vec![1]);
        assert_eq!(d.tasks[0].section, 0);
        assert_eq!(d.tasks[1].section, 1);
    }

    #[test]
    fn auto_id_when_missing() {
        let src = "\
gantt
    dateFormat YYYY-MM-DD
    A task : 2014-01-01, 30d
    Another : after a1, 20d
";
        let d = parse(src).unwrap();
        // Without explicit IDs, auto-generated.
        assert_eq!(d.tasks[0].id.as_deref(), Some("task1"));
        assert_eq!(d.tasks[1].id.as_deref(), Some("task2"));
    }

    #[test]
    fn rejects_empty_source() {
        let result = parse("");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_wrong_header() {
        let result = parse("pie\n  title X\n");
        assert!(result.is_err());
    }
}
