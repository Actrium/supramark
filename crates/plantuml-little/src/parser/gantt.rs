use log::{debug, trace, warn};
use regex::Regex;

use crate::model::gantt::{GanttColoredRange, GanttDependency, GanttDiagram, GanttNote, GanttTask};
use crate::Result;

/// Parse Gantt diagram source text into a `GanttDiagram` IR.
pub fn parse_gantt_diagram(source: &str) -> Result<GanttDiagram> {
    let block = super::common::extract_block(source).unwrap_or_else(|| source.to_string());

    let mut tasks: Vec<GanttTask> = Vec::new();
    let mut dependencies: Vec<GanttDependency> = Vec::new();
    let mut project_start: Option<String> = None;
    let mut closed_days: Vec<String> = Vec::new();
    let mut colored_ranges: Vec<GanttColoredRange> = Vec::new();
    let mut scale: Option<u32> = None;
    let mut print_scale: Option<String> = None;
    let mut notes: Vec<GanttNote> = Vec::new();
    let mut in_note_block = false;
    let mut note_block_position = String::new();
    let mut note_block_target: Option<String> = None;
    let mut note_block_lines: Vec<String> = Vec::new();

    // Regex patterns
    // [Task Name] lasts N days
    let re_task_lasts = Regex::new(r"^\[([^\]]+)\]\s+lasts\s+(\d+)\s+days?$").expect("regex");
    // [Task Name] as [ALIAS] lasts N days
    let re_task_alias_lasts =
        Regex::new(r"^\[([^\]]+)\]\s+as\s+\[([^\]]+)\]\s+lasts\s+(\d+)\s+days?$").expect("regex");
    // Project starts the YYYY-MM-DD or YYYY/MM/DD
    let re_project_start =
        Regex::new(r"^[Pp]roject\s+starts\s+the\s+(\d{4}[-/]\d{2}[-/]\d{2})$").expect("regex");
    // [A]->[B] dependency
    let re_dependency = Regex::new(r"^\[([^\]]+)\]\s*->\s*\[([^\]]+)\]$").expect("regex");
    // Leading task reference for `starts` clauses
    let re_task_name = Regex::new(r"^\[([^\]]+)\]").expect("regex");
    // `after [Y]'s end` / `after [Y] end` — the accepted starts-argument form
    let re_starts_after =
        Regex::new(r"^after\s+\[([^\]]+)\]'?s?\s+end$").expect("regex");
    // [Task] is colored in Color or Color/Color
    let re_colored = Regex::new(r"^\[([^\]]+)\]\s+is\s+colored\s+in\s+(.+)$").expect("regex");
    // scale N
    let re_scale = Regex::new(r"^scale\s+(\d+)$").expect("regex");
    // printscale weekly|daily
    let re_printscale = Regex::new(r"^printscale\s+(\w+)$").expect("regex");
    // sunday|saturday|... are closed
    let re_closed = Regex::new(r"^(\w+)\s+are\s+closed$").expect("regex");
    // YYYY/MM/DD to YYYY/MM/DD are colored in <color>
    let re_range_colored =
        Regex::new(r"^(\d{4}/\d{2}/\d{2})\s+to\s+(\d{4}/\d{2}/\d{2})\s+are\s+colored\s+in\s+(.+)$")
            .expect("regex");

    for (line_num, line) in block.lines().enumerate() {
        let trimmed = line.trim();
        let line_num = line_num + 1;

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('\'') {
            trace!("line {line_num}: skip empty/comment");
            continue;
        }

        // Handle multi-line note accumulation
        if in_note_block {
            if trimmed == "end note" || trimmed == "endnote" {
                let text = note_block_lines.join("\n");
                debug!("line {line_num}: end note block, text={text:?}");
                notes.push(GanttNote {
                    text,
                    position: note_block_position.clone(),
                    target: note_block_target.take(),
                });
                note_block_lines.clear();
                in_note_block = false;
            } else {
                note_block_lines.push(trimmed.to_string());
            }
            continue;
        }

        // Task with alias: [Name] as [ALIAS] lasts N days
        if let Some(caps) = re_task_alias_lasts.captures(trimmed) {
            let name = caps[1].to_string();
            let alias = caps[2].to_string();
            let days: u32 = caps[3].parse().unwrap_or(1);
            debug!("line {line_num}: task '{name}' as [{alias}] lasts {days} days");
            tasks.push(GanttTask {
                name,
                alias: Some(alias),
                duration_days: days,
                color: None,
                start_date: None,
            });
            continue;
        }

        // Task: [Name] lasts N days
        if let Some(caps) = re_task_lasts.captures(trimmed) {
            let name = caps[1].to_string();
            let days: u32 = caps[2].parse().unwrap_or(1);
            debug!("line {line_num}: task '{name}' lasts {days} days");
            tasks.push(GanttTask {
                name,
                alias: None,
                duration_days: days,
                color: None,
                start_date: None,
            });
            continue;
        }

        // Project starts the YYYY-MM-DD
        if let Some(caps) = re_project_start.captures(trimmed) {
            let date = caps[1].to_string();
            debug!("line {line_num}: project starts {date}");
            project_start = Some(date);
            continue;
        }

        // Dependency: [A]->[B]
        if let Some(caps) = re_dependency.captures(trimmed) {
            let from = caps[1].to_string();
            let to = caps[2].to_string();
            debug!("line {line_num}: dependency {from} -> {to}");
            dependencies.push(GanttDependency { from, to });
            continue;
        }

        // [Task] starts after [Other]'s end — the one `starts` form jar v1.2026.3
        // accepts (probe after.svg); rendered as a dependency edge. Other `starts`
        // forms are rejected upstream by the official sentence set (probes s2/s3/abs
        // -> error page), so reject here too instead of silently ignoring.
        if trimmed.contains(" starts ") {
            if let Some(caps) = re_task_name.captures(trimmed) {
                let task_ref = caps[1].to_string();
                let rest = &trimmed[caps[0].len()..].trim();
                if let Some(clause) = rest.strip_prefix("starts") {
                    let clause = clause.trim();
                    if let Some(dc) = re_starts_after.captures(clause) {
                        let from = dc[1].to_string();
                        debug!("line {line_num}: {task_ref} starts after {from}");
                        dependencies.push(GanttDependency { from, to: task_ref });
                        continue;
                    }
                    // jar renders its error page for rejected starts clauses
                    // (probes s2/s3/abs); route through the same page path.
                    return Err(crate::Error::JavaErrorPage {
                        line: line_num,
                        message: format!("cannot parse {clause} in starts clause"),
                    });
                }
            }
        }

        // Colored task: [Task] is colored in Color/Color
        if let Some(caps) = re_colored.captures(trimmed) {
            let task_ref = caps[1].to_string();
            let color_spec = caps[2].trim().to_string();
            debug!("line {line_num}: task '{task_ref}' colored '{color_spec}'");
            // Find the task by name or alias and set its color
            let found = tasks
                .iter_mut()
                .find(|t| t.name == task_ref || t.alias.as_deref() == Some(&task_ref));
            if let Some(task) = found {
                task.color = Some(color_spec);
            } else {
                warn!("line {line_num}: color target '{task_ref}' not found among tasks");
            }
            continue;
        }

        // Scale
        if let Some(caps) = re_scale.captures(trimmed) {
            let s: u32 = caps[1].parse().unwrap_or(1);
            debug!("line {line_num}: scale {s}");
            scale = Some(s);
            continue;
        }

        // Printscale
        if let Some(caps) = re_printscale.captures(trimmed) {
            let ps = caps[1].to_string();
            debug!("line {line_num}: printscale {ps}");
            print_scale = Some(ps);
            continue;
        }

        // Closed days
        if let Some(caps) = re_closed.captures(trimmed) {
            let day = caps[1].to_lowercase();
            debug!("line {line_num}: closed day '{day}'");
            closed_days.push(day);
            continue;
        }

        // Colored range: YYYY/MM/DD to YYYY/MM/DD are colored in <color>
        if let Some(caps) = re_range_colored.captures(trimmed) {
            let start = caps[1].to_string();
            let end = caps[2].to_string();
            let color = caps[3].trim().to_string();
            debug!("line {line_num}: colored range {start} to {end} in {color}");
            colored_ranges.push(GanttColoredRange { start, end, color });
            continue;
        }

        // Note parsing: `note for [Task] : text` or `note : text`
        if let Some(note_result) = try_parse_gantt_note(trimmed) {
            match note_result {
                GanttNoteParseResult::SingleLine(note) => {
                    debug!("line {}: single-line note for {:?}", line_num, note.target);
                    notes.push(note);
                }
                GanttNoteParseResult::MultiLineStart { position, target } => {
                    debug!("line {line_num}: start multi-line note for {target:?}");
                    in_note_block = true;
                    note_block_position = position;
                    note_block_target = target;
                    note_block_lines.clear();
                }
            }
            continue;
        }

        // Unknown line - log and skip
        trace!("line {line_num}: ignored '{trimmed}'");
    }

    debug!(
        "parse_gantt_diagram: {} tasks, {} deps, {} closed days",
        tasks.len(),
        dependencies.len(),
        closed_days.len()
    );

    Ok(GanttDiagram {
        tasks,
        dependencies,
        project_start,
        closed_days,
        colored_ranges,
        scale,
        print_scale,
        notes,
    })
}

// ---------------------------------------------------------------------------
// Note parsing
// ---------------------------------------------------------------------------

enum GanttNoteParseResult {
    SingleLine(GanttNote),
    MultiLineStart {
        position: String,
        target: Option<String>,
    },
}

/// Parse a Gantt note line.
///
/// Supported forms:
///   `note for [Task] : text`    (single-line, attached to task)
///   `note for [Task]`           (multi-line start)
///   `note : text`               (floating single-line)
///   `note bottom`               (floating multi-line)
///   `note left of [Task] : text`
fn try_parse_gantt_note(line: &str) -> Option<GanttNoteParseResult> {
    let trimmed = line.trim();
    if !trimmed.starts_with("note ") && trimmed != "note" {
        return None;
    }

    if trimmed == "note" {
        return Some(GanttNoteParseResult::MultiLineStart {
            position: "bottom".to_string(),
            target: None,
        });
    }

    let rest = trimmed[5..].trim();

    // `note for [Task] : text` or `note for [Task]`
    if let Some(after_for) = rest.strip_prefix("for ") {
        let after_for = after_for.trim();
        // Expect [TaskName]
        if after_for.starts_with('[') {
            if let Some(close_bracket) = after_for.find(']') {
                let target = after_for[1..close_bracket].trim().to_string();
                let after_bracket = after_for[close_bracket + 1..].trim();

                if let Some(colon_rest) = after_bracket.strip_prefix(':') {
                    let text = colon_rest
                        .trim()
                        .replace("\\n", "\n")
                        .replace(crate::NEWLINE_CHAR, "\n");
                    return Some(GanttNoteParseResult::SingleLine(GanttNote {
                        text,
                        position: "bottom".to_string(),
                        target: Some(target),
                    }));
                }

                return Some(GanttNoteParseResult::MultiLineStart {
                    position: "bottom".to_string(),
                    target: Some(target),
                });
            }
        }
    }

    // `note left/right/top/bottom of [Task] : text`
    for pos in &["left", "right", "top", "bottom"] {
        if !rest.starts_with(pos) {
            continue;
        }
        let after_pos = rest[pos.len()..].trim();
        if let Some(after_of) = after_pos.strip_prefix("of ") {
            let after_of = after_of.trim();
            if after_of.starts_with('[') {
                if let Some(close_bracket) = after_of.find(']') {
                    let target = after_of[1..close_bracket].trim().to_string();
                    let after_bracket = after_of[close_bracket + 1..].trim();

                    if let Some(colon_rest) = after_bracket.strip_prefix(':') {
                        let text = colon_rest
                            .trim()
                            .replace("\\n", "\n")
                            .replace(crate::NEWLINE_CHAR, "\n");
                        return Some(GanttNoteParseResult::SingleLine(GanttNote {
                            text,
                            position: pos.to_string(),
                            target: Some(target),
                        }));
                    }

                    return Some(GanttNoteParseResult::MultiLineStart {
                        position: pos.to_string(),
                        target: Some(target),
                    });
                }
            }
        }
    }

    // `note : text` (floating)
    if let Some(after_colon) = rest.strip_prefix(':') {
        let text = after_colon
            .trim()
            .replace("\\n", "\n")
            .replace(crate::NEWLINE_CHAR, "\n");
        return Some(GanttNoteParseResult::SingleLine(GanttNote {
            text,
            position: "bottom".to_string(),
            target: None,
        }));
    }

    // `note bottom` or `note left` etc. (floating multi-line)
    for pos in &["left", "right", "top", "bottom"] {
        if rest == *pos {
            return Some(GanttNoteParseResult::MultiLineStart {
                position: pos.to_string(),
                target: None,
            });
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> GanttDiagram {
        parse_gantt_diagram(source).expect("parse failed")
    }

    // Task-4 (v1.2026.3 alignment): jar rejects `[X] starts <bare>` forms
    // (probes: temp/jar-probe/s2|s3|abs — error page), while `starts after
    // [Y]'s end` is accepted and rendered as a dependency (probe: after.svg).
    #[test]
    fn starts_bare_forms_are_rejected_like_jar() {
        for bad in ["[TaskB] starts at D2", "[TaskB] starts D2", "[TaskB] starts 2026-01-02"] {
            let src = format!("@startgantt\n[TaskA] lasts 2 days\n{}\n@endgantt", bad);
            assert!(parse_gantt_diagram(&src).is_err(), "should reject: {bad}");
        }
    }

    #[test]
    fn starts_after_maps_to_dependency_edge() {
        let gd = parse(
            "@startgantt\n[TaskA] lasts 2 days\n[TaskB] lasts 1 days\n[TaskB] starts after [TaskA]'s end\n@endgantt",
        );
        assert!(gd.dependencies.iter().any(|d| d.from == "TaskA" && d.to == "TaskB"));
    }

    // 1. Basic task parsing
    #[test]
    fn test_basic_task() {
        let gd = parse("@startgantt\n[Design] lasts 10 days\n@endgantt");
        assert_eq!(gd.tasks.len(), 1);
        assert_eq!(gd.tasks[0].name, "Design");
        assert_eq!(gd.tasks[0].duration_days, 10);
        assert!(gd.tasks[0].alias.is_none());
    }

    // 2. Task with alias
    #[test]
    fn test_task_with_alias() {
        let gd = parse("@startgantt\n[Prototype design] as [TASK1] lasts 25 days\n@endgantt");
        assert_eq!(gd.tasks.len(), 1);
        assert_eq!(gd.tasks[0].name, "Prototype design");
        assert_eq!(gd.tasks[0].alias.as_deref(), Some("TASK1"));
        assert_eq!(gd.tasks[0].duration_days, 25);
    }

    // 3. Project start date
    #[test]
    fn test_project_start() {
        let gd = parse("@startgantt\nProject starts the 2020-10-15\n[T] lasts 5 days\n@endgantt");
        assert_eq!(gd.project_start.as_deref(), Some("2020-10-15"));
    }

    // 4. Project start date with slash format
    #[test]
    fn test_project_start_slash() {
        let gd = parse("@startgantt\nProject starts the 2020/10/15\n[T] lasts 5 days\n@endgantt");
        assert_eq!(gd.project_start.as_deref(), Some("2020/10/15"));
    }

    // 5. Dependencies
    #[test]
    fn test_dependency() {
        let gd = parse("@startgantt\n[A] lasts 5 days\n[B] lasts 3 days\n[A]->[B]\n@endgantt");
        assert_eq!(gd.dependencies.len(), 1);
        assert_eq!(gd.dependencies[0].from, "A");
        assert_eq!(gd.dependencies[0].to, "B");
    }

    // 6. Closed days
    #[test]
    fn test_closed_days() {
        let gd = parse(
            "@startgantt\nsunday are closed\nsaturday are closed\n[T] lasts 5 days\n@endgantt",
        );
        assert_eq!(gd.closed_days.len(), 2);
        assert!(gd.closed_days.contains(&"sunday".to_string()));
        assert!(gd.closed_days.contains(&"saturday".to_string()));
    }

    // 7. Scale
    #[test]
    fn test_scale() {
        let gd = parse("@startgantt\nscale 2\n[T] lasts 5 days\n@endgantt");
        assert_eq!(gd.scale, Some(2));
    }

    // 8. Printscale
    #[test]
    fn test_printscale() {
        let gd = parse("@startgantt\nprintscale weekly\n[T] lasts 5 days\n@endgantt");
        assert_eq!(gd.print_scale.as_deref(), Some("weekly"));
    }

    // 9. Colored task
    #[test]
    fn test_colored_task() {
        let gd = parse(
            "@startgantt\n[Prototype design] as [TASK1] lasts 25 days\n[TASK1] is colored in Lavender/LightBlue\n@endgantt",
        );
        assert_eq!(gd.tasks.len(), 1);
        assert_eq!(gd.tasks[0].color.as_deref(), Some("Lavender/LightBlue"));
    }

    // 10. Colored range
    #[test]
    fn test_colored_range() {
        let gd = parse(
            "@startgantt\n2020/10/26 to 2020/11/01 are colored in salmon\n[T] lasts 5 days\n@endgantt",
        );
        assert_eq!(gd.colored_ranges.len(), 1);
        assert_eq!(gd.colored_ranges[0].start, "2020/10/26");
        assert_eq!(gd.colored_ranges[0].end, "2020/11/01");
        assert_eq!(gd.colored_ranges[0].color, "salmon");
    }

    // 11. Full fixture (a0003.puml)
    #[test]
    fn test_full_fixture() {
        let source = r#"@startgantt
scale 2
printscale weekly
2020/10/26 to 2020/11/01 are colored in salmon
sunday are closed
saturday are closed

Project starts the 2020-10-15
[Prototype design] as [TASK1] lasts 25 days
[TASK1] is colored in Lavender/LightBlue
[Testing] lasts 5 days
[TASK1]->[Testing]
@endgantt"#;
        let gd = parse(source);
        assert_eq!(gd.scale, Some(2));
        assert_eq!(gd.print_scale.as_deref(), Some("weekly"));
        assert_eq!(gd.colored_ranges.len(), 1);
        assert_eq!(gd.closed_days.len(), 2);
        assert_eq!(gd.project_start.as_deref(), Some("2020-10-15"));
        assert_eq!(gd.tasks.len(), 2);
        assert_eq!(gd.tasks[0].name, "Prototype design");
        assert_eq!(gd.tasks[0].alias.as_deref(), Some("TASK1"));
        assert_eq!(gd.tasks[0].duration_days, 25);
        assert_eq!(gd.tasks[0].color.as_deref(), Some("Lavender/LightBlue"));
        assert_eq!(gd.tasks[1].name, "Testing");
        assert_eq!(gd.tasks[1].duration_days, 5);
        assert_eq!(gd.dependencies.len(), 1);
        assert_eq!(gd.dependencies[0].from, "TASK1");
        assert_eq!(gd.dependencies[0].to, "Testing");
    }

    // 12. Empty diagram
    #[test]
    fn test_empty_diagram() {
        let gd = parse("@startgantt\n@endgantt");
        assert!(gd.tasks.is_empty());
        assert!(gd.dependencies.is_empty());
        assert!(gd.project_start.is_none());
        assert!(gd.closed_days.is_empty());
    }

    // 13. Multiple tasks without dependencies
    #[test]
    fn test_multiple_tasks() {
        let gd =
            parse("@startgantt\n[A] lasts 3 days\n[B] lasts 7 days\n[C] lasts 2 days\n@endgantt");
        assert_eq!(gd.tasks.len(), 3);
        assert_eq!(gd.tasks[0].duration_days, 3);
        assert_eq!(gd.tasks[1].duration_days, 7);
        assert_eq!(gd.tasks[2].duration_days, 2);
    }

    // 14. Dependency with alias reference
    #[test]
    fn test_dependency_with_alias() {
        let gd = parse(
            "@startgantt\n[Design] as [D] lasts 10 days\n[Code] lasts 5 days\n[D]->[Code]\n@endgantt",
        );
        assert_eq!(gd.dependencies.len(), 1);
        assert_eq!(gd.dependencies[0].from, "D");
        assert_eq!(gd.dependencies[0].to, "Code");
    }

    // 15. Comments are ignored
    #[test]
    fn test_comments_ignored() {
        let gd = parse("@startgantt\n' This is a comment\n[T] lasts 5 days\n@endgantt");
        assert_eq!(gd.tasks.len(), 1);
    }

    // 16. Singular "day" form
    #[test]
    fn test_singular_day() {
        let gd = parse("@startgantt\n[Quick] lasts 1 day\n@endgantt");
        assert_eq!(gd.tasks.len(), 1);
        assert_eq!(gd.tasks[0].duration_days, 1);
    }

    // 17. Without @startgantt (raw content)
    #[test]
    fn test_raw_content() {
        let gd = parse("[Task A] lasts 10 days\n[Task B] lasts 5 days");
        assert_eq!(gd.tasks.len(), 2);
    }

    // 18. Note for task
    #[test]
    fn test_note_for_task() {
        let gd = parse(
            "@startgantt\n[Design] lasts 5 days\nnote for [Design] : important task\n@endgantt",
        );
        assert_eq!(gd.notes.len(), 1);
        assert_eq!(gd.notes[0].target.as_deref(), Some("Design"));
        assert_eq!(gd.notes[0].text, "important task");
    }
}
