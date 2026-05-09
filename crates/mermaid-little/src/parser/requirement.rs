//! Requirement-diagram parser — hand-rolled line-oriented scanner
//! that mirrors upstream `requirementDiagram.jison`.
//!
//! We don't need a full jison runtime: the grammar is simple — a
//! header (`requirementDiagram`), then a sequence of top-level
//! statements. Each statement is either a requirement/element block
//! (`<kind> <name> { body }`), a relationship (`A - <verb> -> B` /
//! `A <- <verb> - B`), a `direction`, `style`, `classDef`, `class`,
//! `accTitle`/`accDescr` directive, or a comment.
//!
//! Upstream reference:
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/requirement/parser/requirementDiagram.jison`
//!
//! Supported cases:
//! * Block bodies can span any number of whitespace-separated lines
//!   between `{` and `}`; inner keys are `id` / `text` / `risk` /
//!   `verifymethod` (requirements) or `type` / `docref` (elements).
//! * Names and text payloads may be double-quoted to include spaces,
//!   colons and punctuation; unquoted tokens stop at the first
//!   `:,<>-=\r\n{` (see the `unqString` rule in upstream's lexer).
//! * `NAME:::cls1,cls2` attaches classes at declaration site.
//! * `direction` accepts TB | BT | LR | RL (case-insensitive).
//! * Comments start with `#` or `%%` and run to EOL.

use crate::error::{MermaidError, Result};
use crate::model::requirement::{
    ClassDef, Element, Relation, Relationship, Requirement, RequirementDiagram, RequirementKind,
    RiskLevel, VerifyMethod,
};

pub fn parse(source: &str) -> Result<RequirementDiagram> {
    let mut diag = RequirementDiagram::new();

    // Running "latest" buffers — upstream's db keeps the partial
    // requirement/element between `id:` / `text:` / … lines and the
    // closing `}` that actually inserts it.
    let mut pending_req_id = String::new();
    let mut pending_req_text = String::new();
    #[allow(unused_assignments)]
    let mut pending_req_risk: Option<RiskLevel> = None;
    #[allow(unused_assignments)]
    let mut pending_req_verify: Option<VerifyMethod> = None;
    let mut pending_elem_type = String::new();
    let mut pending_elem_docref = String::new();

    let (src, fm_title) = strip_frontmatter(source);
    if let Some(t) = fm_title {
        if diag.meta.title.is_none() {
            diag.meta.title = Some(t);
        }
    }
    let src = strip_init_directive(&src);

    // Tokenise into logical statements. A block body counts as a
    // single statement because we need to see the closing brace
    // before committing the Requirement/Element to the AST.
    let mut scan = Scanner::new(&src);
    // Header — "requirementDiagram" must appear first (possibly
    // preceded by directives/comments/blank lines).
    let mut saw_header = false;
    while let Some(line) = scan.next_logical_line() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.eq_ignore_ascii_case("requirementdiagram") {
            saw_header = true;
            break;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "acctitle") {
            let rest = rest.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
            diag.meta.acc_title = Some(rest.to_string());
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "accdescr") {
            let rest = rest.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
            diag.meta.acc_descr = Some(rest.trim_end_matches('}').trim().to_string());
            continue;
        }
        if trimmed.starts_with('#') || trimmed.starts_with("%%") {
            continue;
        }
        return Err(MermaidError::Parse {
            line: scan.line_no,
            col: 1,
            message: format!("expected 'requirementDiagram', found: {:?}", trimmed),
        });
    }
    if !saw_header {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: "missing 'requirementDiagram' header".into(),
        });
    }

    // Body loop. We peek tokens rather than lines so block bodies
    // stretch across arbitrary whitespace.
    while let Some(raw) = scan.next_logical_line() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') || line.starts_with("%%") {
            continue;
        }

        // --- directive: accTitle / accDescr / title ---
        if let Some(rest) = strip_prefix_ci(line, "title ") {
            diag.meta.title = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = strip_prefix_ci(line, "acctitle") {
            let rest = rest.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
            diag.meta.acc_title = Some(rest.to_string());
            continue;
        }
        if let Some(rest) = strip_prefix_ci(line, "accdescr") {
            let rest = rest.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
            diag.meta.acc_descr = Some(rest.trim_end_matches('}').trim().to_string());
            continue;
        }

        // --- direction ---
        if let Some(rest) = strip_prefix_ci(line, "direction ") {
            let d = rest.trim().to_uppercase();
            if matches!(d.as_str(), "TB" | "BT" | "LR" | "RL") {
                diag.direction = d;
            }
            continue;
        }

        // --- classDef / class / style ---
        if let Some(rest) = strip_prefix_ci(line, "classdef ") {
            parse_classdef(rest, &mut diag)?;
            continue;
        }
        if let Some(rest) = strip_prefix_ci(line, "class ") {
            parse_class_assignment(rest, &mut diag)?;
            continue;
        }
        if let Some(rest) = strip_prefix_ci(line, "style ") {
            parse_style(rest, &mut diag)?;
            continue;
        }

        // --- requirement / element blocks ---
        if let Some((kind, rest)) = try_req_kind(line) {
            let (name, rest, decl_classes) = read_name_and_classes(rest)?;
            // Expect `{` — might be on this line or the next.
            if !rest.trim().starts_with('{') {
                // Consume following lines until '{'.
                if !advance_until_brace(&mut scan) {
                    return Err(MermaidError::Parse {
                        line: scan.line_no,
                        col: 1,
                        message: "expected '{' after requirement name".into(),
                    });
                }
            }

            pending_req_id.clear();
            pending_req_text.clear();
            pending_req_risk = None;
            pending_req_verify = None;

            while let Some(body_raw) = scan.next_logical_line() {
                let body = body_raw.trim();
                if body.is_empty() {
                    continue;
                }
                if body == "}" {
                    break;
                }
                // Tolerate trailing `}` on same line as last key.
                let (body, closing) = if let Some(idx) = body.rfind('}') {
                    if body[idx..].trim() == "}" {
                        (body[..idx].trim(), true)
                    } else {
                        (body, false)
                    }
                } else {
                    (body, false)
                };
                if !body.is_empty() {
                    parse_req_body_line(
                        body,
                        &mut pending_req_id,
                        &mut pending_req_text,
                        &mut pending_req_risk,
                        &mut pending_req_verify,
                    )?;
                }
                if closing {
                    break;
                }
            }
            let mut r = Requirement::default();
            r.name = name.clone();
            r.kind = Some(kind);
            r.id = std::mem::take(&mut pending_req_id);
            r.text = std::mem::take(&mut pending_req_text);
            r.risk = pending_req_risk.take();
            r.verify = pending_req_verify.take();
            r.classes = vec!["default".to_string()];
            for c in decl_classes {
                r.classes.push(c);
            }
            if !diag.requirements_map.contains_key(&name) {
                diag.requirement_order.push(name.clone());
            }
            diag.requirements_map.insert(name, r);
            continue;
        }
        if let Some(rest) = strip_prefix_ci(line, "element ") {
            let (name, rest, decl_classes) = read_name_and_classes(rest)?;
            if !rest.trim().starts_with('{') && !advance_until_brace(&mut scan) {
                return Err(MermaidError::Parse {
                    line: scan.line_no,
                    col: 1,
                    message: "expected '{' after element name".into(),
                });
            }
            pending_elem_type.clear();
            pending_elem_docref.clear();
            while let Some(body_raw) = scan.next_logical_line() {
                let body = body_raw.trim();
                if body.is_empty() {
                    continue;
                }
                if body == "}" {
                    break;
                }
                let (body, closing) = if let Some(idx) = body.rfind('}') {
                    if body[idx..].trim() == "}" {
                        (body[..idx].trim(), true)
                    } else {
                        (body, false)
                    }
                } else {
                    (body, false)
                };
                if !body.is_empty() {
                    parse_elem_body_line(body, &mut pending_elem_type, &mut pending_elem_docref)?;
                }
                if closing {
                    break;
                }
            }
            let mut e = Element::default();
            e.name = name.clone();
            e.element_type = std::mem::take(&mut pending_elem_type);
            e.doc_ref = std::mem::take(&mut pending_elem_docref);
            e.classes = vec!["default".to_string()];
            for c in decl_classes {
                e.classes.push(c);
            }
            if !diag.elements_map.contains_key(&name) {
                diag.element_order.push(name.clone());
            }
            diag.elements_map.insert(name, e);
            continue;
        }

        // --- relationship ---
        if let Some(rel) = parse_relationship(line) {
            diag.relations.push(rel);
            continue;
        }

        // --- inline `id:::cls` without a preceding block ---
        if let Some(pos) = line.find(":::") {
            let id = line[..pos].trim().to_string();
            let rest = &line[pos + 3..];
            let classes = split_id_list(rest);
            apply_classes(&mut diag, &[id], &classes);
            continue;
        }

        // Anything left is probably a syntax error, but upstream's
        // jison is loose — skip silently rather than fail, so a single
        // bad line doesn't blow up the whole diagram.
        log::debug!(
            "requirement parser: skipping unrecognised line {}: {:?}",
            scan.line_no,
            line
        );
    }

    Ok(diag)
}

/// Strip a leading `---\n…\n---` YAML frontmatter block, if present.
/// Returns the stripped source and an optional `title:` value extracted from the block.
fn strip_frontmatter(src: &str) -> (String, Option<String>) {
    if !src.trim_start().starts_with("---") {
        return (src.to_string(), None);
    }
    let mut lines = src.lines();
    let first = lines.next().unwrap_or("");
    if first.trim() != "---" {
        return (src.to_string(), None);
    }
    let mut found_end = false;
    let mut tail = String::new();
    let mut fm_title: Option<String> = None;
    for l in lines {
        if !found_end {
            if l.trim() == "---" {
                found_end = true;
                continue;
            }
            // Extract `title: ...` from frontmatter
            let trimmed = l.trim();
            if let Some(rest) = trimmed.strip_prefix("title:") {
                fm_title = Some(rest.trim().to_string());
            }
            continue;
        }
        tail.push_str(l);
        tail.push('\n');
    }
    if found_end {
        (tail, fm_title)
    } else {
        (src.to_string(), None)
    }
}

/// Drop `%%{init: ...}%%` lines. The shared `config::directive`
/// module does a more thorough job at the top-level convert pipeline;
/// here we just strip them so the parser doesn't trip on their `{`.
fn strip_init_directive(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("%%{") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() < prefix.len() {
        return None;
    }
    if s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

fn try_req_kind(line: &str) -> Option<(RequirementKind, &str)> {
    for (kw, kind) in [
        ("functionalRequirement", RequirementKind::Functional),
        ("interfaceRequirement", RequirementKind::Interface),
        ("performanceRequirement", RequirementKind::Performance),
        ("physicalRequirement", RequirementKind::Physical),
        ("designConstraint", RequirementKind::DesignConstraint),
        ("requirement", RequirementKind::Requirement),
    ] {
        if line.len() > kw.len() {
            let head = &line[..kw.len()];
            let tail_ch = line.as_bytes()[kw.len()];
            if head.eq_ignore_ascii_case(kw)
                && (tail_ch == b' ' || tail_ch == b'\t' || tail_ch == b'"')
            {
                return Some((kind, &line[kw.len()..]));
            }
        }
    }
    None
}

/// Read a name (quoted or unquoted) followed by optional `:::cls,cls`.
/// Returns `(name, rest, classes)`.
fn read_name_and_classes(input: &str) -> Result<(String, &str, Vec<String>)> {
    let s = input.trim_start();
    let (name, rest) = if let Some(stripped) = s.strip_prefix('"') {
        let end = stripped.find('"').ok_or_else(|| MermaidError::Parse {
            line: 0,
            col: 0,
            message: "unterminated quoted name".into(),
        })?;
        (stripped[..end].to_string(), &stripped[end + 1..])
    } else {
        let end = s
            .find(|c: char| {
                matches!(c, ':' | ',' | '<' | '>' | '-' | '=' | '{') || c.is_whitespace()
            })
            .unwrap_or(s.len());
        (s[..end].trim().to_string(), &s[end..])
    };
    // Optional ':::cls,cls'.
    let rest = rest.trim_start();
    if let Some(after) = rest.strip_prefix(":::") {
        // Consume until whitespace or `{`.
        let end = after
            .find(|c: char| c.is_whitespace() || c == '{')
            .unwrap_or(after.len());
        let classes = split_id_list(&after[..end]);
        Ok((name, &after[end..], classes))
    } else {
        Ok((name, rest, Vec::new()))
    }
}

fn split_id_list(s: &str) -> Vec<String> {
    s.split(|c: char| c == ',' || c == ' ' || c == '\t')
        .filter_map(|t| {
            let t = t.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        })
        .collect()
}

fn advance_until_brace(scan: &mut Scanner) -> bool {
    while let Some(line) = scan.next_logical_line() {
        if line.trim().starts_with('{') {
            return true;
        }
        if !line.trim().is_empty() {
            // Allow trailing chars, just confirm brace appears.
            if line.contains('{') {
                return true;
            }
        }
    }
    false
}

fn parse_req_body_line(
    body: &str,
    id: &mut String,
    text: &mut String,
    risk: &mut Option<RiskLevel>,
    verify: &mut Option<VerifyMethod>,
) -> Result<()> {
    let (key, val) = split_colon(body)?;
    let k = key.trim().to_ascii_lowercase();
    match k.as_str() {
        "id" => *id = val,
        "text" => *text = val,
        "risk" => {
            *risk = match val.to_ascii_lowercase().as_str() {
                "low" => Some(RiskLevel::Low),
                "medium" => Some(RiskLevel::Medium),
                "high" => Some(RiskLevel::High),
                _ => None,
            };
        }
        "verifymethod" => {
            *verify = match val.to_ascii_lowercase().as_str() {
                "analysis" => Some(VerifyMethod::Analysis),
                "demonstration" => Some(VerifyMethod::Demonstration),
                "inspection" => Some(VerifyMethod::Inspection),
                "test" => Some(VerifyMethod::Test),
                _ => None,
            };
        }
        other => log::debug!("unknown requirement body key {:?}", other),
    }
    Ok(())
}

fn parse_elem_body_line(body: &str, etype: &mut String, docref: &mut String) -> Result<()> {
    let (key, val) = split_colon(body)?;
    let k = key.trim().to_ascii_lowercase();
    match k.as_str() {
        "type" => *etype = val,
        "docref" => *docref = val,
        other => log::debug!("unknown element body key {:?}", other),
    }
    Ok(())
}

fn split_colon(body: &str) -> Result<(String, String)> {
    let idx = body.find(':').ok_or_else(|| MermaidError::Parse {
        line: 0,
        col: 0,
        message: format!("missing ':' in body line {:?}", body),
    })?;
    let key = body[..idx].to_string();
    let rest = body[idx + 1..].trim();
    // Values may be quoted.
    let val = if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"').unwrap_or(stripped.len());
        stripped[..end].to_string()
    } else {
        rest.to_string()
    };
    Ok((key, val))
}

fn parse_relationship(line: &str) -> Option<Relation> {
    // Two arrow shapes:
    //   A - verb -> B   (A is src, B is dst)
    //   A <- verb - B   (B is src, A is dst)  (upstream: yy.addRelationship($3,$5,$1))
    let (left_end, op_start) = if let Some(idx) = line.find(" - ") {
        (idx, idx + 1)
    } else if let Some(idx) = line.find("- ") {
        // start-of-line tolerance
        (idx, idx)
    } else {
        return None;
    };
    // Determine direction.
    let rest_after_dash = &line[op_start + 1..];
    // Two cases: `- verb ->` or `- b`  -> NOT relationship.  `<- verb -`.
    // Simpler: split on whitespace, look for `<-`, `->`, `-`.
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 5 {
        let _ = (left_end, rest_after_dash);
        return None;
    }
    // Expected shapes:
    // [src, "-", verb, "->", dst]
    // [src, "<-", verb, "-", dst]
    // but names may be quoted with embedded spaces. The split-on-ws
    // approach breaks on "my req" containing spaces. Use a char-level
    // splitter that respects double quotes.
    let tokens = tokenise_relationship_line(line)?;
    if tokens.len() != 5 {
        return None;
    }
    let verb = match tokens[2].to_ascii_lowercase().as_str() {
        "contains" => Relationship::Contains,
        "copies" => Relationship::Copies,
        "derives" => Relationship::Derives,
        "satisfies" => Relationship::Satisfies,
        "verifies" => Relationship::Verifies,
        "refines" => Relationship::Refines,
        "traces" => Relationship::Traces,
        _ => return None,
    };
    let (src, dst) = match (tokens[1].as_str(), tokens[3].as_str()) {
        ("-", "->") => (tokens[0].clone(), tokens[4].clone()),
        ("<-", "-") => (tokens[4].clone(), tokens[0].clone()),
        _ => return None,
    };
    Some(Relation {
        kind: verb,
        src,
        dst,
    })
}

/// Split a relationship line into 5 tokens, respecting quoted names.
fn tokenise_relationship_line(line: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' => {
                i += 1;
            }
            b'"' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
                out.push(line[start..i].to_string());
                if i < bytes.len() {
                    i += 1;
                }
            }
            b'<' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                out.push("<-".into());
                i += 2;
            }
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'>' => {
                out.push("->".into());
                i += 2;
            }
            b'-' => {
                out.push("-".into());
                i += 1;
            }
            _ => {
                let start = i;
                while i < bytes.len()
                    && !matches!(bytes[i], b' ' | b'\t' | b'<' | b'>' | b'-' | b'"')
                {
                    i += 1;
                }
                if i > start {
                    out.push(line[start..i].to_string());
                }
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn parse_classdef(rest: &str, diag: &mut RequirementDiagram) -> Result<()> {
    let rest = rest.trim();
    // `classDef <ids> <styles>` — ids is comma- or space-separated,
    // styles is a comma-separated list of `key:value` items.
    let (ids_part, styles_part) = split_ids_styles(rest);
    let ids = split_id_list(&ids_part);
    let styles = split_styles(&styles_part);

    for id in ids {
        let entry = diag
            .class_defs
            .entry(id.clone())
            .or_insert_with(|| ClassDef {
                id: id.clone(),
                ..ClassDef::default()
            });
        let was_new = entry.styles.is_empty() && entry.text_styles.is_empty();
        for s in &styles {
            entry.styles.push(s.clone());
            if s.contains("color") {
                entry.text_styles.push(s.replace("fill", "bgFill"));
            }
        }
        if was_new && !diag.class_def_order.contains(&id) {
            diag.class_def_order.push(id);
        }
    }
    // After defining a classDef, apply its styles to any pre-existing
    // requirement/element that declared the class.
    let pairs: Vec<(String, Vec<String>)> = diag
        .class_defs
        .iter()
        .map(|(k, v)| (k.clone(), v.styles.clone()))
        .collect();
    for (cls, styles) in pairs {
        for name in diag.requirement_order.clone() {
            if let Some(r) = diag.requirements_map.get_mut(&name) {
                if r.classes.contains(&cls) {
                    for s in &styles {
                        for part in s.split(',') {
                            let p = part.trim();
                            if !p.is_empty() && !r.css_styles.iter().any(|x| x == p) {
                                r.css_styles.push(p.to_string());
                            }
                        }
                    }
                }
            }
        }
        for name in diag.element_order.clone() {
            if let Some(e) = diag.elements_map.get_mut(&name) {
                if e.classes.contains(&cls) {
                    for s in &styles {
                        for part in s.split(',') {
                            let p = part.trim();
                            if !p.is_empty() && !e.css_styles.iter().any(|x| x == p) {
                                e.css_styles.push(p.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn parse_class_assignment(rest: &str, diag: &mut RequirementDiagram) -> Result<()> {
    // `class ids classes` — upstream's `setClass` semantics.
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }
    let ids = split_id_list(parts[0]);
    let classes = split_id_list(&parts[1..].join(" "));
    apply_classes(diag, &ids, &classes);
    Ok(())
}

fn apply_classes(diag: &mut RequirementDiagram, ids: &[String], classes: &[String]) {
    // Collect styles up-front so we don't double-borrow `diag`.
    let class_styles: Vec<Vec<String>> = classes
        .iter()
        .map(|c| {
            diag.class_defs
                .get(c)
                .map(|cd| cd.styles.clone())
                .unwrap_or_default()
        })
        .collect();
    for id in ids {
        let mut handled = false;
        if let Some(r) = diag.requirements_map.get_mut(id) {
            for (c, styles) in classes.iter().zip(class_styles.iter()) {
                r.classes.push(c.clone());
                for s in styles {
                    if !r.css_styles.iter().any(|x| x == s) {
                        r.css_styles.push(s.clone());
                    }
                }
            }
            handled = true;
        }
        if !handled {
            if let Some(e) = diag.elements_map.get_mut(id) {
                for (c, styles) in classes.iter().zip(class_styles.iter()) {
                    e.classes.push(c.clone());
                    for s in styles {
                        if !e.css_styles.iter().any(|x| x == s) {
                            e.css_styles.push(s.clone());
                        }
                    }
                }
            }
        }
    }
}

fn parse_style(rest: &str, diag: &mut RequirementDiagram) -> Result<()> {
    // `style ids <styles>` — comma-separated styles.
    let (ids_part, styles_part) = split_ids_styles(rest);
    let ids = split_id_list(&ids_part);
    let styles = split_styles(&styles_part);
    for id in ids {
        let node_styles: Vec<String> = styles
            .iter()
            .flat_map(|s| {
                if s.contains(',') {
                    s.split(',').map(|t| t.trim().to_string()).collect()
                } else {
                    vec![s.clone()]
                }
            })
            .collect();
        if let Some(r) = diag.requirements_map.get_mut(&id) {
            for s in &node_styles {
                r.css_styles.push(s.clone());
            }
        } else if let Some(e) = diag.elements_map.get_mut(&id) {
            for s in &node_styles {
                e.css_styles.push(s.clone());
            }
        }
    }
    Ok(())
}

/// Split a `<ids> <styles>` line — the first whitespace-separated
/// token is the id list (possibly comma-separated), everything after
/// is the style list.
fn split_ids_styles(s: &str) -> (String, String) {
    let s = s.trim_start();
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'\t' {
        i += 1;
    }
    let ids = s[..i].to_string();
    let styles = s[i..].trim_start().to_string();
    (ids, styles)
}

fn split_styles(s: &str) -> Vec<String> {
    s.split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Tiny scanner that yields logical lines, skipping blank ones but
/// preserving position for error reporting.
struct Scanner<'a> {
    rest: &'a str,
    pub line_no: usize,
}

impl<'a> Scanner<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            rest: src,
            line_no: 0,
        }
    }
    fn next_logical_line(&mut self) -> Option<&'a str> {
        if self.rest.is_empty() {
            return None;
        }
        let (line, rest) = match self.rest.find('\n') {
            Some(i) => (&self.rest[..i], &self.rest[i + 1..]),
            None => (self.rest, ""),
        };
        self.rest = rest;
        self.line_no += 1;
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_diagram() {
        let src = "requirementDiagram\n\
                   requirement foo {\n\
                   id: 1\n\
                   text: hello\n\
                   risk: high\n\
                   verifymethod: test\n\
                   }\n";
        let d = parse(src).expect("parse");
        assert_eq!(d.requirement_order.len(), 1);
        let r = &d.requirements_map["foo"];
        assert_eq!(r.id, "1");
        assert_eq!(r.text, "hello");
        assert_eq!(r.risk, Some(RiskLevel::High));
        assert_eq!(r.verify, Some(VerifyMethod::Test));
    }

    #[test]
    fn parses_element_and_relationship() {
        let src = "requirementDiagram\n\
                   requirement r {\n id: 1\n text: x\n risk: low\n verifymethod: analysis\n}\n\
                   element e { type: simulation\n }\n\
                   e - satisfies -> r\n";
        let d = parse(src).expect("parse");
        assert_eq!(d.element_order, vec!["e".to_string()]);
        assert_eq!(d.relations.len(), 1);
        assert_eq!(d.relations[0].kind, Relationship::Satisfies);
        assert_eq!(d.relations[0].src, "e");
        assert_eq!(d.relations[0].dst, "r");
    }

    #[test]
    fn parses_reverse_arrow() {
        let src = "requirementDiagram\n\
                   requirement r { id: 1\n text: t\n risk: low\n verifymethod: test\n }\n\
                   element e { type: x\n }\n\
                   r <- copies - e\n";
        let d = parse(src).expect("parse");
        // "r <- copies - e" means: src=e dst=r
        assert_eq!(d.relations[0].src, "e");
        assert_eq!(d.relations[0].dst, "r");
        assert_eq!(d.relations[0].kind, Relationship::Copies);
    }

    #[test]
    fn honours_direction() {
        let src = "requirementDiagram\ndirection RL\n";
        let d = parse(src).expect("parse");
        assert_eq!(d.direction, "RL");
    }

    #[test]
    fn parses_quoted_names() {
        let src = "requirementDiagram\n\
                   requirement \"my req\" {\n id: 1\n text: t\n risk: low\n verifymethod: test\n}\n";
        let d = parse(src).expect("parse");
        assert!(d.requirements_map.contains_key("my req"));
    }
}
