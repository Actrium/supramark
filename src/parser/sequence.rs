//! Sequence-diagram parser (scaffold).
//!
//! Upstream grammar:
//!   `packages/mermaid/src/diagrams/sequence/parser/sequenceDiagram.jison`
//!
//! This is a tolerant line-oriented parser that:
//!  - never panics on any of the 150 cypress + demos fixtures,
//!  - extracts enough structure (actors, boxes, simple messages,
//!    notes, loop / alt / opt / par / critical / break / rect, plus
//!    create / destroy / activate / deactivate / autonumber) for the
//!    layout/render scaffold to walk a non-empty model,
//!  - recognises the `@{ "type": "..." }` actor-type annotation,
//!  - tolerates the exotic arrow tokens used by fixtures 125-132 by
//!    skipping any line that does not contain a recognised arrow.
//!
//! Byte-exact rendering is NOT a goal of this scaffold — the
//! corresponding fixtures stay in `tests/known_ignored.txt` until the
//! upstream `sequenceRenderer.ts` + `svgDraw.js` ports land.

use crate::error::{MermaidError, Result};
use crate::model::sequence::{
    Actor, ActorBox, ActorType, AltBranch, ArrowType, DiagramItem, Message, Note, NotePlacement,
    ParBranch, SequenceDiagram,
};

enum Frame {
    Loop {
        label: String,
        items: Vec<DiagramItem>,
    },
    Opt {
        label: String,
        items: Vec<DiagramItem>,
    },
    Break {
        label: String,
        items: Vec<DiagramItem>,
    },
    Rect {
        fill: String,
        items: Vec<DiagramItem>,
    },
    Alt {
        branches: Vec<AltBranch>,
    },
    Par {
        branches: Vec<ParBranch>,
    },
    Critical {
        branches: Vec<AltBranch>,
    },
}

impl Frame {
    fn push(&mut self, item: DiagramItem) {
        match self {
            Frame::Loop { items, .. }
            | Frame::Opt { items, .. }
            | Frame::Break { items, .. }
            | Frame::Rect { items, .. } => items.push(item),
            Frame::Alt { branches } | Frame::Critical { branches } => {
                if let Some(b) = branches.last_mut() {
                    b.items.push(item);
                } else {
                    branches.push(AltBranch {
                        label: String::new(),
                        items: vec![item],
                    });
                }
            }
            Frame::Par { branches } => {
                if let Some(b) = branches.last_mut() {
                    b.items.push(item);
                } else {
                    branches.push(ParBranch {
                        label: String::new(),
                        items: vec![item],
                    });
                }
            }
        }
    }

    fn into_item(self) -> DiagramItem {
        match self {
            Frame::Loop { label, items } => DiagramItem::Loop { label, items },
            Frame::Opt { label, items } => DiagramItem::Opt { label, items },
            Frame::Break { label, items } => DiagramItem::Break { label, items },
            Frame::Rect { fill, items } => DiagramItem::Rect { fill, items },
            Frame::Alt { branches } => DiagramItem::Alt { branches },
            Frame::Par { branches } => DiagramItem::Par { branches },
            Frame::Critical { branches } => DiagramItem::Critical { branches },
        }
    }
}

fn push_item(top: &mut Vec<DiagramItem>, stack: &mut [Frame], item: DiagramItem) {
    if let Some(frame) = stack.last_mut() {
        frame.push(item);
    } else {
        top.push(item);
    }
}

pub fn parse(source: &str) -> Result<SequenceDiagram> {
    let mut d = SequenceDiagram::default();

    // 1. Strip an optional YAML frontmatter (between `---` lines).
    let after_fm = strip_frontmatter(source, &mut d);

    // 2. Strip `%%{init: ... }%%` directives. We don't fully parse the
    //    JSON; only sniff a `theme` key for now.
    let cleaned = strip_init_directives(&after_fm, &mut d);

    // 3. Drop pure-comment lines (`%% ...` but not `%%{`).
    let stream: Vec<&str> = cleaned
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("%%") || t.starts_with("%%{")
        })
        .collect();

    let mut idx = 0;
    let mut saw_header = false;
    let mut current_box: Option<usize> = None;
    let mut stack: Vec<Frame> = Vec::new();

    while idx < stream.len() {
        let raw = stream[idx];
        idx += 1;
        let line = trim_comments(raw).trim();
        if line.is_empty() {
            continue;
        }

        // sequenceDiagram header — accept then continue.
        if !saw_header
            && (eq_keyword(line, "sequenceDiagram") || line.starts_with("sequenceDiagram"))
        {
            saw_header = true;
            continue;
        }

        // Title / accTitle / accDescr.
        if let Some(rest) = strip_kw(line, "title") {
            let v = rest.trim_start_matches(':').trim().to_string();
            d.title = Some(v.clone());
            d.meta.title = Some(v);
            continue;
        }
        if let Some(rest) = strip_kw(line, "accTitle") {
            if let Some(v) = rest.trim_start().strip_prefix(':') {
                d.meta.acc_title = Some(v.trim().to_string());
            }
            continue;
        }
        if let Some(rest) = strip_kw(line, "accDescr") {
            if let Some(v) = rest.trim_start().strip_prefix(':') {
                d.meta.acc_descr = Some(v.trim().to_string());
            }
            continue;
        }

        // autonumber [start [step]] | autonumber off
        if line == "autonumber"
            || line.starts_with("autonumber ")
            || line.starts_with("autonumber\t")
        {
            let rest = if line == "autonumber" {
                ""
            } else {
                line[10..].trim()
            };
            let (start, step, visible) = if rest.is_empty() {
                (None, None, true)
            } else if rest == "off" {
                (None, None, false)
            } else {
                let mut parts = rest.split_whitespace();
                let start = parts.next().and_then(|s| s.parse::<i64>().ok());
                let step = parts.next().and_then(|s| s.parse::<i64>().ok());
                (start, step, true)
            };
            push_item(
                &mut d.items,
                &mut stack,
                DiagramItem::Autonumber { start, step, visible },
            );
            continue;
        }

        // box <fill?> <label?> ... end.
        if let Some(rest) = strip_kw(line, "box") {
            let (fill, label) = split_box_header(rest.trim());
            d.boxes.push(ActorBox {
                fill,
                label,
                actors: Vec::new(),
            });
            current_box = Some(d.boxes.len() - 1);
            continue;
        }

        // `end` closes whatever the innermost frame is. If we're inside
        // a `box`, `end` closes that.
        if eq_keyword(line, "end") {
            if let Some(frame) = stack.pop() {
                let item = frame.into_item();
                push_item(&mut d.items, &mut stack, item);
            } else if current_box.is_some() {
                current_box = None;
            }
            continue;
        }

        // Container openers.
        if let Some(rest) = strip_kw(line, "loop") {
            stack.push(Frame::Loop {
                label: rest.trim().to_string(),
                items: Vec::new(),
            });
            continue;
        }
        if let Some(rest) = strip_kw(line, "opt") {
            stack.push(Frame::Opt {
                label: rest.trim().to_string(),
                items: Vec::new(),
            });
            continue;
        }
        if let Some(rest) = strip_kw(line, "break") {
            stack.push(Frame::Break {
                label: rest.trim().to_string(),
                items: Vec::new(),
            });
            continue;
        }
        if let Some(rest) = strip_kw(line, "rect") {
            stack.push(Frame::Rect {
                fill: rest.trim().to_string(),
                items: Vec::new(),
            });
            continue;
        }
        if let Some(rest) = strip_kw(line, "alt") {
            stack.push(Frame::Alt {
                branches: vec![AltBranch {
                    label: rest.trim().to_string(),
                    items: Vec::new(),
                }],
            });
            continue;
        }
        if let Some(rest) = strip_kw(line, "else") {
            if let Some(Frame::Alt { branches } | Frame::Critical { branches }) = stack.last_mut()
            {
                branches.push(AltBranch {
                    label: rest.trim().to_string(),
                    items: Vec::new(),
                });
            }
            continue;
        }
        if let Some(rest) = strip_kw(line, "par") {
            stack.push(Frame::Par {
                branches: vec![ParBranch {
                    label: rest.trim().to_string(),
                    items: Vec::new(),
                }],
            });
            continue;
        }
        if eq_keyword(line, "and")
            || line.starts_with("and ")
            || line.starts_with("and\t")
        {
            let label = strip_kw(line, "and")
                .map(|r| r.trim().to_string())
                .unwrap_or_default();
            if let Some(Frame::Par { branches }) = stack.last_mut() {
                branches.push(ParBranch {
                    label,
                    items: Vec::new(),
                });
            }
            continue;
        }
        if let Some(rest) = strip_kw(line, "critical") {
            stack.push(Frame::Critical {
                branches: vec![AltBranch {
                    label: rest.trim().to_string(),
                    items: Vec::new(),
                }],
            });
            continue;
        }
        if let Some(rest) = strip_kw(line, "option") {
            if let Some(Frame::Critical { branches }) = stack.last_mut() {
                branches.push(AltBranch {
                    label: rest.trim().to_string(),
                    items: Vec::new(),
                });
            }
            continue;
        }

        // create / destroy.
        if let Some(rest) = strip_kw(line, "create") {
            let rest = rest.trim_start();
            let (actor, _kind) = if let Some(r) = strip_kw(rest, "actor") {
                (parse_actor_decl(r.trim(), ActorType::Actor, current_box), "actor")
            } else if let Some(r) = strip_kw(rest, "participant") {
                (
                    parse_actor_decl(r.trim(), ActorType::Participant, current_box),
                    "participant",
                )
            } else {
                (
                    parse_actor_decl(rest.trim(), ActorType::Participant, current_box),
                    "participant",
                )
            };
            push_item(
                &mut d.items,
                &mut stack,
                DiagramItem::Create(actor.clone()),
            );
            add_actor(&mut d, actor, true);
            continue;
        }
        if let Some(rest) = strip_kw(line, "destroy") {
            let id = rest.split_whitespace().next().unwrap_or("").to_string();
            push_item(&mut d.items, &mut stack, DiagramItem::Destroy(id.clone()));
            mark_destroyed(&mut d, &id);
            continue;
        }
        if let Some(rest) = strip_kw(line, "participant") {
            let actor = parse_actor_decl(rest.trim(), ActorType::Participant, current_box);
            add_actor(&mut d, actor, false);
            continue;
        }
        if let Some(rest) = strip_kw(line, "actor") {
            let actor = parse_actor_decl(rest.trim(), ActorType::Actor, current_box);
            add_actor(&mut d, actor, false);
            continue;
        }

        // activate / deactivate.
        if let Some(rest) = strip_kw(line, "activate") {
            let id = rest.trim().to_string();
            push_item(&mut d.items, &mut stack, DiagramItem::Activate(id));
            continue;
        }
        if let Some(rest) = strip_kw(line, "deactivate") {
            let id = rest.trim().to_string();
            push_item(&mut d.items, &mut stack, DiagramItem::Deactivate(id));
            continue;
        }

        // links / link / properties / details — record-and-skip.
        if line.starts_with("links ")
            || line.starts_with("link ")
            || line.starts_with("properties ")
            || line.starts_with("details ")
        {
            continue;
        }

        // Note left/right/over <actor> [, <actor>] : <text>
        if let Some(rest) = strip_kw_ci(line, "note") {
            if let Some(note) = parse_note(rest.trim()) {
                push_item(&mut d.items, &mut stack, DiagramItem::Note(note));
            }
            continue;
        }

        // Otherwise: try to read it as a Message.
        if let Some(msg) = parse_message_line(line) {
            // Auto-register actors that have not been declared yet.
            if !d.actors.iter().any(|a| a.id == msg.from) {
                add_actor(
                    &mut d,
                    Actor {
                        id: msg.from.clone(),
                        description: msg.from.clone(),
                        actor_type: ActorType::Participant,
                        box_index: current_box,
                        ..Default::default()
                    },
                    false,
                );
            }
            if !d.actors.iter().any(|a| a.id == msg.to) {
                add_actor(
                    &mut d,
                    Actor {
                        id: msg.to.clone(),
                        description: msg.to.clone(),
                        actor_type: ActorType::Participant,
                        box_index: current_box,
                        ..Default::default()
                    },
                    false,
                );
            }
            push_item(&mut d.items, &mut stack, DiagramItem::Message(msg));
            continue;
        }

        // Anything else — silently drop. Sequence has many minor
        // keywords that shouldn't fail the parse.
    }

    // Pop any unclosed frames so they still appear in the model.
    while let Some(frame) = stack.pop() {
        let item = frame.into_item();
        d.items.push(item);
    }

    if !saw_header {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: "not a sequenceDiagram".into(),
        });
    }

    Ok(d)
}

// ---------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------

fn add_actor(d: &mut SequenceDiagram, mut actor: Actor, created: bool) {
    actor.created = created || actor.created;
    if let Some(existing) = d.actors.iter_mut().find(|a| a.id == actor.id) {
        if actor.description != actor.id {
            existing.description = actor.description;
        }
        if actor.actor_type != ActorType::Participant {
            existing.actor_type = actor.actor_type;
        }
        if let Some(b) = actor.box_index {
            existing.box_index = Some(b);
        }
    } else {
        if let Some(b) = actor.box_index {
            if let Some(bx) = d.boxes.get_mut(b) {
                bx.actors.push(actor.id.clone());
            }
        }
        d.actors.push(actor);
    }
}

fn mark_destroyed(d: &mut SequenceDiagram, id: &str) {
    if let Some(a) = d.actors.iter_mut().find(|a| a.id == id) {
        a.destroyed = true;
    }
}

fn trim_comments(line: &str) -> &str {
    if let Some(p) = line.find("%%") {
        if !line[p..].starts_with("%%{") {
            return &line[..p];
        }
    }
    line
}

fn strip_kw_ci<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    if s.len() < kw.len() {
        return None;
    }
    let head = &s[..kw.len()];
    if !head.eq_ignore_ascii_case(kw) {
        return None;
    }
    let rest = &s[kw.len()..];
    match rest.chars().next() {
        None => Some(rest),
        Some(c) if c.is_whitespace() || c == ':' => Some(rest),
        _ => None,
    }
}

fn strip_kw<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    match rest.chars().next() {
        None => Some(rest),
        Some(c) if c.is_whitespace() => Some(rest),
        _ => None,
    }
}

fn eq_keyword(s: &str, kw: &str) -> bool {
    s == kw
}

fn split_box_header(s: &str) -> (Option<String>, String) {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("rgb(") {
        if let Some(close) = rest.find(')') {
            let fill = format!("rgb({}", &rest[..=close]);
            let label = rest[close + 1..].trim().to_string();
            return (Some(fill), label);
        }
    }
    if s.starts_with('#') {
        let mut parts = s.splitn(2, char::is_whitespace);
        let fill = parts.next().unwrap_or("").to_string();
        let label = parts.next().unwrap_or("").trim().to_string();
        return (Some(fill), label);
    }
    (None, s.to_string())
}

fn parse_actor_decl(s: &str, default_type: ActorType, box_index: Option<usize>) -> Actor {
    let (head, type_anno) = match s.find("@{") {
        Some(p) => {
            let after = &s[p + 2..];
            let close = after.find('}').map(|q| p + 2 + q + 1).unwrap_or(s.len());
            (s[..p].trim(), Some(&s[p..close]))
        }
        None => (s, None),
    };
    let (id, mut descr) = match find_as(head) {
        Some((a, b)) => (a.trim().to_string(), b.trim().to_string()),
        None => {
            let id = head.trim().to_string();
            (id.clone(), id)
        }
    };
    let actor_type = type_anno.and_then(parse_type_anno).unwrap_or(default_type);
    // Optional `@{ ..., "alias": "Display Name" }` overrides the
    // description (when no `as` clause was given). Mirrors upstream
    // sequenceDb's `addActor(actor, name, description, type)` flow:
    // when `alias` is provided, mermaid uses it as the rendered label.
    if descr == id {
        if let Some(anno) = type_anno {
            if let Some(alias) = parse_alias_anno(anno) {
                descr = alias;
            }
        }
    }
    // Strip `wrap:` / `nowrap:` prefix from the description, mirroring
    // upstream `parseMessage` -> `extractWrap`. The description label
    // shown in the actor box never includes the literal prefix.
    let (wrap, descr_clean) = strip_wrap_prefix(&descr);
    let descr = descr_clean.trim().to_string();
    Actor {
        id,
        description: descr,
        actor_type,
        box_index,
        created: false,
        destroyed: false,
        wrap,
    }
}

/// Extract the `alias` value from an `@{ ... }` annotation. The
/// upstream sequenceDb mini-grammar lets users write `participant X@{
/// "alias": "Display Name" }` to set the label without an `as`
/// clause. We parse this with a tolerant string-search rather than
/// the full JSON grammar — it covers every cypress fixture.
fn parse_alias_anno(s: &str) -> Option<String> {
    let s_lower = s.to_ascii_lowercase();
    let key_idx = s_lower.find("\"alias\"")?;
    let after = &s[key_idx + 7..];
    let after = after.trim_start_matches(|c: char| c.is_whitespace() || c == ':');
    let after = after.strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

fn find_as(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if (bytes[i] == b' ' || bytes[i] == b'\t')
            && bytes[i + 1] == b'a'
            && bytes[i + 2] == b's'
            && (bytes[i + 3] == b' ' || bytes[i + 3] == b'\t')
        {
            return Some((&s[..i], &s[i + 4..]));
        }
        i += 1;
    }
    None
}

fn parse_type_anno(s: &str) -> Option<ActorType> {
    let s_lower = s.to_ascii_lowercase();
    if !s_lower.contains("type") {
        return None;
    }
    let after = s_lower.split_once("type")?.1;
    let after = after.trim_start_matches(|c: char| c.is_whitespace() || c == '"' || c == ':');
    let after = after.trim_start_matches('"');
    let value = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>();
    match value.as_str() {
        "actor" => Some(ActorType::Actor),
        "participant" => Some(ActorType::Participant),
        "boundary" => Some(ActorType::Boundary),
        "control" => Some(ActorType::Control),
        "entity" => Some(ActorType::Entity),
        "database" => Some(ActorType::Database),
        "collections" => Some(ActorType::Collections),
        "queue" => Some(ActorType::Queue),
        _ => None,
    }
}

fn parse_note(s: &str) -> Option<Note> {
    let s = s.trim_start_matches(':').trim_start();
    let (placement, rest) = if let Some(r) = s.strip_prefix("left of ") {
        (NotePlacement::LeftOf, r)
    } else if let Some(r) = s.strip_prefix("right of ") {
        (NotePlacement::RightOf, r)
    } else if let Some(r) = s.strip_prefix("over ") {
        (NotePlacement::Over, r)
    } else {
        return None;
    };
    let (anchors_str, body) = rest.split_once(':')?;
    let anchors: Vec<String> = anchors_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let (wrap, text) = strip_wrap_prefix(body.trim_start());
    Some(Note {
        placement_actors: anchors,
        placement: Some(placement),
        text: text.trim().to_string(),
        wrap,
    })
}

fn strip_wrap_prefix(s: &str) -> (bool, &str) {
    if let Some(r) = s.strip_prefix("wrap:") {
        (true, r)
    } else if let Some(r) = s.strip_prefix("nowrap:") {
        (false, r)
    } else {
        (false, s)
    }
}


fn parse_message_line(s: &str) -> Option<Message> {
    static ARROWS: &[(&str, ArrowType)] = &[
        ("<<-->>", ArrowType::BiDotted),
        ("<<->>", ArrowType::BiSolid),
        ("-->>", ArrowType::DottedArrow),
        ("->>", ArrowType::SolidArrow),
        ("--x", ArrowType::DottedCross),
        ("--)", ArrowType::DottedPoint),
        ("-->", ArrowType::DottedLine),
        ("-x", ArrowType::SolidCross),
        ("-)", ArrowType::SolidPoint),
        ("->", ArrowType::SolidLine),
    ];

    let mut found: Option<(usize, usize, ArrowType)> = None;
    for (token, kind) in ARROWS {
        if let Some(pos) = s.find(token) {
            let end = pos + token.len();
            match found {
                Some((p, _, _)) if p <= pos => {}
                _ => found = Some((pos, end, *kind)),
            }
        }
    }
    let (arr_start, arr_end, kind) = found?;
    let from = s[..arr_start]
        .trim()
        .trim_end_matches('+')
        .trim()
        .to_string();
    let after = &s[arr_end..];
    let (activate, deactivate, after) = strip_activation(after);
    let colon = after.find(':')?;
    let to = after[..colon].trim().to_string();
    let text_raw = after[colon + 1..].trim_start();
    let (wrap, text) = strip_wrap_prefix(text_raw);
    if from.is_empty() || to.is_empty() {
        return None;
    }
    Some(Message {
        from,
        to,
        text: text.trim().to_string(),
        arrow: Some(kind),
        activate,
        deactivate,
        wrap,
    })
}

fn strip_activation(s: &str) -> (bool, bool, &str) {
    let s = s.trim_start();
    if let Some(r) = s.strip_prefix('+') {
        return (true, false, r);
    }
    if let Some(r) = s.strip_prefix('-') {
        return (false, true, r);
    }
    (false, false, s)
}

fn strip_frontmatter(src: &str, d: &mut SequenceDiagram) -> String {
    let lead = src.trim_start_matches(['\n', '\r', ' ', '\t']);
    if !lead.starts_with("---") {
        return src.to_string();
    }
    let body = lead[3..].trim_start_matches(['\n', '\r']);
    let close = match body.find("\n---") {
        Some(p) => p,
        None => return src.to_string(),
    };
    let yaml = &body[..close];
    for line in yaml.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("title:") {
            let v = v.trim().to_string();
            d.title = Some(v.clone());
            d.meta.title = Some(v);
        } else if let Some(v) = t.strip_prefix("theme:") {
            d.theme_name = Some(v.trim().to_string());
        }
    }
    let after = &body[close + 4..];
    let after = after.trim_start_matches(['\n', '\r']);
    after.to_string()
}

fn strip_init_directives(src: &str, d: &mut SequenceDiagram) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(p) = rest.find("%%{") {
        out.push_str(&rest[..p]);
        let after = &rest[p..];
        if let Some(end) = after.find("}%%") {
            let block = &after[..end + 3];
            if let Some(theme) = sniff_theme(block) {
                d.theme_name.get_or_insert(theme);
            }
            // Sniff `mirrorActors: false`. Default is true; only the
            // explicit `false` (with optional whitespace) flips it.
            if let Some(v) = sniff_bool(block, "mirrorActors") {
                d.config.mirror_actors = v;
            }
            // Sniff `wrap: true|false`. Sets the diagram-level wrap
            // flag (applied to messages/notes that don't carry their
            // own `wrap:` / `nowrap:` prefix).
            if let Some(v) = sniff_bool(block, "wrap") {
                d.config.wrap = v;
            }
            rest = &after[end + 3..];
        } else {
            out.push_str(after);
            return out;
        }
    }
    out.push_str(rest);
    out
}

fn sniff_theme(block: &str) -> Option<String> {
    let p = block.find("\"theme\"")?;
    let after = &block[p + 7..];
    let after = after.trim_start_matches(|c: char| c.is_whitespace() || c == ':');
    let after = after.strip_prefix('"')?;
    let close = after.find('"')?;
    Some(after[..close].to_string())
}

/// Sniff a boolean directive value of the form `"key": true|false` or
/// `key: true|false` from an `%%{init: { ... }}%%` block. Returns
/// `Some(value)` when the key is present, `None` otherwise.
fn sniff_bool(block: &str, key: &str) -> Option<bool> {
    let needle = format!("\"{}\"", key);
    let p = block.find(&needle).or_else(|| {
        // Unquoted key fallback (some directives use bare-key syntax).
        let bare = format!("{}:", key);
        block.find(&bare).map(|i| i)
    })?;
    let key_len = if block[p..].starts_with('"') {
        needle.len()
    } else {
        key.len()
    };
    let after = &block[p + key_len..];
    let after = after.trim_start_matches(|c: char| c.is_whitespace() || c == ':');
    if after.starts_with("true") {
        Some(true)
    } else if after.starts_with("false") {
        Some(false)
    } else {
        None
    }
}
