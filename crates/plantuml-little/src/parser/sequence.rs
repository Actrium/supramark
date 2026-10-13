use log::{debug, warn};
use regex::Regex;

use crate::model::sequence::FragmentKind;
use crate::model::{
    Message, Participant, ParticipantKind, SeqArrowHead, SeqArrowStyle, SeqDirection, SeqEvent,
    SequenceDiagram,
};
use crate::Result;

/// Parse sequence diagram source text into SequenceDiagram IR.
/// `original_source` is the raw .puml text before preprocessing, used to
/// compute data-source-line attributes that reference original line numbers.
pub fn parse_sequence_diagram(source: &str) -> Result<SequenceDiagram> {
    parse_sequence_diagram_with_original(source, None)
}

pub fn parse_sequence_diagram_with_original(
    source: &str,
    original_source: Option<&str>,
) -> Result<SequenceDiagram> {
    let block = super::common::extract_block(source).unwrap_or_else(|| source.to_string());

    // Java data-source-line uses 0-based absolute line numbers from the
    // ORIGINAL source file (before preprocessing).  Build a mapping from
    // cleaned-block line index to original source line number.
    let line_mapping: Vec<usize> = build_line_mapping(source, original_source, &block);

    let mut declared_participants: Vec<Participant> = Vec::new();
    let mut auto_participants: Vec<Participant> = Vec::new();
    let mut events: Vec<SeqEvent> = Vec::new();
    let mut inline_life_events: Vec<usize> = Vec::new();
    let mut last_to_participant: Option<String> = None;
    let mut last_from_participant: Option<String> = None;
    let mut in_style_block = false;
    let mut in_skinparam_block = false;
    // Multiline note collection
    let mut in_note_block = false;
    let mut note_kind: Option<&str> = None; // "right", "left", "over"
    let mut note_participant: Option<String> = None;
    let mut note_participants: Vec<String> = Vec::new();
    let mut note_lines: Vec<String> = Vec::new();
    let mut note_is_parallel = false;
    let mut note_color: Option<String> = None;
    // Multiline note: `true` when the note has no explicit `of PARTICIPANT`
    // clause (a *message* note attached to the last arrow, like Java
    // `Note(null)`). `false` when `of X` was written (a *participant* note
    // that occupies its own slot, like Java `Note(p)`).
    let mut note_on_message = false;
    // Track fragment nesting depth so "end" emits FragmentEnd when inside fragments
    let mut fragment_depth: usize = 0;
    // Whether `!pragma teoz true` was encountered
    let mut teoz_mode = false;
    // Whether `hide footbox` was encountered
    let mut hide_footbox = false;
    // Shadow offset from skin directive (e.g. `skin rose` -> 4.0)
    let mut delta_shadow: f64 = 0.0;

    let participant_re = Regex::new(
        r"(?i)^(participant|actor|boundary|control|entity|database|collections|queue)\s+(.+)$",
    )
    .unwrap();

    // Arrow regex: full PlantUML arrow syntax including half-arrows and decorators.
    // The bracket group matches both color [#...] and style [hidden] annotations.
    let arrow_re = Regex::new(
        r"^(.+?)\s+([oxX]*(?:<<?)?(?:[/\\]{1,2})?(?:\[#[^\]]+\]|\[(?i:hidden)\])?-+(?:\[#[^\]]+\]|\[(?i:hidden)\])?-*(?:[/\\]{1,2})?(?:>?>?)?[oxX]*)\s+(.+?)(?:\s*:\s*(.*))?$",
    )
    .unwrap();
    let arrow_nospace_re = Regex::new(
        r"^([A-Za-z_]\w*)\s*([oxX]*(?:<<?)?(?:[/\\]{1,2})?(?:\[#[^\]]+\]|\[(?i:hidden)\])?-+(?:\[#[^\]]+\]|\[(?i:hidden)\])?-*(?:[/\\]{1,2})?(?:>?>?)?[oxX]*)\s*([A-Za-z_]\w*)(?:\s*:\s*(.*))?$",
    )
    .unwrap();
    // Boundary arrow from left: [-> or [<-> participant
    let boundary_left_re = Regex::new(
        r"^\[([oxX]*(?:<<?)?(?:[/\\]{1,2})?(?:\[#[^\]]+\]|\[(?i:hidden)\])?-+(?:\[#[^\]]+\]|\[(?i:hidden)\])?-*(?:[/\\]{1,2})?(?:>?>?)?[oxX]*)\s+(.+?)(?:\s*:\s*(.*))?$",
    )
    .unwrap();
    // Boundary arrow to right: participant ->]
    let boundary_right_re = Regex::new(
        r"^(.+?)\s+([oxX]*(?:<<?)?(?:[/\\]{1,2})?(?:\[#[^\]]+\]|\[(?i:hidden)\])?-+(?:\[#[^\]]+\]|\[(?i:hidden)\])?-*(?:[/\\]{1,2})?(?:>?>?)?[oxX]*\])\s*(?:\s*:\s*(.*))?$",
    )
    .unwrap();

    // Gate/found message: ?->X or X->?  (? is a gate/lost/found participant)
    // Arrow patterns include [#color] annotations to match the regular arrow regex.
    let gate_left_re = Regex::new(
        r"^\?([ox]*(?:[/\\]{1,2})?(?:\[#[^\]]+\])?-+(?:\[#[^\]]+\])?-*(?:[/\\]{1,2})?(?:>?>?)?[ox]*)(.+?)(?:\s*:\s*(.*))?$",
    )
    .unwrap();
    let gate_right_re = Regex::new(
        r"^(.+?)\s*([oxX]*(?:<<?)?(?:[/\\]{1,2})?(?:\[#[^\]]+\]|\[(?i:hidden)\])?-+(?:\[#[^\]]+\]|\[(?i:hidden)\])?-*(?:[/\\]{1,2})?(?:>?>?)?[oxX]*)\?\s*(?::\s*(.*))?$",
    )
    .unwrap();

    let divider_re = Regex::new(r"^==\s*(.*?)\s*==$").unwrap();
    let delay_re = Regex::new(r"^\|\|\|$|^\|\|(\d+)\|\|$").unwrap();
    // Delay with text: ...text... or just ...
    let delay_text_re = Regex::new(r"^\.\.\.(.*)?\.\.\.$|^\.\.\.$").unwrap();
    // Spacing: || N || (with mandatory space around number)
    let spacing_re = Regex::new(r"^\|\|\s+(\d+)\s+\|\|$").unwrap();
    // Ref over: ref over A, B : label
    let ref_re = Regex::new(r"(?i)^ref\s+over\s+(.+?)\s*:\s*(.+)$").unwrap();
    // Autonumber: autonumber or autonumber N
    let autonumber_re = Regex::new(r"(?i)^autonumber(?:\s+(\d+))?$").unwrap();

    for (block_line_idx, line) in block.lines().enumerate() {
        let source_line = line_mapping
            .get(block_line_idx)
            .copied()
            .unwrap_or(block_line_idx);
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Skip comments (lines starting with ')
        if trimmed.starts_with('\'') {
            continue;
        }

        // Handle <style>...</style> blocks
        if trimmed.to_lowercase().starts_with("<style>") {
            in_style_block = true;
            debug!("entering <style> block");
            continue;
        }
        if in_style_block {
            if trimmed.to_lowercase().starts_with("</style>") {
                in_style_block = false;
                debug!("leaving <style> block");
            }
            continue;
        }

        // Handle skinparam blocks
        if trimmed.to_lowercase().starts_with("skinparam") {
            if trimmed.contains('{') {
                in_skinparam_block = true;
                debug!("entering skinparam block");
            }
            // Single-line skinparam is also skipped
            continue;
        }
        if in_skinparam_block {
            if trimmed.contains('}') {
                in_skinparam_block = false;
                debug!("leaving skinparam block");
            }
            continue;
        }

        // Collect multiline note content
        if in_note_block {
            if trimmed.eq_ignore_ascii_case("end note") || trimmed.eq_ignore_ascii_case("endnote") {
                let text = note_lines.join("\n");
                let evt = match note_kind {
                    Some("right") => SeqEvent::NoteRight {
                        participant: note_participant.take().unwrap_or_default(),
                        text,
                        parallel: note_is_parallel,
                        color: note_color.take(),
                        on_message: note_on_message,
                    },
                    Some("left") => SeqEvent::NoteLeft {
                        participant: note_participant.take().unwrap_or_default(),
                        text,
                        parallel: note_is_parallel,
                        color: note_color.take(),
                        on_message: note_on_message,
                    },
                    _ => SeqEvent::NoteOver {
                        participants: std::mem::take(&mut note_participants),
                        text,
                        parallel: note_is_parallel,
                    },
                };
                debug!("parsed multiline note event");
                events.push(evt);
                in_note_block = false;
                note_kind = None;
                note_lines.clear();
                note_is_parallel = false;
                note_on_message = false;
            } else {
                // Java BodyEnhanced2 preserves trailing whitespace inside
                // multi-line notes (a sprite atom followed by " text  " has
                // measurable trailing spaces). Strip only the leading
                // indentation, never the trailing run.
                let preserved = line.trim_start().trim_end_matches(['\r']).to_string();
                note_lines.push(preserved);
            }
            continue;
        }

        // Skip title, legend, footer, header, caption, hide, show, !pragma
        {
            let lower = trimmed.to_lowercase();
            if let Some(stripped) = lower.strip_prefix("!pragma") {
                // Capture `!pragma teoz true`
                let rest = stripped.trim();
                if rest == "teoz true" {
                    debug!("pragma teoz true enabled");
                    teoz_mode = true;
                } else {
                    debug!("skipping pragma: {trimmed}");
                }
                continue;
            }
            if lower.starts_with("title ")
                || lower == "title"
                || lower.starts_with("legend")
                || lower.starts_with("footer")
                || lower.starts_with("header")
                || lower.starts_with("caption")
                || (lower.starts_with("hide ") && !lower.contains("footbox"))
                || lower.starts_with("show ")
            {
                debug!("skipping directive: {trimmed}");
                continue;
            }
            if lower == "hide footbox" {
                hide_footbox = true;
                debug!("hide footbox enabled");
                continue;
            }
            if let Some(stripped) = lower.strip_prefix("skin ") {
                let skin_name = stripped.trim();
                if skin_name == "rose" {
                    delta_shadow = 4.0;
                    debug!("skin rose: delta_shadow=4");
                } else {
                    debug!("skipping skin directive: {trimmed}");
                }
                continue;
            }
        }

        // Parse divider: == text ==
        if let Some(caps) = divider_re.captures(trimmed) {
            let text = caps.get(1).map(|m| m.as_str().trim().to_string());
            let text = text.filter(|t| !t.is_empty());
            debug!("parsed divider: {text:?}");
            events.push(SeqEvent::Divider { text });
            continue;
        }

        // Parse spacing: || N || (must check before delay_re since ||| overlaps)
        if let Some(_caps) = spacing_re.captures(trimmed) {
            return Err(crate::Error::JavaErrorPage {
                line: source_line + 1,
                message: "Syntax Error? (Assumed diagram type: sequence)".into(),
            });
        }

        // Parse delay: ||| or ||N|| (legacy, N treated as spacing)
        if let Some(caps) = delay_re.captures(trimmed) {
            let text = caps.get(1).map(|m| m.as_str().to_string());
            if let Some(digits) = text.as_deref() {
                if digits.chars().all(|c| c.is_ascii_digit()) {
                    let pixels = digits.parse().unwrap_or(20);
                    debug!("parsed legacy spacing: {pixels} px");
                    events.push(SeqEvent::Spacing { pixels });
                    continue;
                }
            }
            debug!("parsed delay: {text:?}");
            events.push(SeqEvent::Delay { text });
            continue;
        }

        // Parse delay with text: ...text... or ...
        if let Some(caps) = delay_text_re.captures(trimmed) {
            let text = caps
                .get(1)
                .map(|m| m.as_str().trim().to_string())
                .filter(|t| !t.is_empty());
            debug!("parsed delay text: {text:?}");
            events.push(SeqEvent::Delay { text });
            continue;
        }

        // Parse autonumber
        if let Some(caps) = autonumber_re.captures(trimmed) {
            let start = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
            debug!("parsed autonumber: start={start:?}");
            events.push(SeqEvent::AutoNumber { start });
            continue;
        }

        // Parse ref over
        if let Some(caps) = ref_re.captures(trimmed) {
            let participants_str = caps.get(1).unwrap().as_str();
            let label = caps.get(2).unwrap().as_str().trim().to_string();
            let participants: Vec<String> = participants_str
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            debug!("parsed ref over {participants:?} : {label}");
            events.push(SeqEvent::Ref {
                participants,
                label,
            });
            continue;
        }

        // Parse activate/deactivate/destroy
        {
            let lower = trimmed.to_lowercase();
            if lower.starts_with("activate ") {
                let mut name = trimmed[9..].trim().to_string();
                // Extract #color suffix (e.g. "activate a #green" → name="a", color="#008000")
                let mut act_color = None;
                if let Some(hash_pos) = name.rfind(" #") {
                    let color_raw = name[hash_pos + 1..].trim();
                    act_color = crate::klimt::color::resolve_color(color_raw).map(|c| c.to_svg());
                    name = name[..hash_pos].trim().to_string();
                } else if let Some(hash_pos) = name.rfind('#') {
                    // No space: "activate a#green"
                    let color_raw = name[hash_pos..].trim();
                    act_color = crate::klimt::color::resolve_color(color_raw).map(|c| c.to_svg());
                    name = name[..hash_pos].trim().to_string();
                }
                debug!("parsed activate: {name} color={act_color:?}");
                ensure_participant(
                    &mut declared_participants,
                    &mut auto_participants,
                    &name,
                    source_line,
                );
                events.push(SeqEvent::Activate(name, act_color));
                continue;
            }
            if lower.starts_with("deactivate ") {
                let name = trimmed[11..].trim().to_string();
                debug!("parsed deactivate: {name}");
                events.push(SeqEvent::Deactivate(name));
                continue;
            }
            if lower.starts_with("destroy ") {
                // `destroy [<kind>] <name>` — the optional participant kind
                // keyword (participant/actor/...) is accepted but ignored; only
                // the name matters for matching the lifeline to terminate.
                let name = strip_participant_keyword(trimmed[8..].trim()).1.to_string();
                debug!("parsed destroy: {name}");
                events.push(SeqEvent::Destroy(name));
                continue;
            }
            // `create [<kind>] <name>` — marks the participant as created by
            // the next message that targets it. Java: SequenceDiagram.pendingCreate.
            // The optional kind keyword (participant/actor/boundary/control/entity/
            // database/collections/queue) selects the participant shape; `<name>`
            // follows the same `"Display" as Name` / `Name as "Display"` / `#color`
            // rules as a regular participant declaration.
            if lower.starts_with("create ") {
                let (kind, details_rest) = strip_participant_keyword(trimmed[7..].trim());
                let (name, display_name, color, link_url) = parse_participant_details(details_rest);
                if !name.is_empty() {
                    debug!("parsed create: name={name}, display={display_name:?}, kind={kind:?}");
                    ensure_create_participant(
                        &mut declared_participants,
                        &mut auto_participants,
                        &name,
                        display_name,
                        kind,
                        color,
                        link_url,
                        source_line,
                    );
                    events.push(SeqEvent::Create(name));
                    continue;
                }
            }
        }

        // Parse note right/left/over (single-line or start multiline block)
        // Also handle teoz parallel note prefix "& note ..."
        {
            let is_parallel_note = trimmed.starts_with("& ");
            let note_trimmed = if is_parallel_note {
                trimmed[2..].trim()
            } else {
                trimmed
            };
            let lower = note_trimmed.to_lowercase();
            if lower.starts_with("note ") {
                match parse_note(note_trimmed, &last_to_participant, &last_from_participant) {
                    Some(mut evt) => {
                        // Propagate parallel flag to the event
                        match &mut evt {
                            SeqEvent::NoteRight { parallel, .. }
                            | SeqEvent::NoteLeft { parallel, .. }
                            | SeqEvent::NoteOver { parallel, .. } => {
                                *parallel = is_parallel_note;
                            }
                            _ => {}
                        }
                        debug!("parsed note event");
                        events.push(evt);
                        continue;
                    }
                    None => {
                        // Check if this starts a multiline note block
                        let rest = note_trimmed[5..].trim();
                        let rest_lower = rest.to_lowercase();
                        if rest_lower.starts_with("right") {
                            in_note_block = true;
                            note_kind = Some("right");
                            note_is_parallel = is_parallel_note;
                            let after = rest[5..].trim();
                            let (after, nc) = extract_note_color(after);
                            note_color = nc;
                            let (_remainder, explicit_p) = strip_of_participant(after);
                            note_on_message = explicit_p.is_none();
                            note_participant = explicit_p.or_else(|| last_to_participant.clone());
                            note_lines.clear();
                            debug!("starting multiline note right");
                            continue;
                        } else if rest_lower.starts_with("left") {
                            in_note_block = true;
                            note_kind = Some("left");
                            note_is_parallel = is_parallel_note;
                            let after = rest[4..].trim();
                            let (after, nc) = extract_note_color(after);
                            note_color = nc;
                            let (_remainder, explicit_p) = strip_of_participant(after);
                            note_on_message = explicit_p.is_none();
                            note_participant = explicit_p.or_else(|| last_from_participant.clone());
                            note_lines.clear();
                            debug!("starting multiline note left");
                            continue;
                        } else if rest_lower.starts_with("over") {
                            in_note_block = true;
                            note_kind = Some("over");
                            note_is_parallel = is_parallel_note;
                            let after = rest[4..].trim();
                            note_participants = after
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            note_lines.clear();
                            debug!("starting multiline note over");
                            continue;
                        }
                    }
                }
            }
        }

        // Parse combined fragments and group/end/else
        {
            // Teoz parallel fragment prefix: "& opt ..." means this fragment
            // is parallel with the previous tile block.
            let (frag_parallel, frag_trimmed) = if let Some(stripped) = trimmed.strip_prefix("& ") {
                (true, stripped.trim())
            } else {
                (false, trimmed)
            };
            // ASCII-only folding: offsets found in `lower` are applied to
            // `frag_trimmed` below, and `to_lowercase` can change a string's
            // byte length (U+212A KELVIN SIGN folds to a one-byte `k`), which
            // would carry those offsets into the middle of a character.
            let lower = frag_trimmed.to_ascii_lowercase();

            // "end" closes a fragment or legacy group
            if lower == "end" {
                if fragment_depth > 0 {
                    fragment_depth -= 1;
                    debug!("parsed fragment end (depth now {fragment_depth})");
                    events.push(SeqEvent::FragmentEnd);
                } else {
                    debug!("parsed group end");
                    events.push(SeqEvent::GroupEnd);
                }
                continue;
            }

            // "else" within a fragment
            if lower.starts_with("else") && fragment_depth > 0 {
                let rest = frag_trimmed[4..].trim();
                let label = strip_leading_colors(rest).to_string();
                debug!("parsed fragment separator: {label:?}");
                events.push(SeqEvent::FragmentSeparator { label });
                continue;
            }

            // Fragment start keywords: alt, loop, opt, par, break, critical
            if let Some((kind, rest_start)) = parse_fragment_keyword(&lower) {
                let (label_str, color1, _color2) =
                    strip_leading_colors_with_colors(frag_trimmed[rest_start..].trim());
                let label = label_str.to_string();
                let color =
                    color1.and_then(|c| crate::klimt::color::resolve_color(&c).map(|c| c.to_svg()));
                fragment_depth += 1;
                debug!(
                    "parsed fragment start {kind:?} label={label:?} parallel={frag_parallel} (depth now {fragment_depth})"
                );
                events.push(SeqEvent::FragmentStart {
                    kind,
                    label,
                    parallel: frag_parallel,
                    color,
                });
                continue;
            }

            // Legacy "group" keyword
            if lower.starts_with("group") {
                let rest = frag_trimmed[5..].trim();
                // Strip leading #color spec(s) — Java regex:
                //   COLORS = "((?<!else)(?<!also)(?<!end)#\w+)?(?:\s+(#\w+))?"
                // Up to two #word tokens can precede the label text.
                let (rest, color1, _color2) = strip_leading_colors_with_colors(rest);
                let label = if rest.is_empty() {
                    None
                } else {
                    Some(rest.to_string())
                };
                let color =
                    color1.and_then(|c| crate::klimt::color::resolve_color(&c).map(|c| c.to_svg()));
                // Track as fragment for proper "end" matching
                fragment_depth += 1;
                debug!(
                    "parsed group start: {label:?} color={color:?} (depth now {fragment_depth})"
                );
                events.push(SeqEvent::FragmentStart {
                    kind: FragmentKind::Group,
                    label: label.unwrap_or_default(),
                    parallel: frag_parallel,
                    color,
                });
                continue;
            }
        }

        // Parse message arrows: boundary arrows first, then regular.
        // Arrow check must come before participant declarations so that lines
        // like "Database -> Bob : ack" are parsed as messages, not as a
        // database participant declaration with rest "-> Bob : ack".

        // Teoz parallel message prefix: "& A -> B : msg" means this message
        // is parallel with the previous one. Strip the "&" prefix and mark
        // the message as parallel for layout.
        let is_parallel = trimmed.starts_with("& ");
        let trimmed_arrow = if is_parallel {
            trimmed[2..].trim()
        } else {
            trimmed
        };

        // Gate/found messages: ?->X (incoming) and X->? (outgoing)
        // Pre-strip inline activation/deactivation and color suffixes
        // before testing gate regexes, since gate syntax like `A <-o? ++`
        // has the `?` before inline suffixes.
        let gate_stripped = {
            let mut s = trimmed_arrow.to_string();
            // Strip trailing `: text` for separate handling
            let gate_text_part: Option<String>;
            if let Some(colon_pos) = s.find(" : ").or_else(|| s.find(":\t")) {
                gate_text_part = Some(
                    s[colon_pos + 1..]
                        .trim_start_matches(':')
                        .trim()
                        .to_string(),
                );
                s = s[..colon_pos].trim().to_string();
            } else if s.ends_with(':') {
                gate_text_part = Some(String::new());
                s = s[..s.len() - 1].trim().to_string();
            } else {
                gate_text_part = None;
            }
            // Strip color suffix (e.g. " #red")
            let gate_color: Option<String>;
            if let Some(hash_pos) = s.rfind(" #") {
                let color_raw = s[hash_pos + 1..].trim();
                gate_color = crate::klimt::color::resolve_color(color_raw).map(|c| c.to_svg());
                s = s[..hash_pos].trim().to_string();
            } else {
                gate_color = None;
            }
            // Strip inline activation/deactivation suffixes
            let mut gate_activate = false;
            let mut gate_deactivate_source = false;
            let mut gate_deactivate = false;
            if s.ends_with("--++") {
                s = s[..s.len() - 4].trim().to_string();
                gate_deactivate_source = true;
                gate_activate = true;
            } else if s.ends_with("++--") {
                s = s[..s.len() - 4].trim().to_string();
                gate_activate = true;
                gate_deactivate = true;
            } else if s.ends_with("++") {
                s = s[..s.len() - 2].trim().to_string();
                gate_activate = true;
            } else if s.ends_with("--") {
                s = s[..s.len() - 2].trim().to_string();
                gate_deactivate = true;
            }
            // Re-append text part for regex matching
            if let Some(ref t) = gate_text_part {
                if !t.is_empty() {
                    s = format!("{s} : {t}");
                }
            }
            (
                s,
                gate_text_part,
                gate_color,
                gate_activate,
                gate_deactivate_source,
                gate_deactivate,
            )
        };
        let (
            ref gate_stripped_line,
            ref _gate_text,
            ref gate_color,
            gate_activate,
            gate_deactivate_source,
            gate_deactivate,
        ) = gate_stripped;
        if let Some(caps) = gate_left_re.captures(gate_stripped_line) {
            let arrow = caps.get(1).unwrap().as_str();
            let mut right = caps.get(2).unwrap().as_str().trim().to_string();
            let text = caps
                .get(3)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            // Strip color from right participant if present (e.g. "B #red")
            if let Some(hash_pos) = right.rfind(" #") {
                right = right[..hash_pos].trim().to_string();
            }
            if let Some(mut msg) = parse_arrow("[", &format!("[{arrow}"), &right, &text) {
                msg.source_line = Some(source_line);
                msg.parallel = is_parallel;
                msg.is_short_gate = true;
                debug!("parsed gate-left message: ?-> {} : {}", msg.to, msg.text);
                ensure_participant(
                    &mut declared_participants,
                    &mut auto_participants,
                    &msg.to,
                    source_line,
                );
                last_to_participant = Some(msg.to.clone());
                let target = msg.to.clone();
                let source = msg.from.clone();
                events.push(SeqEvent::Message(msg));
                // Emit inline life events
                if gate_deactivate_source {
                    inline_life_events.push(events.len());
                    events.push(SeqEvent::Deactivate(source));
                }
                if gate_activate {
                    inline_life_events.push(events.len());
                    events.push(SeqEvent::Activate(target.clone(), gate_color.clone()));
                }
                if gate_deactivate {
                    inline_life_events.push(events.len());
                    events.push(SeqEvent::Deactivate(target));
                }
                continue;
            }
        }
        if let Some(caps) = gate_right_re.captures(gate_stripped_line) {
            let left = caps.get(1).unwrap().as_str().trim();
            let arrow = caps.get(2).unwrap().as_str();
            let text = caps
                .get(3)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            if let Some(mut msg) = parse_arrow(left, &format!("{arrow}]"), "]", &text) {
                msg.source_line = Some(source_line);
                msg.parallel = is_parallel;
                msg.is_short_gate = true;
                // For RTL arrows (A <-o?), the gate is on the RIGHT side
                // of participant A. parse_arrow gives from=A, to=]
                // (right boundary) with RightToLeft direction.
                // Convert to from=[, to=A so the layout sees is_gate_from=true,
                // and gate_right_border=true handles the reversed direction.
                // Do NOT swap circle/cross: they are already correct from
                // parse_arrow's direction-based mapping — circle_from means
                // circle on the original "from" (which becomes the gate side).
                if msg.direction == SeqDirection::RightToLeft {
                    msg.from = "[".to_string();
                    msg.to = left.to_string();
                }
                debug!("parsed gate-right message: {} ->? : {}", msg.from, msg.text);
                // The real participant is whichever is not [ or ]
                let real_participant = if msg.from == "[" || msg.from == "]" {
                    msg.to.clone()
                } else {
                    msg.from.clone()
                };
                ensure_participant(
                    &mut declared_participants,
                    &mut auto_participants,
                    &real_participant,
                    source_line,
                );
                last_to_participant = Some(real_participant.clone());
                events.push(SeqEvent::Message(msg));
                // Emit inline life events — activation target is the real participant
                if gate_deactivate_source {
                    inline_life_events.push(events.len());
                    events.push(SeqEvent::Deactivate(real_participant.clone()));
                }
                if gate_activate {
                    inline_life_events.push(events.len());
                    events.push(SeqEvent::Activate(
                        real_participant.clone(),
                        gate_color.clone(),
                    ));
                }
                if gate_deactivate {
                    inline_life_events.push(events.len());
                    events.push(SeqEvent::Deactivate(real_participant.clone()));
                }
                continue;
            }
        }

        // Boundary arrow from left: [-> participant : text
        if let Some(caps) = boundary_left_re.captures(trimmed_arrow) {
            let arrow = caps.get(1).unwrap().as_str();
            let right = caps.get(2).unwrap().as_str().trim();
            let text = caps
                .get(3)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            if let Some(mut msg) = parse_arrow("[", &format!("[{arrow}"), right, &text) {
                msg.source_line = Some(source_line);
                debug!(
                    "parsed boundary-left message: [-> {} : {}",
                    msg.to, msg.text
                );
                ensure_participant(
                    &mut declared_participants,
                    &mut auto_participants,
                    &msg.to,
                    source_line,
                );
                last_to_participant = Some(msg.to.clone());
                events.push(SeqEvent::Message(msg));
                continue;
            }
        }

        // Boundary arrow to right: participant ->] : text
        if let Some(caps) = boundary_right_re.captures(trimmed_arrow) {
            let left = caps.get(1).unwrap().as_str().trim();
            let arrow = caps.get(2).unwrap().as_str();
            let text = caps
                .get(3)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            if let Some(mut msg) = parse_arrow(left, arrow, "]", &text) {
                msg.source_line = Some(source_line);
                debug!(
                    "parsed boundary-right message: {} ->] : {}",
                    msg.from, msg.text
                );
                ensure_participant(
                    &mut declared_participants,
                    &mut auto_participants,
                    &msg.from,
                    source_line,
                );
                last_to_participant = Some(msg.from.clone());
                events.push(SeqEvent::Message(msg));
                continue;
            }
        }

        // Regular arrows (spaced and no-space variants)
        let arrow_caps = arrow_re
            .captures(trimmed_arrow)
            .or_else(|| arrow_nospace_re.captures(trimmed_arrow));
        if let Some(caps) = arrow_caps {
            let left = caps.get(1).unwrap().as_str().trim();
            let arrow = caps.get(2).unwrap().as_str();
            let mut right = caps.get(3).unwrap().as_str().trim().to_string();
            let text = caps
                .get(4)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();

            // Extract #color suffix (e.g. "a --++ #red" -> right="a --++", inline_color="#FF0000")
            // Java: color annotation applies to the activation bar
            let mut inline_color: Option<String> = None;
            if let Some(hash_pos) = right.rfind(" #") {
                let color_raw = right[hash_pos + 1..].trim();
                inline_color = crate::klimt::color::resolve_color(color_raw).map(|c| c.to_svg());
                right = right[..hash_pos].trim().to_string();
            } else if right.starts_with('#') {
                // Entire right is a color - shouldn't happen, skip
            }
            // Handle inline activation/deactivation suffixes on the target participant.
            // Java: `--++` = deactivate source + activate target, `++--` = activate + deactivate,
            // `++` = activate target, `--` = deactivate target.
            // Must check 4-char suffixes before 2-char ones.
            let mut inline_activate = false;
            let mut inline_deactivate_source = false;
            let mut inline_deactivate = false;
            let mut inline_destroy = false;
            if right.ends_with("--++") {
                right = right[..right.len() - 4].trim().to_string();
                inline_deactivate_source = true;
                inline_activate = true;
            } else if right.ends_with("++--") {
                right = right[..right.len() - 4].trim().to_string();
                inline_activate = true;
                inline_deactivate = true;
            } else if right.ends_with("++") {
                right = right[..right.len() - 2].trim().to_string();
                inline_activate = true;
            } else if right.ends_with("--") {
                right = right[..right.len() - 2].trim().to_string();
                inline_deactivate = true;
            } else if right.ends_with("!!") {
                right = right[..right.len() - 2].trim().to_string();
                inline_destroy = true;
            }

            if let Some(mut msg) = parse_arrow(left, arrow, &right, &text) {
                msg.source_line = Some(source_line);
                msg.parallel = is_parallel;
                debug!("parsed message: {} -> {} : {}", msg.from, msg.to, msg.text);

                // Auto-create participants
                ensure_participant(
                    &mut declared_participants,
                    &mut auto_participants,
                    &msg.from,
                    source_line,
                );
                ensure_participant(
                    &mut declared_participants,
                    &mut auto_participants,
                    &msg.to,
                    source_line,
                );

                last_to_participant = Some(msg.to.clone());
                last_from_participant = Some(msg.from.clone());
                let source = msg.from.clone();
                let target = msg.to.clone();
                events.push(SeqEvent::Message(msg));
                // Java: --++ = deactivate source + activate target
                if inline_deactivate_source {
                    inline_life_events.push(events.len());
                    events.push(SeqEvent::Deactivate(source.clone()));
                }
                if inline_activate {
                    inline_life_events.push(events.len());
                    events.push(SeqEvent::Activate(target.clone(), inline_color.clone()));
                }
                if inline_deactivate {
                    inline_life_events.push(events.len());
                    events.push(SeqEvent::Deactivate(source.clone()));
                }
                if inline_destroy {
                    inline_life_events.push(events.len());
                    events.push(SeqEvent::Destroy(target.clone()));
                }
                continue;
            }
        }

        // Parse participant declarations
        if let Some(caps) = participant_re.captures(trimmed) {
            let kind_str = caps.get(1).unwrap().as_str().to_lowercase();
            let rest = caps.get(2).unwrap().as_str().trim();

            let kind = match kind_str.as_str() {
                "participant" => ParticipantKind::Default,
                "actor" => ParticipantKind::Actor,
                "boundary" => ParticipantKind::Boundary,
                "control" => ParticipantKind::Control,
                "entity" => ParticipantKind::Entity,
                "database" => ParticipantKind::Database,
                "collections" => ParticipantKind::Collections,
                "queue" => ParticipantKind::Queue,
                _ => ParticipantKind::Default,
            };

            let (name, display_name, color, link_url) = parse_participant_details(rest);
            debug!(
                "parsed participant declaration: name={name}, display={display_name:?}, color={color:?}, kind={kind:?}"
            );

            // Remove from auto_participants if it was auto-created
            auto_participants.retain(|p| p.name != name);

            // Avoid duplicate declarations
            if !declared_participants.iter().any(|p| p.name == name) {
                declared_participants.push(Participant {
                    name,
                    display_name,
                    kind,
                    color,
                    source_line: Some(source_line),
                    link_url,
                });
            }
            continue;
        }

        warn!("unrecognized sequence diagram line: {trimmed}");
    }

    // Merge participants: declared first, then auto-created
    let mut participants = declared_participants;
    participants.append(&mut auto_participants);

    // Java: seed = StringUtils.seed(source.getPlainString("\n"))
    let source_seed = {
        let mut h: i64 = 1125899906842597;
        for ch in source.chars() {
            h = h.wrapping_mul(31).wrapping_add(ch as i64);
        }
        h
    };

    Ok(SequenceDiagram {
        participants,
        events,
        teoz_mode,
        hide_footbox,
        delta_shadow,
        inline_life_events,
        source_seed,
    })
}

/// Strip leading `#color` tokens from fragment/group labels.
/// Java regex: `COLORS = "(#\w+)?(?:\s+(#\w+))?"` — up to two `#word` tokens
/// can precede the actual label text. Returns (remaining_text, color1, color2).
fn strip_leading_colors_with_colors(s: &str) -> (&str, Option<String>, Option<String>) {
    let mut rest = s;
    let mut colors: Vec<String> = Vec::new();
    // Strip up to two leading #word tokens
    for _ in 0..2 {
        let trimmed = rest.trim_start();
        if let Some(after_hash) = trimmed.strip_prefix('#') {
            // Find end of #word token (word chars = alphanumeric or _)
            let word_end = after_hash
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .unwrap_or(after_hash.len());
            colors.push(format!("#{}", &after_hash[..word_end]));
            rest = after_hash[word_end..].trim_start();
        } else {
            break;
        }
    }
    let c1 = colors.first().cloned();
    let c2 = colors.get(1).cloned();
    (rest, c1, c2)
}

/// Strip leading `#color` tokens, returning only the remaining text.
fn strip_leading_colors(s: &str) -> &str {
    strip_leading_colors_with_colors(s).0
}

/// Parse combined fragment keyword, return fragment kind and label start position
fn parse_fragment_keyword(lower: &str) -> Option<(FragmentKind, usize)> {
    // Order matters: check longer keywords first to avoid prefix conflicts
    if lower.starts_with("critical")
        && (lower.len() == 8 || lower.as_bytes()[8].is_ascii_whitespace())
    {
        Some((FragmentKind::Critical, 8))
    } else if lower.starts_with("break")
        && (lower.len() == 5 || lower.as_bytes()[5].is_ascii_whitespace())
    {
        Some((FragmentKind::Break, 5))
    } else if lower.starts_with("loop")
        && (lower.len() == 4 || lower.as_bytes()[4].is_ascii_whitespace())
    {
        Some((FragmentKind::Loop, 4))
    } else if lower.starts_with("alt")
        && (lower.len() == 3 || lower.as_bytes()[3].is_ascii_whitespace())
    {
        Some((FragmentKind::Alt, 3))
    } else if lower.starts_with("opt")
        && (lower.len() == 3 || lower.as_bytes()[3].is_ascii_whitespace())
    {
        Some((FragmentKind::Opt, 3))
    } else if lower.starts_with("par")
        && (lower.len() == 3 || lower.as_bytes()[3].is_ascii_whitespace())
    {
        Some((FragmentKind::Par, 3))
    } else {
        None
    }
}

/// Participant declaration keywords accepted by `participant`/`create`/etc.
const PARTICIPANT_KEYWORDS: &[&str] = &[
    "participant",
    "actor",
    "boundary",
    "control",
    "entity",
    "database",
    "collections",
    "queue",
];

/// Strip an optional leading participant-kind keyword from `s` (case-insensitive),
/// returning the parsed kind (if a keyword was present) and the remainder.
fn strip_participant_keyword(s: &str) -> (Option<ParticipantKind>, &str) {
    for kw in PARTICIPANT_KEYWORDS {
        // Match `kw` followed by whitespace, so `actorFoo` is not treated as
        // kind `actor` + name `Foo`.
        // Compare bytes: `s[..kw.len()]` panics when that index lands inside a
        // multi-byte character, and `s` is a user-supplied name. A name of
        // two three-byte characters is cut in half by the 5-byte `actor`.
        if s.len() > kw.len()
            && s.as_bytes()[..kw.len()].eq_ignore_ascii_case(kw.as_bytes())
            && s.as_bytes()[kw.len()].is_ascii_whitespace()
        {
            let kind = match *kw {
                "participant" => ParticipantKind::Default,
                "actor" => ParticipantKind::Actor,
                "boundary" => ParticipantKind::Boundary,
                "control" => ParticipantKind::Control,
                "entity" => ParticipantKind::Entity,
                "database" => ParticipantKind::Database,
                "collections" => ParticipantKind::Collections,
                "queue" => ParticipantKind::Queue,
                _ => ParticipantKind::Default,
            };
            return (Some(kind), s[kw.len()..].trim_start());
        }
    }
    (None, s)
}

/// Ensure a participant created via `create [<kind>] <name>` exists in the
/// auto-created list, preserving the kind / display name / color / link given
/// on the `create` line. Unlike [`ensure_participant`], this only adds to the
/// auto-created list (never the declared list) so the bare `create Job` form
/// keeps the same column ordering as before — declared participants stay first.
fn ensure_create_participant(
    declared: &[Participant],
    auto_created: &mut Vec<Participant>,
    name: &str,
    display_name: Option<String>,
    kind: Option<ParticipantKind>,
    color: Option<String>,
    link_url: Option<String>,
    source_line: usize,
) {
    if declared.iter().any(|p| p.name == name) {
        return;
    }
    if auto_created.iter().any(|p| p.name == name) {
        return;
    }
    debug!("auto-creating participant via `create`: {name}");
    auto_created.push(Participant {
        name: name.to_string(),
        display_name,
        kind: kind.unwrap_or_default(),
        color,
        source_line: Some(source_line),
        link_url,
    });
}

/// Ensure participant exists in either the declared or auto-created list
fn ensure_participant(
    declared: &mut [Participant],
    auto_created: &mut Vec<Participant>,
    name: &str,
    source_line: usize,
) {
    if declared.iter().any(|p| p.name == name) {
        return;
    }
    if auto_created.iter().any(|p| p.name == name) {
        return;
    }
    debug!("auto-creating participant: {name}");
    auto_created.push(Participant {
        name: name.to_string(),
        display_name: None,
        kind: ParticipantKind::Default,
        color: None,
        source_line: Some(source_line),
        link_url: None,
    });
}

/// Normalize a line for fuzzy comparison in line mapping.
/// Expands `%newline()` / `%n()` to the private-use \u{E100} character,
/// so preprocessed lines can match their original source counterparts.
fn normalize_for_mapping(s: &str) -> String {
    s.replace("%newline()", "\u{E100}")
        .replace("%n()", "\u{E100}")
}

/// Check if two lines match for mapping purposes, tolerating variable
/// substitution differences. Compares exact first, then via normalize,
/// then via prefix before the first `\n` / `\\n` (newline escape in
/// message labels is the most common place variables appear).
fn lines_match_for_mapping(block_line: &str, orig_line: &str) -> bool {
    if block_line == orig_line {
        return true;
    }
    if normalize_for_mapping(block_line) == normalize_for_mapping(orig_line) {
        return true;
    }
    // Compare up to the first literal `\n` or `\\n` — if the prefix
    // matches and the original has a variable reference after it,
    // they are almost certainly the same line.
    let bl_prefix = block_line.split("\\n").next().unwrap_or(block_line);
    let ol_prefix = orig_line.split("\\n").next().unwrap_or(orig_line);
    if !bl_prefix.is_empty() && bl_prefix.len() > 5 && bl_prefix == ol_prefix {
        return true;
    }
    false
}

/// Build a mapping from block line index to original source line number.
///
/// Java's data-source-line uses the ORIGINAL .puml file line number (0-based).
/// When preprocessing strips lines (sprite, !pragma, !include, etc.), the
/// cleaned block has fewer lines and different indices.  This function matches
/// each cleaned block line to its position in the original source by content.
fn build_line_mapping(
    cleaned_source: &str,
    original_source: Option<&str>,
    block: &str,
) -> Vec<usize> {
    let orig = original_source.unwrap_or(cleaned_source);
    let orig_lines: Vec<&str> = orig.lines().collect();

    // Find @startuml position in original source
    let start_pos = orig_lines
        .iter()
        .position(|l| {
            let t = l.trim();
            t.starts_with("@startuml") || t.starts_with("@start")
        })
        .unwrap_or(0);

    let block_lines: Vec<&str> = block.lines().collect();
    let mut mapping = Vec::with_capacity(block_lines.len());

    // For each block line, find the matching line in original source
    // searching forward from the last matched position.
    // Only match non-trivial lines (skip empty/whitespace-only and skinparam
    // lines that could come from theme expansion) to avoid false matches
    // that advance search_from past the actual user content.
    let mut search_from = start_pos + 1;
    for bl in &block_lines {
        let trimmed = bl.trim();
        // Skip matching for blank lines and skinparam lines from theme expansion;
        // these are too common and cause false-positive matches that advance
        // search_from past the user's real content lines.
        if trimmed.is_empty() || trimmed.starts_with("skinparam ") {
            mapping.push(start_pos + 1 + mapping.len());
            continue;
        }
        let found_idx = orig_lines[search_from..]
            .iter()
            .position(|ol| {
                // Exact match, or match after expanding %newline() / %n() macros,
                // or prefix match before `\n` to tolerate variable substitution.
                lines_match_for_mapping(trimmed, ol.trim())
            })
            .map(|i| i + search_from);
        if let Some(oi) = found_idx {
            mapping.push(oi);
            search_from = oi + 1;
        } else {
            // Fallback: use sequential offset from @startuml
            mapping.push(start_pos + 1 + mapping.len());
        }
    }

    mapping
}

/// Strip `[[url text]]` markup from a display name, returning (display_text, first_url).
/// E.g. `"[[http://example.com Line 1 Line 2]]"` → `("Line 1 Line 2", Some("http://example.com"))`
fn strip_url_markup(s: &str) -> (String, Option<String>) {
    let mut result = s.to_string();
    let mut first_url = None;
    while let Some(start) = result.find("[[") {
        if let Some(end) = result[start..].find("]]") {
            let inner = &result[start + 2..start + end];
            // Java: if the [[...]] content spans multiple lines (contains
            // the %n() newline placeholder \u{E100}), the URL markup is NOT
            // rendered as a hyperlink — the raw text is displayed literally.
            // However, the URL IS still extracted for the lifeline <title>
            // encoding (encode_link_title).
            if inner.contains(crate::NEWLINE_CHAR) {
                if first_url.is_none() {
                    let url_part = if let Some(sp) = inner.find(' ') {
                        &inner[..sp]
                    } else {
                        inner
                    };
                    first_url = Some(url_part.to_string());
                }
                // Leave display text raw (with [[...]]) — rendering shows literal markup.
                break;
            }
            // Extract URL (first token) and text after URL
            let (url, display) = if let Some(space_pos) = inner.find(' ') {
                (
                    inner[..space_pos].to_string(),
                    inner[space_pos + 1..].to_string(),
                )
            } else {
                (inner.to_string(), String::new())
            };
            if first_url.is_none() {
                first_url = Some(url);
            }
            result.replace_range(start..start + end + 2, &display);
        } else {
            break;
        }
    }
    (result, first_url)
}

/// Parse participant declaration details: name, display name, and color
fn parse_participant_details(
    rest: &str,
) -> (String, Option<String>, Option<String>, Option<String>) {
    // Patterns:
    //   "Display Name" as Name #color
    //   "Display Name" as Name
    //   Name as "Display Name" #color
    //   Name #color
    //   Name

    let mut remaining = rest.trim();
    let mut name: String;
    let mut display_name: Option<String> = None;
    let mut link_url: Option<String> = None;

    if remaining.starts_with('"') {
        // Quoted display name first: "Display Name" as Name ...
        if let Some(end_quote) = remaining[1..].find('"') {
            let quoted = remaining[1..=end_quote].to_string();
            remaining = remaining[end_quote + 2..].trim();
            // Strip [[url text]] patterns — extract display text + URL
            let (cleaned, url) = strip_url_markup(&quoted);
            link_url = url;
            display_name = Some(cleaned);

            // Expect "as Name" next
            let lower = remaining.to_lowercase();
            if lower.starts_with("as ") {
                remaining = remaining[3..].trim();
            }
            // Extract name (next token)
            let (n, rest_after) = take_token(remaining);
            name = n;
            remaining = rest_after;
        } else {
            // No closing quote, treat whole thing as name
            name = remaining.to_string();
            remaining = "";
        }
    } else {
        // Name first
        let (n, rest_after) = take_token(remaining);
        name = n;
        remaining = rest_after.trim();

        // Check for "as" — "Name as Alias" means alias is the canonical name,
        // and Name becomes the display name
        let lower = remaining.to_lowercase();
        if lower.starts_with("as ") {
            remaining = remaining[3..].trim();
            let original_name = name.clone();
            if remaining.starts_with('"') {
                // as "Display Name" ... (alias is quoted - unusual but handle it)
                if let Some(end_quote) = remaining[1..].find('"') {
                    name = remaining[1..=end_quote].to_string();
                    display_name = Some(original_name);
                    remaining = remaining[end_quote + 2..].trim();
                }
            } else {
                let (alias, rest_after2) = take_token(remaining);
                name = alias;
                display_name = Some(original_name);
                remaining = rest_after2;
            }
        }
    }

    // Check for color at the end
    let remaining = remaining.trim();
    let color = if remaining.starts_with('#') {
        Some(remaining.to_string())
    } else {
        None
    };

    (name, display_name, color, link_url)
}

/// Extract the first whitespace-delimited token from the beginning of the string
fn take_token(s: &str) -> (String, &str) {
    let s = s.trim();
    if s.is_empty() {
        return (String::new(), "");
    }
    match s.find(char::is_whitespace) {
        Some(pos) => (s[..pos].to_string(), &s[pos..]),
        None => (s.to_string(), ""),
    }
}

/// Extract `[#color]` from an arrow string, returning (color, cleaned_arrow).
/// E.g. `"[#blue]->"` → `(Some("blue"), "->")`, `"-[#green]>"` → `(Some("green"), "->")`
fn extract_arrow_color(arrow: &str) -> (Option<String>, String) {
    if let Some(start) = arrow.find("[#") {
        if let Some(end) = arrow[start..].find(']') {
            let color = arrow[start + 2..start + end].to_string();
            let cleaned = format!("{}{}", &arrow[..start], &arrow[start + end + 1..]);
            return (Some(color), cleaned);
        }
    }
    (None, arrow.to_string())
}

/// Check whether the arrow contains `[hidden]` and strip it if so.
fn extract_arrow_hidden(arrow: &str) -> (bool, String) {
    let lower = arrow.to_lowercase();
    if let Some(start) = lower.find("[hidden]") {
        let cleaned = format!("{}{}", &arrow[..start], &arrow[start + 8..]);
        return (true, cleaned);
    }
    (false, arrow.to_string())
}

/// Parse arrow syntax and return a Message.
/// Handles full PlantUML syntax: heads (>, >>), half-arrows (/, \),
/// decorators (o, x), boundary ([, ]), and shaft (-, --).
fn parse_arrow(left: &str, arrow: &str, right: &str, text: &str) -> Option<Message> {
    let (is_hidden, arrow_no_hidden) = extract_arrow_hidden(arrow);
    let (color, clean_arrow) = extract_arrow_color(&arrow_no_hidden);

    // Strip boundary markers first, then detect circle/cross decorators.
    // For boundary arrows like [o-> or ->o], the [ and ] come outermost.
    let has_left_boundary = clean_arrow.starts_with('[');
    let has_right_boundary = clean_arrow.ends_with(']');
    let after_boundary = if has_left_boundary {
        &clean_arrow[1..]
    } else {
        &clean_arrow[..]
    };
    let after_boundary = if has_right_boundary {
        &after_boundary[..after_boundary.len().saturating_sub(1)]
    } else {
        after_boundary
    };

    // Detect circle and cross decorators after boundary markers
    let has_left_circle = after_boundary.starts_with('o');
    let has_right_circle = after_boundary.ends_with('o');
    let has_left_cross = after_boundary.starts_with('x') || after_boundary.starts_with('X');
    let has_right_cross = after_boundary.ends_with('x') || after_boundary.ends_with('X');

    // Strip outer decorators (o, x, X)
    let stripped = after_boundary.trim_start_matches(['o', 'x', 'X']);
    let stripped = stripped.trim_end_matches(['o', 'x', 'X']);

    // Check for arrow heads / half-arrows on left and right
    let has_left_arrow =
        stripped.starts_with('<') || stripped.starts_with('/') || stripped.starts_with('\\');
    let _has_open_left =
        stripped.starts_with("<<") || stripped.starts_with("//") || stripped.starts_with("\\\\");
    let has_right_arrow =
        stripped.ends_with('>') || stripped.ends_with('/') || stripped.ends_with('\\');
    let _has_open_right =
        stripped.ends_with(">>") || stripped.ends_with("//") || stripped.ends_with("\\\\");

    // Must have at least one arrowhead, half-arrow, or boundary marker
    if !has_left_arrow && !has_right_arrow && !has_left_boundary && !has_right_boundary {
        return None;
    }

    // Direction: left-pointing heads mean right-to-left
    let direction =
        if stripped.starts_with('<') || stripped.starts_with('/') || stripped.starts_with('\\') {
            SeqDirection::RightToLeft
        } else {
            SeqDirection::LeftToRight
        };

    let arrow_head = if stripped.starts_with("<<") || stripped.ends_with(">>") {
        SeqArrowHead::Open
    } else if stripped.starts_with("//") || stripped.ends_with("//") {
        SeqArrowHead::HalfTop
    } else if stripped.starts_with("\\\\") || stripped.ends_with("\\\\") {
        SeqArrowHead::HalfBottom
    } else if stripped.ends_with('\\') || stripped.starts_with('/') {
        SeqArrowHead::FilledHalfTop
    } else if stripped.ends_with('/') || stripped.starts_with('\\') {
        SeqArrowHead::FilledHalfBottom
    } else {
        SeqArrowHead::Filled
    };

    // Shaft style: -- is dashed, - is solid
    let shaft = stripped
        .trim_start_matches(['<', '/', '\\'])
        .trim_end_matches(['>', '/', '\\']);
    let arrow_style = if shaft.contains("--") {
        SeqArrowStyle::Dashed
    } else {
        SeqArrowStyle::Solid
    };

    let (from, to) = if has_left_boundary {
        (left.to_string(), right.to_string())
    } else if has_right_boundary {
        (left.to_string(), "]".to_string())
    } else {
        match direction {
            SeqDirection::LeftToRight => (left.to_string(), right.to_string()),
            SeqDirection::RightToLeft => (right.to_string(), left.to_string()),
        }
    };

    // Map left/right circle/cross decorators to from/to based on direction
    let (circle_from, circle_to) = match direction {
        SeqDirection::LeftToRight => (has_left_circle, has_right_circle),
        SeqDirection::RightToLeft => (has_right_circle, has_left_circle),
    };
    let (cross_from, cross_to) = match direction {
        SeqDirection::LeftToRight => (has_left_cross, has_right_cross),
        SeqDirection::RightToLeft => (has_right_cross, has_left_cross),
    };

    let is_reverse_define = !has_right_arrow && has_left_arrow;
    let bidirectional = has_left_arrow && has_right_arrow;

    Some(Message {
        from,
        to,
        text: text.to_string(),
        arrow_style,
        arrow_head,
        direction,
        color,
        source_line: None, // set by caller
        circle_from,
        circle_to,
        cross_from,
        cross_to,
        parallel: false, // set by caller if & prefix detected
        is_reverse_define,
        hidden: is_hidden,
        bidirectional,
        is_short_gate: false, // set by caller for ?-> / ->? arrows
    })
}

/// Parse a single-line note (with `:` inline text).
/// Returns None if the note has no inline text (multiline note handled by caller).
///
/// Supported syntax:
/// - `note right : text`       — note on last message target
/// - `note right of Bob : text` — note on explicit participant
/// - `note right #color : text` — note with background color
fn parse_note(
    line: &str,
    last_to: &Option<String>,
    last_from: &Option<String>,
) -> Option<SeqEvent> {
    let rest = line.trim().strip_prefix("note ")?.trim_start();
    let lower = rest.to_lowercase();

    if lower.starts_with("right") {
        let after = rest[5..].trim();
        let (after, note_color) = extract_note_color(after);
        // Handle `of PARTICIPANT` clause
        let (after, explicit_participant) = strip_of_participant(after);
        if let Some(text) = after.strip_prefix(':') {
            let text = text.trim().to_string();
            let on_message = explicit_participant.is_none();
            let participant = explicit_participant
                .or_else(|| last_to.clone())
                .unwrap_or_default();
            Some(SeqEvent::NoteRight {
                participant,
                text,
                parallel: false,
                color: note_color,
                on_message,
            })
        } else {
            // No inline text — will be handled as multiline note
            None
        }
    } else if lower.starts_with("left") {
        let after = rest[4..].trim();
        let (after, note_color) = extract_note_color(after);
        let (after, explicit_participant) = strip_of_participant(after);
        if let Some(text) = after.strip_prefix(':') {
            let text = text.trim().to_string();
            let on_message = explicit_participant.is_none();
            let participant = explicit_participant
                .or_else(|| last_from.clone())
                .unwrap_or_default();
            Some(SeqEvent::NoteLeft {
                participant,
                text,
                parallel: false,
                color: note_color,
                on_message,
            })
        } else {
            None
        }
    } else if lower.starts_with("over") {
        let after = rest[4..].trim();
        // note over A,B : text  or  note over A : text
        if let Some(colon_pos) = after.find(':') {
            let participants_str = after[..colon_pos].trim();
            let text = after[colon_pos + 1..].trim().to_string();
            let participants: Vec<String> = participants_str
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            Some(SeqEvent::NoteOver {
                participants,
                text,
                parallel: false,
            })
        } else {
            // No inline text — multiline
            None
        }
    } else {
        None
    }
}

/// Extract optional note background color specifier (e.g., `#red`, `#AABBCC`).
/// Returns (remaining_str, Option<color_string>).
/// Java: `note right #color : text` syntax.
fn extract_note_color(s: &str) -> (&str, Option<String>) {
    if let Some(rest) = s.strip_prefix('#') {
        // Find end of color: next whitespace or ':'
        let end = rest
            .find(|c: char| c.is_whitespace() || c == ':')
            .unwrap_or(rest.len());
        let color_raw = &rest[..end];
        // Try resolving via klimt color (handles named colors like "red" → #FF0000)
        let resolved = crate::klimt::color::resolve_color(&format!("#{color_raw}"))
            .map(|c| c.to_svg())
            .unwrap_or_else(|| crate::style::normalize_color(&format!("#{color_raw}")));
        (rest[end..].trim_start(), Some(resolved))
    } else {
        (s, None)
    }
}

/// Skip optional note background color specifier (backward compat wrapper).
#[allow(dead_code)] // backward compat helper
fn skip_note_color(s: &str) -> &str {
    extract_note_color(s).0
}

/// Strip optional `of PARTICIPANT` clause from note direction remainder.
/// Returns (remaining_str, Option<participant_name>).
///
/// Input examples:
/// - `"of Alice: ok"`   -> `(": ok", Some("Alice"))`
/// - `": text"`         -> `(": text", None)`
/// - `""`               -> `("", None)`
fn strip_of_participant(s: &str) -> (&str, Option<String>) {
    let lower = s.to_lowercase();
    if lower.starts_with("of ") {
        // `s` has the original case; skip same number of chars
        let rest_orig = s["of ".len()..].trim_start();
        // Participant name ends at ':' or end of string
        if let Some(colon_pos) = rest_orig.find(':') {
            let participant = rest_orig[..colon_pos].trim().to_string();
            let remainder = &rest_orig[colon_pos..];
            (remainder, Some(participant))
        } else {
            // No colon — multiline note with explicit participant
            let participant = rest_orig.trim().to_string();
            let empty = &s[s.len()..]; // empty slice at end
            if participant.is_empty() {
                (empty, None)
            } else {
                (empty, Some(participant))
            }
        }
    } else {
        (s, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1. Parse simplest message
    #[test]
    fn parse_simplest_message() {
        let src = "@startuml\nalice->bob: hello\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.participants.len(), 2);
        assert_eq!(diagram.participants[0].name, "alice");
        assert_eq!(diagram.participants[1].name, "bob");
        assert_eq!(diagram.events.len(), 1);

        if let SeqEvent::Message(msg) = &diagram.events[0] {
            assert_eq!(msg.from, "alice");
            assert_eq!(msg.to, "bob");
            assert_eq!(msg.text, "hello");
            assert_eq!(msg.arrow_style, SeqArrowStyle::Solid);
            assert_eq!(msg.arrow_head, SeqArrowHead::Filled);
            assert_eq!(msg.direction, SeqDirection::LeftToRight);
        } else {
            panic!("expected Message event");
        }
    }

    /// 2. Parse dashed arrow
    #[test]
    fn parse_dashed_arrow() {
        let src = "@startuml\nA --> B : msg\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.events.len(), 1);
        if let SeqEvent::Message(msg) = &diagram.events[0] {
            assert_eq!(msg.from, "A");
            assert_eq!(msg.to, "B");
            assert_eq!(msg.text, "msg");
            assert_eq!(msg.arrow_style, SeqArrowStyle::Dashed);
            assert_eq!(msg.direction, SeqDirection::LeftToRight);
        } else {
            panic!("expected Message event");
        }
    }

    /// 3. Parse left arrow
    #[test]
    fn parse_left_arrow() {
        let src = "@startuml\nA <- B : msg\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        if let SeqEvent::Message(msg) = &diagram.events[0] {
            assert_eq!(msg.from, "B");
            assert_eq!(msg.to, "A");
            assert_eq!(msg.direction, SeqDirection::RightToLeft);
            assert_eq!(msg.arrow_style, SeqArrowStyle::Solid);
        } else {
            panic!("expected Message event");
        }
    }

    /// 4. Parse self-message
    #[test]
    fn parse_self_message() {
        let src = "@startuml\nBob->Bob: hello\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.participants.len(), 1);
        assert_eq!(diagram.participants[0].name, "Bob");

        if let SeqEvent::Message(msg) = &diagram.events[0] {
            assert_eq!(msg.from, "Bob");
            assert_eq!(msg.to, "Bob");
        } else {
            panic!("expected Message event");
        }
    }

    /// 5. Parse activate/deactivate
    #[test]
    fn parse_activate_deactivate() {
        let src = "@startuml\nA -> B : a\nactivate B\nB --> A : b\ndeactivate B\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(matches!(&diagram.events[0], SeqEvent::Message(_)));
        assert!(matches!(&diagram.events[1], SeqEvent::Activate(ref n, _) if n == "B"));
        assert!(matches!(&diagram.events[2], SeqEvent::Message(_)));
        assert!(matches!(&diagram.events[3], SeqEvent::Deactivate(ref n) if n == "B"));
    }

    /// 6. Parse destroy
    #[test]
    fn parse_destroy() {
        let src = "@startuml\nBob->Bob: hello\ndestroy Bob\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(matches!(&diagram.events[1], SeqEvent::Destroy(ref n) if n == "Bob"));
    }

    /// 6b. Parse create directive (issue #36)
    #[test]
    fn parse_create() {
        let src = "@startuml\nClient -> Worker: enqueue\ncreate Job\nWorker -> Job: new\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        // `create Job` emits a Create event before the creating message.
        let create = diagram
            .events
            .iter()
            .find_map(|e| match e {
                SeqEvent::Create(n) => Some(n.clone()),
                _ => None,
            })
            .expect("expected a Create event");
        assert_eq!(create, "Job");
        // The created participant is registered so it gets a column + tail box.
        assert!(diagram.participants.iter().any(|p| p.name == "Job"));
    }

    /// 6c. Parse `create` with a participant-kind keyword (issue #36 nit)
    #[test]
    fn parse_create_with_kind_keyword() {
        // `create participant Job` must register `Job`, not `"participant Job"`.
        let src = "@startuml\nClient -> Worker: enqueue\ncreate participant Job\nWorker -> Job: new\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        let create = diagram
            .events
            .iter()
            .find_map(|e| match e {
                SeqEvent::Create(n) => Some(n.clone()),
                _ => None,
            })
            .expect("expected a Create event");
        assert_eq!(create, "Job");
        let job = diagram
            .participants
            .iter()
            .find(|p| p.name == "Job")
            .expect("Job should be registered");
        assert_eq!(job.kind, ParticipantKind::Default);

        // `create actor Job` selects the Actor shape.
        let src =
            "@startuml\nClient -> Worker: enqueue\ncreate actor Job\nWorker -> Job: new\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();
        let job = diagram
            .participants
            .iter()
            .find(|p| p.name == "Job")
            .expect("Job should be registered");
        assert_eq!(job.kind, ParticipantKind::Actor);

        // `create database "Job" as J` registers the alias `J` with display "Job".
        let src = "@startuml\nClient -> Worker: enqueue\ncreate database \"Job\" as J\nWorker -> J: new\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();
        let create = diagram
            .events
            .iter()
            .find_map(|e| match e {
                SeqEvent::Create(n) => Some(n.clone()),
                _ => None,
            })
            .expect("expected a Create event");
        assert_eq!(create, "J");
        let j = diagram
            .participants
            .iter()
            .find(|p| p.name == "J")
            .expect("J should be registered");
        assert_eq!(j.kind, ParticipantKind::Database);
        assert_eq!(j.display_name.as_deref(), Some("Job"));

        // Bare `create Job` (no keyword) is unchanged.
        let src = "@startuml\nClient -> Worker: enqueue\ncreate Job\nWorker -> Job: new\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();
        assert!(diagram
            .participants
            .iter()
            .all(|p| p.name != "participant Job"));
    }

    /// 7. Parse participant declaration with color
    #[test]
    fn parse_participant_with_color() {
        let src = "@startuml\nparticipant Alice #FFFFFF\nAlice -> Bob : hi\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.participants[0].name, "Alice");
        assert_eq!(diagram.participants[0].color.as_deref(), Some("#FFFFFF"));
        assert_eq!(diagram.participants[0].kind, ParticipantKind::Default);
    }

    /// 8. Parse actor declaration
    #[test]
    fn parse_actor_declaration() {
        let src = "@startuml\nactor Bob\nBob -> Alice : hi\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.participants[0].name, "Bob");
        assert_eq!(diagram.participants[0].kind, ParticipantKind::Actor);
    }

    /// 9. Parse group/end (now emits FragmentStart/FragmentEnd)
    #[test]
    fn parse_group_end() {
        let src = "@startuml\ngroup My Group\na -> b : msg\nend\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(
            matches!(&diagram.events[0], SeqEvent::FragmentStart { kind, label, .. } if *kind == FragmentKind::Group && label == "My Group")
        );
        assert!(matches!(&diagram.events[1], SeqEvent::Message(_)));
        assert!(matches!(&diagram.events[2], SeqEvent::FragmentEnd));
    }

    /// 10. Parse note right/left
    #[test]
    fn parse_note_right_left() {
        let src = "@startuml\nTest --> Test: Text\nnote right: comment\nnote left: other\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(matches!(&diagram.events[0], SeqEvent::Message(_)));
        if let SeqEvent::NoteRight {
            participant,
            text,
            on_message,
            ..
        } = &diagram.events[1]
        {
            assert_eq!(participant, "Test");
            assert_eq!(text, "comment");
            // `note right:` with no explicit `of PARTICIPANT` is a *message* note
            // (Java `Note(null)`) — attached to the last arrow, not a standalone slot.
            assert!(*on_message, "note right: should be on_message=true");
        } else {
            panic!("expected NoteRight");
        }
        if let SeqEvent::NoteLeft {
            participant,
            text,
            on_message,
            ..
        } = &diagram.events[2]
        {
            assert_eq!(participant, "Test");
            assert_eq!(text, "other");
            assert!(*on_message, "note left: should be on_message=true");
        } else {
            panic!("expected NoteLeft");
        }
    }

    /// 11. Parse divider
    #[test]
    fn parse_divider() {
        let src = "@startuml\n== My Divider ==\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.events.len(), 1);
        assert!(
            matches!(&diagram.events[0], SeqEvent::Divider { text } if text.as_deref() == Some("My Divider"))
        );
    }

    /// 12. Auto-create participants from messages
    #[test]
    fn auto_create_participants() {
        let src = "@startuml\nAlice -> Bob : hi\nBob -> Charlie : hey\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.participants.len(), 3);
        assert_eq!(diagram.participants[0].name, "Alice");
        assert_eq!(diagram.participants[1].name, "Bob");
        assert_eq!(diagram.participants[2].name, "Charlie");
        // All auto-created should be Default kind
        for p in &diagram.participants {
            assert_eq!(p.kind, ParticipantKind::Default);
        }
    }

    /// 13. Skip style blocks and skinparam
    #[test]
    fn skip_style_and_skinparam() {
        let src = r#"@startuml
title title
legend legend
footer footer
header header
caption caption
<style>
    document {
       BackGroundColor orange
    }
</style>
skinparam {
   Maxmessagesize 200
}
Sally --> Bob
@enduml"#;
        let diagram = parse_sequence_diagram(src).unwrap();

        // Only one message, style/skinparam/title etc. are all skipped
        assert_eq!(diagram.events.len(), 1);
        assert!(matches!(&diagram.events[0], SeqEvent::Message(_)));
        assert_eq!(diagram.participants.len(), 2);
    }

    /// 14. Parse fixture test_0.puml
    #[test]
    fn parse_fixture_test_0() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/sequence/test_0.puml"
        ))
        .unwrap();
        let diagram = parse_sequence_diagram(&src).unwrap();

        assert_eq!(diagram.participants.len(), 2);
        assert_eq!(diagram.participants[0].name, "alice");
        assert_eq!(diagram.participants[1].name, "bob");
        assert_eq!(diagram.events.len(), 1);

        if let SeqEvent::Message(msg) = &diagram.events[0] {
            assert_eq!(msg.from, "alice");
            assert_eq!(msg.to, "bob");
            assert_eq!(msg.text, "this is a test");
        } else {
            panic!("expected Message event");
        }
    }

    /// 15. Parse fixture a0001.puml
    #[test]
    fn parse_fixture_a0001() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/sequence/a0001.puml"
        ))
        .unwrap();
        let diagram = parse_sequence_diagram(&src).unwrap();

        // Participants: Bob and Alice (auto-created)
        assert_eq!(diagram.participants.len(), 2);
        assert_eq!(diagram.participants[0].name, "Bob");
        assert_eq!(diagram.participants[1].name, "Alice");

        // Events: message, activate, message, destroy, message, message
        // Bob->Bob: hello1
        // activate Bob
        // Bob->Bob: hello2
        // destroy Bob
        // Bob->Bob: this is an\nexample of long\nmessage
        // Bob->Alice: And this\nis an other on\nvery long too
        assert!(
            matches!(&diagram.events[0], SeqEvent::Message(m) if m.from == "Bob" && m.to == "Bob")
        );
        assert!(matches!(&diagram.events[1], SeqEvent::Activate(ref n, _) if n == "Bob"));
        assert!(
            matches!(&diagram.events[2], SeqEvent::Message(m) if m.from == "Bob" && m.to == "Bob")
        );
        assert!(matches!(&diagram.events[3], SeqEvent::Destroy(ref n) if n == "Bob"));
        assert!(
            matches!(&diagram.events[4], SeqEvent::Message(m) if m.text.contains("an\\nexample"))
        );
        assert!(
            matches!(&diagram.events[5], SeqEvent::Message(m) if m.from == "Bob" && m.to == "Alice")
        );
    }

    /// 16. Parse participant with 'as' alias
    #[test]
    fn parse_participant_with_alias() {
        let src = "@startuml\nparticipant \"Long Name\" as LN\nLN -> B : hi\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.participants[0].name, "LN");
        assert_eq!(
            diagram.participants[0].display_name.as_deref(),
            Some("Long Name")
        );
    }

    /// 17. Parse open arrowhead
    #[test]
    fn parse_open_arrowhead() {
        let src = "@startuml\nA ->> B : open\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        if let SeqEvent::Message(msg) = &diagram.events[0] {
            assert_eq!(msg.arrow_head, SeqArrowHead::Open);
            assert_eq!(msg.arrow_style, SeqArrowStyle::Solid);
        } else {
            panic!("expected Message event");
        }
    }

    /// 18. Parse delay (|||) and spacing (||45||)
    #[test]
    fn parse_delay_and_spacing() {
        let src = "@startuml\n|||\n||45||\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.events.len(), 2);
        assert!(matches!(&diagram.events[0], SeqEvent::Delay { text } if text.is_none()));
        // ||45|| is explicit spacing, not delay
        assert!(matches!(&diagram.events[1], SeqEvent::Spacing { pixels } if *pixels == 45));
    }

    /// 19. Declared participants come first, then auto-created
    #[test]
    fn participant_ordering() {
        let src = "@startuml\nAlice -> Bob : hi\nparticipant Bob\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        // Bob was declared, Alice was auto-created
        assert_eq!(diagram.participants[0].name, "Bob");
        assert_eq!(diagram.participants[1].name, "Alice");
    }

    /// 20. Parse dashed left arrow
    #[test]
    fn parse_dashed_left_arrow() {
        let src = "@startuml\nA <-- B : msg\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        if let SeqEvent::Message(msg) = &diagram.events[0] {
            assert_eq!(msg.from, "B");
            assert_eq!(msg.to, "A");
            assert_eq!(msg.direction, SeqDirection::RightToLeft);
            assert_eq!(msg.arrow_style, SeqArrowStyle::Dashed);
        } else {
            panic!("expected Message event");
        }
    }

    /// 21. Parse fixture sequencelayout_0003 (notes after arrows)
    #[test]
    fn parse_fixture_sequencelayout_0003() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/sequence/sequencelayout_0003.puml"
        ))
        .unwrap();
        let diagram = parse_sequence_diagram(&src).unwrap();

        // Should have 1 participant (Test), 8 events (4 messages + 4 notes)
        assert_eq!(diagram.participants.len(), 1);
        assert_eq!(diagram.participants[0].name, "Test");
        assert_eq!(diagram.events.len(), 8);
    }

    /// 22. Parse arrow without colon text (message is empty)
    #[test]
    fn parse_arrow_no_text() {
        let src = "@startuml\nSally --> Bob\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.events.len(), 1);
        if let SeqEvent::Message(msg) = &diagram.events[0] {
            assert_eq!(msg.from, "Sally");
            assert_eq!(msg.to, "Bob");
            assert!(msg.text.is_empty());
        } else {
            panic!("expected Message event");
        }
    }

    /// 23. Parse alt/else/end
    #[test]
    fn parse_alt_else_end() {
        let src = "@startuml\nA -> B : req\nalt success\nB -> A : ok\nelse failure\nB -> A : err\nend\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(
            matches!(&diagram.events[1], SeqEvent::FragmentStart { kind, label, .. } if *kind == FragmentKind::Alt && label == "success")
        );
        assert!(
            matches!(&diagram.events[3], SeqEvent::FragmentSeparator { label } if label == "failure")
        );
        assert!(matches!(&diagram.events[5], SeqEvent::FragmentEnd));
    }

    /// 24. Parse loop
    #[test]
    fn parse_loop() {
        let src = "@startuml\nloop 1000 times\nA -> B : data\nend\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(
            matches!(&diagram.events[0], SeqEvent::FragmentStart { kind, label, .. } if *kind == FragmentKind::Loop && label == "1000 times")
        );
        assert!(matches!(&diagram.events[2], SeqEvent::FragmentEnd));
    }

    /// 25. Parse opt
    #[test]
    fn parse_opt() {
        let src = "@startuml\nopt extra processing\nA -> B : do\nend\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(
            matches!(&diagram.events[0], SeqEvent::FragmentStart { kind, label, .. } if *kind == FragmentKind::Opt && label == "extra processing")
        );
    }

    /// 26. Parse par with else
    #[test]
    fn parse_par_else() {
        let src = "@startuml\npar thread 1\nA -> B : t1\nelse thread 2\nA -> C : t2\nend\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(
            matches!(&diagram.events[0], SeqEvent::FragmentStart { kind, label, .. } if *kind == FragmentKind::Par && label == "thread 1")
        );
        assert!(
            matches!(&diagram.events[2], SeqEvent::FragmentSeparator { label } if label == "thread 2")
        );
        assert!(matches!(&diagram.events[4], SeqEvent::FragmentEnd));
    }

    /// 27. Parse break
    #[test]
    fn parse_break() {
        let src = "@startuml\nbreak\nA -> B : err\nend\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(
            matches!(&diagram.events[0], SeqEvent::FragmentStart { kind, .. } if *kind == FragmentKind::Break)
        );
    }

    /// 28. Parse critical
    #[test]
    fn parse_critical() {
        let src = "@startuml\ncritical\nA -> B : write\nend\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(
            matches!(&diagram.events[0], SeqEvent::FragmentStart { kind, .. } if *kind == FragmentKind::Critical)
        );
    }

    /// 29. Parse ref over
    #[test]
    fn parse_ref_over() {
        let src = "@startuml\nref over A, B : init phase\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.events.len(), 1);
        if let SeqEvent::Ref {
            participants,
            label,
        } = &diagram.events[0]
        {
            assert_eq!(participants, &["A", "B"]);
            assert_eq!(label, "init phase");
        } else {
            panic!("expected Ref event");
        }
    }

    /// 30. Parse delay with text
    #[test]
    fn parse_delay_with_text() {
        let src = "@startuml\n...waiting...\n...\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.events.len(), 2);
        assert!(
            matches!(&diagram.events[0], SeqEvent::Delay { text } if text.as_deref() == Some("waiting"))
        );
        assert!(matches!(&diagram.events[1], SeqEvent::Delay { text } if text.is_none()));
    }

    /// 31. Parse spacing
    #[test]
    fn parse_spacing() {
        let src = "@startuml\n|| 50 ||\n@enduml";
        let err = parse_sequence_diagram(src).unwrap_err();
        match err {
            crate::Error::JavaErrorPage { line, message } => {
                assert_eq!(line, 2);
                assert_eq!(message, "Syntax Error? (Assumed diagram type: sequence)");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    /// 32. Parse autonumber
    #[test]
    fn parse_autonumber() {
        let src = "@startuml\nautonumber\nA -> B : hello\nautonumber 10\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(matches!(&diagram.events[0], SeqEvent::AutoNumber { start } if start.is_none()));
        assert!(matches!(&diagram.events[2], SeqEvent::AutoNumber { start } if *start == Some(10)));
    }

    /// 33. Parse colored arrow [#blue]->
    #[test]
    fn parse_colored_arrow_prefix() {
        let src = "@startuml\na[#blue]->b: hello\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.events.len(), 1);
        if let SeqEvent::Message(msg) = &diagram.events[0] {
            assert_eq!(msg.from, "a");
            assert_eq!(msg.to, "b");
            assert_eq!(msg.text, "hello");
            assert_eq!(msg.arrow_style, SeqArrowStyle::Solid);
            assert_eq!(msg.arrow_head, SeqArrowHead::Filled);
            assert_eq!(msg.direction, SeqDirection::LeftToRight);
            assert_eq!(msg.color.as_deref(), Some("blue"));
        } else {
            panic!("expected Message event");
        }
    }

    /// 34. Parse colored arrow -[#green]>
    #[test]
    fn parse_colored_arrow_infix() {
        let src = "@startuml\na-[#green]>b: world\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.events.len(), 1);
        if let SeqEvent::Message(msg) = &diagram.events[0] {
            assert_eq!(msg.from, "a");
            assert_eq!(msg.to, "b");
            assert_eq!(msg.text, "world");
            assert_eq!(msg.arrow_style, SeqArrowStyle::Solid);
            assert_eq!(msg.arrow_head, SeqArrowHead::Filled);
            assert_eq!(msg.direction, SeqDirection::LeftToRight);
            assert_eq!(msg.color.as_deref(), Some("green"));
        } else {
            panic!("expected Message event");
        }
    }

    /// 35. Parse colored dashed arrow --[#red]>>
    #[test]
    fn parse_colored_dashed_open_arrow() {
        let src = "@startuml\nA --[#red]>> B : msg\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.events.len(), 1);
        if let SeqEvent::Message(msg) = &diagram.events[0] {
            assert_eq!(msg.from, "A");
            assert_eq!(msg.to, "B");
            assert_eq!(msg.text, "msg");
            assert_eq!(msg.arrow_style, SeqArrowStyle::Dashed);
            assert_eq!(msg.arrow_head, SeqArrowHead::Open);
            assert_eq!(msg.direction, SeqDirection::LeftToRight);
            assert_eq!(msg.color.as_deref(), Some("red"));
        } else {
            panic!("expected Message event");
        }
    }

    /// 36. Parse colored left arrow <[#FF0000]--
    #[test]
    fn parse_colored_left_arrow() {
        let src = "@startuml\nA <[#FF0000]-- B : back\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert_eq!(diagram.events.len(), 1);
        if let SeqEvent::Message(msg) = &diagram.events[0] {
            assert_eq!(msg.from, "B");
            assert_eq!(msg.to, "A");
            assert_eq!(msg.text, "back");
            assert_eq!(msg.arrow_style, SeqArrowStyle::Dashed);
            assert_eq!(msg.direction, SeqDirection::RightToLeft);
            assert_eq!(msg.color.as_deref(), Some("FF0000"));
        } else {
            panic!("expected Message event");
        }
    }

    /// 37. Parse nested fragments (was #33)
    #[test]
    fn parse_nested_fragments() {
        let src = "@startuml\nalt case1\nloop retry\nalt inner\nA -> B : x\nend\nend\nelse case2\nA -> B : y\nend\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        // alt case1 -> loop retry -> alt inner -> msg -> end(inner) -> end(loop) -> else case2 -> msg -> end(outer alt)
        assert!(
            matches!(&diagram.events[0], SeqEvent::FragmentStart { kind, .. } if *kind == FragmentKind::Alt)
        );
        assert!(
            matches!(&diagram.events[1], SeqEvent::FragmentStart { kind, .. } if *kind == FragmentKind::Loop)
        );
        assert!(
            matches!(&diagram.events[2], SeqEvent::FragmentStart { kind, .. } if *kind == FragmentKind::Alt)
        );
        assert!(matches!(&diagram.events[4], SeqEvent::FragmentEnd)); // end inner alt
        assert!(matches!(&diagram.events[5], SeqEvent::FragmentEnd)); // end loop
        assert!(matches!(
            &diagram.events[6],
            SeqEvent::FragmentSeparator { .. }
        )); // else case2
        assert!(matches!(&diagram.events[8], SeqEvent::FragmentEnd)); // end outer alt
    }

    #[test]
    fn parse_pragma_teoz_true() {
        let src = "@startuml\n!pragma teoz true\nA -> B : msg\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();
        assert!(
            diagram.teoz_mode,
            "teoz_mode should be true when pragma teoz true is set"
        );
    }

    #[test]
    fn parse_no_pragma_teoz_defaults_false() {
        let src = "@startuml\nA -> B : msg\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();
        assert!(!diagram.teoz_mode, "teoz_mode should default to false");
    }

    #[test]
    fn parse_other_pragma_ignored() {
        let src = "@startuml\n!pragma graphviz_dot jdot\nA -> B : msg\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();
        assert!(
            !diagram.teoz_mode,
            "other pragmas should not enable teoz_mode"
        );
    }

    #[test]
    fn hide_footbox_parsed() {
        let src = "@startuml\nhide footbox\nA -> B : msg\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();
        assert!(diagram.hide_footbox, "hide footbox should be true");
    }

    #[test]
    fn hide_footbox_default_false() {
        let src = "@startuml\nA -> B : msg\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();
        assert!(!diagram.hide_footbox, "hide footbox defaults to false");
    }

    #[test]
    fn parallel_ampersand_prefix_not_create_participant() {
        // "& foo -> foo : test" should NOT create a participant named "& foo".
        // The "&" is a teoz parallel-message prefix — strip it.
        let src = "@startuml\n!pragma teoz true\nparticipant foo\nfoo -> foo : first\n& foo -> foo : parallel\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();
        assert_eq!(diagram.participants.len(), 1, "only 'foo', not '& foo'");
        assert_eq!(diagram.participants[0].name, "foo");
        assert_eq!(
            diagram
                .events
                .iter()
                .filter(|e| matches!(e, SeqEvent::Message(_)))
                .count(),
            2
        );
    }

    /// Parse `note right of PARTICIPANT : text` single-line syntax
    #[test]
    fn parse_note_right_of_participant() {
        let src = "@startuml\nBob -> Alice : hello\nnote right of Alice: standalone\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(matches!(&diagram.events[0], SeqEvent::Message(_)));
        if let SeqEvent::NoteRight {
            participant,
            text,
            on_message,
            ..
        } = &diagram.events[1]
        {
            assert_eq!(participant, "Alice");
            assert_eq!(text, "standalone");
            // `note right of Alice` is a *participant* note (Java `Note(p)`) —
            // occupies its own vertical slot via DrawableSetInitializer.prepareNote.
            assert!(
                !*on_message,
                "note right of Alice should be on_message=false"
            );
        } else {
            panic!("expected NoteRight, got {:?}", &diagram.events[1]);
        }
    }

    /// Parse `note left of PARTICIPANT : text` single-line syntax
    #[test]
    fn parse_note_left_of_participant() {
        let src = "@startuml\nBob -> Alice : hello\nnote left of Bob: remark\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(matches!(&diagram.events[0], SeqEvent::Message(_)));
        if let SeqEvent::NoteLeft {
            participant,
            text,
            on_message,
            ..
        } = &diagram.events[1]
        {
            assert_eq!(participant, "Bob");
            assert_eq!(text, "remark");
            assert!(!*on_message, "note left of Bob should be on_message=false");
        } else {
            panic!("expected NoteLeft, got {:?}", &diagram.events[1]);
        }
    }

    /// Parse multiline `note right of PARTICIPANT` (no colon, ends with `end note`)
    #[test]
    fn parse_multiline_note_right_of_participant() {
        let src =
            "@startuml\nBob -> Alice : hello\nnote right of Alice\nline1\nline2\nend note\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(matches!(&diagram.events[0], SeqEvent::Message(_)));
        if let SeqEvent::NoteRight {
            participant,
            text,
            on_message,
            ..
        } = &diagram.events[1]
        {
            assert_eq!(participant, "Alice");
            assert_eq!(text, "line1\nline2");
            assert!(
                !*on_message,
                "multiline `note right of Alice` should be on_message=false"
            );
        } else {
            panic!("expected NoteRight, got {:?}", &diagram.events[1]);
        }
    }

    /// A multiline `note right` with NO `of PARTICIPANT` clause is a *message*
    /// note (`on_message = true`), attached to the last arrow — the multiline
    /// counterpart of `note right: text`. This is the parser-level distinction
    /// that drives the layout slot assignment (issue #35).
    #[test]
    fn parse_multiline_note_right_without_of_is_message_note() {
        let src = "@startuml\nBob -> Alice : hello\nnote right\nline1\nline2\nend note\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();

        assert!(matches!(&diagram.events[0], SeqEvent::Message(_)));
        if let SeqEvent::NoteRight {
            participant,
            text,
            on_message,
            ..
        } = &diagram.events[1]
        {
            // No explicit `of`, so the participant falls back to the last
            // message's target (Alice) — but the note is still a *message* note.
            assert_eq!(participant, "Alice");
            assert_eq!(text, "line1\nline2");
            assert!(
                *on_message,
                "multiline `note right` (no `of`) should be on_message=true"
            );
        } else {
            panic!("expected NoteRight, got {:?}", &diagram.events[1]);
        }
    }

    /// Inline -- suffix deactivates the SOURCE, not the target.
    /// Java: `B -->> A-- : Data` deactivates B (the sender).
    #[test]
    fn inline_deactivate_targets_source() {
        let src = "@startuml\nactivate B\nB -->> A-- : Data\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();
        // Events: Activate(B), Message(B→A), Deactivate(B)
        assert!(matches!(&diagram.events[0], SeqEvent::Activate(ref n, _) if n == "B"));
        assert!(
            matches!(&diagram.events[1], SeqEvent::Message(ref m) if m.from == "B" && m.to == "A")
        );
        // The -- suffix deactivates the SOURCE (B), not the target (A)
        assert!(
            matches!(&diagram.events[2], SeqEvent::Deactivate(ref n) if n == "B"),
            "expected Deactivate(B) but got {:?}",
            &diagram.events[2]
        );
    }

    /// --++ suffix deactivates source and activates target.
    #[test]
    fn inline_deactivate_source_activate_target() {
        let src = "@startuml\nactivate A\nA ->> B --++ : msg\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();
        // Events: Activate(A), Message(A→B), Deactivate(A), Activate(B)
        assert!(matches!(&diagram.events[0], SeqEvent::Activate(ref n, _) if n == "A"));
        assert!(
            matches!(&diagram.events[1], SeqEvent::Message(ref m) if m.from == "A" && m.to == "B")
        );
        assert!(
            matches!(&diagram.events[2], SeqEvent::Deactivate(ref n) if n == "A"),
            "expected Deactivate(A) but got {:?}",
            &diagram.events[2]
        );
        assert!(matches!(&diagram.events[3], SeqEvent::Activate(ref n, _) if n == "B"));
    }

    /// ++-- suffix activates target and deactivates source.
    #[test]
    fn inline_activate_target_deactivate_source() {
        let src = "@startuml\nactivate A\nA -> B++-- : msg\n@enduml";
        let diagram = parse_sequence_diagram(src).unwrap();
        // Events: Activate(A), Message(A→B), Activate(B), Deactivate(A)
        assert!(matches!(&diagram.events[0], SeqEvent::Activate(ref n, _) if n == "A"));
        assert!(
            matches!(&diagram.events[1], SeqEvent::Message(ref m) if m.from == "A" && m.to == "B")
        );
        assert!(matches!(&diagram.events[2], SeqEvent::Activate(ref n, _) if n == "B"));
        assert!(
            matches!(&diagram.events[3], SeqEvent::Deactivate(ref n) if n == "A"),
            "expected Deactivate(A) but got {:?}",
            &diagram.events[3]
        );
    }
}
