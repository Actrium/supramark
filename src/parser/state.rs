//! State-diagram parser — line-oriented scan matching the upstream
//! Langium grammar at
//! `packages/mermaid/src/diagrams/state/parser/stateDiagram.langium`.
//!
//! Scope: everything cypress/demos fixtures exercise — start/end
//! markers `[*]`, `state X { ... }` composite blocks with arbitrary
//! nesting, `state "Long name" as S`, `state X <<fork|join|choice>>`,
//! history markers `[H]` / `[H*]`, transitions with multi-line labels
//! (`<br>` / `<br/>` / `\n`), `note left|right|above|below of X` ... `end note`,
//! `direction TB|BT|LR|RL`, `classDef`, class applications via
//! `state X:::className` or the `class X name` shortcut, frontmatter
//! and `%%{init: {...}}%%` directive.
//!
//! What is NOT supported yet (rare in fixtures):
//! * `---` divider interaction with composite layout (kept in AST, not styled).
//! * full `style` line parsing (`style X fill:red,stroke:blue`) — kept as opaque.
//! * `link` / `hyperLink` clauses.
//!
//! The parser is forgiving: unknown lines are skipped with a debug log
//! rather than erroring, matching upstream behaviour on malformed input.

use crate::config::{directive, frontmatter};
use crate::error::{MermaidError, Result};
use crate::model::state::{
    ClassApply, ClassDef, Note, NotePosition, ParseItem, State, StateDiagram, StateKind,
    Transition,
};

/// Public entry.
pub fn parse(source: &str) -> Result<StateDiagram> {
    let mut diagram = StateDiagram::default();

    // 1. Strip frontmatter -> extract title / themeOverride.
    let (fm, rest) = frontmatter::parse_frontmatter(source);
    if let Some(fm) = fm {
        if let Some(title) = fm.title {
            diagram.meta.title = Some(title);
        }
        if let Some(config) = fm.config {
            if let Some(theme) = config.theme {
                diagram.theme_override = Some(theme);
            }
        }
    }

    // 2. Extract `%%{init: ...}%%` directives (themeVariables etc).
    let directives = directive::parse_directives(rest);
    for dr in directives {
        if let Some(theme) = dr.theme {
            diagram.theme_override = Some(theme);
        }
    }
    let body = directive::remove_directives(rest);

    // 3. Line-oriented scan.
    let body_owned: String = strip_percent_comments(&body);
    let lines: Vec<&str> = body_owned.lines().collect();

    // Track the composite-state stack for brace `{ ... }` nesting.
    let mut parent_stack: Vec<String> = Vec::new();
    let mut header_seen = false;
    let mut next_start_end_idx = 0usize;

    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let line = raw.trim();
        i += 1;

        if line.is_empty() {
            continue;
        }

        // Header — `stateDiagram` or `stateDiagram-v2`, optionally with direction.
        if !header_seen {
            if let Some(rest) = line
                .strip_prefix("stateDiagram-v2")
                .or_else(|| line.strip_prefix("stateDiagram"))
            {
                diagram.is_v2 = line.starts_with("stateDiagram-v2");
                header_seen = true;
                let rest = rest.trim();
                if !rest.is_empty() {
                    // Accept inline direction `stateDiagram LR`.
                    if let Some(d) = parse_direction_token(rest) {
                        diagram.direction = Some(d);
                    }
                }
                continue;
            }
            // Tolerate a missing header — many demos omit it, detect already voted.
            header_seen = true;
        }

        // --- Meta lines ---------------------------------------------------
        if let Some(rest) = strip_kw(line, "title") {
            diagram.meta.title = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = strip_kw(line, "accTitle") {
            diagram.meta.acc_title = Some(rest.trim_start_matches(':').trim().to_string());
            continue;
        }
        if let Some(rest) = strip_kw(line, "accDescr") {
            diagram.meta.acc_descr = Some(rest.trim_start_matches(':').trim().to_string());
            continue;
        }
        if let Some(rest) = strip_kw(line, "direction") {
            let t = rest.trim();
            if let Some(d) = parse_direction_token(t) {
                if let Some(parent) = parent_stack.last() {
                    // Inside a composite — attach direction to the parent state.
                    if let Some(s) = diagram.states.iter_mut().find(|s| &s.id == parent) {
                        s.direction = Some(d);
                    }
                } else {
                    diagram.direction = Some(d);
                }
            }
            continue;
        }

        // --- Closing brace — pop composite --------------------------------
        if line == "}" {
            parent_stack.pop();
            continue;
        }

        // --- Note block ---------------------------------------------------
        if let Some(note_header) = parse_note_header(line) {
            // Collect body until `end note`.
            let mut buf = String::new();
            while i < lines.len() {
                let l = lines[i].trim();
                i += 1;
                if l == "end note" {
                    break;
                }
                if !buf.is_empty() {
                    buf.push('\n');
                }
                buf.push_str(l);
            }
            let note_idx = diagram.notes.len();
            diagram.notes.push(Note {
                target: note_header.0,
                position: note_header.1,
                text: buf,
            });
            diagram.items.push(ParseItem::NoteDecl(note_idx));
            continue;
        }

        // Single-line note: `note left of X : text` / `note "Hi" as NSomething`
        if line.starts_with("note ") {
            if let Some((target, pos, text)) = parse_inline_note(line) {
                let note_idx = diagram.notes.len();
                diagram.notes.push(Note {
                    target,
                    position: pos,
                    text,
                });
                diagram.items.push(ParseItem::NoteDecl(note_idx));
                continue;
            }
        }

        // --- classDef / class -------------------------------------------
        if let Some(rest) = strip_kw(line, "classDef") {
            if let Some((name, styles)) = split_once_ws(rest.trim()) {
                diagram.class_defs.push(ClassDef {
                    name: name.to_string(),
                    styles: styles.to_string(),
                });
            }
            continue;
        }
        if let Some(rest) = strip_kw(line, "class") {
            let rest = rest.trim();
            // Syntax: `class <id1>, <id2>, ... <className>`
            // The class name is the last whitespace-separated token; everything
            // before it (comma-separated) are the state IDs.
            if let Some((ids_part, cls)) = split_last_ws(rest) {
                for id in ids_part.split(',') {
                    let id = id.trim();
                    if !id.is_empty() {
                        diagram.class_applies.push(ClassApply {
                            state_id: id.to_string(),
                            class_name: cls.to_string(),
                        });
                    }
                }
            }
            continue;
        }

        // --- style — inline node style (`style X fill:...,stroke:...`)
        if let Some(rest) = strip_kw(line, "style") {
            // rest may be "X fill:red,stroke:blue" or "X fill:red,..."
            if let Some((id, css)) = split_once_ws(rest.trim()) {
                let id = id.trim();
                if !id.is_empty() {
                    ensure_state(&mut diagram, id, parent_stack.last().cloned());
                    if let Some(s) = diagram.states.iter_mut().find(|s| s.id == id) {
                        s.style = Some(css.trim().trim_end_matches(';').to_string());
                    }
                }
            }
            continue;
        }

        // --- State declaration (explicit `state ...`) --------------------
        if let Some(rest) = strip_kw(line, "state") {
            // Might open a composite — `state Foo {` / `state "Name" as Foo {` /
            // `state Foo <<fork>>` / `state Foo` (plain).
            let rest = rest.trim();
            if let Some(stripped) = rest.strip_suffix('{') {
                let decl = stripped.trim();
                let id = ingest_state_decl(&mut diagram, decl, parent_stack.last().cloned());
                diagram.items.push(ParseItem::StateDecl(id.clone()));
                // Promote to composite.
                if let Some(s) = diagram.states.iter_mut().find(|s| s.id == id) {
                    if s.kind == StateKind::Simple {
                        s.kind = StateKind::Composite;
                    }
                }
                parent_stack.push(id);
                continue;
            }
            let id = ingest_state_decl(&mut diagram, rest, parent_stack.last().cloned());
            diagram.items.push(ParseItem::StateDecl(id));
            continue;
        }

        // --- Divider `---` / `===` inside composite -----------------------
        if line.starts_with("---") || line.starts_with("===") {
            if let Some(parent) = parent_stack.last().cloned() {
                let id = format!("divider-{}-{}", parent, diagram.states.len());
                diagram.states.push(State {
                    id,
                    kind: StateKind::Divider,
                    parent: Some(parent),
                    implicit: true,
                    ..State::default()
                });
            }
            continue;
        }

        // --- Transition ---------------------------------------------------
        if let Some(tr) = parse_transition(line, &mut diagram, &mut next_start_end_idx, &parent_stack) {
            let idx = diagram.transitions.len();
            diagram.transitions.push(tr);
            diagram.items.push(ParseItem::Relation(idx));
            continue;
        }

        // --- `X : description` label attachment ---------------------------
        // Upstream: the description text IS the display label (SHAPE_STATE,
        // single-line). Only becomes SHAPE_STATE_WITH_DESC when a second
        // description is appended (alias + colon-description).
        if let Some((lhs, rhs)) = split_once_colon(line) {
            let id = lhs.trim().to_string();
            if !id.is_empty() {
                let parent = parent_stack.last().cloned();
                ensure_state(&mut diagram, &id, parent);
                let desc_text = rhs.trim().to_string();
                if let Some(s) = diagram.states.iter_mut().find(|s| s.id == id) {
                    if s.label.is_none() {
                        // No prior alias: description becomes the sole display label.
                        s.label = Some(desc_text);
                    } else {
                        // Already has an alias (state "X" as Y then Y: desc):
                        // description is appended as extra lines.
                        let desc_lines = split_label_lines(rhs.trim());
                        s.description = Some(desc_lines);
                    }
                }
                continue;
            }
        }

        // Fallback — bare identifier is a state declaration.
        if is_identifier(line) {
            ensure_state(&mut diagram, line, parent_stack.last().cloned());
            diagram.items.push(ParseItem::StateDecl(line.to_string()));
            continue;
        }

        // Unknown — tolerate.
        log::debug!("state parser: unrecognised line '{}'", line);
    }

    // Sanity: populate composite children lists.
    let mut children_by_parent: Vec<(String, String)> = Vec::new();
    for s in &diagram.states {
        if let Some(p) = &s.parent {
            children_by_parent.push((p.clone(), s.id.clone()));
        }
    }
    for (p, c) in children_by_parent {
        if let Some(ps) = diagram.states.iter_mut().find(|x| x.id == p) {
            if !ps.children.contains(&c) {
                ps.children.push(c);
            }
            if ps.kind == StateKind::Simple {
                ps.kind = StateKind::Composite;
            }
        }
    }

    Ok(diagram)
}

/// Strip `%%`-prefixed comment lines (but leave `%%{...}%%` directives
/// alone — they were handled in directive::extract_directives).
fn strip_percent_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        let trim = line.trim_start();
        if trim.starts_with("%%") && !trim.starts_with("%%{") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn strip_kw<'a>(line: &'a str, kw: &str) -> Option<&'a str> {
    // Keyword must be followed by whitespace OR colon OR be the whole line.
    if let Some(rest) = line.strip_prefix(kw) {
        if rest.is_empty() {
            return Some(rest);
        }
        let c = rest.chars().next().unwrap();
        if c.is_whitespace() || c == ':' {
            return Some(rest);
        }
    }
    None
}

fn parse_direction_token(t: &str) -> Option<String> {
    let up = t.to_ascii_uppercase();
    match up.as_str() {
        "TB" | "BT" | "LR" | "RL" | "TD" => Some(if up == "TD" { "TB".into() } else { up }),
        _ => None,
    }
}

/// Split `"a  b"` on first whitespace run, returning (a, b). Returns
/// None when there's no second token.
fn split_once_ws(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i == bytes.len() {
        return None;
    }
    let head = &s[..i];
    let tail = s[i..].trim_start();
    if tail.is_empty() {
        None
    } else {
        Some((head, tail))
    }
}

/// Split `"a b c"` on the **last** whitespace run, returning ("a b", "c").
/// Returns None when there's only one token.
fn split_last_ws(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    // Find last whitespace character index.
    let last_ws = s.rfind(|c: char| c.is_ascii_whitespace())?;
    let head = s[..last_ws].trim_end();
    let tail = s[last_ws..].trim_start();
    if head.is_empty() || tail.is_empty() {
        None
    } else {
        Some((head, tail))
    }
}

/// Split `lhs : rhs` on the first `:` that isn't inside quotes.
fn split_once_colon(s: &str) -> Option<(&str, &str)> {
    let mut in_q = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_q = !in_q,
            ':' if !in_q => return Some((&s[..i], &s[i + 1..])),
            _ => {}
        }
    }
    None
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| {
            c.is_alphanumeric() || c == '_' || c == '-' || c == '*' || c == '[' || c == ']'
        })
}

/// Parse `X --> Y` / `X --> Y : label` lines.
///
/// Returns a built `Transition`; also synthesises `[*]` states as
/// `state-root_start` / `state-root_end` (or composite-local) when they
/// appear as endpoints. Each occurrence gets its own unique id so dagre
/// can rank them independently.
fn parse_transition(
    line: &str,
    diagram: &mut StateDiagram,
    start_end_idx: &mut usize,
    parent_stack: &[String],
) -> Option<Transition> {
    // Find the arrow.
    let arrow = "-->";
    let idx = line.find(arrow)?;
    let (lhs, after) = line.split_at(idx);
    let rhs_full = &after[arrow.len()..];

    let lhs = lhs.trim();
    // Strip `:::className` decoration from LHS before anything else.
    let (lhs_id, lhs_class) = split_class_suffix(lhs);

    // For the RHS, we must strip `:::class` BEFORE splitting on `:` for the
    // label.  The colon inside `:::` would otherwise be mistaken for the start
    // of a transition label (e.g. `Still:::notMoving` → wrong label "::notMoving").
    let rhs_full_trimmed = rhs_full.trim();
    // Split out `:::class` from the RHS token first.
    let (rhs_raw, rhs_class_str) = if let Some(i) = rhs_full_trimmed.find(":::") {
        (&rhs_full_trimmed[..i], Some(&rhs_full_trimmed[i + 3..]))
    } else {
        (rhs_full_trimmed, None)
    };
    // Now split the remaining RHS (without :::class) on `:` for the label.
    let (rhs, label) = if let Some((r, l)) = split_once_colon(rhs_raw) {
        (r.trim(), Some(l.trim().to_string()))
    } else {
        (rhs_raw.trim(), None)
    };
    // Extract `:::class` after the label separator too (edge case: `B : lbl:::class` — ignore).
    // The rhs_class_str may itself contain a trailing ` : label` if the input was
    // `B:::class : label`.  Split the rhs_class_str further.
    let (rhs_class_token, label_from_class_suffix) = if let Some(cs) = rhs_class_str {
        // class token may be followed by ` : label text`
        if let Some((c, l)) = split_once_colon(cs) {
            (c.trim(), Some(l.trim().to_string()))
        } else {
            (cs.trim(), None)
        }
    } else {
        ("", None)
    };
    // Merge labels: label from ` : label` takes priority over label from class suffix.
    let label = label.or(label_from_class_suffix);
    let rhs_class: Option<String> = if rhs_class_token.is_empty() { None } else { Some(rhs_class_token.to_string()) };

    // Strip `:::className` decoration from LHS (done above); derive rhs_id.
    let rhs_id = rhs;

    if lhs_id.is_empty() || rhs_id.is_empty() {
        return None;
    }

    let parent = parent_stack.last().cloned();

    let source = resolve_endpoint(diagram, lhs_id, start_end_idx, &parent, true);
    let target = resolve_endpoint(diagram, rhs_id, start_end_idx, &parent, false);

    if let Some(cn) = lhs_class {
        diagram.class_applies.push(ClassApply {
            state_id: source.clone(),
            class_name: cn,
        });
    }
    if let Some(cn) = rhs_class {
        diagram.class_applies.push(ClassApply {
            state_id: target.clone(),
            class_name: cn,
        });
    }

    Some(Transition {
        source,
        target,
        label: label.map(|l| split_label_lines(&l)),
        style: None,
    })
}

fn split_class_suffix(s: &str) -> (&str, Option<String>) {
    if let Some(i) = s.find(":::") {
        let id = s[..i].trim();
        let cn = s[i + 3..].trim();
        (id, if cn.is_empty() { None } else { Some(cn.to_string()) })
    } else {
        (s, None)
    }
}

/// Break a label on HTML line-break tags only: `<br/>`, `<br>`, `<br />`.
///
/// Literal `\n` (two chars backslash + n) is intentionally preserved as-is —
/// it is valid label text that the renderer displays verbatim, and its two
/// characters contribute to the label's measured width.  Only HTML `<br/>`
/// variants produce a visual line break in the upstream renderer's HTML
/// foreignObject content.
fn split_label_lines(raw: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut buf = String::new();
    let bytes: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        // <br/>, <br />, or <br>
        if bytes[i] == '<' {
            let j = if bytes[i..].starts_with(&['<', 'b', 'r', '/', '>']) {
                Some(5) // <br/>
            } else if bytes[i..].len() >= 6
                && bytes[i..].starts_with(&['<', 'b', 'r', ' ', '/', '>'])
            {
                Some(6) // <br />
            } else if bytes[i..].starts_with(&['<', 'b', 'r', '>']) {
                Some(4) // <br>
            } else {
                None
            };
            if let Some(n) = j {
                parts.push(std::mem::take(&mut buf));
                i += n;
                continue;
            }
        }
        buf.push(bytes[i]);
        i += 1;
    }
    parts.push(buf);
    parts
}

fn resolve_endpoint(
    diagram: &mut StateDiagram,
    tok: &str,
    _start_end_idx: &mut usize,
    parent: &Option<String>,
    is_source: bool,
) -> String {
    if tok == "[*]" {
        let root = parent.clone().unwrap_or_else(|| "root".into());
        let role = if is_source { "start" } else { "end" };
        let id = format!("root_{}", role);
        // Reuse existing start/end node within the same parent scope,
        // matching upstream where [*] always maps to a single node per scope.
        if !diagram.states.iter().any(|s| s.id == id && s.parent == *parent) {
            diagram.states.push(State {
                id: id.clone(),
                kind: StateKind::StartEnd,
                parent: parent.clone(),
                implicit: true,
                ..State::default()
            });
        }
        id
    } else if tok == "[H]" {
        ensure_state(diagram, tok, parent.clone());
        if let Some(s) = diagram.states.iter_mut().find(|s| s.id == tok) {
            s.kind = StateKind::History;
        }
        tok.to_string()
    } else if tok == "[H*]" {
        ensure_state(diagram, tok, parent.clone());
        if let Some(s) = diagram.states.iter_mut().find(|s| s.id == tok) {
            s.kind = StateKind::HistoryDeep;
        }
        tok.to_string()
    } else {
        ensure_state(diagram, tok, parent.clone());
        tok.to_string()
    }
}

fn ensure_state(diagram: &mut StateDiagram, id: &str, parent: Option<String>) {
    if !diagram.states.iter().any(|s| s.id == id) {
        diagram.states.push(State {
            id: id.to_string(),
            // label left as None so callers can set it explicitly via alias
            // or colon-desc syntax; the layout falls back to id when None.
            label: None,
            parent,
            ..State::default()
        });
    }
}

/// Parse `state NAME` / `state "Alias" as NAME` / `state NAME <<fork>>` etc.
/// Returns the resolved state id.
fn ingest_state_decl(diagram: &mut StateDiagram, decl: &str, parent: Option<String>) -> String {
    let decl = decl.trim();

    // `state "Nice name" as S` — optionally followed by `: description`
    if let Some(rest) = decl.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            let alias = &rest[..end];
            let tail = rest[end + 1..].trim();
            if let Some(after_as) = tail.strip_prefix("as ") {
                // after_as may be "S1" or "S1: The description" or "S1 { ... }".
                // Split on whitespace to get the id token (trim trailing '{').
                let raw_token = after_as.split_whitespace().next().unwrap_or("").trim_end_matches('{').trim();
                // Strip trailing ':' from the id (happens when `as S1: desc`
                // is parsed by split_whitespace — the `: desc` is not separated).
                let (id, maybe_desc) = if let Some((tok, desc_rest)) = split_once_colon(raw_token) {
                    // e.g. raw_token = "S1:" — colon is a suffix with no desc text here.
                    // The actual description is in `after_as` after the id+colon.
                    let _ = desc_rest; // empty or irrelevant
                    (tok.trim(), None::<&str>)
                } else {
                    (raw_token, None)
                };
                // Check for description after the id token in `after_as`.
                // Find id in after_as, then look for `: desc` after it.
                let desc_in_tail = {
                    // after_as looks like "S1: The description" or "S1" or "S1 { ..."
                    // Find the id, skip it, then check for ": desc".
                    if let Some(pos) = after_as.find(id) {
                        let rest_after_id = after_as[pos + id.len()..].trim_start_matches('{').trim();
                        if let Some((_, desc_text)) = split_once_colon(rest_after_id) {
                            // Preserve leading space — upstream Jison grammar captures
                            // the text after `:` verbatim, including the leading space.
                            let desc_text = desc_text.trim_end();
                            if !desc_text.trim().is_empty() { Some(desc_text) } else { None }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                if !id.is_empty() {
                    ensure_state(diagram, id, parent.clone());
                    if let Some(s) = diagram.states.iter_mut().find(|s| s.id == id) {
                        s.label = Some(alias.to_string());
                        if let Some(desc) = desc_in_tail {
                            s.description = Some(split_label_lines(desc));
                        }
                    }
                    return id.to_string();
                }
            }
        }
    }

    // `state X <<fork>>` / `state X <<join>>` / `state X <<choice>>`
    if let Some(open) = decl.find("<<") {
        let id = decl[..open].trim();
        let close = decl[open + 2..].find(">>").map(|i| open + 2 + i);
        let stereotype = close.map(|c| decl[open + 2..c].trim()).unwrap_or("");
        ensure_state(diagram, id, parent.clone());
        if let Some(s) = diagram.states.iter_mut().find(|s| s.id == id) {
            s.kind = match stereotype {
                "fork" => StateKind::Fork,
                "join" => StateKind::Join,
                "choice" => StateKind::Choice,
                _ => s.kind,
            };
        }
        return id.to_string();
    }

    // `state X : description` — description is the display label when no
    // explicit alias was set yet; otherwise appended as extra description lines.
    if let Some((lhs, rhs)) = split_once_colon(decl) {
        let id = lhs.trim();
        ensure_state(diagram, id, parent.clone());
        if let Some(s) = diagram.states.iter_mut().find(|s| s.id == id) {
            if s.label.is_none() {
                s.label = Some(rhs.trim().to_string());
            } else {
                s.description = Some(split_label_lines(rhs.trim()));
            }
        }
        return id.to_string();
    }

    // Plain `state X` — possibly with class application `state X:::highlight`.
    let (id, cls) = split_class_suffix(decl);
    let id = id.trim();
    ensure_state(diagram, id, parent.clone());
    if let Some(cn) = cls {
        diagram.class_applies.push(ClassApply {
            state_id: id.to_string(),
            class_name: cn,
        });
    }
    id.to_string()
}

/// Parse a `note ... of X` block header. Returns (target, position) when matched.
fn parse_note_header(line: &str) -> Option<(String, NotePosition)> {
    let rest = line.strip_prefix("note ")?;
    let rest = rest.trim();
    // `note left of X` / `note right of X` / `note above` / `note below`
    let (pos, rest) = if let Some(r) = rest.strip_prefix("left of ") {
        (NotePosition::LeftOf, r)
    } else if let Some(r) = rest.strip_prefix("right of ") {
        (NotePosition::RightOf, r)
    } else if let Some(r) = rest.strip_prefix("above of ") {
        (NotePosition::Above, r)
    } else if let Some(r) = rest.strip_prefix("below of ") {
        (NotePosition::Below, r)
    } else {
        return None;
    };
    // Trailing colon / inline content indicates it's actually the
    // one-liner form; caller handles that.
    if rest.contains(':') {
        return None;
    }
    Some((rest.trim().to_string(), pos))
}

fn parse_inline_note(line: &str) -> Option<(String, NotePosition, String)> {
    let rest = line.strip_prefix("note ")?.trim();
    let (pos, rest) = if let Some(r) = rest.strip_prefix("left of ") {
        (NotePosition::LeftOf, r)
    } else if let Some(r) = rest.strip_prefix("right of ") {
        (NotePosition::RightOf, r)
    } else if let Some(r) = rest.strip_prefix("above of ") {
        (NotePosition::Above, r)
    } else if let Some(r) = rest.strip_prefix("below of ") {
        (NotePosition::Below, r)
    } else {
        return None;
    };
    let (target, text) = split_once_colon(rest)?;
    Some((target.trim().to_string(), pos, text.trim().to_string()))
}

// Shim — provide an empty err-free fallback if preprocess doesn't
// include the crate's full directive extractor. The helper is already
// implemented and used by other diagrams; here we only need its public
// surface.
#[allow(dead_code)]
fn _ensure_error_type_shape(_: MermaidError) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_v2() {
        let src = "stateDiagram-v2\n[*] --> S1\nS1 --> [*]\n";
        let d = parse(src).unwrap();
        assert!(d.is_v2);
        assert_eq!(d.transitions.len(), 2);
        // Two implicit [*] states + S1.
        let s_count = d.states.iter().filter(|s| !s.implicit).count();
        assert_eq!(s_count, 1);
    }

    #[test]
    fn parses_v1_header() {
        let src = "stateDiagram\n[*] --> S\nS --> [*]\n";
        let d = parse(src).unwrap();
        assert!(!d.is_v2);
    }

    #[test]
    fn parses_composite_state_block() {
        let src = "stateDiagram-v2\nstate Parent {\n  A --> B\n}\nParent --> Done\n";
        let d = parse(src).unwrap();
        let parent = d.states.iter().find(|s| s.id == "Parent").unwrap();
        assert_eq!(parent.kind, StateKind::Composite);
        assert!(parent.children.contains(&"A".to_string()));
        assert!(parent.children.contains(&"B".to_string()));
    }

    #[test]
    fn parses_fork_stereotype() {
        let src = "stateDiagram-v2\nstate F <<fork>>\n[*] --> F\nF --> A\n";
        let d = parse(src).unwrap();
        let f = d.states.iter().find(|s| s.id == "F").unwrap();
        assert_eq!(f.kind, StateKind::Fork);
    }

    #[test]
    fn parses_note_block() {
        let src = "stateDiagram\nA : desc\nnote right of A\n  some text\nend note\n";
        let d = parse(src).unwrap();
        assert_eq!(d.notes.len(), 1);
        assert_eq!(d.notes[0].target, "A");
        assert_eq!(d.notes[0].position, NotePosition::RightOf);
    }

    #[test]
    fn splits_multi_line_transition_label() {
        // <br/> produces a split; literal \n (backslash+n) is preserved as text.
        let src = "stateDiagram-v2\nA --> B : line one<br/>line two\\nline three\n";
        let d = parse(src).unwrap();
        let t = &d.transitions[0];
        let lbl = t.label.as_ref().unwrap();
        // Only the <br/> splits: ["line one", "line two\\nline three"]
        assert_eq!(lbl.len(), 2);
        assert_eq!(lbl[0], "line one");
        assert_eq!(lbl[1], "line two\\nline three");
    }

    #[test]
    fn alias_form_state_as() {
        let src = "stateDiagram\n[*] --> S1\nstate \"Some long name\" as S1\n";
        let d = parse(src).unwrap();
        let s = d.states.iter().find(|s| s.id == "S1").unwrap();
        assert_eq!(s.label.as_deref(), Some("Some long name"));
    }

    /// X: desc syntax — description becomes the display label (upstream SHAPE_STATE).
    #[test]
    fn colon_desc_becomes_label() {
        let src = "stateDiagram-v2\n  Yswsii: Your state with spaces in it\n  [*] --> Yswsii\n";
        let d = parse(src).unwrap();
        let s = d.states.iter().find(|s| s.id == "Yswsii").unwrap();
        eprintln!("Yswsii label={:?} desc={:?}", s.label, s.description);
        assert_eq!(s.label.as_deref(), Some("Your state with spaces in it"),
            "colon-desc should become the display label");
        assert!(s.description.is_none(),
            "description should be None when colon-desc became label");
    }
}
