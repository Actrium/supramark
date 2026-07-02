use std::collections::HashMap;

use super::DiagramHint;
use crate::model::DiagramMeta;

/// Detect special @start tags and return the determined diagram type
pub fn detect_start_tag(source: &str) -> Option<DiagramHint> {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("@startbpm") {
            return Some(DiagramHint::Bpm);
        }
        if trimmed.starts_with("@startchart") {
            return Some(DiagramHint::Chart);
        }
        if trimmed.starts_with("@startdef") {
            return Some(DiagramHint::Def);
        }
        if trimmed.starts_with("@startflow") {
            return Some(DiagramHint::Flow);
        }
        if trimmed.starts_with("@startchen") {
            return Some(DiagramHint::Erd);
        }
        if trimmed.starts_with("@startfiles") {
            return Some(DiagramHint::Files);
        }
        if trimmed.starts_with("@startjcckit") {
            return Some(DiagramHint::Jcckit);
        }
        if trimmed.starts_with("@startgantt") {
            return Some(DiagramHint::Gantt);
        }
        if trimmed.starts_with("@startproject") {
            return Some(DiagramHint::Project);
        }
        if trimmed.starts_with("@startditaa") {
            return Some(DiagramHint::Ditaa);
        }
        if trimmed.starts_with("@startjson") {
            return Some(DiagramHint::Json);
        }
        if trimmed.starts_with("@startmindmap") {
            return Some(DiagramHint::Mindmap);
        }
        if trimmed.starts_with("@startnwdiag") {
            return Some(DiagramHint::Nwdiag);
        }
        if trimmed.starts_with("@startsalt") {
            return Some(DiagramHint::Salt);
        }
        if trimmed.starts_with("@startwbs") {
            return Some(DiagramHint::Wbs);
        }
        if trimmed.starts_with("@startyaml") {
            return Some(DiagramHint::Yaml);
        }
        if trimmed.starts_with("@startdot") {
            return Some(DiagramHint::Dot);
        }
        if trimmed.starts_with("@startpacket") {
            return Some(DiagramHint::Packet);
        }
        if trimmed.starts_with("@startgit") {
            return Some(DiagramHint::Git);
        }
        if trimmed.starts_with("@startregex") {
            return Some(DiagramHint::Regex);
        }
        if trimmed.starts_with("@startebnf") {
            return Some(DiagramHint::Ebnf);
        }
        if trimmed.starts_with("@startpie") {
            return Some(DiagramHint::Pie);
        }
        if trimmed.starts_with("@startboard") {
            return Some(DiagramHint::Board);
        }
        if trimmed.starts_with("@startchronology") {
            return Some(DiagramHint::Chronology);
        }
        if trimmed.starts_with("@starthcl") {
            return Some(DiagramHint::Hcl);
        }
        if trimmed.starts_with("@startwire") {
            return Some(DiagramHint::Wire);
        }
        if trimmed.starts_with("@startmath") {
            return Some(DiagramHint::Math);
        }
        if trimmed.starts_with("@startlatex") {
            return Some(DiagramHint::Latex);
        }
        if trimmed.starts_with("@startcreole") {
            return Some(DiagramHint::Creole);
        }
        if trimmed.starts_with("@startuml") {
            // Skip @startuml — continue scanning for specialized @start tags inside
            continue;
        }
        if trimmed.starts_with("@start") {
            return None;
        }
    }
    None
}

/// Extract the content within @startuml/@enduml block from PlantUML text
pub fn extract_block(source: &str) -> Option<String> {
    let mut inside = false;
    let mut lines = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if inside {
            if trimmed.starts_with("@end") {
                break;
            }
            lines.push(line);
        } else if trimmed.starts_with("@startuml")
            || trimmed.starts_with("@startchen")
            || trimmed.starts_with("@startflow")
            || trimmed.starts_with("@startgantt")
            || trimmed.starts_with("@startjcckit")
            || trimmed.starts_with("@startproject")
            || trimmed.starts_with("@startditaa")
            || trimmed.starts_with("@startjson")
            || trimmed.starts_with("@startmindmap")
            || trimmed.starts_with("@startnwdiag")
            || trimmed.starts_with("@startsalt")
            || trimmed.starts_with("@startwbs")
            || trimmed.starts_with("@startyaml")
            || trimmed.starts_with("@startdot")
            || trimmed.starts_with("@startpacket")
            || trimmed.starts_with("@startgit")
            || trimmed.starts_with("@startpie")
            || trimmed.starts_with("@startboard")
            || trimmed.starts_with("@startchronology")
            || trimmed.starts_with("@starthcl")
            || trimmed.starts_with("@startwire")
            || trimmed.starts_with("@startmath")
            || trimmed.starts_with("@startlatex")
            || trimmed.starts_with("@startcreole")
        {
            inside = true;
            continue;
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Case-insensitive prefix check.
fn starts_with_ci(line: &str, keyword: &str) -> bool {
    line.len() >= keyword.len()
        && line.as_bytes()[..keyword.len()].eq_ignore_ascii_case(keyword.as_bytes())
}

/// Detect diagram type (heuristic detection for @startuml)
pub fn detect_diagram_type(content: &str) -> DiagramHint {
    let class_keywords = [
        "class ",
        "interface ",
        "abstract ",
        "enum ",
        "extends ",
        "implements ",
        "object ",
        "map ",
    ];

    let sequence_keywords_definitive = ["participant ", "boundary ", "control ", "collections "];
    let sequence_keywords_ambiguous = ["database ", "queue "];

    let seq_fragment_keywords = [
        "alt ",
        "else ",
        "loop ",
        "opt ",
        "par ",
        "break",
        "critical",
        "ref over ",
        "group ",
    ];

    let mut has_seq_actor = false;
    let mut has_seq_ambiguous_role = false;
    let mut has_activity_action = false;
    let mut has_activity_start_stop = false;
    let mut has_activity_swimlane = false;
    let mut has_activity_old = false;
    let mut has_state_keyword = false;
    let mut has_component_keyword_definitive = false;
    let mut has_component_keyword_ambiguous = false;
    let mut has_port_keyword = false;
    let mut has_component_brace_only = true; // true if all component keywords have brace bodies
    let mut has_usecase_keyword = false;
    let mut has_salt_keyword = false;
    let mut has_timing_keyword = false;
    let mut has_arrow = false;
    let mut has_seq_arrow = false;
    let mut has_seq_fragment = false;
    let mut has_seq_lifecycle = false;
    let mut has_class_kw = false;
    let mut has_class_relation = false;
    let mut in_bracket_display = false;

    let mut in_note_block = false;
    let mut in_style_block = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('\'') {
            continue;
        }

        // Skip content inside <style> blocks: CSS-like selectors like
        // `note {` should not be mistaken for PlantUML note blocks.
        if in_style_block {
            if trimmed == "</style>" {
                in_style_block = false;
            }
            continue;
        }
        if trimmed == "<style>" {
            in_style_block = true;
            continue;
        }

        // Skip multi-line note blocks: content inside notes should not affect diagram type
        if in_note_block {
            if trimmed == "end note" || trimmed == "endnote" {
                in_note_block = false;
            }
            continue;
        }
        if trimmed.starts_with("note ") && !trimmed.contains(':') {
            in_note_block = true;
            continue;
        }
        // Skip single-line note: `note ... : text`
        if trimmed.starts_with("note ") && trimmed.contains(':') {
            continue;
        }

        // Skip content inside bracket-display blocks: `name [\n...\n]`
        if in_bracket_display {
            // Close bracket display when line is exactly "]" — NOT when it merely
            // ends with "]" since `]]` (link close) would falsely terminate the block.
            if trimmed == "]" {
                in_bracket_display = false;
            }
            continue;
        }
        // Detect bracket-display opener (e.g. `component C [`, `file f [`)
        // Java compat: `rectangle [` alone → CLASS diagram (not COMPONENT).
        // Only non-rectangle bracket-display triggers definitive COMPONENT.
        if (trimmed.ends_with(" [") || trimmed.ends_with('[')) && !trimmed.starts_with('[') {
            let before = trimmed[..trimmed.len() - 1].trim();
            if !before.is_empty() && !before.ends_with('-') && !before.ends_with('<') {
                in_bracket_display = true;
                // `rectangle ... [` and `file ... [` are ambiguous (Java treats as CLASS);
                // only `component`, `node`, etc. are definitive COMPONENT indicators.
                let lower_before = before.to_lowercase();
                let is_class_compatible_bracket = lower_before.starts_with("rectangle ")
                    || lower_before.starts_with("file ")
                    || lower_before.starts_with("folder ")
                    || lower_before.starts_with("frame ")
                    || lower_before.starts_with("card ")
                    || lower_before.starts_with("package ");
                if !is_class_compatible_bracket {
                    has_component_keyword_definitive = true;
                }
            }
        }

        // Activity: `:action;`
        if trimmed.starts_with(':') && trimmed.ends_with(';') && trimmed.len() > 2 {
            has_activity_action = true;
        }
        if matches!(trimmed, "start" | "stop") {
            has_activity_start_stop = true;
        }
        // Activity swimlane: `|Name|` (no internal `|` -- Creole tables have them)
        if trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 2 {
            let inner = &trimmed[1..trimmed.len() - 1];
            if !inner.contains('|') {
                has_activity_swimlane = true;
            }
        }
        // Old activity: `(*)` start/end
        if trimmed.contains("(*)") {
            has_activity_old = true;
        }
        // Old activity: `if "..." then` / `endif`
        if (trimmed.starts_with("if ") && trimmed.contains(" then")) || trimmed == "endif" {
            has_activity_old = true;
        }
        // Old activity synchbar: `===NAME===`
        if trimmed.starts_with("===") && trimmed.ends_with("===") && trimmed.len() > 6 {
            has_activity_old = true;
        }

        // Sequence fragment keywords
        for kw in &seq_fragment_keywords {
            if trimmed.starts_with(kw) || trimmed == kw.trim() {
                has_seq_fragment = true;
            }
        }

        // Sequence lifecycle (case-insensitive)
        if starts_with_ci(trimmed, "activate ")
            || starts_with_ci(trimmed, "deactivate ")
            || starts_with_ci(trimmed, "destroy ")
            || starts_with_ci(trimmed, "create ")
            || starts_with_ci(trimmed, "autoactivate ")
            || trimmed == "activate"
            || trimmed == "deactivate"
        {
            has_seq_lifecycle = true;
        }

        // Sequence gate: `[-> target` or `[<- target`
        if trimmed.starts_with("[->") || trimmed.starts_with("[<-") {
            has_seq_arrow = true;
            has_arrow = true;
            continue;
        }

        // State
        if trimmed.starts_with("state ") {
            has_state_keyword = true;
        }
        if trimmed.contains("[*]") {
            has_state_keyword = true;
        }

        // Component / deployment keywords
        let is_bracket_opener = in_bracket_display;
        let is_component_kw = trimmed.starts_with("component ")
            || trimmed.starts_with("node ")
            || trimmed.starts_with("cloud ")
            || trimmed.starts_with("card ")
            || (trimmed.starts_with("file ") && !is_bracket_opener)
            || trimmed.starts_with("artifact ")
            || trimmed.starts_with("storage ")
            || trimmed.starts_with("folder ")
            || trimmed.starts_with("frame ")
            || trimmed.starts_with("agent ")
            || trimmed.starts_with("archimate ")
            || trimmed.starts_with("archimate ")
            || trimmed.starts_with("stack ");
        if is_component_kw {
            has_component_keyword_definitive = true;
            // Track whether ALL component keywords have brace bodies.
            // Standalone `component A` (no brace) stays DESCRIPTION;
            // `component A {` (brace body) may go to CLASS.
            if !(trimmed.ends_with('{') || trimmed.ends_with("{}")) {
                has_component_brace_only = false;
            }
        }
        // Port keywords: only present in DESCRIPTION/component diagrams
        if trimmed.starts_with("portin ")
            || trimmed.starts_with("portout ")
            || trimmed == "portin"
            || trimmed == "portout"
        {
            has_port_keyword = true;
        }
        if trimmed.starts_with('[')
            && !trimmed.starts_with("[->")
            && !trimmed.starts_with("[<-")
            && !is_bracket_opener
        {
            has_component_keyword_definitive = true;
        }
        if trimmed.starts_with("rectangle ")
            || trimmed.starts_with("package ")
            || trimmed.starts_with("file ")
            || trimmed.starts_with("folder ")
            || trimmed.starts_with("frame ")
            || trimmed.starts_with("card ")
        {
            has_component_keyword_ambiguous = true;
        }
        // `rectangle ... as <alias>` is unambiguously component/deployment
        // (e.g. C4 macro expansion).
        if trimmed.starts_with("rectangle ") && trimmed.contains(" as ") {
            has_component_keyword_definitive = true;
        }

        if trimmed == "salt" {
            has_salt_keyword = true;
        }

        // Use case
        if trimmed.starts_with("usecase ") || trimmed.starts_with("usecase\"") {
            has_usecase_keyword = true;
        }
        if trimmed.starts_with('(')
            && trimmed.contains(')')
            && !trimmed.starts_with("()")
            && !trimmed.contains("(*)")
        {
            has_usecase_keyword = true;
        }

        // Timing
        if trimmed.starts_with("robust ") || trimmed.starts_with("concise ") {
            has_timing_keyword = true;
        }

        // Class keywords
        for kw in &class_keywords {
            let check = trimmed.strip_prefix('-').unwrap_or(trimmed);
            if check.starts_with(kw) || trimmed.contains(&format!(" {}", kw.trim())) {
                has_class_kw = true;
            }
        }

        // Definitive sequence keywords (case-insensitive)
        for kw in &sequence_keywords_definitive {
            if starts_with_ci(trimmed, kw) {
                return DiagramHint::Sequence;
            }
        }
        if starts_with_ci(trimmed, "actor ") {
            has_seq_actor = true;
        }
        for kw in &sequence_keywords_ambiguous {
            if trimmed.starts_with(kw) {
                has_seq_ambiguous_role = true;
            }
        }

        // Arrow detection
        if trimmed.contains("->") || trimmed.contains("<-") {
            has_arrow = true;
            if let Some(pos) = trimmed.find("->").or_else(|| trimmed.find("<-")) {
                let before = trimmed[..pos].trim();
                let after_arrow = &trimmed[pos + 2..];
                let after = after_arrow.trim_start_matches(['>', '-']);
                let after = after.trim();
                let before_is_dashes = !before.is_empty() && before.chars().all(|c| c == '-');
                let after_is_label = after.starts_with('[') && after.contains(']');
                if !before.is_empty()
                    && !before_is_dashes
                    && !before.starts_with(':')
                    && !before.starts_with("(*")
                    && !before.starts_with("===")
                    && !after.is_empty()
                    && !after.starts_with(';')
                    && !after.starts_with("(*")
                    && !after.starts_with("===")
                    && !after_is_label
                {
                    has_seq_arrow = true;
                }
            }
        }

        // Class relations
        if trimmed.contains("<|")
            || trimmed.contains("|>")
            || trimmed.contains(" o--")
            || trimmed.contains("--o")
            || trimmed.contains(" *--")
            || trimmed.contains("--*")
            || trimmed.contains(" +--")
            || trimmed.contains("--+")
            || trimmed.contains(" o..")
            || trimmed.contains("..o")
            || trimmed.contains(" *..")
            || trimmed.contains("..*")
            || trimmed.contains(" +..")
            || trimmed.contains("..+")
        {
            has_class_relation = true;
        }
        if trimmed.contains('[')
            && trimmed.contains(']')
            && (trimmed.contains("--") || trimmed.contains("..") || trimmed.contains("->"))
        {
            has_class_relation = true;
        }
    }

    // Priority resolution

    if has_timing_keyword {
        return DiagramHint::Timing;
    }
    if has_salt_keyword {
        return DiagramHint::Salt;
    }
    if has_state_keyword {
        return DiagramHint::State;
    }

    // Sequence: lifecycle (activate/deactivate) is unambiguous
    if has_seq_lifecycle {
        return DiagramHint::Sequence;
    }
    if has_seq_fragment && !has_component_keyword_definitive && !has_activity_old {
        return DiagramHint::Sequence;
    }

    // Use case -- but (*) is old-activity, not use-case
    if has_usecase_keyword && !has_activity_old {
        return DiagramHint::UseCase;
    }
    // Activity (new syntax).  Swimlane-like lines are ambiguous: single-cell
    // Creole table rows inside bracket-display bodies look identical.  When
    // component keywords are present, do not let swimlane alone claim Activity.
    if has_activity_action || has_activity_start_stop {
        return DiagramHint::Activity;
    }
    if has_activity_swimlane
        && !has_component_keyword_definitive
        && !has_component_keyword_ambiguous
    {
        return DiagramHint::Activity;
    }

    // Activity (old syntax)
    if has_activity_old {
        return DiagramHint::Activity;
    }

    // Component: only when no class keywords override.
    // When all component keywords have brace bodies and there are no arrows
    // or ports, Java's ClassDiagramFactory takes priority (it can fully parse
    // `component a {}` as a class entity). Otherwise DESCRIPTION wins.
    if has_component_keyword_definitive && !has_class_kw {
        if has_component_brace_only && !has_arrow && !has_port_keyword {
            return DiagramHint::Class;
        }
        return DiagramHint::Component;
    }
    // Sequence arrows override plain class associations when no class keywords
    // are present — BUT a class-exclusive relation (`*--`, `o--`, `<|`, `|>`,
    // …) unambiguously marks a class diagram, so it must win over the
    // ambiguous `-->`/`->` arrow.  Without this guard, `Class01 *-- Class02`
    // combined with `Class05 --> Class06` was misdetected as a sequence
    // diagram (see issue #29).
    if has_seq_arrow && !has_class_kw && !has_class_relation {
        return DiagramHint::Sequence;
    }
    if has_class_kw || has_class_relation {
        return DiagramHint::Class;
    }
    if has_seq_actor {
        return DiagramHint::Sequence;
    }
    if has_seq_arrow {
        return DiagramHint::Sequence;
    }
    if has_seq_ambiguous_role {
        return DiagramHint::Sequence;
    }
    if has_component_keyword_definitive {
        return DiagramHint::Component;
    }
    // Ambiguous keywords (rectangle, package) alone -> Class
    if has_component_keyword_ambiguous {
        return DiagramHint::Class;
    }
    if has_arrow {
        return DiagramHint::Sequence;
    }

    DiagramHint::Unknown("unknown".into())
}

/// Return `true` when the `@startuml` body contains actual diagram content,
/// excluding metadata and cosmetic directives.
pub fn has_meaningful_uml_content(content: &str) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    let mut in_style_block = false;
    let mut skinparam_depth = 0usize;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if in_style_block {
            if trimmed == "</style>" {
                in_style_block = false;
            }
            i += 1;
            continue;
        }

        if skinparam_depth > 0 {
            skinparam_depth = skinparam_depth
                .saturating_add(trimmed.matches('{').count())
                .saturating_sub(trimmed.matches('}').count());
            i += 1;
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with('\'') {
            i += 1;
            continue;
        }

        match trimmed {
            "title" => {
                if let Some((_, end)) = collect_block(&lines, None, i + 1, "end title", "endtitle")
                {
                    i = end + 1;
                } else {
                    i += 1;
                }
                continue;
            }
            "header" => {
                if let Some((_, end)) =
                    collect_block(&lines, None, i + 1, "end header", "endheader")
                {
                    i = end + 1;
                } else {
                    i += 1;
                }
                continue;
            }
            "footer" => {
                if let Some((_, end)) =
                    collect_block(&lines, None, i + 1, "end footer", "endfooter")
                {
                    i = end + 1;
                } else {
                    i += 1;
                }
                continue;
            }
            "legend" => {
                if let Some((_, end)) =
                    collect_block(&lines, None, i + 1, "end legend", "endlegend")
                {
                    i = end + 1;
                } else {
                    i += 1;
                }
                continue;
            }
            "left to right direction" | "top to bottom direction" => {
                i += 1;
                continue;
            }
            "<style>" => {
                in_style_block = true;
                i += 1;
                continue;
            }
            _ => {}
        }

        if trimmed.starts_with("title ")
            || trimmed.starts_with("header ")
            || trimmed.starts_with("footer ")
            || trimmed.starts_with("caption ")
            || trimmed.starts_with("legend ")
            || trimmed.starts_with("hide ")
            || trimmed.starts_with("show ")
            || trimmed.starts_with("scale ")
            || trimmed.starts_with('!')  // preprocessor directives (!pragma, !define, !include, etc.)
            || trimmed.starts_with("sprite ")
        // sprite definitions
        {
            // For multi-line sprites, skip the SVG body
            if trimmed.starts_with("sprite ") && trimmed.contains("<svg") {
                // Skip until closing </svg> tag
                while i < lines.len() && !lines[i].contains("</svg>") {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }

        if trimmed.starts_with("<style>") {
            if !trimmed.contains("</style>") {
                in_style_block = true;
            }
            i += 1;
            continue;
        }

        if trimmed.starts_with("skinparam ") {
            skinparam_depth = trimmed
                .matches('{')
                .count()
                .saturating_sub(trimmed.matches('}').count());
            i += 1;
            continue;
        }

        return true;
    }

    false
}

/// Extract meta information (title / header / footer / legend / caption) from PlantUML source.
///
/// Supports both single-line and multi-line syntax:
/// - Single-line: `title My Title`
/// - Multi-line: `title\n...\nend title`
pub fn parse_meta(source: &str) -> DiagramMeta {
    parse_meta_with_original(source, None)
}

pub fn parse_meta_with_original(source: &str, original_source: Option<&str>) -> DiagramMeta {
    let mut meta = DiagramMeta::default();
    let lines: Vec<&str> = source.lines().collect();
    let original_lines: Option<Vec<&str>> = original_source.map(|src| src.lines().collect());
    let mut i = 0;
    let mut in_style_block = false;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Skip <style> blocks — they may contain selectors like "title {" that
        // must NOT be interpreted as diagram meta declarations.
        if trimmed.starts_with("<style>") || trimmed == "<style>" {
            if !trimmed.contains("</style>") {
                in_style_block = true;
            }
            i += 1;
            continue;
        }
        if in_style_block {
            if trimmed.contains("</style>") {
                in_style_block = false;
            }
            i += 1;
            continue;
        }

        // title
        if trimmed == "title" {
            if let Some((block, end)) = collect_block(
                &lines,
                original_lines.as_deref(),
                i + 1,
                "end title",
                "endtitle",
            ) {
                meta.title = Some(block);
                i = end + 1;
                continue;
            }
        } else if let Some(rest) = trimmed.strip_prefix("title ") {
            let rest = rest.trim();
            if !rest.is_empty() {
                meta.title = Some(rest.to_string());
            }
        }

        // header
        if trimmed == "header" {
            if let Some((block, end)) = collect_block(
                &lines,
                original_lines.as_deref(),
                i + 1,
                "end header",
                "endheader",
            ) {
                meta.header = Some(block);
                i = end + 1;
                continue;
            }
        } else if let Some(rest) = trimmed.strip_prefix("header ") {
            let rest = rest.trim();
            if !rest.is_empty() {
                meta.header = Some(rest.to_string());
            }
        }

        // footer
        if trimmed == "footer" {
            if let Some((block, end)) = collect_block(
                &lines,
                original_lines.as_deref(),
                i + 1,
                "end footer",
                "endfooter",
            ) {
                meta.footer = Some(block);
                i = end + 1;
                continue;
            }
        } else if let Some(rest) = trimmed.strip_prefix("footer ") {
            let rest = rest.trim();
            if !rest.is_empty() {
                meta.footer = Some(rest.to_string());
            }
        }

        // legend
        if trimmed == "legend" || trimmed.starts_with("legend ") {
            if let Some((block, end)) = collect_block(
                &lines,
                original_lines.as_deref(),
                i + 1,
                "end legend",
                "endlegend",
            ) {
                meta.legend = Some(block);
                i = end + 1;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("legend ") {
                let rest = rest.trim();
                if !rest.is_empty() {
                    meta.legend = Some(rest.to_string());
                }
            }
        }

        // caption
        if let Some(rest) = trimmed.strip_prefix("caption ") {
            let rest = rest.trim();
            if !rest.is_empty() {
                meta.caption = Some(rest.to_string());
            }
        }

        // pragma
        if let Some(rest) = trimmed.strip_prefix("!pragma ") {
            let rest = rest.trim();
            if let Some((key, val)) = rest.split_once(char::is_whitespace) {
                meta.pragmas.insert(key.to_string(), val.trim().to_string());
            }
        }

        i += 1;
    }

    meta
}

/// Collect a multi-line block from lines[start_idx..] until end_marker or end_marker_alt is found.
fn collect_block(
    lines: &[&str],
    original_lines: Option<&[&str]>,
    start_idx: usize,
    end_marker: &str,
    end_marker_alt: &str,
) -> Option<(String, usize)> {
    let mut collected = Vec::new();
    for (offset, line) in lines[start_idx..].iter().enumerate() {
        let t = line.trim();
        if t.eq_ignore_ascii_case(end_marker) || t.eq_ignore_ascii_case(end_marker_alt) {
            return Some((collected.join("\n"), start_idx + offset));
        }
        if t.is_empty()
            && original_lines
                .and_then(|orig| orig.get(start_idx + offset))
                .map(|orig| orig.trim_start().starts_with('\''))
                .unwrap_or(false)
        {
            continue;
        }
        collected.push(t);
    }
    None
}

/// Extract SVG sprite definitions from source text.
///
/// Parses `sprite NAME <svg ...>...</svg>` blocks (single- or multi-line)
/// and hex-encoded sprite definitions `sprite $NAME [WxH/depth] { hex_rows }`.
/// Returns a map of sprite name -> SVG content, plus the cleaned source
/// with sprite definitions removed.
pub fn extract_sprites(
    source: &str,
) -> (
    String,
    HashMap<String, String>,
    HashMap<String, SpriteGrayData>,
) {
    let mut sprites = HashMap::new();
    let mut gray_data_map: HashMap<String, SpriteGrayData> = HashMap::new();
    let mut cleaned = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Match: sprite [optional $]NAME ...
        if let Some(rest) = trimmed.strip_prefix("sprite ") {
            let rest = rest.trim();
            // Strip optional leading $
            let rest = rest.strip_prefix('$').unwrap_or(rest);

            // Try SVG format: sprite NAME <svg ...>...</svg>
            if let Some(svg_start) = rest.find("<svg") {
                let name = rest[..svg_start].trim().to_string();
                if !name.is_empty() {
                    // Collect SVG content (may span multiple lines)
                    let mut svg_buf = rest[svg_start..].to_string();
                    if svg_buf.contains("</svg>") {
                        // Single-line sprite: replace with blank to preserve line numbering
                        sprites.insert(name, svg_buf);
                        cleaned.push("");
                        i += 1;
                        continue;
                    }
                    // Multi-line: accumulate until </svg>, replace all lines with blanks
                    cleaned.push(""); // first sprite line
                    i += 1;
                    while i < lines.len() {
                        svg_buf.push('\n');
                        svg_buf.push_str(lines[i]);
                        cleaned.push(""); // continuation line
                        if lines[i].contains("</svg>") {
                            break;
                        }
                        i += 1;
                    }
                    sprites.insert(name, svg_buf);
                    i += 1;
                    continue;
                }
            }

            // Try hex format: sprite $NAME [WxH/depth] { hex_rows }
            if let Some((name, width, height, depth, header_has_brace)) =
                parse_hex_sprite_header(rest)
            {
                cleaned.push(""); // sprite header line
                i += 1;

                // If the header line didn't contain '{', look for it on the next line
                let mut found_brace = header_has_brace;
                if !found_brace {
                    while i < lines.len() {
                        let t = lines[i].trim();
                        cleaned.push("");
                        i += 1;
                        if t == "{" || t.starts_with('{') {
                            found_brace = true;
                            break;
                        }
                    }
                }

                if found_brace {
                    // Collect hex rows until '}'
                    let mut hex_rows: Vec<String> = Vec::new();
                    while i < lines.len() {
                        let t = lines[i].trim();
                        cleaned.push("");
                        if t == "}" || t.starts_with('}') {
                            i += 1;
                            break;
                        }
                        hex_rows.push(t.to_string());
                        i += 1;
                    }

                    // Convert hex data to PNG and wrap in SVG
                    if let Some((svg, gdata)) = hex_sprite_to_svg(&hex_rows, width, height, depth) {
                        sprites.insert(name.clone(), svg);
                        gray_data_map.insert(name, gdata);
                    }
                }
                continue;
            }
        }

        cleaned.push(lines[i]);
        i += 1;
    }

    (cleaned.join("\n"), sprites, gray_data_map)
}

/// Parse a hex sprite header line like `NAME [WxH/depth] {`
/// Returns (name, width, height, depth, has_opening_brace).
fn parse_hex_sprite_header(rest: &str) -> Option<(String, usize, usize, usize, bool)> {
    // rest is after "sprite " and optional "$", e.g.:
    //   "businessProcess [16x16/16] {"
    //   "myIcon [32x32/4]"
    let bracket_start = rest.find('[')?;
    let name = rest[..bracket_start].trim().to_string();
    if name.is_empty() {
        return None;
    }
    let bracket_end = rest.find(']')?;
    let dims = &rest[bracket_start + 1..bracket_end];

    // Parse "WxH" or "WxH/depth"
    let (wh, depth_str) = if let Some(slash) = dims.find('/') {
        (&dims[..slash], &dims[slash + 1..])
    } else {
        (dims, "16") // default 16 gray levels
    };
    let x_pos = wh.find('x')?;
    let width: usize = wh[..x_pos].trim().parse().ok()?;
    let height: usize = wh[x_pos + 1..].trim().parse().ok()?;
    let depth: usize = depth_str.trim().parse().ok()?;

    let has_brace = rest[bracket_end + 1..].contains('{');
    Some((name, width, height, depth, has_brace))
}

/// Raw gray-level data for a monochrome sprite, stored for deferred PNG generation.
/// Java re-renders sprites at draw time with context-dependent background colors
/// (e.g. entity fill color), so we keep the raw data for on-demand PNG generation.
#[derive(Clone, Debug)]
pub struct SpriteGrayData {
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    /// Flat row-major array of gray levels, each in `0..depth`.
    pub gray: Vec<u8>,
    /// Pre-computed max coefficient (max gray / (depth-1)) for alpha scaling.
    pub max_coef: f64,
}

/// Convert hex sprite data to an SVG string wrapping a base64-encoded PNG image.
///
/// Java PlantUML renders monochrome sprites as raster PNG images encoded inline.
/// The hex data encodes grayscale pixels: each hex character represents one pixel
/// with a gray level from 0 (foreground/black) to depth-1 (background/transparent).
fn hex_sprite_to_svg(
    hex_rows: &[String],
    width: usize,
    height: usize,
    depth: usize,
) -> Option<(String, SpriteGrayData)> {
    let gray_data = parse_sprite_gray(hex_rows, width, height, depth);
    let data_uri = sprite_gray_to_data_uri(&gray_data, 255, 255, 255)?;

    // Wrap in SVG container (Java format: <svg> with viewBox, containing <image>)
    let svg = format!(
        r#"<svg viewBox="0 0 {width} {height}"><image width="{width}" height="{height}" xlink:href="{data_uri}"/></svg>"#,
    );
    Some((svg, gray_data))
}

/// Parse hex sprite rows into a `SpriteGrayData` structure.
fn parse_sprite_gray(
    hex_rows: &[String],
    width: usize,
    height: usize,
    depth: usize,
) -> SpriteGrayData {
    let mut gray = Vec::with_capacity(width * height);
    for row in hex_rows {
        let mut count = 0;
        for ch in row.chars() {
            if let Some(val) = ch.to_digit(16) {
                gray.push(val as u8);
                count += 1;
            }
        }
        // Pad to width
        while count < width {
            gray.push((depth - 1) as u8);
            count += 1;
        }
    }
    // Pad missing rows
    while gray.len() < width * height {
        gray.push((depth - 1) as u8);
    }

    let max_level = (depth - 1) as f64;
    let mut max_coef: f64 = 0.0;
    for &g in &gray {
        let coef = g as f64 / max_level;
        if coef > max_coef {
            max_coef = coef;
        }
    }
    if max_coef == 0.0 {
        max_coef = 1.0;
    }

    SpriteGrayData {
        width,
        height,
        depth,
        gray,
        max_coef,
    }
}

/// Generate a PNG data URI from sprite gray data with a custom background color.
///
/// Java's `SpriteMonochrome.toUImage` uses a gradient from `backcolor` to `forecolor`
/// (black). The background color comes from the current drawing context (e.g. entity
/// fill color `#F1F1F1`), which affects the RGB of transparent pixels.
pub fn sprite_gray_to_data_uri(
    data: &SpriteGrayData,
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
) -> Option<String> {
    sprite_gray_to_data_uri_scaled(data, bg_r, bg_g, bg_b, data.width, data.height)
}

/// Encode a sprite as a PNG data URI, optionally scaling from the raw sprite
/// dimensions (`data.width` × `data.height`) to `out_w` × `out_h`.
///
/// When `out_w == data.width && out_h == data.height` the encoding is
/// pixel-for-pixel identical to the raw sprite. Otherwise the raw grayscale
/// sprite is resampled with bilinear interpolation — matching Java PlantUML's
/// `SpriteMonochrome.toUImage`, which draws the sprite with an AWT
/// `AffineTransform` so that the encoded PNG dimensions match the `<image>`
/// wrapper. Without this, C4-style entity sprites (48×48 raw, rendered at
/// 52×52) ship with a 48×48 PNG inside a 52×52 `<image>` — the browser
/// upscales at render time but byte parity with the Java reference is lost.
pub fn sprite_gray_to_data_uri_scaled(
    data: &SpriteGrayData,
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
    out_w: usize,
    out_h: usize,
) -> Option<String> {
    if out_w == 0 || out_h == 0 {
        return None;
    }
    let max_level = (data.depth - 1) as f64;
    let mut rgba = Vec::with_capacity(out_w * out_h * 4);
    let sample_gray = |gx: f64, gy: f64| -> f64 {
        // Bilinear resample into the raw gray grid. We use the AWT convention:
        // source coordinates map 1:1 at the image edges, i.e.
        // `src_x = dst_x * (src_w / dst_w)`. Out-of-range samples (when
        // `out_w == src_w`) fall back to exact integer lookup.
        let sx1 = gx.floor() as isize;
        let sy1 = gy.floor() as isize;
        let sx2 = sx1 + 1;
        let sy2 = sy1 + 1;
        let fx = gx - sx1 as f64;
        let fy = gy - sy1 as f64;
        let get = |x: isize, y: isize| -> f64 {
            if x < 0 || y < 0 || x >= data.width as isize || y >= data.height as isize {
                return 0.0;
            }
            data.gray[(y as usize) * data.width + (x as usize)] as f64
        };
        let v00 = get(sx1, sy1);
        let v10 = get(sx2, sy1);
        let v01 = get(sx1, sy2);
        let v11 = get(sx2, sy2);
        (1.0 - fy) * ((1.0 - fx) * v00 + fx * v10) + fy * ((1.0 - fx) * v01 + fx * v11)
    };

    let scale_x = data.width as f64 / out_w as f64;
    let scale_y = data.height as f64 / out_h as f64;
    let same_size = out_w == data.width && out_h == data.height;

    for oy in 0..out_h {
        for ox in 0..out_w {
            let g = if same_size {
                data.gray[oy * data.width + ox] as f64
            } else {
                // Map output pixel center to the input image.
                let gx = (ox as f64 + 0.5) * scale_x - 0.5;
                let gy = (oy as f64 + 0.5) * scale_y - 0.5;
                sample_gray(gx, gy)
            };
            let coef = g / max_level;
            // Color: interpolate between background and black (foreground)
            // coef=0 → background, coef=1 → foreground (black)
            let r = ((1.0 - coef) * bg_r as f64) as u8;
            let green = ((1.0 - coef) * bg_g as f64) as u8;
            let b = ((1.0 - coef) * bg_b as f64) as u8;
            // Alpha: fully opaque when coef > maxCoef/4, otherwise proportional
            let alpha = if coef > data.max_coef / 4.0 {
                255u8
            } else {
                (255.0 * (coef * 4.0 / data.max_coef)) as u8
            };
            rgba.push(r);
            rgba.push(green);
            rgba.push(b);
            rgba.push(alpha);
        }
    }

    let png_data = encode_png_rgba(out_w, out_h, &rgba)?;

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_data);
    Some(format!("data:image/png;base64,{b64}"))
}

/// Minimal PNG encoder for RGBA pixel data.
fn encode_png_rgba(width: usize, height: usize, rgba: &[u8]) -> Option<Vec<u8>> {
    if rgba.len() != width * height * 4 {
        return None;
    }

    let mut buf = Vec::new();

    // PNG signature
    buf.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    // IHDR chunk
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type: RGBA
    ihdr.push(0); // compression method
    ihdr.push(0); // filter method
    ihdr.push(0); // interlace method
    write_png_chunk(&mut buf, b"IHDR", &ihdr);

    // IDAT chunk: raw pixel data with filter byte per row, deflate-compressed
    let mut raw_data = Vec::new();
    for y in 0..height {
        raw_data.push(0); // filter type: None
        let row_start = y * width * 4;
        let row_end = row_start + width * 4;
        raw_data.extend_from_slice(&rgba[row_start..row_end]);
    }

    // Compress with deflate (zlib format)
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;
    // Java's ImageIO PNG encoder uses zlib compression level producing FLEVEL=1 (fast).
    // flate2/miniz_oxide: level 1 produces the closest match to Java's zlib level 4.
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&raw_data).ok()?;
    let compressed = encoder.finish().ok()?;
    write_png_chunk(&mut buf, b"IDAT", &compressed);

    // IEND chunk
    write_png_chunk(&mut buf, b"IEND", &[]);

    Some(buf)
}

fn write_png_chunk(buf: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buf.extend_from_slice(chunk_type);
    buf.extend_from_slice(data);
    // CRC32 over type + data
    let crc = crc32(chunk_type, data);
    buf.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(chunk_type: &[u8], data: &[u8]) -> u32 {
    // Standard CRC32 with PNG polynomial
    static CRC_TABLE: std::sync::LazyLock<[u32; 256]> = std::sync::LazyLock::new(|| {
        let mut table = [0u32; 256];
        for n in 0..256u32 {
            let mut c = n;
            for _ in 0..8 {
                if c & 1 != 0 {
                    c = 0xEDB88320 ^ (c >> 1);
                } else {
                    c >>= 1;
                }
            }
            table[n as usize] = c;
        }
        table
    });

    let mut crc = 0xFFFF_FFFFu32;
    for &byte in chunk_type.iter().chain(data.iter()) {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = CRC_TABLE[index] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_block_basic() {
        let src = "@startuml\nclass Foo {}\n@enduml\n";
        let block = extract_block(src).unwrap();
        assert_eq!(block, "class Foo {}");
    }

    #[test]
    fn extract_block_with_name() {
        let src = "@startuml myDiagram\nclass Foo {}\n@enduml\n";
        let block = extract_block(src).unwrap();
        assert_eq!(block, "class Foo {}");
    }

    #[test]
    fn extract_block_none_when_empty() {
        let src = "no startuml here";
        assert!(extract_block(src).is_none());
    }

    #[test]
    fn extract_block_chen() {
        let src = "@startchen movies\nentity Foo {}\n@endchen\n";
        let block = extract_block(src).unwrap();
        assert_eq!(block, "entity Foo {}");
    }

    #[test]
    fn extract_block_gantt() {
        let src = "@startgantt\n[Task] lasts 5 days\n@endgantt\n";
        let block = extract_block(src).unwrap();
        assert_eq!(block, "[Task] lasts 5 days");
    }

    #[test]
    fn extract_block_json() {
        let src = "@startjson\n{\"a\": 1}\n@endjson\n";
        let block = extract_block(src).unwrap();
        assert_eq!(block, "{\"a\": 1}");
    }

    #[test]
    fn extract_block_mindmap() {
        let src = "@startmindmap\n* root\n@endmindmap\n";
        let block = extract_block(src).unwrap();
        assert_eq!(block, "* root");
    }

    #[test]
    fn extract_block_wbs() {
        let src = "@startwbs\n* root\n@endwbs\n";
        let block = extract_block(src).unwrap();
        assert_eq!(block, "* root");
    }

    #[test]
    fn extract_block_yaml() {
        let src = "@startyaml\nkey: value\n@endyaml\n";
        let block = extract_block(src).unwrap();
        assert_eq!(block, "key: value");
    }

    #[test]
    fn detect_class_diagram() {
        let content = "class Foo {}\n";
        assert!(matches!(detect_diagram_type(content), DiagramHint::Class));
    }

    #[test]
    fn detect_unknown_diagram() {
        let content = "something else\n";
        assert!(matches!(
            detect_diagram_type(content),
            DiagramHint::Unknown(_)
        ));
    }

    #[test]
    fn detect_sequence_by_participant() {
        let content = "participant Alice\nAlice -> Bob : Hello\n";
        assert!(matches!(
            detect_diagram_type(content),
            DiagramHint::Sequence
        ));
    }

    #[test]
    fn detect_sequence_by_arrow() {
        let content = "Alice -> Bob : Hello\n";
        assert!(matches!(
            detect_diagram_type(content),
            DiagramHint::Sequence
        ));
    }

    #[test]
    fn detect_activity_by_action() {
        let content = ":foo;\nstop\n";
        assert!(matches!(
            detect_diagram_type(content),
            DiagramHint::Activity
        ));
    }

    #[test]
    fn detect_activity_by_swimlane() {
        let content = "|Actor 1|\nstart\n:foo;\n";
        assert!(matches!(
            detect_diagram_type(content),
            DiagramHint::Activity
        ));
    }

    #[test]
    fn detect_state_by_keyword() {
        let content = "state s1\n[*] --> s1\n";
        assert!(matches!(detect_diagram_type(content), DiagramHint::State));
    }

    #[test]
    fn detect_component_by_keyword() {
        let content = "component A\ncomponent B\nA -> B\n";
        assert!(matches!(
            detect_diagram_type(content),
            DiagramHint::Component
        ));
    }

    #[test]
    fn detect_component_by_archimate_keyword() {
        let content = "archimate #438DD5 \"App\" <<application-component>> as app\n";
        assert!(matches!(
            detect_diagram_type(content),
            DiagramHint::Component
        ));
    }

    #[test]
    fn detect_component_by_file_keyword() {
        let content = "file Report\n";
        assert!(matches!(
            detect_diagram_type(content),
            DiagramHint::Component
        ));
    }

    #[test]
    fn detect_class_with_rectangle_group() {
        let content = "rectangle Foo {\n  class A\n}\npackage Bar {\n  class B\n}\n";
        assert!(matches!(detect_diagram_type(content), DiagramHint::Class));
    }

    #[test]
    fn detect_class_with_interface_and_qualified_assoc() {
        let content = "interface Map<K,V>\nclass HashMap\nHashMap [id: Long] --> Customer\n";
        assert!(matches!(detect_diagram_type(content), DiagramHint::Class));
    }

    #[test]
    fn detect_timing_by_robust() {
        let content = "robust \"DNS\" as DNS\nconcise \"Web\" as WB\n";
        assert!(matches!(detect_diagram_type(content), DiagramHint::Timing));
    }

    #[test]
    fn detect_start_tag_chen() {
        assert!(matches!(
            detect_start_tag("@startchen movies\nentity X {}\n@endchen"),
            Some(DiagramHint::Erd)
        ));
    }

    #[test]
    fn detect_start_tag_gantt() {
        assert!(matches!(
            detect_start_tag("@startgantt\n[T] lasts 5 days\n@endgantt"),
            Some(DiagramHint::Gantt)
        ));
    }

    #[test]
    fn detect_start_tag_json() {
        assert!(matches!(
            detect_start_tag("@startjson\n{}\n@endjson"),
            Some(DiagramHint::Json)
        ));
    }

    #[test]
    fn detect_start_tag_mindmap() {
        assert!(matches!(
            detect_start_tag("@startmindmap\n* root\n@endmindmap"),
            Some(DiagramHint::Mindmap)
        ));
    }

    #[test]
    fn detect_start_tag_wbs() {
        assert!(matches!(
            detect_start_tag("@startwbs\n* root\n@endwbs"),
            Some(DiagramHint::Wbs)
        ));
    }

    #[test]
    fn detect_start_tag_yaml() {
        assert!(matches!(
            detect_start_tag("@startyaml\nkey: val\n@endyaml"),
            Some(DiagramHint::Yaml)
        ));
    }

    #[test]
    fn detect_start_tag_uml_returns_none() {
        assert!(detect_start_tag("@startuml\nclass Foo\n@enduml").is_none());
    }

    // -- parse_meta tests --

    #[test]
    fn parse_meta_empty_source() {
        let meta = parse_meta("");
        assert!(meta.is_empty());
    }

    #[test]
    fn parse_meta_single_line_title() {
        let src = "@startuml\ntitle My Title\nclass Foo\n@enduml";
        let meta = parse_meta(src);
        assert_eq!(meta.title.as_deref(), Some("My Title"));
    }

    #[test]
    fn parse_meta_multi_line_title() {
        let src = "@startuml\ntitle\nLine 1\nLine 2\nend title\nclass Foo\n@enduml";
        let meta = parse_meta(src);
        assert_eq!(meta.title.as_deref(), Some("Line 1\nLine 2"));
    }

    #[test]
    fn parse_meta_single_line_header() {
        let src = "header My Header\nclass Foo";
        let meta = parse_meta(src);
        assert_eq!(meta.header.as_deref(), Some("My Header"));
    }

    #[test]
    fn parse_meta_multi_line_header() {
        let src = "header\nH1\nH2\nend header\nclass Foo";
        let meta = parse_meta(src);
        assert_eq!(meta.header.as_deref(), Some("H1\nH2"));
    }

    #[test]
    fn parse_meta_single_line_footer() {
        let src = "footer Page 1\nclass Foo";
        let meta = parse_meta(src);
        assert_eq!(meta.footer.as_deref(), Some("Page 1"));
    }

    #[test]
    fn parse_meta_multi_line_footer() {
        let src = "footer\nF1\nF2\nend footer\nclass Foo";
        let meta = parse_meta(src);
        assert_eq!(meta.footer.as_deref(), Some("F1\nF2"));
    }

    #[test]
    fn parse_meta_caption() {
        let src = "caption Figure 1. Overview\nclass Foo";
        let meta = parse_meta(src);
        assert_eq!(meta.caption.as_deref(), Some("Figure 1. Overview"));
    }

    #[test]
    fn parse_meta_legend_multiline() {
        let src = "legend\nLegend line 1\nLegend line 2\nend legend\nclass Foo";
        let meta = parse_meta(src);
        assert_eq!(meta.legend.as_deref(), Some("Legend line 1\nLegend line 2"));
    }

    #[test]
    fn parse_meta_legend_skips_preproc_comment_blank_lines_when_original_available() {
        let expanded = "legend\nFirst\n\nSecond\n\nThird\nendlegend";
        let original = "legend\nFirst\n'comment\nSecond\n'comment\nThird\nendlegend";
        let meta = parse_meta_with_original(expanded, Some(original));
        assert_eq!(meta.legend.as_deref(), Some("First\nSecond\nThird"));
    }

    #[test]
    fn parse_meta_legend_with_position() {
        let src = "legend right\nSome legend\nend legend";
        let meta = parse_meta(src);
        assert_eq!(meta.legend.as_deref(), Some("Some legend"));
    }

    #[test]
    fn parse_meta_all_fields() {
        let src =
            "header Top\ntitle Big Title\ncaption Fig 1\nfooter Bottom\nlegend\nL1\nend legend";
        let meta = parse_meta(src);
        assert_eq!(meta.title.as_deref(), Some("Big Title"));
        assert_eq!(meta.header.as_deref(), Some("Top"));
        assert_eq!(meta.footer.as_deref(), Some("Bottom"));
        assert_eq!(meta.caption.as_deref(), Some("Fig 1"));
        assert_eq!(meta.legend.as_deref(), Some("L1"));
        assert!(!meta.is_empty());
    }

    #[test]
    fn parse_meta_no_directives() {
        let src = "@startuml\nclass Foo {}\nFoo --> Bar\n@enduml";
        let meta = parse_meta(src);
        assert!(meta.is_empty());
    }

    #[test]
    fn parse_meta_endtitle_alt_form() {
        let src = "title\nAlt form\nendtitle";
        let meta = parse_meta(src);
        assert_eq!(meta.title.as_deref(), Some("Alt form"));
    }

    #[test]
    fn parse_meta_endheader_alt_form() {
        let src = "header\nAlt header\nendheader";
        let meta = parse_meta(src);
        assert_eq!(meta.header.as_deref(), Some("Alt header"));
    }

    #[test]
    fn parse_meta_endfooter_alt_form() {
        let src = "footer\nAlt footer\nendfooter";
        let meta = parse_meta(src);
        assert_eq!(meta.footer.as_deref(), Some("Alt footer"));
    }

    #[test]
    fn parse_meta_endlegend_alt_form() {
        let src = "legend\nAlt legend\nendlegend";
        let meta = parse_meta(src);
        assert_eq!(meta.legend.as_deref(), Some("Alt legend"));
    }

    #[test]
    fn parse_meta_is_empty_default() {
        let meta = DiagramMeta::default();
        assert!(meta.is_empty());
    }

    #[test]
    fn meta_only_content_is_not_meaningful() {
        let content = "title\nHello\nend title\nheader Top\n";
        assert!(!has_meaningful_uml_content(content));
    }

    #[test]
    fn file_content_is_meaningful() {
        let content = "title Example\nfile report [\nhello\n]\n";
        assert!(has_meaningful_uml_content(content));
    }

    #[test]
    fn extract_sprites_single_line() {
        let src = "Alice -> Bob : hi\nsprite redrect <svg viewBox=\"0 0 100 50\"><rect/></svg>\nBob -> Alice : ok\n";
        let (cleaned, sprites, _) = extract_sprites(src);
        assert_eq!(sprites.len(), 1);
        assert!(sprites.contains_key("redrect"));
        assert!(sprites["redrect"].contains("<rect/>"));
        assert!(!cleaned.contains("sprite"));
        assert!(cleaned.contains("Alice -> Bob"));
    }

    #[test]
    fn extract_sprites_multiline() {
        let src = "sprite myicon <svg viewBox=\"0 0 50 50\">\n  <circle cx=\"25\" cy=\"25\" r=\"20\"/>\n</svg>\nAlice -> Bob\n";
        let (cleaned, sprites, _) = extract_sprites(src);
        assert_eq!(sprites.len(), 1);
        assert!(sprites["myicon"].contains("<circle"));
        assert!(cleaned.contains("Alice -> Bob"));
        assert!(!cleaned.contains("sprite"));
    }

    #[test]
    fn extract_sprites_dollar_prefix() {
        let src = "sprite $icon <svg viewBox=\"0 0 10 10\"><rect/></svg>\n";
        let (_, sprites, _) = extract_sprites(src);
        assert_eq!(sprites.len(), 1);
        assert!(sprites.contains_key("icon"));
    }

    #[test]
    fn extract_sprites_none() {
        let src = "Alice -> Bob : hello\n";
        let (cleaned, sprites, _) = extract_sprites(src);
        assert!(sprites.is_empty());
        assert_eq!(cleaned, "Alice -> Bob : hello");
    }

    #[test]
    fn detect_class_by_rectangle_bracket_display() {
        // Java treats `rectangle [...]` as CLASS, not COMPONENT.
        let content = "rectangle A [\ntest 1\ntest 2\n]\n";
        assert!(matches!(detect_diagram_type(content), DiagramHint::Class));
    }

    #[test]
    fn extract_sprites_hex_format() {
        let src = "sprite $icon [4x2/16] {\nF00F\n0FF0\n}\nAlice -> Bob\n";
        let (cleaned, sprites, _) = extract_sprites(src);
        assert_eq!(sprites.len(), 1, "should extract one hex sprite");
        assert!(sprites.contains_key("icon"), "key should be 'icon'");
        assert!(
            sprites["icon"].contains("<image"),
            "SVG should contain <image> element"
        );
        assert!(
            sprites["icon"].contains("data:image/png;base64,"),
            "should have base64 PNG"
        );
        assert!(
            !cleaned.contains("sprite"),
            "sprite lines should be removed from cleaned"
        );
        assert!(cleaned.contains("Alice -> Bob"));
    }

    #[test]
    fn extract_sprites_hex_16x16() {
        let src = concat!(
            "sprite $businessProcess [16x16/16] {\n",
            "FFFFFFFFFFFFFFFF\n",
            "FFFFFFFFFFFFFFFF\n",
            "FFFFFFFFFFFFFFFF\n",
            "FFFFFFFFFFFFFFFF\n",
            "FFFFFFFFFF0FFFFF\n",
            "FFFFFFFFFF00FFFF\n",
            "FF00000000000FFF\n",
            "FF000000000000FF\n",
            "FF00000000000FFF\n",
            "FFFFFFFFFF00FFFF\n",
            "FFFFFFFFFF0FFFFF\n",
            "FFFFFFFFFFFFFFFF\n",
            "FFFFFFFFFFFFFFFF\n",
            "FFFFFFFFFFFFFFFF\n",
            "FFFFFFFFFFFFFFFF\n",
            "FFFFFFFFFFFFFFFF\n",
            "}\n",
            "rectangle A\n",
        );
        let (cleaned, sprites, _) = extract_sprites(src);
        assert_eq!(sprites.len(), 1);
        assert!(sprites.contains_key("businessProcess"));
        let svg = &sprites["businessProcess"];
        assert!(
            svg.contains("viewBox=\"0 0 16 16\""),
            "viewBox should be 16x16"
        );
        assert!(!cleaned.contains("sprite"));
        assert!(cleaned.contains("rectangle A"));
    }
}
