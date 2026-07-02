use log::{debug, trace, warn};

use crate::model::activity::{
    ActivityDiagram, ActivityEvent, NotePosition, OldActivityGraph, OldActivityLink,
    OldActivityNode, OldActivityNodeKind,
};
use crate::model::Direction;
use crate::Result;

use std::collections::HashMap;

/// Parser state for multi-line constructs
#[derive(Debug)]
enum ParseState {
    /// Normal line-by-line parsing
    Normal,
    /// Inside a `:...; ` multi-line action, accumulating text
    Action {
        text: String,
        leading_blank_line: bool,
        start_line: usize,
        start_column: usize,
    },
    /// Inside a `note left/right` multi-line block
    Note {
        position: NotePosition,
        lines: Vec<String>,
        start_line: usize,
        start_column: usize,
    },
    /// Inside a `<style>...</style>` block — accumulate lines to extract properties
    StyleBlock {
        start_line: usize,
        start_column: usize,
        lines: Vec<String>,
    },
    /// Inside a `skinparam { ... }` block (skip all content)
    SkinparamBlock {
        start_line: usize,
        start_column: usize,
    },
    /// Inside a `legend ... end legend` block (skip)
    LegendBlock {
        start_line: usize,
        start_column: usize,
    },
    /// Inside a `header ... end header` block (skip)
    HeaderBlock {
        start_line: usize,
        start_column: usize,
    },
}

/// Parse activity diagram source text into an ActivityDiagram IR
pub fn parse_activity_diagram(source: &str) -> Result<ActivityDiagram> {
    let block = super::common::extract_block(source).unwrap_or_else(|| source.to_string());

    let mut events: Vec<ActivityEvent> = Vec::new();
    let mut swimlanes: Vec<String> = Vec::new();
    let mut direction = Direction::default();
    let mut note_max_width: Option<f64> = None;
    let mut state = ParseState::Normal;
    // Track old-style node names to avoid duplicating sync bars / actions
    let mut old_seen_nodes = std::collections::HashSet::<String>::new();
    let mut is_old_style = false;
    let mut old_graph = OldGraphBuilder::new();
    // Most recent `RepeatWhile` event, remembered as `(event_idx, line_num)` so
    // a following `-> label;` line can be attached as that loop's exit (`not`)
    // label.  PlantUML's repeat syntax allows the exit label on its own line:
    //   repeat while (cond) is (yes)
    //   -> no;
    // The exit-label line must come IMMEDIATELY after the `repeat while` line
    // (adjacency checked via `line_num`); this prevents a much-later, unrelated
    // `-> label;` from being misattached to a loop that had no exit label
    // (review #44).
    let mut last_repeat_while: Option<(usize, usize)> = None;

    for (line_num, line) in block.lines().enumerate() {
        let line_num = line_num + 1; // 1-based for diagnostics

        match state {
            ParseState::StyleBlock { ref mut lines, .. } => {
                if line.trim().to_lowercase().starts_with("</style>") {
                    debug!("line {line_num}: leaving <style> block");
                    // Extract note MaximumWidth from accumulated style lines
                    if let Some(w) = extract_note_max_width(lines) {
                        debug!("line {line_num}: extracted note MaximumWidth = {w}");
                        note_max_width = Some(w);
                    }
                    state = ParseState::Normal;
                } else {
                    lines.push(line.to_string());
                    trace!("line {line_num}: accumulating style content");
                }
                continue;
            }
            ParseState::SkinparamBlock { .. } => {
                if line.trim().contains('}') {
                    debug!("line {line_num}: leaving skinparam block");
                    state = ParseState::Normal;
                } else {
                    trace!("line {line_num}: skipping skinparam content");
                }
                continue;
            }
            ParseState::LegendBlock { .. } => {
                let trimmed = line.trim().to_lowercase();
                if trimmed == "end legend" || trimmed == "endlegend" {
                    debug!("line {line_num}: leaving legend block");
                    state = ParseState::Normal;
                } else {
                    trace!("line {line_num}: skipping legend content");
                }
                continue;
            }
            ParseState::HeaderBlock { .. } => {
                let trimmed = line.trim().to_lowercase();
                if trimmed == "end header" || trimmed == "endheader" {
                    debug!("line {line_num}: leaving header block");
                    state = ParseState::Normal;
                } else {
                    trace!("line {line_num}: skipping header content");
                }
                continue;
            }
            ParseState::Note {
                ref position,
                ref mut lines,
                ..
            } => {
                let trimmed = line.trim();
                if trimmed.to_lowercase() == "end note" || trimmed.to_lowercase() == "endnote" {
                    let text = lines.join("\n");
                    debug!(
                        "line {}: closing multi-line note ({:?}), {} lines",
                        line_num,
                        position,
                        lines.len()
                    );
                    events.push(ActivityEvent::Note {
                        position: position.clone(),
                        text,
                    });
                    state = ParseState::Normal;
                } else {
                    // Trim leading indentation uniformly (2 spaces is PlantUML convention)
                    let content = line.strip_prefix("  ").unwrap_or_else(|| line.trim_start());
                    lines.push(content.to_string());
                    trace!("line {line_num}: accumulating note line");
                }
                continue;
            }
            ParseState::Action {
                ref mut text,
                ref mut leading_blank_line,
                ..
            } => {
                // Continue accumulating action text until we find a line ending with `;`
                if let Some(suffix) = line.strip_suffix(';') {
                    // Last line of multi-line action
                    if !text.is_empty() || *leading_blank_line {
                        text.push('\n');
                        *leading_blank_line = false;
                    }
                    text.push_str(suffix);
                    debug!(
                        "line {}: closing multi-line action, text len={}",
                        line_num,
                        text.len()
                    );
                    events.push(ActivityEvent::Action { text: text.clone() });
                    state = ParseState::Normal;
                } else {
                    if !text.is_empty() || *leading_blank_line {
                        text.push('\n');
                        *leading_blank_line = false;
                    }
                    text.push_str(line);
                    trace!("line {line_num}: accumulating action line");
                }
                continue;
            }
            ParseState::Normal => {
                // Fall through to normal parsing below
            }
        }

        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Skip single-line comments
        if trimmed.starts_with('\'') {
            trace!("line {line_num}: skipping comment");
            continue;
        }

        // Handle <style>...</style> blocks
        if trimmed.to_lowercase().starts_with("<style>") {
            // Check if it closes on the same line
            if trimmed.to_lowercase().contains("</style>") {
                debug!("line {line_num}: skipping single-line style block");
            } else {
                debug!("line {line_num}: entering <style> block");
                state = ParseState::StyleBlock {
                    start_line: line_num,
                    start_column: line.find("<style>").unwrap_or(0) + 1,
                    lines: Vec::new(),
                };
            }
            continue;
        }

        // Handle skinparam blocks
        let lower = trimmed.to_lowercase();
        if lower.starts_with("skinparam") {
            if trimmed.contains('{') && !trimmed.contains('}') {
                debug!("line {line_num}: entering skinparam block");
                state = ParseState::SkinparamBlock {
                    start_line: line_num,
                    start_column: line.to_lowercase().find("skinparam").unwrap_or(0) + 1,
                };
            } else {
                debug!("line {line_num}: skipping single-line skinparam");
            }
            continue;
        }

        // Skip directives: title, footer, caption, hide, show
        if lower.starts_with("title ")
            || lower == "title"
            || lower.starts_with("footer ")
            || lower == "footer"
            || lower.starts_with("caption ")
            || lower == "caption"
            || lower.starts_with("hide ")
            || lower.starts_with("show ")
        {
            debug!("line {line_num}: skipping directive: {trimmed}");
            continue;
        }

        // Handle legend (may be multi-line)
        if lower.starts_with("legend") {
            // Single-line legend: `legend right : text`
            // Multi-line: `legend` or `legend right` followed by content until `end legend`
            let rest = trimmed[6..].trim();
            if rest.is_empty()
                || rest.to_lowercase() == "left"
                || rest.to_lowercase() == "right"
                || rest.to_lowercase() == "center"
            {
                debug!("line {line_num}: entering legend block");
                state = ParseState::LegendBlock {
                    start_line: line_num,
                    start_column: line.to_lowercase().find("legend").unwrap_or(0) + 1,
                };
            } else {
                debug!("line {line_num}: skipping single-line legend");
            }
            continue;
        }

        // Handle header (may be multi-line)
        if lower == "header" || lower.starts_with("header ") {
            // `header` alone starts multi-line; `header text` is single-line
            if lower == "header" {
                debug!("line {line_num}: entering header block");
                state = ParseState::HeaderBlock {
                    start_line: line_num,
                    start_column: line.to_lowercase().find("header").unwrap_or(0) + 1,
                };
            } else {
                debug!("line {line_num}: skipping single-line header");
            }
            continue;
        }

        // --- Direction ---
        if lower == "left to right direction" {
            direction = Direction::LeftToRight;
            debug!("line {line_num}: direction set to LeftToRight");
            continue;
        }
        if lower == "top to bottom direction" {
            direction = Direction::TopToBottom;
            debug!("line {line_num}: direction set to TopToBottom");
            continue;
        }

        // --- Action: `:text;` (may be multi-line) ---
        if let Some(after_colon) = trimmed.strip_prefix(':') {
            if let Some(text) = after_colon.strip_suffix(';') {
                // Single-line action.
                // Java Display.create (legacy mode): \n → line break, \\ → literal backslash.
                let text = expand_backslash_n(text);
                debug!("line {line_num}: single-line action: {text:?}");
                events.push(ActivityEvent::Action { text });
            } else {
                // Start of multi-line action: line starts with `:` but doesn't end with `;`
                // Check the raw line (not trimmed) for the colon position
                let raw_after_colon = if let Some(raw_stripped) = line.strip_suffix(';') {
                    // Edge case: trimmed starts with `:` but raw line ends with `;`
                    let colon_pos = raw_stripped.find(':').unwrap();
                    let raw_content = &raw_stripped[colon_pos + 1..];
                    events.push(ActivityEvent::Action {
                        text: raw_content.to_string(),
                    });
                    debug!("line {line_num}: single-line action (raw): {raw_content:?}");
                    continue;
                } else {
                    after_colon.to_string()
                };
                debug!("line {line_num}: starting multi-line action");
                state = ParseState::Action {
                    text: raw_after_colon,
                    leading_blank_line: after_colon.is_empty(),
                    start_line: line_num,
                    start_column: line.find(':').unwrap_or(0) + 1,
                };
            }
            continue;
        }

        // --- Start / Stop / End ---
        if lower == "start" {
            debug!("line {line_num}: start");
            events.push(ActivityEvent::Start);
            continue;
        }
        if lower == "stop" || lower == "end" {
            debug!("line {line_num}: stop/end");
            events.push(ActivityEvent::Stop);
            continue;
        }

        // --- Swimlane: |Name| ---
        if trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 2 {
            let name = trimmed[1..trimmed.len() - 1].to_string();
            if swimlanes.contains(&name) {
                debug!("line {line_num}: switching to swimlane: {name}");
            } else {
                swimlanes.push(name.clone());
                debug!("line {line_num}: new swimlane: {name}");
            }
            events.push(ActivityEvent::Swimlane { name });
            continue;
        }

        // --- Floating note ---
        if lower.starts_with("floating note left") || lower.starts_with("floating note right") {
            let (position, rest_offset) = if lower.starts_with("floating note left") {
                (NotePosition::Left, "floating note left".len())
            } else {
                (NotePosition::Right, "floating note right".len())
            };
            let rest = trimmed[rest_offset..].trim();
            let text = rest.strip_prefix(':').unwrap_or(rest).trim().to_string();
            debug!("line {line_num}: floating note {position:?}: {text:?}");
            events.push(ActivityEvent::FloatingNote { position, text });
            continue;
        }

        // --- Note (single-line and multi-line) ---
        if lower.starts_with("note left") || lower.starts_with("note right") {
            let (position, rest_offset) = if lower.starts_with("note left") {
                (NotePosition::Left, "note left".len())
            } else {
                (NotePosition::Right, "note right".len())
            };
            let rest = trimmed[rest_offset..].trim();
            if rest.is_empty() {
                // Multi-line note
                debug!("line {line_num}: starting multi-line note {position:?}");
                state = ParseState::Note {
                    position,
                    lines: Vec::new(),
                    start_line: line_num,
                    start_column: line.to_lowercase().find("note").unwrap_or(0) + 1,
                };
            } else {
                // Single-line note: `note right: text`
                let text = rest.strip_prefix(':').unwrap_or(rest).trim().to_string();
                debug!("line {line_num}: single-line note {position:?}: {text:?}");
                events.push(ActivityEvent::Note { position, text });
            }
            continue;
        }

        // --- if (condition) then (label) ---
        if lower.starts_with("if ") || lower.starts_with("if(") {
            if let Some((condition, then_label)) = parse_if_line(trimmed) {
                debug!("line {line_num}: if ({condition}) then ({then_label})");
                if is_old_style {
                    old_graph.add_if(&condition, line_num)?;
                }
                events.push(ActivityEvent::If {
                    condition,
                    then_label,
                });
                continue;
            }
        }

        // --- elseif (condition) then (label) ---
        if lower.starts_with("elseif ")
            || lower.starts_with("elseif(")
            || lower.starts_with("else if ")
            || lower.starts_with("else if(")
        {
            if let Some((condition, label)) = parse_elseif_line(trimmed) {
                debug!("line {line_num}: elseif ({condition}) then ({label})");
                events.push(ActivityEvent::ElseIf { condition, label });
                continue;
            }
        }

        // --- else (label) ---
        if lower.starts_with("else") {
            // Make sure it's not "elseif"
            if !lower.starts_with("elseif") && !lower.starts_with("else if") {
                let rest = trimmed[4..].trim();
                let label = extract_parenthesized(rest).unwrap_or_default();
                debug!("line {line_num}: else ({label})");
                if is_old_style {
                    old_graph.enter_else(line_num)?;
                }
                events.push(ActivityEvent::Else { label });
                continue;
            }
        }

        // --- endif ---
        if lower == "endif" {
            debug!("line {line_num}: endif");
            if is_old_style {
                old_graph.end_if(line_num)?;
            }
            events.push(ActivityEvent::EndIf);
            continue;
        }

        // --- while (condition) is (label) ---
        if lower.starts_with("while ") || lower.starts_with("while(") {
            if let Some((condition, label)) = parse_while_line(trimmed) {
                debug!("line {line_num}: while ({condition}) is ({label})");
                events.push(ActivityEvent::While { condition, label });
                continue;
            }
        }

        // --- endwhile (label) ---
        if lower.starts_with("endwhile") || lower.starts_with("end while") {
            let rest = if lower.starts_with("endwhile") {
                trimmed[8..].trim()
            } else {
                trimmed[9..].trim()
            };
            let label = extract_parenthesized(rest).unwrap_or_default();
            debug!("line {line_num}: endwhile ({label})");
            events.push(ActivityEvent::EndWhile { label });
            continue;
        }

        // --- repeat ---
        if lower == "repeat" {
            debug!("line {line_num}: repeat");
            events.push(ActivityEvent::Repeat);
            continue;
        }

        // --- repeat while (condition) is (label) not (label) ---
        if lower.starts_with("repeat while") {
            let rest = trimmed[12..].trim();
            let condition = extract_parenthesized(rest).unwrap_or_default();
            // Parse optional "is (label)" and "not (label)" after the condition
            let (is_text, not_text) = {
                let after_cond = rest
                    .strip_prefix('(')
                    .and_then(|s| {
                        let mut depth = 1;
                        for (i, ch) in s.char_indices() {
                            match ch {
                                '(' => depth += 1,
                                ')' => {
                                    depth -= 1;
                                    if depth == 0 {
                                        return Some(&s[i + 1..]);
                                    }
                                }
                                _ => {}
                            }
                        }
                        None
                    })
                    .unwrap_or("");
                let after_trim = after_cond.trim();
                let lower_after = after_trim.to_lowercase();
                if lower_after.starts_with("is ") {
                    let is_rest = after_trim[3..].trim();
                    let is_label = extract_parenthesized(is_rest);
                    // Look for "not (label)" after the is clause
                    let not_label = is_rest
                        .strip_prefix('(')
                        .and_then(|s| {
                            let mut depth = 1;
                            for (i, ch) in s.char_indices() {
                                match ch {
                                    '(' => depth += 1,
                                    ')' => {
                                        depth -= 1;
                                        if depth == 0 {
                                            return Some(&s[i + 1..]);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            None
                        })
                        .and_then(|after_is_paren| {
                            let t = after_is_paren.trim();
                            if t.to_lowercase().starts_with("not ") {
                                let not_rest = t[4..].trim();
                                extract_parenthesized(not_rest).filter(|s| !s.is_empty())
                            } else {
                                None
                            }
                        });
                    (is_label.filter(|s| !s.is_empty()), not_label)
                } else if lower_after.starts_with("not ") {
                    let not_rest = after_trim[4..].trim();
                    let not_label = extract_parenthesized(not_rest).filter(|s| !s.is_empty());
                    (None, not_label)
                } else {
                    (None, None)
                }
            };
            debug!("line {line_num}: repeat while ({condition}) is={is_text:?} not={not_text:?}");
            last_repeat_while = Some((events.len(), line_num));
            events.push(ActivityEvent::RepeatWhile {
                condition,
                is_text,
                not_text,
            });
            continue;
        }

        // --- fork / fork again / end fork ---
        if lower == "fork" {
            debug!("line {line_num}: fork");
            events.push(ActivityEvent::Fork);
            continue;
        }
        if lower == "fork again" {
            debug!("line {line_num}: fork again");
            events.push(ActivityEvent::ForkAgain);
            continue;
        }
        if lower == "end fork" || lower == "endfork" {
            debug!("line {line_num}: end fork");
            events.push(ActivityEvent::EndFork);
            continue;
        }

        // --- detach ---
        if lower == "detach" {
            debug!("line {line_num}: detach");
            events.push(ActivityEvent::Detach);
            continue;
        }

        // --- label NAME ---
        if lower.starts_with("label ") {
            let name = trimmed[6..].trim().trim_end_matches(';').trim().to_string();
            if !name.is_empty() {
                debug!("line {line_num}: label {name}");
                events.push(ActivityEvent::Label { name });
                continue;
            }
        }

        // --- goto NAME ---
        if lower.starts_with("goto ") {
            let name = trimmed[5..].trim().trim_end_matches(';').trim().to_string();
            if !name.is_empty() {
                debug!("line {line_num}: goto {name}");
                events.push(ActivityEvent::Goto { name });
                continue;
            }
        }

        // --- backward:text; ---
        if lower.starts_with("backward:") || lower.starts_with("backward :") {
            let colon_pos = trimmed.find(':').unwrap();
            let rest = &trimmed[colon_pos + 1..];
            let text = rest.trim_end_matches(';').trim().to_string();
            debug!("line {line_num}: backward: {text}");
            events.push(ActivityEvent::Backward { text });
            continue;
        }

        // --- break ---
        if lower == "break" || lower == "break;" {
            debug!("line {line_num}: break");
            events.push(ActivityEvent::Break);
            continue;
        }

        // --- Sync bar: ===NAME=== (standalone, not part of an arrow line) ---
        if let Some(name) = parse_sync_bar(trimmed) {
            let key = format!("==={name}===");
            if !old_seen_nodes.contains(&key) {
                debug!("line {line_num}: standalone sync bar: {name}");
                is_old_style = true;
                old_graph.ensure_sync_bar(&name);
                events.push(ActivityEvent::SyncBar(name.clone()));
                old_seen_nodes.insert(key);
            } else {
                debug!("line {line_num}: goto existing sync bar: {name}");
                events.push(ActivityEvent::GotoSyncBar(name));
            }
            continue;
        }

        // --- `-> label;` after a `repeat while`: attach as the loop's exit
        // (not) label.  PlantUML allows the exit label on its own line:
        //   repeat while (cond) is (yes)
        //   -> no;
        // Without this, the line falls through to old-style arrow parsing
        // which rejects it (issue #31: "old-style arrow has no source").
        // The attach is gated on line adjacency (`line_num == rw_line + 1`) so
        // only the label immediately following the `repeat while` qualifies —
        // a stray `-> label;` further down never attaches to a prior loop
        // (review #44).
        if let Some((rw_idx, rw_line)) = last_repeat_while {
            if line_num == rw_line + 1 && trimmed.starts_with("->") {
                let raw = trimmed[2..].trim().trim_end_matches(';').trim();
                if !raw.is_empty() {
                    if let Some(ActivityEvent::RepeatWhile { not_text, .. }) =
                        events.get_mut(rw_idx)
                    {
                        if not_text.is_none() {
                            *not_text = Some(raw.to_string());
                            debug!("line {line_num}: repeat exit label -> {raw:?}");
                        }
                    }
                    last_repeat_while = None;
                    continue;
                }
            }
            // Any other line (or a `->` that is not immediately adjacent)
            // closes the window for attaching an exit label to this loop.
            last_repeat_while = None;
        }

        // --- Old-style arrow lines: [source] --> [label] target ---
        if let Some(parsed) = parse_old_style_arrow_line(trimmed) {
            debug!("line {line_num}: old-style arrow: {:?}", parsed);
            is_old_style = true;
            old_graph.add_old_arrow(&parsed, line_num)?;
            // Emit source node if present and non-continuation.
            // When the source is a sync bar that's already placed, emit
            // ResumeFromSyncBar to reposition y_cursor without convergence.
            if let Some(ref src) = parsed.source {
                let src_trimmed = src.trim();
                if let Some(name) = parse_sync_bar(src_trimmed) {
                    let key = format!("==={name}===");
                    if old_seen_nodes.contains(&key) {
                        debug!("line {line_num}: resume from sync bar: {name}");
                        events.push(ActivityEvent::ResumeFromSyncBar(name));
                    } else {
                        emit_old_node(&mut events, src, line_num, &mut old_seen_nodes);
                    }
                } else {
                    emit_old_node(&mut events, src, line_num, &mut old_seen_nodes);
                }
            }
            // Emit target node
            emit_old_node(&mut events, &parsed.target, line_num, &mut old_seen_nodes);
            continue;
        }

        warn!("line {line_num}: unrecognized activity diagram line: {trimmed}");
    }

    // Verify state machine ended cleanly
    match state {
        ParseState::Normal => {}
        ParseState::Action {
            start_line,
            start_column,
            ..
        } => {
            return Err(crate::Error::Parse {
                line: start_line,
                column: Some(start_column),
                message: "unterminated action (missing closing `;`)".to_string(),
            });
        }
        ParseState::Note {
            start_line,
            start_column,
            ..
        } => {
            return Err(crate::Error::Parse {
                line: start_line,
                column: Some(start_column),
                message: "unterminated note (missing `end note`)".to_string(),
            });
        }
        ParseState::StyleBlock {
            start_line,
            start_column,
            ..
        } => {
            return Err(crate::Error::Parse {
                line: start_line,
                column: Some(start_column),
                message: "unterminated <style> block (missing `</style>`)".to_string(),
            });
        }
        ParseState::SkinparamBlock {
            start_line,
            start_column,
        } => {
            return Err(crate::Error::Parse {
                line: start_line,
                column: Some(start_column),
                message: "unterminated skinparam block (missing `}`)".to_string(),
            });
        }
        ParseState::LegendBlock {
            start_line,
            start_column,
        } => {
            return Err(crate::Error::Parse {
                line: start_line,
                column: Some(start_column),
                message: "unterminated legend block (missing `end legend`)".to_string(),
            });
        }
        ParseState::HeaderBlock {
            start_line,
            start_column,
        } => {
            return Err(crate::Error::Parse {
                line: start_line,
                column: Some(start_column),
                message: "unterminated header block (missing `end header`)".to_string(),
            });
        }
    }

    debug!(
        "parsed activity diagram: {} events, {} swimlanes, old_style={}",
        events.len(),
        swimlanes.len(),
        is_old_style
    );
    Ok(ActivityDiagram {
        events,
        swimlanes,
        direction,
        note_max_width,
        is_old_style,
        old_graph: is_old_style.then(|| old_graph.finish()),
    })
}

/// Extract content inside double quotes: `"text"` -> `text`
/// Returns None if no quotes found.
fn extract_quoted(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(start) = s.find('"') {
        if let Some(end) = s[start + 1..].find('"') {
            return Some(s[start + 1..start + 1 + end].to_string());
        }
    }
    None
}

/// Extract the content of the first balanced parenthesized group in `s`.
///
/// Supports nested parentheses so that labels like `(a(b)c)` or `(a%n()b)`
/// (from substituted variable values containing `%n()` literals) are parsed
/// correctly. Returns `None` if no balanced group is found.
fn extract_parenthesized(s: &str) -> Option<String> {
    let s = s.trim();
    let start = s.find('(')?;
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start + 1..i].trim().to_string());
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Parse an `if (condition) then (label)` or `if "condition" then` line
fn parse_if_line(line: &str) -> Option<(String, String)> {
    let lower = line.to_lowercase();
    // Find "if" then first parenthesized group for condition
    let if_pos = lower.find("if")?;
    let after_if = &line[if_pos + 2..];

    // Try parenthesized condition first: `if (cond) then (label)`
    if let Some(condition) = extract_parenthesized(after_if) {
        let lower_after = after_if.to_lowercase();
        let then_pos = lower_after.find("then")?;
        let after_then = &after_if[then_pos + 4..];
        let then_label = extract_parenthesized(after_then).unwrap_or_default();
        return Some((condition, then_label));
    }

    // Try quoted condition (old-style): `if "cond" then`
    if let Some(condition) = extract_quoted(after_if) {
        let lower_after = after_if.to_lowercase();
        if lower_after.contains("then") {
            return Some((condition, String::new()));
        }
    }

    None
}

/// Parse an `elseif (condition) then (label)` line
fn parse_elseif_line(line: &str) -> Option<(String, String)> {
    let lower = line.to_lowercase();
    // Find the keyword boundary
    let keyword_end = if lower.starts_with("else if") {
        7 // "else if"
    } else if lower.starts_with("elseif") {
        6 // "elseif"
    } else {
        return None;
    };
    let after_keyword = &line[keyword_end..];
    let condition = extract_parenthesized(after_keyword)?;

    let lower_after = after_keyword.to_lowercase();
    let then_pos = lower_after.find("then")?;
    let after_then = &after_keyword[then_pos + 4..];
    let label = extract_parenthesized(after_then).unwrap_or_default();

    Some((condition, label))
}

/// Expand `\n` → newline, `\\` → literal backslash (Java Display.create legacy mode).
/// Also expands U+E100 (from `%newline()` preprocessor).
fn expand_backslash_n(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            match chars[i + 1] {
                'n' => {
                    result.push('\n');
                    i += 2;
                }
                '\\' => {
                    result.push('\\');
                    i += 2;
                }
                other => {
                    result.push('\\');
                    result.push(other);
                    i += 2;
                }
            }
        } else if chars[i] == crate::NEWLINE_CHAR {
            result.push('\n');
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Parse a `while (condition) is (label)` line
fn parse_while_line(line: &str) -> Option<(String, String)> {
    let lower = line.to_lowercase();
    let while_pos = lower.find("while")?;
    let after_while = &line[while_pos + 5..];
    let condition = extract_parenthesized(after_while)?;

    // Optional "is (label)" part
    let lower_after = after_while.to_lowercase();
    let label = if let Some(is_pos) = lower_after.find(" is ") {
        let after_is = &after_while[is_pos + 4..];
        extract_parenthesized(after_is).unwrap_or_default()
    } else {
        String::new()
    };

    Some((condition, label))
}

/// Extract `MaximumWidth` value from a `note { ... }` block inside `<style>` lines.
///
/// Scans for a `note {` line, then looks for `MaximumWidth NNN` inside that block.
fn extract_note_max_width(style_lines: &[String]) -> Option<f64> {
    let mut in_note_block = false;
    let mut brace_depth = 0;
    for line in style_lines {
        let trimmed = line.trim().to_lowercase();
        if !in_note_block {
            // Look for "note {" — may be nested inside another block
            if trimmed.contains("note") && trimmed.contains('{') {
                in_note_block = true;
                brace_depth = 1;
            }
        } else {
            // Track nested braces
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            in_note_block = false;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            // Check for MaximumWidth inside the note block
            let orig_trimmed = line.trim();
            if let Some(rest) = orig_trimmed
                .strip_prefix("MaximumWidth")
                .or_else(|| orig_trimmed.strip_prefix("maximumwidth"))
                .or_else(|| {
                    let lower = orig_trimmed.to_lowercase();
                    if lower.starts_with("maximumwidth") {
                        Some(&orig_trimmed[12..])
                    } else {
                        None
                    }
                })
            {
                if let Ok(val) = rest.trim().parse::<f64>() {
                    return Some(val);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Old-style activity diagram helpers
// ---------------------------------------------------------------------------

/// Try to parse `===NAME===` sync bar pattern, returning the name.
fn parse_sync_bar(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() > 6 && s.starts_with("===") && s.ends_with("===") {
        let name = s[3..s.len() - 3].trim().to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

/// Represents a parsed old-style arrow line.
#[derive(Debug)]
struct OldArrowLine {
    source: Option<String>,
    target: String,
    label: String,
    length: u32,
}

/// Parse old-style arrow lines:
///   `(*) --> VerifyReservation`
///   `-> [incorrect] sendToAirport`
///   `--> (*)`
///   `-->[correct] getPreference`
///   `--> ===Y1===`
///   `===Y1=== --> PrintBoadingboard`
fn parse_old_style_arrow_line(line: &str) -> Option<OldArrowLine> {
    let trimmed = line.trim();

    // Find the arrow (`-->` or `->`)
    let (arrow_start, arrow_end) = find_arrow(trimmed)?;

    let before_arrow = trimmed[..arrow_start].trim();
    let after_arrow = trimmed[arrow_end..].trim();

    // Extract optional label [text] from after the arrow
    let (label, target_str) = extract_bracket_label(after_arrow);

    let target = target_str.trim().to_string();
    if target.is_empty() {
        return None;
    }

    let source = if before_arrow.is_empty() {
        None
    } else {
        Some(before_arrow.to_string())
    };

    Some(OldArrowLine {
        source,
        target,
        label,
        length: (arrow_end - arrow_start - 1) as u32,
    })
}

/// Find the position of an arrow (`-->` or `->`, also `--->`, `---->`, etc.)
/// Returns (start, end) byte offsets in the string.
fn find_arrow(s: &str) -> Option<(usize, usize)> {
    // Match `-+>` pattern (one or more dashes followed by `>`)
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'-' {
            let start = i;
            while i < bytes.len() && bytes[i] == b'-' {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'>' {
                return Some((start, i + 1));
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Extract `[label]` from the beginning of a string.
/// Returns (label, remaining_string).
fn extract_bracket_label(s: &str) -> (String, String) {
    let trimmed = s.trim();
    if trimmed.starts_with('[') {
        if let Some(end) = trimmed.find(']') {
            let label = trimmed[1..end].trim().to_string();
            let rest = trimmed[end + 1..].trim().to_string();
            return (label, rest);
        }
    }
    (String::new(), trimmed.to_string())
}

/// Emit an event for an old-style node reference.
/// Deduplicates sync bars (only first occurrence creates a node).
fn emit_old_node(
    events: &mut Vec<ActivityEvent>,
    node_ref: &str,
    line_num: usize,
    seen: &mut std::collections::HashSet<String>,
) {
    let trimmed = node_ref.trim();
    if trimmed == "(*)" {
        // Start or stop — in old syntax, (*) at the beginning is start,
        // (*) as a target is stop. For simplicity, we check if we already
        // have a Start event to decide.
        let has_start = events.iter().any(|e| matches!(e, ActivityEvent::Start));
        if has_start {
            debug!("line {line_num}: old-style stop: (*)");
            events.push(ActivityEvent::Stop);
        } else {
            debug!("line {line_num}: old-style start: (*)");
            events.push(ActivityEvent::Start);
        }
    } else if let Some(name) = parse_sync_bar(trimmed) {
        let key = format!("==={name}===");
        if !seen.contains(&key) {
            debug!("line {line_num}: old-style sync bar: {name}");
            events.push(ActivityEvent::SyncBar(name.clone()));
            seen.insert(key);
        } else {
            debug!("line {line_num}: goto existing sync bar: {name}");
            events.push(ActivityEvent::GotoSyncBar(name));
        }
    } else {
        // Regular action node — only emit if not seen before
        if !seen.contains(trimmed) {
            debug!("line {line_num}: old-style action: {trimmed}");
            events.push(ActivityEvent::Action {
                text: trimmed.to_string(),
            });
            seen.insert(trimmed.to_string());
        } else {
            debug!("line {line_num}: skipping duplicate action: {trimmed}");
        }
    }
}

#[derive(Default)]
struct OldGraphBuilder {
    counter: u32,
    nodes: Vec<OldActivityNode>,
    node_index: HashMap<String, usize>,
    links: Vec<OldActivityLink>,
    last_entity_id: Option<String>,
    if_stack: Vec<String>,
}

impl OldGraphBuilder {
    fn new() -> Self {
        Self {
            counter: 1,
            ..Default::default()
        }
    }

    fn finish(self) -> OldActivityGraph {
        OldActivityGraph {
            nodes: self.nodes,
            links: self.links,
        }
    }

    fn next_value(&mut self) -> u32 {
        self.counter += 1;
        self.counter
    }

    fn next_link_uid(&mut self) -> String {
        format!("lnk{}", self.next_value())
    }

    fn next_entity_uid(&mut self) -> String {
        format!("ent{:04}", self.next_value())
    }

    fn next_generated_id(&mut self, prefix: &str) -> String {
        format!("{prefix}{}", self.next_value())
    }

    fn ensure_node(
        &mut self,
        id: &str,
        kind: OldActivityNodeKind,
        text: String,
        qualified_name: String,
    ) -> String {
        if let Some(idx) = self.node_index.get(id).copied() {
            return self.nodes[idx].id.clone();
        }
        let node = OldActivityNode {
            id: id.to_string(),
            uid: self.next_entity_uid(),
            qualified_name,
            kind,
            text,
        };
        let idx = self.nodes.len();
        self.nodes.push(node);
        self.node_index.insert(id.to_string(), idx);
        id.to_string()
    }

    fn ensure_start(&mut self) -> String {
        self.ensure_node(
            "start",
            OldActivityNodeKind::Start,
            String::new(),
            "start".to_string(),
        )
    }

    fn ensure_end(&mut self) -> String {
        self.ensure_node(
            "end",
            OldActivityNodeKind::End,
            String::new(),
            "end".to_string(),
        )
    }

    fn ensure_action(&mut self, text: &str) -> String {
        self.ensure_node(
            text,
            OldActivityNodeKind::Action,
            text.to_string(),
            text.to_string(),
        )
    }

    fn ensure_sync_bar(&mut self, name: &str) -> String {
        self.ensure_node(
            name,
            OldActivityNodeKind::SyncBar,
            String::new(),
            name.to_string(),
        )
    }

    fn create_branch(&mut self) -> String {
        let id = self.next_generated_id("#");
        let qualified_name = format!(".{}", id.trim_start_matches('#'));
        self.ensure_node(
            &id,
            OldActivityNodeKind::Branch,
            String::new(),
            qualified_name,
        )
    }

    fn ensure_ref(&mut self, node_ref: &str) -> String {
        let trimmed = node_ref.trim();
        if trimmed == "(*)" {
            if self.node_index.contains_key("start")
                && self.last_entity_id.as_deref() != Some("start")
            {
                return self.ensure_end();
            }
            return self.ensure_start();
        }
        if let Some(name) = parse_sync_bar(trimmed) {
            return self.ensure_sync_bar(&name);
        }
        self.ensure_action(trimmed)
    }

    fn add_link(
        &mut self,
        from_id: String,
        to_id: String,
        label: Option<String>,
        head_label: Option<String>,
        source_line: usize,
        length: u32,
    ) {
        let uid = self.next_link_uid();
        self.links.push(OldActivityLink {
            uid,
            from_id,
            to_id: to_id.clone(),
            label,
            head_label,
            source_line,
            length,
        });
        self.last_entity_id = Some(to_id);
    }

    fn add_old_arrow(&mut self, parsed: &OldArrowLine, line_num: usize) -> Result<()> {
        let from_id = if let Some(source) = parsed.source.as_deref() {
            self.ensure_ref(source)
        } else if let Some(last) = self.last_entity_id.clone() {
            last
        } else {
            return Err(crate::Error::Parse {
                line: line_num,
                column: Some(1),
                message: "old-style arrow has no source".to_string(),
            });
        };
        let to_id = self.ensure_ref(&parsed.target);
        let label = (!parsed.label.is_empty()).then(|| parsed.label.clone());
        self.add_link(from_id, to_id, label, None, line_num, parsed.length.max(1));
        Ok(())
    }

    fn add_if(&mut self, condition: &str, line_num: usize) -> Result<()> {
        let Some(from_id) = self.last_entity_id.clone() else {
            return Err(crate::Error::Parse {
                line: line_num,
                column: Some(1),
                message: "if has no incoming source in old-style activity".to_string(),
            });
        };
        let branch_id = self.create_branch();
        self.add_link(
            from_id,
            branch_id.clone(),
            None,
            Some(condition.to_string()),
            line_num,
            2,
        );
        self.if_stack.push(branch_id);
        Ok(())
    }

    fn enter_else(&mut self, line_num: usize) -> Result<()> {
        let Some(branch_id) = self.if_stack.last().cloned() else {
            return Err(crate::Error::Parse {
                line: line_num,
                column: Some(1),
                message: "else without matching if".to_string(),
            });
        };
        self.last_entity_id = Some(branch_id);
        Ok(())
    }

    fn end_if(&mut self, line_num: usize) -> Result<()> {
        if self.if_stack.pop().is_none() {
            return Err(crate::Error::Parse {
                line: line_num,
                column: Some(1),
                message: "endif without matching if".to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Basic parsing tests ----

    #[test]
    fn parse_basic_action() {
        let src = "@startuml\n:hello world;\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.events.len(), 1);
        if let ActivityEvent::Action { text } = &diagram.events[0] {
            assert_eq!(text, "hello world");
        } else {
            panic!("expected Action event");
        }
    }

    #[test]
    fn parse_multi_line_action() {
        let src = "@startuml\n:line1\nline2\nline3;\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.events.len(), 1);
        if let ActivityEvent::Action { text } = &diagram.events[0] {
            assert_eq!(text, "line1\nline2\nline3");
        } else {
            panic!("expected Action event");
        }
    }

    #[test]
    fn parse_start_stop() {
        let src = "@startuml\nstart\n:do stuff;\nstop\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.events.len(), 3);
        assert!(matches!(&diagram.events[0], ActivityEvent::Start));
        assert!(matches!(&diagram.events[1], ActivityEvent::Action { .. }));
        assert!(matches!(&diagram.events[2], ActivityEvent::Stop));
    }

    #[test]
    fn parse_end_keyword() {
        let src = "@startuml\nstart\n:task;\nend\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.events.len(), 3);
        assert!(matches!(&diagram.events[2], ActivityEvent::Stop));
    }

    #[test]
    fn parse_swimlanes() {
        let src = "@startuml\n|Alice|\nstart\n:task;\n|Bob|\n:other;\nstop\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.swimlanes.len(), 2);
        assert_eq!(diagram.swimlanes[0], "Alice");
        assert_eq!(diagram.swimlanes[1], "Bob");
        // Swimlane events should be present
        assert!(matches!(
            &diagram.events[0],
            ActivityEvent::Swimlane { name } if name == "Alice"
        ));
        assert!(matches!(
            &diagram.events[3],
            ActivityEvent::Swimlane { name } if name == "Bob"
        ));
    }

    #[test]
    fn parse_swimlane_dedup() {
        let src = "@startuml\n|A|\n:x;\n|B|\n:y;\n|A|\n:z;\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.swimlanes.len(), 2);
        assert_eq!(diagram.swimlanes[0], "A");
        assert_eq!(diagram.swimlanes[1], "B");
        // Third |A| should still generate a Swimlane event
        let swimlane_events: Vec<_> = diagram
            .events
            .iter()
            .filter(|e| matches!(e, ActivityEvent::Swimlane { .. }))
            .collect();
        assert_eq!(swimlane_events.len(), 3);
    }

    #[test]
    fn parse_single_line_note() {
        let src = "@startuml\n:task;\nnote right: this is a note\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.events.len(), 2);
        if let ActivityEvent::Note { position, text } = &diagram.events[1] {
            assert_eq!(*position, NotePosition::Right);
            assert_eq!(text, "this is a note");
        } else {
            panic!("expected Note event");
        }
    }

    #[test]
    fn parse_multi_line_note() {
        let src = "@startuml\n:task;\nnote right\n  line one\n  line two\nend note\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.events.len(), 2);
        if let ActivityEvent::Note { position, text } = &diagram.events[1] {
            assert_eq!(*position, NotePosition::Right);
            assert_eq!(text, "line one\nline two");
        } else {
            panic!("expected Note event");
        }
    }

    #[test]
    fn parse_floating_note() {
        let src = "@startuml\nfloating note left: hello there\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.events.len(), 1);
        if let ActivityEvent::FloatingNote { position, text } = &diagram.events[0] {
            assert_eq!(*position, NotePosition::Left);
            assert_eq!(text, "hello there");
        } else {
            panic!("expected FloatingNote event");
        }
    }

    #[test]
    fn parse_if_else_endif() {
        let src = "@startuml\nif (ok?) then (yes)\n:do it;\nelse (no)\n:skip;\nendif\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert!(matches!(
            &diagram.events[0],
            ActivityEvent::If { condition, then_label }
            if condition == "ok?" && then_label == "yes"
        ));
        assert!(matches!(&diagram.events[1], ActivityEvent::Action { .. }));
        assert!(matches!(
            &diagram.events[2],
            ActivityEvent::Else { label } if label == "no"
        ));
        assert!(matches!(&diagram.events[3], ActivityEvent::Action { .. }));
        assert!(matches!(&diagram.events[4], ActivityEvent::EndIf));
    }

    #[test]
    fn parse_elseif() {
        let src = "@startuml\nif (a) then (yes)\n:x;\nelseif (b) then (maybe)\n:y;\nendif\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert!(matches!(
            &diagram.events[2],
            ActivityEvent::ElseIf { condition, label }
            if condition == "b" && label == "maybe"
        ));
    }

    #[test]
    fn parse_while_endwhile() {
        let src = "@startuml\nwhile (has more?) is (yes)\n:process;\nendwhile (done)\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert!(matches!(
            &diagram.events[0],
            ActivityEvent::While { condition, label }
            if condition == "has more?" && label == "yes"
        ));
        assert!(matches!(
            &diagram.events[2],
            ActivityEvent::EndWhile { label } if label == "done"
        ));
    }

    #[test]
    fn parse_repeat_while() {
        let src = "@startuml\nrepeat\n:action;\nrepeat while (again?)\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert!(matches!(&diagram.events[0], ActivityEvent::Repeat));
        assert!(matches!(
            &diagram.events[2],
            ActivityEvent::RepeatWhile { condition, is_text: _, .. } if condition == "again?"
        ));
    }

    #[test]
    fn parse_repeat_while_is_label() {
        let src = "@startuml\nrepeat\n:read;\nrepeat while (more?) is (yes)\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert!(matches!(
            &diagram.events[2],
            ActivityEvent::RepeatWhile { condition, is_text: Some(label), .. }
            if condition == "more?" && label == "yes"
        ));
    }

    #[test]
    fn parse_fork() {
        let src = "@startuml\nfork\n:a;\nfork again\n:b;\nend fork\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert!(matches!(&diagram.events[0], ActivityEvent::Fork));
        assert!(matches!(&diagram.events[2], ActivityEvent::ForkAgain));
        assert!(matches!(&diagram.events[4], ActivityEvent::EndFork));
    }

    #[test]
    fn parse_endfork_no_space() {
        let src = "@startuml\nfork\n:a;\nendfork\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert!(matches!(&diagram.events[2], ActivityEvent::EndFork));
    }

    #[test]
    fn parse_detach() {
        let src = "@startuml\nstart\n:task;\ndetach\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert!(matches!(&diagram.events[2], ActivityEvent::Detach));
    }

    #[test]
    fn skip_style_block() {
        let src = "@startuml\n<style>\nfoo { bar }\n</style>\n:action;\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.events.len(), 1);
        assert!(matches!(&diagram.events[0], ActivityEvent::Action { .. }));
    }

    #[test]
    fn skip_skinparam_block() {
        let src = "@startuml\nskinparam {\nfoo bar\n}\n:action;\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.events.len(), 1);
        assert!(matches!(&diagram.events[0], ActivityEvent::Action { .. }));
    }

    #[test]
    fn skip_comments() {
        let src = "@startuml\n' this is a comment\n:action;\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.events.len(), 1);
    }

    #[test]
    fn unterminated_action_returns_error() {
        let src = "@startuml\n:this has no semicolon\n@enduml";
        let result = parse_activity_diagram(src);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("unterminated action"));
        assert!(err_msg.contains("line 1:1"));
    }

    #[test]
    fn unterminated_note_returns_error() {
        let src = "@startuml\nnote right\nsome text\n@enduml";
        let result = parse_activity_diagram(src);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("unterminated note"));
        assert!(err_msg.contains("line 1:1"));
    }

    // ---- Fixture file tests ----

    #[test]
    fn parse_fixture_a0002() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/activity/a0002.puml"
        ))
        .unwrap();
        let diagram = parse_activity_diagram(&src).unwrap();

        // Swimlanes: Actor 1, Actor 2
        assert_eq!(diagram.swimlanes.len(), 2);
        assert_eq!(diagram.swimlanes[0], "Actor 1");
        assert_eq!(diagram.swimlanes[1], "Actor 2");

        // Expected events in order:
        // Swimlane(Actor 1), Start, Action(foo1), Note(right, multi-line),
        // FloatingNote(left, ...), Swimlane(Actor 2), Action(foo2), Note(right, multi-line), Stop
        assert_eq!(diagram.events.len(), 9);

        assert!(matches!(
            &diagram.events[0],
            ActivityEvent::Swimlane { name } if name == "Actor 1"
        ));
        assert!(matches!(&diagram.events[1], ActivityEvent::Start));
        if let ActivityEvent::Action { text } = &diagram.events[2] {
            assert_eq!(text, "foo1");
        } else {
            panic!("expected Action event at index 2");
        }
        assert!(matches!(
            &diagram.events[3],
            ActivityEvent::Note {
                position: NotePosition::Right,
                ..
            }
        ));
        assert!(matches!(
            &diagram.events[4],
            ActivityEvent::FloatingNote { position: NotePosition::Left, text }
            if text == "This is a note"
        ));
        assert!(matches!(
            &diagram.events[5],
            ActivityEvent::Swimlane { name } if name == "Actor 2"
        ));
        if let ActivityEvent::Action { text } = &diagram.events[6] {
            assert_eq!(text, "foo2");
        } else {
            panic!("expected Action event at index 6");
        }
        assert!(matches!(
            &diagram.events[7],
            ActivityEvent::Note {
                position: NotePosition::Right,
                ..
            }
        ));
        assert!(matches!(&diagram.events[8], ActivityEvent::Stop));
    }

    #[test]
    fn parse_fixture_activity_creole_table_01() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/activity/activity_creole_table_01.puml"
        ))
        .unwrap();
        let diagram = parse_activity_diagram(&src).unwrap();

        assert_eq!(diagram.events.len(), 1);
        if let ActivityEvent::Action { text } = &diagram.events[0] {
            assert_eq!(text, "|Creole Table Line1|\n|Line2|");
        } else {
            panic!("expected Action event");
        }
    }

    #[test]
    fn parse_fixture_activity_creole_table_02() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/activity/activity_creole_table_02.puml"
        ))
        .unwrap();
        let diagram = parse_activity_diagram(&src).unwrap();

        // 3 actions
        assert_eq!(diagram.events.len(), 3);
        for event in &diagram.events {
            assert!(matches!(event, ActivityEvent::Action { .. }));
        }

        // First action is single-line
        if let ActivityEvent::Action { text } = &diagram.events[0] {
            // Java Display.create: \n → line break, \\ → literal backslash
            // Source has \\n = escaped backslash + n → literal "\n" text, not line break
            assert_eq!(text, "| Creole Table \\n multi-line1| a |\n| Line2| b |");
        } else {
            panic!("expected Action");
        }

        // Second action is multi-line (starts with `:` + newline, ends with `;`)
        if let ActivityEvent::Action { text } = &diagram.events[1] {
            assert!(text.starts_with('\n'));
            assert!(text.contains("Creole Table"));
            assert!(text.contains("Line2"));
        } else {
            panic!("expected Action");
        }

        // Third action is also multi-line
        if let ActivityEvent::Action { text } = &diagram.events[2] {
            assert!(text.starts_with('\n'));
            assert!(text.contains("%newline()"));
            assert!(text.contains("Creole Table"));
            assert!(text.contains("Line2"));
        } else {
            panic!("expected Action");
        }
    }

    #[test]
    fn parse_fixture_activity_mono_multi_line() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/activity/activity_mono_multi_line.puml"
        ))
        .unwrap();
        let diagram = parse_activity_diagram(&src).unwrap();

        // 2 actions: one multi-line, one single-line
        assert_eq!(diagram.events.len(), 2);

        // First action: multi-line spanning 2 source lines.
        // Multi-line actions do NOT expand \n — physical newlines already separate.
        if let ActivityEvent::Action { text } = &diagram.events[0] {
            assert_eq!(
                text,
                "Here is the line executed:\na  \\n fprintf(\"hello\\n\", %s)"
            );
        } else {
            panic!("expected Action at index 0");
        }

        // Second action: single-line (all \n expanded to real newlines)
        if let ActivityEvent::Action { text } = &diagram.events[1] {
            assert_eq!(
                text,
                "Here is the line executed:a  \n fprintf(\"hello\n\", %s)"
            );
        } else {
            panic!("expected Action at index 1");
        }
    }

    #[test]
    fn parse_fixture_activity_mono_multi_line2() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/activity/activity_mono_multi_line2.puml"
        ))
        .unwrap();
        let diagram = parse_activity_diagram(&src).unwrap();

        assert_eq!(diagram.events.len(), 1);
        if let ActivityEvent::Action { text } = &diagram.events[0] {
            assert_eq!(text, "1 %n() fprintf( hello%n() , %s)");
        } else {
            panic!("expected Action event");
        }
    }

    #[test]
    fn parse_fixture_activity_mono_multi_line_v2() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/activity/activity_mono_multi_line_v2.puml"
        ))
        .unwrap();
        let diagram = parse_activity_diagram(&src).unwrap();

        // 2 actions
        assert_eq!(diagram.events.len(), 2);

        // First: multi-line with !!! markers
        if let ActivityEvent::Action { text } = &diagram.events[0] {
            assert!(text.starts_with("!!!Here is the line executed:"));
            assert!(text.ends_with("!!!"));
        } else {
            panic!("expected Action at index 0");
        }

        // Second: single-line
        if let ActivityEvent::Action { text } = &diagram.events[1] {
            assert_eq!(
                text,
                "Here is the line executed:a  \n fprintf(\"hello\n\", %s)"
            );
        } else {
            panic!("expected Action at index 1");
        }
    }

    #[test]
    fn parse_fixture_activity_mono_multi_line2_v2() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/activity/activity_mono_multi_line2_v2.puml"
        ))
        .unwrap();
        let diagram = parse_activity_diagram(&src).unwrap();

        // 2 actions
        assert_eq!(diagram.events.len(), 2);

        // First: multi-line (3 source lines) with !!! markers
        if let ActivityEvent::Action { text } = &diagram.events[0] {
            assert!(text.starts_with("!!!Here is the line executed:"));
            assert!(text.contains("foobar"));
            assert!(text.ends_with("!!!"));
        } else {
            panic!("expected Action at index 0");
        }
    }

    #[test]
    fn parse_note_left_single_line() {
        let src = "@startuml\nnote left: left note text\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.events.len(), 1);
        if let ActivityEvent::Note { position, text } = &diagram.events[0] {
            assert_eq!(*position, NotePosition::Left);
            assert_eq!(text, "left note text");
        } else {
            panic!("expected Note event");
        }
    }

    #[test]
    fn parse_empty_diagram() {
        let src = "@startuml\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert!(diagram.events.is_empty());
        assert!(diagram.swimlanes.is_empty());
    }

    #[test]
    fn skip_title_and_directives() {
        let src = "@startuml\ntitle My Diagram\ncaption fig 1\nhide footbox\nshow members\n:action;\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.events.len(), 1);
        assert!(matches!(&diagram.events[0], ActivityEvent::Action { .. }));
    }

    #[test]
    fn parse_direction_left_to_right() {
        let src = "@startuml\nleft to right direction\nstart\n:task;\nstop\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.direction, crate::model::Direction::LeftToRight);
    }

    #[test]
    fn parse_while_without_is_label() {
        let src = "@startuml\nwhile (condition)\n:work;\nendwhile\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert!(matches!(
            &diagram.events[0],
            ActivityEvent::While { condition, label }
            if condition == "condition" && label.is_empty()
        ));
        assert!(matches!(
            &diagram.events[2],
            ActivityEvent::EndWhile { label } if label.is_empty()
        ));
    }

    #[test]
    fn extract_note_maximum_width() {
        let src = "@startuml\n<style>\nactivityDiagram {\n  note {\n    MaximumWidth 100\n  }\n}\n</style>\n:action;\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.note_max_width, Some(100.0));
    }

    #[test]
    fn no_style_means_no_max_width() {
        let src = "@startuml\n:action;\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        assert_eq!(diagram.note_max_width, None);
    }

    #[test]
    fn fixture_a0002_has_max_width() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/activity/a0002.puml"
        ))
        .unwrap();
        let diagram = parse_activity_diagram(&src).unwrap();
        assert_eq!(diagram.note_max_width, Some(100.0));
    }

    // ── issue #31: repeat-while exit label on its own line ──────────────
    //   repeat while (more data?) is (yes)
    //   -> no;
    // The `-> no;` line must attach as the loop's `not_text` exit label
    // instead of falling through to old-style arrow parsing (which rejects
    // it with "old-style arrow has no source").  Gated on line adjacency so
    // only a label immediately following the `repeat while` qualifies.
    #[test]
    fn parse_repeat_while_exit_label_on_own_line() {
        let src = "@startuml\nrepeat\n:read;\nrepeat while (more data?) is (yes)\n-> no;\n@enduml";
        let diagram = parse_activity_diagram(src).unwrap();
        let rw = diagram
            .events
            .iter()
            .find_map(|e| match e {
                ActivityEvent::RepeatWhile { not_text, .. } => Some(not_text.clone()),
                _ => None,
            })
            .expect("expected a RepeatWhile event");
        assert_eq!(rw, Some("no".to_string()));
    }
}
