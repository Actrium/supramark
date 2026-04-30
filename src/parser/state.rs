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
    ClassApply, ClassDef, Note, NotePosition, ParseItem, State, StateDiagram, StateKind, Transition,
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
            if let Some(look) = config.look {
                diagram.look_override = Some(look);
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
    // Upstream `stateDb.getDividerId` increments a per-diagram counter and
    // returns `divider-id-N`. Track it here so divider lines reproduce the
    // exact id sequence the upstream parser emits.
    let mut divider_cnt: usize = 0;

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
        // Upstream `style A,B,C fill:red,stroke:blue` syntax: a comma-
        // separated list of state ids precedes the css declarations.
        if let Some(rest) = strip_kw(line, "style") {
            if let Some((ids, css)) = split_once_ws(rest.trim()) {
                let css_trimmed = css.trim().trim_end_matches(';').to_string();
                for id in ids.split(',') {
                    let id = id.trim();
                    if id.is_empty() {
                        continue;
                    }
                    ensure_state(&mut diagram, id, parent_stack.last().cloned());
                    if let Some(s) = diagram.states.iter_mut().find(|s| s.id == id) {
                        s.style = Some(css_trimmed.clone());
                    }
                }
            }
            continue;
        }

        // --- State declaration (explicit `state ...`) --------------------
        if let Some(rest) = strip_kw(line, "state") {
            // Might open a composite — `state Foo {` / `state "Name" as Foo {` /
            // `state Foo <<fork>>` / `state Foo` (plain).
            //
            // Upstream's Jison grammar accepts the opening `{` either glued
            // to the state line OR on a subsequent (possibly blank-line
            // separated) line.  The byte-exact fixtures `cypress/state/47`
            // and the `Multiple States` demo rely on the latter form, e.g.
            //   ```
            //   state State1
            //   {
            //      c0
            //   }
            //   ```
            // Mirror that by peeking ahead for a standalone `{` token after
            // skipping blank/comment lines.
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
            // Peek ahead — does the next non-empty line open a composite?
            let mut peek = i;
            while peek < lines.len() && lines[peek].trim().is_empty() {
                peek += 1;
            }
            if peek < lines.len() && lines[peek].trim() == "{" {
                let id = ingest_state_decl(&mut diagram, rest, parent_stack.last().cloned());
                diagram.items.push(ParseItem::StateDecl(id.clone()));
                if let Some(s) = diagram.states.iter_mut().find(|s| s.id == id) {
                    if s.kind == StateKind::Simple {
                        s.kind = StateKind::Composite;
                    }
                }
                parent_stack.push(id);
                // Consume the lookahead `{` line so the main loop does not
                // re-process it as a stray identifier.
                i = peek + 1;
                continue;
            }
            let id = ingest_state_decl(&mut diagram, rest, parent_stack.last().cloned());
            diagram.items.push(ParseItem::StateDecl(id));
            continue;
        }

        // --- Divider `--` (concurrent-state separator) inside composite ----
        // Upstream lexer rule: `/^(?:--)/i` produces a CONCURRENT token that
        // becomes a state stmt with `id = yy.getDividerId()` and `type =
        // "divider"`. Subsequent `docTranslator` partitions the parent doc
        // into one cluster wrapper per chunk between dividers (see
        // `apply_divider_translation` below).
        //
        // Accept `--` exactly (with optional leading/trailing whitespace
        // already trimmed). Avoid colliding with edge syntax `-->` — that
        // token is matched earlier by parse_transition.
        if line == "--" || (line.starts_with("--") && line.chars().all(|c| c == '-')) {
            if let Some(parent) = parent_stack.last().cloned() {
                divider_cnt += 1;
                let id = format!("divider-id-{}", divider_cnt);
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
        if let Some(tr) =
            parse_transition(line, &mut diagram, &mut next_start_end_idx, &parent_stack)
        {
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
                // Upstream's root-level `X: description` lines participate in the
                // same first-touch ordering as explicit state declarations.
                // Nested bare-colon states are left relation-driven so isolated
                // inner-root node order matches the reference SVGs.
                if parent_stack.is_empty() {
                    diagram.items.push(ParseItem::StateDecl(id.clone()));
                }
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

    // Apply upstream `docTranslator` semantics: every composite parent that
    // contains divider stmts in its doc gets its children regrouped — each
    // chunk between dividers becomes its own divider-cluster wrapper. The
    // first chunks reuse the divider ids (`divider-id-N`); the trailing
    // chunk gets a generated `id-XXXXXX-K` id derived from the upstream
    // `Math.random()`-seeded PRNG so byte-exact ids reproduce.
    apply_divider_translation(&mut diagram);

    Ok(diagram)
}

/// Replicates the divider phase of upstream's `StateDB.docTranslator`.
///
/// For every composite parent whose `children` list contains at least one
/// `Divider` state, partition the children into chunks separated by the
/// dividers and replace those children with one wrapper Composite per
/// chunk. The wrapper carries:
/// * `id` — `divider-id-N` (first wrappers, reusing the divider stmt id)
///   then a generated `id-{base36}-{cnt}` for the trailing chunk.
/// * `kind = StateKind::Divider` — picked up by the layout/render path to
///   produce the dashed-rect cluster shape upstream emits.
/// * `parent` — the original composite (so cluster nesting is preserved).
///
/// Children of each wrapper get their `parent` rewritten to the wrapper id.
/// The original composite's `children` list is replaced by the wrapper ids
/// in chunk order.
fn apply_divider_translation(diagram: &mut crate::model::state::StateDiagram) {
    use crate::model::state::{State, StateKind};
    use std::collections::HashSet;

    fn collect_chunk_members(
        diagram: &crate::model::state::StateDiagram,
        chunk_children: &[String],
    ) -> HashSet<String> {
        let mut members: HashSet<String> = HashSet::new();
        let mut queue: Vec<String> = chunk_children.to_vec();
        while let Some(cid) = queue.pop() {
            if !members.insert(cid.clone()) {
                continue;
            }
            for s in &diagram.states {
                if s.parent.as_deref() == Some(cid.as_str()) {
                    queue.push(s.id.clone());
                }
            }
        }
        members
    }

    fn first_state_index(
        diagram: &crate::model::state::StateDiagram,
        ids: &[String],
    ) -> Option<usize> {
        ids.iter()
            .filter_map(|id| diagram.states.iter().position(|s| s.id == *id))
            .min()
    }

    // Identify composite parents that need translation.
    let parents_to_translate: Vec<String> = diagram
        .states
        .iter()
        .filter(|s| {
            // Only consider parents whose direct children include a divider.
            s.children.iter().any(|cid| {
                matches!(
                    diagram.states.iter().find(|c| &c.id == cid).map(|c| c.kind),
                    Some(StateKind::Divider)
                )
            })
        })
        .map(|s| s.id.clone())
        .collect();

    if parents_to_translate.is_empty() {
        return;
    }

    // Diagram-scoped PRNG state for `generateId()` calls. Upstream resets
    // `Math.random()` to mulberry32 seeded at 0x12345678 before each render,
    // so we mirror that here.
    let mut rng_state: u32 = 0x12345678;
    let mut generate_cnt: u32 = 0;

    for parent_id in parents_to_translate {
        // Snapshot the parent's children list (in declaration order).
        let children_list: Vec<String> = match diagram.states.iter().find(|s| s.id == parent_id) {
            Some(p) => p.children.clone(),
            None => continue,
        };

        // Partition children into chunks separated by Divider entries.
        // Each chunk is paired with the id of the divider that ENDED it
        // (i.e. the divider following the chunk in source order). The
        // trailing chunk has no divider — the wrapper id for it is
        // generated below via mulberry32.
        let mut chunks: Vec<(Option<String>, Vec<String>)> = Vec::new();
        let mut current: Vec<String> = Vec::new();
        for cid in &children_list {
            let kind = diagram.states.iter().find(|s| &s.id == cid).map(|s| s.kind);
            if matches!(kind, Some(StateKind::Divider)) {
                // Close the current chunk with this divider's id.
                chunks.push((Some(cid.clone()), std::mem::take(&mut current)));
            } else {
                current.push(cid.clone());
            }
        }
        // Trailing chunk after the last divider (if any non-divider items
        // remain). Upstream only emits the trailing wrapper when there is at
        // least one divider AND a non-empty trailing chunk.
        if !current.is_empty() && chunks.iter().any(|(d, _)| d.is_some()) {
            chunks.push((None, current));
        } else if !chunks.is_empty() && current.is_empty() {
            // No trailing chunk — leave as-is.
        } else if chunks.is_empty() {
            // No dividers at all — nothing to translate.
            continue;
        }

        // Build wrapper states + remap children's parent pointer.
        let mut wrapper_ids: Vec<String> = Vec::new();
        for (divider_id_opt, chunk_children) in chunks {
            let chunk_members = collect_chunk_members(diagram, &chunk_children);
            let parent_start = format!("{parent_id}_start");
            let parent_end = format!("{parent_id}_end");
            let wrapper_id = match divider_id_opt {
                Some(div_id) => div_id, // reuse the divider stmt id
                None => {
                    generate_cnt += 1;
                    let r = mulberry32_next(&mut rng_state);
                    format!("id-{}-{}", js_random_to_base36_prefix(r), generate_cnt)
                }
            };
            let wrapper_start = format!("{wrapper_id}_start");
            let wrapper_end = format!("{wrapper_id}_end");
            let mut wrapper_children = chunk_children.clone();
            let uses_parent_start = diagram
                .transitions
                .iter()
                .any(|t| t.source == parent_start && chunk_members.contains(&t.target));
            let uses_parent_end = diagram
                .transitions
                .iter()
                .any(|t| t.target == parent_end && chunk_members.contains(&t.source));
            if uses_parent_start {
                if let Some(pos) = wrapper_children.iter().position(|id| id == &parent_start) {
                    wrapper_children[pos] = wrapper_start.clone();
                } else {
                    wrapper_children.insert(0, wrapper_start.clone());
                }
            }
            if uses_parent_end {
                if let Some(pos) = wrapper_children.iter().position(|id| id == &parent_end) {
                    wrapper_children[pos] = wrapper_end.clone();
                } else {
                    wrapper_children.push(wrapper_end.clone());
                }
            }

            // Rewrite the original divider state (if reused id) into a
            // composite-like wrapper, OR insert a brand-new wrapper state.
            let already_present = diagram.states.iter().any(|s| s.id == wrapper_id);
            if already_present {
                if let Some(s) = diagram.states.iter_mut().find(|s| s.id == wrapper_id) {
                    s.kind = StateKind::Divider; // semantically: divider-cluster
                    s.parent = Some(parent_id.clone());
                    s.children = wrapper_children.clone();
                    s.implicit = true;
                }
            } else {
                diagram.states.push(State {
                    id: wrapper_id.clone(),
                    kind: StateKind::Divider,
                    parent: Some(parent_id.clone()),
                    children: wrapper_children.clone(),
                    implicit: true,
                    ..State::default()
                });
            }

            // Upstream `docTranslator` scopes each chunk's `[ * ]` to the
            // divider wrapper BEFORE node extraction, so concurrent regions
            // get `divider-id-N_start` / `_end` nodes instead of sharing the
            // composite parent's `Parent_start`. We emulate that by
            // re-scoping transitions whose non-start endpoint lives in this
            // chunk, then renaming/inserting the corresponding implicit
            // start/end node under the wrapper.
            let insert_at = first_state_index(diagram, &chunk_children).unwrap_or(diagram.states.len());
            if uses_parent_start {
                if chunk_children.iter().any(|id| id == &parent_start) {
                    if let Some(s) = diagram.states.iter_mut().find(|s| s.id == parent_start) {
                        s.id = wrapper_start.clone();
                        s.parent = Some(wrapper_id.clone());
                        s.kind = StateKind::StartEnd;
                        s.implicit = true;
                    }
                } else {
                    diagram.states.insert(
                        insert_at,
                        State {
                            id: wrapper_start.clone(),
                            kind: StateKind::StartEnd,
                            parent: Some(wrapper_id.clone()),
                            implicit: true,
                            ..State::default()
                        },
                    );
                }
            }
            if uses_parent_end {
                if chunk_children.iter().any(|id| id == &parent_end) {
                    if let Some(s) = diagram.states.iter_mut().find(|s| s.id == parent_end) {
                        s.id = wrapper_end.clone();
                        s.parent = Some(wrapper_id.clone());
                        s.kind = StateKind::StartEnd;
                        s.implicit = true;
                    }
                } else {
                    let end_insert_at = first_state_index(diagram, &chunk_children)
                        .map(|idx| idx + chunk_children.len())
                        .unwrap_or(diagram.states.len());
                    diagram.states.insert(
                        end_insert_at,
                        State {
                            id: wrapper_end.clone(),
                            kind: StateKind::StartEnd,
                            parent: Some(wrapper_id.clone()),
                            implicit: true,
                            ..State::default()
                        },
                    );
                }
            }

            for tr in &mut diagram.transitions {
                if uses_parent_start && tr.source == parent_start && chunk_members.contains(&tr.target)
                {
                    tr.source = wrapper_start.clone();
                }
                if uses_parent_end && tr.target == parent_end && chunk_members.contains(&tr.source) {
                    tr.target = wrapper_end.clone();
                }
            }

            // Re-parent all child states to the wrapper.
            for cid in &chunk_children {
                if let Some(c) = diagram.states.iter_mut().find(|s| &s.id == cid) {
                    c.parent = Some(wrapper_id.clone());
                }
            }

            wrapper_ids.push(wrapper_id);
        }

        // Replace the parent's children list with the wrapper list.
        if let Some(ps) = diagram.states.iter_mut().find(|s| s.id == parent_id) {
            ps.children = wrapper_ids;
        }

        for stale_id in [format!("{parent_id}_start"), format!("{parent_id}_end")] {
            let still_used = diagram
                .transitions
                .iter()
                .any(|t| t.source == stale_id || t.target == stale_id);
            if still_used {
                continue;
            }
            if let Some(pos) = diagram.states.iter().position(|s| s.id == stale_id) {
                diagram.states.remove(pos);
            }
        }
    }
}

/// mulberry32 PRNG matching upstream `tests/support/generate_ref.mjs`'s
/// `__mulberry32` seeded at 0x12345678. Returns next value in [0, 1).
fn mulberry32_next(state: &mut u32) -> f64 {
    *state = state.wrapping_add(0x6d2b79f5);
    let mut t: u32 = *state;
    let a = t ^ (t >> 15);
    let b = 1u32 | t;
    t = a.wrapping_mul(b);
    let a2 = t ^ (t >> 7);
    let b2 = 61u32 | t;
    let m = a2.wrapping_mul(b2);
    t = (t.wrapping_add(m)) ^ t;
    let v = (t ^ (t >> 14)) as f64;
    v / 4294967296.0
}

/// JS `Math.random().toString(36).substr(2, 12)` — exact bignum-style
/// base-36 conversion used by V8 for non-decimal radices. Mirrors the
/// implementation in `parser/block.rs::js_random_to_base36_prefix`.
fn js_random_to_base36_prefix(x: f64) -> String {
    if x <= 0.0 {
        return String::new();
    }
    let bits = x.to_bits();
    let raw_exp = ((bits >> 52) & 0x7ff) as i32;
    let exp = raw_exp - 1023;
    let mantissa: u64 = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let shift_i: i32 = 52 - exp;
    if shift_i <= 0 || shift_i >= 63 {
        return String::new();
    }
    let shift = shift_i as u32;
    let scale2: u128 = 1u128 << (shift + 1);
    let mut lo: u128 = 2 * (mantissa as u128) - 1;
    let mut hi: u128 = 2 * (mantissa as u128) + 1;
    let mut exact: u128 = 2 * (mantissa as u128);

    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = String::with_capacity(12);
    for _ in 0..12 {
        lo *= 36;
        hi *= 36;
        exact *= 36;
        let lo_d = lo / scale2;
        let hi_d = hi / scale2;
        let ex_d = exact / scale2;
        lo %= scale2;
        hi %= scale2;
        exact %= scale2;
        let mut digit = ex_d as usize;
        if lo_d != hi_d {
            if exact >= scale2 / 2 {
                digit = digit.saturating_add(1);
            }
            if digit >= 36 {
                out.push('z');
            } else {
                out.push(DIGITS[digit] as char);
            }
            break;
        }
        if digit >= 36 {
            out.push('z');
        } else {
            out.push(DIGITS[digit] as char);
        }
    }
    out
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
    let rhs_class: Option<String> = if rhs_class_token.is_empty() {
        None
    } else {
        Some(rhs_class_token.to_string())
    };

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
        (
            id,
            if cn.is_empty() {
                None
            } else {
                Some(cn.to_string())
            },
        )
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
        // Upstream `stateDb` namespaces start/end ids by scope: at root level
        // it emits `root_start` / `root_end`, and inside a composite `Foo` it
        // emits `Foo_start` / `Foo_end`.  Without this scoping the outer
        // `[*]` and an inner `[*]` collapse onto the same id, which then
        // confuses both the dagre extractor (the inner clone is excluded
        // from the outer graph as a "descendant" of its parent cluster but
        // shares its id with the legitimate outer node) and the renderer
        // (one inner-pass `[*]` would steal the outer `<g class="root">`
        // wrapper).
        let scope = parent.clone().unwrap_or_else(|| "root".into());
        let role = if is_source { "start" } else { "end" };
        let id = format!("{}_{}", scope, role);
        // Reuse existing start/end node within the same parent scope,
        // matching upstream where [*] always maps to a single node per scope.
        if !diagram
            .states
            .iter()
            .any(|s| s.id == id && s.parent == *parent)
        {
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
    if let Some(existing) = diagram.states.iter_mut().find(|s| s.id == id) {
        // Forward references can first materialize under the "wrong" composite
        // when an edge points to a state that is declared later in a different
        // nested block (`innerFirst --> 2nd` before `state Second { 2nd --> [*] }`).
        // Upstream doc translation eventually attaches that placeholder to the
        // declaration scope. Mirror that here, but only for still-plain
        // placeholder nodes so we do not override explicit declarations.
        if let Some(new_parent) = parent {
            let is_placeholder = existing.label.is_none()
                && existing.description.is_none()
                && existing.children.is_empty()
                && matches!(existing.kind, StateKind::Simple);
            if is_placeholder && existing.parent.as_deref() != Some(new_parent.as_str()) {
                existing.parent = Some(new_parent);
            }
        }
    } else {
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
                let raw_token = after_as
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches('{')
                    .trim();
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
                        let rest_after_id =
                            after_as[pos + id.len()..].trim_start_matches('{').trim();
                        if let Some((_, desc_text)) = split_once_colon(rest_after_id) {
                            // Preserve leading space — upstream Jison grammar captures
                            // the text after `:` verbatim, including the leading space.
                            let desc_text = desc_text.trim_end();
                            if !desc_text.trim().is_empty() {
                                Some(desc_text)
                            } else {
                                None
                            }
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

    // `state X [[choice]]` / `state X [[fork]]` / `state X [[join]]`
    // Compatibility form: upstream mermaid.js treats `[[shape]]` as a
    // synonym for the `<<shape>>` stereotype syntax.
    if let Some(open) = decl.find("[[") {
        let id = decl[..open].trim();
        let close = decl[open + 2..].find("]]").map(|i| open + 2 + i);
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
        assert_eq!(
            s.label.as_deref(),
            Some("Your state with spaces in it"),
            "colon-desc should become the display label"
        );
        assert!(
            s.description.is_none(),
            "description should be None when colon-desc became label"
        );
    }
}

#[cfg(test)]
mod test_divider {
    use super::*;

    /// Parser-only check: cy/44's `--` separators turn into wrapper Divider
    /// clusters with the expected ids (`divider-id-1/2` and a mulberry32-based
    /// `id-3tkmm1l27ep-1` for the trailing chunk).
    #[test]
    fn divider_translation_cy44_ids_and_parents() {
        let src = "stateDiagram-v2\n  state s2 {\n      s3\n      --\n      s4\n      --\n      55\n  }\n";
        let d = parse(src).unwrap();
        let by_id = |id: &str| d.states.iter().find(|s| s.id == id).cloned();
        let s2 = by_id("s2").unwrap();
        assert_eq!(
            s2.children,
            vec![
                "divider-id-1".to_string(),
                "divider-id-2".to_string(),
                "id-3tkmm1l27ep-1".to_string(),
            ],
            "s2 children should be the 3 divider-cluster wrappers in source order"
        );
        for (wrapper, leaf) in [
            ("divider-id-1", "s3"),
            ("divider-id-2", "s4"),
            ("id-3tkmm1l27ep-1", "55"),
        ] {
            let w = by_id(wrapper).unwrap();
            assert_eq!(w.kind, crate::model::state::StateKind::Divider);
            assert_eq!(w.parent.as_deref(), Some("s2"));
            assert_eq!(w.children, vec![leaf.to_string()]);
            let l = by_id(leaf).unwrap();
            assert_eq!(l.parent.as_deref(), Some(wrapper));
        }
    }

    #[test]
    fn divider_translation_rescopes_start_nodes_per_chunk() {
        let src = "stateDiagram-v2\n[*] --> Active\nstate Active {\n  [*] --> NumLockOff\n  NumLockOff --> NumLockOn\n  --\n  [*] --> CapsLockOff\n  CapsLockOff --> CapsLockOn\n  --\n  [*] --> ScrollLockOff\n  ScrollLockOff --> ScrollLockOn\n}\n";
        let d = parse(src).unwrap();
        let active = d.states.iter().find(|s| s.id == "Active").unwrap();
        assert_eq!(
            active.children,
            vec![
                "divider-id-1".to_string(),
                "divider-id-2".to_string(),
                "id-3tkmm1l27ep-1".to_string(),
            ]
        );
        for (wrapper, start_id, first_leaf) in [
            ("divider-id-1", "divider-id-1_start", "NumLockOff"),
            ("divider-id-2", "divider-id-2_start", "CapsLockOff"),
            ("id-3tkmm1l27ep-1", "id-3tkmm1l27ep-1_start", "ScrollLockOff"),
        ] {
            let w = d.states.iter().find(|s| s.id == wrapper).unwrap();
            assert_eq!(w.children.first().map(|s| s.as_str()), Some(start_id));
            assert!(w.children.iter().any(|s| s == first_leaf));
            let start = d.states.iter().find(|s| s.id == start_id).unwrap();
            assert_eq!(start.kind, crate::model::state::StateKind::StartEnd);
            assert_eq!(start.parent.as_deref(), Some(wrapper));
        }
        assert!(
            d.states.iter().all(|s| s.id != "Active_start"),
            "parent-scoped start node must be replaced by per-divider starts"
        );
        let start_edges: Vec<(&str, &str)> = d
            .transitions
            .iter()
            .filter(|t| t.source.ends_with("_start"))
            .map(|t| (t.source.as_str(), t.target.as_str()))
            .collect();
        assert!(start_edges.contains(&("divider-id-1_start", "NumLockOff")));
        assert!(start_edges.contains(&("divider-id-2_start", "CapsLockOff")));
        assert!(start_edges.contains(&("id-3tkmm1l27ep-1_start", "ScrollLockOff")));
    }
}

#[cfg(test)]
mod test_scope_reparenting {
    use super::*;

    #[test]
    fn forward_ref_is_reparented_to_later_composite_scope() {
        let src = "stateDiagram-v2\nstate First {\n  innerFirst --> 2nd\n}\nstate Second {\n  2nd --> [*]\n}\n";
        let d = parse(src).unwrap();
        let second_child = d.states.iter().find(|s| s.id == "2nd").unwrap();
        assert_eq!(
            second_child.parent.as_deref(),
            Some("Second"),
            "later declaration scope must own the placeholder node"
        );
    }
}
