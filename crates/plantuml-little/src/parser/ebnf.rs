use crate::model::ebnf::{EbnfDiagram, EbnfExpr, EbnfRule};
use crate::Result;

pub fn parse_ebnf_diagram(source: &str) -> Result<EbnfDiagram> {
    let lines: Vec<&str> = source.lines().collect();
    let start_idx = lines
        .iter()
        .position(|line| line.trim().starts_with("@startebnf"))
        .ok_or_else(|| crate::Error::Parse {
            line: 1,
            column: Some(1),
            message: "missing @startebnf".into(),
        })?;
    let end_idx = lines
        .iter()
        .position(|line| line.trim().starts_with("@endebnf"))
        .ok_or_else(|| crate::Error::Parse {
            line: lines.len().max(1),
            column: Some(1),
            message: "missing @endebnf".into(),
        })?;
    let body_lines = &lines[start_idx + 1..end_idx];
    let mut title = None;
    let mut comment = None;
    let mut rules = Vec::new();
    for line in body_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("title ") {
            title = Some(rest.trim().to_string());
            continue;
        }
        if trimmed == "title" {
            continue;
        }
        // `(*)` satisfies both ends at once and would give a reversed range.
        if trimmed.len() >= 4 && trimmed.starts_with("(*") && trimmed.ends_with("*)") {
            comment = Some(trimmed[2..trimmed.len() - 2].trim().to_string());
            continue;
        }
        if let Some(rule) = parse_rule(trimmed)? {
            rules.push(rule);
        }
    }
    Ok(EbnfDiagram {
        title,
        comment,
        rules,
    })
}

fn parse_rule(line: &str) -> Result<Option<EbnfRule>> {
    let eq_pos = match line.find('=') {
        Some(p) => p,
        None => return Ok(None),
    };
    let name = line[..eq_pos].trim().to_string();
    if name.is_empty() {
        return Ok(None);
    }
    let rest = line[eq_pos + 1..]
        .trim()
        .strip_suffix(';')
        .unwrap_or(line[eq_pos + 1..].trim())
        .trim();
    let expr = parse_expr(rest)?;
    Ok(Some(EbnfRule { name, expr }))
}

fn parse_expr(input: &str) -> Result<EbnfExpr> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(EbnfExpr::Terminal(String::new()));
    }
    let alts = split_top_level(input, '|');
    if alts.len() > 1 {
        let mut exprs = Vec::new();
        for a in &alts {
            exprs.push(parse_expr(a.trim())?);
        }
        return Ok(EbnfExpr::Alternation(exprs));
    }
    let seqs = split_top_level(input, ',');
    if seqs.len() > 1 {
        let mut exprs = Vec::new();
        for s in &seqs {
            exprs.push(parse_expr(s.trim())?);
        }
        return Ok(EbnfExpr::Sequence(exprs));
    }
    if (input.starts_with('"') && input.ends_with('"'))
        || (input.starts_with('\'') && input.ends_with('\''))
    {
        return Ok(EbnfExpr::Terminal(input[1..input.len() - 1].to_string()));
    }
    if input.starts_with('[') && input.ends_with(']') {
        return Ok(EbnfExpr::Optional(Box::new(parse_expr(
            &input[1..input.len() - 1],
        )?)));
    }
    // Java: { inner } → ETileZeroOrMore = ETileOptional2(ETileOneOrMore(inner))
    if input.starts_with('{') && input.ends_with('}') {
        let inner = parse_expr(&input[1..input.len() - 1])?;
        return Ok(EbnfExpr::Optional(Box::new(EbnfExpr::Repetition(
            Box::new(inner),
        ))));
    }
    if input.starts_with('(') && input.ends_with(')') {
        let inner = &input[1..input.len() - 1];
        if inner.starts_with('*') && inner.ends_with('*') {
            return Ok(EbnfExpr::Special(
                inner[1..inner.len() - 1].trim().to_string(),
            ));
        }
        return Ok(EbnfExpr::Group(Box::new(parse_expr(inner)?)));
    }
    // Bare identifiers (no quotes) are non-terminal references
    if input
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == ' ')
        && !input.is_empty()
    {
        // Check if it looks like a non-terminal (starts with letter, no spaces except in identifiers)
        let trimmed = input.trim();
        if trimmed
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            return Ok(EbnfExpr::NonTerminal(trimmed.to_string()));
        }
    }
    Ok(EbnfExpr::Terminal(input.to_string()))
}

fn split_top_level(input: &str, delim: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let (mut dp, mut db, mut dbr, mut iq) = (0i32, 0i32, 0i32, false);
    let mut qc = '"';
    for ch in input.chars() {
        if iq {
            cur.push(ch);
            if ch == qc {
                iq = false;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                iq = true;
                qc = ch;
                cur.push(ch);
            }
            '(' => {
                dp += 1;
                cur.push(ch);
            }
            ')' => {
                dp -= 1;
                cur.push(ch);
            }
            '[' => {
                db += 1;
                cur.push(ch);
            }
            ']' => {
                db -= 1;
                cur.push(ch);
            }
            '{' => {
                dbr += 1;
                cur.push(ch);
            }
            '}' => {
                dbr -= 1;
                cur.push(ch);
            }
            c if c == delim && dp == 0 && db == 0 && dbr == 0 => {
                parts.push(cur.clone());
                cur.clear();
            }
            _ => {
                cur.push(ch);
            }
        }
    }
    if !cur.is_empty() || !parts.is_empty() {
        parts.push(cur);
    }
    parts
}
