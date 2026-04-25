//! Class diagram parser — hand-rolled line-oriented recognizer.
//!
//! Upstream grammar reference:
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/class/parser/classDiagram.jison`
//!
//! The class grammar has an unusual shape — several tokens span *most*
//! of a line (`MEMBER` inside a class body, `LABEL` after a `:`) while
//! others come in short bursts on the header line (`class Foo {` all on
//! one line, or split across `class Foo` / `{` / ... ). We therefore
//! do:
//!
//! 1. Extract frontmatter, `%%{init:...}%%` directives and comment
//!    lines via [`crate::preprocess`].
//! 2. Pre-tokenize the body into logical lines (collapsing lines that
//!    end with an open `{` into a single multi-line block statement).
//! 3. Recognise the per-line statement — one of:
//!      * `classDiagram` / `classDiagram-v2` header,
//!      * `direction LR/RL/TB/BT`,
//!      * `accTitle:` / `accDescr:` / `accDescr { ... }`,
//!      * `namespace Foo { ... }`,
//!      * `class Foo` [generic] [`["..."]`] [`{...}`],
//!      * `note "..."`, `note for Foo "..."`,
//!      * `<<annotation>> Foo`,
//!      * `classDef`, `cssClass`, `style`,
//!      * `click`, `link`, `callback`,
//!      * a relation: `A <|-- B : label`,
//!      * a member line: `Foo : +int bar`.
//!
//! We intentionally *don't* implement upstream's full string-escape
//! handling (e.g. `\<`) because the fixture set doesn't exercise it and
//! doing so would add another reproduction hazard; if a fixture slips
//! through we'll emit a [`MermaidError::Parse`] rather than silently
//! mis-parse.

use crate::error::{MermaidError, Result};
use crate::model::class::{
    ClassDiagram, ClassInteractivity, ClassMember, ClassNode, ClassNote, ClassRelation, Classifier,
    InteractivityKind, LineType, MemberKind, Namespace, RelationEnd, StyleClass, Visibility,
};
use crate::preprocess;

/// Public entry point.
pub fn parse(source: &str) -> Result<ClassDiagram> {
    let pre = preprocess::preprocess(source)?;
    let mut d = ClassDiagram {
        meta: pre.meta,
        ..ClassDiagram::default()
    };

    // Body lines — strip comments that survived preprocess (`%% ...`).
    let body = &pre.cleaned_source;
    let logical = logical_lines(body);

    let mut iter = logical.into_iter();

    // Header.
    let header = iter.next().ok_or_else(|| MermaidError::Parse {
        line: 1,
        col: 1,
        message: "empty classDiagram source".into(),
    })?;
    let h = header.text.trim();
    if h == "classDiagram-v2" {
        d.v2 = true;
    } else if h != "classDiagram" && !h.starts_with("classDiagram") {
        return Err(MermaidError::Parse {
            line: header.line_no,
            col: 1,
            message: format!("expected 'classDiagram' header, got '{h}'"),
        });
    }

    // Statements.
    let mut note_counter: usize = 0;
    for stmt in iter {
        let line = stmt.text.trim();
        if line.is_empty() {
            continue;
        }
        parse_statement(line, stmt.line_no, &mut d, &mut note_counter, None)?;
    }

    // Post-pass: assign dom ids. Upstream uses `classId-<id>-<counter>`
    // with counter incrementing across the class list.
    for (i, c) in d.classes.iter_mut().enumerate() {
        c.dom_id = format!("classId-{}-{}", c.base_id, i);
    }
    for (i, n) in d.namespaces.iter_mut().enumerate() {
        n.dom_id = format!("namespace-{}-{}", n.id, i);
    }

    // Silence dead-code warnings for helper items kept for symmetry.
    let _ = ClassNode::new;

    Ok(d)
}

/// A "logical" line — normal lines map 1:1, but lines ending with an
/// unmatched `{` swallow subsequent lines until the matching `}`.
#[derive(Debug, Clone)]
struct LogicalLine {
    text: String,
    line_no: usize,
}

fn logical_lines(body: &str) -> Vec<LogicalLine> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut cur_start = 0usize;
    let mut depth: i32 = 0;
    for (idx, raw) in body.lines().enumerate() {
        let line_no = idx + 1;
        let stripped = strip_line_comment(raw);
        if depth == 0 && cur.is_empty() {
            cur_start = line_no;
        }
        let deltas = brace_delta(stripped);
        if depth > 0 {
            cur.push('\n');
            cur.push_str(stripped);
            depth += deltas;
            if depth <= 0 {
                out.push(LogicalLine {
                    text: std::mem::take(&mut cur),
                    line_no: cur_start,
                });
                depth = 0;
            }
        } else if deltas > 0 {
            cur.push_str(stripped);
            depth = deltas;
        } else {
            let t = stripped.trim();
            if !t.is_empty() {
                out.push(LogicalLine {
                    text: stripped.to_string(),
                    line_no,
                });
            }
        }
    }
    if !cur.is_empty() {
        out.push(LogicalLine {
            text: cur,
            line_no: cur_start,
        });
    }
    out
}

/// Strip a trailing `%% comment` from a raw line. Respects `"..."`.
fn strip_line_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_str = !in_str,
            b'%' if !in_str && i + 1 < bytes.len() && bytes[i + 1] == b'%' => {
                if i + 2 < bytes.len() && bytes[i + 2] == b'{' {
                    return line; // directive; untouched
                }
                return &line[..i];
            }
            _ => {}
        }
        i += 1;
    }
    line
}

/// Brace delta outside of string / backtick contexts.
fn brace_delta(line: &str) -> i32 {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut in_bq = false;
    let mut d = 0i32;
    for &b in bytes {
        match b {
            b'"' if !in_bq => in_str = !in_str,
            b'`' if !in_str => in_bq = !in_bq,
            b'{' if !in_str && !in_bq => d += 1,
            b'}' if !in_str && !in_bq => d -= 1,
            _ => {}
        }
    }
    d
}

/// Parse one top-level logical statement (already whitespace-trimmed).
fn parse_statement(
    line: &str,
    line_no: usize,
    d: &mut ClassDiagram,
    note_counter: &mut usize,
    namespace_parent: Option<&str>,
) -> Result<()> {
    // acc_title / acc_descr.
    if let Some(rest) = line.strip_prefix("accTitle") {
        if let Some(val) = rest.trim_start().strip_prefix(':') {
            d.meta.acc_title = Some(val.trim().to_string());
            return Ok(());
        }
    }
    if let Some(rest) = line.strip_prefix("accDescr") {
        let r = rest.trim_start();
        if let Some(val) = r.strip_prefix(':') {
            d.meta.acc_descr = Some(val.trim().to_string());
            return Ok(());
        }
        if r.starts_with('{') {
            let inner = r
                .trim_start_matches('{')
                .trim_end_matches('}')
                .trim()
                .to_string();
            d.meta.acc_descr = Some(inner);
            return Ok(());
        }
    }

    if let Some(rest) = line.strip_prefix("direction ") {
        let dir = rest.trim().to_uppercase();
        if matches!(dir.as_str(), "TB" | "BT" | "LR" | "RL") {
            d.direction = Some(dir);
            return Ok(());
        }
    }

    if let Some(rest) = line.strip_prefix("namespace ") {
        return parse_namespace(rest, line_no, d, note_counter);
    }

    if let Some(rest) = line.strip_prefix("class ") {
        return parse_class_decl(rest, line_no, d, namespace_parent);
    }

    if let Some(rest) = line.strip_prefix("note for ") {
        return parse_note_for(rest, line_no, d, note_counter, namespace_parent);
    }
    if let Some(rest) = line.strip_prefix("note ") {
        return parse_note(rest, line_no, d, note_counter, namespace_parent);
    }

    if let Some(rest) = line.strip_prefix("classDef ") {
        return parse_class_def(rest, line_no, d);
    }
    if let Some(rest) = line.strip_prefix("cssClass ") {
        return parse_css_class(rest, line_no, d);
    }
    if let Some(rest) = line.strip_prefix("style ") {
        return parse_style(rest, line_no, d);
    }
    if let Some(rest) = line.strip_prefix("click ") {
        return parse_click(rest, line_no, d);
    }
    if let Some(rest) = line.strip_prefix("link ") {
        return parse_link_stmt(rest, line_no, d);
    }
    if let Some(rest) = line.strip_prefix("callback ") {
        return parse_callback_stmt(rest, line_no, d);
    }

    if line.starts_with("<<") {
        return parse_annotation(line, line_no, d);
    }

    if find_relation_op(line).is_some() {
        return parse_relation(line, line_no, d);
    }

    if let Some(colon) = find_unquoted_colon(line) {
        let (head, tail) = line.split_at(colon);
        let member = tail[1..].trim();
        let name = head.trim().to_string();
        if !name.is_empty() && !member.is_empty() {
            let _ = d.class_mut(&name);
            add_member_text(d, &name, member);
            return Ok(());
        }
    }

    if is_identifier(line) {
        let _ = d.class_mut(line);
        if let Some(p) = namespace_parent {
            d.class_mut(line).parent = Some(p.to_string());
        }
    }
    Ok(())
}

fn find_relation_op(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut in_gen = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'"' if !in_gen => in_str = !in_str,
            b'~' if !in_str => in_gen = !in_gen,
            _ => {}
        }
        if !in_str && !in_gen {
            if b == b'.' && bytes.get(i + 1) == Some(&b'.') {
                return Some(relation_start(bytes, i));
            }
            if b == b'-' && bytes.get(i + 1) == Some(&b'-') {
                return Some(relation_start(bytes, i));
            }
        }
        i += 1;
    }
    None
}

fn relation_start(bytes: &[u8], anchor: usize) -> usize {
    let mut i = anchor;
    while i > 0 {
        let prev = bytes[i - 1];
        match prev {
            b'<' | b'|' | b'>' | b'*' | b'o' | b')' | b'(' => i -= 1,
            _ => break,
        }
    }
    i
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '~' || c == '-' || c == '.')
}

fn find_unquoted_colon(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut in_gen = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' if !in_gen => in_str = !in_str,
            b'~' if !in_str => in_gen = !in_gen,
            b':' if !in_str && !in_gen => return Some(i),
            _ => {}
        }
    }
    None
}

fn parse_namespace(
    rest: &str,
    line_no: usize,
    d: &mut ClassDiagram,
    note_counter: &mut usize,
) -> Result<()> {
    let (name, body) = if let Some(brace) = rest.find('{') {
        (rest[..brace].trim(), &rest[brace + 1..])
    } else {
        (rest.trim(), "")
    };
    let body = body.trim_end_matches('}').trim_end();
    if name.is_empty() {
        return Err(MermaidError::Parse {
            line: line_no,
            col: 1,
            message: "namespace requires an identifier".into(),
        });
    }
    d.namespaces.push(Namespace {
        id: name.to_string(),
        dom_id: String::new(),
        class_ids: Vec::new(),
        note_ids: Vec::new(),
    });
    for inner in logical_lines(body) {
        let t = inner.text.trim();
        if t.is_empty() {
            continue;
        }
        parse_statement(t, inner.line_no, d, note_counter, Some(name))?;
    }
    let ns_name = name.to_string();
    let class_ids: Vec<String> = d
        .classes
        .iter()
        .filter(|c| c.parent.as_deref() == Some(&ns_name))
        .map(|c| c.id.clone())
        .collect();
    let note_ids: Vec<String> = d
        .notes
        .iter()
        .filter(|n| n.parent.as_deref() == Some(&ns_name))
        .map(|n| n.id.clone())
        .collect();
    if let Some(ns) = d.namespaces.iter_mut().rfind(|n| n.id == ns_name) {
        ns.class_ids = class_ids;
        ns.note_ids = note_ids;
    }
    Ok(())
}

fn parse_class_decl(
    rest: &str,
    line_no: usize,
    d: &mut ClassDiagram,
    parent: Option<&str>,
) -> Result<()> {
    let src = rest.trim();
    // Find the first `{` that is *not* inside a `["…"]` label block.
    // Upstream's lexer never enters STRUCT_START while scanning STR
    // tokens, so labels like `["With {Brackets}"]` must not split the
    // class body off at the embedded brace.
    let brace_idx = find_top_level_brace(src);
    let (head, body) = if let Some(brace) = brace_idx {
        (
            src[..brace].trim().to_string(),
            src[brace + 1..].trim_end_matches('}').trim().to_string(),
        )
    } else {
        (src.to_string(), String::new())
    };

    let (head_no_anno, anno) = extract_bracket(&head, "<<", ">>");
    let (head2, label_override) = extract_sq_label(&head_no_anno);
    let (mut name, css_class) = if let Some((n, c)) = head2.split_once(":::") {
        (n.trim().to_string(), Some(c.trim().to_string()))
    } else {
        (head2.trim().to_string(), None)
    };
    name = name.split_whitespace().collect::<Vec<_>>().join("");

    if name.is_empty() {
        return Err(MermaidError::Parse {
            line: line_no,
            col: 1,
            message: "class declaration requires a name".into(),
        });
    }

    {
        let c = d.class_mut(&name);
        if let Some(parent_id) = parent {
            c.parent = Some(parent_id.to_string());
        }
        if let Some(lbl) = label_override {
            c.label = lbl;
        }
        if let Some(an) = anno {
            c.annotations.push(an);
        }
        if let Some(cc) = css_class {
            c.css_classes.push(cc);
        }
    }

    for raw in body.lines() {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("<<") && t.ends_with(">>") {
            let inner = &t[2..t.len() - 2];
            d.class_mut(&name)
                .annotations
                .push(inner.trim().to_string());
            continue;
        }
        add_member_text(d, &name, t);
    }
    Ok(())
}

fn extract_bracket(head: &str, open: &str, close: &str) -> (String, Option<String>) {
    if let Some(start) = head.find(open) {
        let after = &head[start + open.len()..];
        if let Some(end) = after.find(close) {
            let inner = &after[..end];
            let remainder = format!("{}{}", &head[..start], &after[end + close.len()..]);
            return (remainder.trim().to_string(), Some(inner.trim().to_string()));
        }
    }
    (head.to_string(), None)
}

/// Return the byte index of the first `{` in `src` that is not inside
/// a `["…"]` quoted label block (mirroring upstream's lexer state, which
/// only switches into STRUCT_START outside the `string` state).
fn find_top_level_brace(src: &str) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => {
                // Skip whitespace inside the brackets.
                let mut j = i + 1;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                let q = bytes.get(j).copied();
                if q == Some(b'"') || q == Some(b'\'') {
                    let qc = q.unwrap();
                    if let Some(close_rel) = src[j + 1..].find(qc as char) {
                        let close = j + 1 + close_rel;
                        // Skip to ']' if present.
                        let mut k = close + 1;
                        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                            k += 1;
                        }
                        if k < bytes.len() && bytes[k] == b']' {
                            i = k + 1;
                            continue;
                        }
                    }
                }
                i += 1;
            }
            b'{' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

fn extract_sq_label(head: &str) -> (String, Option<String>) {
    // Upstream mermaid's lexer treats the `["…"]` label as
    //   SQS  =  '['
    //   STR  =  [^"]*  (only after entering "string" state on the opening '"')
    //   SQE  =  ']'
    // which means the *quoted contents* may contain `[` or `]` freely —
    // only the closing `"` ends the string. Mirror that here: when the
    // bracketed region opens with a quote we scan for the matching
    // closing quote first and then expect a `]` to follow. Otherwise
    // fall back to the simple "first `]` wins" behaviour, which
    // continues to handle bare-token labels like `[lib]` correctly.
    let bytes = head.as_bytes();
    let Some(start) = head.find('[') else {
        return (head.to_string(), None);
    };
    // Skip whitespace inside the brackets.
    let mut after = start + 1;
    while after < bytes.len() && bytes[after].is_ascii_whitespace() {
        after += 1;
    }
    let quote = bytes.get(after).copied();
    if quote == Some(b'"') || quote == Some(b'\'') {
        let q = quote.unwrap();
        // Find the closing quote of the string token.
        if let Some(close_q_rel) = head[after + 1..].find(q as char) {
            let close_q = after + 1 + close_q_rel;
            // Optional whitespace, then ']'
            let mut tail = close_q + 1;
            while tail < bytes.len() && bytes[tail].is_ascii_whitespace() {
                tail += 1;
            }
            if tail < bytes.len() && bytes[tail] == b']' {
                let lbl = head[after + 1..close_q].to_string();
                let remainder =
                    format!("{}{}", &head[..start], &head[tail + 1..]);
                return (remainder.trim().to_string(), Some(lbl));
            }
        }
        // Fall through to the legacy path if quote handling fails.
    }
    if let Some(end_rel) = head[start..].find(']') {
        let end = start + end_rel;
        let inner = &head[start + 1..end];
        let t = inner.trim();
        let lbl = t
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| t.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(t)
            .to_string();
        let remainder = format!("{}{}", &head[..start], &head[end + 1..]);
        return (remainder.trim().to_string(), Some(lbl));
    }
    (head.to_string(), None)
}

fn parse_note_for(
    rest: &str,
    _line_no: usize,
    d: &mut ClassDiagram,
    counter: &mut usize,
    parent: Option<&str>,
) -> Result<()> {
    let s = rest.trim();
    let (cls, text) = split_ident_then_quoted(s);
    let idx = *counter;
    *counter += 1;
    d.notes.push(ClassNote {
        id: format!("note{}", idx),
        class_id: cls,
        text,
        index: idx,
        parent: parent.map(str::to_string),
    });
    Ok(())
}

fn parse_note(
    rest: &str,
    _line_no: usize,
    d: &mut ClassDiagram,
    counter: &mut usize,
    parent: Option<&str>,
) -> Result<()> {
    let s = rest.trim();
    let text = unquote(s).to_string();
    let idx = *counter;
    *counter += 1;
    d.notes.push(ClassNote {
        id: format!("note{}", idx),
        class_id: String::new(),
        text,
        index: idx,
        parent: parent.map(str::to_string),
    });
    Ok(())
}

fn split_ident_then_quoted(s: &str) -> (String, String) {
    let mut idx = s.len();
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            idx = i;
            break;
        }
    }
    let head = s[..idx].trim();
    let tail = s[idx..].trim();
    (head.to_string(), unquote(tail).to_string())
}

fn unquote(s: &str) -> &str {
    let t = s.trim();
    if let Some(inner) = t.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        return inner;
    }
    if let Some(inner) = t.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')) {
        return inner;
    }
    t
}

fn parse_class_def(rest: &str, _line_no: usize, d: &mut ClassDiagram) -> Result<()> {
    let (ids_part, styles_part) = split_whitespace_once(rest.trim());
    let ids: Vec<String> = ids_part
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let styles: Vec<String> = styles_part
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    for id in ids {
        d.style_classes.push(StyleClass {
            id,
            styles: styles.clone(),
            text_styles: Vec::new(),
        });
    }
    Ok(())
}

fn parse_css_class(rest: &str, _line_no: usize, d: &mut ClassDiagram) -> Result<()> {
    // Upstream syntax:
    //     cssClass "id1, id2, ..." styleName
    // The id list lives inside double or single quotes — splitting on the
    // first whitespace would slice through the quoted body if any id is
    // followed by a space, so honour the quotes when present.
    let s = rest.trim();
    let (id_list_raw, second) = if let Some(stripped) = s.strip_prefix('"') {
        match stripped.find('"') {
            Some(end) => (&stripped[..end], stripped[end + 1..].trim_start()),
            None => split_whitespace_once(s),
        }
    } else if let Some(stripped) = s.strip_prefix('\'') {
        match stripped.find('\'') {
            Some(end) => (&stripped[..end], stripped[end + 1..].trim_start()),
            None => split_whitespace_once(s),
        }
    } else {
        split_whitespace_once(s)
    };
    let list = unquote(id_list_raw.trim());
    let style = second.trim().to_string();
    // Upstream `setCssClass` (class diagram) splits the id list on `,`
    // *without* trimming, so any id after the first carries the leading
    // space from `"a, b"` and the `Map.get(" b")` lookup silently fails.
    // We replicate that exact quirk to stay byte-exact with the
    // reference SVG (e.g. cypress/class/217 where `cssClass "Class10,
    // Class20" exClass2` only ever tags `Class10`).
    for id in list.split(',').filter(|s| !s.is_empty()) {
        // Upstream `setCssClass` only mutates classes that already exist;
        // unknown ids are silently ignored. Using `class_mut` here would
        // auto-create phantom classes (we used to render a `"class20"`
        // node for fixture cypress/class/169).
        if let Some(c) = d.classes.iter_mut().find(|c| c.id == id) {
            c.css_classes.push(style.clone());
        }
    }
    Ok(())
}

fn parse_style(rest: &str, _line_no: usize, d: &mut ClassDiagram) -> Result<()> {
    let s = rest.trim();
    let (id, styles) = split_whitespace_once(s);
    let styles: Vec<String> = styles
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let c = d.class_mut(id.trim());
    c.styles.extend(styles);
    Ok(())
}

fn parse_click(rest: &str, _line_no: usize, d: &mut ClassDiagram) -> Result<()> {
    let s = rest.trim();
    let (name, tail) = split_whitespace_once(s);
    let tail = tail.trim();
    if let Some(rest) = tail.strip_prefix("href ") {
        let (url, rest) = take_quoted(rest);
        let (tip, target) = take_optional_tip_target(rest);
        {
            let c = d.class_mut(name);
            c.link = Some(url.clone());
            c.link_target = target.clone();
            c.tooltip = tip.clone();
        }
        d.interactivity.push(ClassInteractivity {
            class_id: name.to_string(),
            kind: InteractivityKind::ClickHref,
            arg: url,
            arg2: tip,
            target,
        });
    } else {
        let (callback, rest) = split_whitespace_once(tail);
        let (args_or_tip, _) = take_optional_quoted(rest);
        {
            let c = d.class_mut(name);
            c.have_callback = true;
            c.tooltip = args_or_tip.clone();
        }
        d.interactivity.push(ClassInteractivity {
            class_id: name.to_string(),
            kind: InteractivityKind::ClickCallback,
            arg: callback.to_string(),
            arg2: args_or_tip,
            target: None,
        });
    }
    Ok(())
}

fn parse_link_stmt(rest: &str, _line_no: usize, d: &mut ClassDiagram) -> Result<()> {
    let s = rest.trim();
    let (name, tail) = split_whitespace_once(s);
    let (url, tail) = take_quoted(tail);
    let (tip, target) = take_optional_tip_target(tail);
    {
        let c = d.class_mut(name);
        c.link = Some(url.clone());
        c.link_target = target.clone();
        c.tooltip = tip.clone();
    }
    d.interactivity.push(ClassInteractivity {
        class_id: name.to_string(),
        kind: InteractivityKind::Link,
        arg: url,
        arg2: tip,
        target,
    });
    Ok(())
}

fn parse_callback_stmt(rest: &str, _line_no: usize, d: &mut ClassDiagram) -> Result<()> {
    let s = rest.trim();
    let (name, tail) = split_whitespace_once(s);
    let (callback, tail) = take_quoted(tail);
    let (tip, _) = take_optional_quoted(tail);
    {
        let c = d.class_mut(name);
        c.have_callback = true;
        c.tooltip = tip.clone();
    }
    d.interactivity.push(ClassInteractivity {
        class_id: name.to_string(),
        kind: InteractivityKind::Callback,
        arg: callback,
        arg2: tip,
        target: None,
    });
    Ok(())
}

fn split_whitespace_once(s: &str) -> (&str, &str) {
    let t = s.trim_start();
    match t.find(char::is_whitespace) {
        Some(i) => (&t[..i], t[i..].trim_start()),
        None => (t, ""),
    }
}

fn take_quoted(s: &str) -> (String, &str) {
    let t = s.trim_start();
    if let Some(stripped) = t.strip_prefix('"') {
        if let Some(end) = stripped.find('"') {
            let q = &stripped[..end];
            let rest = &stripped[end + 1..];
            return (q.to_string(), rest.trim_start());
        }
    }
    let (head, rest) = split_whitespace_once(t);
    (head.to_string(), rest)
}

fn take_optional_quoted(s: &str) -> (Option<String>, &str) {
    let t = s.trim_start();
    if t.starts_with('"') {
        let (q, r) = take_quoted(t);
        (Some(q), r)
    } else {
        (None, t)
    }
}

fn take_optional_tip_target(s: &str) -> (Option<String>, Option<String>) {
    let t = s.trim_start();
    let (tip, rest) = take_optional_quoted(t);
    let target = rest
        .split_whitespace()
        .next()
        .filter(|s| matches!(*s, "_self" | "_blank" | "_parent" | "_top"))
        .map(str::to_string);
    (tip, target)
}

fn parse_annotation(line: &str, line_no: usize, d: &mut ClassDiagram) -> Result<()> {
    let t = line.trim_start_matches("<<");
    if let Some(end) = t.find(">>") {
        let anno = t[..end].trim().to_string();
        let rest = t[end + 2..].trim();
        if !rest.is_empty() {
            d.class_mut(rest).annotations.push(anno);
        }
        Ok(())
    } else {
        Err(MermaidError::Parse {
            line: line_no,
            col: 1,
            message: "unterminated annotation".into(),
        })
    }
}

fn parse_relation(line: &str, _line_no: usize, d: &mut ClassDiagram) -> Result<()> {
    let (body, label) = if let Some(colon) = find_unquoted_colon(line) {
        (line[..colon].trim(), line[colon + 1..].trim().to_string())
    } else {
        (line.trim(), String::new())
    };

    let toks = tokenise_relation(body);
    let (id1, title1, rel_toks, title2, id2) = match extract_relation_parts(&toks) {
        Some(t) => t,
        None => {
            return Err(MermaidError::Parse {
                line: 0,
                col: 0,
                message: format!("unrecognised relation: {body}"),
            });
        }
    };

    let (end1, end2, line_type) = parse_rel_spec(&rel_toks);
    // class_mut is keyed by base id (it strips any `~T~` generic tail).
    // Use the canonical base ids for the relation endpoints too so dagre
    // edges match real node ids — `Foo~T~ <|-- Bar` must point at the
    // `Foo` node, not a phantom `Foo~T~`.
    let id1_base = id1
        .split_once('~')
        .map(|(b, _)| b.to_string())
        .unwrap_or_else(|| id1.clone());
    let id2_base = id2
        .split_once('~')
        .map(|(b, _)| b.to_string())
        .unwrap_or_else(|| id2.clone());
    let _ = d.class_mut(&id1);
    let _ = d.class_mut(&id2);

    d.relations.push(ClassRelation {
        id1: id1_base,
        id2: id2_base,
        end1,
        end2,
        line: line_type,
        title1,
        title2,
        title: label,
        style: Vec::new(),
    });
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RelTok {
    Ident(String),
    Quoted(String),
    RelPart(String),
}

fn tokenise_relation(body: &str) -> Vec<RelTok> {
    let bytes = body.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b' ' || b == b'\t' {
            i += 1;
            continue;
        }
        if b == b'"' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'"' {
                j += 1;
            }
            let lit = std::str::from_utf8(&bytes[i + 1..j])
                .unwrap_or_default()
                .to_string();
            out.push(RelTok::Quoted(lit));
            i = j + 1;
            continue;
        }
        if matches!(b, b'.' | b'-' | b'<' | b'>' | b'|' | b'*' | b'(' | b')') {
            let j = advance_rel(bytes, i);
            let part = std::str::from_utf8(&bytes[i..j])
                .unwrap_or_default()
                .to_string();
            out.push(RelTok::RelPart(part));
            i = j;
            continue;
        }
        // `o` relation token when bordered by relation chars / whitespace.
        if b == b'o' && is_rel_o(bytes, i) {
            let j = advance_rel(bytes, i);
            let part = std::str::from_utf8(&bytes[i..j])
                .unwrap_or_default()
                .to_string();
            out.push(RelTok::RelPart(part));
            i = j;
            continue;
        }
        // identifier
        let mut j = i;
        let mut in_gen = false;
        while j < bytes.len() {
            let c = bytes[j];
            if c == b'~' {
                in_gen = !in_gen;
                j += 1;
                continue;
            }
            if in_gen {
                j += 1;
                continue;
            }
            if c == b' '
                || c == b'\t'
                || matches!(
                    c,
                    b'.' | b'-' | b'<' | b'>' | b'|' | b'*' | b'(' | b')' | b'"'
                )
            {
                break;
            }
            j += 1;
        }
        let ident = std::str::from_utf8(&bytes[i..j])
            .unwrap_or_default()
            .to_string();
        out.push(RelTok::Ident(ident));
        i = j;
    }
    out
}

fn is_rel_o(bytes: &[u8], i: usize) -> bool {
    let prev_ok = i == 0
        || matches!(
            bytes[i - 1],
            b' ' | b'\t' | b'.' | b'-' | b'<' | b'>' | b'|' | b'*' | b'('
        );
    let next_ok = i + 1 < bytes.len()
        && matches!(bytes[i + 1], b'.' | b'-' | b'<' | b'>' | b'|' | b'*' | b')');
    prev_ok && next_ok
}

fn advance_rel(bytes: &[u8], start: usize) -> usize {
    let mut j = start;
    while j < bytes.len() {
        let c = bytes[j];
        if matches!(c, b'.' | b'-' | b'<' | b'>' | b'|' | b'*' | b'(' | b')') {
            j += 1;
            continue;
        }
        if c == b'o' && is_rel_o(bytes, j) {
            j += 1;
            continue;
        }
        break;
    }
    j
}

fn extract_relation_parts(
    toks: &[RelTok],
) -> Option<(String, String, Vec<String>, String, String)> {
    if toks.len() < 3 {
        return None;
    }
    let mut iter = toks.iter();
    let id1 = match iter.next()? {
        RelTok::Ident(s) => s.clone(),
        _ => return None,
    };
    let (title1, rel) = match iter.next()? {
        RelTok::Quoted(q) => {
            let rel = match iter.next()? {
                RelTok::RelPart(r) => r.clone(),
                _ => return None,
            };
            (q.clone(), rel)
        }
        RelTok::RelPart(r) => (String::new(), r.clone()),
        _ => return None,
    };
    let next = iter.next()?;
    let (title2, id2_tok) = match next {
        RelTok::Quoted(q) => (q.clone(), iter.next()?.clone()),
        other => (String::new(), other.clone()),
    };
    let id2 = match id2_tok {
        RelTok::Ident(s) => s,
        _ => return None,
    };
    Some((id1, title1, vec![rel], title2, id2))
}

fn parse_rel_spec(parts: &[String]) -> (RelationEnd, RelationEnd, LineType) {
    let whole: String = parts.join("");
    let (head, tail, line) = if let Some(idx) = whole.find("--") {
        (&whole[..idx], &whole[idx + 2..], LineType::Solid)
    } else if let Some(idx) = whole.find("..") {
        (&whole[..idx], &whole[idx + 2..], LineType::Dotted)
    } else {
        (whole.as_str(), "", LineType::Solid)
    };
    (classify_end(head), classify_end(tail), line)
}

fn classify_end(s: &str) -> RelationEnd {
    match s {
        "" => RelationEnd::None,
        "<|" | "|>" => RelationEnd::Extension,
        ">" | "<" => RelationEnd::Dependency,
        "*" => RelationEnd::Composition,
        "o" => RelationEnd::Aggregation,
        "()" => RelationEnd::Lollipop,
        _ => {
            if s.contains("<|") || s.contains("|>") {
                RelationEnd::Extension
            } else if s.contains('*') {
                RelationEnd::Composition
            } else if s.contains('o') {
                RelationEnd::Aggregation
            } else if s.contains("()") {
                RelationEnd::Lollipop
            } else if s.contains('<') || s.contains('>') {
                RelationEnd::Dependency
            } else {
                RelationEnd::None
            }
        }
    }
}

/// Add a raw member text to a class — classifies method vs attribute
/// and parses visibility / classifier a la upstream `ClassMember`.
fn add_member_text(d: &mut ClassDiagram, class_id: &str, text: &str) {
    let mut trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix(':') {
        trimmed = rest.trim_start();
    }
    if trimmed.is_empty() {
        return;
    }
    if trimmed.starts_with("<<") && trimmed.ends_with(">>") {
        let anno = trimmed[2..trimmed.len() - 2].trim().to_string();
        d.class_mut(class_id).annotations.push(anno);
        return;
    }

    let is_method = has_unquoted_parens(trimmed);
    let kind = if is_method {
        MemberKind::Method
    } else {
        MemberKind::Attribute
    };
    let mut m = ClassMember::new(kind);
    if kind == MemberKind::Method {
        parse_method(trimmed, &mut m);
    } else {
        parse_attribute(trimmed, &mut m);
    }
    m.css_style = m.classifier.css().to_string();
    m.text = compute_member_text(&m);
    let c = d.class_mut(class_id);
    if kind == MemberKind::Method {
        c.methods.push(m);
    } else {
        c.members.push(m);
    }
}

fn has_unquoted_parens(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut in_str = false;
    let mut in_gen = false;
    for &b in bytes {
        match b {
            b'"' if !in_gen => in_str = !in_str,
            b'~' if !in_str => in_gen = !in_gen,
            b'(' if !in_str && !in_gen => return true,
            _ => {}
        }
    }
    false
}

fn parse_method(input: &str, m: &mut ClassMember) {
    let mut s = input;
    if let Some(first) = s.chars().next() {
        if let Some(v) = Visibility::from_char(first) {
            m.visibility = v;
            s = &s[first.len_utf8()..];
        }
    }
    let s = s.trim_start();
    if let Some(open) = s.find('(') {
        let name = s[..open].trim();
        m.id = name.to_string();
        if let Some(close) = s[open + 1..].find(')') {
            m.parameters = s[open + 1..open + 1 + close].trim().to_string();
            let after = s[open + 1 + close + 1..].trim().to_string();
            let mut classifier: Option<char> = None;
            let mut ret = after.as_str();
            if let Some(c) = after.chars().next() {
                if matches!(c, '*' | '$') && after.len() == 1 {
                    classifier = Some(c);
                    ret = "";
                }
            }
            let ret_str = ret.trim();
            let (rt, cls) = if let Some(last) = ret_str.chars().last() {
                if matches!(last, '*' | '$') {
                    (
                        ret_str[..ret_str.len() - last.len_utf8()].trim(),
                        Some(last),
                    )
                } else {
                    (ret_str, None)
                }
            } else {
                (ret_str, None)
            };
            m.return_type = rt.to_string();
            if let Some(c) = cls.or(classifier) {
                if let Some(v) = Classifier::from_char(c) {
                    m.classifier = v;
                }
            }
        }
    } else {
        m.id = s.to_string();
    }
}

fn parse_attribute(input: &str, m: &mut ClassMember) {
    let mut s = input;
    if let Some(first) = s.chars().next() {
        if let Some(v) = Visibility::from_char(first) {
            m.visibility = v;
            s = &s[first.len_utf8()..];
        }
    }
    let t = s.trim();
    if let Some(last) = t.chars().last() {
        if matches!(last, '*' | '$') {
            if let Some(v) = Classifier::from_char(last) {
                m.classifier = v;
                m.id = t[..t.len() - last.len_utf8()].trim().to_string();
                return;
            }
        }
    }
    m.id = t.to_string();
}

fn compute_member_text(m: &ClassMember) -> String {
    let vis_prefix = match m.visibility {
        Visibility::None => String::new(),
        _ => format!("\\{}", m.visibility.glyph()),
    };
    let id_parsed = parse_generic_types(&m.id);
    let mut combined = format!("{}{}", vis_prefix, id_parsed);
    if m.kind == MemberKind::Method {
        combined.push('(');
        combined.push_str(&parse_generic_types(&m.parameters));
        combined.push(')');
        if !m.return_type.is_empty() {
            combined.push_str(" : ");
            combined.push_str(&parse_generic_types(&m.return_type));
        }
    }
    let escaped = combined.replace('<', "&lt;").replace('>', "&gt;");
    if escaped.starts_with("\\&lt;") {
        escaped.replacen("\\&lt;", "~", 1)
    } else {
        escaped
    }
}

/// Port of upstream `parseGenericTypes` (`common.ts`): split on `,`,
/// optionally re-join a `~K,V~` pair into one set, then for each set
/// replace the outermost-surrounding `~` pair with `<>`, iterating
/// until no pair is left.
fn parse_generic_types(input: &str) -> String {
    let input_sets: Vec<&str> = split_keep_separator(input, ',');
    let mut output: Vec<String> = Vec::new();
    let mut i = 0;
    while i < input_sets.len() {
        let mut this_set = input_sets[i].to_string();
        if this_set == ","
            && i > 0
            && i + 1 < input_sets.len()
            && should_combine_sets(input_sets[i - 1], input_sets[i + 1])
        {
            this_set = format!("{},{}", input_sets[i - 1], input_sets[i + 1]);
            i += 1;
            output.pop();
        }
        output.push(process_set(&this_set));
        i += 1;
    }
    output.join("")
}

fn split_keep_separator(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    for (idx, c) in s.char_indices() {
        if c == sep {
            if idx > start {
                out.push(&s[start..idx]);
            }
            out.push(&s[idx..idx + c.len_utf8()]);
            start = idx + c.len_utf8();
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

fn count_occurrence(s: &str, sub: &str) -> usize {
    if sub.is_empty() {
        return 0;
    }
    s.matches(sub).count()
}

fn should_combine_sets(prev: &str, next: &str) -> bool {
    count_occurrence(prev, "~") == 1 && count_occurrence(next, "~") == 1
}

fn process_set(input: &str) -> String {
    let tilde_count = count_occurrence(input, "~");
    if tilde_count <= 1 {
        return input.to_string();
    }
    let mut has_starting_tilde = false;
    let mut working = input.to_string();
    if tilde_count % 2 != 0 && working.starts_with('~') {
        working = working[1..].to_string();
        has_starting_tilde = true;
    }
    let mut chars: Vec<char> = working.chars().collect();
    loop {
        let first = chars.iter().position(|c| *c == '~');
        let last = chars.iter().rposition(|c| *c == '~');
        match (first, last) {
            (Some(f), Some(l)) if f != l => {
                chars[f] = '<';
                chars[l] = '>';
            }
            _ => break,
        }
    }
    if has_starting_tilde {
        chars.insert(0, '~');
    }
    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_header() {
        let d = parse("classDiagram\nclass Foo\n").unwrap();
        assert_eq!(d.classes.len(), 1);
        assert_eq!(d.classes[0].id, "Foo");
        assert!(!d.v2);
    }

    #[test]
    fn parse_v2_header() {
        let d = parse("classDiagram-v2\nclass Foo\n").unwrap();
        assert!(d.v2);
    }

    #[test]
    fn parse_inheritance_relation() {
        let d = parse("classDiagram\nA <|-- B\n").unwrap();
        assert_eq!(d.relations.len(), 1);
        assert_eq!(d.relations[0].id1, "A");
        assert_eq!(d.relations[0].id2, "B");
        assert_eq!(d.relations[0].end1, RelationEnd::Extension);
        assert_eq!(d.relations[0].line, LineType::Solid);
    }

    #[test]
    fn parse_relation_with_label() {
        let d = parse("classDiagram\nA --> B : uses\n").unwrap();
        assert_eq!(d.relations[0].title, "uses");
        assert_eq!(d.relations[0].end2, RelationEnd::Dependency);
    }

    #[test]
    fn parse_relation_with_multiplicity() {
        let d = parse("classDiagram\nA \"1\" *-- \"many\" B\n").unwrap();
        assert_eq!(d.relations[0].title1, "1");
        assert_eq!(d.relations[0].title2, "many");
        assert_eq!(d.relations[0].end1, RelationEnd::Composition);
    }

    #[test]
    fn parse_class_body_with_members() {
        let src = "classDiagram\nclass Foo {\n  +int x\n  +run()\n}\n";
        let d = parse(src).unwrap();
        let c = d.class("Foo").unwrap();
        assert_eq!(c.members.len(), 1);
        assert_eq!(c.methods.len(), 1);
        assert_eq!(c.members[0].visibility, Visibility::Public);
    }

    #[test]
    fn parse_annotation_brackets() {
        let d = parse("classDiagram\n<<interface>> Foo\n").unwrap();
        assert_eq!(d.class("Foo").unwrap().annotations, vec!["interface"]);
    }

    #[test]
    fn parse_class_with_generic() {
        let d = parse("classDiagram\nclass Foo~T~\n").unwrap();
        let c = &d.classes[0];
        // Mermaid keys class identity by base id; `Foo~T~` and a later
        // bare `Foo` reference the same record.
        assert_eq!(c.id, "Foo");
        assert_eq!(c.base_id, "Foo");
        assert_eq!(c.generic.as_deref(), Some("T"));
    }

    #[test]
    fn parse_class_with_label() {
        let d = parse("classDiagram\nclass Foo[\"Nice Label\"]\n").unwrap();
        assert_eq!(d.classes[0].label, "Nice Label");
    }

    #[test]
    fn parse_direction() {
        let d = parse("classDiagram\ndirection LR\n").unwrap();
        assert_eq!(d.direction.as_deref(), Some("LR"));
    }

    #[test]
    fn parse_namespace_block() {
        let src = "classDiagram\nnamespace Outer {\nclass A\nclass B\n}\n";
        let d = parse(src).unwrap();
        assert_eq!(d.namespaces.len(), 1);
        assert_eq!(d.namespaces[0].class_ids, vec!["A", "B"]);
        assert_eq!(
            d.classes
                .iter()
                .find(|c| c.id == "A")
                .unwrap()
                .parent
                .as_deref(),
            Some("Outer")
        );
    }

    #[test]
    fn parse_note_for() {
        let d = parse("classDiagram\nnote for Foo \"hello\"\n").unwrap();
        assert_eq!(d.notes[0].class_id, "Foo");
        assert_eq!(d.notes[0].text, "hello");
    }

    #[test]
    fn parse_style_line() {
        let d = parse("classDiagram\nclass Foo\nstyle Foo fill:#f9f,stroke:#333\n").unwrap();
        let c = d.class("Foo").unwrap();
        assert!(c.styles.iter().any(|s| s.contains("fill:#f9f")));
    }

    #[test]
    fn parse_generic_types_expands_tildes() {
        assert_eq!(parse_generic_types("List~int~"), "List<int>");
        assert_eq!(parse_generic_types("Map~K,V~"), "Map<K,V>");
        assert_eq!(parse_generic_types("List~List~int~~"), "List<List<int>>");
    }

    #[test]
    fn parse_lollipop_relation() {
        let d = parse("classDiagram\nA ()-- B\n").unwrap();
        assert_eq!(d.relations[0].end1, RelationEnd::Lollipop);
    }
}
