use log::{debug, trace, warn};

use crate::model::state::{State, StateDiagram, StateKind, StateNote, Transition};
use crate::model::Direction;
use crate::Result;

/// Parser state for multi-line constructs
#[derive(Debug)]
enum ParseMode {
    /// Normal line-by-line parsing
    Normal,
    /// Inside a `note as ALIAS` block, accumulating text until `end note`
    Note {
        alias: Option<String>,
        entity_id: Option<String>,
        position: Option<String>,
        target: Option<String>,
        source_line: usize,
        lines: Vec<String>,
        start_line: usize,
        start_column: usize,
    },
    /// Inside a `<style>...</style>` block (skip all content)
    StyleBlock {
        start_line: usize,
        start_column: usize,
    },
    /// Inside a `skinparam { ... }` block (skip all content)
    SkinparamBlock {
        start_line: usize,
        start_column: usize,
    },
}

/// Context frame for composite state nesting
#[derive(Debug)]
struct CompositeFrame {
    /// The composite state being built
    state: State,
    /// Transitions collected inside this composite scope
    transitions: Vec<Transition>,
    /// Source location of the opening `state ... {` line.
    start_line: usize,
    start_column: usize,
    /// Java-style CONC<N> name for the current concurrent region. None for
    /// the first region (which uses the parent state directly). Set to
    /// `CONC<N>` after each `--` separator inside this frame.
    current_conc_scope: Option<String>,
}

/// Parse state diagram source text into a StateDiagram IR
pub fn parse_state_diagram(source: &str) -> Result<StateDiagram> {
    let block = super::common::extract_block(source).unwrap_or_else(|| source.to_string());

    // Pre-process: join continuation lines (trailing `\`) while preserving
    // the original physical line numbers within the extracted @startuml block.
    let joined = join_continuation_lines(&block);

    let mut top_states: Vec<State> = Vec::new();
    let mut top_transitions: Vec<Transition> = Vec::new();
    let mut notes: Vec<StateNote> = Vec::new();
    let mut note_sequence = 1usize;
    let mut direction = Direction::default();
    let mut mode = ParseMode::Normal;

    // Stack for composite state nesting
    let mut stack: Vec<CompositeFrame> = Vec::new();
    // Java cpt2 starts at 1 and addAndGet(1) returns the next, so the first
    // CONC scope is CONC2. Mirror that here so qualified names match.
    let mut conc_counter: u32 = 1;

    for (line_num, line) in joined {
        match mode {
            ParseMode::StyleBlock { .. } => {
                if line.trim().to_lowercase().starts_with("</style>") {
                    debug!("line {line_num}: leaving <style> block");
                    mode = ParseMode::Normal;
                } else {
                    trace!("line {line_num}: skipping style content");
                }
                continue;
            }
            ParseMode::SkinparamBlock { .. } => {
                if line.trim().contains('}') {
                    debug!("line {line_num}: leaving skinparam block");
                    mode = ParseMode::Normal;
                } else {
                    trace!("line {line_num}: skipping skinparam content");
                }
                continue;
            }
            ParseMode::Note {
                ref alias,
                ref entity_id,
                ref position,
                ref target,
                ref mut source_line,
                ref mut lines,
                ..
            } => {
                let trimmed = line.trim();
                let lower = trimmed.to_lowercase();
                if lower == "end note" || lower == "endnote" {
                    let text = lines.join("\n");
                    debug!(
                        "line {}: closing note (alias={:?}), {} lines",
                        line_num,
                        alias,
                        lines.len()
                    );
                    notes.push(StateNote {
                        alias: alias.clone(),
                        entity_id: entity_id.clone(),
                        text,
                        position: position.clone().unwrap_or_default(),
                        target: target.clone(),
                        source_line: Some(*source_line),
                    });
                    mode = ParseMode::Normal;
                } else {
                    if lines.is_empty() {
                        // Java multiline note line location points at the first body line,
                        // not the `note ...` opener.
                        *source_line = line_num;
                    }
                    lines.push(trimmed.to_string());
                    trace!("line {line_num}: accumulating note line");
                }
                continue;
            }
            ParseMode::Normal => {
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

        let lower = trimmed.to_lowercase();

        // Handle <style>...</style> blocks
        if lower.starts_with("<style>") {
            if lower.contains("</style>") {
                debug!("line {line_num}: skipping single-line style block");
            } else {
                debug!("line {line_num}: entering <style> block");
                mode = ParseMode::StyleBlock {
                    start_line: line_num,
                    start_column: line.find("<style>").unwrap_or(0) + 1,
                };
            }
            continue;
        }

        // Handle skinparam
        if lower.starts_with("skinparam") {
            if trimmed.contains('{') && !trimmed.contains('}') {
                debug!("line {line_num}: entering skinparam block");
                mode = ParseMode::SkinparamBlock {
                    start_line: line_num,
                    start_column: line.to_lowercase().find("skinparam").unwrap_or(0) + 1,
                };
            } else {
                debug!("line {line_num}: skipping single-line skinparam");
            }
            continue;
        }

        // Skip directives: hide, show, title, footer, caption
        if lower.starts_with("hide ")
            || lower.starts_with("show ")
            || lower.starts_with("title ")
            || lower == "title"
            || lower.starts_with("footer ")
            || lower == "footer"
            || lower.starts_with("caption ")
            || lower == "caption"
        {
            debug!("line {line_num}: skipping directive: {trimmed}");
            continue;
        }

        // Direction
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

        // Handle concurrent region separator `--` inside composite states
        if trimmed == "--" {
            if let Some(frame) = stack.last_mut() {
                // Move current children into a new region, start fresh for the next region
                let current_children = std::mem::take(&mut frame.state.children);
                frame.state.regions.push(current_children);
                // Java getUniqueSequence2("CONC") returns CONC<n+1> for the
                // next region. Each `--` advances the per-diagram counter.
                conc_counter += 1;
                frame.current_conc_scope = Some(format!("CONC{conc_counter}"));
                debug!(
                    "line {}: concurrent region separator in composite '{}' → scope {}",
                    line_num,
                    frame.state.id,
                    frame.current_conc_scope.as_deref().unwrap_or("")
                );
            } else {
                warn!("line {line_num}: `--` separator outside composite state");
            }
            continue;
        }

        // Handle closing brace `}` for composite states
        if trimmed == "}" {
            if let Some(mut frame) = stack.pop() {
                // Apply Java two-pass ordering to children before closing.
                reorder_java_pass_order(&mut frame.state.children);
                for region in &mut frame.state.regions {
                    reorder_java_pass_order(region);
                }
                debug!(
                    "line {}: closing composite state '{}' with {} children, {} transitions",
                    line_num,
                    frame.state.id,
                    frame.state.children.len(),
                    frame.transitions.len()
                );
                let completed_state = frame.state;
                let inner_transitions = frame.transitions;

                // Push the completed composite state into the parent scope
                if let Some(parent) = stack.last_mut() {
                    merge_or_add_state(&mut parent.state.children, completed_state);
                    parent.transitions.extend(inner_transitions);
                } else {
                    merge_or_add_state(&mut top_states, completed_state);
                    top_transitions.extend(inner_transitions);
                }
            } else {
                warn!("line {line_num}: unexpected closing brace");
            }
            continue;
        }

        // Handle note: `note as ALIAS`
        if lower.starts_with("note ") {
            if let Some(alias) = parse_note_as(trimmed) {
                debug!("line {line_num}: starting note with alias '{alias}'");
                mode = ParseMode::Note {
                    alias: Some(alias.clone()),
                    entity_id: Some(alias.clone()),
                    position: None,
                    target: None,
                    source_line: line_num,
                    lines: Vec::new(),
                    start_line: line_num,
                    start_column: line.to_lowercase().find("note").unwrap_or(0) + 1,
                };
                continue;
            }
            // Handle `note right of <State> : text` or `note left of <State> : text`
            if let Some((position, target, note_text)) = parse_note_direction_inline(trimmed) {
                let entity_id = format!("GMN{}", note_sequence + 1);
                note_sequence += 1;
                debug!(
                    "line {line_num}: inline note position={position} target={target} text='{note_text}'"
                );
                notes.push(StateNote {
                    alias: None,
                    entity_id: Some(entity_id),
                    text: note_text,
                    position,
                    target: Some(target),
                    source_line: Some(line_num),
                });
                continue;
            }

            // Handle multi-line `note left of <State>` / `note right of <State>` (no inline text)
            if let Some((position, target)) = parse_note_direction_block_start(trimmed) {
                let entity_id = format!("GMN{}", note_sequence + 1);
                note_sequence += 1;
                debug!(
                    "line {line_num}: starting multi-line note block position={position} target={target}"
                );
                mode = ParseMode::Note {
                    alias: None,
                    entity_id: Some(entity_id),
                    position: Some(position),
                    target: Some(target),
                    source_line: line_num,
                    lines: Vec::new(),
                    start_line: line_num,
                    start_column: line.to_lowercase().find("note").unwrap_or(0) + 1,
                };
                continue;
            }

            // Skip other unrecognized note syntax
            debug!("line {line_num}: skipping unrecognized note syntax");
            continue;
        }

        // Handle `state` keyword
        if lower.starts_with("state ") {
            let rest = &trimmed[6..];

            // Check for composite state: `state Name {` or `state Name{` or
            // `state "Quoted" <<stereo>> {`
            if let Some(composite) = try_parse_composite_state(rest) {
                debug!(
                    "line {}: entering composite state '{}' (name='{}')",
                    line_num, composite.id, composite.name
                );
                let mut composite = composite;
                composite.source_line = Some(line_num);
                composite.explicit_source_line = Some(line_num);
                stack.push(CompositeFrame {
                    state: composite,
                    transitions: Vec::new(),
                    start_line: line_num,
                    start_column: line.to_lowercase().find("state").unwrap_or(0) + 1,
                    current_conc_scope: None,
                });
                continue;
            }

            // Simple state declaration: `state Name`, `state "Quoted"`,
            // `state Name <<stereo>>`, `state Name: desc`
            if let Some(mut parsed) = parse_state_declaration(rest) {
                debug!(
                    "line {}: state declaration id='{}', name='{}', stereo={:?}, desc={:?}",
                    line_num, parsed.id, parsed.name, parsed.stereotype, parsed.description
                );
                parsed.source_line = Some(line_num);
                parsed.explicit_source_line = Some(line_num);
                let current_states = current_states_mut(&mut stack, &mut top_states);
                merge_or_add_state(current_states, parsed);
                continue;
            }

            warn!("line {line_num}: unrecognized state declaration: {trimmed}");
            continue;
        }

        // Handle transitions: `A --> B : label`, `A -> B`, `[*] --> A`
        if let Some(mut trans) = try_parse_transition(trimmed) {
            scope_transition_special_state_ids(&mut trans, &stack);
            debug!(
                "line {}: transition '{}' -> '{}' label='{}' dashed={}",
                line_num, trans.from, trans.to, trans.label, trans.dashed
            );
            // Java uses 0-based line numbers from the original file.
            // Our line_num is 1-based within the block (after @startuml strip),
            // which equals 0-based original file line (accounting for @startuml).
            trans.source_line = Some(line_num);

            // Auto-create states referenced in this transition
            let current_states = current_states_mut(&mut stack, &mut top_states);
            ensure_state(current_states, &trans.from, trans.source_line);
            ensure_state(current_states, &trans.to, trans.source_line);

            let current_transitions = current_transitions_mut(&mut stack, &mut top_transitions);
            current_transitions.push(trans);
            continue;
        }

        // Handle description addition: `StateName : description text`
        if let Some((state_id, desc)) = try_parse_description_line(trimmed) {
            debug!("line {line_num}: adding description to '{state_id}': '{desc}'");
            let current_states = current_states_mut(&mut stack, &mut top_states);
            ensure_state(current_states, &state_id, Some(line_num));
            if let Some(s) = find_state_mut(current_states, &state_id) {
                if s.explicit_source_line.is_none() {
                    s.explicit_source_line = Some(line_num);
                }
                s.description.push(desc);
            }
            continue;
        }

        warn!("line {line_num}: unrecognized state diagram line: {trimmed}");
    }

    // Verify state machine ended cleanly
    match mode {
        ParseMode::Normal => {}
        ParseMode::Note {
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
        ParseMode::StyleBlock {
            start_line,
            start_column,
        } => {
            return Err(crate::Error::Parse {
                line: start_line,
                column: Some(start_column),
                message: "unterminated <style> block (missing `</style>`)".to_string(),
            });
        }
        ParseMode::SkinparamBlock {
            start_line,
            start_column,
        } => {
            return Err(crate::Error::Parse {
                line: start_line,
                column: Some(start_column),
                message: "unterminated skinparam block (missing `}`)".to_string(),
            });
        }
    }

    // Check for unclosed composite states
    if !stack.is_empty() {
        let unclosed: Vec<String> = stack.iter().map(|f| f.state.id.clone()).collect();
        let first = &stack[0];
        return Err(crate::Error::Parse {
            line: first.start_line,
            column: Some(first.start_column),
            message: format!("unclosed composite state(s): {}", unclosed.join(", ")),
        });
    }

    // Java uses a two-pass parser: pass ONE processes `state` keyword
    // declarations (creating Quark children), pass TWO processes transitions
    // (creating link entities like [*]).  This means explicitly declared
    // states always precede auto-created ones in the Quark children order.
    // We emulate this by reordering: explicitly declared states first
    // (preserving relative order), then auto-created states.
    reorder_java_pass_order(&mut top_states);

    debug!(
        "parsed state diagram: {} states, {} transitions, {} notes",
        top_states.len(),
        top_transitions.len(),
        notes.len()
    );

    Ok(StateDiagram {
        states: top_states,
        transitions: top_transitions,
        notes,
        direction,
    })
}

/// Join lines ending with `\` (continuation) into logical lines while keeping
/// the 1-based physical line number of the first contributing line.
fn join_continuation_lines(source: &str) -> Vec<(usize, String)> {
    let mut result = Vec::new();
    let mut continuation = String::new();
    let mut continuation_start = None;

    for (idx, line) in source.lines().enumerate() {
        let line_num = idx + 1;
        if let Some(without_backslash) = line.strip_suffix('\\') {
            // Remove the trailing backslash and append to continuation buffer
            if continuation.is_empty() {
                continuation_start = Some(line_num);
            }
            continuation.push_str(without_backslash);
        } else if !continuation.is_empty() {
            continuation.push_str(line);
            result.push((continuation_start.unwrap_or(line_num), continuation.clone()));
            continuation.clear();
            continuation_start = None;
        } else {
            result.push((line_num, line.to_string()));
        }
    }

    // If there's a dangling continuation, add it anyway
    if !continuation.is_empty() {
        result.push((continuation_start.unwrap_or(1), continuation));
    }

    result
}

/// Parse `note as ALIAS` line. Returns the alias if matched.
fn parse_note_as(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    // Pattern: `note as <ALIAS>`
    let rest = lower.strip_prefix("note ")?.trim_start();
    let rest = rest.strip_prefix("as ")?;
    let alias = rest.trim();
    if alias.is_empty() {
        return None;
    }
    // Return the original-case alias
    let orig_rest = line.strip_prefix("note ")?.trim_start();
    let orig_alias = orig_rest
        .strip_prefix("as ")
        .or_else(|| orig_rest.strip_prefix("As "))
        .or_else(|| orig_rest.strip_prefix("AS "))
        .unwrap_or(orig_rest);
    Some(orig_alias.trim().to_string())
}

/// Parse an inline note with direction: `note right of State : text` or `note left of State : text`
fn parse_note_direction_inline(line: &str) -> Option<(String, String, String)> {
    let (position, target, maybe_text) = parse_note_direction(line)?;
    let text = maybe_text?;
    if text.is_empty() {
        return None;
    }
    Some((position, target, text))
}

fn parse_note_direction(line: &str) -> Option<(String, String, Option<String>)> {
    // ASCII-only folding: the prefix length measured on `lower` indexes
    // `line` below, and `to_lowercase` can change a string's byte length
    // (U+1E9E folds to two-byte `ss`), which would carry that length into the
    // middle of a character.
    let lower = line.to_ascii_lowercase();
    let rest = lower.strip_prefix("note ")?;
    for (position, prefix) in [
        ("right", "right of "),
        ("left", "left of "),
        ("top", "top of "),
        ("bottom", "bottom of "),
    ] {
        if let Some(after_dir) = rest.strip_prefix(prefix) {
            let lower_prefix_len = line.len() - after_dir.len();
            let orig_after = &line[lower_prefix_len..];
            if let Some(colon_pos) = after_dir.find(':') {
                let target = normalize_note_target(orig_after[..colon_pos].trim());
                let text = orig_after[colon_pos + 1..].trim().to_string();
                return Some((position.to_string(), target, Some(text)));
            }
            let target = normalize_note_target(orig_after.trim());
            return Some((position.to_string(), target, None));
        }
    }
    None
}

/// Check if a line starts a multi-line note block: `note right of State` (no colon)
fn parse_note_direction_block_start(line: &str) -> Option<(String, String)> {
    let (position, target, maybe_text) = parse_note_direction(line)?;
    if maybe_text.is_none() {
        return Some((position, target));
    }
    None
}

fn normalize_note_target(target: &str) -> String {
    let trimmed = target.trim();
    if trimmed == "[*]" {
        return "[*]".to_string();
    }
    parse_state_header(trimmed).0
}

/// Try to parse a composite state declaration from the rest after `state `.
/// The `rest` is everything after `state ` keyword.
/// Returns the initial State (with empty children) if it ends with `{`.
fn try_parse_composite_state(rest: &str) -> Option<State> {
    let rest = rest.trim();

    // Must contain `{` to be composite
    let brace_pos = rest.find('{')?;
    let before_brace = rest[..brace_pos].trim();

    // Check nothing significant after `{` (except possible whitespace/closing brace)
    let after_brace = rest[brace_pos + 1..].trim();
    if !after_brace.is_empty() && after_brace != "}" {
        // There's content after `{` on the same line -- not a clean composite opening
        // (We ignore this edge case for now)
        return None;
    }

    // Parse the part before `{` as a state header
    let (id, name, stereotype) = parse_state_header(before_brace);

    let kind = stereotype_to_kind(stereotype.as_deref());

    Some(State {
        name,
        id,
        description: Vec::new(),
        stereotype,
        children: Vec::new(),
        is_special: false,
        kind,
        regions: Vec::new(),
        source_line: None,
        explicit_source_line: None,
    })
}

/// Parse a state declaration (everything after `state `).
/// Handles: `Name`, `"Quoted Name"`, `Name <<stereo>>`, `Name: desc`
fn parse_state_declaration(rest: &str) -> Option<State> {
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }

    // Split off description after `:` (but be careful with quoted names
    // that might contain colons like `"count_val[3:0]"`)
    let (header_part, description) = split_state_description(rest);

    let (id, name, stereotype) = parse_state_header(header_part);

    if id.is_empty() {
        return None;
    }

    let desc_vec = if let Some(d) = description {
        vec![d]
    } else {
        Vec::new()
    };

    let kind = stereotype_to_kind(stereotype.as_deref());

    Some(State {
        name,
        id,
        description: desc_vec,
        stereotype,
        children: Vec::new(),
        is_special: false,
        kind,
        regions: Vec::new(),
        source_line: None,
        explicit_source_line: None,
    })
}

/// Split a state declaration into header + optional description.
/// The description starts after the first `:` that is outside quotes.
fn split_state_description(s: &str) -> (&str, Option<String>) {
    let mut in_quotes = false;
    let mut after_quotes = false;
    for (i, ch) in s.char_indices() {
        match ch {
            '"' => {
                if in_quotes {
                    in_quotes = false;
                    after_quotes = true;
                } else {
                    in_quotes = true;
                }
            }
            ':' if !in_quotes => {
                let header = s[..i].trim();
                let desc = s[i + 1..].trim().to_string();
                return (header, Some(desc));
            }
            _ => {
                // If we've passed the quoted name, check for colon after stereotype etc.
                let _ = after_quotes;
            }
        }
    }
    (s.trim(), None)
}

/// Parse a state header (name and optional stereotype) from text like:
/// - `StateName`
/// - `"Quoted Name"`
/// - `StateName <<stereo>>`
/// - `"Quoted Name" <<stereo>>`
///
/// Returns (id, display_name, stereotype).
fn parse_state_header(s: &str) -> (String, String, Option<String>) {
    let s = s.trim();

    let (raw_name, remainder) = if let Some(after_quote) = s.strip_prefix('"') {
        // Quoted name
        if let Some(end_quote) = after_quote.find('"') {
            let quoted = &after_quote[..end_quote];
            let rest = after_quote[end_quote + 1..].trim();
            (quoted.to_string(), rest)
        } else {
            // No closing quote -- use everything
            let name = s.trim_start_matches('"').to_string();
            (name, "")
        }
    } else {
        // Unquoted: take first whitespace-delimited token
        match s.find(|c: char| c.is_whitespace()) {
            Some(pos) => (s[..pos].to_string(), s[pos..].trim()),
            None => (s.to_string(), ""),
        }
    };

    // Handle "as alias" in the remainder: `"Display Name" as alias <<stereo>>`
    let (alias, after_alias) = if let Some(rest) = remainder.strip_prefix("as ") {
        let rest = rest.trim_start();
        // The alias is the next token (before stereotype or end of string)
        if let Some(stereo_start) = rest.find("<<") {
            let alias = rest[..stereo_start].trim();
            (Some(alias.to_string()), &rest[stereo_start..])
        } else {
            // No stereotype after alias: take everything as alias
            match rest.find(|c: char| c.is_whitespace()) {
                Some(pos) => (Some(rest[..pos].to_string()), rest[pos..].trim()),
                None => (Some(rest.to_string()), ""),
            }
        }
    } else {
        (None, remainder)
    };

    // Parse stereotype from remainder: <<something>>
    let stereotype = parse_stereotype(after_alias);

    // If alias is present, use it as the ID; otherwise generate from name
    let id = if let Some(alias) = alias {
        alias
    } else {
        sanitize_id(&raw_name)
    };
    let display_name = raw_name;

    (id, display_name, stereotype)
}

/// Extract stereotype `<<text>>` from a string
fn parse_stereotype(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(start) = s.find("<<") {
        if let Some(end) = s[start..].find(">>") {
            let stereo = &s[start + 2..start + end];
            return Some(stereo.to_string());
        }
    }
    None
}

/// Map a stereotype string to a `StateKind`.
fn stereotype_to_kind(stereotype: Option<&str>) -> StateKind {
    match stereotype {
        Some("fork") => StateKind::Fork,
        Some("join") => StateKind::Join,
        Some("choice") => StateKind::Choice,
        Some("end") => StateKind::End,
        Some("entryPoint") => StateKind::EntryPoint,
        Some("exitPoint") => StateKind::ExitPoint,
        _ => StateKind::Normal,
    }
}

/// Sanitize a name to produce a valid ID
/// For names like `count_val[3:0]`, produce `count_val[3:0]` (keep as-is).
/// The ID is the raw name stripped of surrounding whitespace.
fn sanitize_id(name: &str) -> String {
    name.trim().to_string()
}

/// Try to parse a transition line.
/// Patterns:
///   - `A --> B : label`
///   - `A -> B : label`
///   - `A --> B`
///   - `[*] --> A`
///   - `A --> [*]`
fn try_parse_transition(line: &str) -> Option<Transition> {
    let line = line.trim();

    // Find arrow pattern: `->` or `-->` (possibly with direction indicators like `-up->`, etc.)
    // We look for sequences of `-` and `>` that form arrows.
    // Strategy: find `->` or `-->` by scanning for the arrow pattern.

    // Try to find an arrow: look for `->`  or `-->`
    // The arrow can be `->`, `-->`, `--->`, etc.
    // We need to split: `from <arrow> to [: label]`

    let (from_part, arrow, rest) = split_arrow(line)?;

    let from = from_part.trim().to_string();
    if from.is_empty() {
        return None;
    }

    // Determine if dashed: `-->` has 2+ dashes, `->` has 1
    let dashes = arrow.chars().filter(|&c| c == '-').count();
    let dashed = dashes >= 2;

    // Split rest into `to` and optional `: label`
    let (to_part, label) = if let Some(colon_pos) = rest.find(':') {
        let to = rest[..colon_pos].trim().to_string();
        let lbl = rest[colon_pos + 1..].trim().to_string();
        (to, lbl)
    } else {
        (rest.trim().to_string(), String::new())
    };

    if to_part.is_empty() {
        return None;
    }

    // Normalize special state `[*]`
    let from_id = normalize_state_ref(&from);
    let to_id = normalize_state_ref(&to_part);

    Some(Transition {
        from: from_id,
        to: to_id,
        label,
        dashed,
        length: dashes,
        source_line: None, // set by caller
    })
}

/// Split a line into (from, arrow, rest) at the arrow position.
/// Arrow patterns: `->`, `-->`, `--->`, etc.
/// We need to handle `[*]` which contains `*` but no arrow chars.
fn split_arrow(line: &str) -> Option<(&str, &str, &str)> {
    // Strategy: scan for patterns like `->` or `-->`.
    // Be careful with `[*]` which shouldn't be confused with arrows.
    // We scan for the first occurrence of `->' that forms a valid arrow.

    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Skip over `[*]` -- don't look for arrows inside it
        if bytes[i] == b'[' {
            if let Some(close) = line[i..].find(']') {
                i += close + 1;
                continue;
            }
        }

        // Look for start of arrow: `-`
        if bytes[i] == b'-' {
            let arrow_start = i;
            // Consume all dashes
            while i < len && bytes[i] == b'-' {
                i += 1;
            }
            // Optionally skip direction words like `up`, `down`, `left`, `right`
            // embedded in arrows: `-up->`, `-down->`, etc.
            let mut j = i;
            if j < len && bytes[j].is_ascii_alphabetic() {
                // Check for direction keyword
                let rest_lower = line[j..].to_lowercase();
                for dir in &["up", "down", "left", "right", "u", "d", "l", "r"] {
                    if rest_lower.starts_with(dir) {
                        let dir_end = j + dir.len();
                        if dir_end < len && bytes[dir_end] == b'-' {
                            j = dir_end;
                            // Consume trailing dashes
                            while j < len && bytes[j] == b'-' {
                                j += 1;
                            }
                            break;
                        }
                    }
                }
            }
            // Now check for `>`
            if j < len && bytes[j] == b'>' {
                j += 1;
                let arrow = &line[arrow_start..j];
                let from = &line[..arrow_start];
                let to_and_rest = &line[j..];
                return Some((from.trim(), arrow, to_and_rest.trim()));
            }
            // Not a valid arrow, continue scanning from after dashes
            i = j.max(i + 1);
            continue;
        }

        i += 1;
    }

    None
}

/// Normalize a state reference: `[*]` stays as `[*]`, others are trimmed.
fn normalize_state_ref(s: &str) -> String {
    let trimmed = s.trim();
    // Strip surrounding quotes if present (e.g., in `sig_ff -> entry2 : "!"`)
    trimmed.to_string()
}

/// Scope and split `[*]` into separate start/end IDs.
/// Java: `[*]` as source → `getStart()` = `*start*` + scope
///       `[*]` as target → `getEnd()` = `*end*` + scope
fn scope_transition_special_state_ids(trans: &mut Transition, stack: &[CompositeFrame]) {
    if trans.from == "[*]" {
        trans.from = scoped_special_state_id(stack, true);
    }
    if trans.to == "[*]" {
        trans.to = scoped_special_state_id(stack, false);
    }
}

fn scoped_special_state_id(stack: &[CompositeFrame], is_start: bool) -> String {
    let suffix = if is_start { "__start" } else { "__end" };
    if stack.is_empty() {
        return format!("[*]{suffix}");
    }
    // Java's StateDiagram.concurrentState wraps each non-first region in an
    // anonymous CONCURRENT_STATE group named CONC<N>. Mirror that by tacking
    // the current frame's current_conc_scope onto the scope so the special
    // state id (e.g. `[*]__startActive.CONC2`) is unique per region.
    let mut scope: Vec<&str> = stack.iter().map(|frame| frame.state.id.as_str()).collect();
    if let Some(frame) = stack.last() {
        if let Some(conc) = frame.current_conc_scope.as_deref() {
            scope.push(conc);
        }
    }
    let scope_str = scope.join(".");
    format!("[*]{suffix}{scope_str}")
}

/// Try to parse a description-addition line: `StateName : text`
/// This is NOT a `state` keyword line, just a bare `ID : description`.
fn try_parse_description_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();

    // Must contain ` : ` (with spaces around colon) to be a description line
    // But also handle `s1 :text` (space before colon, maybe not after)
    // We look for a colon that is preceded by a state identifier

    // Skip lines that could be transitions (contain `->` or `-->`)
    if line.contains("->") || line.contains("-->") {
        return None;
    }

    // Skip lines starting with keywords
    let lower = line.to_lowercase();
    if lower.starts_with("state ")
        || lower.starts_with("note ")
        || lower.starts_with("skinparam")
        || lower.starts_with("hide ")
        || lower.starts_with("show ")
    {
        return None;
    }

    // Find the first `:` that separates ID from description
    let colon_pos = line.find(':')?;
    let id_part = line[..colon_pos].trim();
    let desc_part = line[colon_pos + 1..].trim();

    // The id_part must look like a valid state ID (no spaces, or `[*]`)
    if id_part.is_empty() || desc_part.is_empty() {
        return None;
    }

    // Validate that id_part is a simple identifier (alphanumeric, underscore)
    // or `[*]`
    if id_part == "[*]" {
        return Some(("[*]".to_string(), desc_part.to_string()));
    }

    if id_part.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Some((id_part.to_string(), desc_part.to_string()));
    }

    None
}

/// Get a mutable reference to the current states list (top-level or inside composite)
fn current_states_mut<'a>(
    stack: &'a mut [CompositeFrame],
    top: &'a mut Vec<State>,
) -> &'a mut Vec<State> {
    if let Some(frame) = stack.last_mut() {
        &mut frame.state.children
    } else {
        top
    }
}

/// Get a mutable reference to the current transitions list
fn current_transitions_mut<'a>(
    stack: &'a mut [CompositeFrame],
    top: &'a mut Vec<Transition>,
) -> &'a mut Vec<Transition> {
    if let Some(frame) = stack.last_mut() {
        &mut frame.transitions
    } else {
        top
    }
}

fn find_state_recursive<'a>(states: &'a [State], id: &str) -> Option<&'a State> {
    for state in states {
        if state.id == id {
            return Some(state);
        }
        if let Some(found) = find_state_recursive(&state.children, id) {
            return Some(found);
        }
        for region in &state.regions {
            if let Some(found) = find_state_recursive(region, id) {
                return Some(found);
            }
        }
    }
    None
}

fn find_state_recursive_mut<'a>(states: &'a mut [State], id: &str) -> Option<&'a mut State> {
    for state in states {
        if state.id == id {
            return Some(state);
        }
        if let Some(found) = find_state_recursive_mut(&mut state.children, id) {
            return Some(found);
        }
        for region in &mut state.regions {
            if let Some(found) = find_state_recursive_mut(region, id) {
                return Some(found);
            }
        }
    }
    None
}

fn history_parent_id(id: &str) -> Option<&str> {
    id.strip_suffix("[H*]").or_else(|| id.strip_suffix("[H]"))
}

/// Ensure a state with the given ID exists in the states list.
/// If not found, auto-create it. Handles `[*]` as a special state and attaches
/// history pseudo states to their owning composite when that composite exists.
fn ensure_state(states: &mut Vec<State>, id: &str, source_line: Option<usize>) {
    if id == "[*]" || id.starts_with("[*]") {
        if states.iter().any(|s| s.id == id) {
            return;
        }
        debug!("auto-creating special state {id}");
        states.push(State {
            name: id.to_string(),
            id: id.to_string(),
            description: Vec::new(),
            stereotype: None,
            children: Vec::new(),
            is_special: true,
            kind: StateKind::default(),
            regions: Vec::new(),
            source_line,
            explicit_source_line: None,
        });
        return;
    }

    if find_state_recursive(states, id).is_some() {
        return;
    }

    // Detect history pseudo-state references: "StateName[H]" or "StateName[H*]"
    let kind = if id.ends_with("[H*]") {
        StateKind::DeepHistory
    } else if id.ends_with("[H]") {
        StateKind::History
    } else {
        StateKind::default()
    };

    if matches!(kind, StateKind::History | StateKind::DeepHistory) {
        if let Some(parent_id) = history_parent_id(id) {
            if let Some(parent) = find_state_recursive_mut(states, parent_id) {
                debug!("auto-creating history child '{id}' under '{parent_id}'");
                parent.children.push(State {
                    name: id.to_string(),
                    id: id.to_string(),
                    description: Vec::new(),
                    stereotype: None,
                    children: Vec::new(),
                    is_special: false,
                    kind,
                    source_line,
                    explicit_source_line: None,
                    regions: Vec::new(),
                });
                return;
            }
        }
    }

    debug!("auto-creating state '{id}' (kind={kind:?})");
    states.push(State {
        name: id.to_string(),
        id: id.to_string(),
        description: Vec::new(),
        stereotype: None,
        children: Vec::new(),
        is_special: false,
        kind,
        source_line,
        explicit_source_line: None,
        regions: Vec::new(),
    });
}

/// Find a state by ID in the list (non-recursive, current level only)
#[allow(dead_code)] // immutable counterpart of find_state_mut
fn find_state<'a>(states: &'a [State], id: &str) -> Option<&'a State> {
    states.iter().find(|s| s.id == id)
}

/// Find a mutable state by ID in the list
fn find_state_mut<'a>(states: &'a mut [State], id: &str) -> Option<&'a mut State> {
    states.iter_mut().find(|s| s.id == id)
}

/// Merge a parsed state into the list: if a state with the same ID already exists,
/// merge its fields (add descriptions, update stereotype if missing, etc.).
/// Otherwise, add it to the list.
fn merge_or_add_state(states: &mut Vec<State>, new_state: State) {
    if let Some(existing) = states.iter_mut().find(|s| s.id == new_state.id) {
        let State {
            name,
            id: _,
            description,
            stereotype,
            children,
            is_special,
            kind,
            regions,
            source_line,
            explicit_source_line,
        } = new_state;
        // Merge: add descriptions
        for desc in description {
            existing.description.push(desc);
        }
        // Update display name if the existing one is just the ID
        if existing.name == existing.id && name != existing.id {
            existing.name = name;
        }
        // Set stereotype if not already set
        if existing.stereotype.is_none() && stereotype.is_some() {
            existing.stereotype = stereotype;
        }
        if !is_special {
            existing.is_special = false;
        }
        if matches!(existing.kind, StateKind::Normal) && !matches!(kind, StateKind::Normal) {
            existing.kind = kind;
        }
        for child in children {
            merge_or_add_state(&mut existing.children, child);
        }
        if existing.regions.is_empty() {
            existing.regions = regions;
        } else {
            for (idx, region) in regions.into_iter().enumerate() {
                if let Some(existing_region) = existing.regions.get_mut(idx) {
                    for child in region {
                        merge_or_add_state(existing_region, child);
                    }
                } else {
                    existing.regions.push(region);
                }
            }
        }
        existing.source_line = match (existing.source_line, source_line) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        existing.explicit_source_line = match (existing.explicit_source_line, explicit_source_line)
        {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
    } else {
        states.push(new_state);
    }
}

/// Reorder state children to match Java's two-pass ordering.
///
/// Java processes `state` keyword declarations in pass ONE and transitions
/// in pass TWO.  The Quark LinkedHashMap preserves insertion order, so
/// states declared with `state` keyword appear before states auto-created
/// by transitions.  We emulate this by stable-sorting: explicitly declared
/// states (those with `explicit_source_line.is_some()`) first, then
/// auto-created states, preserving relative order within each group.
/// Also recursively reorder children of composite states.
fn reorder_java_pass_order(states: &mut Vec<State>) {
    // Stable partition: explicit-first, auto-created-second.
    // We use a two-pass collect to preserve relative order.
    let explicit: Vec<State> = states
        .iter()
        .filter(|s| s.explicit_source_line.is_some())
        .cloned()
        .collect();
    let auto: Vec<State> = states
        .iter()
        .filter(|s| s.explicit_source_line.is_none())
        .cloned()
        .collect();
    states.clear();
    states.extend(explicit);
    states.extend(auto);

    // Recursively reorder children of composite states
    for state in states.iter_mut() {
        if !state.children.is_empty() {
            reorder_java_pass_order(&mut state.children);
        }
        for region in &mut state.regions {
            reorder_java_pass_order(region);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Basic state parsing ----

    #[test]
    fn parse_simple_state() {
        let src = "@startuml\nstate s1\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "s1");
        assert_eq!(diagram.states[0].name, "s1");
        assert!(!diagram.states[0].is_special);
    }

    #[test]
    fn parse_two_states() {
        let src = "@startuml\nstate s1\nstate s2\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 2);
        assert_eq!(diagram.states[0].id, "s1");
        assert_eq!(diagram.states[1].id, "s2");
    }

    #[test]
    fn parse_quoted_state_name() {
        let src = "@startuml\nstate \"My State\"\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].name, "My State");
        assert_eq!(diagram.states[0].id, "My State");
    }

    #[test]
    fn parse_state_with_stereotype() {
        let src = "@startuml\nstate entry1 <<inputPin>>\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "entry1");
        assert_eq!(diagram.states[0].stereotype.as_deref(), Some("inputPin"));
    }

    #[test]
    fn parse_state_with_inline_description() {
        let src = "@startuml\nstate count_idle: count_val := 0\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "count_idle");
        assert_eq!(diagram.states[0].description.len(), 1);
        assert_eq!(diagram.states[0].description[0], "count_val := 0");
    }

    #[test]
    fn parse_description_added_to_existing_state() {
        let src = "@startuml\nstate s1\ns1 : line1\ns1 : line2\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "s1");
        assert_eq!(diagram.states[0].description.len(), 2);
        assert_eq!(diagram.states[0].description[0], "line1");
        assert_eq!(diagram.states[0].description[1], "line2");
    }

    #[test]
    fn join_continuation_lines_preserves_physical_line_numbers() {
        let joined = join_continuation_lines("state A\nA : hello\\\nworld\nA -> A\n");
        assert_eq!(
            joined,
            vec![
                (1, "state A".to_string()),
                (2, "A : helloworld".to_string()),
                (4, "A -> A".to_string()),
            ]
        );
    }

    // ---- Transitions ----

    #[test]
    fn parse_simple_transition() {
        let src = "@startuml\nstate s1\nstate s2\ns1 --> s2 : play\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.transitions.len(), 1);
        assert_eq!(diagram.transitions[0].from, "s1");
        assert_eq!(diagram.transitions[0].to, "s2");
        assert_eq!(diagram.transitions[0].label, "play");
        assert!(diagram.transitions[0].dashed);
    }

    #[test]
    fn parse_solid_transition() {
        let src = "@startuml\ns1 -> s2 : go\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.transitions.len(), 1);
        assert_eq!(diagram.transitions[0].from, "s1");
        assert_eq!(diagram.transitions[0].to, "s2");
        assert_eq!(diagram.transitions[0].label, "go");
        assert!(!diagram.transitions[0].dashed);
    }

    #[test]
    fn parse_transition_without_label() {
        let src = "@startuml\ns1 --> s2\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.transitions.len(), 1);
        assert_eq!(diagram.transitions[0].from, "s1");
        assert_eq!(diagram.transitions[0].to, "s2");
        assert!(diagram.transitions[0].label.is_empty());
    }

    #[test]
    fn parse_special_state_transitions() {
        let src = "@startuml\n[*] --> s1\ns1 --> [*]\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.transitions.len(), 2);
        // Java: [*] as source → getStart() = "*start*", as target → getEnd() = "*end*"
        assert_eq!(diagram.transitions[0].from, "[*]__start");
        assert_eq!(diagram.transitions[0].to, "s1");
        assert_eq!(diagram.transitions[1].from, "s1");
        assert_eq!(diagram.transitions[1].to, "[*]__end");
        // Both should be auto-created as special
        let start = diagram
            .states
            .iter()
            .find(|s| s.id == "[*]__start")
            .unwrap();
        assert!(start.is_special);
        let end = diagram.states.iter().find(|s| s.id == "[*]__end").unwrap();
        assert!(end.is_special);
    }

    #[test]
    fn parse_transition_auto_creates_states() {
        let src = "@startuml\nA --> B : go\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 2);
        assert!(diagram.states.iter().any(|s| s.id == "A"));
        assert!(diagram.states.iter().any(|s| s.id == "B"));
    }

    // ---- Composite states ----

    #[test]
    fn parse_simple_composite() {
        let src = "@startuml\nstate parent {\n  state child1\n  state child2\n}\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "parent");
        assert_eq!(diagram.states[0].children.len(), 2);
        assert_eq!(diagram.states[0].children[0].id, "child1");
        assert_eq!(diagram.states[0].children[1].id, "child2");
    }

    #[test]
    fn parse_composite_with_transitions() {
        let src = "@startuml\nstate parent {\n  [*] -> child1\n  child1 -> child2\n}\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.transitions.len(), 2);
        assert_eq!(diagram.transitions[0].from, "[*]__startparent");
        assert_eq!(diagram.transitions[0].to, "child1");
    }

    #[test]
    fn parse_nested_composite() {
        let src = "@startuml\nstate outer {\n  state inner {\n    state deep\n  }\n}\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "outer");
        assert_eq!(diagram.states[0].children.len(), 1);
        assert_eq!(diagram.states[0].children[0].id, "inner");
        assert_eq!(diagram.states[0].children[0].children.len(), 1);
        assert_eq!(diagram.states[0].children[0].children[0].id, "deep");
    }

    // ---- Notes ----

    #[test]
    fn parse_note_with_alias() {
        let src = "@startuml\nnote as PARAMS\nfoo bar\nbaz\nend note\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.notes.len(), 1);
        assert_eq!(diagram.notes[0].alias.as_deref(), Some("PARAMS"));
        assert_eq!(diagram.notes[0].text, "foo bar\nbaz");
    }

    // ---- Skip directives ----

    #[test]
    fn skip_skinparam_single_line() {
        let src = "@startuml\nskinparam tabSize 2\nstate s1\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "s1");
    }

    #[test]
    fn skip_skinparam_block() {
        let src = "@startuml\nskinparam {\nfoo bar\n}\nstate s1\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
    }

    #[test]
    fn skip_style_block() {
        let src = "@startuml\n<style>\nfoo { bar }\n</style>\nstate s1\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
    }

    #[test]
    fn skip_comments() {
        let src = "@startuml\n' this is a comment\nstate s1\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
    }

    #[test]
    fn skip_hide_show() {
        let src = "@startuml\nhide empty description\nshow footbox\nstate s1\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
    }

    // ---- Line continuation ----

    #[test]
    fn parse_line_continuation() {
        let src = "@startuml\nstate s1\ns1 : line1\\\ncontinued\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].description.len(), 1);
        assert_eq!(diagram.states[0].description[0], "line1continued");
    }

    #[test]
    fn parse_transition_after_continuation_uses_physical_line_number() {
        let src = "@startuml\nstate A\nA : adding bold:\\n\\\nsecond line\nA -> A\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.transitions.len(), 1);
        assert_eq!(diagram.transitions[0].source_line, Some(4));
    }

    // ---- Error cases ----

    #[test]
    fn unterminated_note_returns_error() {
        let src = "@startuml\nnote as FOO\nsome text\n@enduml";
        let result = parse_state_diagram(src);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("unterminated note"));
        assert!(err_msg.contains("line 1:1"));
    }

    #[test]
    fn unclosed_composite_returns_error() {
        let src = "@startuml\nstate parent {\nstate child\n@enduml";
        let result = parse_state_diagram(src);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("unclosed composite"));
        assert!(err_msg.contains("line 1:1"));
    }

    #[test]
    fn parse_empty_diagram() {
        let src = "@startuml\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert!(diagram.states.is_empty());
        assert!(diagram.transitions.is_empty());
        assert!(diagram.notes.is_empty());
    }

    // ---- Quoted state with colons in name ----

    #[test]
    fn parse_quoted_state_with_colon() {
        let src = "@startuml\nstate \"count_val[3:0]\"\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].name, "count_val[3:0]");
    }

    #[test]
    fn parse_quoted_state_with_stereotype() {
        let src = "@startuml\nstate \"count_val[3:0]\" <<outputPin>>\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].name, "count_val[3:0]");
        assert_eq!(diagram.states[0].stereotype.as_deref(), Some("outputPin"));
    }

    // ---- Fixture file tests ----

    #[test]
    fn parse_fixture_scxml0001() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/scxml0001.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        // States: s1, s2, [*]__start
        assert_eq!(diagram.states.len(), 3);
        assert!(diagram.states.iter().any(|s| s.id == "s1"));
        assert!(diagram.states.iter().any(|s| s.id == "s2"));
        assert!(diagram
            .states
            .iter()
            .any(|s| s.id == "[*]__start" && s.is_special));

        // Transitions: [*]__start --> s1, s1 --> s2 : play
        assert_eq!(diagram.transitions.len(), 2);
        assert_eq!(diagram.transitions[0].from, "[*]__start");
        assert_eq!(diagram.transitions[0].to, "s1");
        assert_eq!(diagram.transitions[1].from, "s1");
        assert_eq!(diagram.transitions[1].to, "s2");
        assert_eq!(diagram.transitions[1].label, "play");
    }

    #[test]
    fn parse_fixture_scxml0002() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/scxml0002.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        // One top-level composite state: counter
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "counter");

        // Children inside counter
        let counter = &diagram.states[0];
        assert!(
            counter.children.len() >= 5,
            "expected at least 5 children, got {}",
            counter.children.len()
        );

        // Check that count_idle has a description
        let idle = counter
            .children
            .iter()
            .find(|s| s.id == "count_idle")
            .unwrap();
        assert!(!idle.description.is_empty());
        assert_eq!(idle.description[0], "count_val := 0");

        // Check that "count_val[3:0]" state exists (quoted name)
        assert!(counter.children.iter().any(|s| s.name == "count_val[3:0]"));

        // Transitions should be present
        assert!(diagram.transitions.len() >= 4);
    }

    #[test]
    fn parse_fixture_scxml0003() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/scxml0003.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        // One top-level composite: module
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "module");

        let module = &diagram.states[0];
        // module should have children: Somp, flop, counter, ex, exitAx
        // plus auto-created states from transitions
        assert!(
            module.children.len() >= 5,
            "expected >= 5 children in module, got {}",
            module.children.len()
        );

        // Somp is a composite with children
        let somp = module.children.iter().find(|s| s.id == "Somp").unwrap();
        assert!(!somp.children.is_empty(), "Somp should have children");

        // flop is a composite with children
        let flop = module.children.iter().find(|s| s.id == "flop").unwrap();
        assert!(!flop.children.is_empty(), "flop should have children");

        // counter is a composite with children
        let counter_state = module.children.iter().find(|s| s.id == "counter").unwrap();
        assert!(
            !counter_state.children.is_empty(),
            "counter should have children"
        );

        // Check stereotypes
        let ex = module.children.iter().find(|s| s.id == "ex").unwrap();
        assert_eq!(ex.stereotype.as_deref(), Some("inputPin"));
    }

    #[test]
    fn parse_fixture_scxml0004() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/scxml0004.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        // One top-level: module
        assert_eq!(diagram.states.len(), 1);
        let module = &diagram.states[0];
        assert_eq!(module.id, "module");

        // module has children: flop, Somp
        assert!(module.children.iter().any(|s| s.id == "flop"));
        assert!(module.children.iter().any(|s| s.id == "Somp"));

        // Somp is a composite with children: entry1, entry2, sin, sin2 (auto-created)
        let somp = module.children.iter().find(|s| s.id == "Somp").unwrap();
        assert!(!somp.children.is_empty());
        assert!(somp.children.iter().any(|s| s.id == "entry1"));
        assert!(somp.children.iter().any(|s| s.id == "entry2"));
    }

    #[test]
    fn parse_fixture_scxml0005() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/scxml0005.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        // One top-level composite: module (empty)
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "module");

        // One note with alias PARAMETERS
        assert_eq!(diagram.notes.len(), 1);
        assert_eq!(diagram.notes[0].alias.as_deref(), Some("PARAMETERS"));
        assert!(diagram.notes[0].text.contains("localparam MAX_VAL 10"));
        assert!(diagram.notes[0].text.contains("parameter COUNT_WIDTH 4"));
    }

    #[test]
    fn parse_fixture_state_monoline_01() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/state_monoline_01.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        // One state: s1 with 3 description lines
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "s1");
        assert_eq!(diagram.states[0].description.len(), 3);
        assert_eq!(diagram.states[0].description[0], "line1");
        assert_eq!(diagram.states[0].description[1], r"\tline2");
        assert_eq!(diagram.states[0].description[2], r"\t\tline3");
    }

    #[test]
    fn parse_fixture_state_monoline_02() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/state_monoline_02.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        // One state: s1 with 3 description lines
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "s1");
        assert_eq!(diagram.states[0].description.len(), 3);
        assert_eq!(diagram.states[0].description[0], "line1");
        assert_eq!(diagram.states[0].description[1], r"\nline2");
        assert_eq!(diagram.states[0].description[2], r"\n\nline3");
    }

    #[test]
    fn parse_fixture_state_monoline_03() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/state_monoline_03.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        // States: [*]__start, [*]__end, State1, State2
        assert!(diagram.states.iter().any(|s| s.id == "[*]__start"));
        assert!(diagram.states.iter().any(|s| s.id == "[*]__end"));
        assert!(diagram.states.iter().any(|s| s.id == "State1"));
        assert!(diagram.states.iter().any(|s| s.id == "State2"));

        // State1 has description lines (including the continuation line)
        let state1 = diagram.states.iter().find(|s| s.id == "State1").unwrap();
        assert!(
            state1.description.len() >= 2,
            "State1 should have at least 2 description lines, got {}",
            state1.description.len()
        );
        assert_eq!(state1.description[0], "this is a string");

        // Transitions: [*]__start --> State1, State1 --> [*]__end, State1 -> State2, State2 --> [*]__end
        assert_eq!(diagram.transitions.len(), 4);
    }

    // ---- Additional edge case tests ----

    #[test]
    fn parse_composite_no_space_before_brace() {
        // Like scxml0002: `state counter{` (no space before brace)
        let src = "@startuml\nstate counter{\nstate inner\n}\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "counter");
        assert_eq!(diagram.states[0].children.len(), 1);
    }

    #[test]
    fn parse_state_description_colon_in_value() {
        // `state count_finish: count_done:=1` -- colon inside description
        let src = "@startuml\nstate count_finish: count_done:=1\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "count_finish");
        assert_eq!(diagram.states[0].description[0], "count_done:=1");
    }

    #[test]
    fn parse_transition_no_space_before_colon() {
        // `count_idle --> count_ongoing: count_start` (no space before colon)
        let src = "@startuml\ncount_idle --> count_ongoing: count_start\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.transitions.len(), 1);
        assert_eq!(diagram.transitions[0].from, "count_idle");
        assert_eq!(diagram.transitions[0].to, "count_ongoing");
        assert_eq!(diagram.transitions[0].label, "count_start");
    }

    #[test]
    fn parse_direction_left_to_right() {
        let src = "@startuml\nleft to right direction\nstate A\nstate B\nA --> B\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.direction, crate::model::Direction::LeftToRight);
    }

    #[test]
    fn parse_description_creates_state_if_missing() {
        // State not declared with `state` keyword, only referenced via `id : desc`
        let src = "@startuml\nState1 : description text\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "State1");
        assert_eq!(diagram.states[0].description[0], "description text");
    }

    // ---- Pseudo-state tests ----

    #[test]
    fn parse_fork_state() {
        let src = "@startuml\nstate fork_state <<fork>>\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].id, "fork_state");
        assert_eq!(diagram.states[0].kind, StateKind::Fork);
        assert_eq!(diagram.states[0].stereotype.as_deref(), Some("fork"));
    }

    #[test]
    fn parse_join_state() {
        let src = "@startuml\nstate join_state <<join>>\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].kind, StateKind::Join);
    }

    #[test]
    fn parse_choice_state() {
        let src = "@startuml\nstate choice1 <<choice>>\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].kind, StateKind::Choice);
    }

    #[test]
    fn parse_end_state() {
        let src = "@startuml\nstate end1 <<end>>\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].kind, StateKind::End);
    }

    #[test]
    fn parse_entry_point_state() {
        let src = "@startuml\nstate ep1 <<entryPoint>>\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].kind, StateKind::EntryPoint);
    }

    #[test]
    fn parse_exit_point_state() {
        let src = "@startuml\nstate xp1 <<exitPoint>>\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        assert_eq!(diagram.states[0].kind, StateKind::ExitPoint);
    }

    #[test]
    fn parse_history_transition() {
        let src = "@startuml\nPaused --> Active[H]\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.transitions.len(), 1);
        assert_eq!(diagram.transitions[0].to, "Active[H]");
        let hist_state = diagram.states.iter().find(|s| s.id == "Active[H]").unwrap();
        assert_eq!(hist_state.kind, StateKind::History);
    }

    #[test]
    fn parse_deep_history_transition() {
        let src = "@startuml\nPaused --> Active[H*]\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.transitions.len(), 1);
        assert_eq!(diagram.transitions[0].to, "Active[H*]");
        let hist_state = diagram
            .states
            .iter()
            .find(|s| s.id == "Active[H*]")
            .unwrap();
        assert_eq!(hist_state.kind, StateKind::DeepHistory);
    }

    #[test]
    fn parse_concurrent_regions() {
        let src = "@startuml\nstate Active {\n  state Sub1\n  --\n  state Sub3\n}\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.states.len(), 1);
        let active = &diagram.states[0];
        // After `--`, first region is moved to regions[0], children has second region
        assert_eq!(active.regions.len(), 1, "should have 1 additional region");
        assert!(
            active.regions[0].iter().any(|s| s.id == "Sub1"),
            "first region should contain Sub1"
        );
        assert!(
            active.children.iter().any(|s| s.id == "Sub3"),
            "children (last region) should contain Sub3"
        );
    }

    #[test]
    fn parse_guard_condition_on_transition() {
        let src = "@startuml\nchoice1 --> State2 : [condition A]\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.transitions.len(), 1);
        assert_eq!(diagram.transitions[0].label, "[condition A]");
    }

    #[test]
    fn parse_note_right_of_inline() {
        let src = "@startuml\nstate A\nnote right of A : This is active\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.notes.len(), 1);
        assert_eq!(diagram.notes[0].text, "This is active");
        assert_eq!(diagram.notes[0].position, "right");
        assert_eq!(diagram.notes[0].target.as_deref(), Some("A"));
        assert_eq!(diagram.notes[0].entity_id.as_deref(), Some("GMN2"));
    }

    #[test]
    fn parse_note_left_of_multiline() {
        let src =
            "@startuml\nstate B\nnote left of B\n  Multi line\n  note text\nend note\n@enduml";
        let diagram = parse_state_diagram(src).unwrap();
        assert_eq!(diagram.notes.len(), 1);
        assert_eq!(diagram.notes[0].text, "Multi line\nnote text");
        assert_eq!(diagram.notes[0].position, "left");
        assert_eq!(diagram.notes[0].target.as_deref(), Some("B"));
        assert_eq!(diagram.notes[0].entity_id.as_deref(), Some("GMN2"));
        assert_eq!(diagram.notes[0].source_line, Some(3));
    }

    // ---- Fixture file tests for pseudo-states ----

    #[test]
    fn parse_fixture_state_fork001() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/state_fork001.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        // Should have fork and join states with correct kinds
        let fork = diagram
            .states
            .iter()
            .find(|s| s.id == "fork_state")
            .unwrap();
        assert_eq!(fork.kind, StateKind::Fork);

        let join = diagram
            .states
            .iter()
            .find(|s| s.id == "join_state")
            .unwrap();
        assert_eq!(join.kind, StateKind::Join);

        // Should have transitions from fork to State1/State2
        assert!(diagram.transitions.len() >= 4);
    }

    #[test]
    fn parse_fixture_state_choice001() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/state_choice001.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        let choice = diagram.states.iter().find(|s| s.id == "choice1").unwrap();
        assert_eq!(choice.kind, StateKind::Choice);

        // Guard conditions in labels
        let guarded = diagram
            .transitions
            .iter()
            .filter(|t| t.label.contains('['))
            .count();
        assert!(guarded >= 2, "expected at least 2 guarded transitions");
    }

    #[test]
    fn parse_fixture_state_history001() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/state_history001.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        // Should have a history transition target
        let hist_tr = diagram
            .transitions
            .iter()
            .find(|t| t.to.contains("[H]"))
            .expect("should have a transition to Active[H]");
        assert_eq!(hist_tr.to, "Active[H]");

        let active = diagram
            .states
            .iter()
            .find(|s| s.id == "Active")
            .expect("should have Active state");
        let hist_state = active
            .children
            .iter()
            .find(|s| s.id == "Active[H]")
            .expect("should have auto-created Active[H] child");
        assert_eq!(hist_state.kind, StateKind::History);
    }

    #[test]
    fn parse_fixture_state_concurrent001() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/state_concurrent001.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        // Active state should have concurrent regions.
        // Note: the auto-created simple "Active" from `[*] --> Active` comes first,
        // so find the composite one (the one with children or regions).
        let active = diagram
            .states
            .iter()
            .filter(|s| s.id == "Active")
            .find(|s| !s.regions.is_empty() || !s.children.is_empty())
            .expect("should have composite Active state with regions");
        assert!(
            !active.regions.is_empty(),
            "Active should have concurrent regions"
        );
    }

    #[test]
    fn parse_fixture_state_note001() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/state/state_note001.puml"
        ))
        .unwrap();
        let diagram = parse_state_diagram(&src).unwrap();

        // Should have 2 notes: one inline, one multi-line
        assert_eq!(
            diagram.notes.len(),
            2,
            "expected 2 notes, got {}",
            diagram.notes.len()
        );
        assert_eq!(diagram.notes[0].text, "This is active");
        assert_eq!(diagram.notes[1].text, "Multi line\nnote text");
        assert_eq!(diagram.notes[0].source_line, Some(6));
        assert_eq!(diagram.notes[1].source_line, Some(8));
        assert_eq!(diagram.transitions[2].from, "[*]__startActive");
    }
}
