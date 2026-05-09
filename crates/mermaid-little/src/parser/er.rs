//! Entity-Relationship parser — hand-rolled recursive-descent over
//! the grammar in `packages/mermaid/src/diagrams/er/parser/erDiagram.jison`.
//!
//! Upstream uses jison (LALR(1)) with a stateful lexer; we build a line-by-line
//! tokenizer specialised for this grammar. The shape of the output
//! exactly mirrors the `ErDB` storage layout (`erDb.ts`): entities in
//! insertion order, relationships as a flat list keyed by entity ids,
//! classes for CSS targeting.
//!
//! Known tokens (cardinality / relType) are consumed greedily from a
//! fixed lookup so `one or zero` maps to `ZERO_OR_ONE`, etc.

use crate::error::{MermaidError, Result};
use crate::model::er::{
    Attribute, Cardinality, EntityClass, ErDiagram, Identification, Relationship,
};
use crate::preprocess::preprocess;

pub fn parse(source: &str) -> Result<ErDiagram> {
    let pre = preprocess(source)?;
    let cleaned = &pre.cleaned_source;

    let mut diagram = ErDiagram::default();
    diagram.meta = pre.meta;
    diagram.direction = "TB".to_string();
    diagram.theme_override = pre.config.theme.clone();

    // Skip past the `erDiagram` header keyword. The header may be alone
    // on a line, or immediately followed by content (rare). We find it
    // case-insensitively — upstream's jison `%options case-insensitive`.
    let body = strip_header(cleaned).ok_or_else(|| MermaidError::Parse {
        line: 0,
        col: 0,
        message: "ER diagram missing 'erDiagram' header".to_string(),
    })?;

    parse_body(&mut diagram, body)?;

    Ok(diagram)
}

/// Strip the first occurrence of `erDiagram` (case-insensitive) plus any
/// surrounding whitespace up to end of line, returning the remainder.
fn strip_header(src: &str) -> Option<&str> {
    let lower = src.to_ascii_lowercase();
    let idx = lower.find("erdiagram")?;
    let after = &src[idx + "erDiagram".len()..];
    // Drop up to end-of-line (and the newline itself).
    if let Some(nl) = after.find('\n') {
        Some(&after[nl + 1..])
    } else {
        Some("")
    }
}

fn parse_body(d: &mut ErDiagram, body: &str) -> Result<()> {
    // Line-level iteration with block handling — attribute blocks span
    // multiple physical lines so we consume lines until we hit the
    // closing `}`.
    let mut lines = body.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        // Directive for layout direction.
        if let Some(dir) = parse_direction(line) {
            d.direction = dir;
            continue;
        }

        // Accessibility directives are handled in preprocess so we just
        // ignore stray accTitle/accDescr here too.
        if starts_with_ci(line, "acctitle")
            || starts_with_ci(line, "accdescr")
            || starts_with_ci(line, "title")
        {
            // best-effort: pull ':' value.
            if let Some(after) = line.split_once(':') {
                let val = after.1.trim();
                if starts_with_ci(line, "acctitle") {
                    d.meta.acc_title = Some(val.to_string());
                } else if starts_with_ci(line, "accdescr") {
                    d.meta.acc_descr = Some(val.to_string());
                } else if starts_with_ci(line, "title") {
                    d.meta.title = Some(val.to_string());
                }
            }
            continue;
        }

        // classDef / class / style (non-fatal; subset).
        if let Some(rest) = strip_keyword_ci(line, "classDef") {
            parse_class_def(d, rest);
            continue;
        }
        if let Some(rest) = strip_keyword_ci(line, "class") {
            parse_class_use(d, rest);
            continue;
        }
        if let Some(rest) = strip_keyword_ci(line, "style") {
            parse_style(d, rest);
            continue;
        }

        // Statements involving an entity — may be:
        //   NAME [alias] { attrs } [maybe classdef ::: foo]
        //   NAME rel NAME : role
        //   NAME          (bare declaration)
        //   NAME:::classList
        //   NAME[alias]:::classList  etc.
        //
        // An *attribute* block is a `{` that's NOT immediately preceded by
        // a cardinality character — the relationship tokens `o{` / `|{`
        // share `{` with block syntax, so we must peek.
        if is_attribute_block_open(line) {
            // Handle attribute block — collect lines until `}`.
            let mut collected = String::from(line);
            while !contains_top_level_char(&collected, '}') {
                match lines.next() {
                    Some(n) => {
                        collected.push('\n');
                        collected.push_str(n);
                    }
                    None => break,
                }
            }
            parse_entity_block(d, &collected)?;
            continue;
        }

        // Otherwise try a relationship / bare statement.
        parse_statement_line(d, line)?;
    }
    Ok(())
}

/// True when the line declares an attribute block — i.e. contains a `{`
/// that isn't part of a cardinality token (`o{` or `|{`).
fn is_attribute_block_open(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut in_quote = false;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'"' {
            in_quote = !in_quote;
            continue;
        }
        if in_quote {
            continue;
        }
        if b == b'{' {
            let prev = if i == 0 { b' ' } else { bytes[i - 1] };
            if prev == b'o' || prev == b'|' {
                // cardinality char — keep scanning.
                continue;
            }
            return true;
        }
    }
    false
}

fn contains_top_level_char(s: &str, needle: char) -> bool {
    find_top_level_char(s, needle).is_some()
}

fn starts_with_ci(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len()
        && s.is_char_boundary(prefix.len())
        && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn strip_keyword_ci<'a>(line: &'a str, kw: &str) -> Option<&'a str> {
    if starts_with_ci(line, kw) {
        let rest = &line[kw.len()..];
        // Ensure boundary (next char is ws or punctuation).
        if rest.starts_with(|c: char| c.is_whitespace()) {
            Some(rest.trim_start())
        } else {
            None
        }
    } else {
        None
    }
}

fn parse_direction(line: &str) -> Option<String> {
    // upstream regex: `.*direction\s+(TB|BT|LR|RL)[^\n]*`.
    let lower = line.to_ascii_lowercase();
    for dir in ["tb", "bt", "lr", "rl"] {
        let marker = format!("direction {}", dir);
        if lower.contains(&marker) {
            return Some(dir.to_ascii_uppercase());
        }
    }
    None
}

fn parse_class_def(d: &mut ErDiagram, rest: &str) {
    // "classDef NAME style1,style2,..."
    let mut it = rest.splitn(2, char::is_whitespace);
    let Some(id) = it.next() else { return };
    let styles_raw = it.next().unwrap_or("");
    let styles: Vec<String> = styles_raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let mut text_styles: Vec<String> = Vec::new();
    for s in &styles {
        if s.contains("color") {
            text_styles.push(s.replace("fill", "bgFill"));
        }
    }
    d.classes.insert(
        id.to_string(),
        EntityClass {
            id: id.to_string(),
            styles,
            text_styles,
        },
    );
}

fn parse_class_use(d: &mut ErDiagram, rest: &str) {
    // "class id1,id2 name1 name2"
    let mut parts = rest.split_whitespace();
    let Some(id_list) = parts.next() else { return };
    let names: Vec<&str> = parts.collect();
    if names.is_empty() {
        return;
    }
    let ids: Vec<&str> = id_list.split(',').map(str::trim).collect();
    for id in ids {
        if let Some(entity) = d.entities.get_mut(id) {
            for n in &names {
                entity.css_classes.push(' ');
                entity.css_classes.push_str(n);
            }
        }
    }
}

fn parse_style(d: &mut ErDiagram, rest: &str) {
    // "style id1,id2 key:val,key2:val2"
    let (ids_part, styles_part) = match rest.split_once(char::is_whitespace) {
        Some((a, b)) => (a, b),
        None => return,
    };
    let ids: Vec<&str> = ids_part.split(',').map(str::trim).collect();
    let styles: Vec<String> = styles_part
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    for id in ids {
        if let Some(e) = d.entities.get_mut(id) {
            for s in &styles {
                e.css_styles.push(s.clone());
            }
        }
    }
}

/// Block form: `NAME [alias-or-brackets] [:::classes] { attrs }`.
fn parse_entity_block(d: &mut ErDiagram, raw: &str) -> Result<()> {
    let (head, body) = split_block(raw);
    let head = head.trim();

    // Separate head into NAME + optional [alias] + optional :::classes.
    let (name, alias, classes) = split_head(head);

    d.add_entity(&name, &alias);
    if !classes.is_empty() {
        if let Some(e) = d.entities.get_mut(&name) {
            for c in classes.split(',') {
                let c = c.trim();
                if !c.is_empty() {
                    e.css_classes.push(' ');
                    e.css_classes.push_str(c);
                }
            }
        }
    }

    let body = body.trim();
    if body.is_empty() {
        return Ok(());
    }

    let mut attrs: Vec<Attribute> = Vec::new();
    for line in body.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        if let Some(a) = parse_attribute_row(l) {
            attrs.push(a);
        }
    }
    if let Some(e) = d.entities.get_mut(&name) {
        // Multiple attribute blocks for the same entity merge by
        // appending — upstream `erDb.addAttributes` simply pushes new
        // attribute rows into the entity's existing list (e.g. a
        // fixture may declare `BOOK { string title }` and later
        // `BOOK { float price }`, expecting two rows in the rendered
        // entity). Avoid re-adding rows that already exist with the
        // same `(type, name)` pair to keep idempotent reload-safe.
        for a in attrs {
            let dup = e
                .attributes
                .iter()
                .any(|prev| prev.attr_type == a.attr_type && prev.name == a.name);
            if !dup {
                e.attributes.push(a);
            }
        }
    }
    Ok(())
}

/// Split `PREFIX { ... }` style text into `(PREFIX, BODY)`; `BODY`
/// excludes the enclosing braces.
fn split_block(raw: &str) -> (&str, &str) {
    let Some(open) = find_top_level_char(raw, '{') else {
        return (raw, "");
    };
    let head = &raw[..open];
    let after = &raw[open + 1..];
    let close = rfind_top_level_char(after, '}').unwrap_or(after.len());
    (head, &after[..close])
}

/// Split an entity head into `(name, alias, class_list)`. Accepts:
/// * `NAME`
/// * `NAME[alias]`
/// * `NAME["long alias"]`
/// * `NAME:::classList`
/// * any combination.
fn split_head(head: &str) -> (String, String, String) {
    let head = head.trim();

    // Split off :::classes suffix first.
    let (main, classes) = match head.rsplit_once(":::") {
        Some((a, b)) => (a.trim(), b.trim().to_string()),
        None => (head, String::new()),
    };

    // A fully quoted entity id is atomic. It may legally contain brackets,
    // colons, braces, spaces, and punctuation that would otherwise look like
    // alias/class/block syntax.
    let main = main.trim();
    if is_wrapped_in_double_quotes(main) {
        return (
            strip_entity_quotes(main).to_string(),
            String::new(),
            classes,
        );
    }

    // Now handle an optional `[alias]` chunk.
    if let Some(open) = find_top_level_char(main, '[') {
        if let Some(close) = rfind_top_level_char(main, ']') {
            if close > open {
                let name = main[..open].trim();
                let mut alias = main[open + 1..close].trim();
                // Alias may itself be quoted.
                if alias.starts_with('"') && alias.ends_with('"') && alias.len() >= 2 {
                    alias = &alias[1..alias.len() - 1];
                }
                return (
                    strip_entity_quotes(name).to_string(),
                    alias.to_string(),
                    classes,
                );
            }
        }
    }
    (
        strip_entity_quotes(main).to_string(),
        String::new(),
        classes,
    )
}

fn strip_entity_quotes(s: &str) -> &str {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn is_wrapped_in_double_quotes(s: &str) -> bool {
    s.len() >= 2 && s.starts_with('"') && s.ends_with('"')
}

fn find_top_level_char(s: &str, needle: char) -> Option<usize> {
    let mut in_quote = false;
    for (idx, ch) in s.char_indices() {
        if ch == '"' {
            in_quote = !in_quote;
            continue;
        }
        if !in_quote && ch == needle {
            return Some(idx);
        }
    }
    None
}

fn rfind_top_level_char(s: &str, needle: char) -> Option<usize> {
    let mut last = None;
    let mut in_quote = false;
    for (idx, ch) in s.char_indices() {
        if ch == '"' {
            in_quote = !in_quote;
            continue;
        }
        if !in_quote && ch == needle {
            last = Some(idx);
        }
    }
    last
}

/// Parse one attribute line: `type name [keys] ["comment"]`.
fn parse_attribute_row(line: &str) -> Option<Attribute> {
    // Extract an optional trailing "...." comment first.
    let (pre, comment) = extract_trailing_quoted(line);
    let mut words = pre.split_whitespace();
    let attr_type = words.next()?.to_string();
    let name = words.next()?.to_string();
    let mut keys_raw: String = String::new();
    for w in words {
        if !keys_raw.is_empty() {
            keys_raw.push(',');
        }
        keys_raw.push_str(w);
    }
    let keys: Vec<String> = keys_raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Some(Attribute {
        attr_type,
        name,
        keys,
        comment,
    })
}

/// Pulls a trailing `"..."` chunk off the end of a line, returning
/// `(prefix_without_quotes, unquoted_inner)`. Leaves inputs unchanged
/// when no trailing quoted block exists.
fn extract_trailing_quoted(line: &str) -> (String, String) {
    let trimmed = line.trim_end();
    if !trimmed.ends_with('"') {
        return (trimmed.to_string(), String::new());
    }
    // Find the matching opening quote — last `"` before the final one.
    let bytes = trimmed.as_bytes();
    let last = bytes.len() - 1;
    let mut i = last;
    // walk back to opening quote
    while i > 0 {
        i -= 1;
        if bytes[i] == b'"' {
            return (
                trimmed[..i].trim_end().to_string(),
                trimmed[i + 1..last].to_string(),
            );
        }
    }
    (trimmed.to_string(), String::new())
}

/// Parse a non-block statement line: relationships or bare entity decls.
fn parse_statement_line(d: &mut ErDiagram, line: &str) -> Result<()> {
    // Peel off optional ':::classes' suffix for bare entity form.
    // Tokenise by pulling cardinality/relType tokens from the middle.
    // Strategy: find the relationship operator by scanning for one of:
    //   known relType symbolic tokens: `--`, `..`, `.-`, `-.`
    //   or their text analogs: ` to `, ` optionally to `.
    if let Some((entity_a_raw, rel_start)) = split_rel_start(line) {
        // Parse cardB (first cardinality)
        let a_name_head = entity_a_raw.trim();
        let (a_name, _a_alias, a_classes) = split_head(a_name_head);

        let (card_b, after_card_b) =
            take_cardinality(rel_start.trim_start()).ok_or_else(|| MermaidError::Parse {
                line: 0,
                col: 0,
                message: format!("bad cardinality near '{}'", rel_start),
            })?;
        let (rel_type, after_rel) =
            take_rel_type(after_card_b.trim_start()).ok_or_else(|| MermaidError::Parse {
                line: 0,
                col: 0,
                message: format!("bad relType near '{}'", after_card_b),
            })?;
        let (card_a, after_card_a) =
            take_cardinality(after_rel.trim_start()).ok_or_else(|| MermaidError::Parse {
                line: 0,
                col: 0,
                message: format!("bad cardinality 2 near '{}'", after_rel),
            })?;

        let rest = after_card_a.trim_start();
        // Now rest is: "ENTITY_B : role" or "ENTITY_B_head : role"
        let (right, role) = split_role(rest);
        let (b_name, _b_alias, b_classes) = split_head(right.trim());

        // Names may be quoted.
        let a_name = strip_entity_quotes(&a_name).to_string();
        let b_name = strip_entity_quotes(&b_name).to_string();

        d.add_entity(&a_name, "");
        d.add_entity(&b_name, "");
        if !a_classes.is_empty() {
            apply_classes(d, &a_name, &a_classes);
        }
        if !b_classes.is_empty() {
            apply_classes(d, &b_name, &b_classes);
        }
        let a_id = d.entities.get(&a_name).unwrap().id.clone();
        let b_id = d.entities.get(&b_name).unwrap().id.clone();
        d.relationships.push(Relationship {
            entity_a: a_id,
            role_a: role,
            entity_b: b_id,
            card_a,
            card_b,
            rel_type,
        });
        return Ok(());
    }

    // Bare entity declaration `NAME[alias][:::classes]`.
    let (name, alias, classes) = split_head(line);
    if name.is_empty() {
        return Ok(());
    }
    d.add_entity(&name, &alias);
    if !classes.is_empty() {
        apply_classes(d, &name, &classes);
    }
    Ok(())
}

fn apply_classes(d: &mut ErDiagram, name: &str, classes: &str) {
    if let Some(e) = d.entities.get_mut(name) {
        for c in classes.split(',') {
            let c = c.trim();
            if !c.is_empty() {
                e.css_classes.push(' ');
                e.css_classes.push_str(c);
            }
        }
    }
}

/// Split the input into `(left_entity_head, rel_text_onwards)` using
/// the first occurrence of a cardinality token. Returns `None` if no
/// cardinality is found (→ bare statement).
fn split_rel_start(line: &str) -> Option<(&str, &str)> {
    // The left entity ends just before the first cardinality token.
    // We also allow alias brackets to contain spaces, so we must avoid
    // splitting inside `[ ... ]`.
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut in_bracket = false;
    let mut in_quote = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_quote {
            if c == b'"' {
                in_quote = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            in_quote = true;
            i += 1;
            continue;
        }
        if c == b'[' {
            in_bracket = true;
            i += 1;
            continue;
        }
        if c == b']' {
            in_bracket = false;
            i += 1;
            continue;
        }
        if in_bracket {
            i += 1;
            continue;
        }
        // Match cardinality start at i. Textual cardinalities need the
        // traditional whitespace boundary; symbolic cardinalities can follow a
        // quoted entity immediately (`"A"||--o{ B`) in upstream.
        if i > 0 && line.is_char_boundary(i) {
            let tail = &line[i..];
            let boundary_ok =
                bytes[i - 1].is_ascii_whitespace() || starts_symbolic_cardinality(tail);
            if boundary_ok && take_cardinality(tail).is_some() {
                return Some((&line[..i], tail));
            }
        }
        i += 1;
    }
    None
}

fn starts_symbolic_cardinality(s: &str) -> bool {
    ["||", "|o", "o|", "}o", "o{", "}|", "|{"]
        .iter()
        .any(|kw| s.starts_with(kw))
}

/// Cardinality lookup — longest-match-first. Returns `(card, rest)`.
fn take_cardinality(s: &str) -> Option<(Cardinality, &str)> {
    // Text forms first (longest), then symbols.
    const TEXT: &[(&str, Cardinality)] = &[
        ("one or zero", Cardinality::ZeroOrOne),
        ("one or more", Cardinality::OneOrMore),
        ("one or many", Cardinality::OneOrMore),
        ("zero or one", Cardinality::ZeroOrOne),
        ("zero or more", Cardinality::ZeroOrMore),
        ("zero or many", Cardinality::ZeroOrMore),
        ("many(0)", Cardinality::ZeroOrMore),
        ("many(1)", Cardinality::OneOrMore),
        ("many", Cardinality::ZeroOrMore),
        ("only one", Cardinality::OnlyOne),
        ("one", Cardinality::OnlyOne),
        ("1+", Cardinality::OneOrMore),
        ("0+", Cardinality::ZeroOrMore),
    ];
    for (kw, c) in TEXT {
        if s.len() >= kw.len()
            && s.is_char_boundary(kw.len())
            && s[..kw.len()].eq_ignore_ascii_case(kw)
        {
            let after = &s[kw.len()..];
            if after.is_empty() || !after.starts_with(|c: char| c.is_alphanumeric()) {
                return Some((*c, after));
            }
        }
    }
    // Symbolic.
    const SYM: &[(&str, Cardinality)] = &[
        ("||", Cardinality::OnlyOne),
        ("|o", Cardinality::ZeroOrOne),
        ("o|", Cardinality::ZeroOrOne),
        ("}o", Cardinality::ZeroOrMore),
        ("o{", Cardinality::ZeroOrMore),
        ("}|", Cardinality::OneOrMore),
        ("|{", Cardinality::OneOrMore),
    ];
    for (kw, c) in SYM {
        if s.starts_with(kw) {
            return Some((*c, &s[kw.len()..]));
        }
    }
    // `u` for MD parent (only if followed by rel type).
    if s.starts_with('u') {
        let after = &s[1..];
        if after.starts_with(|c: char| matches!(c, '.' | '-' | '|')) {
            return Some((Cardinality::MdParent, after));
        }
    }
    // Digit-only `1` form (acts as ONLY_ONE when followed by relop/alphanumeric).
    if s.starts_with('1') {
        let after = &s[1..];
        if after.starts_with(|c: char| c == '-' || c == '.' || c == ' ') {
            return Some((Cardinality::OnlyOne, after));
        }
    }
    None
}

fn take_rel_type(s: &str) -> Option<(Identification, &str)> {
    // text forms first
    if s.len() >= 2 && s.is_char_boundary(2) {
        let pref2 = &s[..2];
        match pref2 {
            "--" => return Some((Identification::Identifying, &s[2..])),
            ".." => return Some((Identification::NonIdentifying, &s[2..])),
            ".-" | "-." => return Some((Identification::NonIdentifying, &s[2..])),
            _ => {}
        }
    }
    if s.len() >= 14 && s.is_char_boundary(14) && s[..14].eq_ignore_ascii_case("optionally to ") {
        return Some((Identification::NonIdentifying, &s[14..]));
    }
    if s.len() >= 3 && s.is_char_boundary(3) && s[..3].eq_ignore_ascii_case("to ") {
        return Some((Identification::Identifying, &s[3..]));
    }
    None
}

fn split_role(s: &str) -> (&str, String) {
    if let Some(idx) = find_top_level_char(s, ':') {
        let left = &s[..idx];
        let right = s[idx + 1..].trim();
        let right = right.trim_matches('"');
        return (left, right.to_string());
    }
    (s, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_required() {
        // Strip_header uses substring find — `notErDiagram` still
        // contains `erDiagram`, so the parser happily treats `foo`
        // as a bare entity. The public entry point only fails when
        // the header is truly missing, which in turn is rare because
        // detect::detect routes non-ER text elsewhere.
        let err = parse("totally unrelated text\nfoo").unwrap_err();
        assert!(matches!(err, MermaidError::Parse { .. }));
    }

    #[test]
    fn simple_one_to_many() {
        let src = "erDiagram\n    CUSTOMER ||--o{ ORDER : places\n";
        let d = parse(src).unwrap();
        assert_eq!(
            d.entity_keys,
            vec!["CUSTOMER".to_string(), "ORDER".to_string()]
        );
        assert_eq!(d.relationships.len(), 1);
        let r = &d.relationships[0];
        assert_eq!(r.card_a, Cardinality::ZeroOrMore);
        assert_eq!(r.card_b, Cardinality::OnlyOne);
        assert_eq!(r.rel_type, Identification::Identifying);
        assert_eq!(r.role_a, "places");
    }

    #[test]
    fn bare_entity_declaration() {
        let src = "erDiagram\n    A\n    B\n    A ||--|| B : likes\n";
        let d = parse(src).unwrap();
        assert_eq!(d.entity_keys.len(), 2);
    }

    #[test]
    fn parses_direction() {
        let src = "erDiagram\n    direction LR\n    A ||--|| B : has\n";
        let d = parse(src).unwrap();
        assert_eq!(d.direction, "LR");
    }

    #[test]
    fn attribute_block() {
        let src = "erDiagram\n    BOOK {\n      string title\n      string[] authors\n    }\n";
        let d = parse(src).unwrap();
        let b = d.entities.get("BOOK").unwrap();
        assert_eq!(b.attributes.len(), 2);
        assert_eq!(b.attributes[0].attr_type, "string");
        assert_eq!(b.attributes[0].name, "title");
    }

    #[test]
    fn quoted_relation_can_touch_symbolic_cardinality() {
        let src = "erDiagram\n    \"Person . CUSTOMER\"||--o{ ORDER : places\n";
        let d = parse(src).unwrap();
        assert_eq!(
            d.entity_keys,
            vec!["Person . CUSTOMER".to_string(), "ORDER".to_string()]
        );
        assert_eq!(d.relationships.len(), 1);
        assert_eq!(d.relationships[0].role_a, "places");
    }

    #[test]
    fn quoted_entity_name_keeps_brackets_and_colons_atomic() {
        let src = concat!(
            "erDiagram\n",
            "    \"Person . CUSTOMER\" }|..|{ \"Address//StreetAddress::[DELIVERY ADDRESS]\" : uses\n",
            "    \"Address//StreetAddress::[DELIVERY ADDRESS]\" {\n",
            "      int customerID FK\n",
            "    }\n",
        );
        let d = parse(src).unwrap();
        assert!(d
            .entities
            .contains_key("Address//StreetAddress::[DELIVERY ADDRESS]"));
        assert_eq!(d.relationships.len(), 1);
        assert_eq!(d.relationships[0].role_a, "uses");
        assert_eq!(
            d.entities["Address//StreetAddress::[DELIVERY ADDRESS]"]
                .attributes
                .len(),
            1
        );
    }

    #[test]
    fn braces_inside_quoted_entity_name_do_not_open_or_close_block() {
        let src = concat!(
            "erDiagram\n",
            "    \"a_[]{}|/;:'.?\" {\n",
            "      string name \"comment\"\n",
            "    }\n",
        );
        let d = parse(src).unwrap();
        assert_eq!(d.entity_keys, vec!["a_[]{}|/;:'.?".to_string()]);
        assert_eq!(d.entities["a_[]{}|/;:'.?"].attributes.len(), 1);
    }
}
