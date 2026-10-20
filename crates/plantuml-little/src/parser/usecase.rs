use log::{debug, trace, warn};

use crate::model::usecase::{
    UseCase, UseCaseActor, UseCaseBoundary, UseCaseDiagram, UseCaseLink, UseCaseLinkStyle,
    UseCaseNote,
};
use crate::model::Direction;
use crate::Result;

// ---------------------------------------------------------------------------
// Parse mode for multi-line constructs
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum ParseMode {
    Normal,
    /// Inside a `note ... end note` multi-line block
    NoteBlock {
        position: String,
        target: Option<String>,
        lines: Vec<String>,
    },
    /// Inside a `<style>...</style>` block (skip)
    StyleBlock,
    /// Inside a `skinparam { ... }` block (skip); depth tracks braces
    SkinparamBlock {
        depth: usize,
    },
}

// ---------------------------------------------------------------------------
// Boundary stack frame for nested package/rectangle blocks
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct BoundaryFrame {
    name: String,
    id: String,
    children: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Parse a PlantUML use case diagram source.
pub fn parse_usecase_diagram(source: &str) -> Result<UseCaseDiagram> {
    let block = super::common::extract_block(source).unwrap_or_else(|| source.to_string());
    debug!("usecase: extracted block, {} lines", block.lines().count());

    let mut actors: Vec<UseCaseActor> = Vec::new();
    let mut usecases: Vec<UseCase> = Vec::new();
    let mut links: Vec<UseCaseLink> = Vec::new();
    let mut boundaries: Vec<UseCaseBoundary> = Vec::new();
    let mut notes: Vec<UseCaseNote> = Vec::new();
    let mut direction = Direction::default();
    let mut mode = ParseMode::Normal;
    let mut boundary_stack: Vec<BoundaryFrame> = Vec::new();

    let lines: Vec<&str> = block.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line_num = i + 1;
        let raw = lines[i];
        let line = raw.trim();
        i += 1;

        // --- mode dispatch ---
        match mode {
            ParseMode::StyleBlock => {
                if line == "</style>" || line.starts_with("</style>") {
                    trace!("line {line_num}: leaving <style> block");
                    mode = ParseMode::Normal;
                } else {
                    trace!("line {line_num}: skipping style content");
                }
                continue;
            }
            ParseMode::SkinparamBlock { ref mut depth } => {
                *depth = depth
                    .saturating_add(line.matches('{').count())
                    .saturating_sub(line.matches('}').count());
                if *depth == 0 {
                    trace!("line {line_num}: leaving skinparam block");
                    mode = ParseMode::Normal;
                } else {
                    trace!("line {line_num}: skipping skinparam content");
                }
                continue;
            }
            ParseMode::NoteBlock {
                ref position,
                ref target,
                ref mut lines,
            } => {
                let lower = line.to_lowercase();
                if lower == "end note" || lower == "endnote" {
                    let text = lines.join("\n");
                    debug!(
                        "line {}: closing note block (target={:?}), {} lines",
                        line_num,
                        target,
                        text.lines().count()
                    );
                    notes.push(UseCaseNote {
                        text,
                        position: position.clone(),
                        target: target.clone(),
                    });
                    mode = ParseMode::Normal;
                } else {
                    lines.push(line.to_string());
                }
                continue;
            }
            ParseMode::Normal => {}
        }

        // --- skip empty lines and comments ---
        if line.is_empty() || line.starts_with('\'') || line.starts_with("/'") {
            continue;
        }

        // --- skip directives ---
        if line.starts_with("hide ") || line.starts_with("show ") {
            trace!("line {line_num}: skipping hide/show directive");
            continue;
        }
        if line == "<style>" || line.starts_with("<style>") {
            if !line.contains("</style>") {
                mode = ParseMode::StyleBlock;
            }
            trace!("line {line_num}: entering/skipping style block");
            continue;
        }
        if line.starts_with("skinparam ") || line.starts_with("skinparam{") {
            let open = line.matches('{').count();
            let close = line.matches('}').count();
            let depth = open.saturating_sub(close);
            if depth > 0 {
                mode = ParseMode::SkinparamBlock { depth };
                trace!("line {line_num}: entering skinparam block (depth={depth})");
            }
            // single-line skinparam without braces, just skip
            continue;
        }

        // --- direction ---
        {
            let lower = line.to_lowercase();
            if lower == "left to right direction" {
                direction = Direction::LeftToRight;
                debug!("line {line_num}: direction=LeftToRight");
                continue;
            }
            if lower == "top to bottom direction" {
                direction = Direction::TopToBottom;
                debug!("line {line_num}: direction=TopToBottom");
                continue;
            }
        }

        // --- closing brace ends boundary block ---
        if line == "}" {
            if let Some(frame) = boundary_stack.pop() {
                debug!("line {}: close boundary '{}'", line_num, frame.name);
                boundaries.push(UseCaseBoundary {
                    id: frame.id,
                    name: frame.name,
                    children: frame.children,
                });
            }
            continue;
        }

        // --- note syntax ---
        if let Some(note) = try_parse_note_start(line) {
            match note {
                NoteStart::SingleLine {
                    text,
                    position,
                    target,
                } => {
                    debug!("line {line_num}: single-line note (target={target:?})");
                    notes.push(UseCaseNote {
                        text,
                        position,
                        target,
                    });
                }
                NoteStart::MultiLine { position, target } => {
                    debug!("line {line_num}: start multi-line note (target={target:?})");
                    mode = ParseMode::NoteBlock {
                        position,
                        target,
                        lines: Vec::new(),
                    };
                }
            }
            continue;
        }

        // --- actor "Name" as id / actor id ---
        if let Some(mut actor) = try_parse_actor_decl(line) {
            debug!(
                "line {}: actor id='{}' name='{}'",
                line_num, actor.id, actor.name
            );
            check_enclosing_clash(line_num, &actor.id, &boundary_stack)?;
            if !actors.iter().any(|a| a.id == actor.id) {
                actor.source_line = Some(line_num);
                if let Some(parent) = boundary_stack.last_mut() {
                    parent.children.push(actor.id.clone());
                }
                actors.push(actor);
            }
            continue;
        }

        // --- :Actor Name: colon syntax ---
        if let Some(mut actor) = try_parse_actor_colon(line) {
            debug!(
                "line {}: colon-actor id='{}' name='{}'",
                line_num, actor.id, actor.name
            );
            check_enclosing_clash(line_num, &actor.id, &boundary_stack)?;
            if !actors.iter().any(|a| a.id == actor.id) {
                actor.source_line = Some(line_num);
                if let Some(parent) = boundary_stack.last_mut() {
                    parent.children.push(actor.id.clone());
                }
                actors.push(actor);
            }
            continue;
        }

        // --- `(a) --> (b)` linkage lines ---
        //
        // A parenthesised endpoint followed by an arrow is a link, not a
        // `(Name)` declaration: Java resolves `(a)` against the objects
        // declared so far and allows it to name the enclosing group. Handled
        // before the declaration branches so the arrow is not dropped.
        if paren_line_starts_link(line) {
            if let Some(link) = try_parse_link(line) {
                // Java reads parenthesised endpoints as use cases (not actors)
                // and keeps them inside the enclosing boundary. Declaring them
                // here also keeps the actor heuristic out of the picture.
                for name in paren_link_endpoint_names(line) {
                    let id = name_to_id(&name);
                    if is_boundary_id(&id, &boundary_stack, &boundaries)
                        || actors.iter().any(|a| a.id == id)
                        || usecases.iter().any(|u| u.id == id)
                    {
                        continue;
                    }
                    let parent_id = boundary_stack.last().map(|f| f.id.clone());
                    if let Some(parent) = boundary_stack.last_mut() {
                        parent.children.push(id.clone());
                    }
                    usecases.push(UseCase {
                        id,
                        name: name.clone(),
                        code: name,
                        stereotype: None,
                        color: None,
                        parent: parent_id,
                        source_line: Some(line_num),
                    });
                }
                record_link(
                    line_num,
                    link,
                    &mut actors,
                    &mut usecases,
                    &mut links,
                    &boundary_stack,
                    &boundaries,
                );
                continue;
            }
        }

        // --- usecase "Name" as id / usecase id ---
        if let Some(mut uc) = try_parse_usecase_decl(line) {
            debug!(
                "line {}: usecase id='{}' name='{}'",
                line_num, uc.id, uc.name
            );
            check_enclosing_clash(line_num, &uc.id, &boundary_stack)?;
            let parent_id = boundary_stack.last().map(|f| f.id.clone());
            uc.parent = parent_id.clone();
            uc.source_line = Some(line_num);
            if !usecases.iter().any(|u| u.id == uc.id) {
                if let Some(parent) = boundary_stack.last_mut() {
                    parent.children.push(uc.id.clone());
                }
                usecases.push(uc);
            }
            continue;
        }

        // --- (Use Case Name) or (Name) as id shorthand ---
        if let Some(mut uc) = try_parse_usecase_paren(line) {
            debug!(
                "line {}: paren-usecase id='{}' name='{}'",
                line_num, uc.id, uc.name
            );
            check_enclosing_clash(line_num, &uc.id, &boundary_stack)?;
            let parent_id = boundary_stack.last().map(|f| f.id.clone());
            uc.parent = parent_id.clone();
            uc.source_line = Some(line_num);
            if !usecases.iter().any(|u| u.id == uc.id) {
                if let Some(parent) = boundary_stack.last_mut() {
                    parent.children.push(uc.id.clone());
                }
                usecases.push(uc);
            }
            continue;
        }

        // --- package "Name" { or rectangle "Name" { ---
        if let Some(frame) = try_parse_boundary_open(line) {
            debug!(
                "line {}: open boundary '{}' (id='{}')",
                line_num, frame.name, frame.id
            );
            boundary_stack.push(frame);
            continue;
        }

        // --- relationship arrows ---
        if let Some(link) = try_parse_link(line) {
            record_link(
                line_num,
                link,
                &mut actors,
                &mut usecases,
                &mut links,
                &boundary_stack,
                &boundaries,
            );
            continue;
        }

        warn!("line {line_num}: unrecognized line: {line:?}");
    }

    // Close any unclosed boundary frames
    while let Some(frame) = boundary_stack.pop() {
        warn!(
            "usecase: unclosed boundary '{}', closing implicitly",
            frame.name
        );
        boundaries.push(UseCaseBoundary {
            id: frame.id,
            name: frame.name,
            children: frame.children,
        });
    }

    debug!(
        "usecase: parsed {} actors, {} usecases, {} links, {} boundaries, {} notes",
        actors.len(),
        usecases.len(),
        links.len(),
        boundaries.len(),
        notes.len()
    );

    Ok(UseCaseDiagram {
        actors,
        usecases,
        links,
        boundaries,
        notes,
        direction,
    })
}

// ---------------------------------------------------------------------------
// Helper: ensure entity (actor or usecase) exists when referenced in a link
// ---------------------------------------------------------------------------

/// True when `id` names a boundary, still open or already closed.
fn is_boundary_id(id: &str, open: &[BoundaryFrame], closed: &[UseCaseBoundary]) -> bool {
    open.iter().any(|f| f.id == id) || closed.iter().any(|b| b.id == id)
}

/// Record a parsed link and auto-create endpoints that are not declared yet.
///
/// An endpoint naming a boundary stays a reference to that boundary: Java
/// resolves `(a) .> (aa)` inside `rectangle a {` to the boundary itself, so no
/// phantom actor/usecase may be created for it.
fn record_link(
    line_num: usize,
    mut link: UseCaseLink,
    actors: &mut Vec<UseCaseActor>,
    usecases: &mut Vec<UseCase>,
    links: &mut Vec<UseCaseLink>,
    open_boundaries: &[BoundaryFrame],
    closed_boundaries: &[UseCaseBoundary],
) {
    debug!(
        "line {}: link '{}' {:?} '{}' label='{}'",
        line_num, link.from, link.style, link.to, link.label
    );
    link.source_line = Some(line_num);
    for endpoint in [&link.from, &link.to] {
        if is_boundary_id(endpoint, open_boundaries, closed_boundaries) {
            continue;
        }
        ensure_entity_exists(endpoint, actors, usecases);
    }
    links.push(link);
}

/// Reject a leaf declaration that reuses the id of the group it is declared in.
///
/// Java reports `Error line N` for `rectangle a { (a) }`, `{ usecase a }`,
/// `{ actor a }` and for `rectangle web { component web }`. The same shape is
/// what used to build a self-referential parent chain (and an uncatchable
/// stack overflow) here.
fn check_enclosing_clash(line: usize, id: &str, open: &[BoundaryFrame]) -> Result<()> {
    if open.last().is_some_and(|frame| frame.id == id) {
        return Err(crate::Error::Parse {
            line,
            column: Some(1),
            message: format!(
                "'{id}' is already used by the enclosing group; rename the entity or the boundary"
            ),
        });
    }
    Ok(())
}

/// When a link references an id that has not been explicitly declared, we
/// auto-create a minimal entity.  We use a simple heuristic: if the raw id
/// looks like an actor name (bare word without spaces), create an actor;
/// otherwise create a usecase.  This matches PlantUML's own behaviour.
fn ensure_entity_exists(id: &str, actors: &mut Vec<UseCaseActor>, usecases: &mut Vec<UseCase>) {
    let already = actors.iter().any(|a| a.id == id) || usecases.iter().any(|u| u.id == id);
    if already {
        return;
    }
    // Heuristic: if no spaces in name treat as actor; otherwise usecase.
    // In practice both are valid so we default to usecase for multi-word ids.
    if id.contains(' ') {
        let uc_id = name_to_id(id);
        debug!("auto-creating usecase '{id}' (id='{uc_id}')");
        usecases.push(UseCase {
            id: uc_id,
            name: id.to_string(),
            code: id.to_string(),
            stereotype: None,
            color: None,
            parent: None,
            source_line: None,
        });
    } else {
        debug!("auto-creating actor '{id}'");
        actors.push(UseCaseActor {
            id: id.to_string(),
            name: id.to_string(),
            code: id.to_string(),
            stereotype: None,
            color: None,
            source_line: None,
        });
    }
}

// ---------------------------------------------------------------------------
// Name → id normalisation
// ---------------------------------------------------------------------------

/// Convert a display name to a stable id by lowercasing and replacing spaces
/// (and other non-alnum chars) with underscores.
fn name_to_id(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

// ---------------------------------------------------------------------------
// Stereotype / color helpers
// ---------------------------------------------------------------------------

/// Parse trailing `<<stereotype>>` and/or `#color` tokens from the tail of a
/// declaration line.  Returns `(rest, stereotype, color)`.
fn parse_stereotype_color(s: &str) -> (String, Option<String>, Option<String>) {
    let mut rest = s.to_string();
    let mut stereotype: Option<String> = None;
    let mut color: Option<String> = None;

    // Extract <<...>>
    if let Some(st_start) = rest.find("<<") {
        if let Some(st_end) = rest[st_start..].find(">>") {
            let st = rest[st_start + 2..st_start + st_end].trim().to_string();
            if !st.is_empty() {
                stereotype = Some(st);
            }
            rest = format!("{}{}", &rest[..st_start], &rest[st_start + st_end + 2..]);
            rest = rest.trim().to_string();
        }
    }

    // Extract trailing #color token
    if let Some(hash_pos) = rest.rfind('#') {
        let candidate = rest[hash_pos + 1..].trim();
        // A color token is alphanumeric (possibly with hex digits)
        if !candidate.is_empty() && candidate.chars().all(char::is_alphanumeric) {
            color = Some(candidate.to_string());
            rest = rest[..hash_pos].trim().to_string();
        }
    }

    (rest, stereotype, color)
}

// ---------------------------------------------------------------------------
// Actor declaration parsing
// ---------------------------------------------------------------------------

/// Try to parse `actor "Name" as id`, `actor "Name"`, or `actor id` syntax.
fn try_parse_actor_decl(line: &str) -> Option<UseCaseActor> {
    let rest = line.strip_prefix("actor ")?.trim();
    // rest may be: `"Name" as id <<st>> #color`, `"Name" <<st>>`, `id <<st>>`

    let (base, stereotype, color) = parse_stereotype_color(rest);
    let base = base.trim();

    let (name, id) = if let Some(quoted) = base.strip_prefix('"') {
        // Quoted name
        if let Some(end_q) = quoted.find('"') {
            let name = &quoted[..end_q];
            let after = quoted[end_q + 1..].trim();
            let id = if let Some(alias) = after.strip_prefix("as ") {
                alias.trim().to_string()
            } else {
                name_to_id(name)
            };
            (name.to_string(), id)
        } else {
            return None;
        }
    } else {
        // Bare word(s)
        if let Some(as_pos) = base.find(" as ") {
            let name = base[..as_pos].trim().to_string();
            let id = base[as_pos + 4..].trim().to_string();
            (name.clone(), id)
        } else {
            // single token is both name and id
            let id = base.to_string();
            let name = id.clone();
            (name, id)
        }
    };

    if name.is_empty() || id.is_empty() {
        return None;
    }

    // For actor decl with "as", code = id (alias); otherwise code = name
    let code = id.clone();
    Some(UseCaseActor {
        id,
        name,
        code,
        stereotype,
        color,
        source_line: None,
    })
}

/// Try to parse `:Actor Name:` colon syntax.
/// Must NOT match activity diagram actions which look like `:name;`.
fn try_parse_actor_colon(line: &str) -> Option<UseCaseActor> {
    if !line.starts_with(':') {
        return None;
    }
    // Find the closing colon (the second `:` after the opening one).
    // Handles both `:Name:` and `:Name: as ALIAS` syntax.
    let rest = &line[1..];
    let close_pos = rest.find(':')?;
    let name = rest[..close_pos].trim();
    if name.is_empty() {
        return None;
    }
    // Activity actions end with `;` or `|`; colon-actors have a closing `:`
    // followed by optional `as ALIAS`, stereotype, or color — not `;`.
    let after_close = rest[close_pos + 1..].trim();
    if after_close.is_empty() {
        // Simple `:Name:` form — code = display name (no alias)
        let id = name_to_id(name);
        return Some(UseCaseActor {
            id,
            name: name.to_string(),
            code: name.to_string(),
            stereotype: None,
            color: None,
            source_line: None,
        });
    }
    // Check for `as ALIAS` after closing colon
    let (base, stereotype, color) = parse_stereotype_color(after_close);
    let base = base.trim();
    if let Some(alias) = base.strip_prefix("as ") {
        let alias = alias.trim();
        if alias.is_empty() {
            return None;
        }
        return Some(UseCaseActor {
            id: alias.to_string(),
            name: name.to_string(),
            code: alias.to_string(),
            stereotype,
            color,
            source_line: None,
        });
    }
    // Unrecognized suffix — not a colon-actor
    None
}

// ---------------------------------------------------------------------------
// UseCase declaration parsing
// ---------------------------------------------------------------------------

/// Try to parse `usecase "Name" as id`, `usecase "Name"`, or `usecase id`.
fn try_parse_usecase_decl(line: &str) -> Option<UseCase> {
    let rest = line.strip_prefix("usecase ")?.trim();

    let (base, stereotype, color) = parse_stereotype_color(rest);
    let base = base.trim();

    let (name, id) = if let Some(quoted) = base.strip_prefix('"') {
        if let Some(end_q) = quoted.find('"') {
            let name = &quoted[..end_q];
            let after = quoted[end_q + 1..].trim();
            let id = if let Some(alias) = after.strip_prefix("as ") {
                alias.trim().to_string()
            } else {
                name_to_id(name)
            };
            (name.to_string(), id)
        } else {
            return None;
        }
    } else if let Some(as_pos) = base.find(" as ") {
        let name = base[..as_pos].trim().to_string();
        let id = base[as_pos + 4..].trim().to_string();
        (name.clone(), id)
    } else {
        let id = base.to_string();
        (id.clone(), id)
    };

    if name.is_empty() || id.is_empty() {
        return None;
    }

    let code = id.clone();
    Some(UseCase {
        id,
        name,
        code,
        stereotype,
        color,
        parent: None,
        source_line: None,
    })
}

/// Try to parse `(Name)` or `(Name) as id` use-case shorthand.
fn try_parse_usecase_paren(line: &str) -> Option<UseCase> {
    if !line.starts_with('(') {
        return None;
    }
    let close = line.find(')')?;
    let inner = line[1..close].trim();
    if inner.is_empty() {
        return None;
    }
    let after = line[close + 1..].trim();
    let (base_after, stereotype, color) = parse_stereotype_color(after);
    let base_after = base_after.trim();

    let (id, code) = if let Some(alias) = base_after.strip_prefix("as ") {
        let a = alias.trim().to_string();
        (a.clone(), a)
    } else {
        (name_to_id(inner), inner.to_string())
    };

    Some(UseCase {
        id,
        name: inner.to_string(),
        code,
        stereotype,
        color,
        parent: None,
        source_line: None,
    })
}

// ---------------------------------------------------------------------------
// Boundary (package / rectangle) parsing
// ---------------------------------------------------------------------------

fn try_parse_boundary_open(line: &str) -> Option<BoundaryFrame> {
    let keyword = ["package ", "rectangle ", "node ", "cloud "];
    let rest = keyword.iter().find_map(|kw| line.strip_prefix(kw))?;

    // Must end with `{` (possibly after a name / label)
    if !line.trim_end().ends_with('{') {
        return None;
    }

    // Strip trailing `{`
    let without_brace = rest.trim_end_matches('{').trim();

    let (base, _stereotype, _color) = parse_stereotype_color(without_brace);
    let base = base.trim();

    let name = if let Some(q) = base.strip_prefix('"') {
        if let Some(eq) = q.find('"') {
            q[..eq].to_string()
        } else {
            base.to_string()
        }
    } else {
        // strip optional `as alias` suffix
        if let Some(as_pos) = base.find(" as ") {
            base[..as_pos].trim().to_string()
        } else {
            base.to_string()
        }
    };

    if name.is_empty() {
        return None;
    }

    let id = name_to_id(&name);
    Some(BoundaryFrame {
        name,
        id,
        children: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Relationship / link parsing
// ---------------------------------------------------------------------------

/// Result of parsing an arrow declaration.
struct LinkParseResult {
    from: String,
    to: String,
    style: UseCaseLinkStyle,
    direction_hint: Option<String>,
    /// True when the arrow goes right-to-left (reversed)
    reversed: bool,
}

/// Try to parse a relationship line.
/// Normalize a link endpoint reference to its canonical id.
/// Handles colon-actor syntax `:Name:` and paren-usecase syntax `(Name)`.
fn normalize_link_endpoint(raw: &str) -> String {
    let s = raw.trim();
    // Colon-actor: `:Name:` → name_to_id(Name)
    if s.starts_with(':') && s.ends_with(':') && s.len() > 2 {
        let inner = s[1..s.len() - 1].trim();
        return name_to_id(inner);
    }
    // Paren-usecase: `(Name)` → name_to_id(Name)
    if s.starts_with('(') && s.ends_with(')') && s.len() > 2 {
        let inner = s[1..s.len() - 1].trim();
        return name_to_id(inner);
    }
    // Already an id (alias)
    s.to_string()
}

fn try_parse_link(line: &str) -> Option<UseCaseLink> {
    // Strategy: split on ` : ` to separate arrow+endpoints from label
    let (arrow_part, label) = if let Some(colon) = line.find(" : ") {
        (line[..colon].trim(), line[colon + 3..].trim().to_string())
    } else {
        (line.trim(), String::new())
    };

    // Try each arrow pattern
    let r = try_split_arrow(arrow_part)?;
    let (from, to) = if r.reversed {
        (r.to, r.from)
    } else {
        (r.from, r.to)
    };

    Some(UseCaseLink {
        from: normalize_link_endpoint(&from),
        to: normalize_link_endpoint(&to),
        label: label.trim().to_string(),
        style: r.style,
        direction_hint: r.direction_hint,
        source_line: None, // set by caller
    })
}

/// Arrow patterns we recognise, longest-first so a longer arrow never loses to
/// one of its own prefixes (`-->` before `->`, `..>>` before `..>`).
///
/// Inheritance:  `--|>` / `<|--`
/// Dashed:       `..>` / `<..` / `..>>` / `<<..`
/// Dotted:       `.>` / `<.`
/// Association:  `-->` / `<--` / `->` / `<-`
///               with optional direction hints: `-up->` etc.
const ARROW_CANDIDATES: &[(&str, UseCaseLinkStyle, bool)] = &[
    // Inheritance
    ("--|>", UseCaseLinkStyle::Inheritance, false),
    ("<|--", UseCaseLinkStyle::Inheritance, true),
    ("-up-|>", UseCaseLinkStyle::Inheritance, false),
    ("-down-|>", UseCaseLinkStyle::Inheritance, false),
    ("-left-|>", UseCaseLinkStyle::Inheritance, false),
    ("-right-|>", UseCaseLinkStyle::Inheritance, false),
    // Dashed
    ("..>>", UseCaseLinkStyle::Dashed, false),
    ("<<..", UseCaseLinkStyle::Dashed, true),
    ("..>", UseCaseLinkStyle::Dashed, false),
    ("<..", UseCaseLinkStyle::Dashed, true),
    // Dotted (single dot)
    (".>", UseCaseLinkStyle::Dotted, false),
    ("<.", UseCaseLinkStyle::Dotted, true),
    // Association with direction hints
    ("-up->", UseCaseLinkStyle::Association, false),
    ("<-up-", UseCaseLinkStyle::Association, true),
    ("-down->", UseCaseLinkStyle::Association, false),
    ("<-down-", UseCaseLinkStyle::Association, true),
    ("-left->", UseCaseLinkStyle::Association, false),
    ("<-left-", UseCaseLinkStyle::Association, true),
    ("-right->", UseCaseLinkStyle::Association, false),
    ("<-right-", UseCaseLinkStyle::Association, true),
    // Plain association
    ("-->", UseCaseLinkStyle::Association, false),
    ("<--", UseCaseLinkStyle::Association, true),
    ("->", UseCaseLinkStyle::Association, false),
    ("<-", UseCaseLinkStyle::Association, true),
    // Undirected
    ("--", UseCaseLinkStyle::Association, false),
    ("..", UseCaseLinkStyle::Dotted, false),
];

/// Find the arrow token that starts a linkage line, if any.
///
/// The token must begin the text: `(a) -- (b)` is a link, while
/// `(a) as x <<..>>` is a declaration whose stereotype merely contains dots.
fn leading_arrow(s: &str) -> Option<&'static str> {
    ARROW_CANDIDATES
        .iter()
        .filter(|(pattern, _, _)| s.starts_with(*pattern))
        .max_by_key(|(pattern, _, _)| pattern.len())
        .map(|(pattern, _, _)| *pattern)
}

/// True when a line that starts with a parenthesised use case is actually a
/// linkage line (Java resolves `(a)` against the objects declared so far), not
/// a `(Name)` declaration.
///
/// Only the text between the closing `)` and the label separator is inspected,
/// so arrows appearing inside a quoted name or a stereotype cannot turn a
/// declaration into a link.
fn paren_line_starts_link(line: &str) -> bool {
    let mut in_quotes = false;
    let mut close = None;
    for (i, ch) in line.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ')' if !in_quotes => {
                close = Some(i);
                break;
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return false;
    };
    let after = &line[close + 1..];
    let arrow_part = after.split(" : ").next().unwrap_or(after).trim_start();
    // `(a) <<stereo>>` is a declaration; no linkage arrow starts with `<<`.
    if arrow_part.starts_with("<<") {
        return false;
    }
    leading_arrow(arrow_part).is_some()
}

/// Names of the parenthesised endpoints of a linkage line, in source order.
///
/// `(a) .> (aa) : <<a>>` yields `["a", "aa"]`; endpoints written as bare ids
/// are skipped (they keep the existing auto-create heuristic).
fn paren_link_endpoint_names(line: &str) -> Vec<String> {
    let arrow_part = line.split(" : ").next().unwrap_or(line);
    let Some((pos, pattern)) = ARROW_CANDIDATES
        .iter()
        .filter_map(|(p, _, _)| arrow_part.find(p).map(|i| (i, *p)))
        .min_by_key(|(i, p)| (*i, std::cmp::Reverse(p.len())))
    else {
        return Vec::new();
    };
    let sides = [
        arrow_part[..pos].trim(),
        arrow_part[pos + pattern.len()..].trim(),
    ];
    sides
        .iter()
        .filter_map(|side| {
            side.strip_prefix('(')?
                .strip_suffix(')')
                .map(|inner| inner.trim().to_string())
                .filter(|inner| !inner.is_empty())
        })
        .collect()
}

fn try_split_arrow(s: &str) -> Option<LinkParseResult> {
    // Collect direction hints embedded in dashes: `-up-`, `-down-`, `-left-`, `-right-`
    let dir_hints = ["up", "down", "left", "right"];

    for (pattern, style, rev) in ARROW_CANDIDATES {
        if let Some(pos) = s.find(pattern) {
            let left = s[..pos].trim();
            let right = s[pos + pattern.len()..].trim();
            if left.is_empty() || right.is_empty() {
                continue;
            }

            // Extract direction hint from pattern if present
            let dir_hint = dir_hints
                .iter()
                .find(|&&d| pattern.contains(d))
                .map(std::string::ToString::to_string);

            return Some(LinkParseResult {
                from: left.to_string(),
                to: right.to_string(),
                style: style.clone(),
                direction_hint: dir_hint,
                reversed: *rev,
            });
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Note parsing
// ---------------------------------------------------------------------------

enum NoteStart {
    SingleLine {
        text: String,
        position: String,
        target: Option<String>,
    },
    MultiLine {
        position: String,
        target: Option<String>,
    },
}

/// Try to parse a note line.
///
/// Supported forms:
/// - `note "text" as N`
/// - `note right of X : text`
/// - `note right of X` (multi-line, until `end note`)
/// - `note as N` (multi-line floating note)
fn try_parse_note_start(line: &str) -> Option<NoteStart> {
    if !line.starts_with("note ") && line != "note" {
        return None;
    }
    let rest = line["note".len()..].trim();

    // `note "text" as N`
    if rest.starts_with('"') {
        if let Some(end_q) = rest.get(1..).and_then(|s| s.find('"')) {
            let text = rest[1..end_q + 1].to_string();
            let after = rest[end_q + 2..].trim();
            let alias = after.strip_prefix("as ").map(|a| a.trim().to_string());
            return Some(NoteStart::SingleLine {
                text,
                position: "floating".to_string(),
                target: alias,
            });
        }
    }

    // `note <position> of <target> : text` or `note <position> of <target>`
    // positions: right, left, top, bottom, over
    let positions = ["right of ", "left of ", "top of ", "bottom of ", "over "];
    for pos_prefix in &positions {
        if let Some(after_pos) = rest.strip_prefix(pos_prefix) {
            let pos_name = pos_prefix.trim_end_matches(" of ").trim_end_matches(' ');
            // Split on ` : ` for single-line text
            if let Some(colon) = after_pos.find(" : ") {
                let target = after_pos[..colon].trim().to_string();
                let text = after_pos[colon + 3..].trim().to_string();
                return Some(NoteStart::SingleLine {
                    text,
                    position: pos_name.to_string(),
                    target: if target.is_empty() {
                        None
                    } else {
                        Some(target)
                    },
                });
            } else {
                let target = after_pos.trim().to_string();
                return Some(NoteStart::MultiLine {
                    position: pos_name.to_string(),
                    target: if target.is_empty() {
                        None
                    } else {
                        Some(target)
                    },
                });
            }
        }
    }

    // `note as N` (floating multi-line)
    if let Some(alias) = rest.strip_prefix("as ") {
        return Some(NoteStart::MultiLine {
            position: "floating".to_string(),
            target: Some(alias.trim().to_string()),
        });
    }

    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> UseCaseDiagram {
        parse_usecase_diagram(src).expect("parse failed")
    }

    // ── actor declarations ──────────────────────────────────────────────────

    #[test]
    fn actor_keyword_quoted() {
        let d = parse("@startuml\nactor \"System Admin\" as admin\n@enduml");
        assert_eq!(d.actors.len(), 1);
        assert_eq!(d.actors[0].id, "admin");
        assert_eq!(d.actors[0].name, "System Admin");
    }

    #[test]
    fn actor_keyword_bare() {
        let d = parse("@startuml\nactor User\n@enduml");
        assert_eq!(d.actors.len(), 1);
        assert_eq!(d.actors[0].id, "User");
        assert_eq!(d.actors[0].name, "User");
    }

    #[test]
    fn actor_colon_syntax() {
        let d = parse("@startuml\n:Customer Service:\n@enduml");
        assert_eq!(d.actors.len(), 1);
        assert_eq!(d.actors[0].name, "Customer Service");
        assert_eq!(d.actors[0].id, "customer_service");
    }

    #[test]
    fn actor_colon_not_confused_with_activity_action() {
        // Activity actions end with `;`, colon actors end with `:`
        let d = parse("@startuml\n:action;\n:RealActor:\n@enduml");
        // Only the colon actor should be parsed; the activity action is not a valid colon actor
        assert_eq!(d.actors.len(), 1);
        assert_eq!(d.actors[0].name, "RealActor");
    }

    #[test]
    fn actor_with_stereotype() {
        let d = parse("@startuml\nactor Admin <<admin>>\n@enduml");
        assert_eq!(d.actors[0].stereotype.as_deref(), Some("admin"));
    }

    #[test]
    fn actor_with_color() {
        let d = parse("@startuml\nactor Admin #lightblue\n@enduml");
        assert_eq!(d.actors[0].color.as_deref(), Some("lightblue"));
    }

    // ── usecase declarations ────────────────────────────────────────────────

    #[test]
    fn usecase_keyword_quoted_with_alias() {
        let d = parse("@startuml\nusecase \"Login\" as UC1\n@enduml");
        assert_eq!(d.usecases.len(), 1);
        assert_eq!(d.usecases[0].id, "UC1");
        assert_eq!(d.usecases[0].name, "Login");
    }

    #[test]
    fn usecase_paren_syntax() {
        let d = parse("@startuml\n(View Report)\n@enduml");
        assert_eq!(d.usecases.len(), 1);
        assert_eq!(d.usecases[0].name, "View Report");
        assert_eq!(d.usecases[0].id, "view_report");
    }

    #[test]
    fn usecase_paren_with_alias() {
        let d = parse("@startuml\n(View Report) as VR\n@enduml");
        assert_eq!(d.usecases[0].id, "VR");
        assert_eq!(d.usecases[0].name, "View Report");
    }

    #[test]
    fn usecase_with_stereotype() {
        let d = parse("@startuml\nusecase \"Checkout\" as UC <<important>>\n@enduml");
        assert_eq!(d.usecases[0].stereotype.as_deref(), Some("important"));
    }

    // ── relationships ───────────────────────────────────────────────────────

    #[test]
    fn association_arrow() {
        let d = parse("@startuml\nactor User\n(Login)\nUser --> Login\n@enduml");
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].style, UseCaseLinkStyle::Association);
        assert_eq!(d.links[0].from, "User");
        assert_eq!(d.links[0].to, "Login");
    }

    #[test]
    fn dashed_arrow() {
        let d = parse("@startuml\nactor A\n(UC)\nA ..> UC\n@enduml");
        assert_eq!(d.links[0].style, UseCaseLinkStyle::Dashed);
    }

    #[test]
    fn inheritance_arrow() {
        let d = parse("@startuml\nactor Admin\nactor User\nAdmin --|> User\n@enduml");
        assert_eq!(d.links[0].style, UseCaseLinkStyle::Inheritance);
    }

    #[test]
    fn link_with_label() {
        let d = parse("@startuml\nactor User\n(Login)\nUser --> Login : uses\n@enduml");
        assert_eq!(d.links[0].label, "uses");
    }

    #[test]
    fn direction_hint_in_arrow() {
        let d = parse("@startuml\nactor A\n(UC)\nA -up-> UC\n@enduml");
        assert_eq!(d.links[0].direction_hint.as_deref(), Some("up"));
    }

    #[test]
    fn reversed_arrow_swaps_from_to() {
        let d = parse("@startuml\nactor User\n(Login)\nLogin <-- User\n@enduml");
        // <-- means right points to left, so from=User to=Login
        assert_eq!(d.links[0].from, "User");
        assert_eq!(d.links[0].to, "Login");
    }

    // ── boundaries ──────────────────────────────────────────────────────────

    #[test]
    fn package_boundary() {
        let d = parse("@startuml\npackage \"Banking System\" {\n(Login)\n}\n@enduml");
        assert_eq!(d.boundaries.len(), 1);
        assert_eq!(d.boundaries[0].name, "Banking System");
        assert_eq!(d.usecases[0].parent.as_deref(), Some("banking_system"));
    }

    #[test]
    fn rectangle_boundary() {
        let d = parse("@startuml\nrectangle \"System\" {\nactor User\n}\n@enduml");
        assert_eq!(d.boundaries.len(), 1);
        assert!(d.boundaries[0].children.contains(&"User".to_string()));
    }

    // ── direction ───────────────────────────────────────────────────────────

    #[test]
    fn direction_left_to_right() {
        let d = parse("@startuml\nleft to right direction\nactor A\n@enduml");
        assert_eq!(d.direction, Direction::LeftToRight);
    }

    #[test]
    fn direction_top_to_bottom() {
        let d = parse("@startuml\ntop to bottom direction\nactor A\n@enduml");
        assert_eq!(d.direction, Direction::TopToBottom);
    }

    // ── notes ───────────────────────────────────────────────────────────────

    #[test]
    fn note_floating_quoted() {
        let d = parse("@startuml\nnote \"This is a note\" as N\n@enduml");
        assert_eq!(d.notes.len(), 1);
        assert_eq!(d.notes[0].text, "This is a note");
        assert_eq!(d.notes[0].target.as_deref(), Some("N"));
    }

    #[test]
    fn note_right_of_single_line() {
        let d = parse("@startuml\nactor User\nnote right of User : important\n@enduml");
        assert_eq!(d.notes.len(), 1);
        assert_eq!(d.notes[0].position, "right");
        assert_eq!(d.notes[0].target.as_deref(), Some("User"));
        assert_eq!(d.notes[0].text, "important");
    }

    #[test]
    fn note_multiline_block() {
        let src = "@startuml\nnote right of User\nLine 1\nLine 2\nend note\n@enduml";
        let d = parse(src);
        assert_eq!(d.notes[0].text, "Line 1\nLine 2");
    }

    // ── auto-creation ───────────────────────────────────────────────────────

    #[test]
    fn auto_create_from_link() {
        let d = parse("@startuml\nUser --> Login\n@enduml");
        // User is a bare word → auto-created as actor
        assert!(d.actors.iter().any(|a| a.id == "User"));
    }

    // ── skinparam/style skipping ────────────────────────────────────────────

    #[test]
    fn skinparam_block_skipped() {
        let src = "@startuml\nskinparam actor {\nBackgroundColor white\n}\nactor User\n@enduml";
        let d = parse(src);
        assert_eq!(d.actors.len(), 1);
    }

    #[test]
    fn style_block_skipped() {
        let src = "@startuml\n<style>\nactor { color: red; }\n</style>\nactor User\n@enduml";
        let d = parse(src);
        assert_eq!(d.actors.len(), 1);
    }

    // ── issue #249: parenthesised linkage lines ─────────────────────────────

    #[test]
    fn paren_endpoints_start_a_link() {
        let d = parse("@startuml\n(a) --> (b)\n@enduml");
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].from, "a");
        assert_eq!(d.links[0].to, "b");
        // Parenthesised endpoints are use cases (Java draws ellipses), not actors.
        assert_eq!(d.usecases.len(), 2);
        assert!(d.actors.is_empty());
    }

    #[test]
    fn paren_link_inside_same_named_boundary_resolves_to_the_boundary() {
        // Java accepts this: `(a)` is a reference to the enclosing boundary,
        // not a declaration of a second `a`.
        let d = parse("@startuml\nrectangle a {\n  (a) .> (aa) : <<a>>\n}\n@enduml");
        assert_eq!(d.boundaries.len(), 1);
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.links[0].from, "a");
        assert_eq!(d.links[0].to, "aa");
        assert_eq!(d.links[0].label, "<<a>>");
        assert!(d.usecases.iter().all(|u| u.id != "a"));
        assert!(d.actors.iter().all(|a| a.id != "a"));
        // The other endpoint still lives inside the boundary.
        let aa = d.usecases.iter().find(|u| u.id == "aa").expect("aa");
        assert_eq!(aa.parent.as_deref(), Some("a"));
    }

    #[test]
    fn quoted_names_and_stereotypes_stay_declarations() {
        let d = parse("@startuml\n(\"a --> b\") as x\n@enduml");
        assert!(d.links.is_empty());
        assert_eq!(d.usecases.len(), 1);

        let d = parse("@startuml\n(a) <<stereo>>\n@enduml");
        assert!(d.links.is_empty());
        assert_eq!(d.usecases.len(), 1);
        assert_eq!(d.usecases[0].stereotype.as_deref(), Some("stereo"));

        let d = parse("@startuml\n(a) as x\n@enduml");
        assert!(d.links.is_empty());
        assert_eq!(d.usecases.len(), 1);
        assert_eq!(d.usecases[0].id, "x");
    }

    // ── issue #249: same-name declarations are rejected, like Java ──────────

    #[test]
    fn same_name_leaf_declaration_inside_boundary_is_rejected() {
        for src in [
            "@startuml\nrectangle a {\n  (a)\n}\n@enduml",
            "@startuml\nrectangle a {\n  usecase a\n}\n@enduml",
            "@startuml\nactor u\nrectangle a {\n  (a)\n}\n@enduml",
        ] {
            let err = parse_usecase_diagram(src).expect_err("must be rejected");
            assert!(
                matches!(err, crate::Error::Parse { .. }),
                "unexpected error for {src:?}: {err:?}"
            );
        }
    }

    #[test]
    fn nested_same_named_container_is_allowed() {
        // Java allows a nested container with the same name (`rectangle a {
        // rectangle a { ... } }`); only leaf entities clash.
        let d = parse("@startuml\nrectangle a {\n  rectangle a {\n    (x)\n  }\n}\n@enduml");
        assert_eq!(d.boundaries.len(), 2);
        assert_eq!(d.usecases.len(), 1);
    }
}
