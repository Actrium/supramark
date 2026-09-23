use log::{debug, trace, warn};

use crate::model::component::{
    ComponentDiagram, ComponentEntity, ComponentGroup, ComponentKind, ComponentLink, ComponentNote,
};
use crate::model::Direction;
use crate::Result;

// ---------------------------------------------------------------------------
// Parse mode for multi-line constructs
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum ParseMode {
    Normal,
    /// Inside a `note ... of Target` multi-line block
    NoteBlock {
        position: String,
        target: Option<String>,
        lines: Vec<String>,
        start_line: usize,
    },
    /// Inside a `sprite ... { ... }` block (skip)
    SpriteBlock,
    /// Inside a `<style>...</style>` block (skip)
    StyleBlock,
    /// Inside a `skinparam { ... }` block (skip)
    SkinparamBlock,
    /// Inside a `rectangle Name [ description ]` multi-line description
    DescriptionBlock {
        entity_id: String,
        lines: Vec<String>,
    },
}

/// Context frame for nesting (e.g., `rectangle Foo { ... }`)
#[derive(Debug)]
struct GroupFrame {
    name: String,
    id: String,
    kind: ComponentKind,
    stereotype: Option<String>,
    stereotypes: Vec<String>,
    children: Vec<String>,
    source_line: Option<usize>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn parse_component_diagram(source: &str) -> Result<ComponentDiagram> {
    let block = super::common::extract_block(source).unwrap_or_else(|| source.to_string());

    let mut entities: Vec<ComponentEntity> = Vec::new();
    let mut links: Vec<ComponentLink> = Vec::new();
    let mut groups: Vec<ComponentGroup> = Vec::new();
    let mut notes: Vec<ComponentNote> = Vec::new();
    let mut direction = Direction::default();
    let mut mode = ParseMode::Normal;
    let mut group_stack: Vec<GroupFrame> = Vec::new();

    for (line_num, raw_line) in block.lines().enumerate() {
        let line_num = line_num + 1;
        let line = raw_line.trim();

        match mode {
            ParseMode::SpriteBlock => {
                if line == "}" {
                    trace!("line {line_num}: end sprite block");
                    mode = ParseMode::Normal;
                }
                continue;
            }
            ParseMode::StyleBlock => {
                if line == "</style>" {
                    trace!("line {line_num}: end style block");
                    mode = ParseMode::Normal;
                }
                continue;
            }
            ParseMode::SkinparamBlock => {
                if line == "}" {
                    trace!("line {line_num}: end skinparam block");
                    mode = ParseMode::Normal;
                }
                continue;
            }
            ParseMode::NoteBlock {
                ref position,
                ref target,
                ref mut lines,
                start_line,
            } => {
                if line == "end note" {
                    let text = lines.join("\n");
                    debug!("line {line_num}: end note block, text={text:?}");
                    // Java records source_line as start_line + 1 (the first body
                    // line), not the "end note" line.
                    notes.push(ComponentNote {
                        text,
                        position: position.clone(),
                        target: target.clone(),
                        source_line: Some(start_line + 1),
                        is_block: true,
                    });
                    mode = ParseMode::Normal;
                } else {
                    lines.push(line.to_string());
                }
                continue;
            }
            ParseMode::DescriptionBlock {
                ref entity_id,
                ref mut lines,
            } => {
                if line == "]" {
                    let desc = expand_body_newlines(&lines.join("\n"));
                    let desc_lines: Vec<String> =
                        desc.lines().map(std::string::ToString::to_string).collect();
                    debug!(
                        "line {}: end description block for '{}', {} lines",
                        line_num,
                        entity_id,
                        desc_lines.len()
                    );
                    // Update the entity's description
                    if let Some(e) = entities.iter_mut().find(|e| e.id == *entity_id) {
                        e.description = desc_lines;
                    }
                    mode = ParseMode::Normal;
                } else {
                    lines.push(raw_line.trim().to_string());
                }
                continue;
            }
            ParseMode::Normal => {}
        }

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('\'') || line.starts_with("/'") {
            continue;
        }

        // Skip directives
        if line.starts_with("skinparam ") && line.contains('{') {
            // Single-line skinparam (or several concatenated ones) where every
            // `{` has a matching `}` on the same line — preprocessor stdlib
            // styles often emit this layout, e.g. C4_*.puml. Skip the whole
            // line instead of entering a multi-line block we'd never close.
            let opens = line.matches('{').count();
            let closes = line.matches('}').count();
            if opens > 0 && opens == closes {
                trace!(
                    "line {line_num}: skip single-line skinparam ({} braces)",
                    opens
                );
                continue;
            }
            mode = ParseMode::SkinparamBlock;
            continue;
        }
        if line.starts_with("skinparam ") || line.starts_with("hide ") || line.starts_with("show ")
        {
            continue;
        }
        if line == "<style>" || line.starts_with("<style>") {
            mode = ParseMode::StyleBlock;
            continue;
        }
        if line.starts_with("sprite ") {
            if line.contains('{') && !line.contains('}') {
                mode = ParseMode::SpriteBlock;
            }
            continue;
        }

        // Direction
        {
            let lower = line.to_lowercase();
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
        }

        // Check for `end note` (shouldn't reach here, but safety)
        if line == "end note" {
            continue;
        }

        // Check for closing brace (end of group)
        if line == "}" {
            if let Some(frame) = group_stack.pop() {
                debug!("line {}: close group '{}'", line_num, frame.name);
                let code = frame.id.clone();
                groups.push(ComponentGroup {
                    name: frame.name,
                    id: frame.id,
                    code,
                    kind: frame.kind,
                    stereotype: frame.stereotype,
                    stereotypes: frame.stereotypes,
                    children: frame.children,
                    source_line: frame.source_line,
                });
            }
            continue;
        }

        // Try to parse note
        if let Some(note_result) = try_parse_note(line) {
            match note_result {
                NoteParseResult::SingleLine(mut note) => {
                    debug!("line {}: single-line note for {:?}", line_num, note.target);
                    note.source_line = Some(line_num);
                    notes.push(note);
                }
                NoteParseResult::MultiLineStart { position, target } => {
                    debug!("line {line_num}: start multi-line note for {target:?}");
                    mode = ParseMode::NoteBlock {
                        position,
                        target,
                        lines: Vec::new(),
                        start_line: line_num,
                    };
                }
            }
            continue;
        }

        // Try to parse entity declaration (component, rectangle, node, etc.)
        if let Some(decl) = try_parse_entity_decl(line) {
            debug!(
                "line {}: entity '{}' (id='{}', kind={:?})",
                line_num, decl.name, decl.id, decl.kind
            );

            // Check if this opens a group (has `{`)
            if decl.opens_group {
                let parent_id = group_stack.last().map(|f| f.id.clone());
                // Register the entity for the group container itself
                entities.push(ComponentEntity {
                    name: decl.name.clone(),
                    id: decl.id.clone(),
                    code: decl.id.clone(),
                    kind: decl.kind.clone(),
                    stereotype: decl.stereotype.clone(),
                    stereotypes: decl.stereotypes.clone(),
                    description: vec![],
                    parent: parent_id.clone(),
                    color: decl.color.clone(),
                    source_line: Some(line_num),
                });
                if let Some(parent) = group_stack.last_mut() {
                    parent.children.push(decl.id.clone());
                }
                group_stack.push(GroupFrame {
                    name: decl.name,
                    id: decl.id,
                    kind: decl.kind,
                    stereotype: decl.stereotype,
                    stereotypes: decl.stereotypes,
                    children: Vec::new(),
                    source_line: Some(line_num),
                });
                continue;
            }

            // Check for description block `Name [ ...`
            if decl.opens_description {
                let parent_id = group_stack.last().map(|f| f.id.clone());
                entities.push(ComponentEntity {
                    name: decl.name.clone(),
                    id: decl.id.clone(),
                    code: decl.id.clone(),
                    kind: decl.kind,
                    stereotype: decl.stereotype,
                    stereotypes: decl.stereotypes,
                    description: vec![],
                    parent: parent_id,
                    color: decl.color,
                    source_line: Some(line_num),
                });
                if let Some(parent) = group_stack.last_mut() {
                    parent.children.push(decl.id.clone());
                }
                mode = ParseMode::DescriptionBlock {
                    entity_id: decl.id,
                    lines: if decl.initial_desc.is_empty() {
                        Vec::new()
                    } else {
                        vec![decl.initial_desc]
                    },
                };
                continue;
            }

            let parent_id = group_stack.last().map(|f| f.id.clone());
            let code = decl.id.clone();
            entities.push(ComponentEntity {
                name: decl.name.clone(),
                id: decl.id.clone(),
                code,
                kind: decl.kind,
                stereotype: decl.stereotype,
                stereotypes: decl.stereotypes,
                description: decl.description,
                parent: parent_id,
                color: decl.color,
                source_line: Some(line_num),
            });
            if let Some(parent) = group_stack.last_mut() {
                parent.children.push(decl.id);
            }
            continue;
        }

        // Try to parse component shorthand `[Name]` or `[Name] as alias`
        if let Some((name, id)) = try_parse_component_shorthand(line) {
            debug!("line {line_num}: component shorthand [{name}] as {id}");
            let parent_id = group_stack.last().map(|f| f.id.clone());
            // Only add if not already declared
            if !entities.iter().any(|e| e.id == id) {
                entities.push(ComponentEntity {
                    name,
                    id: id.clone(),
                    code: id.clone(),
                    kind: ComponentKind::Component,
                    stereotype: None,
                    stereotypes: Vec::new(),
                    description: vec![],
                    parent: parent_id,
                    color: None,
                    source_line: Some(line_num),
                });
                if let Some(parent) = group_stack.last_mut() {
                    parent.children.push(id);
                }
            }
            continue;
        }

        // Try to parse arrow / link
        if let Some(mut link) = try_parse_link(line) {
            link.source_line = Some(line_num);
            debug!(
                "line {}: link '{}' -> '{}' label='{}' dashed={} hint={:?}",
                line_num, link.from, link.to, link.label, link.dashed, link.direction_hint
            );

            // Ensure both endpoints exist as entities
            for endpoint_id in [&link.from, &link.to] {
                if !entities.iter().any(|e| e.id == *endpoint_id) {
                    let parent_id = group_stack.last().map(|f| f.id.clone());
                    entities.push(ComponentEntity {
                        name: endpoint_id.clone(),
                        id: endpoint_id.clone(),
                        code: endpoint_id.clone(),
                        kind: ComponentKind::Component,
                        stereotype: None,
                        stereotypes: Vec::new(),
                        description: vec![],
                        parent: parent_id,
                        color: None,
                        source_line: Some(line_num),
                    });
                    if let Some(parent) = group_stack.last_mut() {
                        parent.children.push(endpoint_id.clone());
                    }
                }
            }

            links.push(link);
            continue;
        }

        // Unrecognized line
        trace!("line {line_num}: skipping unrecognized: {line:?}");
    }

    // Close any unclosed groups
    while let Some(frame) = group_stack.pop() {
        warn!("unclosed group '{}', auto-closing", frame.name);
        let code = frame.id.clone();
        groups.push(ComponentGroup {
            name: frame.name,
            id: frame.id,
            code,
            kind: frame.kind,
            stereotype: frame.stereotype,
            stereotypes: frame.stereotypes,
            children: frame.children,
            source_line: frame.source_line,
        });
    }

    debug!(
        "parse_component_diagram: {} entities, {} links, {} groups, {} notes",
        entities.len(),
        links.len(),
        groups.len(),
        notes.len()
    );

    Ok(ComponentDiagram {
        entities,
        links,
        groups,
        notes,
        direction,
    })
}

// ---------------------------------------------------------------------------
// Entity declaration parsing
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct EntityDecl {
    name: String,
    id: String,
    kind: ComponentKind,
    stereotype: Option<String>,
    /// Full stereotype list in declaration order. Used for stereotype-keyed
    /// skinparam lookups when the C4 stdlib emits chained tags like
    /// `<<container>><<boundary>>`.
    stereotypes: Vec<String>,
    description: Vec<String>,
    opens_group: bool,
    opens_description: bool,
    initial_desc: String,
    color: Option<String>,
}

fn try_parse_entity_decl(line: &str) -> Option<EntityDecl> {
    let keywords = [
        ("component ", ComponentKind::Component),
        ("rectangle ", ComponentKind::Rectangle),
        ("node ", ComponentKind::Node),
        ("database ", ComponentKind::Database),
        ("cloud ", ComponentKind::Cloud),
        ("package ", ComponentKind::Package),
        ("interface ", ComponentKind::Interface),
        ("card ", ComponentKind::Card),
        // Deployment-style `file` blocks are rendered with the rectangle path for now.
        ("file ", ComponentKind::Rectangle),
        // Deployment diagram keywords
        ("artifact ", ComponentKind::Artifact),
        ("storage ", ComponentKind::Storage),
        ("folder ", ComponentKind::Folder),
        ("frame ", ComponentKind::Frame),
        ("agent ", ComponentKind::Agent),
        ("stack ", ComponentKind::Stack),
        ("queue ", ComponentKind::Queue),
        ("portin ", ComponentKind::PortIn),
        ("portout ", ComponentKind::PortOut),
        ("port ", ComponentKind::PortIn),
    ];

    for (prefix, kind) in &keywords {
        if !line.starts_with(prefix) {
            continue;
        }
        let rest = &line[prefix.len()..];
        return Some(parse_entity_rest(rest, kind.clone()));
    }

    // Archimate keyword: `archimate COLOR "LABEL" <<STEREOTYPE>> as ALIAS`
    // The color appears before the name, unlike other keywords.
    if let Some(stripped) = line.strip_prefix("archimate ") {
        let rest = stripped.trim();
        return Some(parse_archimate_rest(rest));
    }

    None
}

/// Parse archimate entity declaration: `COLOR "LABEL" <<STEREO>> as ALIAS`
/// The distinguishing feature is COLOR comes first (before the name).
fn parse_archimate_rest(rest: &str) -> EntityDecl {
    let mut color = None;
    let mut remaining = rest.to_string();

    // First, consume any color specification (e.g. `#RRGGBB`, `#NamedColor`)
    let trimmed = remaining.trim();
    if let Some(after_hash) = trimmed.strip_prefix('#') {
        let end_pos = after_hash
            .find(|c: char| c.is_whitespace() || c == '"' || c == '<')
            .map(|p| p + 1)
            .unwrap_or(trimmed.len());
        let color_str = &trimmed[..end_pos];
        if let Some(hcolor) = crate::klimt::color::resolve_color(color_str) {
            color = Some(hcolor.to_svg());
        } else {
            color = Some(color_str.to_string());
        }
        remaining = trimmed[end_pos..].trim().to_string();
    }

    // Now parse the rest as a normal entity (name, stereotype, alias, etc.)
    let mut decl = parse_entity_rest(&remaining, ComponentKind::Archimate);
    // Archimate color from the keyword position takes precedence
    if color.is_some() {
        decl.color = color;
    }
    decl
}

fn parse_entity_rest(rest: &str, kind: ComponentKind) -> EntityDecl {
    let mut name;
    let mut id;
    let mut stereotype = None;
    let mut stereotypes: Vec<String> = Vec::new();
    let mut color = None;
    let mut opens_group = false;
    let mut opens_description = false;
    let mut initial_desc = String::new();

    let trimmed = rest.trim();

    // Check if the name is quoted: "Name" as alias ...
    if let Some(after_open_quote) = trimmed.strip_prefix('"') {
        if let Some(end_quote) = after_open_quote.find('"') {
            name = after_open_quote[..end_quote].to_string();
            id = name.clone();
            // Parse remaining: `as alias`, `<<stereo>>`, `#Color`
            // modifiers can be interleaved in any order.
            let after = parse_entity_modifiers(
                after_open_quote[end_quote + 1..].trim(),
                &mut id,
                &mut stereotype,
                &mut stereotypes,
                &mut color,
            );
            parse_entity_tail(
                &after,
                &mut opens_group,
                &mut opens_description,
                &mut initial_desc,
            );
        } else {
            // Unclosed quote, just use entire rest as name
            name = trimmed.to_string();
            id = name.clone();
        }
    } else {
        // Unquoted name: take until whitespace, `{`, `[`, `<`, `#`
        let end_pos = trimmed
            .find(|c: char| c.is_whitespace() || c == '{' || c == '[' || c == '<' || c == '#')
            .unwrap_or(trimmed.len());
        name = trimmed[..end_pos].to_string();
        id = name.clone();

        let after = parse_entity_modifiers(
            trimmed[end_pos..].trim(),
            &mut id,
            &mut stereotype,
            &mut stereotypes,
            &mut color,
        );
        parse_entity_tail(
            &after,
            &mut opens_group,
            &mut opens_description,
            &mut initial_desc,
        );
    }

    // Clean up name: expand newline sequences
    name = expand_newlines(&name);

    EntityDecl {
        name,
        id,
        kind,
        stereotype,
        stereotypes,
        description: vec![],
        opens_group,
        opens_description,
        initial_desc,
        color,
    }
}

fn parse_as_alias(after: &str, id: &mut String) -> String {
    let after = after.trim();
    if let Some(rest) = after.strip_prefix("as ") {
        let rest = rest.trim();
        let end_pos = rest
            .find(|c: char| c.is_whitespace() || c == '{' || c == '[' || c == '<')
            .unwrap_or(rest.len());
        *id = rest[..end_pos].to_string();
        rest[end_pos..].trim().to_string()
    } else {
        after.to_string()
    }
}

fn parse_entity_modifiers(
    after: &str,
    id: &mut String,
    stereotype: &mut Option<String>,
    stereotypes: &mut Vec<String>,
    color: &mut Option<String>,
) -> String {
    let mut remaining = after.trim().to_string();

    loop {
        let after_alias = parse_as_alias(&remaining, id);
        let after_stereotype = parse_all_stereotypes(&after_alias, stereotypes);
        if stereotype.is_none() && !stereotypes.is_empty() {
            *stereotype = stereotypes.first().cloned();
        }
        let after_color = parse_color_spec(&after_stereotype, color);
        if after_color == remaining {
            return remaining;
        }
        remaining = after_color;
    }
}

/// Parse `#Color` or `#RRGGBB` specification from entity declaration.
fn parse_color_spec(after: &str, color: &mut Option<String>) -> String {
    let trimmed = after.trim();
    if !trimmed.starts_with('#') {
        return trimmed.to_string();
    }
    // Extract color name/hex: everything up to whitespace, '{', '[', or end
    let end_pos = trimmed[1..]
        .find(|c: char| c.is_whitespace() || c == '{' || c == '[' || c == '<')
        .map(|p| p + 1)
        .unwrap_or(trimmed.len());
    let color_str = &trimmed[..end_pos]; // includes the '#'
    if color.is_none() {
        // Resolve named colors to hex via klimt color system
        if let Some(hcolor) = crate::klimt::color::resolve_color(color_str) {
            *color = Some(hcolor.to_svg());
        } else {
            // Fallback: use the raw string (including #)
            *color = Some(color_str.to_string());
        }
    }
    trimmed[end_pos..].trim().to_string()
}

#[allow(dead_code)]
fn parse_stereotypes(after: &str, stereotype: &mut Option<String>) -> String {
    // Thin wrapper that ignores the full list — kept for callers that only
    // need the primary stereotype and trailing text.
    let mut list: Vec<String> = Vec::new();
    let rest = parse_all_stereotypes(after, &mut list);
    if stereotype.is_none() {
        *stereotype = list.first().cloned();
    }
    rest
}

/// Parse every `<<stereotype>>` marker at the start of `after`, populating
/// the `list` in declaration order and returning the remaining trailing text.
fn parse_all_stereotypes(after: &str, list: &mut Vec<String>) -> String {
    let mut remaining = after.trim().to_string();
    loop {
        let trimmed = remaining.trim();
        if !trimmed.starts_with("<<") {
            return trimmed.to_string();
        }
        let Some(end) = trimmed.find(">>") else {
            return trimmed.to_string();
        };
        list.push(trimmed[2..end].to_string());
        remaining = trimmed[end + 2..].trim().to_string();
    }
}

fn parse_entity_tail(
    after: &str,
    opens_group: &mut bool,
    opens_description: &mut bool,
    initial_desc: &mut String,
) {
    let trimmed = after.trim();
    if trimmed.starts_with('{') {
        *opens_group = true;
    } else if let Some(desc_content) = trimmed.strip_prefix('[') {
        *opens_description = true;
        // Check if description is on the same line and closes
        if let Some(close) = desc_content.find(']') {
            // Single-line description [text]
            *initial_desc = desc_content[..close].trim().to_string();
            *opens_description = false; // actually closes immediately
        } else {
            // Multi-line description starts
            *initial_desc = desc_content.trim().to_string();
        }
    }
}

// ---------------------------------------------------------------------------
// Component shorthand `[Name]`
// ---------------------------------------------------------------------------

fn try_parse_component_shorthand(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    let close = trimmed.find(']')?;
    let name = trimmed[1..close].to_string();
    let rest = trimmed[close + 1..].trim();
    let id = if let Some(alias) = rest.strip_prefix("as ") {
        let alias = alias.trim();
        let end = alias
            .find(|c: char| c.is_whitespace())
            .unwrap_or(alias.len());
        alias[..end].to_string()
    } else {
        name.clone()
    };
    Some((name, id))
}

// ---------------------------------------------------------------------------
// Arrow / link parsing
// ---------------------------------------------------------------------------

fn try_parse_link(line: &str) -> Option<ComponentLink> {
    // Patterns to match:
    //   A -> B : label
    //   A --> B : label     (long solid)
    //   A ..> B : label     (dashed)
    //   A -up-> B : label   (direction hints)
    //   A <-down- B : label (reverse arrow with hint)
    //   A -left-> B
    //   A <-right- B

    let trimmed = line.trim();

    // Try forward arrow first (something ending with `>`)
    if let Some(result) = try_parse_forward_arrow(trimmed) {
        return Some(result);
    }

    // Try backward arrow (something starting with `<`)
    if let Some(result) = try_parse_backward_arrow(trimmed) {
        return Some(result);
    }

    None
}

fn try_parse_forward_arrow(line: &str) -> Option<ComponentLink> {
    // Find `>` that's part of an arrow
    for (i, _) in line.match_indices('>') {
        if line[i + 1..].starts_with('>') {
            continue;
        }
        // Look backwards from `>` to find start of arrow
        let before_gt = &line[..i];
        if let Some(arrow_start) = find_arrow_start_forward(before_gt) {
            let from_part = line[..arrow_start].trim();
            let arrow_str = &line[arrow_start..=i];
            let after_part = line[i + 1..].trim();

            if from_part.is_empty() {
                continue;
            }

            let from_id = extract_last_token(from_part);
            if from_id.is_empty() {
                continue;
            }

            let (to_id, label) = parse_to_and_label(after_part);
            if to_id.is_empty() {
                continue;
            }

            let dashed = arrow_str.contains("..");
            let direction_hint = extract_direction_hint(arrow_str);
            let arrow_len = count_arrow_len(arrow_str);
            // Java calls Link.getInv() for forward arrows with UP/LEFT direction,
            // consuming an extra UID counter value.
            let direction_inverted = direction_hint
                .as_deref()
                .is_some_and(|d| d == "up" || d == "left");
            // `>>` (or `_>>` for italic-route variants) at the head end maps to
            // Java `LinkDecor.ARROW_TRIANGLE` — the C4 stdlib `Rel(...)`
            // procedure expands to `-->>` so this is the common case for C4
            // diagrams.  Detect by trimming a trailing direction marker like
            // `[>>]` would already be split out by `extract_direction_hint`.
            let head_arrow_triangle = arrow_str.trim_end_matches(']').ends_with(">>");
            let tail_arrow_triangle = false;

            return Some(ComponentLink {
                from: from_id,
                to: to_id,
                label,
                dashed,
                direction_hint,
                arrow_len,
                source_line: None,
                direction_inverted,
                head_arrow_triangle,
                tail_arrow_triangle,
            });
        }
    }
    None
}

fn try_parse_backward_arrow(line: &str) -> Option<ComponentLink> {
    // Find `<` that starts an arrow
    for (i, _) in line.match_indices('<') {
        if i == 0 {
            continue;
        }

        let before_lt = &line[..i];
        let after_lt = &line[i + 1..];

        // Find end of arrow (first whitespace after `<`)
        let arrow_end = after_lt
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after_lt.len());
        let arrow_tail = &after_lt[..arrow_end];

        // Arrow tail should be `-` or `.` characters with optional direction hint
        if !is_valid_arrow_tail(arrow_tail) {
            continue;
        }

        let to_part = before_lt.trim();
        let to_id = extract_last_token(to_part);
        if to_id.is_empty() {
            continue;
        }

        let remaining = after_lt[arrow_end..].trim();
        let (from_id, label) = parse_to_and_label(remaining);
        if from_id.is_empty() {
            continue;
        }

        let full_arrow = format!("<{arrow_tail}");
        let dashed = full_arrow.contains("..");
        // Invert direction hint for backward arrows since from/to are swapped
        let direction_hint = extract_direction_hint(&full_arrow).map(|d| match d.as_str() {
            "up" => "down".to_string(),
            "down" => "up".to_string(),
            "left" => "right".to_string(),
            "right" => "left".to_string(),
            _ => d,
        });
        let arrow_len = count_arrow_len(&full_arrow);
        // For backward arrows, the original `<<` decoration ends up at the
        // semantic head after the from/to swap (we already swapped to/from
        // above); track it as `head_arrow_triangle` so the renderer applies
        // the triangle at the same end Java does.
        let head_arrow_triangle = full_arrow.starts_with("<<");
        let tail_arrow_triangle = false;

        // Backward arrows do NOT trigger Java's Link.getInv(), so no extra UID.
        return Some(ComponentLink {
            from: from_id,
            to: to_id,
            label,
            dashed,
            direction_hint,
            arrow_len,
            source_line: None,
            direction_inverted: false,
            head_arrow_triangle,
            tail_arrow_triangle,
        });
    }
    None
}

fn find_arrow_start_forward(before_gt: &str) -> Option<usize> {
    let bytes = before_gt.as_bytes();
    let mut pos = bytes.len();

    // Compare bytes throughout: the walk steps back one byte at a time, so
    // slicing `before_gt` as a `str` would panic on a multi-byte character
    // ending where a direction word is being looked for. Every token here is
    // ASCII, so a byte match implies a character-boundary match.
    while pos > 0 {
        let b = bytes[pos - 1];
        if b == b'-' || b == b'.' || b == b'>' {
            pos -= 1;
        } else if pos >= 2 && &bytes[pos - 2..pos] == b"up" {
            pos -= 2;
        } else if pos >= 4 && (&bytes[pos - 4..pos] == b"down" || &bytes[pos - 4..pos] == b"left") {
            pos -= 4;
        } else if pos >= 5 && &bytes[pos - 5..pos] == b"right" {
            pos -= 5;
        } else {
            break;
        }
    }

    // Verify we actually found arrow characters (at least one `-` or `.`)
    let arrow_part = &before_gt[pos..];
    if arrow_part.contains('-') || arrow_part.contains('.') {
        // Check there's whitespace before the arrow (or it's at the start)
        if pos == 0 || before_gt.as_bytes()[pos - 1].is_ascii_whitespace() {
            Some(pos)
        } else {
            None
        }
    } else {
        None
    }
}

fn is_valid_arrow_tail(tail: &str) -> bool {
    if tail.is_empty() {
        return false;
    }
    let cleaned = tail
        .replace("up", "")
        .replace("down", "")
        .replace("left", "")
        .replace("right", "");
    cleaned.chars().all(|c| c == '-' || c == '.')
}

fn extract_direction_hint(arrow: &str) -> Option<String> {
    for dir in &["up", "down", "left", "right"] {
        if arrow.contains(dir) {
            return Some(dir.to_string());
        }
    }
    None
}

/// Count the arrow stem length (number of dashes or dots in the arrow).
fn count_arrow_len(arrow: &str) -> usize {
    let cleaned = arrow
        .replace("up", "")
        .replace("down", "")
        .replace("left", "")
        .replace("right", "")
        .replace(['<', '>'], "");
    cleaned.chars().filter(|c| *c == '-' || *c == '.').count()
}

fn extract_last_token(s: &str) -> String {
    let trimmed = s.trim();
    // Handle [Name] shorthand
    if trimmed.ends_with(']') {
        if let Some(open) = trimmed.rfind('[') {
            return trimmed[open + 1..trimmed.len() - 1].to_string();
        }
    }
    // Handle quoted "Name"
    if let Some(without_end_quote) = trimmed.strip_suffix('"') {
        if let Some(open) = without_end_quote.rfind('"') {
            return without_end_quote[open + 1..].to_string();
        }
    }
    // Regular identifier: last whitespace-separated token
    trimmed
        .rsplit_once(char::is_whitespace)
        .map_or_else(|| trimmed.to_string(), |(_, last)| last.to_string())
}

fn parse_to_and_label(s: &str) -> (String, String) {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return (String::new(), String::new());
    }

    // Split on `:` for label
    if let Some(colon_pos) = trimmed.find(':') {
        let to_part = trimmed[..colon_pos].trim();
        let label = trimmed[colon_pos + 1..].trim().to_string();
        let to_id = extract_first_token(to_part);
        (to_id, label)
    } else {
        let to_id = extract_first_token(trimmed);
        (to_id, String::new())
    }
}

fn extract_first_token(s: &str) -> String {
    let trimmed = s.trim();
    // Handle [Name] shorthand
    if trimmed.starts_with('[') {
        if let Some(close) = trimmed.find(']') {
            return trimmed[1..close].to_string();
        }
    }
    // Handle quoted "Name"
    if let Some(after_open_quote) = trimmed.strip_prefix('"') {
        if let Some(close) = after_open_quote.find('"') {
            return after_open_quote[..close].to_string();
        }
    }
    // Regular identifier: first whitespace-separated token
    trimmed
        .split_once(char::is_whitespace)
        .map_or_else(|| trimmed.to_string(), |(first, _)| first.to_string())
}

// ---------------------------------------------------------------------------
// Note parsing
// ---------------------------------------------------------------------------

enum NoteParseResult {
    SingleLine(ComponentNote),
    MultiLineStart {
        position: String,
        target: Option<String>,
    },
}

fn try_parse_note(line: &str) -> Option<NoteParseResult> {
    let trimmed = line.trim();
    if !trimmed.starts_with("note ") {
        return None;
    }

    let rest = trimmed[5..].trim();

    // `note top of Target: text` (single-line)
    // `note bottom of Target` (multi-line start)
    for pos in &["top", "bottom", "left", "right"] {
        if !rest.starts_with(pos) {
            continue;
        }
        let after_pos = rest[pos.len()..].trim();
        if !after_pos.starts_with("of ") {
            continue;
        }
        let after_of = after_pos[3..].trim();

        // Check for single-line note with `: text`
        if let Some(colon_pos) = after_of.find(':') {
            let target = after_of[..colon_pos].trim().to_string();
            let text = expand_newlines(after_of[colon_pos + 1..].trim());
            return Some(NoteParseResult::SingleLine(ComponentNote {
                text,
                position: pos.to_string(),
                target: Some(target),
                source_line: None, // set by caller
                is_block: false,
            }));
        }

        // Multi-line note (no colon, lines follow until "end note")
        let target = after_of.trim().to_string();
        return Some(NoteParseResult::MultiLineStart {
            position: pos.to_string(),
            target: if target.is_empty() {
                None
            } else {
                Some(target)
            },
        });
    }

    None
}

// ---------------------------------------------------------------------------
// Newline expansion
// ---------------------------------------------------------------------------

/// Expand all newline markers in inline text (notes, entity names).
/// Both literal `\n` escape and U+E100 placeholder become real newlines.
fn expand_newlines(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace(crate::NEWLINE_CHAR, "\n")
        .replace("%chr(10)", "\n")
}

/// Expand newlines in bracket-display body `[...]` content.
/// Literal `\n` is preserved as display text (Java compatibility).
/// Only leftover `%chr(10)` is expanded; the U+E100 placeholder from
/// `%newline()` is preserved verbatim so the downstream renderer (Display
/// layer) decides how to interpret it. In particular table parsers treat
/// U+E100 as ordinary cell content while text renderers split lines on it.
fn expand_body_newlines(s: &str) -> String {
    s.replace("%chr(10)", "\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> ComponentDiagram {
        parse_component_diagram(src).expect("parse failed")
    }

    // 1. Simplest component
    #[test]
    fn test_single_component() {
        let d = parse("@startuml\ncomponent comp1\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "comp1");
        assert_eq!(d.entities[0].id, "comp1");
        assert_eq!(d.entities[0].kind, ComponentKind::Component);
    }

    // 2. Two components
    #[test]
    fn test_two_components() {
        let d = parse("@startuml\ncomponent comp1\ncomponent comp2\n@enduml");
        assert_eq!(d.entities.len(), 2);
    }

    // 3. Quoted name with alias
    #[test]
    fn test_quoted_name_with_alias() {
        let d = parse("@startuml\ncomponent \"My Component\" as mc\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "My Component");
        assert_eq!(d.entities[0].id, "mc");
    }

    // 4. Arrow with label
    #[test]
    fn test_arrow_with_label() {
        let d = parse("@startuml\ncomponent A\ncomponent B\nA -> B : uses\n@enduml");
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].from, "A");
        assert_eq!(d.links[0].to, "B");
        assert_eq!(d.links[0].label, "uses");
        assert!(!d.links[0].dashed);
    }

    // 5. Dashed arrow
    #[test]
    fn test_dashed_arrow() {
        let d = parse("@startuml\ncomponent A\ncomponent B\nA ..> B : dashed\n@enduml");
        assert_eq!(d.links.len(), 1);
        assert!(d.links[0].dashed);
    }

    // 6. Direction hints
    #[test]
    fn test_direction_hints() {
        let d = parse("@startuml\ncomponent A\ncomponent B\nA -up-> B : up arrow\n@enduml");
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].direction_hint, Some("up".to_string()));
    }

    // 7. Backward arrow
    #[test]
    fn test_backward_arrow() {
        let d = parse("@startuml\ncomponent A\ncomponent B\nB <-down- A : backward\n@enduml");
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].from, "A");
        assert_eq!(d.links[0].to, "B");
    }

    // 8. Single-line note
    #[test]
    fn test_single_line_note() {
        let d = parse("@startuml\ncomponent test\nnote top of test: first line\n@enduml");
        assert_eq!(d.notes.len(), 1);
        assert_eq!(d.notes[0].position, "top");
        assert_eq!(d.notes[0].target, Some("test".to_string()));
        assert_eq!(d.notes[0].text, "first line");
    }

    // 9. Multi-line note
    #[test]
    fn test_multi_line_note() {
        let d = parse(
            "@startuml\ncomponent test\nnote bottom of test\n    first line\n    second line\nend note\n@enduml",
        );
        assert_eq!(d.notes.len(), 1);
        assert_eq!(d.notes[0].position, "bottom");
        assert_eq!(d.notes[0].text, "first line\nsecond line");
    }

    // 10. Rectangle with children (group)
    #[test]
    fn test_rectangle_group() {
        let d =
            parse("@startuml\nrectangle \"Container\" {\n  component A\n  component B\n}\n@enduml");
        assert_eq!(d.groups.len(), 1);
        assert_eq!(d.groups[0].name, "Container");
        assert_eq!(d.groups[0].children.len(), 2);
    }

    // 11. Rectangle with description block
    #[test]
    fn test_rectangle_description() {
        let d = parse("@startuml\nrectangle A [\ntest 1\\ntest 11\ntest 2\n]\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].id, "A");
        let desc = &d.entities[0].description;
        assert!(!desc.is_empty());
    }

    // 12. Component with stereotype
    #[test]
    fn test_stereotype() {
        let d =
            parse("@startuml\nrectangle \"inner process\" <<$businessProcess>> as src\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(
            d.entities[0].stereotype,
            Some("$businessProcess".to_string())
        );
        assert_eq!(d.entities[0].id, "src");
    }

    #[test]
    fn test_c4_person_rectangle_label() {
        let d = parse(
            "@startuml\nrectangle \"<$person>\\n== Administrator\" <<person>> as admin\n@enduml",
        );
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].id, "admin");
        assert_eq!(d.entities[0].name, "<$person>\n== Administrator");
        assert_eq!(d.entities[0].stereotype.as_deref(), Some("person"));
    }

    #[test]
    fn test_c4_preprocessed_excerpt_parses_entities() {
        let d = parse(
            "@startuml\nrectangle \"<$person>\\n== Administrator\" <<person>> as admin\nrectangle \"==Sample System\" <<system_boundary>><<boundary>> as c1 {\n  rectangle \"== Web Application\\n//<size:12>[C#, ASP.NET Core 2.1 MVC]</size>//\\n\\nAllows users to compare multiple Twitter timelines\" <<container>> as web_app\n}\nrectangle \"== Twitter\" <<system>> as twitter\nadmin -->> web_app :**Uses**\\n//<size:12>[HTTPS]</size>//\nweb_app -->> twitter :**Gets tweets from**\\n//<size:12>[HTTPS]</size>//\n@enduml",
        );
        assert!(
            d.entities
                .iter()
                .any(|e| e.id == "admin" && e.name.contains("Administrator")),
            "{:?}",
            d.entities
        );
        assert!(
            d.entities
                .iter()
                .any(|e| e.id == "web_app" && e.name.contains("Web Application")),
            "{:?}",
            d.entities
        );
        assert!(
            d.entities
                .iter()
                .any(|e| e.id == "twitter" && e.name.contains("Twitter")),
            "{:?}",
            d.entities
        );
        assert!(
            d.groups
                .iter()
                .any(|g| g.id == "c1" && g.name.contains("Sample System")),
            "{:?}",
            d.groups
        );
        assert!(
            d.links
                .iter()
                .any(|l| l.from == "admin" && l.to == "web_app" && l.label.contains("Uses")),
            "{:?}",
            d.links
        );
        assert!(
            d.links.iter().any(|l| l.from == "web_app"
                && l.to == "twitter"
                && l.label.contains("Gets tweets from")),
            "{:?}",
            d.links
        );
    }

    #[test]
    fn test_double_head_forward_arrow() {
        let d = parse("@startuml\ncomponent A\ncomponent B\nA -->> B : uses\n@enduml");
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].from, "A");
        assert_eq!(d.links[0].to, "B");
        assert_eq!(d.links[0].label, "uses");
    }

    #[test]
    fn test_multi_stereotype_group_alias_decl() {
        let decl = try_parse_entity_decl(
            r#"rectangle "==Sample System" <<system_boundary>><<boundary>> as c1 {"#,
        )
        .expect("must parse");
        assert_eq!(decl.name, "==Sample System");
        assert_eq!(decl.id, "c1");
        assert_eq!(decl.stereotype.as_deref(), Some("system_boundary"));
        assert!(decl.opens_group);
    }

    // 13. Skip skinparam
    #[test]
    fn test_skip_skinparam() {
        let d = parse(
            "@startuml\nskinparam component {\n  BackgroundColor #FEFECE\n}\ncomponent A\n@enduml",
        );
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "A");
    }

    // 14. Skip sprite
    #[test]
    fn test_skip_sprite() {
        let d = parse("@startuml\nsprite $bp [16x16/16] {\nFFFF\nFFFF\n}\ncomponent A\n@enduml");
        assert_eq!(d.entities.len(), 1);
    }

    // 14b. Single-line skinparam (preprocessor stdlib emit) must NOT consume
    // following declarations as block content. Reproduces a regression where
    // C4 stdlib's compact one-line skinparam would swallow the rest of the
    // file.
    #[test]
    fn test_skip_single_line_skinparam_block() {
        let src = concat!(
            "@startuml\n",
            "skinparam rectangle<<boundary>> {    FontColor #444444    BackgroundColor transparent    BorderColor #444444}skinparam database<<boundary>> {    FontColor #444444}\n",
            "rectangle \"Outer\" <<system_boundary>> as c1 {\n",
            "  rectangle \"Inner\" <<container>> as c2\n",
            "}\n",
            "@enduml\n",
        );
        let d = parse(src);
        // 2 entities (c1 group container + c2 inner) and 1 group
        assert_eq!(d.entities.len(), 2);
        assert_eq!(d.groups.len(), 1);
        assert_eq!(d.groups[0].id, "c1");
        assert_eq!(d.groups[0].children, vec!["c2".to_string()]);
    }

    // 15. Multiple arrows
    #[test]
    fn test_multiple_arrows() {
        let d = parse(
            "@startuml\ncomponent A\ncomponent B\ncomponent C\nA -> B\nB -> C\nC -> A\n@enduml",
        );
        assert_eq!(d.links.len(), 3);
    }

    // 16. Component shorthand [Name]
    #[test]
    fn test_component_shorthand() {
        let d = parse("@startuml\n[MyComp]\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "MyComp");
        assert_eq!(d.entities[0].kind, ComponentKind::Component);
    }

    // 17. Full arrows fixture
    #[test]
    fn test_arrows_fixture() {
        let src = r#"@startuml
component A
component B
component C
component D

A -up-> B : > up arrow
B <-down- A : < up arrow works
B -right-> C : > right arrow works
C -down-> D : > down arrow works
D -left-> A : > left arrow
A <-right- D : < left arrow works
@enduml"#;
        let d = parse(src);
        assert_eq!(d.entities.len(), 4);
        assert_eq!(d.links.len(), 6);
    }

    // 18. Note with newline escape
    #[test]
    fn test_note_newline_escape() {
        let d = parse("@startuml\ncomponent test\nnote top of test: first\\nsecond\n@enduml");
        assert_eq!(d.notes[0].text, "first\nsecond");
    }

    // 19. Empty diagram
    #[test]
    fn test_empty_diagram() {
        let d = parse("@startuml\n@enduml");
        assert!(d.entities.is_empty());
        assert!(d.links.is_empty());
    }

    // 20. file block with description
    #[test]
    fn test_file_description_block() {
        let d = parse("@startuml\nfile report [\nline 1\nline 2\n]\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "report");
        assert_eq!(d.entities[0].kind, ComponentKind::Rectangle);
        assert_eq!(
            d.entities[0].description,
            vec!["line 1".to_string(), "line 2".to_string()]
        );
    }

    // 21. Long arrow
    #[test]
    fn test_long_arrow() {
        let d = parse("@startuml\ncomponent A\ncomponent B\nA --> B : long\n@enduml");
        assert_eq!(d.links.len(), 1);
        assert!(!d.links[0].dashed);
        assert_eq!(d.links[0].label, "long");
    }

    // 22. Nested rectangle group
    #[test]
    fn test_nested_rectangle() {
        let src = r#"@startuml
rectangle "Outer" {
  rectangle "Inner" {
    component A
  }
}
@enduml"#;
        let d = parse(src);
        assert_eq!(d.groups.len(), 2);
        let inner = d.groups.iter().find(|g| g.name == "Inner").unwrap();
        assert!(inner.children.contains(&"A".to_string()));
    }

    // 23. Arrow between shorthand components
    #[test]
    fn test_arrow_shorthand_implicit() {
        let d = parse("@startuml\nsrc -> tgt\n@enduml");
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.entities.len(), 2);
    }

    // 24. jaws5 fixture (complex)
    #[test]
    fn test_jaws5_fixture() {
        let src = r#"@startuml
sprite $businessProcess [16x16/16] {
FFFFFFFFFFFFFFFF
FFFFFFFFFFFFFFFF
FFFFFFFFFFFFFFFF
FFFFFFFFFFFFFFFF
FFFFFFFFFF0FFFFF
FFFFFFFFFF00FFFF
FF00000000000FFF
FF000000000000FF
FF00000000000FFF
FFFFFFFFFF00FFFF
FFFFFFFFFF0FFFFF
FFFFFFFFFFFFFFFF
FFFFFFFFFFFFFFFF
FFFFFFFFFFFFFFFF
FFFFFFFFFFFFFFFF
FFFFFFFFFFFFFFFF
}

rectangle " End to End%newline()business process" <<$businessProcess>> {
 rectangle "inner process 1" <<$businessProcess>> as src
 rectangle "inner process 2" <<$businessProcess>> as tgt
 src -> tgt
}
@enduml"#;
        let d = parse(src);
        // Should have 3 entities (outer group + 2 inner)
        assert!(d.entities.len() >= 3);
        assert!(!d.groups.is_empty());
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].from, "src");
        assert_eq!(d.links[0].to, "tgt");
    }

    // 24. xmi0001 fixture
    #[test]
    fn test_xmi0001_fixture() {
        let src = r#"@startuml
component test
note top of test: first line1\nsecond line2
note bottom of test
    first line3
    second line4
end note
@enduml"#;
        let d = parse(src);
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.notes.len(), 2);
        assert_eq!(d.notes[0].position, "top");
        assert_eq!(d.notes[0].text, "first line1\nsecond line2");
        assert_eq!(d.notes[1].position, "bottom");
        assert_eq!(d.notes[1].text, "first line3\nsecond line4");
    }

    // 25. Expand newlines helpers
    #[test]
    fn test_expand_newlines() {
        // Inline text: all forms expand to real newlines
        assert_eq!(expand_newlines("a\\nb"), "a\nb");
        assert_eq!(expand_newlines("a\u{E100}b"), "a\nb");
        assert_eq!(expand_newlines("a%chr(10)b"), "a\nb");
    }

    #[test]
    fn test_expand_body_newlines() {
        // Bracket-display body: literal `\n` preserved (Java compat).
        // U+E100 placeholder is also preserved so downstream table parsers
        // can keep it as cell content; text renderers split on it later.
        assert_eq!(expand_body_newlines("a\\nb"), "a\\nb");
        assert_eq!(expand_body_newlines("a\u{E100}b"), "a\u{E100}b");
        assert_eq!(expand_body_newlines("a%chr(10)b"), "a\nb");
    }

    // 26. Node kind
    #[test]
    fn test_node_kind() {
        let d = parse("@startuml\nnode MyNode\n@enduml");
        assert_eq!(d.entities[0].kind, ComponentKind::Node);
    }

    // 27. Database kind
    #[test]
    fn test_database_kind() {
        let d = parse("@startuml\ndatabase MyDB\n@enduml");
        assert_eq!(d.entities[0].kind, ComponentKind::Database);
    }

    // 28. Cloud kind
    #[test]
    fn test_cloud_kind() {
        let d = parse("@startuml\ncloud MyCloud\n@enduml");
        assert_eq!(d.entities[0].kind, ComponentKind::Cloud);
    }

    // 29. Component shorthand with alias
    #[test]
    fn test_shorthand_with_alias() {
        let d = parse("@startuml\n[My Component] as mc\n@enduml");
        assert_eq!(d.entities[0].name, "My Component");
        assert_eq!(d.entities[0].id, "mc");
    }

    // 30. Direction parsing
    #[test]
    fn test_direction_left_to_right() {
        let d =
            parse("@startuml\nleft to right direction\ncomponent A\ncomponent B\nA -> B\n@enduml");
        assert_eq!(d.direction, crate::model::Direction::LeftToRight);
    }

    // 31. Parent tracking for nested entities
    #[test]
    fn test_parent_tracking() {
        let d = parse("@startuml\nrectangle Outer {\n  component Inner\n}\n@enduml");
        let inner = d.entities.iter().find(|e| e.name == "Inner").unwrap();
        assert_eq!(inner.parent, Some("Outer".to_string()));
    }

    // 32. Deployment: artifact keyword
    #[test]
    fn test_artifact_kind() {
        let d = parse("@startuml\nartifact \"webapp.war\" as war\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "webapp.war");
        assert_eq!(d.entities[0].id, "war");
        assert_eq!(d.entities[0].kind, ComponentKind::Artifact);
    }

    // 33. Deployment: storage keyword
    #[test]
    fn test_storage_kind() {
        let d = parse("@startuml\nstorage \"File Storage\" as fs\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "File Storage");
        assert_eq!(d.entities[0].id, "fs");
        assert_eq!(d.entities[0].kind, ComponentKind::Storage);
    }

    // 34. Deployment: folder keyword
    #[test]
    fn test_folder_kind() {
        let d = parse("@startuml\nfolder \"Shared Docs\" as docs\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "Shared Docs");
        assert_eq!(d.entities[0].id, "docs");
        assert_eq!(d.entities[0].kind, ComponentKind::Folder);
    }

    // 35. Deployment: frame keyword as group
    #[test]
    fn test_frame_kind_group() {
        let d = parse("@startuml\nframe \"DMZ\" {\n  component proxy\n}\n@enduml");
        assert_eq!(d.groups.len(), 1);
        assert_eq!(d.groups[0].name, "DMZ");
        assert_eq!(d.groups[0].kind, ComponentKind::Frame);
        assert!(d.groups[0].children.contains(&"proxy".to_string()));
    }

    // 36. Deployment: agent keyword
    #[test]
    fn test_agent_kind() {
        let d = parse("@startuml\nagent \"Monitoring\" as mon\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "Monitoring");
        assert_eq!(d.entities[0].id, "mon");
        assert_eq!(d.entities[0].kind, ComponentKind::Agent);
    }

    // 37. Deployment: stack keyword
    #[test]
    fn test_stack_kind() {
        let d = parse("@startuml\nstack \"Docker\" as docker\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "Docker");
        assert_eq!(d.entities[0].id, "docker");
        assert_eq!(d.entities[0].kind, ComponentKind::Stack);
    }

    // 38. Deployment: queue keyword
    #[test]
    fn test_queue_kind() {
        let d = parse("@startuml\nqueue \"Message Queue\" as mq\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "Message Queue");
        assert_eq!(d.entities[0].id, "mq");
        assert_eq!(d.entities[0].kind, ComponentKind::Queue);
    }

    // 39. Deployment: mixed keywords in one diagram
    #[test]
    fn test_deployment_mixed_kinds() {
        let src = "@startuml\nartifact app.jar\nstorage volume\nfolder config\nagent watchdog\nstack infra\nqueue events\n@enduml";
        let d = parse(src);
        assert_eq!(d.entities.len(), 6);
        assert!(d.entities.iter().any(|e| e.kind == ComponentKind::Artifact));
        assert!(d.entities.iter().any(|e| e.kind == ComponentKind::Storage));
        assert!(d.entities.iter().any(|e| e.kind == ComponentKind::Folder));
        assert!(d.entities.iter().any(|e| e.kind == ComponentKind::Agent));
        assert!(d.entities.iter().any(|e| e.kind == ComponentKind::Stack));
        assert!(d.entities.iter().any(|e| e.kind == ComponentKind::Queue));
    }

    // 40. Deployment: artifact nested inside node group
    #[test]
    fn test_artifact_inside_node() {
        let src =
            "@startuml\nnode \"App Server\" as app {\n  artifact \"app.jar\" as jar\n}\n@enduml";
        let d = parse(src);
        assert_eq!(d.groups.len(), 1);
        let jar = d.entities.iter().find(|e| e.id == "jar").unwrap();
        assert_eq!(jar.kind, ComponentKind::Artifact);
        assert_eq!(jar.parent, Some("app".to_string()));
    }

    // --- Archimate tests ---

    #[test]
    fn test_archimate_basic() {
        let d = parse(
            "@startuml\narchimate #438DD5 \"My App\" <<application-component>> as app\n@enduml",
        );
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "My App");
        assert_eq!(d.entities[0].id, "app");
        assert_eq!(d.entities[0].kind, ComponentKind::Archimate);
        assert_eq!(
            d.entities[0].stereotype,
            Some("application-component".to_string())
        );
        assert!(d.entities[0].color.is_some());
    }

    #[test]
    fn test_archimate_with_link() {
        let d = parse(
            "@startuml\narchimate #438DD5 \"App\" <<application-component>> as app\narchimate #85BBF0 \"DB\" <<technology-artifact>> as db\napp --> db\n@enduml",
        );
        assert_eq!(d.entities.len(), 2);
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].from, "app");
        assert_eq!(d.links[0].to, "db");
    }

    #[test]
    fn test_archimate_unquoted_name() {
        let d =
            parse("@startuml\narchimate #F5DEAA MyResource <<strategy-resource>> as res\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "MyResource");
        assert_eq!(d.entities[0].id, "res");
        assert_eq!(d.entities[0].kind, ComponentKind::Archimate);
    }

    #[test]
    fn test_archimate_no_color() {
        let d = parse("@startuml\narchimate \"App\" <<application-component>> as app\n@enduml");
        assert_eq!(d.entities.len(), 1);
        assert_eq!(d.entities[0].name, "App");
        assert_eq!(d.entities[0].id, "app");
        assert_eq!(d.entities[0].kind, ComponentKind::Archimate);
    }
}
